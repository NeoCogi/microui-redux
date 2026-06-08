use std::{cell::RefCell, rc::Rc};

use crate::input::{ContainerOption, ScrollBehavior};
use crate::id::IdNamespace;
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, scrollbar_viewport_body, ScrollAxis};
use crate::{ControlColor, Dimensioni, GridSpan, Policy, Recti, Vec2i};

use super::{Column, InputCtx, InputResult, LayoutCtx, MeasureCtx, NodeBehavior, PaintCtx, UiInputEvent};
use crate::ui_node::{UiNode, UiNodeData, UiNodeId};

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

/// Scroll-area container.
#[derive(Clone)]
pub(crate) struct ScrollArea {
    /// Shared state used by the scroll-area container, viewport, and scrollbar parts.
    state: SharedScrollAreaState,
    /// Scroll behavior shared by internal viewport and scrollbar nodes.
    scroll_behavior: ScrollBehavior,
    /// Scrollbar interaction/paint component.
    scrollbars: Scrollbars,
    /// Rendering options applied to the scroll-area panel.
    pub(crate) opt: ContainerOption,
}

impl ScrollArea {
    /// Creates a composed scroll area from viewport, scrollbar, and content components.
    pub(crate) fn new(state: SharedScrollAreaState, scroll_behavior: ScrollBehavior, opt: ContainerOption) -> Self {
        Self {
            state: state.clone(),
            scroll_behavior,
            scrollbars: Scrollbars { state, scroll_behavior },
            opt,
        }
    }
}

/// Viewport component that owns content-space layout and scroll translation.
#[derive(Clone)]
pub(crate) struct ScrollViewport {
    pub(crate) state: SharedScrollAreaState,
    pub(crate) content: Column,
    pub(crate) scroll_behavior: ScrollBehavior,
}

impl ScrollViewport {
    pub(crate) fn new(state: SharedScrollAreaState, scroll_behavior: ScrollBehavior) -> Self {
        Self { state, content: Column, scroll_behavior }
    }

    fn layout_content(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, viewport: Recti, outer: Recti, clip: Recti) -> Dimensioni {
        let content_size = layout_viewport_content(ctx, &self.state, &mut self.content, id, viewport, clip);
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
    fn layout_nodes(&self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, body: Recti, outer_clip: Recti) {
        let clip = outer_clip;
        let scrollbar_size = ctx.style.scrollbar_size.max(0);
        let vertical = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
        let horizontal = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
        let corner = Recti::new(vertical.x, horizontal.y, vertical.width, horizontal.height);
        ctx.layout_node(scrollbar_part_id(id, ScrollbarPart::Track(ScrollAxis::Vertical)), vertical, clip);
        ctx.layout_node(scrollbar_part_id(id, ScrollbarPart::Track(ScrollAxis::Horizontal)), horizontal, clip);
        ctx.layout_node(scrollbar_part_id(id, ScrollbarPart::Corner), corner, clip);
    }
}

impl NodeBehavior for ScrollArea {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        Some(scroll_viewport_id(id))
            .map(|viewport| ctx.measure_node(viewport, available))
            .unwrap_or_default()
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let viewport_id = scroll_viewport_id(id);
        let content_hint = self.state.borrow().content_size;
        let mut body = viewport_body_for_scroll_behavior(rect, content_hint, ctx.style.padding, ctx.style.scrollbar_size, self.scroll_behavior);
        ctx.set_content_space_geometry(
            id,
            rect,
            body,
            rect,
            clip,
            Dimensioni::new(rect.width.max(0), rect.height.max(0)),
            Vec2i::default(),
        );
        ctx.set_content_size(id, Dimensioni::new(rect.width.max(0), rect.height.max(0)));
        ctx.set_child_overflow_propagation(id, false);
        ctx.layout_node(viewport_id, body, clip);
        let content_size = self.state.borrow().content_size;
        let next_body = viewport_body_for_scroll_behavior(rect, content_size, ctx.style.padding, ctx.style.scrollbar_size, self.scroll_behavior);
        if !super::super::same_rect(next_body, body) {
            body = next_body;
            ctx.set_content_space_geometry(
                id,
                rect,
                body,
                rect,
                clip,
                Dimensioni::new(rect.width.max(0), rect.height.max(0)),
                Vec2i::default(),
            );
            ctx.layout_node(viewport_id, body, clip);
        }
        let content_size = self.state.borrow().content_size;
        set_viewport_layout_state(&self.state, rect, body, content_size);
        self.scrollbars.layout_nodes(ctx, id, body, rect);
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, id: UiNodeId, event: &UiInputEvent) -> InputResult {
        update_scroll_area_viewport_on_input(ctx, id, &self.state, self.scroll_behavior, event)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, id: UiNodeId) -> bool {
        paint_scroll_area_panel(ctx, id, self.opt);
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

impl NodeBehavior for ScrollViewport {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        self.content.measure(ctx, id, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        self.layout_content(ctx, id, rect, rect, clip);
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, id: UiNodeId, event: &UiInputEvent) -> InputResult {
        update_scroll_area_viewport_on_input(ctx, id, &self.state, self.scroll_behavior, event)
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

impl NodeBehavior for ScrollbarNode {
    fn measure(&self, _ctx: &MeasureCtx<'_>, _id: UiNodeId, _available: Dimensioni) -> Dimensioni {
        Dimensioni::default()
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        if let Some(node) = ctx.runtime.nodes.get_mut(&id) {
            node.set_layout_from_rect(rect, clip, Dimensioni::new(rect.width.max(0), rect.height.max(0)));
        }
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, id: UiNodeId, event: &UiInputEvent) -> InputResult {
        let ScrollbarNodePart::Track(state) = &self.part else {
            return InputResult::Ignored;
        };
        update_scrollbar_on_input(ctx, id, &self.state, state, self.scroll_behavior, event)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, id: UiNodeId) -> bool {
        paint_scrollbar_node(ctx, id, &self.state, self.scroll_behavior, &self.part);
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
        Some(parent),
        Policy::auto(),
        GridSpan::ONE,
        UiNodeData::Leaf {
            behavior: Box::new(ScrollbarNode { state, scroll_behavior, part: node_part }),
        },
    )
}

pub(crate) fn scroll_viewport_node(parent: UiNodeId, state: SharedScrollAreaState, scroll_behavior: ScrollBehavior, children: Vec<UiNodeId>) -> UiNode {
    UiNode::new(
        scroll_viewport_id(parent),
        Some(parent),
        Policy::auto(),
        GridSpan::ONE,
        UiNodeData::Branch {
            behavior: Box::new(ScrollViewport::new(state, scroll_behavior)),
            children,
        },
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

fn layout_viewport_content(ctx: &mut LayoutCtx<'_>, state: &SharedScrollAreaState, content: &mut Column, id: UiNodeId, viewport: Recti, clip: Recti) -> Dimensioni {
    let state_snapshot = *state.borrow();
    let content_hint = state_snapshot.content_size;
    let requested_scroll = state_snapshot.scroll;
    let (mut content_size, mut scroll) = layout_viewport_content_once(ctx, content, id, viewport, clip, content_hint, requested_scroll);
    let next_scroll = clamp_scroll_for_viewport(requested_scroll, content_size, viewport, ctx.style.padding.max(0));
    if next_scroll.x != scroll.x || next_scroll.y != scroll.y {
        (content_size, scroll) = layout_viewport_content_once(ctx, content, id, viewport, clip, content_size, next_scroll);
    }
    set_viewport_scroll(state, scroll);
    content_size
}

fn layout_viewport_content_once(
    ctx: &mut LayoutCtx<'_>,
    content: &mut Column,
    id: UiNodeId,
    viewport: Recti,
    clip: Recti,
    content_hint: Dimensioni,
    scroll: Vec2i,
) -> (Dimensioni, Vec2i) {
    let padded_hint = super::super::add_padding(content_hint, ctx.style.padding.max(0));
    let scroll = clamp_scroll_for_padded_content(scroll, padded_hint, viewport);
    let padding = ctx.style.padding.max(0);
    let child_rect = crate::expand_rect(Recti::new(0, 0, viewport.width, viewport.height), -padding);
    let child_clip = Recti::new(scroll.x, scroll.y, viewport.width, viewport.height);
    let content_to_parent_translation = Vec2i::new(viewport.x.saturating_sub(scroll.x), viewport.y.saturating_sub(scroll.y));
    let padded_virtual_size = super::super::add_padding(content_hint, ctx.style.padding.max(0));
    ctx.set_content_space_geometry(id, viewport, viewport, viewport, clip, padded_virtual_size, content_to_parent_translation);

    content.layout(ctx, id, child_rect, child_clip);
    let content_size = ctx
        .child_content_bounds(id)
        .map(|bounds| {
            Dimensioni::new(
                (bounds.x + bounds.width - child_rect.x).max(0),
                (bounds.y + bounds.height - child_rect.y).max(0),
            )
        })
        .unwrap_or_default();

    ctx.set_content_size(id, Dimensioni::new(viewport.width.max(0), viewport.height.max(0)));
    ctx.set_child_overflow_propagation(id, false);

    (content_size, scroll)
}

fn clamp_scroll_for_viewport(scroll: Vec2i, content_size: Dimensioni, viewport: Recti, padding: i32) -> Vec2i {
    clamp_scroll_for_padded_content(scroll, super::super::add_padding(content_size, padding), viewport)
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
    id: UiNodeId,
    state: &SharedScrollAreaState,
    scroll_behavior: ScrollBehavior,
    event: &UiInputEvent,
) -> InputResult {
    let Some((clip, body)) = ctx.node_clip_and_control(id) else {
        return InputResult::Ignored;
    };
    if scroll_behavior.is_no_scroll() {
        return InputResult::Ignored;
    }

    let state_snapshot = *state.borrow();
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
    id: UiNodeId,
    shared_state: &SharedScrollAreaState,
    state: &ScrollAxisState,
    scroll_behavior: ScrollBehavior,
    event: &UiInputEvent,
) -> InputResult {
    let Some(track) = ctx.node_rect(id) else {
        return InputResult::Ignored;
    };
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
            match state.axis {
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
            match state.axis {
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

fn paint_scroll_area_panel(ctx: &mut PaintCtx<'_>, id: UiNodeId, opt: ContainerOption) {
    let Some(rect) = ctx.node_rect(id) else {
        return;
    };
    if !opt.intersects(ContainerOption::NO_FRAME) {
        ctx.draw_frame(rect, ControlColor::PanelBG);
    }
}

fn paint_scrollbar_node(ctx: &mut PaintCtx<'_>, id: UiNodeId, shared_state: &SharedScrollAreaState, scroll_behavior: ScrollBehavior, part: &ScrollbarNodePart) {
    if scroll_behavior.is_no_scroll() {
        return;
    }
    let scrollbar_size = ctx.style.scrollbar_size.max(0);
    if scrollbar_size <= 0 {
        return;
    }
    let Some(track) = ctx.node_rect(id) else {
        return;
    };
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
