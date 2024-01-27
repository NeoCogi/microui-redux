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
use super::*;
use std::cell::RefCell;

#[derive(Clone)]
pub enum Command {
    Clip {
        rect: Recti,
    },
    Recti {
        rect: Recti,
        color: Color,
    },
    Text {
        font: FontId,
        pos: Vec2i,
        color: Color,
        str_start: usize,
        str_len: usize,
    },
    Icon {
        rect: Recti,
        id: Icon,
        color: Color,
    },
    None,
}

impl Default for Command {
    fn default() -> Self {
        Command::None
    }
}

#[derive(Clone)]
pub struct Container {
    pub(crate) id: Id,
    pub(crate) atlas: Rc<dyn Atlas>,
    pub style: Style,
    pub name: String,
    pub rect: Recti,
    pub body: Recti,
    pub content_size: Vec2i,
    pub scroll: Vec2i,
    pub zindex: i32,
    pub open: bool,
    pub command_list: Vec<Command>,
    pub clip_stack: Vec<Recti>,
    pub children: Vec<usize>,
    pub is_root: bool,
    pub text_stack: Vec<char>,
    pub(crate) layout: LayoutManager,
}

impl Container {
    pub fn push_clip_rect(&mut self, rect: Recti) {
        let last = self.get_clip_rect();
        self.clip_stack.push(rect.intersect(&last).unwrap_or_default());
    }

    pub fn pop_clip_rect(&mut self) {
        self.clip_stack.pop();
    }

    pub fn get_clip_rect(&mut self) -> Recti {
        match self.clip_stack.last() {
            Some(r) => *r,
            None => UNCLIPPED_RECT,
        }
    }

    pub fn check_clip(&mut self, r: Recti) -> Clip {
        let cr = self.get_clip_rect();
        if r.x > cr.x + cr.width || r.x + r.width < cr.x || r.y > cr.y + cr.height || r.y + r.height < cr.y {
            return Clip::All;
        }
        if r.x >= cr.x && r.x + r.width <= cr.x + cr.width && r.y >= cr.y && r.y + r.height <= cr.y + cr.height {
            return Clip::None;
        }
        return Clip::Part;
    }

    pub fn push_command(&mut self, cmd: Command) {
        self.command_list.push(cmd);
    }

    pub fn push_text(&mut self, str: &str) -> usize {
        let str_start = self.text_stack.len();
        for c in str.chars() {
            self.text_stack.push(c);
        }
        return str_start;
    }

    pub fn set_clip(&mut self, rect: Recti) {
        self.push_command(Command::Clip { rect });
    }

    pub fn draw_rect(&mut self, mut rect: Recti, color: Color) {
        rect = rect.intersect(&self.get_clip_rect()).unwrap_or_default();
        if rect.width > 0 && rect.height > 0 {
            self.push_command(Command::Recti { rect, color });
        }
    }

    pub fn draw_box(&mut self, r: Recti, color: Color) {
        self.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
        self.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
        self.draw_rect(rect(r.x, r.y, 1, r.height), color);
        self.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
    }

    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        let tsize = self.atlas.get_text_size(font, str);
        let rect: Recti = rect(pos.x, pos.y, tsize.width, tsize.height);
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }

        let str_start = self.push_text(str);
        self.push_command(Command::Text {
            str_start,
            str_len: str.len(),
            pos,
            color,
            font,
        });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub fn draw_icon(&mut self, id: Icon, rect: Recti, color: Color) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::Icon { id, rect, color });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub fn text(&mut self, text: &str) {
        let font = self.style.font;
        let color = self.style.colors[ControlColor::Text as usize];
        let h = self.atlas.get_font_height(font) as i32;
        self.layout.begin_column();
        self.layout.row(&[-1], h);

        // lines() doesn't count line terminator
        for line in text.lines() {
            let mut r = self.layout.next();
            let mut rx = r.x;
            let words = line.split_inclusive(' ');
            for w in words {
                // TODO: split w when its width > w into many lines
                let tw = self.atlas.get_text_size(font, w).width;
                if tw + rx < r.x + r.width {
                    self.draw_text(font, w, vec2(rx, r.y), color);
                    rx += tw;
                } else {
                    r = self.layout.next();
                    rx = r.x;
                }
            }
        }
        self.layout.end_column();
    }

    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
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
}

pub type ContainerHandle = Rc<RefCell<Container>>;
