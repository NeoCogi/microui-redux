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
use crate::*;
use std::cmp::{max, min};
use std::rc::Rc;

pub(crate) struct DrawCtx<'a> {
    commands: &'a mut Vec<Command>,
    clip_stack: &'a mut Vec<Recti>,
    style: &'a Style,
    atlas: &'a AtlasHandle,
}

impl<'a> DrawCtx<'a> {
    pub(crate) fn new(commands: &'a mut Vec<Command>, clip_stack: &'a mut Vec<Recti>, style: &'a Style, atlas: &'a AtlasHandle) -> Self {
        Self { commands, clip_stack, style, atlas }
    }

    pub(crate) fn style(&self) -> &Style { self.style }

    pub(crate) fn atlas(&self) -> &AtlasHandle { self.atlas }

    pub(crate) fn current_clip_rect(&self) -> Recti { self.clip_stack.last().copied().unwrap_or(UNCLIPPED_RECT) }

    pub(crate) fn push_clip_rect(&mut self, rect: Recti) {
        let last = self.current_clip_rect();
        self.clip_stack.push(rect.intersect(&last).unwrap_or_default());
    }

    pub(crate) fn pop_clip_rect(&mut self) { self.clip_stack.pop(); }

    pub(crate) fn push_command(&mut self, cmd: Command) { self.commands.push(cmd); }

    pub(crate) fn set_clip(&mut self, rect: Recti) { self.push_command(Command::Clip { rect }); }

    pub(crate) fn check_clip(&self, r: Recti) -> Clip {
        let cr = self.current_clip_rect();
        if r.x > cr.x + cr.width || r.x + r.width < cr.x || r.y > cr.y + cr.height || r.y + r.height < cr.y {
            return Clip::All;
        }
        if r.x >= cr.x && r.x + r.width <= cr.x + cr.width && r.y >= cr.y && r.y + r.height <= cr.y + cr.height {
            return Clip::None;
        }
        Clip::Part
    }

    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) {
        let rect = rect.intersect(&self.current_clip_rect()).unwrap_or_default();
        if rect.width > 0 && rect.height > 0 {
            self.push_command(Command::Recti { rect, color });
        }
    }

    pub(crate) fn draw_box(&mut self, r: Recti, color: Color) {
        self.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
        self.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
        self.draw_rect(rect(r.x, r.y, 1, r.height), color);
        self.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
    }

    pub(crate) fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        let tsize = self.atlas.get_text_size(font, text);
        let rect = rect(pos.x, pos.y, tsize.width, tsize.height);
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.current_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }

        self.push_command(Command::Text {
            text: String::from(text),
            pos,
            color,
            font,
        });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub(crate) fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.current_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::Icon { id, rect, color });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub(crate) fn push_image(&mut self, image: Image, rect: Recti, color: Color) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.current_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::Image { image, rect, color });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub(crate) fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.current_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::SlotRedraw { id, rect, color, payload: f });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub(crate) fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let color = self.style.colors[colorid as usize];
        self.draw_rect(rect, color);
        if colorid == ControlColor::ScrollBase || colorid == ControlColor::ScrollThumb || colorid == ControlColor::TitleBG {
            return;
        }
        let border_color = self.style.colors[ControlColor::Border as usize];
        if border_color.a != 0 {
            self.draw_box(expand_rect(rect, 1), border_color);
        }
    }

    pub(crate) fn draw_widget_frame(&mut self, focused: bool, hovered: bool, rect: Recti, mut colorid: ControlColor, opt: WidgetOption) {
        if opt.has_no_frame() {
            return;
        }
        if focused {
            colorid.focus()
        } else if hovered {
            colorid.hover()
        }
        self.draw_frame(rect, colorid);
    }

    pub(crate) fn draw_control_text(&mut self, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let mut pos = Vec2i { x: 0, y: 0 };
        let font = self.style.font;
        let tsize = self.atlas.get_text_size(font, text);
        let padding = self.style.padding;
        let color = self.style.colors[colorid as usize];
        let line_height = self.atlas.get_font_height(font) as i32;
        let baseline = self.atlas.get_font_baseline(font);

        self.push_clip_rect(rect);
        pos.y = Self::baseline_aligned_top(rect, line_height, baseline);
        if opt.is_aligned_center() {
            pos.x = rect.x + (rect.width - tsize.width) / 2;
        } else if opt.is_aligned_right() {
            pos.x = rect.x + rect.width - tsize.width - padding;
        } else {
            pos.x = rect.x + padding;
        }
        self.draw_text(font, text, pos, color);
        self.pop_clip_rect();
    }

    fn baseline_aligned_top(rect: Recti, line_height: i32, baseline: i32) -> i32 {
        if rect.height >= line_height {
            return rect.y + (rect.height - line_height) / 2;
        }

        let baseline_center = rect.y + rect.height / 2;
        let min_top = rect.y + rect.height - line_height;
        let max_top = rect.y;
        Self::clamp(baseline_center - baseline, min_top, max_top)
    }

    fn clamp(x: i32, a: i32, b: i32) -> i32 { min(b, max(a, x)) }
}
