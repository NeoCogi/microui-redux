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

use crate::{ControlRole};

use crate::*;

/// One-shot construction input for a [`ColorSwatch`].
pub struct ColorSwatchParameters {
    /// Initial fill color rendered inside the swatch.
    pub fill: Color,
    /// Initial optional label rendered on top of the swatch.
    pub label: String,
    /// Font used for the label.
    pub font: FontRef,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl crate::LeafWidget for ColorSwatch {
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
}

impl WidgetParameters for ColorSwatchParameters {}

impl ColorSwatchParameters {
    /// Creates swatch parameters with the provided fill color.
    pub fn new(fill: Color) -> Self {
        Self {
            fill,
            label: String::new(),
            font: FontRef::Role(FontRole::Body),
            opt: WidgetOption::NO_INTERACT | WidgetOption::ALIGN_CENTER | WidgetOption::FRAME,
        }
    }

    /// Seeds the label rendered over the swatch.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Replaces the font used for the label.
    pub fn font(mut self, font: FontRef) -> Self {
        // Keep the stable reference unresolved until the swatch is measured or painted.
        self.font = font;
        self
    }
}

/// Concrete retained color swatch, including its semantic state.
pub struct ColorSwatch {
    /// Initialization-only font.
    font: FontRef,
    /// Base widget options.
    opt: WidgetOption,
    /// Mutable fill color.
    fill: Color,
    /// Mutable label.
    label: String,
}

impl ColorSwatch {
    /// Constructs a retained node and a weak typed handle to its concrete color swatch.
    pub fn create(parameters: ColorSwatchParameters) -> (crate::TypedWidgetHandle<Self>, crate::Node) {
        let widget = ColorSwatchBuilder::create_widget(parameters);
        crate::Node::typed_widget(widget)
    }

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

    /// Measures a square-ish color swatch with a text-friendly default height.
    fn preferred_size_widget(&self, style: &Skin, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        let padding = style.metrics.padding.max(0);
        let font = style.resolve_font(atlas, &self.font);
        let label_width = if self.label.is_empty() {
            0
        } else {
            atlas.get_text_size(font, self.label.as_str()).width.max(0)
        };
        // Saturating extents keep an otherwise valid extreme font from wrapping intrinsic widget
        // geometry before layout has a chance to constrain it.
        let padding_extent = padding.saturating_mul(2);
        let height = (atlas.get_font_height(font) as i32).saturating_add(padding_extent).max(24);
        Dimensioni::new(label_width.saturating_add(padding_extent).max(24), height)
    }

    /// Paints the swatch fill, border, and optional label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let rect = ctx.local_rect();
        ctx.draw_rect(rect, self.fill);
        if !self.label.is_empty() {
            let font = ctx.skin().resolve_font(ctx.atlas(), &self.font);
            ctx.draw_control_text_with_font(font, self.label.as_str(), rect, AppearanceRole::Control(ControlRole::Button), self.opt);
        }
    }
}

impl TypedWidgetHandle<ColorSwatch> {
    /// Returns the current fill color while the widget is retained.
    pub fn fill(&self) -> Option<Color> {
        self.try_read(ColorSwatch::fill)
    }

    /// Replaces the retained swatch fill.
    pub fn set_fill(&self, fill: Color) -> Option<()> {
        self.try_update(|widget| widget.set_fill(fill))
    }

    /// Clones the current label while the widget is retained.
    pub fn label(&self) -> Option<String> {
        self.try_read(|widget| widget.label().to_owned())
    }

    /// Replaces the retained swatch label.
    pub fn set_label(&self, label: impl Into<String>) -> Option<()> {
        self.try_update_with(label.into(), |widget, label| widget.set_label(label)).ok()
    }
}

impl Widget for ColorSwatch {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
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
            fill: parameters.fill,
            label: parameters.label,
        }
    }
}
