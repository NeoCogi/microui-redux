use crate::sizing::SizePolicy;
use crate::{Dimensioni, Recti, StackDirection};

use super::{NodeBehavior, LayoutCtx, MeasureCtx};
use crate::ui_node::UiNodeId;

/// Stack container.
#[derive(Clone)]
pub(crate) struct Stack {
    /// Width policy applied to emitted items.
    pub(crate) width: SizePolicy,
    /// Height policy applied to emitted items.
    pub(crate) height: SizePolicy,
    /// Stack direction.
    pub(crate) direction: StackDirection,
}

impl NodeBehavior for Stack {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        let mut width = 0;
        let mut height: i32 = 0;
        for index in 0..ctx.child_count(id) {
            let Some(child) = ctx.child_at(id, index) else { continue };
            let child_size = ctx.measure_node(child, available);
            width = width.max(super::super::resolve_size(self.width, child_size.width, available.width, available.width, None));
            height = height.saturating_add(super::super::resolve_size(
                self.height,
                child_size.height,
                available.height,
                available.height,
                None,
            ));
            if index + 1 < ctx.child_count(id) {
                height = height.saturating_add(ctx.style.spacing);
            }
        }
        Dimensioni::new(width.max(0), height.max(0))
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let count = ctx.child_count(id);
        let mut heights = Vec::with_capacity(count);
        for index in 0..count {
            let Some(child) = ctx.child_at(id, index) else { continue };
            let child_size = ctx.measure_node(child, Dimensioni::new(rect.width, rect.height));
            heights.push(super::super::resolve_size(self.height, child_size.height, rect.height, rect.height, None));
        }
        match self.direction {
            StackDirection::TopToBottom => {
                let mut y = rect.y;
                for index in 0..count {
                    let Some(child) = ctx.child_at(id, index) else { continue };
                    let height = heights.get(index).copied().unwrap_or_default();
                    let width = super::super::resolve_size(self.width, rect.width, rect.width, rect.width, None);
                    ctx.layout_node(child, Recti::new(rect.x, y, width, height), clip);
                    y = y.saturating_add(height).saturating_add(ctx.style.spacing);
                }
            }
            StackDirection::BottomToTop => {
                let mut y = rect.y + rect.height;
                for index in (0..count).rev() {
                    let Some(child) = ctx.child_at(id, index) else { continue };
                    let height = heights.get(index).copied().unwrap_or_default();
                    let width = super::super::resolve_size(self.width, rect.width, rect.width, rect.width, None);
                    y = y.saturating_sub(height);
                    ctx.layout_node(child, Recti::new(rect.x, y, width, height), clip);
                    y = y.saturating_sub(ctx.style.spacing);
                }
            }
        }
    }
}
