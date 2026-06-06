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
//! Slider and numeric entry widgets.
//!
//! Sliders support dragging, wheel increments, snapping, and shift-click text entry; `Number`
//! shares the same text-editing helpers without a slider thumb.
use crate::*;
use std::fmt::Write;
use super::WidgetConfig;

use super::textbox::{textbox_paint, textbox_update};

/// Formats a numeric value using the widget's display precision.
fn number_label(value: Real, precision: usize) -> String {
    let mut label = String::new();
    let _ = write!(label, "{:.*}", precision, value);
    label
}

/// Computes preferred size for numeric widgets with optional visual affordance width.
fn number_preferred_size(
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

/// Keeps numeric widgets active while dragging, editing, or after local state changes.
fn number_active_result(ctx: &WidgetCtx<'_>, editing: bool, changed: bool) -> ResourceState {
    if ctx.active() || editing || changed {
        ResourceState::ACTIVE
    } else {
        ResourceState::NONE
    }
}

/// Adds hold-focus while the inline numeric textbox is active.
fn number_effective_widget_opt(opt: WidgetOption, editing: bool) -> WidgetOption {
    if editing { opt | WidgetOption::HOLD_FOCUS } else { opt }
}

/// Chooses drag or text-edit focus behavior for numeric widgets.
fn number_focus_policy(editing: bool) -> FocusPolicy {
    if editing { FocusPolicy::HoldUntilBlur } else { FocusPolicy::DragCapture }
}

#[derive(Clone)]
/// Persistent state for slider widgets.
pub struct Slider {
    /// Current slider value.
    value: Real,
    /// Lower bound of the slider range.
    pub low: Real,
    /// Upper bound of the slider range.
    pub high: Real,
    /// Step size used for snapping (0 for continuous).
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Shared widget configuration.
    pub config: WidgetConfig,
    /// Text editing state for shift-click numeric entry.
    edit: NumberEditState,
}

impl Slider {
    /// Creates a slider with default widget options.
    pub fn new(value: Real, low: Real, high: Real) -> Self {
        Self {
            value: clamp_slider_value(value, low, high),
            low,
            high,
            step: 0.0,
            precision: 0,
            config: WidgetConfig::new(WidgetOption::NONE, ScrollBehavior::GRAB_SCROLL),
            edit: NumberEditState::default(),
        }
    }

    /// Creates a slider with explicit widget options.
    pub fn with_opt(value: Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value: clamp_slider_value(value, low, high),
            low,
            high,
            step,
            precision,
            config: WidgetConfig::new(opt, ScrollBehavior::GRAB_SCROLL),
            edit: NumberEditState::default(),
        }
    }

    /// Returns the current slider value.
    pub fn value(&self) -> Real {
        self.value
    }

    /// Updates the current value, clamping it to the slider range.
    pub fn set_value(&mut self, value: Real) {
        self.value = clamp_slider_value(value, self.low, self.high);
    }

    /// Returns whether the inline numeric editor is active.
    pub fn is_editing(&self) -> bool {
        self.edit.editing
    }

    /// Measures the slider track plus formatted value label.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let thumb_size = style.thumb_size.max(0);
        number_preferred_size(style, atlas, self.config.font, self.value, self.precision, thumb_size, thumb_size)
    }

    /// Updates slider value from shift-click text entry, scroll, or pointer drag.
    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>) -> ResourceState {
        let mut res = ResourceState::NONE;
        let base = ctx.screen_rect();
        let last = self.value;
        let mut v = last;
        let font = ctx.style().resolve_font_choice(self.config.font);
        if !number_textbox_update(ctx, &mut self.edit, self.precision, font, &mut v).is_none() {
            // While the text editor is active it owns state changes for this frame.
            return res;
        }
        if let Some(delta) = ctx.scroll_delta() {
            let range = self.high - self.low;
            if range != 0.0 {
                let wheel = if delta.y != 0 { delta.y.signum() } else { delta.x.signum() };
                if wheel != 0 {
                    // Wheel increments use explicit step when set, otherwise one percent range.
                    let step_amount = if self.step != 0. { self.step.abs() } else { range / 100.0 };
                    v += wheel as Real * step_amount;
                    if self.step != 0. {
                        v = snap_slider_value(v, self.low, self.step);
                    }
                }
            }
        }
        let range = self.high - self.low;
        if ctx.focused() && (!ctx.mouse_down().is_empty() || ctx.mouse_pressed().intersects(MouseButton::LEFT)) && base.width > 0 && range != 0.0 {
            // Mouse x maps linearly across the slider track.
            v = self.low + ctx.mouse_pos().x as Real * range / base.width as Real;
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

    /// Paints either the inline numeric editor or the slider track/thumb/value label.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.config.font);
        if self.edit.editing {
            number_textbox_paint(ctx, &self.edit, font);
            return;
        }

        let base = ctx.screen_rect();
        let range = self.high - self.low;
        ctx.draw_widget_frame(base, ControlColor::Base, self.config.opt);
        let w = ctx.style().thumb_size;
        let available = (base.width - w).max(0);
        let x = if range != 0.0 && available > 0 {
            ((self.value - self.low) * available as Real / range) as i32
        } else {
            0
        };
        let thumb = rect(base.x + x, base.y, w, base.height);
        ctx.draw_widget_frame(thumb, ControlColor::Button, self.config.opt);
        let label = number_label(self.value, self.precision);
        ctx.draw_control_text_with_font(font, label.as_str(), base, ControlColor::Text, self.config.opt);
    }
}

/// Snaps a value to the nearest step relative to the lower bound.
fn snap_slider_value(value: Real, low: Real, step: Real) -> Real {
    let step = step.abs();
    if step == 0.0 { value } else { low + ((value - low) / step).round() * step }
}

/// Clamps a slider value even when the range was provided high-to-low.
fn clamp_slider_value(value: Real, low: Real, high: Real) -> Real {
    let min = low.min(high);
    let max = low.max(high);
    if !value.is_finite() {
        min
    } else if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Runs the shared textbox editor for shift-click numeric input.
fn number_textbox_update(ctx: &mut WidgetCtx<'_>, edit: &mut NumberEditState, precision: usize, font: FontId, value: &mut Real) -> ResourceState {
    let shift_click = { ctx.mouse_pressed().intersects(MouseButton::LEFT) && ctx.key_mods().intersects(KeyMode::SHIFT) && ctx.hovered() };

    if shift_click {
        // Enter edit mode by seeding the textbox with the current formatted value.
        edit.editing = true;
        edit.buf.clear();
        let _ = write!(edit.buf, "{:.*}", precision, value);
        edit.cursor = edit.buf.len();
    }

    if edit.editing {
        let res = textbox_update(ctx, &mut edit.buf, &mut edit.cursor, WidgetOption::NONE, font);
        if res.is_submitted() || !ctx.focused() {
            if let Ok(v) = edit.buf.parse::<f32>() {
                *value = v as Real;
            }
            // Commit valid parsed values and leave edit mode on submit or blur.
            edit.editing = false;
            edit.cursor = 0;
        } else {
            return ResourceState::ACTIVE;
        }
    }
    ResourceState::NONE
}

/// Paints the shared textbox editor for a numeric widget.
fn number_textbox_paint(ctx: &mut WidgetCtx<'_>, edit: &NumberEditState, font: FontId) {
    textbox_paint(ctx, edit.buf.as_str(), edit.cursor, WidgetOption::NONE, font);
}

impl Widget for Slider {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.config.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>) -> ResourceState {
        let old_value = self.value;
        let old_edit = self.edit.clone();
        let mut res = self.update_widget(ctx);
        let changed = self.value != old_value || self.edit != old_edit;
        res |= number_active_result(ctx, self.edit.editing, changed);
        res
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        number_effective_widget_opt(self.config.opt, self.edit.editing)
    }

    fn focus_policy(&self) -> FocusPolicy {
        number_focus_policy(self.edit.editing)
    }
}

#[derive(Clone)]
/// Persistent state for number input widgets.
pub struct Number {
    /// Current number value.
    value: Real,
    /// Step applied when dragging.
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Shared widget configuration.
    pub config: WidgetConfig,
    /// Text editing state for shift-click numeric entry.
    edit: NumberEditState,
}

#[derive(Clone, Default, PartialEq)]
/// Editing buffer for number-style widgets.
struct NumberEditState {
    /// Whether the widget is currently in edit mode.
    editing: bool,
    /// Text buffer for numeric input.
    buf: String,
    /// Cursor position within the buffer (byte index).
    cursor: usize,
}

impl Number {
    /// Creates a number input with default widget options.
    pub fn new(value: Real, step: Real, precision: usize) -> Self {
        Self {
            value: if value.is_finite() { value } else { 0.0 },
            step,
            precision,
            config: WidgetConfig::default(),
            edit: NumberEditState::default(),
        }
    }

    /// Returns the current number value.
    pub fn value(&self) -> Real {
        self.value
    }

    /// Updates the current number value, replacing non-finite input with zero.
    pub fn set_value(&mut self, value: Real) {
        self.value = if value.is_finite() { value } else { 0.0 };
    }

    /// Returns whether the inline numeric editor is active.
    pub fn is_editing(&self) -> bool {
        self.edit.editing
    }

    /// Creates a number input with explicit widget options.
    pub fn with_opt(value: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value: if value.is_finite() { value } else { 0.0 },
            step,
            precision,
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
            edit: NumberEditState::default(),
        }
    }

    /// Measures the formatted number label.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        number_preferred_size(style, atlas, self.config.font, self.value, self.precision, 0, 0)
    }

    /// Updates number value from shift-click text entry or horizontal drag.
    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>) -> ResourceState {
        let mut res = ResourceState::NONE;
        let last = self.value;
        let font = ctx.style().resolve_font_choice(self.config.font);
        if !number_textbox_update(ctx, &mut self.edit, self.precision, font, &mut self.value).is_none() {
            // Text editing suppresses drag updates while active.
            self.set_value(self.value);
            return res;
        }
        if ctx.focused() && ctx.mouse_down().intersects(MouseButton::LEFT) {
            self.set_value(self.value + ctx.mouse_delta().x as Real * self.step);
        } else {
            self.set_value(self.value);
        }
        if self.value != last {
            res |= ResourceState::CHANGE;
        }
        res
    }

    /// Paints either the inline numeric editor or the formatted value.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>) {
        let font = ctx.style().resolve_font_choice(self.config.font);
        if self.edit.editing {
            number_textbox_paint(ctx, &self.edit, font);
            return;
        }

        let base = ctx.screen_rect();
        ctx.draw_widget_frame(base, ControlColor::Base, self.config.opt);
        let label = number_label(self.value, self.precision);
        ctx.draw_control_text_with_font(font, label.as_str(), base, ControlColor::Text, self.config.opt);
    }
}

impl Widget for Number {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.config.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>) -> ResourceState {
        let old_value = self.value;
        let old_edit = self.edit.clone();
        let mut res = self.update_widget(ctx);
        let changed = self.value != old_value || self.edit != old_edit;
        res |= number_active_result(ctx, self.edit.editing, changed);
        res
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        number_effective_widget_opt(self.config.opt, self.edit.editing)
    }

    fn focus_policy(&self) -> FocusPolicy {
        number_focus_policy(self.edit.editing)
    }
}

#[cfg(test)]
mod tests;
