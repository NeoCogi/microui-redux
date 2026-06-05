use crate::input::{ContainerOption, ScrollBehavior};
use crate::scroll::ScrollAreaHandle;
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis};
use crate::{ControlColor, Dimensioni, Recti, Style, Vec2i};

use super::{ClientArea, Column, ContainerTrait, LayoutCtx, MeasureCtx, PaintCtx, ScrollDispatchCtx};
use crate::context::NodeLayout;
use crate::ui_node::UiNodeId;

/// Scroll-area container.
#[derive(Clone)]
pub(crate) struct ScrollArea {
    /// Scrollable content layout.
    pub(crate) content: Column,
    /// Retained scroll-area state shared with user handles.
    pub(crate) handle: ScrollAreaHandle,
    /// Internal scrollable content size.
    pub(crate) content_size: Dimensioni,
    /// Current scroll offset.
    pub(crate) scroll_offset: Vec2i,
    /// Active scrollbar drag axis.
    pub(crate) scroll_drag: Option<ScrollAxis>,
    /// Scroll behavior applied while traversing this node's children.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Rendering options applied to the scroll-area panel.
    pub(crate) opt: ContainerOption,
}

impl ContainerTrait for ScrollArea {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        self.content.measure(ctx, id, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let layout = layout_scroll_area_children(ctx, &mut self.content, id, rect, clip, &self.handle, self.scroll_behavior);
        self.content_size = layout.content_size;
        self.scroll_offset = layout.scroll;
    }

    fn dispatch_scroll(&mut self, ctx: &mut ScrollDispatchCtx<'_>, id: UiNodeId) -> bool {
        dispatch_scroll_area_input(ctx, id, self)
    }

    fn is_scroll_area(&self) -> bool {
        true
    }

    fn scroll_area_runtime_state(&self) -> Option<(Dimensioni, Vec2i, Option<ScrollAxis>)> {
        Some((self.content_size, self.scroll_offset, self.scroll_drag))
    }

    fn set_scroll_area_runtime_state(&mut self, content_size: Dimensioni, offset: Vec2i, drag: Option<ScrollAxis>) {
        self.content_size = content_size;
        self.scroll_offset = offset;
        self.scroll_drag = drag;
    }

    fn paint_before_children(&mut self, ctx: &mut PaintCtx<'_>, id: UiNodeId) -> bool {
        paint_scroll_area_panel(ctx, id, self.opt);
        ctx.push_node_clip(id);
        true
    }

    fn paint_after_children(&mut self, ctx: &mut PaintCtx<'_>, id: UiNodeId) {
        ctx.pop_node_clip();
        paint_scroll_area_scrollbars(ctx, id, self);
    }
}

struct ScrollAreaLayout {
    body: Recti,
    content_size: Dimensioni,
    scroll: Vec2i,
}

fn layout_scroll_area_children(
    ctx: &mut LayoutCtx<'_>,
    content: &mut Column,
    id: UiNodeId,
    rect: Recti,
    clip: Recti,
    handle: &ScrollAreaHandle,
    scroll_behavior: ScrollBehavior,
) -> ScrollAreaLayout {
    let mut content_hint = handle.with(|area| area.content_size());
    let requested_scroll = handle.with(|area| area.scroll());
    let mut layout = ScrollAreaLayout {
        body: scroll_area_body_for_content(rect, ctx.style, scroll_behavior, content_hint),
        content_size: content_hint,
        scroll: requested_scroll,
    };

    for _ in 0..3 {
        layout = layout_scroll_area_once(ctx, content, id, rect, clip, scroll_behavior, content_hint, requested_scroll);
        let next_body = scroll_area_body_for_content(rect, ctx.style, scroll_behavior, layout.content_size);
        if super::super::same_rect(next_body, layout.body) {
            break;
        }
        content_hint = layout.content_size;
    }

    let layout_snapshot = NodeLayout::new(rect, layout.body, layout.content_size);
    handle.with_inner_mut(|area| {
        area.apply_viewport_layout(layout_snapshot);
        area.set_scroll(layout.scroll);
    });
    layout
}

fn layout_scroll_area_once(
    ctx: &mut LayoutCtx<'_>,
    content: &mut Column,
    id: UiNodeId,
    rect: Recti,
    clip: Recti,
    scroll_behavior: ScrollBehavior,
    content_hint: Dimensioni,
    scroll: Vec2i,
) -> ScrollAreaLayout {
    let body = scroll_area_body_for_content(rect, ctx.style, scroll_behavior, content_hint);
    let padded_hint = super::super::add_padding(content_hint, ctx.style.padding.max(0));
    let scroll = Vec2i::new(
        scroll.x.clamp(0, scrollbar_max_scroll(padded_hint.width, body.width)),
        scroll.y.clamp(0, scrollbar_max_scroll(padded_hint.height, body.height)),
    );
    let mut child_rect = crate::expand_rect(body, -ctx.style.padding);
    child_rect.x = child_rect.x.saturating_sub(scroll.x);
    child_rect.y = child_rect.y.saturating_sub(scroll.y);

    let padded_virtual_size = super::super::add_padding(content_hint, ctx.style.padding.max(0));
    let client_area = ClientArea {
        visible_rect: body,
        virtual_size: padded_virtual_size,
        virtual_clip: body,
        translation: Vec2i::new(-scroll.x, -scroll.y),
    };
    let child_clip = client_area.effective_clip(clip);
    ctx.set_client_area_geometry(id, rect, client_area, clip);

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

    ctx.set_content_size(id, Dimensioni::new(rect.width.max(0), rect.height.max(0)));

    ScrollAreaLayout { body, content_size, scroll }
}

fn scroll_area_body_for_content(rect: Recti, style: &Style, scroll_behavior: ScrollBehavior, content_size: Dimensioni) -> Recti {
    if scroll_behavior.is_no_scroll() {
        return rect;
    }
    let scrollbar_size = style.scrollbar_size.max(0);
    if scrollbar_size <= 0 {
        return rect;
    }
    let content = super::super::add_padding(content_size, style.padding.max(0));
    let mut body = rect;
    for _ in 0..3 {
        let needs_vertical = content.height > body.height && body.height > 0;
        let needs_horizontal = content.width > body.width && body.width > 0;
        let mut next = rect;
        if needs_vertical {
            next.width = next.width.saturating_sub(scrollbar_size);
        }
        if needs_horizontal {
            next.height = next.height.saturating_sub(scrollbar_size);
        }
        if super::super::same_rect(next, body) {
            break;
        }
        body = next;
    }
    body
}

fn dispatch_scroll_area_input(ctx: &mut ScrollDispatchCtx<'_>, id: UiNodeId, scroll_area: &mut ScrollArea) -> bool {
    let Some((clip, body)) = ctx.node_clip_and_client(id) else {
        return false;
    };
    let content_size = scroll_area.content_size;
    let handle = scroll_area.handle.clone();
    let scroll_behavior = scroll_area.scroll_behavior;
    let mut scroll_drag = scroll_area.scroll_drag;
    if scroll_behavior.is_no_scroll() {
        return false;
    }

    let content = super::super::add_padding(content_size, ctx.style.padding.max(0));
    let max_x = scrollbar_max_scroll(content.width, body.width);
    let max_y = scrollbar_max_scroll(content.height, body.height);
    let scrollbar_size = ctx.style.scrollbar_size.max(0);
    let vertical = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
    let horizontal = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
    let hovered_body = body.contains(&ctx.input.mouse_pos) && clip.contains(&ctx.input.mouse_pos);
    let hovered_vertical = max_y > 0 && vertical.contains(&ctx.input.mouse_pos);
    let hovered_horizontal = max_x > 0 && horizontal.contains(&ctx.input.mouse_pos);
    let wheel_input = ctx.input.scroll_delta.x != 0 || ctx.input.scroll_delta.y != 0;
    let mut owns_event = hovered_body && wheel_input;
    let mut scroll = handle.with(|area| area.scroll());
    if ctx.input.mouse_down.is_empty() {
        scroll_drag = None;
    } else if ctx.input.mouse_pressed.intersects(crate::MouseButton::LEFT) && scrollbar_size > 0 {
        if hovered_vertical {
            scroll_drag = Some(ScrollAxis::Vertical);
            owns_event = true;
        } else if hovered_horizontal {
            scroll_drag = Some(ScrollAxis::Horizontal);
            owns_event = true;
        }
    }

    match scroll_drag {
        Some(ScrollAxis::Vertical) if max_y > 0 => {
            let base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
            scroll.y = scroll
                .y
                .saturating_add(scrollbar_drag_delta(ScrollAxis::Vertical, ctx.input.mouse_delta, content.height, base));
        }
        Some(ScrollAxis::Horizontal) if max_x > 0 => {
            let base = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
            scroll.x = scroll
                .x
                .saturating_add(scrollbar_drag_delta(ScrollAxis::Horizontal, ctx.input.mouse_delta, content.width, base));
        }
        _ => {}
    }

    if hovered_body {
        scroll.x = scroll.x.saturating_sub(ctx.input.scroll_delta.x);
        scroll.y = scroll.y.saturating_sub(ctx.input.scroll_delta.y);
    }
    scroll.x = scroll.x.clamp(0, max_x);
    scroll.y = scroll.y.clamp(0, max_y);

    handle.with_inner_mut(|area| area.set_scroll(scroll));
    scroll_area.scroll_offset = scroll;
    scroll_area.scroll_drag = scroll_drag;
    owns_event || scroll_drag.is_some()
}

fn paint_scroll_area_panel(ctx: &mut PaintCtx<'_>, id: UiNodeId, opt: ContainerOption) {
    let Some(rect) = ctx.node_rect(id) else {
        return;
    };
    if !opt.intersects(ContainerOption::NO_FRAME) {
        ctx.draw_frame(rect, ControlColor::PanelBG);
    }
}

fn paint_scroll_area_scrollbars(ctx: &mut PaintCtx<'_>, id: UiNodeId, scroll_area: &ScrollArea) {
    let Some(body) = ctx.node_client(id) else {
        return;
    };
    let content_size = scroll_area.content_size;
    let scroll_offset = scroll_area.scroll_offset;
    let scroll_behavior = scroll_area.scroll_behavior;
    if scroll_behavior.is_no_scroll() {
        return;
    }
    let scrollbar_size = ctx.style.scrollbar_size.max(0);
    if scrollbar_size <= 0 {
        return;
    }
    let content = super::super::add_padding(content_size, ctx.style.padding.max(0));
    if content.height > body.height {
        let base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
        let thumb = scrollbar_thumb(ScrollAxis::Vertical, base, body.height, content.height, scroll_offset.y, scrollbar_size);
        ctx.draw_frame(base, ControlColor::Base);
        ctx.draw_frame(thumb, ControlColor::Button);
    }
    if content.width > body.width {
        let base = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
        let thumb = scrollbar_thumb(ScrollAxis::Horizontal, base, body.width, content.width, scroll_offset.x, scrollbar_size);
        ctx.draw_frame(base, ControlColor::Base);
        ctx.draw_frame(thumb, ControlColor::Button);
    }
}
