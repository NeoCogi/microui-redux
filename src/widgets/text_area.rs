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
//! Multiline text-area widget state and editing behavior.
//!
//! Text areas share the UTF-8 editing core with textboxes but track line layout, vertical scroll,
//! and mouse-driven cursor placement across multiple wrapped lines.
use crate::*;
use super::WidgetConfig;
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis};
use crate::text_layout::{build_text_lines, TextLine};

use super::text_edit::{
    apply_text_input, caret_rect, clamp_cursor_boundary, clamp_scroll, cursor_from_x, cursor_x_in_line, font_line_metrics, line_index_for_cursor,
    FontLineMetrics, ReturnBehavior,
};

#[derive(Clone)]
/// Persistent state for multi-line text area widgets.
pub struct TextArea {
    /// Buffer edited by the text area.
    buf: String,
    /// Current cursor position within the buffer (byte index).
    cursor: usize,
    /// Scroll offset applied to the text view.
    scroll: Vec2i,
    /// Wrapping mode used when rendering the buffer.
    pub wrap: TextWrap,
    /// Shared widget configuration.
    pub config: WidgetConfig,
    /// Desired caret x position preserved while moving vertically.
    preferred_x: Option<i32>,
    /// Whether the vertical scrollbar thumb is being dragged.
    dragging_y: bool,
    /// Whether the horizontal scrollbar thumb is being dragged.
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
            config: WidgetConfig::new(WidgetOption::FRAME, ScrollBehavior::GRAB_SCROLL),
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
            config: WidgetConfig::new(opt, ScrollBehavior::GRAB_SCROLL),
            preferred_x: None,
            dragging_y: false,
            dragging_x: false,
        }
    }

    /// Returns the current text buffer.
    pub fn text(&self) -> &str {
        self.buf.as_str()
    }

    /// Replaces the current text and moves the cursor to the end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buf = text.into();
        self.cursor = self.buf.len();
        self.preferred_x = None;
    }

    /// Clears the current text and resets cursor/scroll state.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
        self.scroll = vec2(0, 0);
        self.preferred_x = None;
    }

    /// Returns the current cursor byte position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Moves the cursor to a valid UTF-8 boundary within the current text.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = clamp_cursor_boundary(&self.buf, cursor);
        self.preferred_x = None;
    }

    /// Moves the cursor to the end of the current text.
    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.buf.len();
        self.preferred_x = None;
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.scroll
    }

    /// Updates the current scroll offset, clamping negative offsets to zero.
    pub fn set_scroll(&mut self, scroll: Vec2i) {
        self.scroll = vec2(scroll.x.max(0), scroll.y.max(0));
    }

    /// Measures the text area content, respecting wrapping and available constraints.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let font = style.resolve_font_choice(self.config.font);
        let max_width = if self.wrap == TextWrap::Word && avail.width > 0 {
            (avail.width - padding * 2).max(1)
        } else {
            i32::MAX / 4
        };
        let lines = build_text_lines(self.buf.as_str(), self.wrap, max_width, font, atlas);
        let text_w = lines.iter().map(|line| line.width).max().unwrap_or(0);
        let line_count = (lines.len() as i32).max(1);
        let line_height = atlas.get_font_height(font) as i32;
        let mut width = text_w.saturating_add(padding * 2).max(0);
        let mut height = line_height.saturating_mul(line_count).saturating_add(padding * 2).max(0);
        if avail.width > 0 {
            width = width.min(avail.width.max(0));
        }
        if avail.height > 0 {
            height = height.min(avail.height.max(0));
        }
        Dimensioni::new(width, height)
    }

    /// Applies multiline editing, scrolling, and scrollbar dragging.
    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>, input: &[UiInputEvent]) -> ResourceState {
        let font = ctx.style().resolve_font_choice(self.config.font);
        textarea_update(ctx, input, self, font)
    }

    /// Paints the multiline editor and scrollbars.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.config.font);
        textarea_paint(ctx, self, font);
    }
}

/// Complete derived layout for one text-area frame.
struct TextAreaLayout {
    /// Outer text-area rectangle in screen coordinates.
    bounds: Recti,
    /// Text body after padding and scrollbars.
    body: Recti,
    /// Text body expressed in widget-local coordinates.
    body_local: Recti,
    /// Wrapped display lines.
    lines: Vec<TextLine>,
    /// Text content dimensions.
    content_size: Vec2i,
    /// Maximum vertical scroll offset.
    maxscroll_y: i32,
    /// Maximum horizontal scroll offset.
    maxscroll_x: i32,
    /// Whether a vertical scrollbar is needed.
    needs_v: bool,
    /// Whether a horizontal scrollbar is needed.
    needs_h: bool,
    /// Vertical scrollbar track rectangle.
    vscroll_base: Recti,
    /// Horizontal scrollbar track rectangle.
    hscroll_base: Recti,
    /// Style padding used by this layout.
    padding: i32,
    /// Scrollbar thumb size used by this layout.
    thumb_size: i32,
    /// Font metrics used for line positioning and caret drawing.
    metrics: FontLineMetrics,
}

/// Resolves wrapped lines, content size, scrollbar visibility, and scrollbar geometry.
fn textarea_layout(ctx: &WidgetCtx<'_>, state: &TextArea, font: FontId) -> TextAreaLayout {
    let bounds = ctx.screen_content_rect();
    let style = ctx.style();
    let padding = style.padding;
    let scrollbar_size = style.scrollbar_size;
    let thumb_size = style.thumb_size;
    let metrics = font_line_metrics(font, ctx.atlas());
    let line_height = metrics.line_height;

    let base_body = bounds;
    let mut body = base_body;
    let mut lines = Vec::new();
    let mut content_width = 0;
    let mut content_height = line_height.max(1);
    let mut needs_v = false;
    let mut needs_h = false;

    for _ in 0..3 {
        // Vertical and horizontal scrollbars can force each other to appear. Iterate a few times
        // until the body stabilizes without making the layout solver recursive.
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

    let content_size = vec2(content_width + padding * 2, content_height + padding * 2);
    let maxscroll_y = scrollbar_max_scroll(content_size.y, body.height);
    let maxscroll_x = scrollbar_max_scroll(content_size.x, body.width);
    let vscroll_base = if needs_v && maxscroll_y > 0 && body.height > 0 {
        scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size)
    } else {
        bounds
    };
    let hscroll_base = if needs_h && maxscroll_x > 0 && body.width > 0 {
        scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size)
    } else {
        bounds
    };
    let body_local = rect(body.x - bounds.x, body.y - bounds.y, body.width, body.height);

    TextAreaLayout {
        bounds,
        body,
        body_local,
        lines,
        content_size,
        maxscroll_y,
        maxscroll_x,
        needs_v,
        needs_h,
        vscroll_base,
        hscroll_base,
        padding,
        thumb_size,
        metrics,
    }
}

/// Updates text-area buffer, cursor, scroll position, and scrollbar drag state.
fn textarea_update(ctx: &mut WidgetCtx<'_>, input: &[UiInputEvent], state: &mut TextArea, font: FontId) -> ResourceState {
    let mut res = ResourceState::NONE;
    if !ctx.focused() {
        // Blurred text areas park the cursor at the end and forget vertical cursor preference.
        state.cursor = state.buf.len();
        state.preferred_x = None;
    }
    let mut cursor_pos = clamp_cursor_boundary(&state.buf, state.cursor);

    let mut ensure_visible = false;
    let mut reset_preferred = false;
    let mut vertical_moved = false;
    let mut preferred_x = state.preferred_x;

    if ctx.focused() {
        let text_input = input.text_input();
        let edit = apply_text_input(
            &mut state.buf,
            cursor_pos,
            text_input.as_str(),
            input.key_mods(),
            input.key_pressed(),
            input.key_code_pressed(),
            true,
            ReturnBehavior::Newline { submit_on_ctrl: true },
        );
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

    let layout = textarea_layout(ctx, state, font);
    let content_in_frame = ctx.frame_local_content_rect();
    let content_mouse_pos = input.mouse_pos() - Vec2i::new(content_in_frame.x, content_in_frame.y);

    if let Some(delta) = input.scroll_delta() {
        // Wheel/trackpad scrolling only affects axes that actually overflow.
        if layout.maxscroll_y > 0 {
            state.scroll.y += delta.y;
        }
        if layout.maxscroll_x > 0 {
            state.scroll.x += delta.x;
        }
    }

    if !input.mouse_down().intersects(MouseButton::LEFT) {
        state.dragging_y = false;
        state.dragging_x = false;
    }

    let mut clicked_scrollbar = false;

    if layout.needs_v && layout.maxscroll_y > 0 && layout.body.height > 0 {
        let vscroll_base_local = rect(
            layout.vscroll_base.x - layout.bounds.x,
            layout.vscroll_base.y - layout.bounds.y,
            layout.vscroll_base.width,
            layout.vscroll_base.height,
        );
        if input.mouse_pressed().intersects(MouseButton::LEFT) && vscroll_base_local.contains(&content_mouse_pos) {
            // Track scrollbar drag separately so text clicks do not also move the caret.
            state.dragging_y = true;
            clicked_scrollbar = true;
        }
        if state.dragging_y {
            state.scroll.y += scrollbar_drag_delta(ScrollAxis::Vertical, input.mouse_delta(), layout.content_size.y, layout.vscroll_base);
        }
    }

    if layout.needs_h && layout.maxscroll_x > 0 && layout.body.width > 0 {
        let hscroll_base_local = rect(
            layout.hscroll_base.x - layout.bounds.x,
            layout.hscroll_base.y - layout.bounds.y,
            layout.hscroll_base.width,
            layout.hscroll_base.height,
        );
        if input.mouse_pressed().intersects(MouseButton::LEFT) && hscroll_base_local.contains(&content_mouse_pos) {
            state.dragging_x = true;
            clicked_scrollbar = true;
        }
        if state.dragging_x {
            state.scroll.x += scrollbar_drag_delta(ScrollAxis::Horizontal, input.mouse_delta(), layout.content_size.x, layout.hscroll_base);
        }
    }

    let mut cursor_line = line_index_for_cursor(&layout.lines, cursor_pos);
    let mut caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    if ctx.focused() {
        if input.key_code_pressed().intersects(KeyCode::END) {
            cursor_pos = layout.lines[cursor_line].end;
            caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());
            ensure_visible = true;
            reset_preferred = true;
        }

        if input.key_code_pressed().intersects(KeyCode::UP) {
            // Vertical movement preserves preferred x so repeated Up/Down follows a visual column.
            let target_x = preferred_x.unwrap_or(caret_x);
            if cursor_line > 0 {
                cursor_line -= 1;
                cursor_pos = cursor_from_x(&layout.lines[cursor_line], state.buf.as_str(), target_x, font, ctx.atlas());
            }
            preferred_x = Some(target_x);
            ensure_visible = true;
            vertical_moved = true;
        }

        if input.key_code_pressed().intersects(KeyCode::DOWN) {
            // Vertical movement preserves preferred x so repeated Up/Down follows a visual column.
            let target_x = preferred_x.unwrap_or(caret_x);
            if cursor_line + 1 < layout.lines.len() {
                cursor_line += 1;
                cursor_pos = cursor_from_x(&layout.lines[cursor_line], state.buf.as_str(), target_x, font, ctx.atlas());
            }
            preferred_x = Some(target_x);
            ensure_visible = true;
            vertical_moved = true;
        }
    }

    if ctx.focused() && input.mouse_pressed().intersects(MouseButton::LEFT) && ctx.mouse_over(layout.bounds, input.mouse_pos()) && !clicked_scrollbar {
        // Convert a widget-local click to content-local coordinates before resolving cursor.
        let mouse_pos = content_mouse_pos;
        let local_x = mouse_pos.x - (layout.body_local.x + layout.padding) + state.scroll.x;
        let local_y = mouse_pos.y - (layout.body_local.y + layout.padding) + state.scroll.y;
        let line_idx = (local_y / layout.metrics.line_height).clamp(0, layout.lines.len().saturating_sub(1) as i32) as usize;
        cursor_pos = cursor_from_x(&layout.lines[line_idx], state.buf.as_str(), local_x, font, ctx.atlas());
        ensure_visible = true;
        reset_preferred = true;
    }

    cursor_pos = clamp_cursor_boundary(&state.buf, cursor_pos);
    cursor_line = line_index_for_cursor(&layout.lines, cursor_pos);
    caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    if reset_preferred && !vertical_moved {
        preferred_x = None;
    }
    if preferred_x.is_none() {
        preferred_x = Some(caret_x);
    }

    if ensure_visible && !state.dragging_x && !state.dragging_y {
        // Auto-scroll only when text editing moved the caret, not while the user drags scrollbars.
        let view_width = (layout.body.width - layout.padding * 2).max(0);
        let view_height = (layout.body.height - layout.padding * 2).max(0);
        let caret_y = cursor_line as i32 * layout.metrics.line_height;
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
            } else if caret_y + layout.metrics.line_height > state.scroll.y + view_height {
                state.scroll.y = caret_y + layout.metrics.line_height - view_height;
            }
        }
    }

    state.scroll.x = clamp_scroll(state.scroll.x, layout.maxscroll_x);
    state.scroll.y = clamp_scroll(state.scroll.y, layout.maxscroll_y);
    state.cursor = cursor_pos;
    state.preferred_x = preferred_x;
    res
}

/// Paints text-area frame, visible text lines, caret, and scrollbars.
fn textarea_paint(ctx: &mut WidgetCtx<'_>, state: &mut TextArea, font: FontId) {
    let layout = textarea_layout(ctx, state, font);
    let cursor_pos = clamp_cursor_boundary(&state.buf, state.cursor);
    let cursor_line = line_index_for_cursor(&layout.lines, cursor_pos);
    let caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    ctx.draw_widget_fill(layout.bounds, ControlColor::Base);

    let text_origin = vec2(layout.body.x + layout.padding - state.scroll.x, layout.body.y + layout.padding - state.scroll.y);
    let color = ctx.style().colors[ControlColor::Text as usize];
    let local_body = ctx.screen_to_local_rect(layout.body);
    let local_text_origin = ctx.screen_to_local_pos(text_origin);
    let local_caret = if ctx.focused() {
        let caret_line_top = text_origin.y + cursor_line as i32 * layout.metrics.line_height;
        let baseline_y = caret_line_top + layout.metrics.baseline;
        Some(ctx.screen_to_local_rect(caret_rect(text_origin.x + caret_x, baseline_y, layout.metrics, layout.body)))
    } else {
        None
    };
    let mut painter = ctx.painter();
    painter.with_clip(local_body, |painter| {
        for (idx, line) in layout.lines.iter().enumerate() {
            let line_top = text_origin.y + idx as i32 * layout.metrics.line_height;
            let line_bottom = line_top + layout.metrics.line_height;
            if line_bottom < layout.body.y || line_top > layout.body.y + layout.body.height {
                // Skip fully clipped lines before slicing/drawing text.
                continue;
            }
            let text = &state.buf[line.start..line.end];
            if !text.is_empty() {
                painter.text(
                    font,
                    text,
                    vec2(local_text_origin.x, local_text_origin.y + idx as i32 * layout.metrics.line_height),
                    color,
                );
            }
        }

        if let Some(caret) = local_caret {
            painter.fill_rect(caret, color);
        }
    });

    if layout.needs_v && layout.maxscroll_y > 0 && layout.body.height > 0 {
        ctx.draw_rect(layout.vscroll_base, ctx.style().colors[ControlColor::ScrollBase as usize]);
        let thumb = scrollbar_thumb(
            ScrollAxis::Vertical,
            layout.vscroll_base,
            layout.body.height,
            layout.content_size.y,
            state.scroll.y,
            layout.thumb_size,
        );
        ctx.draw_rect(thumb, ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }

    if layout.needs_h && layout.maxscroll_x > 0 && layout.body.width > 0 {
        ctx.draw_rect(layout.hscroll_base, ctx.style().colors[ControlColor::ScrollBase as usize]);
        let thumb = scrollbar_thumb(
            ScrollAxis::Horizontal,
            layout.hscroll_base,
            layout.body.width,
            layout.content_size.x,
            state.scroll.x,
            layout.thumb_size,
        );
        ctx.draw_rect(thumb, ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }
}

impl Widget for TextArea {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.config.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, input: Vec<UiInputEvent>) -> ResourceState {
        let old_buf = self.buf.clone();
        let old_cursor = self.cursor;
        let old_scroll = self.scroll;
        let old_preferred_x = self.preferred_x;
        let old_dragging_y = self.dragging_y;
        let old_dragging_x = self.dragging_x;
        let mut res = self.update_widget(ctx, &input);
        let scroll_changed = self.scroll.x != old_scroll.x || self.scroll.y != old_scroll.y;
        let changed = self.buf != old_buf
            || self.cursor != old_cursor
            || scroll_changed
            || self.preferred_x != old_preferred_x
            || self.dragging_y != old_dragging_y
            || self.dragging_x != old_dragging_x;
        if ctx.focused() || changed {
            res |= ResourceState::ACTIVE;
        }
        res
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        self.config.opt | WidgetOption::HOLD_FOCUS
    }

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::HoldUntilBlur
    }
}
