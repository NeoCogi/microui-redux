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

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use crate::render::Painter;
use crate::ui_node::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Children, Container, ContainerLayoutCtx, ContainerSurface, ControlColor, Dimensioni, FocusPolicy, Layout, MouseButton, Node, Recti, Style,
    UiInputEvent, Vec2i, Widget, WidgetOption, WidgetPaintCtx, WidgetState, WidgetStateHandle, WidgetUpdateCtx, WindowOption,
};

use super::RootId;

/// Cloneable non-owning capability for one retained root.
///
/// The [`crate::Context`] remains the sole owner of the root and its complete tree. Cloning or
/// dropping this handle cannot extend or shorten that lifetime. Hiding the root preserves its
/// runtime and state; [`crate::Context::destroy_root`] permanently unregisters it, drops the tree,
/// and causes the weak state capability to expire once active access closures finish.
#[derive(Clone)]
pub struct RootHandle {
    id: RootId,
    state: WidgetStateHandle<RootState>,
}

impl RootHandle {
    /// Returns the lifecycle identifier accepted by [`crate::Context`] root operations.
    pub fn id(&self) -> RootId {
        self.id
    }

    /// Returns the weak checked capability for current chrome state and pending root events.
    pub fn state(&self) -> &WidgetStateHandle<RootState> {
        &self.state
    }
}

/// Failure reported by a checked root-state mutation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootMutationError {
    /// The identifier does not name a currently registered root.
    UnknownRoot,
    /// The root state is already borrowed by an active state-access closure.
    Borrowed,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootInteraction {
    None,
    Moving,
    Resizing,
}

/// Application-facing state retained by a window, dialog, or popup root.
///
/// Queries report current chrome values, including programmatic changes made through
/// [`crate::Context`]. Hiding is persistent state and does not destroy the owned application node.
/// Root content itself cannot be replaced; mutate typed descendant/container state or destroy and
/// recreate the root instead.
pub struct RootState {
    name: String,
    options: WindowOption,
    rect: Recti,
    visible: bool,
    interaction: RootInteraction,
    changed_event: crate::event::WidgetEventPort<RootChanged>,
    submitted_event: crate::event::WidgetEventPort<RootSubmitted>,
    geometry: RootChromeGeometry,
}

impl WidgetState for RootState {}

impl RootState {
    fn new(name: String, options: WindowOption, rect: Recti, visible: bool) -> Self {
        Self {
            name,
            options,
            rect,
            visible,
            interaction: RootInteraction::None,
            changed_event: crate::event::WidgetEventPort::new(),
            submitted_event: crate::event::WidgetEventPort::new(),
            geometry: RootChromeGeometry::default(),
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
        // RootState exposes its active mode publicly, so the window manager updates it atomically
        // with cross-root capture policy instead of waiting for a later widget traversal.
        self.interaction = RootInteraction::None;
    }

    pub(super) fn dismiss_popup(&mut self) {
        self.set_visible_silent(false);
        self.submitted_event.emit(RootSubmitted::PopupDismissed);
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

impl WidgetStateHandle<RootState> {
    /// Returns the native event endpoint emitted after each user-driven move or resize.
    pub fn changed(&self) -> crate::WidgetEventHandle<RootState, RootChanged> {
        crate::WidgetEventHandle::new(self.clone(), |state| &mut state.changed_event)
    }

    /// Returns the native event endpoint emitted for close and outside-popup submissions.
    pub fn submitted(&self) -> crate::WidgetEventHandle<RootState, RootSubmitted> {
        crate::WidgetEventHandle::new(self.clone(), |state| &mut state.submitted_event)
    }
}

pub(super) struct RootChromeParameters {
    pub(super) name: String,
    pub(super) options: WindowOption,
    pub(super) rect: Recti,
    pub(super) visible: bool,
    pub(super) content: Node,
}

/// Builds the private root container and returns the weak state capability registered by Context.
pub(super) fn create_root_chrome(parameters: RootChromeParameters) -> (WidgetStateHandle<RootState>, Container) {
    // Allocate application-visible root state once. RootChromeLayout retains the strong owner;
    // RootHandle and all window-manager references remain weak checked capabilities.
    let state = Rc::new(RefCell::new(RootState::new(
        parameters.name,
        parameters.options,
        parameters.rect,
        parameters.visible,
    )));
    // Capture the public handle before moving state ownership into the private root layout.
    let handle = WidgetStateHandle::new(&state);
    let layout = RootChromeLayout { state: state.clone() };
    // Chrome interaction and paint use weak access so RootChromeLayout remains the sole state owner.
    let surface = RootChromeSurface {
        state: Rc::downgrade(&state),
        opt: WidgetOption::NONE,
    };
    // Root chrome is one ordinary Container: one application child, one Layout, one surface Widget.
    let container = Container::new(layout, WidgetOption::NONE, [parameters.content]).with_surface(surface);
    (handle, container)
}

/// Geometry-only root policy over the single application child.
pub(super) struct RootChromeLayout {
    state: Rc<RefCell<RootState>>,
}

/// Wheel-independent widget behavior installed on the root's own chrome surface.
struct RootChromeSurface {
    state: Weak<RefCell<RootState>>,
    opt: WidgetOption,
}

impl Widget for RootChromeSurface {
    fn widget_opt(&self) -> &WidgetOption {
        // Root chrome uses dynamic effective options below; this is its static baseline.
        &self.opt
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        // Hidden or destroyed roots cannot become pointer targets even if stale geometry remains.
        let Some(state) = self.state.upgrade() else {
            return self.opt | WidgetOption::NO_INTERACT;
        };
        runtime_read_state(&state, "RootChromeSurface::options", |state| {
            if state.visible { self.opt } else { self.opt | WidgetOption::NO_INTERACT }
        })
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        // RootChromeLayout owns the preference because it alone can inspect the application child.
        Dimensioni::default()
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // The dispatcher already selected this surface and localized pointer coordinates to root.
        let Some(state) = self.state.upgrade() else { return };
        runtime_update_state(&state, "RootChromeSurface::update", |state| {
            // Compare against the initial rectangle once so any move/resize records one occurrence.
            let initial = state.rect;
            if !ctx.active() {
                // Runtime capture is authoritative. Reconcile a stale local mode here after normal
                // release, cross-root transfer, modal exclusion, or a prior visibility gate.
                state.interaction = RootInteraction::None;
            }
            if let Some(event) = input {
                // Hit classification uses the single geometry snapshot committed by latest layout.
                match event {
                    UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => match state.geometry.hit_test(*pos) {
                        Some(RootChromePart::Close) => {
                            state.set_visible_silent(false);
                            state.submitted_event.emit(RootSubmitted::Close);
                        }
                        Some(RootChromePart::Resize) => state.interaction = RootInteraction::Resizing,
                        Some(RootChromePart::Title) => state.interaction = RootInteraction::Moving,
                        // A fresh press always replaces any mode retained from an earlier update.
                        None => state.interaction = RootInteraction::None,
                    },
                    UiInputEvent::MouseDrag { delta, .. } if ctx.active() => match state.interaction {
                        RootInteraction::Moving => {
                            // Movement changes origin only; programmed size remains authoritative.
                            // next_origin = previous_origin + pointer_delta.
                            state.rect.x = state.rect.x.saturating_add(delta.x);
                            state.rect.y = state.rect.y.saturating_add(delta.y);
                        }
                        RootInteraction::Resizing => {
                            // Chrome minimum prevents title/body geometry from becoming invalid.
                            // next_extent = max(previous_extent + pointer_delta, minimum_extent).
                            state.rect.width = state.rect.width.saturating_add(delta.x).max(state.geometry.minimum_outer.width);
                            state.rect.height = state.rect.height.saturating_add(delta.y).max(state.geometry.minimum_outer.height);
                        }
                        RootInteraction::None => {}
                    },
                    _ => {}
                }
            }
            if (state.rect.x, state.rect.y, state.rect.width, state.rect.height) != (initial.x, initial.y, initial.width, initial.height) {
                state.changed_event.emit(RootChanged { rect: state.rect });
            }
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Paint panel/frame beneath application content. Title text and affordances are submitted as
        // a post-tree overlay because they occupy the topmost root paint layer.
        let Some(state) = self.state.upgrade() else { return };
        runtime_read_state(&state, "RootChromeSurface::paint", |state| {
            let outer = ctx.local_rect();
            if state.options.intersects(WindowOption::FRAME) {
                let _ = ctx.draw_internal_frame(outer, ControlColor::WindowBG);
            } else {
                ctx.draw_rect(outer, ctx.style().colors[ControlColor::WindowBG as usize]);
            }
        });
    }

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::DragCapture
    }
}

impl ContainerSurface for RootChromeSurface {
    /// Accepts pointer events only over committed post-tree chrome geometry.
    fn accepts_event(&self, event: &UiInputEvent) -> bool {
        // Captured drag and release events bypass this geometric query in the dispatcher. Keeping
        // that rule out of RootState prevents stale widget-local modes from influencing routing.
        let Some(state) = self.state.upgrade() else { return false };
        runtime_read_state(&state, "RootChromeSurface::accepts_event", |state| {
            event_position(event).is_some_and(|position| state.geometry.hit_test(position).is_some()) && !matches!(event, UiInputEvent::Scroll { .. })
        })
    }
}

impl Layout for RootChromeLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        // Resolve chrome-only minimum/insets first, then measure the one application child inside
        // that body. Auto-size and placement therefore share root_chrome_geometry.
        let (outer, shell) = runtime_read_state(&self.state, "RootChromeLayout::measure_shell", |state| {
            let minimum = root_chrome_geometry(Recti::default(), Dimensioni::default(), &state.name, state.options, style, atlas).minimum_outer;
            let outer = Recti::new(0, 0, available.width.max(minimum.width), available.height.max(minimum.height));
            (
                outer,
                root_chrome_geometry(outer, Dimensioni::default(), &state.name, state.options, style, atlas),
            )
        });
        // Convert the outer measurement bound into remaining application-content space.
        // chrome_occupancy = outer_extent - body_extent.
        let horizontal_chrome = outer.width.saturating_sub(shell.body.width);
        let vertical_chrome = outer.height.saturating_sub(shell.body.height);
        let child_available = Dimensioni::new(
            inset_available(available.width, horizontal_chrome),
            inset_available(available.height, vertical_chrome),
        );
        let policy = children.child_policy(0).unwrap_or_else(crate::Policy::auto);
        let child = children
            .measure_child(
                0,
                style,
                atlas,
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
        runtime_read_state(&self.state, "RootChromeLayout::measure_result", |state| {
            root_chrome_geometry(Recti::default(), child, &state.name, state.options, style, atlas).intrinsic_outer
        })
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Provisional shell geometry supplies the exact measurement constraint for the child.
        let shell = runtime_read_state(&self.state, "RootChromeLayout::shell", |state| {
            root_chrome_geometry(rect, Dimensioni::default(), &state.name, state.options, ctx.style(), ctx.atlas())
        });
        let policy = children.child_policy(0).unwrap_or_else(crate::Policy::auto);
        let child = children
            .measure_child(
                0,
                ctx.style(),
                ctx.atlas(),
                Dimensioni::new(
                    policy.width.measurement_bound(shell.body.width.max(1)),
                    policy.height.measurement_bound(shell.body.height.max(1)),
                ),
            )
            .unwrap_or_default();
        // Commit one snapshot used by layout, hit testing, surface paint, and overlay paint.
        let body = runtime_update_state(&self.state, "RootChromeLayout::commit", |state| {
            state.geometry = root_chrome_geometry(rect, child, &state.name, state.options, ctx.style(), ctx.atlas());
            state.geometry.body
        });
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
        let state = Rc::new(RefCell::new(RootState::new(
            "root".to_owned(),
            WindowOption::FRAME,
            Recti::new(10, 20, 100, 80),
            true,
        )));
        let mut surface = RootChromeSurface {
            state: Rc::downgrade(&state),
            opt: WidgetOption::NONE,
        };
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
        state.borrow_mut().interaction = RootInteraction::Moving;
        surface.update(&mut ctx, None);
        assert!(!state.borrow().is_active());
    }

    #[test]
    fn root_chrome_runtime_is_the_only_persistent_strong_state_owner() {
        let content = Node::widget(Custom::create(CustomParameters::new("content")));
        let (state, container) = create_root_chrome(RootChromeParameters {
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

pub(super) fn record_root_overlay(display_list: &mut crate::render::DisplayList, viewport: Recti, state: &RootState, style: &Style, atlas: &AtlasHandle) {
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

pub(super) fn root_handle(id: RootId, state: WidgetStateHandle<RootState>) -> RootHandle {
    RootHandle { id, state }
}
