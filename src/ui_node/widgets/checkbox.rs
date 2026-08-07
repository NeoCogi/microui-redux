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
//! The checkbox toggles persistent boolean state on click and paints the atlas check icon when
//! selected.

use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::ui_node::{runtime_read_state, runtime_update_state};
use crate::{WidgetBuilder, WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner};

/// One-shot construction input for a [`Checkbox`].
///
/// The label, font, and widget options configure the retained runtime. The checked value seeds
/// [`CheckboxState`], which remains application-mutable after construction.
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

/// Application-facing persistent checkbox state.
pub struct CheckboxState {
    /// Current checked value.
    checked: bool,
    /// User-originated value changes waiting to be consumed.
    pending_changes: u32,
}

impl WidgetState for CheckboxState {}

impl CheckboxState {
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

    /// Consumes one pending user-originated value change.
    pub fn take_changed(&mut self) -> bool {
        crate::widgets::take_pending_event(&mut self.pending_changes)
    }
}

/// Concrete checkbox runtime and sole strong owner of its application state.
pub struct Checkbox {
    /// Label displayed beside the checkbox square.
    label: String,
    /// Font used for the label.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation owned for exactly this runtime's lifetime.
    state: Rc<RefCell<CheckboxState>>,
}

impl Checkbox {
    /// Constructs a typed [`CheckboxState`] handle and its unique concrete runtime.
    pub fn create(parameters: CheckboxParameters) -> (WidgetStateHandle<CheckboxState>, Self) {
        let widget = CheckboxBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures the checkbox square plus optional label.
    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle) -> Dimensioni {
        let padding = style.padding.max(0);
        let check_icon = atlas.get_icon_size(style.icons.check);
        let height = content_height(style, atlas, self.font, check_icon.height);
        let mut width = padding * 2 + height;
        if !self.label.is_empty() {
            width += text_size(style, atlas, self.font, &self.label).width + padding;
        }
        Dimensioni::new(width.max(0), height)
    }

    /// Paints the checkbox square, checked icon, and label.
    fn paint_widget(&self, checked: bool, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        let box_rect = rect(bounds.x, bounds.y, bounds.height, bounds.height);
        let box_content = ctx.draw_widget_internal_frame(box_rect, ControlColor::Base);
        if checked {
            let color = ctx.style().colors[ControlColor::Text as usize];
            if let Some(box_content) = box_content {
                ctx.draw_icon(ctx.style().icons.check, box_content, color);
            }
        }
        let text_rect = rect(bounds.x + box_rect.width, bounds.y, bounds.width - box_rect.width, bounds.height);
        if !self.label.is_empty() {
            let font = ctx.style().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, &self.label, text_rect, ControlColor::Text, self.opt);
        }
    }
}

impl Widget for Checkbox {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        self.preferred_size(style, atlas)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        if !ctx.clicked() {
            return;
        }

        runtime_update_state(&self.state, "Checkbox::update", |state| {
            state.checked = !state.checked;
            crate::widgets::record_pending_event(&mut state.pending_changes);
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let checked = runtime_read_state(&self.state, "Checkbox::paint", CheckboxState::checked);
        self.paint_widget(checked, ctx);
    }
}

impl WidgetStateOwner for Checkbox {
    type State = CheckboxState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
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
            state: Rc::new(RefCell::new(CheckboxState {
                checked: parameters.checked,
                pending_changes: 0,
            })),
        }
    }
}
