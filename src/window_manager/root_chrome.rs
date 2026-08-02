//! Persistent retained root state and the private chrome container.

use std::{cell::RefCell, rc::Rc};

use crate::render::Painter;
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerInputCtx, ContainerInputResult, ContainerLayoutCtx,
    ContainerState, ControlColor, Dimensioni, FocusPolicy, MouseButton, Node, Recti, Style, UiInputEvent, Vec2i, Widget, WidgetOption, WidgetPaintCtx,
    WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx, WindowOption,
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
/// [`crate::Context`]. `take_changed` and `take_submitted` are counted, state-local events: each
/// successful call consumes exactly one pending occurrence. Hiding is persistent state and does
/// not destroy the owned application node. Root content itself cannot be replaced; mutate typed
/// descendant/container state or destroy and recreate the root instead.
pub struct RootState {
    name: String,
    options: WindowOption,
    children: Children,
    rect: Recti,
    visible: bool,
    interaction: RootInteraction,
    pending_changes: u32,
    pending_submissions: u32,
    geometry: RootChromeGeometry,
}

impl WidgetState for RootState {}
impl ContainerState for RootState {}

impl RootState {
    fn new(name: String, options: WindowOption, rect: Recti, visible: bool, children: Children) -> Self {
        assert_eq!(children.len(), 1, "root chrome must own exactly one application node");
        Self {
            name,
            options,
            children,
            rect,
            visible,
            interaction: RootInteraction::None,
            pending_changes: 0,
            pending_submissions: 0,
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

    /// Returns whether title movement is active.
    pub fn is_moving(&self) -> bool {
        self.interaction == RootInteraction::Moving
    }

    /// Returns whether resizing is active.
    pub fn is_resizing(&self) -> bool {
        self.interaction == RootInteraction::Resizing
    }

    /// Consumes one pending user-driven move or resize occurrence.
    pub fn take_changed(&mut self) -> bool {
        take_pending(&mut self.pending_changes)
    }

    /// Consumes one pending close or outside-popup submission occurrence.
    pub fn take_submitted(&mut self) -> bool {
        take_pending(&mut self.pending_submissions)
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

    pub(super) fn dismiss_popup(&mut self) {
        self.set_visible_silent(false);
        record_pending(&mut self.pending_submissions);
    }
}

fn record_pending(pending: &mut u32) {
    *pending = pending.saturating_add(1);
}

fn take_pending(pending: &mut u32) -> bool {
    if *pending == 0 {
        false
    } else {
        *pending -= 1;
        true
    }
}

pub(super) struct RootChromeParameters {
    pub(super) name: String,
    pub(super) options: WindowOption,
    pub(super) rect: Recti,
    pub(super) visible: bool,
    pub(super) content: Node,
}

impl WidgetParameters for RootChromeParameters {}

pub(super) struct RootChromeBuilder;

impl ContainerBuilder for RootChromeBuilder {
    type Parameters = RootChromeParameters;
    type W = RootChromeContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        RootChromeContainer {
            state: Rc::new(RefCell::new(RootState::new(
                parameters.name,
                parameters.options,
                parameters.rect,
                parameters.visible,
                core::iter::once(parameters.content).collect(),
            ))),
            opt: WidgetOption::NONE,
        }
    }
}

pub(super) struct RootChromeContainer {
    state: Rc<RefCell<RootState>>,
    opt: WidgetOption,
}

impl RootChromeContainer {
    pub(super) fn create(parameters: RootChromeParameters) -> (WidgetStateHandle<RootState>, Node) {
        let container = RootChromeBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}

impl WidgetStateOwner for RootChromeContainer {
    type State = RootState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for RootChromeContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "RootChrome::measure", |state| {
            // Resolve chrome once against the supplied bound to learn how much of each axis remains
            // available to application content. This keeps title/frame policy inside root chrome.
            let outer = Recti::new(0, 0, available.width.max(1), available.height.max(1));
            let shell = root_chrome_geometry(outer, Dimensioni::default(), &state.name, state.options, style, atlas);
            let child_available = Dimensioni::new(
                inset_available(available.width, outer.width.saturating_sub(shell.body.width)),
                inset_available(available.height, outer.height.saturating_sub(shell.body.height)),
            );
            // Child placement policy belongs to this parent and determines its measurement bound;
            // the child's own measure call still reports content only.
            let policy = state.children.child_policy(0).unwrap_or_else(crate::Policy::auto);
            let measured_available = Dimensioni::new(
                policy.width.measurement_bound(child_available.width),
                policy.height.measurement_bound(child_available.height),
            );
            let child = state.children.measure_child(0, style, atlas, measured_available).unwrap_or_default();
            let child = Dimensioni::new(
                policy.width.preferred_extent(child.width, child_available.width),
                policy.height.preferred_extent(child.height, child_available.height),
            );
            // Re-run the single chrome formula with measured content to obtain intrinsic outer size.
            root_chrome_geometry(Recti::default(), child, &state.name, state.options, style, atlas).intrinsic_outer
        })
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        runtime_update_state(&self.state, "RootChrome::update", |state| {
            let initial = state.rect;
            if let Some(event) = input {
                match event {
                    UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => match state.geometry.hit_test(*pos) {
                        Some(RootChromePart::Close) => {
                            state.set_visible_silent(false);
                            record_pending(&mut state.pending_submissions);
                        }
                        Some(RootChromePart::Resize) => state.interaction = RootInteraction::Resizing,
                        Some(RootChromePart::Title) => state.interaction = RootInteraction::Moving,
                        None => {}
                    },
                    UiInputEvent::MouseDrag { delta, buttons, .. } if buttons.intersects(MouseButton::LEFT) => match state.interaction {
                        RootInteraction::Moving => {
                            state.rect.x = state.rect.x.saturating_add(delta.x);
                            state.rect.y = state.rect.y.saturating_add(delta.y);
                        }
                        RootInteraction::Resizing => {
                            state.rect.width = state.rect.width.saturating_add(delta.x).max(state.geometry.minimum_outer.width);
                            state.rect.height = state.rect.height.saturating_add(delta.y).max(state.geometry.minimum_outer.height);
                        }
                        RootInteraction::None => {}
                    },
                    UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT) => state.interaction = RootInteraction::None,
                    _ => {}
                }
            }
            if (state.rect.x, state.rect.y, state.rect.width, state.rect.height) != (initial.x, initial.y, initial.width, initial.height) {
                record_pending(&mut state.pending_changes);
            }
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        runtime_read_state(&self.state, "RootChrome::paint", |state| {
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

impl Container for RootChromeContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        runtime_read_state(&self.state, "RootChrome::visit_children", |state| visitor.visit(&state.children));
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        runtime_update_state(&self.state, "RootChrome::visit_children_mut", |state| visitor.visit(&mut state.children));
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        runtime_update_state(&self.state, "RootChrome::layout", |state| {
            // The shell determines the actual body slot before content is measured for wrapping.
            let shell = root_chrome_geometry(rect, Dimensioni::default(), &state.name, state.options, ctx.style(), ctx.atlas());
            let policy = state.children.child_policy(0).unwrap_or_else(crate::Policy::auto);
            let child = state
                .children
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
            // Commit one geometry value used by layout, hit testing, interaction, and overlay paint.
            state.geometry = root_chrome_geometry(rect, child, &state.name, state.options, ctx.style(), ctx.atlas());
            let body = state.geometry.body;
            let _ = ctx.layout_child(&mut state.children, 0, body);
            ctx.set_children_viewport(body, Vec2i::default());
            ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
            ctx.set_child_overflow_propagation(false);
        });
    }

    fn children_visible(&self) -> bool {
        runtime_read_state(&self.state, "RootChrome::children_visible", RootState::is_visible)
    }

    fn retains_pointer_capture(&self) -> bool {
        runtime_read_state(&self.state, "RootChrome::retains_pointer_capture", RootState::is_active)
    }

    fn on_pointer_capture_lost(&mut self) {
        runtime_update_state(&self.state, "RootChrome::on_pointer_capture_lost", |state| {
            state.interaction = RootInteraction::None;
        });
    }

    fn route_input(&mut self, ctx: &mut ContainerInputCtx<'_>, event: &UiInputEvent) -> ContainerInputResult {
        let has_pointer_capture = ctx.has_pointer_capture();
        let (surface, part) = runtime_read_state(&self.state, "RootChrome::route_input", |state| {
            if has_pointer_capture && matches!(event, UiInputEvent::MouseDrag { .. } | UiInputEvent::MouseUp { .. }) {
                (Some(state.geometry.outer), None)
            } else {
                let part = event_position(event).and_then(|pos| state.geometry.hit_test(pos));
                (part.map(|part| state.geometry.rect_for(part)), part)
            }
        });
        let Some(surface) = surface else { return ContainerInputResult::Ignored };
        let result = ctx.route_widget_in_rect(event, surface, WidgetOption::NONE);
        if matches!(part, Some(RootChromePart::Close)) && result == ContainerInputResult::Captured {
            ContainerInputResult::Consumed
        } else {
            result
        }
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
    use crate::{Custom, CustomParameters};

    #[test]
    fn root_chrome_retains_capture_only_for_local_move_or_resize_mode() {
        let content = Node::widget(Custom::create(CustomParameters::new("content")));
        let mut container = RootChromeBuilder::create_container(RootChromeParameters {
            name: "root".to_owned(),
            options: WindowOption::FRAME,
            rect: Recti::new(10, 20, 100, 80),
            visible: true,
            content,
        });

        assert!(!container.retains_pointer_capture());
        container.state.borrow_mut().interaction = RootInteraction::Moving;
        assert!(container.retains_pointer_capture());
        container.on_pointer_capture_lost();
        assert!(!container.retains_pointer_capture());
        assert!(!container.state.borrow().is_active());

        container.state.borrow_mut().interaction = RootInteraction::Resizing;
        assert!(container.retains_pointer_capture());
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
    pub(super) outer: Recti,
    pub(super) client: Recti,
    pub(super) title: Option<Recti>,
    pub(super) close: Option<Recti>,
    pub(super) body: Recti,
    pub(super) resize: Option<Recti>,
    pub(super) minimum_outer: Dimensioni,
    pub(super) intrinsic_outer: Dimensioni,
}

impl RootChromeGeometry {
    fn hit_test(self, point: Vec2i) -> Option<RootChromePart> {
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

    fn rect_for(self, part: RootChromePart) -> Recti {
        match part {
            RootChromePart::Title => self.title.unwrap_or_default(),
            RootChromePart::Close => self.close.unwrap_or_default(),
            RootChromePart::Resize => self.resize.unwrap_or_default(),
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
    let border_extent = border.checked_mul(2).expect("root chrome frame extent overflowed i32");
    let auto_size = options.intersects(WindowOption::AUTO_SIZE);
    let mut minimum_width: i32 = if auto_size { 1 } else { 96 };
    let mut minimum_height: i32 = if auto_size { 1 } else { 64 };
    if !options.intersects(WindowOption::NO_TITLE) {
        let close_width = if options.intersects(WindowOption::NO_CLOSE) { 0 } else { title_height };
        minimum_width = minimum_width.max(
            atlas
                .get_text_size(style.title_font, name)
                .width
                .saturating_add(close_width)
                .saturating_add(padding.saturating_mul(2)),
        );
        minimum_height = minimum_height.max(if auto_size {
            title_height
        } else {
            title_height.saturating_add(padding.saturating_mul(2))
        });
    }
    let minimum_outer = Dimensioni::new(
        minimum_width.checked_add(border_extent).expect("root chrome minimum width overflowed i32"),
        minimum_height.checked_add(border_extent).expect("root chrome minimum height overflowed i32"),
    );
    let title_extent = if options.intersects(WindowOption::NO_TITLE) { 0 } else { title_height };
    let intrinsic_outer = Dimensioni::new(
        child_intrinsic
            .width
            .saturating_add(padding.saturating_mul(2))
            .checked_add(border_extent)
            .expect("root chrome intrinsic width overflowed i32")
            .max(minimum_outer.width),
        child_intrinsic
            .height
            .saturating_add(padding.saturating_mul(2))
            .saturating_add(title_extent)
            .checked_add(border_extent)
            .expect("root chrome intrinsic height overflowed i32")
            .max(minimum_outer.height),
    );

    let client = crate::frame::frame_geometry(outer, options.intersects(WindowOption::FRAME), style).content_or_empty();
    let title =
        (!options.intersects(WindowOption::NO_TITLE)).then(|| Recti::new(client.x, client.y, client.width.max(0), title_height.min(client.height.max(0))));
    let close = title.and_then(|title| {
        (!options.intersects(WindowOption::NO_CLOSE)).then(|| {
            let width = title.height.min(title.width.max(0));
            Recti::new(title.x.saturating_add(title.width).saturating_sub(width), title.y, width, title.height)
        })
    });
    let mut body = client;
    if let Some(title) = title {
        body.y = body.y.saturating_add(title.height);
        body.height = body.height.saturating_sub(title.height).max(0);
    }
    body = crate::expand_rect(body, -padding);
    body.width = body.width.max(0);
    body.height = body.height.max(0);
    let resize = (!options.intersects(WindowOption::AUTO_SIZE | WindowOption::NO_RESIZE)).then(|| {
        let size = style.scrollbar_size.max(0);
        Recti::new(
            client.x.saturating_add(client.width).saturating_sub(size),
            client.y.saturating_add(client.height).saturating_sub(size),
            size.min(client.width.max(0)),
            size.min(client.height.max(0)),
        )
    });
    RootChromeGeometry {
        outer,
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
    if value > 0 { value.saturating_sub(inset.max(0)).max(1) } else { 0 }
}

fn root_titlebar_height(style: &Style, atlas: &AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.title_font) as i32;
    style
        .title_height
        .max(font_height.saturating_add((style.padding.max(0) / 2).max(1).saturating_mul(2)))
}

pub(super) fn record_root_overlay(display_list: &mut crate::render::DisplayList, viewport: Recti, state: &RootState, style: &Style, atlas: &AtlasHandle) {
    let geometry = root_chrome_geometry(state.rect, Dimensioni::default(), &state.name, state.options, style, atlas);
    let mut painter = Painter::screen_space(display_list, viewport);
    if let Some(title) = geometry.title {
        painter.fill_rect(title, style.colors[ControlColor::TitleBG as usize]);
        let mut text = title;
        if let Some(close) = geometry.close {
            text.width = close.x.saturating_sub(title.x).max(0);
        }
        if text.width > 0 && text.height > 0 {
            let color = style.colors[ControlColor::TitleText as usize];
            let pos = crate::text_layout::control_text_position_with_font(style, atlas, style.title_font, &state.name, text, WidgetOption::NONE);
            painter.with_clip(text, |painter| painter.text(style.title_font, &state.name, pos, color));
        }
        if let Some(close) = geometry.close {
            painter.icon(crate::CLOSE_ICON, close, style.colors[ControlColor::TitleText as usize]);
        }
    }
    if let Some(visual) = geometry
        .resize
        .filter(|resize| resize.width > 0 && resize.height > 0)
        .and_then(|resize| resize.intersect(&geometry.client))
    {
        crate::frame::paint_internal_frame(&mut painter, visual, Some(style.colors[ControlColor::WindowBG as usize]), style.frame_border());
    }
}

pub(super) fn root_handle(id: RootId, state: WidgetStateHandle<RootState>) -> RootHandle {
    RootHandle { id, state }
}
