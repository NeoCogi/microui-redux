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
use super::*;
use std::{borrow::BorrowMut, collections::HashMap};

#[derive(Default, Copy, Clone)]
#[repr(C)]
/// Vertex submitted by the UI.
pub struct Vertex {
    pos: Vec2f,
    tex: Vec2f,
    color: Color4b,
}

impl Vertex {
    /// Creates a vertex with the provided position, texture coordinate, and color.
    pub fn new(pos: Vec2f, tex: Vec2f, color: Color4b) -> Self { Self { pos, tex, color } }

    /// Returns the position of the vertex in screen space.
    pub fn position(&self) -> Vec2f { self.pos }

    /// Returns the texture coordinates associated with the vertex.
    pub fn tex_coord(&self) -> Vec2f { self.tex }

    /// Returns the vertex color.
    pub fn color(&self) -> Color4b { self.color }
}

/// High-level drawing helper that batches draw commands for a renderer.
pub struct Canvas<R: Renderer> {
    current_dim: Dimensioni,
    renderer: RendererHandle<R>,
    clip: Recti,
    next_texture_id: u32,
    textures: HashMap<TextureId, TextureInfo>,
    rect_batch: Vec<(Recti, Recti, Color)>,
}

#[derive(Clone, Copy)]
struct TextureInfo {
    width: i32,
    height: i32,
}

impl<R: Renderer> Canvas<R> {
    /// Creates a canvas around the provided renderer handle.
    pub fn from(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        Self {
            current_dim: dim,
            renderer,
            clip: Recti::new(0, 0, dim.width, dim.height),
            next_texture_id: 1,
            textures: HashMap::new(),
            rect_batch: Vec::new(),
        }
    }

    /// Returns the atlas associated with the renderer.
    pub fn get_atlas(&self) -> AtlasHandle { self.renderer.scope(|r| r.get_atlas()) }

    #[inline(never)]
    /// Computes the clipped destination/source rectangles for rendering.
    pub fn clip_rect(dst_r: Recti, src_r: Recti, clip_r: Recti) -> Option<(Recti, Recti)> {
        match dst_r.intersect(&clip_r) {
            Some(rect) if rect.width == dst_r.width && rect.height == dst_r.height => Some((dst_r, src_r)),
            Some(rect) if rect.width != 0 && rect.height != 0 => {
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
        let atlas_dim = self.renderer.scope(|r| r.get_atlas()).get_texture_dimension();
        let clip = self.clip;
        self.renderer.scope_mut(move |r| {
            for (dst, src, color) in rects {
                if let Some((dst, src)) = Self::clip_rect(*dst, *src, clip) {
                    let x = src.x as f32 / atlas_dim.width as f32;
                    let y = src.y as f32 / atlas_dim.height as f32;
                    let w = src.width as f32 / atlas_dim.width as f32;
                    let h = src.height as f32 / atlas_dim.height as f32;

                    let mut v0 = Vertex::default();
                    let mut v1 = Vertex::default();
                    let mut v2 = Vertex::default();
                    let mut v3 = Vertex::default();

                    // tex coordinates
                    v0.tex.x = x;
                    v0.tex.y = y;
                    v1.tex.x = x + w;
                    v1.tex.y = y;
                    v2.tex.x = x + w;
                    v2.tex.y = y + h;
                    v3.tex.x = x;
                    v3.tex.y = y + h;

                    // position
                    v0.pos.x = dst.x as f32;
                    v0.pos.y = dst.y as f32;
                    v1.pos.x = dst.x as f32 + dst.width as f32;
                    v1.pos.y = dst.y as f32;
                    v2.pos.x = dst.x as f32 + dst.width as f32;
                    v2.pos.y = dst.y as f32 + dst.height as f32;
                    v3.pos.x = dst.x as f32;
                    v3.pos.y = dst.y as f32 + dst.height as f32;

                    // color
                    v0.color = color4b(color.r, color.g, color.b, color.a);
                    v1.color = v0.color;
                    v2.color = v0.color;
                    v3.color = v0.color;

                    r.push_quad_vertices(&v0, &v1, &v2, &v3);
                }
            }
        })
    }

    /// Draws a solid colored rectangle.
    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        let icon_rect = self.renderer.scope(|r| r.get_atlas()).get_icon_rect(WHITE_ICON);
        self.push_rect(rect, icon_rect, color);
    }

    #[inline(never)]
    /// Draws UTF-8 text using the supplied font.
    pub fn draw_chars(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        let atlas = self.renderer.scope(|r| r.get_atlas());
        let mut rect_batch = std::mem::take(&mut self.rect_batch);
        rect_batch.clear();
        {
            let rect_batch = &mut rect_batch;
            atlas.draw_string(font, text, |_, _, dst, src| {
                let dst = Rect::new(pos.x + dst.x, pos.y + dst.y, dst.width, dst.height);
                rect_batch.push((dst, src, color));
            });
        }
        self.push_rects(rect_batch.as_slice());
        self.rect_batch = rect_batch;
    }

    /// Draws an icon centered inside the provided rectangle.
    pub fn draw_icon(&mut self, id: IconId, r: Recti, color: Color) {
        let src = self.renderer.scope(|r| r.get_atlas()).get_icon_rect(id);
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    /// Draws an atlas slot centered inside the provided rectangle.
    pub fn draw_slot(&mut self, id: SlotId, r: Recti, color: Color) {
        let src = self.renderer.scope(|r| r.get_atlas()).get_slot_rect(id);
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    /// Renders a slot with the callback before drawing it.
    pub fn draw_slot_with_function(&mut self, id: SlotId, r: Recti, color: Color, payload: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let src = self.renderer.scope(|r| r.get_atlas()).get_slot_rect(id);
        let pl = payload.clone();
        self.renderer.scope_mut(move |r| r.get_atlas().borrow_mut().render_slot(id, pl.clone()));
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    /// Sets the clip rectangle used for subsequent draw calls.
    pub fn set_clip_rect(&mut self, rect: Recti) { self.clip = rect; }

    /// Returns the clip rectangle currently applied to draw commands.
    pub fn current_clip_rect(&self) -> Recti { self.clip }

    /// Begins a new drawing pass and resets the clip rectangle.
    pub fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.current_dim = Dimensioni::new(width, height);
        self.set_clip_rect(Rect::new(0, 0, width, height));
        self.renderer.scope_mut(move |r| r.begin(width, height, clr));
    }

    /// Ends the current drawing pass.
    pub fn end(&mut self) { self.renderer.scope_mut(|r| r.end()) }

    /// Flushes any buffered geometry without ending the frame.
    pub fn flush(&mut self) { self.renderer.scope_mut(|r| r.flush()) }

    /// Returns the last viewport dimensions passed to [`Canvas::begin`].
    pub fn current_dimension(&self) -> Dimensioni { self.current_dim }

    /// Returns a clone of the underlying renderer handle.
    pub fn renderer_handle(&self) -> RendererHandle<R> { self.renderer.clone() }

    /// Uploads raw RGBA pixels as a renderer-owned texture.
    pub fn load_texture_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId {
        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;
        self.textures.insert(id, TextureInfo { width, height });
        self.renderer.scope_mut(|r| r.create_texture(id, width, height, pixels));
        id
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
            Image::Slot(slot) => self.draw_slot(slot, rect, color),
            Image::Texture(tex) => self.draw_texture(tex, rect, color),
        }
    }

    fn draw_texture(&mut self, texture: TextureId, rect: Recti, color: Color) {
        let info = match self.textures.get(&texture) {
            Some(info) => *info,
            None => return,
        };
        let src = Recti::new(0, 0, info.width, info.height);
        let clip = self.clip;
        if let Some((dst, src)) = Self::clip_rect(rect, src, clip) {
            let mut v0 = Vertex::default();
            let mut v1 = Vertex::default();
            let mut v2 = Vertex::default();
            let mut v3 = Vertex::default();

            let color = color4b(color.r, color.g, color.b, color.a);

            let tex_width = info.width as f32;
            let tex_height = info.height as f32;

            // texture coordinates
            v0.tex.x = src.x as f32 / tex_width;
            v0.tex.y = src.y as f32 / tex_height;
            v1.tex.x = (src.x + src.width) as f32 / tex_width;
            v1.tex.y = src.y as f32 / tex_height;
            v2.tex.x = (src.x + src.width) as f32 / tex_width;
            v2.tex.y = (src.y + src.height) as f32 / tex_height;
            v3.tex.x = src.x as f32 / tex_width;
            v3.tex.y = (src.y + src.height) as f32 / tex_height;

            // positions
            v0.pos.x = dst.x as f32;
            v0.pos.y = dst.y as f32;
            v1.pos.x = (dst.x + dst.width) as f32;
            v1.pos.y = dst.y as f32;
            v2.pos.x = (dst.x + dst.width) as f32;
            v2.pos.y = (dst.y + dst.height) as f32;
            v3.pos.x = dst.x as f32;
            v3.pos.y = (dst.y + dst.height) as f32;

            v0.color = color;
            v1.color = color;
            v2.color = color;
            v3.color = color;

            self.renderer.scope_mut(|r| r.draw_texture(texture, [v0, v1, v2, v3]));
        }
    }
}

impl<R: Renderer> Drop for Canvas<R> {
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
mod tests {
    use super::*;

    struct NoopRenderer;

    impl Renderer for NoopRenderer {
        fn get_atlas(&self) -> AtlasHandle { unimplemented!() }
        fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}
        fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {}
        fn flush(&mut self) {}
        fn end(&mut self) {}
        fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) {}
        fn destroy_texture(&mut self, _id: TextureId) {}
        fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
    }

    fn assert_rect_eq(actual: Recti, expected: Recti) {
        assert_eq!(
            (actual.x, actual.y, actual.width, actual.height),
            (expected.x, expected.y, expected.width, expected.height)
        );
    }

    #[test]
    fn clip_rect_passthrough() {
        let dst = Recti::new(0, 0, 10, 10);
        let src = Recti::new(5, 5, 10, 10);
        let clip = Recti::new(0, 0, 20, 20);
        let res = Canvas::<NoopRenderer>::clip_rect(dst, src, clip).unwrap();
        assert_rect_eq(res.0, dst);
        assert_rect_eq(res.1, src);
    }

    #[test]
    fn clip_rect_partial() {
        let dst = Recti::new(0, 0, 100, 100);
        let src = Recti::new(0, 0, 50, 50);
        let clip = Recti::new(20, 20, 40, 40);
        let res = Canvas::<NoopRenderer>::clip_rect(dst, src, clip).unwrap();
        assert_rect_eq(res.0, Recti::new(20, 20, 40, 40));
        assert_rect_eq(res.1, Recti::new(10, 10, 20, 20));
    }

    #[test]
    fn clip_rect_none() {
        let dst = Recti::new(0, 0, 10, 10);
        let src = Recti::new(0, 0, 10, 10);
        let clip = Recti::new(50, 50, 10, 10);
        assert!(Canvas::<NoopRenderer>::clip_rect(dst, src, clip).is_none());
    }
}
