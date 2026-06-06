use crate::sizing::SizePolicy;
use crate::{Dimensioni, Recti};

use super::{NodeBehavior, LayoutCtx, MeasureCtx};
use crate::ui_node::UiNodeId;

/// Row container.
#[derive(Clone)]
pub(crate) struct Row {
    /// Width policies for row tracks.
    pub(crate) widths: Vec<SizePolicy>,
    /// Shared row height policy.
    pub(crate) height: SizePolicy,
}

impl NodeBehavior for Row {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        let mut preferred_heights = Vec::new();
        let mut preferred_widths = Vec::new();
        for index in 0..ctx.child_count(id) {
            let Some(child) = ctx.child_at(id, index) else { continue };
            let child_size = ctx.measure_node(child, available);
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
        let spacing = ctx.style.spacing.saturating_mul(ctx.child_count(id).saturating_sub(1) as i32);
        let width = preferred_widths.into_iter().sum::<i32>().saturating_add(spacing).max(0);
        Dimensioni::new(width, height)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let count = ctx.child_count(id);
        let available_width = rect.width.saturating_sub(ctx.style.spacing.saturating_mul(count.saturating_sub(1) as i32));
        let mut preferred = Vec::with_capacity(count);
        let mut policies = Vec::with_capacity(count);
        for index in 0..count {
            let Some(child) = ctx.child_at(id, index) else { continue };
            let child_size = ctx.measure_node(child, Dimensioni::new(available_width, rect.height));
            preferred.push(child_size.width);
            policies.push(ctx.horizontal_track_policy(child, self.widths.get(index).copied().unwrap_or(SizePolicy::Auto)));
        }
        let child_widths = super::super::resolve_axis_tracks(&policies, &preferred, available_width);
        let height = super::super::resolve_size(self.height, rect.height, rect.height, rect.height, None);
        let mut x = rect.x;
        for index in 0..count {
            let Some(child) = ctx.child_at(id, index) else { continue };
            let width = child_widths.get(index).copied().unwrap_or_default();
            let child_rect = Recti::new(x, rect.y, width, height);
            ctx.layout_node(child, child_rect, clip);
            x = x.saturating_add(width).saturating_add(ctx.style.spacing);
        }
    }

    fn vertical_child_policy(&self) -> Option<SizePolicy> {
        Some(self.height)
    }
}
