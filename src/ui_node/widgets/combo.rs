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

//! Combo-box retained state.
//!
//! The combo widget tracks selected item text and popup-open state; the context root layer owns the
//! actual popup traversal.

use super::*;
use std::{cell::RefCell, rc::Rc};

/// Compares rectangles structurally because the external geometry type does not implement equality.
fn same_rect(left: Recti, right: Recti) -> bool {
    (left.x, left.y, left.width, left.height) == (right.x, right.y, right.width, right.height)
}

/// One-shot construction input for a [`Combo`].
pub struct ComboParameters {
    /// Font used for the current label.
    pub font: FontRef,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl crate::LeafWidget for Combo {
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
}

impl WidgetParameters for ComboParameters {}

impl ComboParameters {
    /// Creates combo parameters with default widget options.
    pub const fn new() -> Self {
        Self {
            font: FontRef::Role(FontRole::Body),
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates combo parameters with explicit widget options.
    pub const fn with_opt(opt: WidgetOption) -> Self {
        Self { font: FontRef::Role(FontRole::Body), opt }
    }

    /// Replaces the font used for the current label.
    pub fn font(mut self, font: FontRef) -> Self {
        // Retain the stable selection across popup and skin replacement transactions.
        self.font = font;
        self
    }
}

impl Default for ComboParameters {
    fn default() -> Self {
        Self::new()
    }
}

/// Concrete retained combo, including its semantic and popup state.
pub struct Combo {
    /// Currently selected item index.
    selected: usize,
    /// Whether the combo popup should be open.
    open: bool,
    /// Label text for the currently selected item.
    label: String,
    /// Initialization-only font.
    font: FontRef,
    /// Base widget options.
    opt: WidgetOption,
    /// Runtime-owned source for selection changes.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<ComboChanged>>>,
    /// Runtime-owned source for header submissions.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<ComboSubmitted>>>,
}

impl Combo {
    /// Returns the currently selected item index.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns the current selected-item label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns `true` while the combo popup should remain open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the popup and marks the combo open.
    pub fn open_popup(&mut self) {
        self.open = true;
    }

    /// Closes the popup.
    pub fn close_popup(&mut self) {
        self.open = false;
    }

    /// Updates the cached label and clamps the selected index to the provided items.
    pub fn update_items<S: AsRef<str>>(&mut self, items: &[S]) {
        let previous_selected = self.selected;
        if items.is_empty() {
            self.selected = 0;
            self.label.clear();
            if self.selected != previous_selected {
                self.emit_changed();
            }
            return;
        }

        if self.selected >= items.len() {
            // Clamp stale selections after the backing item list changes.
            self.selected = items.len() - 1;
        }

        self.label.clear();
        if let Some(label) = items.get(self.selected) {
            self.label.push_str(label.as_ref());
        }
        if self.selected != previous_selected {
            self.emit_changed();
        }
    }

    /// Applies a submitted popup item selection and closes the popup.
    pub fn select<S: AsRef<str>>(&mut self, index: usize, items: &[S]) -> Option<String> {
        let previous_selected = self.selected;
        let previous_label = self.label.clone();
        if items.is_empty() {
            self.selected = 0;
            self.label.clear();
            self.close_popup();
            self.emit_change_if_needed(previous_selected, &previous_label);
            return None;
        }

        self.selected = index.min(items.len() - 1);
        self.label.clear();
        self.label.push_str(items[self.selected].as_ref());
        let selected_label = self.label.clone();
        self.close_popup();
        self.emit_change_if_needed(previous_selected, &previous_label);
        Some(selected_label)
    }

    fn emit_change_if_needed(&mut self, previous_selected: usize, previous_label: &str) {
        if self.selected == previous_selected && self.label == previous_label {
            return;
        }
        self.changed_event.borrow_mut().emit(ComboChanged {
            selected: self.selected,
            label: self.label.clone(),
        });
    }

    fn emit_changed(&mut self) {
        self.changed_event.borrow_mut().emit(ComboChanged {
            selected: self.selected,
            label: self.label.clone(),
        });
    }
}

impl TypedWidgetHandle<Combo> {
    /// Returns the selected item index while the combo is retained.
    pub fn selected(&self) -> Option<usize> {
        self.try_read(Combo::selected)
    }

    /// Clones the current selected-item label while the combo is retained.
    pub fn label(&self) -> Option<String> {
        self.try_read(|widget| widget.label().to_owned())
    }

    /// Returns whether the retained combo popup is open.
    pub fn is_open(&self) -> Option<bool> {
        self.try_read(Combo::is_open)
    }

    /// Opens the retained combo popup.
    pub fn open_popup(&self) -> Option<()> {
        self.try_update(Combo::open_popup)
    }

    /// Closes the retained combo popup.
    pub fn close_popup(&self) -> Option<()> {
        self.try_update(Combo::close_popup)
    }

    /// Updates the retained item snapshot and clamps selection.
    pub fn update_items<S: AsRef<str>>(&self, items: &[S]) -> Option<()> {
        self.try_update(|widget| widget.update_items(items))
    }

    /// Applies a retained popup selection and returns the selected label.
    pub fn select<S: AsRef<str>>(&self, index: usize, items: &[S]) -> Option<Option<String>> {
        self.try_update(|widget| widget.select(index, items))
    }

    /// Returns the combo's native selection-change endpoint.
    pub fn changed(&self) -> WidgetEventPortHandle<ComboChanged> {
        self.widget_event()
    }

    /// Returns the combo's native header-submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<ComboSubmitted> {
        self.widget_event()
    }
}

/// Selection snapshot emitted after a combo's selected value changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComboChanged {
    /// Selected item index after applying the change.
    pub selected: usize,
    /// Selected item label after applying the change.
    pub label: String,
}

impl crate::WidgetEvent for ComboChanged {}

/// Popup-state snapshot emitted when the user submits the combo header.
#[derive(Copy, Clone, Debug)]
pub struct ComboSubmitted {
    /// Whether the triggering header submission left the popup open.
    pub open: bool,
    /// Screen-space popup anchor from the update that handled the submission.
    pub anchor: Recti,
}

impl PartialEq for ComboSubmitted {
    fn eq(&self, other: &Self) -> bool {
        self.open == other.open && same_rect(self.anchor, other.anchor)
    }
}

impl Eq for ComboSubmitted {}

impl crate::WidgetEvent for ComboSubmitted {}

impl Combo {
    /// Constructs a retained node and a weak typed handle to its concrete combo.
    pub fn create(parameters: ComboParameters) -> (TypedWidgetHandle<Self>, Node) {
        let widget = ComboBuilder::create_widget(parameters);
        Node::typed_widget(widget)
    }

    /// Returns the native event endpoint emitted after every selection change.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<ComboChanged> {
        <Self as crate::TypedWidget<ComboChanged>>::event(self)
    }

    /// Returns the native event endpoint emitted whenever the user submits the combo header.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<ComboSubmitted> {
        <Self as crate::TypedWidget<ComboSubmitted>>::event(self)
    }

    /// Measures the combo header label plus dropdown indicator.
    fn preferred_size_widget(&self, style: &Skin, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        let padding = style.metrics.padding.max(0);
        let text_w = if self.label.is_empty() {
            0
        } else {
            text_size(style, atlas, &self.font, self.label.as_str()).width
        };
        let indicator = atlas.get_icon_size(crate::IconRole::ExpandDown.resolve(atlas));
        let width = padding.saturating_mul(3).saturating_add(text_w.max(0)).saturating_add(indicator.width.max(0));
        let height = content_height(style, atlas, &self.font, indicator.height);
        Dimensioni::new(width, height)
    }

    /// Updates popup-open state and records header submissions with their routed geometry.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // The Combo owns only semantic open state. The application composes that state with whichever
        // retained root it selected as popup content through the typed submission event below.
        let action = ctx.action(input, self.keyboard_behavior());
        let submitted = if let Some(action) = action {
            let previous_open = self.open;
            let screen_header = ctx.screen_content_rect();
            let anchor = rect(screen_header.x, screen_header.y.saturating_add(screen_header.height), screen_header.width, 1);
            self.open = match action {
                KeyboardAction::Activate => !self.open,
                KeyboardAction::Expand => true,
                KeyboardAction::Collapse => false,
                // Combo headers declare no adjustment capability, so these branches remain a
                // defensive no-op if a custom caller supplies inconsistent flags.
                KeyboardAction::Decrease | KeyboardAction::Increase => self.open,
            };
            // Idempotent Expand/Collapse repeats do not ask the application to reapply identical
            // popup state; every pointer or activation toggle necessarily changes the value.
            (self.open != previous_open).then_some(ComboSubmitted { open: self.open, anchor })
        } else {
            None
        };
        if let Some(event) = submitted {
            self.submitted_event.borrow_mut().emit(event);
        }
    }

    /// Paints the combo header from already-committed retained state.
    fn paint_widget(&self, ctx: &mut WidgetPaintCtx<'_>) {
        let header = ctx.local_rect();
        let indicator_id = crate::IconRole::ExpandDown.resolve(ctx.atlas());
        let indicator_size = ctx.atlas().get_icon_size(indicator_id);
        let indicator_x = header.x + header.width - indicator_size.width;
        let indicator_y = header.y + ((header.height - indicator_size.height) / 2).max(0);
        let indicator = rect(indicator_x, indicator_y, indicator_size.width, indicator_size.height);

        let mut text_rect = header;
        let reserved_width = indicator_size.width;
        text_rect.width = (text_rect.width - reserved_width).max(0);
        let font = ctx.skin().resolve_font(ctx.atlas(), &self.font);
        ctx.draw_control_text_with_font(font, self.label.as_str(), text_rect, AppearanceRole::Combo, self.opt);

        let indicator_content = ctx.draw_appearance(AppearanceRole::Button, indicator);
        let icon_color = ctx.foreground(AppearanceRole::Combo);
        if let Some(indicator_content) = indicator_content {
            ctx.draw_icon(indicator_id, indicator_content, icon_color);
        }
    }
}

impl crate::TypedWidget<ComboChanged> for Combo {
    fn event(&self) -> crate::WidgetEventPortHandle<ComboChanged> {
        crate::WidgetEventPortHandle::new(&self.changed_event)
    }
}

impl crate::TypedWidget<ComboSubmitted> for Combo {
    fn event(&self) -> crate::WidgetEventPortHandle<ComboSubmitted> {
        crate::WidgetEventPortHandle::new(&self.submitted_event)
    }
}

impl Widget for Combo {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn frame_appearance_role(&self) -> AppearanceRole {
        // Combo headers use a distinct role so themes can separate them from command buttons.
        AppearanceRole::Combo
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.update_widget(ctx, input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::TAB_STOP | KeyboardBehavior::ACTIVATE_ENTER | KeyboardBehavior::ACTIVATE_SPACE | KeyboardBehavior::POPUP
    }
}

/// Builder associating combo parameters with the concrete runtime.
pub struct ComboBuilder;

impl WidgetBuilder for ComboBuilder {
    type Parameters = ComboParameters;
    type W = Combo;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Combo {
            selected: 0,
            open: false,
            label: String::new(),
            font: parameters.font,
            opt: parameters.opt,
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
            submitted_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}
