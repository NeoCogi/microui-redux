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
//! [`DisplayListExecutor`] intersects that clip with the current viewport immediately before
//! submission.

use super::{
    backend::{
        CustomRenderArgs, CustomRenderHandle, CustomRenderKey, CustomRenderRegistry, CustomRenderRegistryError, FrameError, FrameInfo, RendererBackend,
        RendererFrame, Vertex,
    },
    display_list::{DisplayList, DrawKind, DrawOp},
    geometry::{ClipRect, SolidTriangle, textured_quad_from_uv},
};
use crate::{
    atlas::{AtlasHandle, FontId, IconId, WHITE_ICON},
    math::RectExt,
    render::{Color, TextureId},
};
use rs_math3d::{Dimensioni, Recti, Vec2f, Vec2i};
use std::collections::HashSet;
use std::{error::Error, fmt};

/// Failure to validate, acquire, or execute one destructive display-list submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderError {
    /// Retained input/state/layout has not been committed for the requested frame dimensions.
    UiUpdateRequired,
    /// The backend could not acquire per-frame resources.
    Frame(FrameError),
    /// An external texture operation references a texture not owned by this Renderer.
    UnknownTexture {
        /// Unknown texture identifier.
        id: TextureId,
        /// Painter-order operation index containing the reference.
        operation_index: usize,
    },
    /// A custom operation references a removed or foreign registry entry.
    UnknownCustomRenderer {
        /// Painter-order operation index containing the reference.
        operation_index: usize,
    },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UiUpdateRequired => write!(f, "UI update is required before rendering"),
            Self::Frame(error) => write!(f, "backend frame acquisition failed: {error}"),
            Self::UnknownTexture { id, operation_index } => {
                write!(f, "unknown texture {:?} in display-list operation {operation_index}", id)
            }
            Self::UnknownCustomRenderer { operation_index } => {
                write!(f, "unknown custom renderer in display-list operation {operation_index}")
            }
        }
    }
}

impl Error for RenderError {}

impl From<FrameError> for RenderError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

/// High-level UI renderer that owns backend resources and executes crate-recorded UI frames.
///
/// A Renderer uniquely owns its backend and is intended to remain on its owning thread. Backend
/// frames and registered custom-render callbacks execute synchronously during
/// [`ContextFrame::render_ui`](crate::ContextFrame::render_ui).
///
/// Renderer has no mutable drawing clip and exposes no clip stack. Clips belong to internal
/// display-list operations, making execution deterministic and independent of prior draw calls.
/// Applications paint through [`WidgetPaintCtx::painter`](crate::WidgetPaintCtx::painter); display
/// list construction and submission remain crate-owned.
pub struct Renderer<B: RendererBackend> {
    /// Uniquely owned backend.
    backend: B,
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
    /// Complete handles for backend-owned external textures.
    textures: HashSet<TextureId>,
    /// Backend-specialized persistent custom callbacks.
    custom_renderers: CustomRenderRegistry<B>,
    /// Scratch output reused by final rectangular triangle clipping.
    clipped_triangles: Vec<Vertex>,
}

impl<B: RendererBackend> Renderer<B> {
    /// Creates a renderer with unique ownership of the provided backend.
    pub fn new(backend: B) -> Self {
        let atlas = backend.get_atlas();
        let atlas_dim = atlas.get_texture_dimension();
        let white_icon_rect = atlas.get_icon_rect(WHITE_ICON);
        let white_icon_min = Vec2f::new(white_icon_rect.x as f32, white_icon_rect.y as f32);
        let white_icon_extent = Vec2f::new(white_icon_rect.width as f32, white_icon_rect.height as f32);
        // atlas_extent = max(actual_extent, 1) on each axis, keeping UV division non-zero.
        let atlas_extent = Vec2f::new(atlas_dim.width.max(1) as f32, atlas_dim.height.max(1) as f32);
        // white_uv = (white_icon_origin + white_icon_extent / 2) / atlas_extent.
        let white_uv = (white_icon_min + white_icon_extent * 0.5) / atlas_extent;
        Self {
            backend,
            atlas,
            atlas_dim,
            white_icon_rect,
            white_uv,
            next_texture_id: 1,
            textures: HashSet::new(),
            custom_renderers: CustomRenderRegistry::new(),
            clipped_triangles: Vec::new(),
        }
    }

    /// Executes one destructive display-list submission and leaves the list empty for reuse.
    pub(crate) fn render(&mut self, info: FrameInfo, list: &mut DisplayList) -> Result<(), RenderError> {
        let result = self.render_once(info, list);
        list.clear();
        result
    }

    /// Validates, acquires, and executes one display list before the outer wrapper clears it.
    fn render_once(&mut self, info: FrameInfo, list: &mut DisplayList) -> Result<(), RenderError> {
        self.validate_display_list(list)?;
        let viewport = Recti::new(0, 0, info.dimensions().width, info.dimensions().height);
        let frame = self.backend.frame(info)?;
        let DisplayList { ops, solid_geometry } = list;
        let solid_triangles = solid_geometry.triangles();
        let executor = DisplayListExecutor {
            frame,
            custom_renderers: &mut self.custom_renderers,
            atlas: &self.atlas,
            atlas_dim: self.atlas_dim,
            white_icon_rect: self.white_icon_rect,
            white_uv: self.white_uv,
            viewport,
            clipped_triangles: &mut self.clipped_triangles,
            solid_triangles,
        };
        executor.run(ops.drain(..));
        Ok(())
    }

    /// Preflights resource references before a backend frame is acquired.
    fn validate_display_list(&self, list: &DisplayList) -> Result<(), RenderError> {
        for (operation_index, operation) in list.ops.iter().enumerate() {
            match &operation.kind {
                DrawKind::Image { id, .. } if !self.textures.contains(id) => {
                    return Err(RenderError::UnknownTexture { id: *id, operation_index });
                }
                DrawKind::Custom { renderer, .. } if !self.custom_renderers.contains(*renderer) => {
                    return Err(RenderError::UnknownCustomRenderer { operation_index });
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Returns the atlas associated with the renderer.
    pub fn atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    /// Registers one persistent custom renderer specialized for this backend.
    pub(crate) fn register_custom_renderer<F>(&mut self, callback: F) -> Result<CustomRenderHandle<B>, CustomRenderRegistryError>
    where
        F: for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs) + 'static,
    {
        self.custom_renderers.register(callback)
    }

    /// Removes a previously registered custom renderer.
    pub(crate) fn unregister_custom_renderer(&mut self, handle: CustomRenderHandle<B>) -> Result<(), CustomRenderRegistryError> {
        self.custom_renderers.remove(handle)
    }

    /// Attempts to upload raw RGBA pixels as a backend-owned texture.
    ///
    /// Dimensions and byte length are checked before an id is allocated or backend state is
    /// mutated. The texture is tracked by the renderer only after the backend reports success.
    ///
    /// ```
    /// use microui_redux::{
    ///     prelude::TextureId,
    ///     render::{Renderer, RendererBackend},
    /// };
    ///
    /// fn upload_checkerboard<B: RendererBackend>(
    ///     renderer: &mut Renderer<B>,
    /// ) -> Result<TextureId, String> {
    ///     let rgba = [
    ///         255, 255, 255, 255, 0, 0, 0, 255,
    ///         0, 0, 0, 255, 255, 255, 255, 255,
    ///     ];
    ///     renderer.try_load_texture_rgba(2, 2, &rgba)
    /// }
    /// ```
    pub fn try_load_texture_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, String> {
        crate::image::validate_rgba_buffer(width, height, pixels.len())?;
        // next_texture_id = current_texture_id + 1.
        let next_texture_id = self.next_texture_id.checked_add(1).ok_or_else(|| String::from("Texture id space exhausted"))?;
        let id = TextureId::new(self.next_texture_id, width, height);
        self.backend.create_texture(id, width, height, pixels)?;
        self.next_texture_id = next_texture_id;
        self.textures.insert(id);
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
    ///
    /// Destroying an unknown or already-freed handle triggers a debug assertion. In release builds
    /// the repeated operation is an idempotent no-op. The backend is never called more than once
    /// for the same live handle.
    pub fn free_texture(&mut self, id: TextureId) {
        let removed = self.textures.remove(&id);
        debug_assert!(removed, "attempted to destroy an unknown or already-freed texture: {id:?}");
        if removed {
            self.backend.destroy_texture(id);
        }
    }
}

/// Interprets one display list while owning its active backend frame.
struct DisplayListExecutor<'frame, 'resources, B: RendererBackend> {
    /// Active backend frame finalized when this executor is dropped.
    frame: B::Frame<'frame>,
    /// Renderer-owned custom callbacks available at painter-order barriers.
    custom_renderers: &'resources mut CustomRenderRegistry<B>,
    /// Cached atlas used for glyph and icon expansion.
    atlas: &'resources AtlasHandle,
    /// Atlas dimensions used to normalize texture coordinates.
    atlas_dim: Dimensioni,
    /// Atlas source sampled by semantic solid rectangles.
    white_icon_rect: Recti,
    /// Atlas white-pixel coordinate assigned to solid geometry.
    white_uv: Vec2f,
    /// Final viewport clip.
    viewport: Recti,
    /// Reusable triangle clipping output.
    clipped_triangles: &'resources mut Vec<Vertex>,
    /// Complete typed triangle arena referenced by operation ranges.
    solid_triangles: &'resources [SolidTriangle],
}

impl<B: RendererBackend> DisplayListExecutor<'_, '_, B> {
    /// Drains every operation through one frame-scoped interpreter.
    fn run(mut self, operations: impl Iterator<Item = DrawOp>) {
        for operation in operations {
            self.execute(operation);
        }
    }

    /// Executes one operation after resolving its final viewport clip.
    fn execute(&mut self, DrawOp { clip, kind }: DrawOp) {
        let Some(clip) = clip.positive_intersection(self.viewport) else {
            return;
        };
        match kind {
            DrawKind::FillRect { rect, color } => {
                submit_atlas_rect(&mut self.frame, self.atlas_dim, rect, self.white_icon_rect, color, clip);
            }
            DrawKind::Text { font, pos, color, text } => self.draw_text(font, &text, pos, color, clip),
            DrawKind::Icon { id, rect, color } => self.draw_icon(id, rect, color, clip),
            DrawKind::Image { id, rect, color } => self.draw_texture(id, rect, color, clip),
            DrawKind::SolidTriangles { triangles } => {
                let Some(triangles) = self.solid_triangles.get(triangles.as_range()) else {
                    debug_assert!(false, "DisplayList contained an invalid solid-triangle range");
                    return;
                };
                self.draw_solid_triangles(triangles, clip);
            }
            DrawKind::Custom { renderer, content_area } => self.draw_custom(renderer, content_area, clip),
        }
    }

    /// Expands and submits a UTF-8 text run without retaining glyph scratch.
    fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color, clip: Recti) {
        let frame = &mut self.frame;
        let atlas_dim = self.atlas_dim;
        self.atlas.draw_string(font, text, |_, _, dst, src| {
            let dst = Recti::new(pos.x + dst.x, pos.y + dst.y, dst.width, dst.height);
            submit_atlas_rect(frame, atlas_dim, dst, src, color, clip);
        });
    }

    /// Centers an icon inside its semantic destination and submits it.
    fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color, clip: Recti) {
        let src = self.atlas.get_icon_rect(id);
        let dst = Recti::new(
            rect.x + (rect.width - src.width) / 2,
            rect.y + (rect.height - src.height) / 2,
            src.width,
            src.height,
        );
        submit_atlas_rect(&mut self.frame, self.atlas_dim, dst, src, color, clip);
    }

    /// Clips and submits one backend-owned external texture.
    fn draw_texture(&mut self, id: TextureId, dst: Recti, color: Color, clip: Recti) {
        let size = id.size();
        let src = Recti::new(0, 0, size.width, size.height);
        let Some(vertices) = clipped_textured_quad(dst, src, size, color, clip) else {
            return;
        };
        self.frame.flush();
        self.frame.draw_texture(id, vertices);
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
                self.frame.push_triangle([triangle[0], triangle[1], triangle[2]]);
            }
        }
    }

    /// Flushes atlas work and invokes one visible custom callback in painter order.
    fn draw_custom(&mut self, renderer: CustomRenderKey, content_area: Recti, clip: Recti) {
        let Some(view) = clip.positive_intersection(content_area) else {
            return;
        };
        let callback = self
            .custom_renderers
            .get_mut(renderer)
            .expect("custom-render keys were validated before backend-frame acquisition");
        self.frame.flush();
        callback.render(
            &mut self.frame,
            CustomRenderArgs {
                dimensions: Dimensioni::new(self.viewport.width, self.viewport.height),
                content_area,
                view,
            },
        );
    }
}

/// Clips and submits one atlas-backed rectangle.
fn submit_atlas_rect<F: RendererFrame>(frame: &mut F, atlas_dim: Dimensioni, dst: Recti, src: Recti, color: Color, clip: Recti) {
    let Some(vertices) = clipped_textured_quad(dst, src, atlas_dim, color, clip) else {
        return;
    };
    frame.push_quad(vertices);
}

/// Clips a textured destination and preserves projected source coordinates through final UVs.
fn clipped_textured_quad(dst: Recti, src: Recti, texture_dim: Dimensioni, color: Color, clip: Recti) -> Option<[Vertex; 4]> {
    if !dst.has_positive_area() || !src.has_positive_area() || texture_dim.width <= 0 || texture_dim.height <= 0 {
        return None;
    }
    let clipped = dst.positive_intersection(clip)?;

    let dst_extent = Vec2f::new(dst.width as f32, dst.height as f32);
    let dst_x0 = i64::from(dst.x);
    let dst_y0 = i64::from(dst.y);
    let clipped_offset_min = Vec2f::new((i64::from(clipped.x) - dst_x0) as f32, (i64::from(clipped.y) - dst_y0) as f32);
    let clipped_offset_max = Vec2f::new(
        (i64::from(clipped.x) + i64::from(clipped.width) - dst_x0) as f32,
        (i64::from(clipped.y) + i64::from(clipped.height) - dst_y0) as f32,
    );
    // t = clipped_destination_offset / complete_destination_extent.
    let t_min = clipped_offset_min / dst_extent;
    let t_max = clipped_offset_max / dst_extent;

    let src_min = Vec2f::new(src.x as f32, src.y as f32);
    let src_extent = Vec2f::new(src.width as f32, src.height as f32);
    let texture_extent = Vec2f::new(texture_dim.width as f32, texture_dim.height as f32);
    // uv = (source_origin + destination_fraction * source_extent) / texture_extent.
    let uv_min = (src_min + t_min * src_extent) / texture_extent;
    let uv_max = (src_min + t_max * src_extent) / texture_extent;

    Some(textured_quad_from_uv(clipped, uv_min, uv_max, color))
}

impl<B: RendererBackend> Drop for Renderer<B> {
    /// Releases all backend-owned textures allocated through the renderer.
    fn drop(&mut self) {
        for id in self.textures.drain() {
            self.backend.destroy_texture(id);
        }
    }
}

#[cfg(test)]
mod tests;
