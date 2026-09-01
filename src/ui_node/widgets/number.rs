//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

//! Retained numeric-entry widget.

use crate::*;
use std::{cell::RefCell, rc::Rc};

use super::numeric_edit::*;

/// One-shot construction input for a [`Number`].
pub struct NumberParameters {
    /// Validated finite initial number value.
    value: Real,
    /// Validated finite non-negative step applied by dragging and arrow keys.
    step: Real,
    /// Bounded number of digits after the decimal point when rendering.
    precision: DecimalPrecision,
    /// Font used for the numeric label and editor.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
}

impl crate::LeafWidget for Number {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
}

impl WidgetParameters for NumberParameters {}

impl NumberParameters {
    /// Creates number parameters with default widget options.
    ///
    /// # Errors
    ///
    /// Returns [`NumericParameterError`] when `value` is not finite or `step` is not finite and
    /// non-negative. Precision is already valid by construction through [`DecimalPrecision`].
    pub fn new(value: Real, step: Real, precision: DecimalPrecision) -> Result<Self, NumericParameterError> {
        Self::with_opt(value, step, precision, WidgetOption::FRAME)
    }

    /// Creates number parameters with explicit widget options.
    ///
    /// # Errors
    ///
    /// Returns [`NumericParameterError`] when `value` is not finite or `step` is not finite and
    /// non-negative. Failed construction publishes no partially normalized parameter value.
    pub fn with_opt(value: Real, step: Real, precision: DecimalPrecision, opt: WidgetOption) -> Result<Self, NumericParameterError> {
        // The numeric fields stay private, so this one gate establishes the invariant consumed by
        // NumberBuilder and every later update/paint path.
        validate_numeric_value_and_step(value, step)?;
        Ok(Self {
            value,
            step,
            precision,
            font: FontChoice::Role(FontRole::Body),
            opt,
        })
    }

    /// Replaces the font used for the numeric label and editor.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Value snapshot emitted after a user-originated number change.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct NumberChanged {
    /// Number value after applying the triggering input event.
    pub value: Real,
}

impl crate::WidgetEvent for NumberChanged {}

/// Concrete retained number input, including its semantic and editing state.
pub struct Number {
    /// Initialization-only validated drag and arrow-key step.
    step: Real,
    /// Initialization-only bounded display precision.
    precision: DecimalPrecision,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Current number value.
    value: Real,
    /// Text editing state for shift-click numeric entry.
    edit: NumberEditState,
    /// Runtime-owned source for user-originated value changes.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<NumberChanged>>>,
}

impl Number {
    /// Constructs a retained node and a weak typed handle to its concrete number input.
    pub fn create(parameters: NumberParameters) -> (TypedWidgetHandle<Self>, Node) {
        let widget = NumberBuilder::create_widget(parameters);
        Node::typed_widget(widget)
    }

    /// Returns the current number value.
    pub fn value(&self) -> Real {
        self.value
    }

    /// Updates the current number value when `value` is finite.
    ///
    /// NaN and infinities are ignored so invalid external input cannot erase the last valid
    /// retained value. User interaction uses the same finite-state invariant.
    pub fn set_value(&mut self, value: Real) {
        if value.is_finite() {
            self.value = value;
        }
    }

    /// Returns whether the inline numeric editor is active.
    pub fn is_editing(&self) -> bool {
        self.edit.editing
    }

    /// Returns the native event endpoint emitted after every user-originated value change.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<NumberChanged> {
        <Self as crate::TypedWidget<NumberChanged>>::event(self)
    }

    /// Measures the formatted number label.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        number_preferred_size(style, atlas, self.font, self.value, self.precision, 0, 0)
    }

    /// Updates number value from shift-click text entry or horizontal drag.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let font = ctx.style().resolve_font_choice(self.font);
        let last = self.value;
        if !self.edit.editing {
            // Construction guarantees one non-negative direction policy: Up increases and Down
            // decreases by the same configured amount used by horizontal dragging.
            let amount = f64::from(self.step);
            match ctx.action(input, self.keyboard_behavior()) {
                Some(KeyboardAction::Decrease) => self.value = add_number_delta(self.value, -amount),
                Some(KeyboardAction::Increase) => self.value = add_number_delta(self.value, amount),
                Some(KeyboardAction::Activate | KeyboardAction::Expand | KeyboardAction::Collapse) | None => {}
            }
        }
        if !number_textbox_update(ctx, input, &mut self.edit, self.precision, font, &mut self.value)
            && ctx.focused()
            && ctx.mouse_buttons().intersects(MouseButton::LEFT)
            && let Some(UiInputEvent::MouseDrag { delta, .. }) = input
        {
            // Widen both multiplication and addition. An i32 drag multiplied by a finite f32 step
            // always fits f64, including cancellation cases whose correct result fits Real even
            // though the intermediate f32 product would be infinite.
            let drag_delta = f64::from(delta.x) * f64::from(self.step);
            self.value = add_number_delta(self.value, drag_delta);
        }
        let changed = (self.value != last).then_some(NumberChanged { value: self.value });
        if let Some(event) = changed {
            self.changed_event.borrow_mut().emit(event);
        }
    }

    /// Paints either the inline numeric editor or the formatted value.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.font);
        if self.edit.editing {
            number_textbox_paint(ctx, &self.edit, font);
            return;
        }

        let base = ctx.local_rect();
        ctx.draw_widget_fill(base, ControlColor::Base);
        let label = number_label(self.value, self.precision);
        ctx.draw_control_text_with_font(font, label.as_str(), base, ControlColor::Text, self.opt);
    }
}

impl TypedWidgetHandle<Number> {
    /// Returns the current number value while the widget is retained.
    pub fn value(&self) -> Option<Real> {
        self.try_read(Number::value)
    }

    /// Replaces the number value without emitting a user event.
    ///
    /// Non-finite input is ignored, preserving the widget's last finite value.
    pub fn set_value(&self, value: Real) -> Option<()> {
        self.try_update(|widget| widget.set_value(value))
    }

    /// Returns whether the retained number is currently editing text.
    pub fn is_editing(&self) -> Option<bool> {
        self.try_read(Number::is_editing)
    }

    /// Returns the number input's native value-change endpoint.
    pub fn changed(&self) -> WidgetEventPortHandle<NumberChanged> {
        self.widget_event()
    }
}

impl crate::TypedWidget<NumberChanged> for Number {
    fn event(&self) -> crate::WidgetEventPortHandle<NumberChanged> {
        crate::WidgetEventPortHandle::new(&self.changed_event)
    }
}

/// Adds one interaction delta in a wider domain and narrows with directional saturation.
fn add_number_delta(value: Real, delta: f64) -> Real {
    // Parameters and retained state are finite f32 values, while pointer deltas are i32. Their
    // complete product and sum fit f64, so clamping there prevents valid cancellation from being
    // lost to an infinite f32 intermediate and keeps overflow at the corresponding finite limit.
    let result = f64::from(value) + delta;
    result.clamp(f64::from(Real::MIN), f64::from(Real::MAX)) as Real
}

impl Widget for Number {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.update_widget(ctx, input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::TAB_STOP | KeyboardBehavior::ADJUST_VERTICAL
    }
}

/// Type-level constructor for [`Number`].
pub struct NumberBuilder;

impl WidgetBuilder for NumberBuilder {
    type Parameters = NumberParameters;
    type W = Number;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        // NumberParameters' private numeric fields can only be produced by its fallible validated
        // constructors, so the runtime starts finite without a second repair policy.
        Number {
            step: parameters.step,
            precision: parameters.precision,
            font: parameters.font,
            opt: parameters.opt,
            value: parameters.value,
            edit: NumberEditState::default(),
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}
