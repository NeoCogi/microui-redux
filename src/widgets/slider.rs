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
use crate::*;
use std::fmt::Write;

use super::textbox::{textbox_paint, textbox_update};

#[derive(Clone)]
/// Persistent state for slider widgets.
pub struct Slider {
    /// Current slider value.
    pub value: Real,
    /// Lower bound of the slider range.
    pub low: Real,
    /// Upper bound of the slider range.
    pub high: Real,
    /// Step size used for snapping (0 for continuous).
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Font selection used for the slider value display.
    pub font: FontChoice,
    /// Widget options applied to the slider.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the slider.
    pub scroll_behavior: ScrollBehavior,
    /// Text editing state for shift-click numeric entry.
    pub edit: NumberEditState,
}

impl Slider {
    /// Creates a slider with default widget options.
    pub fn new(value: Real, low: Real, high: Real) -> Self {
        Self {
            value,
            low,
            high,
            step: 0.0,
            precision: 0,
            font: FontChoice::default(),
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::GRAB_SCROLL,
            edit: NumberEditState::default(),
        }
    }

    /// Creates a slider with explicit widget options.
    pub fn with_opt(value: Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            low,
            high,
            step,
            precision,
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::GRAB_SCROLL,
            edit: NumberEditState::default(),
        }
    }

    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let mut label = String::new();
        let _ = write!(label, "{:.*}", self.precision, self.value);
        let font = style.resolve_font_choice(self.font);
        let text_w = atlas.get_text_size(font, label.as_str()).width;
        let padding = style.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        let font_height = atlas.get_font_height(font) as i32;
        let thumb_size = style.thumb_size.max(0);
        let width = (text_w + padding * 2 + thumb_size).max(0);
        let height = (font_height.max(thumb_size) + vertical_pad * 2).max(0);
        Dimensioni::new(width, height)
    }

    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let base = ctx.rect();
        let last = self.value;
        let mut v = last;
        let font = ctx.style().resolve_font_choice(self.font);
        if !number_textbox_update(ctx, control, &mut self.edit, self.precision, font, &mut v).is_none() {
            return res;
        }
        if let Some(delta) = control.scroll_delta {
            let range = self.high - self.low;
            if range != 0.0 {
                let wheel = if delta.y != 0 { delta.y.signum() } else { delta.x.signum() };
                if wheel != 0 {
                    let step_amount = if self.step != 0. { self.step.abs() } else { range / 100.0 };
                    v += wheel as Real * step_amount;
                    if self.step != 0. {
                        v = snap_slider_value(v, self.low, self.step);
                    }
                }
            }
        }
        let input = ctx.input_or_default();
        let range = self.high - self.low;
        if control.focused && (!input.mouse_down.is_none() || input.mouse_pressed.is_left()) && base.width > 0 && range != 0.0 {
            v = self.low + input.mouse_pos.x as Real * range / base.width as Real;
            if self.step != 0. {
                v = snap_slider_value(v, self.low, self.step);
            }
        }
        if range == 0.0 {
            v = self.low;
        }
        v = clamp_slider_value(v, self.low, self.high);
        self.value = v;
        if last != v {
            res |= ResourceState::CHANGE;
        }
        res
    }

    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let font = ctx.style().resolve_font_choice(self.font);
        if self.edit.editing {
            number_textbox_paint(ctx, control, &self.edit, font);
            return;
        }

        let base = ctx.rect();
        let range = self.high - self.low;
        ctx.draw_widget_frame(control, base, ControlColor::Base, self.opt);
        let w = ctx.style().thumb_size;
        let available = (base.width - w).max(0);
        let x = if range != 0.0 && available > 0 {
            ((self.value - self.low) * available as Real / range) as i32
        } else {
            0
        };
        let thumb = rect(base.x + x, base.y, w, base.height);
        ctx.draw_widget_frame(control, thumb, ControlColor::Button, self.opt);
        let mut label = String::new();
        let _ = write!(label, "{:.*}", self.precision, self.value);
        ctx.draw_control_text_with_font(font, label.as_str(), base, ControlColor::Text, self.opt);
    }
}

fn snap_slider_value(value: Real, low: Real, step: Real) -> Real {
    let step = step.abs();
    if step == 0.0 { value } else { low + ((value - low) / step).round() * step }
}

fn clamp_slider_value(value: Real, low: Real, high: Real) -> Real {
    let min = low.min(high);
    let max = low.max(high);
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

fn number_textbox_update(
    ctx: &mut WidgetCtx<'_>,
    control: &ControlState,
    edit: &mut NumberEditState,
    precision: usize,
    font: FontId,
    value: &mut Real,
) -> ResourceState {
    let shift_click = {
        let input = ctx.input_or_default();
        input.mouse_pressed.is_left() && input.key_mods.is_shift() && control.hovered
    };

    if shift_click {
        edit.editing = true;
        edit.buf.clear();
        let _ = write!(edit.buf, "{:.*}", precision, value);
        edit.cursor = edit.buf.len();
    }

    if edit.editing {
        let res = textbox_update(ctx, control, &mut edit.buf, &mut edit.cursor, WidgetOption::NONE, font);
        if res.is_submitted() || !control.focused {
            if let Ok(v) = edit.buf.parse::<f32>() {
                *value = v as Real;
            }
            edit.editing = false;
            edit.cursor = 0;
        } else {
            return ResourceState::ACTIVE;
        }
    }
    ResourceState::NONE
}

fn number_textbox_paint(ctx: &mut WidgetCtx<'_>, control: &ControlState, edit: &NumberEditState, font: FontId) {
    textbox_paint(ctx, control, edit.buf.as_str(), edit.cursor, WidgetOption::NONE, font);
}

impl Widget for Slider {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let old_value = self.value;
        let old_edit = self.edit.clone();
        let mut res = self.update_widget(ctx, control);
        let changed = self.value != old_value || self.edit != old_edit;
        if control.active || self.edit.editing || changed {
            res |= ResourceState::ACTIVE;
        }
        res
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        self.paint_widget(ctx, control);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        if self.edit.editing { self.opt | WidgetOption::HOLD_FOCUS } else { self.opt }
    }

    fn focus_policy(&self) -> FocusPolicy {
        if self.edit.editing {
            FocusPolicy::HoldUntilBlur
        } else {
            FocusPolicy::DragCapture
        }
    }

    fn needs_input_snapshot(&self) -> bool {
        true
    }
}

#[derive(Clone)]
/// Persistent state for number input widgets.
pub struct Number {
    /// Current number value.
    pub value: Real,
    /// Step applied when dragging.
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Font selection used for the number display.
    pub font: FontChoice,
    /// Widget options applied to the number input.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the number input.
    pub scroll_behavior: ScrollBehavior,
    /// Text editing state for shift-click numeric entry.
    pub edit: NumberEditState,
}

#[derive(Clone, Default, PartialEq)]
/// Editing buffer for number-style widgets.
pub struct NumberEditState {
    /// Whether the widget is currently in edit mode.
    pub editing: bool,
    /// Text buffer for numeric input.
    pub buf: String,
    /// Cursor position within the buffer (byte index).
    pub cursor: usize,
}

impl Number {
    /// Creates a number input with default widget options.
    pub fn new(value: Real, step: Real, precision: usize) -> Self {
        Self {
            value,
            step,
            precision,
            font: FontChoice::default(),
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            edit: NumberEditState::default(),
        }
    }

    /// Creates a number input with explicit widget options.
    pub fn with_opt(value: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            step,
            precision,
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::NONE,
            edit: NumberEditState::default(),
        }
    }

    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let mut label = String::new();
        let _ = write!(label, "{:.*}", self.precision, self.value);
        let font = style.resolve_font_choice(self.font);
        let text_w = atlas.get_text_size(font, label.as_str()).width;
        let padding = style.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        let font_height = atlas.get_font_height(font) as i32;
        let width = (text_w + padding * 2).max(0);
        let height = (font_height + vertical_pad * 2).max(0);
        Dimensioni::new(width, height)
    }

    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let last = self.value;
        let font = ctx.style().resolve_font_choice(self.font);
        if !number_textbox_update(ctx, control, &mut self.edit, self.precision, font, &mut self.value).is_none() {
            return res;
        }
        let input = ctx.input_or_default();
        if control.focused && input.mouse_down.is_left() {
            self.value += input.mouse_delta.x as Real * self.step;
        }
        if self.value != last {
            res |= ResourceState::CHANGE;
        }
        res
    }

    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let font = ctx.style().resolve_font_choice(self.font);
        if self.edit.editing {
            number_textbox_paint(ctx, control, &self.edit, font);
            return;
        }

        let base = ctx.rect();
        ctx.draw_widget_frame(control, base, ControlColor::Base, self.opt);
        let mut label = String::new();
        let _ = write!(label, "{:.*}", self.precision, self.value);
        ctx.draw_control_text_with_font(font, label.as_str(), base, ControlColor::Text, self.opt);
    }
}

impl Widget for Number {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let old_value = self.value;
        let old_edit = self.edit.clone();
        let mut res = self.update_widget(ctx, control);
        let changed = self.value != old_value || self.edit != old_edit;
        if control.active || self.edit.editing || changed {
            res |= ResourceState::ACTIVE;
        }
        res
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        self.paint_widget(ctx, control);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        if self.edit.editing { self.opt | WidgetOption::HOLD_FOCUS } else { self.opt }
    }

    fn focus_policy(&self) -> FocusPolicy {
        if self.edit.editing {
            FocusPolicy::HoldUntilBlur
        } else {
            FocusPolicy::DragCapture
        }
    }

    fn needs_input_snapshot(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
