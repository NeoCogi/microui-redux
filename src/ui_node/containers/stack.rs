use crate::sizing::SizePolicy;
use crate::{Dimensioni, Recti, StackDirection};

use super::{Container, LayoutCtx, MeasureCtx, Widget};
use crate::ui_node::{UiNode, UiNodeState};

/// Stack container.
pub(crate) struct Stack {
    /// Width policy applied to emitted items.
    pub(crate) width: SizePolicy,
    /// Height policy applied to emitted items.
    pub(crate) height: SizePolicy,
    /// Stack direction.
    pub(crate) direction: StackDirection,
    /// Child nodes arranged in stack order.
    pub(crate) children: Vec<UiNode>,
}

impl Widget for Stack {
    fn measure(&self, ctx: &MeasureCtx<'_>, _state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        let mut width = 0;
        let mut height: i32 = 0;
        for (index, child) in self.children.iter().enumerate() {
            let child_size = ctx.measure_node_ref(child, available);
            width = width.max(super::super::resolve_size(self.width, child_size.width, available.width, available.width, None));
            height = height.saturating_add(super::super::resolve_size(
                self.height,
                child_size.height,
                available.height,
                available.height,
                None,
            ));
            if index + 1 < self.children.len() {
                height = height.saturating_add(ctx.style.spacing);
            }
        }
        Dimensioni::new(width.max(0), height.max(0))
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _state: &mut UiNodeState, rect: Recti) {
        let count = self.children.len();
        let mut heights = Vec::with_capacity(count);
        for child in &self.children {
            let child_size = ctx.measure_node_ref(child, Dimensioni::new(rect.width, rect.height));
            heights.push(super::super::resolve_size(self.height, child_size.height, rect.height, rect.height, None));
        }
        match self.direction {
            StackDirection::TopToBottom => {
                let mut y = rect.y;
                for (index, child) in self.children.iter_mut().enumerate() {
                    let height = heights.get(index).copied().unwrap_or_default();
                    let width = super::super::resolve_size(self.width, rect.width, rect.width, rect.width, None);
                    ctx.layout_node_ref(child, Recti::new(rect.x, y, width, height));
                    y = y.saturating_add(height).saturating_add(ctx.style.spacing);
                }
            }
            StackDirection::BottomToTop => {
                let mut y = rect.y + rect.height;
                for index in (0..count).rev() {
                    let child = &mut self.children[index];
                    let height = heights.get(index).copied().unwrap_or_default();
                    let width = super::super::resolve_size(self.width, rect.width, rect.width, rect.width, None);
                    y = y.saturating_sub(height);
                    ctx.layout_node_ref(child, Recti::new(rect.x, y, width, height));
                    y = y.saturating_sub(ctx.style.spacing);
                }
            }
        }
    }
}

impl Container for Stack {
    fn children(&self) -> &[UiNode] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<UiNode> {
        &mut self.children
    }
}
