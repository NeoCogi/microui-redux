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

/// Describes how a layout dimension should be resolved.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SizePolicy {
    Auto,           // Example: `0` width -> fall back to style default (e.g. 84px button slot)
    Fixed(i32),     // Example: `Fixed(120)` -> the cell is always 120px wide
    Remainder(i32), // Example: `Remainder(0)` fills leftovers; `Remainder(9)` keeps 9px margin
}

impl SizePolicy {
    fn resolve(self, default_size: i32, available_space: i32) -> i32 {
        let resolved = match self {
            SizePolicy::Auto => default_size,
            SizePolicy::Fixed(value) => value,
            SizePolicy::Remainder(margin) => available_space.saturating_sub(margin),
        };
        resolved.max(0)
    }
}

impl Default for SizePolicy {
    fn default() -> Self {
        SizePolicy::Auto
    }
}

#[derive(Clone, Default)]
struct Row {
    start: usize,
    len: usize,
    item_index: usize,
    height: SizePolicy,
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
    pub row_widths_stack: Vec<SizePolicy>,
    row_stack: Vec<Row>,

    pub current_row_widths: Vec<SizePolicy>,
    current_row_height: SizePolicy,
    pub item_index: usize,
}

impl LayoutManager {
    pub fn reset(&mut self, body: Recti, scroll: Vec2i) {
        self.stack.clear();
        self.row_widths_stack.clear();
        self.row_stack.clear();
        self.current_row_widths.clear();
        self.current_row_height = SizePolicy::Auto;
        self.item_index = 0;
        self.last_rect = Recti::default();
        self.push_layout(body, scroll);
    }

    fn push_layout(&mut self, body: Recti, scroll: Vec2i) {
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

        self.stack.push(layout);
        self.row(&[SizePolicy::Auto], SizePolicy::Auto);
    }

    pub fn top(&self) -> &Layout {
        self.stack.last().expect("Layout stack should never be empty when accessed")
    }

    pub fn top_mut(&mut self) -> &mut Layout {
        self.stack.last_mut().expect("Layout stack should never be empty when accessed")
    }

    pub fn begin_column(&mut self) {
        let layout = self.next();

        let row = Row {
            start: self.row_stack.len(),
            len: self.current_row_widths.len(),
            item_index: self.item_index,
            height: self.current_row_height,
        };
        // backup the parent row's width policies so we can restore them in end_column()
        for width in &self.current_row_widths {
            self.row_widths_stack.push(*width);
        }
        self.current_row_widths.clear();
        self.item_index = 0;
        self.row_stack.push(row);
        // consume the current cell and treat it as a nested layout scope
        self.push_layout(layout, vec2(0, 0));
    }

    pub fn end_column(&mut self) {
        let b = self.top().clone();
        self.stack.pop();
        let row = self.row_stack.pop().expect("Row stack should not be empty");
        // restore the parent row configuration that was active before begin_column()
        self.current_row_widths.clear();
        for i in 0..row.len {
            // index = i + row.start
            let index = i.saturating_add(row.start);
            if let Some(width) = self.row_widths_stack.get(index) {
                self.current_row_widths.push(*width);
            }
        }
        let new_len = self.row_widths_stack.len().saturating_sub(row.len);
        self.row_widths_stack.shrink_to(new_len);
        self.current_row_height = row.height;
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

    fn row_for_layout(&mut self, height: SizePolicy) {
        self.current_row_height = height;
        let layout = self.top_mut();
        layout.position = vec2(layout.indent, layout.next_row);
        self.item_index = 0;
    }

    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy) {
        self.current_row_widths.clear();
        self.current_row_widths.extend_from_slice(widths);
        self.row_for_layout(height);
    }

    fn resolve_horizontal(&self, cursor_x: i32, policy: SizePolicy, default_width: i32) -> i32 {
        let available_width = self.top().body.width.saturating_sub(cursor_x);
        policy.resolve(default_width, available_width)
    }

    fn resolve_vertical(&self, cursor_y: i32, policy: SizePolicy, default_height: i32) -> i32 {
        let available_height = self.top().body.height.saturating_sub(cursor_y);
        policy.resolve(default_height, available_height)
    }

    pub fn next(&mut self) -> Recti {
        let dcell_size = self.style.default_cell_size;
        let padding = self.style.padding;
        let spacing = self.style.spacing;
        let default_width = dcell_size.width + padding * 2;
        let default_height = dcell_size.height + padding * 2;
        let row_cells_count = self.current_row_widths.len();

        let mut res: Recti = Recti { x: 0, y: 0, width: 0, height: 0 };

        // start a new row if the previous span was fully consumed
        if self.item_index == row_cells_count {
            let height_policy = self.current_row_height;
            self.row_for_layout(height_policy);
        }

        res.x = self.top().position.x;
        res.y = self.top().position.y;

        let width_policy = if row_cells_count > 0 {
            self.current_row_widths.get(self.item_index).copied().unwrap_or(SizePolicy::Auto)
        } else {
            SizePolicy::Auto
        };

        res.width = self.resolve_horizontal(res.x, width_policy, default_width);
        res.height = self.resolve_vertical(res.y, self.current_row_height, default_height);

        // ensure it will never exceeds
        if self.item_index < row_cells_count {
            self.item_index += 1;
        }

        ///////////
        // update the next position/row/body/max/...
        ////////
        self.top_mut().position.x = self.top().position.x.saturating_add(res.width).saturating_add(spacing);
        self.top_mut().next_row = if self.top().next_row > res.y + res.height + spacing {
            self.top().next_row
        } else {
            res.y + res.height + spacing
        };

        res.x += self.top().body.x;
        res.y += self.top().body.y;

        // track the furthest extent reached so far for content-size computation
        match self.top_mut().max {
            None => self.top_mut().max = Some(Vec2i::new(res.x + res.width, res.y + res.height)),
            Some(am) => self.top_mut().max = Some(Vec2i::new(max(am.x, res.x + res.width), max(am.y, res.y + res.height))),
        }
        self.last_rect = res;
        return self.last_rect;
    }
}
