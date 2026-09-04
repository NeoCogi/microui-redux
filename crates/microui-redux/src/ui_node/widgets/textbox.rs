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
//! and deletion on valid Unicode scalar-value boundaries. Construction, programmatic replacement,
//! and text input remove CR and LF so the stored value always matches the one-line measurement
//! contract. Other characters absent from the selected atlas render through its fallback.
use crate::{ControlRole};

use crate::*;
use crate::math::clamp_i64_to_i32;
use std::{cell::RefCell, rc::Rc};

use super::text_edit::{
    ReturnBehavior, apply_text_input, caret_rect, centered_line_top, clamp_cursor_boundary, cursor_from_text_x, font_line_metrics, normalize_single_line_text,
};

/// One-shot construction input for a [`Textbox`].
pub struct TextboxParameters {
    /// Initial buffer edited by the textbox; CR and LF are removed when the widget is mounted.
    buf: String,
    /// Font used for measurement, editing, and paint.
    pub font: FontRef,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl crate::LeafWidget for Textbox {
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
}

impl WidgetParameters for TextboxParameters {}

impl TextboxParameters {
    /// Creates textbox parameters with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        Self {
            buf: buf.into(),
            font: FontRef::Role(FontRole::Body),
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates textbox parameters with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            buf: buf.into(),
            font: FontRef::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used by the textbox.
    pub fn font(mut self, font: FontRef) -> Self {
        // The editable buffer retains a typed semantic or exact ID reference, never a font name.
        self.font = font;
        self
    }
}

/// Concrete retained textbox, including its semantic and editing state.
pub struct Textbox {
    /// Current text buffer.
    buf: String,
    /// Current byte cursor, always positioned at a Unicode scalar-value boundary.
    cursor: usize,
    /// Initialization-only font.
    font: FontRef,
    /// Base widget options.
    opt: WidgetOption,
    /// Runtime-owned source for user-originated text changes.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<TextboxChanged>>>,
    /// Runtime-owned source for user submissions.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<TextboxSubmitted>>>,
}

impl Textbox {
    /// Returns the current text buffer.
    pub fn text(&self) -> &str {
        self.buf.as_str()
    }

    /// Replaces the current text, removes CR and LF, and moves the cursor to the end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        // Apply the same single-line policy as construction and pasted input. Keeping the stored
        // invariant here prevents measurement, hit testing, and paint from disagreeing about rows.
        self.buf = normalize_single_line_text(text);
        self.cursor = self.buf.len();
    }

    /// Clears the current text and resets the cursor.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    /// Returns the current byte cursor at a Unicode scalar-value boundary.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Moves the cursor to the nearest preceding Unicode scalar-value boundary in the current text.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = clamp_cursor_boundary(&self.buf, cursor);
    }

    /// Moves the cursor to the end of the current text.
    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.buf.len();
    }
}

impl TypedWidgetHandle<Textbox> {
    /// Clones the current text while the widget is retained.
    pub fn text(&self) -> Option<String> {
        self.try_read(|widget| widget.text().to_owned())
    }

    /// Replaces retained text, removes CR and LF, and moves its cursor to the end.
    pub fn set_text(&self, text: impl Into<String>) -> Option<()> {
        self.try_update_with(text.into(), |widget, text| widget.set_text(text)).ok()
    }

    /// Clears the retained text.
    pub fn clear(&self) -> Option<()> {
        self.try_update(Textbox::clear)
    }

    /// Returns the scalar-aligned byte cursor while the widget is retained.
    pub fn cursor(&self) -> Option<usize> {
        self.try_read(Textbox::cursor)
    }

    /// Moves the retained cursor to the nearest preceding Unicode scalar-value boundary.
    pub fn set_cursor(&self, cursor: usize) -> Option<()> {
        self.try_update(|widget| widget.set_cursor(cursor))
    }

    /// Moves the retained cursor to the end.
    pub fn move_cursor_to_end(&self) -> Option<()> {
        self.try_update(Textbox::move_cursor_to_end)
    }

    /// Returns the textbox's native value-change endpoint.
    pub fn changed(&self) -> WidgetEventPortHandle<TextboxChanged> {
        self.widget_event()
    }

    /// Returns the textbox's native submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<TextboxSubmitted> {
        self.widget_event()
    }
}

/// Snapshot emitted after a user-originated textbox value change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextboxChanged {
    /// Complete text value after applying the triggering input event.
    pub text: String,
    /// Scalar-aligned byte cursor after applying the triggering input event.
    pub cursor: usize,
}

impl crate::WidgetEvent for TextboxChanged {}

/// Snapshot emitted when the user submits a textbox value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextboxSubmitted {
    /// Complete text value at submission time.
    pub text: String,
}

impl crate::WidgetEvent for TextboxSubmitted {}

impl Textbox {
    /// Constructs a retained node and a weak typed handle to its concrete textbox.
    pub fn create(parameters: TextboxParameters) -> (TypedWidgetHandle<Self>, Node) {
        let widget = TextboxBuilder::create_widget(parameters);
        Node::typed_widget(widget)
    }

    /// Returns the native event endpoint emitted after every user-originated text change.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<TextboxChanged> {
        <Self as crate::TypedWidget<TextboxChanged>>::event(self)
    }

    /// Returns the native event endpoint emitted whenever the user submits the current text.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<TextboxSubmitted> {
        <Self as crate::TypedWidget<TextboxSubmitted>>::event(self)
    }

    /// Measures a single-line editor, bounded by available width when supplied.
    fn preferred_size_widget(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        let padding = style.metrics.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        let font = style.resolve_font(atlas, &self.font);
        let font_height = atlas.get_font_height(font) as i32;
        let text_w = if self.buf.is_empty() {
            0
        } else {
            atlas.get_text_size(font, self.buf.as_str()).width
        };
        // Text width and style padding are independent; clamp their preferred sum before applying
        // an optional finite parent constraint.
        let mut width = text_w.max(0).saturating_add(padding.saturating_mul(2)).saturating_add(1);
        if let Some(max_width) = constraints.width.bound() {
            width = width.min(max_width);
        }
        let height = font_height.max(0).saturating_add(vertical_pad.saturating_mul(2));
        Dimensioni::new(width, height)
    }

    /// Applies input and cursor movement for this textbox.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let font = ctx.skin().resolve_font(ctx.atlas(), &self.font);
        let outcome = textbox_update(ctx, input, &mut self.buf, &mut self.cursor, self.opt, font);
        let changed = outcome.changed.then(|| TextboxChanged {
            text: self.buf.clone(),
            cursor: self.cursor,
        });
        let submitted = outcome.submitted.then(|| TextboxSubmitted { text: self.buf.clone() });
        if let Some(event) = changed {
            self.changed_event.borrow_mut().emit(event);
        }
        if let Some(event) = submitted {
            self.submitted_event.borrow_mut().emit(event);
        }
    }

    /// Paints the textbox frame, text, and caret.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let font = ctx.skin().resolve_font(ctx.atlas(), &self.font);
        textbox_paint(ctx, self.buf.as_str(), self.cursor, self.opt, font);
    }
}

impl crate::TypedWidget<TextboxChanged> for Textbox {
    fn event(&self) -> crate::WidgetEventPortHandle<TextboxChanged> {
        crate::WidgetEventPortHandle::new(&self.changed_event)
    }
}

impl crate::TypedWidget<TextboxSubmitted> for Textbox {
    fn event(&self) -> crate::WidgetEventPortHandle<TextboxSubmitted> {
        crate::WidgetEventPortHandle::new(&self.submitted_event)
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
    let key_event = match input {
        Some(UiInputEvent::Key { event }) => Some(*event),
        _ => None,
    };
    let text_input = match input {
        Some(UiInputEvent::Text { text }) => text.as_str(),
        _ => "",
    };
    let home_pressed = key_event.is_some_and(|event| event.is_pressed() && event.key == Key::Home);
    let end_pressed = key_event.is_some_and(|event| event.is_pressed() && event.key == Key::End);
    let edit = if ctx.focused() {
        apply_text_input(buf, cursor_pos, text_input, key_event, ReturnBehavior::Submit)
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
        if home_pressed {
            // A single-line editor has one visual start, so Home always selects byte zero.
            cursor_pos = 0;
        } else if end_pressed {
            // End is the symmetric single-line boundary and selects the complete buffer length.
            cursor_pos = buf.len();
        }
    }
    // Submission records an event but does not cooperatively mutate focus. The runtime dispatcher
    // remains authoritative and continues routing keyboard/text input to its focused node.

    let text_metrics = ctx.atlas().get_text_size(font, buf.as_str());
    let padding = ctx.skin().metrics.padding;
    let textx = textbox_text_x(r, padding, text_metrics.width);

    if ctx.focused() && mouse_pressed.intersects(MouseButton::LEFT) && ctx.mouse_over(r, mouse_pos) {
        // Convert local click x into a UTF-8 boundary cursor position.
        let click_x = clamp_i64_to_i32(i64::from(mouse_pos.x) - (i64::from(textx) - i64::from(r.x)));
        cursor_pos = cursor_from_text_x(buf, click_x, font, ctx.atlas());
    }

    cursor_pos = clamp_cursor_boundary(buf, cursor_pos);
    *cursor = cursor_pos;
    outcome
}

/// Semantic editing events produced by the shared single-line editor.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TextboxUpdateOutcome {
    /// Whether user input changed the retained text buffer.
    pub(crate) changed: bool,
    /// Whether user input requested submission without changing focus.
    pub(crate) submitted: bool,
}

/// Chooses the shared single-line text origin, scrolling overflowing text toward its trailing edge.
fn textbox_text_x(bounds: Recti, padding: i32, text_width: i32) -> i32 {
    // Evaluate the complete signed expression in i64. Negative application padding and saturated
    // measured text can then cancel correctly before the retained coordinate is clamped once.
    let trailing_origin = clamp_i64_to_i32(i64::from(bounds.width) - i64::from(padding) - i64::from(text_width) - 1);
    clamp_i64_to_i32(i64::from(bounds.x) + i64::from(trailing_origin.min(padding)))
}

/// Shared single-line textbox painting used by textbox and numeric inline editors.
pub(crate) fn textbox_paint(ctx: &mut WidgetPaintCtx<'_>, buf: &str, cursor: usize, opt: WidgetOption, font: FontId) {
    let r = ctx.local_rect();
    if !opt.intersects(WidgetOption::FRAME) {
        // Runtime-owned framed editors already painted the complete input patch below this content.
        ctx.draw_control_center(ControlRole::TextInput, r);
    }

    let metrics = font_line_metrics(font, ctx.atlas());
    let texty = centered_line_top(r, metrics.line_height);
    let baseline_y = clamp_i64_to_i32(i64::from(texty) + i64::from(metrics.baseline));

    let text_metrics = ctx.atlas().get_text_size(font, buf);
    let padding = ctx.skin().metrics.padding;
    let textx = textbox_text_x(r, padding, text_metrics.width);
    let cursor_pos = clamp_cursor_boundary(buf, cursor);
    let caret_offset = if cursor_pos == 0 {
        0
    } else {
        ctx.atlas().get_text_size(font, &buf[..cursor_pos]).width
    };

    if ctx.focused() {
        // Focused editing path clips text/caret to the textbox bounds.
        let color = ctx.control_content_color(ControlRole::TextInput);
        let caret = caret_rect(clamp_i64_to_i32(i64::from(textx) + i64::from(caret_offset)), baseline_y, metrics, r);
        let mut painter = ctx.painter();
        painter.with_clip(r, |painter| {
            painter.text(font, buf, vec2(textx, texty), color);
            painter.fill_rect(caret, color);
        });
    } else {
        ctx.draw_control_text_with_font(font, buf, r, ControlRole::TextInput, opt);
    }
}

impl Widget for Textbox {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn frame_appearance_role(&self) -> FrameRole {
        // Single-line textboxes use the semantic input frame for every interaction state.
        FrameRole::Control(ControlRole::TextInput)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.update_widget(ctx, input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::TAB_STOP
    }
}

/// Builder associating textbox parameters with the concrete runtime.
pub struct TextboxBuilder;

impl WidgetBuilder for TextboxBuilder {
    type Parameters = TextboxParameters;
    type W = Textbox;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        // Parameters retain their original input until the concrete storage boundary. Normalize
        // here so even a future alternate constructor cannot mount a multiline textbox buffer.
        let buf = normalize_single_line_text(parameters.buf);
        let cursor = buf.len();
        Textbox {
            buf,
            cursor,
            font: parameters.font,
            opt: parameters.opt,
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
            submitted_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{test_atlas, test_skin};

    #[derive(Debug, Eq, PartialEq)]
    enum RecordedEvent {
        Changed(String, usize),
        Submitted(String),
    }

    fn record_changed(events: &mut Vec<RecordedEvent>, event: &TextboxChanged) {
        events.push(RecordedEvent::Changed(event.text.clone(), event.cursor));
    }

    fn record_submitted(events: &mut Vec<RecordedEvent>, event: &TextboxSubmitted) {
        events.push(RecordedEvent::Submitted(event.text.clone()));
    }

    fn text_dispatcher(textbox: &Textbox) -> crate::event::WidgetEventDispatcher<Vec<RecordedEvent>> {
        let mut dispatcher = crate::event::WidgetEventDispatcher::new();
        dispatcher.subscribe(textbox.changed(), record_changed).unwrap();
        dispatcher.subscribe(textbox.submitted(), record_submitted).unwrap();
        dispatcher
    }

    fn update_textbox(textbox: &mut Textbox, focused: bool, input: Vec<UiInputEvent>) {
        let atlas = test_atlas();
        let style = test_skin(&atlas);
        let bounds = rect(0, 0, 120, 20);
        let mut modifiers = Modifiers::NONE;
        for event in &input {
            if let UiInputEvent::Key { event } = event {
                modifiers = event.modifiers;
            }
            let mut ctx =
                WidgetUpdateCtx::new_with_interaction(bounds, bounds, &style, &atlas, true, false, focused, false, false, MouseButton::NONE, modifiers);
            textbox.update(&mut ctx, Some(event));
        }
    }

    #[test]
    fn text_events_preserve_complete_snapshots_and_per_port_fifo() {
        let mut textbox = TextboxBuilder::create_widget(TextboxParameters::new(""));
        let mut dispatcher = text_dispatcher(&textbox);

        update_textbox(
            &mut textbox,
            true,
            vec![
                UiInputEvent::Text { text: "ab".into() },
                UiInputEvent::Text { text: "cd".into() },
                UiInputEvent::Key {
                    event: KeyEvent::pressed(Key::Enter, Modifiers::NONE),
                },
            ],
        );
        update_textbox(&mut textbox, true, vec![UiInputEvent::Text { text: "e".into() }]);

        assert_eq!(textbox.text(), "abcde");
        let mut events = Vec::new();
        assert!(dispatcher.dispatch(&mut events));
        assert_eq!(
            events,
            [
                RecordedEvent::Changed("ab".to_owned(), 2),
                RecordedEvent::Changed("abcd".to_owned(), 4),
                RecordedEvent::Changed("abcde".to_owned(), 5),
                RecordedEvent::Submitted("abcd".to_owned()),
            ]
        );
    }

    #[test]
    fn programmatic_text_and_cursor_setters_are_silent() {
        let mut textbox = TextboxBuilder::create_widget(TextboxParameters::new("initial"));
        let mut dispatcher = text_dispatcher(&textbox);
        textbox.set_text("replacement");
        textbox.set_cursor(3);
        textbox.move_cursor_to_end();
        textbox.clear();
        assert!(!dispatcher.dispatch(&mut Vec::new()));
    }

    /// Verifies the public Home and End keys select the two single-line cursor boundaries.
    #[test]
    fn home_and_end_keys_select_single_line_boundaries() {
        let mut textbox = TextboxBuilder::create_widget(TextboxParameters::new("abcd"));
        textbox.set_cursor(2);

        update_textbox(
            &mut textbox,
            true,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::Home, Modifiers::NONE),
            }],
        );
        assert_eq!(textbox.cursor(), 0);

        update_textbox(
            &mut textbox,
            true,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::End, Modifiers::NONE),
            }],
        );
        assert_eq!(textbox.cursor(), textbox.text().len());
    }

    /// Verifies construction, programmatic replacement, and pasted input all remain single-line.
    #[test]
    fn every_text_ingress_removes_line_endings() {
        let mut textbox = TextboxBuilder::create_widget(TextboxParameters::new("a\r\nb\rc\n"));
        assert_eq!(textbox.text(), "abc");
        assert_eq!(textbox.cursor(), 3);

        textbox.set_text("d\r\ne\rf\n");
        assert_eq!(textbox.text(), "def");
        assert_eq!(textbox.cursor(), 3);

        update_textbox(&mut textbox, true, vec![UiInputEvent::Text { text: "g\r\nh\ri\n".into() }]);
        assert_eq!(textbox.text(), "defghi");
        assert_eq!(textbox.cursor(), 6);
    }
}
