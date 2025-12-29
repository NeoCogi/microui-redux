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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
use crate::{
    draw_context::DrawCtx, Clip, Color, Color4b, ControlColor, FontId, IconId, Image, Recti, SlotId, Vec2i, WidgetOption,
};
use std::rc::Rc;

pub(crate) trait DrawCtxAccess {
    fn with_draw_ctx<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut DrawCtx<'_>) -> R;
}

pub(crate) trait DrawOps: DrawCtxAccess {
    fn push_clip_rect(&mut self, rect: Recti) { self.with_draw_ctx(|draw| draw.push_clip_rect(rect)); }

    fn pop_clip_rect(&mut self) { self.with_draw_ctx(|draw| draw.pop_clip_rect()); }

    fn set_clip(&mut self, rect: Recti) { self.with_draw_ctx(|draw| draw.set_clip(rect)); }

    fn check_clip(&mut self, r: Recti) -> Clip { self.with_draw_ctx(|draw| draw.check_clip(r)) }

    fn current_clip_rect(&mut self) -> Recti { self.with_draw_ctx(|draw| draw.current_clip_rect()) }

    fn draw_rect(&mut self, rect: Recti, color: Color) { self.with_draw_ctx(|draw| draw.draw_rect(rect, color)); }

    fn draw_box(&mut self, r: Recti, color: Color) { self.with_draw_ctx(|draw| draw.draw_box(r, color)); }

    fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        self.with_draw_ctx(|draw| draw.draw_text(font, text, pos, color));
    }

    fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) { self.with_draw_ctx(|draw| draw.draw_icon(id, rect, color)); }

    fn push_image(&mut self, image: Image, rect: Recti, color: Color) {
        self.with_draw_ctx(|draw| draw.push_image(image, rect, color));
    }

    fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        self.with_draw_ctx(|draw| draw.draw_slot_with_function(id, rect, color, f));
    }

    fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) { self.with_draw_ctx(|draw| draw.draw_frame(rect, colorid)); }

    fn draw_widget_frame(&mut self, focused: bool, hovered: bool, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.with_draw_ctx(|draw| draw.draw_widget_frame(focused, hovered, rect, colorid, opt));
    }

    fn draw_control_text(&mut self, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.with_draw_ctx(|draw| draw.draw_control_text(text, rect, colorid, opt));
    }
}

impl<T: DrawCtxAccess> DrawOps for T {}
