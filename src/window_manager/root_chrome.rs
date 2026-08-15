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

//! Persistent retained root state and the private chrome container.

use std::{cell::RefCell, rc::Rc};

use crate::render::Painter;
use crate::{
    AtlasHandle, Children, Container, ContainerLayoutCtx, ContainerWidget, ControlColor, Dimensioni, FocusPolicy, MeasureCtx, MouseButton, Node, Recti, Style,
    TypedWidgetHandle, UiInputEvent, Vec2i, Widget, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx, WindowOption,
};

use super::RootId;

/// Cloneable non-owning capability for one retained root.
///
/// The [`crate::Context`] remains the sole owner of the root and its complete tree. Cloning or
/// dropping this handle cannot extend or shorten that lifetime. Hiding the root preserves its
/// runtime and state; [`crate::Context::destroy_root`] permanently unregisters it, drops the tree,
/// and causes the weak typed widget capability to expire once active access closures finish.
#[derive(Clone)]
pub struct RootHandle {
    id: RootId,
    widget: TypedWidgetHandle<RootChrome>,
    changed: crate::WidgetEventHandle<RootChanged>,
    submitted: crate::WidgetEventHandle<RootSubmitted>,
}

impl RootHandle {
    /// Returns the lifecycle identifier accepted by [`crate::Context`] root operations.
    pub fn id(&self) -> RootId {
        self.id
    }

    /// Returns the weak typed handle for the concrete root-chrome widget.
    pub fn widget(&self) -> &TypedWidgetHandle<RootChrome> {
        &self.widget
    }

    /// Returns the native event endpoint emitted after each user-driven move or resize.
    pub fn changed(&self) -> crate::WidgetEventHandle<RootChanged> {
        self.changed.clone()
    }

    /// Returns the native event endpoint emitted for close and outside-popup submissions.
    pub fn submitted(&self) -> crate::WidgetEventHandle<RootSubmitted> {
        self.submitted.clone()
    }
}

/// Failure reported by a checked root-state mutation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootMutationError {
    /// The identifier does not name a currently registered root.
    UnknownRoot,
    /// The root widget is already borrowed by an active typed-access closure.
    Borrowed,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootInteraction {
    None,
    Moving,
    Resizing,
}

/// Concrete container widget retained by a window, dialog, or popup root.
///
/// Queries report current chrome values, including programmatic changes made through
/// [`crate::Context`]. Hiding is persistent state and does not destroy the owned application node.
/// Root content itself cannot be replaced; mutate typed descendant/container widgets or destroy and
/// recreate the root instead.
pub struct RootChrome {
    name: String,
    options: WindowOption,
    rect: Recti,
    visible: bool,
    interaction: RootInteraction,
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<RootChanged>>>,
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<RootSubmitted>>>,
    geometry: RootChromeGeometry,
    opt: WidgetOption,
}

impl RootChrome {
    fn new(
        name: String,
        options: WindowOption,
        rect: Recti,
        visible: bool,
        changed_event: Rc<RefCell<crate::event::WidgetEventPort<RootChanged>>>,
        submitted_event: Rc<RefCell<crate::event::WidgetEventPort<RootSubmitted>>>,
    ) -> Self {
        Self {
            name,
            options,
            rect,
            visible,
            interaction: RootInteraction::None,
            changed_event,
            submitted_event,
            geometry: RootChromeGeometry::default(),
            opt: WidgetOption::NONE,
        }
    }

    /// Returns the immutable registered name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the current chrome options.
    pub fn options(&self) -> WindowOption {
        self.options
    }

    /// Returns the authoritative outer rectangle in screen coordinates.
    pub fn rect(&self) -> Recti {
        self.rect
    }

    /// Returns whether the retained root is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Returns whether a title move or resize is active.
    pub fn is_active(&self) -> bool {
        self.interaction != RootInteraction::None
    }

    /// Returns whether the pointer occupies post-tree window chrome.
    pub(super) fn pointer_hits_chrome(&self, pos: Vec2i) -> bool {
        // Root chrome is private window-manager geometry, so it is resolved before generic tree
        // allocation targeting rather than exposed through the public Container contract. Stored
        // chrome is root-local while raw input remains in screen coordinates at this boundary.
        let local_pos = pos - Vec2i::new(self.rect.x, self.rect.y);
        self.geometry.hit_test(local_pos).is_some()
    }

    /// Returns whether title movement is active.
    pub fn is_moving(&self) -> bool {
        self.interaction == RootInteraction::Moving
    }

    /// Returns whether resizing is active.
    pub fn is_resizing(&self) -> bool {
        self.interaction == RootInteraction::Resizing
    }

    pub(super) fn set_rect_silent(&mut self, rect: Recti) {
        self.rect = rect;
    }

    pub(super) fn set_size_silent(&mut self, size: Dimensioni) {
        self.rect.width = size.width;
        self.rect.height = size.height;
    }

    pub(super) fn set_options_silent(&mut self, options: WindowOption) {
        self.options = options;
        if (options.intersects(WindowOption::NO_TITLE) && self.is_moving())
            || (options.intersects(WindowOption::NO_RESIZE | WindowOption::AUTO_SIZE) && self.is_resizing())
        {
            self.interaction = RootInteraction::None;
        }
    }

    pub(super) fn set_visible_silent(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.interaction = RootInteraction::None;
        }
    }

    /// Clears private chrome interaction after window-manager policy revokes runtime capture.
    pub(super) fn clear_interaction_silent(&mut self) {
        // RootChrome exposes its active mode publicly, so the window manager updates it atomically
        // with cross-root capture policy instead of waiting for a later widget traversal.
        self.interaction = RootInteraction::None;
    }

    pub(super) fn dismiss_popup(&mut self) {
        self.set_visible_silent(false);
        self.submitted_event.borrow_mut().emit(RootSubmitted::PopupDismissed);
    }
}

/// Geometry snapshot emitted after a user-driven root move or resize.
#[derive(Copy, Clone, Debug)]
pub struct RootChanged {
    /// Authoritative outer rectangle after applying the interaction.
    pub rect: Recti,
}

impl crate::WidgetEvent for RootChanged {}

/// Reason emitted when a root is submitted by its chrome or popup policy.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootSubmitted {
    /// The user submitted the root's close affordance.
    Close,
    /// The user dismissed a popup by interacting outside it.
    PopupDismissed,
}

impl crate::WidgetEvent for RootSubmitted {}

pub(super) struct RootChromeParameters {
    pub(super) name: String,
    pub(super) options: WindowOption,
    pub(super) rect: Recti,
    pub(super) visible: bool,
    pub(super) content: Node,
}

/// Builds the private root container and returns its weak widget and event capabilities.
pub(super) fn create_root_chrome(
    parameters: RootChromeParameters,
) -> (
    TypedWidgetHandle<RootChrome>,
    crate::WidgetEventHandle<RootChanged>,
    crate::WidgetEventHandle<RootSubmitted>,
    Container,
) {
    let changed_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
    let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
    let widget = RootChrome::new(
        parameters.name,
        parameters.options,
        parameters.rect,
        parameters.visible,
        changed_event.clone(),
        submitted_event.clone(),
    );
    let changed = crate::WidgetEventHandle::new(&changed_event);
    let submitted = crate::WidgetEventHandle::new(&submitted_event);
    let (handle, container) = Container::new(widget, [parameters.content]);
    (handle, changed, submitted, container)
}

impl crate::TypedWidget<RootChanged> for RootChrome {
    fn event(&self) -> crate::WidgetEventHandle<RootChanged> {
        crate::WidgetEventHandle::new(&self.changed_event)
    }
}

impl crate::TypedWidget<RootSubmitted> for RootChrome {
    fn event(&self) -> crate::WidgetEventHandle<RootSubmitted> {
        crate::WidgetEventHandle::new(&self.submitted_event)
    }
}

impl Widget for RootChrome {
    fn widget_opt(&self) -> &WidgetOption {
        // Root chrome uses dynamic effective options below; this is its static baseline.
        &self.opt
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        if self.visible { self.opt } else { self.opt | WidgetOption::NO_INTERACT }
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let initial = self.rect;
        if !ctx.active() {
            self.interaction = RootInteraction::None;
        }
        let mut submitted = None;
        if let Some(event) = input {
            match event {
                UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => match self.geometry.hit_test(*pos) {
                    Some(RootChromePart::Close) => {
                        self.set_visible_silent(false);
                        submitted = Some(RootSubmitted::Close);
                    }
                    Some(RootChromePart::Resize) => self.interaction = RootInteraction::Resizing,
                    Some(RootChromePart::Title) => self.interaction = RootInteraction::Moving,
                    None => self.interaction = RootInteraction::None,
                },
                UiInputEvent::MouseDrag { delta, .. } if ctx.active() => match self.interaction {
                    RootInteraction::Moving => {
                        self.rect.x = self.rect.x.saturating_add(delta.x);
                        self.rect.y = self.rect.y.saturating_add(delta.y);
                    }
                    RootInteraction::Resizing => {
                        self.rect.width = self.rect.width.saturating_add(delta.x).max(self.geometry.minimum_outer.width);
                        self.rect.height = self.rect.height.saturating_add(delta.y).max(self.geometry.minimum_outer.height);
                    }
                    RootInteraction::None => {}
                },
                _ => {}
            }
        }
        let changed = ((self.rect.x, self.rect.y, self.rect.width, self.rect.height) != (initial.x, initial.y, initial.width, initial.height))
            .then_some(RootChanged { rect: self.rect });
        if let Some(event) = changed {
            self.changed_event.borrow_mut().emit(event);
        }
        if let Some(event) = submitted {
            self.submitted_event.borrow_mut().emit(event);
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let outer = ctx.local_rect();
        if self.options.intersects(WindowOption::FRAME) {
            let _ = ctx.draw_internal_frame(outer, ControlColor::WindowBG);
        } else {
            ctx.draw_rect(outer, ctx.style().colors[ControlColor::WindowBG as usize]);
        }
    }

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::DragCapture
    }
}

impl ContainerWidget for RootChrome {
    fn accepts_event(&self, event: &UiInputEvent) -> bool {
        event_position(event).is_some_and(|position| self.geometry.hit_test(position).is_some()) && !matches!(event, UiInputEvent::Scroll { .. })
    }

    fn measure(&self, ctx: &mut MeasureCtx<'_>, available: Dimensioni) -> Dimensioni {
        // Resolve chrome-only minimum/insets first, then measure the one application child inside
        // that body. Auto-size and placement therefore share root_chrome_geometry.
        let minimum = root_chrome_geometry(Recti::default(), Dimensioni::default(), &self.name, self.options, ctx.style(), ctx.atlas()).minimum_outer;
        let outer = Recti::new(0, 0, available.width.max(minimum.width), available.height.max(minimum.height));
        let shell = root_chrome_geometry(outer, Dimensioni::default(), &self.name, self.options, ctx.style(), ctx.atlas());
        // Convert the outer measurement bound into remaining application-content space.
        // chrome_occupancy = outer_extent - body_extent.
        let horizontal_chrome = outer.width.saturating_sub(shell.body.width);
        let vertical_chrome = outer.height.saturating_sub(shell.body.height);
        let child_available = Dimensioni::new(
            inset_available(available.width, horizontal_chrome),
            inset_available(available.height, vertical_chrome),
        );
        let policy = ctx.child_policy(0).unwrap_or_else(crate::Policy::auto);
        let child = ctx
            .measure_child(
                0,
                Dimensioni::new(
                    policy.width.measurement_bound(child_available.width),
                    policy.height.measurement_bound(child_available.height),
                ),
            )
            .unwrap_or_default();
        let child = Dimensioni::new(
            policy.width.preferred_extent(child.width, child_available.width),
            policy.height.preferred_extent(child.height, child_available.height),
        );
        // Rebuild geometry with measured content and expose only its intrinsic outer extent.
        root_chrome_geometry(Recti::default(), child, &self.name, self.options, ctx.style(), ctx.atlas()).intrinsic_outer
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Provisional shell geometry supplies the exact measurement constraint for the child.
        let shell = root_chrome_geometry(rect, Dimensioni::default(), &self.name, self.options, ctx.style(), ctx.atlas());
        let policy = children.child_policy(0).unwrap_or_else(crate::Policy::auto);
        let child = ctx
            .measure_child(
                children,
                0,
                Dimensioni::new(
                    policy.width.measurement_bound(shell.body.width.max(1)),
                    policy.height.measurement_bound(shell.body.height.max(1)),
                ),
            )
            .unwrap_or_default();
        // Commit one snapshot used by layout, hit testing, surface paint, and overlay paint.
        self.geometry = root_chrome_geometry(rect, child, &self.name, self.options, ctx.style(), ctx.atlas());
        let body = self.geometry.body;
        // The single child is application content and is clipped to the committed body.
        let _ = ctx.layout_child(children, 0, body);
        ctx.set_children_viewport(body, Vec2i::default());
        ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
        ctx.set_child_overflow_propagation(false);
    }
}

fn event_position(event: &UiInputEvent) -> Option<Vec2i> {
    match event {
        UiInputEvent::MouseMove { pos, .. }
        | UiInputEvent::MouseDrag { pos, .. }
        | UiInputEvent::MouseDown { pos, .. }
        | UiInputEvent::MouseUp { pos, .. }
        | UiInputEvent::Scroll { pos, .. } => Some(*pos),
        _ => None,
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use crate::test_support::test_atlas;
    use crate::{Custom, CustomParameters, KeyCode, KeyMode};

    #[test]
    fn root_chrome_inactive_update_clears_a_stale_local_mode() {
        let changed_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let mut state = RootChrome::new(
            "root".to_owned(),
            WindowOption::FRAME,
            Recti::new(10, 20, 100, 80),
            true,
            changed_event,
            submitted_event,
        );
        let style = Style::default();
        let atlas = test_atlas();
        let bounds = Recti::new(0, 0, 100, 80);
        let mut ctx = WidgetUpdateCtx::new_with_interaction(
            bounds,
            bounds,
            &style,
            &atlas,
            true,
            false,
            false,
            false,
            false,
            MouseButton::NONE,
            KeyMode::NONE,
            KeyCode::NONE,
        );

        // Simulate state left by a surface that was gated while capture was invalidated. Its next
        // ordered update receives the authoritative inactive snapshot and clears the private mode.
        state.interaction = RootInteraction::Moving;
        state.update(&mut ctx, None);
        assert!(!state.is_active());
    }

    #[test]
    fn root_chrome_runtime_is_the_only_persistent_strong_widget_owner() {
        let content = Node::widget(Custom::create(CustomParameters::new("content")));
        let (state, changed, submitted, container) = create_root_chrome(RootChromeParameters {
            name: "root".to_owned(),
            options: WindowOption::FRAME,
            rect: Recti::new(10, 20, 100, 80),
            visible: true,
            content,
        });
        let consumer = state.clone();

        assert!(consumer.is_alive());
        drop(state);
        assert!(consumer.is_alive(), "weak handle lifetime is independent of other weak clones");
        drop(container);
        assert!(!consumer.is_alive());
        assert!(!changed.is_alive());
        assert!(!submitted.is_alive());
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootChromePart {
    Title,
    Close,
    Resize,
}

#[derive(Copy, Clone, Debug, Default)]
pub(super) struct RootChromeGeometry {
    pub(super) client: Recti,
    pub(super) title: Option<Recti>,
    pub(super) close: Option<Recti>,
    pub(super) body: Recti,
    pub(super) resize: Option<Recti>,
    pub(super) minimum_outer: Dimensioni,
    pub(super) intrinsic_outer: Dimensioni,
}

impl RootChromeGeometry {
    /// Classifies one pointer position against chrome parts in interaction priority order.
    fn hit_test(self, point: Vec2i) -> Option<RootChromePart> {
        // The close button overlays the title, and the resize grip may overlay the body, so test
        // both specialized controls before the remaining title surface.
        if self.close.is_some_and(|rect| rect.contains(&point)) {
            Some(RootChromePart::Close)
        } else if self.resize.is_some_and(|rect| rect.contains(&point)) {
            Some(RootChromePart::Resize)
        } else if self.title.is_some_and(|rect| rect.contains(&point)) {
            Some(RootChromePart::Title)
        } else {
            None
        }
    }
}

pub(super) fn root_chrome_geometry(
    outer: Recti,
    child_intrinsic: Dimensioni,
    name: &str,
    options: WindowOption,
    style: &Style,
    atlas: &AtlasHandle,
) -> RootChromeGeometry {
    let padding = style.padding.max(0);
    let title_height = root_titlebar_height(style, atlas);
    let border = if options.intersects(WindowOption::FRAME) {
        style.frame_border().width.max(0)
    } else {
        0
    };
    // border_extent = leading_border_width + trailing_border_width = border_width * 2.
    let border_extent = border.checked_mul(2).expect("root chrome frame extent overflowed i32");
    // padding_extent = leading_padding + trailing_padding = padding * 2.
    let padding_extent = padding.saturating_mul(2);
    let auto_width = options.intersects(WindowOption::AUTO_WIDTH);
    let auto_height = options.intersects(WindowOption::AUTO_HEIGHT);
    let mut minimum_width: i32 = if auto_width { 1 } else { 96 };
    let mut minimum_height: i32 = if auto_height { 1 } else { 64 };
    if !options.intersects(WindowOption::NO_TITLE) {
        let close_width = if options.intersects(WindowOption::NO_CLOSE) { 0 } else { title_height };
        // title_minimum_width = text_width + close_button_width + left_and_right_padding.
        let title_minimum_width = atlas
            .get_text_size(style.title_font, name)
            .width
            .saturating_add(close_width)
            .saturating_add(padding_extent);
        minimum_width = minimum_width.max(title_minimum_width);
        minimum_height = minimum_height.max(if auto_height {
            title_height
        } else {
            // title_minimum_height = title_height + top_and_bottom_padding.
            title_height.saturating_add(padding_extent)
        });
    }
    // minimum_outer_extent = minimum_client_extent + frame_border_extent.
    let minimum_outer = Dimensioni::new(
        minimum_width.checked_add(border_extent).expect("root chrome minimum width overflowed i32"),
        minimum_height.checked_add(border_extent).expect("root chrome minimum height overflowed i32"),
    );
    let title_extent = if options.intersects(WindowOption::NO_TITLE) { 0 } else { title_height };
    // intrinsic_width = child_width + horizontal_padding + frame_border_extent.
    let intrinsic_width = child_intrinsic
        .width
        .saturating_add(padding_extent)
        .checked_add(border_extent)
        .expect("root chrome intrinsic width overflowed i32")
        .max(minimum_outer.width);
    // intrinsic_height = child_height + vertical_padding + title_extent + frame_border_extent.
    let intrinsic_height = child_intrinsic
        .height
        .saturating_add(padding_extent)
        .saturating_add(title_extent)
        .checked_add(border_extent)
        .expect("root chrome intrinsic height overflowed i32")
        .max(minimum_outer.height);
    let intrinsic_outer = Dimensioni::new(intrinsic_width, intrinsic_height);

    let client = crate::ui_node::frame::frame_geometry(outer, options.intersects(WindowOption::FRAME), style).content_or_empty();
    let title =
        (!options.intersects(WindowOption::NO_TITLE)).then(|| Recti::new(client.x, client.y, client.width.max(0), title_height.min(client.height.max(0))));
    let close = title.and_then(|title| {
        (!options.intersects(WindowOption::NO_CLOSE)).then(|| {
            let width = title.height.min(title.width.max(0));
            // close_x = title_x + title_width - close_width.
            let x = title.x.saturating_add(title.width).saturating_sub(width);
            Recti::new(x, title.y, width, title.height)
        })
    });
    let mut body = client;
    if let Some(title) = title {
        // body_y = client_y + title_height; body_height = client_height - title_height.
        body.y = body.y.saturating_add(title.height);
        body.height = body.height.saturating_sub(title.height).max(0);
    }
    body = crate::expand_rect(body, -padding);
    body.width = body.width.max(0);
    body.height = body.height.max(0);
    let resize = (!options.intersects(WindowOption::AUTO_SIZE | WindowOption::NO_RESIZE)).then(|| {
        let size = style.scrollbar_size.max(0);
        // resize_origin = client_far_edge - resize_grip_extent.
        let x = client.x.saturating_add(client.width).saturating_sub(size);
        let y = client.y.saturating_add(client.height).saturating_sub(size);
        Recti::new(x, y, size.min(client.width.max(0)), size.min(client.height.max(0)))
    });
    RootChromeGeometry {
        client,
        title,
        close,
        body,
        resize,
        minimum_outer,
        intrinsic_outer,
    }
}

/// Removes root-chrome occupancy from a positive measurement bound while preserving intrinsic zero.
fn inset_available(value: i32, inset: i32) -> i32 {
    // A positive remainder stays positive because zero requests unconstrained child measurement.
    // content_bound = max(available_bound - non_negative_chrome_inset, 1).
    if value > 0 { value.saturating_sub(inset.max(0)).max(1) } else { 0 }
}

fn root_titlebar_height(style: &Style, atlas: &AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.title_font) as i32;
    let vertical_padding = (style.padding.max(0) / 2).max(1);
    // text_height = font_height + top_padding + bottom_padding.
    let text_height = font_height.saturating_add(vertical_padding.saturating_mul(2));
    style.title_height.max(text_height)
}

pub(super) fn record_root_overlay(display_list: &mut crate::render::DisplayList, viewport: Recti, state: &RootChrome, style: &Style, atlas: &AtlasHandle) {
    let geometry = root_chrome_geometry(state.rect, Dimensioni::default(), &state.name, state.options, style, atlas);
    let mut painter = Painter::screen_space(display_list, viewport);
    if let Some(title) = geometry.title {
        painter.fill_rect(title, style.colors[ControlColor::TitleBG as usize]);
        let mut text = title;
        if let Some(close) = geometry.close {
            // text_width = close_button_x - title_x.
            text.width = close.x.saturating_sub(title.x).max(0);
        }
        if text.width > 0 && text.height > 0 {
            let color = style.colors[ControlColor::TitleText as usize];
            let pos = crate::ui_node::text_layout::control_text_position_with_font(style, atlas, style.title_font, &state.name, text, WidgetOption::NONE);
            painter.with_clip(text, |painter| painter.text(style.title_font, &state.name, pos, color));
        }
        if let Some(close) = geometry.close {
            painter.icon(style.icons.close, close, style.colors[ControlColor::TitleText as usize]);
        }
    }
    if let Some(visual) = geometry
        .resize
        .filter(|resize| resize.width > 0 && resize.height > 0)
        .and_then(|resize| resize.intersect(&geometry.client))
    {
        crate::ui_node::frame::paint_internal_frame(&mut painter, visual, Some(style.colors[ControlColor::WindowBG as usize]), style.frame_border());
    }
}

pub(super) fn root_handle(
    id: RootId,
    widget: TypedWidgetHandle<RootChrome>,
    changed: crate::WidgetEventHandle<RootChanged>,
    submitted: crate::WidgetEventHandle<RootSubmitted>,
) -> RootHandle {
    RootHandle { id, widget, changed, submitted }
}
