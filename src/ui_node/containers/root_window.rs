use crate::input::{ContainerOption, ScrollBehavior};
use crate::scrollbar::ScrollAxis;
use crate::{Dimensioni, Recti, Vec2i};

use super::{Column, ContainerTrait, LayoutCtx, MeasureCtx};
use crate::ui_node::UiNodeId;

/// Top-level window root container.
#[derive(Clone)]
pub(crate) struct RootWindow {
    /// Body content layout.
    pub(crate) body: Column,
    /// Root chrome/sizing options.
    pub(crate) opt: ContainerOption,
    /// Scroll behavior applied to the root body.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Current body scroll offset.
    pub(crate) scroll_offset: Vec2i,
    /// Active body scrollbar drag axis.
    pub(crate) scroll_drag: Option<ScrollAxis>,
    /// Root z-order value.
    pub(crate) z_index: i32,
}

impl Default for RootWindow {
    fn default() -> Self {
        Self {
            body: Column,
            opt: ContainerOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            scroll_offset: Vec2i::default(),
            scroll_drag: None,
            z_index: 0,
        }
    }
}

impl ContainerTrait for RootWindow {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        self.body.measure(ctx, id, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let scroll = ctx.root_window_scroll_offset();
        let scrolled_rect = Recti::new(rect.x.saturating_sub(scroll.x), rect.y.saturating_sub(scroll.y), rect.width, rect.height);
        self.body.layout(ctx, id, scrolled_rect, clip);

        let content_size = ctx
            .child_content_bounds(id)
            .map(|bounds| {
                Dimensioni::new(
                    (bounds.x + scroll.x + bounds.width - rect.x).max(0),
                    (bounds.y + scroll.y + bounds.height - rect.y).max(0),
                )
            })
            .unwrap_or_default();
        ctx.set_content_size(id, content_size);
    }

    fn is_root_window(&self) -> bool {
        true
    }

    fn root_scroll_state(&self) -> Option<(Vec2i, Option<ScrollAxis>)> {
        Some((self.scroll_offset, self.scroll_drag))
    }

    fn configure_root_scroll(&mut self, scroll_behavior: ScrollBehavior) {
        self.scroll_behavior = scroll_behavior;
    }

    fn set_root_scroll_state(&mut self, offset: Vec2i, drag: Option<ScrollAxis>) {
        self.scroll_offset = offset;
        self.scroll_drag = drag;
    }
}
