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
use crate::render::Vertex;
use crate::render_command::Command;
use crate::draw_context::DrawCtx;
use crate::graphics::Graphics;
use crate::input::{ControlColor, KeyCode, KeyMode, MouseButton, WidgetOption};
use crate::ui_node::UiInputEvent;
use crate::style::{Color, Image, Style};
use crate::ui_node::UiNodeId;

/// Convenience methods for a widget-local routed input batch.
pub trait WidgetInputEvents {
    /// Returns the currently held mouse buttons.
    fn mouse_down(&self) -> MouseButton;
    /// Returns mouse buttons pressed by routed events.
    fn mouse_pressed(&self) -> MouseButton;
    /// Returns the last routed mouse position, or `(0, 0)` if this frame has no routed pointer event.
    fn mouse_pos(&self) -> Vec2i;
    /// Returns accumulated routed mouse movement.
    fn mouse_delta(&self) -> Vec2i;
    /// Returns currently held modifier keys.
    fn key_mods(&self) -> KeyMode;
    /// Returns modifier keys pressed by routed events.
    fn key_pressed(&self) -> KeyMode;
    /// Returns navigation keys pressed by routed events.
    fn key_code_pressed(&self) -> KeyCode;
    /// Returns text input from routed events.
    fn text_input(&self) -> String;
    /// Returns scroll delta from routed events.
    fn scroll_delta(&self) -> Option<Vec2i>;
}

impl WidgetInputEvents for [UiInputEvent] {
    fn mouse_down(&self) -> MouseButton {
        self.iter().fold(MouseButton::NONE, |buttons, event| match event {
            UiInputEvent::MouseDrag { buttons: held, .. } => buttons | *held,
            _ => buttons,
        })
    }

    fn mouse_pressed(&self) -> MouseButton {
        self.iter().fold(MouseButton::NONE, |buttons, event| match event {
            UiInputEvent::MouseDown { button, .. } => buttons | *button,
            _ => buttons,
        })
    }

    fn mouse_pos(&self) -> Vec2i {
        self.iter()
            .rev()
            .find_map(|event| match event {
                UiInputEvent::MouseMove { pos, .. }
                | UiInputEvent::MouseDrag { pos, .. }
                | UiInputEvent::MouseDown { pos, .. }
                | UiInputEvent::MouseUp { pos, .. }
                | UiInputEvent::Scroll { pos, .. } => Some(*pos),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn mouse_delta(&self) -> Vec2i {
        self.iter().fold(Vec2i::default(), |mut delta, event| {
            match event {
                UiInputEvent::MouseMove { delta: event_delta, .. } | UiInputEvent::MouseDrag { delta: event_delta, .. } => {
                    delta.x += event_delta.x;
                    delta.y += event_delta.y;
                }
                _ => {}
            }
            delta
        })
    }

    fn key_mods(&self) -> KeyMode {
        self.iter().fold(KeyMode::NONE, |keys, event| match event {
            UiInputEvent::KeyState { keys: state } => keys | *state,
            _ => keys,
        })
    }

    fn key_pressed(&self) -> KeyMode {
        self.iter().fold(KeyMode::NONE, |keys, event| match event {
            UiInputEvent::KeyDown { key } => keys | *key,
            _ => keys,
        })
    }

    fn key_code_pressed(&self) -> KeyCode {
        self.iter().fold(KeyCode::NONE, |keys, event| match event {
            UiInputEvent::KeyCodeDown { code } => keys | *code,
            _ => keys,
        })
    }

    fn text_input(&self) -> String {
        let mut text = String::new();
        for event in self {
            if let UiInputEvent::Text { text: event_text } = event {
                text.push_str(event_text);
            }
        }
        text
    }

    fn scroll_delta(&self) -> Option<Vec2i> {
        self.iter().fold(None, |acc, event| match event {
            UiInputEvent::Scroll { delta, .. } if delta.x != 0 || delta.y != 0 => Some(*delta),
            _ => acc,
        })
    }
}

/// Converts routed events from container coordinates into widget-local coordinates.
pub(crate) fn localize_events(rect: Recti, events: Vec<UiInputEvent>) -> Vec<UiInputEvent> {
    let origin = Vec2i::new(rect.x, rect.y);
    events
        .into_iter()
        .map(|event| match event {
            UiInputEvent::MouseMove { pos, delta } => UiInputEvent::MouseMove { pos: pos - origin, delta },
            UiInputEvent::MouseDrag { pos, delta, buttons } => UiInputEvent::MouseDrag { pos: pos - origin, delta, buttons },
            UiInputEvent::MouseDown { pos, button } => UiInputEvent::MouseDown { pos: pos - origin, button },
            UiInputEvent::MouseUp { pos, button } => UiInputEvent::MouseUp { pos: pos - origin, button },
            UiInputEvent::Scroll { pos, delta } => UiInputEvent::Scroll { pos: pos - origin, delta },
            event => event,
        })
        .collect()
}

/// Shared context passed to widget handlers.
pub struct WidgetCtx<'a> {
    /// Runtime node identity used for focus operations.
    interaction_id: UiNodeId,
    /// Widget rectangle in container/screen coordinates.
    rect: Recti,
    /// Draw command recorder borrowed from the active container.
    draw: DrawCtx<'a>,
    /// Focus slot owned by the active container.
    focus: &'a mut Option<UiNodeId>,
    /// Flag indicating whether focus was refreshed or changed this frame.
    updated_focus: &'a mut bool,
    /// Whether this widget is inside the current hover root.
    in_hover_root: bool,
    /// Whether the routed pointer is currently over this widget.
    hovered: bool,
    /// Whether this widget currently owns focus.
    focused: bool,
    /// Whether this widget received the current click transition.
    clicked: bool,
    /// Whether this widget is in an active pointer interaction.
    active: bool,
    /// Scroll delta committed by update for this frame.
    scroll_delta: Option<Vec2i>,
}

impl<'a> WidgetCtx<'a> {
    /// Converts a screen/container-space rectangle into widget-local space.
    fn local_rect_for(&self, rect: Recti) -> Recti {
        Recti::new(rect.x - self.rect.x, rect.y - self.rect.y, rect.width, rect.height)
    }

    /// Converts a screen/container-space point into widget-local space.
    fn local_pos_for(&self, pos: Vec2i) -> Vec2i {
        pos - Vec2i::new(self.rect.x, self.rect.y)
    }

    /// Creates a widget context with a stable runtime interaction identity.
    pub(crate) fn new_with_interaction(
        interaction_id: UiNodeId,
        rect: Recti,
        commands: &'a mut Vec<Command>,
        triangle_vertices: &'a mut Vec<Vertex>,
        clip_stack: &'a mut Vec<Recti>,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        focus: &'a mut Option<UiNodeId>,
        updated_focus: &'a mut bool,
        in_hover_root: bool,
        hovered: bool,
        focused: bool,
        clicked: bool,
        active: bool,
        scroll_delta: Option<Vec2i>,
    ) -> Self {
        Self {
            interaction_id,
            rect,
            draw: DrawCtx::new(commands, triangle_vertices, clip_stack, style, atlas),
            focus,
            updated_focus,
            in_hover_root,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
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

    /// Converts a screen-space point into this widget's local coordinate space.
    pub fn screen_to_local_pos(&self, pos: Vec2i) -> Vec2i {
        self.local_pos_for(pos)
    }

    /// Converts a widget-local point into screen space.
    pub fn local_to_screen_pos(&self, pos: Vec2i) -> Vec2i {
        pos + Vec2i::new(self.rect.x, self.rect.y)
    }

    /// Converts a screen-space rectangle into this widget's local coordinate space.
    pub fn screen_to_local_rect(&self, rect: Recti) -> Recti {
        self.local_rect_for(rect)
    }

    /// Converts a widget-local rectangle into screen space.
    pub fn local_to_screen_rect(&self, rect: Recti) -> Recti {
        Recti::new(rect.x + self.rect.x, rect.y + self.rect.y, rect.width, rect.height)
    }

    /// Returns the widget-local rectangle for this context.
    ///
    /// This is kept as the short geometry accessor for custom widgets. Code that needs absolute
    /// container coordinates should call [`Self::screen_rect`] explicitly.
    pub fn rect(&self) -> Recti {
        self.local_rect()
    }

    /// Returns whether the pointer is currently over this widget.
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Returns whether this widget currently owns focus.
    pub fn focused(&self) -> bool {
        self.focused
    }

    /// Returns whether this widget received the current click transition.
    pub fn clicked(&self) -> bool {
        self.clicked
    }

    /// Returns whether this widget is in an active pointer interaction.
    pub fn active(&self) -> bool {
        self.active
    }

    /// Returns scroll delta routed to this widget for this frame.
    pub fn scroll_delta(&self) -> Option<Vec2i> {
        self.scroll_delta
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
    pub(crate) fn draw_widget_frame(&mut self, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let rect = self.local_rect_for(rect);
        let focused = self.focused;
        let hovered = self.hovered;
        let mut graphics = self.begin_widget_paint();
        graphics.draw_widget_frame(focused, hovered, rect, colorid, opt);
    }

    /// Draws aligned control text with an explicit font.
    pub(crate) fn draw_control_text_with_font(&mut self, font: FontId, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let rect = self.local_rect_for(rect);
        let mut graphics = self.begin_widget_paint();
        graphics.draw_control_text_with_font(font, text, rect, colorid, opt);
    }

    /// Hit-tests a screen-space rect against a widget-local mouse position and the active clip.
    pub(crate) fn mouse_over(&self, rect: Recti, mouse_pos: Vec2i) -> bool {
        if !self.in_hover_root {
            return false;
        }
        // Both the target rect and current clip are translated so the localized input can be used.
        let local_rect = self.local_rect_for(rect);
        let clip_rect = self.local_rect_for(self.current_clip_rect());
        local_rect.contains(&mouse_pos) && clip_rect.contains(&mouse_pos)
    }
}
