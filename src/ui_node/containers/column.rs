use crate::{Dimensioni, Recti};

use super::{ContainerTrait, LayoutCtx, MeasureCtx};
use crate::ui_node::UiNodeId;

/// Column container.
#[derive(Clone, Default)]
pub(crate) struct Column;

impl ContainerTrait for Column {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        measure_column(ctx, id, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        layout_column_children(ctx, id, rect, clip);
    }
}

fn measure_column(ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
    let mut width = 0;
    let mut height: i32 = 0;
    for index in 0..ctx.child_count(id) {
        let Some(child) = ctx.child_at(id, index) else { continue };
        let child_size = ctx.measure_node(child, available);
        width = width.max(child_size.width);
        height = height.saturating_add(child_size.height);
        if index + 1 < ctx.child_count(id) {
            height = height.saturating_add(ctx.style.spacing);
        }
    }
    Dimensioni::new(width.max(0), height.max(0))
}

fn layout_column_children(ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
    let count = ctx.child_count(id);
    let available_height = rect.height.saturating_sub(ctx.style.spacing.saturating_mul(count.saturating_sub(1) as i32));
    let mut preferred = Vec::with_capacity(count);
    let mut policies = Vec::with_capacity(count);
    for index in 0..count {
        let Some(child) = ctx.child_at(id, index) else { continue };
        let child_size = ctx.measure_node(child, Dimensioni::new(rect.width, available_height));
        preferred.push(child_size.height);
        policies.push(ctx.vertical_child_policy(child));
    }
    let heights = super::super::resolve_axis_tracks(&policies, &preferred, available_height);
    let mut y = rect.y;
    for index in 0..count {
        let Some(child) = ctx.child_at(id, index) else { continue };
        let height = heights.get(index).copied().unwrap_or_default();
        let child_rect = Recti::new(rect.x, y, rect.width, height);
        ctx.layout_node(child, child_rect, clip);
        y = y.saturating_add(height).saturating_add(ctx.style.spacing);
    }
}
