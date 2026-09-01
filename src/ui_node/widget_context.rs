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
use crate::math::RectExt;
use crate::render::{DisplayList, Painter, TextureId};
use crate::input::{Modifiers, MouseButton};
use crate::{KeyboardAction, KeyboardBehavior, WidgetOption};
use crate::theme::{AppearanceRole, Color, Style, VisualState};
use crate::ui_node::text_layout::control_text_position_with_font;

use super::UiInputEvent;

/// Converts one screen point into a saturated coordinate relative to `origin`.
fn local_point(origin: Vec2i, point: Vec2i) -> Vec2i {
    // Event positions and root origins are independently application-controlled. Saturating each
    // subtraction keeps localization total when their mathematical distance exceeds i32.
    Vec2i::new(point.x.saturating_sub(origin.x), point.y.saturating_sub(origin.y))
}

/// Converts one screen-space clip into coordinates relative to a content rectangle.
fn localize_clip(content_rect: Recti, screen_clip: Recti) -> Recti {
    // RectExt owns the saturated subtraction policy shared by clipping and hit testing.
    screen_clip.relative_to(Vec2i::new(content_rect.x, content_rect.y))
}

/// Converts one routed event into coordinates relative to `origin`.
pub(crate) fn localize_event(origin: Vec2i, event: UiInputEvent) -> UiInputEvent {
    match event {
        UiInputEvent::MouseMove { pos, delta } => UiInputEvent::MouseMove { pos: local_point(origin, pos), delta },
        UiInputEvent::MouseDrag { pos, delta, buttons } => UiInputEvent::MouseDrag {
            pos: local_point(origin, pos),
            delta,
            buttons,
        },
        UiInputEvent::MouseDown { pos, button } => UiInputEvent::MouseDown { pos: local_point(origin, pos), button },
        UiInputEvent::MouseUp { pos, button } => UiInputEvent::MouseUp { pos: local_point(origin, pos), button },
        UiInputEvent::Scroll { pos, delta } => UiInputEvent::Scroll { pos: local_point(origin, pos), delta },
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
        }
    }

    fn local_rect(&self) -> Recti {
        Recti::new(0, 0, self.content_rect.width, self.content_rect.height)
    }

    fn local_clip(&self) -> Recti {
        // Reuse the crate-wide saturated rectangle translation so update hit tests and paint clips
        // agree even when screen and content origins span the complete coordinate domain.
        localize_clip(self.content_rect, self.screen_clip)
    }
}

/// Interaction and state-mutation services available during [`Widget::update`](crate::Widget::update).
///
/// This context deliberately has no `DisplayList`, [`Painter`], or drawing helpers. Consequently
/// an update implementation cannot record visual work, and retained traversal does not need to
/// carry rendering state through the update pass. Interaction and focus are dispatcher-produced
/// snapshots; widgets can inspect them but cannot cooperatively assign or clear focus.
///
/// The diagnostic-matched `tests/ui/widget_update_cannot_paint.rs` contract test verifies that an
/// update context has no painter. Its adjacent passing fixture exercises the distinct public
/// capabilities exposed by update and paint contexts.
pub struct WidgetUpdateCtx<'a> {
    /// Common read-only data, intentionally separated from phase capabilities.
    common: WidgetContextData<'a>,
    /// Whether this widget is inside the current hover root.
    in_hover_root: bool,
    /// Mouse buttons held after applying the current raw input event.
    mouse_buttons: MouseButton,
    /// Modifier state committed by the latest logical keyboard transition.
    modifiers: Modifiers,
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
        mouse_buttons: MouseButton,
        modifiers: Modifiers,
    ) -> Self {
        Self::new_with_content_geometry(
            rect,
            screen_clip,
            style,
            atlas,
            in_hover_root,
            hovered,
            focused,
            clicked,
            active,
            mouse_buttons,
            modifiers,
        )
    }

    /// Creates update services for one traversal-derived content rectangle.
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
        mouse_buttons: MouseButton,
        modifiers: Modifiers,
    ) -> Self {
        Self {
            common: WidgetContextData::new(content_rect, screen_clip, style, atlas, hovered, focused, clicked, active),
            in_hover_root,
            mouse_buttons,
            modifiers,
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

    /// Resolves this update's pointer click or declared key press to one shared widget action.
    pub fn action(&self, input: Option<&UiInputEvent>, keyboard: KeyboardBehavior) -> Option<KeyboardAction> {
        if self.clicked() {
            // The router records only an eligible primary-button press as a click transition, so
            // pointer and keyboard activation can share the same semantic branch in each widget.
            Some(KeyboardAction::Activate)
        } else {
            keyboard.action(input)
        }
    }

    /// Returns whether this widget owns pointer capture while the left button is held.
    ///
    /// Widgets that retain a local drag mode must reconcile it from this value on every update.
    /// A `false` value is the complete capture-loss signal; no separate lifecycle callback runs.
    pub fn active(&self) -> bool {
        // UiRuntime commits this immutable capture snapshot before calling Widget::update.
        self.common.active
    }

    /// Returns mouse buttons held after applying the current input event.
    pub fn mouse_buttons(&self) -> MouseButton {
        self.mouse_buttons
    }

    /// Returns the modifier snapshot committed by the latest keyboard transition.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Returns this widget's resolved style for update logic.
    pub fn style(&self) -> &Style {
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
        local_rect.contains_point(mouse_pos) && self.common.local_clip().contains_point(mouse_pos)
    }
}

/// Geometry and recording services available during [`Widget::paint`](crate::Widget::paint).
///
/// Interaction values are immutable snapshots produced by update. Painting may inspect them to
/// choose visuals, but it cannot mutate focus or consume input.
pub struct WidgetPaintCtx<'a> {
    /// Common read-only data, intentionally separated from phase capabilities.
    common: WidgetContextData<'a>,
    /// Whether this widget and every visible ancestor accept interaction.
    enabled: bool,
    /// Whether the containing top-level window currently owns activation.
    window_active: bool,
    /// Display list receiving this widget's paint operations.
    display_list: &'a mut DisplayList,
}

impl<'a> WidgetPaintCtx<'a> {
    /// Creates paint services for one traversal-derived content rectangle.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_content_geometry(
        content_rect: Recti,
        display_list: &'a mut DisplayList,
        screen_clip: Recti,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        enabled: bool,
        hovered: bool,
        focused: bool,
        clicked: bool,
        active: bool,
        window_active: bool,
    ) -> Self {
        Self {
            common: WidgetContextData::new(content_rect, screen_clip, style, atlas, hovered, focused, clicked, active),
            enabled,
            window_active,
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

    /// Returns the traversal-derived effective clip in widget-local coordinates.
    pub fn local_clip(&self) -> Recti {
        // Translate the inherited screen clip through the widget's committed content origin without
        // widening it. Scrolled content can therefore cull paint work in stable local coordinates.
        self.common.local_clip()
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

    /// Returns the pointer-capture activity snapshot committed during the latest update.
    pub fn active(&self) -> bool {
        // Paint observes the retained update result and cannot alter capture ownership.
        self.common.active
    }

    /// Returns whether this widget and its visible ancestor chain accept interaction.
    pub fn enabled(&self) -> bool {
        // Participation is inherited during paint so a disabled container gives every descendant
        // the disabled theme state without rewriting each concrete widget's options.
        self.enabled
    }

    /// Returns whether this widget belongs to the manager's active top-level window.
    pub fn window_active(&self) -> bool {
        // Activation is a paint-time presentation fact and does not rewrite focus or pointer state.
        self.window_active
    }

    /// Resolves the exact semantic appearance state for this paint snapshot.
    pub fn visual_state(&self) -> VisualState {
        // Capture counts as a visible press only while the pointer remains over the widget. A drag
        // outside retains capture for correct release delivery but returns to the non-pressed art.
        VisualState::from_interaction(self.enabled, self.hovered(), self.focused(), self.active() && self.hovered())
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

    /// Returns this widget's resolved style.
    pub fn style(&self) -> &Style {
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
        // Callers resolve a semantic foreground before reaching this primitive, so explicit image
        // tints and role-specific state colors are preserved exactly.
        self.painter().icon(id, rect, color);
    }

    /// Draws an external texture through a widget-local painter.
    pub(crate) fn push_image(&mut self, image: TextureId, rect: Recti, color: Color) {
        self.painter().image(image, rect, color);
    }

    /// Draws one semantic role using this widget's resolved interaction state.
    pub(crate) fn draw_appearance(&mut self, role: AppearanceRole, rect: Recti) -> Option<Recti> {
        // Resolve state before borrowing the display list through Painter so immutable and mutable
        // context borrows do not overlap.
        let state = self.visual_state();
        self.draw_appearance_state(role, state, rect)
    }

    /// Draws one semantic role using an explicit state selected by a composite control.
    pub(crate) fn draw_appearance_state(&mut self, role: AppearanceRole, state: VisualState, rect: Recti) -> Option<Recti> {
        // Menu rows and selected list items own semantic state beyond the widget-wide interaction
        // snapshot. They still resolve the same typed catalog and checked nine-patch geometry.
        let patch = self.common.style.appearance(role, state);
        let mut painter = self.painter();
        crate::ui_node::frame::paint_internal_frame(&mut painter, rect, patch)
    }

    /// Draws only the stretchable center of one semantic role over an unframed rectangle.
    pub(crate) fn draw_appearance_center(&mut self, role: AppearanceRole, rect: Recti) {
        // Interactive controls resolve their ordinary pointer/focus state before sharing the same
        // center-only recording path used by passive containers with an explicit state.
        self.draw_appearance_center_state(role, self.visual_state(), rect);
    }

    /// Draws only the stretchable center of one semantic role using an explicit state.
    pub(crate) fn draw_appearance_center_state(&mut self, role: AppearanceRole, state: VisualState, rect: Recti) {
        // Zero destination insets collapse all eight outer cells. Flat patches retain their center
        // cell and image patches sample only their center source, preserving the old unframed fill
        // contract without introducing a second theme asset vocabulary.
        let patch = self.common.style.appearance(role, state).with_insets(crate::SliceInsets::ZERO);
        let mut painter = self.painter();
        let _ = crate::ui_node::frame::paint_internal_frame(&mut painter, rect, patch);
    }

    /// Resolves a role foreground using this widget's complete interaction state.
    pub(crate) fn foreground(&self, role: AppearanceRole) -> Color {
        // visual_state gives explicit disabling precedence over retained hover, focus, and press.
        self.common.style.foreground(role, self.visual_state())
    }

    /// Resolves a role foreground using a composite control's explicit interaction state.
    pub(crate) fn foreground_state(&self, role: AppearanceRole, state: VisualState) -> Color {
        // The composite owns this exact typed state; window activation affects chrome roles and
        // focus visibility rather than rewriting enabled widget presentation.
        self.common.style.foreground(role, state)
    }

    /// Draws aligned control text using the foreground for one semantic appearance role.
    pub(crate) fn draw_control_text_with_font(&mut self, font: FontId, text: &str, rect: Recti, role: AppearanceRole, opt: WidgetOption) {
        // Resolve the same state as the adjacent appearance before mutable display-list recording.
        let color = self.foreground(role);
        self.draw_control_text_color_with_font(font, text, rect, color, opt);
    }

    /// Draws aligned control text with an explicit font and resolved color.
    ///
    /// Most built-in controls select one complete semantic appearance role and should continue to use
    /// [`Self::draw_control_text_with_font`]. Composite controls such as menu panels need to derive
    /// a disabled-text color from that role while retaining the same padding, alignment, clipping,
    /// and glyph-placement rules. Keeping that variation here prevents each composite from
    /// duplicating the authoritative control-text geometry.
    pub(crate) fn draw_control_text_color_with_font(&mut self, font: FontId, text: &str, rect: Recti, color: Color, opt: WidgetOption) {
        // Composite callers have already resolved an exact role/state foreground. This primitive
        // owns only authoritative text geometry, clipping, and recording.
        let pos = control_text_position_with_font(self.common.style, self.common.atlas, font, text, rect, opt);
        let mut painter = self.painter();
        painter.with_clip(rect, |painter| painter.text(font, text, pos, color));
    }
}

#[cfg(test)]
mod tests {
    //! Boundary tests for traversal-to-widget coordinate conversion.

    use super::*;

    /// Verifies pointer localization saturates both directions instead of overflowing Vec2i
    /// subtraction before a widget receives the routed event.
    #[test]
    fn event_localization_handles_extreme_screen_and_widget_origins() {
        let event = localize_event(
            Vec2i::new(i32::MAX, i32::MIN),
            UiInputEvent::MouseDown {
                pos: Vec2i::new(i32::MIN, i32::MAX),
                button: MouseButton::LEFT,
            },
        );
        let UiInputEvent::MouseDown { pos, .. } = event else {
            panic!("localization must preserve the concrete event variant");
        };
        assert_eq!((pos.x, pos.y), (i32::MIN, i32::MAX));

        let clip = localize_clip(Recti::new(i32::MAX, i32::MIN, 1, 1), Recti::new(i32::MIN, i32::MAX, 1, 1));
        assert_eq!((clip.x, clip.y, clip.width, clip.height), (i32::MIN, i32::MAX, 1, 1));
    }

    /// Verifies paint state distinguishes visible presses, capture outside, and disabled ancestry.
    #[test]
    fn paint_visual_state_uses_enabled_hover_focus_and_visible_capture() {
        let atlas = crate::test_support::test_atlas();
        let style = crate::test_support::test_style(&atlas);
        let bounds = Recti::new(0, 0, 20, 10);
        let mut list = DisplayList::new();

        {
            let ctx = WidgetPaintCtx::new_with_content_geometry(bounds, &mut list, bounds, &style, &atlas, true, true, true, false, true, true);
            assert_eq!(ctx.visual_state(), VisualState::PressedFocused);
        }
        {
            // Capture remains active outside for release routing, but theme art is no longer pressed.
            let ctx = WidgetPaintCtx::new_with_content_geometry(bounds, &mut list, bounds, &style, &atlas, true, false, true, false, true, true);
            assert_eq!(ctx.visual_state(), VisualState::Focused);
        }
        {
            // Disabled ancestry wins over stale retained interaction snapshots.
            let ctx = WidgetPaintCtx::new_with_content_geometry(bounds, &mut list, bounds, &style, &atlas, false, true, true, true, true, true);
            assert_eq!(ctx.visual_state(), VisualState::Disabled);
        }
        {
            // Window activation does not turn an otherwise enabled widget into a disabled one.
            let ctx = WidgetPaintCtx::new_with_content_geometry(bounds, &mut list, bounds, &style, &atlas, true, true, true, true, true, false);
            assert_eq!(ctx.visual_state(), VisualState::PressedFocused);
        }
    }
}
