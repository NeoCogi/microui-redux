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


#[derive(Clone)]
/// Persistent state for textbox widgets.
pub struct Textbox {
    /// Buffer edited by the textbox.
    pub buf: String,
    /// Current cursor position within the buffer (byte index).
    pub cursor: usize,
    /// Widget options applied to the textbox.
    pub opt: WidgetOption,
    /// Behaviour options applied to the textbox.
    pub bopt: WidgetBehaviourOption,
}

impl Textbox {
    /// Creates a textbox with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self { buf, cursor, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a textbox with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self { buf, cursor, opt, bopt: WidgetBehaviourOption::NONE }
    }
}

fn insert_text(buf: &mut String, cursor: &mut usize, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let insert_at = (*cursor).min(buf.len());
    buf.insert_str(insert_at, text);
    *cursor = insert_at + text.len();
    true
}

fn delete_prev(buf: &mut String, cursor: &mut usize, allow_leading_newline: bool) -> bool {
    if buf.is_empty() {
        return false;
    }
    if *cursor == 0 {
        if allow_leading_newline && buf.as_bytes().first() == Some(&b'\n') {
            buf.replace_range(0..1, "");
            return true;
        }
        return false;
    }
    let mut start = (*cursor).min(buf.len());
    start -= 1;
    while start > 0 && !buf.is_char_boundary(start) {
        start -= 1;
    }
    buf.replace_range(start..*cursor, "");
    *cursor = start;
    true
}

fn delete_next(buf: &mut String, cursor: usize) -> bool {
    if buf.is_empty() || cursor >= buf.len() {
        return false;
    }
    let mut end = cursor + 1;
    while end < buf.len() && !buf.is_char_boundary(end) {
        end += 1;
    }
    buf.replace_range(cursor..end, "");
    true
}

fn move_left(buf: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut new_cursor = cursor - 1;
    while new_cursor > 0 && !buf.is_char_boundary(new_cursor) {
        new_cursor -= 1;
    }
    new_cursor
}

fn move_right(buf: &str, cursor: usize) -> usize {
    if cursor >= buf.len() {
        return buf.len();
    }
    let mut new_cursor = cursor + 1;
    while new_cursor < buf.len() && !buf.is_char_boundary(new_cursor) {
        new_cursor += 1;
    }
    new_cursor
}

pub(crate) fn textbox_handle(
    ctx: &mut WidgetCtx<'_>,
    control: &ControlState,
    buf: &mut String,
    cursor: &mut usize,
    opt: WidgetOption,
) -> ResourceState {
    let mut res = ResourceState::NONE;
    let r = ctx.rect();
    if !control.focused {
        *cursor = buf.len();
    }
    let mut cursor_pos = (*cursor).min(buf.len());

    let (mouse_pressed, mouse_pos, should_submit) = {
        let default_input = InputSnapshot::default();
        let input = ctx.input().unwrap_or(&default_input);
        let mut should_submit = false;

        if control.focused {
            if insert_text(buf, &mut cursor_pos, input.text_input.as_str()) {
                res |= ResourceState::CHANGE;
            }

            if input.key_pressed.is_backspace() && delete_prev(buf, &mut cursor_pos, false) {
                res |= ResourceState::CHANGE;
            }

            let delete_pressed = input.key_pressed.is_delete() || input.key_code_pressed.is_delete();
            if delete_pressed && delete_next(buf, cursor_pos) {
                res |= ResourceState::CHANGE;
            }

            if input.key_code_pressed.is_left() && cursor_pos > 0 {
                cursor_pos = move_left(buf, cursor_pos);
            }

            if input.key_code_pressed.is_right() && cursor_pos < buf.len() {
                cursor_pos = move_right(buf, cursor_pos);
            }

            if input.key_pressed.is_return() {
                should_submit = true;
            }
        }

        (input.mouse_pressed, input.mouse_pos, should_submit)
    };

    if should_submit {
        ctx.clear_focus();
        res |= ResourceState::SUBMIT;
    }

    ctx.draw_widget_frame(control, r, ControlColor::Base, opt);

    let font = ctx.style().font;
    let line_height = ctx.atlas().get_font_height(font) as i32;
    let baseline = ctx.atlas().get_font_baseline(font);
    let descent = (line_height - baseline).max(0);

    let mut texty = r.y + r.height / 2 - line_height / 2;
    if texty < r.y {
        texty = r.y;
    }
    let max_texty = (r.y + r.height - line_height).max(r.y);
    if texty > max_texty {
        texty = max_texty;
    }
    let baseline_y = texty + line_height - descent;

    let text_metrics = ctx.atlas().get_text_size(font, buf.as_str());
    let padding = ctx.style().padding;
    let ofx = r.width - padding - text_metrics.width - 1;
    let textx = r.x + if ofx < padding { ofx } else { padding };

    if control.focused && mouse_pressed.is_left() && ctx.mouse_over(r) {
        let click_x = mouse_pos.x - textx;
        if click_x <= 0 {
            cursor_pos = 0;
        } else {
            let mut last_width = 0;
            let mut new_cursor = buf.len();
            for (idx, ch) in buf.char_indices() {
                let next = idx + ch.len_utf8();
                let width = ctx.atlas().get_text_size(font, &buf[..next]).width;
                if click_x < width {
                    if click_x < (last_width + width) / 2 {
                        new_cursor = idx;
                    } else {
                        new_cursor = next;
                    }
                    break;
                }
                last_width = width;
            }
            cursor_pos = new_cursor.min(buf.len());
        }
    }

    cursor_pos = cursor_pos.min(buf.len());
    *cursor = cursor_pos;

    let caret_offset = if cursor_pos == 0 {
        0
    } else {
        ctx.atlas().get_text_size(font, &buf[..cursor_pos]).width
    };

    if control.focused {
        let color = ctx.style().colors[ControlColor::Text as usize];
        ctx.push_clip_rect(r);
        ctx.draw_text(font, buf.as_str(), vec2(textx, texty), color);
        let caret_top = (baseline_y - baseline + 2).max(r.y).min(r.y + r.height);
        let caret_bottom = (baseline_y + descent - 2).max(r.y).min(r.y + r.height);
        let caret_height = (caret_bottom - caret_top).max(1);
        ctx.draw_rect(rect(textx + caret_offset, caret_top, 1, caret_height), color);
        ctx.pop_clip_rect();
    } else {
        ctx.draw_control_text(buf.as_str(), r, ControlColor::Text, opt);
    }
    res
}

impl Widget for Textbox {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        textbox_handle(ctx, control, &mut self.buf, &mut self.cursor, self.opt)
    }
}

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
        }
    }
}

#[derive(Clone, Copy)]
struct TextLine {
    start: usize,
    end: usize,
    width: i32,
}

fn push_wrapped_line(
    lines: &mut Vec<TextLine>,
    buf: &str,
    line_start: usize,
    line_end: usize,
    wrap: TextWrap,
    max_width: i32,
    font: FontId,
    atlas: &AtlasHandle,
) {
    let line = &buf[line_start..line_end];
    if line.is_empty() {
        lines.push(TextLine {
            start: line_start,
            end: line_start,
            width: 0,
        });
        return;
    }

    if wrap != TextWrap::Word || max_width <= 0 {
        let width = atlas.get_text_size(font, line).width;
        lines.push(TextLine {
            start: line_start,
            end: line_end,
            width,
        });
        return;
    }

    let mut offset = 0;
    let mut seg_start = 0;
    let mut seg_width = 0;
    for word in line.split_inclusive(' ') {
        let word_len = word.len();
        let word_width = atlas.get_text_size(font, word).width;
        if seg_width > 0 && seg_width + word_width > max_width {
            let seg_end = offset;
            lines.push(TextLine {
                start: line_start + seg_start,
                end: line_start + seg_end,
                width: seg_width,
            });
            seg_start = offset;
            seg_width = 0;
        }
        seg_width += word_width;
        offset += word_len;
    }

    lines.push(TextLine {
        start: line_start + seg_start,
        end: line_start + line.len(),
        width: seg_width,
    });
}

fn build_text_lines(buf: &str, wrap: TextWrap, max_width: i32, font: FontId, atlas: &AtlasHandle) -> Vec<TextLine> {
    let mut lines = Vec::new();
    if buf.is_empty() {
        lines.push(TextLine {
            start: 0,
            end: 0,
            width: 0,
        });
        return lines;
    }

    let mut line_start = 0;
    for (idx, ch) in buf.char_indices() {
        if ch == '\n' {
            push_wrapped_line(&mut lines, buf, line_start, idx, wrap, max_width, font, atlas);
            line_start = idx + ch.len_utf8();
        }
    }

    if line_start <= buf.len() {
        push_wrapped_line(&mut lines, buf, line_start, buf.len(), wrap, max_width, font, atlas);
    }

    if lines.is_empty() {
        lines.push(TextLine {
            start: 0,
            end: 0,
            width: 0,
        });
    }
    lines
}

fn line_index_for_cursor(lines: &[TextLine], cursor: usize) -> usize {
    for (idx, line) in lines.iter().enumerate() {
        if cursor <= line.end {
            return idx;
        }
    }
    lines.len().saturating_sub(1)
}

fn cursor_x_in_line(line: &TextLine, buf: &str, cursor: usize, font: FontId, atlas: &AtlasHandle) -> i32 {
    let end = cursor.min(line.end).max(line.start);
    if end <= line.start {
        0
    } else {
        atlas.get_text_size(font, &buf[line.start..end]).width
    }
}

fn cursor_from_x(line: &TextLine, buf: &str, target_x: i32, font: FontId, atlas: &AtlasHandle) -> usize {
    if target_x <= 0 {
        return line.start;
    }
    let slice = &buf[line.start..line.end];
    let mut last_width = 0;
    for (idx, ch) in slice.char_indices() {
        let next = idx + ch.len_utf8();
        let width = atlas.get_text_size(font, &slice[..next]).width;
        if target_x < width {
            if target_x < (last_width + width) / 2 {
                return line.start + idx;
            }
            return line.start + next;
        }
        last_width = width;
    }
    line.end
}

fn clamp_scroll(value: i32, max_value: i32) -> i32 {
    if max_value <= 0 {
        0
    } else {
        value.clamp(0, max_value)
    }
}

fn textarea_handle(ctx: &mut WidgetCtx<'_>, control: &ControlState, state: &mut TextArea) -> ResourceState {
    let mut res = ResourceState::NONE;
    let bounds = ctx.rect();
    if !control.focused {
        state.cursor = state.buf.len();
        state.preferred_x = None;
    }
    let mut cursor_pos = state.cursor.min(state.buf.len());

    let default_input = InputSnapshot::default();
    let input = ctx.input().unwrap_or(&default_input);
    let mut ensure_visible = false;
    let mut reset_preferred = false;
    let mut vertical_moved = false;
    let mut preferred_x = state.preferred_x;

    if control.focused {
        if insert_text(&mut state.buf, &mut cursor_pos, input.text_input.as_str()) {
            res |= ResourceState::CHANGE;
            ensure_visible = true;
            reset_preferred = true;
        }

        if input.key_pressed.is_backspace() && delete_prev(&mut state.buf, &mut cursor_pos, true) {
            res |= ResourceState::CHANGE;
            ensure_visible = true;
            reset_preferred = true;
        }

        let delete_pressed = input.key_pressed.is_delete() || input.key_code_pressed.is_delete();
        if delete_pressed && delete_next(&mut state.buf, cursor_pos) {
            res |= ResourceState::CHANGE;
            ensure_visible = true;
            reset_preferred = true;
        }

        if input.key_code_pressed.is_left() && cursor_pos > 0 {
            cursor_pos = move_left(&state.buf, cursor_pos);
            ensure_visible = true;
            reset_preferred = true;
        }

        if input.key_code_pressed.is_right() && cursor_pos < state.buf.len() {
            cursor_pos = move_right(&state.buf, cursor_pos);
            ensure_visible = true;
            reset_preferred = true;
        }

        if input.key_pressed.is_return() {
            if input.key_mods.is_ctrl() {
                res |= ResourceState::SUBMIT;
            } else {
                if insert_text(&mut state.buf, &mut cursor_pos, "\n") {
                    res |= ResourceState::CHANGE;
                    ensure_visible = true;
                    reset_preferred = true;
                }
            }
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
        lines.push(TextLine {
            start: 0,
            end: 0,
            width: 0,
        });
    }

    let cs = vec2(content_width + padding * 2, content_height + padding * 2);
    let maxscroll_y = (cs.y - body.height).max(0);
    let maxscroll_x = (cs.x - body.width).max(0);

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
        vscroll_base = body;
        vscroll_base.x = body.x + body.width;
        vscroll_base.width = scrollbar_size;
        if input.mouse_pressed.is_left() && vscroll_base.contains(&input.mouse_pos) {
            state.dragging_y = true;
            clicked_scrollbar = true;
        }
        if state.dragging_y && vscroll_base.height > 0 {
            state.scroll.y += input.mouse_delta.y * cs.y / vscroll_base.height;
        }
    }

    if needs_h && maxscroll_x > 0 && body.width > 0 {
        hscroll_base = body;
        hscroll_base.y = body.y + body.height;
        hscroll_base.height = scrollbar_size;
        if input.mouse_pressed.is_left() && hscroll_base.contains(&input.mouse_pos) {
            state.dragging_x = true;
            clicked_scrollbar = true;
        }
        if state.dragging_x && hscroll_base.width > 0 {
            state.scroll.x += input.mouse_delta.x * cs.x / hscroll_base.width;
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
        let mut thumb = vscroll_base;
        thumb.height = if thumb_size > vscroll_base.height * body.height / cs.y {
            thumb_size
        } else {
            vscroll_base.height * body.height / cs.y
        };
        thumb.y += state.scroll.y * (vscroll_base.height - thumb.height) / maxscroll_y;
        ctx.draw_frame(thumb, ControlColor::ScrollThumb);
    }

    if needs_h && maxscroll_x > 0 && body.width > 0 {
        ctx.draw_frame(hscroll_base, ControlColor::ScrollBase);
        let mut thumb = hscroll_base;
        thumb.width = if thumb_size > hscroll_base.width * body.width / cs.x {
            thumb_size
        } else {
            hscroll_base.width * body.width / cs.x
        };
        thumb.x += state.scroll.x * (hscroll_base.width - thumb.width) / maxscroll_x;
        ctx.draw_frame(thumb, ControlColor::ScrollThumb);
    }

    res
}

impl Widget for TextArea {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        textarea_handle(ctx, control, self)
    }
}
