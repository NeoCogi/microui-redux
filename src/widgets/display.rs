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

use crate::text_layout::build_text_lines;
use crate::*;

fn baseline_aligned_top(rect: Recti, line_height: i32, baseline: i32) -> i32 {
    if rect.height >= line_height {
        return rect.y + (rect.height - line_height) / 2;
    }

    let baseline_center = rect.y + rect.height / 2;
    let min_top = rect.y + rect.height - line_height;
    let max_top = rect.y;
    (baseline_center - baseline).clamp(min_top, max_top)
}

fn text_lines<'a>(text: &'a str, wrap: TextWrap, max_width: i32, font: FontId, atlas: &AtlasHandle) -> Vec<crate::text_layout::TextLine> {
    let mut lines = build_text_lines(text, wrap, max_width, font, atlas);
    if text.ends_with('\n') {
        if let Some(last) = lines.last() {
            if last.start == text.len() && last.end == text.len() {
                lines.pop();
            }
        }
    }
    lines
}

#[derive(Clone)]
/// Non-interactive retained text block that can optionally wrap.
pub struct TextBlock {
    /// Text rendered by the widget.
    pub text: String,
    /// Wrapping mode used for layout and rendering.
    pub wrap: TextWrap,
    /// Widget options applied to the block.
    pub opt: WidgetOption,
    /// Behaviour options applied to the block.
    pub bopt: WidgetBehaviourOption,
}

impl TextBlock {
    /// Creates a non-interactive text block without wrapping.
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_wrap(text, TextWrap::None)
    }

    /// Creates a non-interactive text block with an explicit wrapping mode.
    pub fn with_wrap(text: impl Into<String>, wrap: TextWrap) -> Self {
        Self {
            text: text.into(),
            wrap,
            opt: WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME,
            bopt: WidgetBehaviourOption::NONE,
        }
    }

    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        if self.text.is_empty() {
            return Dimensioni::new(0, 0);
        }

        let line_height = atlas.get_font_height(style.font) as i32;
        let max_width = if self.wrap == TextWrap::Word && avail.width > 0 {
            avail.width.max(1)
        } else {
            i32::MAX / 4
        };
        let lines = text_lines(self.text.as_str(), self.wrap, max_width, style.font, atlas);
        let width = lines.iter().map(|line| line.width).max().unwrap_or(0).max(0);
        let height = line_height.saturating_mul((lines.len() as i32).max(1)).max(0);
        Dimensioni::new(width, height)
    }

    fn handle_widget(&mut self, ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        if self.text.is_empty() {
            return ResourceState::NONE;
        }

        let bounds = ctx.rect();
        let font = ctx.style().font;
        let color = ctx.style().colors[ControlColor::Text as usize];
        let line_height = ctx.atlas().get_font_height(font) as i32;
        let baseline = ctx.atlas().get_font_baseline(font);
        let max_width = if self.wrap == TextWrap::Word { bounds.width.max(1) } else { i32::MAX / 4 };
        let lines = text_lines(self.text.as_str(), self.wrap, max_width, font, ctx.atlas());

        ctx.push_clip_rect(bounds);
        for (idx, line) in lines.iter().enumerate() {
            let line_rect = rect(bounds.x, bounds.y + idx as i32 * line_height, bounds.width, line_height);
            let line_top = baseline_aligned_top(line_rect, line_height, baseline);
            let slice = &self.text[line.start..line.end];
            if !slice.is_empty() {
                ctx.draw_text(font, slice, vec2(line_rect.x, line_top), color);
            }
        }
        ctx.pop_clip_rect();

        ResourceState::NONE
    }
}

implement_widget!(TextBlock, handle_widget, preferred_size_widget);

#[derive(Clone)]
/// Non-interactive filled rectangle used for retained preview swatches.
pub struct ColorSwatch {
    /// Fill color rendered inside the swatch.
    pub fill: Color,
    /// Optional label rendered on top of the swatch.
    pub label: String,
    /// Widget options applied to the swatch.
    pub opt: WidgetOption,
    /// Behaviour options applied to the swatch.
    pub bopt: WidgetBehaviourOption,
}

impl ColorSwatch {
    /// Creates a swatch with the provided fill color.
    pub fn new(fill: Color) -> Self {
        Self {
            fill,
            label: String::new(),
            opt: WidgetOption::NO_INTERACT | WidgetOption::ALIGN_CENTER,
            bopt: WidgetBehaviourOption::NONE,
        }
    }

    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let label_width = if self.label.is_empty() {
            0
        } else {
            atlas.get_text_size(style.font, self.label.as_str()).width.max(0)
        };
        let height = (atlas.get_font_height(style.font) as i32 + padding * 2).max(24);
        Dimensioni::new((label_width + padding * 2).max(24), height)
    }

    fn handle_widget(&mut self, ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        let rect = ctx.rect();
        ctx.draw_rect(rect, self.fill);
        let border = ctx.style().colors[ControlColor::Border as usize];
        ctx.draw_box(rect, border);
        if !self.label.is_empty() {
            ctx.draw_control_text(self.label.as_str(), rect, ControlColor::Text, self.opt);
        }
        ResourceState::NONE
    }
}

implement_widget!(ColorSwatch, handle_widget, preferred_size_widget);
