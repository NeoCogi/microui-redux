//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
//! Non-interactive display widgets.
//!
//! These retained widgets render static text and color previews while still participating in the
//! same measurement and paint pipeline as interactive controls.

use crate::text_layout::{baseline_aligned_top, build_display_text_lines, text_block_size};
use crate::ui_node::runtime_read_state;
use crate::*;
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`TextBlock`].
pub struct TextBlockParameters {
    /// Initial text rendered by the widget.
    pub text: String,
    /// Wrapping mode used for layout and rendering.
    pub wrap: TextWrap,
    /// Font used for text measurement and paint.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for TextBlockParameters {}

impl TextBlockParameters {
    /// Creates text-block parameters without wrapping.
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_wrap(text, TextWrap::None)
    }

    /// Creates text-block parameters with an explicit wrapping mode.
    pub fn with_wrap(text: impl Into<String>, wrap: TextWrap) -> Self {
        Self {
            text: text.into(),
            wrap,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NO_INTERACT,
        }
    }

    /// Replaces the font used for text measurement and paint.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent text-block state.
pub struct TextBlockState {
    /// Mutable display text.
    text: String,
}

impl WidgetState for TextBlockState {}

impl TextBlockState {
    /// Returns the current display text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the display text.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// Clears the display text.
    pub fn clear(&mut self) {
        self.text.clear();
    }
}

/// Concrete text-block runtime and sole strong owner of its application state.
pub struct TextBlock {
    /// Initialization-only wrapping mode.
    wrap: TextWrap,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<TextBlockState>>,
}

impl TextBlock {
    /// Constructs a typed state handle and unique text-block runtime.
    pub fn create(parameters: TextBlockParameters) -> (WidgetStateHandle<TextBlockState>, Self) {
        let widget = TextBlockBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures wrapped display text using the available width when requested.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "TextBlock::measure", |state| {
            if state.text.is_empty() {
                return Dimensioni::new(0, 0);
            }

            let font = style.resolve_font_choice(self.font);
            let line_height = atlas.get_font_height(font) as i32;
            let max_width = if self.wrap == TextWrap::Word && avail.width > 0 {
                avail.width.max(1)
            } else {
                i32::MAX / 4
            };
            let lines = build_display_text_lines(state.text.as_str(), self.wrap, max_width, font, atlas);
            text_block_size(&lines, line_height)
        })
    }

    /// Paints each measured display line with baseline alignment.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        runtime_read_state(&self.state, "TextBlock::paint", |state| {
            if state.text.is_empty() {
                return;
            }

            let bounds = ctx.local_rect();
            let font = ctx.style().resolve_font_choice(self.font);
            let color = ctx.style().colors[ControlColor::Text as usize];
            let line_height = ctx.atlas().get_font_height(font) as i32;
            let baseline = ctx.atlas().get_font_baseline(font);
            let max_width = if self.wrap == TextWrap::Word { bounds.width.max(1) } else { i32::MAX / 4 };
            let lines = build_display_text_lines(state.text.as_str(), self.wrap, max_width, font, ctx.atlas());

            let mut painter = ctx.painter();
            painter.with_clip(bounds, |painter| {
                for (idx, line) in lines.iter().enumerate() {
                    let line_rect = rect(bounds.x, bounds.y + idx as i32 * line_height, bounds.width, line_height);
                    let line_top = baseline_aligned_top(line_rect, line_height, baseline);
                    let slice = &state.text[line.start..line.end];
                    if !slice.is_empty() {
                        painter.text(font, slice, vec2(line_rect.x, line_top), color);
                    }
                }
            });
        });
    }
}

impl Widget for TextBlock {
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

impl WidgetStateOwner for TextBlock {
    type State = TextBlockState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating text-block parameters with the concrete runtime.
pub struct TextBlockBuilder;

impl WidgetBuilder for TextBlockBuilder {
    type Parameters = TextBlockParameters;
    type W = TextBlock;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        TextBlock {
            wrap: parameters.wrap,
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(TextBlockState { text: parameters.text })),
        }
    }
}

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
