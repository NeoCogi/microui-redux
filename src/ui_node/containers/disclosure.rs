use crate::{Dimensioni, Node, Recti, WidgetHandle};

use super::{Column, Container, InputCtx, InputResult, LayoutCtx, MeasureCtx, NodeBehavior, PaintCtx, UiInputEvent, UpdateCtx};
use crate::ui_node::{UiNode, UiNodeState};

/// Header/tree disclosure container.
pub(crate) struct Disclosure {
    /// Widget state for the disclosure row.
    pub(crate) state: WidgetHandle<Node>,
    /// Whether child layout should be indented when expanded.
    pub(crate) indent_children: bool,
    /// Disclosure-row rectangle in this node's local coordinates.
    pub(crate) header_rect: Recti,
    /// Child content layout used when expanded.
    pub(crate) content_layout: Column,
}

impl NodeBehavior for Disclosure {
    fn measure(&self, ctx: &MeasureCtx<'_>, state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        let widget = crate::window_manager::erased_widget_state(self.state.clone());
        let framed = widget.effective_widget_opt().intersects(crate::WidgetOption::FRAME);
        let border_width = if framed { ctx.style.frame_border().width } else { 0 };
        let header_size = crate::frame::outer_preferred(
            widget.measure(ctx.style, ctx.atlas, crate::frame::content_available(available, border_width)),
            border_width,
        );
        if !self.state.read(|state| state.state).is_expanded() {
            return Dimensioni::new(available.width.max(header_size.width), header_size.height);
        }

        let child_available = Dimensioni::new(
            available
                .width
                .saturating_sub(super::super::disclosure_child_indent(self.indent_children, ctx.style)),
            available.height.saturating_sub(header_size.height),
        );
        let child_size = self.content_layout.measure(ctx, state, child_available);
        Dimensioni::new(
            available
                .width
                .max(header_size.width)
                .max(child_size.width + super::super::disclosure_child_indent(self.indent_children, ctx.style)),
            header_size.height.saturating_add(ctx.style.spacing).saturating_add(child_size.height),
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        let widget = crate::window_manager::erased_widget_state(self.state.clone());
        let framed = widget.effective_widget_opt().intersects(crate::WidgetOption::FRAME);
        let border_width = if framed { ctx.style.frame_border().width } else { 0 };
        let header_preferred = crate::frame::outer_preferred(
            widget.measure(
                ctx.style,
                ctx.atlas,
                crate::frame::content_available(Dimensioni::new(rect.width, rect.height), border_width),
            ),
            border_width,
        );
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
        self.content_layout.layout(ctx, state, child_rect);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, state: &mut UiNodeState) -> bool {
        ctx.update_container_widget_in_rect(state, self.header_rect, self.state.clone(), "ui node disclosure");
        self.state.read(|state| state.state).is_expanded()
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, state: &mut UiNodeState) -> bool {
        ctx.paint_container_widget_in_rect(state, self.header_rect, self.state.clone());
        self.state.read(|state| state.state).is_expanded()
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        let widget = crate::window_manager::erased_widget_state(self.state.clone());
        ctx.route_widget_input(
            state,
            self.header_rect,
            widget.effective_widget_opt(),
            widget.effective_scroll_behavior(),
            event,
        )
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
