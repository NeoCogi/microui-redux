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
use std::borrow::BorrowMut;

#[derive(Default, Copy, Clone)]
#[repr(C)]
pub struct Vertex {
    pos: Vec2f,
    tex: Vec2f,
    color: Color4b,
}

pub struct Canvas<R: Renderer> {
    current_dim: Dimensioni,
    renderer: RendererHandle<R>,
    clip: Recti,
}

impl<R: Renderer> Canvas<R> {
    pub fn from(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        Self {
            current_dim: dim,
            renderer,
            clip: Recti::new(0, 0, dim.width, dim.height),
        }
    }

    pub fn get_atlas(&self) -> AtlasHandle {
        self.renderer.scope(|r| r.get_atlas())
    }

    #[inline(never)]
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
    pub fn push_rect(&mut self, dst: Recti, src: Recti, color: Color) {
        let atlas_dim = self.renderer.scope(|r| r.get_atlas()).get_texture_dimension();

        let clip = self.clip;
        self.renderer.scope_mut(move |r| {
            match Self::clip_rect(dst, src, clip) {
                Some((dst, src)) => {
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
                None => (),
            }
        })
    }

    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        let icon_rect = self.renderer.scope(|r| r.get_atlas()).get_icon_rect(WHITE_ICON);
        self.push_rect(rect, icon_rect, color);
    }

    #[inline(never)]
    pub fn draw_chars(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        let atlas = self.renderer.scope(|r| r.get_atlas());
        atlas.draw_string(font, text, |_, _, dst, src| {
            let dst = Rect::new(pos.x + dst.x, pos.y + dst.y, dst.width, dst.height);
            self.push_rect(dst, src, color)
        });
    }

    pub fn draw_icon(&mut self, id: IconId, r: Recti, color: Color) {
        let src = self.renderer.scope(|r| r.get_atlas()).get_icon_rect(id);
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    pub fn draw_slot(&mut self, id: SlotId, r: Recti, color: Color) {
        let src = self.renderer.scope(|r| r.get_atlas()).get_slot_rect(id);
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    pub fn draw_slot_with_function(&mut self, id: SlotId, r: Recti, color: Color, payload: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let src = self.renderer.scope(|r| r.get_atlas()).get_slot_rect(id);
        let pl = payload.clone();
        self.renderer.scope_mut(move |r| r.get_atlas().borrow_mut().render_slot(id, pl.clone()));
        let x = r.x + (r.width - src.width) / 2;
        let y = r.y + (r.height - src.height) / 2;
        self.push_rect(rect(x, y, src.width, src.height), src, color);
    }

    pub fn set_clip_rect(&mut self, rect: Recti) {
        self.clip = rect;
    }

    pub fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.current_dim = Dimensioni::new(width, height);
        self.set_clip_rect(Rect::new(0, 0, width, height));
        self.renderer.scope_mut(move |r| r.begin(width, height, clr));
    }

    pub fn end(&mut self) {
        self.renderer.scope_mut(|r| r.end())
    }

    pub fn current_dimension(&self) -> Dimensioni {
        self.current_dim
    }
}
