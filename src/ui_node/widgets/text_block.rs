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
//! Retained text-block widget.
//!
//! Text blocks render static or application-mutated text while participating in the same
//! measurement and paint pipeline as interactive controls. CRLF and lone CR are normalized to LF
//! when text enters concrete widget storage. Visible glyph coverage and missing-character fallback
//! come from the selected atlas font.

use crate::ui_node::text_layout::{baseline_aligned_top, build_display_text_lines, text_block_size};
use crate::*;

use super::text_edit::normalize_multiline_text;

/// One-shot construction input for a [`TextBlock`].
pub struct TextBlockParameters {
    /// Initial text rendered by the widget; line endings are normalized when mounted.
    pub text: String,
    /// Wrapping mode used for layout and rendering.
    pub wrap: TextWrap,
    /// Font used for text measurement and paint.
    pub font: FontRef,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl crate::LeafWidget for TextBlock {
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
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
            font: FontRef::Role(FontRole::Body),
            opt: WidgetOption::NO_INTERACT,
        }
    }

    /// Replaces the font used for text measurement and paint.
    pub fn font(mut self, font: FontRef) -> Self {
        // Text layout resolves this stable value against whichever skin bundle is active later.
        self.font = font;
        self
    }
}

/// Concrete retained text block, including its semantic state.
pub struct TextBlock {
    /// Initialization-only wrapping mode.
    wrap: TextWrap,
    /// Initialization-only font.
    font: FontRef,
    /// Base widget options.
    opt: WidgetOption,
    /// Mutable display text.
    text: String,
}

impl TextBlock {
    /// Constructs a retained node and a weak typed handle to its concrete text block.
    pub fn create(parameters: TextBlockParameters) -> (TypedWidgetHandle<Self>, Node) {
        let widget = TextBlockBuilder::create_widget(parameters);
        Node::typed_widget(widget)
    }

    /// Returns the current display text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces display text after normalizing line endings to LF.
    pub fn set_text(&mut self, text: impl Into<String>) {
        // Static multiline text shares the same canonical LF representation as editable content,
        // keeping measured line ranges identical to the slices submitted during paint.
        self.text = normalize_multiline_text(text);
    }

    /// Clears the display text.
    pub fn clear(&mut self) {
        self.text.clear();
    }

    /// Measures wrapped display text using the available width when requested.
    fn preferred_size_widget(&self, style: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        if self.text.is_empty() {
            return Dimensioni::new(0, 0);
        }

        let font = style.resolve_font(atlas, &self.font);
        let line_height = atlas.get_font_height(font) as i32;
        let max_width = if self.wrap == TextWrap::Word {
            constraints.width.bound().unwrap_or(i32::MAX / 4).max(1)
        } else {
            i32::MAX / 4
        };
        let lines = build_display_text_lines(self.text.as_str(), self.wrap, max_width, font, atlas);
        text_block_size(&lines, line_height)
    }

    /// Paints each measured display line with baseline alignment.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        if self.text.is_empty() {
            return;
        }

        let bounds = ctx.local_rect();
        let font = ctx.skin().resolve_font(ctx.atlas(), &self.font);
        let color = ctx.foreground(AppearanceRole::GenericFrame);
        let line_height = ctx.atlas().get_font_height(font) as i32;
        let baseline = ctx.atlas().get_font_baseline(font);
        let max_width = if self.wrap == TextWrap::Word { bounds.width.max(1) } else { i32::MAX / 4 };
        let lines = build_display_text_lines(self.text.as_str(), self.wrap, max_width, font, ctx.atlas());

        let mut painter = ctx.painter();
        painter.with_clip(bounds, |painter| {
            for (idx, line) in lines.iter().enumerate() {
                // A large validated line height or long block must clamp at the geometry boundary
                // instead of overflowing while deriving a later row's origin.
                let line_index = i32::try_from(idx).unwrap_or(i32::MAX);
                let line_y = bounds.y.saturating_add(line_index.saturating_mul(line_height));
                let line_rect = rect(bounds.x, line_y, bounds.width, line_height);
                let line_top = baseline_aligned_top(line_rect, line_height, baseline);
                let slice = &self.text[line.start..line.end];
                if !slice.is_empty() {
                    painter.text(font, slice, vec2(line_rect.x, line_top), color);
                }
            }
        });
    }
}

impl TypedWidgetHandle<TextBlock> {
    /// Clones the current display text while the widget is retained.
    pub fn text(&self) -> Option<String> {
        self.try_read(|widget| widget.text().to_owned())
    }

    /// Replaces retained display text after normalizing line endings to LF.
    pub fn set_text(&self, text: impl Into<String>) -> Option<()> {
        self.try_update_with(text.into(), |widget, text| widget.set_text(text)).ok()
    }

    /// Clears the retained display text.
    pub fn clear(&self) -> Option<()> {
        self.try_update(TextBlock::clear)
    }
}

impl Widget for TextBlock {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
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
            // `text` is public construction data and may be supplied through a struct literal, so
            // enforce canonical storage at the concrete widget boundary rather than only in `new`.
            text: normalize_multiline_text(parameters.text),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Storage-boundary regressions for static multiline text.

    use super::*;

    /// Verifies both public parameter forms and later replacements canonicalize line endings.
    #[test]
    fn text_block_stores_only_lf_line_endings() {
        let mut block = TextBlockBuilder::create_widget(TextBlockParameters {
            text: "a\r\nb\rc\n".to_owned(),
            wrap: TextWrap::None,
            font: FontRef::Role(FontRole::Body),
            opt: WidgetOption::NO_INTERACT,
        });
        assert_eq!(block.text(), "a\nb\nc\n");

        block.set_text("d\r\ne\rf\n");
        assert_eq!(block.text(), "d\ne\nf\n");
    }
}
