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

//! Authoritative retained-tree phase orchestration around an independent raw-input router.

use super::*;

mod input_router;
mod layout_traversal;
mod paint;
mod update;

#[cfg(test)]
pub(crate) use input_router::RouteResult;
use input_router::InputRouter;

use crate::input::InputSnapshot;
use crate::math::RectExt;
use crate::render::DisplayList;
use crate::{Constraints, Dimensioni, MouseButton, Recti, Style, UNCLIPPED_RECT, Vec2i};
#[cfg(test)]
use std::cell::Cell;

/// Test-only counters for one retained root's most recent update/paint cycle.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeMetrics {
    pub(crate) tree_layouts: u64,
    pub(crate) measures: u64,
    pub(crate) layouts: u64,
    pub(crate) updates: u64,
    pub(crate) paints: u64,
    pub(crate) routed_input_routes: u64,
}

/// Coordinates retained measurement, layout, update, and paint for one root tree.
///
/// Raw-input state is intentionally delegated to [`InputRouter`]. `UiRuntime` supplies the router
/// with the authoritative tree and committed transform at explicit routing boundaries, then runs
/// the complete update traversal that consumes the router's one staged node-local event.
pub(crate) struct UiRuntime {
    /// Aggregate root content size in root body coordinates.
    root_content_size: Dimensioni,
    /// Transform from root body coordinates into screen coordinates.
    root_transform: Transform,
    /// Independent state machine for raw-input targeting and transient interaction ownership.
    input_router: InputRouter,
    /// Structural phase counters used by P0/P5 characterization.
    #[cfg(test)]
    metrics: Cell<RuntimeMetrics>,
}

impl Default for UiRuntime {
    fn default() -> Self {
        Self {
            root_content_size: Dimensioni::default(),
            root_transform: Transform::root(UNCLIPPED_RECT),
            input_router: InputRouter::default(),
            #[cfg(test)]
            metrics: Cell::new(RuntimeMetrics::default()),
        }
    }
}

impl UiRuntime {
    /// Creates an empty runtime.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Clears update-cycle metrics before the initial synchronization layout.
    pub(crate) fn begin_update(&mut self) {
        // InputRouter independently clears event-local delivery while preserving focus/capture;
        // UiRuntime resets only traversal metrics owned by the remaining runtime phases.
        self.input_router.begin_update();
        #[cfg(test)]
        self.metrics.set(RuntimeMetrics::default());
    }

    /// Starts input routing for one normalized raw event in this retained tree.
    pub(crate) fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        // WindowManager has already decided whether this root is pointer-eligible. The router owns
        // all remaining per-node policy and stages at most one event for the update traversal.
        self.input_router.begin_input_event(pointer_input_enabled, event);
    }

    /// Clears focus, hover, capture, and queued input while preserving retained node state.
    pub(crate) fn clear_transient_targets(&mut self) {
        self.input_router.clear_transient_targets();
    }

    /// Measures one persistent root node for auto-size without introducing a parallel projection.
    ///
    /// Root chrome owns the conversion from application constraints to the final outer extent.
    pub(crate) fn measure_tree_root(&mut self, root: &mut Node, style: &Style, atlas: &crate::AtlasHandle, constraints: Constraints) -> Dimensioni {
        root.synchronize_measurement_invalidation();
        self.measure_node(root, style, atlas, constraints)
    }

    /// Lays out one persistent root node at its authoritative screen-space rectangle.
    pub(crate) fn layout_tree_root(&mut self, root: &mut Node, style: &Style, atlas: crate::AtlasHandle, outer: Recti, viewport: Recti) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.tree_layouts += 1);
        root.synchronize_measurement_invalidation();
        // Root layout establishes the transform reused by subsequent routing, update, and paint.
        self.root_transform = Transform::root(viewport);
        self.layout_node_ref(root, style, &atlas, outer);
        // Cache only derived geometry; the persistent Node remains the authoritative tree.
        self.root_content_size = root.state.layout.content_size;
        // Layout may hide or remove the current transient target, so validate identities now.
        self.input_router.sanitize_transient_targets(std::slice::from_mut(root), self.root_transform);
    }

    /// Updates one persistent root node and its eligible descendants.
    pub(crate) fn update_tree_root(&mut self, root: &mut Node, style: &Style, atlas: crate::AtlasHandle, input: InputSnapshot) {
        // Update is parent-first and consumes at most one event previously routed to one identity.
        self.update_node_ref(root, self.root_transform, style, atlas, input);
        // Topology may change during update; remove identities whose retained path no longer
        // participates before the next event is routed.
        self.input_router.sanitize_transient_targets(std::slice::from_mut(root), self.root_transform);
    }

    /// Paints one persistent root node and its eligible descendants.
    pub(crate) fn paint_tree_root(&mut self, root: &mut Node, display_list: &mut DisplayList, style: &Style, atlas: crate::AtlasHandle) {
        self.paint_node_ref(root, self.root_transform, display_list, style, atlas);
    }

    /// Returns the aggregate root content size from the most recent layout.
    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self) -> Dimensioni {
        self.root_content_size
    }

    /// Returns structural phase counters since the most recent explicit update began.
    #[cfg(test)]
    pub(crate) fn debug_metrics(&self) -> RuntimeMetrics {
        // Preserve the aggregate diagnostic while each subsystem owns and resets its own counter.
        let mut metrics = self.metrics.get();
        metrics.routed_input_routes = self.input_router.debug_routed_input_routes();
        metrics
    }

    /// Applies one mutation to the retained traversal counters without exposing interior mutability.
    #[cfg(test)]
    fn bump_metric(&self, update: impl FnOnce(&mut RuntimeMetrics)) {
        let mut metrics = self.metrics.get();
        update(&mut metrics);
        self.metrics.set(metrics);
    }

    /// Returns whether this runtime accepts pointer hit routing for the current event.
    pub(crate) fn accepts_pointer_input(&self) -> bool {
        self.input_router.accepts_pointer_input()
    }

    /// Returns whether this retained tree currently owns pointer capture.
    pub(crate) fn has_pointer_capture(&self) -> bool {
        self.input_router.has_capture()
    }

    /// Routes drag/release continuation to the current capture owner, when one remains valid.
    pub(crate) fn route_captured_pointer_input_event(
        &mut self,
        roots: &mut [Node],
        style: &Style,
        mouse_buttons: MouseButton,
        event: &UiInputEvent,
    ) -> Option<bool> {
        // Supply committed layout state explicitly; InputRouter owns no measurement or transform.
        self.input_router
            .route_captured_pointer_input_event(roots, self.root_transform, style, mouse_buttons, event)
    }

    /// Routes keyboard or text input directly to the current valid focus owner.
    pub(crate) fn route_focus_input_event(&mut self, roots: &mut [Node], style: &Style, event: &UiInputEvent) -> bool {
        self.input_router.route_focus_input_event(roots, self.root_transform, style, event)
    }

    /// Routes a pointer event through root chrome or the deepest topmost application node.
    pub(crate) fn route_root_input_event_to_node_ref(
        &mut self,
        node: &mut Node,
        style: &Style,
        event: &UiInputEvent,
        root_chrome_hit: bool,
    ) -> Option<(RuntimeNodeId, input_router::RouteResult)> {
        self.input_router
            .route_root_input_event_to_node_ref(node, self.root_transform, style, event, root_chrome_hit)
    }

    /// Commits pointer-capture ownership after the selected target has classified an event.
    pub(crate) fn update_pointer_capture(&mut self, owner: RuntimeNodeId, result: input_router::RouteResult, event: &UiInputEvent, mouse_buttons: MouseButton) {
        self.input_router.update_pointer_capture(owner, result, event, mouse_buttons);
    }

    /// Routes one ordinary pointer hit for focused retained-runtime tests.
    #[cfg(test)]
    pub(crate) fn route_input_event_to_node_ref(&mut self, node: &mut Node, style: &Style, event: &UiInputEvent) -> Option<(RuntimeNodeId, RouteResult)> {
        self.input_router.route_input_event_to_node_ref(node, self.root_transform, style, event)
    }

    /// Records a synthetic routed event for invalidation tests.
    #[cfg(test)]
    pub(crate) fn push_routed_event(&mut self, node: RuntimeNodeId, event: UiInputEvent) {
        self.input_router.push_routed_event(node, event);
    }

    /// Attempts to consume a synthetic routed event from one identity in tests.
    #[cfg(test)]
    pub(crate) fn take_routed_event(&mut self, node: RuntimeNodeId) -> Option<UiInputEvent> {
        self.input_router.take_routed_event(node)
    }

    /// Returns the router's focused identity for integration tests.
    #[cfg(test)]
    pub(crate) fn debug_focus_target(&self) -> Option<RuntimeNodeId> {
        self.input_router.debug_focus_target()
    }

    /// Returns the router's hovered identity for integration tests.
    #[cfg(test)]
    pub(crate) fn debug_hover_target(&self) -> Option<RuntimeNodeId> {
        self.input_router.debug_hover_target()
    }

    /// Returns the router's capture identity for integration tests.
    #[cfg(test)]
    pub(crate) fn debug_capture_target(&self) -> Option<RuntimeNodeId> {
        self.input_router.debug_capture_target()
    }

    /// Installs explicit router targets for topology-invalidation tests.
    #[cfg(test)]
    pub(crate) fn debug_set_transient_targets(&mut self, focus: Option<RuntimeNodeId>, hover: Option<RuntimeNodeId>, capture: Option<RuntimeNodeId>) {
        self.input_router.debug_set_transient_targets(focus, hover, capture);
    }

    /// Installs one capture owner for gesture-transition tests.
    #[cfg(test)]
    pub(crate) fn debug_set_capture_target(&mut self, capture: Option<RuntimeNodeId>) {
        self.input_router.debug_set_capture_target(capture);
    }

    /// Reports whether routing is suppressing a revoked gesture's remaining events in tests.
    #[cfg(test)]
    pub(crate) fn debug_discards_invalidated_capture_events(&self) -> bool {
        self.input_router.debug_discards_invalidated_capture_events()
    }

    /// Returns the current full rectangle for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_rect(&self, roots: &[Node], id: RuntimeNodeId) -> Option<Recti> {
        roots.iter().find_map(|root| Self::debug_node_rect_from(root, id, self.root_transform))
    }

    #[cfg(test)]
    fn debug_node_rect_from(current: &Node, target: RuntimeNodeId, parent: Transform) -> Option<Recti> {
        if current.id() == target {
            return Some(parent.resolve(current.state.layout.allocation));
        }
        let child_parent = parent.push(current.state.layout);
        current.with_children(|children| children.iter().find_map(|child| Self::debug_node_rect_from(child, target, child_parent)))
    }
}

/// Returns whether a node participates in traversal through every ancestor visibility gate.
fn contains_active_node_in(roots: &[Node], id: RuntimeNodeId, root_transform: Transform) -> bool {
    roots.iter().any(|root| contains_active_node(root, id, root_transform))
}

/// Searches one retained branch while enforcing every layout participation and clip gate.
fn contains_active_node(node: &Node, id: RuntimeNodeId, parent_transform: Transform) -> bool {
    // A disabled or hidden child filters its complete subtree from router-owned identities.
    // Roots use the default active value, so the same predicate is valid at every depth.
    if !node.state.participation.accepts_input() || !node.intersects_clip(parent_transform) {
        return false;
    }
    if node.id() == id {
        return true;
    }
    if !node_children_visible(node) {
        return false;
    }
    let child_transform = parent_transform.push(node.state.layout);
    node.with_children(|children| {
        children
            .iter()
            .filter(|child| child.intersects_clip(child_transform))
            .any(|child| contains_active_node(child, id, child_transform))
    })
}

/// Returns whether this node kind can expose retained descendants to any runtime phase.
fn node_children_visible(node: &Node) -> bool {
    // Only Container can own children; participation is checked separately by the caller.
    node.is_container()
}

/// Returns whether parent layout allows update and paint traversal through this node.
fn node_is_visible(node: &Node) -> bool {
    node.state.participation.is_visible()
}

/// Returns whether parent layout allows the input router to enter this node's subtree.
fn node_accepts_input(node: &Node) -> bool {
    node.state.participation.accepts_input()
}

/// Resolves whether the node's current widget options request shared frame geometry.
fn node_is_framed(node: &Node) -> bool {
    // Dynamic widget options are authoritative because surfaces can enable/disable behavior.
    node.data.with_widget(|widget| widget.effective_widget_opt().intersects(WidgetOption::FRAME))
}

/// Resolves effective event options and focus behavior for the current update pass.
fn node_interaction_config(node: &Node) -> (WidgetOption, FocusPolicy) {
    let (mut opt, focus_policy) = node.data.with_widget(|widget| (widget.effective_widget_opt(), widget.focus_policy()));
    if !node_accepts_input(node) {
        // Keep layout eligibility authoritative without mutating the concrete widget's options.
        opt |= WidgetOption::NO_INTERACT;
    }
    (opt, focus_policy)
}

#[cfg(test)]
mod tests;
