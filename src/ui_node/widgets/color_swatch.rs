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

//! Retained color-swatch widget.

use crate::ui_node::runtime_read_state;
use crate::*;
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`ColorSwatch`].
pub struct ColorSwatchParameters {
    /// Initial fill color rendered inside the swatch.
    pub fill: Color,
    /// Initial optional label rendered on top of the swatch.
    pub label: String,
    /// Font used for the label.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for ColorSwatchParameters {}

impl ColorSwatchParameters {
    /// Creates swatch parameters with the provided fill color.
    pub fn new(fill: Color) -> Self {
        Self {
            fill,
            label: String::new(),
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NO_INTERACT | WidgetOption::ALIGN_CENTER | WidgetOption::FRAME,
        }
    }

    /// Seeds the label rendered over the swatch.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Replaces the font used for the label.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent color-swatch state.
pub struct ColorSwatchState {
    /// Mutable fill color.
    fill: Color,
    /// Mutable label.
    label: String,
}

impl WidgetState for ColorSwatchState {}

impl ColorSwatchState {
    /// Returns the current fill color.
    pub const fn fill(&self) -> Color {
        self.fill
    }

    /// Replaces the fill color.
    pub fn set_fill(&mut self, fill: Color) {
        self.fill = fill;
    }

    /// Returns the current label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Replaces the label.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }
}

/// Concrete color-swatch runtime and sole strong owner of its application state.
pub struct ColorSwatch {
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ColorSwatchState>>,
}

impl ColorSwatch {
    /// Constructs a typed state handle and unique color-swatch runtime.
    pub fn create(parameters: ColorSwatchParameters) -> (WidgetStateHandle<ColorSwatchState>, Self) {
        let widget = ColorSwatchBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures a square-ish color swatch with a text-friendly default height.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        runtime_read_state(&self.state, "ColorSwatch::measure", |state| {
            let font = style.resolve_font_choice(self.font);
            let label_width = if state.label.is_empty() {
                0
            } else {
                atlas.get_text_size(font, state.label.as_str()).width.max(0)
            };
            let height = (atlas.get_font_height(font) as i32 + padding * 2).max(24);
            Dimensioni::new((label_width + padding * 2).max(24), height)
        })
    }

    /// Paints the swatch fill, border, and optional label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        runtime_read_state(&self.state, "ColorSwatch::paint", |state| {
            let rect = ctx.local_rect();
            ctx.draw_rect(rect, state.fill);
            if !state.label.is_empty() {
                let font = ctx.style().resolve_font_choice(self.font);
                ctx.draw_control_text_with_font(font, state.label.as_str(), rect, ControlColor::Text, self.opt);
            }
        });
    }
}

impl Widget for ColorSwatch {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for ColorSwatch {
    type State = ColorSwatchState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating color-swatch parameters with the concrete runtime.
pub struct ColorSwatchBuilder;

impl WidgetBuilder for ColorSwatchBuilder {
    type Parameters = ColorSwatchParameters;
    type W = ColorSwatch;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        ColorSwatch {
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(ColorSwatchState {
                fill: parameters.fill,
                label: parameters.label,
            })),
        }
    }
}
