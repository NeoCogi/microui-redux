use std::collections::HashMap;
use std::rc::Rc;

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
                UiNodeData::Container {
                    container: Box::new(RootWindow::default()),
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

        self.layout_roots_in_view(style, canvas.get_atlas(), body_view);
        self.dispatch_scroll_input(style, input);
        self.layout_roots_in_view(style, canvas.get_atlas(), body_view);

        let mut root_index = 0;
        while let Some(root) = self.root_at(root_index) {
            self.update_node(root_id, root_name, root, style, canvas.get_atlas(), input, results);
            root_index += 1;
        }

        self.layout_roots_in_view(style, canvas.get_atlas(), body_view);

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

    /// Returns a cloned container trait object for traversal without holding a node borrow.
    pub(super) fn container_clone(&self, node: UiNodeId) -> Option<Box<dyn ContainerTrait>> {
        self.nodes.get(&node).and_then(|node| match &node.data {
            UiNodeData::Container { container, .. } => Some(container.clone()),
            _ => None,
        })
    }

    /// Replaces a container trait object after behavior mutates its own state.
    pub(super) fn set_container(&mut self, node: UiNodeId, replacement: Box<dyn ContainerTrait>) {
        if let Some(UiNodeData::Container { container, .. }) = self.nodes.get_mut(&node).map(|node| &mut node.data) {
            *container = replacement;
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
            UiNodeData::Container { children, .. } => Some(children.len()),
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
            if let (UiNodeData::Container { container, .. }, UiNodeData::Container { container: previous_container, .. }) =
                (&mut node.data, &previous_node.data)
            {
                container.transfer_runtime_state_from(previous_container.as_ref());
            }
        }

        self.focus = previous.focus.filter(|id| self.nodes.contains_key(id));
        self.hover = previous.hover.filter(|id| self.nodes.contains_key(id));
        self.capture = previous.capture.filter(|id| self.nodes.contains_key(id));
        self.hover_root = previous.hover_root.filter(|id| self.nodes.contains_key(id));
        self.hover_root_active = previous.hover_root_active;
        self.updated_focus = previous.updated_focus && self.focus.is_some();
    }

    /// Lays out root nodes inside an already resolved root client area.
    pub(super) fn layout_roots_in_view(&mut self, style: &Style, atlas: crate::AtlasHandle, client: Recti) -> Dimensioni {
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
            let is_root_window = self.container_clone(root).map(|container| container.is_root_window()).unwrap_or(false);
            let rect = if is_root_window {
                Recti::new(client.x, client.y, client.width, client.height)
            } else {
                Recti::new(client.x, y, client.width, height)
            };
            self.layout_node(root, style, &atlas, rect, client);
            if let Some(node) = self.nodes.get(&root) {
                let unscrolled = Recti::new(
                    node.rect.x,
                    node.rect.y,
                    node.rect.width.max(node.content_size.width),
                    node.rect.height.max(node.content_size.height),
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
        match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => {
                let policy = self.nodes.get(&id).map(|node| node.policy).unwrap_or_else(crate::Policy::auto);
                let measure_available = Dimensioni::new(
                    measure_axis_available(policy.width, available.width),
                    measure_axis_available(policy.height, available.height),
                );
                let preferred = widget.measure(style, atlas, measure_available);
                Dimensioni::new(
                    resolve_size(policy.width, preferred.width, available.width, available.width, None),
                    resolve_size(policy.height, preferred.height, available.height, available.height, None),
                )
            }
            Some(UiNodeData::Container { .. }) => self.measure_container(id, style, atlas, available),
            None => Dimensioni::default(),
        }
    }

    /// Measures a container node by delegating to its concrete container trait implementation.
    pub(super) fn measure_container(&self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let ctx = MeasureCtx { runtime: self, style, atlas };
        self.container_clone(id)
            .map(|container| container.measure(&ctx, id, available))
            .unwrap_or_default()
    }

    /// Lays out one node using the enum-based runtime path.
    pub(super) fn layout_node(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) -> Dimensioni {
        let kind = self.node_kind_tag(id);
        match kind {
            RuntimeNodeKind::Widget => self.layout_widget(id, style, atlas, rect, clip),
            RuntimeNodeKind::Container => self.layout_container(id, style, atlas, rect, clip),
        }
    }

    /// Returns whether a node is a widget or container without holding a borrow.
    pub(super) fn node_kind_tag(&self, id: UiNodeId) -> RuntimeNodeKind {
        match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { .. }) => RuntimeNodeKind::Widget,
            _ => RuntimeNodeKind::Container,
        }
    }

    /// Lays out a widget leaf.
    pub(super) fn layout_widget(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) -> Dimensioni {
        let policy = self.nodes.get(&id).map(|node| node.policy).unwrap_or_else(crate::Policy::auto);
        let measure_available = Dimensioni::new(
            measure_axis_available(policy.width, rect.width),
            measure_axis_available(policy.height, rect.height),
        );
        let preferred = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.measure(style, atlas, measure_available),
            _ => Dimensioni::default(),
        };
        let rect = Recti::new(
            rect.x,
            rect.y,
            resolve_allocated_size(policy.width, preferred.width, rect.width, rect.width, None),
            resolve_allocated_size(policy.height, preferred.height, rect.height, rect.height, None),
        );
        let size = Dimensioni::new(rect.width, rect.height);
        if let Some(node) = self.nodes.get_mut(&id) {
            let client_area = ClientArea::from_rect(rect);
            node.rect = rect;
            node.client = client_area.visible_rect;
            node.clip = client_area.effective_clip(clip);
            node.client_area = client_area;
            node.content_size = size;
        }
        size
    }

    /// Lays out a container and descendants.
    pub(super) fn layout_container(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) -> Dimensioni {
        let client_area = ClientArea::from_rect(rect);
        let node_clip = client_area.effective_clip(clip);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.rect = rect;
            node.client = client_area.visible_rect;
            node.clip = node_clip;
            node.client_area = client_area;
        }

        self.layout_container_children(id, style, atlas, rect, node_clip);

        if self.container_clone(id).map(|container| container.is_scroll_area()).unwrap_or(false) {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.content_size = Dimensioni::new(rect.width.max(0), rect.height.max(0));
            }
            return Dimensioni::new(rect.width, rect.height);
        }
        if self.container_clone(id).map(|container| container.is_root_window()).unwrap_or(false) {
            return Dimensioni::new(rect.width, rect.height);
        }

        let content_rect = self.child_content_bounds(id).unwrap_or(rect);
        let content_size = Dimensioni::new(
            (content_rect.x + content_rect.width - rect.x).max(0),
            (content_rect.y + content_rect.height - rect.y).max(0),
        );
        if let Some(node) = self.nodes.get_mut(&id) {
            node.content_size = content_size;
        }
        Dimensioni::new(rect.width, rect.height)
    }

    /// Lays out a container node by delegating to its concrete container trait implementation.
    pub(super) fn layout_container_children(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        if let Some(mut container) = self.container_clone(id) {
            let mut ctx = LayoutCtx { runtime: self, style, atlas };
            container.layout(&mut ctx, id, rect, clip);
            self.set_container(id, container);
        }
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
        self.container_clone(child)
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
        let kind = self.node_kind_tag(id);
        match kind {
            RuntimeNodeKind::Widget => self.update_widget(root_id, root_name, id, style, atlas, input, results),
            RuntimeNodeKind::Container => {
                let mut container = self.container_clone(id);
                let traverse_children = container
                    .as_mut()
                    .map(|container| {
                        let mut ctx = UpdateCtx {
                            runtime: self,
                            root_id,
                            style,
                            atlas: atlas.clone(),
                            input,
                            results,
                        };
                        container.update(&mut ctx, id)
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
                if let Some(container) = container {
                    self.set_container(id, container);
                }
            }
        }
    }

    /// Updates a widget leaf.
    pub(super) fn update_widget(
        &mut self,
        root_id: crate::RootId,
        root_name: &str,
        id: UiNodeId,
        style: &Style,
        atlas: crate::AtlasHandle,
        input: &Input,
        results: &mut FrameResults,
    ) {
        let rect = self.nodes.get(&id).map(|node| node.rect).unwrap_or_default();
        let opt = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.effective_widget_opt(),
            _ => WidgetOption::NONE,
        };
        let scroll_behavior = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.effective_scroll_behavior(),
            _ => ScrollBehavior::NONE,
        };
        let focus_policy = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.focus_policy(),
            _ => FocusPolicy::Momentary,
        };
        let needs_input = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.needs_input_snapshot(),
            _ => false,
        };
        let control = self.control_for(id, rect, input, opt, scroll_behavior, focus_policy);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.control = control;
        }

        let mut focus_slot = self.focus.map(RetainedId::node);
        let mut focus_seen = self.updated_focus;
        let mut input_snapshot = needs_input.then(|| Rc::new(snapshot_from_input(input)));
        let result = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => {
                let mut ctx = WidgetCtx::new_with_interaction(
                    RetainedId::node(id),
                    rect,
                    &mut self.commands,
                    &mut self.triangle_vertices,
                    &mut self.clip_stack,
                    style,
                    &atlas,
                    &mut focus_slot,
                    &mut focus_seen,
                    self.hover_root_active,
                    input_snapshot.take(),
                );
                widget.update(&mut ctx, &control)
            }
            _ => ResourceState::NONE,
        };
        self.focus = retained_focus_to_node(focus_slot);
        self.updated_focus = focus_seen;

        let widget_handle_id = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.widget_handle_id(),
            _ => id,
        };
        results.record_retained_with_context(
            RetainedId::root_node(root_id, id),
            widget_handle_id,
            result,
            format!("root {root_name:?} ui node {:?}", id),
        );
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

    /// Dispatches scroll input to the deepest eligible owner below the root.
    pub(super) fn dispatch_scroll_input(&mut self, style: &Style, input: &Input) -> bool {
        if !self.hover_root_active || (input.scroll_delta.x == 0 && input.scroll_delta.y == 0 && input.mouse_down.is_empty() && input.mouse_pressed.is_empty())
        {
            return false;
        }
        for index in (0..self.roots.len()).rev() {
            let Some(root) = self.root_at(index) else { continue };
            if self.dispatch_scroll_input_to_node(root, style, input) {
                return true;
            }
        }
        false
    }

    /// Walks children first so nested scroll areas and scroll-grabbing widgets beat ancestors.
    pub(super) fn dispatch_scroll_input_to_node(&mut self, id: UiNodeId, style: &Style, input: &Input) -> bool {
        for index in (0..self.child_count(id)).rev() {
            let Some(child) = self.child_at(id, index) else { continue };
            if self.dispatch_scroll_input_to_node(child, style, input) {
                return true;
            }
        }

        match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => {
                let rect = self.nodes.get(&id).map(|node| node.rect).unwrap_or_default();
                let clip = self.nodes.get(&id).map(|node| node.clip).unwrap_or_default();
                let hovered = rect.contains(&input.mouse_pos) && clip.contains(&input.mouse_pos);
                hovered && widget.effective_scroll_behavior().is_grab_scroll() && (input.scroll_delta.x != 0 || input.scroll_delta.y != 0)
            }
            Some(UiNodeData::Container { .. }) => {
                let Some(mut container) = self.container_clone(id) else {
                    return false;
                };
                let mut ctx = ScrollDispatchCtx { runtime: self, style, input };
                let consumed = container.dispatch_scroll(&mut ctx, id);
                self.set_container(id, container);
                consumed
            }
            _ => false,
        }
    }

    /// Paints one node and descendants.
    pub(super) fn paint_node(&mut self, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
        let kind = self.node_kind_tag(id);
        match kind {
            RuntimeNodeKind::Widget => self.paint_widget(id, style, atlas, input),
            RuntimeNodeKind::Container => {
                let mut container = self.container_clone(id);
                let traverse_children = container
                    .as_mut()
                    .map(|container| {
                        let mut ctx = PaintCtx {
                            runtime: self,
                            style,
                            atlas: atlas.clone(),
                        };
                        container.paint_before_children(&mut ctx, id)
                    })
                    .unwrap_or(true);
                if traverse_children {
                    for index in 0..self.child_count(id) {
                        let Some(child) = self.child_at(id, index) else { continue };
                        self.paint_node(child, style, atlas.clone(), input);
                    }
                }
                if let Some(container) = container.as_mut() {
                    let mut ctx = PaintCtx { runtime: self, style, atlas };
                    container.paint_after_children(&mut ctx, id);
                }
                if let Some(container) = container {
                    self.set_container(id, container);
                }
            }
        }
    }

    /// Paints a widget leaf.
    pub(super) fn paint_widget(&mut self, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
        let rect = self.nodes.get(&id).map(|node| node.rect).unwrap_or_default();
        let control = self.nodes.get(&id).map(|node| node.control).unwrap_or_default();
        let needs_input = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.needs_input_snapshot(),
            _ => false,
        };
        let mut focus_slot = self.focus.map(RetainedId::node);
        let mut focus_seen = self.updated_focus;
        let mut input_snapshot = needs_input.then(|| Rc::new(snapshot_from_input(input)));
        let custom_render = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { custom_render, .. }) => custom_render.clone(),
            _ => None,
        };
        let node_clip = self.nodes.get(&id).map(|node| node.clip).unwrap_or(UNCLIPPED_RECT);
        self.push_node_clip(id);
        if let Some(UiNodeData::Widget { widget, .. }) = self.nodes.get(&id).map(|node| &node.data) {
            let mut ctx = WidgetCtx::new_with_interaction(
                RetainedId::node(id),
                rect,
                &mut self.commands,
                &mut self.triangle_vertices,
                &mut self.clip_stack,
                style,
                &atlas,
                &mut focus_slot,
                &mut focus_seen,
                true,
                input_snapshot.take(),
            );
            widget.paint(&mut ctx, &control);
        }
        self.pop_node_clip();
        self.focus = retained_focus_to_node(focus_slot);
        self.updated_focus = focus_seen;

        if let Some(render) = custom_render {
            let snapshot = snapshot_from_input(input);
            let active = control.focused;
            let view = node_clip.intersect(&rect).unwrap_or_else(|| Recti::new(rect.x, rect.y, 0, 0));
            let cra = CustomRenderArgs {
                content_area: rect,
                view,
                mouse_event: input_to_mouse_event(&control, &snapshot, rect),
                scroll_delta: control.scroll_delta,
                widget_opt: match self.nodes.get(&id).map(|node| &node.data) {
                    Some(UiNodeData::Widget { widget, .. }) => widget.effective_widget_opt(),
                    _ => WidgetOption::NONE,
                },
                scroll_behavior: match self.nodes.get(&id).map(|node| &node.data) {
                    Some(UiNodeData::Widget { widget, .. }) => widget.effective_scroll_behavior(),
                    _ => ScrollBehavior::NONE,
                },
                key_mods: if active { snapshot.key_mods } else { KeyMode::NONE },
                key_codes: if active { snapshot.key_codes } else { KeyCode::NONE },
                text_input: if active { snapshot.text_input } else { String::new() },
            };
            self.commands
                .push(Command::BackendCustomRender(cra, Box::new(NodeCustomRenderCommand { render })));
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

/// Coarse node kind used to route passes without holding a node borrow.
#[derive(Copy, Clone)]
pub(super) enum RuntimeNodeKind {
    /// Leaf widget.
    Widget,
    /// Container node.
    Container,
}
