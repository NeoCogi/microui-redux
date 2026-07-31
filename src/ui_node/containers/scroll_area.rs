use std::{cell::RefCell, rc::Rc};

use bitflags::bitflags;

use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, scrollbar_viewport_body, ScrollAxis};
use crate::{ControlColor, Dimensioni, Recti, Vec2i};

use super::{InputCtx, InputResult, LayoutCtx, LegacyColumn, LegacyContainer, MeasureCtx, NodeBehavior, PaintCtx, UiInputEvent};
use crate::ui_node::{UiNode, UiNodeId, UiNodeState};
#[cfg(test)]
use crate::ui_node::UiNodeData;

bitflags! {
    #[derive(Copy, Clone)]
    /// Options that control a retained scroll area.
    pub struct ScrollAreaOption : u32 {
        /// Gives the scroll area a Style-owned outer border and inset content area.
        const FRAME = 1024;
        /// Enables scrolling and scrollbars for overflowing content.
        const ENABLE_SCROLL = 32;
        /// No special options.
        const NONE = 0;
    }
}

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
    let root = roots.iter().find(|root| root.with_node(scroll_area, |_| ()).is_some())?;
    root.with_node(scroll_area, |node| match &node.data {
        UiNodeData::Widget(widget) => widget.debug_scroll_area_state(),
        UiNodeData::Container(_) => None,
        UiNodeData::LegacyContainer(container) => container.debug_scroll_area_state(),
        UiNodeData::LegacyWidget(widget) => widget.debug_scroll_area_state(),
    })?
}

#[cfg(test)]
pub(crate) fn set_scroll_area_scroll(roots: &mut [UiNode], scroll_area: UiNodeId, scroll: Vec2i) -> bool {
    let Some(index) = roots.iter().position(|root| root.with_node(scroll_area, |_| ()).is_some()) else {
        return false;
    };
    roots[index]
        .with_node_mut(scroll_area, |node| match &mut node.data {
            UiNodeData::Widget(widget) => widget.debug_set_scroll_area_scroll(scroll),
            UiNodeData::Container(_) => false,
            UiNodeData::LegacyContainer(container) => container.debug_set_scroll_area_scroll(scroll),
            UiNodeData::LegacyWidget(widget) => widget.debug_set_scroll_area_scroll(scroll),
        })
        .unwrap_or(false)
}

/// Scroll-area container.
pub(crate) struct ScrollArea {
    /// Shared state used by the scroll-area container, viewport, and scrollbar parts.
    state: SharedScrollAreaState,
    /// Whether overflow scrolling and scrollbar interaction are enabled.
    scroll_enabled: bool,
    /// Scrollbar interaction/paint component.
    scrollbars: Scrollbars,
    /// Rendering options applied to the scroll-area panel.
    pub(crate) opt: ScrollAreaOption,
    /// Internal viewport and scrollbar child nodes.
    pub(crate) children: Vec<UiNode>,
}

impl ScrollArea {
    /// Creates a composed scroll area from viewport, scrollbar, and content components.
    pub(crate) fn new(state: SharedScrollAreaState, opt: ScrollAreaOption, children: Vec<UiNode>) -> Self {
        let scroll_enabled = opt.intersects(ScrollAreaOption::ENABLE_SCROLL);
        Self {
            state: state.clone(),
            scroll_enabled,
            scrollbars: Scrollbars { state, scroll_enabled },
            opt,
            children,
        }
    }
}

/// Viewport component that owns content-space layout and scroll translation.
pub(crate) struct ScrollViewport {
    pub(crate) state: SharedScrollAreaState,
    pub(crate) content: LegacyColumn,
    pub(crate) scroll_enabled: bool,
}

impl ScrollViewport {
    pub(crate) fn new(state: SharedScrollAreaState, scroll_enabled: bool, children: Vec<UiNode>) -> Self {
        Self {
            state,
            content: LegacyColumn { children },
            scroll_enabled,
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
    scroll_enabled: bool,
}

impl Scrollbars {
    fn layout_nodes(&self, ctx: &mut LayoutCtx<'_>, children: &mut [UiNode], body: Recti) {
        let scrollbar_size = ctx.style.scrollbar_size.max(0);
        let vertical = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
        let horizontal = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
        let corner = Recti::new(vertical.x, horizontal.y, vertical.width, horizontal.height);
        if let Some(child) = children.get_mut(1) {
            ctx.layout_node_ref(child, vertical);
        }
        if let Some(child) = children.get_mut(2) {
            ctx.layout_node_ref(child, horizontal);
        }
        if let Some(child) = children.get_mut(3) {
            ctx.layout_node_ref(child, corner);
        }
    }
}

impl NodeBehavior for ScrollArea {
    fn is_framed(&self) -> bool {
        self.opt.intersects(ScrollAreaOption::FRAME)
    }

    fn measure(&self, ctx: &MeasureCtx<'_>, _state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        self.children
            .first()
            .map(|viewport| ctx.measure_node_ref(viewport, available))
            .unwrap_or_default()
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        let content_hint = self.state.borrow().content_size;
        let mut body = viewport_body_for_scroll_enabled(rect, content_hint, ctx.style.padding, ctx.style.scrollbar_size, self.scroll_enabled);
        ctx.set_content_space_geometry(state, rect, rect, Vec2i::default());
        ctx.set_content_size(state, Dimensioni::new(rect.width.max(0), rect.height.max(0)));
        ctx.set_child_overflow_propagation(state, false);
        if let Some(viewport) = self.children.first_mut() {
            ctx.layout_node_ref(viewport, body);
        }
        let content_size = self.state.borrow().content_size;
        let next_body = viewport_body_for_scroll_enabled(rect, content_size, ctx.style.padding, ctx.style.scrollbar_size, self.scroll_enabled);
        if !super::super::same_rect(next_body, body) {
            body = next_body;
            ctx.set_content_space_geometry(state, rect, rect, Vec2i::default());
            if let Some(viewport) = self.children.first_mut() {
                ctx.layout_node_ref(viewport, body);
            }
        }
        let content_size = self.state.borrow().content_size;
        set_viewport_layout_state(&self.state, rect, body, content_size);
        self.scrollbars.layout_nodes(ctx, &mut self.children, body);
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        update_scroll_area_viewport_on_input(ctx, state, &self.state, self.scroll_enabled, event)
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

impl LegacyContainer for ScrollArea {
    fn children(&self) -> &[UiNode] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<UiNode> {
        &mut self.children
    }
}

impl NodeBehavior for ScrollViewport {
    fn measure(&self, ctx: &MeasureCtx<'_>, state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        self.content.measure(ctx, state, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        self.layout_content(ctx, state, rect, rect);
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        update_scroll_area_viewport_on_input(ctx, state, &self.state, self.scroll_enabled, event)
    }
}

impl LegacyContainer for ScrollViewport {
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
    scroll_enabled: bool,
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
        update_scrollbar_on_input(ctx, state, &self.state, axis_state, self.scroll_enabled, event)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, state: &mut UiNodeState) -> bool {
        paint_scrollbar_node(ctx, state, &self.state, self.scroll_enabled, &self.part);
        false
    }
}

fn scrollbar_node(state: SharedScrollAreaState, scroll_enabled: bool, part: ScrollbarPart) -> UiNode {
    let node_part = match part {
        ScrollbarPart::Track(axis) => ScrollbarNodePart::Track(ScrollAxisState { axis }),
        ScrollbarPart::Corner => ScrollbarNodePart::Corner,
    };
    UiNode::legacy_widget(Box::new(ScrollbarNode { state, scroll_enabled, part: node_part }))
}

pub(crate) fn scroll_viewport_node(_parent: UiNodeId, state: SharedScrollAreaState, scroll_enabled: bool, children: Vec<UiNode>) -> UiNode {
    UiNode::legacy_container(Box::new(ScrollViewport::new(state, scroll_enabled, children)))
}

pub(crate) fn scrollbar_nodes(_parent: UiNodeId, state: SharedScrollAreaState, scroll_enabled: bool) -> Vec<UiNode> {
    vec![
        scrollbar_node(state.clone(), scroll_enabled, ScrollbarPart::Track(ScrollAxis::Vertical)),
        scrollbar_node(state.clone(), scroll_enabled, ScrollbarPart::Track(ScrollAxis::Horizontal)),
        scrollbar_node(state, scroll_enabled, ScrollbarPart::Corner),
    ]
}

fn layout_viewport_content(
    ctx: &mut LayoutCtx<'_>,
    shared_state: &SharedScrollAreaState,
    content: &mut LegacyColumn,
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
    content: &mut LegacyColumn,
    state: &mut UiNodeState,
    viewport: Recti,
    content_hint: Dimensioni,
    scroll: Vec2i,
) -> (Dimensioni, Vec2i) {
    let padded_hint = super::super::add_padding(content_hint, ctx.style.padding.max(0));
    let scroll = clamp_scroll_for_padded_content(scroll, padded_hint, viewport);
    let padding = ctx.style.padding.max(0);
    let child_rect = crate::expand_rect(Recti::new(0, 0, viewport.width, viewport.height), -padding);
    let child_offset = Vec2i::new(viewport.x.saturating_sub(scroll.x), viewport.y.saturating_sub(scroll.y));
    ctx.set_content_space_geometry(state, viewport, viewport, child_offset);

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

fn viewport_body_for_scroll_enabled(rect: Recti, content_size: Dimensioni, padding: i32, scrollbar_size: i32, scroll_enabled: bool) -> Recti {
    if !scroll_enabled {
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
    scroll_enabled: bool,
    event: &UiInputEvent,
) -> InputResult {
    let state_snapshot = *state.borrow();
    let body = state_snapshot.body;
    if !scroll_enabled {
        return InputResult::Ignored;
    }

    let content_size = state_snapshot.content_size;
    let content = super::super::add_padding(content_size, ctx.style.padding.max(0));
    let max_x = scrollbar_max_scroll(content.width, body.width);
    let max_y = scrollbar_max_scroll(content.height, body.height);
    let mut scroll = state_snapshot.scroll;

    let result = match event {
        UiInputEvent::Scroll { pos, delta } => {
            let hovered_body = ctx.contains(body, *pos);
            if hovered_body && (delta.x != 0 || delta.y != 0) {
                scroll.x = scroll.x.saturating_add(delta.x);
                scroll.y = scroll.y.saturating_add(delta.y);
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
    scroll_enabled: bool,
    event: &UiInputEvent,
) -> InputResult {
    let _ = node_state;
    let track = ctx.content_rect();
    if !scroll_enabled || ctx.style.scrollbar_size.max(0) <= 0 {
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
        UiInputEvent::MouseDown { pos, button } if button.intersects(crate::MouseButton::LEFT) && ctx.contains(track, *pos) => InputResult::Captured,
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
        UiInputEvent::Scroll { pos, delta } if ctx.contains(track, *pos) => {
            match axis_state.axis {
                ScrollAxis::Vertical if delta.y != 0 => scroll.y = scroll.y.saturating_add(delta.y),
                ScrollAxis::Horizontal if delta.x != 0 => scroll.x = scroll.x.saturating_add(delta.x),
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

fn paint_scroll_area_panel(ctx: &mut PaintCtx<'_>, state: &UiNodeState, opt: ScrollAreaOption) {
    let _ = opt;
    let _ = state;
    let rect = ctx.content_rect();
    ctx.draw_flat_rect(rect, ControlColor::PanelBG);
}

fn paint_scrollbar_node(ctx: &mut PaintCtx<'_>, state: &UiNodeState, shared_state: &SharedScrollAreaState, scroll_enabled: bool, part: &ScrollbarNodePart) {
    if !scroll_enabled {
        return;
    }
    let scrollbar_size = ctx.style.scrollbar_size.max(0);
    if scrollbar_size <= 0 {
        return;
    }
    let _ = state;
    let track = ctx.content_rect();
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
            ctx.draw_flat_rect(track, ControlColor::ScrollBase);
            ctx.draw_flat_rect(thumb, ControlColor::ScrollThumb);
        }
        ScrollAxis::Horizontal if content.width > body.width => {
            let thumb = scrollbar_thumb(ScrollAxis::Horizontal, track, body.width, content.width, scroll_offset.x, scrollbar_size);
            ctx.draw_flat_rect(track, ControlColor::ScrollBase);
            ctx.draw_flat_rect(thumb, ControlColor::ScrollThumb);
        }
        _ => {}
    }
}
