//! Parent-first paint traversal and custom-render dispatch.

use super::*;

impl UiRuntime {
    /// Paints one already-borrowed node and descendants.
    pub(super) fn paint_node_ref(
        &mut self,
        node: &mut Node,
        parent_transform: Transform,
        display_list: &mut DisplayList,
        style: &Style,
        atlas: crate::AtlasHandle,
    ) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.paints += 1);
        // Resolve the same outer/content geometry used for input and update before recording paint.
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let frame_geometry = crate::ui_node::frame::frame_geometry(local_rect, framed, style);
        let content_rect = frame_geometry.content_or_empty();
        let screen_clip = parent_transform.clip.intersect(&screen_rect).unwrap_or_default();
        if framed {
            // Generic framing belongs beneath the widget's own paint and descendant paint.
            let mut painter = crate::render::Painter::screen_space(display_list, screen_clip);
            crate::ui_node::frame::paint_internal_frame(&mut painter, screen_rect, None, style.frame_border());
        }
        let child_transform = parent_transform.push(node.state.layout);
        let local_clip = rect_relative_to(screen_clip, screen_origin);
        let content_clip = local_clip
            .intersect(&content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));
        let screen_content_rect = translate_local_rect(content_rect, screen_origin);
        let screen_content_clip = translate_local_rect(content_clip, screen_origin);
        {
            // Limit the mutable display-list borrow to this widget call before custom/child output.
            let mut widget_ctx = crate::WidgetPaintCtx::new_with_content_geometry(
                screen_content_rect,
                display_list,
                screen_content_clip,
                style,
                &atlas,
                node.state.hovered,
                node.state.focused,
                node.state.clicked,
                node.state.active,
            );
            node.data.widget_mut().paint(&mut widget_ctx);
        }

        if let NodeKind::Widget(widget) = &node.data
            && let Some(renderer) = widget.custom_render()
        {
            // Custom backend work is an ordered barrier immediately after ordinary widget paint.
            display_list.push_custom(screen_content_clip, renderer, screen_content_rect);
        }

        let traverse_children = node.is_container();
        if traverse_children {
            // Children paint after their ordinary parent surface, matching reverse-order hit tests.
            node.with_children_mut(|children| {
                for child in children.iter_mut().filter(|child| node_is_visible(child)) {
                    self.paint_node_ref(child, child_transform, display_list, style, atlas.clone());
                }
            });
        }
    }
}
