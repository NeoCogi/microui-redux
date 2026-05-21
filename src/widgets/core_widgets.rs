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
//! Shared helpers and re-exports for the basic retained widgets.
//!
//! The split widget modules keep concrete widget state small; this file holds common sizing,
//! coloring, and submit helpers used by buttons, lists, combos, checkboxes, and custom controls.
use crate::*;
/// Measures text with the widget's resolved font choice.
fn text_size(style: &Style, atlas: &AtlasHandle, font: FontChoice, text: &str) -> Dimensioni {
    atlas.get_text_size(style.resolve_font_choice(font), text)
}

/// Computes a control height that can fit both text and an optional visual element.
fn content_height(style: &Style, atlas: &AtlasHandle, font: FontChoice, visual_height: i32) -> i32 {
    let font_height = atlas.get_font_height(style.resolve_font_choice(font)) as i32;
    let vertical_pad = (style.padding / 2).max(1);
    (font_height.max(visual_height) + vertical_pad * 2).max(0)
}

/// Computes preferred size for a single-line label plus optional icon/image/slot.
fn inline_content_size(style: &Style, atlas: &AtlasHandle, font: FontChoice, label: &str, visual_size: Option<Dimensioni>) -> Dimensioni {
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
    let mut width = padding * 2 + text_size.width.max(0);
    if has_visual {
        width += visual_size.width.max(0);
        if has_text {
            width += padding;
        }
    }

    let height = content_height(style, atlas, font, visual_size.height);
    Dimensioni::new(width.max(0), height)
}

#[derive(Copy, Clone)]
/// Resolved inline placement for an optional visual and text region.
struct InlineContentLayout {
    visual: Option<Recti>,
    text: Recti,
}

/// Places an optional visual before text while keeping visual-only content centered.
fn layout_inline_content(bounds: Recti, style: &Style, label: &str, visual_size: Option<Dimensioni>) -> InlineContentLayout {
    let padding = style.padding.max(0);
    let visual_size = visual_size.unwrap_or_default();
    let has_visual = visual_size.width > 0 && visual_size.height > 0;
    let has_text = !label.is_empty();

    if !has_visual {
        // Text-only controls can use the whole bounds; text alignment is handled by draw helpers.
        return InlineContentLayout { visual: None, text: bounds };
    }

    let visual_width = visual_size.width.min((bounds.width - padding * 2).max(0)).max(0);
    let visual_height = visual_size.height.min(bounds.height.max(0)).max(0);
    let visual_y = bounds.y + ((bounds.height - visual_height) / 2).max(0);

    if !has_text {
        // Visual-only controls center the visual and do not reserve a text rect.
        let visual_x = bounds.x + ((bounds.width - visual_width) / 2).max(0);
        let visual = rect(visual_x, visual_y, visual_width, visual_height);
        return InlineContentLayout {
            visual: Some(visual),
            text: Recti::default(),
        };
    }

    let visual_x = bounds.x + padding;
    let visual = rect(visual_x, visual_y, visual_width, visual_height);
    let text_x = visual.x + visual.width;
    let right = bounds.x + bounds.width;
    let text = rect(text_x, bounds.y, (right - text_x).max(0), bounds.height);
    InlineContentLayout { visual: Some(visual), text }
}

/// Selects which control color should be painted for a widget's fill policy and state.
fn widget_fill_color(control: &ControlState, base: ControlColor, fill: WidgetFillOption) -> Option<ControlColor> {
    if control.focused && fill.fill_click() {
        let mut color = base;
        color.focus();
        Some(color)
    } else if control.hovered && fill.fill_hover() {
        let mut color = base;
        color.hover();
        Some(color)
    } else if fill.fill_normal() {
        Some(base)
    } else {
        None
    }
}

/// Converts a click state into the standard submit result.
fn submit_on_click(control: &ControlState) -> ResourceState {
    if control.clicked { ResourceState::SUBMIT } else { ResourceState::NONE }
}

mod button;
mod checkbox;
mod combo;
mod custom;
mod internal;
mod list;

pub use button::{Button, ButtonContent};
pub use checkbox::Checkbox;
pub use combo::Combo;
pub use custom::Custom;
pub use internal::Internal;
pub use list::{ListBox, ListItem};

#[cfg(test)]
mod tests;
