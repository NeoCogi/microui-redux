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
//! Multiline editable text content composed inside a retained scroll area.
//!
//! [`TextArea`] owns only text, cursor, editing, and caret behavior. [`TextArea::create`] mounts
//! that leaf inside [`ScrollArea`], which exclusively owns clipping, translation, wheel input,
//! scroll offsets, and its two reusable scrollbar children. Editing operates on Unicode scalar
//! values rather than grapheme clusters. Stored text uses LF line endings: construction,
//! programmatic replacement, and text input convert CRLF and lone CR at ingress. Rendering uses the
//! selected atlas's glyph coverage and missing-character fallback.

use crate::{ControlRole};

use std::{cell::RefCell, rc::Rc};

use crate::ui_node::text_layout::{TextLine, build_text_lines, text_block_size};
use crate::*;

use super::text_edit::{
    CaretAffinity, FontLineMetrics, ReturnBehavior, affinity_for_line, apply_text_input, caret_rect, clamp_cursor_boundary, cursor_from_x, cursor_x_in_line,
    font_line_metrics, line_index_for_cursor, normalize_multiline_text,
};

/// One-shot construction input for a composed [`TextArea`].
pub struct TextAreaParameters {
    /// Initial buffer edited by the text area; line endings are normalized when mounted.
    buf: String,
    /// Wrapping mode used for measurement, editing, and paint.
    pub wrap: TextWrap,
    /// Font used for measurement, editing, and paint.
    pub font: FontRef,
    /// Presentation and scrolling options applied to the containing [`ScrollArea`].
    pub scroll_options: ScrollAreaOption,
}

impl WidgetParameters for TextAreaParameters {}

impl TextAreaParameters {
    /// Creates framed, scrollable text-area parameters without line wrapping.
    pub fn new(buf: impl Into<String>) -> Self {
        // Text editing is always implemented by the inner leaf. The default outer options provide
        // the familiar framed viewport while keeping all scrolling policy in ScrollArea.
        Self {
            buf: buf.into(),
            wrap: TextWrap::None,
            font: FontRef::Role(FontRole::Body),
            scroll_options: ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
        }
    }

    /// Replaces the options applied to the containing scroll area.
    pub const fn scroll_options(mut self, scroll_options: ScrollAreaOption) -> Self {
        // Store the complete outer policy directly; TextArea contributes no hidden scrolling or
        // framing options of its own.
        self.scroll_options = scroll_options;
        self
    }

    /// Replaces the wrapping mode used by the editable text content.
    pub const fn wrap(mut self, wrap: TextWrap) -> Self {
        // Wrapping is retained by the inner leaf because it determines intrinsic content geometry.
        self.wrap = wrap;
        self
    }

    /// Replaces the font used by the editable text content.
    pub fn font(mut self, font: FontRef) -> Self {
        // Preserve the semantic font choice until each phase resolves it against the active style.
        self.font = font;
        self
    }
}

/// Concrete editable text leaf retained as the ordinary content of a [`ScrollArea`].
pub struct TextArea {
    /// Current text buffer.
    buf: String,
    /// Current byte cursor, always positioned at a Unicode scalar-value boundary.
    cursor: usize,
    /// Requests a runtime-only vertical-cursor preference reset.
    reset_preferred_x: bool,
    /// Requests that the next update reveal the current caret through the containing scroll area.
    reveal_caret: bool,
    /// Wrapping mode used by measurement, editing, and paint.
    wrap: TextWrap,
    /// Font used by measurement, editing, and paint.
    font: FontRef,
    /// Runtime-only derived editing state.
    interaction: TextAreaInteraction,
    /// Weak capability for scroll queries and caret reveal requests after composition.
    scroll_area: Option<TypedWidgetHandle<ScrollArea>>,
    /// Runtime-owned source for user-originated text changes.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<TextAreaChanged>>>,
    /// Runtime-owned source for user submissions.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<TextAreaSubmitted>>>,
}

/// Snapshot emitted after a user-originated text-area value change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextAreaChanged {
    /// Complete text value after applying the triggering input event.
    pub text: String,
    /// Scalar-aligned byte cursor after applying the triggering input event.
    pub cursor: usize,
}

impl crate::WidgetEvent for TextAreaChanged {}

/// Snapshot emitted when the user submits a text-area value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextAreaSubmitted {
    /// Complete text value at submission time.
    pub text: String,
}

impl crate::WidgetEvent for TextAreaSubmitted {}

/// Runtime-only cursor navigation state retained by the editable leaf.
#[derive(Default)]
struct TextAreaInteraction {
    /// Desired caret x position preserved while moving vertically.
    preferred_x: Option<i32>,
    /// Visual side retained when a wrapped line end and continuation start share one byte offset.
    caret_affinity: CaretAffinity,
}

impl TextArea {
    /// Creates editable content inside a scroll area and returns the nested text handle plus outer node.
    pub fn create(parameters: TextAreaParameters) -> (TypedWidgetHandle<Self>, Node) {
        // Split the one-shot input before mounting because presentation belongs to the outer
        // ScrollArea while text, wrapping, and font belong to the retained editable leaf.
        let scroll_options = parameters.scroll_options;
        let editor = Self::new_editor(parameters);
        let (editor_handle, editor_node) = Node::typed_widget(editor);
        let (scroll_handle, root) = ScrollArea::create(ScrollAreaParameters::new(scroll_options, editor_node));

        // Install the weak parent capability only after ScrollArea construction. Both handles are
        // weak, so this internal back-reference cannot create an ownership cycle.
        editor_handle
            .try_update_without_measurement(|editor| editor.scroll_area = Some(scroll_handle))
            .expect("newly composed TextArea content must remain mounted in its ScrollArea");
        (editor_handle, root)
    }

    /// Creates the unmounted editable leaf before its containing scroll area exists.
    fn new_editor(parameters: TextAreaParameters) -> Self {
        // Initialize semantic and event state together so direct unit tests and composed creation
        // observe the same editing behavior.
        let buf = normalize_multiline_text(parameters.buf);
        let cursor = buf.len();
        Self {
            buf,
            cursor,
            reset_preferred_x: false,
            reveal_caret: false,
            wrap: parameters.wrap,
            font: parameters.font,
            interaction: TextAreaInteraction::default(),
            scroll_area: None,
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
            submitted_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }

    /// Returns the current text buffer.
    pub fn text(&self) -> &str {
        // Expose a borrowed semantic view without cloning or consulting presentation state.
        self.buf.as_str()
    }

    /// Replaces text, normalizes line endings to LF, moves the cursor to the end, and reveals it.
    pub fn set_text(&mut self, text: impl Into<String>) {
        // Replace buffer and cursor atomically so no phase can observe a cursor outside the new
        // scalar-aligned byte range. Canonical LF storage also keeps layout and atlas rendering on
        // one line-break convention for programmatic replacements.
        self.buf = normalize_multiline_text(text);
        self.cursor = self.buf.len();
        self.interaction.caret_affinity = CaretAffinity::Upstream;
        self.reset_preferred_x = true;
        self.reveal_caret = true;
    }

    /// Clears text, cursor, navigation preference, and the containing scroll offset.
    pub fn clear(&mut self) {
        // Reset semantic state before synchronizing presentation so the next measure sees the empty
        // document even if the weak scroll capability has already expired during teardown.
        self.buf.clear();
        self.cursor = 0;
        self.interaction.caret_affinity = CaretAffinity::Upstream;
        self.reset_preferred_x = true;
        self.reveal_caret = false;
        self.set_scroll(Vec2i::default());
    }

    /// Returns the current byte cursor at a Unicode scalar-value boundary.
    pub const fn cursor(&self) -> usize {
        // Every mutation path clamps or derives the cursor from valid source boundaries.
        self.cursor
    }

    /// Moves the cursor to the nearest preceding Unicode scalar-value boundary.
    pub fn set_cursor(&mut self, cursor: usize) {
        // Clamp external byte offsets before scheduling navigation-state reset and caret reveal.
        self.cursor = clamp_cursor_boundary(&self.buf, cursor);
        // A bare byte offset has no visual-side information, so public positioning deterministically
        // selects the preceding wrapped segment when the offset is shared.
        self.interaction.caret_affinity = CaretAffinity::Upstream;
        self.reset_preferred_x = true;
        self.reveal_caret = true;
    }

    /// Moves the cursor to the end of the current text and schedules caret reveal.
    pub fn move_cursor_to_end(&mut self) {
        // String length is always a valid terminal UTF-8 boundary.
        self.cursor = self.buf.len();
        self.interaction.caret_affinity = CaretAffinity::Upstream;
        self.reset_preferred_x = true;
        self.reveal_caret = true;
    }

    /// Returns the offset owned by the containing scroll area.
    pub fn scroll(&self) -> Vec2i {
        // An unmounted editor exists only during construction or direct unit tests and therefore has
        // no viewport translation to report.
        self.scroll_area
            .as_ref()
            .and_then(|scroll| scroll.try_read(ScrollArea::offset))
            .unwrap_or_default()
    }

    /// Requests a non-negative offset through the containing scroll area.
    pub fn set_scroll(&self, scroll: Vec2i) {
        // Keep ScrollArea and its two scrollbar children as the only owners of scroll state. Direct
        // editor unit tests have no parent capability and intentionally treat this as a no-op.
        if let Some(scroll_area) = &self.scroll_area {
            scroll_area
                .try_update_without_measurement(|state| state.set_offset(scroll))
                .expect("a retained TextArea's containing ScrollArea must remain available");
        }
    }

    /// Returns the native event endpoint emitted after every user-originated text change.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<TextAreaChanged> {
        // Delegate to the typed-event implementation so widget and handle APIs share one endpoint.
        <Self as crate::TypedWidget<TextAreaChanged>>::event(self)
    }

    /// Returns the native event endpoint emitted whenever the user submits the current text.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<TextAreaSubmitted> {
        // Delegate to the typed-event implementation so widget and handle APIs share one endpoint.
        <Self as crate::TypedWidget<TextAreaSubmitted>>::event(self)
    }

    /// Measures intrinsic editable text using the wrapping width supplied by ScrollArea.
    fn preferred_size_widget(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        // Resolve word wrapping against a bounded viewport width while leaving unwrapped content
        // intrinsically wide enough to create horizontal overflow.
        let font = style.resolve_font(atlas, &self.font);
        let max_width = if self.wrap == TextWrap::Word {
            constraints.width.bound().unwrap_or(i32::MAX / 4).max(1)
        } else {
            i32::MAX / 4
        };
        let lines = build_text_lines(self.buf.as_str(), self.wrap, max_width, font, atlas);
        // Editable text retains the final blank row after a trailing newline so the caret always has
        // a measurable line. Long indivisible words may exceed a bounded wrapping width by design.
        text_block_size(&lines, atlas.get_font_height(font) as i32)
    }

    /// Requests that the containing scroll area reveal one content-local caret rectangle.
    fn reveal_rect(&self, rect: Recti) {
        // Mutate only the independently retained parent widget. ScrollArea's update has completed
        // before child traversal reaches TextArea, and the weak capability cannot extend ownership.
        if let Some(scroll_area) = &self.scroll_area {
            scroll_area
                .try_update_without_measurement(|state| state.scroll_rect_into_view(rect))
                .expect("a retained TextArea's containing ScrollArea must remain available");
        }
    }

    /// Applies one routed editing event and publishes resulting semantic events.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Runtime-only navigation preference is reset lazily so typed cursor setters need no phase
        // context and remain safe between retained updates.
        if self.reset_preferred_x {
            self.interaction.preferred_x = None;
            self.reset_preferred_x = false;
        }
        let font = ctx.skin().resolve_font(ctx.atlas(), &self.font);
        let outcome = textarea_update(ctx, input, self, font);

        // Emit complete immutable snapshots after the semantic update has committed cursor and text.
        if outcome.changed {
            self.changed_event.borrow_mut().emit(TextAreaChanged {
                text: self.buf.clone(),
                cursor: self.cursor,
            });
        }
        if outcome.submitted {
            self.submitted_event.borrow_mut().emit(TextAreaSubmitted { text: self.buf.clone() });
        }
    }

    /// Paints editable content and its caret in stable content-local coordinates.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // ScrollArea has already applied translation and clipping before this child paint phase, so
        // the editor records no offset arithmetic or scrollbar operations.
        let font = ctx.skin().resolve_font(ctx.atlas(), &self.font);
        textarea_paint(ctx, self, font);
    }
}

impl TypedWidgetHandle<TextArea> {
    /// Clones the current text while the editable leaf remains mounted.
    pub fn text(&self) -> Option<String> {
        // Clone inside the checked read closure so no borrow escapes typed access.
        self.try_read(|widget| widget.text().to_owned())
    }

    /// Replaces retained text with canonical LF endings, moves its cursor to the end, and reveals it.
    pub fn set_text(&self, text: impl Into<String>) -> Option<()> {
        // Preserve ownership of the replacement string when access cannot begin.
        self.try_update_with(text.into(), |widget, text| widget.set_text(text)).ok()
    }

    /// Clears retained text, cursor, and containing scroll offset.
    pub fn clear(&self) -> Option<()> {
        // Clearing changes intrinsic measurement, so use the ordinary invalidating typed update.
        self.try_update(TextArea::clear)
    }

    /// Returns the scalar-aligned byte cursor while the editable leaf remains mounted.
    pub fn cursor(&self) -> Option<usize> {
        // Cursor reads do not consult or mutate containing viewport state.
        self.try_read(TextArea::cursor)
    }

    /// Moves the retained cursor to the nearest preceding Unicode scalar boundary.
    pub fn set_cursor(&self, cursor: usize) -> Option<()> {
        // Cursor movement does not change intrinsic text measurement.
        self.try_update_without_measurement(|widget| widget.set_cursor(cursor))
    }

    /// Moves the retained cursor to the end of its text.
    pub fn move_cursor_to_end(&self) -> Option<()> {
        // Cursor movement does not change intrinsic text measurement.
        self.try_update_without_measurement(TextArea::move_cursor_to_end)
    }

    /// Returns the offset owned by the containing scroll area.
    pub fn scroll(&self) -> Option<Vec2i> {
        // Read through the editable leaf's weak parent capability without duplicating offset state.
        self.try_read(TextArea::scroll)
    }

    /// Requests a containing scroll-area offset without invalidating text measurement.
    pub fn set_scroll(&self, scroll: Vec2i) -> Option<()> {
        // The nested call mutates only ScrollArea's interactive state; the text leaf remains the
        // checked access anchor exposed to applications.
        self.try_update_without_measurement(|widget| widget.set_scroll(scroll))
    }

    /// Returns the text area's native value-change endpoint.
    pub fn changed(&self) -> WidgetEventPortHandle<TextAreaChanged> {
        // Resolve the endpoint through the generic typed-widget helper.
        self.widget_event()
    }

    /// Returns the text area's native submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<TextAreaSubmitted> {
        // Resolve the endpoint through the generic typed-widget helper.
        self.widget_event()
    }
}

impl crate::TypedWidget<TextAreaChanged> for TextArea {
    /// Returns the runtime-owned user-change event endpoint.
    fn event(&self) -> crate::WidgetEventPortHandle<TextAreaChanged> {
        // Create a weak endpoint handle without transferring ownership from the retained leaf.
        crate::WidgetEventPortHandle::new(&self.changed_event)
    }
}

impl crate::TypedWidget<TextAreaSubmitted> for TextArea {
    /// Returns the runtime-owned submission event endpoint.
    fn event(&self) -> crate::WidgetEventPortHandle<TextAreaSubmitted> {
        // Create a weak endpoint handle without transferring ownership from the retained leaf.
        crate::WidgetEventPortHandle::new(&self.submitted_event)
    }
}

/// Converts a text-line index into a saturated content-local y coordinate.
fn text_line_y(index: usize, line_height: i32) -> i32 {
    // Retained documents can contain more rows than i32 and valid fonts can use the complete
    // positive metric range. Clamp both conversion and multiplication at the geometry boundary.
    i32::try_from(index).unwrap_or(i32::MAX).saturating_mul(line_height)
}

/// Complete derived text layout in stable content-local coordinates.
struct TextAreaLayout {
    /// Complete allocation supplied by the containing scroll surface.
    bounds: Recti,
    /// Wrapped editable lines, including a trailing blank caret row.
    lines: Vec<TextLine>,
    /// Font metrics shared by navigation, caret geometry, and paint.
    metrics: FontLineMetrics,
}

/// Builds editable lines for the exact content allocation used by update or paint.
fn textarea_layout(bounds: Recti, atlas: &AtlasHandle, state: &TextArea, font: FontId) -> TextAreaLayout {
    // Word wrapping uses the allocated content width selected by ScrollArea convergence. Unwrapped
    // content retains intrinsic line widths and is translated horizontally by the parent viewport.
    let max_width = if state.wrap == TextWrap::Word { bounds.width.max(1) } else { i32::MAX / 4 };
    let lines = build_text_lines(state.buf.as_str(), state.wrap, max_width, font, atlas);
    TextAreaLayout {
        bounds,
        lines,
        metrics: font_line_metrics(font, atlas),
    }
}

/// Applies multiline editing and requests parent-owned scrolling when the caret moves.
fn textarea_update(ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>, state: &mut TextArea, font: FontId) -> TextAreaUpdateOutcome {
    // External cursor and text setters schedule reveal without fabricating an input event.
    let mut outcome = TextAreaUpdateOutcome::default();
    let mut ensure_visible = std::mem::take(&mut state.reveal_caret);
    let mut reset_preferred = false;
    let mut vertical_moved = false;
    let mut preferred_x = state.interaction.preferred_x;
    let mut caret_affinity = state.interaction.caret_affinity;
    let mut cursor_pos = clamp_cursor_boundary(&state.buf, state.cursor);

    // Normalize the one routed input into the scalar and navigation components used by the shared
    // text-edit core. Eventless updates still process pending programmatic caret reveal.
    let text_input = match input {
        Some(UiInputEvent::Text { text }) => text.as_str(),
        _ => "",
    };
    let key_event = match input {
        Some(UiInputEvent::Key { event }) => Some(*event),
        _ => None,
    };

    if ctx.focused() {
        // Apply scalar editing and horizontal cursor movement before building layout because inserted
        // text can immediately change wrapping, line indices, and intrinsic content height.
        let cursor_before_edit = cursor_pos;
        let edit = apply_text_input(
            &mut state.buf,
            cursor_pos,
            text_input,
            key_event,
            ReturnBehavior::Newline { submit_on_ctrl: true },
        );
        cursor_pos = edit.cursor;
        if edit.changed {
            outcome.changed = true;
            ensure_visible = true;
            reset_preferred = true;
            if edit.cursor != cursor_before_edit {
                // Insertion and backspace produce a new byte position with no retained visual-side
                // meaning. Forward Delete leaves the cursor fixed and therefore preserves its
                // selected continuation side when the wrap boundary survives reflow.
                caret_affinity = CaretAffinity::Upstream;
            }
        }
        if edit.moved {
            ensure_visible = true;
            reset_preferred = true;
            // Horizontal movement can land on the byte shared by two wrapped lines. Left arrives
            // from inside the continuation and must keep that downstream visual side; Right
            // arrives from the preceding line and keeps the upstream side. At unshared positions
            // the affinity is inert but retaining the same rule makes the next layout deterministic.
            caret_affinity = if key_event.is_some_and(|event| event.is_pressed() && event.key == Key::ArrowLeft) {
                CaretAffinity::Downstream
            } else {
                CaretAffinity::Upstream
            };
        }
        outcome.submitted = edit.submit;
    }

    // Resolve line geometry from the updated buffer in content-local coordinates. ScrollArea's
    // translation already localizes pointer positions into this same coordinate system.
    let layout = textarea_layout(ctx.local_rect(), ctx.atlas(), state, font);
    let mut cursor_line = line_index_for_cursor(&layout.lines, cursor_pos, caret_affinity);
    let mut caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    if ctx.focused() {
        if key_event.is_some_and(|event| event.is_pressed() && event.key == Key::Home) {
            // Home targets the current visual line. On a wrapped continuation its start shares a
            // byte with the previous line's end, so preserve the selected downstream side.
            cursor_pos = layout.lines[cursor_line].start;
            caret_affinity = affinity_for_line(&layout.lines, cursor_line, cursor_pos);
            caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());
            ensure_visible = true;
            reset_preferred = true;
        } else if key_event.is_some_and(|event| event.is_pressed() && event.key == Key::End) {
            // End targets the current visual line, including wrapped segments.
            cursor_pos = layout.lines[cursor_line].end;
            caret_affinity = affinity_for_line(&layout.lines, cursor_line, cursor_pos);
            caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());
            ensure_visible = true;
            reset_preferred = true;
        }

        if key_event.is_some_and(|event| event.is_pressed() && event.key == Key::ArrowUp) {
            // Preserve the desired visual x while moving to the nearest scalar boundary above.
            let target_x = preferred_x.unwrap_or(caret_x);
            if cursor_line > 0 {
                cursor_line -= 1;
                cursor_pos = cursor_from_x(&layout.lines[cursor_line], state.buf.as_str(), target_x, font, ctx.atlas());
                caret_affinity = affinity_for_line(&layout.lines, cursor_line, cursor_pos);
            }
            preferred_x = Some(target_x);
            ensure_visible = true;
            vertical_moved = true;
        }

        if key_event.is_some_and(|event| event.is_pressed() && event.key == Key::ArrowDown) {
            // Preserve the desired visual x while moving to the nearest scalar boundary below.
            let target_x = preferred_x.unwrap_or(caret_x);
            if cursor_line + 1 < layout.lines.len() {
                cursor_line += 1;
                cursor_pos = cursor_from_x(&layout.lines[cursor_line], state.buf.as_str(), target_x, font, ctx.atlas());
                caret_affinity = affinity_for_line(&layout.lines, cursor_line, cursor_pos);
            }
            preferred_x = Some(target_x);
            ensure_visible = true;
            vertical_moved = true;
        }
    }

    if ctx.focused()
        && let Some(UiInputEvent::MouseDown { pos, button }) = input
        && button.intersects(MouseButton::LEFT)
        && ctx.mouse_over(layout.bounds, *pos)
    {
        // Pointer positions already include the parent's scroll translation. Convert directly from
        // content-local pixels into a visual line and nearest scalar boundary.
        let last_line = i32::try_from(layout.lines.len().saturating_sub(1)).unwrap_or(i32::MAX);
        let line_idx = (pos.y / layout.metrics.line_height.max(1)).clamp(0, last_line) as usize;
        cursor_pos = cursor_from_x(&layout.lines[line_idx], state.buf.as_str(), pos.x, font, ctx.atlas());
        caret_affinity = affinity_for_line(&layout.lines, line_idx, cursor_pos);
        ensure_visible = true;
        reset_preferred = true;
    }

    // Re-resolve final caret geometry after keyboard and pointer navigation have both completed.
    cursor_pos = clamp_cursor_boundary(&state.buf, cursor_pos);
    cursor_line = line_index_for_cursor(&layout.lines, cursor_pos, caret_affinity);
    caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());

    if reset_preferred && !vertical_moved {
        // Horizontal edits and direct pointer placement start a fresh preferred visual column.
        preferred_x = None;
    }
    if preferred_x.is_none() {
        // Seed vertical navigation from the final caret x when no earlier column is retained.
        preferred_x = Some(caret_x);
    }

    state.cursor = cursor_pos;
    state.interaction.preferred_x = preferred_x;
    state.interaction.caret_affinity = caret_affinity;
    if ensure_visible {
        // Reveal the complete caret line rather than a single pixel so vertical navigation keeps its
        // baseline and descent inside the parent viewport.
        state.reveal_rect(Recti::new(
            caret_x,
            text_line_y(cursor_line, layout.metrics.line_height),
            1,
            layout.metrics.line_height,
        ));
    }
    outcome
}

/// Outcome flags consumed by the retained text-area event publisher.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct TextAreaUpdateOutcome {
    /// Whether user input changed the complete text buffer.
    changed: bool,
    /// Whether user input requested submission without inserting a newline.
    submitted: bool,
}

/// Paints text and the focused caret while ScrollArea supplies translation and clipping.
fn textarea_paint(ctx: &mut WidgetPaintCtx<'_>, state: &TextArea, font: FontId) {
    // Derive the same wrapped lines used by update, then limit recording to lines intersecting the
    // traversal-owned local clip so large documents do not emit invisible text operations.
    let layout = textarea_layout(ctx.local_rect(), ctx.atlas(), state, font);
    let local_clip = ctx.local_clip();
    let line_height = layout.metrics.line_height.max(1);
    let first_line = (local_clip.y.max(0) / line_height) as usize;
    let visible_bottom = local_clip.y.saturating_add(local_clip.height).max(0);
    let last_line = ((visible_bottom / line_height) as usize + 1).min(layout.lines.len());

    // Fill the translated content surface with the editable base color; Painter clips the large
    // semantic rectangle to the effective ScrollArea viewport.
    ctx.draw_control_center(ControlRole::TextInput, layout.bounds);
    let color = ctx.control_foreground(ControlRole::TextInput);
    let cursor_pos = clamp_cursor_boundary(&state.buf, state.cursor);
    let cursor_line = line_index_for_cursor(&layout.lines, cursor_pos, state.interaction.caret_affinity);
    let caret_x = cursor_x_in_line(&layout.lines[cursor_line], state.buf.as_str(), cursor_pos, font, ctx.atlas());
    let caret = if ctx.focused() {
        // Align the caret with the same baseline metrics as the corresponding text line.
        let line_top = text_line_y(cursor_line, line_height);
        Some(caret_rect(
            caret_x,
            line_top.saturating_add(layout.metrics.baseline),
            layout.metrics,
            layout.bounds,
        ))
    } else {
        None
    };

    let mut painter = ctx.painter();
    painter.with_clip(layout.bounds, |painter| {
        // Record only the line interval intersecting the inherited viewport clip.
        for (idx, line) in layout.lines.iter().enumerate().take(last_line).skip(first_line) {
            let text = &state.buf[line.start..line.end];
            if !text.is_empty() {
                painter.text(font, text, Vec2i::new(0, text_line_y(idx, line_height)), color);
            }
        }

        if let Some(caret) = caret {
            // Painter's inherited clip rejects an offscreen caret without editor-owned scroll math.
            painter.fill_rect(caret, color);
        }
    });
}

impl Widget for TextArea {
    /// Returns the editable leaf's base options without outer framing or wheel policy.
    fn widget_opt(&self) -> &WidgetOption {
        // ScrollArea owns frame and GRAB_SCROLL. Keyboard focus is declared separately below.
        &WidgetOption::NONE
    }

    /// Applies one routed event to retained text and cursor state.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Keep the trait boundary thin so unit tests and retained traversal share one implementation.
        self.update_widget(ctx, input)
    }

    /// Records text and caret paint operations in content-local coordinates.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Keep the trait boundary thin so direct paint tests and traversal share one implementation.
        self.paint_widget(ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        // The inner editor is the composed control's one Tab stop. Its containing ScrollArea and
        // structural scrollbars remain non-focusable pointer surfaces and therefore preserve this focus.
        KeyboardBehavior::TAB_STOP
    }
}

impl LeafWidget for TextArea {
    /// Measures intrinsic editable text content for the containing scroll surface.
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        // Delegate to the phase-independent measurement helper used by composition and tests.
        self.preferred_size_widget(style, atlas, constraints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;
    use crate::test_support::{test_atlas, test_skin};
    use crate::ui_node::UiRuntime;
    use crate::UNCLIPPED_RECT;

    #[derive(Debug, Eq, PartialEq)]
    enum RecordedEvent {
        Changed(String, usize),
        Submitted(String),
    }

    /// Records one changed snapshot in dispatcher order.
    fn record_changed(events: &mut Vec<RecordedEvent>, event: &TextAreaChanged) {
        // Clone the event payload because the test log outlives this dispatch callback.
        events.push(RecordedEvent::Changed(event.text.clone(), event.cursor));
    }

    /// Records one submitted snapshot in dispatcher order.
    fn record_submitted(events: &mut Vec<RecordedEvent>, event: &TextAreaSubmitted) {
        // Clone the event payload because the test log outlives this dispatch callback.
        events.push(RecordedEvent::Submitted(event.text.clone()));
    }

    /// Creates a dispatcher subscribed to both native TextArea event ports.
    fn text_dispatcher(text_area: &TextArea) -> crate::event::WidgetEventDispatcher<Vec<RecordedEvent>> {
        // Register both ports against one ordered state log to characterize independent delivery.
        let mut dispatcher = crate::event::WidgetEventDispatcher::new();
        dispatcher.subscribe(text_area.changed(), record_changed).unwrap();
        dispatcher.subscribe(text_area.submitted(), record_submitted).unwrap();
        dispatcher
    }

    /// Runs direct focused editor updates at one explicit content allocation.
    fn update_text_area_in_bounds(text_area: &mut TextArea, bounds: Recti, input: Vec<UiInputEvent>) {
        // Reproduce held key state across the supplied FIFO events while keeping one stable content
        // allocation and focused interaction snapshot. Supplying bounds lets wrapping regressions
        // exercise the same update path without mounting the surrounding scroll area.
        let atlas = test_atlas();
        let style = test_skin(&atlas);
        let mut modifiers = Modifiers::NONE;
        for event in &input {
            if let UiInputEvent::Key { event } = event {
                modifiers = event.modifiers;
            }
            let mut ctx = WidgetUpdateCtx::new_with_interaction(bounds, bounds, &style, &atlas, true, true, true, false, false, MouseButton::NONE, modifiers);
            text_area.update(&mut ctx, Some(event));
        }
    }

    /// Runs direct focused editor updates at the ordinary non-wrapping test allocation.
    fn update_text_area(text_area: &mut TextArea, input: Vec<UiInputEvent>) {
        update_text_area_in_bounds(text_area, Recti::new(0, 0, 160, 80), input);
    }

    /// Verifies independent changed and submitted snapshots from the editable content leaf.
    #[test]
    fn text_area_dispatches_independent_change_and_submission_events() {
        // Directly exercise the inner leaf because scrolling composition is orthogonal to event
        // payload order and the constructor uses this exact semantic initialization path.
        let mut text_area = TextArea::new_editor(TextAreaParameters::new(""));
        let mut dispatcher = text_dispatcher(&text_area);
        update_text_area(
            &mut text_area,
            vec![
                UiInputEvent::Text { text: "line".into() },
                UiInputEvent::Key {
                    event: KeyEvent::pressed(Key::Control, Modifiers::CTRL),
                },
                UiInputEvent::Key {
                    event: KeyEvent::pressed(Key::Enter, Modifiers::CTRL),
                },
            ],
        );
        let mut events = Vec::new();
        assert!(dispatcher.dispatch(&mut events));
        assert_eq!(
            events,
            [RecordedEvent::Changed("line".to_owned(), 4), RecordedEvent::Submitted("line".to_owned())]
        );
    }

    /// Verifies construction, replacement, and pasted text share canonical LF storage.
    #[test]
    fn every_multiline_text_ingress_normalizes_line_endings() {
        let mut text_area = TextArea::new_editor(TextAreaParameters::new("a\r\nb\rc\n"));
        assert_eq!(text_area.text(), "a\nb\nc\n");
        assert_eq!(text_area.cursor(), text_area.text().len());

        text_area.set_text("d\r\ne\rf\n");
        assert_eq!(text_area.text(), "d\ne\nf\n");

        update_text_area(&mut text_area, vec![UiInputEvent::Text { text: "g\r\nh\ri\n".into() }]);
        assert_eq!(text_area.text(), "d\ne\nf\ng\nh\ni\n");
        assert_eq!(text_area.cursor(), text_area.text().len());
    }

    /// Verifies vertical navigation retains the continuation side of a wrapped byte boundary.
    #[test]
    fn wrapped_vertical_navigation_retains_visual_caret_affinity() {
        // With the test font's eight-pixel advances, this width produces ranges 0..2 and 2..4.
        // Byte 2 is therefore both the first line's end and the continuation line's start.
        let mut text_area = TextArea::new_editor(TextAreaParameters::new("_ __").wrap(TextWrap::Word));
        text_area.set_cursor(0);
        let atlas = test_atlas();
        let style = test_skin(&atlas);
        let bounds = Recti::new(0, 0, 16, 30);
        let down = UiInputEvent::Key {
            event: KeyEvent::pressed(Key::ArrowDown, Modifiers::NONE),
        };
        let mut ctx = WidgetUpdateCtx::new_with_interaction(
            bounds,
            bounds,
            &style,
            &atlas,
            true,
            true,
            true,
            false,
            false,
            MouseButton::NONE,
            Modifiers::NONE,
        );
        text_area.update(&mut ctx, Some(&down));

        let font = style.resolve_font(&atlas, &text_area.font);
        let layout = textarea_layout(bounds, &atlas, &text_area, font);
        assert_eq!(text_area.cursor(), 2);
        assert_eq!(text_area.interaction.caret_affinity, CaretAffinity::Downstream);
        assert_eq!(
            line_index_for_cursor(&layout.lines, text_area.cursor(), text_area.interaction.caret_affinity),
            1
        );

        // Forward Delete does not move the byte cursor. The shorter replacement still wraps at the
        // same boundary, so the caret must remain on the continuation side after reflow.
        let delete = UiInputEvent::Key {
            event: KeyEvent::pressed(Key::Delete, Modifiers::NONE),
        };
        let mut ctx = WidgetUpdateCtx::new_with_interaction(
            bounds,
            bounds,
            &style,
            &atlas,
            true,
            true,
            true,
            false,
            false,
            MouseButton::NONE,
            Modifiers::NONE,
        );
        text_area.update(&mut ctx, Some(&delete));
        assert_eq!(text_area.text(), "_ _");
        assert_eq!(text_area.cursor(), 2);
        assert_eq!(text_area.interaction.caret_affinity, CaretAffinity::Downstream);

        // End must now operate on the selected continuation rather than jumping back to line zero.
        let end = UiInputEvent::Key {
            event: KeyEvent::pressed(Key::End, Modifiers::NONE),
        };
        let mut ctx = WidgetUpdateCtx::new_with_interaction(
            bounds,
            bounds,
            &style,
            &atlas,
            true,
            true,
            true,
            false,
            false,
            MouseButton::NONE,
            Modifiers::NONE,
        );
        text_area.update(&mut ctx, Some(&end));
        assert_eq!(text_area.cursor(), 3);
    }

    /// Verifies horizontal arrows retain the visual side from which they enter a wrap boundary.
    #[test]
    fn wrapped_horizontal_navigation_selects_the_arrival_side_of_a_shared_boundary() {
        // With the test font's eight-pixel advances, this allocation produces ranges 0..2 and
        // 2..4. Byte 2 therefore has distinct upstream and downstream visual positions.
        let bounds = Recti::new(0, 0, 16, 30);
        let parameters = || TextAreaParameters::new("_ __").wrap(TextWrap::Word);

        let mut from_continuation = TextArea::new_editor(parameters());
        from_continuation.set_cursor(3);
        update_text_area_in_bounds(
            &mut from_continuation,
            bounds,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::ArrowLeft, Modifiers::NONE),
            }],
        );
        assert_eq!(from_continuation.cursor(), 2);
        assert_eq!(from_continuation.interaction.caret_affinity, CaretAffinity::Downstream);

        let mut from_first_line = TextArea::new_editor(parameters());
        from_first_line.set_cursor(1);
        update_text_area_in_bounds(
            &mut from_first_line,
            bounds,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::ArrowRight, Modifiers::NONE),
            }],
        );
        assert_eq!(from_first_line.cursor(), 2);
        assert_eq!(from_first_line.interaction.caret_affinity, CaretAffinity::Upstream);

        // Resolve both stored affinities against the actual wrapped layout, proving that equal
        // public byte cursors paint and reveal on the two intended visual lines.
        let atlas = test_atlas();
        let style = test_skin(&atlas);
        let font = style.resolve_font(&atlas, &from_continuation.font);
        let layout = textarea_layout(bounds, &atlas, &from_continuation, font);
        assert_eq!(line_index_for_cursor(&layout.lines, 2, from_continuation.interaction.caret_affinity), 1);
        assert_eq!(line_index_for_cursor(&layout.lines, 2, from_first_line.interaction.caret_affinity), 0);
    }

    /// Verifies Home selects the start of the current wrapped visual line, not the whole buffer.
    #[test]
    fn wrapped_home_selects_the_continuation_start() {
        let bounds = Recti::new(0, 0, 16, 30);
        let mut text_area = TextArea::new_editor(TextAreaParameters::new("_ __").wrap(TextWrap::Word));
        text_area.set_cursor(3);

        update_text_area_in_bounds(
            &mut text_area,
            bounds,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::Home, Modifiers::NONE),
            }],
        );

        assert_eq!(text_area.cursor(), 2);
        assert_eq!(text_area.interaction.caret_affinity, CaretAffinity::Downstream);
    }

    /// Verifies multiline backspace at byte zero cannot delete the following newline.
    #[test]
    fn backspace_at_start_preserves_leading_newline() {
        let mut text_area = TextArea::new_editor(TextAreaParameters::new("\ntext"));
        text_area.set_cursor(0);
        update_text_area(
            &mut text_area,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::Backspace, Modifiers::NONE),
            }],
        );

        assert_eq!(text_area.text(), "\ntext");
        assert_eq!(text_area.cursor(), 0);
    }

    /// Verifies the five-node composition and nested typed-handle lifetime.
    #[test]
    fn text_area_create_returns_nested_editor_handle_and_scroll_area_node() {
        // The outer node owns ScrollArea, its transform surface and two bars, plus exactly one
        // ordinary TextArea content leaf.
        let (text_area, root) = TextArea::create(TextAreaParameters::new("content"));
        assert_eq!(root.debug_node_count(), 5);
        assert!(text_area.is_alive());
        assert_eq!(text_area.text().as_deref(), Some("content"));

        // Dropping the outer owner expires the nested semantic handle and its weak parent link.
        drop(root);
        assert!(!text_area.is_alive());
    }

    /// Verifies that programmatic semantic and scroll mutations do not emit user events.
    #[test]
    fn programmatic_text_cursor_and_scroll_setters_are_silent() {
        // Subscribe through the nested editor while applying semantic and parent-scroll mutations.
        let (text_area, _root) = TextArea::create(TextAreaParameters::new("initial"));
        let mut dispatcher = text_area.try_read(text_dispatcher).unwrap();
        text_area.set_text("replacement").unwrap();
        text_area.set_cursor(3).unwrap();
        text_area.move_cursor_to_end().unwrap();
        text_area.set_scroll(Vec2i::new(5, 7)).unwrap();
        text_area.clear().unwrap();
        assert!(!dispatcher.dispatch(&mut Vec::new()));
        assert_eq!(text_area.scroll().map(|offset| (offset.x, offset.y)), Some((0, 0)));
    }

    /// Verifies that a real scrollbar capture gesture preserves TextArea keyboard focus.
    #[test]
    fn scrollbar_press_preserves_text_area_keyboard_focus() {
        // Compose enough narrow lines to activate only the vertical scrollbar in a predictable
        // unframed, zero-padding viewport.
        let document = (0..20).map(|index| format!("line {index}")).collect::<Vec<_>>().join("\n");
        let (text_area, mut root) = TextArea::create(TextAreaParameters::new(document.clone()).scroll_options(ScrollAreaOption::ENABLE_SCROLL));
        let atlas = test_atlas();
        let style = test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });
        let viewport = Recti::new(0, 0, 100, 60);
        let mut runtime = UiRuntime::new();
        let mut input = Input::default();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);

        // Focus the text content near its first line and complete that captured press gesture.
        input.mousedown(1, 1, MouseButton::LEFT);
        let editor_down = input.pop_event().unwrap();
        let editor_down_state = input.snapshot();
        runtime.begin_input_event(true, &editor_down);
        let (editor_owner, editor_result) = runtime
            .route_input_event_to_node_ref(&mut root, &style, &editor_down)
            .expect("the editable content must receive its pointer press");
        runtime.update_pointer_capture(editor_owner, editor_result, &editor_down, editor_down_state.mouse_buttons);
        runtime.update_tree_root(&mut root, &style, atlas.clone(), editor_down_state);
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);

        input.mouseup(1, 1, MouseButton::LEFT);
        let editor_up = input.pop_event().unwrap();
        let editor_up_state = input.snapshot();
        runtime.begin_input_event(true, &editor_up);
        assert_eq!(
            runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, editor_up_state.mouse_buttons, &editor_up,),
            Some(true)
        );
        runtime.update_tree_root(&mut root, &style, atlas.clone(), editor_up_state);
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);

        // Press and release the vertical scrollbar at x=95. It must own this pointer capture without
        // replacing the editor's hold-focus identity.
        input.mousedown(95, 1, MouseButton::LEFT);
        let scrollbar_down = input.pop_event().unwrap();
        let scrollbar_down_state = input.snapshot();
        runtime.begin_input_event(true, &scrollbar_down);
        let (scrollbar_owner, scrollbar_result) = runtime
            .route_input_event_to_node_ref(&mut root, &style, &scrollbar_down)
            .expect("the visible vertical scrollbar must receive its pointer press");
        assert_ne!(scrollbar_owner, editor_owner);
        runtime.update_pointer_capture(scrollbar_owner, scrollbar_result, &scrollbar_down, scrollbar_down_state.mouse_buttons);
        runtime.update_tree_root(&mut root, &style, atlas.clone(), scrollbar_down_state);
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);

        input.mouseup(95, 1, MouseButton::LEFT);
        let scrollbar_up = input.pop_event().unwrap();
        let scrollbar_up_state = input.snapshot();
        runtime.begin_input_event(true, &scrollbar_up);
        assert_eq!(
            runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, scrollbar_up_state.mouse_buttons, &scrollbar_up,),
            Some(true)
        );
        runtime.update_tree_root(&mut root, &style, atlas.clone(), scrollbar_up_state);
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);

        // Keyboard text must still route to TextArea after the pointer-only scrollbar gesture.
        input.text("X");
        let text_event = input.pop_event().unwrap();
        let text_state = input.snapshot();
        runtime.begin_input_event(true, &text_event);
        assert!(runtime.route_focus_input_event(std::slice::from_mut(&mut root), &style, &text_event));
        runtime.update_tree_root(&mut root, &style, atlas, text_state);
        assert_eq!(text_area.text().map(|text| text.len()), Some(document.len() + 1));
    }

    /// Verifies ancestor-only wheel bubbling from editable content to its ScrollArea.
    #[test]
    fn wheel_input_bubbles_from_text_content_to_scroll_area() {
        // Build vertical overflow while leaving TextArea itself free of GRAB_SCROLL so the routed
        // wheel event must follow ancestor-only bubbling to its containing ScrollArea.
        let document = (0..20).map(|index| format!("line {index}")).collect::<Vec<_>>().join("\n");
        let (text_area, mut root) = TextArea::create(TextAreaParameters::new(document).scroll_options(ScrollAreaOption::ENABLE_SCROLL));
        let atlas = test_atlas();
        let style = test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });
        let viewport = Recti::new(0, 0, 100, 60);
        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);

        // Seed a pointer position over editable content, then route one wheel transition through the
        // ordinary deepest-target and ancestor-bubbling path.
        let mut input = Input::default();
        input.mousemove(1, 1);
        let _ = input.pop_event().expect("pointer seed must be queued");
        input.scroll(0, 12);
        let wheel = input.pop_event().expect("wheel input must be queued");
        let wheel_state = input.snapshot();
        runtime.begin_input_event(true, &wheel);
        let (_, result) = runtime
            .route_input_event_to_node_ref(&mut root, &style, &wheel)
            .expect("wheel input over TextArea must find its composed target path");
        assert!(result.is_consumed(), "ScrollArea must consume a wheel delta that changes its range");
        runtime.update_tree_root(&mut root, &style, atlas.clone(), wheel_state);
        runtime.layout_tree_root(&mut root, &style, atlas, viewport, UNCLIPPED_RECT);

        // The nested semantic handle observes the offset owned and changed by ScrollArea.
        assert_eq!(text_area.scroll().map(|offset| (offset.x, offset.y)), Some((0, 12)));
    }

    /// Verifies that a typed cursor mutation requests parent-owned caret reveal.
    #[test]
    fn programmatic_cursor_reveal_scrolls_the_composed_viewport() {
        // A narrow multiline document overflows vertically without requiring a horizontal bar.
        let document = (0..20).map(|index| format!("line {index}"));
        let (text_area, mut root) = TextArea::create(TextAreaParameters::new(document.collect::<Vec<_>>().join("\n")));
        let atlas = test_atlas();
        let style = test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });
        let viewport = Recti::new(0, 0, 100, 60);
        let mut runtime = UiRuntime::new();

        // Commit initial ranges, schedule an end-cursor reveal, then let the ordinary child update
        // request and subsequent layout clamp the parent-owned vertical offset.
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);
        text_area.move_cursor_to_end().unwrap();
        runtime.update_tree_root(&mut root, &style, atlas.clone(), Input::default().snapshot());
        runtime.layout_tree_root(&mut root, &style, atlas, viewport, UNCLIPPED_RECT);

        let scroll = text_area.scroll().unwrap();
        assert_eq!(scroll.x, 0);
        assert!(scroll.y > 0, "revealing the final caret must move the parent-owned vertical offset");
    }
}
