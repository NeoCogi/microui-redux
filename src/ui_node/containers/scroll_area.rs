use std::{cell::RefCell, rc::Rc};

use crate::input::{ContainerOption, ScrollBehavior};
use crate::id::IdNamespace;
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, scrollbar_viewport_body, ScrollAxis};
use crate::{ControlColor, Dimensioni, Policy, Recti, Vec2i};

use super::{Column, Container, InputCtx, InputResult, LayoutCtx, MeasureCtx, PaintCtx, UiInputEvent, Widget};
use crate::ui_node::{UiNode, UiNodeData, UiNodeId, UiNodeState};

/// Shared state owned by a scroll-area composition.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct ScrollAreaState {
    /// Scroll-area allocation/control rect.
    pub(crate) rect: Recti,
    /// Viewport body visible to child content.
    pub(crate) body: Recti,
    /// Preferred child content size before viewport padding.
    pub(crate) content_size: Dimensioni,
    /// Current scroll offset in padded viewport content coordinates.
    pub(crate) scroll: Vec2i,
}

pub(crate) type SharedScrollAreaState = Rc<RefCell<ScrollAreaState>>;

pub(crate) fn shared_scroll_area_state() -> SharedScrollAreaState {
    Rc::new(RefCell::new(ScrollAreaState::default()))
}

#[cfg(test)]
pub(crate) fn scroll_area_state(roots: &[UiNode], scroll_area: UiNodeId) -> Option<ScrollAreaState> {
    roots.iter().find_map(|root| root.find(scroll_area)).and_then(|node| match &node.data {
        UiNodeData::Widget(widget) => widget.debug_scroll_area_state(),
        UiNodeData::Container(container) => container.debug_scroll_area_state(),
    })
}

#[cfg(test)]
pub(crate) fn set_scroll_area_scroll(roots: &mut [UiNode], scroll_area: UiNodeId, scroll: Vec2i) -> bool {
    roots
        .iter_mut()
        .find_map(|root| root.find_mut(scroll_area))
        .is_some_and(|node| match &mut node.data {
            UiNodeData::Widget(widget) => widget.debug_set_scroll_area_scroll(scroll),
            UiNodeData::Container(container) => container.debug_set_scroll_area_scroll(scroll),
        })
}

/// Scroll-area container.
pub(crate) struct ScrollArea {
    /// Shared state used by the scroll-area container, viewport, and scrollbar parts.
    state: SharedScrollAreaState,
    /// Scroll behavior shared by internal viewport and scrollbar nodes.
    scroll_behavior: ScrollBehavior,
    /// Scrollbar interaction/paint component.
    scrollbars: Scrollbars,
    /// Rendering options applied to the scroll-area panel.
    pub(crate) opt: ContainerOption,
    /// Internal viewport and scrollbar child nodes.
    pub(crate) children: Vec<UiNode>,
}

impl ScrollArea {
    /// Creates a composed scroll area from viewport, scrollbar, and content components.
    pub(crate) fn new(state: SharedScrollAreaState, scroll_behavior: ScrollBehavior, opt: ContainerOption, children: Vec<UiNode>) -> Self {
        Self {
            state: state.clone(),
            scroll_behavior,
            scrollbars: Scrollbars { state, scroll_behavior },
            opt,
            children,
        }
    }
}

/// Viewport component that owns content-space layout and scroll translation.
pub(crate) struct ScrollViewport {
    pub(crate) state: SharedScrollAreaState,
    pub(crate) content: Column,
    pub(crate) scroll_behavior: ScrollBehavior,
}

impl ScrollViewport {
    pub(crate) fn new(state: SharedScrollAreaState, scroll_behavior: ScrollBehavior, children: Vec<UiNode>) -> Self {
        Self {
            state,
            content: Column { children },
            scroll_behavior,
        }
    }

    fn layout_content(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, viewport: Recti, outer: Recti) -> Dimensioni {
        let content_size = layout_viewport_content(ctx, &self.state, &mut self.content, state, viewport);
        set_viewport_layout_state(&self.state, outer, viewport, content_size);
        content_size
    }
}

/// Scrollbar component that owns internal scrollbar node layout.
#[derive(Clone)]
struct Scrollbars {
    state: SharedScrollAreaState,
    scroll_behavior: ScrollBehavior,
}

impl Scrollbars {
    fn layout_nodes(&self, ctx: &mut LayoutCtx<'_>, children: &mut [UiNode], id: UiNodeId, body: Recti) {
        let scrollbar_size = ctx.style.scrollbar_size.max(0);
        let vertical = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
        let horizontal = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
        let corner = Recti::new(vertical.x, horizontal.y, vertical.width, horizontal.height);
        if let Some(child) = child_mut(children, scrollbar_part_id(id, ScrollbarPart::Track(ScrollAxis::Vertical))) {
            ctx.layout_node_ref(child, vertical);
        }
        if let Some(child) = child_mut(children, scrollbar_part_id(id, ScrollbarPart::Track(ScrollAxis::Horizontal))) {
            ctx.layout_node_ref(child, horizontal);
        }
        if let Some(child) = child_mut(children, scrollbar_part_id(id, ScrollbarPart::Corner)) {
            ctx.layout_node_ref(child, corner);
        }
    }
}

impl Widget for ScrollArea {
    fn measure(&self, ctx: &MeasureCtx<'_>, state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        let id = state.id();
        child(&self.children, scroll_viewport_id(id))
            .map(|viewport| ctx.measure_node_ref(viewport, available))
            .unwrap_or_default()
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        let id = state.id();
        let viewport_id = scroll_viewport_id(id);
        let content_hint = self.state.borrow().content_size;
        let mut body = viewport_body_for_scroll_behavior(rect, content_hint, ctx.style.padding, ctx.style.scrollbar_size, self.scroll_behavior);
        ctx.set_content_space_geometry(state, rect, rect, Dimensioni::new(rect.width.max(0), rect.height.max(0)), Vec2i::default());
        ctx.set_content_size(state, Dimensioni::new(rect.width.max(0), rect.height.max(0)));
        ctx.set_child_overflow_propagation(state, false);
        if let Some(viewport) = child_mut(&mut self.children, viewport_id) {
            ctx.layout_node_ref(viewport, body);
        }
        let content_size = self.state.borrow().content_size;
        let next_body = viewport_body_for_scroll_behavior(rect, content_size, ctx.style.padding, ctx.style.scrollbar_size, self.scroll_behavior);
        if !super::super::same_rect(next_body, body) {
            body = next_body;
            ctx.set_content_space_geometry(state, rect, rect, Dimensioni::new(rect.width.max(0), rect.height.max(0)), Vec2i::default());
            if let Some(viewport) = child_mut(&mut self.children, viewport_id) {
                ctx.layout_node_ref(viewport, body);
            }
        }
        let content_size = self.state.borrow().content_size;
        set_viewport_layout_state(&self.state, rect, body, content_size);
        self.scrollbars.layout_nodes(ctx, &mut self.children, id, body);
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        update_scroll_area_viewport_on_input(ctx, state, &self.state, self.scroll_behavior, event)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, state: &mut UiNodeState) -> bool {
        paint_scroll_area_panel(ctx, state, self.opt);
        true
    }

    #[cfg(test)]
    fn debug_scroll_area_state(&self) -> Option<ScrollAreaState> {
        Some(*self.state.borrow())
    }

    #[cfg(test)]
    fn debug_set_scroll_area_scroll(&mut self, scroll: Vec2i) -> bool {
        self.state.borrow_mut().scroll = scroll;
        true
    }
}

impl Container for ScrollArea {
    fn children(&self) -> &[UiNode] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<UiNode> {
        &mut self.children
    }
}

impl Widget for ScrollViewport {
    fn measure(&self, ctx: &MeasureCtx<'_>, state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        self.content.measure(ctx, state, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        self.layout_content(ctx, state, rect, rect);
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        update_scroll_area_viewport_on_input(ctx, state, &self.state, self.scroll_behavior, event)
    }
}

impl Container for ScrollViewport {
    fn children(&self) -> &[UiNode] {
        &self.content.children
    }

    fn children_mut(&mut self) -> &mut Vec<UiNode> {
        &mut self.content.children
    }
}

#[derive(Copy, Clone, Debug)]
enum ScrollbarPart {
    Track(ScrollAxis),
    Corner,
}

#[derive(Clone)]
struct ScrollbarNode {
    state: SharedScrollAreaState,
    scroll_behavior: ScrollBehavior,
    part: ScrollbarNodePart,
}

#[derive(Clone)]
enum ScrollbarNodePart {
    Track(ScrollAxisState),
    Corner,
}

#[derive(Clone)]
struct ScrollAxisState {
    axis: ScrollAxis,
}

impl Widget for ScrollbarNode {
    fn measure(&self, _ctx: &MeasureCtx<'_>, _state: &UiNodeState, _available: Dimensioni) -> Dimensioni {
        Dimensioni::default()
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        let _ = ctx;
        state.set_layout_from_rect(rect, Dimensioni::new(rect.width.max(0), rect.height.max(0)));
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        let ScrollbarNodePart::Track(axis_state) = &self.part else {
            return InputResult::Ignored;
        };
        update_scrollbar_on_input(ctx, state, &self.state, axis_state, self.scroll_behavior, event)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, state: &mut UiNodeState) -> bool {
        paint_scrollbar_node(ctx, state, &self.state, self.scroll_behavior, &self.part);
        false
    }
}

fn scrollbar_node(parent: UiNodeId, state: SharedScrollAreaState, scroll_behavior: ScrollBehavior, part: ScrollbarPart) -> UiNode {
    let node_part = match part {
        ScrollbarPart::Track(axis) => ScrollbarNodePart::Track(ScrollAxisState { axis }),
        ScrollbarPart::Corner => ScrollbarNodePart::Corner,
    };
    UiNode::new(
        scrollbar_part_id(parent, part),
        Policy::auto(),
        UiNodeData::Widget(Box::new(ScrollbarNode { state, scroll_behavior, part: node_part })),
    )
}

pub(crate) fn scroll_viewport_node(parent: UiNodeId, state: SharedScrollAreaState, scroll_behavior: ScrollBehavior, children: Vec<UiNode>) -> UiNode {
    UiNode::new(
        scroll_viewport_id(parent),
        Policy::auto(),
        UiNodeData::Container(Box::new(ScrollViewport::new(state, scroll_behavior, children))),
    )
}

pub(crate) fn scrollbar_nodes(parent: UiNodeId, state: SharedScrollAreaState, scroll_behavior: ScrollBehavior) -> Vec<UiNode> {
    vec![
        scrollbar_node(parent, state.clone(), scroll_behavior, ScrollbarPart::Track(ScrollAxis::Vertical)),
        scrollbar_node(parent, state.clone(), scroll_behavior, ScrollbarPart::Track(ScrollAxis::Horizontal)),
        scrollbar_node(parent, state, scroll_behavior, ScrollbarPart::Corner),
    ]
}

pub(crate) fn scroll_viewport_id(parent: UiNodeId) -> UiNodeId {
    IdNamespace::UINODE_INTERNAL.id([parent.raw() as u64, 0])
}

fn scrollbar_part_id(parent: UiNodeId, part: ScrollbarPart) -> UiNodeId {
    let part = match part {
        ScrollbarPart::Track(ScrollAxis::Vertical) => 1,
        ScrollbarPart::Track(ScrollAxis::Horizontal) => 2,
        ScrollbarPart::Corner => 3,
    };
    IdNamespace::UINODE_INTERNAL.id([parent.raw() as u64, part])
}

fn child(children: &[UiNode], id: UiNodeId) -> Option<&UiNode> {
    children.iter().find(|child| child.id() == id)
}

fn child_mut(children: &mut [UiNode], id: UiNodeId) -> Option<&mut UiNode> {
    children.iter_mut().find(|child| child.id() == id)
}

fn layout_viewport_content(
    ctx: &mut LayoutCtx<'_>,
    shared_state: &SharedScrollAreaState,
    content: &mut Column,
    state: &mut UiNodeState,
    viewport: Recti,
) -> Dimensioni {
    let state_snapshot = *shared_state.borrow();
    let content_hint = state_snapshot.content_size;
    let requested_scroll = state_snapshot.scroll;
    let (mut content_size, mut scroll) = layout_viewport_content_once(ctx, content, state, viewport, content_hint, requested_scroll);
    let next_scroll = clamp_scroll_for_viewport(requested_scroll, content_size, viewport, ctx.style.padding.max(0));
    if next_scroll.x != scroll.x || next_scroll.y != scroll.y {
        (content_size, scroll) = layout_viewport_content_once(ctx, content, state, viewport, content_size, next_scroll);
    }
    set_viewport_scroll(shared_state, scroll);
    content_size
}

fn layout_viewport_content_once(
    ctx: &mut LayoutCtx<'_>,
    content: &mut Column,
    state: &mut UiNodeState,
    viewport: Recti,
    content_hint: Dimensioni,
    scroll: Vec2i,
) -> (Dimensioni, Vec2i) {
    let padded_hint = super::super::add_padding(content_hint, ctx.style.padding.max(0));
    let scroll = clamp_scroll_for_padded_content(scroll, padded_hint, viewport);
    let padding = ctx.style.padding.max(0);
    let child_rect = crate::expand_rect(Recti::new(0, 0, viewport.width, viewport.height), -padding);
    let content_to_parent_translation = Vec2i::new(viewport.x.saturating_sub(scroll.x), viewport.y.saturating_sub(scroll.y));
    let padded_virtual_size = super::super::add_padding(content_hint, ctx.style.padding.max(0));
    ctx.set_content_space_geometry(state, viewport, viewport, padded_virtual_size, content_to_parent_translation);

    content.layout(ctx, state, child_rect);
    let content_size = child_content_bounds(&content.children)
        .map(|bounds| {
            Dimensioni::new(
                (bounds.x + bounds.width - child_rect.x).max(0),
                (bounds.y + bounds.height - child_rect.y).max(0),
            )
        })
        .unwrap_or_default();

    ctx.set_content_size(state, Dimensioni::new(viewport.width.max(0), viewport.height.max(0)));
    ctx.set_child_overflow_propagation(state, false);

    (content_size, scroll)
}

fn clamp_scroll_for_viewport(scroll: Vec2i, content_size: Dimensioni, viewport: Recti, padding: i32) -> Vec2i {
    clamp_scroll_for_padded_content(scroll, super::super::add_padding(content_size, padding), viewport)
}

fn child_content_bounds(children: &[UiNode]) -> Option<Recti> {
    let mut bounds = None;
    for child in children {
        let child_rect = super::super::child_content_rect(child);
        bounds = Some(match bounds {
            Some(rect) => super::super::union_rect(rect, child_rect),
            None => child_rect,
        });
    }
    bounds
}

fn clamp_scroll_for_padded_content(scroll: Vec2i, padded_content: Dimensioni, viewport: Recti) -> Vec2i {
    Vec2i::new(
        scroll.x.clamp(0, scrollbar_max_scroll(padded_content.width, viewport.width)),
        scroll.y.clamp(0, scrollbar_max_scroll(padded_content.height, viewport.height)),
    )
}

fn viewport_body_for_scroll_behavior(rect: Recti, content_size: Dimensioni, padding: i32, scrollbar_size: i32, scroll_behavior: ScrollBehavior) -> Recti {
    if scroll_behavior.is_no_scroll() {
        return rect;
    }
    scrollbar_viewport_body(rect, content_size, padding, scrollbar_size)
}

fn set_viewport_scroll(state: &SharedScrollAreaState, scroll: Vec2i) {
    state.borrow_mut().scroll = scroll;
}

fn set_viewport_layout_state(state: &SharedScrollAreaState, rect: Recti, body: Recti, content_size: Dimensioni) {
    let mut state = state.borrow_mut();
    state.rect = rect;
    state.body = body;
    state.content_size = content_size;
}

fn update_scroll_area_viewport_on_input(
    ctx: &mut InputCtx<'_>,
    _state: &UiNodeState,
    state: &SharedScrollAreaState,
    scroll_behavior: ScrollBehavior,
    event: &UiInputEvent,
) -> InputResult {
    let state_snapshot = *state.borrow();
    let (clip, body) = ctx.node_clip_and_rect(state_snapshot.body);
    if scroll_behavior.is_no_scroll() {
        return InputResult::Ignored;
    }

    let content_size = state_snapshot.content_size;
    let content = super::super::add_padding(content_size, ctx.style.padding.max(0));
    let max_x = scrollbar_max_scroll(content.width, body.width);
    let max_y = scrollbar_max_scroll(content.height, body.height);
    let mut scroll = state_snapshot.scroll;

    let result = match event {
        UiInputEvent::Scroll { pos, delta } => {
            let hovered_body = body.contains(&pos) && clip.contains(&pos);
            if hovered_body && (delta.x != 0 || delta.y != 0) {
                scroll.x = scroll.x.saturating_sub(delta.x);
                scroll.y = scroll.y.saturating_sub(delta.y);
                InputResult::Consumed
            } else {
                InputResult::Ignored
            }
        }
        _ => InputResult::Ignored,
    };

    scroll.x = scroll.x.clamp(0, max_x);
    scroll.y = scroll.y.clamp(0, max_y);

    state.borrow_mut().scroll = scroll;
    result
}

fn update_scrollbar_on_input(
    ctx: &mut InputCtx<'_>,
    node_state: &UiNodeState,
    shared_state: &SharedScrollAreaState,
    axis_state: &ScrollAxisState,
    scroll_behavior: ScrollBehavior,
    event: &UiInputEvent,
) -> InputResult {
    let track = ctx.node_rect(node_state);
    if scroll_behavior.is_no_scroll() || ctx.style.scrollbar_size.max(0) <= 0 {
        return InputResult::Ignored;
    }
    let viewport_state = *shared_state.borrow();
    let body = viewport_state.body;
    let content_size = viewport_state.content_size;
    let content = super::super::add_padding(content_size, ctx.style.padding.max(0));
    let max_x = scrollbar_max_scroll(content.width, body.width);
    let max_y = scrollbar_max_scroll(content.height, body.height);
    let mut scroll = viewport_state.scroll;

    let result = match event {
        UiInputEvent::MouseDown { pos, button } if button.intersects(crate::MouseButton::LEFT) && track.contains(&pos) && ctx.node_clip().contains(&pos) => {
            InputResult::Captured
        }
        UiInputEvent::MouseDrag { delta, buttons, .. } if buttons.intersects(crate::MouseButton::LEFT) => {
            match axis_state.axis {
                ScrollAxis::Vertical if max_y > 0 => {
                    scroll.y = scroll
                        .y
                        .saturating_add(scrollbar_drag_delta(ScrollAxis::Vertical, *delta, content.height, track));
                }
                ScrollAxis::Horizontal if max_x > 0 => {
                    scroll.x = scroll
                        .x
                        .saturating_add(scrollbar_drag_delta(ScrollAxis::Horizontal, *delta, content.width, track));
                }
                _ => {}
            }
            InputResult::Captured
        }
        UiInputEvent::MouseUp { button, .. } if button.intersects(crate::MouseButton::LEFT) => InputResult::Captured,
        UiInputEvent::Scroll { pos, delta } if track.contains(&pos) && ctx.node_clip().contains(&pos) => {
            match axis_state.axis {
                ScrollAxis::Vertical if delta.y != 0 => scroll.y = scroll.y.saturating_sub(delta.y),
                ScrollAxis::Horizontal if delta.x != 0 => scroll.x = scroll.x.saturating_sub(delta.x),
                _ => return InputResult::Ignored,
            }
            InputResult::Consumed
        }
        _ => InputResult::Ignored,
    };

    scroll.x = scroll.x.clamp(0, max_x);
    scroll.y = scroll.y.clamp(0, max_y);
    shared_state.borrow_mut().scroll = scroll;
    result
}

fn paint_scroll_area_panel(ctx: &mut PaintCtx<'_>, state: &UiNodeState, opt: ContainerOption) {
    let rect = ctx.node_rect(state);
    if !opt.intersects(ContainerOption::NO_FRAME) {
        ctx.draw_frame(rect, ControlColor::PanelBG);
    }
}

fn paint_scrollbar_node(
    ctx: &mut PaintCtx<'_>,
    state: &UiNodeState,
    shared_state: &SharedScrollAreaState,
    scroll_behavior: ScrollBehavior,
    part: &ScrollbarNodePart,
) {
    if scroll_behavior.is_no_scroll() {
        return;
    }
    let scrollbar_size = ctx.style.scrollbar_size.max(0);
    if scrollbar_size <= 0 {
        return;
    }
    let track = ctx.node_rect(state);
    if track.width <= 0 || track.height <= 0 {
        return;
    }
    let ScrollbarNodePart::Track(state) = part else {
        return;
    };
    let viewport_state = *shared_state.borrow();
    let content_size = viewport_state.content_size;
    let scroll_offset = viewport_state.scroll;
    let body = viewport_state.body;
    let content = super::super::add_padding(content_size, ctx.style.padding.max(0));
    match state.axis {
        ScrollAxis::Vertical if content.height > body.height => {
            let thumb = scrollbar_thumb(ScrollAxis::Vertical, track, body.height, content.height, scroll_offset.y, scrollbar_size);
            ctx.draw_frame(track, ControlColor::Base);
            ctx.draw_frame(thumb, ControlColor::Button);
        }
        ScrollAxis::Horizontal if content.width > body.width => {
            let thumb = scrollbar_thumb(ScrollAxis::Horizontal, track, body.width, content.width, scroll_offset.x, scrollbar_size);
            ctx.draw_frame(track, ControlColor::Base);
            ctx.draw_frame(thumb, ControlColor::Button);
        }
        _ => {}
    }
}
