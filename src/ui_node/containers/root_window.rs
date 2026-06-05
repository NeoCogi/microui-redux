use crate::input::ContainerOption;
use crate::{Dimensioni, Recti};

use super::{Column, ContainerTrait, LayoutCtx, MeasureCtx};
use crate::ui_node::UiNodeId;

/// Top-level window root container.
#[derive(Clone)]
pub(crate) struct RootWindow {
    /// Body content layout.
    pub(crate) body: Column,
    /// Root chrome/sizing options.
    pub(crate) opt: ContainerOption,
    /// Root z-order value.
    pub(crate) z_index: i32,
}

impl Default for RootWindow {
    fn default() -> Self {
        Self {
            body: Column,
            opt: ContainerOption::NONE,
            z_index: 0,
        }
    }
}

impl ContainerTrait for RootWindow {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        self.body.measure(ctx, id, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        self.body.layout(ctx, id, rect, clip);

        let content_size = ctx
            .child_content_bounds(id)
            .map(|bounds| Dimensioni::new((bounds.x + bounds.width - rect.x).max(0), (bounds.y + bounds.height - rect.y).max(0)))
            .unwrap_or_default();
        ctx.set_content_size(id, content_size);
    }

    fn is_root_window(&self) -> bool {
        true
    }
}
