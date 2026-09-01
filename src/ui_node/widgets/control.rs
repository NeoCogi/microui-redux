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
//! Shared sizing, coloring, and inline-content placement helpers for retained controls.
//!
//! The split widget modules keep concrete widget state small; this file holds common sizing,
//! exact content placement, coloring, and submit helpers used by buttons, lists, combos,
//! checkboxes, and custom controls.
use crate::*;
/// Measures text with the widget's resolved font choice.
pub(super) fn text_size(style: &Style, atlas: &AtlasHandle, font: FontChoice, text: &str) -> Dimensioni {
    atlas.get_text_size(style.resolve_font_choice(font), text)
}

/// Computes a control height that can fit both text and an optional visual element.
pub(crate) fn content_height(style: &Style, atlas: &AtlasHandle, font: FontChoice, visual_height: i32) -> i32 {
    // Fit whichever visual is taller, then apply the theme's compact vertical breathing room.
    let font_height = atlas.get_font_height(style.resolve_font_choice(font)) as i32;
    let vertical_pad = (style.padding / 2).max(1);
    font_height.max(visual_height).max(0).saturating_add(vertical_pad.saturating_mul(2))
}

/// Computes preferred size for a single-line label plus an optional icon or texture.
pub(super) fn inline_content_size(style: &Style, atlas: &AtlasHandle, font: FontChoice, label: &str, visual_size: Option<Dimensioni>) -> Dimensioni {
    let padding = style.padding.max(0);
    let text_size = if label.is_empty() {
        Dimensioni::default()
    } else {
        text_size(style, atlas, font, label)
    };
    let visual_size = visual_size.unwrap_or_default();
    let has_text = !label.is_empty();
    let has_visual = visual_size.width > 0 && visual_size.height > 0;

    // Text gets horizontal padding on both sides; visuals add one extra text/visual gap.
    let mut width = padding.saturating_mul(2).saturating_add(text_size.width.max(0));
    if has_visual {
        width = width.saturating_add(visual_size.width.max(0));
        if has_text {
            width = width.saturating_add(padding);
        }
    }

    let height = content_height(style, atlas, font, visual_size.height);
    Dimensioni::new(width.max(0), height)
}

/// Computes preferred size for an image-like button that scales its visual to the available width.
pub(super) fn scaled_visual_content_size(constraints: Constraints, visual_size: Option<Dimensioni>) -> Dimensioni {
    let visual_size = visual_size.unwrap_or_default();
    if visual_size.width <= 0 || visual_size.height <= 0 {
        return Dimensioni::default();
    }

    let width = constraints.width.bound().unwrap_or(visual_size.width);
    // Positive i32 factors fit one i64 product. Clamp only after the complete ratio so an extreme
    // aspect ratio cannot wrap while deriving intrinsic height.
    let height = ((i64::from(width.max(0)) * i64::from(visual_size.height)) / i64::from(visual_size.width)).min(i64::from(i32::MAX)) as i32;
    Dimensioni::new(width.max(0), height.max(0))
}

#[derive(Copy, Clone)]
/// Exact inline placement for an optional visual and text region inside resolved bounds.
pub(super) struct InlineContentPlacement {
    /// Optional visual/icon rectangle.
    pub(super) visual: Option<Recti>,
    /// Text rectangle.
    pub(super) text: Recti,
}

/// Places an optional visual before text while keeping visual-only content centered.
pub(super) fn place_inline_content(bounds: Recti, style: &Style, label: &str, visual_size: Option<Dimensioni>) -> InlineContentPlacement {
    let padding = style.padding.max(0);
    let visual_size = visual_size.unwrap_or_default();
    let has_visual = visual_size.width > 0 && visual_size.height > 0;
    let has_text = !label.is_empty();

    if !has_visual {
        // Text-only controls can use the whole bounds; text alignment is handled by draw helpers.
        return InlineContentPlacement { visual: None, text: bounds };
    }

    let visual_width = visual_size.width.min(bounds.width.saturating_sub(padding.saturating_mul(2)).max(0)).max(0);
    let visual_height = visual_size.height.min(bounds.height.max(0)).max(0);
    let visual_y = bounds.y.saturating_add((bounds.height.saturating_sub(visual_height) / 2).max(0));

    if !has_text {
        // Visual-only controls center the visual and do not reserve a text rect.
        let visual_x = bounds.x.saturating_add((bounds.width.saturating_sub(visual_width) / 2).max(0));
        let visual = rect(visual_x, visual_y, visual_width, visual_height);
        return InlineContentPlacement {
            visual: Some(visual),
            text: Recti::default(),
        };
    }

    let visual_x = bounds.x.saturating_add(padding);
    let visual = rect(visual_x, visual_y, visual_width, visual_height);
    let text_x = visual.x.saturating_add(visual.width);
    let right = bounds.x.saturating_add(bounds.width);
    let text = rect(text_x, bounds.y, right.saturating_sub(text_x).max(0), bounds.height);
    InlineContentPlacement { visual: Some(visual), text }
}

/// Places an image-like visual by fitting it to the button bounds while preserving aspect ratio.
pub(super) fn place_scaled_visual_content(bounds: Recti, visual_size: Option<Dimensioni>) -> InlineContentPlacement {
    let visual_size = visual_size.unwrap_or_default();
    if visual_size.width <= 0 || visual_size.height <= 0 || bounds.width <= 0 || bounds.height <= 0 {
        return InlineContentPlacement { visual: None, text: bounds };
    }

    // Compare the first aspect-ratio result in i64 before narrowing it. If height is the limiting
    // axis, the inverse ratio is necessarily no wider than the original positive bounds width.
    let proposed_height = i64::from(bounds.width) * i64::from(visual_size.height) / i64::from(visual_size.width);
    let (width, height) = if proposed_height > i64::from(bounds.height) {
        let height = bounds.height;
        let width = (i64::from(height) * i64::from(visual_size.width) / i64::from(visual_size.height)) as i32;
        (width, height)
    } else {
        (bounds.width, proposed_height as i32)
    };

    let visual = rect(
        bounds.x.saturating_add(bounds.width.saturating_sub(width).max(0) / 2),
        bounds.y.saturating_add(bounds.height.saturating_sub(height).max(0) / 2),
        width.max(0),
        height.max(0),
    );
    InlineContentPlacement { visual: Some(visual), text: bounds }
}

/// Reports whether a button-like control's fill policy enables its current semantic state.
pub(super) fn widget_fill_visible(ctx: &WidgetPaintCtx<'_>, fill: WidgetFillOption) -> bool {
    // Preserve the public visibility policy while delegating actual pixels to the typed appearance
    // catalog. Combined focus states accept either relevant policy bit; a pressed state remains a
    // click visual even when pointer capture outlives the one-update clicked transition.
    match ctx.visual_state() {
        VisualState::Normal | VisualState::Disabled => fill.intersects(WidgetFillOption::NORMAL),
        VisualState::Hovered => fill.intersects(WidgetFillOption::HOVER),
        VisualState::Pressed | VisualState::Focused | VisualState::PressedFocused => fill.intersects(WidgetFillOption::CLICK),
        VisualState::HoveredFocused => fill.intersects(WidgetFillOption::HOVER | WidgetFillOption::CLICK),
    }
}

#[cfg(test)]
mod tests {
    //! Focused coverage for shared control geometry at the integer boundary.

    use super::*;

    /// Verifies extreme positive aspect ratios clamp or fit without narrowing through a wrapped
    /// intermediate coordinate.
    #[test]
    fn scaled_visual_geometry_uses_wide_ratio_arithmetic() {
        let intrinsic = scaled_visual_content_size(Constraints::bounded(Dimensioni::new(i32::MAX, i32::MAX)), Some(Dimensioni::new(1, i32::MAX)));
        assert_eq!((intrinsic.width, intrinsic.height), (i32::MAX, i32::MAX));

        let placement = place_scaled_visual_content(Recti::new(i32::MAX, i32::MAX, i32::MAX, i32::MAX), Some(Dimensioni::new(1, i32::MAX)));
        let visual = placement.visual.expect("positive image and bounds must produce a fitted rectangle");
        assert_eq!((visual.x, visual.y, visual.width, visual.height), (i32::MAX, i32::MAX, 1, i32::MAX));
    }
}
