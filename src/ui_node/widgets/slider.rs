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
//! Retained slider widget.
//!
//! Sliders support dragging, wheel increments, snapping, and shift-click text entry.
use crate::*;
use crate::ui_node::{runtime_read_state, runtime_update_state};
use std::{cell::RefCell, rc::Rc};

use super::numeric_edit::*;

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
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let base = ctx.local_rect();
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_update_state(&self.state, "Slider::update", |state| {
            let last = state.value;
            let mut v = last;
            if number_textbox_update(ctx, input, &mut state.edit, self.precision, font, &mut v) {
                return;
            }
            if let Some(UiInputEvent::Scroll { delta, .. }) = input {
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
            let pointer_pos = match input {
                Some(
                    UiInputEvent::MouseMove { pos, .. }
                    | UiInputEvent::MouseDrag { pos, .. }
                    | UiInputEvent::MouseDown { pos, .. }
                    | UiInputEvent::MouseUp { pos, .. },
                ) => Some(*pos),
                _ => None,
            };
            if ctx.focused()
                && ctx.mouse_buttons().intersects(MouseButton::LEFT)
                && let Some(pointer_pos) = pointer_pos
                && base.width > 0
                && range != 0.0
            {
                let content_x = pointer_pos.x;
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

impl Widget for Slider {
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

#[cfg(test)]
mod tests {
    //! Tests for slider and numeric editing behavior.

    use super::*;
    use crate::test_support::test_atlas as make_test_atlas;
    use crate::ui_node::{UiInputEvent, widget_context::localize_event};
    use crate::{Number, NumberParameters, NumberState};

    fn run_slider_once(slider: &mut Slider, rect: Recti, events: Vec<UiInputEvent>, hovered: bool, focused: bool, active: bool, scroll: Option<Vec2i>) {
        let atlas = make_test_atlas();
        let style = Style::default();
        let held = if active { MouseButton::LEFT } else { MouseButton::NONE };
        let mut events = events
            .into_iter()
            .map(|event| localize_event(Vec2i::new(rect.x, rect.y), event))
            .collect::<Vec<_>>();
        if let Some(delta) = scroll {
            events.push(UiInputEvent::Scroll { pos: Vec2i::default(), delta });
        }
        for event in &events {
            let mut ctx = WidgetUpdateCtx::new_with_interaction(
                rect,
                rect,
                &style,
                &atlas,
                true,
                hovered,
                focused,
                false,
                active,
                held,
                KeyMode::NONE,
                KeyCode::NONE,
            );
            slider.update(&mut ctx, Some(event));
        }
    }

    fn run_number_once(number: &mut Number, events: Vec<UiInputEvent>) {
        let atlas = make_test_atlas();
        let style = Style::default();
        let bounds = rect(0, 0, 100, 20);
        for event in &events {
            let mut ctx = WidgetUpdateCtx::new_with_interaction(
                bounds,
                bounds,
                &style,
                &atlas,
                true,
                true,
                true,
                false,
                true,
                MouseButton::LEFT,
                KeyMode::NONE,
                KeyCode::NONE,
            );
            number.update(&mut ctx, Some(event));
        }
    }

    fn assert_real_close(actual: Real, expected: Real) {
        assert!((actual - expected).abs() < 1.0e-5, "expected {expected}, got {actual}");
    }

    #[test]
    fn slider_zero_range_keeps_value() {
        let atlas = make_test_atlas();
        let style = Style::default();

        let (state, mut slider) = Slider::create(SliderParameters::new(5.0, 5.0, 5.0));
        let rect = rect(0, 0, 100, 20);
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(50, 10),
            delta: vec2(5, 0),
            buttons: MouseButton::LEFT,
        }];
        let mut ctx = WidgetUpdateCtx::new_with_interaction(
            rect,
            rect,
            &style,
            &atlas,
            true,
            true,
            true,
            false,
            true,
            MouseButton::LEFT,
            KeyMode::NONE,
            KeyCode::NONE,
        );

        let event = localize_event(Vec2i::new(rect.x, rect.y), input.into_iter().next().unwrap());
        slider.update(&mut ctx, Some(&event));

        assert_eq!(state.try_read(|state| state.value().is_finite()), Some(true));
        assert_eq!(state.try_read(SliderState::value), Some(5.0));
        assert_eq!(state.try_update(SliderState::take_changed), Some(false));
    }

    #[test]
    fn slider_wheel_snaps_fractional_step_from_lower_bound() {
        let (state, mut slider) = Slider::create(SliderParameters::with_opt(1.15, 1.0, 2.0, 0.2, 2, WidgetOption::FRAME));
        run_slider_once(&mut slider, rect(0, 0, 100, 20), Vec::new(), true, false, false, Some(vec2(0, 1)));

        assert_real_close(state.try_read(SliderState::value).unwrap(), 1.4);
        assert_eq!(state.try_update(SliderState::take_changed), Some(true));
        assert_eq!(state.try_update(SliderState::take_changed), Some(false));
    }

    #[test]
    fn slider_drag_snaps_fractional_step_from_lower_bound() {
        let (state, mut slider) = Slider::create(SliderParameters::with_opt(10.0, 10.0, 20.0, 0.25, 2, WidgetOption::FRAME));
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(33, 10),
            delta: Vec2i::default(),
            buttons: MouseButton::LEFT,
        }];
        run_slider_once(&mut slider, rect(0, 0, 100, 20), input, true, true, true, None);

        assert_real_close(state.try_read(SliderState::value).unwrap(), 13.25);
        assert_eq!(state.try_update(SliderState::take_changed), Some(true));
    }

    #[test]
    fn slider_uses_widget_local_mouse_position() {
        let atlas = make_test_atlas();
        let style = Style::default();

        let (state, mut slider) = Slider::create(SliderParameters::new(0.0, 0.0, 100.0));
        let rect = rect(40, 20, 100, 20);
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(90, 30),
            delta: Vec2i::default(),
            buttons: MouseButton::LEFT,
        }];
        let mut ctx = WidgetUpdateCtx::new_with_interaction(
            rect,
            rect,
            &style,
            &atlas,
            true,
            true,
            true,
            false,
            true,
            MouseButton::LEFT,
            KeyMode::NONE,
            KeyCode::NONE,
        );

        let event = localize_event(Vec2i::new(rect.x, rect.y), input.into_iter().next().unwrap());
        slider.update(&mut ctx, Some(&event));

        assert_eq!(state.try_read(SliderState::value), Some(50.0));
        assert_eq!(state.try_update(SliderState::take_changed), Some(true));
    }

    #[test]
    fn number_drag_records_a_typed_change_and_programmatic_setter_is_silent() {
        let (state, mut number) = Number::create(NumberParameters::new(0.0, 2.0, 0));
        state.try_update(|state| state.set_value(4.0)).unwrap();
        assert_eq!(state.try_update(NumberState::take_changed), Some(false));

        run_number_once(
            &mut number,
            vec![UiInputEvent::MouseDrag {
                pos: vec2(10, 10),
                delta: vec2(3, 0),
                buttons: MouseButton::LEFT,
            }],
        );
        assert_eq!(state.try_read(NumberState::value), Some(10.0));
        assert_eq!(state.try_update(NumberState::take_changed), Some(true));
        assert_eq!(state.try_update(NumberState::take_changed), Some(false));
    }

    #[test]
    fn slider_programmatic_setter_is_silent() {
        let (state, _slider) = Slider::create(SliderParameters::new(0.0, -5.0, 5.0));
        state.try_update(|state| state.set_value(4.0)).unwrap();
        assert_eq!(state.try_read(SliderState::value), Some(4.0));
        assert_eq!(state.try_update(SliderState::take_changed), Some(false));
    }
}
