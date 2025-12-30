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
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis};
use crate::text_layout::{build_text_lines, TextLine};

use super::text_edit::{apply_text_input, clamp_scroll, cursor_from_x, cursor_x_in_line, line_index_for_cursor, ReturnBehavior};

#[derive(Clone)]
/// Persistent state for multi-line text area widgets.
pub struct TextArea {
    /// Buffer edited by the text area.
    pub buf: String,
    /// Current cursor position within the buffer (byte index).
    pub cursor: usize,
    /// Scroll offset applied to the text view.
    pub scroll: Vec2i,
    /// Wrapping mode used when rendering the buffer.
    pub wrap: TextWrap,
    /// Widget options applied to the text area.
    pub opt: WidgetOption,
    /// Behaviour options applied to the text area.
    pub bopt: WidgetBehaviourOption,
    preferred_x: Option<i32>,
    dragging_y: bool,
    dragging_x: bool,
    id: Option<Id>,
}

impl TextArea {
    /// Creates a text area with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self {
            buf,
            cursor,
            scroll: vec2(0, 0),
            wrap: TextWrap::None,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
            preferred_x: None,
            dragging_y: false,
            dragging_x: false,
            id: None,
        }
    }

    /// Creates a text area with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self {
            buf,
            cursor,
            scroll: vec2(0, 0),
            wrap: TextWrap::None,
            opt,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
            preferred_x: None,
            dragging_y: false,
            dragging_x: false,
            id: None,
        }
    }

    /// Returns a copy of the text area with an explicit ID.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    fn handle_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState { textarea_handle(ctx, control, self) }
}

fn textarea_handle(ctx: &mut WidgetCtx<'_>, control: &ControlState, state: &mut TextArea) -> ResourceState {
    let mut res = ResourceState::NONE;
    let bounds = ctx.rect();
    if !control.focused {
        state.cursor = state.buf.len();
        state.preferred_x = None;
    }
    let mut cursor_pos = state.cursor.min(state.buf.len());

    let input = ctx.input_or_default();
    let mut ensure_visible = false;
    let mut reset_preferred = false;
    let mut vertical_moved = false;
    let mut preferred_x = state.preferred_x;

    if control.focused {
        let edit = apply_text_input(&mut state.buf, cursor_pos, input, true, ReturnBehavior::Newline { submit_on_ctrl: true });
        cursor_pos = edit.cursor;
        if edit.changed {
            res |= ResourceState::CHANGE;
            ensure_visible = true;
            reset_preferred = true;
        }
        if edit.moved {
            ensure_visible = true;
            reset_preferred = true;
        }
        if edit.submit {
            res |= ResourceState::SUBMIT;
        }
    }

    let style = ctx.style();
    let font = style.font;
    let padding = style.padding;
    let scrollbar_size = style.scrollbar_size;
    let thumb_size = style.thumb_size;
    let line_height = ctx.atlas().get_font_height(font) as i32;
    let baseline = ctx.atlas().get_font_baseline(font);
    let descent = (line_height - baseline).max(0);

    let base_body = bounds;
    let mut body = base_body;
    let mut lines = Vec::new();
    let mut content_width = 0;
    let mut content_height = line_height.max(1);
    let mut needs_v = false;
    let mut needs_h = false;

    for _ in 0..3 {
        let available_width = (body.width - padding * 2).max(0);
        lines = build_text_lines(state.buf.as_str(), state.wrap, available_width, font, ctx.atlas());
        content_width = lines.iter().map(|line| line.width).max().unwrap_or(0);
        content_height = line_height * lines.len() as i32;
        let cs = vec2(content_width + padding * 2, content_height + padding * 2);
        needs_v = cs.y > body.height;
        needs_h = cs.x > body.width;
        let mut new_body = base_body;
        if needs_v {
            new_body.width = (new_body.width - scrollbar_size).max(0);
        }
        if needs_h {
            new_body.height = (new_body.height - scrollbar_size).max(0);
        }
        if new_body.x == body.x && new_body.y == body.y && new_body.width == body.width && new_body.height == body.height {
            break;
        }
        body = new_body;
    }

    if lines.is_empty() {
        lines.push(TextLine { start: 0, end: 0, width: 0 });
    }

    let cs = vec2(content_width + padding * 2, content_height + padding * 2);
    let maxscroll_y = scrollbar_max_scroll(cs.y, body.height);
    let maxscroll_x = scrollbar_max_scroll(cs.x, body.width);

    if let Some(delta) = control.scroll_delta {
        if maxscroll_y > 0 {
            state.scroll.y += delta.y;
        }
        if maxscroll_x > 0 {
            state.scroll.x += delta.x;
        }
    }

    if !input.mouse_down.is_left() {
        state.dragging_y = false;
        state.dragging_x = false;
    }

    let mut clicked_scrollbar = false;
    let mut vscroll_base = bounds;
    let mut hscroll_base = bounds;

    if needs_v && maxscroll_y > 0 && body.height > 0 {
        vscroll_base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
        if input.mouse_pressed.is_left() && vscroll_base.contains(&input.mouse_pos) {
            state.dragging_y = true;
            clicked_scrollbar = true;
        }
        if state.dragging_y {
            state.scroll.y += scrollbar_drag_delta(ScrollAxis::Vertical, input.mouse_delta, cs.y, vscroll_base);
        }
    }

    if needs_h && maxscroll_x > 0 && body.width > 0 {
        hscroll_base = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
        if input.mouse_pressed.is_left() && hscroll_base.contains(&input.mouse_pos) {
            state.dragging_x = true;
            clicked_scrollbar = true;
        }
        if state.dragging_x {
            state.scroll.x += scrollbar_drag_delta(ScrollAxis::Horizontal, input.mouse_delta, cs.x, hscroll_base);
        }
    }

    let mut cursor_line = line_index_for_cursor(&lines, cursor_pos);
    let mut caret_x = cursor_x_in_line(&lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    if control.focused {
        if input.key_code_pressed.is_end() {
            cursor_pos = lines[cursor_line].end;
            caret_x = cursor_x_in_line(&lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());
            ensure_visible = true;
            reset_preferred = true;
        }

        if input.key_code_pressed.is_up() {
            let target_x = preferred_x.unwrap_or(caret_x);
            if cursor_line > 0 {
                cursor_line -= 1;
                cursor_pos = cursor_from_x(&lines[cursor_line], state.buf.as_str(), target_x, font, ctx.atlas());
            }
            preferred_x = Some(target_x);
            ensure_visible = true;
            vertical_moved = true;
        }

        if input.key_code_pressed.is_down() {
            let target_x = preferred_x.unwrap_or(caret_x);
            if cursor_line + 1 < lines.len() {
                cursor_line += 1;
                cursor_pos = cursor_from_x(&lines[cursor_line], state.buf.as_str(), target_x, font, ctx.atlas());
            }
            preferred_x = Some(target_x);
            ensure_visible = true;
            vertical_moved = true;
        }
    }

    if control.focused && input.mouse_pressed.is_left() && ctx.mouse_over(bounds) && !clicked_scrollbar {
        let local_x = input.mouse_pos.x - (body.x + padding) + state.scroll.x;
        let local_y = input.mouse_pos.y - (body.y + padding) + state.scroll.y;
        let line_idx = if lines.is_empty() {
            0
        } else {
            (local_y / line_height).clamp(0, lines.len().saturating_sub(1) as i32) as usize
        };
        cursor_pos = cursor_from_x(&lines[line_idx], state.buf.as_str(), local_x, font, ctx.atlas());
        ensure_visible = true;
        reset_preferred = true;
    }

    cursor_pos = cursor_pos.min(state.buf.len());
    cursor_line = line_index_for_cursor(&lines, cursor_pos);
    caret_x = cursor_x_in_line(&lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    if reset_preferred && !vertical_moved {
        preferred_x = None;
    }
    if preferred_x.is_none() {
        preferred_x = Some(caret_x);
    }

    if ensure_visible && !state.dragging_x && !state.dragging_y {
        let view_width = (body.width - padding * 2).max(0);
        let view_height = (body.height - padding * 2).max(0);
        let caret_y = cursor_line as i32 * line_height;
        if view_width > 0 {
            if caret_x < state.scroll.x {
                state.scroll.x = caret_x;
            } else if caret_x + 1 > state.scroll.x + view_width {
                state.scroll.x = caret_x + 1 - view_width;
            }
        }
        if view_height > 0 {
            if caret_y < state.scroll.y {
                state.scroll.y = caret_y;
            } else if caret_y + line_height > state.scroll.y + view_height {
                state.scroll.y = caret_y + line_height - view_height;
            }
        }
    }

    state.scroll.x = clamp_scroll(state.scroll.x, maxscroll_x);
    state.scroll.y = clamp_scroll(state.scroll.y, maxscroll_y);
    state.cursor = cursor_pos;
    state.preferred_x = preferred_x;

    ctx.draw_widget_frame(control, bounds, ControlColor::Base, state.opt);

    let text_origin = vec2(body.x + padding - state.scroll.x, body.y + padding - state.scroll.y);
    let color = ctx.style().colors[ControlColor::Text as usize];
    ctx.push_clip_rect(body);
    for (idx, line) in lines.iter().enumerate() {
        let line_top = text_origin.y + idx as i32 * line_height;
        let line_bottom = line_top + line_height;
        if line_bottom < body.y || line_top > body.y + body.height {
            continue;
        }
        let text = &state.buf[line.start..line.end];
        if !text.is_empty() {
            ctx.draw_text(font, text, vec2(text_origin.x, line_top), color);
        }
    }

    if control.focused {
        let caret_line_top = text_origin.y + cursor_line as i32 * line_height;
        let baseline_y = caret_line_top + baseline;
        let caret_top = (baseline_y - baseline + 2).max(body.y).min(body.y + body.height);
        let caret_bottom = (baseline_y + descent - 2).max(body.y).min(body.y + body.height);
        let caret_height = (caret_bottom - caret_top).max(1);
        ctx.draw_rect(rect(text_origin.x + caret_x, caret_top, 1, caret_height), color);
    }
    ctx.pop_clip_rect();

    if needs_v && maxscroll_y > 0 && body.height > 0 {
        ctx.draw_frame(vscroll_base, ControlColor::ScrollBase);
        let thumb = scrollbar_thumb(ScrollAxis::Vertical, vscroll_base, body.height, cs.y, state.scroll.y, thumb_size);
        ctx.draw_frame(thumb, ControlColor::ScrollThumb);
    }

    if needs_h && maxscroll_x > 0 && body.width > 0 {
        ctx.draw_frame(hscroll_base, ControlColor::ScrollBase);
        let thumb = scrollbar_thumb(ScrollAxis::Horizontal, hscroll_base, body.width, cs.x, state.scroll.x, thumb_size);
        ctx.draw_frame(thumb, ControlColor::ScrollThumb);
    }

    res
}

implement_widget!(TextArea, handle_widget);
