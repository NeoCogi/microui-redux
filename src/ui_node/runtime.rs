use super::*;
use std::collections::HashMap;
#[cfg(test)]
use std::cell::Cell;

/// Test-only counters for one retained root's most recent frame.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeMetrics {
    pub(crate) tree_layouts: u64,
    pub(crate) measures: u64,
    pub(crate) layouts: u64,
    pub(crate) updates: u64,
    pub(crate) paints: u64,
    pub(crate) routed_input_dispatches: u64,
    pub(crate) raw_interaction_derivations: u64,
}

pub(crate) struct UiRuntime {
    /// Aggregate root content size in root body coordinates.
    root_content_size: Dimensioni,
    /// Transform from root body coordinates into screen coordinates.
    root_transform: Transform,
    /// Focused node.
    pub(crate) focus: Option<UiNodeId>,
    /// Hovered node.
    pub(crate) hover: Option<UiNodeId>,
    /// Pointer-capturing node.
    pub(crate) capture: Option<UiNodeId>,
    /// Whether this runtime accepts pointer routing for the frame.
    pub(super) pointer_input_enabled: bool,
    /// Snapshot of text operations from the most recently recorded display list.
    #[cfg(test)]
    debug_texts: Vec<String>,
    /// Snapshot of rectangle operations from the most recently recorded display list.
    #[cfg(test)]
    debug_rects: Vec<Recti>,
    /// Whether focus was refreshed or changed this frame.
    pub(super) updated_focus: bool,
    /// Input events routed to each node during the current frame, consumed by update.
    routed_events: HashMap<UiNodeId, Vec<UiInputEvent>>,
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
            pointer_input_enabled: false,
            #[cfg(test)]
            debug_texts: Vec::new(),
            #[cfg(test)]
            debug_rects: Vec::new(),
            updated_focus: false,
            routed_events: HashMap::new(),
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
    pub(crate) fn set_focus_node(&mut self, roots: &[UiNode], node: UiNodeId) {
        if contains_active_node_in(roots, node) {
            self.focus = Some(node);
            self.updated_focus = true;
        }
    }

    /// Measures the outer root size needed for `AUTO_SIZE` node roots.
    pub(crate) fn measure_auto_size(&self, roots: &[UiNode], style: &Style, atlas: &crate::AtlasHandle, opt: WindowOption, min_width: i32) -> Dimensioni {
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
            let preferred = self.measure_node_ref(root, style, atlas, available);
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

    /// Clears frame-local runtime state before layout/input/update/paint passes.
    pub(crate) fn begin_frame(&mut self, pointer_input_enabled: bool) {
        self.updated_focus = false;
        self.routed_events.clear();
        self.pointer_input_enabled = pointer_input_enabled;
        #[cfg(test)]
        self.metrics.set(RuntimeMetrics::default());
    }

    /// Clears focus, hover, capture, and queued input while preserving retained node state.
    pub(crate) fn clear_transient_targets(&mut self) {
        self.focus = None;
        self.hover = None;
        self.capture = None;
        self.routed_events.clear();
        self.updated_focus = false;
    }

    /// Measures one persistent root node without introducing a parallel root projection.
    pub(crate) fn measure_tree_root(&self, root: &UiNode, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        self.measure_node_ref(root, style, atlas, available)
    }

    /// Lays out one persistent root node at its authoritative screen-space rectangle.
    pub(crate) fn layout_tree_root(&mut self, root: &mut UiNode, style: &Style, atlas: crate::AtlasHandle, outer: Recti, viewport: Recti) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.tree_layouts += 1);
        self.root_transform = Transform::root(viewport);
        self.layout_allocated_node_ref(root, style, &atlas, outer);
        self.root_content_size = root.state.layout.content_size;
        self.sanitize_transient_targets(std::slice::from_ref(root));
    }

    /// Updates one persistent root node and its eligible descendants.
    pub(crate) fn update_tree_root(&mut self, root: &mut UiNode, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
        self.update_node_ref(root, self.root_transform, style, atlas, input);
    }

    /// Paints one persistent root node and its eligible descendants.
    pub(crate) fn paint_tree_root(&mut self, root: &mut UiNode, display_list: &mut DisplayList, style: &Style, atlas: crate::AtlasHandle) {
        self.paint_node_ref(root, self.root_transform, display_list, style, atlas);
        #[cfg(test)]
        {
            self.debug_texts = display_list.debug_texts();
            self.debug_rects = display_list.debug_rects();
        }
    }

    /// Records one routed event for a node-local widget update.
    pub(crate) fn push_routed_event(&mut self, node: UiNodeId, event: UiInputEvent) {
        self.routed_events.entry(node).or_default().push(event);
    }

    /// Takes routed events for update and preserves a same-frame snapshot for paint.
    pub(crate) fn take_routed_events(&mut self, node: UiNodeId) -> Vec<UiInputEvent> {
        self.routed_events.remove(&node).unwrap_or_default()
    }

    /// Runs the pre-input layout pass that establishes hit targets.
    pub(crate) fn layout_frame_roots(&mut self, roots: &mut [UiNode], style: &Style, atlas: crate::AtlasHandle, body: Recti) -> Dimensioni {
        self.set_root_body(body);
        let local_body = local_rect_for(body);
        let body_view = root_window_body_view(local_body, style);
        let size = self.layout_roots_in_view(roots, style, atlas, body_view);
        self.sanitize_transient_targets(roots);
        size
    }

    /// Runs post-input update/paint passes and appends directly to a borrowed display list.
    ///
    /// The caller must run one pre-input layout pass and route input before calling this method. A
    /// final layout runs after update so paint observes post-update widget/container state.
    pub(crate) fn update_paint_frame(
        &mut self,
        roots: &mut [UiNode],
        display_list: &mut DisplayList,
        atlas: crate::AtlasHandle,
        style: &Style,
        input: &Input,
        body: Recti,
    ) {
        self.set_root_body(body);
        let local_body = local_rect_for(body);
        let body_view = root_window_body_view(local_body, style);

        self.layout_roots_in_view(roots, style, atlas.clone(), body_view);
        self.sanitize_transient_targets(roots);

        let mut root_index = 0;
        while root_index < roots.len() {
            let root = &mut roots[root_index];
            self.update_node_ref(root, self.root_transform, style, atlas.clone(), input);
            root_index += 1;
        }

        self.layout_roots_in_view(roots, style, atlas.clone(), body_view);
        self.sanitize_transient_targets(roots);

        let mut root_index = 0;
        while root_index < roots.len() {
            let root = &mut roots[root_index];
            self.paint_node_ref(root, self.root_transform, display_list, style, atlas.clone());
            root_index += 1;
        }

        if !self.updated_focus {
            self.focus = None;
        }
        #[cfg(test)]
        {
            self.debug_texts = display_list.debug_texts();
            self.debug_rects = display_list.debug_rects();
        }
    }

    /// Clears interaction state that points at a removed or container-gated descendant.
    ///
    /// Sanitization runs only at layout boundaries, when no container state borrow is active. A
    /// replacement node cannot inherit a stale target because every owning node has a fresh ID.
    fn sanitize_transient_targets(&mut self, roots: &[UiNode]) {
        self.focus = self.focus.filter(|id| contains_active_node_in(roots, *id));
        self.hover = self.hover.filter(|id| contains_active_node_in(roots, *id));
        self.capture = self.capture.filter(|id| contains_active_node_in(roots, *id));
        self.routed_events.retain(|id, _| contains_active_node_in(roots, *id));
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

    /// Returns structural phase counters for the most recently completed frame.
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

    /// Returns whether this runtime accepts pointer hit routing for the current frame.
    pub(crate) fn accepts_pointer_input(&self) -> bool {
        self.pointer_input_enabled
    }

    fn set_root_body(&mut self, body: Recti) {
        self.root_transform = Transform::root_at(root_origin_for(body), body);
    }

    /// Returns the current root-body transform.
    pub(crate) fn root_transform(&self) -> Transform {
        self.root_transform
    }

    /// Returns the current full rectangle for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_rect(&self, roots: &[UiNode], id: UiNodeId) -> Option<Recti> {
        with_node(roots, id, |node| {
            self.parent_transform_for_node(roots, id).resolve(node.state.layout.allocation)
        })
    }

    /// Returns a node-local rectangle in screen coordinates for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_local_rect(&self, roots: &[UiNode], id: UiNodeId, rect: Recti) -> Option<Recti> {
        with_node(roots, id, |node| {
            let screen_rect = self.parent_transform_for_node(roots, id).resolve(node.state.layout.allocation);
            Recti::new(screen_rect.x + rect.x, screen_rect.y + rect.y, rect.width, rect.height)
        })
    }

    /// Returns whether a node exists in this runtime.
    pub(crate) fn contains_node(&self, roots: &[UiNode], id: UiNodeId) -> bool {
        contains_node_in(roots, id)
    }

    /// Runs test/debug work against a matching node without exposing an attached borrow publicly.
    #[cfg(test)]
    pub(crate) fn with_node<R>(&self, roots: &[UiNode], id: UiNodeId, f: impl FnOnce(&UiNode) -> R) -> Option<R> {
        with_node(roots, id, f)
    }

    /// Finds the current parent of `child` by walking root/container child membership.
    pub(super) fn parent_of(&self, roots: &[UiNode], child: UiNodeId) -> Option<UiNodeId> {
        for root in roots {
            if let Some(parent) = Self::parent_of_from(root, child) {
                return Some(parent);
            }
        }
        None
    }

    fn parent_of_from(current: &UiNode, child: UiNodeId) -> Option<UiNodeId> {
        current.with_children(|children| {
            if children.iter().any(|node| node.id() == child) {
                return Some(current.id());
            }
            children.iter().find_map(|descendant| Self::parent_of_from(descendant, child))
        })
    }

    /// Lays out root nodes inside an already resolved root client area.
    pub(super) fn layout_roots_in_view(&mut self, roots: &mut [UiNode], style: &Style, atlas: crate::AtlasHandle, client: Recti) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.tree_layouts += 1);
        let mut y = client.y;
        let mut content_bounds = None;
        for root_node in roots.iter_mut() {
            let remaining_height = (client.y + client.height - y).max(0);
            let preferred = self.measure_node_ref(root_node, style, &atlas, Dimensioni::new(client.width, remaining_height));
            // Root position must not change sizing semantics: `Auto` keeps its measured height,
            // while callers that want the remaining client height request `Remainder` explicitly.
            let policy = root_node.state.policy;
            let width = resolve_allocated_size(policy.width, preferred.width, client.width, client.width, None);
            let height = resolve_size(policy.height, preferred.height, remaining_height, remaining_height, None).max(0);
            let rect = Recti::new(client.x, y, width, height);
            // Root flow resolves the root node's policies above, so the resulting rectangle is an
            // allocation rather than an unresolved parent slot.
            self.layout_allocated_node_ref(root_node, style, &atlas, rect);
            let allocation = root_node.state.layout.allocation;
            let content_size = root_node.state.layout.content_size;
            let unscrolled = Recti::new(
                allocation.x,
                allocation.y,
                allocation.width.max(content_size.width),
                allocation.height.max(content_size.height),
            );
            content_bounds = Some(match content_bounds {
                Some(bounds) => union_rect(bounds, unscrolled),
                None => unscrolled,
            });
            y = allocation.y + allocation.height + style.spacing;
        }

        let content_size = content_bounds
            .map(|bounds| Dimensioni::new((bounds.x + bounds.width - client.x).max(0), (bounds.y + bounds.height - client.y).max(0)))
            .unwrap_or_default();
        self.root_content_size = content_size;
        content_size
    }

    /// Measures one already-borrowed node's preferred size.
    pub(super) fn measure_node_ref(&self, node: &UiNode, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.measures += 1);
        let framed = match &node.data {
            UiNodeData::Widget(widget) => widget.is_framed(),
            UiNodeData::Container(container) => NodeBehavior::is_framed(&**container),
        };
        let border_width = if framed { style.frame_border().width } else { 0 };
        let policy = node.state.policy;
        let outer_available = Dimensioni::new(
            measure_axis_available(policy.width, available.width),
            measure_axis_available(policy.height, available.height),
        );
        let content_available = crate::frame::content_available(outer_available, border_width);
        let ctx = MeasureCtx { runtime: self, style, atlas };
        let preferred_content = match &node.data {
            UiNodeData::Widget(widget) => widget.measure(&ctx, node.state(), content_available),
            UiNodeData::Container(container) => NodeBehavior::measure(&**container, &ctx, node.state(), content_available),
        };
        let preferred_outer = crate::frame::outer_preferred(preferred_content, border_width);
        Dimensioni::new(
            resolve_size(policy.width, preferred_outer.width, available.width, available.width, None),
            resolve_size(policy.height, preferred_outer.height, available.height, available.height, None),
        )
    }

    /// Lays out one already-borrowed node through its behavior.
    pub(super) fn layout_node_ref(&mut self, node: &mut UiNode, style: &Style, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.layouts += 1);
        let framed = match &node.data {
            UiNodeData::Widget(widget) => widget.is_framed(),
            UiNodeData::Container(container) => NodeBehavior::is_framed(&**container),
        };
        let preferred = self.measure_node_ref(node, style, atlas, Dimensioni::new(rect.width, rect.height));
        let policy = node.state.policy;
        let outer = Recti::new(
            rect.x,
            rect.y,
            resolve_allocated_size(policy.width, preferred.width, rect.width, rect.width, None),
            resolve_allocated_size(policy.height, preferred.height, rect.height, rect.height, None),
        );
        self.layout_node_outer_ref(node, style, atlas, framed, outer)
    }

    /// Lays out a node whose parent/root flow has already resolved its size policy.
    fn layout_allocated_node_ref(&mut self, node: &mut UiNode, style: &Style, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.layouts += 1);
        let framed = match &node.data {
            UiNodeData::Widget(widget) => widget.is_framed(),
            UiNodeData::Container(container) => NodeBehavior::is_framed(&**container),
        };
        // Preserve the established measure/layout phase contract while keeping the resolved root
        // allocation authoritative.
        let _preferred = self.measure_node_ref(node, style, atlas, Dimensioni::new(rect.width, rect.height));
        let outer = Recti::new(rect.x, rect.y, rect.width.max(0), rect.height.max(0));
        self.layout_node_outer_ref(node, style, atlas, framed, outer)
    }

    /// Applies frame/content geometry and delegates layout for one resolved outer allocation.
    fn layout_node_outer_ref(&mut self, node: &mut UiNode, style: &Style, atlas: &crate::AtlasHandle, framed: bool, outer: Recti) -> Dimensioni {
        let local_outer = Recti::new(0, 0, outer.width, outer.height);
        let frame_geometry = crate::frame::frame_geometry(local_outer, framed, style);
        let content = frame_geometry.content_or_empty();
        let is_branch = node.is_container();
        node.set_layout(NodeLayout::from_parts(outer, content, Dimensioni::new(outer.width.max(0), outer.height.max(0))));

        let mut ctx = LayoutCtx {
            runtime: self,
            style,
            atlas,
            outer,
            content,
            border_width: frame_geometry.border_width,
        };
        match &mut node.data {
            UiNodeData::Widget(widget) => widget.layout(&mut ctx, &mut node.state, content),
            UiNodeData::Container(container) => NodeBehavior::layout(&mut **container, &mut ctx, &mut node.state, content),
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
        if is_branch && propagate_child_overflow {
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
    pub(super) fn update_node_ref(&mut self, node: &mut UiNode, parent_transform: Transform, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
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

        if let Some((opt, focus_policy)) = node_interaction_config(node) {
            let id = node.id();
            let (hovered, focused, clicked, active, scroll_delta) = self.interaction_for(id, screen_rect, screen_clip, input, opt, focus_policy);
            node.state.hovered = hovered;
            node.state.focused = focused;
            node.state.clicked = clicked;
            node.state.active = active;
            node.state.scroll_delta = scroll_delta;
        }

        let traverse_children = {
            let mut ctx = UpdateCtx {
                runtime: self,
                style,
                atlas: atlas.clone(),
                input,
                screen_origin,
                content_rect,
                content_clip,
            };
            match &mut node.data {
                UiNodeData::Widget(widget) => widget.update(&mut ctx, &mut node.state),
                UiNodeData::Container(container) => NodeBehavior::update(&mut **container, &mut ctx, &mut node.state),
            }
        };
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
    pub(super) fn interaction_for(
        &mut self,
        id: UiNodeId,
        rect: Recti,
        clip: Recti,
        input: &Input,
        opt: WidgetOption,
        focus_policy: FocusPolicy,
    ) -> (bool, bool, bool, bool, Option<Vec2i>) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.raw_interaction_derivations += 1);
        if opt.intersects(WidgetOption::NO_INTERACT) {
            return (false, false, false, false, None);
        }

        let hovered = self.pointer_input_enabled && rect.contains(&input.mouse_pos) && clip.contains(&input.mouse_pos);
        if hovered {
            self.hover = Some(id);
        }

        if self.focus == Some(id) {
            self.updated_focus = true;
            let pressed_outside = !input.mouse_pressed.is_empty() && !hovered;
            let released_without_hold_focus = input.mouse_down.is_empty() && focus_policy.releases_on_mouse_up();
            if pressed_outside || released_without_hold_focus {
                self.focus = None;
            }
        }

        if self.hover == Some(id) {
            if !hovered {
                self.hover = None;
            } else if !input.mouse_pressed.is_empty() {
                self.focus = Some(id);
                self.updated_focus = true;
            }
        } else if hovered && !input.mouse_pressed.is_empty() {
            self.focus = Some(id);
            self.updated_focus = true;
        }

        let focused = self.focus == Some(id);
        let active = focused && input.mouse_down.intersects(MouseButton::LEFT);
        let clicked = focused && input.mouse_pressed.intersects(MouseButton::LEFT);
        let scroll_delta = if opt.intersects(WidgetOption::GRAB_SCROLL) && hovered && (input.scroll_delta.x != 0 || input.scroll_delta.y != 0) {
            Some(input.scroll_delta)
        } else {
            None
        };
        (hovered, focused, clicked, active, scroll_delta)
    }

    /// Routes focus input events to the focused node.
    pub(crate) fn route_focus_input_events(&mut self, roots: &mut [UiNode], style: &Style, input: &Input) -> bool {
        let mut consumed = false;
        for event in focus_events_from_input(input) {
            consumed |= self.route_focus_input_event(roots, style, &event);
        }
        for event in held_events_from_input(input) {
            consumed |= self.route_focus_input_event(roots, style, &event);
        }
        consumed
    }

    /// Routes one pointer event to the capturing node, if there is one.
    pub(crate) fn route_captured_pointer_input_event(&mut self, roots: &mut [UiNode], style: &Style, input: &Input, event: &UiInputEvent) -> Option<bool> {
        let capture = self.capture.filter(|id| contains_active_node_in(roots, *id))?;
        let parent_transform = self.parent_transform_for_node(roots, capture);
        let result = self.route_input_event_to_node_only(roots, capture, parent_transform, style, event);
        self.update_pointer_capture(capture, result, event, input);
        Some(result.is_consumed())
    }

    /// Routes keyboard/text input to the focused node only.
    fn route_focus_input_event(&mut self, roots: &mut [UiNode], style: &Style, event: &UiInputEvent) -> bool {
        let Some(focus) = self.focus.filter(|id| contains_active_node_in(roots, *id)) else {
            return false;
        };
        let parent_transform = self.parent_transform_for_node(roots, focus);
        self.route_input_event_to_node_only(roots, focus, parent_transform, style, event).is_consumed()
    }

    /// Applies runtime pointer-capture ownership from one routed event result.
    pub(crate) fn update_pointer_capture(&mut self, owner: UiNodeId, result: InputResult, event: &UiInputEvent, input: &Input) {
        if event.is_pointer_release() && input.mouse_down.is_empty() {
            self.capture = None;
        } else if result == InputResult::Captured {
            self.capture = Some(owner);
        } else if self.capture == Some(owner) && input.mouse_down.is_empty() {
            self.capture = None;
        }
    }

    /// Walks borrowed children first so nested owners beat ancestors.
    pub(crate) fn route_input_event_to_node_ref(
        &mut self,
        node: &mut UiNode,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> Option<(UiNodeId, InputResult)> {
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

    /// Routes an event to exactly one node behavior without traversing descendants.
    fn route_input_event_to_node_only(
        &mut self,
        roots: &mut [UiNode],
        id: UiNodeId,
        parent_transform: Transform,
        style: &Style,
        event: &UiInputEvent,
    ) -> InputResult {
        with_node_mut(roots, id, |node| self.route_input_event_to_node_only_ref(node, parent_transform, style, event)).unwrap_or(InputResult::Ignored)
    }

    /// Routes an event to exactly one borrowed node behavior without traversing descendants.
    fn route_input_event_to_node_only_ref(&mut self, node: &mut UiNode, parent_transform: Transform, style: &Style, event: &UiInputEvent) -> InputResult {
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

        if matches!(&node.data, UiNodeData::Widget(_)) {
            let (opt, _focus_policy) = node_interaction_config(node).expect("direct widget interaction config missing");
            return super::containers::route_public_widget_input(self, &node.state, local_rect, local_clip, opt, &local_event);
        }

        let mut ctx = InputCtx {
            runtime: self,
            style,
            content_rect,
            content_clip,
        };
        match &mut node.data {
            UiNodeData::Widget(widget) => widget.update_on(&mut ctx, &mut node.state, &local_event),
            UiNodeData::Container(container) => NodeBehavior::update_on(&mut **container, &mut ctx, &mut node.state, &local_event),
        }
    }

    /// Paints one already-borrowed node and descendants.
    pub(super) fn paint_node_ref(
        &mut self,
        node: &mut UiNode,
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
        let traverse_children = {
            let mut ctx = PaintCtx {
                display_list,
                style,
                atlas: atlas.clone(),
                screen_origin,
                content_rect,
                content_clip,
            };
            match &mut node.data {
                UiNodeData::Widget(widget) => widget.paint(&mut ctx, &mut node.state),
                UiNodeData::Container(container) => NodeBehavior::paint(&mut **container, &mut ctx, &mut node.state),
            }
        };
        if traverse_children {
            node.with_children_mut(|children| {
                for child in children {
                    self.paint_node_ref(child, child_transform, display_list, style, atlas.clone());
                }
            });
        }
    }

    /// Pushes this node onto a parent transform.
    pub(super) fn node_transform(&self, roots: &[UiNode], id: UiNodeId, parent: Transform) -> Transform {
        with_node(roots, id, |node| parent.push(node.state.layout)).unwrap_or(parent)
    }

    /// Derives the child transform for one node by walking its parent chain.
    pub(super) fn transform_for_node(&self, roots: &[UiNode], id: UiNodeId) -> Transform {
        let parent = self.parent_transform_for_node(roots, id);
        self.node_transform(roots, id, parent)
    }

    pub(super) fn parent_transform_for_node(&self, roots: &[UiNode], id: UiNodeId) -> Transform {
        self.contains_node(roots, id)
            .then(|| self.parent_of(roots, id))
            .flatten()
            .map(|parent| self.transform_for_node(roots, parent))
            .unwrap_or(self.root_transform)
    }
}

fn local_rect_for(rect: Recti) -> Recti {
    Recti::new(0, 0, rect.width, rect.height)
}

fn root_origin_for(body: Recti) -> Vec2i {
    Vec2i::new(body.x, body.y)
}

/// Returns the root content viewport. Root/window layout clips to this rect but never scrolls it.
fn root_window_body_view(body: Recti, style: &Style) -> Recti {
    expand_rect(body, -style.padding)
}

fn with_node<R>(roots: &[UiNode], id: UiNodeId, f: impl FnOnce(&UiNode) -> R) -> Option<R> {
    let root = roots.iter().find(|root| root.with_node(id, |_| ()).is_some())?;
    root.with_node(id, f)
}

fn with_node_mut<R>(roots: &mut [UiNode], id: UiNodeId, f: impl FnOnce(&mut UiNode) -> R) -> Option<R> {
    let index = roots.iter().position(|root| root.with_node(id, |_| ()).is_some())?;
    roots[index].with_node_mut(id, f)
}

fn contains_node_in(roots: &[UiNode], id: UiNodeId) -> bool {
    roots.iter().any(|root| root.with_node(id, |_| ()).is_some())
}

/// Returns whether a node participates in traversal through every ancestor visibility gate.
fn contains_active_node_in(roots: &[UiNode], id: UiNodeId) -> bool {
    roots.iter().any(|root| contains_active_node(root, id))
}

fn contains_active_node(node: &UiNode, id: UiNodeId) -> bool {
    if node.id() == id {
        return true;
    }
    if !node_children_visible(node) {
        return false;
    }
    node.with_children(|children| children.iter().any(|child| contains_active_node(child, id)))
}

fn node_children_visible(node: &UiNode) -> bool {
    match &node.data {
        UiNodeData::Container(container) => Container::children_visible(&**container),
        UiNodeData::Widget(_) => true,
    }
}

fn node_is_framed(node: &UiNode) -> bool {
    match &node.data {
        UiNodeData::Widget(widget) => widget.is_framed(),
        UiNodeData::Container(container) => NodeBehavior::is_framed(&**container),
    }
}

fn node_interaction_config(node: &UiNode) -> Option<(WidgetOption, FocusPolicy)> {
    match &node.data {
        UiNodeData::Widget(widget) => widget.interaction_config(),
        UiNodeData::Container(container) => NodeBehavior::interaction_config(&**container),
    }
}

fn rect_relative_to(rect: Recti, origin: Vec2i) -> Recti {
    Recti::new(rect.x - origin.x, rect.y - origin.y, rect.width, rect.height)
}

fn child_content_bounds_from_children(children: &[UiNode]) -> Option<Recti> {
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
