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
//! Shared widget execution context built on top of draw command recording.

use std::rc::Rc;

use rs_math3d::{Color4b, Recti, Vec2i};

use crate::atlas::{AtlasHandle, FontId, IconId, SlotId};
use crate::canvas::Vertex;
use crate::render_command::Command;
use crate::draw_context::DrawCtx;
use crate::graphics::Graphics;
use crate::input::{ControlColor, ControlState, InputSnapshot, WidgetOption};
use crate::style::{Color, Image, Style};
use crate::widget::RetainedId;

/// Shared context passed to widget handlers.
pub struct WidgetCtx<'a> {
    /// Retained identity used for focus operations.
    interaction_id: RetainedId,
    /// Widget rectangle in container/screen coordinates.
    rect: Recti,
    /// Draw command recorder borrowed from the active container.
    draw: DrawCtx<'a>,
    /// Focus slot owned by the active container.
    focus: &'a mut Option<RetainedId>,
    /// Flag indicating whether focus was refreshed or changed this frame.
    updated_focus: &'a mut bool,
    /// Whether this widget is inside the current hover root.
    in_hover_root: bool,
    /// Optional widget-local input snapshot.
    input: Option<Rc<InputSnapshot>>,
    /// Empty fallback snapshot for widgets that did not request input.
    default_input: InputSnapshot,
}

impl<'a> WidgetCtx<'a> {
    /// Converts an input snapshot from container coordinates into widget-local coordinates.
    fn localize_input(rect: Recti, input: Option<Rc<InputSnapshot>>) -> Option<Rc<InputSnapshot>> {
        input.map(|input| {
            let mut localized = input.as_ref().clone();
            localized.mouse_pos = localized.mouse_pos - Vec2i::new(rect.x, rect.y);
            Rc::new(localized)
        })
    }

    /// Converts a screen/container-space rectangle into widget-local space.
    fn local_rect_for(&self, rect: Recti) -> Recti {
        Recti::new(rect.x - self.rect.x, rect.y - self.rect.y, rect.width, rect.height)
    }

    /// Converts a screen/container-space point into widget-local space.
    fn local_pos_for(&self, pos: Vec2i) -> Vec2i {
        pos - Vec2i::new(self.rect.x, self.rect.y)
    }

    /// Creates a widget context with a stable retained interaction identity.
    pub(crate) fn new_with_interaction(
        interaction_id: RetainedId,
        rect: Recti,
        commands: &'a mut Vec<Command>,
        triangle_vertices: &'a mut Vec<Vertex>,
        clip_stack: &'a mut Vec<Recti>,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        focus: &'a mut Option<RetainedId>,
        updated_focus: &'a mut bool,
        in_hover_root: bool,
        input: Option<Rc<InputSnapshot>>,
    ) -> Self {
        Self {
            interaction_id,
            rect,
            draw: DrawCtx::new(commands, triangle_vertices, clip_stack, style, atlas),
            focus,
            updated_focus,
            in_hover_root,
            // Widgets should see pointer coordinates relative to their own rect.
            input: Self::localize_input(rect, input),
            default_input: InputSnapshot::default(),
        }
    }

    /// Returns the widget-local rectangle for this context.
    ///
    /// The top-left corner is always `(0, 0)`. Use this with [`Self::input`] and
    /// [`Self::graphics`], which also operate in widget-local coordinates.
    pub fn local_rect(&self) -> Recti {
        Recti::new(0, 0, self.rect.width, self.rect.height)
    }

    /// Returns the widget rectangle in container/screen coordinates.
    ///
    /// Built-in paint helpers and backend-facing callbacks use this coordinate space.
    pub fn screen_rect(&self) -> Recti {
        self.rect
    }

    /// Returns the widget-local rectangle for this context.
    ///
    /// This is kept as the short geometry accessor for custom widgets. Code that needs absolute
    /// container coordinates should call [`Self::screen_rect`] explicitly.
    pub fn rect(&self) -> Recti {
        self.local_rect()
    }

    /// Returns the widget-local input snapshot for this widget, if provided.
    pub fn input(&self) -> Option<&InputSnapshot> {
        self.input.as_deref()
    }

    /// Returns a default empty input snapshot when this widget did not request one.
    pub(crate) fn input_or_default(&self) -> &InputSnapshot {
        self.input.as_deref().unwrap_or(&self.default_input)
    }

    /// Sets focus to this widget for the current frame.
    pub fn set_focus(&mut self) {
        *self.focus = Some(self.interaction_id);
        *self.updated_focus = true;
    }

    /// Clears focus from the current widget.
    pub fn clear_focus(&mut self) {
        *self.focus = None;
        *self.updated_focus = true;
    }

    /// Pushes a new clip rectangle onto the stack.
    pub(crate) fn push_clip_rect(&mut self, rect: Recti) {
        self.draw.push_clip_rect(rect);
    }

    /// Pops the current clip rectangle.
    pub(crate) fn pop_clip_rect(&mut self) {
        self.draw.pop_clip_rect();
    }

    /// Executes `f` with a widget-local 2D graphics builder.
    ///
    /// Geometry passed through this API uses coordinates relative to the current widget's top-left
    /// corner instead of container space. The builder forwards widget-local clips onto the shared
    /// draw-context clip stack, translates vertices into screen space once while recording, and
    /// flushes its retained triangle batches automatically when the closure returns.
    pub fn graphics<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Graphics<'_, 'a>),
    {
        let mut graphics = self.begin_graphics();
        f(&mut graphics);
    }

    /// Returns a widget-local graphics builder whose clip root is the widget rect itself.
    ///
    /// Use this for custom widget-local geometry. The builder starts clipped to the visible part
    /// of the widget, so local clips can only reduce visibility further.
    pub fn begin_graphics(&mut self) -> Graphics<'_, 'a> {
        Graphics::new(&mut self.draw, self.rect)
    }

    /// Starts a graphics builder for built-in widget paint helpers.
    ///
    /// Internal widget paint helpers need local coordinates but must preserve the legacy draw
    /// semantics where borders may extend a pixel beyond the widget rect. Starting the graphics
    /// builder from the current container clip instead of the widget rect keeps that behavior while
    /// still routing paint through `Graphics`.
    fn begin_widget_paint(&mut self) -> Graphics<'_, 'a> {
        let clip_root = self.draw.current_clip_rect();
        Graphics::new_with_clip_root(&mut self.draw, self.rect, clip_root)
    }

    /// Returns the current screen-space clip rectangle from the shared draw context.
    fn current_clip_rect(&self) -> Recti {
        self.draw.current_clip_rect()
    }

    /// Returns the active style.
    pub(crate) fn style(&self) -> &Style {
        self.draw.style()
    }

    /// Returns the active atlas.
    pub(crate) fn atlas(&self) -> &AtlasHandle {
        self.draw.atlas()
    }

    /// Draws a filled rectangle through the widget-local graphics path.
    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_rect(rect, color);
    }

    /// Draws a 1-pixel box outline using the supplied color.
    pub(crate) fn draw_box(&mut self, r: Recti, color: Color) {
        let rect = self.local_rect_for(r);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_box(rect, color);
    }

    /// Draws text through the widget-local graphics path.
    pub(crate) fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        let pos = self.local_pos_for(pos);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_text(font, text, pos, color);
    }

    /// Draws an atlas icon through the widget-local graphics path.
    pub(crate) fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_icon(id, rect, color);
    }

    /// Draws an atlas slot or external image through the widget-local graphics path.
    pub(crate) fn push_image(&mut self, image: Image, rect: Recti, color: Color) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_image(image, rect, color);
    }

    /// Draws a dynamic atlas slot through the widget-local graphics path.
    pub(crate) fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_slot_with_function(id, rect, color, f);
    }

    /// Draws a control frame through the widget-local graphics path.
    pub(crate) fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_frame(rect, colorid);
    }

    /// Draws a control frame with hover/focus color adjustment.
    pub(crate) fn draw_widget_frame(&mut self, control: &ControlState, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_widget_frame(control.focused, control.hovered, rect, colorid, opt);
    }

    /// Draws aligned control text with an explicit font.
    pub(crate) fn draw_control_text_with_font(&mut self, font: FontId, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_control_text_with_font(font, text, rect, colorid, opt);
    }

    /// Hit-tests a screen-space rect against widget-local input and the active clip.
    pub(crate) fn mouse_over(&self, rect: Recti) -> bool {
        let input = match self.input.as_ref() {
            Some(input) => input,
            None => return false,
        };
        if !self.in_hover_root {
            return false;
        }
        // Both the target rect and current clip are translated so the localized input can be used.
        let local_rect = self.local_rect_for(rect);
        let clip_rect = self.local_rect_for(self.current_clip_rect());
        local_rect.contains(&input.mouse_pos) && clip_rect.contains(&input.mouse_pos)
    }
}
