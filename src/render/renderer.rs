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
    backend::{
        CustomRenderArgs, CustomRenderHandle, CustomRenderRegistry, CustomRenderRegistryError, FrameError, FrameInfo, RendererBackend, RendererFrame, Vertex,
    },
    display_list::{DisplayList, DrawKind, DrawOp},
    geometry::{textured_quad_vertices, ClipRect, SolidTriangle},
};
use crate::{
    atlas::{AtlasFrameError, AtlasHandle, FontId, IconId, SlotId, WHITE_ICON},
    style::{Color, Image, TextureId},
};
use rs_math3d::{Dimensioni, Recti, Vec2f, Vec2i};
use std::collections::HashMap;
use std::{error::Error, fmt};

/// Failure to validate, acquire, or execute one destructive display-list submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderError {
    /// The backend could not acquire per-frame resources.
    Frame(FrameError),
    /// The renderer could not freeze its atlas for synchronous execution.
    Atlas(AtlasFrameError),
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
            Self::Frame(error) => write!(f, "backend frame acquisition failed: {error}"),
            Self::Atlas(error) => write!(f, "atlas frame freeze failed: {error}"),
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

impl From<AtlasFrameError> for RenderError {
    fn from(error: AtlasFrameError) -> Self {
        Self::Atlas(error)
    }
}

/// High-level UI renderer that executes display lists and owns frame resources.
///
/// Renderer has no mutable drawing clip and exposes no clip stack. Clips belong to operations in a
/// [`DisplayList`], making execution deterministic and independent of prior draw calls.
///
/// A low-level integration can record and execute a display list directly:
///
/// ```
/// use microui_redux::{
///     prelude::{color, Dimensioni, Recti, Vec2i},
///     render::{DisplayList, FrameInfo, Painter, Renderer, RendererBackend},
/// };
///
/// fn render_frame<B: RendererBackend>(
///     renderer: &mut Renderer<B>,
///     display_list: &mut DisplayList,
/// ) -> Result<(), Box<dyn std::error::Error>> {
///     let dimensions = Dimensioni::new(640, 480);
///     let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
///
///     {
///         let mut painter =
///             Painter::new(display_list, Vec2i::default(), viewport, viewport);
///         painter.fill_rect(Recti::new(8, 8, 80, 24), color(70, 110, 180, 255));
///     }
///     let info = FrameInfo::try_new(dimensions, color(18, 20, 24, 255))?;
///     renderer.render(info, display_list)?;
///     Ok(())
/// }
/// ```
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
    /// Dimensions of backend-owned external textures.
    textures: HashMap<TextureId, TextureInfo>,
    /// Backend-specialized persistent custom callbacks.
    custom_renderers: CustomRenderRegistry<B>,
    /// Scratch glyph rectangles reused while expanding text operations.
    rect_batch: Vec<(Recti, Recti, Color)>,
    /// Scratch output reused by final rectangular triangle clipping.
    clipped_triangles: Vec<Vertex>,
    /// Number of display lists executed, used to assert frame-level ownership.
    #[cfg(test)]
    render_count: usize,
    /// Drawable size used by low-level retained-runtime tests.
    #[cfg(test)]
    test_dimensions: Dimensioni,
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
    /// Creates a renderer with unique ownership of the provided backend.
    pub fn new(backend: B) -> Self {
        let atlas = backend.get_atlas();
        let atlas_dim = atlas.get_texture_dimension();
        let white_icon_rect = atlas.get_icon_rect(WHITE_ICON);
        let white_icon_min = Vec2f::new(white_icon_rect.x as f32, white_icon_rect.y as f32);
        let white_icon_extent = Vec2f::new(white_icon_rect.width as f32, white_icon_rect.height as f32);
        let atlas_extent = Vec2f::new(atlas_dim.width.max(1) as f32, atlas_dim.height.max(1) as f32);
        let white_uv = (white_icon_min + white_icon_extent * 0.5) / atlas_extent;
        Self {
            backend,
            atlas,
            atlas_dim,
            white_icon_rect,
            white_uv,
            next_texture_id: 1,
            textures: HashMap::new(),
            custom_renderers: CustomRenderRegistry::new(),
            rect_batch: Vec::new(),
            clipped_triangles: Vec::new(),
            #[cfg(test)]
            render_count: 0,
            #[cfg(test)]
            test_dimensions: Dimensioni::new(1, 1),
        }
    }

    /// Creates a Renderer whose test-only submission helper uses `dimensions`.
    #[cfg(test)]
    pub(crate) fn new_test(backend: B, dimensions: Dimensioni) -> Self {
        let mut renderer = Self::new(backend);
        renderer.test_dimensions = dimensions;
        renderer
    }

    /// Executes one complete low-level frame for retained-runtime tests.
    #[cfg(test)]
    pub(crate) fn render_test(&mut self, list: &mut DisplayList) {
        let info = FrameInfo::try_new(self.test_dimensions, crate::color(0, 0, 0, 0)).expect("test Renderer dimensions must be positive");
        self.render(info, list).expect("test backend frame should render");
    }

    /// Executes one destructive display-list submission and leaves the list empty for reuse.
    pub fn render(&mut self, info: FrameInfo, list: &mut DisplayList) -> Result<(), RenderError> {
        #[cfg(test)]
        {
            self.render_count += 1;
        }
        let mut frame = list.take();
        let result = self.render_recorded(info, &mut frame);
        list.recycle(frame);
        result
    }

    fn render_recorded(&mut self, info: FrameInfo, recorded: &mut super::display_list::RecordedFrame) -> Result<(), RenderError> {
        self.validate_recorded(recorded)?;
        let _atlas_guard = self.atlas.freeze_for_frame()?;
        let viewport = Recti::new(0, 0, info.dimensions().width, info.dimensions().height);
        let backend = &mut self.backend;
        let custom_renderers = &mut self.custom_renderers;
        let mut backend_frame = backend.frame(info)?;
        execute_display_list(
            &mut backend_frame,
            custom_renderers,
            recorded,
            &self.atlas,
            self.atlas_dim,
            self.white_icon_rect,
            self.white_uv,
            viewport,
            &self.textures,
            &mut self.rect_batch,
            &mut self.clipped_triangles,
        );
        drop(backend_frame);
        Ok(())
    }

    fn validate_recorded(&self, recorded: &super::display_list::RecordedFrame) -> Result<(), RenderError> {
        for (operation_index, operation) in recorded.ops.iter().enumerate() {
            match &operation.kind {
                DrawKind::Image { image: Image::Texture(id), .. } if !self.textures.contains_key(id) => {
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

    /// Returns how many display lists this Renderer has executed.
    #[cfg(test)]
    pub(crate) fn debug_render_count(&self) -> usize {
        self.render_count
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
        crate::atlas::validate_rgba_buffer(width, height, pixels.len())?;
        let next_texture_id = self.next_texture_id.checked_add(1).ok_or_else(|| String::from("Texture id space exhausted"))?;
        let id = TextureId::new(self.next_texture_id, width, height);
        self.backend.create_texture(id, width, height, pixels)?;
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
            self.backend.destroy_texture(id);
        }
    }
}

/// Private normal-operation executor used while one backend frame is active.
struct Executor<'a, F: RendererFrame> {
    /// Mutably borrowed active backend frame.
    frame: &'a mut F,
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

impl<F: RendererFrame> Executor<'_, F> {
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
        self.frame.push_quad(textured_quad_vertices(dst, src, self.atlas_dim, color));
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
        self.frame.flush();
        self.frame
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
                self.frame.push_triangle([triangle[0], triangle[1], triangle[2]]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_display_list<'frame, B: RendererBackend>(
    frame: &mut B::Frame<'frame>,
    custom_renderers: &mut CustomRenderRegistry<B>,
    recorded: &mut super::display_list::RecordedFrame,
    atlas: &AtlasHandle,
    atlas_dim: Dimensioni,
    white_icon_rect: Recti,
    white_uv: Vec2f,
    viewport: Recti,
    textures: &HashMap<TextureId, TextureInfo>,
    rect_batch: &mut Vec<(Recti, Recti, Color)>,
    clipped_triangles: &mut Vec<Vertex>,
) {
    let dimensions = Dimensioni::new(viewport.width, viewport.height);
    let solid_triangles = recorded.solid_geometry.triangles();
    for operation in recorded.ops.drain(..) {
        let DrawOp { clip, kind } = operation;
        match kind {
            DrawKind::Custom { renderer, content_area } => {
                let Some(view) = intersect_rects(clip, viewport).and_then(|clip| intersect_rects(clip, content_area)) else {
                    continue;
                };
                let callback = custom_renderers
                    .get_mut(renderer)
                    .expect("custom-render keys were validated before backend-frame acquisition");
                frame.flush();
                callback.render(frame, CustomRenderArgs { dimensions, content_area, view });
            }
            kind => {
                let mut executor = Executor {
                    frame,
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
                executor.execute(DrawOp { clip, kind });
            }
        }
    }
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
        for id in ids {
            self.backend.destroy_texture(id);
        }
        self.textures.clear();
    }
}

#[cfg(test)]
mod tests;
