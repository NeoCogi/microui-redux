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
use crate::ui_node::scrollbar::{ScrollAxis, ScrollbarGeometry, scrollbar_base, scrollbar_max_scroll};
use crate::ui_node::{runtime_read_state, runtime_update_state};
use crate::*;
use std::{cell::RefCell, rc::Rc};
use crate::ui_node::text_layout::{TextLine, build_text_lines};

use super::text_edit::{
    apply_text_input, caret_rect, clamp_cursor_boundary, clamp_scroll, cursor_from_x, cursor_x_in_line, font_line_metrics, line_index_for_cursor,
    FontLineMetrics, ReturnBehavior,
};

/// One-shot construction input for a [`TextArea`].
pub struct TextAreaParameters {
    /// Initial buffer edited by the text area.
    buf: String,
    /// Wrapping mode used when rendering the buffer.
    pub wrap: TextWrap,
    /// Font used for measurement, editing, and paint.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for TextAreaParameters {}

impl TextAreaParameters {
    /// Creates text-area parameters with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        Self {
            buf: buf.into(),
            wrap: TextWrap::None,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::FRAME | WidgetOption::GRAB_SCROLL,
        }
    }

    /// Creates text-area parameters with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            buf: buf.into(),
            wrap: TextWrap::None,
            font: FontChoice::Role(FontRole::Body),
            opt: opt | WidgetOption::GRAB_SCROLL,
        }
    }

    /// Replaces the wrapping mode used by the text area.
    pub const fn wrap(mut self, wrap: TextWrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Replaces the font used by the text area.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent text-area state.
pub struct TextAreaState {
    /// Current text buffer.
    buf: String,
    /// Current UTF-8 byte cursor.
    cursor: usize,
    /// Current scroll offset.
    scroll: Vec2i,
    /// Requests a runtime-only vertical-cursor preference reset.
    reset_preferred_x: bool,
    /// User text changes waiting to be consumed.
    pending_changes: u32,
    /// User submissions waiting to be consumed.
    pending_submissions: u32,
}

impl WidgetState for TextAreaState {}

impl TextAreaState {
    /// Returns the current text buffer.
    pub fn text(&self) -> &str {
        self.buf.as_str()
    }

    /// Replaces the current text and moves the cursor to the end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buf = text.into();
        self.cursor = self.buf.len();
        self.reset_preferred_x = true;
    }

    /// Clears the current text and resets cursor/scroll state.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
        self.scroll = vec2(0, 0);
        self.reset_preferred_x = true;
    }

    /// Returns the current cursor byte position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Moves the cursor to a valid UTF-8 boundary within the current text.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = clamp_cursor_boundary(&self.buf, cursor);
        self.reset_preferred_x = true;
    }

    /// Moves the cursor to the end of the current text.
    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.buf.len();
        self.reset_preferred_x = true;
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.scroll
    }

    /// Updates the current scroll offset, clamping negative offsets to zero.
    pub fn set_scroll(&mut self, scroll: Vec2i) {
        self.scroll = vec2(scroll.x.max(0), scroll.y.max(0));
    }

    /// Consumes one pending user text change.
    pub fn take_changed(&mut self) -> bool {
        crate::widgets::take_pending_event(&mut self.pending_changes)
    }

    /// Consumes one pending user submission.
    pub fn take_submitted(&mut self) -> bool {
        crate::widgets::take_pending_event(&mut self.pending_submissions)
    }
}

/// Runtime-only text-area editing state.
#[derive(Default)]
struct TextAreaInteraction {
    /// Desired caret x position preserved while moving vertically.
    preferred_x: Option<i32>,
    /// Whether the vertical scrollbar thumb is being dragged.
    dragging_y: bool,
    /// Whether the horizontal scrollbar thumb is being dragged.
    dragging_x: bool,
}

/// Concrete text-area runtime and sole strong owner of its application state.
pub struct TextArea {
    /// Initialization-only wrapping mode.
    wrap: TextWrap,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Runtime-only derived editing state.
    interaction: TextAreaInteraction,
    /// Persistent state allocation.
    state: Rc<RefCell<TextAreaState>>,
}

impl TextArea {
    /// Constructs a typed state handle and unique text-area runtime.
    pub fn create(parameters: TextAreaParameters) -> (WidgetStateHandle<TextAreaState>, Self) {
        let widget = TextAreaBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures the text area content, respecting wrapping and available constraints.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let font = style.resolve_font_choice(self.font);
        let max_width = if self.wrap == TextWrap::Word && avail.width > 0 {
            (avail.width - padding * 2).max(1)
        } else {
            i32::MAX / 4
        };
        runtime_read_state(&self.state, "TextArea::measure", |state| {
            let lines = build_text_lines(state.buf.as_str(), self.wrap, max_width, font, atlas);
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
        })
    }

    /// Applies multiline editing, scrolling, and scrollbar dragging.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let font = ctx.style().resolve_font_choice(self.font);
        let old_preferred_x = self.interaction.preferred_x;
        let old_dragging_y = self.interaction.dragging_y;
        let old_dragging_x = self.interaction.dragging_x;
        runtime_update_state(&self.state, "TextArea::update", |state| {
            let old_buf = state.buf.clone();
            let old_cursor = state.cursor;
            let old_scroll = state.scroll;
            if state.reset_preferred_x {
                self.interaction.preferred_x = None;
                state.reset_preferred_x = false;
            }
            let outcome = textarea_update(ctx, input, state, &mut self.interaction, self.wrap, font);
            if outcome.changed {
                crate::widgets::record_pending_event(&mut state.pending_changes);
            }
            if outcome.submitted {
                crate::widgets::record_pending_event(&mut state.pending_submissions);
            }
            let changed = state.buf != old_buf
                || state.cursor != old_cursor
                || state.scroll.x != old_scroll.x
                || state.scroll.y != old_scroll.y
                || self.interaction.preferred_x != old_preferred_x
                || self.interaction.dragging_y != old_dragging_y
                || self.interaction.dragging_x != old_dragging_x;
            let _ = (ctx.focused(), changed);
        })
    }

    /// Paints the multiline editor and scrollbars.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_read_state(&self.state, "TextArea::paint", |state| {
            textarea_paint(ctx, state, self.wrap, font);
        });
    }
}

/// Complete derived layout for one text-area frame.
struct TextAreaLayout {
    /// Outer text-area rectangle in widget-local coordinates.
    bounds: Recti,
    /// Text body after padding and scrollbars.
    body: Recti,
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
///
/// Raw read-only inputs keep this calculation phase-neutral: update and paint derive the same
/// geometry without either context borrowing capabilities from the other.
fn textarea_layout(content_rect: Recti, style: &Style, atlas: &AtlasHandle, state: &TextAreaState, wrap: TextWrap, font: FontId) -> TextAreaLayout {
    let bounds = content_rect;
    let padding = style.padding;
    let scrollbar_size = style.scrollbar_size;
    let thumb_size = style.thumb_size;
    let metrics = font_line_metrics(font, atlas);
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
        lines = build_text_lines(state.buf.as_str(), wrap, available_width, font, atlas);
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
    TextAreaLayout {
        bounds,
        body,
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
fn textarea_update(
    ctx: &mut WidgetUpdateCtx<'_>,
    input: Option<&UiInputEvent>,
    state: &mut TextAreaState,
    interaction: &mut TextAreaInteraction,
    wrap: TextWrap,
    font: FontId,
) -> TextAreaUpdateOutcome {
    let mut outcome = TextAreaUpdateOutcome::default();
    if !ctx.focused() {
        // Blurred text areas park the cursor at the end and forget vertical cursor preference.
        state.cursor = state.buf.len();
        interaction.preferred_x = None;
    }
    let mut cursor_pos = clamp_cursor_boundary(&state.buf, state.cursor);

    let mut ensure_visible = false;
    let mut reset_preferred = false;
    let mut vertical_moved = false;
    let mut preferred_x = interaction.preferred_x;
    let text_input = match input {
        Some(UiInputEvent::Text { text }) => text.as_str(),
        _ => "",
    };
    let key_pressed = match input {
        Some(UiInputEvent::KeyDown { key }) => *key,
        _ => KeyMode::NONE,
    };
    let key_code_pressed = match input {
        Some(UiInputEvent::KeyCodeDown { code }) => *code,
        _ => KeyCode::NONE,
    };
    let mouse_pressed = match input {
        Some(UiInputEvent::MouseDown { button, .. }) => *button,
        _ => MouseButton::NONE,
    };
    let content_mouse_pos = match input {
        Some(
            UiInputEvent::MouseMove { pos, .. }
            | UiInputEvent::MouseDrag { pos, .. }
            | UiInputEvent::MouseDown { pos, .. }
            | UiInputEvent::MouseUp { pos, .. }
            | UiInputEvent::Scroll { pos, .. },
        ) => *pos,
        _ => Vec2i::default(),
    };
    let mouse_delta = match input {
        Some(UiInputEvent::MouseMove { delta, .. } | UiInputEvent::MouseDrag { delta, .. }) => *delta,
        _ => Vec2i::default(),
    };

    if ctx.focused() {
        let edit = apply_text_input(
            &mut state.buf,
            cursor_pos,
            text_input,
            ctx.key_modes(),
            key_pressed,
            key_code_pressed,
            true,
            ReturnBehavior::Newline { submit_on_ctrl: true },
        );
        cursor_pos = edit.cursor;
        if edit.changed {
            outcome.changed = true;
            ensure_visible = true;
            reset_preferred = true;
        }
        if edit.moved {
            ensure_visible = true;
            reset_preferred = true;
        }
        if edit.submit {
            outcome.submitted = true;
        }
    }

    let layout = textarea_layout(ctx.local_rect(), ctx.style(), ctx.atlas(), state, wrap, font);
    if let Some(UiInputEvent::Scroll { delta, .. }) = input {
        // Wheel/trackpad scrolling only affects axes that actually overflow.
        if layout.maxscroll_y > 0 {
            state.scroll.y += delta.y;
        }
        if layout.maxscroll_x > 0 {
            state.scroll.x += delta.x;
        }
    }

    if !ctx.mouse_buttons().intersects(MouseButton::LEFT) {
        interaction.dragging_y = false;
        interaction.dragging_x = false;
    }

    let mut clicked_scrollbar = false;

    if layout.needs_v && layout.maxscroll_y > 0 && layout.body.height > 0 {
        if mouse_pressed.intersects(MouseButton::LEFT) && layout.vscroll_base.contains(&content_mouse_pos) {
            // Track scrollbar drag separately so text clicks do not also move the caret.
            interaction.dragging_y = true;
            clicked_scrollbar = true;
        }
        if interaction.dragging_y {
            let scrollbar = ScrollbarGeometry::new(
                ScrollAxis::Vertical,
                layout.vscroll_base,
                layout.body.height,
                layout.content_size.y,
                state.scroll.y,
                layout.thumb_size,
            );
            state.scroll.y += scrollbar.drag_delta(mouse_delta);
        }
    }

    if layout.needs_h && layout.maxscroll_x > 0 && layout.body.width > 0 {
        if mouse_pressed.intersects(MouseButton::LEFT) && layout.hscroll_base.contains(&content_mouse_pos) {
            interaction.dragging_x = true;
            clicked_scrollbar = true;
        }
        if interaction.dragging_x {
            let scrollbar = ScrollbarGeometry::new(
                ScrollAxis::Horizontal,
                layout.hscroll_base,
                layout.body.width,
                layout.content_size.x,
                state.scroll.x,
                layout.thumb_size,
            );
            state.scroll.x += scrollbar.drag_delta(mouse_delta);
        }
    }

    let mut cursor_line = line_index_for_cursor(&layout.lines, cursor_pos);
    let mut caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    if ctx.focused() {
        if key_code_pressed.intersects(KeyCode::END) {
            cursor_pos = layout.lines[cursor_line].end;
            caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());
            ensure_visible = true;
            reset_preferred = true;
        }

        if key_code_pressed.intersects(KeyCode::UP) {
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

        if key_code_pressed.intersects(KeyCode::DOWN) {
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

    if ctx.focused() && mouse_pressed.intersects(MouseButton::LEFT) && ctx.mouse_over(layout.bounds, content_mouse_pos) && !clicked_scrollbar {
        // Convert a widget-local click to content-local coordinates before resolving cursor.
        let mouse_pos = content_mouse_pos;
        let local_x = mouse_pos.x - (layout.body.x + layout.padding) + state.scroll.x;
        let local_y = mouse_pos.y - (layout.body.y + layout.padding) + state.scroll.y;
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

    if ensure_visible && !interaction.dragging_x && !interaction.dragging_y {
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
    interaction.preferred_x = preferred_x;
    outcome
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct TextAreaUpdateOutcome {
    changed: bool,
    submitted: bool,
}

/// Paints text-area frame, visible text lines, caret, and scrollbars.
fn textarea_paint(ctx: &mut WidgetPaintCtx<'_>, state: &TextAreaState, wrap: TextWrap, font: FontId) {
    let layout = textarea_layout(ctx.local_rect(), ctx.style(), ctx.atlas(), state, wrap, font);
    let cursor_pos = clamp_cursor_boundary(&state.buf, state.cursor);
    let cursor_line = line_index_for_cursor(&layout.lines, cursor_pos);
    let caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    ctx.draw_widget_fill(layout.bounds, ControlColor::Base);

    let text_origin = vec2(layout.body.x + layout.padding - state.scroll.x, layout.body.y + layout.padding - state.scroll.y);
    let color = ctx.style().colors[ControlColor::Text as usize];
    let caret = if ctx.focused() {
        let caret_line_top = text_origin.y + cursor_line as i32 * layout.metrics.line_height;
        let baseline_y = caret_line_top + layout.metrics.baseline;
        Some(caret_rect(text_origin.x + caret_x, baseline_y, layout.metrics, layout.body))
    } else {
        None
    };
    let mut painter = ctx.painter();
    painter.with_clip(layout.body, |painter| {
        for (idx, line) in layout.lines.iter().enumerate() {
            let line_top = text_origin.y + idx as i32 * layout.metrics.line_height;
            let line_bottom = line_top + layout.metrics.line_height;
            if line_bottom < layout.body.y || line_top > layout.body.y + layout.body.height {
                // Skip fully clipped lines before slicing/drawing text.
                continue;
            }
            let text = &state.buf[line.start..line.end];
            if !text.is_empty() {
                painter.text(font, text, vec2(text_origin.x, text_origin.y + idx as i32 * layout.metrics.line_height), color);
            }
        }

        if let Some(caret) = caret {
            painter.fill_rect(caret, color);
        }
    });

    if layout.needs_v && layout.maxscroll_y > 0 && layout.body.height > 0 {
        ctx.draw_rect(layout.vscroll_base, ctx.style().colors[ControlColor::ScrollBase as usize]);
        let thumb = ScrollbarGeometry::new(
            ScrollAxis::Vertical,
            layout.vscroll_base,
            layout.body.height,
            layout.content_size.y,
            state.scroll.y,
            layout.thumb_size,
        )
        .thumb();
        ctx.draw_rect(thumb, ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }

    if layout.needs_h && layout.maxscroll_x > 0 && layout.body.width > 0 {
        ctx.draw_rect(layout.hscroll_base, ctx.style().colors[ControlColor::ScrollBase as usize]);
        let thumb = ScrollbarGeometry::new(
            ScrollAxis::Horizontal,
            layout.hscroll_base,
            layout.body.width,
            layout.content_size.x,
            state.scroll.x,
            layout.thumb_size,
        )
        .thumb();
        ctx.draw_rect(thumb, ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }
}

impl Widget for TextArea {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.update_widget(ctx, input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        self.opt | WidgetOption::HOLD_FOCUS
    }

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::HoldUntilBlur
    }
}

impl WidgetStateOwner for TextArea {
    type State = TextAreaState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating text-area parameters with the concrete runtime.
pub struct TextAreaBuilder;

impl WidgetBuilder for TextAreaBuilder {
    type Parameters = TextAreaParameters;
    type W = TextArea;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        let cursor = parameters.buf.len();
        TextArea {
            wrap: parameters.wrap,
            font: parameters.font,
            opt: parameters.opt,
            interaction: TextAreaInteraction::default(),
            state: Rc::new(RefCell::new(TextAreaState {
                buf: parameters.buf,
                cursor,
                scroll: vec2(0, 0),
                reset_preferred_x: false,
                pending_changes: 0,
                pending_submissions: 0,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;

    fn update_text_area(text_area: &mut TextArea, input: Vec<UiInputEvent>) {
        let atlas = test_atlas();
        let style = Style::default();
        let bounds = rect(0, 0, 160, 80);
        let mut keys = KeyMode::NONE;
        let mut codes = KeyCode::NONE;
        for event in &input {
            match event {
                UiInputEvent::KeyDown { key } => keys |= *key,
                UiInputEvent::KeyUp { key } => keys &= !*key,
                UiInputEvent::KeyCodeDown { code } => codes |= *code,
                UiInputEvent::KeyCodeUp { code } => codes &= !*code,
                _ => {}
            }
            let mut ctx = WidgetUpdateCtx::new_with_interaction(bounds, bounds, &style, &atlas, true, true, true, false, false, MouseButton::NONE, keys, codes);
            text_area.update(&mut ctx, Some(event));
        }
    }

    #[test]
    fn text_area_records_independent_change_and_submission_events() {
        let (state, mut text_area) = TextArea::create(TextAreaParameters::new(""));
        update_text_area(
            &mut text_area,
            vec![
                UiInputEvent::Text { text: "line".into() },
                UiInputEvent::KeyDown { key: KeyMode::CTRL },
                UiInputEvent::KeyDown { key: KeyMode::RETURN },
            ],
        );
        assert_eq!(state.try_update(TextAreaState::take_changed), Some(true));
        assert_eq!(state.try_update(TextAreaState::take_changed), Some(false));
        assert_eq!(state.try_update(TextAreaState::take_submitted), Some(true));
        assert_eq!(state.try_update(TextAreaState::take_submitted), Some(false));
    }

    #[test]
    fn programmatic_text_cursor_and_scroll_setters_are_silent() {
        let (state, _text_area) = TextArea::create(TextAreaParameters::new("initial"));
        state
            .try_update(|state| {
                state.set_text("replacement");
                state.set_cursor(3);
                state.move_cursor_to_end();
                state.set_scroll(vec2(5, 7));
                state.clear();
            })
            .unwrap();
        assert_eq!(state.try_update(TextAreaState::take_changed), Some(false));
        assert_eq!(state.try_update(TextAreaState::take_submitted), Some(false));
    }
}
