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

use super::textbox::textbox_handle;

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
    /// Widget options applied to the slider.
    pub opt: WidgetOption,
    /// Behaviour options applied to the slider.
    pub bopt: WidgetBehaviourOption,
    /// Text editing state for shift-click numeric entry.
    pub edit: NumberEditState,
    id: Option<Id>,
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
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
            edit: NumberEditState::default(),
            id: None,
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
            opt,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
            edit: NumberEditState::default(),
            id: None,
        }
    }

    /// Returns a copy of the slider with an explicit ID.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    fn handle_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let base = ctx.rect();
        let last = self.value;
        let mut v = last;
        if !number_textbox_handle(ctx, control, &mut self.edit, self.precision, &mut v).is_none() {
            return res;
        }
        if let Some(delta) = control.scroll_delta {
            let range = self.high - self.low;
            if range != 0.0 {
                let wheel = if delta.y != 0 { delta.y.signum() } else { delta.x.signum() };
                if wheel != 0 {
                    let step_amount = if self.step != 0. { self.step } else { range / 100.0 };
                    v += wheel as Real * step_amount;
                    if self.step != 0. {
                        v = (v + self.step / 2 as Real) / self.step * self.step;
                    }
                }
            }
        }
        let input = ctx.input_or_default();
        let range = self.high - self.low;
        if control.focused && (!input.mouse_down.is_none() || input.mouse_pressed.is_left()) && base.width > 0 && range != 0.0 {
            v = self.low + (input.mouse_pos.x - base.x) as Real * range / base.width as Real;
            if self.step != 0. {
                v = (v + self.step / 2 as Real) / self.step * self.step;
            }
        }
        if range == 0.0 {
            v = self.low;
        }
        v = if self.high < (if self.low > v { self.low } else { v }) {
            self.high
        } else if self.low > v {
            self.low
        } else {
            v
        };
        self.value = v;
        if last != v {
            res |= ResourceState::CHANGE;
        }
        ctx.draw_widget_frame(control, base, ControlColor::Base, self.opt);
        let w = ctx.style().thumb_size;
        let available = (base.width - w).max(0);
        let x = if range != 0.0 && available > 0 {
            ((v - self.low) * available as Real / range) as i32
        } else {
            0
        };
        let thumb = rect(base.x + x, base.y, w, base.height);
        ctx.draw_widget_frame(control, thumb, ControlColor::Button, self.opt);
        self.edit.buf.clear();
        let _ = write!(self.edit.buf, "{:.*}", self.precision, self.value);
        ctx.draw_control_text(self.edit.buf.as_str(), base, ControlColor::Text, self.opt);
        res
    }
}

fn number_textbox_handle(ctx: &mut WidgetCtx<'_>, control: &ControlState, edit: &mut NumberEditState, precision: usize, value: &mut Real) -> ResourceState {
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
        let res = textbox_handle(ctx, control, &mut edit.buf, &mut edit.cursor, WidgetOption::NONE);
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

implement_widget!(Slider, handle_widget);

#[derive(Clone)]
/// Persistent state for number input widgets.
pub struct Number {
    /// Current number value.
    pub value: Real,
    /// Step applied when dragging.
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Widget options applied to the number input.
    pub opt: WidgetOption,
    /// Behaviour options applied to the number input.
    pub bopt: WidgetBehaviourOption,
    /// Text editing state for shift-click numeric entry.
    pub edit: NumberEditState,
    id: Option<Id>,
}

#[derive(Clone, Default)]
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
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
            edit: NumberEditState::default(),
            id: None,
        }
    }

    /// Creates a number input with explicit widget options.
    pub fn with_opt(value: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            step,
            precision,
            opt,
            bopt: WidgetBehaviourOption::NONE,
            edit: NumberEditState::default(),
            id: None,
        }
    }

    /// Returns a copy of the number input with an explicit ID.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    fn handle_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let base = ctx.rect();
        let last = self.value;
        if !number_textbox_handle(ctx, control, &mut self.edit, self.precision, &mut self.value).is_none() {
            return res;
        }
        let input = ctx.input_or_default();
        if control.focused && input.mouse_down.is_left() {
            self.value += input.mouse_delta.x as Real * self.step;
        }
        if self.value != last {
            res |= ResourceState::CHANGE;
        }
        ctx.draw_widget_frame(control, base, ControlColor::Base, self.opt);
        self.edit.buf.clear();
        let _ = write!(self.edit.buf, "{:.*}", self.precision, self.value);
        ctx.draw_control_text(self.edit.buf.as_str(), base, ControlColor::Text, self.opt);
        res
    }
}

implement_widget!(Number, handle_widget);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, FontEntry, SourceFormat};
    use std::rc::Rc;

    const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

    fn make_test_atlas() -> AtlasHandle {
        let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
        let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
        let entries = vec![
            (
                '_',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'a',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'b',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
        ];
        let fonts = vec![(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &entries,
            },
        )];
        let source = AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &[],
        };
        AtlasHandle::from(&source)
    }

    #[test]
    fn slider_zero_range_keeps_value() {
        let atlas = make_test_atlas();
        let style = Style::default();
        let mut commands = Vec::new();
        let mut clip_stack = Vec::new();
        let mut focus = None;
        let mut updated_focus = false;

        let mut slider = Slider::new(5.0, 5.0, 5.0);
        let id = slider.get_id();
        let rect = rect(0, 0, 100, 20);
        let text_input = String::new();
        let input = Rc::new(InputSnapshot {
            mouse_pos: vec2(50, 10),
            mouse_delta: vec2(5, 0),
            mouse_down: MouseButton::LEFT,
            mouse_pressed: MouseButton::LEFT,
            text_input,
            ..Default::default()
        });
        let mut ctx = WidgetCtx::new(
            id,
            rect,
            &mut commands,
            &mut clip_stack,
            &style,
            &atlas,
            &mut focus,
            &mut updated_focus,
            true,
            Some(input),
        );
        let control = ControlState {
            hovered: true,
            focused: true,
            clicked: false,
            active: true,
            scroll_delta: None,
        };

        let res = slider.handle(&mut ctx, &control);

        assert!(res.is_none());
        assert!(slider.value.is_finite());
        assert_eq!(slider.value, 5.0);
    }
}
