use std::{cell::RefCell, rc::Rc};

use bitflags::bitflags;

use crate::scrollbar::{scrollbar_base, scrollbar_max_scroll, ScrollAxis, ScrollbarGeometry};
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, ControlColor, Dimensioni, FocusPolicy, MouseButton, Recti, Style, UiInputEvent, Vec2i, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters,
    WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};

use super::{
    Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerInputCtx, ContainerInputResult, ContainerLayoutCtx, ContainerState,
    Node,
};

bitflags! {
    #[derive(Copy, Clone)]
    /// Options fixed when a retained scroll area is constructed.
    pub struct ScrollAreaOption : u32 {
        /// Gives the scroll area a Style-owned outer border and inset content area.
        const FRAME = 1024;
        /// Enables scrolling and scrollbars initially.
        const ENABLE_SCROLL = 32;
        /// No special options.
        const NONE = 0;
    }
}

/// One-shot construction input for a retained scroll area.
///
/// Framing is fixed here. Initial scrolling enablement moves into [`ScrollAreaState`] and remains
/// mutable after mounting.
pub struct ScrollAreaParameters {
    children: Children,
    opt: ScrollAreaOption,
}

impl WidgetParameters for ScrollAreaParameters {}

impl ScrollAreaParameters {
    /// Creates a scroll area that owns `children` in iterator order.
    pub fn new(opt: ScrollAreaOption, children: impl IntoIterator<Item = Node>) -> Self {
        Self {
            children: children.into_iter().collect(),
            opt,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum DragAxis {
    Horizontal,
    Vertical,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct ScrollbarPresence {
    vertical: bool,
    horizontal: bool,
}

impl ScrollbarPresence {
    fn union(self, other: Self) -> Self {
        Self {
            vertical: self.vertical || other.vertical,
            horizontal: self.horizontal || other.horizontal,
        }
    }
}

/// Geometry fixed by the surface, padding, and current scrollbar-presence candidate.
#[derive(Copy, Clone, Debug, Default)]
struct ScrollAreaFrame {
    surface: Recti,
    body: Recti,
    content_view: Recti,
    vertical_track: Option<Recti>,
    horizontal_track: Option<Recti>,
    corner: Option<Recti>,
}

impl ScrollAreaFrame {
    fn new(surface: Recti, padding: i32, scrollbar_size: i32, presence: ScrollbarPresence) -> Self {
        let surface = Recti::new(surface.x, surface.y, surface.width.max(0), surface.height.max(0));
        let scrollbar_size = scrollbar_size.max(0);
        let vertical_width = if presence.vertical { scrollbar_size.min(surface.width) } else { 0 };
        let horizontal_height = if presence.horizontal { scrollbar_size.min(surface.height) } else { 0 };
        let body = Recti::new(
            surface.x,
            surface.y,
            surface.width.saturating_sub(vertical_width).max(0),
            surface.height.saturating_sub(horizontal_height).max(0),
        );
        let content_view = inset_rect(body, padding.max(0));
        let vertical_track = (vertical_width > 0 && body.height > 0).then(|| scrollbar_base(ScrollAxis::Vertical, body, vertical_width));
        let horizontal_track = (horizontal_height > 0 && body.width > 0).then(|| scrollbar_base(ScrollAxis::Horizontal, body, horizontal_height));
        let corner = (vertical_width > 0 && horizontal_height > 0).then(|| {
            Recti::new(
                body.x.saturating_add(body.width),
                body.y.saturating_add(body.height),
                vertical_width,
                horizontal_height,
            )
        });

        Self {
            surface,
            body,
            content_view,
            vertical_track,
            horizontal_track,
            corner,
        }
    }

    fn required_scrollbars(self, child_extent: Dimensioni, scrollbar_size: i32) -> ScrollbarPresence {
        let usable = scrollbar_size > 0 && self.surface.width > 0 && self.surface.height > 0;
        ScrollbarPresence {
            vertical: usable && child_extent.height > self.content_view.height,
            horizontal: usable && child_extent.width > self.content_view.width,
        }
    }

    fn commit(self, child_extent: Dimensioni, requested_offset: Vec2i, min_thumb_len: i32) -> ScrollAreaGeometry {
        let child_extent = Dimensioni::new(child_extent.width.max(0), child_extent.height.max(0));
        let max_offset = Vec2i::new(
            scrollbar_max_scroll(child_extent.width, self.content_view.width),
            scrollbar_max_scroll(child_extent.height, self.content_view.height),
        );
        let offset = Vec2i::new(requested_offset.x.clamp(0, max_offset.x), requested_offset.y.clamp(0, max_offset.y));
        let vertical = self.vertical_track.map(|track| {
            ScrollbarGeometry::new(
                ScrollAxis::Vertical,
                track,
                self.content_view.height,
                child_extent.height,
                offset.y,
                min_thumb_len,
            )
        });
        let horizontal = self.horizontal_track.map(|track| {
            ScrollbarGeometry::new(
                ScrollAxis::Horizontal,
                track,
                self.content_view.width,
                child_extent.width,
                offset.x,
                min_thumb_len,
            )
        });
        ScrollAreaGeometry {
            surface: self.surface,
            body: self.body,
            content_view: self.content_view,
            child_extent,
            offset,
            max_offset,
            vertical,
            horizontal,
            corner: self.corner,
        }
    }
}

/// Committed geometry used unchanged by routing, update, and paint.
#[derive(Copy, Clone, Debug, Default)]
struct ScrollAreaGeometry {
    surface: Recti,
    body: Recti,
    content_view: Recti,
    child_extent: Dimensioni,
    offset: Vec2i,
    max_offset: Vec2i,
    vertical: Option<ScrollbarGeometry>,
    horizontal: Option<ScrollbarGeometry>,
    corner: Option<Recti>,
}

impl ScrollAreaGeometry {
    fn clamp_offset(&mut self) {
        self.offset.x = self.offset.x.clamp(0, self.max_offset.x);
        self.offset.y = self.offset.y.clamp(0, self.max_offset.y);
    }

    fn disable(&mut self) {
        self.offset = Vec2i::default();
        self.max_offset = Vec2i::default();
        self.vertical = None;
        self.horizontal = None;
        self.corner = None;
    }

    fn child_translation(self) -> Vec2i {
        Vec2i::new(
            self.content_view.x.saturating_sub(self.offset.x),
            self.content_view.y.saturating_sub(self.offset.y),
        )
    }

    fn track_at(self, pos: Vec2i) -> Option<(DragAxis, ScrollbarGeometry)> {
        if let Some(vertical) = self.vertical.filter(|bar| bar.track().contains(&pos)) {
            return Some((DragAxis::Vertical, vertical));
        }
        self.horizontal.filter(|bar| bar.track().contains(&pos)).map(|bar| (DragAxis::Horizontal, bar))
    }

    fn wheel_route_rect(self, pos: Vec2i, delta: Vec2i) -> Option<Recti> {
        let hit_rect = if self.content_view.contains(&pos) {
            Some(self.content_view)
        } else if let Some(vertical) = self.vertical.filter(|bar| bar.track().contains(&pos)) {
            Some(vertical.track())
        } else {
            self.horizontal.filter(|bar| bar.track().contains(&pos)).map(ScrollbarGeometry::track)
        }?;
        let next = Vec2i::new(
            self.offset.x.saturating_add(delta.x).clamp(0, self.max_offset.x),
            self.offset.y.saturating_add(delta.y).clamp(0, self.max_offset.y),
        );
        ((next.x, next.y) != (self.offset.x, self.offset.y)).then_some(hit_rect)
    }

    fn apply_wheel(&mut self, delta: Vec2i) {
        self.offset.x = self.offset.x.saturating_add(delta.x);
        self.offset.y = self.offset.y.saturating_add(delta.y);
        self.clamp_offset();
    }

    fn apply_drag(&mut self, axis: DragAxis, delta: Vec2i) {
        match axis {
            DragAxis::Vertical => {
                if let Some(bar) = self.vertical {
                    self.offset.y = self.offset.y.saturating_add(bar.drag_delta(delta));
                }
            }
            DragAxis::Horizontal => {
                if let Some(bar) = self.horizontal {
                    self.offset.x = self.offset.x.saturating_add(bar.drag_delta(delta));
                }
            }
        }
        self.clamp_offset();
    }

    fn center_on(&mut self, axis: DragAxis, pos: Vec2i) {
        let bar = match axis {
            DragAxis::Vertical => self.vertical,
            DragAxis::Horizontal => self.horizontal,
        };
        let Some(bar) = bar else { return };
        if bar.thumb().contains(&pos) {
            return;
        }
        match axis {
            DragAxis::Vertical => self.offset.y = bar.centered_offset(pos),
            DragAxis::Horizontal => self.offset.x = bar.centered_offset(pos),
        }
    }
}

fn inset_rect(rect: Recti, amount: i32) -> Recti {
    let amount = amount.max(0);
    Recti::new(
        rect.x.saturating_add(amount.min(rect.width.max(0))),
        rect.y.saturating_add(amount.min(rect.height.max(0))),
        rect.width.saturating_sub(amount.saturating_mul(2)).max(0),
        rect.height.saturating_sub(amount.saturating_mul(2)).max(0),
    )
}

/// Application-facing state and direct child owner for a scroll area.
///
/// This is the sole mounted authority for ordered membership, content offset, and whether
/// scrolling is enabled. Derived geometry and local drag state remain runtime-managed.
pub struct ScrollAreaState {
    children: Children,
    scrolling_enabled: bool,
    drag_axis: Option<DragAxis>,
    geometry: ScrollAreaGeometry,
}

impl WidgetState for ScrollAreaState {}
impl ContainerState for ScrollAreaState {}

impl ScrollAreaState {
    /// Returns the number of owned children.
    pub fn len(&self) -> usize {
        self.children.len()
    }
    /// Returns whether the area owns no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
    /// Appends one unmounted child.
    pub fn push(&mut self, node: Node) {
        self.children.push(node);
    }
    /// Inserts a child or returns it unchanged when `index > len`.
    #[allow(clippy::result_large_err)]
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.children.insert(index, node)
    }
    /// Drops one indexed child and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> bool {
        self.children.remove_drop(index)
    }
    /// Drops all children.
    pub fn clear(&mut self) {
        self.children.clear();
    }
    /// Replaces all children in iterator order.
    pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>) {
        self.children.replace(nodes);
    }

    /// Returns the current non-negative content offset.
    pub fn offset(&self) -> Vec2i {
        self.geometry.offset
    }
    /// Requests a content offset; negative components clamp immediately.
    pub fn set_offset(&mut self, offset: Vec2i) {
        self.geometry.offset = if self.scrolling_enabled {
            Vec2i::new(offset.x.max(0), offset.y.max(0))
        } else {
            Vec2i::default()
        };
    }
    /// Returns whether scrolling and scrollbar interaction are enabled.
    pub fn scrolling_enabled(&self) -> bool {
        self.scrolling_enabled
    }
    /// Enables or disables scrolling.
    ///
    /// Disabling synchronously clears local drag state and resets the offset.
    pub fn set_scrolling_enabled(&mut self, enabled: bool) {
        self.scrolling_enabled = enabled;
        if !enabled {
            self.drag_axis = None;
            self.geometry.disable();
        }
    }

    fn clamp_offset(&mut self) {
        if !self.scrolling_enabled {
            self.geometry.offset = Vec2i::default();
            return;
        }
        self.geometry.clamp_offset();
    }
}

/// Concrete state-owning scroll-area runtime.
pub struct ScrollAreaContainer {
    state: Rc<RefCell<ScrollAreaState>>,
    widget_opt: WidgetOption,
}

impl Widget for ScrollAreaContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.widget_opt
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        runtime_read_state(&self.state, "ScrollArea::effective_widget_opt", |state| {
            if state.scrolling_enabled {
                self.widget_opt | WidgetOption::GRAB_SCROLL
            } else {
                self.widget_opt
            }
        })
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "ScrollArea::measure", |state| measure_scroll_area(state, style, atlas, available))
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        runtime_update_state(&self.state, "ScrollArea::update", |state| {
            update_scroll_state(state, input);
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        runtime_read_state(&self.state, "ScrollArea::paint", |state| paint_scroll_area(state, ctx));
    }

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::DragCapture
    }
}

impl WidgetStateOwner for ScrollAreaContainer {
    type State = ScrollAreaState;
    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Container for ScrollAreaContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        runtime_read_state(&self.state, "ScrollArea::visit_children", |state| visitor.visit(&state.children));
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        runtime_update_state(&self.state, "ScrollArea::visit_children_mut", |state| visitor.visit(&mut state.children));
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        runtime_update_state(&self.state, "ScrollArea::layout", |state| layout_scroll_area(state, ctx, rect));
    }

    fn retains_pointer_capture(&self) -> bool {
        runtime_read_state(&self.state, "ScrollArea::retains_pointer_capture", |state| {
            state.scrolling_enabled && state.drag_axis.is_some()
        })
    }

    fn on_pointer_capture_lost(&mut self) {
        runtime_update_state(&self.state, "ScrollArea::on_pointer_capture_lost", |state| {
            state.drag_axis = None;
        });
    }

    fn route_input(&mut self, ctx: &mut ContainerInputCtx<'_>, event: &UiInputEvent) -> ContainerInputResult {
        let has_pointer_capture = ctx.has_pointer_capture();
        let surface = runtime_read_state(&self.state, "ScrollArea::route_input", |state| route_surface(state, event, has_pointer_capture));
        let Some(surface) = surface else { return ContainerInputResult::Ignored };
        ctx.route_widget_in_rect(event, surface, self.effective_widget_opt())
    }
}

/// Builder associating [`ScrollAreaParameters`] with [`ScrollAreaContainer`].
pub struct ScrollAreaBuilder;

impl ContainerBuilder for ScrollAreaBuilder {
    type Parameters = ScrollAreaParameters;
    type W = ScrollAreaContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        let scrolling_enabled = parameters.opt.intersects(ScrollAreaOption::ENABLE_SCROLL);
        let widget_opt = if parameters.opt.intersects(ScrollAreaOption::FRAME) {
            WidgetOption::FRAME
        } else {
            WidgetOption::NONE
        };
        ScrollAreaContainer {
            state: Rc::new(RefCell::new(ScrollAreaState {
                children: parameters.children,
                scrolling_enabled,
                drag_axis: None,
                geometry: ScrollAreaGeometry::default(),
            })),
            widget_opt,
        }
    }
}

/// Convenience constructor namespace for retained scroll areas.
pub struct ScrollArea;

impl ScrollArea {
    /// Creates a state-owned scroll area and its weak application capability.
    pub fn create(parameters: ScrollAreaParameters) -> (WidgetStateHandle<ScrollAreaState>, Node) {
        let container = ScrollAreaBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}

fn measure_scroll_area(state: &ScrollAreaState, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    let padding = style.padding.max(0);
    let inner = Dimensioni::new(
        available.width.saturating_sub(padding.saturating_mul(2)).max(0),
        available.height.saturating_sub(padding.saturating_mul(2)).max(0),
    );
    super::super::add_padding(measure_children(state, style, atlas, inner), padding)
}

fn measure_children(state: &ScrollAreaState, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    let mut width = 0;
    let mut height: i32 = 0;
    for index in 0..state.children.len() {
        let child = state.children.measure_child(index, style, atlas, available).unwrap_or_default();
        width = width.max(child.width);
        height = height.saturating_add(child.height);
        if index + 1 < state.children.len() {
            height = height.saturating_add(style.spacing);
        }
    }
    Dimensioni::new(width.max(0), height.max(0))
}

fn layout_scroll_area(state: &mut ScrollAreaState, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
    let padding = ctx.style().padding.max(0);
    let scrollbar_size = ctx.style().scrollbar_size.max(0);
    let requested_offset = state.geometry.offset;
    let mut presence = ScrollbarPresence::default();
    let mut committed = None;

    // There are only four possible presence states. Starting without bars and only adding a bar
    // once overflow requires it makes convergence monotonic and strictly bounded.
    for _ in 0..4 {
        let frame = ScrollAreaFrame::new(rect, padding, scrollbar_size, presence);
        let child_extent = layout_children(state, ctx, Dimensioni::new(frame.content_view.width, frame.content_view.height));
        let required = if state.scrolling_enabled {
            frame.required_scrollbars(child_extent, scrollbar_size)
        } else {
            ScrollbarPresence::default()
        };
        let next = presence.union(required);
        if next == presence {
            committed = Some(frame.commit(child_extent, requested_offset, ctx.style().thumb_size.max(0)));
            break;
        }
        presence = next;
    }

    let mut geometry = committed.expect("scroll-area scrollbar presence must converge within four states");
    if !state.scrolling_enabled {
        geometry.disable();
    }
    state.geometry = geometry;
    state.clamp_offset();

    ctx.set_children_viewport(state.geometry.content_view, state.geometry.child_translation());
    ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
    ctx.set_child_overflow_propagation(false);
}

fn layout_children(state: &mut ScrollAreaState, ctx: &mut ContainerLayoutCtx<'_>, view: Dimensioni) -> Dimensioni {
    let child_width = view.width.max(0);
    let child_height = view.height.max(0);
    let mut y: i32 = 0;
    let mut width = 0;
    for index in 0..state.children.len() {
        let preferred = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(child_width, child_height))
            .unwrap_or_default();
        let offered_width = child_width.max(preferred.width);
        let size = ctx
            .layout_child(&mut state.children, index, Recti::new(0, y, offered_width, preferred.height))
            .unwrap_or_default();
        width = width.max(offered_width.max(size.width));
        y = y.saturating_add(preferred.height);
        if index + 1 < state.children.len() {
            y = y.saturating_add(ctx.style().spacing);
        }
    }
    Dimensioni::new(width.max(0), y.max(0))
}

fn route_surface(state: &ScrollAreaState, event: &UiInputEvent, has_pointer_capture: bool) -> Option<Recti> {
    if !state.scrolling_enabled {
        return None;
    }
    match *event {
        UiInputEvent::Scroll { pos, delta } => state.geometry.wheel_route_rect(pos, delta),
        UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => state.geometry.track_at(pos).map(|(_, bar)| bar.track()),
        UiInputEvent::MouseDrag { buttons, .. } if has_pointer_capture && state.drag_axis.is_some() && buttons.intersects(MouseButton::LEFT) => {
            Some(state.geometry.surface)
        }
        UiInputEvent::MouseUp { button, .. } if has_pointer_capture && state.drag_axis.is_some() && button.intersects(MouseButton::LEFT) => {
            Some(state.geometry.surface)
        }
        _ => None,
    }
}

fn update_scroll_state(state: &mut ScrollAreaState, event: Option<&UiInputEvent>) {
    if !state.scrolling_enabled {
        state.geometry.offset = Vec2i::default();
        state.drag_axis = None;
        return;
    }
    if let Some(event) = event {
        match *event {
            UiInputEvent::Scroll { delta, .. } => {
                state.geometry.apply_wheel(delta);
            }
            UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => {
                if let Some((axis, _)) = state.geometry.track_at(pos) {
                    state.drag_axis = Some(axis);
                    state.geometry.center_on(axis, pos);
                }
            }
            UiInputEvent::MouseDrag { delta, buttons, .. } if buttons.intersects(MouseButton::LEFT) => {
                if let Some(axis) = state.drag_axis {
                    state.geometry.apply_drag(axis, delta);
                }
            }
            UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT) => state.drag_axis = None,
            _ => {}
        }
        state.clamp_offset();
    }
}

fn paint_scroll_area(state: &ScrollAreaState, ctx: &mut WidgetPaintCtx<'_>) {
    ctx.draw_rect(state.geometry.surface, ctx.style().colors[ControlColor::PanelBG as usize]);
    if let Some(bar) = state.geometry.vertical {
        ctx.draw_rect(bar.track(), ctx.style().colors[ControlColor::ScrollBase as usize]);
        ctx.draw_rect(bar.thumb(), ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }
    if let Some(bar) = state.geometry.horizontal {
        ctx.draw_rect(bar.track(), ctx.style().colors[ControlColor::ScrollBase as usize]);
        ctx.draw_rect(bar.thumb(), ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }
    if let Some(corner) = state.geometry.corner {
        ctx.draw_rect(corner, ctx.style().colors[ControlColor::PanelBG as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;
    use crate::ui_node::UiRuntime;
    use crate::{Custom, CustomParameters, Policy, UNCLIPPED_RECT};

    #[test]
    fn scroll_area_owns_direct_children_without_synthetic_semantic_nodes() {
        let child = Custom::create(CustomParameters::new("child"));
        let child_state = child.state_handle();
        let (scroll, node) = ScrollArea::create(ScrollAreaParameters::new(
            ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
            [Node::widget(child)],
        ));

        assert_eq!(scroll.try_read(ScrollAreaState::len), Some(1));
        assert_eq!(node.debug_node_count(), 2);
        scroll.try_update(|state| state.set_offset(Vec2i::new(-4, 12))).unwrap();
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 12)));
        scroll.try_update(|state| state.set_scrolling_enabled(false)).unwrap();
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 0)));
        assert_eq!(scroll.try_read(ScrollAreaState::scrolling_enabled), Some(false));

        assert_eq!(scroll.try_update(|state| state.remove_drop(0)), Some(true));
        assert!(!child_state.is_alive());
        drop(node);
        assert!(!scroll.is_alive());
    }

    #[test]
    fn capture_retention_tracks_enabled_drag_and_loss_clears_only_drag_mode() {
        let mut container = ScrollAreaBuilder::create_container(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, []));
        {
            let mut state = container.state.borrow_mut();
            state.drag_axis = Some(DragAxis::Vertical);
            state.geometry.offset = Vec2i::new(3, 7);
        }
        assert!(container.retains_pointer_capture());

        container.on_pointer_capture_lost();
        assert!(!container.retains_pointer_capture());
        assert_eq!((container.state.borrow().offset().x, container.state.borrow().offset().y), (3, 7));

        {
            let mut state = container.state.borrow_mut();
            state.drag_axis = Some(DragAxis::Horizontal);
            state.set_scrolling_enabled(false);
            state.set_scrolling_enabled(true);
        }
        assert!(!container.retains_pointer_capture(), "disable/re-enable must not resurrect the old drag");
    }

    #[test]
    fn drag_and_release_require_a_matching_left_track_press() {
        let container = ScrollAreaBuilder::create_container(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, []));
        let mut state = container.state.borrow_mut();
        state.geometry = ScrollAreaFrame::new(Recti::new(0, 0, 80, 60), 0, 10, ScrollbarPresence { vertical: true, horizontal: false }).commit(
            Dimensioni::new(70, 120),
            Vec2i::default(),
            8,
        );
        let drag = UiInputEvent::MouseDrag {
            pos: Vec2i::new(200, 180),
            delta: Vec2i::new(1, 2),
            buttons: MouseButton::LEFT,
        };

        assert!(route_surface(&state, &drag, false).is_none());
        assert!(route_surface(&state, &drag, true).is_none(), "tree capture alone is not a local drag lease");

        state.drag_axis = Some(DragAxis::Vertical);
        let captured = route_surface(&state, &drag, true).expect("captured drag must route before local update");
        assert_eq!((captured.x, captured.y, captured.width, captured.height), (0, 0, 80, 60));
        assert!(route_surface(&state, &drag, false).is_none());

        let right_down = UiInputEvent::MouseDown {
            pos: Vec2i::new(75, 10),
            button: MouseButton::RIGHT,
        };
        assert!(route_surface(&state, &right_down, false).is_none());
    }

    #[test]
    fn content_view_includes_padding_and_mutually_induced_bars_converge_monotonically() {
        let fits = ScrollAreaFrame::new(Recti::new(0, 0, 100, 100), 10, 10, ScrollbarPresence::default());
        assert_eq!((fits.content_view.width, fits.content_view.height), (80, 80));
        assert_eq!(fits.required_scrollbars(Dimensioni::new(80, 80), 10), ScrollbarPresence::default());

        let child_extent = Dimensioni::new(95, 101);
        let mut presence = ScrollbarPresence::default();
        let mut visited = 0;
        let final_frame = loop {
            visited += 1;
            let frame = ScrollAreaFrame::new(Recti::new(0, 0, 100, 100), 0, 10, presence);
            let next = presence.union(frame.required_scrollbars(child_extent, 10));
            if next == presence {
                break frame;
            }
            presence = next;
        };

        assert!(visited <= 4);
        assert_eq!(presence, ScrollbarPresence { vertical: true, horizontal: true });
        assert_eq!((final_frame.body.width, final_frame.body.height), (90, 90));
        assert!(final_frame.corner.is_some());
    }

    #[test]
    fn diagonal_wheel_is_atomic_and_boundary_wheel_bubbles_over_tracks_too() {
        let frame = ScrollAreaFrame::new(Recti::new(0, 0, 100, 100), 0, 10, ScrollbarPresence { vertical: true, horizontal: true });
        let mut geometry = frame.commit(Dimensioni::new(200, 200), Vec2i::new(0, 110), 8);
        let body_pos = Vec2i::new(20, 20);
        assert!(geometry.wheel_route_rect(body_pos, Vec2i::new(15, 15)).is_some());
        geometry.apply_wheel(Vec2i::new(15, 15));
        assert_eq!(
            (geometry.offset.x, geometry.offset.y),
            (15, 110),
            "the consumed event applies both requested axes"
        );

        geometry.offset = geometry.max_offset;
        let vertical_track_pos = Vec2i::new(95, 20);
        assert!(geometry.wheel_route_rect(vertical_track_pos, Vec2i::new(0, 10)).is_none());
        assert!(geometry.wheel_route_rect(body_pos, Vec2i::new(10, 10)).is_none());
    }

    #[test]
    fn committed_geometry_clamps_offsets_after_content_shrinks() {
        let frame = ScrollAreaFrame::new(Recti::new(0, 0, 100, 100), 0, 10, ScrollbarPresence { vertical: true, horizontal: true });
        let geometry = frame.commit(Dimensioni::new(120, 130), Vec2i::new(500, 500), 8);
        assert_eq!((geometry.offset.x, geometry.offset.y), (30, 40));
    }

    #[test]
    fn framed_scroll_area_applies_content_origin_once_and_offset_does_not_relayout_children() {
        let child = Node::widget(Custom::create(CustomParameters::new("child"))).with_policy(Policy::fixed(160, 200));
        let child_id = child.id();
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, [child]));
        let mut runtime = UiRuntime::new();
        let style = Style {
            frame_border_width: 3,
            padding: 5,
            scrollbar_size: 10,
            ..Style::default()
        };
        let atlas = test_atlas();
        let outer = Recti::new(10, 20, 100, 80);

        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), outer, UNCLIPPED_RECT);
        let allocation_before = root.with_node(child_id, |node| node.state.layout.allocation).unwrap();
        let screen_before = runtime.debug_node_rect(std::slice::from_ref(&root), child_id).unwrap();
        assert_eq!((allocation_before.x, allocation_before.y), (0, 0));
        assert_eq!(
            (screen_before.x, screen_before.y),
            (
                outer.x + style.frame_border_width + style.padding,
                outer.y + style.frame_border_width + style.padding
            )
        );

        scroll.try_update(|state| state.set_offset(Vec2i::new(0, 12))).unwrap();
        runtime.layout_tree_root(&mut root, &style, atlas, outer, UNCLIPPED_RECT);
        let allocation_after = root.with_node(child_id, |node| node.state.layout.allocation).unwrap();
        let screen_after = runtime.debug_node_rect(std::slice::from_ref(&root), child_id).unwrap();

        assert_eq!(
            (allocation_after.x, allocation_after.y, allocation_after.width, allocation_after.height),
            (allocation_before.x, allocation_before.y, allocation_before.width, allocation_before.height),
            "offset-only changes must not rearrange child allocations"
        );
        assert_eq!((screen_after.x, screen_after.y), (screen_before.x, screen_before.y - 12));

        runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(10, 20, 240, 260), UNCLIPPED_RECT);
        assert_eq!(
            scroll.try_read(|state| (state.offset().x, state.offset().y)),
            Some((0, 0)),
            "resize must clamp the existing state"
        );
    }
}
