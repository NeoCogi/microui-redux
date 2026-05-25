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
//! Renderer-facing canvas and texture batching.
//!
//! `Canvas` turns retained draw commands into renderer calls, handles atlas/external texture
//! quads, applies clipping, and owns the external texture id lifetime for a renderer handle.
use crate::graphics::clip_triangle_vertices_to_rect;
use super::*;
use std::collections::HashMap;

mod quad;
use quad::textured_quad_vertices;

#[derive(Default, Copy, Clone)]
#[repr(C)]
/// Vertex submitted by the UI.
pub struct Vertex {
    /// Screen-space position in pixels.
    pos: Vec2f,
    /// Normalized texture coordinate.
    tex: Vec2f,
    /// Vertex color multiplied with the sampled texture.
    color: Color4b,
}

impl Vertex {
    /// Creates a vertex with the provided position, texture coordinate, and color.
    pub fn new(pos: Vec2f, tex: Vec2f, color: Color4b) -> Self {
        Self { pos, tex, color }
    }

    /// Returns the position of the vertex in screen space.
    pub fn position(&self) -> Vec2f {
        self.pos
    }

    /// Returns the texture coordinates associated with the vertex.
    pub fn tex_coord(&self) -> Vec2f {
        self.tex
    }

    /// Returns the vertex color.
    pub fn color(&self) -> Color4b {
        self.color
    }
}

/// High-level drawing helper that batches draw commands for a renderer.
pub struct Canvas<R: Renderer> {
    /// Current viewport dimensions in pixels.
    current_dim: Dimensioni,
    /// Shared renderer handle used for frame-scoped backend access.
    renderer: RendererHandle<R>,
    /// Atlas cached from the renderer.
    atlas: AtlasHandle,
    /// Atlas texture dimensions used for UV normalization.
    atlas_dim: Dimensioni,
    /// Rectangle of the baked white icon used for solid-color fills.
    white_icon_rect: Recti,
    /// Active screen-space clip rectangle.
    clip: Recti,
    /// Next external texture id allocated by this canvas.
    next_texture_id: u32,
    /// Dimensions of renderer-owned external textures.
    textures: HashMap<TextureId, TextureInfo>,
    /// Scratch buffer used to batch glyph rectangles without reallocating.
    rect_batch: Vec<(Recti, Recti, Color)>,
}

#[derive(Clone, Copy)]
/// Dimensions tracked for an uploaded external texture.
struct TextureInfo {
    /// Texture width in pixels.
    width: i32,
    /// Texture height in pixels.
    height: i32,
}

impl<R: Renderer> Canvas<R> {
    /// Creates a canvas around the provided renderer handle.
    pub fn from(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        let atlas = renderer.scope(|r| r.get_atlas());
        let atlas_dim = atlas.get_texture_dimension();
        let white_icon_rect = atlas.get_icon_rect(WHITE_ICON);
        Self {
            current_dim: dim,
            renderer,
            atlas,
            atlas_dim,
            white_icon_rect,
            clip: Recti::new(0, 0, dim.width, dim.height),
            next_texture_id: 1,
            textures: HashMap::new(),
            rect_batch: Vec::new(),
        }
    }

    /// Returns the atlas associated with the renderer.
    pub fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    #[inline(never)]
    /// Computes the clipped destination/source rectangles for rendering.
    pub fn clip_rect(dst_r: Recti, src_r: Recti, clip_r: Recti) -> Option<(Recti, Recti)> {
        match dst_r.intersect(&clip_r) {
            Some(rect) if rect.width == dst_r.width && rect.height == dst_r.height => Some((dst_r, src_r)),
            Some(rect) if rect.width != 0 && rect.height != 0 => {
                // Preserve texture mapping by projecting the clipped destination rectangle back
                // into the original source rectangle.
                let dx = dst_r.x as f32;
                let dy = dst_r.y as f32;
                let dw = dst_r.width as f32;
                let dh = dst_r.height as f32;

                let rx = rect.x as f32;
                let ry = rect.y as f32;
                let rw = rect.width as f32;
                let rh = rect.height as f32;

                let tx = (rx - dx) / dw;
                let ty = (ry - dy) / dh;
                let tw = (rx + rw - dx) / dw;
                let th = (ry + rh - dy) / dh;

                let sx = src_r.x as f32;
                let sy = src_r.y as f32;
                let sw = src_r.width as f32;
                let sh = src_r.height as f32;

                let st_x = sx + tx * sw;
                let st_y = sy + ty * sh;
                let st_w = sx + tw * sw - st_x;
                let st_h = sy + th * sh - st_y;

                Some((rect, Recti::new(st_x as _, st_y as _, st_w as _, st_h as _)))
            }
            _ => None,
        }
    }

    #[inline(never)]
    /// Pushes a textured quad referencing the atlas to the renderer.
    pub fn push_rect(&mut self, dst: Recti, src: Recti, color: Color) {
        let rects = [(dst, src, color)];
        self.push_rects(&rects);
    }

    #[inline(never)]
    /// Pushes multiple textured quads referencing the atlas in one renderer lock scope.
    pub fn push_rects(&mut self, rects: &[(Recti, Recti, Color)]) {
        if rects.is_empty() {
            return;
        }
        self.render_scope(|frame| frame.push_rects(rects));
    }

    /// Draws a solid colored rectangle.
    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        self.render_scope(|frame| frame.draw_rect(rect, color));
    }

    #[inline(never)]
    /// Draws UTF-8 text using the supplied font.
    pub fn draw_chars(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        if text.is_empty() {
            return;
        }
        self.render_scope(|frame| frame.draw_chars(font, text, pos, color));
    }

    /// Draws an icon centered inside the provided rectangle.
    pub fn draw_icon(&mut self, id: IconId, r: Recti, color: Color) {
        self.render_scope(|frame| frame.draw_icon(id, r, color));
    }

    /// Draws an atlas slot centered inside the provided rectangle.
    pub fn draw_slot(&mut self, id: SlotId, r: Recti, color: Color) {
        self.render_scope(|frame| frame.draw_slot(id, r, color));
    }

    /// Renders a slot with the callback before drawing it.
    pub fn draw_slot_with_function(&mut self, id: SlotId, r: Recti, color: Color, payload: Rc<dyn Fn(usize, usize) -> Color4b>) {
        self.render_scope(|frame| frame.draw_slot_with_function(id, r, color, payload));
    }

    /// Runs a short mutable renderer scope with a prepared frame helper.
    pub(crate) fn render_scope<Res, F: FnOnce(&mut CanvasFrame<'_, R>) -> Res>(&mut self, f: F) -> Res {
        let Self {
            current_dim,
            renderer,
            atlas,
            atlas_dim,
            white_icon_rect,
            clip,
            textures,
            rect_batch,
            ..
        } = self;
        // Copy immutable fields out before borrowing the renderer so the frame helper can borrow
        // the mutable texture/clip buffers without fighting the renderer handle borrow.
        let current_dim = *current_dim;
        let atlas_dim = *atlas_dim;
        let white_icon_rect = *white_icon_rect;
        renderer.scope_mut(|renderer| {
            let mut frame = CanvasFrame {
                renderer,
                current_dim,
                atlas,
                atlas_dim,
                white_icon_rect,
                clip,
                textures,
                rect_batch,
            };
            f(&mut frame)
        })
    }

    /// Sets the clip rectangle used for subsequent draw calls.
    pub fn set_clip_rect(&mut self, rect: Recti) {
        self.clip = rect;
    }

    /// Returns the clip rectangle currently applied to draw commands.
    pub fn current_clip_rect(&self) -> Recti {
        self.clip
    }

    /// Draws a triangle list using the canvas' current clip state.
    ///
    /// Triangles are clipped in software and then appended to the renderer's regular UI batch,
    /// which keeps retained widget graphics on the same batching path as the rest of the UI.
    pub fn draw_triangles(&mut self, vertices: &[Vertex]) {
        if vertices.is_empty() {
            return;
        }
        self.render_scope(|frame| frame.draw_triangles(vertices));
    }

    /// Begins a new drawing pass and resets the clip rectangle.
    pub fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.current_dim = Dimensioni::new(width, height);
        self.set_clip_rect(Rect::new(0, 0, width, height));
        self.renderer.scope_mut(move |r| r.begin(width, height, clr));
    }

    /// Ends the current drawing pass.
    pub fn end(&mut self) {
        self.renderer.scope_mut(|r| r.end())
    }

    /// Flushes any buffered geometry without ending the frame.
    pub fn flush(&mut self) {
        self.renderer.scope_mut(|r| r.flush())
    }

    /// Returns the last viewport dimensions passed to [`Canvas::begin`].
    pub fn current_dimension(&self) -> Dimensioni {
        self.current_dim
    }

    /// Returns a clone of the underlying renderer handle.
    pub fn renderer_handle(&self) -> RendererHandle<R> {
        self.renderer.clone()
    }

    /// Attempts to upload raw RGBA pixels as a renderer-owned texture.
    ///
    /// Dimensions and byte length are checked before an id is allocated or backend state is
    /// mutated. The texture is tracked by the canvas only after the backend reports success.
    pub fn try_load_texture_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, String> {
        crate::atlas::validate_rgba_buffer(width, height, pixels.len())?;
        let next_texture_id = self.next_texture_id.checked_add(1).ok_or_else(|| String::from("Texture id space exhausted"))?;
        let id = TextureId::new(self.next_texture_id, width, height);
        self.renderer.scope_mut(|r| r.create_texture(id, width, height, pixels))?;
        self.next_texture_id = next_texture_id;
        self.textures.insert(id, TextureInfo { width, height });
        Ok(id)
    }

    /// Uploads raw RGBA pixels as a renderer-owned texture.
    ///
    /// Panics if the RGBA dimensions/byte length are invalid or the backend rejects the upload.
    /// Prefer [`Canvas::try_load_texture_rgba`] when callers can handle upload failure.
    #[track_caller]
    pub fn load_texture_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId {
        self.try_load_texture_rgba(width, height, pixels).expect("failed to upload RGBA texture")
    }

    /// Destroys a texture allocated via [`Canvas::load_texture_rgba`].
    pub fn free_texture(&mut self, id: TextureId) {
        if self.textures.remove(&id).is_some() {
            self.renderer.scope_mut(|r| r.destroy_texture(id));
        }
    }

    /// Draws either an atlas slot or an external texture inside `rect`.
    pub fn draw_image(&mut self, image: Image, rect: Recti, color: Color) {
        match image {
            Image::Slot(slot) => self.render_scope(|frame| frame.draw_slot(slot, rect, color)),
            Image::Texture(tex) if self.textures.contains_key(&tex) => self.render_scope(|frame| frame.draw_texture(tex, rect, color)),
            Image::Texture(_) => (),
        }
    }
}

/// Per-scope drawing facade used while the renderer is mutably borrowed.
pub(crate) struct CanvasFrame<'a, R: Renderer> {
    /// Mutably borrowed renderer for the duration of the scope.
    renderer: &'a mut R,
    /// Current viewport dimensions in pixels.
    current_dim: Dimensioni,
    /// Atlas used by atlas-backed draw commands.
    atlas: &'a AtlasHandle,
    /// Atlas texture dimensions used for UV normalization.
    atlas_dim: Dimensioni,
    /// Rectangle of the solid white icon.
    white_icon_rect: Recti,
    /// Shared active clip rectangle.
    clip: &'a mut Recti,
    /// External texture dimensions visible to this frame.
    textures: &'a HashMap<TextureId, TextureInfo>,
    /// Reusable glyph rectangle batch owned by the parent canvas.
    rect_batch: &'a mut Vec<(Recti, Recti, Color)>,
}

impl<R: Renderer> CanvasFrame<'_, R> {
    #[inline(never)]
    /// Pushes one atlas-backed rectangle.
    pub(crate) fn push_rect(&mut self, dst: Recti, src: Recti, color: Color) {
        let rects = [(dst, src, color)];
        self.push_rects(&rects);
    }

    #[inline(never)]
    /// Converts clipped atlas rectangles into renderer quad vertices.
    pub(crate) fn push_rects(&mut self, rects: &[(Recti, Recti, Color)]) {
        if rects.is_empty() {
            return;
        }
        let atlas_dim = self.atlas_dim;
        let clip = *self.clip;
        for (dst, src, color) in rects {
            if let Some((dst, src)) = Canvas::<R>::clip_rect(*dst, *src, clip) {
                let [v0, v1, v2, v3] = textured_quad_vertices(dst, src, atlas_dim, *color);
                self.renderer.push_quad_vertices(&v0, &v1, &v2, &v3);
            }
        }
    }

    /// Draws a solid rectangle by sampling the atlas white pixel.
    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) {
        self.push_rect(rect, self.white_icon_rect, color);
    }

    #[inline(never)]
    /// Expands text into atlas quads and submits them in one batch.
    pub(crate) fn draw_chars(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        let mut rect_batch = std::mem::take(self.rect_batch);
        rect_batch.clear();
        {
            let rect_batch = &mut rect_batch;
            self.atlas.draw_string(font, text, |_, _, dst, src| {
                let dst = Rect::new(pos.x + dst.x, pos.y + dst.y, dst.width, dst.height);
                rect_batch.push((dst, src, color));
            });
        }
        self.push_rects(rect_batch.as_slice());
        *self.rect_batch = rect_batch;
    }

    /// Draws an atlas icon centered inside the destination rectangle.
    pub(crate) fn draw_icon(&mut self, id: IconId, r: Recti, color: Color) {
        let src = self.atlas.get_icon_rect(id);
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    /// Draws a named atlas slot centered inside the destination rectangle.
    pub(crate) fn draw_slot(&mut self, id: SlotId, r: Recti, color: Color) {
        let src = self.atlas.get_slot_rect(id);
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    /// Renders dynamic slot pixels before drawing the slot quad.
    pub(crate) fn draw_slot_with_function(&mut self, id: SlotId, r: Recti, color: Color, payload: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let src = self.atlas.get_slot_rect(id);
        let mut atlas = self.atlas.clone();
        atlas.render_slot(id, payload);
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    /// Updates the shared clip rectangle for this canvas frame.
    pub(crate) fn set_clip_rect(&mut self, rect: Recti) {
        *self.clip = rect;
    }

    /// Clips retained custom triangles against the active frame clip and submits survivors.
    pub(crate) fn draw_triangles(&mut self, vertices: &[Vertex]) {
        if vertices.is_empty() {
            return;
        }
        let frame_bounds = Recti::new(0, 0, self.current_dim.width.max(0), self.current_dim.height.max(0));
        let clip = (*self.clip).intersect(&frame_bounds).unwrap_or_default();
        if clip.width <= 0 || clip.height <= 0 {
            return;
        }

        for triangle in vertices.chunks_exact(3) {
            clip_triangle_vertices_to_rect(triangle[0], triangle[1], triangle[2], clip, |a, b, c| {
                self.renderer.push_triangle_vertices(&a, &b, &c);
            });
        }
    }

    /// Dispatches an image draw to either an atlas slot or external texture.
    pub(crate) fn draw_image(&mut self, image: Image, rect: Recti, color: Color) {
        match image {
            Image::Slot(slot) => self.draw_slot(slot, rect, color),
            Image::Texture(tex) => self.draw_texture(tex, rect, color),
        }
    }

    /// Draws an uploaded external texture with normalized source coordinates.
    pub(crate) fn draw_texture(&mut self, texture: TextureId, rect: Recti, color: Color) {
        let info = match self.textures.get(&texture) {
            Some(info) => *info,
            None => return,
        };
        let src = Recti::new(0, 0, info.width, info.height);
        let clip = *self.clip;
        if let Some((dst, src)) = Canvas::<R>::clip_rect(rect, src, clip) {
            self.renderer
                .draw_texture(texture, textured_quad_vertices(dst, src, Dimensioni::new(info.width, info.height), color));
        }
    }
}

impl<R: Renderer> Drop for Canvas<R> {
    /// Releases all renderer-owned textures allocated through the canvas.
    fn drop(&mut self) {
        let ids: Vec<_> = self.textures.keys().copied().collect();
        self.renderer.scope_mut(|r| {
            for id in &ids {
                r.destroy_texture(*id);
            }
        });
        self.textures.clear();
    }
}

#[cfg(test)]
mod tests;
