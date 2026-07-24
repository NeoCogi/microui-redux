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
//! Single-pass display-list execution and high-level frame resources.
//!
//! [`Renderer`] is deliberately not a drawing context. It owns frame resources and executes an
//! already-recorded [`DisplayList`]. Every operation carries its own clip, and the private
//! [`Executor`] intersects that clip with the current viewport immediately before submission.

use super::{
    backend::{BackendHandle, CustomRenderArgs, CustomRenderCommand, RendererBackend, Vertex},
    display_list::{DisplayList, DrawKind, DrawOp},
    geometry::{textured_quad_vertices, ClipRect, SolidTriangle},
};
use crate::{
    atlas::{AtlasHandle, FontId, IconId, SlotId, WHITE_ICON},
    style::{Color, Image, TextureId},
};
use rs_math3d::{Dimensioni, Recti, Vec2f, Vec2i};
use std::collections::HashMap;

/// High-level UI renderer that executes display lists and owns frame resources.
///
/// Renderer has no mutable drawing clip and exposes no clip stack. Clips belong to operations in a
/// [`DisplayList`], making execution deterministic and independent of prior draw calls.
pub struct Renderer<B: RendererBackend> {
    /// Current viewport dimensions in pixels.
    current_dim: Dimensioni,
    /// Shared backend handle used for frame and operation execution.
    backend: BackendHandle<B>,
    /// Atlas cached once from the backend.
    atlas: AtlasHandle,
    /// Atlas texture dimensions used for UV normalization.
    atlas_dim: Dimensioni,
    /// Rectangle of the baked white icon used for solid fills.
    white_icon_rect: Recti,
    /// UV at the center of the baked white icon used for solid triangles.
    white_uv: Vec2f,
    /// Next external texture id allocated by this renderer.
    next_texture_id: u32,
    /// Dimensions of backend-owned external textures.
    textures: HashMap<TextureId, TextureInfo>,
    /// Scratch glyph rectangles reused while expanding text operations.
    rect_batch: Vec<(Recti, Recti, Color)>,
    /// Scratch output reused by final rectangular triangle clipping.
    clipped_triangles: Vec<Vertex>,
    /// Number of display lists executed, used to assert frame-level ownership.
    #[cfg(test)]
    render_count: usize,
}

#[derive(Clone, Copy)]
/// Dimensions tracked for an uploaded external texture.
struct TextureInfo {
    /// Texture width in pixels.
    width: i32,
    /// Texture height in pixels.
    height: i32,
}

impl<B: RendererBackend> Renderer<B> {
    /// Creates a renderer around the provided backend handle.
    pub fn new(backend: BackendHandle<B>, dim: Dimensioni) -> Self {
        let atlas = backend.scope(RendererBackend::get_atlas);
        let atlas_dim = atlas.get_texture_dimension();
        let white_icon_rect = atlas.get_icon_rect(WHITE_ICON);
        let white_icon_min = Vec2f::new(white_icon_rect.x as f32, white_icon_rect.y as f32);
        let white_icon_extent = Vec2f::new(white_icon_rect.width as f32, white_icon_rect.height as f32);
        let atlas_extent = Vec2f::new(atlas_dim.width.max(1) as f32, atlas_dim.height.max(1) as f32);
        let white_uv = (white_icon_min + white_icon_extent * 0.5) / atlas_extent;
        Self {
            current_dim: dim,
            backend,
            atlas,
            atlas_dim,
            white_icon_rect,
            white_uv,
            next_texture_id: 1,
            textures: HashMap::new(),
            rect_batch: Vec::new(),
            clipped_triangles: Vec::new(),
            #[cfg(test)]
            render_count: 0,
        }
    }

    /// Executes a display list once in painter order and leaves it empty for reuse.
    ///
    /// Consecutive normal operations share one backend lock. A custom operation is a barrier:
    /// Renderer releases the lock, flushes before and after the callback, then starts a new normal
    /// segment. The operation iterator is never restarted or searched for later barriers.
    pub fn render(&mut self, list: &mut DisplayList) {
        #[cfg(test)]
        {
            self.render_count += 1;
        }
        let mut frame = list.take();
        let viewport = self.viewport();
        let current_dim = self.current_dim;
        let backend = &mut self.backend;
        let atlas = &self.atlas;
        let atlas_dim = self.atlas_dim;
        let white_icon_rect = self.white_icon_rect;
        let white_uv = self.white_uv;
        let textures = &self.textures;
        let rect_batch = &mut self.rect_batch;
        let clipped_triangles = &mut self.clipped_triangles;

        {
            let solid_triangles = frame.solid_geometry.triangles();
            let mut operations = frame.ops.drain(..).peekable();

            while let Some(operation) = operations.peek() {
                if matches!(&operation.kind, DrawKind::Custom { .. }) {
                    let operation = operations.next().expect("peeked custom operation must exist");
                    let DrawOp {
                        clip,
                        kind: DrawKind::Custom { args, command },
                    } = operation
                    else {
                        unreachable!("custom barrier changed after inspection");
                    };
                    execute_custom(backend, current_dim, viewport, clip, args, command);
                    continue;
                }

                backend.scope_mut(|backend| {
                    let mut executor = Executor {
                        backend,
                        atlas,
                        atlas_dim,
                        white_icon_rect,
                        white_uv,
                        viewport,
                        textures,
                        rect_batch,
                        clipped_triangles,
                        solid_triangles,
                    };

                    while operations.peek().is_some_and(|operation| !matches!(&operation.kind, DrawKind::Custom { .. })) {
                        executor.execute(operations.next().expect("peeked normal operation must exist"));
                    }
                });
            }
        }

        list.recycle(frame);
    }

    /// Returns the atlas associated with the renderer.
    pub fn atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    /// Begins a new drawing pass and updates the viewport used by final clipping.
    pub fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.current_dim = Dimensioni::new(width, height);
        self.backend.scope_mut(move |backend| backend.begin(width, height, clr));
    }

    /// Ends the current drawing pass.
    pub fn end(&mut self) {
        self.backend.scope_mut(RendererBackend::end);
    }

    /// Flushes any buffered geometry without ending the frame.
    pub fn flush(&mut self) {
        self.backend.scope_mut(RendererBackend::flush);
    }

    /// Returns the last viewport dimensions passed to [`Renderer::begin`].
    pub fn dimensions(&self) -> Dimensioni {
        self.current_dim
    }

    /// Returns how many display lists this Renderer has executed.
    #[cfg(test)]
    pub(crate) fn debug_render_count(&self) -> usize {
        self.render_count
    }

    /// Returns a clone of the underlying backend handle.
    pub fn backend_handle(&self) -> BackendHandle<B> {
        self.backend.clone()
    }

    /// Attempts to upload raw RGBA pixels as a backend-owned texture.
    ///
    /// Dimensions and byte length are checked before an id is allocated or backend state is
    /// mutated. The texture is tracked by the renderer only after the backend reports success.
    pub fn try_load_texture_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, String> {
        crate::atlas::validate_rgba_buffer(width, height, pixels.len())?;
        let next_texture_id = self.next_texture_id.checked_add(1).ok_or_else(|| String::from("Texture id space exhausted"))?;
        let id = TextureId::new(self.next_texture_id, width, height);
        self.backend.scope_mut(|backend| backend.create_texture(id, width, height, pixels))?;
        self.next_texture_id = next_texture_id;
        self.textures.insert(id, TextureInfo { width, height });
        Ok(id)
    }

    /// Uploads raw RGBA pixels as a backend-owned texture.
    ///
    /// Panics if the RGBA dimensions/byte length are invalid or the backend rejects the upload.
    /// Prefer [`Renderer::try_load_texture_rgba`] when callers can handle upload failure.
    #[track_caller]
    pub fn load_texture_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId {
        self.try_load_texture_rgba(width, height, pixels).expect("failed to upload RGBA texture")
    }

    /// Destroys a texture allocated via [`Renderer::load_texture_rgba`].
    pub fn free_texture(&mut self, id: TextureId) {
        if self.textures.remove(&id).is_some() {
            self.backend.scope_mut(|backend| backend.destroy_texture(id));
        }
    }

    /// Returns the positive current viewport rectangle.
    fn viewport(&self) -> Recti {
        Recti::new(0, 0, self.current_dim.width.max(0), self.current_dim.height.max(0))
    }
}

/// Private normal-operation executor used only while one backend segment is locked.
struct Executor<'a, B: RendererBackend> {
    /// Mutably borrowed backend for the current normal segment.
    backend: &'a mut B,
    /// Cached atlas used for glyph, icon, and slot expansion.
    atlas: &'a AtlasHandle,
    /// Atlas dimensions used to normalize texture coordinates.
    atlas_dim: Dimensioni,
    /// Atlas source sampled by semantic solid rectangles.
    white_icon_rect: Recti,
    /// Atlas white-pixel coordinate assigned to solid geometry.
    white_uv: Vec2f,
    /// Final viewport clip.
    viewport: Recti,
    /// Backend-owned texture dimensions.
    textures: &'a HashMap<TextureId, TextureInfo>,
    /// Reusable text expansion scratch.
    rect_batch: &'a mut Vec<(Recti, Recti, Color)>,
    /// Reusable triangle clipping output.
    clipped_triangles: &'a mut Vec<Vertex>,
    /// Complete typed triangle arena referenced by operation ranges.
    solid_triangles: &'a [SolidTriangle],
}

impl<B: RendererBackend> Executor<'_, B> {
    /// Executes one non-custom operation after resolving its final clip.
    fn execute(&mut self, operation: DrawOp) {
        let Some(clip) = intersect_rects(operation.clip, self.viewport) else {
            return;
        };
        match operation.kind {
            DrawKind::FillRect { rect, color } => self.push_atlas_rect(rect, self.white_icon_rect, color, clip),
            DrawKind::Text { font, pos, color, text } => self.draw_text(font, &text, pos, color, clip),
            DrawKind::Icon { id, rect, color } => self.draw_icon(id, rect, color, clip),
            DrawKind::Image { image, rect, color } => self.draw_image(image, rect, color, clip),
            DrawKind::SolidTriangles { triangles } => {
                let Some(triangles) = self.solid_triangles.get(triangles.as_range()) else {
                    debug_assert!(false, "DisplayList contained an invalid solid-triangle range");
                    return;
                };
                self.draw_solid_triangles(triangles, clip);
            }
            DrawKind::RedrawSlot { id, rect, color, payload } => {
                let mut atlas = self.atlas.clone();
                atlas.render_slot(id, payload);
                self.draw_slot(id, rect, color, clip);
            }
            DrawKind::Custom { .. } => unreachable!("custom operations are execution barriers"),
        }
    }

    /// Expands a UTF-8 text run and submits each visible glyph quad.
    fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color, clip: Recti) {
        self.rect_batch.clear();
        let rect_batch = &mut self.rect_batch;
        self.atlas.draw_string(font, text, |_, _, dst, src| {
            rect_batch.push((Recti::new(pos.x + dst.x, pos.y + dst.y, dst.width, dst.height), src, color));
        });
        for index in 0..self.rect_batch.len() {
            let (dst, src, color) = self.rect_batch[index];
            self.push_atlas_rect(dst, src, color, clip);
        }
    }

    /// Centers an icon inside its semantic destination and submits it.
    fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color, clip: Recti) {
        let src = self.atlas.get_icon_rect(id);
        self.push_centered_atlas_rect(rect, src, color, clip);
    }

    /// Centers an atlas slot inside its semantic destination and submits it.
    fn draw_slot(&mut self, id: SlotId, rect: Recti, color: Color, clip: Recti) {
        let src = self.atlas.get_slot_rect(id);
        self.push_centered_atlas_rect(rect, src, color, clip);
    }

    /// Dispatches an image to either its atlas slot or external texture.
    fn draw_image(&mut self, image: Image, rect: Recti, color: Color, clip: Recti) {
        match image {
            Image::Slot(id) => self.draw_slot(id, rect, color, clip),
            Image::Texture(id) => self.draw_texture(id, rect, color, clip),
        }
    }

    /// Centers and submits one atlas source rectangle.
    fn push_centered_atlas_rect(&mut self, rect: Recti, src: Recti, color: Color, clip: Recti) {
        let dst = Recti::new(
            rect.x + (rect.width - src.width) / 2,
            rect.y + (rect.height - src.height) / 2,
            src.width,
            src.height,
        );
        self.push_atlas_rect(dst, src, color, clip);
    }

    /// Clips and submits one atlas-backed rectangle.
    fn push_atlas_rect(&mut self, dst: Recti, src: Recti, color: Color, clip: Recti) {
        let Some((dst, src)) = clip_textured_rect(dst, src, clip) else {
            return;
        };
        let [v0, v1, v2, v3] = textured_quad_vertices(dst, src, self.atlas_dim, color);
        self.backend.push_quad_vertices(&v0, &v1, &v2, &v3);
    }

    /// Clips and submits one backend-owned external texture.
    fn draw_texture(&mut self, id: TextureId, dst: Recti, color: Color, clip: Recti) {
        let Some(info) = self.textures.get(&id).copied() else {
            return;
        };
        let src = Recti::new(0, 0, info.width, info.height);
        let Some((dst, src)) = clip_textured_rect(dst, src, clip) else {
            return;
        };
        self.backend
            .draw_texture(id, textured_quad_vertices(dst, src, Dimensioni::new(info.width, info.height), color));
    }

    /// Converts and clips typed solid triangles immediately before backend submission.
    fn draw_solid_triangles(&mut self, triangles: &[SolidTriangle], clip: Recti) {
        let Some(clip) = ClipRect::new(clip) else {
            return;
        };
        for triangle in triangles {
            let vertices = (*triangle.vertices()).map(|vertex| Vertex::new(vertex.position, self.white_uv, vertex.color));
            self.clipped_triangles.clear();
            clip.clip_triangle(vertices, self.clipped_triangles);
            for triangle in self.clipped_triangles.chunks_exact(3) {
                self.backend.push_triangle_vertices(&triangle[0], &triangle[1], &triangle[2]);
            }
        }
    }
}

/// Executes one custom barrier without holding the backend lock during the callback.
fn execute_custom<B: RendererBackend>(
    backend: &mut BackendHandle<B>,
    dimensions: Dimensioni,
    viewport: Recti,
    operation_clip: Recti,
    mut args: CustomRenderArgs,
    mut command: Box<dyn CustomRenderCommand>,
) {
    args.view = intersect_rects(operation_clip, viewport)
        .and_then(|clip| intersect_rects(clip, args.view))
        .unwrap_or_else(|| Recti::new(args.content_area.x, args.content_area.y, 0, 0));
    backend.scope_mut(RendererBackend::flush);
    command.render(dimensions, &args);
    backend.scope_mut(RendererBackend::flush);
}

/// Projects clipping of a destination rectangle back into its texture source rectangle.
fn clip_textured_rect(dst: Recti, src: Recti, clip: Recti) -> Option<(Recti, Recti)> {
    if dst.width <= 0 || dst.height <= 0 || src.width <= 0 || src.height <= 0 {
        return None;
    }
    let clipped = intersect_rects(dst, clip)?;
    if same_rect(clipped, dst) {
        return Some((dst, src));
    }

    let dst_extent = Vec2f::new(dst.width as f32, dst.height as f32);
    let clipped_offset_min = Vec2f::new((clipped.x - dst.x) as f32, (clipped.y - dst.y) as f32);
    let clipped_offset_max = Vec2f::new((clipped.x + clipped.width - dst.x) as f32, (clipped.y + clipped.height - dst.y) as f32);
    let t_min = clipped_offset_min / dst_extent;
    let t_max = clipped_offset_max / dst_extent;

    let src_min = Vec2f::new(src.x as f32, src.y as f32);
    let src_extent = Vec2f::new(src.width as f32, src.height as f32);
    let projected_min = src_min + t_min * src_extent;
    let projected_max = src_min + t_max * src_extent;

    Some((
        clipped,
        Recti::new(
            projected_min.x as i32,
            projected_min.y as i32,
            (projected_max.x - projected_min.x) as i32,
            (projected_max.y - projected_min.y) as i32,
        ),
    ))
}

/// Returns the positive-area intersection of two integer rectangles.
fn intersect_rects(left: Recti, right: Recti) -> Option<Recti> {
    let intersection = left.intersect(&right)?;
    (intersection.width > 0 && intersection.height > 0).then_some(intersection)
}

/// Compares rectangle components without requiring an equality implementation.
fn same_rect(left: Recti, right: Recti) -> bool {
    (left.x, left.y, left.width, left.height) == (right.x, right.y, right.width, right.height)
}

impl<B: RendererBackend> Drop for Renderer<B> {
    /// Releases all backend-owned textures allocated through the renderer.
    fn drop(&mut self) {
        let ids: Vec<_> = self.textures.keys().copied().collect();
        self.backend.scope_mut(|backend| {
            for id in ids {
                backend.destroy_texture(id);
            }
        });
        self.textures.clear();
    }
}

#[cfg(test)]
mod tests;
