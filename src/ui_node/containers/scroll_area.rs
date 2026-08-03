use std::{cell::RefCell, rc::Rc};

use bitflags::bitflags;

use crate::ui_node::scrollbar::{ScrollAxis, ScrollbarGeometry, scrollbar_base, scrollbar_max_scroll};
use crate::ui_node::{runtime_read_state, runtime_update_state};
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

/// Committed geometry used unchanged by routing, update, and paint.
#[derive(Copy, Clone, Debug, Default)]
struct ScrollAreaGeometry {
    surface: Recti,
    content_view: Recti,
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

    fn track_at(self, pos: Vec2i) -> Option<(DragAxis, ScrollbarGeometry)> {
        if let Some(vertical) = self.vertical.filter(|bar| bar.track().contains(&pos)) {
            return Some((DragAxis::Vertical, vertical));
        }
        self.horizontal.filter(|bar| bar.track().contains(&pos)).map(|bar| (DragAxis::Horizontal, bar))
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
            self.geometry.offset = Vec2i::default();
            self.geometry.max_offset = Vec2i::default();
            self.geometry.vertical = None;
            self.geometry.horizontal = None;
            self.geometry.corner = None;
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

/// Measures scroll content intrinsically while accounting for panel padding.
///
/// Height remains unbounded because scrolling exists specifically to contain vertical overflow;
/// a positive outer width still constrains wrapping inside the padded viewport.
fn measure_scroll_area(state: &ScrollAreaState, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    let padding = style.padding.max(0);
    // Preserve zero as the unbounded-width marker while removing both horizontal padding edges.
    let width = if available.width > 0 {
        available.width.saturating_sub(padding.saturating_mul(2)).max(1)
    } else {
        0
    };
    let content = super::column::measure_column(&state.children, style, atlas, Dimensioni::new(width, 0));
    let inset = padding.saturating_mul(2);
    Dimensioni::new(content.width.saturating_add(inset), content.height.saturating_add(inset))
}

/// Commits child, viewport, and scrollbar geometry for one ScrollArea allocation.
fn layout_scroll_area(state: &mut ScrollAreaState, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
    // Layout may shrink content or the viewport, so clamp the previously requested offset against
    // geometry derived from this exact allocation.
    let requested_offset = state.geometry.offset;
    state.geometry = resolve_scroll_area_geometry(state, ctx, rect, requested_offset);

    let view = state.geometry.content_view;
    let offset = state.geometry.offset;
    ctx.set_children_viewport(view, Vec2i::new(view.x.saturating_sub(offset.x), view.y.saturating_sub(offset.y)));
    ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
    ctx.set_child_overflow_propagation(false);
}

/// Lays out children and resolves the one geometry value used until the next layout.
fn resolve_scroll_area_geometry(state: &mut ScrollAreaState, ctx: &mut ContainerLayoutCtx<'_>, surface: Recti, requested_offset: Vec2i) -> ScrollAreaGeometry {
    let padding = ctx.style().padding.max(0);
    let scrollbar_size = ctx.style().scrollbar_size.max(0);
    let min_thumb_len = ctx.style().thumb_size.max(0);
    let surface = Recti::new(surface.x, surface.y, surface.width.max(0), surface.height.max(0));
    let bars_usable = state.scrolling_enabled && scrollbar_size > 0 && surface.width > 0 && surface.height > 0;
    let mut has_vertical = false;
    let mut has_horizontal = false;

    // There are only four possible presence states. Starting without bars and only adding a bar
    // once overflow requires it makes convergence monotonic and strictly bounded.
    for _ in 0..4 {
        let vertical_width = if has_vertical { scrollbar_size.min(surface.width) } else { 0 };
        let horizontal_height = if has_horizontal { scrollbar_size.min(surface.height) } else { 0 };
        let body = Recti::new(
            surface.x,
            surface.y,
            surface.width.saturating_sub(vertical_width).max(0),
            surface.height.saturating_sub(horizontal_height).max(0),
        );
        let content_view = inset_rect(body, padding);
        let child_extent = layout_children(state, ctx, Dimensioni::new(content_view.width, content_view.height));

        let next_vertical = has_vertical || (bars_usable && child_extent.height > content_view.height);
        let next_horizontal = has_horizontal || (bars_usable && child_extent.width > content_view.width);
        if next_vertical != has_vertical || next_horizontal != has_horizontal {
            has_vertical = next_vertical;
            has_horizontal = next_horizontal;
            continue;
        }

        let max_offset = if state.scrolling_enabled {
            Vec2i::new(
                scrollbar_max_scroll(child_extent.width, content_view.width),
                scrollbar_max_scroll(child_extent.height, content_view.height),
            )
        } else {
            Vec2i::default()
        };
        let offset = Vec2i::new(requested_offset.x.clamp(0, max_offset.x), requested_offset.y.clamp(0, max_offset.y));
        let vertical = (has_vertical && vertical_width > 0 && body.height > 0).then(|| {
            ScrollbarGeometry::new(
                ScrollAxis::Vertical,
                scrollbar_base(ScrollAxis::Vertical, body, vertical_width),
                content_view.height,
                child_extent.height,
                offset.y,
                min_thumb_len,
            )
        });
        let horizontal = (has_horizontal && horizontal_height > 0 && body.width > 0).then(|| {
            ScrollbarGeometry::new(
                ScrollAxis::Horizontal,
                scrollbar_base(ScrollAxis::Horizontal, body, horizontal_height),
                content_view.width,
                child_extent.width,
                offset.x,
                min_thumb_len,
            )
        });
        let corner = (has_vertical && has_horizontal && vertical_width > 0 && horizontal_height > 0).then(|| {
            Recti::new(
                body.x.saturating_add(body.width),
                body.y.saturating_add(body.height),
                vertical_width,
                horizontal_height,
            )
        });

        return ScrollAreaGeometry {
            surface,
            content_view,
            offset,
            max_offset,
            vertical,
            horizontal,
            corner,
        };
    }

    unreachable!("scroll-area scrollbar presence must converge within four states")
}

/// Lays out vertical scroll content and returns its complete overflow extent.
///
/// Each child is measured once for intrinsic width and again at its policy-adjusted offered width;
/// the second height is necessary for wrapped content. Cursor movement uses actual committed child
/// size, not the pre-layout preference, so fixed node policies cannot desynchronize later children.
fn layout_children(state: &mut ScrollAreaState, ctx: &mut ContainerLayoutCtx<'_>, view: Dimensioni) -> Dimensioni {
    let child_width = view.width.max(0);
    let mut y: i32 = 0;
    let mut width = 0;
    for index in 0..state.children.len() {
        // Allow content wider than the viewport so horizontal overflow remains observable.
        let preferred = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(child_width.max(1), 0))
            .unwrap_or_default();
        let offered_width = child_width.max(preferred.width);
        let measured_width = state
            .children
            .child_policy(index)
            .unwrap_or_else(crate::Policy::auto)
            .width
            .measurement_bound(offered_width);
        // Height must correspond to the width that generic node layout will actually apply.
        let preferred_height = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(measured_width, 0))
            .unwrap_or_default()
            .height;
        let size = ctx
            .layout_child(&mut state.children, index, Recti::new(0, y, offered_width, preferred_height))
            .unwrap_or_default();
        // Advance by committed geometry because the node policy may override the measured height.
        width = width.max(offered_width.max(size.width));
        y = y.saturating_add(size.height);
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
        UiInputEvent::Scroll { pos, delta } => {
            let hit_rect = if state.geometry.content_view.contains(&pos) {
                Some(state.geometry.content_view)
            } else if let Some(vertical) = state.geometry.vertical.filter(|bar| bar.track().contains(&pos)) {
                Some(vertical.track())
            } else {
                state.geometry.horizontal.filter(|bar| bar.track().contains(&pos)).map(ScrollbarGeometry::track)
            }?;
            let next = Vec2i::new(
                state.geometry.offset.x.saturating_add(delta.x).clamp(0, state.geometry.max_offset.x),
                state.geometry.offset.y.saturating_add(delta.y).clamp(0, state.geometry.max_offset.y),
            );
            ((next.x, next.y) != (state.geometry.offset.x, state.geometry.offset.y)).then_some(hit_rect)
        }
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
                state.geometry.offset.x = state.geometry.offset.x.saturating_add(delta.x);
                state.geometry.offset.y = state.geometry.offset.y.saturating_add(delta.y);
            }
            UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => {
                if let Some((axis, bar)) = state.geometry.track_at(pos) {
                    state.drag_axis = Some(axis);
                    if !bar.thumb().contains(&pos) {
                        match axis {
                            DragAxis::Vertical => state.geometry.offset.y = bar.centered_offset(pos),
                            DragAxis::Horizontal => state.geometry.offset.x = bar.centered_offset(pos),
                        }
                    }
                }
            }
            UiInputEvent::MouseDrag { delta, buttons, .. } if buttons.intersects(MouseButton::LEFT) => match state.drag_axis {
                Some(DragAxis::Vertical) => {
                    if let Some(bar) = state.geometry.vertical {
                        state.geometry.offset.y = state.geometry.offset.y.saturating_add(bar.drag_delta(delta));
                    }
                }
                Some(DragAxis::Horizontal) => {
                    if let Some(bar) = state.geometry.horizontal {
                        state.geometry.offset.x = state.geometry.offset.x.saturating_add(bar.drag_delta(delta));
                    }
                }
                None => {}
            },
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
    use crate::{
        Column, ColumnParameters, Custom, CustomParameters, Policy, Row, RowParameters, SizePolicy, Stack, StackDirection, StackParameters, TextBlock,
        TextBlockParameters, TextWrap, UNCLIPPED_RECT,
    };

    fn laid_out_geometry(child_size: Dimensioni, surface: Recti, style: Style, requested_offset: Vec2i) -> ScrollAreaGeometry {
        let child = Node::widget(Custom::create(CustomParameters::new("child"))).with_policy(Policy::fixed(child_size.width, child_size.height));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [child]));
        scroll.try_update(|state| state.set_offset(requested_offset)).unwrap();

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), surface, UNCLIPPED_RECT);
        scroll.try_read(|state| state.geometry).unwrap()
    }

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
        let track = Recti::new(70, 0, 10, 60);
        state.geometry = ScrollAreaGeometry {
            surface: Recti::new(0, 0, 80, 60),
            content_view: Recti::new(0, 0, 70, 60),
            max_offset: Vec2i::new(0, 60),
            vertical: Some(ScrollbarGeometry::new(ScrollAxis::Vertical, track, 60, 120, 0, 8)),
            ..ScrollAreaGeometry::default()
        };
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
        let surface = Recti::new(0, 0, 100, 100);
        let fits = laid_out_geometry(
            Dimensioni::new(80, 80),
            surface,
            Style {
                padding: 10,
                scrollbar_size: 10,
                ..Style::default()
            },
            Vec2i::default(),
        );
        assert_eq!((fits.content_view.width, fits.content_view.height), (80, 80));
        assert!(fits.vertical.is_none() && fits.horizontal.is_none());

        let induced = laid_out_geometry(
            Dimensioni::new(95, 101),
            surface,
            Style {
                padding: 0,
                scrollbar_size: 10,
                ..Style::default()
            },
            Vec2i::default(),
        );
        assert_eq!((induced.content_view.width, induced.content_view.height), (90, 90));
        assert!(induced.vertical.is_some() && induced.horizontal.is_some());
        assert!(induced.corner.is_some());
    }

    #[test]
    fn wrapped_remainder_column_does_not_create_horizontal_overflow() {
        let (_, label) = TextBlock::create(TextBlockParameters::new("label"));
        let (_, text) = TextBlock::create(TextBlockParameters::with_wrap(
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum.",
            TextWrap::Word,
        ));
        let text = Node::widget(text);
        let text_id = text.id();
        let (_, text_stack) = Stack::create(StackParameters::new(
            SizePolicy::Remainder(0),
            SizePolicy::Auto,
            StackDirection::TopToBottom,
            [text],
        ));
        let (_, text_column) = Column::create(ColumnParameters::new([text_stack]));
        let (_, row) = Row::create(RowParameters::new(
            [SizePolicy::Fixed(40), SizePolicy::Remainder(0)],
            SizePolicy::Auto,
            [Node::widget(label), text_column],
        ));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [row]));
        let style = Style {
            padding: 0,
            spacing: 4,
            scrollbar_size: 10,
            ..Style::default()
        };

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(0, 0, 100, 300), UNCLIPPED_RECT);

        assert_eq!(scroll.try_read(|state| state.geometry.horizontal.is_none()), Some(true));
        let text_rect = runtime.debug_node_rect(std::slice::from_ref(&root), text_id).unwrap();
        assert_eq!(
            text_rect.width, 56,
            "the remainder track receives only the width left by the fixed track and spacing"
        );
        assert!(
            text_rect.height > test_atlas().get_font_height(style.font) as i32,
            "height must reflect wrapping at the remainder width"
        );
    }

    #[test]
    fn diagonal_wheel_is_atomic_and_boundary_wheel_bubbles_over_tracks_too() {
        let geometry = laid_out_geometry(
            Dimensioni::new(200, 200),
            Recti::new(0, 0, 100, 100),
            Style {
                padding: 0,
                scrollbar_size: 10,
                thumb_size: 8,
                ..Style::default()
            },
            Vec2i::new(0, 110),
        );
        let container = ScrollAreaBuilder::create_container(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, []));
        let mut state = container.state.borrow_mut();
        state.geometry = geometry;
        let body_pos = Vec2i::new(20, 20);
        let diagonal = UiInputEvent::Scroll { pos: body_pos, delta: Vec2i::new(15, 15) };
        assert!(route_surface(&state, &diagonal, false).is_some());
        update_scroll_state(&mut state, Some(&diagonal));
        assert_eq!(
            (state.geometry.offset.x, state.geometry.offset.y),
            (15, 110),
            "the consumed event applies both requested axes"
        );

        state.geometry.offset = state.geometry.max_offset;
        let track_boundary = UiInputEvent::Scroll {
            pos: Vec2i::new(95, 20),
            delta: Vec2i::new(0, 10),
        };
        let body_boundary = UiInputEvent::Scroll { pos: body_pos, delta: Vec2i::new(10, 10) };
        assert!(route_surface(&state, &track_boundary, false).is_none());
        assert!(route_surface(&state, &body_boundary, false).is_none());
    }

    #[test]
    fn committed_geometry_clamps_offsets_after_content_shrinks() {
        let geometry = laid_out_geometry(
            Dimensioni::new(120, 130),
            Recti::new(0, 0, 100, 100),
            Style {
                padding: 0,
                scrollbar_size: 10,
                ..Style::default()
            },
            Vec2i::new(500, 500),
        );
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
