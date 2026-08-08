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
//! Single-line textbox widget and shared textbox update/paint helpers.
//!
//! The textbox stores a UTF-8 byte cursor and uses shared text-edit helpers to keep cursor movement
//! and deletion on valid character boundaries.
use crate::*;
use crate::ui_node::{runtime_read_state, runtime_update_state};
use std::{cell::RefCell, rc::Rc};

use super::text_edit::{apply_text_input, caret_rect, centered_line_top, clamp_cursor_boundary, cursor_from_text_x, font_line_metrics, ReturnBehavior};

/// One-shot construction input for a [`Textbox`].
pub struct TextboxParameters {
    /// Initial buffer edited by the textbox.
    buf: String,
    /// Font used for measurement, editing, and paint.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for TextboxParameters {}

impl TextboxParameters {
    /// Creates textbox parameters with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        Self {
            buf: buf.into(),
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates textbox parameters with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            buf: buf.into(),
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used by the textbox.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent textbox state.
pub struct TextboxState {
    /// Current text buffer.
    buf: String,
    /// Current UTF-8 byte cursor.
    cursor: usize,
    /// User text changes waiting to be consumed.
    pending_changes: u32,
    /// User submissions waiting to be consumed.
    pending_submissions: u32,
    /// Session connection for user-originated text changes.
    changed_event: crate::event::WidgetEventPort<TextboxChanged>,
    /// Session connection for user submissions.
    submitted_event: crate::event::WidgetEventPort<TextboxSubmitted>,
}

impl WidgetState for TextboxState {}

impl TextboxState {
    /// Returns the current text buffer.
    pub fn text(&self) -> &str {
        self.buf.as_str()
    }

    /// Replaces the current text and moves the cursor to the end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buf = text.into();
        self.cursor = self.buf.len();
    }

    /// Clears the current text and resets the cursor.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    /// Returns the current cursor byte position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Moves the cursor to a valid UTF-8 boundary within the current text.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = clamp_cursor_boundary(&self.buf, cursor);
    }

    /// Moves the cursor to the end of the current text.
    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.buf.len();
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

/// Snapshot emitted after a user-originated textbox value change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextboxChanged {
    /// Complete text value after applying the triggering input event.
    pub text: String,
    /// UTF-8 byte cursor after applying the triggering input event.
    pub cursor: usize,
}

/// Snapshot emitted when the user submits a textbox value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextboxSubmitted {
    /// Complete text value at submission time.
    pub text: String,
}

impl WidgetStateHandle<TextboxState> {
    /// Returns the native event endpoint emitted after every user-originated text change.
    pub fn changed(&self) -> crate::WidgetEvent<TextboxState, TextboxChanged> {
        crate::WidgetEvent::new(self.clone(), |state| &mut state.changed_event)
    }

    /// Returns the native event endpoint emitted whenever the user submits the current text.
    pub fn submitted(&self) -> crate::WidgetEvent<TextboxState, TextboxSubmitted> {
        crate::WidgetEvent::new(self.clone(), |state| &mut state.submitted_event)
    }
}

/// Concrete textbox runtime and sole strong owner of its application state.
pub struct Textbox {
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<TextboxState>>,
}

impl Textbox {
    /// Constructs a typed state handle and unique textbox runtime.
    pub fn create(parameters: TextboxParameters) -> (WidgetStateHandle<TextboxState>, Self) {
        let widget = TextboxBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures a single-line editor, bounded by available width when supplied.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        runtime_read_state(&self.state, "Textbox::measure", |state| {
            let font = style.resolve_font_choice(self.font);
            let font_height = atlas.get_font_height(font) as i32;
            let text_w = if state.buf.is_empty() {
                0
            } else {
                atlas.get_text_size(font, state.buf.as_str()).width
            };
            let mut width = (text_w + padding * 2 + 1).max(0);
            if avail.width > 0 {
                width = width.min(avail.width.max(0));
            }
            let height = (font_height + vertical_pad * 2).max(0);
            Dimensioni::new(width, height)
        })
    }

    /// Applies input and cursor movement for this textbox.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_update_state(&self.state, "Textbox::update", |state| {
            let outcome = textbox_update(ctx, input, &mut state.buf, &mut state.cursor, self.opt, font);
            if outcome.changed {
                crate::widgets::record_pending_event(&mut state.pending_changes);
                state.changed_event.emit(TextboxChanged {
                    text: state.buf.clone(),
                    cursor: state.cursor,
                });
            }
            if outcome.submitted {
                crate::widgets::record_pending_event(&mut state.pending_submissions);
                state.submitted_event.emit(TextboxSubmitted { text: state.buf.clone() });
            }
        });
    }

    /// Paints the textbox frame, text, and caret.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_read_state(&self.state, "Textbox::paint", |state| {
            textbox_paint(ctx, state.buf.as_str(), state.cursor, self.opt, font);
        });
    }
}

/// Shared single-line text editing update used by textbox and numeric inline editors.
pub(crate) fn textbox_update(
    ctx: &mut WidgetUpdateCtx<'_>,
    input: Option<&UiInputEvent>,
    buf: &mut String,
    cursor: &mut usize,
    _opt: WidgetOption,
    font: FontId,
) -> TextboxUpdateOutcome {
    let mut outcome = TextboxUpdateOutcome::default();
    let r = ctx.local_rect();
    if !ctx.focused() {
        // Reset to end when blurred so refocusing starts from a predictable position.
        *cursor = buf.len();
    }
    let mut cursor_pos = clamp_cursor_boundary(buf, *cursor);

    let mouse_pressed = match input {
        Some(UiInputEvent::MouseDown { button, .. }) => *button,
        _ => MouseButton::NONE,
    };
    let mouse_pos = match input {
        Some(
            UiInputEvent::MouseMove { pos, .. }
            | UiInputEvent::MouseDrag { pos, .. }
            | UiInputEvent::MouseDown { pos, .. }
            | UiInputEvent::MouseUp { pos, .. }
            | UiInputEvent::Scroll { pos, .. },
        ) => *pos,
        _ => Vec2i::default(),
    };
    let key_pressed = match input {
        Some(UiInputEvent::KeyDown { key }) => *key,
        _ => KeyMode::NONE,
    };
    let key_code_pressed = match input {
        Some(UiInputEvent::KeyCodeDown { code }) => *code,
        _ => KeyCode::NONE,
    };
    let text_input = match input {
        Some(UiInputEvent::Text { text }) => text.as_str(),
        _ => "",
    };
    let end_pressed = key_code_pressed.intersects(KeyCode::END);
    let edit = if ctx.focused() {
        apply_text_input(
            buf,
            cursor_pos,
            text_input,
            ctx.key_modes(),
            key_pressed,
            key_code_pressed,
            false,
            ReturnBehavior::Submit,
        )
    } else {
        // Without focus, the textbox ignores key/text input but keeps a consistent outcome.
        super::text_edit::TextEditOutcome {
            cursor: cursor_pos,
            changed: false,
            moved: false,
            submit: false,
        }
    };
    if ctx.focused() {
        cursor_pos = edit.cursor;
        if edit.changed {
            outcome.changed = true;
        }
        if edit.submit {
            outcome.submitted = true;
        }
        if end_pressed {
            cursor_pos = buf.len();
        }
    }
    // Submission records an event but does not cooperatively mutate focus. The runtime dispatcher
    // remains authoritative and continues routing keyboard/text input to its focused node.

    let text_metrics = ctx.atlas().get_text_size(font, buf.as_str());
    let padding = ctx.style().padding;
    let ofx = r.width - padding - text_metrics.width - 1;
    let textx = r.x + if ofx < padding { ofx } else { padding };

    if ctx.focused() && mouse_pressed.intersects(MouseButton::LEFT) && ctx.mouse_over(r, mouse_pos) {
        // Convert local click x into a UTF-8 boundary cursor position.
        let click_x = mouse_pos.x - (textx - r.x);
        cursor_pos = cursor_from_text_x(buf, click_x, font, ctx.atlas());
    }

    cursor_pos = clamp_cursor_boundary(buf, cursor_pos);
    *cursor = cursor_pos;
    outcome
}

/// Semantic editing events produced by the shared single-line editor.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TextboxUpdateOutcome {
    pub(crate) changed: bool,
    pub(crate) submitted: bool,
}

/// Shared single-line textbox painting used by textbox and numeric inline editors.
pub(crate) fn textbox_paint(ctx: &mut WidgetPaintCtx<'_>, buf: &str, cursor: usize, opt: WidgetOption, font: FontId) {
    let r = ctx.local_rect();
    let _ = opt;
    ctx.draw_widget_fill(r, ControlColor::Base);

    let metrics = font_line_metrics(font, ctx.atlas());
    let texty = centered_line_top(r, metrics.line_height);
    let baseline_y = texty + metrics.baseline;

    let text_metrics = ctx.atlas().get_text_size(font, buf);
    let padding = ctx.style().padding;
    let ofx = r.width - padding - text_metrics.width - 1;
    let textx = r.x + if ofx < padding { ofx } else { padding };
    let cursor_pos = clamp_cursor_boundary(buf, cursor);
    let caret_offset = if cursor_pos == 0 {
        0
    } else {
        ctx.atlas().get_text_size(font, &buf[..cursor_pos]).width
    };

    if ctx.focused() {
        // Focused editing path clips text/caret to the textbox bounds.
        let color = ctx.style().colors[ControlColor::Text as usize];
        let caret = caret_rect(textx + caret_offset, baseline_y, metrics, r);
        let mut painter = ctx.painter();
        painter.with_clip(r, |painter| {
            painter.text(font, buf, vec2(textx, texty), color);
            painter.fill_rect(caret, color);
        });
    } else {
        ctx.draw_control_text_with_font(font, buf, r, ControlColor::Text, opt);
    }
}

impl Widget for Textbox {
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

impl WidgetStateOwner for Textbox {
    type State = TextboxState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating textbox parameters with the concrete runtime.
pub struct TextboxBuilder;

impl WidgetBuilder for TextboxBuilder {
    type Parameters = TextboxParameters;
    type W = Textbox;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        let cursor = parameters.buf.len();
        Textbox {
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(TextboxState {
                buf: parameters.buf,
                cursor,
                pending_changes: 0,
                pending_submissions: 0,
                changed_event: crate::event::WidgetEventPort::new(),
                submitted_event: crate::event::WidgetEventPort::new(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;

    fn update_textbox(textbox: &mut Textbox, focused: bool, input: Vec<UiInputEvent>) {
        let atlas = test_atlas();
        let style = Style::default();
        let bounds = rect(0, 0, 120, 20);
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
            let mut ctx = WidgetUpdateCtx::new_with_interaction(
                bounds,
                bounds,
                &style,
                &atlas,
                true,
                false,
                focused,
                false,
                false,
                MouseButton::NONE,
                keys,
                codes,
            );
            textbox.update(&mut ctx, Some(event));
        }
    }

    #[test]
    fn text_events_record_once_per_update_and_accumulate_across_updates() {
        let (state, mut textbox) = Textbox::create(TextboxParameters::new(""));

        update_textbox(
            &mut textbox,
            true,
            vec![
                UiInputEvent::Text { text: "ab".into() },
                UiInputEvent::Text { text: "cd".into() },
                UiInputEvent::KeyDown { key: KeyMode::RETURN },
            ],
        );
        update_textbox(&mut textbox, true, vec![UiInputEvent::Text { text: "e".into() }]);

        assert_eq!(state.try_read(|state| state.text().to_owned()).as_deref(), Some("abcde"));
        assert_eq!(state.try_update(TextboxState::take_changed), Some(true));
        assert_eq!(state.try_update(TextboxState::take_changed), Some(true));
        assert_eq!(state.try_update(TextboxState::take_changed), Some(true));
        assert_eq!(state.try_update(TextboxState::take_changed), Some(false));
        assert_eq!(state.try_update(TextboxState::take_submitted), Some(true));
        assert_eq!(state.try_update(TextboxState::take_submitted), Some(false));
    }

    #[test]
    fn programmatic_text_and_cursor_setters_are_silent() {
        let (state, _textbox) = Textbox::create(TextboxParameters::new("initial"));
        state
            .try_update(|state| {
                state.set_text("replacement");
                state.set_cursor(3);
                state.move_cursor_to_end();
                state.clear();
            })
            .unwrap();
        assert_eq!(state.try_update(TextboxState::take_changed), Some(false));
        assert_eq!(state.try_update(TextboxState::take_submitted), Some(false));
    }
}
