//! Container scroll state, scrollbar geometry, and scrollbar dispatch.

use super::*;
use crate::scrollbar::{scrollbar_max_scroll, ScrollAxis};

#[derive(Clone)]
pub(super) struct ScrollState {
    /// Accumulated scroll offset.
    pub(super) offset: Vec2i,
    /// Determines whether container scrollbars and scroll consumption are enabled.
    pub(super) enabled: bool,
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
    fn new() -> Self {
        Self {
            offset: Vec2i::default(),
            enabled: true,
            vertical: Scrollbar::new(ScrollAxis::Vertical),
            horizontal: Scrollbar::new(ScrollAxis::Horizontal),
        }
    }

    pub(super) fn reset(&mut self) {
        self.offset = Vec2i::default();
        self.enabled = true;
    }

    pub(super) fn prepare_frame(&mut self) {
        self.enabled = true;
    }

    pub(super) fn clear_offset(&mut self) {
        self.offset = Vec2i::default();
    }
}

impl ScrollState {
    fn axis(&self, axis: ScrollAxis) -> i32 {
        match axis {
            ScrollAxis::Vertical => self.offset.y,
            ScrollAxis::Horizontal => self.offset.x,
        }
    }

    fn set_axis(&mut self, axis: ScrollAxis, value: i32) {
        match axis {
            ScrollAxis::Vertical => self.offset.y = value,
            ScrollAxis::Horizontal => self.offset.x = value,
        }
    }

    fn take_scrollbar(&mut self, axis: ScrollAxis) -> Scrollbar {
        match axis {
            ScrollAxis::Vertical => std::mem::replace(&mut self.vertical, Scrollbar::new(ScrollAxis::Vertical)),
            ScrollAxis::Horizontal => std::mem::replace(&mut self.horizontal, Scrollbar::new(ScrollAxis::Horizontal)),
        }
    }

    fn restore_scrollbar(&mut self, scrollbar: Scrollbar) {
        match scrollbar.axis() {
            ScrollAxis::Vertical => self.vertical = scrollbar,
            ScrollAxis::Horizontal => self.horizontal = scrollbar,
        }
    }
}

impl Container {
    #[cfg(test)]
    pub(crate) fn scrollbar_node_ids_for_test(&self) -> (NodeId, NodeId) {
        (
            self.scroll.vertical.node_id(self.internal_id_seed),
            self.scroll.horizontal.node_id(self.internal_id_seed),
        )
    }

    /// Applies pending wheel/trackpad scroll to this container when it owns the hover route.
    pub(crate) fn consume_pending_scroll(&mut self) {
        if !self.scroll.enabled {
            return;
        }
        let delta = match self.interaction.pending_scroll {
            Some(delta) if delta.x != 0 || delta.y != 0 => delta,
            _ => return,
        };

        let mut consumed = false;
        let mut scroll = self.scroll.offset;
        let mut content_size = self.content_size;
        let padding = self.style.as_ref().padding * 2;
        // Scrollbars account for padded content because layout extents are tracked inside padding.
        content_size.width += padding;
        content_size.height += padding;
        let body = self.body;

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
            self.scroll.offset = scroll;
            self.interaction.clear_pending_scroll();
        }
    }

    /// Shrinks the body for visible scrollbars and clamps scroll offsets into valid ranges.
    fn resolve_scrollbars(&mut self, body: &mut Recti) {
        let (scrollbar_size, padding) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding)
        };
        let sz = scrollbar_size;
        let mut cs = self.content_size;
        cs.width += padding * 2;
        cs.height += padding * 2;
        let base_body = *body;
        if cs.height > base_body.height {
            body.width -= sz;
        }
        if cs.width > base_body.width {
            body.height -= sz;
        }
        let body = *body;
        let maxscroll_y = scrollbar_max_scroll(cs.height, body.height);
        self.scroll.offset.y = if maxscroll_y > 0 && body.height > 0 {
            Self::clamp(self.scroll.offset.y, 0, maxscroll_y)
        } else {
            0
        };

        let maxscroll_x = scrollbar_max_scroll(cs.width, body.width);
        self.scroll.offset.x = if maxscroll_x > 0 && body.width > 0 {
            Self::clamp(self.scroll.offset.x, 0, maxscroll_x)
        } else {
            0
        };
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[inline(never)]
    /// Updates and paints scrollbars for tests and legacy callers.
    pub(crate) fn scrollbars(&mut self, body: &mut Recti) {
        self.resolve_scrollbars(body);
        self.render_scrollbars(*body);
    }

    /// Updates and paints scrollbars for the active body when scrolling is enabled.
    pub(crate) fn render_active_scrollbars(&mut self) {
        if self.scroll.enabled {
            self.update_scrollbars(self.body);
            self.paint_scrollbars(self.body);
        }
    }

    /// Updates active scrollbar interaction without painting it.
    pub(crate) fn update_active_scrollbars(&mut self) {
        if self.scroll.enabled {
            self.update_scrollbars(self.body);
        }
    }

    /// Paints active scrollbars using interaction recorded earlier in the frame.
    pub(crate) fn paint_active_scrollbars(&mut self) {
        if self.scroll.enabled {
            self.paint_scrollbars(self.body);
        }
    }

    /// Runs scrollbar update and paint passes for `body`.
    pub(crate) fn render_scrollbars(&mut self, body: Recti) {
        self.update_scrollbars(body);
        self.paint_scrollbars(body);
    }

    /// Returns content size including style padding that contributes to scrollbar range.
    fn scrollbar_content_size(&self, padding: i32) -> Dimensioni {
        let mut cs = self.content_size;
        cs.width += padding * 2;
        cs.height += padding * 2;
        cs
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
            ScrollAxis::Vertical => self.scroll.vertical.resolve_layout(self.internal_id_seed, body, content_size, scrollbar_size),
            ScrollAxis::Horizontal => self.scroll.horizontal.resolve_layout(self.internal_id_seed, body, content_size, scrollbar_size),
        }
    }

    /// Updates one scrollbar axis and adjusts the matching scroll offset when dragged.
    fn update_scrollbar(&mut self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32) {
        let Some(layout) = self.scrollbar_layout(axis, body, content_size, scrollbar_size) else {
            self.scroll.set_axis(axis, 0);
            return;
        };

        let scroll_value = self.scroll.axis(axis);
        let mut scrollbar = self.scroll.take_scrollbar(axis);
        scrollbar.configure(layout, scroll_value);
        self.record_tree_layout(
            layout.node_id,
            NodeLayout::new(layout.base, layout.base, Dimensioni::new(layout.base.width, layout.base.height)),
        );
        let (control, result) = self.update_internal_node(layout.node_id, &mut scrollbar, layout.base);
        self.record_tree_interaction(layout.node_id, NodeInteraction::new(control, result));
        self.scroll.set_axis(axis, scrollbar.value());
        self.scroll.restore_scrollbar(scrollbar);
    }

    /// Paints one scrollbar axis using the interaction state recorded during update.
    fn paint_scrollbar(&mut self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32) {
        let Some(layout) = self.scrollbar_layout(axis, body, content_size, scrollbar_size) else {
            return;
        };

        let scroll_value = self.scroll.axis(axis);
        let mut scrollbar = self.scroll.take_scrollbar(axis);
        scrollbar.configure(layout, scroll_value);
        let control = self
            .tree_cache
            .current_interaction(layout.node_id)
            .map(|interaction| interaction.control)
            .unwrap_or_default();
        self.paint_internal_node(layout.node_id, &mut scrollbar, layout.base, &control);
        self.scroll.restore_scrollbar(scrollbar);
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
            self.scroll.offset.y = 0;
        }
        if maxscroll_x > 0 {
            self.update_scrollbar(ScrollAxis::Horizontal, body, cs, scrollbar_size);
        } else {
            self.scroll.offset.x = 0;
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
        self.scroll.enabled = !scroll_behavior.is_no_scroll();
        if self.scroll.enabled {
            self.resolve_scrollbars(&mut body);
        }
        let (layout_padding, style_padding, font, style_clone) = {
            let style = self.style.as_ref();
            (-style.padding, style.padding, style.font, *style)
        };
        let scroll = self.scroll.offset;
        self.layout.reset(expand_rect(body, layout_padding), scroll);
        self.layout.style = style_clone;
        let font_height = self.atlas.get_font_height(font) as i32;
        let vertical_pad = crate::text_layout::vertical_text_padding(style_padding);
        let icon_height = self.atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let default_height = max(font_height + vertical_pad * 2, icon_height);
        self.layout.set_default_cell_height(default_height);
        self.body = body;
    }

    /// Configures layout state for the container's client area, handling scrollbars when necessary.
    #[cfg(test)]
    pub fn push_container_body(&mut self, body: Recti, _opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        self.configure_container_body(body, scroll_behavior);
        self.render_active_scrollbars();
    }
}
