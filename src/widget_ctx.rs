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
//! Phase-specific widget execution contexts.

use rs_math3d::{Recti, Vec2i};

use crate::atlas::{AtlasHandle, FontId, IconId};
use crate::render::{DisplayList, Painter};
use crate::input::{ControlColor, KeyCode, KeyMode, MouseButton};
use crate::WidgetOption;
use crate::ui_node::UiInputEvent;
use crate::style::{Color, Style, TextureId};
use crate::text_layout::control_text_position_with_font;

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
        self.iter().fold(Vec2i::default(), |delta, event| match event {
            UiInputEvent::MouseMove { delta: event_delta, .. } | UiInputEvent::MouseDrag { delta: event_delta, .. } => delta + *event_delta,
            _ => delta,
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

/// Converts routed events from the enclosing local surface into widget content-local coordinates.
pub(crate) fn localize_events(rect: Recti, events: Vec<UiInputEvent>) -> Vec<UiInputEvent> {
    let origin = Vec2i::new(rect.x, rect.y);
    events.into_iter().map(|event| localize_event(origin, event)).collect()
}

/// Converts one routed event into coordinates relative to `origin`.
pub(crate) fn localize_event(origin: Vec2i, event: UiInputEvent) -> UiInputEvent {
    match event {
        UiInputEvent::MouseMove { pos, delta } => UiInputEvent::MouseMove { pos: pos - origin, delta },
        UiInputEvent::MouseDrag { pos, delta, buttons } => UiInputEvent::MouseDrag { pos: pos - origin, delta, buttons },
        UiInputEvent::MouseDown { pos, button } => UiInputEvent::MouseDown { pos: pos - origin, button },
        UiInputEvent::MouseUp { pos, button } => UiInputEvent::MouseUp { pos: pos - origin, button },
        UiInputEvent::Scroll { pos, delta } => UiInputEvent::Scroll { pos: pos - origin, delta },
        event => event,
    }
}

/// Read-only state shared by update and paint without sharing their capabilities.
///
/// Keeping this type private is intentional: public widgets receive one phase-specific wrapper,
/// while the runtime has one authoritative place for derived content geometry and interaction
/// snapshots.
struct WidgetContextData<'a> {
    /// Derived content allocation; the parent-owned outer border box is never stored here.
    content_rect: Recti,
    /// Effective screen-space clip derived by retained traversal.
    screen_clip: Recti,
    /// Style used by built-in widget layout and paint helpers.
    style: &'a Style,
    /// Atlas used for text and icon metrics.
    atlas: &'a AtlasHandle,
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

impl<'a> WidgetContextData<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        content_rect: Recti,
        screen_clip: Recti,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        hovered: bool,
        focused: bool,
        clicked: bool,
        active: bool,
        scroll_delta: Option<Vec2i>,
    ) -> Self {
        Self {
            content_rect,
            screen_clip,
            style,
            atlas,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        }
    }

    fn local_rect(&self) -> Recti {
        Recti::new(0, 0, self.content_rect.width, self.content_rect.height)
    }

    fn local_clip(&self) -> Recti {
        Recti::new(
            self.screen_clip.x - self.content_rect.x,
            self.screen_clip.y - self.content_rect.y,
            self.screen_clip.width,
            self.screen_clip.height,
        )
    }
}

/// Interaction and state-mutation services available during [`Widget::update`](crate::Widget::update).
///
/// This context deliberately has no `DisplayList`, [`Painter`], or drawing helpers. Consequently
/// an update implementation cannot record visual work, and retained traversal does not need to
/// carry rendering state through the update pass. Interaction and focus are router-produced
/// snapshots; widgets can inspect them but cannot cooperatively assign or clear focus.
///
/// ```compile_fail
/// use microui_redux::prelude::WidgetUpdateCtx;
///
/// fn painting_is_not_an_update_capability(ctx: &mut WidgetUpdateCtx<'_>) {
///     let _ = ctx.painter();
/// }
/// ```
pub struct WidgetUpdateCtx<'a> {
    /// Common read-only data, intentionally separated from phase capabilities.
    common: WidgetContextData<'a>,
    /// Whether this widget is inside the current hover root.
    in_hover_root: bool,
}

impl<'a> WidgetUpdateCtx<'a> {
    /// Creates an update context from router-owned interaction state.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_interaction(
        rect: Recti,
        screen_clip: Recti,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        in_hover_root: bool,
        hovered: bool,
        focused: bool,
        clicked: bool,
        active: bool,
        scroll_delta: Option<Vec2i>,
    ) -> Self {
        Self::new_with_content_geometry(rect, screen_clip, style, atlas, in_hover_root, hovered, focused, clicked, active, scroll_delta)
    }

    /// Creates update services for one traversal-derived content surface.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_content_geometry(
        content_rect: Recti,
        screen_clip: Recti,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        in_hover_root: bool,
        hovered: bool,
        focused: bool,
        clicked: bool,
        active: bool,
        scroll_delta: Option<Vec2i>,
    ) -> Self {
        Self {
            common: WidgetContextData::new(content_rect, screen_clip, style, atlas, hovered, focused, clicked, active, scroll_delta),
            in_hover_root,
        }
    }

    /// Returns the derived content rectangle in widget-local coordinates.
    ///
    /// Routed pointer positions use this same `(0, 0)`-origin coordinate space. The runtime-owned
    /// outer border and hit rectangle is not exposed.
    pub fn local_rect(&self) -> Recti {
        self.common.local_rect()
    }

    /// Returns the widget's derived content allocation in screen coordinates.
    pub fn screen_content_rect(&self) -> Recti {
        self.common.content_rect
    }

    /// Returns whether the pointer is currently over this widget.
    pub fn hovered(&self) -> bool {
        self.common.hovered
    }

    /// Returns whether this widget currently owns focus.
    pub fn focused(&self) -> bool {
        self.common.focused
    }

    /// Returns whether this widget received the current click transition.
    pub fn clicked(&self) -> bool {
        self.common.clicked
    }

    /// Returns whether this widget is in an active pointer interaction.
    pub fn active(&self) -> bool {
        self.common.active
    }

    /// Returns scroll delta routed to this widget for this frame.
    pub fn scroll_delta(&self) -> Option<Vec2i> {
        self.common.scroll_delta
    }

    /// Returns the active style for built-in update logic.
    pub(crate) fn style(&self) -> &Style {
        self.common.style
    }

    /// Returns the active atlas for built-in metric queries.
    pub(crate) fn atlas(&self) -> &AtlasHandle {
        self.common.atlas
    }

    /// Hit-tests a widget-local rectangle against a routed local pointer and the active clip.
    pub(crate) fn mouse_over(&self, local_rect: Recti, mouse_pos: Vec2i) -> bool {
        if !self.in_hover_root {
            return false;
        }
        local_rect.contains(&mouse_pos) && self.common.local_clip().contains(&mouse_pos)
    }
}

/// Geometry and recording services available during [`Widget::paint`](crate::Widget::paint).
///
/// Interaction values are immutable snapshots produced by update. Painting may inspect them to
/// choose visuals, but it cannot mutate focus or consume input.
pub struct WidgetPaintCtx<'a> {
    /// Common read-only data, intentionally separated from phase capabilities.
    common: WidgetContextData<'a>,
    /// Display list receiving this widget's paint operations.
    display_list: &'a mut DisplayList,
}

impl<'a> WidgetPaintCtx<'a> {
    /// Creates paint services for one traversal-derived content surface.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_content_geometry(
        content_rect: Recti,
        display_list: &'a mut DisplayList,
        screen_clip: Recti,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        hovered: bool,
        focused: bool,
        clicked: bool,
        active: bool,
        scroll_delta: Option<Vec2i>,
    ) -> Self {
        Self {
            common: WidgetContextData::new(content_rect, screen_clip, style, atlas, hovered, focused, clicked, active, scroll_delta),
            display_list,
        }
    }

    /// Returns the derived content rectangle in widget-local coordinates.
    ///
    /// Its top-left corner is always `(0, 0)`, and its size is the bounds used by [`Self::painter`].
    pub fn local_rect(&self) -> Recti {
        self.common.local_rect()
    }

    /// Returns the widget's derived content allocation in screen coordinates.
    pub fn screen_content_rect(&self) -> Recti {
        self.common.content_rect
    }

    /// Returns whether the pointer is currently over this widget.
    pub fn hovered(&self) -> bool {
        self.common.hovered
    }

    /// Returns whether this widget currently owns focus.
    pub fn focused(&self) -> bool {
        self.common.focused
    }

    /// Returns whether this widget received the current click transition.
    pub fn clicked(&self) -> bool {
        self.common.clicked
    }

    /// Returns whether this widget is in an active pointer interaction.
    pub fn active(&self) -> bool {
        self.common.active
    }

    /// Returns scroll delta routed to this widget for this frame.
    pub fn scroll_delta(&self) -> Option<Vec2i> {
        self.common.scroll_delta
    }

    /// Returns a widget-local painter that records directly into the current frame display list.
    ///
    /// The painter receives the widget origin, local bounds, and traversal-derived effective clip.
    /// Custom widgets can narrow that clip with [`Painter::with_clip`] without mutating traversal
    /// state.
    pub fn painter(&mut self) -> Painter<'_> {
        let screen_content_bounds = self.common.content_rect;
        let screen_clip = self.common.screen_clip;
        Painter::for_widget(&mut *self.display_list, screen_content_bounds, screen_clip)
    }

    /// Returns the active style.
    pub(crate) fn style(&self) -> &Style {
        self.common.style
    }

    /// Returns the active atlas.
    pub(crate) fn atlas(&self) -> &AtlasHandle {
        self.common.atlas
    }

    /// Draws a filled rectangle through a widget-local painter.
    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) {
        self.painter().fill_rect(rect, color);
    }

    /// Draws an atlas icon through a widget-local painter.
    pub(crate) fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        self.painter().icon(id, rect, color);
    }

    /// Draws an external texture through a widget-local painter.
    pub(crate) fn push_image(&mut self, image: TextureId, rect: Recti, color: Color) {
        self.painter().image(image, rect, color);
    }

    /// Draws an explicit widget-owned internal frame.
    pub(crate) fn draw_internal_frame(&mut self, rect: Recti, colorid: ControlColor) -> Option<Recti> {
        let color = self.common.style.colors[colorid as usize];
        let border = self.common.style.frame_border();
        let mut painter = self.painter();
        crate::frame::paint_internal_frame(&mut painter, rect, Some(color), border)
    }

    /// Draws an explicit widget-owned internal frame with interaction fill coloring.
    pub(crate) fn draw_widget_internal_frame(&mut self, rect: Recti, mut colorid: ControlColor) -> Option<Recti> {
        if self.common.focused {
            colorid.focus();
        } else if self.common.hovered {
            colorid.hover();
        }
        self.draw_internal_frame(rect, colorid)
    }

    /// Fills derived outer-frame content with interaction coloring.
    pub(crate) fn draw_widget_fill(&mut self, rect: Recti, mut colorid: ControlColor) {
        if self.common.focused {
            colorid.focus();
        } else if self.common.hovered {
            colorid.hover();
        }
        self.draw_rect(rect, self.common.style.colors[colorid as usize]);
    }

    /// Draws aligned control text with an explicit font.
    pub(crate) fn draw_control_text_with_font(&mut self, font: FontId, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let color = self.common.style.colors[colorid as usize];
        let pos = control_text_position_with_font(self.common.style, self.common.atlas, font, text, rect, opt);
        let mut painter = self.painter();
        painter.with_clip(rect, |painter| painter.text(font, text, pos, color));
    }
}
