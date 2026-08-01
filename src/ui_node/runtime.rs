use super::*;
use crate::input::InputSnapshot;
use crate::{MouseButton, Vec2i};
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
    /// Snapshot of text operations from the most recently recorded display list.
    #[cfg(test)]
    debug_texts: Vec<String>,
    /// Snapshot of rectangle operations from the most recently recorded display list.
    #[cfg(test)]
    debug_rects: Vec<Recti>,
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
            #[cfg(test)]
            debug_texts: Vec::new(),
            #[cfg(test)]
            debug_rects: Vec::new(),
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

    /// Moves focus to a node in this runtime.
    pub(crate) fn set_focus_node(&mut self, roots: &[Node], node: RuntimeNodeId) {
        if contains_active_node_in(roots, node) {
            self.focus = Some(node);
        }
    }

    /// Measures the outer root size needed for `AUTO_SIZE` node roots.
    pub(crate) fn measure_auto_size(&self, roots: &[Node], style: &Style, atlas: &crate::AtlasHandle, opt: WindowOption, min_width: i32) -> Dimensioni {
        let title_height = if opt.intersects(WindowOption::NO_TITLE) {
            0
        } else {
            root_titlebar_height(style, atlas)
        };
        let padding = style.padding.max(0);
        let border_width = if opt.intersects(WindowOption::FRAME) { style.frame_border().width } else { 0 };
        let border_extent = border_width.checked_mul(2).expect("root frame extent overflowed i32");
        let horizontal_padding = padding.saturating_mul(2);
        let available = Dimensioni::new(min_width.saturating_sub(border_extent).saturating_sub(horizontal_padding).max(1), 10_000);
        let mut width: i32 = 0;
        let mut height: i32 = 0;
        for (index, root) in roots.iter().enumerate() {
            let preferred = self.measure_node(root, style, atlas, available).resolved_outer;
            width = width.max(preferred.width);
            height = height.saturating_add(preferred.height);
            if index + 1 < roots.len() {
                height = height.saturating_add(style.spacing);
            }
        }
        let outer_width = width
            .saturating_add(horizontal_padding)
            .checked_add(border_extent)
            .expect("auto-sized root width overflowed i32");
        let outer_height = height
            .saturating_add(padding.saturating_mul(2))
            .saturating_add(title_height)
            .checked_add(border_extent)
            .expect("auto-sized root height overflowed i32");
        Dimensioni::new(outer_width.max(min_width).max(1), outer_height.max(1))
    }

    /// Clears update-cycle metrics before the initial synchronization layout.
    pub(crate) fn begin_update(&mut self) {
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
        debug_assert!(self.routed_event.is_none(), "the previous routed event was not consumed by update");
        debug_assert!(self.capture_loss_after_update.is_none(), "pointer-capture loss was not delivered after update");
        self.pointer_input_enabled = pointer_input_enabled;
        self.pointer_event_active = event.is_pointer();
        self.pointer_release_active = event.is_pointer_release();
        self.clicked = None;
        if self.pointer_event_active {
            self.hover = None;
        }
        if matches!(event, UiInputEvent::MouseDown { .. }) {
            self.focus = None;
        }
    }

    /// Clears focus, hover, capture, and queued input while preserving retained node state.
    pub(crate) fn clear_transient_targets(&mut self, roots: &mut [Node]) {
        self.focus = None;
        self.hover = None;
        self.routed_event = None;
        self.clicked = None;
        self.clear_all_pointer_capture(roots);
    }

    /// Measures one persistent root node without introducing a parallel root projection.
    pub(crate) fn measure_tree_root(&self, root: &Node, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        self.measure_node(root, style, atlas, available).resolved_outer
    }

    /// Lays out one persistent root node at its authoritative screen-space rectangle.
    pub(crate) fn layout_tree_root(&mut self, root: &mut Node, style: &Style, atlas: crate::AtlasHandle, outer: Recti, viewport: Recti) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.tree_layouts += 1);
        self.root_transform = Transform::root(viewport);
        self.layout_allocated_node_ref(root, style, &atlas, outer);
        self.root_content_size = root.state.layout.content_size;
        self.sanitize_transient_targets(std::slice::from_mut(root));
    }

    /// Updates one persistent root node and its eligible descendants.
    pub(crate) fn update_tree_root(&mut self, root: &mut Node, style: &Style, atlas: crate::AtlasHandle, input: InputSnapshot) {
        self.update_node_ref(root, self.root_transform, style, atlas, input);
        let roots = std::slice::from_mut(root);
        self.flush_capture_loss(roots);
        self.sanitize_transient_targets(roots);
    }

    /// Paints one persistent root node and its eligible descendants.
    pub(crate) fn paint_tree_root(&mut self, root: &mut Node, display_list: &mut DisplayList, style: &Style, atlas: crate::AtlasHandle) {
        self.paint_node_ref(root, self.root_transform, display_list, style, atlas);
        #[cfg(test)]
        {
            self.debug_texts = display_list.debug_texts();
            self.debug_rects = display_list.debug_rects();
        }
    }

    /// Records one routed event for a node-local widget update.
    pub(crate) fn push_routed_event(&mut self, node: RuntimeNodeId, event: UiInputEvent) {
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
        if self.routed_event.as_ref().is_some_and(|(recipient, _)| *recipient == node) {
            self.routed_event.take().map(|(_, event)| event)
        } else {
            None
        }
    }

    /// Clears interaction state that points at a removed or container-gated descendant.
    ///
    /// Sanitization runs at layout/update boundaries and before direct target delivery, when no
    /// container state borrow is active. A replacement node cannot inherit a stale target because
    /// every owning node has a fresh ID.
    fn sanitize_transient_targets(&mut self, roots: &mut [Node]) {
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
    fn clear_all_pointer_capture(&mut self, roots: &mut [Node]) {
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
    fn finish_pointer_capture_update(&mut self, node: &mut Node) {
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
    fn flush_capture_loss(&mut self, roots: &mut [Node]) {
        if let Some(owner) = self.capture_loss_after_update.take()
            && self.capture != Some(owner)
        {
            notify_pointer_capture_lost(roots, owner);
        }
    }

    /// Returns text commands recorded by the most recent frame.
    #[cfg(test)]
    pub(crate) fn debug_texts(&self) -> &[String] {
        &self.debug_texts
    }

    /// Returns rectangle commands recorded by the most recent frame.
    #[cfg(test)]
    pub(crate) fn debug_rects(&self) -> &[Recti] {
        &self.debug_rects
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
        with_node(roots, id, |node| {
            self.parent_transform_for_node(roots, id).resolve(node.state.layout.allocation)
        })
    }

    /// Returns a node-local rectangle in screen coordinates for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_local_rect(&self, roots: &[Node], id: RuntimeNodeId, rect: Recti) -> Option<Recti> {
        with_node(roots, id, |node| {
            let screen_rect = self.parent_transform_for_node(roots, id).resolve(node.state.layout.allocation);
            Recti::new(screen_rect.x + rect.x, screen_rect.y + rect.y, rect.width, rect.height)
        })
    }

    /// Returns whether a node exists in this runtime.
    pub(crate) fn contains_node(&self, roots: &[Node], id: RuntimeNodeId) -> bool {
        contains_node_in(roots, id)
    }

    /// Runs test/debug work against a matching node without exposing an attached borrow publicly.
    #[cfg(test)]
    pub(crate) fn with_node<R>(&self, roots: &[Node], id: RuntimeNodeId, f: impl FnOnce(&Node) -> R) -> Option<R> {
        with_node(roots, id, f)
    }

    /// Finds the current parent of `child` by walking root/container child membership.
    pub(super) fn parent_of(&self, roots: &[Node], child: RuntimeNodeId) -> Option<RuntimeNodeId> {
        for root in roots {
            if let Some(parent) = Self::parent_of_from(root, child) {
                return Some(parent);
            }
        }
        None
    }

    fn parent_of_from(current: &Node, child: RuntimeNodeId) -> Option<RuntimeNodeId> {
        current.with_children(|children| {
            if children.iter().any(|node| node.id() == child) {
                return Some(current.id());
            }
            children.iter().find_map(|descendant| Self::parent_of_from(descendant, child))
        })
    }

    /// Measures one already-borrowed node through the authoritative private node path.
    fn measure_node(&self, node: &Node, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> NodeMeasurement {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.measures += 1);
        node.measure(style, atlas, available)
    }

    /// Lays out one already-borrowed node through direct widget/container dispatch.
    pub(super) fn layout_node_ref(&mut self, node: &mut Node, style: &Style, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.layouts += 1);
        let framed = node_is_framed(node);
        let measurement = self.measure_node(node, style, atlas, Dimensioni::new(rect.width, rect.height));
        let preferred = measurement.resolved_outer;
        let policy = node.state.policy;
        let outer = Recti::new(
            rect.x,
            rect.y,
            resolve_allocated_size(policy.width, preferred.width, rect.width, rect.width, None),
            resolve_allocated_size(policy.height, preferred.height, rect.height, rect.height, None),
        );
        self.layout_node_outer_ref(node, style, atlas, framed, outer, measurement)
    }

    /// Lays out a node whose parent/root flow has already resolved its size policy.
    fn layout_allocated_node_ref(&mut self, node: &mut Node, style: &Style, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.layouts += 1);
        let framed = node_is_framed(node);
        // Preserve the established measure/layout phase contract while keeping the resolved root
        // allocation authoritative.
        let measurement = self.measure_node(node, style, atlas, Dimensioni::new(rect.width, rect.height));
        let outer = Recti::new(rect.x, rect.y, rect.width.max(0), rect.height.max(0));
        self.layout_node_outer_ref(node, style, atlas, framed, outer, measurement)
    }

    /// Applies frame/content geometry and delegates layout for one resolved outer allocation.
    fn layout_node_outer_ref(
        &mut self,
        node: &mut Node,
        style: &Style,
        atlas: &crate::AtlasHandle,
        framed: bool,
        outer: Recti,
        measurement: NodeMeasurement,
    ) -> Dimensioni {
        let local_outer = Recti::new(0, 0, outer.width, outer.height);
        let frame_geometry = crate::frame::frame_geometry(local_outer, framed, style);
        let content = frame_geometry.content_or_empty();
        let is_branch = node.is_container();
        node.set_layout(NodeLayout::from_parts(outer, content, Dimensioni::new(outer.width.max(0), outer.height.max(0))));

        match &mut node.data {
            NodeKind::Widget(_) => {
                let content_size = Dimensioni::new(
                    outer.width.max(measurement.preferred_outer.width).max(0),
                    outer.height.max(measurement.preferred_outer.height).max(0),
                );
                node.state.set_layout(node.state.layout.with_content_size(content_size));
            }
            NodeKind::Container(container) => {
                let mut ctx = ContainerLayoutCtx::new(self, style, atlas, content, &mut node.state);
                container.layout(&mut ctx, content);
            }
        }

        node.state.layout.allocation = outer;
        node.state.layout.children.clip = node
            .state
            .layout
            .children
            .clip
            .intersect(&content)
            .unwrap_or_else(|| Recti::new(content.x, content.y, 0, 0));

        let propagate_child_overflow = node.state.layout.propagate_child_overflow;
        if is_branch && node_children_visible(node) && propagate_child_overflow {
            let content_rect = node.with_children(child_content_bounds_from_children).unwrap_or(content);
            let content_size = Dimensioni::new(
                (content_rect.x + content_rect.width).max(outer.width).max(0),
                (content_rect.y + content_rect.height).max(outer.height).max(0),
            );
            node.set_layout(node.state.layout.with_content_size(content_size));
        }
        Dimensioni::new(outer.width, outer.height)
    }

    /// Updates one already-borrowed node and descendants.
    pub(super) fn update_node_ref(&mut self, node: &mut Node, parent_transform: Transform, style: &Style, atlas: crate::AtlasHandle, input: InputSnapshot) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.updates += 1);
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let frame_geometry = crate::frame::frame_geometry(local_rect, framed, style);
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
            .map(|event| crate::widget_ctx::localize_event(Vec2i::new(content_rect.x, content_rect.y), event));
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
                for child in children {
                    // Update recursion carries interaction and layout state only. Rendering enters
                    // the tree later through the distinct paint traversal below.
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
        let parent_transform = self.parent_transform_for_node(roots, capture);
        let result = self.route_input_event_to_node_only(roots, capture, parent_transform, style, event);
        self.update_pointer_capture(capture, result, event, mouse_buttons);
        Some(result.is_consumed())
    }

    /// Routes keyboard/text input to the focused node only.
    fn route_focus_input_event_to_target(&mut self, roots: &mut [Node], style: &Style, event: &UiInputEvent) -> bool {
        let Some(focus) = self.focus.filter(|id| contains_active_node_in(roots, *id)) else {
            return false;
        };
        let parent_transform = self.parent_transform_for_node(roots, focus);
        self.route_input_event_to_node_only(roots, focus, parent_transform, style, event).is_consumed()
    }

    /// Applies runtime pointer-capture ownership from one routed event result.
    pub(crate) fn update_pointer_capture(&mut self, owner: RuntimeNodeId, result: InputResult, event: &UiInputEvent, mouse_buttons: MouseButton) {
        if event.is_pointer_release() && mouse_buttons.is_empty() {
            self.defer_current_pointer_capture_loss();
        } else if result == InputResult::Captured {
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
    ) -> Option<(RuntimeNodeId, InputResult)> {
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

    /// Routes an event to exactly one node without traversing descendants.
    fn route_input_event_to_node_only(
        &mut self,
        roots: &mut [Node],
        id: RuntimeNodeId,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> InputResult {
        with_node_mut(roots, id, |node| self.route_input_event_to_node_only_ref(node, parent_transform, style, event)).unwrap_or(InputResult::Ignored)
    }

    /// Routes an event to exactly one borrowed node without traversing descendants.
    fn route_input_event_to_node_only_ref(&mut self, node: &mut Node, parent_transform: Transform, style: &Style, event: &UiInputEvent) -> InputResult {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.routed_input_dispatches += 1);
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let content_rect = crate::frame::frame_geometry(local_rect, framed, style).content_or_empty();
        let screen_clip = parent_transform.clip.intersect(&screen_rect).unwrap_or_default();
        let local_clip = rect_relative_to(screen_clip, screen_origin);
        let content_clip = local_clip
            .intersect(&content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));
        let local_event = crate::widget_ctx::localize_event(screen_origin, event.clone());

        match &mut node.data {
            NodeKind::Widget(widget) => {
                let opt = widget.widget.effective_widget_opt();
                super::containers::route_public_widget_input(self, &node.state, local_rect, local_clip, opt, &local_event)
            }
            NodeKind::Container(container) => {
                let mut ctx = ContainerInputCtx::new(self, content_rect, content_clip, &node.state);
                container.route_input(&mut ctx, &local_event)
            }
        }
    }

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
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let frame_geometry = crate::frame::frame_geometry(local_rect, framed, style);
        let content_rect = frame_geometry.content_or_empty();
        let screen_clip = parent_transform.clip.intersect(&screen_rect).unwrap_or_default();
        if framed {
            let mut painter = crate::render::Painter::screen_space(display_list, screen_clip);
            crate::frame::paint_internal_frame(&mut painter, screen_rect, None, style.frame_border());
        }
        let child_transform = parent_transform.push(node.state.layout);
        let local_clip = rect_relative_to(screen_clip, screen_origin);
        let content_clip = local_clip
            .intersect(&content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));
        let screen_content_rect = translate_local_rect(content_rect, screen_origin);
        let screen_content_clip = translate_local_rect(content_clip, screen_origin);
        {
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
            display_list.push_custom(screen_content_clip, renderer, screen_content_rect);
        }

        let traverse_children = node.data.container().is_some_and(Container::children_visible);
        if traverse_children {
            node.with_children_mut(|children| {
                for child in children {
                    self.paint_node_ref(child, child_transform, display_list, style, atlas.clone());
                }
            });
        }
    }

    /// Pushes this node onto a parent transform.
    pub(super) fn node_transform(&self, roots: &[Node], id: RuntimeNodeId, parent: Transform) -> Transform {
        with_node(roots, id, |node| parent.push(node.state.layout)).unwrap_or(parent)
    }

    /// Derives the child transform for one node by walking its parent chain.
    pub(super) fn transform_for_node(&self, roots: &[Node], id: RuntimeNodeId) -> Transform {
        let parent = self.parent_transform_for_node(roots, id);
        self.node_transform(roots, id, parent)
    }

    pub(super) fn parent_transform_for_node(&self, roots: &[Node], id: RuntimeNodeId) -> Transform {
        self.contains_node(roots, id)
            .then(|| self.parent_of(roots, id))
            .flatten()
            .map(|parent| self.transform_for_node(roots, parent))
            .unwrap_or(self.root_transform)
    }
}

fn with_node<R>(roots: &[Node], id: RuntimeNodeId, f: impl FnOnce(&Node) -> R) -> Option<R> {
    let root = roots.iter().find(|root| root.with_node(id, |_| ()).is_some())?;
    root.with_node(id, f)
}

fn with_node_mut<R>(roots: &mut [Node], id: RuntimeNodeId, f: impl FnOnce(&mut Node) -> R) -> Option<R> {
    let index = roots.iter().position(|root| root.with_node(id, |_| ()).is_some())?;
    roots[index].with_node_mut(id, f)
}

fn captured_target_retains_pointer_capture(roots: &[Node], id: RuntimeNodeId) -> bool {
    with_node(roots, id, |node| node.data.container().is_none_or(Container::retains_pointer_capture)).unwrap_or(false)
}

fn notify_pointer_capture_lost(roots: &mut [Node], id: RuntimeNodeId) {
    let _ = with_node_mut(roots, id, |node| {
        if let Some(container) = node.data.container_mut() {
            container.on_pointer_capture_lost();
        }
    });
}

fn contains_node_in(roots: &[Node], id: RuntimeNodeId) -> bool {
    roots.iter().any(|root| root.with_node(id, |_| ()).is_some())
}

/// Returns whether a node participates in traversal through every ancestor visibility gate.
fn contains_active_node_in(roots: &[Node], id: RuntimeNodeId) -> bool {
    roots.iter().any(|root| contains_active_node(root, id))
}

fn contains_active_node(node: &Node, id: RuntimeNodeId) -> bool {
    if node.id() == id {
        return true;
    }
    if !node_children_visible(node) {
        return false;
    }
    node.with_children(|children| children.iter().any(|child| contains_active_node(child, id)))
}

fn node_children_visible(node: &Node) -> bool {
    node.data.container().is_none_or(Container::children_visible)
}

fn node_is_framed(node: &Node) -> bool {
    node.data.widget().effective_widget_opt().intersects(WidgetOption::FRAME)
}

fn node_interaction_config(node: &Node) -> (WidgetOption, FocusPolicy) {
    let widget = node.data.widget();
    (widget.effective_widget_opt(), widget.focus_policy())
}

fn rect_relative_to(rect: Recti, origin: Vec2i) -> Recti {
    Recti::new(rect.x - origin.x, rect.y - origin.y, rect.width, rect.height)
}

fn translate_local_rect(rect: Recti, origin: Vec2i) -> Recti {
    Recti::new(rect.x + origin.x, rect.y + origin.y, rect.width, rect.height)
}

fn child_content_bounds_from_children(children: &[Node]) -> Option<Recti> {
    let mut bounds = None;
    for child in children {
        let child_rect = child_content_rect(child);
        bounds = Some(match bounds {
            Some(rect) => union_rect(rect, child_rect),
            None => child_rect,
        });
    }
    bounds
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;
    use crate::test_support::test_atlas;
    use crate::{
        Children, ChildrenVisitor, ChildrenVisitorMut, ContainerState, Input, Widget, WidgetPaintCtx, WidgetState, WidgetStateHandle, WidgetStateOwner,
        WidgetUpdateCtx,
    };

    #[derive(Default)]
    struct ProbeCounts {
        measures: Cell<usize>,
        updates: Cell<usize>,
        paints: Cell<usize>,
        routed_events: Cell<usize>,
    }

    struct Probe {
        state: Rc<RefCell<()>>,
        name: &'static str,
        counts: Rc<ProbeCounts>,
        log: Rc<RefCell<Vec<String>>>,
        opt: WidgetOption,
    }

    impl Probe {
        fn new(name: &'static str, log: Rc<RefCell<Vec<String>>>) -> (Self, Rc<ProbeCounts>) {
            let counts = Rc::new(ProbeCounts::default());
            (
                Self {
                    state: Rc::new(RefCell::new(())),
                    name,
                    counts: counts.clone(),
                    log,
                    opt: WidgetOption::NONE,
                },
                counts,
            )
        }
    }

    impl WidgetStateOwner for Probe {
        type State = ();

        fn state_handle(&self) -> WidgetStateHandle<Self::State> {
            WidgetStateHandle::new(&self.state)
        }
    }

    impl Widget for Probe {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
            self.counts.measures.set(self.counts.measures.get() + 1);
            Dimensioni::new(17, 13)
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
            self.counts.updates.set(self.counts.updates.get() + 1);
            self.counts.routed_events.set(self.counts.routed_events.get() + usize::from(input.is_some()));
            self.log.borrow_mut().push(format!("{}:update", self.name));
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
            self.counts.paints.set(self.counts.paints.get() + 1);
            self.log.borrow_mut().push(format!("{}:paint", self.name));
        }
    }

    struct TraversalState {
        children: Children,
        visible: bool,
    }

    impl WidgetState for TraversalState {}
    impl ContainerState for TraversalState {}

    struct TraversalContainer {
        state: Rc<RefCell<TraversalState>>,
        hide_during_update: bool,
        log: Rc<RefCell<Vec<String>>>,
        opt: WidgetOption,
    }

    impl TraversalContainer {
        fn new(children: impl IntoIterator<Item = Node>, hide_during_update: bool, log: Rc<RefCell<Vec<String>>>) -> Self {
            Self {
                state: Rc::new(RefCell::new(TraversalState {
                    children: children.into_iter().collect(),
                    visible: true,
                })),
                hide_during_update,
                log,
                opt: WidgetOption::NONE,
            }
        }
    }

    impl WidgetStateOwner for TraversalContainer {
        type State = TraversalState;

        fn state_handle(&self) -> WidgetStateHandle<Self::State> {
            WidgetStateHandle::new(&self.state)
        }
    }

    impl Widget for TraversalContainer {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
            let state = self.state.try_borrow().expect("traversal state must be available during measure");
            (0..state.children.len())
                .filter_map(|index| state.children.measure_child(index, style, atlas, available))
                .fold(Dimensioni::default(), |size, child| {
                    Dimensioni::new(size.width.max(child.width), size.height.max(child.height))
                })
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
            self.log.borrow_mut().push("container:update".to_owned());
            if self.hide_during_update {
                self.state.try_borrow_mut().expect("traversal state must be available during update").visible = false;
            }
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
            self.log.borrow_mut().push("container:paint".to_owned());
        }
    }

    impl Container for TraversalContainer {
        fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
            let state = self.state.try_borrow().expect("traversal state must be available during immutable visitation");
            visitor.visit(&state.children);
        }

        fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
            let mut state = self
                .state
                .try_borrow_mut()
                .expect("traversal state must be available during mutable visitation");
            visitor.visit(&mut state.children);
        }

        fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
            let mut state = self.state.try_borrow_mut().expect("traversal state must be available during layout");
            if state.visible {
                for index in 0..state.children.len() {
                    let _ = ctx.layout_child(&mut state.children, index, rect);
                }
            }
        }

        fn children_visible(&self) -> bool {
            self.state
                .try_borrow()
                .expect("traversal state must be available for the visibility gate")
                .visible
        }
    }

    struct CaptureState {
        children: Children,
        active: bool,
        losses: usize,
        drags: usize,
        saw_capture_during_drag: bool,
    }

    impl WidgetState for CaptureState {}
    impl ContainerState for CaptureState {}

    struct CaptureContainer {
        state: Rc<RefCell<CaptureState>>,
        opt: WidgetOption,
    }

    impl CaptureContainer {
        fn new() -> (Self, Rc<RefCell<CaptureState>>) {
            let state = Rc::new(RefCell::new(CaptureState {
                children: Children::new(),
                active: false,
                losses: 0,
                drags: 0,
                saw_capture_during_drag: false,
            }));
            (
                Self {
                    state: state.clone(),
                    opt: WidgetOption::NONE,
                },
                state,
            )
        }
    }

    impl WidgetStateOwner for CaptureContainer {
        type State = CaptureState;

        fn state_handle(&self) -> WidgetStateHandle<Self::State> {
            WidgetStateHandle::new(&self.state)
        }
    }

    impl Widget for CaptureContainer {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
            Dimensioni::new(20, 20)
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
            let mut state = self.state.try_borrow_mut().expect("capture state must be available during update");
            if let Some(event) = input {
                match event {
                    UiInputEvent::MouseDown { button, .. } if button.intersects(MouseButton::LEFT) => state.active = true,
                    UiInputEvent::MouseDrag { buttons, .. } if buttons.intersects(MouseButton::LEFT) && state.active => state.drags += 1,
                    UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT) => state.active = false,
                    _ => {}
                }
            }
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
    }

    impl Container for CaptureContainer {
        fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
            let state = self.state.try_borrow().expect("capture state must be available during visitation");
            visitor.visit(&state.children);
        }

        fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
            let mut state = self.state.try_borrow_mut().expect("capture state must be available during mutable visitation");
            visitor.visit(&mut state.children);
        }

        fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
            ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
        }

        fn retains_pointer_capture(&self) -> bool {
            self.state.try_borrow().expect("capture state must be available for retention").active
        }

        fn on_pointer_capture_lost(&mut self) {
            let mut state = self.state.try_borrow_mut().expect("capture state must be available for loss notification");
            state.active = false;
            state.losses += 1;
        }

        fn route_input(&mut self, ctx: &mut ContainerInputCtx<'_>, event: &UiInputEvent) -> ContainerInputResult {
            if matches!(event, UiInputEvent::MouseDrag { .. }) && ctx.has_pointer_capture() {
                self.state
                    .try_borrow_mut()
                    .expect("capture state must be available during routing")
                    .saw_capture_during_drag = true;
            }
            ctx.route_widget(event, self.opt, FocusPolicy::DragCapture)
        }
    }

    struct CrossSubtreeRemover {
        state: Rc<RefCell<()>>,
        target: Rc<RefCell<TraversalState>>,
        removed: bool,
        opt: WidgetOption,
    }

    impl WidgetStateOwner for CrossSubtreeRemover {
        type State = ();

        fn state_handle(&self) -> WidgetStateHandle<Self::State> {
            WidgetStateHandle::new(&self.state)
        }
    }

    impl Widget for CrossSubtreeRemover {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
            Dimensioni::new(10, 10)
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
            if !self.removed {
                self.target
                    .try_borrow_mut()
                    .expect("cross-subtree target state must be independently available")
                    .children
                    .clear();
                self.removed = true;
            }
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
    }

    fn layout_root(runtime: &mut UiRuntime, root: &mut Node, style: &Style, atlas: crate::AtlasHandle) {
        runtime.layout_tree_root(root, style, atlas, Recti::new(10, 20, 80, 60), Recti::new(0, 0, 320, 240));
    }

    fn empty_input() -> InputSnapshot {
        Input::default().snapshot()
    }

    fn next_input(input: &mut Input) -> (UiInputEvent, InputSnapshot) {
        let event = input.pop_event().expect("test input must contain one queued event");
        (event, input.snapshot())
    }

    #[test]
    fn leaf_layout_reuses_one_authoritative_widget_measurement() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (probe, counts) = Probe::new("leaf", log);
        let mut root = Node::widget(probe);
        let mut runtime = UiRuntime::new();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &Style::default(), test_atlas());

        assert_eq!(counts.measures.get(), 1);
        assert_eq!(runtime.debug_metrics().measures, 1);
        assert_eq!((root.state.layout.content_size.width, root.state.layout.content_size.height), (80, 60));
    }

    #[test]
    fn common_phases_are_parent_first_and_siblings_are_forward() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (first, first_counts) = Probe::new("first", log.clone());
        let (second, second_counts) = Probe::new("second", log.clone());
        let container = TraversalContainer::new([Node::widget(first), Node::widget(second)], false, log.clone());
        let mut root = Node::container(container);
        let mut runtime = UiRuntime::new();
        let style = Style::default();
        let atlas = test_atlas();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, atlas.clone());
        log.borrow_mut().clear();
        runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
        runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas);

        let expected = [
            "container:update",
            "first:update",
            "second:update",
            "container:paint",
            "first:paint",
            "second:paint",
        ]
        .map(str::to_owned);
        assert_eq!(log.borrow().as_slice(), expected.as_slice());
        assert_eq!((first_counts.updates.get(), first_counts.paints.get()), (1, 1));
        assert_eq!((second_counts.updates.get(), second_counts.paints.get()), (1, 1));
    }

    #[test]
    fn post_update_visibility_gate_suppresses_descendants_in_the_same_frame() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (child, child_counts) = Probe::new("child", log.clone());
        let container = TraversalContainer::new([Node::widget(child)], true, log.clone());
        let mut root = Node::container(container);
        let mut runtime = UiRuntime::new();
        let style = Style::default();
        let atlas = test_atlas();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, atlas.clone());
        log.borrow_mut().clear();
        runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
        runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas);

        let expected = ["container:update", "container:paint"].map(str::to_owned);
        assert_eq!(log.borrow().as_slice(), expected.as_slice());
        assert_eq!((child_counts.updates.get(), child_counts.paints.get()), (0, 0));
    }

    #[test]
    fn overlapping_pointer_routing_visits_siblings_in_reverse_z_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (first, first_counts) = Probe::new("first", log.clone());
        let (second, second_counts) = Probe::new("second", log.clone());
        let container = TraversalContainer::new([Node::widget(first), Node::widget(second)], false, log);
        let mut root = Node::container(container);
        let mut runtime = UiRuntime::new();
        let style = Style::default();
        let atlas = test_atlas();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, atlas.clone());
        let event = UiInputEvent::MouseDown {
            pos: Vec2i::new(20, 30),
            button: MouseButton::LEFT,
        };
        runtime.begin_input_event(true, &event);
        let routed = runtime.route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &event);
        assert_eq!(routed.map(|(_, result)| result), Some(InputResult::Captured));
        runtime.update_tree_root(&mut root, &style, atlas, empty_input());

        assert_eq!(first_counts.routed_events.get(), 0);
        assert_eq!(second_counts.routed_events.get(), 1);
    }

    #[test]
    fn captured_container_reports_local_retention_and_receives_loss_notification() {
        let (container, state) = CaptureContainer::new();
        let mut root = Node::container(container);
        let id = root.id();
        let mut runtime = UiRuntime::new();
        let style = Style::default();
        let atlas = test_atlas();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, atlas.clone());

        let mut input = Input::default();
        input.mousedown(20, 30, MouseButton::LEFT);
        let (down, down_state) = next_input(&mut input);
        runtime.begin_input_event(true, &down);
        let (owner, result) = runtime
            .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &down)
            .expect("container pointer-down must route");
        runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
        assert_eq!(runtime.capture, Some(id));
        runtime.update_tree_root(&mut root, &style, atlas.clone(), down_state);
        assert!(state.borrow().active);

        input.mousemove(200, 180);
        let (drag, drag_state) = next_input(&mut input);
        runtime.begin_input_event(true, &drag);
        assert_eq!(
            runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, drag_state.mouse_buttons, &drag,),
            Some(true)
        );
        assert!(state.borrow().saw_capture_during_drag);

        runtime.update_tree_root(&mut root, &style, atlas, drag_state);
        assert_eq!(runtime.capture, Some(id));
        assert!(state.borrow().active);
        assert_eq!(state.borrow().drags, 1);

        state.borrow_mut().active = false;
        layout_root(&mut runtime, &mut root, &style, test_atlas());
        assert_eq!(runtime.capture, None);
        assert_eq!(state.borrow().losses, 1);
        assert!(!state.borrow().active);
    }

    #[test]
    fn routing_time_release_defers_loss_until_that_event_update_finishes() {
        let (container, state) = CaptureContainer::new();
        state.borrow_mut().active = true;
        let mut root = Node::container(container);
        let id = root.id();
        let mut runtime = UiRuntime::new();
        let style = Style::default();
        let atlas = test_atlas();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, atlas.clone());
        runtime.capture = Some(id);

        let mut release_input = Input::default();
        release_input.mousedown(20, 30, MouseButton::LEFT);
        let _ = release_input.pop_event();
        release_input.mouseup(200, 180, MouseButton::LEFT);
        let (release, release_state) = next_input(&mut release_input);
        runtime.begin_input_event(true, &release);
        assert_eq!(
            runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, release_state.mouse_buttons, &release,),
            Some(true)
        );
        assert_eq!(runtime.capture, None);
        assert_eq!(runtime.capture_loss_after_update, Some(id));
        assert!(state.borrow().active, "loss must wait until the release update");
        assert_eq!(state.borrow().losses, 0);

        runtime.update_tree_root(&mut root, &style, atlas, release_state);
        assert!(!state.borrow().active);
        assert_eq!(state.borrow().losses, 1);
        assert_eq!(runtime.capture_loss_after_update, None);
    }

    #[test]
    fn a_new_press_after_release_starts_a_distinct_capture_event() {
        let (container, state) = CaptureContainer::new();
        state.borrow_mut().active = true;
        let mut root = Node::container(container);
        let id = root.id();
        let mut runtime = UiRuntime::new();
        let style = Style::default();
        let atlas = test_atlas();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, atlas.clone());
        runtime.capture = Some(id);

        let mut release_input = Input::default();
        release_input.mousedown(20, 30, MouseButton::LEFT);
        let _ = release_input.pop_event();
        release_input.mouseup(20, 30, MouseButton::LEFT);
        let (release, release_state) = next_input(&mut release_input);
        runtime.begin_input_event(true, &release);
        assert_eq!(
            runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, release_state.mouse_buttons, &release,),
            Some(true)
        );
        runtime.update_tree_root(&mut root, &style, atlas.clone(), release_state);
        assert_eq!(runtime.capture, None);
        assert_eq!(state.borrow().losses, 1);

        let mut down_input = Input::default();
        down_input.mousedown(20, 30, MouseButton::LEFT);
        let (down, down_state) = next_input(&mut down_input);
        runtime.begin_input_event(true, &down);
        let (owner, result) = runtime
            .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &down)
            .expect("same target must reacquire capture");
        runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
        assert_eq!(runtime.capture, Some(id));

        runtime.update_tree_root(&mut root, &style, atlas, down_state);
        assert_eq!(runtime.capture, Some(id));
        assert!(state.borrow().active);
        assert_eq!(state.borrow().losses, 1);
    }

    #[test]
    fn ancestor_gate_clears_all_descendant_targets_and_local_capture_mode() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (captured, capture_state) = CaptureContainer::new();
        capture_state.borrow_mut().active = true;
        let captured = Node::container(captured);
        let captured_id = captured.id();
        let gate = TraversalContainer::new([captured], false, log);
        let gate_state = gate.state.clone();
        let mut root = Node::container(gate);
        let mut runtime = UiRuntime::new();
        let style = Style::default();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, test_atlas());
        runtime.focus = Some(captured_id);
        runtime.hover = Some(captured_id);
        runtime.capture = Some(captured_id);
        runtime.push_routed_event(
            captured_id,
            UiInputEvent::MouseMove {
                pos: Vec2i::new(20, 30),
                delta: Vec2i::default(),
            },
        );

        gate_state.borrow_mut().visible = false;
        layout_root(&mut runtime, &mut root, &style, test_atlas());
        assert_eq!((runtime.focus, runtime.hover, runtime.capture), (None, None, None));
        assert!(runtime.take_routed_event(captured_id).is_none());
        assert!(!capture_state.borrow().active);
        assert_eq!(capture_state.borrow().losses, 1);

        gate_state.borrow_mut().visible = true;
        layout_root(&mut runtime, &mut root, &style, test_atlas());
        assert_eq!(runtime.capture, None, "expansion must not restore old capture");
        assert!(!capture_state.borrow().active, "expansion must not restore old local mode");
    }

    #[test]
    fn removed_target_does_not_notify_or_transfer_state_to_same_index_replacement() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (removed, removed_state) = CaptureContainer::new();
        removed_state.borrow_mut().active = true;
        let removed = Node::container(removed);
        let removed_id = removed.id();
        let (replacement, replacement_state) = CaptureContainer::new();
        let replacement = Node::container(replacement);
        let replacement_id = replacement.id();
        let parent = TraversalContainer::new([removed], false, log);
        let parent_state = parent.state.clone();
        let mut root = Node::container(parent);
        let mut runtime = UiRuntime::new();
        let style = Style::default();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, test_atlas());
        runtime.focus = Some(removed_id);
        runtime.hover = Some(removed_id);
        runtime.capture = Some(removed_id);
        runtime.push_routed_event(
            removed_id,
            UiInputEvent::MouseMove {
                pos: Vec2i::new(20, 30),
                delta: Vec2i::default(),
            },
        );

        parent_state.borrow_mut().children.replace([replacement]);
        layout_root(&mut runtime, &mut root, &style, test_atlas());

        assert_eq!((runtime.focus, runtime.hover, runtime.capture), (None, None, None));
        assert!(runtime.take_routed_event(removed_id).is_none());
        assert!(runtime.take_routed_event(replacement_id).is_none());
        assert_eq!(removed_state.borrow().losses, 0, "removed runtimes are dropped rather than notified");
        assert!(!replacement_state.borrow().active);
        assert_eq!(replacement_state.borrow().losses, 0);

        let mut drag_input = Input::default();
        drag_input.mousedown(20, 30, MouseButton::LEFT);
        let drag = UiInputEvent::MouseDrag {
            pos: Vec2i::new(20, 30),
            delta: Vec2i::new(2, 3),
            buttons: MouseButton::LEFT,
        };
        assert_eq!(
            runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, MouseButton::LEFT, &drag),
            Some(false),
            "the stale drag must be swallowed while awaiting its release"
        );
        assert!(runtime.discard_invalidated_capture_events);

        let mut release_input = Input::default();
        release_input.mouseup(20, 30, MouseButton::LEFT);
        let release = UiInputEvent::MouseUp {
            pos: Vec2i::new(20, 30),
            button: MouseButton::LEFT,
        };
        assert_eq!(
            runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, MouseButton::NONE, &release),
            Some(false),
            "the stale release must be swallowed instead of falling back to replacement hit routing"
        );
        assert!(!runtime.discard_invalidated_capture_events);
        assert!(runtime.take_routed_event(replacement_id).is_none());
    }

    #[test]
    fn cross_subtree_removal_during_update_sanitizes_before_later_delivery() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (captured, captured_state) = CaptureContainer::new();
        captured_state.borrow_mut().active = true;
        let captured = Node::container(captured);
        let captured_id = captured.id();
        let target_parent = TraversalContainer::new([captured], false, log.clone());
        let target_state = target_parent.state.clone();
        let remover = CrossSubtreeRemover {
            state: Rc::new(RefCell::new(())),
            target: target_state,
            removed: false,
            opt: WidgetOption::NONE,
        };
        let root_container = TraversalContainer::new([Node::widget(remover), Node::container(target_parent)], false, log);
        let mut root = Node::container(root_container);
        let mut runtime = UiRuntime::new();
        let style = Style::default();
        let atlas = test_atlas();

        runtime.begin_update();
        layout_root(&mut runtime, &mut root, &style, atlas.clone());
        runtime.focus = Some(captured_id);
        runtime.hover = Some(captured_id);
        runtime.capture = Some(captured_id);
        runtime.push_routed_event(
            captured_id,
            UiInputEvent::MouseMove {
                pos: Vec2i::new(20, 30),
                delta: Vec2i::default(),
            },
        );

        runtime.update_tree_root(&mut root, &style, atlas, empty_input());

        assert_eq!((runtime.focus, runtime.hover, runtime.capture), (None, None, None));
        assert!(runtime.take_routed_event(captured_id).is_none());
        assert_eq!(captured_state.borrow().losses, 0, "removed runtime must not receive a loss callback");
    }
}
