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

impl crate::LeafWidget for Number {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
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

/// Value snapshot emitted after a user-originated number change.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct NumberChanged {
    /// Number value after applying the triggering input event.
    pub value: Real,
}

impl crate::WidgetEvent for NumberChanged {}

/// Concrete retained number input, including its semantic and editing state.
pub struct Number {
    /// Initialization-only drag step.
    step: Real,
    /// Initialization-only display precision.
    precision: usize,
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

    /// Updates the current number value, replacing non-finite input with zero.
    pub fn set_value(&mut self, value: Real) {
        self.value = if value.is_finite() { value } else { 0.0 };
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
        if !number_textbox_update(ctx, input, &mut self.edit, self.precision, font, &mut self.value) {
            if ctx.focused()
                && ctx.mouse_buttons().intersects(MouseButton::LEFT)
                && let Some(UiInputEvent::MouseDrag { delta, .. }) = input
            {
                self.set_value(self.value + delta.x as Real * self.step);
            } else {
                self.set_value(self.value);
            }
        } else {
            self.set_value(self.value);
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
        KeyboardBehavior::TAB_STOP
    }
}

/// Type-level constructor for [`Number`].
pub struct NumberBuilder;

impl WidgetBuilder for NumberBuilder {
    type Parameters = NumberParameters;
    type W = Number;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Number {
            step: parameters.step,
            precision: parameters.precision,
            font: parameters.font,
            opt: parameters.opt,
            value: if parameters.value.is_finite() { parameters.value } else { 0.0 },
            edit: NumberEditState::default(),
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}
