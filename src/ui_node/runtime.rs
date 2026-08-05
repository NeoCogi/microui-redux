//! Authoritative retained-tree runtime state and lifecycle orchestration.

use super::*;

mod layout;
mod paint;
mod routing;
mod update;

use crate::input::InputSnapshot;
use crate::render::DisplayList;
use crate::{Dimensioni, MouseButton, Recti, Style, UNCLIPPED_RECT, Vec2i};
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
    pub(crate) routed_input_dispatches: u64,
}

pub(crate) struct UiRuntime {
    /// Aggregate root content size in root body coordinates.
    root_content_size: Dimensioni,
    /// Transform from root body coordinates into screen coordinates.
    root_transform: Transform,
    /// Focused node.
    pub(crate) focus: Option<RuntimeNodeId>,
    /// Hovered node.
    pub(crate) hover: Option<RuntimeNodeId>,
    /// Pointer-capturing node.
    pub(crate) capture: Option<RuntimeNodeId>,
    /// Capture released by routing whose loss hook runs after that event's update traversal.
    capture_loss_after_update: Option<RuntimeNodeId>,
    /// Whether drag/release events from an externally invalidated capture must be discarded.
    discard_invalidated_capture_events: bool,
    /// Whether this runtime accepts pointer routing for the current event.
    pub(super) pointer_input_enabled: bool,
    /// Whether the current update was initiated by a pointer event.
    pointer_event_active: bool,
    /// Whether the current event releases pointer buttons.
    pointer_release_active: bool,
    /// Node receiving the current click transition.
    clicked: Option<RuntimeNodeId>,
    /// The current event and its sole routed recipient.
    routed_event: Option<(RuntimeNodeId, UiInputEvent)>,
    /// Structural phase counters used by P0/P5 characterization.
    #[cfg(test)]
    metrics: Cell<RuntimeMetrics>,
}

impl Default for UiRuntime {
    fn default() -> Self {
        Self {
            root_content_size: Dimensioni::default(),
            root_transform: Transform::root(UNCLIPPED_RECT),
            focus: None,
            hover: None,
            capture: None,
            capture_loss_after_update: None,
            discard_invalidated_capture_events: false,
            pointer_input_enabled: false,
            pointer_event_active: false,
            pointer_release_active: false,
            clicked: None,
            routed_event: None,
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
        // These values describe one event transaction and must never leak into the next update.
        debug_assert!(self.capture_loss_after_update.is_none(), "pointer-capture loss was not delivered after update");
        self.routed_event = None;
        self.clicked = None;
        self.pointer_event_active = false;
        self.pointer_release_active = false;
        #[cfg(test)]
        self.metrics.set(RuntimeMetrics::default());
    }

    /// Starts exactly one full-tree update for one normalized input event.
    pub(crate) fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        // Routing must have consumed the prior event and completed deferred capture cleanup before
        // a new normalized event can establish its transaction flags.
        debug_assert!(self.routed_event.is_none(), "the previous routed event was not consumed by update");
        debug_assert!(self.capture_loss_after_update.is_none(), "pointer-capture loss was not delivered after update");
        self.pointer_input_enabled = pointer_input_enabled;
        self.pointer_event_active = event.is_pointer();
        self.pointer_release_active = event.is_pointer_release();
        self.clicked = None;
        if self.pointer_event_active {
            // Pointer routing recomputes hover from current committed geometry for every event.
            self.hover = None;
        }
        if matches!(event, UiInputEvent::MouseDown { .. }) {
            // A press starts a new focus claim; the selected recipient assigns focus during routing.
            self.focus = None;
        }
    }

    /// Clears focus, hover, capture, and queued input while preserving retained node state.
    pub(crate) fn clear_transient_targets(&mut self, roots: &mut [Node]) {
        // Clear scalar targets first, then notify any capture owner while the tree is still present.
        self.focus = None;
        self.hover = None;
        self.routed_event = None;
        self.clicked = None;
        self.clear_all_pointer_capture(roots);
    }

    /// Measures one persistent root node for auto-size without introducing a parallel projection.
    ///
    /// A zero component requests unconstrained preferred size; a positive component supplies the
    /// programmed measurement bound. Root chrome owns the conversion from application content to
    /// the final outer window extent.
    pub(crate) fn measure_tree_root(&self, root: &Node, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        self.measure_node(root, style, atlas, available)
    }

    /// Lays out one persistent root node at its authoritative screen-space rectangle.
    pub(crate) fn layout_tree_root(&mut self, root: &mut Node, style: &Style, atlas: crate::AtlasHandle, outer: Recti, viewport: Recti) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.tree_layouts += 1);
        // Root layout establishes the transform reused by subsequent routing, update, and paint.
        self.root_transform = Transform::root(viewport);
        self.layout_allocated_node_ref(root, style, &atlas, outer);
        // Cache only derived geometry; the persistent Node remains the authoritative tree.
        self.root_content_size = root.state.layout.content_size;
        // Layout may hide or remove the current transient target, so validate identities now.
        self.sanitize_transient_targets(std::slice::from_mut(root));
    }

    /// Updates one persistent root node and its eligible descendants.
    pub(crate) fn update_tree_root(&mut self, root: &mut Node, style: &Style, atlas: crate::AtlasHandle, input: InputSnapshot) {
        // Update is parent-first and consumes at most one event previously routed to one identity.
        self.update_node_ref(root, self.root_transform, style, atlas, input);
        let roots = std::slice::from_mut(root);
        // A gated target may not have been visited to receive deferred loss; finish it explicitly.
        self.flush_capture_loss(roots);
        self.sanitize_transient_targets(roots);
    }

    /// Paints one persistent root node and its eligible descendants.
    pub(crate) fn paint_tree_root(&mut self, root: &mut Node, display_list: &mut DisplayList, style: &Style, atlas: crate::AtlasHandle) {
        self.paint_node_ref(root, self.root_transform, display_list, style, atlas);
    }

    /// Records one routed event for a node-local widget update.
    pub(crate) fn push_routed_event(&mut self, node: RuntimeNodeId, event: UiInputEvent) {
        // The dispatcher preselects one recipient. Multiple queued recipients would reintroduce
        // handler-dependent hit testing, so enforce the single-recipient invariant here.
        debug_assert!(self.routed_event.is_none(), "one input event was routed to more than one recipient");
        self.routed_event = Some((node, event));
    }

    /// Assigns focus and the one-event clicked marker to a routed pointer-down recipient.
    pub(crate) fn claim_pointer_focus(&mut self, node: RuntimeNodeId, button: MouseButton) {
        self.focus = Some(node);
        if button.intersects(MouseButton::LEFT) {
            self.clicked = Some(node);
        }
    }

    /// Takes the current routed event if this node is its sole recipient.
    pub(crate) fn take_routed_event(&mut self, node: RuntimeNodeId) -> Option<UiInputEvent> {
        // Leave the event intact while unrelated nodes update; only its selected identity may take it.
        if self.routed_event.as_ref().is_some_and(|(recipient, _)| *recipient == node) {
            self.routed_event.take().map(|(_, event)| event)
        } else {
            None
        }
    }

    /// Returns the aggregate root content size from the most recent layout.
    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self) -> Dimensioni {
        self.root_content_size
    }

    /// Returns structural phase counters since the most recent explicit update began.
    #[cfg(test)]
    pub(crate) fn debug_metrics(&self) -> RuntimeMetrics {
        self.metrics.get()
    }

    #[cfg(test)]
    fn bump_metric(&self, update: impl FnOnce(&mut RuntimeMetrics)) {
        let mut metrics = self.metrics.get();
        update(&mut metrics);
        self.metrics.set(metrics);
    }

    /// Returns whether this runtime accepts pointer hit routing for the current event.
    pub(crate) fn accepts_pointer_input(&self) -> bool {
        self.pointer_input_enabled
    }

    /// Returns the current root-body transform.
    pub(crate) fn root_transform(&self) -> Transform {
        self.root_transform
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

/// Finds one retained node across independent roots and runs a scoped mutable callback.
fn with_node_mut<R>(roots: &mut [Node], id: RuntimeNodeId, f: impl FnOnce(&mut Node) -> R) -> Option<R> {
    // Determine the root index immutably, then open one mutable path into that root.
    let index = roots.iter().position(|root| root.with_node(id, |_| ()).is_some())?;
    roots[index].with_node_mut(id, f)
}

/// Notifies a still-retained target that dispatcher-owned pointer capture ended.
fn notify_pointer_capture_lost(roots: &mut [Node], id: RuntimeNodeId) {
    // Removal is legal before notification; absence simply means the dropped widget needs no hook.
    let _ = with_node_mut(roots, id, |node| {
        node.data.widget_mut().pointer_capture_lost();
    });
}

/// Returns whether a node participates in traversal through every ancestor visibility gate.
fn contains_active_node_in(roots: &[Node], id: RuntimeNodeId) -> bool {
    roots.iter().any(|root| contains_active_node(root, id))
}

fn contains_active_node(node: &Node, id: RuntimeNodeId) -> bool {
    // A disabled or hidden child filters its complete subtree from dispatcher-owned identities.
    // Roots use the default active value, so the same predicate is valid at every depth.
    if !node.state.participation.accepts_input() {
        return false;
    }
    if node.id() == id {
        return true;
    }
    if !node_children_visible(node) {
        return false;
    }
    node.with_children(|children| children.iter().any(|child| contains_active_node(child, id)))
}

fn node_children_visible(node: &Node) -> bool {
    // Only Container can own children; participation is checked separately by the caller.
    node.is_container()
}

/// Returns whether parent layout allows update and paint traversal through this node.
fn node_is_visible(node: &Node) -> bool {
    node.state.participation.is_visible()
}

/// Returns whether parent layout allows the dispatcher to enter this node's subtree.
fn node_accepts_input(node: &Node) -> bool {
    node.state.participation.accepts_input()
}

fn node_is_framed(node: &Node) -> bool {
    // Dynamic widget options are authoritative because surfaces can enable/disable behavior.
    node.data.widget().effective_widget_opt().intersects(WidgetOption::FRAME)
}

/// Resolves effective event options and focus behavior for the current update pass.
fn node_interaction_config(node: &Node) -> (WidgetOption, FocusPolicy) {
    let widget = node.data.widget();
    let mut opt = widget.effective_widget_opt();
    if !node_accepts_input(node) {
        // Keep layout eligibility authoritative without mutating the concrete widget's options.
        opt |= WidgetOption::NO_INTERACT;
    }
    (opt, widget.focus_policy())
}

/// Converts a screen-space rectangle into coordinates relative to `origin`.
fn rect_relative_to(rect: Recti, origin: Vec2i) -> Recti {
    Recti::new(rect.x - origin.x, rect.y - origin.y, rect.width, rect.height)
}

/// Converts a local rectangle into screen coordinates relative to `origin`.
fn translate_local_rect(rect: Recti, origin: Vec2i) -> Recti {
    Recti::new(rect.x + origin.x, rect.y + origin.y, rect.width, rect.height)
}

#[cfg(test)]
mod tests;
