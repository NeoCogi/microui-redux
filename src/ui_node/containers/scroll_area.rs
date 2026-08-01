use std::{cell::RefCell, rc::Rc};

use bitflags::bitflags;

use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, scrollbar_viewport_body, ScrollAxis};
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

/// Application-facing state and direct child owner for a scroll area.
pub struct ScrollAreaState {
    children: Children,
    offset: Vec2i,
    scrolling_enabled: bool,
    drag_axis: Option<DragAxis>,
    rect: Recti,
    body: Recti,
    content_size: Dimensioni,
    max_offset: Vec2i,
    vertical_track: Option<Recti>,
    horizontal_track: Option<Recti>,
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
        self.offset
    }
    /// Requests a content offset; negative components clamp immediately.
    pub fn set_offset(&mut self, offset: Vec2i) {
        self.offset = if self.scrolling_enabled {
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
            self.offset = Vec2i::default();
            self.vertical_track = None;
            self.horizontal_track = None;
            self.max_offset = Vec2i::default();
        }
    }

    fn padded_content(&self, padding: i32) -> Dimensioni {
        super::super::add_padding(self.content_size, padding.max(0))
    }

    fn clamp_offset(&mut self) {
        if !self.scrolling_enabled {
            self.offset = Vec2i::default();
            return;
        }
        self.offset.x = self.offset.x.clamp(0, self.max_offset.x);
        self.offset.y = self.offset.y.clamp(0, self.max_offset.y);
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
        runtime_read_state(&self.state, "ScrollArea::measure", |state| measure_children(state, style, atlas, available))
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        runtime_update_state(&self.state, "ScrollArea::update", |state| {
            update_scroll_state(state, ctx.style(), input);
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        runtime_read_state(&self.state, "ScrollArea::paint", |state| paint_scroll_area(state, ctx));
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
        ctx.route_widget_in_rect(event, surface, self.effective_widget_opt(), FocusPolicy::DragCapture)
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
                offset: Vec2i::default(),
                scrolling_enabled,
                drag_axis: None,
                rect: Recti::default(),
                body: Recti::default(),
                content_size: Dimensioni::default(),
                max_offset: Vec2i::default(),
                vertical_track: None,
                horizontal_track: None,
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
    let hint = measure_children(state, ctx.style(), ctx.atlas(), Dimensioni::new(rect.width, rect.height));
    let mut body = if state.scrolling_enabled {
        scrollbar_viewport_body(rect, hint, padding, scrollbar_size)
    } else {
        rect
    };
    let mut content_size = layout_children(state, ctx, body, padding);
    let next_body = if state.scrolling_enabled {
        scrollbar_viewport_body(rect, content_size, padding, scrollbar_size)
    } else {
        rect
    };
    if (next_body.x, next_body.y, next_body.width, next_body.height) != (body.x, body.y, body.width, body.height) {
        body = next_body;
        content_size = layout_children(state, ctx, body, padding);
    }

    state.rect = rect;
    state.body = body;
    state.content_size = content_size;
    let content = state.padded_content(padding);
    state.max_offset = if state.scrolling_enabled {
        Vec2i::new(
            scrollbar_max_scroll(content.width, body.width),
            scrollbar_max_scroll(content.height, body.height),
        )
    } else {
        Vec2i::default()
    };
    state.clamp_offset();
    state.vertical_track =
        (state.scrolling_enabled && content.height > body.height && scrollbar_size > 0).then(|| scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size));
    state.horizontal_track =
        (state.scrolling_enabled && content.width > body.width && scrollbar_size > 0).then(|| scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size));

    ctx.set_children_viewport(body, Vec2i::new(body.x.saturating_sub(state.offset.x), body.y.saturating_sub(state.offset.y)));
    ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
    ctx.set_child_overflow_propagation(false);
}

fn layout_children(state: &mut ScrollAreaState, ctx: &mut ContainerLayoutCtx<'_>, body: Recti, padding: i32) -> Dimensioni {
    let child_width = body.width.saturating_sub(padding.saturating_mul(2)).max(0);
    let child_height = body.height.saturating_sub(padding.saturating_mul(2)).max(0);
    let mut y = body.y.saturating_add(padding);
    let mut width = 0;
    for index in 0..state.children.len() {
        let preferred = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(child_width, child_height))
            .unwrap_or_default();
        let offered_width = child_width.max(preferred.width);
        let size = ctx
            .layout_child(
                &mut state.children,
                index,
                Recti::new(body.x.saturating_add(padding), y, offered_width, preferred.height),
            )
            .unwrap_or_default();
        width = width.max(offered_width.max(size.width));
        y = y.saturating_add(preferred.height);
        if index + 1 < state.children.len() {
            y = y.saturating_add(ctx.style().spacing);
        }
    }
    Dimensioni::new(width.max(0), y.saturating_sub(body.y).saturating_sub(padding).max(0))
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

fn route_surface(state: &ScrollAreaState, event: &UiInputEvent, has_pointer_capture: bool) -> Option<Recti> {
    if !state.scrolling_enabled {
        return None;
    }
    if has_pointer_capture && matches!(event, UiInputEvent::MouseDrag { .. } | UiInputEvent::MouseUp { .. }) {
        return Some(state.rect);
    }
    let pos = event_position(event)?;
    if let Some(track) = state.vertical_track.filter(|track| track.contains(&pos)) {
        return Some(track);
    }
    if let Some(track) = state.horizontal_track.filter(|track| track.contains(&pos)) {
        return Some(track);
    }
    if let UiInputEvent::Scroll { delta, .. } = event
        && state.body.contains(&pos)
    {
        let next = Vec2i::new(
            state.offset.x.saturating_add(delta.x).clamp(0, state.max_offset.x),
            state.offset.y.saturating_add(delta.y).clamp(0, state.max_offset.y),
        );
        if (next.x, next.y) != (state.offset.x, state.offset.y) {
            return Some(state.body);
        }
    }
    None
}

fn update_scroll_state(state: &mut ScrollAreaState, style: &Style, event: Option<&UiInputEvent>) {
    if !state.scrolling_enabled {
        state.offset = Vec2i::default();
        state.drag_axis = None;
        return;
    }
    let padding = style.padding.max(0);
    let scrollbar_size = style.scrollbar_size.max(0);
    if let Some(event) = event {
        match *event {
            UiInputEvent::Scroll { delta, .. } => {
                state.offset.x = state.offset.x.saturating_add(delta.x);
                state.offset.y = state.offset.y.saturating_add(delta.y);
            }
            UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => {
                if let Some(track) = state.vertical_track.filter(|track| track.contains(&pos)) {
                    state.drag_axis = Some(DragAxis::Vertical);
                    center_track_click(state, ScrollAxis::Vertical, track, pos, scrollbar_size, padding);
                } else if let Some(track) = state.horizontal_track.filter(|track| track.contains(&pos)) {
                    state.drag_axis = Some(DragAxis::Horizontal);
                    center_track_click(state, ScrollAxis::Horizontal, track, pos, scrollbar_size, padding);
                }
            }
            UiInputEvent::MouseDrag { delta, buttons, .. } if buttons.intersects(MouseButton::LEFT) => {
                let content = state.padded_content(padding);
                match state.drag_axis {
                    Some(DragAxis::Vertical) => {
                        if let Some(track) = state.vertical_track {
                            state.offset.y = state
                                .offset
                                .y
                                .saturating_add(scrollbar_drag_delta(ScrollAxis::Vertical, delta, content.height, track));
                        }
                    }
                    Some(DragAxis::Horizontal) => {
                        if let Some(track) = state.horizontal_track {
                            state.offset.x = state
                                .offset
                                .x
                                .saturating_add(scrollbar_drag_delta(ScrollAxis::Horizontal, delta, content.width, track));
                        }
                    }
                    None => {}
                }
            }
            UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT) => state.drag_axis = None,
            _ => {}
        }
        state.clamp_offset();
    }
}

fn center_track_click(state: &mut ScrollAreaState, axis: ScrollAxis, track: Recti, pos: Vec2i, thumb_size: i32, padding: i32) {
    let content = state.padded_content(padding);
    let (view, content_len, scroll, pointer, origin, track_len) = match axis {
        ScrollAxis::Vertical => (state.body.height, content.height, state.offset.y, pos.y, track.y, track.height),
        ScrollAxis::Horizontal => (state.body.width, content.width, state.offset.x, pos.x, track.x, track.width),
    };
    let thumb = scrollbar_thumb(axis, track, view, content_len, scroll, thumb_size);
    if thumb.contains(&pos) {
        return;
    }
    let thumb_len = match axis {
        ScrollAxis::Vertical => thumb.height,
        ScrollAxis::Horizontal => thumb.width,
    };
    let travel = track_len.saturating_sub(thumb_len);
    let max_scroll = scrollbar_max_scroll(content_len, view);
    let centered = pointer.saturating_sub(origin).saturating_sub(thumb_len / 2).clamp(0, travel);
    let next = if travel > 0 { centered.saturating_mul(max_scroll) / travel } else { 0 };
    match axis {
        ScrollAxis::Vertical => state.offset.y = next,
        ScrollAxis::Horizontal => state.offset.x = next,
    }
}

fn paint_scroll_area(state: &ScrollAreaState, ctx: &mut WidgetPaintCtx<'_>) {
    ctx.draw_rect(ctx.local_rect(), ctx.style().colors[ControlColor::PanelBG as usize]);
    let padding = ctx.style().padding.max(0);
    let scrollbar_size = ctx.style().scrollbar_size.max(0);
    let content = state.padded_content(padding);
    if let Some(track) = state.vertical_track {
        ctx.draw_rect(track, ctx.style().colors[ControlColor::ScrollBase as usize]);
        let thumb = scrollbar_thumb(ScrollAxis::Vertical, track, state.body.height, content.height, state.offset.y, scrollbar_size);
        ctx.draw_rect(thumb, ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }
    if let Some(track) = state.horizontal_track {
        ctx.draw_rect(track, ctx.style().colors[ControlColor::ScrollBase as usize]);
        let thumb = scrollbar_thumb(ScrollAxis::Horizontal, track, state.body.width, content.width, state.offset.x, scrollbar_size);
        ctx.draw_rect(thumb, ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Custom, CustomParameters};

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
            state.offset = Vec2i::new(3, 7);
            state.rect = Recti::new(0, 0, 80, 60);
        }
        assert!(container.retains_pointer_capture());

        container.on_pointer_capture_lost();
        assert!(!container.retains_pointer_capture());
        assert_eq!((container.state.borrow().offset.x, container.state.borrow().offset.y), (3, 7));

        {
            let mut state = container.state.borrow_mut();
            state.drag_axis = Some(DragAxis::Horizontal);
            state.set_scrolling_enabled(false);
            state.set_scrolling_enabled(true);
        }
        assert!(!container.retains_pointer_capture(), "disable/re-enable must not resurrect the old drag");
    }

    #[test]
    fn captured_route_surface_does_not_wait_for_queued_pointer_down_update() {
        let container = ScrollAreaBuilder::create_container(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, []));
        let mut state = container.state.borrow_mut();
        state.rect = Recti::new(0, 0, 80, 60);
        let drag = UiInputEvent::MouseDrag {
            pos: Vec2i::new(200, 180),
            delta: Vec2i::new(1, 2),
            buttons: MouseButton::LEFT,
        };

        let captured = route_surface(&state, &drag, true).expect("captured drag must route before local update");
        assert_eq!((captured.x, captured.y, captured.width, captured.height), (0, 0, 80, 60));
        assert!(route_surface(&state, &drag, false).is_none());
    }
}
