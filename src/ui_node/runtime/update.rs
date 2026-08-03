//! Parent-first widget update traversal and interaction snapshots.

use super::*;

impl UiRuntime {
    /// Updates one already-borrowed node and descendants.
    pub(super) fn update_node_ref(&mut self, node: &mut Node, parent_transform: Transform, style: &Style, atlas: crate::AtlasHandle, input: InputSnapshot) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.updates += 1);
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let frame_geometry = crate::ui_node::frame::frame_geometry(local_rect, framed, style);
        let content_rect = frame_geometry.content_or_empty();
        let screen_clip = parent_transform.clip.intersect(&screen_rect).unwrap_or_default();
        let child_transform = parent_transform.push(node.state.layout);
        let local_clip = rect_relative_to(screen_clip, screen_origin);
        let content_clip = local_clip
            .intersect(&content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));

        let (opt, focus_policy) = node_interaction_config(node);
        let id = node.id();
        let (hovered, focused, clicked, active) = self.commit_interaction_snapshot(id, screen_rect, screen_clip, node.state.hovered, input, opt, focus_policy);
        node.state.hovered = hovered;
        node.state.focused = focused;
        node.state.clicked = clicked;
        node.state.active = active;

        let event = self
            .take_routed_event(id)
            .map(|event| super::widget_context::localize_event(Vec2i::new(content_rect.x, content_rect.y), event));
        let accepts_pointer_input = self.accepts_pointer_input();
        let screen_content_rect = translate_local_rect(content_rect, screen_origin);
        let screen_content_clip = translate_local_rect(content_clip, screen_origin);
        let mut widget_ctx = crate::WidgetUpdateCtx::new_with_content_geometry(
            screen_content_rect,
            screen_content_clip,
            style,
            &atlas,
            accepts_pointer_input,
            node.state.hovered,
            node.state.focused,
            node.state.clicked,
            node.state.active,
            input.mouse_buttons,
            input.key_modes,
            input.key_codes,
        );
        node.data.widget_mut().update(&mut widget_ctx, event.as_ref());
        self.finish_pointer_capture_update(node);
        let traverse_children = node.data.container().is_some_and(Container::children_visible);
        if traverse_children {
            node.with_children_mut(|children| {
                for child in children.iter_mut() {
                    // Forward order is observable by deliberate cross-cell mutation: a later child
                    // sees successful earlier changes, while an already-updated child is not rerun.
                    // The mandatory post-event layout observes the final state/topology. Rendering
                    // enters the tree later through the distinct paint traversal below.
                    self.update_node_ref(child, child_transform, style, atlas.clone(), input);
                }
            });
        }
    }

    /// Computes interaction state from node geometry and shared input.
    fn commit_interaction_snapshot(
        &mut self,
        id: RuntimeNodeId,
        rect: Recti,
        clip: Recti,
        prior_hovered: bool,
        input: InputSnapshot,
        opt: WidgetOption,
        focus_policy: FocusPolicy,
    ) -> (bool, bool, bool, bool) {
        if opt.intersects(WidgetOption::NO_INTERACT) {
            return (false, false, false, false);
        }

        let hovered = if self.pointer_event_active {
            self.pointer_input_enabled && rect.contains(&input.mouse_pos) && clip.contains(&input.mouse_pos)
        } else {
            prior_hovered
        };
        if hovered {
            self.hover = Some(id);
        }

        if self.focus == Some(id) {
            let released_without_hold_focus = self.pointer_release_active && input.mouse_buttons.is_empty() && focus_policy.releases_on_mouse_up();
            if released_without_hold_focus {
                self.focus = None;
            }
        }

        let focused = self.focus == Some(id);
        let active = focused && input.mouse_buttons.intersects(MouseButton::LEFT);
        let clicked = self.clicked == Some(id);
        (hovered, focused, clicked, active)
    }
}
