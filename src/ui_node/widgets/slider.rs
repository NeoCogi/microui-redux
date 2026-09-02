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
use std::{cell::RefCell, rc::Rc};

use super::numeric_edit::*;

/// One-shot construction input for a [`Slider`].
pub struct SliderParameters {
    /// Validated finite initial slider value.
    value: Real,
    /// Validated finite lower bound of the ascending slider range.
    low: Real,
    /// Validated finite upper bound of the ascending slider range.
    high: Real,
    /// Validated finite non-negative step used for snapping (zero for continuous).
    step: Real,
    /// Bounded number of digits after the decimal point when rendering.
    precision: DecimalPrecision,
    /// Font used for the numeric label and editor.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
}

impl crate::LeafWidget for Slider {
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
}

impl WidgetParameters for SliderParameters {}

impl SliderParameters {
    /// Creates slider parameters with default widget options.
    ///
    /// # Errors
    ///
    /// Returns [`NumericParameterError`] unless `value` and both range bounds are finite, the
    /// bounds satisfy `low <= high`, and their represented span remains finite.
    pub fn new(value: Real, low: Real, high: Real) -> Result<Self, NumericParameterError> {
        Self::with_opt(value, low, high, 0.0, DecimalPrecision::ZERO, WidgetOption::FRAME)
    }

    /// Creates slider parameters with explicit widget options.
    ///
    /// # Errors
    ///
    /// Returns [`NumericParameterError`] unless `value`, `low`, `high`, and `step` are finite,
    /// `low <= high`, the range span is representable, and `step` is non-negative. Precision is
    /// already bounded by construction through [`DecimalPrecision`].
    pub fn with_opt(value: Real, low: Real, high: Real, step: Real, precision: DecimalPrecision, opt: WidgetOption) -> Result<Self, NumericParameterError> {
        // Validate every arithmetic input before publishing the parameter value. Private fields
        // then make ascending orientation and finite percentage math builder invariants.
        validate_slider_range(low, high)?;
        validate_numeric_value_and_step(value, step)?;
        Ok(Self {
            value,
            low,
            high,
            step,
            precision,
            font: FontChoice::Role(FontRole::Body),
            opt: opt | WidgetOption::GRAB_SCROLL,
        })
    }

    /// Replaces the font used for the numeric label and editor.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Value snapshot emitted after a user-originated slider change.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SliderChanged {
    /// Slider value after applying the triggering input event.
    pub value: Real,
}

impl crate::WidgetEvent for SliderChanged {}

/// Concrete retained slider, including its semantic and editing state.
pub struct Slider {
    /// Initialization-only validated non-negative step size.
    step: Real,
    /// Initialization-only bounded display precision.
    precision: DecimalPrecision,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Current slider value.
    value: Real,
    /// Finite lower bound retained for immediate setter clamping.
    low: Real,
    /// Finite upper bound retained for immediate setter clamping.
    high: Real,
    /// Inline numeric editing state.
    edit: NumberEditState,
    /// Runtime-owned source for user-originated value changes.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<SliderChanged>>>,
}

impl Slider {
    /// Constructs a retained node and a weak typed handle to its concrete slider.
    pub fn create(parameters: SliderParameters) -> (TypedWidgetHandle<Self>, Node) {
        let widget = SliderBuilder::create_widget(parameters);
        Node::typed_widget(widget)
    }

    /// Returns the current slider value.
    pub fn value(&self) -> Real {
        self.value
    }

    /// Updates the current value, clamping finite input to the slider range.
    ///
    /// NaN and infinities are ignored so invalid external input cannot select an arbitrary endpoint
    /// or erase the last valid retained value.
    pub fn set_value(&mut self, value: Real) {
        if value.is_finite() {
            self.value = clamp_slider_value(value, self.low, self.high);
        }
    }

    /// Returns whether the inline numeric editor is active.
    pub fn is_editing(&self) -> bool {
        self.edit.editing
    }

    /// Returns the native event endpoint emitted after every user-originated value change.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<SliderChanged> {
        <Self as crate::TypedWidget<SliderChanged>>::event(self)
    }

    /// Measures the slider track plus formatted value label.
    fn preferred_size_widget(&self, style: &Skin, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        let thumb_size = style.metrics.thumb_size.max(0);
        number_preferred_size(style, atlas, self.font, self.value, self.precision, thumb_size, thumb_size)
    }

    /// Updates slider value from shift-click text entry, scroll, or pointer drag.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let base = ctx.local_rect();
        let font = ctx.skin().resolve_font_choice(self.font);
        let last = self.value;
        let mut value = last;
        if !self.edit.editing {
            // The validated ascending range gives all inputs one direction: Right increases and
            // Left decreases. Continuous sliders use one percent; stepped sliders use one step.
            let range = self.high - self.low;
            let amount = if self.step != 0.0 { self.step } else { range / 100.0 };
            match ctx.action(input, self.keyboard_behavior()) {
                Some(KeyboardAction::Decrease) => value -= amount,
                Some(KeyboardAction::Increase) => value += amount,
                Some(KeyboardAction::Activate | KeyboardAction::Expand | KeyboardAction::Collapse) | None => {}
            }
            if self.step != 0.0 {
                value = snap_slider_value(value, self.low, self.step);
            }
        }
        if number_textbox_update(ctx, input, &mut self.edit, self.precision, font, &mut value) {
            return;
        }
        if let Some(UiInputEvent::Scroll { delta, .. }) = input {
            let range = self.high - self.low;
            if range != 0.0 {
                let wheel = if delta.y != 0 { delta.y.signum() } else { delta.x.signum() };
                if wheel != 0 {
                    // Positive wheel direction follows Right Arrow and increasing pointer x.
                    let step_amount = if self.step != 0. { self.step } else { range / 100.0 };
                    value += wheel as Real * step_amount;
                    if self.step != 0. {
                        value = snap_slider_value(value, self.low, self.step);
                    }
                }
            }
        }
        let range = self.high - self.low;
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
            // Normalize screen space in f64, then interpolate from both endpoints. The endpoint
            // form avoids both multiplication overflow and loss of a small upper bound while
            // cancelling a much larger negative lower bound.
            let fraction = (f64::from(pointer_pos.x) / f64::from(base.width)).clamp(0.0, 1.0);
            value = slider_value_at_fraction(self.low, self.high, fraction);
            if self.step != 0. {
                value = snap_slider_value(value, self.low, self.step);
            }
        }
        if range == 0.0 {
            value = self.low;
        }
        value = clamp_slider_value(value, self.low, self.high);
        self.value = value;
        if last != value {
            self.changed_event.borrow_mut().emit(SliderChanged { value });
        }
    }

    /// Paints either the inline numeric editor or the slider track/thumb/value label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let font = ctx.skin().resolve_font_choice(self.font);
        if self.edit.editing {
            number_textbox_paint(ctx, &self.edit, font);
            return;
        }

        let base = ctx.local_rect();
        if !self.opt.intersects(WidgetOption::FRAME) {
            // Unframed sliders still retain the semantic track center without adding border space.
            ctx.draw_appearance_center(AppearanceRole::SliderTrack, base);
        }
        // Measurement already treats negative theme thumb sizes as zero. Paint applies the same
        // normalization and uses saturated extent arithmetic so either public style or layout
        // input may span the i32 domain without panicking.
        let width = ctx.skin().metrics.thumb_size.max(0);
        let available = base.width.max(0).saturating_sub(width);
        let x = if self.low != self.high && available > 0 {
            // Resolve the normalized numeric position in f64 before multiplying by pixel width.
            // Exact endpoints are handled explicitly by the helper, while all interior results
            // remain bounded in `[0, 1]` and cannot overflow the layout intermediate.
            let fraction = slider_value_fraction(self.value, self.low, self.high);
            (fraction * f64::from(available)) as i32
        } else {
            0
        };
        let thumb = rect(base.x.saturating_add(x), base.y, width, base.height.max(0));
        let _ = ctx.draw_appearance(AppearanceRole::SliderThumb, thumb);
        let label = number_label(self.value, self.precision);
        ctx.draw_control_text_with_font(font, label.as_str(), base, AppearanceRole::SliderTrack, self.opt);
    }
}

impl TypedWidgetHandle<Slider> {
    /// Returns the current slider value while the widget is retained.
    pub fn value(&self) -> Option<Real> {
        self.try_read(Slider::value)
    }

    /// Replaces the slider value without emitting a user event.
    ///
    /// Non-finite input is ignored, preserving the widget's last finite value.
    pub fn set_value(&self, value: Real) -> Option<()> {
        self.try_update(|widget| widget.set_value(value))
    }

    /// Returns whether the retained slider is currently editing text.
    pub fn is_editing(&self) -> Option<bool> {
        self.try_read(Slider::is_editing)
    }

    /// Returns the slider's native value-change endpoint.
    pub fn changed(&self) -> WidgetEventPortHandle<SliderChanged> {
        self.widget_event()
    }
}

impl crate::TypedWidget<SliderChanged> for Slider {
    fn event(&self) -> crate::WidgetEventPortHandle<SliderChanged> {
        crate::WidgetEventPortHandle::new(&self.changed_event)
    }
}

/// Interpolates one bounded slider fraction without overflowing or losing exact endpoints.
fn slider_value_at_fraction(low: Real, high: Real, fraction: f64) -> Real {
    // Pointer normalization supplies `[0, 1]`, but endpoint branches make this helper total and
    // preserve low/high exactly. The weighted endpoint form avoids constructing `high - low`,
    // whose later cancellation with a large `low` can discard a much smaller finite `high`.
    if fraction <= 0.0 {
        low
    } else if fraction >= 1.0 {
        high
    } else {
        ((1.0 - fraction) * f64::from(low) + fraction * f64::from(high)) as Real
    }
}

/// Converts one retained slider value to a bounded fraction of its ascending range.
fn slider_value_fraction(value: Real, low: Real, high: Real) -> f64 {
    // Exact endpoint checks prevent wide-range subtraction from moving either thumb endpoint.
    // Interior subtraction is widened first; every distinct finite f32 pair remains distinct in
    // f64, and the final clamp contains ordinary rounding at the range edges.
    if value <= low {
        0.0
    } else if value >= high {
        1.0
    } else {
        ((f64::from(value) - f64::from(low)) / (f64::from(high) - f64::from(low))).clamp(0.0, 1.0)
    }
}

/// Snaps a value to the nearest step relative to the lower bound.
fn snap_slider_value(value: Real, low: Real, step: Real) -> Real {
    // SliderParameters guarantees a finite non-negative step, so snapping never has to invent an
    // orientation by taking an absolute value. Widening the step count is still necessary: the
    // ratio of two finite f32 values can exceed f32 even when the reconstructed value is finite.
    if step == 0.0 {
        value
    } else {
        let steps = ((f64::from(value) - f64::from(low)) / f64::from(step)).round();
        (f64::from(low) + steps * f64::from(step)) as Real
    }
}

/// Clamps a slider value to its validated ascending range.
fn clamp_slider_value(value: Real, low: Real, high: Real) -> Real {
    // NaN has no direction and selects the lower bound. Signed infinities and finite overflow from
    // one interaction retain their direction through the ordinary endpoint comparisons.
    if value.is_nan() || value <= low {
        low
    } else if value >= high {
        high
    } else {
        value
    }
}

impl Widget for Slider {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn frame_appearance_role(&self) -> AppearanceRole {
        // Retained traversal paints the complete track before the slider paints its thumb and text.
        AppearanceRole::SliderTrack
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.update_widget(ctx, input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::TAB_STOP | KeyboardBehavior::ADJUST_HORIZONTAL
    }
}

/// Type-level constructor for [`Slider`].
pub struct SliderBuilder;

impl WidgetBuilder for SliderBuilder {
    type Parameters = SliderParameters;
    type W = Slider;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        // SliderParameters' private fields arrive finite, ordered, and directionally coherent;
        // only the initial value may need ordinary endpoint clamping.
        Slider {
            step: parameters.step,
            precision: parameters.precision,
            font: parameters.font,
            opt: parameters.opt,
            value: clamp_slider_value(parameters.value, parameters.low, parameters.high),
            low: parameters.low,
            high: parameters.high,
            edit: NumberEditState::default(),
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for slider and numeric editing behavior.

    use super::*;
    use crate::test_support::{test_atlas as make_test_atlas, test_skin};
    use crate::ui_node::{UiInputEvent, widget_context::localize_event};
    use crate::{Number, NumberBuilder, NumberParameters};

    fn run_slider_once(slider: &mut Slider, rect: Recti, events: Vec<UiInputEvent>, hovered: bool, focused: bool, active: bool, scroll: Option<Vec2i>) {
        let atlas = make_test_atlas();
        let style = test_skin(&atlas);
        let held = if active { MouseButton::LEFT } else { MouseButton::NONE };
        let mut events = events
            .into_iter()
            .map(|event| localize_event(Vec2i::new(rect.x, rect.y), event))
            .collect::<Vec<_>>();
        if let Some(delta) = scroll {
            events.push(UiInputEvent::Scroll { pos: Vec2i::default(), delta });
        }
        for event in &events {
            let mut ctx = WidgetUpdateCtx::new_with_interaction(rect, rect, &style, &atlas, true, hovered, focused, false, active, held, Modifiers::NONE);
            slider.update(&mut ctx, Some(event));
        }
    }

    fn run_number_once(number: &mut Number, events: Vec<UiInputEvent>) {
        let atlas = make_test_atlas();
        let style = test_skin(&atlas);
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
                Modifiers::NONE,
            );
            number.update(&mut ctx, Some(event));
        }
    }

    fn assert_real_close(actual: Real, expected: Real) {
        assert!((actual - expected).abs() < 1.0e-5, "expected {expected}, got {actual}");
    }

    /// Converts a small test literal through the same public precision validation as applications.
    fn precision(digits: usize) -> DecimalPrecision {
        DecimalPrecision::try_from(digits).expect("test precision must stay within Real's meaningful bound")
    }

    /// Extracts a Number construction failure without requiring the intentionally opaque
    /// `NumberParameters` success type to implement Debug.
    fn number_parameter_error(result: Result<NumberParameters, NumericParameterError>) -> NumericParameterError {
        match result {
            Err(error) => error,
            Ok(_) => panic!("invalid Number parameters unexpectedly validated"),
        }
    }

    /// Extracts a Slider construction failure without exposing its private parameter fields.
    fn slider_parameter_error(result: Result<SliderParameters, NumericParameterError>) -> NumericParameterError {
        match result {
            Err(error) => error,
            Ok(_) => panic!("invalid Slider parameters unexpectedly validated"),
        }
    }

    fn record_slider_change(values: &mut Vec<Real>, event: &SliderChanged) {
        values.push(event.value);
    }

    fn record_number_change(values: &mut Vec<Real>, event: &NumberChanged) {
        values.push(event.value);
    }

    fn slider_dispatcher(slider: &Slider) -> crate::event::WidgetEventDispatcher<Vec<Real>> {
        let mut dispatcher = crate::event::WidgetEventDispatcher::new();
        dispatcher.subscribe(slider.changed(), record_slider_change).unwrap();
        dispatcher
    }

    fn number_dispatcher(number: &Number) -> crate::event::WidgetEventDispatcher<Vec<Real>> {
        let mut dispatcher = crate::event::WidgetEventDispatcher::new();
        dispatcher.subscribe(number.changed(), record_number_change).unwrap();
        dispatcher
    }

    /// Verifies formatting precision is a small validated value rather than an allocation-sized
    /// application integer.
    #[test]
    fn decimal_precision_rejects_huge_format_requests() {
        assert_eq!(DecimalPrecision::MAX.digits(), Real::DIGITS as usize);
        assert_eq!(DecimalPrecision::try_from(DecimalPrecision::MAX.digits()), Ok(DecimalPrecision::MAX));

        let error = DecimalPrecision::try_from(usize::MAX).expect_err("usize::MAX precision must be rejected before formatting");
        assert_eq!(error.requested(), usize::MAX);
        assert_eq!(error.maximum(), DecimalPrecision::MAX.digits());

        // The accepted maximum remains a fixed small allocation even for the longest permitted
        // fractional suffix.
        let label = number_label(1.0, DecimalPrecision::MAX);
        assert_eq!(label, format!("1.{:0<width$}", "", width = DecimalPrecision::MAX.digits()));
    }

    /// Verifies Number construction rejects every non-finite scalar and does not reinterpret a
    /// negative step as an opposite interaction direction.
    #[test]
    fn number_parameters_reject_non_finite_values_and_invalid_steps() {
        for value in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            let error = number_parameter_error(NumberParameters::new(value, 1.0, DecimalPrecision::ZERO));
            assert!(matches!(error, NumericParameterError::NonFiniteValue { value: rejected } if rejected.to_bits() == value.to_bits()));
        }

        for step in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            let error = number_parameter_error(NumberParameters::new(0.0, step, DecimalPrecision::ZERO));
            assert!(matches!(error, NumericParameterError::NonFiniteStep { step: rejected } if rejected.to_bits() == step.to_bits()));
        }

        let error = number_parameter_error(NumberParameters::new(0.0, -1.0, DecimalPrecision::ZERO));
        assert!(matches!(error, NumericParameterError::NegativeStep { step } if step == -1.0));
        assert!(NumberParameters::new(0.0, 0.0, DecimalPrecision::ZERO).is_ok());
    }

    /// Verifies Slider construction admits only finite ascending ranges with finite non-negative
    /// steps, eliminating the former conflicting descending orientation.
    #[test]
    fn slider_parameters_reject_non_finite_and_descending_ranges() {
        for value in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            let error = slider_parameter_error(SliderParameters::new(value, 0.0, 1.0));
            assert!(matches!(error, NumericParameterError::NonFiniteValue { value: rejected } if rejected.to_bits() == value.to_bits()));
        }

        for (low, high) in [(Real::NAN, 1.0), (0.0, Real::INFINITY), (Real::NEG_INFINITY, 1.0)] {
            let error = slider_parameter_error(SliderParameters::new(0.0, low, high));
            assert!(matches!(error, NumericParameterError::NonFiniteRangeBounds { .. }));
        }

        let descending = slider_parameter_error(SliderParameters::new(5.0, 10.0, 0.0));
        assert!(matches!(descending, NumericParameterError::DescendingRange { low: 10.0, high: 0.0 }));

        let excessive_span = slider_parameter_error(SliderParameters::new(0.0, -Real::MAX, Real::MAX));
        assert!(matches!(excessive_span, NumericParameterError::NonFiniteRangeSpan { .. }));

        for step in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            let error = slider_parameter_error(SliderParameters::with_opt(0.0, 0.0, 1.0, step, DecimalPrecision::ZERO, WidgetOption::FRAME));
            assert!(matches!(error, NumericParameterError::NonFiniteStep { step: rejected } if rejected.to_bits() == step.to_bits()));
        }
        let negative_step = slider_parameter_error(SliderParameters::with_opt(0.0, 0.0, 1.0, -0.5, DecimalPrecision::ZERO, WidgetOption::FRAME));
        assert!(matches!(negative_step, NumericParameterError::NegativeStep { step } if step == -0.5));
    }

    /// Verifies every ascending-slider input maps Left Arrow, negative wheel, and a lower pointer
    /// coordinate to decrease, with their three opposites consistently increasing.
    #[test]
    fn ascending_slider_inputs_agree_on_direction() {
        let parameters = || {
            SliderParameters::with_opt(5.0, 0.0, 10.0, 1.0, DecimalPrecision::ZERO, WidgetOption::FRAME)
                .expect("finite ascending direction fixture must validate")
        };
        let bounds = rect(0, 0, 100, 20);

        let mut keyboard_increase = SliderBuilder::create_widget(parameters());
        run_slider_once(
            &mut keyboard_increase,
            bounds,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::ArrowRight, Modifiers::NONE),
            }],
            false,
            true,
            false,
            None,
        );
        let mut wheel_increase = SliderBuilder::create_widget(parameters());
        run_slider_once(&mut wheel_increase, bounds, Vec::new(), true, false, false, Some(vec2(0, 1)));
        let mut pointer_increase = SliderBuilder::create_widget(parameters());
        run_slider_once(
            &mut pointer_increase,
            bounds,
            vec![UiInputEvent::MouseDrag {
                pos: vec2(60, 10),
                delta: Vec2i::default(),
                buttons: MouseButton::LEFT,
            }],
            true,
            true,
            true,
            None,
        );
        assert_eq!([keyboard_increase.value(), wheel_increase.value(), pointer_increase.value()], [6.0; 3]);

        let mut keyboard_decrease = SliderBuilder::create_widget(parameters());
        run_slider_once(
            &mut keyboard_decrease,
            bounds,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::ArrowLeft, Modifiers::NONE),
            }],
            false,
            true,
            false,
            None,
        );
        let mut wheel_decrease = SliderBuilder::create_widget(parameters());
        run_slider_once(&mut wheel_decrease, bounds, Vec::new(), true, false, false, Some(vec2(0, -1)));
        let mut pointer_decrease = SliderBuilder::create_widget(parameters());
        run_slider_once(
            &mut pointer_decrease,
            bounds,
            vec![UiInputEvent::MouseDrag {
                pos: vec2(40, 10),
                delta: Vec2i::default(),
                buttons: MouseButton::LEFT,
            }],
            true,
            true,
            true,
            None,
        );
        assert_eq!([keyboard_decrease.value(), wheel_decrease.value(), pointer_decrease.value()], [4.0; 3]);
    }

    #[test]
    fn slider_zero_range_keeps_value() {
        let atlas = make_test_atlas();
        let style = test_skin(&atlas);

        let mut slider = SliderBuilder::create_widget(SliderParameters::new(5.0, 5.0, 5.0).expect("equal finite bounds form a valid inert slider"));
        let rect = rect(0, 0, 100, 20);
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(50, 10),
            delta: vec2(5, 0),
            buttons: MouseButton::LEFT,
        }];
        let mut ctx = WidgetUpdateCtx::new_with_interaction(rect, rect, &style, &atlas, true, true, true, false, true, MouseButton::LEFT, Modifiers::NONE);

        let event = localize_event(Vec2i::new(rect.x, rect.y), input.into_iter().next().unwrap());
        slider.update(&mut ctx, Some(&event));

        assert!(slider.value().is_finite());
        assert_eq!(slider.value(), 5.0);
    }

    #[test]
    fn slider_wheel_snaps_fractional_step_from_lower_bound() {
        let mut slider = SliderBuilder::create_widget(
            SliderParameters::with_opt(1.15, 1.0, 2.0, 0.2, precision(2), WidgetOption::FRAME)
                .expect("finite ascending fractional slider parameters must validate"),
        );
        let mut dispatcher = slider_dispatcher(&slider);
        run_slider_once(&mut slider, rect(0, 0, 100, 20), Vec::new(), true, false, false, Some(vec2(0, 1)));

        assert_real_close(slider.value(), 1.4);
        let mut values = Vec::new();
        assert!(dispatcher.dispatch(&mut values));
        assert_eq!(values, [1.4]);
    }

    #[test]
    fn slider_drag_snaps_fractional_step_from_lower_bound() {
        let mut slider = SliderBuilder::create_widget(
            SliderParameters::with_opt(10.0, 10.0, 20.0, 0.25, precision(2), WidgetOption::FRAME)
                .expect("finite ascending fractional slider parameters must validate"),
        );
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(33, 10),
            delta: Vec2i::default(),
            buttons: MouseButton::LEFT,
        }];
        run_slider_once(&mut slider, rect(0, 0, 100, 20), input, true, true, true, None);

        assert_real_close(slider.value(), 13.25);
    }

    #[test]
    fn slider_uses_widget_local_mouse_position() {
        let atlas = make_test_atlas();
        let style = test_skin(&atlas);

        let mut slider = SliderBuilder::create_widget(SliderParameters::new(0.0, 0.0, 100.0).expect("finite ascending slider parameters must validate"));
        let rect = rect(40, 20, 100, 20);
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(90, 30),
            delta: Vec2i::default(),
            buttons: MouseButton::LEFT,
        }];
        let mut ctx = WidgetUpdateCtx::new_with_interaction(rect, rect, &style, &atlas, true, true, true, false, true, MouseButton::LEFT, Modifiers::NONE);

        let event = localize_event(Vec2i::new(rect.x, rect.y), input.into_iter().next().unwrap());
        slider.update(&mut ctx, Some(&event));

        assert_eq!(slider.value(), 50.0);
    }

    /// Verifies pointer mapping normalizes screen space before scaling a valid maximum-size range.
    #[test]
    fn slider_pointer_midpoint_does_not_overflow_a_large_finite_range() {
        let mut slider = SliderBuilder::create_widget(SliderParameters::new(0.0, 0.0, Real::MAX).expect("a maximum finite range span must remain usable"));
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(50, 10),
            delta: Vec2i::default(),
            buttons: MouseButton::LEFT,
        }];

        run_slider_once(&mut slider, rect(0, 0, 100, 20), input, true, true, true, None);

        assert_eq!(slider.value(), Real::MAX / 2.0);
    }

    /// Verifies the right edge selects a small upper bound exactly after a wide cancellation.
    #[test]
    fn slider_pointer_right_edge_preserves_a_small_upper_endpoint() {
        let low = Real::MIN / 2.0;
        let high = 1.0;
        let mut slider = SliderBuilder::create_widget(SliderParameters::new(low, low, high).expect("a finite wide-offset range must remain usable"));
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(100, 10),
            delta: Vec2i::default(),
            buttons: MouseButton::LEFT,
        }];

        run_slider_once(&mut slider, rect(0, 0, 100, 20), input, true, true, true, None);

        assert_eq!(slider.value(), high);
    }

    /// Verifies a finite subnormal step cannot overflow the intermediate snapping step count.
    #[test]
    fn slider_tiny_step_keeps_a_representable_pointer_value() {
        let minimum_step = Real::from_bits(1);
        let mut slider = SliderBuilder::create_widget(
            SliderParameters::with_opt(0.0, 0.0, 1.0, minimum_step, DecimalPrecision::ZERO, WidgetOption::FRAME)
                .expect("the smallest positive finite step must remain usable"),
        );
        let input = vec![UiInputEvent::MouseDrag {
            pos: vec2(50, 10),
            delta: Vec2i::default(),
            buttons: MouseButton::LEFT,
        }];

        run_slider_once(&mut slider, rect(0, 0, 100, 20), input, true, true, true, None);

        assert_eq!(slider.value(), 0.5);
    }

    /// Verifies paint normalizes numeric space before mapping a large finite midpoint to pixels.
    #[test]
    fn slider_thumb_midpoint_does_not_overflow_a_large_finite_range() {
        let atlas = make_test_atlas();
        let mut style = test_skin(&atlas);
        // A borderless ten-pixel thumb records one unambiguous fill rectangle at the computed x.
        style.metrics.thumb_size = 10;
        let frame = style
            .appearance(crate::AppearanceRole::SliderThumb, crate::VisualState::Normal)
            .with_insets(crate::SliceInsets::ZERO);
        style.visuals.set_patches(crate::AppearanceRole::SliderThumb, crate::StateTable::filled(frame));
        let mut slider =
            SliderBuilder::create_widget(SliderParameters::new(Real::MAX / 2.0, 0.0, Real::MAX).expect("a maximum finite range span must remain usable"));
        let bounds = rect(0, 0, 100, 20);
        let mut display_list = crate::render::DisplayList::new();
        let mut ctx = WidgetPaintCtx::new_with_content_geometry(bounds, &mut display_list, bounds, &style, &atlas, true, false, false, false, false, true);

        slider.paint(&mut ctx);

        let fills = display_list.debug_fill_rects();
        assert!(
            fills
                .iter()
                .any(|(fill, _, _)| fill.x == 45 && fill.y == 0 && fill.width == 10 && fill.height == 20),
            "the half-range value must place the ten-pixel thumb halfway across ninety available pixels",
        );
    }

    #[test]
    fn number_drag_records_a_typed_change_and_programmatic_setter_is_silent() {
        let mut number =
            NumberBuilder::create_widget(NumberParameters::new(0.0, 2.0, DecimalPrecision::ZERO).expect("finite non-negative number parameters must validate"));
        let mut dispatcher = number_dispatcher(&number);
        let mut values = Vec::new();
        number.set_value(4.0);
        assert!(!dispatcher.dispatch(&mut values));

        run_number_once(
            &mut number,
            vec![UiInputEvent::MouseDrag {
                pos: vec2(10, 10),
                delta: vec2(3, 0),
                buttons: MouseButton::LEFT,
            }],
        );
        assert_eq!(number.value(), 10.0);
        assert!(dispatcher.dispatch(&mut values));
        assert_eq!(values, [10.0]);
    }

    /// Verifies wide drag arithmetic preserves representable cancellation in both directions.
    #[test]
    fn number_drag_does_not_overflow_before_a_representable_result() {
        let mut increasing = NumberBuilder::create_widget(
            NumberParameters::new(Real::MIN, Real::MAX, DecimalPrecision::ZERO).expect("finite extreme number parameters must validate"),
        );
        run_number_once(
            &mut increasing,
            vec![UiInputEvent::MouseDrag {
                pos: vec2(10, 10),
                delta: vec2(2, 0),
                buttons: MouseButton::LEFT,
            }],
        );
        assert_eq!(increasing.value(), Real::MAX);

        let mut decreasing = NumberBuilder::create_widget(
            NumberParameters::new(Real::MAX, Real::MAX, DecimalPrecision::ZERO).expect("finite extreme number parameters must validate"),
        );
        run_number_once(
            &mut decreasing,
            vec![UiInputEvent::MouseDrag {
                pos: vec2(10, 10),
                delta: vec2(-2, 0),
                buttons: MouseButton::LEFT,
            }],
        );
        assert_eq!(decreasing.value(), Real::MIN);
    }

    /// Verifies arrow arithmetic saturates at the finite Real endpoints instead of jumping to zero.
    #[test]
    fn number_arrow_overflow_saturates_in_the_requested_direction() {
        let mut increasing = NumberBuilder::create_widget(
            NumberParameters::new(Real::MAX, Real::MAX, DecimalPrecision::ZERO).expect("finite extreme number parameters must validate"),
        );
        run_number_once(
            &mut increasing,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::ArrowUp, Modifiers::NONE),
            }],
        );
        assert_eq!(increasing.value(), Real::MAX);

        let mut decreasing = NumberBuilder::create_widget(
            NumberParameters::new(Real::MIN, Real::MAX, DecimalPrecision::ZERO).expect("finite extreme number parameters must validate"),
        );
        run_number_once(
            &mut decreasing,
            vec![UiInputEvent::Key {
                event: KeyEvent::pressed(Key::ArrowDown, Modifiers::NONE),
            }],
        );
        assert_eq!(decreasing.value(), Real::MIN);
    }

    #[test]
    fn slider_programmatic_setter_is_silent() {
        let mut slider = SliderBuilder::create_widget(SliderParameters::new(0.0, -5.0, 5.0).expect("finite ascending slider parameters must validate"));
        let mut dispatcher = slider_dispatcher(&slider);
        slider.set_value(4.0);
        assert_eq!(slider.value(), 4.0);
        assert!(!dispatcher.dispatch(&mut Vec::new()));
    }

    /// Verifies both numeric setters retain their last finite value when given invalid input.
    #[test]
    fn numeric_programmatic_setters_ignore_non_finite_values() {
        let mut number = NumberBuilder::create_widget(NumberParameters::new(3.0, 1.0, DecimalPrecision::ZERO).expect("finite number parameters must validate"));
        let mut slider = SliderBuilder::create_widget(SliderParameters::new(4.0, 0.0, 10.0).expect("finite slider parameters must validate"));

        for invalid in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            number.set_value(invalid);
            slider.set_value(invalid);
            assert_eq!(number.value(), 3.0);
            assert_eq!(slider.value(), 4.0);
        }
    }

    /// Verifies paint shares measurement's negative-thumb normalization without subtract overflow.
    #[test]
    fn negative_extreme_thumb_size_keeps_slider_paint_total() {
        let atlas = make_test_atlas();
        let mut style = test_skin(&atlas);
        style.metrics.thumb_size = i32::MIN;
        let mut slider = SliderBuilder::create_widget(SliderParameters::new(0.5, 0.0, 1.0).expect("finite ascending slider parameters must validate"));
        let bounds = rect(0, 0, 100, 20);
        let mut display_list = crate::render::DisplayList::new();
        let mut ctx = WidgetPaintCtx::new_with_content_geometry(bounds, &mut display_list, bounds, &style, &atlas, true, false, false, false, false, true);

        slider.paint(&mut ctx);

        // The base fill and numeric label remain valid even though the zero-width thumb is omitted.
        assert!(display_list.debug_operation_count() >= 1);
    }
}
