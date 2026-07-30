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
        if contains_node_in(roots, node) {
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
        self.layout_roots_in_view(roots, style, atlas, body_view)
    }

    /// Runs post-input update/paint passes and appends directly to a borrowed display list.
    ///
    /// The caller must run one pre-input layout pass and route input before calling this method. A
    /// final layout runs after update so paint observes post-update widget/container state.
    pub(crate) fn update_paint_frame(
        &mut self,
        roots: &mut [UiNode],
        root_id: crate::RootId,
        root_name: &str,
        display_list: &mut DisplayList,
        atlas: crate::AtlasHandle,
        style: &Style,
        input: &Input,
        results: &mut FrameResults,
        body: Recti,
    ) {
        self.set_root_body(body);
        let local_body = local_rect_for(body);
        let body_view = root_window_body_view(local_body, style);

        self.layout_roots_in_view(roots, style, atlas.clone(), body_view);

        let mut root_index = 0;
        while root_index < roots.len() {
            let root = &mut roots[root_index];
            self.update_node_ref(root_id, root_name, root, self.root_transform, style, atlas.clone(), input, results);
            root_index += 1;
        }

        self.layout_roots_in_view(roots, style, atlas.clone(), body_view);

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
        find_node(roots, id).map(|node| self.parent_transform_for_node(roots, id).resolve(node.state.layout.allocation))
    }

    /// Returns a node-local rectangle in screen coordinates for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_local_rect(&self, roots: &[UiNode], id: UiNodeId, rect: Recti) -> Option<Recti> {
        let node = find_node(roots, id)?;
        let screen_rect = self.parent_transform_for_node(roots, id).resolve(node.state.layout.allocation);
        Some(Recti::new(screen_rect.x + rect.x, screen_rect.y + rect.y, rect.width, rect.height))
    }

    /// Returns whether a node exists in this runtime.
    pub(crate) fn contains_node(&self, roots: &[UiNode], id: UiNodeId) -> bool {
        contains_node_in(roots, id)
    }

    /// Returns a node by id.
    pub(crate) fn node<'a>(&self, roots: &'a [UiNode], id: UiNodeId) -> Option<&'a UiNode> {
        find_node(roots, id)
    }

    /// Returns a mutable node by id.
    pub(crate) fn node_mut<'a>(&mut self, roots: &'a mut [UiNode], id: UiNodeId) -> Option<&'a mut UiNode> {
        find_node_mut(roots, id)
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
        if current.children().iter().any(|node| node.id() == child) {
            return Some(current.id());
        }
        for descendant in current.children() {
            if let Some(parent) = Self::parent_of_from(descendant, child) {
                return Some(parent);
            }
        }
        None
    }

    /// Carries live runtime-only state into a newly submitted projection.
    pub(crate) fn transfer_runtime_state_from(&mut self, roots: &mut [UiNode], previous_roots: &[UiNode], previous: &UiRuntime) {
        for node in roots.iter_mut() {
            transfer_node_runtime_state(node, previous_roots);
        }

        self.focus = previous.focus.filter(|id| contains_node_in(roots, *id));
        self.hover = previous.hover.filter(|id| contains_node_in(roots, *id));
        self.capture = previous.capture.filter(|id| contains_node_in(roots, *id));
        self.pointer_input_enabled = previous.pointer_input_enabled;
        self.updated_focus = previous.updated_focus && self.focus.is_some();
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
            let policy = root_node.state.policy.height;
            let height = resolve_size(policy, preferred.height, remaining_height, remaining_height, None).max(0);
            let rect = Recti::new(client.x, y, client.width, height);
            self.layout_node_ref(root_node, style, &atlas, rect);
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
            UiNodeData::Container(container) => container.is_framed(),
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
            UiNodeData::Container(container) => container.measure(&ctx, node.state(), content_available),
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
            UiNodeData::Container(container) => container.is_framed(),
        };
        let preferred = self.measure_node_ref(node, style, atlas, Dimensioni::new(rect.width, rect.height));
        let policy = node.state.policy;
        let outer = Recti::new(
            rect.x,
            rect.y,
            resolve_allocated_size(policy.width, preferred.width, rect.width, rect.width, None),
            resolve_allocated_size(policy.height, preferred.height, rect.height, rect.height, None),
        );
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
            UiNodeData::Container(container) => container.layout(&mut ctx, &mut node.state, content),
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
            let content_rect = child_content_bounds_from_children(node.children()).unwrap_or(content);
            let content_size = Dimensioni::new(
                (content_rect.x + content_rect.width).max(outer.width).max(0),
                (content_rect.y + content_rect.height).max(outer.height).max(0),
            );
            node.set_layout(node.state.layout.with_content_size(content_size));
        }
        Dimensioni::new(outer.width, outer.height)
    }

    /// Updates one already-borrowed node and descendants.
    pub(super) fn update_node_ref(
        &mut self,
        root_id: crate::RootId,
        root_name: &str,
        node: &mut UiNode,
        parent_transform: Transform,
        style: &Style,
        atlas: crate::AtlasHandle,
        input: &Input,
        results: &mut FrameResults,
    ) {
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
        let content_clip = rect_relative_to(child_transform.clip, screen_origin);

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
                root_id,
                root_name,
                style,
                atlas: atlas.clone(),
                input,
                results,
                screen_origin,
                content_rect,
                content_clip,
            };
            match &mut node.data {
                UiNodeData::Widget(widget) => widget.update(&mut ctx, &mut node.state),
                UiNodeData::Container(container) => container.update(&mut ctx, &mut node.state),
            }
        };
        if traverse_children {
            if let Some(children) = node.children_mut() {
                for child in children {
                    // Update recursion carries interaction and layout state only. Rendering enters
                    // the tree later through the distinct paint traversal below.
                    self.update_node_ref(root_id, root_name, child, child_transform, style, atlas.clone(), input, results);
                }
            }
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
        let capture = self.capture.filter(|id| contains_node_in(roots, *id))?;
        let parent_transform = self.parent_transform_for_node(roots, capture);
        let result = self.route_input_event_to_node_only(roots, capture, parent_transform, style, event);
        self.update_pointer_capture(capture, result, event, input);
        Some(result.is_consumed())
    }

    /// Routes keyboard/text input to the focused node only.
    fn route_focus_input_event(&mut self, roots: &mut [UiNode], style: &Style, event: &UiInputEvent) -> bool {
        let Some(focus) = self.focus.filter(|id| contains_node_in(roots, *id)) else {
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
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if let Some(result) = self.route_input_event_to_node_ref(child, child_transform, style, event) {
                    return Some(result);
                }
            }
        }

        let result = self.route_input_event_to_node_only_ref(node, parent_transform, style, event);
        result.is_consumed().then_some((id, result))
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
        find_node_mut(roots, id)
            .map(|node| self.route_input_event_to_node_only_ref(node, parent_transform, style, event))
            .unwrap_or(InputResult::Ignored)
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
        let child_transform = parent_transform.push(node.state.layout);
        let content_clip = rect_relative_to(child_transform.clip, screen_origin);
        let local_event = crate::widget_ctx::localize_event(screen_origin, event.clone());

        if let Some((opt, _focus_policy)) = node_interaction_config(node) {
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
            UiNodeData::Container(container) => container.update_on(&mut ctx, &mut node.state, &local_event),
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
        let content_clip = rect_relative_to(child_transform.clip, screen_origin);
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
                UiNodeData::Container(container) => container.paint(&mut ctx, &mut node.state),
            }
        };
        if traverse_children {
            if let Some(children) = node.children_mut() {
                for child in children {
                    self.paint_node_ref(child, child_transform, display_list, style, atlas.clone());
                }
            }
        }
    }

    /// Pushes this node onto a parent transform.
    pub(super) fn node_transform(&self, roots: &[UiNode], id: UiNodeId, parent: Transform) -> Transform {
        find_node(roots, id).map(|node| parent.push(node.state.layout)).unwrap_or(parent)
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

fn find_node(roots: &[UiNode], id: UiNodeId) -> Option<&UiNode> {
    roots.iter().find_map(|root| root.find(id))
}

fn find_node_mut(roots: &mut [UiNode], id: UiNodeId) -> Option<&mut UiNode> {
    roots.iter_mut().find_map(|root| root.find_mut(id))
}

fn contains_node_in(roots: &[UiNode], id: UiNodeId) -> bool {
    find_node(roots, id).is_some()
}

fn node_is_framed(node: &UiNode) -> bool {
    match &node.data {
        UiNodeData::Widget(widget) => widget.is_framed(),
        UiNodeData::Container(container) => container.is_framed(),
    }
}

fn node_interaction_config(node: &UiNode) -> Option<(WidgetOption, FocusPolicy)> {
    match &node.data {
        UiNodeData::Widget(widget) => widget.interaction_config(),
        UiNodeData::Container(container) => container.interaction_config(),
    }
}

fn rect_relative_to(rect: Recti, origin: Vec2i) -> Recti {
    Recti::new(rect.x - origin.x, rect.y - origin.y, rect.width, rect.height)
}

fn transfer_node_runtime_state(node: &mut UiNode, previous_roots: &[UiNode]) {
    if let Some(previous_node) = find_node(previous_roots, node.id()) {
        node.state.layout = previous_node.state.layout;
        node.state.visible = previous_node.state.visible;
        node.state.hovered = previous_node.state.hovered;
        node.state.focused = previous_node.state.focused;
        node.state.clicked = previous_node.state.clicked;
        node.state.active = previous_node.state.active;
        node.state.scroll_delta = previous_node.state.scroll_delta;
    }
    if let Some(children) = node.children_mut() {
        for child in children {
            transfer_node_runtime_state(child, previous_roots);
        }
    }
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
