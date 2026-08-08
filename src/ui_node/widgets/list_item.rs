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

//! Retained list-item widget.
//!
//! `ListItem` represents one selectable row in a retained list.

use super::*;
use crate::ui_node::{runtime_read_state, runtime_update_state};
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`ListItem`].
pub struct ListItemParameters {
    /// Initial label displayed for the list item.
    pub label: String,
    /// Optional atlas icon rendered alongside the label.
    pub icon: Option<IconId>,
    /// Font used for the label.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for ListItemParameters {}

impl ListItemParameters {
    /// Creates list-item parameters with default widget options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates list-item parameters with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: None,
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Creates list-item parameters with an icon and default widget options.
    pub fn with_icon(label: impl Into<String>, icon: IconId) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates list-item parameters with an icon and explicit widget options.
    pub fn with_icon_opt(label: impl Into<String>, icon: IconId, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used for the label.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent list-item state.
pub struct ListItemState {
    /// Mutable label displayed for the item.
    label: String,
    /// Session connection for user submissions.
    submitted_event: crate::event::WidgetEventPort<ListItemSubmitted>,
}

/// Snapshot emitted when the user submits a list item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListItemSubmitted {
    /// Item label at submission time.
    pub label: String,
}

impl crate::WidgetEvent for ListItemSubmitted {}

impl WidgetStateHandle<ListItemState> {
    /// Returns the native event endpoint emitted once for every user submission.
    pub fn submitted(&self) -> crate::WidgetEventHandle<ListItemState, ListItemSubmitted> {
        crate::WidgetEventHandle::new(self.clone(), |state| &mut state.submitted_event)
    }
}

impl WidgetState for ListItemState {}

impl ListItemState {
    /// Returns the current label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Replaces the label without recording a user submission.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }
}

/// Concrete list-item runtime and sole strong owner of its application state.
pub struct ListItem {
    /// Initialization-only icon.
    icon: Option<IconId>,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ListItemState>>,
}

impl ListItem {
    /// Constructs a typed state handle and unique list-item runtime.
    pub fn create(parameters: ListItemParameters) -> (WidgetStateHandle<ListItemState>, Self) {
        let widget = ListItemBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures the row label and optional icon.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let mut width = padding * 2;
        let mut visual_h = 0;
        if let Some(icon) = self.icon {
            let size = atlas.get_icon_size(icon);
            width += size.width + padding;
            visual_h = size.height;
        }
        runtime_read_state(&self.state, "ListItem::measure", |state| {
            if !state.label.is_empty() {
                width += text_size(style, atlas, self.font, &state.label).width;
            }
            let height = content_height(style, atlas, self.font, visual_h);
            Dimensioni::new(width.max(0), height)
        })
    }

    /// Paints row highlight, optional icon, and label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();

        if ctx.focused() || ctx.hovered() {
            let mut color = ControlColor::Button;
            if ctx.focused() {
                color.focus();
            } else {
                color.hover();
            }
            let fill = ctx.style().colors[color as usize];
            ctx.draw_rect(bounds, fill);
        }

        let mut text_rect = bounds;
        if let Some(icon) = self.icon {
            // Icons consume the left padding + icon width before the text region starts.
            let padding = ctx.style().padding.max(0);
            let icon_size = ctx.atlas().get_icon_size(icon);
            let icon_x = bounds.x + padding;
            let icon_y = bounds.y + ((bounds.height - icon_size.height) / 2).max(0);
            let icon_rect = rect(icon_x, icon_y, icon_size.width, icon_size.height);
            let consumed = icon_size.width + padding * 2;
            text_rect.x += consumed;
            text_rect.width = (text_rect.width - consumed).max(0);
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.draw_icon(icon, icon_rect, color);
        }

        runtime_read_state(&self.state, "ListItem::paint", |state| {
            if !state.label.is_empty() {
                let font = ctx.style().resolve_font_choice(self.font);
                ctx.draw_control_text_with_font(font, &state.label, text_rect, ControlColor::Text, self.opt);
            }
        });
    }
}

impl Widget for ListItem {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        if !ctx.clicked() {
            return;
        }
        runtime_update_state(&self.state, "ListItem::update", |state| {
            state.submitted_event.emit(ListItemSubmitted { label: state.label.clone() });
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for ListItem {
    type State = ListItemState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating list-item parameters with the concrete runtime.
pub struct ListItemBuilder;

impl WidgetBuilder for ListItemBuilder {
    type Parameters = ListItemParameters;
    type W = ListItem;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        ListItem {
            icon: parameters.icon,
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(ListItemState {
                label: parameters.label,
                submitted_event: crate::event::WidgetEventPort::new(),
            })),
        }
    }
}
