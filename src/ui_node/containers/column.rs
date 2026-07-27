use crate::{Dimensioni, Recti};

use super::{Container, LayoutCtx, MeasureCtx, NodeBehavior};
use crate::ui_node::{UiNode, UiNodeState};

/// Column container.
#[derive(Default)]
pub(crate) struct Column {
    /// Child nodes arranged top-to-bottom.
    pub(crate) children: Vec<UiNode>,
}

impl NodeBehavior for Column {
    fn measure(&self, ctx: &MeasureCtx<'_>, _state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        measure_column(ctx, &self.children, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _state: &mut UiNodeState, rect: Recti) {
        layout_column_children(ctx, &mut self.children, rect);
    }
}

impl Container for Column {
    fn children(&self) -> &[UiNode] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<UiNode> {
        &mut self.children
    }
}

fn measure_column(ctx: &MeasureCtx<'_>, children: &[UiNode], available: Dimensioni) -> Dimensioni {
    let mut width = 0;
    let mut height: i32 = 0;
    for (index, child) in children.iter().enumerate() {
        let child_size = ctx.measure_node_ref(child, available);
        width = width.max(child_size.width);
        height = height.saturating_add(child_size.height);
        if index + 1 < children.len() {
            height = height.saturating_add(ctx.style.spacing);
        }
    }
    Dimensioni::new(width.max(0), height.max(0))
}

fn layout_column_children(ctx: &mut LayoutCtx<'_>, children: &mut [UiNode], rect: Recti) {
    let count = children.len();
    let available_height = rect.height.saturating_sub(ctx.style.spacing.saturating_mul(count.saturating_sub(1) as i32));
    let mut preferred = Vec::with_capacity(count);
    let mut policies = Vec::with_capacity(count);
    for child in children.iter() {
        let child_size = ctx.measure_node_ref(child, Dimensioni::new(rect.width, available_height));
        preferred.push(child_size.height);
        policies.push(child.state.policy.height);
    }
    let heights = super::super::resolve_axis_tracks(&policies, &preferred, available_height);
    let mut y = rect.y;
    for (index, child) in children.iter_mut().enumerate() {
        let height = heights.get(index).copied().unwrap_or_default();
        let child_rect = Recti::new(rect.x, y, rect.width, height);
        ctx.layout_node_ref(child, child_rect);
        y = y.saturating_add(height).saturating_add(ctx.style.spacing);
    }
}
