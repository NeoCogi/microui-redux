//! Framework-owned scrollbar widget used by scrollable containers.

use super::*;
use crate::{
    id::IdNamespace,
    scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis},
};

#[derive(Copy, Clone, Debug)]
/// Resolved geometry and retained identity for a container scrollbar in one frame.
pub(crate) struct ScrollbarLayout {
    /// Retained node ID for the internal scrollbar widget.
    pub(crate) node_id: NodeId,
    /// Track rectangle allocated outside the scrollable body.
    pub(crate) base: Recti,
    /// Visible length along this scrollbar's axis.
    pub(crate) view_len: i32,
    /// Content length along this scrollbar's axis.
    pub(crate) content_len: i32,
}

/// Persistent state for one framework-owned scrollbar.
#[derive(Clone)]
pub(crate) struct Scrollbar {
    axis: ScrollAxis,
    value: i32,
    max_value: i32,
    view_len: i32,
    content_len: i32,
    config: WidgetConfig,
}

impl Scrollbar {
    /// Creates a scrollbar for the selected axis.
    pub(crate) fn new(axis: ScrollAxis) -> Self {
        Self {
            axis,
            value: 0,
            max_value: 0,
            view_len: 0,
            content_len: 0,
            config: WidgetConfig::default(),
        }
    }

    /// Returns the axis controlled by this scrollbar.
    pub(crate) fn axis(&self) -> ScrollAxis {
        self.axis
    }

    /// Derives a stable retained node id in the owning container scope.
    pub(crate) fn node_id(&self, scope_seed: Id) -> NodeId {
        let part = match self.axis {
            ScrollAxis::Vertical => 1,
            ScrollAxis::Horizontal => 2,
        };
        IdNamespace::INTERNAL_CONTROL.id([scope_seed.raw() as u64, part])
    }

    /// Resolves the scrollbar's track, identity, and range for a container body.
    pub(crate) fn resolve_layout(&self, scope_seed: Id, body: Recti, content_size: Dimensioni, scrollbar_size: i32) -> Option<ScrollbarLayout> {
        let (view_len, content_len) = match self.axis {
            ScrollAxis::Vertical => (body.height, content_size.height),
            ScrollAxis::Horizontal => (body.width, content_size.width),
        };
        if scrollbar_max_scroll(content_len, view_len) <= 0 || view_len <= 0 {
            return None;
        }

        Some(ScrollbarLayout {
            node_id: self.node_id(scope_seed),
            base: scrollbar_base(self.axis, body, scrollbar_size),
            view_len,
            content_len,
        })
    }

    /// Updates the frame-local range and clamps the current scroll value.
    pub(crate) fn configure(&mut self, layout: ScrollbarLayout, value: i32) {
        self.view_len = layout.view_len;
        self.content_len = layout.content_len;
        self.max_value = scrollbar_max_scroll(layout.content_len, layout.view_len);
        self.value = value.clamp(0, self.max_value);
    }

    /// Returns the current scroll value after the latest update.
    pub(crate) fn value(&self) -> i32 {
        self.value
    }
}

impl Widget for Scrollbar {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.config.scroll_behavior
    }

    fn measure(&self, style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let size = style.scrollbar_size.max(0);
        Dimensioni::new(size, size)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        if control.active {
            let delta = ctx.input_or_default().mouse_delta;
            self.value += scrollbar_drag_delta(self.axis, delta, self.content_len, ctx.screen_rect());
            self.value = self.value.clamp(0, self.max_value);
        }
        ResourceState::NONE
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, _control: &ControlState) {
        let base = ctx.screen_rect();
        ctx.draw_frame(base, ControlColor::ScrollBase);
        let thumb = scrollbar_thumb(self.axis, base, self.view_len, self.content_len, self.value, ctx.style().thumb_size);
        ctx.draw_frame(thumb, ControlColor::ScrollThumb);
    }

    fn needs_input_snapshot(&self) -> bool {
        true
    }
}
