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
macro_rules! implement_widget {
    ($ty:ty, $handle:ident, $preferred:ident) => {
        impl Widget for $ty {
            fn widget_opt(&self) -> &WidgetOption { &self.opt }
            fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
            fn preferred_size(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni { self.$preferred(style, atlas, avail) }
            fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState { self.$handle(ctx, control) }
        }
    };
}

mod core_widgets;
mod nodes;
mod slider;
mod text_area;
mod text_edit;
mod textbox;

pub use core_widgets::*;
pub use nodes::*;
pub use slider::*;
pub use text_area::*;
pub use textbox::*;

use crate::draw_context::DrawCtx;
use crate::{
    AtlasHandle, Clip, Color, Color4b, Command, ControlColor, ControlState, FontId, IconId, Image, InputSnapshot, Recti, SlotId, Style, Vec2i, WidgetId,
    WidgetOption,
};
use std::rc::Rc;

/// Shared context passed to widget handlers.
pub struct WidgetCtx<'a> {
    id: WidgetId,
    rect: Recti,
    draw: DrawCtx<'a>,
    focus: &'a mut Option<WidgetId>,
    updated_focus: &'a mut bool,
    in_hover_root: bool,
    input: Option<Rc<InputSnapshot>>,
    default_input: InputSnapshot,
}

impl<'a> WidgetCtx<'a> {
    /// Creates a widget context for the given widget ID and rectangle.
    pub(crate) fn new(
        id: WidgetId,
        rect: Recti,
        commands: &'a mut Vec<Command>,
        clip_stack: &'a mut Vec<Recti>,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        focus: &'a mut Option<WidgetId>,
        updated_focus: &'a mut bool,
        in_hover_root: bool,
        input: Option<Rc<InputSnapshot>>,
    ) -> Self {
        Self {
            id,
            rect,
            draw: DrawCtx::new(commands, clip_stack, style, atlas),
            focus,
            updated_focus,
            in_hover_root,
            input,
            default_input: InputSnapshot::default(),
        }
    }

    /// Returns the widget identity pointer.
    pub fn id(&self) -> WidgetId { self.id }

    /// Returns the widget rectangle.
    pub fn rect(&self) -> Recti { self.rect }

    /// Returns the input snapshot for this widget, if provided.
    pub fn input(&self) -> Option<&InputSnapshot> { self.input.as_deref() }

    pub(crate) fn input_or_default(&self) -> &InputSnapshot { self.input.as_deref().unwrap_or(&self.default_input) }

    /// Sets focus to this widget for the current frame.
    pub fn set_focus(&mut self) {
        *self.focus = Some(self.id);
        *self.updated_focus = true;
    }

    /// Clears focus from the current widget.
    pub fn clear_focus(&mut self) {
        *self.focus = None;
        *self.updated_focus = true;
    }

    /// Pushes a new clip rectangle onto the stack.
    pub fn push_clip_rect(&mut self, rect: Recti) { self.draw.push_clip_rect(rect); }

    /// Pops the current clip rectangle.
    pub fn pop_clip_rect(&mut self) { self.draw.pop_clip_rect(); }

    /// Executes `f` with the provided clip rect applied.
    pub fn with_clip<F: FnOnce(&mut Self)>(&mut self, rect: Recti, f: F) {
        self.push_clip_rect(rect);
        f(self);
        self.pop_clip_rect();
    }

    fn current_clip_rect(&self) -> Recti { self.draw.current_clip_rect() }

    pub(crate) fn style(&self) -> &Style { self.draw.style() }

    pub(crate) fn atlas(&self) -> &AtlasHandle { self.draw.atlas() }

    /// Sets the current clip rectangle for subsequent draw commands.
    pub fn set_clip(&mut self, rect: Recti) { self.draw.set_clip(rect); }

    /// Returns the clipping relation between `r` and the current clip rect.
    pub fn check_clip(&self, r: Recti) -> Clip { self.draw.check_clip(r) }

    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) { self.draw.draw_rect(rect, color); }

    /// Draws a 1-pixel box outline using the supplied color.
    pub fn draw_box(&mut self, r: Recti, color: Color) { self.draw.draw_box(r, color); }

    pub(crate) fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) { self.draw.draw_text(font, text, pos, color); }

    pub(crate) fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) { self.draw.draw_icon(id, rect, color); }

    pub(crate) fn push_image(&mut self, image: Image, rect: Recti, color: Color) { self.draw.push_image(image, rect, color); }

    pub(crate) fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        self.draw.draw_slot_with_function(id, rect, color, f);
    }

    pub(crate) fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) { self.draw.draw_frame(rect, colorid); }

    pub(crate) fn draw_widget_frame(&mut self, control: &ControlState, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.draw.draw_widget_frame(control.focused, control.hovered, rect, colorid, opt);
    }

    pub(crate) fn draw_control_text(&mut self, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.draw.draw_control_text(text, rect, colorid, opt);
    }

    pub(crate) fn mouse_over(&self, rect: Recti) -> bool {
        let input = match self.input.as_ref() {
            Some(input) => input,
            None => return false,
        };
        if !self.in_hover_root {
            return false;
        }
        let clip_rect = self.current_clip_rect();
        rect.contains(&input.mouse_pos) && clip_rect.contains(&input.mouse_pos)
    }
}
