//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Input target selection, dispatch, and transient focus, hover, and capture lifecycle.

use super::*;

/// Dispatcher-internal result of delivering one event to an already-selected node.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum DispatchResult {
    /// The selected node declined the event, allowing ancestor-only bubbling.
    Ignored,
    /// The selected node consumed the event without acquiring pointer capture.
    Consumed,
    /// The selected node consumed the event and requests dispatcher-owned pointer capture.
    Captured,
}

impl DispatchResult {
    /// Returns whether dispatch and ancestor bubbling should stop.
    pub(crate) fn is_consumed(self) -> bool {
        matches!(self, Self::Consumed | Self::Captured)
    }
}

impl UiRuntime {
    /// Returns whether generic widget options admit one event kind.
    fn options_accept_event(opt: WidgetOption, event: &UiInputEvent) -> bool {
        // This is stateless dispatcher policy, so it is an associated function rather than a
        // method borrowing runtime state. Wheel input requires an explicit grab and a delta that
        // can represent movement; every other event kind is governed by target geometry, focus,
        // capture, and the NO_INTERACT check in dispatch_widget_input.
        match event {
            UiInputEvent::Scroll { delta, .. } => opt.intersects(WidgetOption::GRAB_SCROLL) && (delta.x != 0 || delta.y != 0),
            _ => true,
        }
    }

    /// Delivers one already-targeted event through common dispatcher interaction rules.
    fn dispatch_widget_input(
        &mut self,
        state: &NodeRuntime,
        rect: Recti,
        clip: Recti,
        opt: WidgetOption,
        accepts_event: bool,
        event: &UiInputEvent,
    ) -> DispatchResult {
        let id = state.id();
        let captured = self.capture == Some(id);
        // Target selection may reach a geometrically matching node whose dynamic options or
        // surface policy declines this event. Such a node remains available for ancestor-only
        // bubbling.
        if opt.intersects(WidgetOption::NO_INTERACT) || !Self::options_accept_event(opt, event) || !accepts_event {
            return DispatchResult::Ignored;
        }

        if event.is_focus_input() {
            // Keyboard/text events have no meaningful rectangle. Deliver them only to the
            // dispatcher-owned focus identity selected by an earlier pointer or programmatic
            // transition.
            if self.focus != Some(id) {
                return DispatchResult::Ignored;
            }
            self.push_routed_event(id, event.clone());
            return DispatchResult::Consumed;
        }

        // Capture lets drag/release escape the original rectangle; all other pointer events still
        // need to hit both the node allocation and its effective clip.
        let event_hits_rect = event.position().is_some_and(|pos| rect.contains(&pos) && clip.contains(&pos));
        match event {
            UiInputEvent::MouseDown { button, .. } if event_hits_rect => {
                // Press establishes focus/click state and asks routing to acquire pointer capture.
                self.claim_pointer_focus(id, *button);
                self.push_routed_event(id, event.clone());
                DispatchResult::Captured
            }
            UiInputEvent::MouseDrag { .. } if captured || event_hits_rect => {
                // An uncaptured drag can be consumed under the pointer but does not create capture.
                self.push_routed_event(id, event.clone());
                if captured { DispatchResult::Captured } else { DispatchResult::Consumed }
            }
            UiInputEvent::MouseUp { .. } if captured || event_hits_rect => {
                // Routing releases capture after the recipient observes this event during update.
                self.push_routed_event(id, event.clone());
                DispatchResult::Consumed
            }
            UiInputEvent::MouseMove { .. } | UiInputEvent::Scroll { .. } if event_hits_rect => {
                // Hover and wheel delivery are hit-based and never acquire capture.
                self.push_routed_event(id, event.clone());
                DispatchResult::Consumed
            }
            _ => DispatchResult::Ignored,
        }
    }

    /// Clears interaction state that points at a removed or container-gated descendant.
    ///
    /// Sanitization runs at layout/update boundaries and before direct target delivery, when no
    /// container state borrow is active. A replacement node cannot inherit a stale target because
    /// every owning node has a fresh ID.
    pub(super) fn sanitize_transient_targets(&mut self, roots: &mut [Node]) {
        self.focus = self.focus.filter(|id| contains_active_node_in(roots, *id, self.root_transform));
        self.hover = self.hover.filter(|id| contains_active_node_in(roots, *id, self.root_transform));
        if self
            .routed_event
            .as_ref()
            .is_some_and(|(id, _)| !contains_active_node_in(roots, *id, self.root_transform))
        {
            self.routed_event = None;
        }

        let capture_valid = self.capture.is_none_or(|id| contains_active_node_in(roots, id, self.root_transform));
        if !capture_valid {
            self.invalidate_pointer_capture();
        }

        debug_assert!(self.focus.is_none_or(|id| contains_active_node_in(roots, id, self.root_transform)));
        debug_assert!(self.hover.is_none_or(|id| contains_active_node_in(roots, id, self.root_transform)));
        debug_assert!(
            self.routed_event
                .as_ref()
                .is_none_or(|(id, _)| contains_active_node_in(roots, *id, self.root_transform))
        );
        debug_assert!(self.capture.is_none() || capture_valid);
    }

    /// Invalidates current capture after its retained target becomes externally ineligible.
    pub(super) fn invalidate_pointer_capture(&mut self) {
        // A subsequent drag/release belongs to the revoked gesture and must not fall through to a
        // replacement target. Widgets reconcile private modes from a later inactive update.
        if self.capture.take().is_some() {
            self.discard_invalidated_capture_events = true;
        }
    }

    /// Acquires capture for one routed owner in the current event transaction.
    fn acquire_pointer_capture(&mut self, owner: RuntimeNodeId) {
        // Replacing an earlier owner is an atomic runtime identity transition. The previous widget
        // observes `active() == false` during the already-scheduled full-tree update.
        if self.capture == Some(owner) {
            return;
        }
        self.capture = Some(owner);
        self.discard_invalidated_capture_events = false;
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
        // Direct delivery will recompute hover from the captured node's clipped allocation.
        self.hover = None;
        let result = self.route_input_event_to_target(roots, capture, style, event);
        self.update_pointer_capture(capture, result, event, mouse_buttons);
        Some(result.is_consumed())
    }

    /// Routes keyboard/text input to the focused node only.
    fn route_focus_input_event_to_target(&mut self, roots: &mut [Node], style: &Style, event: &UiInputEvent) -> bool {
        let Some(focus) = self.focus.filter(|id| contains_active_node_in(roots, *id, self.root_transform)) else {
            return false;
        };
        // Focus input bypasses pointer targeting and goes directly to the retained focus owner.
        self.route_input_event_to_target(roots, focus, style, event).is_consumed()
    }

    /// Applies runtime pointer-capture ownership from one routed event result.
    pub(crate) fn update_pointer_capture(&mut self, owner: RuntimeNodeId, result: DispatchResult, event: &UiInputEvent, mouse_buttons: MouseButton) {
        if event.is_pointer_release() && mouse_buttons.is_empty() {
            // Normal release is not an invalidation: its event was delivered to the old owner, and
            // the following update exposes inactive state without swallowing a future gesture.
            self.capture = None;
        } else if result == DispatchResult::Captured {
            self.acquire_pointer_capture(owner);
        } else if self.capture == Some(owner) && mouse_buttons.is_empty() {
            // Defensive cleanup covers a consumed event that leaves no held buttons even when it
            // is not represented by the ordinary release branch above.
            self.capture = None;
        }
    }

    /// Routes one topmost ordinary hit and bubbles an ignored event only through its ancestors.
    #[cfg(test)]
    pub(crate) fn route_input_event_to_node_ref(
        &mut self,
        node: &mut Node,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> Option<(RuntimeNodeId, DispatchResult)> {
        let pos = event.position()?;
        // Select exactly one target before any handler runs so ignored events cannot reveal a
        // covered sibling.
        let target = self.hit_test_pointer_node_ref(node, parent_transform, pos)?;
        self.hover = Some(target);
        self.route_input_event_to_target_path_from(node, target, parent_transform, style, event)
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
        root_chrome_hit: bool,
    ) -> Option<(RuntimeNodeId, DispatchResult)> {
        let pos = event.position()?;
        // The window manager owns post-tree chrome geometry. When it reports a chrome hit, the
        // root wins before descendants; otherwise ordinary targeting starts inside the root body.
        let target = if root_chrome_hit {
            Some(node.id())
        } else {
            let child_transform = parent_transform.push(node.state.layout);
            node_children_visible(node)
                .then(|| {
                    node.with_children(|children| {
                        children
                            .iter()
                            .rev()
                            .filter(|child| child.intersects_clip(child_transform))
                            .find_map(|child| self.hit_test_pointer_node_ref(child, child_transform, pos))
                    })
                })
                .flatten()
        }?;

        self.hover = Some(target);
        self.route_input_event_to_target_path_from(node, target, parent_transform, style, event)
    }

    /// Selects the deepest topmost node whose clipped allocation contains the pointer.
    fn hit_test_pointer_node_ref(&self, node: &Node, parent_transform: Transform, pos: Vec2i) -> Option<RuntimeNodeId> {
        // Layout eligibility filters the whole branch before any geometry or widget policy is
        // considered. The dispatcher, not the layout, remains responsible for target selection.
        if !node_accepts_input(node) {
            return None;
        }
        let child_transform = parent_transform.push(node.state.layout);
        // Children paint after their ordinary parent surface, so inspect them in reverse paint
        // order before considering the current node.
        if node_children_visible(node)
            && let Some(target) = node.with_children(|children| {
                children
                    .iter()
                    .rev()
                    .filter(|child| child.intersects_clip(child_transform))
                    .find_map(|child| self.hit_test_pointer_node_ref(child, child_transform, pos))
            })
        {
            return Some(target);
        }

        self.pointer_hits_node(node, parent_transform, pos).then(|| node.id())
    }

    /// Tests one node's own allocation using only dispatcher-owned geometry and options.
    fn pointer_hits_node(&self, node: &Node, parent_transform: Transform, pos: Vec2i) -> bool {
        // NO_INTERACT makes only this node's own surface transparent; eligible descendants were
        // already considered by the caller.
        if node
            .data
            .with_widget(|widget| widget.effective_widget_opt().intersects(WidgetOption::NO_INTERACT))
        {
            return false;
        }

        // Allocations are parent-local while the inherited clip is already in screen space.
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        parent_transform.clip.contains(&pos) && screen_rect.contains(&pos)
    }

    /// Dispatches to one selected target, then bubbles an ignored result through ancestors only.
    fn route_input_event_to_target_path_from(
        &mut self,
        current: &mut Node,
        target: RuntimeNodeId,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> Option<(RuntimeNodeId, DispatchResult)> {
        if current.id() == target {
            // The target was already selected geometrically; its result only controls handling.
            let result = self.route_input_event_to_node_only_ref(current, parent_transform, style, event);
            return Some((target, result));
        }

        // Follow the unique target path without revisiting sibling hit testing.
        let child_transform = parent_transform.push(current.state.layout);
        let (owner, result) = current.with_children_mut(|children| {
            children
                .iter_mut()
                .filter(|child| child.intersects_clip(child_transform))
                .find_map(|child| self.route_input_event_to_target_path_from(child, target, child_transform, style, event))
        })??;
        if result.is_consumed() {
            return Some((owner, result));
        }

        // Ignored delivery bubbles to the structural parent regardless of the parent's own hit.
        let parent_result = self.route_input_event_to_node_only_ref(current, parent_transform, style, event);
        Some(if parent_result.is_consumed() {
            (current.id(), parent_result)
        } else {
            (owner, result)
        })
    }

    /// Routes directly to one target during a single transform-carrying tree traversal.
    fn route_input_event_to_target(&mut self, roots: &mut [Node], target: RuntimeNodeId, style: &Style, event: &UiInputEvent) -> DispatchResult {
        // Roots are independent transform origins; stop as soon as the unique target is found.
        for root in roots {
            if let Some(result) = self.route_input_event_to_target_from(root, target, self.root_transform, style, event) {
                return result;
            }
        }
        DispatchResult::Ignored
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
    ) -> Option<DispatchResult> {
        if current.id() == target {
            // Direct focus/capture delivery stops at the target and never bubbles.
            return Some(self.route_input_event_to_node_only_ref(current, parent_transform, style, event));
        }
        // Children share the transform produced by their current parent layout.
        let child_parent = parent_transform.push(current.state.layout);
        current.with_children_mut(|children| {
            children
                .iter_mut()
                .filter(|child| child.intersects_clip(child_parent))
                .find_map(|child| self.route_input_event_to_target_from(child, target, child_parent, style, event))
        })?
    }

    /// Routes an event to exactly one borrowed node without traversing descendants.
    fn route_input_event_to_node_only_ref(&mut self, node: &mut Node, parent_transform: Transform, style: &Style, event: &UiInputEvent) -> DispatchResult {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.routed_input_dispatches += 1);
        // Resolve the same frame/content geometry used by update and paint before localizing the
        // selected event for the concrete widget or container handler.
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let content_rect = crate::ui_node::frame::frame_geometry(local_rect, framed, style).content_or_empty();
        let screen_clip = parent_transform.clip.intersect(&screen_rect).unwrap_or_default();
        let local_clip = screen_clip.relative_to(screen_origin);
        let content_clip = local_clip
            .intersect(&content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));
        let local_event = super::widget_context::localize_event(screen_origin, event.clone());
        let captured = self.capture == Some(node.id());

        if captured && let Some(pos) = event.position() {
            // Capture changes delivery only. Hover still follows the same allocation predicate as
            // ordinary target selection and therefore clears when a drag leaves the owner.
            self.hover = self.pointer_hits_node(node, parent_transform, pos).then(|| node.id());
        }

        match &mut node.data {
            NodeKind::Widget(widget) => {
                // Leaf event-kind policy is completely dispatcher-owned; no Widget query widens
                // the public trait or permits a handler to influence geometric target selection.
                let opt = widget
                    .widget
                    .try_borrow()
                    .expect("retained widget invariant violated during input dispatch")
                    .effective_widget_opt();
                self.dispatch_widget_input(&node.state, local_rect, local_clip, opt, true, &local_event)
            }
            NodeKind::Container(container) => {
                // An overloaded container surface may add a state-dependent filter after target
                // selection. Captured continuation skips that query because UiRuntime already owns
                // this target; this also prevents stale surface-local modes from affecting routing.
                let opt = container.effective_widget_opt();
                let captured_continuation = captured && matches!(&local_event, UiInputEvent::MouseDrag { .. } | UiInputEvent::MouseUp { .. });
                let accepts_event = captured_continuation || container.accepts_event(&local_event);
                self.dispatch_widget_input(&node.state, content_rect, content_clip, opt, accepts_event, &local_event)
            }
        }
    }
}
