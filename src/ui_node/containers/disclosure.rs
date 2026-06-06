use crate::{Dimensioni, Node, Recti, WidgetHandle};

use super::{Column, NodeBehavior, LayoutCtx, MeasureCtx, PaintCtx, UpdateCtx};
use crate::ui_node::UiNodeId;

/// Header/tree disclosure container.
#[derive(Clone)]
pub(crate) struct Disclosure {
    /// Widget state for the disclosure row.
    pub(crate) state: WidgetHandle<Node>,
    /// Whether child layout should be indented when expanded.
    pub(crate) indent_children: bool,
    /// Child content layout used when expanded.
    pub(crate) children: Column,
}

impl NodeBehavior for Disclosure {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        let widget = crate::context::erased_widget_state(self.state.clone());
        let header_size = widget.measure(ctx.style, ctx.atlas, available);
        if !self.state.read(|state| state.state).is_expanded() {
            return Dimensioni::new(available.width.max(header_size.width), header_size.height);
        }

        let child_available = Dimensioni::new(
            available.width.saturating_sub(super::super::disclosure_child_indent(self.indent_children, ctx.style)),
            available.height.saturating_sub(header_size.height),
        );
        let child_size = self.children.measure(ctx, id, child_available);
        Dimensioni::new(
            available
                .width
                .max(header_size.width)
                .max(child_size.width + super::super::disclosure_child_indent(self.indent_children, ctx.style)),
            header_size.height.saturating_add(ctx.style.spacing).saturating_add(child_size.height),
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let widget = crate::context::erased_widget_state(self.state.clone());
        let header_preferred = widget.measure(ctx.style, ctx.atlas, Dimensioni::new(rect.width, rect.height));
        let header_height = header_preferred
            .height
            .max(super::super::default_cell_height(ctx.style, ctx.atlas))
            .min(rect.height.max(0));
        let header_rect = Recti::new(rect.x, rect.y, rect.width, header_height);
        ctx.set_client(id, header_rect);

        if !self.state.read(|state| state.state).is_expanded() {
            return;
        }

        let indent = super::super::disclosure_child_indent(self.indent_children, ctx.style);
        let child_rect = Recti::new(
            rect.x + indent,
            rect.y + header_height + ctx.style.spacing,
            rect.width.saturating_sub(indent),
            rect.height.saturating_sub(header_height).saturating_sub(ctx.style.spacing),
        );
        self.children.layout(ctx, id, child_rect, clip);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, id: UiNodeId) -> bool {
        ctx.update_container_widget(id, self.state.clone(), "ui node disclosure");
        self.state.read(|state| state.state).is_expanded()
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, id: UiNodeId) -> bool {
        ctx.paint_container_widget(id, self.state.clone());
        self.state.read(|state| state.state).is_expanded()
    }
}
