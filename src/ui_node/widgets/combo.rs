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
use crate::ui_node::{runtime_read_state, runtime_update_state};
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`Combo`].
pub struct ComboParameters {
    /// Font used for the current label.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for ComboParameters {}

impl ComboParameters {
    /// Creates combo parameters with default widget options.
    pub const fn new() -> Self {
        Self {
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates combo parameters with explicit widget options.
    pub const fn with_opt(opt: WidgetOption) -> Self {
        Self {
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used for the current label.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

impl Default for ComboParameters {
    fn default() -> Self {
        Self::new()
    }
}

/// Application-facing persistent combo state.
pub struct ComboState {
    /// Currently selected item index.
    selected: usize,
    /// Whether the combo popup should be open.
    open: bool,
    /// Label text for the currently selected item.
    label: String,
    /// Framework-owned popup anchor snapshot published by the latest paint.
    last_anchor: Recti,
    /// Weak publisher used by selection commands; the concrete Combo owns the event port.
    changed_emitter: crate::event::WidgetEventEmitter<ComboChanged>,
}

impl WidgetState for ComboState {}

impl ComboState {
    /// Returns the popup anchor published by the latest completed combo paint.
    ///
    /// This geometry is intended for positioning the popup during a later update/commit; it does not
    /// retroactively affect the frame that produced it.
    pub fn anchor(&self) -> Recti {
        self.last_anchor
    }

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
        self.changed_emitter.emit(ComboChanged {
            selected: self.selected,
            label: self.label.clone(),
        });
    }

    fn emit_changed(&mut self) {
        self.changed_emitter.emit(ComboChanged {
            selected: self.selected,
            label: self.label.clone(),
        });
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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ComboSubmitted {
    /// Whether the triggering header submission left the popup open.
    pub open: bool,
}

impl crate::WidgetEvent for ComboSubmitted {}

/// Concrete combo runtime and sole strong owner of its application state.
pub struct Combo {
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ComboState>>,
    /// Runtime-owned source for selection changes.
    changed_event: Rc<crate::event::WidgetEventPort<ComboChanged>>,
    /// Runtime-owned source for header submissions.
    submitted_event: Rc<crate::event::WidgetEventPort<ComboSubmitted>>,
}

impl Combo {
    /// Constructs a typed state handle and unique combo runtime.
    pub fn create(parameters: ComboParameters) -> (WidgetStateHandle<ComboState>, Self) {
        let widget = ComboBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Returns the native event endpoint emitted after every selection change.
    pub fn changed(&self) -> crate::WidgetEventHandle<ComboChanged> {
        <Self as crate::TypedWidget<ComboChanged>>::event(self)
    }

    /// Returns the native event endpoint emitted whenever the user submits the combo header.
    pub fn submitted(&self) -> crate::WidgetEventHandle<ComboSubmitted> {
        <Self as crate::TypedWidget<ComboSubmitted>>::event(self)
    }

    /// Measures the combo header label plus dropdown indicator.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Combo::measure", |state| {
            let padding = style.padding.max(0);
            let text_w = if state.label.is_empty() {
                0
            } else {
                text_size(style, atlas, self.font, state.label.as_str()).width
            };
            let indicator = atlas.get_icon_size(style.icons.expand_down);
            let width = (padding * 3 + text_w + indicator.width).max(0);
            let height = content_height(style, atlas, self.font, indicator.height);
            Dimensioni::new(width, height)
        })
    }

    /// Updates popup open state and records header submissions.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>) {
        let submitted = runtime_update_state(&self.state, "Combo::update", |state| {
            if ctx.clicked() {
                state.open = !state.open;
                Some(ComboSubmitted { open: state.open })
            } else {
                None
            }
        });
        if let Some(event) = submitted {
            self.submitted_event.emit(event);
        }
    }

    /// Paints the combo header and publishes the read-only popup anchor below it.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let header = ctx.local_rect();
        let screen_header = ctx.screen_content_rect();
        ctx.draw_widget_fill(header, ControlColor::Button);

        let indicator_size = ctx.atlas().get_icon_size(ctx.style().icons.expand_down);
        let indicator_x = header.x + header.width - indicator_size.width;
        let indicator_y = header.y + ((header.height - indicator_size.height) / 2).max(0);
        let indicator = rect(indicator_x, indicator_y, indicator_size.width, indicator_size.height);

        let mut text_rect = header;
        let reserved_width = indicator_size.width;
        text_rect.width = (text_rect.width - reserved_width).max(0);
        runtime_update_state(&self.state, "Combo::paint", |state| {
            state.last_anchor = rect(screen_header.x, screen_header.y + screen_header.height, screen_header.width, 1);
            let font = ctx.style().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, state.label.as_str(), text_rect, ControlColor::Text, self.opt);
        });

        let indicator_content = ctx.draw_widget_internal_frame(indicator, ControlColor::Button);
        let icon_color = ctx.style().colors[ControlColor::Text as usize];
        if let Some(indicator_content) = indicator_content {
            ctx.draw_icon(ctx.style().icons.expand_down, indicator_content, icon_color);
        }
    }
}

impl crate::TypedWidget<ComboChanged> for Combo {
    fn event(&self) -> crate::WidgetEventHandle<ComboChanged> {
        crate::WidgetEventHandle::new(&self.changed_event)
    }
}

impl crate::TypedWidget<ComboSubmitted> for Combo {
    fn event(&self) -> crate::WidgetEventHandle<ComboSubmitted> {
        crate::WidgetEventHandle::new(&self.submitted_event)
    }
}

impl Widget for Combo {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        self.update_widget(ctx)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for Combo {
    type State = ComboState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating combo parameters with the concrete runtime.
pub struct ComboBuilder;

impl WidgetBuilder for ComboBuilder {
    type Parameters = ComboParameters;
    type W = Combo;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        let changed_event = Rc::new(crate::event::WidgetEventPort::new());
        Combo {
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(ComboState {
                selected: 0,
                open: false,
                label: String::new(),
                last_anchor: Recti::default(),
                changed_emitter: crate::event::WidgetEventEmitter::new(&changed_event),
            })),
            changed_event,
            submitted_event: Rc::new(crate::event::WidgetEventPort::new()),
        }
    }
}
