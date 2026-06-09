use crate::{Dimensioni, Node, Recti, WidgetHandle};

use super::{Column, Container, LayoutCtx, MeasureCtx, PaintCtx, UpdateCtx, Widget};
use crate::ui_node::UiNode;

/// Header/tree disclosure container.
pub(crate) struct Disclosure {
    /// Widget state for the disclosure row.
    pub(crate) state: WidgetHandle<Node>,
    /// Whether child layout should be indented when expanded.
    pub(crate) indent_children: bool,
    /// Disclosure-row rectangle in parent content coordinates.
    pub(crate) header_rect: Recti,
    /// Child content layout used when expanded.
    pub(crate) content_layout: Column,
}

impl Widget for Disclosure {
    fn measure(&self, ctx: &MeasureCtx<'_>, node: &UiNode, available: Dimensioni) -> Dimensioni {
        let widget = crate::window_manager::erased_widget_state(self.state.clone());
        let header_size = widget.measure(ctx.style, ctx.atlas, available);
        if !self.state.read(|state| state.state).is_expanded() {
            return Dimensioni::new(available.width.max(header_size.width), header_size.height);
        }

        let child_available = Dimensioni::new(
            available
                .width
                .saturating_sub(super::super::disclosure_child_indent(self.indent_children, ctx.style)),
            available.height.saturating_sub(header_size.height),
        );
        let child_size = self.content_layout.measure(ctx, node, child_available);
        Dimensioni::new(
            available
                .width
                .max(header_size.width)
                .max(child_size.width + super::super::disclosure_child_indent(self.indent_children, ctx.style)),
            header_size.height.saturating_add(ctx.style.spacing).saturating_add(child_size.height),
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, node: &mut UiNode, rect: Recti) {
        let widget = crate::window_manager::erased_widget_state(self.state.clone());
        let header_preferred = widget.measure(ctx.style, ctx.atlas, Dimensioni::new(rect.width, rect.height));
        let header_height = header_preferred
            .height
            .max(super::super::default_cell_height(ctx.style, ctx.atlas))
            .min(rect.height.max(0));
        let header_rect = Recti::new(rect.x, rect.y, rect.width, header_height);
        self.header_rect = header_rect;

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
        self.content_layout.layout(ctx, node, child_rect);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, node: &mut UiNode) -> bool {
        ctx.update_container_widget_in_rect(node, self.header_rect, self.state.clone(), "ui node disclosure");
        self.state.read(|state| state.state).is_expanded()
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, node: &mut UiNode) -> bool {
        ctx.paint_container_widget_in_rect(node, self.header_rect, self.state.clone());
        self.state.read(|state| state.state).is_expanded()
    }
}

impl Container for Disclosure {
    fn children(&self) -> &[UiNode] {
        &self.content_layout.children
    }

    fn children_mut(&mut self) -> &mut Vec<UiNode> {
        &mut self.content_layout.children
    }
}
