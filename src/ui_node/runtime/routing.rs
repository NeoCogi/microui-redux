//! Routed input traversal and transient focus, hover, and capture lifecycle.

use super::*;

impl UiRuntime {
    /// Clears interaction state that points at a removed or container-gated descendant.
    ///
    /// Sanitization runs at layout/update boundaries and before direct target delivery, when no
    /// container state borrow is active. A replacement node cannot inherit a stale target because
    /// every owning node has a fresh ID.
    pub(super) fn sanitize_transient_targets(&mut self, roots: &mut [Node]) {
        self.focus = self.focus.filter(|id| contains_active_node_in(roots, *id));
        self.hover = self.hover.filter(|id| contains_active_node_in(roots, *id));
        if self.routed_event.as_ref().is_some_and(|(id, _)| !contains_active_node_in(roots, *id)) {
            self.routed_event = None;
        }

        let capture_valid = self
            .capture
            .is_none_or(|id| contains_active_node_in(roots, id) && captured_target_retains_pointer_capture(roots, id));
        if !capture_valid {
            self.clear_current_pointer_capture(roots);
        }

        debug_assert!(self.focus.is_none_or(|id| contains_active_node_in(roots, id)));
        debug_assert!(self.hover.is_none_or(|id| contains_active_node_in(roots, id)));
        debug_assert!(self.routed_event.as_ref().is_none_or(|(id, _)| contains_active_node_in(roots, *id)));
        debug_assert!(self.capture.is_none() || capture_valid);
    }

    /// Ends current capture immediately and notifies the still-retained target directly.
    fn clear_current_pointer_capture(&mut self, roots: &mut [Node]) {
        let Some(owner) = self.capture.take() else {
            return;
        };
        if self.capture_loss_after_update == Some(owner) {
            self.capture_loss_after_update = None;
        }
        self.discard_invalidated_capture_events = true;
        notify_pointer_capture_lost(roots, owner);
    }

    /// Clears current and deferred capture state for an explicitly ineligible retained tree.
    pub(super) fn clear_all_pointer_capture(&mut self, roots: &mut [Node]) {
        let pending_loss = self.capture_loss_after_update.take();
        if let Some(owner) = self.capture.take() {
            self.discard_invalidated_capture_events = true;
            notify_pointer_capture_lost(roots, owner);
        }
        if let Some(owner) = pending_loss {
            notify_pointer_capture_lost(roots, owner);
        }
    }

    /// Clears tree ownership now while deferring local cleanup until this event's update has run.
    fn defer_current_pointer_capture_loss(&mut self) {
        let Some(owner) = self.capture.take() else {
            return;
        };
        debug_assert!(self.capture_loss_after_update.is_none() || self.capture_loss_after_update == Some(owner));
        self.capture_loss_after_update = Some(owner);
    }

    /// Acquires capture for one routed owner in the current event transaction.
    fn acquire_pointer_capture(&mut self, owner: RuntimeNodeId) {
        if self.capture == Some(owner) {
            return;
        }
        if let Some(previous) = self.capture.take()
            && previous != owner
        {
            self.capture_loss_after_update = Some(previous);
        }
        self.capture = Some(owner);
        self.discard_invalidated_capture_events = false;
    }

    /// Completes capture lifecycle work after one target consumes its routed event.
    pub(super) fn finish_pointer_capture_update(&mut self, node: &mut Node) {
        let id = node.id();
        if self.capture_loss_after_update == Some(id)
            && self.capture != Some(id)
            && let Some(container) = node.data.container_mut()
        {
            self.capture_loss_after_update = None;
            container.on_pointer_capture_lost();
        }
    }

    /// Delivers losses for targets skipped by update because an ancestor closed its gate.
    pub(super) fn flush_capture_loss(&mut self, roots: &mut [Node]) {
        if let Some(owner) = self.capture_loss_after_update.take()
            && self.capture != Some(owner)
        {
            notify_pointer_capture_lost(roots, owner);
        }
    }

    /// Routes one keyboard/text event to the focused node.
    pub(crate) fn route_focus_input_event(&mut self, roots: &mut [Node], style: &Style, event: &UiInputEvent) -> bool {
        self.sanitize_transient_targets(roots);
        self.route_focus_input_event_to_target(roots, style, event)
    }

    /// Routes one pointer event to the capturing node, if there is one.
    pub(crate) fn route_captured_pointer_input_event(
        &mut self,
        roots: &mut [Node],
        style: &Style,
        mouse_buttons: MouseButton,
        event: &UiInputEvent,
    ) -> Option<bool> {
        self.sanitize_transient_targets(roots);

        if self.capture.is_none() && self.discard_invalidated_capture_events {
            match event {
                UiInputEvent::MouseDrag { .. } => return Some(false),
                UiInputEvent::MouseUp { .. } => {
                    self.discard_invalidated_capture_events = false;
                    return Some(false);
                }
                UiInputEvent::MouseDown { .. } => {
                    self.discard_invalidated_capture_events = false;
                }
                UiInputEvent::MouseMove { .. } | UiInputEvent::Scroll { .. } => {}
                _ => unreachable!("only pointer events are routed through pointer capture"),
            }
        }

        let capture = self.capture?;
        let result = self.route_input_event_to_target(roots, capture, style, event);
        self.update_pointer_capture(capture, result, event, mouse_buttons);
        Some(result.is_consumed())
    }

    /// Routes keyboard/text input to the focused node only.
    fn route_focus_input_event_to_target(&mut self, roots: &mut [Node], style: &Style, event: &UiInputEvent) -> bool {
        let Some(focus) = self.focus.filter(|id| contains_active_node_in(roots, *id)) else {
            return false;
        };
        self.route_input_event_to_target(roots, focus, style, event).is_consumed()
    }

    /// Applies runtime pointer-capture ownership from one routed event result.
    pub(crate) fn update_pointer_capture(&mut self, owner: RuntimeNodeId, result: ContainerInputResult, event: &UiInputEvent, mouse_buttons: MouseButton) {
        if event.is_pointer_release() && mouse_buttons.is_empty() {
            self.defer_current_pointer_capture_loss();
        } else if result == ContainerInputResult::Captured {
            self.acquire_pointer_capture(owner);
        } else if self.capture == Some(owner) && mouse_buttons.is_empty() {
            self.defer_current_pointer_capture_loss();
        }
    }

    /// Walks borrowed children first so nested owners beat ancestors.
    pub(crate) fn route_input_event_to_node_ref(
        &mut self,
        node: &mut Node,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> Option<(RuntimeNodeId, ContainerInputResult)> {
        let id = node.id();
        let child_transform = parent_transform.push(node.state.layout);
        let child_result = if node_children_visible(node) {
            node.with_children_mut(|children| {
                for child in children.iter_mut().rev() {
                    if let Some(result) = self.route_input_event_to_node_ref(child, child_transform, style, event) {
                        return Some(result);
                    }
                }
                None
            })
            .flatten()
        } else {
            None
        };
        child_result.or_else(|| {
            let result = self.route_input_event_to_node_only_ref(node, parent_transform, style, event);
            result.is_consumed().then_some((id, result))
        })
    }

    /// Routes a root's post-tree chrome before its application descendants.
    ///
    /// Ordinary retained containers remain child-first. A root is different because its title,
    /// close button, and resize grip are painted after the complete application tree and therefore
    /// occupy the top input layer where their rectangles overlap application content.
    pub(crate) fn route_root_input_event_to_node_ref(
        &mut self,
        node: &mut Node,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> Option<(RuntimeNodeId, ContainerInputResult)> {
        let id = node.id();
        let root_result = self.route_input_event_to_node_only_ref(node, parent_transform, style, event);
        if root_result.is_consumed() {
            return Some((id, root_result));
        }

        let child_transform = parent_transform.push(node.state.layout);
        if node_children_visible(node) {
            return node
                .with_children_mut(|children| {
                    for child in children.iter_mut().rev() {
                        if let Some(result) = self.route_input_event_to_node_ref(child, child_transform, style, event) {
                            return Some(result);
                        }
                    }
                    None
                })
                .flatten();
        }
        None
    }

    /// Routes directly to one target during a single transform-carrying tree traversal.
    fn route_input_event_to_target(&mut self, roots: &mut [Node], target: RuntimeNodeId, style: &Style, event: &UiInputEvent) -> ContainerInputResult {
        // Roots are independent transform origins; stop as soon as the unique target is found.
        for root in roots {
            if let Some(result) = self.route_input_event_to_target_from(root, target, self.root_transform, style, event) {
                return result;
            }
        }
        ContainerInputResult::Ignored
    }

    /// Descends toward one target while carrying the exact parent transform for each level.
    ///
    /// This replaces recursive parent lookup and transform reconstruction with one forward walk.
    fn route_input_event_to_target_from(
        &mut self,
        current: &mut Node,
        target: RuntimeNodeId,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> Option<ContainerInputResult> {
        if current.id() == target {
            return Some(self.route_input_event_to_node_only_ref(current, parent_transform, style, event));
        }
        // Children share the transform produced by their current parent layout.
        let child_parent = parent_transform.push(current.state.layout);
        current.with_children_mut(|children| {
            children
                .iter_mut()
                .find_map(|child| self.route_input_event_to_target_from(child, target, child_parent, style, event))
        })?
    }

    /// Routes an event to exactly one borrowed node without traversing descendants.
    fn route_input_event_to_node_only_ref(
        &mut self,
        node: &mut Node,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> ContainerInputResult {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.routed_input_dispatches += 1);
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let content_rect = crate::ui_node::frame::frame_geometry(local_rect, framed, style).content_or_empty();
        let screen_clip = parent_transform.clip.intersect(&screen_rect).unwrap_or_default();
        let local_clip = rect_relative_to(screen_clip, screen_origin);
        let content_clip = local_clip
            .intersect(&content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));
        let local_event = super::widget_context::localize_event(screen_origin, event.clone());

        match &mut node.data {
            NodeKind::Widget(widget) => {
                let opt = widget.widget.effective_widget_opt();
                super::container::route_public_widget_input(self, &node.state, local_rect, local_clip, opt, &local_event)
            }
            NodeKind::Container(container) => {
                let mut ctx = ContainerInputCtx::new(self, content_rect, content_clip, &node.state);
                container.route_input(&mut ctx, &local_event)
            }
        }
    }
}
