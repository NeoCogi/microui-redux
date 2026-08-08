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

//! Retained list-box widget.

use super::*;
use crate::ui_node::runtime_update_state;
use crate::widgets::{record_pending_event, take_pending_event};
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`ListBox`].
pub struct ListBoxParameters {
    /// Label displayed for the list box.
    pub label: String,
    /// Optional image rendered alongside the label.
    pub image: Option<TextureId>,
    /// Font used for the label.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for ListBoxParameters {}

impl ListBoxParameters {
    /// Creates list-box parameters with default widget options.
    pub fn new(label: impl Into<String>, image: Option<TextureId>) -> Self {
        Self {
            label: label.into(),
            image,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates list-box parameters with explicit widget options.
    pub fn with_opt(label: impl Into<String>, image: Option<TextureId>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            image,
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

/// Application-facing persistent list-box state.
pub struct ListBoxState {
    /// User submissions waiting to be consumed.
    pending_submissions: u32,
    /// Session connection for user submissions.
    submitted_event: crate::event::WidgetEventPort<ListBoxSubmitted>,
}

/// Semantic payload emitted when the user submits a list box.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ListBoxSubmitted;

impl WidgetStateHandle<ListBoxState> {
    /// Returns the native event endpoint emitted once for every user submission.
    pub fn submitted(&self) -> crate::WidgetEvent<ListBoxState, ListBoxSubmitted> {
        crate::WidgetEvent::new(self.clone(), |state| &mut state.submitted_event)
    }
}

impl WidgetState for ListBoxState {}

impl ListBoxState {
    /// Consumes one pending user submission.
    pub fn take_submitted(&mut self) -> bool {
        take_pending_event(&mut self.pending_submissions)
    }
}

/// Concrete list-box runtime and sole strong owner of its application state.
pub struct ListBox {
    /// Initialization-only label.
    label: String,
    /// Initialization-only image.
    image: Option<TextureId>,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ListBoxState>>,
}

impl ListBox {
    /// Constructs a typed state handle and unique list-box runtime.
    pub fn create(parameters: ListBoxParameters) -> (WidgetStateHandle<ListBoxState>, Self) {
        let widget = ListBoxBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures list-box inline label and optional image.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let visual = self.image.map(TextureId::size);
        inline_content_size(style, atlas, self.font, &self.label, visual)
    }

    /// Paints list-box frame, label, and optional image.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let rect = ctx.local_rect();
        if let Some(colorid) = widget_fill_color(ctx, ControlColor::Button, WidgetFillOption::HOVER | WidgetFillOption::CLICK) {
            ctx.draw_rect(rect, ctx.style().colors[colorid as usize]);
        }
        let visual_size = self.image.map(TextureId::size);
        let layout = layout_inline_content(rect, ctx.style(), &self.label, visual_size);
        if !self.label.is_empty() {
            let font = ctx.style().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, &self.label, layout.text, ControlColor::Text, self.opt);
        }
        if let (Some(image), Some(visual)) = (self.image, layout.visual) {
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.push_image(image, visual, color);
        }
    }
}

impl Widget for ListBox {
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
        runtime_update_state(&self.state, "ListBox::update", |state| {
            record_pending_event(&mut state.pending_submissions);
            state.submitted_event.emit(ListBoxSubmitted);
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for ListBox {
    type State = ListBoxState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating list-box parameters with the concrete runtime.
pub struct ListBoxBuilder;

impl WidgetBuilder for ListBoxBuilder {
    type Parameters = ListBoxParameters;
    type W = ListBox;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        ListBox {
            label: parameters.label,
            image: parameters.image,
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(ListBoxState {
                pending_submissions: 0,
                submitted_event: crate::event::WidgetEventPort::new(),
            })),
        }
    }
}
