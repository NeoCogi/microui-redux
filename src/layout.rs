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
pub(crate) struct SpanState {
    widths: Vec<SizePolicy>,
    height: SizePolicy,
    item_index: usize,
}

#[derive(Clone)]
pub(crate) enum LayoutDirection {
    Row(SpanState),
    Column(SpanState),
}

impl Default for LayoutDirection {
    fn default() -> Self {
        LayoutDirection::Row(SpanState::default())
    }
}

impl LayoutDirection {
    fn row_state(&self) -> &SpanState {
        match self {
            LayoutDirection::Row(state) | LayoutDirection::Column(state) => state,
        }
    }

    fn row_state_mut(&mut self) -> &mut SpanState {
        match self {
            LayoutDirection::Row(state) | LayoutDirection::Column(state) => state,
        }
    }
}

#[derive(Clone, Default)]
struct Layout {
    body: Recti,
    position: Vec2i,
    max: Option<Vec2i>,
    next_row: i32,
    indent: i32,
    direction: LayoutDirection,
}

#[derive(Clone, Default)]
pub(crate) struct LayoutManager {
    pub style: Style,
    pub last_rect: Recti,
    stack: Vec<Layout>,
}

impl LayoutManager {
    pub fn reset(&mut self, body: Recti, scroll: Vec2i) {
        self.stack.clear();
        self.last_rect = Recti::default();
        self.push_layout(body, scroll);
    }

    fn push_layout(&mut self, body: Recti, scroll: Vec2i) {
        let mut layout = Layout {
            body: Recti { x: 0, y: 0, width: 0, height: 0 },
            position: Vec2i { x: 0, y: 0 },
            max: None,
            next_row: 0,
            indent: 0,
            direction: LayoutDirection::default(),
        };
        layout.body = rect(body.x - scroll.x, body.y - scroll.y, body.width, body.height);

        self.stack.push(layout);
        self.row(&[SizePolicy::Auto], SizePolicy::Auto);
    }

    fn top(&self) -> &Layout {
        self.stack.last().expect("Layout stack should never be empty when accessed")
    }

    fn top_mut(&mut self) -> &mut Layout {
        self.stack.last_mut().expect("Layout stack should never be empty when accessed")
    }

    pub fn current_body(&self) -> Recti {
        self.top().body
    }

    pub fn current_max(&self) -> Option<Vec2i> {
        self.top().max
    }

    pub fn pop_scope(&mut self) {
        self.stack.pop();
    }

    pub fn adjust_indent(&mut self, delta: i32) {
        self.top_mut().indent += delta;
    }

    pub fn begin_column(&mut self) {
        let layout_rect = self.next();
        self.push_layout(layout_rect, vec2(0, 0));
        if let Some(top) = self.stack.last_mut() {
            top.direction = LayoutDirection::Column(SpanState::default());
        }
        self.row(&[SizePolicy::Auto], SizePolicy::Auto);
    }

    pub fn end_column(&mut self) {
        let finished = self.stack.pop().expect("cannot end column without an active child layout");
        let parent = self.top_mut();

        parent.position.x = if parent.position.x > finished.position.x + finished.body.x - parent.body.x {
            parent.position.x
        } else {
            finished.position.x + finished.body.x - parent.body.x
        };
        parent.next_row = if parent.next_row > finished.next_row + finished.body.y - parent.body.y {
            parent.next_row
        } else {
            finished.next_row + finished.body.y - parent.body.y
        };

        match (&mut parent.max, finished.max) {
            (None, None) => (),
            (Some(_), None) => (),
            (None, Some(m)) => parent.max = Some(m),
            (Some(am), Some(bm)) => {
                parent.max = Some(Vec2i::new(max(am.x, bm.x), max(am.y, bm.y)));
            }
        }
    }

    fn row_for_layout(&mut self, height: SizePolicy) {
        let layout = self.top_mut();
        {
            let state = layout.direction.row_state_mut();
            state.height = height;
            state.item_index = 0;
        }
        layout.position = vec2(layout.indent, layout.next_row);
    }

    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy) {
        let layout = self.top_mut();
        {
            let state = layout.direction.row_state_mut();
            state.widths.clear();
            state.widths.extend_from_slice(widths);
            state.height = height;
            state.item_index = 0;
        }
        layout.position = vec2(layout.indent, layout.next_row);
    }

    fn resolve_horizontal(&self, cursor_x: i32, policy: SizePolicy, default_width: i32) -> i32 {
        // Amount of horizontal space left in the current scope after the cursor.
        let available_width = self.top().body.width.saturating_sub(cursor_x);
        // Let the policy (Auto/Fixed/Remainder) clamp that space to a final width.
        policy.resolve(default_width, available_width)
    }

    fn resolve_vertical(&self, cursor_y: i32, policy: SizePolicy, default_height: i32) -> i32 {
        // Amount of vertical space left in the current scope after the cursor.
        let available_height = self.top().body.height.saturating_sub(cursor_y);
        // Let the policy decide how tall this cell should be within the remaining room.
        policy.resolve(default_height, available_height)
    }

    pub fn next(&mut self) -> Recti {
        let dcell_size = self.style.default_cell_size;
        let padding = self.style.padding;
        let spacing = self.style.spacing;
        let default_width = dcell_size.width + padding * 2;
        let default_height = dcell_size.height + padding * 2;

        let (row_len, current_index, height_policy) = {
            let layout = self.top();
            let state = layout.direction.row_state();
            let row_len = state.widths.len();
            (row_len, state.item_index, state.height)
        };

        // If we've consumed all cells for this span, reset the row cursor so we start a new row.
        if current_index == row_len {
            self.row_for_layout(height_policy);
        }

        // Determine the width policy for the current cell. This needs to run *after* the
        // potential row reset so the first cell in a new row picks up the configured width.
        let width_policy = {
            let layout = self.top();
            let state = layout.direction.row_state();
            if state.widths.is_empty() {
                SizePolicy::Auto
            } else {
                state.widths.get(state.item_index).copied().unwrap_or(SizePolicy::Auto)
            }
        };

        let mut res: Recti = Recti { x: 0, y: 0, width: 0, height: 0 };

        // Snapshot the current cursor; this is the top-left corner of the cell before sizing.
        {
            let layout = self.top();
            res.x = layout.position.x;
            res.y = layout.position.y;
        }

        // Resolve the actual width/height using the active policies and remaining space.
        res.width = self.resolve_horizontal(res.x, width_policy, default_width);
        res.height = self.resolve_vertical(res.y, height_policy, default_height);

        // Advance the span index so the next call fetches the next column.
        {
            let layout = self.top_mut();
            let state = layout.direction.row_state_mut();
            if state.item_index < state.widths.len() {
                state.item_index += 1;
            }
        }

        // Move the horizontal cursor to the end of this cell plus spacing, and update
        // `next_row` to track the tallest cell seen so far (so we know where to drop down).
        {
            let layout = self.top_mut();
            layout.position.x = layout.position.x.saturating_add(res.width).saturating_add(spacing);
            layout.next_row = if layout.next_row > res.y + res.height + spacing {
                layout.next_row
            } else {
                res.y + res.height + spacing
            };
        }

        // Convert from local coordinates (relative to the layout scope) to absolute coordinates.
        res.x += self.top().body.x;
        res.y += self.top().body.y;

        // Track the maximum extent reached so scrolling/auto-sizing can use it later.
        {
            let layout = self.top_mut();
            match layout.max {
                None => layout.max = Some(Vec2i::new(res.x + res.width, res.y + res.height)),
                Some(am) => layout.max = Some(Vec2i::new(max(am.x, res.x + res.width), max(am.y, res.y + res.height))),
            }
        }

        self.last_rect = res;
        self.last_rect
    }
}
