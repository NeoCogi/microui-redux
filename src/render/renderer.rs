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
//! The crate-private [`Renderer`] is deliberately not a drawing context. It owns frame resources
//! and executes an already-recorded [`DisplayList`]. Every operation carries its own clip, and the
//! private [`DisplayListExecutor`] intersects that clip with the current viewport immediately
//! before submission.

use super::{
    backend::{
        CustomRenderArgs, CustomRenderHandle, CustomRenderKey, CustomRenderRegistry, CustomRenderRegistryError, FrameError, FrameInfo, RendererBackend,
        RendererFrame, Vertex,
    },
    display_list::{DisplayList, DrawKind, DrawOp},
    geometry::{ClipRect, SolidTriangle, textured_quad_from_uv},
    texture::{RendererId, TextureError},
};
use crate::{
    atlas::{AtlasHandle, FontId, IconId},
    math::{clamp_i64_to_i32, RectExt},
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
    /// An external texture operation references a texture not owned by this Context.
    UnknownTexture {
        /// Unknown texture identifier.
        id: TextureId,
        /// Painter-order operation index containing the reference.
        operation_index: usize,
    },
    /// A text operation references a font not owned by this Context's atlas.
    UnknownFont {
        /// Foreign font capability recorded by the painter.
        id: FontId,
        /// Painter-order operation index containing the reference.
        operation_index: usize,
    },
    /// An icon operation references an icon not owned by this Context's atlas.
    UnknownIcon {
        /// Foreign icon capability recorded by the painter.
        id: IconId,
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
            Self::UnknownFont { id, operation_index } => {
                write!(f, "unknown atlas font {:?} in display-list operation {operation_index}", id)
            }
            Self::UnknownIcon { id, operation_index } => {
                write!(f, "unknown atlas icon {:?} in display-list operation {operation_index}", id)
            }
            Self::UnknownCustomRenderer { operation_index } => {
                write!(f, "unknown custom renderer in display-list operation {operation_index}")
            }
        }
    }
}

impl Error for RenderError {
    /// Exposes the concrete backend-frame cause while keeping validation failures terminal.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Only Frame composes another standard error. The remaining variants directly describe
        // executor-owned validation failures and therefore have no lower-level source to expose.
        match self {
            Self::Frame(source) => Some(source),
            Self::UiUpdateRequired | Self::UnknownTexture { .. } | Self::UnknownFont { .. } | Self::UnknownIcon { .. } | Self::UnknownCustomRenderer { .. } => {
                None
            }
        }
    }
}

impl From<FrameError> for RenderError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

/// Crate-private UI executor that owns backend resources and submits recorded frames.
///
/// A Renderer uniquely owns its backend and is intended to remain on its owning thread. Backend
/// frames and registered custom-render callbacks execute synchronously during
/// [`ContextFrame::render_ui`](crate::ContextFrame::render_ui).
///
/// Renderer has no mutable drawing clip and exposes no clip stack. Clips belong to internal
/// display-list operations, making execution deterministic and independent of prior draw calls.
/// Applications paint through [`WidgetPaintCtx::painter`](crate::WidgetPaintCtx::painter); display
/// list construction and submission remain crate-owned.
pub(crate) struct Renderer<B: RendererBackend> {
    /// Process-unique identity copied into every resource capability owned by this renderer.
    id: RendererId,
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
    /// Last renderer-local external texture slot successfully allocated.
    last_texture_slot: u32,
    /// Complete handles for backend-owned external textures.
    textures: HashSet<TextureId>,
    /// Backend-specialized persistent custom callbacks.
    custom_renderers: CustomRenderRegistry<B>,
    /// Scratch output reused by final rectangular triangle clipping.
    clipped_triangles: Vec<Vertex>,
}

impl<B: RendererBackend> Renderer<B> {
    /// Creates a renderer with unique ownership of the provided backend.
    pub(crate) fn new(backend: B) -> Self {
        // Allocate provenance once at the concrete Renderer ownership boundary. Textures and custom
        // callbacks copy this same identity rather than maintaining independent global namespaces.
        let id = RendererId::allocate();
        let atlas = backend.get_atlas();
        let atlas_dim = atlas.get_texture_dimension();
        // Resolve the solid-rendering source by its required semantic name; no positional icon ID
        // exists independently of this atlas allocation.
        let white_icon_rect = atlas.get_icon_rect(atlas.white_icon());
        let white_icon_min = Vec2f::new(white_icon_rect.x as f32, white_icon_rect.y as f32);
        let white_icon_extent = Vec2f::new(white_icon_rect.width as f32, white_icon_rect.height as f32);
        // atlas_extent = max(actual_extent, 1) on each axis, keeping UV division non-zero.
        let atlas_extent = Vec2f::new(atlas_dim.width.max(1) as f32, atlas_dim.height.max(1) as f32);
        // white_uv = (white_icon_origin + white_icon_extent / 2) / atlas_extent.
        let white_uv = (white_icon_min + white_icon_extent * 0.5) / atlas_extent;
        Self {
            id,
            backend,
            atlas,
            atlas_dim,
            white_icon_rect,
            white_uv,
            last_texture_slot: 0,
            textures: HashSet::new(),
            custom_renderers: CustomRenderRegistry::new(id),
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
                DrawKind::Text { font, .. } if !self.atlas.contains_font(*font) => {
                    return Err(RenderError::UnknownFont { id: *font, operation_index });
                }
                DrawKind::Icon { id, .. } if !self.atlas.contains_icon(*id) => {
                    return Err(RenderError::UnknownIcon { id: *id, operation_index });
                }
                DrawKind::NinePatch { patch, .. } => {
                    // Theme images are ordinary atlas capabilities. Validate their provenance just
                    // like standalone icons before a backend frame is acquired.
                    if let Some(image) = patch.image_content()
                        && !self.atlas.contains_icon(image.icon)
                    {
                        return Err(RenderError::UnknownIcon { id: image.icon, operation_index });
                    }
                }
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
    pub(crate) fn atlas(&self) -> AtlasHandle {
        // Return the cheap immutable capability rather than exposing the executor's backend owner.
        self.atlas.clone()
    }

    /// Replaces the backend atlas and every renderer-side metric derived from it.
    ///
    /// The backend commits first so an upload failure cannot expose CPU metadata for pixels that
    /// were never published. Once that transaction succeeds, resolving the required white icon and
    /// its normalized UV is infallible under the validated [`AtlasHandle`] contract.
    #[cfg(feature = "theme-json")]
    pub(crate) fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), super::backend::AtlasUploadError> {
        // Do not re-upload a bundle that is already active. The pointer comparison is exact atlas
        // identity rather than structural equality, matching FontId and IconId provenance.
        if self.atlas.ptr_eq(&atlas) {
            return Ok(());
        }
        self.backend.replace_atlas(atlas.clone())?;
        let atlas_dim = atlas.get_texture_dimension();
        let white_icon_rect = atlas.get_icon_rect(atlas.white_icon());
        let white_icon_min = Vec2f::new(white_icon_rect.x as f32, white_icon_rect.y as f32);
        let white_icon_extent = Vec2f::new(white_icon_rect.width as f32, white_icon_rect.height as f32);
        let atlas_extent = Vec2f::new(atlas_dim.width.max(1) as f32, atlas_dim.height.max(1) as f32);

        // Publish all CPU-side atlas state together only after the backend owns the matching
        // texture. Later layout, validation, and rendering therefore observe one coherent atlas.
        self.atlas = atlas;
        self.atlas_dim = atlas_dim;
        self.white_icon_rect = white_icon_rect;
        self.white_uv = (white_icon_min + white_icon_extent * 0.5) / atlas_extent;
        Ok(())
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

    /// Uploads one validated RGBA buffer through the context's sole public image API.
    pub(crate) fn try_load_texture_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, TextureError> {
        crate::image::validate_rgba_buffer(width, height, pixels.len())?;
        // Compute the next slot without committing it, so exhaustion and failed backend uploads
        // leave allocator state unchanged. The zero initial value keeps every usable u32 slot.
        let slot = self.last_texture_slot.checked_add(1).ok_or(TextureError::IdentifierSpaceExhausted)?;
        let id = TextureId::new(self.id, slot, width, height);
        // TextureId is the sole dimension source at the backend boundary; pixels were validated
        // against those same immutable values immediately above.
        self.backend.create_texture(id, pixels)?;
        // Publish the slot only after the backend owns the corresponding texture.
        self.last_texture_slot = slot;
        self.textures.insert(id);
        Ok(id)
    }

    /// Destroys one texture after the context has relinquished its public capability.
    ///
    /// Destroying an unknown or already-freed handle triggers a debug assertion. In release builds
    /// the repeated operation is an idempotent no-op. The backend is never called more than once
    /// for the same live handle.
    pub(crate) fn free_texture(&mut self, id: TextureId) {
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
            DrawKind::NinePatch { rect, patch } => self.draw_nine_patch(rect, patch, clip),
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

    /// Expands a UTF-8 string through the selected atlas font, applying its missing-character
    /// fallback, and submits the resulting glyph rectangles.
    fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color, clip: Recti) {
        let frame = &mut self.frame;
        let atlas_dim = self.atlas_dim;
        self.atlas.draw_string(font, text, pos, |_, _, dst, src| {
            // Atlas text walking combines unbounded metric accumulation with this application
            // origin before one coordinate clamp, retaining cancellation between the two terms.
            submit_atlas_rect(frame, atlas_dim, dst, src, color, clip);
        });
    }

    /// Expands one semantic patch and submits each visible flat or image cell.
    fn draw_nine_patch(&mut self, rect: Recti, patch: crate::render::NinePatch, clip: Recti) {
        // NinePatch geometry and named cell ordering are resolved together here, making the renderer
        // the only production boundary that turns one compact three-by-three operation into quads.
        let destinations = patch.geometry(rect);
        match patch.content {
            crate::render::NinePatchContent::Flat { cells } => {
                let cells = cells.rows();
                for row in 0..3 {
                    for column in 0..3 {
                        if row == 1 && column == 1 && !patch.center_visible {
                            continue;
                        }
                        let crate::render::NinePatchCell::Color { color } = cells[row][column] else {
                            continue;
                        };
                        submit_atlas_rect(&mut self.frame, self.atlas_dim, destinations[row][column], self.white_icon_rect, color, clip);
                    }
                }
            }
            crate::render::NinePatchContent::Image { image } => {
                // Theme artwork shares the font/icon atlas and therefore joins the ordinary UI
                // vertex batch. Expanding nine cells no longer inserts texture switches, heap
                // allocated backend jobs, or independent draw calls between adjacent controls.
                let source = self.atlas.get_icon_rect(image.icon);
                let sources = crate::render::nine_patch::geometry_with_insets(source, image.source_insets);
                for row in 0..3 {
                    for column in 0..3 {
                        if row == 1 && column == 1 && !patch.center_visible {
                            continue;
                        }
                        submit_atlas_rect(
                            &mut self.frame,
                            self.atlas_dim,
                            destinations[row][column],
                            sources[row][column],
                            image.tint,
                            clip,
                        );
                    }
                }
            }
        }
    }

    /// Centers an icon inside its semantic destination and submits it.
    fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color, clip: Recti) {
        let src = self.atlas.get_icon_rect(id);
        // Destination geometry is application-controlled and may use the complete i32 domain.
        // Evaluate centering in i64 so subtracting the icon extent and adding an extreme origin
        // cannot overflow before the renderer-facing coordinate is clamped.
        let x = i64::from(rect.x) + (i64::from(rect.width) - i64::from(src.width)) / 2;
        let y = i64::from(rect.y) + (i64::from(rect.height) - i64::from(src.height)) / 2;
        let dst = Recti::new(clamp_i64_to_i32(x), clamp_i64_to_i32(y), src.width, src.height);
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
            // Triangle clipping appends complete triples; fixed-size partitioning makes that
            // renderer invariant explicit at the backend call boundary.
            for triangle in self.clipped_triangles.as_chunks::<3>().0 {
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
        callback(
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
