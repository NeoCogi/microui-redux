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

use core::slice;
use std::{collections::HashMap, io, sync::Arc, usize};

use microui_redux::*;
use glow::*;
use rs_math3d::{Vec3f, Vec4f};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone)]
pub struct MeshBuffers {
    vertices: Arc<[MeshVertex]>,
    indices: Arc<[u32]>,
}

impl MeshBuffers {
    pub fn from_vecs(vertices: Vec<MeshVertex>, indices: Vec<u32>) -> Self {
        Self {
            vertices: vertices.into(),
            indices: indices.into(),
        }
    }

    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.indices.is_empty()
    }
}

#[derive(Clone)]
pub struct MeshSubmission {
    pub mesh: MeshBuffers,
    pub pvm: Mat4f,
    pub view_model: Mat4f,
}

#[derive(Clone, Copy)]
pub struct CustomRenderArea {
    pub rect: Recti,
    pub clip: Recti,
}

pub(crate) trait GLCustomRenderer {
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
    last_update_id: usize,
    textures: HashMap<TextureId, NativeTexture>,
}

impl GLRenderer {
    fn white_uv_center(&self) -> Vec2f {
        let atlas = self.get_atlas();
        let rect = atlas.get_icon_rect(WHITE_ICON);
        let dim = atlas.get_texture_dimension();
        Vec2f::new(
            (rect.x as f32 + rect.width as f32 * 0.5) / dim.width as f32,
            (rect.y as f32 + rect.height as f32 * 0.5) / dim.height as f32,
        )
    }

    fn scissor_from_ui(&self, clip: Recti) -> Option<(i32, i32, i32, i32)> {
        if clip.width <= 0 || clip.height <= 0 {
            return None;
        }
        let x = clip.x;
        let y = (self.height as i32).saturating_sub(clip.y + clip.height);
        Some((x, y, clip.width, clip.height))
    }

    fn update_atlas(&mut self) {
        let gl = &self.gl;
        if self.last_update_id != self.atlas.get_last_update_id() {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_o));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                debug_assert!(gl.get_error() == 0);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                debug_assert!(gl.get_error() == 0);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, 0);
                debug_assert!(gl.get_error() == 0);

                // we are going to pass a pointer, hold the atlas pixels in memory since it returns a copy
                self.atlas.apply_pixels(|width, height, pixels| {
                    let pixel_ptr = pixels.as_ptr() as *const u8;
                    let pixel_slice: &[u8] = slice::from_raw_parts(pixel_ptr, pixels.len() * 4);
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
                    debug_assert!(gl.get_error() == 0);
                });
            }
            self.last_update_id = self.atlas.get_last_update_id()
        }
    }

    pub fn new(gl: Arc<glow::Context>, atlas: AtlasHandle, width: u32, height: u32) -> Self {
        assert_eq!(core::mem::size_of::<Vertex>(), 20);
        unsafe {
            // init texture
            let tex_o = gl.create_texture().unwrap();
            debug_assert!(gl.get_error() == 0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex_o));
            debug_assert!(gl.get_error() == 0);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            debug_assert!(gl.get_error() == 0);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            debug_assert!(gl.get_error() == 0);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);

            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                atlas.width() as i32,
                atlas.height() as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(None),
            );
            debug_assert!(gl.get_error() == 0);
            gl.bind_texture(glow::TEXTURE_2D, None);

            let vbo = gl.create_buffer().unwrap();
            let ibo = gl.create_buffer().unwrap();

            let program = create_program(&gl, VERTEX_SHADER, FRAGMENT_SHADER).unwrap();

            Self {
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
                last_update_id: usize::MAX,
                textures: HashMap::new(),
            }
        }
    }
}

impl Renderer for GLRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn flush(&mut self) {
        self.update_atlas();
        if self.verts.len() == 0 || self.indices.len() == 0 {
            return;
        }

        let gl = &self.gl;
        unsafe {
            // opengl rendering states
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

            // set the program
            gl.use_program(Some(self.program));
            debug_assert!(gl.get_error() == 0);

            // set the texture
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_o));
            gl.active_texture(glow::TEXTURE0 + 0);
            let tex_uniform_id = gl.get_uniform_location(self.program, "uTexture").unwrap();
            gl.uniform_1_i32(Some(&tex_uniform_id), 0);
            debug_assert_eq!(gl.get_error(), 0);

            // set the viewport
            let viewport = gl.get_uniform_location(self.program, "uTransform").unwrap();
            let tm = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
            let tm_ptr = tm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&viewport), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            // set the vertex buffer
            let pos_attrib_id = gl.get_attrib_location(self.program, "vertexPosition").unwrap();
            let tex_attrib_id = gl.get_attrib_location(self.program, "vertexTexCoord").unwrap();
            let col_attrib_id = gl.get_attrib_location(self.program, "vertexColor").unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));
            debug_assert!(gl.get_error() == 0);

            // update the vertex buffer
            let vertices_u8: &[u8] = core::slice::from_raw_parts(self.verts.as_ptr() as *const u8, self.verts.len() * core::mem::size_of::<Vertex>());
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
            debug_assert!(gl.get_error() == 0);

            // update the index buffer
            let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u16>());
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
            debug_assert!(gl.get_error() == 0);

            gl.enable_vertex_attrib_array(pos_attrib_id);
            gl.enable_vertex_attrib_array(tex_attrib_id);
            gl.enable_vertex_attrib_array(col_attrib_id);
            debug_assert!(gl.get_error() == 0);

            gl.vertex_attrib_pointer_f32(pos_attrib_id, 2, glow::FLOAT, false, 20, 0);
            gl.vertex_attrib_pointer_f32(tex_attrib_id, 2, glow::FLOAT, false, 20, 8);
            gl.vertex_attrib_pointer_f32(col_attrib_id, 4, glow::UNSIGNED_BYTE, true, 20, 16);
            debug_assert!(gl.get_error() == 0);

            gl.draw_elements(glow::TRIANGLES, self.indices.len() as i32, glow::UNSIGNED_SHORT, 0);
            debug_assert!(gl.get_error() == 0);

            gl.disable_vertex_attrib_array(pos_attrib_id);
            gl.disable_vertex_attrib_array(tex_attrib_id);
            gl.disable_vertex_attrib_array(col_attrib_id);
            debug_assert!(gl.get_error() == 0);
            gl.use_program(None);
            debug_assert!(gl.get_error() == 0);

            self.verts.clear();
            self.indices.clear();
        }
    }

    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        if self.verts.len() + 4 >= 65536 || self.indices.len() + 6 >= 65536 {
            self.flush();
        }

        let is = self.verts.len() as u16;
        self.indices.push(is + 0);
        self.indices.push(is + 1);
        self.indices.push(is + 2);
        self.indices.push(is + 2);
        self.indices.push(is + 3);
        self.indices.push(is + 0);

        self.verts.push(v0.clone());
        self.verts.push(v1.clone());
        self.verts.push(v2.clone());
        self.verts.push(v3.clone());

        // This is needed so that the update happens immediately (not the most optimized way)
        if self.last_update_id != self.atlas.get_last_update_id() {
            self.flush()
        }
    }

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

    fn end(&mut self) {
        self.flush();
    }

    fn create_texture(&mut self, id: TextureId, width: i32, height: i32, pixels: &[u8]) {
        let gl = &self.gl;
        unsafe {
            let tex = gl.create_texture().expect("failed to create texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                width,
                height,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(Some(pixels)),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
            self.textures.insert(id, tex);
        }
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(tex) = self.textures.remove(&id) {
            unsafe {
                self.gl.delete_texture(tex);
            }
        }
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        let tex = match self.textures.get(&id) {
            Some(tex) => *tex,
            None => return,
        };
        self.flush();
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
            gl.active_texture(glow::TEXTURE0 + 0);
            let tex_uniform_id = gl.get_uniform_location(self.program, "uTexture").unwrap();
            gl.uniform_1_i32(Some(&tex_uniform_id), 0);

            let viewport = gl.get_uniform_location(self.program, "uTransform").unwrap();
            let tm = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
            let tm_ptr = tm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&viewport), false, &slice);

            let pos_attrib_id = gl.get_attrib_location(self.program, "vertexPosition").unwrap();
            let tex_attrib_id = gl.get_attrib_location(self.program, "vertexTexCoord").unwrap();
            let col_attrib_id = gl.get_attrib_location(self.program, "vertexColor").unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));

            let vertices_u8: &[u8] = core::slice::from_raw_parts(vertices.as_ptr() as *const u8, vertices.len() * core::mem::size_of::<Vertex>());
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);

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

impl GLRenderer {
    pub(crate) fn enqueue_custom_render<C: GLCustomRenderer + 'static>(&mut self, area: CustomRenderArea, mut cmd: C) {
        self.flush();
        cmd.record(&self.gl, (self.width, self.height), &area);
    }

    pub fn enqueue_colored_vertices(&mut self, area: CustomRenderArea, vertices: Vec<Vertex>) {
        if vertices.is_empty() {
            return;
        }
        self.flush();
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            if let Some((sx, sy, sw, sh)) = self.scissor_from_ui(area.clip) {
                gl.scissor(sx, sy, sw, sh);
            }
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::SCISSOR_TEST);

            gl.use_program(Some(self.program));
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_o));
            gl.active_texture(glow::TEXTURE0 + 0);
            if let Some(tex_uniform_id) = gl.get_uniform_location(self.program, "uTexture") {
                gl.uniform_1_i32(Some(&tex_uniform_id), 0);
            }

            if let Some(viewport) = gl.get_uniform_location(self.program, "uTransform") {
                let tm = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
                let tm_ptr = tm.col.as_ptr() as *const _ as *const f32;
                let slice = std::slice::from_raw_parts(tm_ptr, 16);
                gl.uniform_matrix_4_f32_slice(Some(&viewport), false, &slice);
            }

            let pos_attrib_id = gl.get_attrib_location(self.program, "vertexPosition").unwrap();
            let tex_attrib_id = gl.get_attrib_location(self.program, "vertexTexCoord").unwrap();
            let col_attrib_id = gl.get_attrib_location(self.program, "vertexColor").unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);

            let vertices_u8: &[u8] = core::slice::from_raw_parts(vertices.as_ptr() as *const u8, vertices.len() * core::mem::size_of::<Vertex>());
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
            gl.scissor(0, 0, self.width as i32, self.height as i32);
        }
    }

    pub fn enqueue_mesh_draw(&mut self, _area: CustomRenderArea, _submission: MeshSubmission) {
        if _submission.mesh.is_empty() {
            return;
        }
        // Early exit if the rect is empty; nothing to draw.
        if _area.rect.width <= 0 || _area.rect.height <= 0 {
            return;
        }
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

        for tri in indices.chunks_exact(3) {
            let idxs = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let mut clip_space = [Vec4f::default(); 3];
            for (dst, src_idx) in clip_space.iter_mut().zip(&idxs) {
                let v = &positions[*src_idx];
                *dst = *pvm * Vec4f::new(v.position[0], v.position[1], v.position[2], 1.0);
            }
            // Basic backface culling in clip space (approximate).
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

        // Painter's algorithm: draw farthest triangles first so nearer ones occlude.
        tris.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));

        let mut verts: Vec<Vertex> = Vec::with_capacity(tris.len() * 3);
        for tri in tris {
            verts.extend_from_slice(&tri.verts);
        }

        self.enqueue_colored_vertices(_area, verts);
    }
}

pub fn create_program(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<NativeProgram, io::Error> {
    unsafe {
        let program = gl.create_program().expect("Cannot create program");

        let shader_sources = [(glow::VERTEX_SHADER, vertex_shader_source), (glow::FRAGMENT_SHADER, fragment_shader_source)];

        let mut shaders = Vec::with_capacity(shader_sources.len());

        for (shader_type, shader_source) in shader_sources.iter() {
            let shader = gl.create_shader(*shader_type).expect("Cannot create shader");
            gl.shader_source(shader, shader_source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let error_string = format!("{}", gl.get_shader_info_log(shader));
                for shader in shaders {
                    gl.delete_shader(shader);
                }
                gl.delete_program(program);
                return Err(io::Error::new(io::ErrorKind::Other, error_string));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }

        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let error_string = format!("{}", gl.get_program_info_log(program));
            for shader in shaders {
                gl.delete_shader(shader);
            }
            gl.delete_program(program);
            return Err(io::Error::new(io::ErrorKind::Other, error_string));
        }

        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }

        Ok(program)
    }
}

pub fn get_active_program_attributes(gl: &glow::Context, program: NativeProgram) -> Vec<ActiveAttribute> {
    let mut attribs = Vec::new();
    unsafe {
        let attrib_count = gl.get_active_attributes(program);
        for index in 0..attrib_count {
            let attr = gl.get_active_attribute(program, index);
            match attr {
                Some(attr) => attribs.push(attr),
                _ => (),
            }
        }
    }
    attribs
}

pub fn get_active_program_uniforms(gl: &glow::Context, program: NativeProgram) -> Vec<ActiveUniform> {
    let mut unis = Vec::new();
    unsafe {
        let attrib_count = gl.get_active_uniforms(program);
        for index in 0..attrib_count {
            let uni = gl.get_active_uniform(program, index);
            match uni {
                Some(uni) => unis.push(uni),
                _ => (),
            }
        }
    }
    unis
}

static SOLID_VERTEX_SHADER: &'static str = "#version 100
uniform highp mat4 pvm;
attribute highp vec3 position;
void main()
{
    highp vec4 pos = vec4(position.xyz, 1.0);
    gl_Position = pvm * pos;
}";

static SOLID_PIXEL_SHADER: &'static str = "#version 100
void main()
{
    gl_FragColor = vec4(0.0, 0.5, 1.0, 1.0);
}";

static POLYMESH_VERTEX_SHADER: &'static str = "
#version 300 es
in highp    vec4        position;
in highp    vec3        normal;
in highp    vec2        uv;

uniform     mat4        pvm;
uniform     mat4        view_model;

out         highp   vec3        v_normal;
out         highp   vec2        v_uv;
out         highp   vec3        v_orig_normal;
out         highp   vec3        v_light_dir;

void main() {
    gl_Position = pvm * vec4(position.xyz, 1.0);
    v_normal    = normalize((view_model * vec4(normal, 0.0)).xyz);
    v_light_dir = normalize((pvm * vec4(1.0, 0.0, 0.0, 1.0))).xyz;
    v_orig_normal   = normal;
    v_uv        = uv;
}";

static POLYMESH_PIXEL_SHADER: &'static str = "
#version 300 es
precision mediump float;

        in highp    vec3        v_normal;
        in highp    vec3        v_orig_normal;
        in highp    vec3        v_light_dir;
        in highp    vec2        v_uv;

uniform highp       sampler2D   u_texture;

layout(location = 0) out lowp  vec4     color_buffer;
layout(location = 1) out highp vec4     normal_buffer;

void main() {
    highp float intensity = dot(v_light_dir, v_normal);
    highp vec4 col  = vec4(v_orig_normal.xyz, 1.0) * 0.9 + vec4(v_uv.xy, 0.0, 1.0) * 0.1;
    lowp vec4 t     = texture(u_texture, v_uv);
    color_buffer    = col * intensity;//uvec4(t * 255.0) + uvec4(vec4(v_normal.xyz, 0.0) * 255.0);
    normal_buffer   = col / 255.0 + t;
    color_buffer.a  = 1.0;
}";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ModelUniforms {
    pub pvm: Mat4f,
    pub view_model: Mat4f,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SolidModelUniforms {
    pub pvm: Mat4f,
}

#[repr(C)]
pub struct PolymeshRenderVertex {
    pub position: Vec3f,
    pub normal: Vec3f,
    pub uv: Vec2f,
}

pub struct PolyMeshRenderer {
    vb: NativeBuffer,
    ib: NativeBuffer,

    model_program: NativeProgram,
    solid_program: NativeProgram,

    max_vertex_count: usize,
    max_index_count: usize,

    model_attribs: HashMap<String, u32>,
    model_uniforms: HashMap<String, NativeUniformLocation>,

    solid_attribs: HashMap<String, u32>,
    solid_uniforms: HashMap<String, NativeUniformLocation>,

    vertices: Vec<PolymeshRenderVertex>,
    indices: Vec<u32>,
}

unsafe impl Sync for PolyMeshRenderer {}
unsafe impl Send for PolyMeshRenderer {}

impl PolyMeshRenderer {
    pub fn create(driver: &glow::Context, max_tri_count: usize) -> Self {
        let max_vertex_count = max_tri_count * 3;
        let max_index_count = max_tri_count * 6;
        let vb = unsafe {
            let buff = driver.create_buffer().unwrap();
            let size = max_vertex_count * std::mem::size_of::<PolymeshRenderVertex>();
            driver.bind_buffer(ARRAY_BUFFER, Some(buff));
            debug_assert!(driver.get_error() == 0);
            driver.buffer_data_size(ARRAY_BUFFER, size as _, glow::DYNAMIC_DRAW);
            debug_assert!(driver.get_error() == 0);
            buff
        };

        let ib = unsafe {
            let buff = driver.create_buffer().unwrap();
            let size = max_index_count * std::mem::size_of::<u32>();
            driver.bind_buffer(ELEMENT_ARRAY_BUFFER, Some(buff));
            debug_assert!(driver.get_error() == 0);
            driver.buffer_data_size(ELEMENT_ARRAY_BUFFER, size as _, glow::DYNAMIC_DRAW);
            debug_assert!(driver.get_error() == 0);
            buff
        };

        let model_program = create_program(driver, POLYMESH_VERTEX_SHADER, POLYMESH_PIXEL_SHADER).unwrap();
        let model_attribs = get_active_program_attributes(driver, model_program)
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i as u32))
            .collect::<HashMap<_, _>>();
        let model_uniforms = get_active_program_uniforms(driver, model_program)
            .iter()
            .map(|e| (e.name.clone(), unsafe { driver.get_uniform_location(model_program, &e.name).unwrap() }))
            .collect::<HashMap<_, _>>();

        let solid_program = create_program(driver, SOLID_VERTEX_SHADER, SOLID_PIXEL_SHADER).unwrap();
        let solid_attribs = get_active_program_attributes(driver, solid_program)
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i as u32))
            .collect::<HashMap<_, _>>();
        let solid_uniforms = get_active_program_uniforms(driver, solid_program)
            .iter()
            .map(|e| (e.name.clone(), unsafe { driver.get_uniform_location(solid_program, &e.name).unwrap() }))
            .collect::<HashMap<_, _>>();

        Self {
            vb,
            ib,

            model_program,
            solid_program,

            max_vertex_count,
            max_index_count,

            model_attribs,
            model_uniforms,

            solid_attribs,
            solid_uniforms,

            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn render_wire<T: PolymeshTrait>(&mut self, gl: &glow::Context, pvm: &Mat4f, _view_model: &Mat4f, pmesh: &T) {
        self.vertices.clear();
        self.indices.clear();

        let pos_attr = self.solid_attribs["position"];

        unsafe {
            gl.disable(glow::BLEND);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::CULL_FACE);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::DEPTH_TEST);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::SCISSOR_TEST);
            debug_assert!(gl.get_error() == 0);

            gl.use_program(Some(self.solid_program));

            gl.enable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);

            let tm_ptr = pvm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&self.solid_uniforms["pvm"]), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vb));
            debug_assert!(gl.get_error() == 0);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ib));
            debug_assert!(gl.get_error() == 0);
        }

        for p in pmesh.polys() {
            let vid = self.vertices.len() as u32;
            let vertex_count = p.vertex_count();
            if self.indices.len() + (vertex_count - 2) * 2 > self.max_index_count || self.vertices.len() + vertex_count > self.max_vertex_count {
                unsafe {
                    // update the vertex buffer
                    let vertices_u8: &[u8] = core::slice::from_raw_parts(
                        self.vertices.as_ptr() as *const u8,
                        self.vertices.len() * core::mem::size_of::<PolymeshRenderVertex>(),
                    );
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    // update the index buffer
                    let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 0);
                    debug_assert!(gl.get_error() == 0);

                    gl.draw_elements(glow::LINES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                    debug_assert!(gl.get_error() == 0);
                }

                self.vertices.clear();
                self.indices.clear();
            }

            for v in p {
                let position = pmesh.get_vertex_position(v.pos());
                self.vertices.push(PolymeshRenderVertex {
                    position,
                    normal: Vec3f::zero(),
                    uv: Vec2f::zero(),
                });
            }

            for i in 0..vertex_count as u32 {
                self.indices.push(vid + i);
                self.indices.push(vid + ((i + 1) % (vertex_count as u32)));
            }
        }

        if self.indices.len() > 0 {
            unsafe {
                // update the vertex buffer
                let vertices_u8: &[u8] = core::slice::from_raw_parts(
                    self.vertices.as_ptr() as *const u8,
                    self.vertices.len() * core::mem::size_of::<PolymeshRenderVertex>(),
                );
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);
                gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 0);
                debug_assert!(gl.get_error() == 0);

                // update the index buffer
                let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);

                gl.draw_elements(glow::LINES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                debug_assert!(gl.get_error() == 0);
            }
        }

        unsafe {
            gl.disable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);
            gl.use_program(None);
            debug_assert!(gl.get_error() == 0);
        }
    }

    pub fn render<T: PolymeshTrait>(&mut self, gl: &glow::Context, pvm: &Mat4f, view_model: &Mat4f, pmesh: &T) {
        self.vertices.clear();
        self.indices.clear();

        let pos_attr = self.model_attribs["position"];
        let norm_attr = self.model_attribs["normal"];
        let uv_attr = self.model_attribs["uv"];

        unsafe {
            gl.disable(glow::BLEND);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::CULL_FACE);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::DEPTH_TEST);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::SCISSOR_TEST);
            debug_assert!(gl.get_error() == 0);

            gl.use_program(Some(self.model_program));

            gl.enable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);
            gl.enable_vertex_attrib_array(norm_attr);
            debug_assert!(gl.get_error() == 0);
            gl.enable_vertex_attrib_array(uv_attr);
            debug_assert!(gl.get_error() == 0);

            let tm_ptr = pvm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&self.model_uniforms["pvm"]), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            let tm_ptr = view_model.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&self.model_uniforms["view_model"]), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vb));
            debug_assert!(gl.get_error() == 0);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ib));
            debug_assert!(gl.get_error() == 0);
        }

        for p in pmesh.polys() {
            let vid = self.vertices.len() as u32;
            let vertex_count = p.vertex_count();
            if self.indices.len() + (vertex_count - 2) * 2 > self.max_index_count || self.vertices.len() + vertex_count > self.max_vertex_count {
                unsafe {
                    // update the vertex buffer
                    let vertices_u8: &[u8] = core::slice::from_raw_parts(
                        self.vertices.as_ptr() as *const u8,
                        self.vertices.len() * core::mem::size_of::<PolymeshRenderVertex>(),
                    );
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    // update the index buffer
                    let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 0);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(norm_attr, 3, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 12);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(uv_attr, 2, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 24);
                    debug_assert!(gl.get_error() == 0);

                    gl.draw_elements(glow::TRIANGLES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                    debug_assert!(gl.get_error() == 0);
                }

                self.vertices.clear();
                self.indices.clear();
            }

            for v in p {
                let position = pmesh.get_vertex_position(v.pos());
                let normal = pmesh.get_vertex_normal(v.normal());
                let uv = pmesh.get_vertex_uv(v.tex());
                self.vertices.push(PolymeshRenderVertex { position, normal, uv });
            }

            for i in 2..vertex_count as u32 {
                self.indices.push(vid);
                self.indices.push(vid + i - 1);
                self.indices.push(vid + i);
            }
        }

        if self.indices.len() > 0 {
            unsafe {
                // update the vertex buffer
                let vertices_u8: &[u8] = core::slice::from_raw_parts(
                    self.vertices.as_ptr() as *const u8,
                    self.vertices.len() * core::mem::size_of::<PolymeshRenderVertex>(),
                );
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);

                // update the index buffer
                let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);

                gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 0);
                debug_assert!(gl.get_error() == 0);

                gl.vertex_attrib_pointer_f32(norm_attr, 3, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 12);
                debug_assert!(gl.get_error() == 0);

                gl.vertex_attrib_pointer_f32(uv_attr, 2, glow::FLOAT, false, core::mem::size_of::<PolymeshRenderVertex>() as i32, 24);
                debug_assert!(gl.get_error() == 0);

                gl.draw_elements(glow::TRIANGLES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                debug_assert!(gl.get_error() == 0);
            }
        }

        unsafe {
            gl.disable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);
            gl.disable_vertex_attrib_array(norm_attr);
            debug_assert!(gl.get_error() == 0);
            gl.disable_vertex_attrib_array(uv_attr);
            debug_assert!(gl.get_error() == 0);
            gl.use_program(None);
            debug_assert!(gl.get_error() == 0);
        }
    }
}

pub trait PolymeshTrait {
    type PolyIter: Iterator<Item = Self::VertexIter>;
    type VertexIter: Iterator<Item = Self::Vertex> + PolymeshPolygon;
    type Vertex: PolymeshVertex;

    fn polys(&self) -> Self::PolyIter;
    fn get_vertex_position(&self, index: usize) -> Vec3f;
    fn get_vertex_normal(&self, index: usize) -> Vec3f;
    fn get_vertex_uv(&self, index: usize) -> Vec2f;
}

pub trait PolymeshVertex {
    fn pos(&self) -> usize;
    fn normal(&self) -> usize;
    fn tex(&self) -> usize;
}

pub trait PolymeshPolygon {
    fn vertex_count(&self) -> usize;
}
