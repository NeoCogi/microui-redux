use std::collections::HashMap;
use super::*;

#[derive(Default)]
pub(crate) struct UiRuntime {
    /// Runtime nodes keyed by stable id.
    pub(crate) nodes: HashMap<UiNodeId, UiNode>,
    /// Root nodes in submission order.
    pub(crate) roots: Vec<UiNodeId>,
    /// Root nodes in z-order.
    pub(crate) z_order: Vec<UiNodeId>,
    /// Focused node.
    pub(crate) focus: Option<UiNodeId>,
    /// Hovered node.
    pub(crate) hover: Option<UiNodeId>,
    /// Pointer-capturing node.
    pub(crate) capture: Option<UiNodeId>,
    /// Root currently owning hover routing.
    pub(crate) hover_root: Option<UiNodeId>,
    /// Whether this runtime's current root owns pointer routing for the frame.
    pub(super) hover_root_active: bool,
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
    /// Active screen-space clip stack.
    pub(super) clip_stack: Vec<Recti>,
    /// Whether focus was refreshed or changed this frame.
    pub(super) updated_focus: bool,
}

impl UiRuntime {
    /// Creates an empty runtime.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Builds runtime nodes by consuming a retained UI node set.
    pub(crate) fn from_ui_nodes(tree: UiNodeSet) -> Self {
        let (roots, nodes) = tree.into_parts();
        let mut runtime = Self::new();
        let root_window = runtime_root_id();
        runtime.nodes.insert(
            root_window,
            UiNode::new(
                root_window,
                None,
                crate::Policy::auto(),
                GridSpan::ONE,
                UiNodeData::Branch {
                    behavior: Box::new(RootWindow::default()),
                    children: Vec::new(),
                },
            ),
        );
        runtime.nodes.extend(nodes);
        let mut root_children = Vec::new();
        for root in roots {
            if let Some(node) = runtime.nodes.get_mut(&root) {
                node.parent = Some(root_window);
            }
            root_children.push(root);
        }
        if let Some(children) = runtime.nodes.get_mut(&root_window).and_then(UiNode::children_mut) {
            *children = root_children;
        }
        runtime.roots.push(root_window);
        runtime.z_order.push(root_window);
        runtime
    }

    /// Replaces all runtime nodes by consuming a fresh retained nodes.
    pub(crate) fn replace_ui_nodes(&mut self, tree: UiNodeSet) {
        let mut next = Self::from_ui_nodes(tree);
        next.transfer_runtime_state_from(self);
        *self = next;
    }

    /// Moves focus to a node in this runtime.
    pub(crate) fn set_focus_node(&mut self, node: UiNodeId) {
        if self.nodes.contains_key(&node) {
            self.focus = Some(node);
            self.updated_focus = true;
        }
    }

    /// Measures the outer root size needed for `AUTO_SIZE` node roots.
    pub(crate) fn measure_auto_size(&self, style: &Style, atlas: &crate::AtlasHandle, opt: ContainerOption, min_width: i32) -> Dimensioni {
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
        for index in 0..self.roots.len() {
            let Some(root) = self.root_at(index) else { continue };
            let preferred = self.measure_node(root, style, atlas, available);
            width = width.max(preferred.width);
            height = height.saturating_add(preferred.height);
            if index + 1 < self.roots.len() {
                height = height.saturating_add(style.spacing);
            }
        }
        Dimensioni::new(
            width.saturating_add(horizontal_padding).max(min_width).max(1),
            height.saturating_add(padding.saturating_mul(2)).saturating_add(title_height).max(1),
        )
    }

    /// Returns a read-only node context.
    pub(crate) fn node_ctx(&mut self, id: UiNodeId) -> NodeCtx<'_> {
        NodeCtx { runtime: self, id }
    }

    /// Runs a minimal enum-based frame and replays the recorded commands immediately.
    ///
    /// The pre-update layout establishes hit targets for input and scroll dispatch. `UpdateCtx` may
    /// then add new children or remove direct child subtrees immediately. A second layout runs after
    /// update so paint observes the post-update tree. Later passes keep child topology read-only.
    pub(crate) fn render_frame<R: Renderer>(
        &mut self,
        root_id: crate::RootId,
        root_name: &str,
        canvas: &mut Canvas<R>,
        style: &Style,
        input: &Input,
        results: &mut FrameResults,
        body: Recti,
        scroll_behavior: ScrollBehavior,
        hover_root_active: bool,
    ) {
        self.commands.clear();
        self.triangle_vertices.clear();
        self.clip_stack.clear();
        self.clip_stack.push(UNCLIPPED_RECT);
        self.updated_focus = false;
        self.hover_root_active = hover_root_active;
        self.hover_root = hover_root_active.then(|| self.roots.first().copied()).flatten();
        let _ = scroll_behavior;
        let body_view = root_window_body_view(body, style);

        self.layout_roots_in_view(style, canvas.get_atlas(), body_view, body);
        self.route_input_events(style, input);
        self.layout_roots_in_view(style, canvas.get_atlas(), body_view, body);

        let mut root_index = 0;
        while let Some(root) = self.root_at(root_index) {
            self.update_node(root_id, root_name, root, style, canvas.get_atlas(), input, results);
            root_index += 1;
        }

        self.layout_roots_in_view(style, canvas.get_atlas(), body_view, body);

        let mut root_index = 0;
        while let Some(root) = self.root_at(root_index) {
            self.paint_node(root, style, canvas.get_atlas(), input);
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

    /// Returns the first synthetic root-window content size.
    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self) -> Dimensioni {
        self.root_window_content_size()
    }

    /// Returns the current full rectangle for a retained node.
    #[cfg(test)]
    pub(crate) fn debug_node_rect(&self, id: UiNodeId) -> Option<Recti> {
        self.nodes.get(&id).map(|node| node.rect)
    }

    /// Returns a root id by traversal index.
    pub(super) fn root_at(&self, index: usize) -> Option<UiNodeId> {
        self.roots.get(index).copied()
    }

    /// Returns the number of children on a container node.
    pub(super) fn child_count(&self, node: UiNodeId) -> usize {
        self.nodes.get(&node).map(|node| node.children().len()).unwrap_or(0)
    }

    /// Returns a child id by traversal index.
    pub(super) fn child_at(&self, node: UiNodeId, index: usize) -> Option<UiNodeId> {
        self.nodes.get(&node).and_then(|node| node.children().get(index).copied())
    }

    /// Returns a cloned node behavior object for traversal without holding a node borrow.
    pub(super) fn behavior_clone(&self, node: UiNodeId) -> Option<Box<dyn NodeBehavior>> {
        self.nodes.get(&node).and_then(|node| match &node.data {
            UiNodeData::Leaf { behavior } | UiNodeData::Branch { behavior, .. } => Some(behavior.clone()),
        })
    }

    /// Replaces a node behavior object after it mutates its own state.
    pub(super) fn set_behavior(&mut self, node: UiNodeId, replacement: Box<dyn NodeBehavior>) {
        if let Some(data) = self.nodes.get_mut(&node).map(|node| &mut node.data) {
            match data {
                UiNodeData::Leaf { behavior } | UiNodeData::Branch { behavior, .. } => *behavior = replacement,
            }
        }
    }

    /// Inserts a new node as a child of `parent` immediately.
    ///
    /// This is the only topology-add primitive used after initialization. It rejects reparenting:
    /// the child must not already exist in the runtime and must not already name a parent. Containers
    /// may call this through `UpdateCtx`; measure, layout, paint, and scroll dispatch contexts keep
    /// child membership read-only.
    pub(super) fn insert_child_immediate(&mut self, parent: UiNodeId, mut child: UiNode, index: usize) -> Option<UiNodeId> {
        if child.parent.is_some() || self.nodes.contains_key(&child.id) {
            return None;
        }
        let child_id = child.id;
        let children_len = self.nodes.get(&parent).and_then(|node| match &node.data {
            UiNodeData::Branch { children, .. } => Some(children.len()),
            _ => None,
        })?;

        child.parent = Some(parent);
        self.nodes.insert(child_id, child);
        let insert_at = index.min(children_len);
        if let Some(children) = self.nodes.get_mut(&parent).and_then(UiNode::children_mut) {
            children.insert(insert_at, child_id);
            Some(child_id)
        } else {
            self.nodes.remove(&child_id);
            None
        }
    }

    /// Removes a direct child and its subtree immediately.
    ///
    /// Reparenting is intentionally unsupported, so removal is the only way a node leaves its
    /// parent. Update traversal snapshots child ids before visiting them and skips ids that are no
    /// longer direct children.
    pub(super) fn remove_child_immediate(&mut self, parent: UiNodeId, child: UiNodeId) -> bool {
        if self.nodes.get(&child).and_then(|node| node.parent) != Some(parent) {
            return false;
        }
        self.remove_subtree(child);
        true
    }

    /// Removes a node from its parent child list.
    pub(super) fn detach_from_parent(&mut self, node: UiNodeId) {
        let parent = self.nodes.get(&node).and_then(|node| node.parent);
        if let Some(parent) = parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent) {
                if let Some(children) = parent_node.children_mut() {
                    children.retain(|child| *child != node);
                }
            }
        }
    }

    /// Removes a node and all descendants.
    pub(super) fn remove_subtree(&mut self, node: UiNodeId) {
        self.detach_from_parent(node);
        let mut removed = Vec::new();
        self.collect_subtree_nodes(node, &mut removed);
        self.clear_removed_transient_state(&removed);
        for removed_node in &removed {
            self.roots.retain(|root| *root != *removed_node);
            self.z_order.retain(|root| *root != *removed_node);
            self.nodes.remove(removed_node);
        }
    }

    /// Collects a node and all descendants in removal order.
    pub(super) fn collect_subtree_nodes(&self, node: UiNodeId, removed: &mut Vec<UiNodeId>) {
        removed.push(node);
        if let Some(node) = self.nodes.get(&node) {
            for child in node.children() {
                self.collect_subtree_nodes(*child, removed);
            }
        }
    }

    /// Clears transient runtime pointers that point into a removed subtree.
    pub(super) fn clear_removed_transient_state(&mut self, removed: &[UiNodeId]) {
        if self.focus.is_some_and(|id| removed.contains(&id)) {
            self.focus = None;
            self.updated_focus = true;
        }
        if self.hover.is_some_and(|id| removed.contains(&id)) {
            self.hover = None;
        }
        if self.capture.is_some_and(|id| removed.contains(&id)) {
            self.capture = None;
        }
        if self.hover_root.is_some_and(|id| removed.contains(&id)) {
            self.hover_root = None;
        }
    }

    /// Carries live runtime-only state into a newly submitted projection.
    pub(super) fn transfer_runtime_state_from(&mut self, previous: &UiRuntime) {
        for (id, node) in &mut self.nodes {
            let Some(previous_node) = previous.nodes.get(id) else {
                continue;
            };
            node.rect = previous_node.rect;
            node.client = previous_node.client;
            node.clip = previous_node.clip;
            node.client_area = previous_node.client_area;
            node.content_size = previous_node.content_size;
            node.visible = previous_node.visible;
            node.enabled = previous_node.enabled;
            node.control = previous_node.control;
            let (UiNodeData::Leaf { behavior } | UiNodeData::Branch { behavior, .. }) = &mut node.data;
            let (UiNodeData::Leaf { behavior: previous_behavior } | UiNodeData::Branch { behavior: previous_behavior, .. }) = &previous_node.data;
            behavior.transfer_runtime_state_from(previous_behavior.as_ref());
        }

        self.focus = previous.focus.filter(|id| self.nodes.contains_key(id));
        self.hover = previous.hover.filter(|id| self.nodes.contains_key(id));
        self.capture = previous.capture.filter(|id| self.nodes.contains_key(id));
        self.hover_root = previous.hover_root.filter(|id| self.nodes.contains_key(id));
        self.hover_root_active = previous.hover_root_active;
        self.updated_focus = previous.updated_focus && self.focus.is_some();
    }

    /// Lays out root nodes inside an already resolved root client area.
    pub(super) fn layout_roots_in_view(&mut self, style: &Style, atlas: crate::AtlasHandle, client: Recti, clip: Recti) -> Dimensioni {
        let mut y = client.y;
        let mut content_bounds = None;
        for index in 0..self.roots.len() {
            let Some(root) = self.root_at(index) else { continue };
            let remaining_height = (client.y + client.height - y).max(0);
            let preferred = self.measure_node(root, style, &atlas, Dimensioni::new(client.width, remaining_height));
            let height = if index + 1 == self.roots.len() {
                remaining_height
            } else {
                preferred.height.max(0)
            };
            let is_root_window = self.behavior_clone(root).map(|container| container.is_root_window()).unwrap_or(false);
            let rect = if is_root_window {
                clip
            } else {
                Recti::new(client.x, y, client.width, height)
            };
            self.layout_node(root, style, &atlas, rect, clip);
            if let Some(node) = self.nodes.get(&root) {
                let base = if is_root_window { node.client } else { node.rect };
                let unscrolled = Recti::new(
                    base.x,
                    base.y,
                    base.width.max(node.content_size.width),
                    base.height.max(node.content_size.height),
                );
                content_bounds = Some(match content_bounds {
                    Some(bounds) => union_rect(bounds, unscrolled),
                    None => unscrolled,
                });
                y = node.rect.y + node.rect.height + style.spacing;
            }
        }

        let content_size = content_bounds
            .map(|bounds| Dimensioni::new((bounds.x + bounds.width - client.x).max(0), (bounds.y + bounds.height - client.y).max(0)))
            .unwrap_or_default();
        for index in 0..self.roots.len() {
            if let Some(root) = self.root_at(index).and_then(|root| self.nodes.get_mut(&root)) {
                root.content_size = content_size;
            }
        }
        content_size
    }

    /// Returns the first root content size in the root-window client coordinate space.
    pub(super) fn root_window_content_size(&self) -> Dimensioni {
        self.roots
            .first()
            .and_then(|root| self.nodes.get(root))
            .map(|node| node.content_size)
            .unwrap_or_default()
    }

    /// Measures one node's preferred size in the box-tree layout path.
    pub(super) fn measure_node(&self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let ctx = MeasureCtx { runtime: self, style, atlas };
        self.behavior_clone(id)
            .map(|behavior| behavior.measure(&ctx, id, available))
            .unwrap_or_default()
    }

    /// Lays out one node through its behavior.
    pub(super) fn layout_node(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) -> Dimensioni {
        let is_branch = self.nodes.get(&id).is_some_and(|node| matches!(node.data, UiNodeData::Branch { .. }));
        if is_branch {
            let client_area = ClientArea::from_rect(rect);
            let node_clip = client_area.effective_clip(clip);
            if let Some(node) = self.nodes.get_mut(&id) {
                node.rect = rect;
                node.client = client_area.visible_rect;
                node.clip = node_clip;
                node.client_area = client_area;
            }
        }

        let mut behavior = self.behavior_clone(id);
        if let Some(behavior) = behavior.as_mut() {
            let node_clip = if is_branch {
                self.nodes
                    .get(&id)
                    .map(|node| node.clip)
                    .unwrap_or_else(|| clip.intersect(&rect).unwrap_or_default())
            } else {
                clip
            };
            let mut ctx = LayoutCtx { runtime: self, style, atlas };
            behavior.layout(&mut ctx, id, rect, node_clip);
        }
        let is_scroll_area = behavior.as_ref().is_some_and(|behavior| behavior.is_scroll_area());
        let is_root_window = behavior.as_ref().is_some_and(|behavior| behavior.is_root_window());
        if let Some(behavior) = behavior {
            self.set_behavior(id, behavior);
        }

        if is_scroll_area {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.content_size = Dimensioni::new(rect.width.max(0), rect.height.max(0));
            }
        } else if is_branch && !is_root_window {
            let content_rect = self.child_content_bounds(id).unwrap_or(rect);
            let content_size = Dimensioni::new(
                (content_rect.x + content_rect.width - rect.x).max(0),
                (content_rect.y + content_rect.height - rect.y).max(0),
            );
            if let Some(node) = self.nodes.get_mut(&id) {
                node.content_size = content_size;
            }
        }
        Dimensioni::new(rect.width, rect.height)
    }

    /// Returns the union of child rectangles, including each child's measured content overflow.
    pub(super) fn child_content_bounds(&self, id: UiNodeId) -> Option<Recti> {
        let mut bounds = None;
        for index in 0..self.child_count(id) {
            let child_rect = self.child_at(id, index).and_then(|child| self.nodes.get(&child)).map(child_content_rect)?;
            bounds = Some(match bounds {
                Some(rect) => union_rect(rect, child_rect),
                None => child_rect,
            });
        }
        bounds
    }

    /// Returns the effective vertical placement policy for a child in a column.
    pub(super) fn vertical_child_policy(&self, child: UiNodeId) -> SizePolicy {
        let policy = self.nodes.get(&child).map(|node| node.policy.height).unwrap_or(SizePolicy::Auto);
        if policy != SizePolicy::Auto {
            return policy;
        }
        self.behavior_clone(child)
            .and_then(|container| container.vertical_child_policy())
            .unwrap_or(SizePolicy::Auto)
    }

    /// Returns the effective horizontal placement policy for a child in a row.
    pub(super) fn horizontal_track_policy(&self, child: UiNodeId, track: SizePolicy) -> SizePolicy {
        let policy = self.nodes.get(&child).map(|node| node.policy.width).unwrap_or(SizePolicy::Auto);
        if policy != SizePolicy::Auto { policy } else { track }
    }

    /// Updates one node and descendants.
    pub(super) fn update_node(
        &mut self,
        root_id: crate::RootId,
        root_name: &str,
        id: UiNodeId,
        style: &Style,
        atlas: crate::AtlasHandle,
        input: &Input,
        results: &mut FrameResults,
    ) {
        let mut behavior = self.behavior_clone(id);
        let traverse_children = behavior
            .as_mut()
            .map(|behavior| {
                let mut ctx = UpdateCtx {
                    runtime: self,
                    root_id,
                    root_name,
                    style,
                    atlas: atlas.clone(),
                    input,
                    results,
                };
                behavior.update(&mut ctx, id)
            })
            .unwrap_or(true);
        if traverse_children {
            let children: Vec<_> = (0..self.child_count(id)).filter_map(|index| self.child_at(id, index)).collect();
            for child in children {
                if self.nodes.get(&child).and_then(|node| node.parent) == Some(id) {
                    self.update_node(root_id, root_name, child, style, atlas.clone(), input, results);
                }
            }
        }
        if let Some(behavior) = behavior {
            self.set_behavior(id, behavior);
        }
    }

    /// Computes control state from node geometry and shared input.
    pub(super) fn control_for(
        &mut self,
        id: UiNodeId,
        rect: Recti,
        input: &Input,
        opt: WidgetOption,
        scroll_behavior: ScrollBehavior,
        focus_policy: FocusPolicy,
    ) -> ControlState {
        if opt.intersects(WidgetOption::NO_INTERACT) {
            return ControlState::default();
        }

        let clip = self.nodes.get(&id).map(|node| node.clip).unwrap_or(UNCLIPPED_RECT);
        let hovered = self.hover_root_active && rect.contains(&input.mouse_pos) && clip.contains(&input.mouse_pos);
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
        ControlState {
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        }
    }

    /// Routes pre-update input events to the deepest eligible owner below the root.
    pub(super) fn route_input_events(&mut self, style: &Style, input: &Input) -> bool {
        let mut consumed = false;
        if self.hover_root_active || self.capture.is_some() {
            if !input.mouse_pressed.is_empty() {
                let event = UiInputEvent::MouseDown {
                    pos: input.mouse_pos,
                    button: input.mouse_pressed,
                };
                consumed |= self.route_input_event(style, input, &event);
            }
            if !input.mouse_released.is_empty() {
                let event = UiInputEvent::MouseUp {
                    pos: input.mouse_pos,
                    button: input.mouse_released,
                };
                consumed |= self.route_input_event(style, input, &event);
            }
            if input.mouse_delta.x != 0 || input.mouse_delta.y != 0 {
                let event = if input.mouse_down.is_empty() {
                    UiInputEvent::MouseMove {
                        pos: input.mouse_pos,
                        delta: input.mouse_delta,
                    }
                } else {
                    UiInputEvent::MouseDrag {
                        pos: input.mouse_pos,
                        delta: input.mouse_delta,
                        buttons: input.mouse_down,
                    }
                };
                consumed |= self.route_input_event(style, input, &event);
            }
            if input.scroll_delta.x != 0 || input.scroll_delta.y != 0 {
                let event = UiInputEvent::Scroll {
                    pos: input.mouse_pos,
                    delta: input.scroll_delta,
                };
                consumed |= self.route_input_event(style, input, &event);
            }
        }
        if !input.key_pressed.is_empty() {
            let event = UiInputEvent::KeyDown { key: input.key_pressed };
            consumed |= self.route_input_event(style, input, &event);
        }
        if !input.key_released.is_empty() {
            let event = UiInputEvent::KeyUp { key: input.key_released };
            consumed |= self.route_input_event(style, input, &event);
        }
        if !input.key_code_pressed.is_empty() {
            let event = UiInputEvent::KeyCodeDown { code: input.key_code_pressed };
            consumed |= self.route_input_event(style, input, &event);
        }
        if !input.key_code_released.is_empty() {
            let event = UiInputEvent::KeyCodeUp { code: input.key_code_released };
            consumed |= self.route_input_event(style, input, &event);
        }
        if !input.input_text.is_empty() {
            let event = UiInputEvent::Text { text: input.input_text.clone() };
            consumed |= self.route_input_event(style, input, &event);
        }
        consumed
    }

    /// Routes one input event through roots in z-order.
    pub(super) fn route_input_event(&mut self, style: &Style, input: &Input, event: &UiInputEvent) -> bool {
        if event.is_focus_input() {
            return self.route_focus_input_event(style, input, event);
        }
        if event.is_pointer() {
            return self.route_pointer_input_event(style, input, event);
        }
        self.route_hit_input_event(style, input, event).is_some()
    }

    /// Routes keyboard/text input to the focused node only.
    fn route_focus_input_event(&mut self, style: &Style, input: &Input, event: &UiInputEvent) -> bool {
        let Some(focus) = self.focus.filter(|id| self.nodes.contains_key(id)) else {
            return false;
        };
        self.route_input_event_to_node_only(focus, style, input, event).is_consumed()
    }

    /// Routes pointer input through capture first, then through normal hit traversal.
    fn route_pointer_input_event(&mut self, style: &Style, input: &Input, event: &UiInputEvent) -> bool {
        if let Some(capture) = self.capture.filter(|id| self.nodes.contains_key(id)) {
            let result = self.route_input_event_to_node_only(capture, style, input, event);
            self.update_pointer_capture(capture, result, event, input);
            return result.is_consumed();
        }

        if !self.hover_root_active {
            return false;
        }

        let Some((owner, result)) = self.route_hit_input_event(style, input, event) else {
            return false;
        };
        self.update_pointer_capture(owner, result, event, input);
        result.is_consumed()
    }

    /// Applies runtime pointer-capture ownership from one routed event result.
    fn update_pointer_capture(&mut self, owner: UiNodeId, result: InputResult, event: &UiInputEvent, input: &Input) {
        if event.is_pointer_release() && input.mouse_down.is_empty() {
            self.capture = None;
        } else if result == InputResult::Captured {
            self.capture = Some(owner);
        } else if self.capture == Some(owner) && input.mouse_down.is_empty() {
            self.capture = None;
        }
    }

    /// Routes an event through roots in z-order and returns the consumed owner.
    fn route_hit_input_event(&mut self, style: &Style, input: &Input, event: &UiInputEvent) -> Option<(UiNodeId, InputResult)> {
        for index in (0..self.roots.len()).rev() {
            let Some(root) = self.root_at(index) else { continue };
            if let Some(result) = self.route_input_event_to_node(root, style, input, event) {
                return Some(result);
            }
        }
        None
    }

    /// Walks children first so nested owners beat ancestors.
    pub(super) fn route_input_event_to_node(&mut self, id: UiNodeId, style: &Style, input: &Input, event: &UiInputEvent) -> Option<(UiNodeId, InputResult)> {
        for index in (0..self.child_count(id)).rev() {
            let Some(child) = self.child_at(id, index) else { continue };
            if let Some(result) = self.route_input_event_to_node(child, style, input, event) {
                return Some(result);
            }
        }

        let result = self.route_input_event_to_node_only(id, style, input, event);
        result.is_consumed().then_some((id, result))
    }

    /// Routes an event to exactly one node behavior without traversing descendants.
    fn route_input_event_to_node_only(&mut self, id: UiNodeId, style: &Style, input: &Input, event: &UiInputEvent) -> InputResult {
        let Some(mut behavior) = self.behavior_clone(id) else {
            return InputResult::Ignored;
        };
        let mut ctx = InputCtx { runtime: self, style, input };
        let result = behavior.update_on(&mut ctx, id, event);
        self.set_behavior(id, behavior);
        result
    }

    /// Paints one node and descendants.
    pub(super) fn paint_node(&mut self, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
        let mut behavior = self.behavior_clone(id);
        let traverse_children = behavior
            .as_mut()
            .map(|behavior| {
                let mut ctx = PaintCtx {
                    runtime: self,
                    style,
                    atlas: atlas.clone(),
                    input,
                };
                behavior.paint(&mut ctx, id)
            })
            .unwrap_or(true);
        if traverse_children {
            for index in 0..self.child_count(id) {
                let Some(child) = self.child_at(id, index) else { continue };
                self.paint_node(child, style, atlas.clone(), input);
            }
        }
        if let Some(behavior) = behavior {
            self.set_behavior(id, behavior);
        }
    }

    /// Returns the current effective clip rectangle.
    pub(super) fn current_clip_rect(&self) -> Recti {
        self.clip_stack.last().copied().unwrap_or(UNCLIPPED_RECT)
    }

    /// Pushes a node's effective clip for widget drawing.
    pub(super) fn push_node_clip(&mut self, id: UiNodeId) {
        let clip = self.nodes.get(&id).map(|node| node.clip).unwrap_or(UNCLIPPED_RECT);
        let current = self.current_clip_rect();
        let effective = current.intersect(&clip).unwrap_or_default();
        self.clip_stack.push(effective);
        self.commands.push(Command::PushClip { rect: effective });
    }

    /// Pops a node clip pushed by [`Self::push_node_clip`].
    pub(super) fn pop_node_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
        }
        self.commands.push(Command::PopClip);
    }
}

/// Returns the root content viewport. Root/window layout clips to this rect but never scrolls it.
fn root_window_body_view(body: Recti, style: &Style) -> Recti {
    expand_rect(body, -style.padding)
}
