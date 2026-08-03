//! Retained numeric-entry widget.

use crate::ui_node::{runtime_read_state, runtime_update_state};
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
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let font = ctx.style().resolve_font_choice(self.font);
        runtime_update_state(&self.state, "Number::update", |state| {
            let last = state.value;
            if !number_textbox_update(ctx, input, &mut state.edit, self.precision, font, &mut state.value) {
                if ctx.focused()
                    && ctx.mouse_buttons().intersects(MouseButton::LEFT)
                    && let Some(UiInputEvent::MouseDrag { delta, .. }) = input
                {
                    state.set_value(state.value + delta.x as Real * self.step);
                } else {
                    state.set_value(state.value);
                }
            } else {
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

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.update_widget(ctx, input)
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
