use crate::sizing::SizePolicy;
use crate::{Dimensioni, Recti};

use super::{Container, LayoutCtx, MeasureCtx, NodeBehavior};
use crate::ui_node::{UiNode, UiNodeState};

/// Row container.
pub(crate) struct Row {
    /// Width policies for row tracks.
    pub(crate) widths: Vec<SizePolicy>,
    /// Shared row height policy.
    pub(crate) height: SizePolicy,
    /// Child nodes arranged left-to-right.
    pub(crate) children: Vec<UiNode>,
}

impl NodeBehavior for Row {
    fn measure(&self, ctx: &MeasureCtx<'_>, _state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        let mut preferred_heights = Vec::new();
        let mut preferred_widths = Vec::new();
        for child in &self.children {
            let child_size = ctx.measure_node_ref(child, available);
            preferred_widths.push(child_size.width);
            preferred_heights.push(child_size.height);
        }
        let height = super::super::resolve_size(
            self.height,
            preferred_heights
                .iter()
                .copied()
                .max()
                .unwrap_or_else(|| super::super::default_cell_height(ctx.style, ctx.atlas)),
            available.height,
            available.height,
            None,
        );
        let spacing = ctx.style.spacing.saturating_mul(self.children.len().saturating_sub(1) as i32);
        let width = preferred_widths.into_iter().sum::<i32>().saturating_add(spacing).max(0);
        Dimensioni::new(width, height)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _state: &mut UiNodeState, rect: Recti) {
        let count = self.children.len();
        let available_width = rect.width.saturating_sub(ctx.style.spacing.saturating_mul(count.saturating_sub(1) as i32));
        let mut preferred = Vec::with_capacity(count);
        let mut policies = Vec::with_capacity(count);
        for (index, child) in self.children.iter().enumerate() {
            let child_size = ctx.measure_node_ref(child, Dimensioni::new(available_width, rect.height));
            preferred.push(child_size.width);
            let child_policy = child.state.policy.width;
            let track_policy = self.widths.get(index).copied().unwrap_or(SizePolicy::Auto);
            policies.push(if child_policy != SizePolicy::Auto { child_policy } else { track_policy });
        }
        let child_widths = super::super::resolve_axis_tracks(&policies, &preferred, available_width);
        let height = super::super::resolve_size(self.height, rect.height, rect.height, rect.height, None);
        let mut x = rect.x;
        for (index, child) in self.children.iter_mut().enumerate() {
            let width = child_widths.get(index).copied().unwrap_or_default();
            let child_rect = Recti::new(x, rect.y, width, height);
            ctx.layout_node_ref(child, child_rect);
            x = x.saturating_add(width).saturating_add(ctx.style.spacing);
        }
    }
}

impl Container for Row {
    fn children(&self) -> &[UiNode] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<UiNode> {
        &mut self.children
    }
}
