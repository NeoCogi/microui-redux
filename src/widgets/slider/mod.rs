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
//! Slider and numeric entry widgets.
//!
//! Sliders support dragging, wheel increments, snapping, and shift-click text entry; `Number`
//! shares the same text-editing helpers without a slider thumb.
use crate::*;
use crate::widget::{runtime_read_state, runtime_update_state};
use std::fmt::Write;
use std::{cell::RefCell, rc::Rc};

use super::textbox::{textbox_paint, textbox_update};

/// Formats a numeric value using the widget's display precision.
fn number_label(value: Real, precision: usize) -> String {
    let mut label = String::new();
    let _ = write!(label, "{:.*}", precision, value);
    label
}

/// Computes preferred size for numeric widgets with optional visual affordance width.
fn number_preferred_size(
    style: &Style,
    atlas: &AtlasHandle,
    font: FontChoice,
    value: Real,
    precision: usize,
    visual_width: i32,
    visual_height: i32,
) -> Dimensioni {
    let label = number_label(value, precision);
    let resolved_font = style.resolve_font_choice(font);
    let text_w = atlas.get_text_size(resolved_font, label.as_str()).width;
    let padding = style.padding.max(0);
    let vertical_pad = (padding / 2).max(1);
    let font_height = atlas.get_font_height(resolved_font) as i32;
    let width = (text_w + padding * 2 + visual_width.max(0)).max(0);
    let height = (font_height.max(visual_height.max(0)) + vertical_pad * 2).max(0);
    Dimensioni::new(width, height)
}

/// Adds hold-focus while the inline numeric textbox is active.
fn number_effective_widget_opt(opt: WidgetOption, editing: bool) -> WidgetOption {
    if editing { opt | WidgetOption::HOLD_FOCUS } else { opt }
}

/// Chooses drag or text-edit focus behavior for numeric widgets.
fn number_focus_policy(editing: bool) -> FocusPolicy {
    if editing { FocusPolicy::HoldUntilBlur } else { FocusPolicy::DragCapture }
}

#[derive(Clone, Default, PartialEq)]
/// Editing buffer for number-style widgets.
struct NumberEditState {
    /// Whether the widget is currently in edit mode.
    editing: bool,
    /// Text buffer for numeric input.
    buf: String,
    /// Cursor position within the buffer (byte index).
    cursor: usize,
}

/// One-shot construction input for a [`Slider`].
pub struct SliderParameters {
    /// Initial slider value.
    pub value: Real,
    /// Lower bound of the slider range.
    pub low: Real,
    /// Upper bound of the slider range.
    pub high: Real,
    /// Step size used for snapping (0 for continuous).
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Font used for the numeric label and editor.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for SliderParameters {}

impl SliderParameters {
    /// Creates slider parameters with default widget options.
    pub fn new(value: Real, low: Real, high: Real) -> Self {
        Self {
            value,
            low,
            high,
            step: 0.0,
            precision: 0,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::FRAME | WidgetOption::GRAB_SCROLL,
        }
    }

    /// Creates slider parameters with explicit widget options.
    pub fn with_opt(value: Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            low,
            high,
            step,
            precision,
            font: FontChoice::Role(FontRole::Body),
            opt: opt | WidgetOption::GRAB_SCROLL,
        }
    }

    /// Replaces the font used for the numeric label and editor.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent slider state.
pub struct SliderState {
    /// Current slider value.
    value: Real,
    /// Initialization-only lower bound retained for immediate setter clamping.
    low: Real,
    /// Initialization-only upper bound retained for immediate setter clamping.
    high: Real,
    /// Inline numeric editing state.
    edit: NumberEditState,
    /// User value changes waiting to be consumed.
    pending_changes: u32,
}

impl WidgetState for SliderState {}

impl SliderState {
    /// Returns the current slider value.
    pub fn value(&self) -> Real {
        self.value
    }

    /// Updates the current value, clamping it to the slider range.
    pub fn set_value(&mut self, value: Real) {
        self.value = clamp_slider_value(value, self.low, self.high);
    }

    /// Returns whether the inline numeric editor is active.
    pub fn is_editing(&self) -> bool {
        self.edit.editing
    }

    /// Consumes one pending user-originated value change.
    pub fn take_changed(&mut self) -> bool {
        crate::widgets::take_pending_event(&mut self.pending_changes)
    }
}

/// Concrete slider runtime and sole strong owner of its application state.
pub struct Slider {
    /// Initialization-only step size.
    step: Real,
    /// Initialization-only display precision.
    precision: usize,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<SliderState>>,
}

impl Slider {
    /// Constructs a typed state handle and unique slider runtime.
    pub fn create(parameters: SliderParameters) -> (WidgetStateHandle<SliderState>, Self) {
        let widget = SliderBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures the slider track plus formatted value label.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let thumb_size = style.thumb_size.max(0);
        runtime_read_state(&self.state, "Slider::measure", |state| {
            number_preferred_size(style, atlas, self.font, state.value, self.precision, thumb_size, thumb_size)
        })
    }

    /// Updates slider value from shift-click text entry, scroll, or pointer drag.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: &[UiInputEvent]) {
        let base = ctx.local_rect();
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_update_state(&self.state, "Slider::update", |state| {
            let last = state.value;
            let mut v = last;
            if number_textbox_update(ctx, input, &mut state.edit, self.precision, font, &mut v) {
                return;
            }
            if let Some(delta) = input.scroll_delta() {
                let range = state.high - state.low;
                if range != 0.0 {
                    let wheel = if delta.y != 0 { delta.y.signum() } else { delta.x.signum() };
                    if wheel != 0 {
                        let step_amount = if self.step != 0. { self.step.abs() } else { range / 100.0 };
                        v += wheel as Real * step_amount;
                        if self.step != 0. {
                            v = snap_slider_value(v, state.low, self.step);
                        }
                    }
                }
            }
            let range = state.high - state.low;
            if ctx.focused() && (!input.mouse_down().is_empty() || input.mouse_pressed().intersects(MouseButton::LEFT)) && base.width > 0 && range != 0.0 {
                let content_x = input.mouse_pos().x;
                v = state.low + content_x as Real * range / base.width as Real;
                if self.step != 0. {
                    v = snap_slider_value(v, state.low, self.step);
                }
            }
            if range == 0.0 {
                v = state.low;
            }
            v = clamp_slider_value(v, state.low, state.high);
            state.value = v;
            if last != v {
                crate::widgets::record_pending_event(&mut state.pending_changes);
            }
        })
    }

    /// Paints either the inline numeric editor or the slider track/thumb/value label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_read_state(&self.state, "Slider::paint", |state| {
            if state.edit.editing {
                number_textbox_paint(ctx, &state.edit, font);
                return;
            }

            let base = ctx.local_rect();
            let range = state.high - state.low;
            ctx.draw_widget_fill(base, ControlColor::Base);
            let w = ctx.style().thumb_size;
            let available = (base.width - w).max(0);
            let x = if range != 0.0 && available > 0 {
                ((state.value - state.low) * available as Real / range) as i32
            } else {
                0
            };
            let thumb = rect(base.x + x, base.y, w, base.height);
            ctx.draw_widget_internal_frame(thumb, ControlColor::Button);
            let label = number_label(state.value, self.precision);
            ctx.draw_control_text_with_font(font, label.as_str(), base, ControlColor::Text, self.opt);
        });
    }
}

/// Snaps a value to the nearest step relative to the lower bound.
fn snap_slider_value(value: Real, low: Real, step: Real) -> Real {
    let step = step.abs();
    if step == 0.0 { value } else { low + ((value - low) / step).round() * step }
}

/// Clamps a slider value even when the range was provided high-to-low.
fn clamp_slider_value(value: Real, low: Real, high: Real) -> Real {
    let min = low.min(high);
    let max = low.max(high);
    if !value.is_finite() || value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Runs the shared textbox editor for shift-click numeric input.
fn number_textbox_update(
    ctx: &mut WidgetUpdateCtx<'_>,
    input: &[UiInputEvent],
    edit: &mut NumberEditState,
    precision: usize,
    font: FontId,
    value: &mut Real,
) -> bool {
    let shift_click = { input.mouse_pressed().intersects(MouseButton::LEFT) && input.key_mods().intersects(KeyMode::SHIFT) && ctx.hovered() };

    if shift_click {
        // Enter edit mode by seeding the textbox with the current formatted value.
        edit.editing = true;
        edit.buf.clear();
        let _ = write!(edit.buf, "{:.*}", precision, value);
        edit.cursor = edit.buf.len();
    }

    if edit.editing {
        let res = textbox_update(ctx, input, &mut edit.buf, &mut edit.cursor, WidgetOption::NONE, font);
        if res.submitted || !ctx.focused() {
            if let Ok(v) = edit.buf.parse::<f32>() {
                *value = v as Real;
            }
            // Commit valid parsed values and leave edit mode on submit or blur.
            edit.editing = false;
            edit.cursor = 0;
        } else {
            return true;
        }
    }
    false
}

/// Paints the shared textbox editor for a numeric widget.
fn number_textbox_paint(ctx: &mut WidgetPaintCtx<'_>, edit: &NumberEditState, font: FontId) {
    textbox_paint(ctx, edit.buf.as_str(), edit.cursor, WidgetOption::NONE, font);
}

impl Widget for Slider {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Vec<UiInputEvent>) {
        self.update_widget(ctx, &input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        runtime_read_state(&self.state, "Slider::effective_widget_opt", |state| {
            number_effective_widget_opt(self.opt, state.edit.editing)
        })
    }

    fn focus_policy(&self) -> FocusPolicy {
        runtime_read_state(&self.state, "Slider::focus_policy", |state| number_focus_policy(state.edit.editing))
    }
}

impl WidgetStateOwner for Slider {
    type State = SliderState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Type-level constructor for [`Slider`].
pub struct SliderBuilder;

impl WidgetBuilder for SliderBuilder {
    type Parameters = SliderParameters;
    type W = Slider;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        let state = Rc::new(RefCell::new(SliderState {
            value: clamp_slider_value(parameters.value, parameters.low, parameters.high),
            low: parameters.low,
            high: parameters.high,
            edit: NumberEditState::default(),
            pending_changes: 0,
        }));
        Slider {
            step: parameters.step,
            precision: parameters.precision,
            font: parameters.font,
            opt: parameters.opt,
            state,
        }
    }
}

/// One-shot construction input for a [`Number`].
pub struct NumberParameters {
    /// Initial number value.
    pub value: Real,
    /// Step applied when dragging.
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Font used for the numeric label and editor.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for NumberParameters {}

impl NumberParameters {
    /// Creates number parameters with default widget options.
    pub fn new(value: Real, step: Real, precision: usize) -> Self {
        Self {
            value,
            step,
            precision,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates number parameters with explicit widget options.
    pub fn with_opt(value: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            step,
            precision,
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used for the numeric label and editor.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent number-input state.
pub struct NumberState {
    /// Current number value.
    value: Real,
    /// Text editing state for shift-click numeric entry.
    edit: NumberEditState,
    /// User value changes waiting to be consumed.
    pending_changes: u32,
}

impl WidgetState for NumberState {}

impl NumberState {
    /// Returns the current number value.
    pub fn value(&self) -> Real {
        self.value
    }

    /// Updates the current number value, replacing non-finite input with zero.
    pub fn set_value(&mut self, value: Real) {
        self.value = if value.is_finite() { value } else { 0.0 };
    }

    /// Returns whether the inline numeric editor is active.
    pub fn is_editing(&self) -> bool {
        self.edit.editing
    }

    /// Consumes one pending user-originated value change.
    pub fn take_changed(&mut self) -> bool {
        crate::widgets::take_pending_event(&mut self.pending_changes)
    }
}

/// Concrete number-input runtime and sole strong owner of its application state.
pub struct Number {
    /// Initialization-only drag step.
    step: Real,
    /// Initialization-only display precision.
    precision: usize,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<NumberState>>,
}

impl Number {
    /// Constructs a typed state handle and unique number runtime.
    pub fn create(parameters: NumberParameters) -> (WidgetStateHandle<NumberState>, Self) {
        let widget = NumberBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures the formatted number label.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Number::measure", |state| {
            number_preferred_size(style, atlas, self.font, state.value, self.precision, 0, 0)
        })
    }

    /// Updates number value from shift-click text entry or horizontal drag.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: &[UiInputEvent]) {
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_update_state(&self.state, "Number::update", |state| {
            let last = state.value;
            if !number_textbox_update(ctx, input, &mut state.edit, self.precision, font, &mut state.value) {
                if ctx.focused() && input.mouse_down().intersects(MouseButton::LEFT) {
                    state.set_value(state.value + input.mouse_delta().x as Real * self.step);
                } else {
                    state.set_value(state.value);
                }
            } else {
                // Text editing suppresses drag updates while active.
                state.set_value(state.value);
            }
            if state.value != last {
                crate::widgets::record_pending_event(&mut state.pending_changes);
            }
        })
    }

    /// Paints either the inline numeric editor or the formatted value.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_read_state(&self.state, "Number::paint", |state| {
            if state.edit.editing {
                number_textbox_paint(ctx, &state.edit, font);
                return;
            }

            let base = ctx.local_rect();
            ctx.draw_widget_fill(base, ControlColor::Base);
            let label = number_label(state.value, self.precision);
            ctx.draw_control_text_with_font(font, label.as_str(), base, ControlColor::Text, self.opt);
        });
    }
}

impl Widget for Number {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Vec<UiInputEvent>) {
        self.update_widget(ctx, &input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        runtime_read_state(&self.state, "Number::effective_widget_opt", |state| {
            number_effective_widget_opt(self.opt, state.edit.editing)
        })
    }

    fn focus_policy(&self) -> FocusPolicy {
        runtime_read_state(&self.state, "Number::focus_policy", |state| number_focus_policy(state.edit.editing))
    }
}

impl WidgetStateOwner for Number {
    type State = NumberState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Type-level constructor for [`Number`].
pub struct NumberBuilder;

impl WidgetBuilder for NumberBuilder {
    type Parameters = NumberParameters;
    type W = Number;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        let state = Rc::new(RefCell::new(NumberState {
            value: if parameters.value.is_finite() { parameters.value } else { 0.0 },
            edit: NumberEditState::default(),
            pending_changes: 0,
        }));
        Number {
            step: parameters.step,
            precision: parameters.precision,
            font: parameters.font,
            opt: parameters.opt,
            state,
        }
    }
}

#[cfg(test)]
mod tests;
