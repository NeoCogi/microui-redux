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

#[derive(Clone, Default)]
struct Row {
    start: usize,
    len: usize,
    item_index: usize,
}

#[derive(Default, Copy, Clone)]
pub struct Layout {
    pub body: Recti,
    pub next: Recti,
    pub position: Vec2i,
    pub size: Dimensioni,
    pub max: Option<Vec2i>,
    pub next_row: i32,
    pub indent: i32,
}

#[derive(Clone, Default)]
pub(crate) struct LayoutManager {
    pub style: Style,
    pub last_rect: Recti,
    pub stack: Vec<Layout>,
    pub row_widths_stack: Vec<i32>,
    row_stack: Vec<Row>,

    pub current_row_widths: Vec<i32>,
    pub item_index: usize,
}

impl LayoutManager {
    pub fn push_layout(&mut self, body: Recti, scroll: Vec2i) {
        let mut layout: Layout = Layout {
            body: Recti { x: 0, y: 0, width: 0, height: 0 },
            next: Recti { x: 0, y: 0, width: 0, height: 0 },
            position: Vec2i { x: 0, y: 0 },
            size: Dimension { width: 0, height: 0 },
            max: None,
            next_row: 0,
            indent: 0,
        };
        layout.body = rect(body.x - scroll.x, body.y - scroll.y, body.width, body.height);
        //layout.max = vec2(-i32::MAX, -i32::MAX);
        self.stack.push(layout);
        self.row(&[0], 0);
    }

    pub fn top(&self) -> &Layout {
        return self.stack.last().unwrap();
    }

    pub fn top_mut(&mut self) -> &mut Layout {
        return self.stack.last_mut().unwrap();
    }

    pub fn begin_column(&mut self) {
        let layout = self.next();

        let row = Row {
            start: self.row_stack.len(),
            len: self.current_row_widths.len(),
            item_index: self.item_index,
        };
        for i in 0..self.current_row_widths.len() {
            self.row_widths_stack.push(self.current_row_widths[i]);
        }
        self.current_row_widths.clear();
        self.item_index = 0;
        self.row_stack.push(row);
        self.push_layout(layout, vec2(0, 0));
    }

    pub fn end_column(&mut self) {
        let b = self.top().clone();
        self.stack.pop();
        let row = self.row_stack.pop().unwrap();
        self.current_row_widths.clear();
        for i in 0..row.len {
            self.current_row_widths.push(self.row_widths_stack[i + row.start]);
        }
        self.row_widths_stack.shrink_to(self.row_widths_stack.len() - row.len);
        self.item_index = row.item_index;

        let a = self.top_mut();
        a.position.x = if a.position.x > b.position.x + b.body.x - a.body.x {
            a.position.x
        } else {
            b.position.x + b.body.x - a.body.x
        };
        a.next_row = if a.next_row > b.next_row + b.body.y - a.body.y {
            a.next_row
        } else {
            b.next_row + b.body.y - a.body.y
        };

        // propagate max to the "current" top of the stack (parent) layout
        match (&mut a.max, &b.max) {
            (None, None) => (),
            (Some(_), None) => (),
            (None, Some(m)) => a.max = Some(*m),
            (Some(am), Some(bm)) => {
                a.max = Some(Vec2i::new(max(am.x, bm.x), max(am.y, bm.y)));
            }
        }
    }

    fn row_for_layout(&mut self, height: i32) {
        let layout = self.top_mut();
        layout.position = vec2(layout.indent, layout.next_row);
        layout.size.height = height;
        self.item_index = 0;
    }

    pub fn row(&mut self, widths: &[i32], height: i32) {
        self.current_row_widths.clear();
        for i in 0..widths.len() {
            self.current_row_widths.push(widths[i]);
        }
        self.row_for_layout(height);
    }

    pub fn set_width(&mut self, width: i32) {
        self.top_mut().size.width = width;
    }

    pub fn set_height(&mut self, height: i32) {
        self.top_mut().size.height = height;
    }

    pub fn next(&mut self) -> Recti {
        let dcell_size = self.style.default_cell_size;
        let padding = self.style.padding;
        let spacing = self.style.spacing;
        let row_cells_count = self.current_row_widths.len();

        let mut res: Recti = Recti { x: 0, y: 0, width: 0, height: 0 };

        let lsize_y = self.top().size.height;

        // next grid line
        if self.item_index == row_cells_count {
            self.row_for_layout(lsize_y);
        }

        res.x = self.top().position.x;
        res.y = self.top().position.y;
        res.width = if self.current_row_widths.len() > 0 {
            self.current_row_widths[self.item_index]
        } else {
            self.top().size.width
        };
        res.height = self.top().size.height;

        if res.width == 0 {
            res.width = dcell_size.width + padding * 2;
        }
        if res.height == 0 {
            res.height = dcell_size.height + padding * 2;
        }
        if res.width < 0 {
            res.width += self.top().body.width - res.x + 1;
        }
        if res.height < 0 {
            res.height += self.top().body.height - res.y + 1;
        }

        // ensure it will never exceeds
        if self.item_index < row_cells_count {
            self.item_index += 1;
        }

        ///////////
        // update the next position/row/body/max/...
        ////////
        self.top_mut().position.x += res.width + spacing;
        self.top_mut().next_row = if self.top().next_row > res.y + res.height + spacing {
            self.top().next_row
        } else {
            res.y + res.height + spacing
        };

        res.x += self.top().body.x;
        res.y += self.top().body.y;

        match self.top_mut().max {
            None => self.top_mut().max = Some(Vec2i::new(res.x + res.width, res.y + res.height)),
            Some(am) => self.top_mut().max = Some(Vec2i::new(max(am.x, res.x + res.width), max(am.y, res.y + res.height))),
        }
        self.last_rect = res;
        return self.last_rect;
    }
}
