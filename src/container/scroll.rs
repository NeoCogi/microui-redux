//! Viewport scroll state, scrollbar geometry, and scrollbar dispatch.

use super::*;
use crate::scrollbar::{scrollbar_max_scroll, ScrollAxis};

#[derive(Clone)]
/// Per-container scroll offsets and retained scrollbar widget state.
pub(super) struct ScrollState {
    /// Accumulated scroll offset.
    offset: Vec2i,
    /// Determines whether container scrollbars and scroll consumption are enabled.
    enabled: bool,
    /// Internal widget state for the vertical scrollbar.
    vertical: Scrollbar,
    /// Internal widget state for the horizontal scrollbar.
    horizontal: Scrollbar,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollState {
    /// Creates enabled scroll state with fresh axis-specific scrollbar widgets.
    fn new() -> Self {
        Self {
            offset: Vec2i::default(),
            enabled: true,
            vertical: Scrollbar::new(ScrollAxis::Vertical),
            horizontal: Scrollbar::new(ScrollAxis::Horizontal),
        }
    }

    /// Restores the scroll state to its initial enabled, unscrolled state.
    pub(super) fn reset(&mut self) {
        self.offset = Vec2i::default();
        self.enabled = true;
    }

    /// Re-enables scrolling at the beginning of a frame before widget policy is applied.
    pub(super) fn prepare_frame(&mut self) {
        self.enabled = true;
    }

    /// Returns the current scroll offset.
    pub(super) fn offset(&self) -> Vec2i {
        self.offset
    }

    /// Replaces the current scroll offset.
    pub(super) fn set_offset(&mut self, offset: Vec2i) {
        self.offset = offset;
    }

    /// Returns whether scrolling is currently enabled for this viewport.
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }
}

impl ScrollState {
    /// Returns the scroll offset for one axis.
    fn axis(&self, axis: ScrollAxis) -> i32 {
        match axis {
            ScrollAxis::Vertical => self.offset.y,
            ScrollAxis::Horizontal => self.offset.x,
        }
    }

    /// Replaces the scroll offset for one axis.
    fn set_axis(&mut self, axis: ScrollAxis, value: i32) {
        match axis {
            ScrollAxis::Vertical => self.offset.y = value,
            ScrollAxis::Horizontal => self.offset.x = value,
        }
    }

    /// Temporarily removes a scrollbar so it can be updated without borrowing `self`.
    fn take_scrollbar(&mut self, axis: ScrollAxis) -> Scrollbar {
        match axis {
            ScrollAxis::Vertical => std::mem::replace(&mut self.vertical, Scrollbar::new(ScrollAxis::Vertical)),
            ScrollAxis::Horizontal => std::mem::replace(&mut self.horizontal, Scrollbar::new(ScrollAxis::Horizontal)),
        }
    }

    /// Restores a scrollbar after update or paint dispatch.
    fn restore_scrollbar(&mut self, scrollbar: Scrollbar) {
        match scrollbar.axis() {
            ScrollAxis::Vertical => self.vertical = scrollbar,
            ScrollAxis::Horizontal => self.horizontal = scrollbar,
        }
    }
}

impl TraversalHost {
    #[cfg(test)]
    pub(crate) fn scrollbar_node_ids_for_test(&self) -> (NodeId, NodeId) {
        (
            self.viewport.scroll.vertical.node_id(self.internal_id_seed),
            self.viewport.scroll.horizontal.node_id(self.internal_id_seed),
        )
    }

    /// Applies pending wheel/trackpad scroll to this container when it owns the hover route.
    pub(crate) fn consume_pending_scroll(&mut self) {
        if !self.viewport.scroll.enabled {
            return;
        }
        let delta = match self.interaction.pending_scroll {
            Some(delta) if delta.x != 0 || delta.y != 0 => delta,
            _ => return,
        };

        let mut consumed = false;
        let mut scroll = self.viewport.scroll.offset;
        let padding = self.style.as_ref().padding;
        // Scrollbars account for padded content because layout extents are tracked inside padding.
        let content_size = Self::padded_scrollbar_content_size(self.viewport.content_size, padding);
        let body = self.viewport.body;

        let maxscroll_y = content_size.height - body.height;
        if delta.y != 0 && maxscroll_y > 0 && body.height > 0 {
            let new_scroll = Self::clamp(scroll.y + delta.y, 0, maxscroll_y);
            if new_scroll != scroll.y {
                scroll.y = new_scroll;
                consumed = true;
            }
        }

        let maxscroll_x = content_size.width - body.width;
        if delta.x != 0 && maxscroll_x > 0 && body.width > 0 {
            let new_scroll = Self::clamp(scroll.x + delta.x, 0, maxscroll_x);
            if new_scroll != scroll.x {
                scroll.x = new_scroll;
                consumed = true;
            }
        }

        if consumed {
            // Once a container consumes scroll, parents should not also apply the same delta.
            self.viewport.scroll.offset = scroll;
            self.interaction.clear_pending_scroll();
        }
    }

    /// Applies the caller's scroll policy to this container's scroll area state.
    pub(crate) fn apply_scroll_behavior(&mut self, scroll_behavior: ScrollBehavior) {
        self.viewport.scroll.enabled = !scroll_behavior.is_no_scroll();
    }

    /// Shrinks the body for visible scrollbars and clamps scroll offsets into valid ranges.
    fn resolve_scrollbars(&mut self, body: &mut Recti) {
        let (scrollbar_size, padding) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding)
        };
        let cs = Self::padded_scrollbar_content_size(self.viewport.content_size, padding);
        *body = Self::resolved_scrollbar_body(*body, cs, scrollbar_size);
        let body = *body;
        let maxscroll_y = scrollbar_max_scroll(cs.height, body.height);
        self.viewport.scroll.offset.y = if maxscroll_y > 0 && body.height > 0 {
            Self::clamp(self.viewport.scroll.offset.y, 0, maxscroll_y)
        } else {
            0
        };

        let maxscroll_x = scrollbar_max_scroll(cs.width, body.width);
        self.viewport.scroll.offset.x = if maxscroll_x > 0 && body.width > 0 {
            Self::clamp(self.viewport.scroll.offset.x, 0, maxscroll_x)
        } else {
            0
        };
    }

    /// Adds style padding to content size before evaluating scrollbar ranges.
    fn padded_scrollbar_content_size(mut content_size: Dimensioni, padding: i32) -> Dimensioni {
        content_size.width += padding * 2;
        content_size.height += padding * 2;
        content_size
    }

    /// Resolves the body rectangle after vertical and horizontal scrollbar gutters are considered.
    fn resolved_scrollbar_body(body: Recti, content_size: Dimensioni, scrollbar_size: i32) -> Recti {
        let scrollbar_size = scrollbar_size.max(0);
        let mut resolved = body;
        // A vertical bar can force a horizontal bar and vice versa. Re-evaluate until body size
        // stops shrinking.
        for _ in 0..3 {
            let needs_vertical = content_size.height > resolved.height && resolved.height > 0;
            let needs_horizontal = content_size.width > resolved.width && resolved.width > 0;
            let mut next = body;
            if needs_vertical {
                next.width = next.width.saturating_sub(scrollbar_size).max(0);
            }
            if needs_horizontal {
                next.height = next.height.saturating_sub(scrollbar_size).max(0);
            }
            if (next.x, next.y, next.width, next.height) == (resolved.x, resolved.y, resolved.width, resolved.height) {
                break;
            }
            resolved = next;
        }
        resolved
    }

    /// Resolves the scrollable body for a content-size hint without mutating the container.
    pub(crate) fn resolved_body_for_content(&self, body: Recti, scroll_behavior: ScrollBehavior, content_size: Dimensioni) -> Recti {
        if scroll_behavior.is_no_scroll() {
            return body;
        }
        let style = self.style.as_ref();
        let content_size = Self::padded_scrollbar_content_size(content_size, style.padding);
        Self::resolved_scrollbar_body(body, content_size, style.scrollbar_size)
    }

    /// Commits content size from the active layout scope.
    pub(crate) fn commit_active_layout_content_size(&mut self) -> Dimensioni {
        let layout_body = self.layout.current_body();
        let content_size = self
            .layout
            .current_max()
            .map(|lm| Dimensioni::new(lm.x - layout_body.x, lm.y - layout_body.y))
            .unwrap_or_default();
        self.set_content_size(content_size);
        content_size
    }

    /// Finishes a body layout pass and returns the resolved viewport snapshot.
    pub(crate) fn finish_body_layout_scope(&mut self) -> NodeLayout {
        self.commit_active_layout_content_size();
        let layout = NodeLayout::new(self.rect(), self.body(), self.content_size());
        self.layout.pop_scope();
        layout
    }

    /// Lays out retained children, repeating only when current content changes scrollbar gutters.
    pub(crate) fn layout_body_until_scrollbars_stable(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        rect: Recti,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        self.layout_viewport_body_until_scrollbars_stable(results, resources, rect, rect, scroll_behavior, children)
    }

    /// Lays out a body inside an outer viewport, repeating when content changes scrollbar gutters.
    pub(crate) fn layout_viewport_body_until_scrollbars_stable(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        viewport_rect: Recti,
        body_rect: Recti,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        let mut content_hint = self.content_size();
        let mut layout = self.layout_body_with_content_hint(results, resources, viewport_rect, body_rect, scroll_behavior, content_hint, children);
        for _ in 0..3 {
            let resolved_body = self.resolved_body_for_content(body_rect, scroll_behavior, layout.content_size);
            if Self::same_rect(resolved_body, layout.body) {
                return layout;
            }
            content_hint = layout.content_size;
            layout = self.layout_body_with_content_hint(results, resources, viewport_rect, body_rect, scroll_behavior, content_hint, children);
        }
        layout
    }

    /// Performs one retained child layout pass using `content_hint` for scrollbar decisions.
    fn layout_body_with_content_hint(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        viewport_rect: Recti,
        body_rect: Recti,
        scroll_behavior: ScrollBehavior,
        content_hint: Dimensioni,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        self.tree_cache.begin_frame();
        self.set_rect(viewport_rect);
        self.set_content_size(content_hint);
        self.configure_container_body(body_rect, scroll_behavior);
        self.layout_tree_nodes(results, resources, children);
        self.finish_body_layout_scope()
    }

    /// Runs the retained update pass for a resolved body and updates its scrollbar controls.
    pub(crate) fn update_body_tree(
        &mut self,
        results: &mut FrameResults,
        resources: &WidgetTreeResources,
        layout: NodeLayout,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) {
        self.apply_viewport_layout(layout);
        self.apply_scroll_behavior(scroll_behavior);
        self.push_clip_rect(layout.body);
        self.update_tree_nodes(results, resources, children);
        self.pop_clip_rect();
        self.update_active_scrollbars();
        self.consume_pending_scroll();
    }

    /// Runs the retained paint pass for a resolved body and paints scrollbars above child content.
    pub(crate) fn paint_body_tree(
        &mut self,
        resources: &WidgetTreeResources,
        layout: NodeLayout,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) {
        self.apply_viewport_layout(layout);
        self.apply_scroll_behavior(scroll_behavior);
        self.push_clip_rect(layout.body);
        self.paint_tree_nodes(resources, children);
        self.pop_clip_rect();
        self.paint_active_scrollbars();
    }

    /// Compares rectangle components without relying on external trait behavior.
    fn same_rect(a: Recti, b: Recti) -> bool {
        (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height)
    }

    #[inline(never)]
    /// Updates and paints scrollbars for tests and legacy callers.
    #[cfg(test)]
    pub(crate) fn scrollbars(&mut self, body: &mut Recti) {
        self.resolve_scrollbars(body);
        self.render_scrollbars(*body);
    }

    /// Updates and paints scrollbars for the active body when scrolling is enabled.
    #[cfg(test)]
    pub(crate) fn render_active_scrollbars(&mut self) {
        if self.viewport.scroll.enabled {
            self.update_scrollbars(self.viewport.body);
            self.paint_scrollbars(self.viewport.body);
        }
    }

    /// Updates active scrollbar interaction without painting it.
    pub(crate) fn update_active_scrollbars(&mut self) {
        if self.viewport.scroll.enabled {
            self.update_scrollbars(self.viewport.body);
        }
    }

    /// Paints active scrollbars using interaction recorded earlier in the frame.
    pub(crate) fn paint_active_scrollbars(&mut self) {
        if self.viewport.scroll.enabled {
            self.paint_scrollbars(self.viewport.body);
        }
    }

    /// Runs scrollbar update and paint passes for `body`.
    #[cfg(test)]
    pub(crate) fn render_scrollbars(&mut self, body: Recti) {
        self.update_scrollbars(body);
        self.paint_scrollbars(body);
    }

    /// Returns content size including style padding that contributes to scrollbar range.
    fn scrollbar_content_size(&self, padding: i32) -> Dimensioni {
        Self::padded_scrollbar_content_size(self.viewport.content_size, padding)
    }

    /// Expands the clip to include scrollbar gutters when scrollbars exist.
    fn scrollbar_clip_rect(body: Recti, content_size: Dimensioni, scrollbar_size: i32) -> Recti {
        let mut clip_rect = body;
        if scrollbar_max_scroll(content_size.height, body.height) > 0 && body.height > 0 {
            clip_rect.width += scrollbar_size;
        }
        if scrollbar_max_scroll(content_size.width, body.width) > 0 && body.width > 0 {
            clip_rect.height += scrollbar_size;
        }
        clip_rect
    }

    /// Resolves one scrollbar's layout, returning `None` when that axis is not needed.
    fn scrollbar_layout(&self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32) -> Option<ScrollbarLayout> {
        match axis {
            ScrollAxis::Vertical => self
                .viewport
                .scroll
                .vertical
                .resolve_layout(self.internal_id_seed, body, content_size, scrollbar_size),
            ScrollAxis::Horizontal => self
                .viewport
                .scroll
                .horizontal
                .resolve_layout(self.internal_id_seed, body, content_size, scrollbar_size),
        }
    }

    /// Updates one scrollbar axis and adjusts the matching scroll offset when dragged.
    fn update_scrollbar(&mut self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32) {
        let Some(layout) = self.scrollbar_layout(axis, body, content_size, scrollbar_size) else {
            self.viewport.scroll.set_axis(axis, 0);
            return;
        };

        let scroll_value = self.viewport.scroll.axis(axis);
        let mut scrollbar = self.viewport.scroll.take_scrollbar(axis);
        scrollbar.configure(layout, scroll_value);
        self.record_tree_layout(
            layout.node_id,
            NodeLayout::new(layout.base, layout.base, Dimensioni::new(layout.base.width, layout.base.height)),
        );
        let (control, _result) = self.update_internal_node(layout.node_id, &mut scrollbar, layout.base);
        self.record_tree_control(layout.node_id, control);
        self.viewport.scroll.set_axis(axis, scrollbar.value());
        self.viewport.scroll.restore_scrollbar(scrollbar);
    }

    /// Paints one scrollbar axis using the interaction state recorded during update.
    fn paint_scrollbar(&mut self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32) {
        let Some(layout) = self.scrollbar_layout(axis, body, content_size, scrollbar_size) else {
            return;
        };

        let scroll_value = self.viewport.scroll.axis(axis);
        let mut scrollbar = self.viewport.scroll.take_scrollbar(axis);
        scrollbar.configure(layout, scroll_value);
        let control = self.tree_cache.current_control(layout.node_id).copied().unwrap_or_default();
        self.paint_internal_node(layout.node_id, &mut scrollbar, layout.base, &control);
        self.viewport.scroll.restore_scrollbar(scrollbar);
    }

    /// Updates both scrollbar axes under a clip that includes the scrollbar gutters.
    fn update_scrollbars(&mut self, body: Recti) {
        let (scrollbar_size, padding) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding)
        };
        let cs = self.scrollbar_content_size(padding);
        let maxscroll_y = scrollbar_max_scroll(cs.height, body.height);
        let maxscroll_x = scrollbar_max_scroll(cs.width, body.width);
        let clip_rect = Self::scrollbar_clip_rect(body, cs, scrollbar_size);
        // Internal controls live partly in the gutter, so they need a wider clip than content.
        self.push_clip_rect(clip_rect);
        if maxscroll_y > 0 {
            self.update_scrollbar(ScrollAxis::Vertical, body, cs, scrollbar_size);
        } else {
            self.viewport.scroll.offset.y = 0;
        }
        if maxscroll_x > 0 {
            self.update_scrollbar(ScrollAxis::Horizontal, body, cs, scrollbar_size);
        } else {
            self.viewport.scroll.offset.x = 0;
        }
        self.pop_clip_rect();
    }

    /// Paints both scrollbar axes under a clip that includes the scrollbar gutters.
    fn paint_scrollbars(&mut self, body: Recti) {
        let (scrollbar_size, padding) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding)
        };
        let cs = self.scrollbar_content_size(padding);
        let clip_rect = Self::scrollbar_clip_rect(body, cs, scrollbar_size);
        // Paint after update so thumb hover/active state comes from the current frame cache.
        self.push_clip_rect(clip_rect);
        self.paint_scrollbar(ScrollAxis::Vertical, body, cs, scrollbar_size);
        self.paint_scrollbar(ScrollAxis::Horizontal, body, cs, scrollbar_size);
        self.pop_clip_rect();
    }

    /// Configures layout state for the container's client area without drawing.
    pub(crate) fn configure_container_body(&mut self, body: Recti, scroll_behavior: ScrollBehavior) {
        let mut body = body;
        self.apply_scroll_behavior(scroll_behavior);
        if self.viewport.scroll.enabled {
            self.resolve_scrollbars(&mut body);
        }
        let (layout_padding, style_padding, font, style_clone) = {
            let style = self.style.as_ref();
            (-style.padding, style.padding, style.font, *style)
        };
        let scroll = self.viewport.scroll.offset;
        self.layout.reset(expand_rect(body, layout_padding), scroll);
        self.layout.style = style_clone;
        let font_height = self.atlas.get_font_height(font) as i32;
        let vertical_pad = crate::text_layout::vertical_text_padding(style_padding);
        let icon_height = self.atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let default_height = max(font_height + vertical_pad * 2, icon_height);
        self.layout.set_default_cell_height(default_height);
        self.viewport.body = body;
    }

    /// Configures layout state for the container's client area, handling scrollbars when necessary.
    #[cfg(test)]
    pub fn push_container_body(&mut self, body: Recti, _opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        self.configure_container_body(body, scroll_behavior);
        self.render_active_scrollbars();
    }
}
