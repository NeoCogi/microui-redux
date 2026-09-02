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

//! Checkbox widget state and rendering.
//!
//! The checkbox toggles persistent boolean state on pointer or keyboard activation and paints the
//! atlas check icon when selected.

use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::{WidgetBuilder, WidgetParameters};

/// One-shot construction input for a [`Checkbox`].
///
/// The label, font, and widget options configure the retained runtime. The checked value seeds
/// The checked value remains application-mutable through [`TypedWidgetHandle<Checkbox>`].
pub struct CheckboxParameters {
    /// Label displayed beside the checkbox square.
    pub label: String,
    /// Initial checked value.
    pub checked: bool,
    /// Font used to measure and paint the label.
    pub font: FontChoice,
    /// Base widget options used for interaction and painting.
    pub opt: WidgetOption,
}

impl crate::LeafWidget for Checkbox {
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        self.preferred_size(style, atlas)
    }
}

impl WidgetParameters for CheckboxParameters {}

impl CheckboxParameters {
    /// Creates checkbox parameters with the body font and no special widget options.
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates checkbox parameters with explicit widget options.
    pub fn with_opt(label: impl Into<String>, checked: bool, opt: WidgetOption) -> Self {
        Self { opt, ..Self::new(label, checked) }
    }

    /// Replaces the label font used by the retained runtime.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Value snapshot emitted after a user-originated checkbox change.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CheckboxChanged {
    /// Checked value after applying the triggering user activation.
    pub checked: bool,
}

impl crate::WidgetEvent for CheckboxChanged {}

/// Concrete retained checkbox, including its semantic and interaction state.
pub struct Checkbox {
    /// Label displayed beside the checkbox square.
    label: String,
    /// Font used for the label.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Current checked value.
    checked: bool,
    /// Runtime-owned source for user-originated value changes.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<CheckboxChanged>>>,
}

impl Checkbox {
    /// Constructs a retained node and a weak typed handle to its concrete checkbox.
    pub fn create(parameters: CheckboxParameters) -> (crate::TypedWidgetHandle<Self>, crate::Node) {
        let widget = CheckboxBuilder::create_widget(parameters);
        crate::Node::typed_widget(widget)
    }

    /// Marks the checkbox as checked without recording user interaction.
    pub fn check(&mut self) {
        self.checked = true;
    }

    /// Marks the checkbox as unchecked without recording user interaction.
    pub fn uncheck(&mut self) {
        self.checked = false;
    }

    /// Replaces the checked value without recording user interaction.
    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    /// Returns the current checked value.
    pub const fn checked(&self) -> bool {
        self.checked
    }

    /// Returns the native event endpoint emitted after every user-originated value change.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<CheckboxChanged> {
        <Self as crate::TypedWidget<CheckboxChanged>>::event(self)
    }

    /// Measures the checkbox square plus optional label.
    fn preferred_size(&self, style: &Skin, atlas: &AtlasHandle) -> Dimensioni {
        let padding = style.metrics.padding.max(0);
        let check_icon = atlas.get_icon_size(style.resources.icons.check);
        let height = content_height(style, atlas, self.font, check_icon.height);
        let mut width = padding.saturating_mul(2).saturating_add(height.max(0));
        if !self.label.is_empty() {
            width = width
                .saturating_add(text_size(style, atlas, self.font, &self.label).width.max(0))
                .saturating_add(padding);
        }
        Dimensioni::new(width.max(0), height)
    }

    /// Paints the checkbox square, checked icon, and label.
    fn paint_widget(&self, checked: bool, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        let box_rect = rect(bounds.x, bounds.y, bounds.height, bounds.height);
        let role = if checked { AppearanceRole::CheckboxChecked } else { AppearanceRole::Checkbox };
        let box_content = ctx.draw_appearance(role, box_rect);
        if checked {
            let color = ctx.foreground(role);
            if let Some(box_content) = box_content {
                ctx.draw_icon(ctx.skin().resources.icons.check, box_content, color);
            }
        }
        let text_rect = rect(bounds.x + box_rect.width, bounds.y, bounds.width - box_rect.width, bounds.height);
        if !self.label.is_empty() {
            let font = ctx.skin().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, &self.label, text_rect, role, self.opt);
        }
    }
}

impl crate::TypedWidgetHandle<Checkbox> {
    /// Returns the current checked value while the widget is retained.
    pub fn checked(&self) -> Option<bool> {
        self.try_read(Checkbox::checked)
    }

    /// Marks the retained checkbox as checked.
    pub fn check(&self) -> Option<()> {
        self.try_update(Checkbox::check)
    }

    /// Marks the retained checkbox as unchecked.
    pub fn uncheck(&self) -> Option<()> {
        self.try_update(Checkbox::uncheck)
    }

    /// Replaces the retained checkbox value without emitting a user event.
    pub fn set_checked(&self, checked: bool) -> Option<()> {
        self.try_update(|widget| widget.set_checked(checked))
    }

    /// Returns the checkbox's native value-change endpoint.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<CheckboxChanged> {
        self.widget_event()
    }
}

impl Widget for Checkbox {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        if ctx.action(input, self.keyboard_behavior()) != Some(KeyboardAction::Activate) {
            return;
        }

        // Windows-style checkboxes toggle with Space; Enter remains available to a default button.
        self.checked = !self.checked;
        let checked = self.checked;
        self.changed_event.borrow_mut().emit(CheckboxChanged { checked });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(self.checked, ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::TAB_STOP | KeyboardBehavior::ACTIVATE_SPACE
    }
}

impl crate::TypedWidget<CheckboxChanged> for Checkbox {
    fn event(&self) -> crate::WidgetEventPortHandle<CheckboxChanged> {
        crate::WidgetEventPortHandle::new(&self.changed_event)
    }
}

/// Builder associating checkbox parameters with the concrete [`Checkbox`] runtime.
pub struct CheckboxBuilder;

impl WidgetBuilder for CheckboxBuilder {
    type Parameters = CheckboxParameters;
    type W = Checkbox;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Checkbox {
            label: parameters.label,
            font: parameters.font,
            opt: parameters.opt,
            checked: parameters.checked,
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}
