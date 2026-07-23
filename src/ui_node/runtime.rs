use super::*;
use crate::render::geometry::SolidGeometry;
use std::collections::HashMap;

pub(crate) struct UiRuntime {
    /// Aggregate root content size in root body coordinates.
    root_content_size: Dimensioni,
    /// Traversal from root body coordinates into screen coordinates.
    root_traversal: TraversalState,
    /// Focused node.
    pub(crate) focus: Option<UiNodeId>,
    /// Hovered node.
    pub(crate) hover: Option<UiNodeId>,
    /// Pointer-capturing node.
    pub(crate) capture: Option<UiNodeId>,
    /// Whether this runtime accepts pointer routing for the frame.
    pub(super) pointer_input_enabled: bool,
    /// Commands recorded by the node paint path.
    pub(super) commands: Vec<Command>,
    /// Snapshot of text commands before renderer replay drains them.
    #[cfg(test)]
    debug_texts: Vec<String>,
    /// Snapshot of rectangle commands before renderer replay drains them.
    #[cfg(test)]
    debug_rects: Vec<Recti>,
    /// Triangle vertex arena referenced by retained triangle commands.
    pub(super) triangle_vertices: Vec<Vertex>,
    /// Reusable typed solid geometry and its private polygon workspace.
    pub(super) solid_geometry: SolidGeometry,
    /// Active screen-space clip stack.
    pub(super) clip_stack: Vec<Recti>,
    /// Whether focus was refreshed or changed this frame.
    pub(super) updated_focus: bool,
    /// Input events routed to each node during the current frame, consumed by update.
    routed_events: HashMap<UiNodeId, Vec<UiInputEvent>>,
}

impl Default for UiRuntime {
    fn default() -> Self {
        Self {
            root_content_size: Dimensioni::default(),
            root_traversal: TraversalState::root(UNCLIPPED_RECT),
            focus: None,
            hover: None,
            capture: None,
            pointer_input_enabled: false,
            commands: Vec::new(),
            #[cfg(test)]
            debug_texts: Vec::new(),
            #[cfg(test)]
            debug_rects: Vec::new(),
            triangle_vertices: Vec::new(),
            solid_geometry: SolidGeometry::new(),
            clip_stack: Vec::new(),
            updated_focus: false,
            routed_events: HashMap::new(),
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
    pub(crate) fn measure_auto_size(&self, roots: &[UiNode], style: &Style, atlas: &crate::AtlasHandle, opt: ContainerOption, min_width: i32) -> Dimensioni {
        let title_height = if opt.intersects(ContainerOption::NO_TITLE) {
            0
        } else {
            root_titlebar_height(style, atlas)
        };
        let padding = style.padding.max(0);
        let horizontal_padding = padding.saturating_mul(2);
        let available = Dimensioni::new(min_width.saturating_sub(horizontal_padding).max(1), 10_000);
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
        Dimensioni::new(
            width.saturating_add(horizontal_padding).max(min_width).max(1),
            height.saturating_add(padding.saturating_mul(2)).saturating_add(title_height).max(1),
        )
    }

    /// Clears frame-local runtime state before layout/input/update/paint passes.
    pub(crate) fn begin_frame(&mut self, pointer_input_enabled: bool) {
        self.commands.clear();
        self.triangle_vertices.clear();
        self.solid_geometry.clear();
        self.clip_stack.clear();
        self.clip_stack.push(UNCLIPPED_RECT);
        self.updated_focus = false;
        self.routed_events.clear();
        self.pointer_input_enabled = pointer_input_enabled;
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

    /// Runs post-input update/paint passes and replays the recorded commands immediately.
    ///
    /// The caller must run one pre-input layout pass and route input before calling this method. A
    /// final layout runs after update so paint observes post-update widget/container state.
    pub(crate) fn update_paint_frame<R: Renderer>(
        &mut self,
        roots: &mut [UiNode],
        root_id: crate::RootId,
        root_name: &str,
        canvas: &mut Canvas<R>,
        style: &Style,
        input: &Input,
        results: &mut FrameResults,
        body: Recti,
    ) {
        self.set_root_body(body);
        let local_body = local_rect_for(body);
        let body_view = root_window_body_view(local_body, style);

        self.layout_roots_in_view(roots, style, canvas.get_atlas(), body_view);

        let mut root_index = 0;
        while root_index < roots.len() {
            let root = &mut roots[root_index];
            self.update_node_ref(root_id, root_name, root, self.root_traversal, style, canvas.get_atlas(), input, results);
            root_index += 1;
        }

        self.layout_roots_in_view(roots, style, canvas.get_atlas(), body_view);

        let mut root_index = 0;
        while root_index < roots.len() {
            let root = &mut roots[root_index];
            self.paint_node_ref(root, self.root_traversal, style, canvas.get_atlas(), input);
            root_index += 1;
        }

        if !self.updated_focus {
            self.focus = None;
        }
        self.clip_stack.pop();
        #[cfg(test)]
        {
            self.debug_texts = self
                .commands
                .iter()
                .filter_map(|cmd| match cmd {
                    Command::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect();
            self.debug_rects = self
                .commands
                .iter()
                .filter_map(|cmd| match cmd {
                    Command::Recti { rect, .. } => Some(*rect),
                    _ => None,
                })
                .collect();
        }
        render_command_stream(canvas, &mut self.commands, &self.triangle_vertices);
        self.triangle_vertices.clear();
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

    /// Returns whether this runtime accepts pointer hit routing for the current frame.
    pub(crate) fn accepts_pointer_input(&self) -> bool {
        self.pointer_input_enabled
    }

    fn set_root_body(&mut self, body: Recti) {
        self.root_traversal = TraversalState::root_at(root_origin_for(body), body);
    }

    /// Returns the current root-body traversal state.
    pub(crate) fn root_traversal(&self) -> TraversalState {
        self.root_traversal
    }

    /// Returns the current full rectangle for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_rect(&self, roots: &[UiNode], id: UiNodeId) -> Option<Recti> {
        find_node(roots, id).map(|node| self.traversal_state_for_node(roots, id).screen_frame(node.state.layout))
    }

    /// Returns a parent-content-local rectangle in screen coordinates for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_local_rect(&self, roots: &[UiNode], id: UiNodeId, rect: Recti) -> Option<Recti> {
        let node = find_node(roots, id)?;
        let screen_frame = self.traversal_state_for_node(roots, id).screen_frame(node.state.layout);
        Some(Recti::new(
            rect.x + screen_frame.x - node.state.layout.frame.x,
            rect.y + screen_frame.y - node.state.layout.frame.y,
            rect.width,
            rect.height,
        ))
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
        let mut y = client.y;
        let mut content_bounds = None;
        let root_count = roots.len();
        for (index, root_node) in roots.iter_mut().enumerate() {
            let remaining_height = (client.y + client.height - y).max(0);
            let preferred = self.measure_node_ref(root_node, style, &atlas, Dimensioni::new(client.width, remaining_height));
            let height = if index + 1 == root_count {
                remaining_height
            } else {
                let policy = root_node.state.policy.height;
                resolve_size(policy, preferred.height, remaining_height, remaining_height, None).max(0)
            };
            let rect = Recti::new(client.x, y, client.width, height);
            self.layout_node_ref(root_node, style, &atlas, rect);
            let frame = root_node.state.layout.frame;
            let content_size = root_node.state.layout.content_size;
            let unscrolled = Recti::new(frame.x, frame.y, frame.width.max(content_size.width), frame.height.max(content_size.height));
            content_bounds = Some(match content_bounds {
                Some(bounds) => union_rect(bounds, unscrolled),
                None => unscrolled,
            });
            y = frame.y + frame.height + style.spacing;
        }

        let content_size = content_bounds
            .map(|bounds| Dimensioni::new((bounds.x + bounds.width - client.x).max(0), (bounds.y + bounds.height - client.y).max(0)))
            .unwrap_or_default();
        self.root_content_size = content_size;
        content_size
    }

    /// Measures one already-borrowed node's preferred size.
    pub(super) fn measure_node_ref(&self, node: &UiNode, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let ctx = MeasureCtx { runtime: self, style, atlas };
        match &node.data {
            UiNodeData::Widget(widget) => widget.measure(&ctx, node.state(), available),
            UiNodeData::Container(container) => container.measure(&ctx, node.state(), available),
        }
    }

    /// Lays out one already-borrowed node through its behavior.
    pub(super) fn layout_node_ref(&mut self, node: &mut UiNode, style: &Style, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        let is_branch = node.is_container();
        if is_branch {
            node.set_layout(NodeLayout::from_rect(rect, node.state.layout.content_size));
        }

        let mut ctx = LayoutCtx { runtime: self, style, atlas };
        match &mut node.data {
            UiNodeData::Widget(widget) => widget.layout(&mut ctx, &mut node.state, rect),
            UiNodeData::Container(container) => container.layout(&mut ctx, &mut node.state, rect),
        }

        let propagate_child_overflow = node.state.layout.propagate_child_overflow;
        if is_branch && propagate_child_overflow {
            let content_rect = child_content_bounds_from_children(node.children()).unwrap_or(rect);
            let content_size = Dimensioni::new(
                (content_rect.x + content_rect.width - rect.x).max(0),
                (content_rect.y + content_rect.height - rect.y).max(0),
            );
            node.set_layout(node.state.layout.with_content_size(content_size));
        }
        Dimensioni::new(rect.width, rect.height)
    }

    /// Updates one already-borrowed node and descendants.
    pub(super) fn update_node_ref(
        &mut self,
        root_id: crate::RootId,
        root_name: &str,
        node: &mut UiNode,
        parent_traversal: TraversalState,
        style: &Style,
        atlas: crate::AtlasHandle,
        input: &Input,
        results: &mut FrameResults,
    ) {
        let traversal = parent_traversal.enter(node.state.layout);
        let traverse_children = {
            let mut ctx = UpdateCtx {
                runtime: self,
                root_id,
                root_name,
                style,
                atlas: atlas.clone(),
                input,
                results,
                traversal,
            };
            match &mut node.data {
                UiNodeData::Widget(widget) => widget.update(&mut ctx, &mut node.state),
                UiNodeData::Container(container) => container.update(&mut ctx, &mut node.state),
            }
        };
        if traverse_children {
            if let Some(children) = node.children_mut() {
                for child in children {
                    self.update_node_ref(root_id, root_name, child, traversal, style, atlas.clone(), input, results);
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
        scroll_behavior: ScrollBehavior,
        focus_policy: FocusPolicy,
    ) -> (bool, bool, bool, bool, Option<Vec2i>) {
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
        let scroll_delta = if scroll_behavior.is_grab_scroll() && hovered && (input.scroll_delta.x != 0 || input.scroll_delta.y != 0) {
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
            consumed |= self.route_focus_input_event(roots, style, input, &event);
        }
        for event in held_events_from_input(input) {
            consumed |= self.route_focus_input_event(roots, style, input, &event);
        }
        consumed
    }

    /// Routes one pointer event to the capturing node, if there is one.
    pub(crate) fn route_captured_pointer_input_event(&mut self, roots: &mut [UiNode], style: &Style, input: &Input, event: &UiInputEvent) -> Option<bool> {
        let capture = self.capture.filter(|id| contains_node_in(roots, *id))?;
        let traversal = self.traversal_state_for_node(roots, capture);
        let result = self.route_input_event_to_node_only(roots, capture, traversal, style, input, event);
        self.update_pointer_capture(capture, result, event, input);
        Some(result.is_consumed())
    }

    /// Routes keyboard/text input to the focused node only.
    fn route_focus_input_event(&mut self, roots: &mut [UiNode], style: &Style, input: &Input, event: &UiInputEvent) -> bool {
        let Some(focus) = self.focus.filter(|id| contains_node_in(roots, *id)) else {
            return false;
        };
        let traversal = self.traversal_state_for_node(roots, focus);
        self.route_input_event_to_node_only(roots, focus, traversal, style, input, event).is_consumed()
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
        parent_traversal: TraversalState,
        style: &Style,
        input: &Input,
        event: &UiInputEvent,
    ) -> Option<(UiNodeId, InputResult)> {
        let id = node.id();
        let traversal = parent_traversal.enter(node.state.layout);
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut().rev() {
                if let Some(result) = self.route_input_event_to_node_ref(child, traversal, style, input, event) {
                    return Some(result);
                }
            }
        }

        let result = self.route_input_event_to_node_only_ref(node, traversal, style, input, event);
        result.is_consumed().then_some((id, result))
    }

    /// Routes an event to exactly one node behavior without traversing descendants.
    fn route_input_event_to_node_only(
        &mut self,
        roots: &mut [UiNode],
        id: UiNodeId,
        traversal: TraversalState,
        style: &Style,
        input: &Input,
        event: &UiInputEvent,
    ) -> InputResult {
        find_node_mut(roots, id)
            .map(|node| self.route_input_event_to_node_only_ref(node, traversal, style, input, event))
            .unwrap_or(InputResult::Ignored)
    }

    /// Routes an event to exactly one borrowed node behavior without traversing descendants.
    fn route_input_event_to_node_only_ref(
        &mut self,
        node: &mut UiNode,
        traversal: TraversalState,
        style: &Style,
        input: &Input,
        event: &UiInputEvent,
    ) -> InputResult {
        let mut ctx = InputCtx { runtime: self, style, input, traversal };
        match &mut node.data {
            UiNodeData::Widget(widget) => widget.update_on(&mut ctx, &mut node.state, event),
            UiNodeData::Container(container) => container.update_on(&mut ctx, &mut node.state, event),
        }
    }

    /// Paints one already-borrowed node and descendants.
    pub(super) fn paint_node_ref(&mut self, node: &mut UiNode, parent_traversal: TraversalState, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
        let traversal = parent_traversal.enter(node.state.layout);
        let traverse_children = {
            let mut ctx = PaintCtx {
                runtime: self,
                style,
                atlas: atlas.clone(),
                input,
                traversal,
            };
            match &mut node.data {
                UiNodeData::Widget(widget) => widget.paint(&mut ctx, &mut node.state),
                UiNodeData::Container(container) => container.paint(&mut ctx, &mut node.state),
            }
        };
        if traverse_children {
            if let Some(children) = node.children_mut() {
                for child in children {
                    self.paint_node_ref(child, traversal, style, atlas.clone(), input);
                }
            }
        }
    }

    /// Derives this node's traversal state from its parent traversal state.
    pub(super) fn node_traversal_state(&self, roots: &[UiNode], id: UiNodeId, parent: TraversalState) -> TraversalState {
        find_node(roots, id).map(|node| parent.enter(node.state.layout)).unwrap_or(parent)
    }

    /// Derives traversal state for one node by walking parent links on the call stack.
    pub(super) fn traversal_state_for_node(&self, roots: &[UiNode], id: UiNodeId) -> TraversalState {
        let parent_state = self
            .contains_node(roots, id)
            .then(|| self.parent_of(roots, id))
            .flatten()
            .map(|parent| self.traversal_state_for_node(roots, parent))
            .unwrap_or(self.root_traversal);
        self.node_traversal_state(roots, id, parent_state)
    }

    /// Returns the current effective clip rectangle.
    pub(super) fn current_clip_rect(&self) -> Recti {
        self.clip_stack.last().copied().unwrap_or(UNCLIPPED_RECT)
    }

    /// Pushes the traversal-derived clip for a node.
    pub(super) fn push_node_clip_for_traversal(&mut self, _id: UiNodeId, traversal: TraversalState) {
        let current = self.current_clip_rect();
        let effective = current.intersect(&traversal.screen_clip).unwrap_or_default();
        self.clip_stack.push(effective);
        self.commands.push(Command::PushClip { rect: effective });
    }

    /// Pops a node clip pushed by [`Self::push_node_clip_for_traversal`].
    pub(super) fn pop_node_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
        }
        self.commands.push(Command::PopClip);
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

fn transfer_node_runtime_state(node: &mut UiNode, previous_roots: &[UiNode]) {
    if let Some(previous_node) = find_node(previous_roots, node.id()) {
        node.state.layout = previous_node.state.layout;
        node.state.visible = previous_node.state.visible;
        node.state.enabled = previous_node.state.enabled;
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
