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

//! Shared formatting and inline editing for numeric widgets.

use std::fmt::Write;

use crate::*;

use super::textbox::{textbox_paint, textbox_update};

/// Formats a numeric value using the widget's display precision.
pub(super) fn number_label(value: Real, precision: usize) -> String {
    let mut label = String::new();
    let _ = write!(label, "{:.*}", precision, value);
    label
}

/// Computes preferred size for numeric widgets with optional visual affordance width.
pub(super) fn number_preferred_size(
    style: &Style,
    atlas: &AtlasHandle,
    font: FontChoice,
    value: Real,
    precision: usize,
    visual_width: i32,
    visual_height: i32,
) -> Dimensioni {
    let label = number_label(value, precision);
    let resolved_font = style.resolve_font_choice(font);
    let text_w = atlas.get_text_size(resolved_font, label.as_str()).width;
    let padding = style.padding.max(0);
    let vertical_pad = (padding / 2).max(1);
    let font_height = atlas.get_font_height(resolved_font) as i32;
    let width = (text_w + padding * 2 + visual_width.max(0)).max(0);
    let height = (font_height.max(visual_height.max(0)) + vertical_pad * 2).max(0);
    Dimensioni::new(width, height)
}

#[derive(Clone, Default, PartialEq)]
/// Editing buffer for number-style widgets.
pub(super) struct NumberEditState {
    /// Whether the widget is currently in edit mode.
    pub(super) editing: bool,
    /// Text buffer for numeric input.
    buf: String,
    /// Cursor position within the buffer (byte index).
    cursor: usize,
}

/// Runs the shared textbox editor for shift-click numeric input.
pub(super) fn number_textbox_update(
    ctx: &mut WidgetUpdateCtx<'_>,
    input: Option<&UiInputEvent>,
    edit: &mut NumberEditState,
    precision: usize,
    font: FontId,
    value: &mut Real,
) -> bool {
    let shift_click = matches!(input, Some(UiInputEvent::MouseDown { button, .. }) if button.intersects(MouseButton::LEFT))
        && ctx.modifiers().intersects(Modifiers::SHIFT)
        && ctx.hovered();

    if shift_click {
        edit.editing = true;
        edit.buf.clear();
        let _ = write!(edit.buf, "{:.*}", precision, value);
        edit.cursor = edit.buf.len();
    }

    if edit.editing {
        let res = textbox_update(ctx, input, &mut edit.buf, &mut edit.cursor, WidgetOption::NONE, font);
        if res.submitted || !ctx.focused() {
            if let Ok(v) = edit.buf.parse::<f32>() {
                *value = v as Real;
            }
            edit.editing = false;
            edit.cursor = 0;
        } else {
            return true;
        }
    }
    false
}

/// Paints the shared textbox editor for a numeric widget.
pub(super) fn number_textbox_paint(ctx: &mut WidgetPaintCtx<'_>, edit: &NumberEditState, font: FontId) {
    textbox_paint(ctx, edit.buf.as_str(), edit.cursor, WidgetOption::NONE, font);
}
