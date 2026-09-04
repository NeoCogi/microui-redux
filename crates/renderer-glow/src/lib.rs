#![allow(dead_code)]
//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
//! OpenGL/Glow renderer backend used by examples.
//!
//! This module implements the `RendererBackend` trait, texture uploads, UI batching, and optional custom
//! mesh rendering for the demo application.

use core::slice;
use std::{collections::HashMap, io, sync::Arc};

use microui_redux::{
    prelude::*,
    render::{TextureError, Vertex},
};
use glow::*;
use rs_math3d::{Vec3f, Vec4f};

use microui_redux_renderer_common::{CustomRenderArea, MeshSubmission, resource_guard::ResourceGuard};

// GL backend overview:
// - Regular UI quads are accumulated into CPU-side vertex/index buffers and emitted in one batch
//   from `flush`.
// - Special-case draws (external textures, solid custom vertices, mesh demos) flush the UI batch
//   first, then issue immediate GL commands so ordering stays correct without a larger command
//   graph.
// - Each immutable atlas texture is uploaded transactionally at construction or theme selection.

pub(crate) trait GLCustomRenderer {
    /// Records backend-specific GL commands inside the supplied logical/clip area.
    fn record(&mut self, gl: &glow::Context, framebuffer_size: (u32, u32), area: &CustomRenderArea);
}

const VERTEX_SHADER: &str = "#version 100
uniform highp mat4 uTransform;
attribute highp vec2 vertexPosition;
attribute highp vec2 vertexTexCoord;
attribute lowp vec4 vertexColor;
varying highp vec2 vTexCoord;
varying lowp vec4 vVertexColor;
void main()
{
    vVertexColor = vertexColor;
    vTexCoord = vertexTexCoord;
    highp vec4 pos = vec4(vertexPosition.x, vertexPosition.y, 0.0, 1.0);
    gl_Position = uTransform * pos;
}";

const FRAGMENT_SHADER: &str = "#version 100
varying highp vec2 vTexCoord;
varying lowp vec4 vVertexColor;
uniform sampler2D uTexture;
void main()
{
    lowp vec4 col = texture2D(uTexture, vTexCoord);
    gl_FragColor = col * vVertexColor;
}";

pub struct GLRenderer {
    // `GLRenderer` is deliberately simple: it keeps one shader program plus shared VBO/IBO state
    // for ordinary UI draws, and falls back to explicit immediate draw calls for custom work that
    // cannot be folded into the batch easily.
    gl: Arc<glow::Context>,
    verts: Vec<Vertex>,
    indices: Vec<u16>,

    vbo: NativeBuffer,
    ibo: NativeBuffer,
    tex_o: NativeTexture,

    program: NativeProgram,

    width: u32,
    height: u32,

    atlas: AtlasHandle,
    textures: HashMap<TextureId, NativeTexture>,
}

trait GlFrameOps {
    fn begin(&mut self, width: i32, height: i32, clr: Color);
    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex);
    fn push_triangle_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex);
    fn flush(&mut self);
    fn end(&mut self);
    /// Uploads pixels using the immutable dimensions carried by the renderer-issued ID.
    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> Result<(), TextureError>;
    fn destroy_texture(&mut self, id: TextureId);
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]);
}

impl GLRenderer {
    /// Creates and fully uploads one atlas texture without changing the renderer's active atlas.
    ///
    /// Keeping allocation behind this helper gives atlas replacement a transaction boundary: the
    /// existing texture remains bound to future frames unless all GL setup and upload work for the
    /// candidate succeeds.
    fn create_atlas_texture(&self, atlas: &AtlasHandle) -> Result<NativeTexture, AtlasUploadError> {
        unsafe {
            let texture_gl = self.gl.clone();
            let texture = ResourceGuard::new(
                self.gl
                    .create_texture()
                    .map_err(|error| AtlasUploadError::new(format!("failed to create replacement atlas texture: {error}")))?,
                move |texture| texture_gl.delete_texture(texture),
            );
            self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture.get()));
            self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            atlas.apply_pixels(|width, height, pixels| {
                let bytes = slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), pixels.len() * 4);
                self.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    PixelUnpackData::Slice(Some(bytes)),
                );
            });
            let error = self.gl.get_error();
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            if error != glow::NO_ERROR {
                // The guard deletes the failed candidate while the renderer retains its old atlas.
                return Err(AtlasUploadError::new(format!(
                    "failed to upload replacement atlas texture: GL error 0x{error:04X}"
                )));
            }
            Ok(texture.into_inner())
        }
    }

    /// Returns the atlas UV used as a "white texel" when drawing solid-colored primitives.
    fn white_uv_center(&self) -> Vec2f {
        let atlas = self.get_atlas();
        let rect = atlas.get_icon_rect(atlas.white_icon());
        let dim = atlas.get_texture_dimension();
        let rect_min = Vec2f::new(rect.x as f32, rect.y as f32);
        let rect_extent = Vec2f::new(rect.width as f32, rect.height as f32);
        let texture_extent = Vec2f::new(dim.width as f32, dim.height as f32);
        (rect_min + rect_extent * 0.5) / texture_extent
    }

    /// Converts a UI clip rectangle into GL scissor coordinates with bottom-left origin.
    fn scissor_from_ui(&self, clip: Recti) -> Option<(i32, i32, i32, i32)> {
        if clip.width <= 0 || clip.height <= 0 {
            return None;
        }
        let x = clip.x;
        let y = (self.height as i32).saturating_sub(clip.y + clip.height);
        Some((x, y, clip.width, clip.height))
    }

    pub fn new(gl: Arc<glow::Context>, atlas: AtlasHandle, width: u32, height: u32) -> Result<Self, String> {
        assert_eq!(core::mem::size_of::<Vertex>(), 20);
        unsafe {
            // Each driver object stays guarded until all four persistent resources exist. Any
            // later GL/shader error therefore releases everything acquired earlier instead of
            // leaking a partially constructed renderer.
            let texture_gl = gl.clone();
            let atlas_texture = ResourceGuard::new(
                gl.create_texture().map_err(|err| format!("failed to create atlas texture: {err}"))?,
                move |texture| texture_gl.delete_texture(texture),
            );
            debug_assert!(gl.get_error() == 0);
            gl.bind_texture(glow::TEXTURE_2D, Some(*atlas_texture.get()));
            debug_assert!(gl.get_error() == 0);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            debug_assert!(gl.get_error() == 0);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            debug_assert!(gl.get_error() == 0);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);

            atlas.apply_pixels(|width, height, pixels| {
                let pixel_slice = slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), pixels.len() * 4);
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    PixelUnpackData::Slice(Some(pixel_slice)),
                );
            });
            let atlas_error = gl.get_error();
            if atlas_error != glow::NO_ERROR {
                return Err(format!("failed to upload immutable atlas texture: GL error 0x{atlas_error:04X}"));
            }
            gl.bind_texture(glow::TEXTURE_2D, None);

            let vertex_gl = gl.clone();
            let vertex_buffer = ResourceGuard::new(
                gl.create_buffer().map_err(|err| format!("failed to create vertex buffer: {err}"))?,
                move |buffer| vertex_gl.delete_buffer(buffer),
            );
            let index_gl = gl.clone();
            let index_buffer = ResourceGuard::new(
                gl.create_buffer().map_err(|err| format!("failed to create index buffer: {err}"))?,
                move |buffer| index_gl.delete_buffer(buffer),
            );

            let program_gl = gl.clone();
            let program = ResourceGuard::new(
                create_program(&gl, VERTEX_SHADER, FRAGMENT_SHADER).map_err(|err| format!("failed to create UI program: {err}"))?,
                move |program| program_gl.delete_program(program),
            );

            // Construction is now infallible, so transfer all guarded handles together to the
            // renderer's deterministic `Drop` implementation.
            let tex_o = atlas_texture.into_inner();
            let vbo = vertex_buffer.into_inner();
            let ibo = index_buffer.into_inner();
            let program = program.into_inner();

            Ok(Self {
                gl,
                verts: Vec::new(),
                indices: Vec::new(),

                vbo,
                ibo,
                tex_o,
                program,

                width,
                height,
                atlas,
                textures: HashMap::new(),
            })
        }
    }
}

impl GlFrameOps for GLRenderer {
    /// Flushes the accumulated UI quad batch through the shared atlas pipeline.
    fn flush(&mut self) {
        if self.verts.is_empty() || self.indices.is_empty() {
            return;
        }

        let gl = &self.gl;
        unsafe {
            // Configure fixed-function state for alpha-blended 2D UI.
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            gl.scissor(0, 0, self.width as i32, self.height as i32);
            gl.enable(glow::BLEND);
            debug_assert!(gl.get_error() == 0);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::CULL_FACE);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::DEPTH_TEST);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::SCISSOR_TEST);
            debug_assert!(gl.get_error() == 0);

            // Bind the shared UI shader program.
            gl.use_program(Some(self.program));
            debug_assert!(gl.get_error() == 0);

            // Bind the atlas texture on texture unit 0.
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_o));
            gl.active_texture(glow::TEXTURE0);
            let tex_uniform_id = gl.get_uniform_location(self.program, "uTexture").unwrap();
            gl.uniform_1_i32(Some(&tex_uniform_id), 0);
            debug_assert_eq!(gl.get_error(), 0);

            // Upload the orthographic transform that maps UI pixel coordinates into clip space.
            let viewport = gl.get_uniform_location(self.program, "uTransform").unwrap();
            let tm = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
            let tm_ptr = tm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&viewport), false, slice);
            debug_assert_eq!(gl.get_error(), 0);

            // Resolve attribute locations and bind the shared vertex/index buffers.
            let pos_attrib_id = gl.get_attrib_location(self.program, "vertexPosition").unwrap();
            let tex_attrib_id = gl.get_attrib_location(self.program, "vertexTexCoord").unwrap();
            let col_attrib_id = gl.get_attrib_location(self.program, "vertexColor").unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));
            debug_assert!(gl.get_error() == 0);

            // Stream the current CPU-side batch into the GL buffers.
            let vertices_u8: &[u8] = core::slice::from_raw_parts(self.verts.as_ptr() as *const u8, self.verts.len() * core::mem::size_of::<Vertex>());
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
            debug_assert!(gl.get_error() == 0);

            // Indices are streamed separately because the UI batch is stored as de-duplicated quads.
            let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u16>());
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
            debug_assert!(gl.get_error() == 0);

            // Vertex layout matches `Vertex { pos, uv, color }`.
            gl.enable_vertex_attrib_array(pos_attrib_id);
            gl.enable_vertex_attrib_array(tex_attrib_id);
            gl.enable_vertex_attrib_array(col_attrib_id);
            debug_assert!(gl.get_error() == 0);

            gl.vertex_attrib_pointer_f32(pos_attrib_id, 2, glow::FLOAT, false, 20, 0);
            gl.vertex_attrib_pointer_f32(tex_attrib_id, 2, glow::FLOAT, false, 20, 8);
            gl.vertex_attrib_pointer_f32(col_attrib_id, 4, glow::UNSIGNED_BYTE, true, 20, 16);
            debug_assert!(gl.get_error() == 0);

            // One indexed draw submits the whole accumulated batch.
            gl.draw_elements(glow::TRIANGLES, self.indices.len() as i32, glow::UNSIGNED_SHORT, 0);
            debug_assert!(gl.get_error() == 0);

            gl.disable_vertex_attrib_array(pos_attrib_id);
            gl.disable_vertex_attrib_array(tex_attrib_id);
            gl.disable_vertex_attrib_array(col_attrib_id);
            debug_assert!(gl.get_error() == 0);
            gl.use_program(None);
            debug_assert!(gl.get_error() == 0);

            // The batch was consumed; start clean next frame / next flush.
            self.verts.clear();
            self.indices.clear();
        }
    }

    /// Appends one quad to the UI batch, flushing first if the `u16` index budget would overflow.
    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        if self.verts.len() + 4 >= 65536 || self.indices.len() + 6 >= 65536 {
            self.flush();
        }

        let is = self.verts.len() as u16;
        self.indices.push(is);
        self.indices.push(is + 1);
        self.indices.push(is + 2);
        self.indices.push(is + 2);
        self.indices.push(is + 3);
        self.indices.push(is);

        self.verts.push(*v0);
        self.verts.push(*v1);
        self.verts.push(*v2);
        self.verts.push(*v3);
    }

    /// Appends one triangle to the normal indexed UI batch, flushing first if the `u16` budget
    /// would overflow.
    fn push_triangle_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex) {
        if self.verts.len() + 3 >= 65536 || self.indices.len() + 3 >= 65536 {
            self.flush();
        }

        let is = self.verts.len() as u16;
        self.indices.push(is);
        self.indices.push(is + 1);
        self.indices.push(is + 2);

        self.verts.push(*v0);
        self.verts.push(*v1);
        self.verts.push(*v2);
    }

    /// Starts a new GL frame by clearing the backbuffer and updating cached size.
    fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.width = width as u32;
        self.height = height as u32;
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            gl.scissor(0, 0, self.width as i32, self.height as i32);
            gl.clear_color(clr.r as f32 / 255.0, clr.g as f32 / 255.0, clr.b as f32 / 255.0, clr.a as f32 / 255.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            debug_assert!(gl.get_error() == 0);
        }
    }

    /// Finishes the frame by flushing any remaining batched UI geometry.
    fn end(&mut self) {
        self.flush();
    }

    /// Creates a GL texture for a backend-owned external image.
    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> Result<(), TextureError> {
        // The opaque capability is the sole dimension source validated by the Context executor.
        let dimensions = id.size();

        // Reserving the map entry can fail before any GL resource exists. Preserve that allocation
        // failure as backend context so callers receive the renderer's structured texture error.
        self.textures
            .try_reserve(1)
            .map_err(|err| TextureError::backend(format!("failed to reserve texture ownership entry: {err}")))?;
        let gl = &self.gl;
        unsafe {
            // User textures share the same nearest-neighbor setup as the atlas. Wrap allocation
            // failures immediately, before installing the resource guard, because no native
            // handle exists for the guard to own in that case.
            let texture_gl = gl.clone();
            let texture = ResourceGuard::new(
                gl.create_texture()
                    .map_err(|err| TextureError::backend(format!("failed to create texture: {err}")))?,
                move |texture| texture_gl.delete_texture(texture),
            );
            gl.bind_texture(glow::TEXTURE_2D, Some(*texture.get()));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                dimensions.width,
                dimensions.height,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(Some(pixels)),
            );
            let err = gl.get_error();
            if err != 0 {
                // Restore the binding before returning. `texture` remains guarded here, so the
                // failed native object is deleted while the driver code is surfaced as backend
                // error detail rather than being collapsed into an untyped string.
                gl.bind_texture(glow::TEXTURE_2D, None);
                return Err(TextureError::backend(format!("OpenGL texture upload failed with error 0x{err:04x}")));
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            // IDs are normally unique, but replacing defensively keeps the ownership map sound if
            // a caller retries an existing capability.
            let previous = self.textures.insert(id, *texture.get());
            let _texture = texture.into_inner();
            if let Some(previous) = previous {
                gl.delete_texture(previous);
            }
        }
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(tex) = self.textures.remove(&id) {
            unsafe {
                self.gl.delete_texture(tex);
            }
        }
    }

    /// Draws one pre-clipped textured quad using a backend-owned texture outside the atlas batch.
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        let tex = match self.textures.get(&id) {
            Some(tex) => *tex,
            None => return,
        };
        // External textures cannot be folded into the atlas batch because they change the bound
        // GL texture object. The Context executor has already clipped the vertices, so the one-off draw uses
        // the full framebuffer scissor and relies on the submitted quad geometry for clipping.
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            gl.scissor(0, 0, self.width as i32, self.height as i32);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::SCISSOR_TEST);

            gl.use_program(Some(self.program));
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.active_texture(glow::TEXTURE0);
            let tex_uniform_id = gl.get_uniform_location(self.program, "uTexture").unwrap();
            gl.uniform_1_i32(Some(&tex_uniform_id), 0);

            let viewport = gl.get_uniform_location(self.program, "uTransform").unwrap();
            let tm = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
            let tm_ptr = tm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&viewport), false, slice);

            let pos_attrib_id = gl.get_attrib_location(self.program, "vertexPosition").unwrap();
            let tex_attrib_id = gl.get_attrib_location(self.program, "vertexTexCoord").unwrap();
            let col_attrib_id = gl.get_attrib_location(self.program, "vertexColor").unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));

            let vertices_u8: &[u8] = core::slice::from_raw_parts(vertices.as_ptr() as *const u8, vertices.len() * core::mem::size_of::<Vertex>());
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);

            // Expand the quad into two triangles in-place.
            let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];
            let indices_u8: &[u8] = core::slice::from_raw_parts(indices.as_ptr() as *const u8, indices.len() * core::mem::size_of::<u16>());
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);

            gl.enable_vertex_attrib_array(pos_attrib_id);
            gl.enable_vertex_attrib_array(tex_attrib_id);
            gl.enable_vertex_attrib_array(col_attrib_id);
            gl.vertex_attrib_pointer_f32(pos_attrib_id, 2, glow::FLOAT, false, 20, 0);
            gl.vertex_attrib_pointer_f32(tex_attrib_id, 2, glow::FLOAT, false, 20, 8);
            gl.vertex_attrib_pointer_f32(col_attrib_id, 4, glow::UNSIGNED_BYTE, true, 20, 16);

            gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_SHORT, 0);

            gl.disable_vertex_attrib_array(pos_attrib_id);
            gl.disable_vertex_attrib_array(tex_attrib_id);
            gl.disable_vertex_attrib_array(col_attrib_id);
            gl.use_program(None);
        }
    }
}

impl Drop for GLRenderer {
    /// Releases every persistent GL object, including user textures that remain live at shutdown.
    fn drop(&mut self) {
        unsafe {
            for (_, texture) in self.textures.drain() {
                self.gl.delete_texture(texture);
            }
            self.gl.delete_program(self.program);
            self.gl.delete_buffer(self.ibo);
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_texture(self.tex_o);
        }
    }
}

#[must_use = "the OpenGL frame is finalized when dropped"]
pub struct GlFrame<'a> {
    backend: &'a mut GLRenderer,
}

impl GlFrame<'_> {
    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        self.backend.enqueue_colored_vertices(area, vertices);
    }

    pub fn enqueue_mesh_draw(&mut self, area: CustomRenderArea, submission: MeshSubmission) {
        self.backend.enqueue_mesh_draw(area, submission);
    }
}

impl RendererFrame for GlFrame<'_> {
    fn push_quad(&mut self, vertices: [Vertex; 4]) {
        GlFrameOps::push_quad_vertices(self.backend, &vertices[0], &vertices[1], &vertices[2], &vertices[3]);
    }

    fn push_triangle(&mut self, vertices: [Vertex; 3]) {
        GlFrameOps::push_triangle_vertices(self.backend, &vertices[0], &vertices[1], &vertices[2]);
    }

    fn flush(&mut self) {
        GlFrameOps::flush(self.backend);
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        GlFrameOps::draw_texture(self.backend, id, vertices);
    }
}

impl Drop for GlFrame<'_> {
    fn drop(&mut self) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            GlFrameOps::end(self.backend);
        }))
        .is_err()
        {
            eprintln!("[microui-redux][glow] frame finalization panicked");
        }
    }
}

impl RendererBackend for GLRenderer {
    type Frame<'a> = GlFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), AtlasUploadError> {
        // Upload into a separate texture first. Only a complete candidate may displace the live GL
        // object and matching CPU atlas capability.
        let texture = self.create_atlas_texture(&atlas)?;
        let previous = std::mem::replace(&mut self.tex_o, texture);
        self.atlas = atlas;
        unsafe {
            self.gl.delete_texture(previous);
        }
        Ok(())
    }

    fn frame(&mut self, info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        let dimensions = info.dimensions();
        GlFrameOps::begin(self, dimensions.width, dimensions.height, info.clear());
        Ok(GlFrame { backend: self })
    }

    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> Result<(), TextureError> {
        // Keep the public backend boundary typed; the frame-op layer has already translated each
        // allocation or driver failure into the appropriate backend texture error.
        GlFrameOps::create_texture(self, id, pixels)
    }

    fn destroy_texture(&mut self, id: TextureId) {
        GlFrameOps::destroy_texture(self, id);
    }
}

impl GLRenderer {
    /// Flushes the UI batch and hands control to a backend-specific custom GL recorder.
    pub(crate) fn enqueue_custom_render<C: GLCustomRenderer + 'static>(&mut self, area: CustomRenderArea, mut cmd: C) {
        self.flush();
        cmd.record(&self.gl, (self.width, self.height), &area);
    }

    /// Draws arbitrary colored triangles while preserving the clip rectangle carried by `area`.
    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        self.draw_colored_vertices(area.clip, vertices.as_slice());
    }

    /// Draws arbitrary colored triangles by sampling a white atlas texel and modulating by vertex color.
    fn draw_colored_vertices(&mut self, clip: Recti, vertices: &[Vertex]) {
        if vertices.is_empty() {
            return;
        }
        self.flush();
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            if let Some((sx, sy, sw, sh)) = self.scissor_from_ui(clip) {
                // Custom colored draws still respect the logical UI clip rectangle.
                gl.scissor(sx, sy, sw, sh);
            }
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::SCISSOR_TEST);

            gl.use_program(Some(self.program));
            // Sample the atlas so the shader path stays identical to normal UI; UVs point at white.
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_o));
            gl.active_texture(glow::TEXTURE0);
            if let Some(tex_uniform_id) = gl.get_uniform_location(self.program, "uTexture") {
                gl.uniform_1_i32(Some(&tex_uniform_id), 0);
            }

            if let Some(viewport) = gl.get_uniform_location(self.program, "uTransform") {
                let tm = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
                let tm_ptr = tm.col.as_ptr() as *const _ as *const f32;
                let slice = std::slice::from_raw_parts(tm_ptr, 16);
                gl.uniform_matrix_4_f32_slice(Some(&viewport), false, slice);
            }

            let pos_attrib_id = gl.get_attrib_location(self.program, "vertexPosition").unwrap();
            let tex_attrib_id = gl.get_attrib_location(self.program, "vertexTexCoord").unwrap();
            let col_attrib_id = gl.get_attrib_location(self.program, "vertexColor").unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            // Colored draws are emitted with non-indexed triangles, so only ARRAY_BUFFER is needed.
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);

            let vertices_u8: &[u8] = core::slice::from_raw_parts(vertices.as_ptr() as *const u8, std::mem::size_of_val(vertices));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);

            gl.enable_vertex_attrib_array(pos_attrib_id);
            gl.enable_vertex_attrib_array(tex_attrib_id);
            gl.enable_vertex_attrib_array(col_attrib_id);
            gl.vertex_attrib_pointer_f32(pos_attrib_id, 2, glow::FLOAT, false, 20, 0);
            gl.vertex_attrib_pointer_f32(tex_attrib_id, 2, glow::FLOAT, false, 20, 8);
            gl.vertex_attrib_pointer_f32(col_attrib_id, 4, glow::UNSIGNED_BYTE, true, 20, 16);

            gl.draw_arrays(glow::TRIANGLES, 0, vertices.len() as i32);

            gl.disable_vertex_attrib_array(pos_attrib_id);
            gl.disable_vertex_attrib_array(tex_attrib_id);
            gl.disable_vertex_attrib_array(col_attrib_id);
            gl.use_program(None);
            // Restore full-frame scissor so later UI draws do not inherit this clip.
            gl.scissor(0, 0, self.width as i32, self.height as i32);
        }
    }

    /// Converts a mesh submission into colored screen-space triangles and reuses the colored-vertex path.
    pub fn enqueue_mesh_draw(&mut self, _area: CustomRenderArea, _submission: MeshSubmission) {
        if _submission.mesh.is_empty() {
            return;
        }
        // Early exit if the rect is empty; nothing to draw.
        if _area.rect.width <= 0 || _area.rect.height <= 0 {
            return;
        }
        // The demo mesh path shades from normals on the CPU and samples a white atlas texel.
        let white_uv = self.white_uv_center();
        #[derive(Clone)]
        struct Tri {
            depth: f32,
            verts: [Vertex; 3],
        }
        let mut tris: Vec<Tri> = Vec::with_capacity(_submission.mesh.indices().len() / 3);

        let mesh = &_submission.mesh;
        let pvm = &_submission.pvm;
        let indices = mesh.indices();
        let positions = mesh.vertices();

        // Interpret only complete index triples; an incomplete tail cannot describe a triangle
        // and is ignored exactly as it was by the former `chunks_exact` traversal.
        let (triangles, _) = indices.as_chunks::<3>();
        for tri in triangles {
            let idxs = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let mut clip_space = [Vec4f::default(); 3];
            for (dst, src_idx) in clip_space.iter_mut().zip(&idxs) {
                let v = &positions[*src_idx];
                *dst = *pvm * Vec4f::new(v.position[0], v.position[1], v.position[2], 1.0);
            }
            // Basic backface culling in clip space keeps the software path inexpensive.
            let a = clip_space[0];
            let b = clip_space[1];
            let c = clip_space[2];
            let ab = Vec3f::new(b.x - a.x, b.y - a.y, b.z - a.z);
            let ac = Vec3f::new(c.x - a.x, c.y - a.y, c.z - a.z);
            let cross = Vec3f::cross(&ab, &ac);
            if cross.z <= 0.0 {
                continue;
            }

            let mut verts = [Vertex::new(Vec2f::default(), white_uv, color4b(0, 0, 0, 255)); 3];
            let mut depth_acc = 0.0;
            let mut valid = true;

            for ((clip, src_idx), out_v) in clip_space.iter().zip(&idxs).zip(verts.iter_mut()) {
                if clip.w.abs() < 1e-5 {
                    valid = false;
                    break;
                }
                let ndc = Vec3f::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
                depth_acc += ndc.z;
                let sx = _area.rect.x as f32 + (ndc.x * 0.5 + 0.5) * _area.rect.width as f32;
                let sy = _area.rect.y as f32 + (-ndc.y * 0.5 + 0.5) * _area.rect.height as f32;

                let v = &positions[*src_idx];
                let normal = Vec3f::new(v.normal[0], v.normal[1], v.normal[2]);
                let color = (normal * 0.5) + Vec3f::new(0.5, 0.5, 0.5);
                let r = (color.x.clamp(0.0, 1.0) * 255.0) as u8;
                let g = (color.y.clamp(0.0, 1.0) * 255.0) as u8;
                let b = (color.z.clamp(0.0, 1.0) * 255.0) as u8;

                *out_v = Vertex::new(Vec2f::new(sx, sy), white_uv, color4b(r, g, b, 255));
            }

            if valid {
                tris.push(Tri { depth: depth_acc / 3.0, verts });
            }
        }

        // Painter's algorithm is sufficient here because the GL UI pass does not keep a depth buffer.
        tris.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));

        let mut verts: Vec<Vertex> = Vec::with_capacity(tris.len() * 3);
        for tri in tris {
            verts.extend_from_slice(&tri.verts);
        }

        self.draw_colored_vertices(_area.clip, verts.as_slice());
    }
}

/// Compiles and links a GL program from vertex/fragment shader sources.
pub fn create_program(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<NativeProgram, io::Error> {
    unsafe {
        // Program and shader guards cover allocation errors, compile/link errors, and unwinding.
        // Handles leave these guards only after a successful link.
        let program = ResourceGuard::new(gl.create_program().map_err(io::Error::other)?, |program| gl.delete_program(program));

        let shader_sources = [(glow::VERTEX_SHADER, vertex_shader_source), (glow::FRAGMENT_SHADER, fragment_shader_source)];

        let mut shaders = Vec::with_capacity(shader_sources.len());

        // Compile both stages first so we can bail out with a useful shader log if needed.
        for (shader_type, shader_source) in shader_sources.iter() {
            let shader = ResourceGuard::new(gl.create_shader(*shader_type).map_err(io::Error::other)?, |shader| gl.delete_shader(shader));
            gl.shader_source(*shader.get(), shader_source);
            gl.compile_shader(*shader.get());
            if !gl.get_shader_compile_status(*shader.get()) {
                return Err(io::Error::other(gl.get_shader_info_log(*shader.get())));
            }
            gl.attach_shader(*program.get(), *shader.get());
            shaders.push(shader);
        }

        // Link once both stages compiled successfully.
        gl.link_program(*program.get());
        if !gl.get_program_link_status(*program.get()) {
            return Err(io::Error::other(gl.get_program_info_log(*program.get())));
        }

        for shader in shaders {
            let shader = shader.into_inner();
            gl.detach_shader(*program.get(), shader);
            gl.delete_shader(shader);
        }

        Ok(program.into_inner())
    }
}
