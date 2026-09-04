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

//! Input target selection plus persistent focus and transient pointer-target lifecycle.

use super::*;

/// Per-tree state machine that turns normalized raw input into one node-local delivery.
///
/// `WindowManager` chooses which retained root may receive an input event. `InputRouter` then owns
/// every decision inside that root: hit testing, ancestor bubbling, focus, hover, pointer capture,
/// and staging the sole [`UiInputEvent`] consumed by the subsequent widget update traversal. It
/// never invokes application subscribers or handles semantic [`crate::WidgetEvent`] payloads;
/// those belong to the context-owned [`crate::event::WidgetEventDispatcher`].
///
/// The router stores node identities rather than widget borrows. This lets routing finish before
/// `UiRuntime` begins its full mutable update traversal and lets layout or topology changes validate
/// stale targets without re-entering widget code.
pub(super) struct InputRouter {
    /// Node that persistently receives keyboard and text input while it remains eligible.
    focus: Option<RuntimeNodeId>,
    /// Deepest topmost node under the pointer for the current committed geometry.
    hover: Option<RuntimeNodeId>,
    /// Node that owns drag continuation and the matching pointer release.
    capture: Option<RuntimeNodeId>,
    /// Whether drag/release events from a revoked capture must be swallowed.
    discard_invalidated_capture_events: bool,
    /// Whether this root is eligible for pointer routing during the current raw event.
    pointer_input_enabled: bool,
    /// Whether the current raw event is a pointer event and therefore refreshes hover.
    pointer_event_active: bool,
    /// Node receiving the one-update `clicked` transition from the current pointer press.
    clicked: Option<RuntimeNodeId>,
    /// Sole localized raw event waiting for its selected node's update.
    routed_event: Option<(RuntimeNodeId, UiInputEvent)>,
    /// Allocation-reusing retained-order workspace for sequential keyboard traversal.
    tab_stops: Vec<RuntimeNodeId>,
    /// Number of already-selected node surfaces examined by routing in the current test cycle.
    #[cfg(test)]
    routed_input_routes: u64,
}

impl Default for InputRouter {
    /// Creates an idle router with no transient targets or pending delivery.
    fn default() -> Self {
        Self {
            focus: None,
            hover: None,
            capture: None,
            discard_invalidated_capture_events: false,
            pointer_input_enabled: false,
            pointer_event_active: false,
            clicked: None,
            routed_event: None,
            tab_stops: Vec::new(),
            #[cfg(test)]
            routed_input_routes: 0,
        }
    }
}

/// Router-internal result of delivering one input event to an already-selected node.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum RouteResult {
    /// The selected node declined the event, allowing ancestor-only bubbling.
    Ignored,
    /// The selected node consumed the event without acquiring pointer capture.
    Consumed,
    /// The selected node consumed the event and requests router-owned pointer capture.
    Captured,
}

impl RouteResult {
    /// Returns whether routing and ancestor bubbling should stop.
    pub(crate) fn is_consumed(self) -> bool {
        matches!(self, Self::Consumed | Self::Captured)
    }
}

impl InputRouter {
    /// Clears event-local state before a new explicit UI update begins.
    ///
    /// Focus and capture intentionally survive this boundary because they describe interaction
    /// spanning several raw events. A routed event and click marker belong to exactly one update
    /// transaction and must never leak into a later call.
    pub(super) fn begin_update(&mut self) {
        self.routed_event = None;
        self.clicked = None;
        self.pointer_event_active = false;
        #[cfg(test)]
        {
            self.routed_input_routes = 0;
        }
    }

    /// Establishes routing policy and event-local flags for one normalized raw input event.
    pub(super) fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        // The previous event must have been consumed during the full-tree update before another
        // normalized event can become authoritative for this root.
        debug_assert!(self.routed_event.is_none(), "the previous routed event was not consumed by update");
        self.pointer_input_enabled = pointer_input_enabled;
        self.pointer_event_active = event.is_pointer();
        self.clicked = None;
        if self.pointer_event_active {
            // Pointer routing recomputes hover from current committed geometry for every event.
            self.hover = None;
        }
        // Do not clear focus before target selection. Focusable pointer-down recipients replace it
        // below, while non-focusable surfaces such as scrollbars leave it untouched.
    }

    /// Clears every transient target while preserving no reference to a retained node.
    pub(super) fn clear_transient_targets(&mut self) {
        // Widgets reconcile their private interaction modes from the next neutral update
        // snapshot. Clearing router identities therefore needs no out-of-band widget callback.
        self.focus = None;
        self.hover = None;
        self.routed_event = None;
        self.clicked = None;
        self.invalidate_pointer_capture();
    }

    /// Clears pointer-derived targets while retaining this tree's keyboard focus identity.
    pub(super) fn clear_pointer_targets(&mut self) {
        // Modal exclusion and a competing surface gesture revoke capture and stale hover without
        // erasing the control that should regain keyboard ownership when its scope is active again.
        self.hover = None;
        self.routed_event = None;
        self.clicked = None;
        self.invalidate_pointer_capture();
    }

    /// Returns whether this tree currently owns pointer capture.
    pub(super) const fn has_capture(&self) -> bool {
        self.capture.is_some()
    }

    /// Returns whether the cross-root policy admits pointer routing for the current event.
    pub(super) const fn accepts_pointer_input(&self) -> bool {
        self.pointer_input_enabled
    }

    /// Records one routed event for consumption by exactly one node during widget update.
    pub(super) fn push_routed_event(&mut self, node: RuntimeNodeId, event: UiInputEvent) {
        // Multiple recipients would make delivery depend on update order and would violate the
        // target-first rule that prevents ignored hits from exposing covered siblings.
        debug_assert!(self.routed_event.is_none(), "one input event was routed to more than one recipient");
        self.routed_event = Some((node, event));
    }

    /// Records an accepted pointer press and conditionally replaces the keyboard focus owner.
    fn claim_pointer_press(&mut self, node: RuntimeNodeId, button: MouseButton, focusable: bool) {
        // Pointer capture is acquired separately from keyboard focus. An explicitly focusable
        // control replaces the persistent owner; a non-focusable pointer target preserves it naturally.
        if focusable {
            self.focus = Some(node);
        }
        // The clicked transition describes pointer targeting and is independent from focusability.
        if button.intersects(MouseButton::LEFT) {
            self.clicked = Some(node);
        }
    }

    /// Takes the pending raw event only when `node` is its preselected recipient.
    pub(super) fn take_routed_event(&mut self, node: RuntimeNodeId) -> Option<UiInputEvent> {
        // Leave the event intact while unrelated nodes update; traversal order cannot redirect it.
        if self.routed_event.as_ref().is_some_and(|(recipient, _)| *recipient == node) {
            self.routed_event.take().map(|(_, event)| event)
        } else {
            None
        }
    }

    /// Commits the interaction snapshot exposed to one node during the full-tree update.
    pub(super) fn commit_interaction_snapshot(
        &mut self,
        id: RuntimeNodeId,
        prior_hovered: bool,
        input: InputSnapshot,
        opt: WidgetOption,
    ) -> (bool, bool, bool, bool) {
        // A disabled surface must observe a completely neutral snapshot even if its identity was
        // selected before a state change disabled it.
        if opt.intersects(WidgetOption::NO_INTERACT) {
            return (false, false, false, false);
        }

        // Pointer events recompute hover during routing; keyboard-only updates preserve it.
        let hovered = if self.pointer_event_active { self.hover == Some(id) } else { prior_hovered };

        // Active derives from router-owned capture rather than a widget-local drag flag. This lets
        // capture invalidation reconcile every widget through the ordinary update traversal.
        let focused = self.focus == Some(id);
        let active = self.capture == Some(id) && input.mouse_buttons.intersects(MouseButton::LEFT);
        let clicked = self.clicked == Some(id);
        (hovered, focused, clicked, active)
    }

    /// Returns the current test-only count of selected node surfaces visited by routing.
    #[cfg(test)]
    pub(super) const fn debug_routed_input_routes(&self) -> u64 {
        self.routed_input_routes
    }

    /// Returns the focused identity for retained routing tests.
    #[cfg(test)]
    pub(super) const fn debug_focus_target(&self) -> Option<RuntimeNodeId> {
        self.focus
    }

    /// Returns the hovered identity for retained routing tests.
    #[cfg(test)]
    pub(super) const fn debug_hover_target(&self) -> Option<RuntimeNodeId> {
        self.hover
    }

    /// Returns the pointer-capture identity for retained routing tests.
    #[cfg(test)]
    pub(super) const fn debug_capture_target(&self) -> Option<RuntimeNodeId> {
        self.capture
    }

    /// Installs explicit transient identities for tests that exercise invalidation boundaries.
    #[cfg(test)]
    pub(super) fn debug_set_transient_targets(&mut self, focus: Option<RuntimeNodeId>, hover: Option<RuntimeNodeId>, capture: Option<RuntimeNodeId>) {
        self.focus = focus;
        self.hover = hover;
        self.capture = capture;
    }

    /// Replaces only pointer capture for tests that begin from an active gesture.
    #[cfg(test)]
    pub(super) fn debug_set_capture_target(&mut self, capture: Option<RuntimeNodeId>) {
        self.capture = capture;
    }

    /// Reports whether a revoked gesture is still swallowing its drag/release tail in tests.
    #[cfg(test)]
    pub(super) const fn debug_discards_invalidated_capture_events(&self) -> bool {
        self.discard_invalidated_capture_events
    }

    /// Returns whether generic widget options admit one event kind.
    fn options_accept_event(opt: WidgetOption, event: &UiInputEvent) -> bool {
        // This is stateless router policy, so it is an associated function rather than a
        // method borrowing runtime state. Wheel input requires an explicit grab and a delta that
        // can represent movement; every other event kind is governed by target geometry, focus,
        // capture, and the NO_INTERACT check in route_widget_input.
        match event {
            UiInputEvent::Scroll { delta, .. } => opt.intersects(WidgetOption::GRAB_SCROLL) && (delta.x != 0 || delta.y != 0),
            _ => true,
        }
    }

    /// Delivers one already-targeted event through common router interaction rules.
    fn route_widget_input(
        &mut self,
        state: &NodeRuntime,
        rect: Recti,
        clip: Recti,
        opt: WidgetOption,
        keyboard: KeyboardBehavior,
        accepts_event: bool,
        event: &UiInputEvent,
    ) -> RouteResult {
        let id = state.id();
        let captured = self.capture == Some(id);
        // Target selection may reach a geometrically matching node whose dynamic options or
        // surface policy declines this event. Such a node remains available for ancestor-only
        // bubbling.
        if opt.intersects(WidgetOption::NO_INTERACT) || !Self::options_accept_event(opt, event) || !accepts_event {
            return RouteResult::Ignored;
        }

        if event.is_focus_input() {
            // Keyboard/text events have no meaningful rectangle. Deliver them only to the
            // router-owned focus identity selected by an earlier pointer or programmatic
            // transition.
            if self.focus != Some(id) {
                return RouteResult::Ignored;
            }
            self.push_routed_event(id, event.clone());
            return RouteResult::Consumed;
        }

        // Capture lets drag/release escape the original rectangle; all other pointer events still
        // need to hit both the node allocation and its effective clip.
        let event_hits_rect = event.position().is_some_and(|pos| rect.contains_point(pos) && clip.contains_point(pos));
        match event {
            UiInputEvent::MouseDown { button, .. } if event_hits_rect => {
                // Focusability controls persistent keyboard ownership; the Captured result below
                // independently establishes drag ownership for every accepted pointer press.
                self.claim_pointer_press(id, *button, keyboard.is_focusable());
                self.push_routed_event(id, event.clone());
                RouteResult::Captured
            }
            UiInputEvent::MouseDrag { .. } if captured || event_hits_rect => {
                // An uncaptured drag can be consumed under the pointer but does not create capture.
                self.push_routed_event(id, event.clone());
                if captured { RouteResult::Captured } else { RouteResult::Consumed }
            }
            UiInputEvent::MouseUp { .. } if captured || event_hits_rect => {
                // Routing releases capture after the recipient observes this event during update.
                self.push_routed_event(id, event.clone());
                RouteResult::Consumed
            }
            UiInputEvent::MouseMove { .. } | UiInputEvent::Scroll { .. } if event_hits_rect => {
                // Hover and wheel delivery are hit-based and never acquire capture.
                self.push_routed_event(id, event.clone());
                RouteResult::Consumed
            }
            _ => RouteResult::Ignored,
        }
    }

    /// Clears interaction state that points at a removed or container-gated descendant.
    ///
    /// Sanitization runs at layout/update boundaries and before direct target delivery, when no
    /// container state borrow is active. A replacement node cannot inherit a stale target because
    /// every owning node has a fresh ID.
    pub(super) fn sanitize_transient_targets(&mut self, roots: &mut [Node], root_transform: Transform) {
        // The transform remains owned by UiRuntime's committed layout. Passing it in keeps the
        // router independent of measurement, placement, and paint state.
        self.focus = self.focus.filter(|id| contains_focusable_node_in(roots, *id, root_transform));
        self.hover = self.hover.filter(|id| contains_active_node_in(roots, *id, root_transform));
        if self
            .routed_event
            .as_ref()
            .is_some_and(|(id, _)| !contains_active_node_in(roots, *id, root_transform))
        {
            self.routed_event = None;
        }

        let capture_valid = self.capture.is_none_or(|id| contains_active_node_in(roots, id, root_transform));
        if !capture_valid {
            self.invalidate_pointer_capture();
        }

        debug_assert!(self.focus.is_none_or(|id| contains_focusable_node_in(roots, id, root_transform)));
        debug_assert!(self.hover.is_none_or(|id| contains_active_node_in(roots, id, root_transform)));
        debug_assert!(
            self.routed_event
                .as_ref()
                .is_none_or(|(id, _)| contains_active_node_in(roots, *id, root_transform))
        );
        debug_assert!(self.capture.is_none() || capture_valid);
    }

    /// Invalidates current capture after its retained target becomes externally ineligible.
    pub(super) fn invalidate_pointer_capture(&mut self) {
        // A subsequent drag/release belongs to the revoked gesture and must not fall through to a
        // replacement target. Widgets reconcile private modes from a later neutral update.
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
    pub(super) fn route_focus_input_event(&mut self, roots: &mut [Node], root_transform: Transform, style: &Skin, event: &UiInputEvent) -> bool {
        // Sanitization is the single authoritative eligibility check for this transaction. It
        // clears focus unless the identity still exists beneath active, intersecting ancestors and
        // the target itself remains interactive and focusable.
        self.sanitize_transient_targets(roots, root_transform);
        let Some(focus) = self.focus else {
            return false;
        };

        // Keyboard and text events have no pointer target. Deliver directly by stable identity;
        // the search also accumulates the target's current transform and inherited style without
        // retaining either as parallel state between layout commits.
        self.route_input_event_to_target(roots, focus, root_transform, style, event).is_consumed()
    }

    /// Advances persistent keyboard focus through eligible Tab stops in retained tree order.
    pub(super) fn advance_focus(&mut self, roots: &mut [Node], root_transform: Transform, reverse: bool) -> bool {
        self.sanitize_transient_targets(roots, root_transform);
        self.tab_stops.clear();
        for root in roots.iter() {
            collect_tab_stops(root, root_transform, &mut self.tab_stops);
        }
        if self.tab_stops.is_empty() {
            self.focus = None;
            return false;
        }

        // A missing current identity starts at the direction-appropriate edge. Otherwise move one
        // position with explicit wrap so behavior does not depend on numeric node identities.
        let current = self.focus.and_then(|focus| self.tab_stops.iter().position(|candidate| *candidate == focus));
        let next = match (current, reverse) {
            (Some(0), true) | (None, true) => self.tab_stops.len() - 1,
            (Some(index), true) => index - 1,
            (Some(index), false) if index + 1 == self.tab_stops.len() => 0,
            (Some(index), false) => index + 1,
            (None, false) => 0,
        };
        self.focus = Some(self.tab_stops[next]);
        true
    }

    /// Routes one pointer event to the capturing node, if there is one.
    pub(super) fn route_captured_pointer_input_event(
        &mut self,
        roots: &mut [Node],
        root_transform: Transform,
        style: &Skin,
        mouse_buttons: MouseButton,
        event: &UiInputEvent,
    ) -> Option<bool> {
        self.sanitize_transient_targets(roots, root_transform);

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
        let result = self.route_input_event_to_target(roots, capture, root_transform, style, event);
        self.update_pointer_capture(capture, result, event, mouse_buttons);
        Some(result.is_consumed())
    }

    /// Applies runtime pointer-capture ownership from one routed event result.
    pub(super) fn update_pointer_capture(&mut self, owner: RuntimeNodeId, result: RouteResult, event: &UiInputEvent, mouse_buttons: MouseButton) {
        if event.is_pointer_release() && mouse_buttons.is_empty() {
            // Normal release is not an invalidation: its event was delivered to the old owner, and
            // the following update exposes the non-pressed state without swallowing a future gesture.
            self.capture = None;
        } else if result == RouteResult::Captured {
            self.acquire_pointer_capture(owner);
        } else if self.capture == Some(owner) && mouse_buttons.is_empty() {
            // Defensive cleanup covers a consumed event that leaves no held buttons even when it
            // is not represented by the ordinary release branch above.
            self.capture = None;
        }
    }

    /// Routes one topmost ordinary hit and bubbles an ignored event only through its ancestors.
    pub(super) fn route_input_event_to_node_ref(
        &mut self,
        node: &mut Node,
        parent_transform: Transform,
        style: &Skin,
        event: &UiInputEvent,
    ) -> Option<(RuntimeNodeId, RouteResult)> {
        let pos = event.position()?;
        // Select exactly one target before any handler runs so ignored events cannot reveal a
        // covered sibling.
        let target = self.hit_test_pointer_node_ref(node, parent_transform, pos)?;
        self.hover = Some(target);
        self.route_input_event_to_target_path_from(node, target, parent_transform, style, event)
    }

    /// Selects the deepest topmost node whose clipped allocation contains the pointer.
    fn hit_test_pointer_node_ref(&self, node: &Node, parent_transform: Transform, pos: Vec2i) -> Option<RuntimeNodeId> {
        // Layout eligibility filters the whole branch before any geometry or widget policy is
        // considered. The router, not layout, remains responsible for target selection.
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

    /// Tests one node's own allocation using only router-owned geometry and options.
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
        parent_transform.clip.contains_point(pos) && screen_rect.contains_point(pos)
    }

    /// Routes to one selected target, then bubbles an ignored result through ancestors only.
    fn route_input_event_to_target_path_from(
        &mut self,
        current: &mut Node,
        target: RuntimeNodeId,
        parent_transform: Transform,
        style: &Skin,
        event: &UiInputEvent,
    ) -> Option<(RuntimeNodeId, RouteResult)> {
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
    fn route_input_event_to_target(
        &mut self,
        roots: &mut [Node],
        target: RuntimeNodeId,
        root_transform: Transform,
        style: &Skin,
        event: &UiInputEvent,
    ) -> RouteResult {
        // Roots are independent transform origins; stop as soon as the unique target is found.
        for root in roots {
            if let Some(result) = self.route_input_event_to_target_from(root, target, root_transform, style, event) {
                return result;
            }
        }
        RouteResult::Ignored
    }

    /// Descends toward one target while carrying the exact parent transform for each level.
    ///
    /// This replaces recursive parent lookup and transform reconstruction with one forward walk.
    fn route_input_event_to_target_from(
        &mut self,
        current: &mut Node,
        target: RuntimeNodeId,
        parent_transform: Transform,
        style: &Skin,
        event: &UiInputEvent,
    ) -> Option<RouteResult> {
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
    fn route_input_event_to_node_only_ref(&mut self, node: &mut Node, parent_transform: Transform, style: &Skin, event: &UiInputEvent) -> RouteResult {
        #[cfg(test)]
        {
            self.routed_input_routes += 1;
        }
        // Resolve the same frame/content geometry used by update and paint before localizing the
        // selected event for the concrete widget or container handler.
        let frame_role = node_frame_role(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let content_rect = crate::ui_node::frame::frame_geometry(local_rect, frame_role, style).content_or_empty();
        let screen_clip = parent_transform.clip.positive_intersection(screen_rect).unwrap_or_default();
        let local_clip = screen_clip.relative_to(screen_origin);
        let content_clip = local_clip
            .positive_intersection(content_rect)
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
                // Leaf event-kind policy is completely router-owned; no Widget query widens
                // the public trait or permits a handler to influence geometric target selection.
                let (opt, keyboard) = {
                    let widget = widget.widget.try_borrow().expect("retained widget invariant violated during input routing");
                    (widget.widget.effective_widget_opt(), widget.widget.keyboard_behavior())
                };
                self.route_widget_input(&node.state, local_rect, local_clip, opt, keyboard, true, &local_event)
            }
            NodeKind::Container(container) => {
                // An overloaded container surface may add a state-dependent filter after target
                // selection. Captured continuation skips that query because InputRouter already
                // owns this target; this also prevents stale surface-local modes from affecting
                // routing.
                let opt = container.effective_widget_opt();
                let keyboard = container.with_widget(|widget| widget.keyboard_behavior());
                let captured_continuation = captured && matches!(&local_event, UiInputEvent::MouseDrag { .. } | UiInputEvent::MouseUp { .. });
                let accepts_event = captured_continuation || container.accepts_event(&local_event);
                self.route_widget_input(&node.state, content_rect, content_clip, opt, keyboard, accepts_event, &local_event)
            }
        }
    }
}

/// Collects eligible Tab stops in stable parent-first retained order.
fn collect_tab_stops(node: &Node, parent_transform: Transform, output: &mut Vec<RuntimeNodeId>) {
    // Participation and clipping are the same gates used by pointer and focused-key routing. The
    // MVP deliberately skips fully clipped descendants until scroll-to-focus policy is introduced.
    if !node_accepts_input(node) || !node.intersects_clip(parent_transform) {
        return;
    }
    let (opt, keyboard) = node_interaction_config(node);
    if !opt.intersects(WidgetOption::NO_INTERACT) && keyboard.is_tab_stop() {
        output.push(node.id());
    }
    if !node_children_visible(node) {
        return;
    }
    let child_transform = parent_transform.push(node.state.layout);
    node.with_children(|children| {
        for child in children.iter() {
            collect_tab_stops(child, child_transform, output);
        }
    });
}
