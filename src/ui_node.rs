//! Common runtime node model used by the next retained traversal path.
//!
//! Internal retained UI node runtime.
//! It gives the crate one node representation that can own either a leaf widget or a framework
//! container without introducing a broad container trait before the enum-based passes exist.
#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use crate::{
    expand_rect, Canvas, ControlColor, CustomRenderArgs, CustomRenderCommand, Dimensioni, FrameResults, Id, Input, InputSnapshot, KeyCode, KeyMode, GridSpan,
    MouseButton, MouseEvent, Node, Recti, Renderer, RetainedId, StackDirection, Style, UNCLIPPED_RECT, Vec2i, Vertex, WidgetHandle, UiNodeSet,
};
use crate::container::{render_command_stream, Command, ScrollAreaHandle};
use crate::draw_context::DrawCtx;
use crate::id::IdNamespace;
use crate::input::{ContainerOption, ControlState, ResourceState, ScrollBehavior, WidgetOption};
use crate::sizing::SizePolicy;
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis};
use crate::widget::FocusPolicy;
use crate::widget_ctx::WidgetCtx;
use crate::context::{erased_widget_state, NodeLayout, TreeCustomRender, WidgetStateHandleDyn};

/// Command wrapper that lets node-runtime custom render callbacks enter the backend stream.
struct NodeCustomRenderCommand {
    /// Shared retained callback invoked during renderer replay.
    render: TreeCustomRender,
}

impl CustomRenderCommand for NodeCustomRenderCommand {
    fn render(&mut self, dim: Dimensioni, args: &CustomRenderArgs) {
        self.render.borrow_mut().render(dim, args);
    }
}

/// Stable internal id for the synthetic root-window container.
fn runtime_root_id() -> UiNodeId {
    IdNamespace::UINODE_ROOT.id([0])
}

/// Stable runtime node identifier.
pub(crate) type UiNodeId = Id;

/// Common runtime node state shared by widgets and containers.
pub(crate) struct UiNode {
    /// Stable runtime node id.
    pub(crate) id: UiNodeId,
    /// Parent node id when this node is nested under a container.
    pub(crate) parent: Option<UiNodeId>,
    /// Full screen-space node bounds.
    pub(crate) rect: Recti,
    /// Child/content area computed by the runtime.
    pub(crate) client: Recti,
    /// Effective visible clip after ancestor clips.
    pub(crate) clip: Recti,
    /// Measured child/content size.
    pub(crate) content_size: Dimensioni,
    /// Whether this node participates in traversal.
    pub(crate) visible: bool,
    /// Whether this node can interact.
    pub(crate) enabled: bool,
    /// Last control state produced for this node.
    pub(crate) control: ControlState,
    /// Placement policy used by runtime layout passes.
    pub(crate) policy: crate::Policy,
    /// Grid span used when this node is a child of a grid container.
    pub(crate) grid_span: GridSpan,
    /// Node-specific payload.
    pub(crate) data: UiNodeData,
}

impl UiNode {
    /// Creates a node with default geometry and traversal state.
    pub(crate) fn new(id: UiNodeId, parent: Option<UiNodeId>, policy: crate::Policy, grid_span: GridSpan, data: UiNodeData) -> Self {
        Self {
            id,
            parent,
            rect: Recti::default(),
            client: Recti::default(),
            clip: Recti::default(),
            content_size: Dimensioni::default(),
            visible: true,
            enabled: true,
            control: ControlState::default(),
            policy,
            grid_span,
            data,
        }
    }

    /// Returns the node's container children when it is a container.
    pub(crate) fn children(&self) -> &[UiNodeId] {
        match &self.data {
            UiNodeData::Widget { .. } => &[],
            UiNodeData::Container { children, .. } => children,
        }
    }

    /// Returns the node's mutable container children when it is a container.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<UiNodeId>> {
        match &mut self.data {
            UiNodeData::Widget { .. } => None,
            UiNodeData::Container { children, .. } => Some(children),
        }
    }
}

/// Runtime payload for a common UI node.
pub(crate) enum UiNodeData {
    /// Leaf widget node.
    Widget {
        /// Type-erased retained widget state.
        widget: Box<dyn WidgetStateHandleDyn>,
        /// Optional custom backend render callback for custom-render leaves.
        custom_render: Option<TreeCustomRender>,
    },
    /// Framework-owned container node.
    Container {
        /// Concrete child-owning container object.
        container: Box<dyn ContainerTrait>,
        /// Child membership. Leaf widgets do not carry this allocation.
        children: Vec<UiNodeId>,
    },
}

/// Top-level window root container.
#[derive(Clone)]
pub(crate) struct RootWindow {
    /// Root chrome/sizing options.
    pub(crate) opt: ContainerOption,
    /// Scroll behavior applied to the root body.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Current body scroll offset.
    pub(crate) scroll_offset: Vec2i,
    /// Active body scrollbar drag axis.
    pub(crate) scroll_drag: Option<ScrollAxis>,
    /// Root z-order value.
    pub(crate) z_index: i32,
}

impl Default for RootWindow {
    fn default() -> Self {
        Self {
            opt: ContainerOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            scroll_offset: Vec2i::default(),
            scroll_drag: None,
            z_index: 0,
        }
    }
}

/// Scroll-area container.
#[derive(Clone)]
pub(crate) struct ScrollArea {
    /// Retained scroll-area state shared with user handles.
    pub(crate) handle: ScrollAreaHandle,
    /// Internal scrollable content size.
    pub(crate) content_size: Dimensioni,
    /// Current scroll offset.
    pub(crate) scroll_offset: Vec2i,
    /// Active scrollbar drag axis.
    pub(crate) scroll_drag: Option<ScrollAxis>,
    /// Scroll behavior applied while traversing this node's children.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Rendering options applied to the scroll-area panel.
    pub(crate) opt: ContainerOption,
}

/// Header/tree disclosure container.
#[derive(Clone)]
pub(crate) struct Disclosure {
    /// Widget state for the disclosure row.
    pub(crate) state: WidgetHandle<Node>,
    /// Whether child layout should be indented when expanded.
    pub(crate) indent_children: bool,
}

/// Row container.
#[derive(Clone)]
pub(crate) struct Row {
    /// Width policies for row tracks.
    pub(crate) widths: Vec<SizePolicy>,
    /// Shared row height policy.
    pub(crate) height: SizePolicy,
}

/// Grid container.
#[derive(Clone)]
pub(crate) struct Grid {
    /// Width policies for columns.
    pub(crate) widths: Vec<SizePolicy>,
    /// Height policies for rows.
    pub(crate) heights: Vec<SizePolicy>,
}

/// Column container.
#[derive(Clone)]
pub(crate) struct Column;

/// Stack container.
#[derive(Clone)]
pub(crate) struct Stack {
    /// Width policy applied to emitted items.
    pub(crate) width: SizePolicy,
    /// Height policy applied to emitted items.
    pub(crate) height: SizePolicy,
    /// Stack direction.
    pub(crate) direction: StackDirection,
}

/// Deferred topology operation applied outside traversal.
pub(crate) enum TreeOp {
    /// Adds `child` under `parent`.
    AddChild { parent: UiNodeId, child: UiNodeId },
    /// Removes a node and its current parent membership.
    RemoveNode { node: UiNodeId },
    /// Moves a node under a new parent at an index.
    MoveNode { node: UiNodeId, new_parent: UiNodeId, index: usize },
    /// Replaces node-specific payload.
    ReplaceData { node: UiNodeId, data: UiNodeData },
}

/// Internal node owner that `Context` can adopt as its window-manager runtime.
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
    hover_root_active: bool,
    /// Deferred topology/data edits.
    pub(crate) deferred: Vec<TreeOp>,
    /// Commands recorded by the node paint path.
    commands: Vec<Command>,
    /// Snapshot of text commands before renderer replay drains them.
    #[cfg(test)]
    debug_texts: Vec<String>,
    /// Snapshot of rectangle commands before renderer replay drains them.
    #[cfg(test)]
    debug_rects: Vec<Recti>,
    /// Triangle vertex arena referenced by retained triangle commands.
    triangle_vertices: Vec<Vertex>,
    /// Active screen-space clip stack.
    clip_stack: Vec<Recti>,
    /// Whether focus was refreshed or changed this frame.
    updated_focus: bool,
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
        *self = Self::from_ui_nodes(tree);
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

    /// Applies and clears deferred topology operations.
    pub(crate) fn apply_deferred(&mut self) {
        let ops = std::mem::take(&mut self.deferred);
        for op in ops {
            self.apply_tree_op(op);
        }
    }

    /// Runs a minimal enum-based frame and replays the recorded commands immediately.
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
        self.configure_root_window(scroll_behavior);

        self.layout_roots_until_scrollbars_stable(style, canvas.get_atlas(), body, scroll_behavior);
        let scroll_consumed = self.dispatch_scroll_input(style, input);
        if !scroll_consumed {
            self.update_root_window_scroll(body, style, input, scroll_behavior);
        }
        self.layout_roots_until_scrollbars_stable(style, canvas.get_atlas(), body, scroll_behavior);

        let mut root_index = 0;
        while let Some(root) = self.root_at(root_index) {
            self.update_node(root_id, root_name, root, style, canvas.get_atlas(), input, results);
            root_index += 1;
        }

        let mut root_index = 0;
        while let Some(root) = self.root_at(root_index) {
            self.paint_node(root, style, canvas.get_atlas(), input);
            root_index += 1;
        }
        self.paint_root_window_scrollbars(body, style, &canvas.get_atlas(), scroll_behavior);

        if !self.updated_focus {
            self.focus = None;
        }
        self.apply_deferred();
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

    /// Applies one deferred tree operation.
    fn apply_tree_op(&mut self, op: TreeOp) {
        match op {
            TreeOp::AddChild { parent, child } => {
                self.detach_from_parent(child);
                if let Some(parent_node) = self.nodes.get_mut(&parent) {
                    if let Some(children) = parent_node.children_mut() {
                        children.push(child);
                    }
                }
                if let Some(child_node) = self.nodes.get_mut(&child) {
                    child_node.parent = Some(parent);
                }
            }
            TreeOp::RemoveNode { node } => {
                self.detach_from_parent(node);
                self.remove_subtree(node);
            }
            TreeOp::MoveNode { node, new_parent, index } => {
                self.detach_from_parent(node);
                if let Some(parent_node) = self.nodes.get_mut(&new_parent) {
                    if let Some(children) = parent_node.children_mut() {
                        let index = index.min(children.len());
                        children.insert(index, node);
                    }
                }
                if let Some(node) = self.nodes.get_mut(&node) {
                    node.parent = Some(new_parent);
                }
            }
            TreeOp::ReplaceData { node, data } => {
                if let Some(node) = self.nodes.get_mut(&node) {
                    node.data = data;
                }
            }
        }
    }

    /// Returns a root id by traversal index.
    fn root_at(&self, index: usize) -> Option<UiNodeId> {
        self.roots.get(index).copied()
    }

    /// Returns the number of children on a container node.
    fn child_count(&self, node: UiNodeId) -> usize {
        self.nodes.get(&node).map(|node| node.children().len()).unwrap_or(0)
    }

    /// Returns a child id by traversal index.
    fn child_at(&self, node: UiNodeId, index: usize) -> Option<UiNodeId> {
        self.nodes.get(&node).and_then(|node| node.children().get(index).copied())
    }

    /// Returns a cloned container trait object for traversal without holding a node borrow.
    fn container_clone(&self, node: UiNodeId) -> Option<Box<dyn ContainerTrait>> {
        self.nodes.get(&node).and_then(|node| match &node.data {
            UiNodeData::Container { container, .. } => Some(container.clone()),
            _ => None,
        })
    }

    /// Replaces a container trait object after behavior mutates its own state.
    fn set_container(&mut self, node: UiNodeId, replacement: Box<dyn ContainerTrait>) {
        if let Some(UiNodeData::Container { container, .. }) = self.nodes.get_mut(&node).map(|node| &mut node.data) {
            *container = replacement;
        }
    }

    /// Paints root scrollbars when node content exceeds the root client area.
    fn paint_root_window_scrollbars(&mut self, body: Recti, style: &Style, atlas: &crate::AtlasHandle, scroll_behavior: ScrollBehavior) {
        if scroll_behavior.is_no_scroll() {
            return;
        }
        let view = self.root_window_view_for_content(body, style, scroll_behavior, self.root_window_content_size());
        let content = self.root_window_content_size();
        let scroll_offset = self.root_window_scroll_offset();
        let scrollbar_size = style.scrollbar_size.max(0);
        if scrollbar_size <= 0 {
            return;
        }

        let mut draw = DrawCtx::new(&mut self.commands, &mut self.triangle_vertices, &mut self.clip_stack, style, atlas);
        if content.height > view.height {
            let base = scrollbar_base(ScrollAxis::Vertical, view, scrollbar_size);
            let thumb = scrollbar_thumb(ScrollAxis::Vertical, base, view.height, content.height, scroll_offset.y, scrollbar_size);
            draw.draw_frame(base, ControlColor::Base);
            draw.draw_frame(thumb, ControlColor::Button);
        }
        if content.width > view.width {
            let base = scrollbar_base(ScrollAxis::Horizontal, view, scrollbar_size);
            let thumb = scrollbar_thumb(ScrollAxis::Horizontal, base, view.width, content.width, scroll_offset.x, scrollbar_size);
            draw.draw_frame(base, ControlColor::Base);
            draw.draw_frame(thumb, ControlColor::Button);
        }
    }

    /// Removes a node from its parent child list.
    fn detach_from_parent(&mut self, node: UiNodeId) {
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
    fn remove_subtree(&mut self, node: UiNodeId) {
        while let Some(child) = self.nodes.get_mut(&node).and_then(UiNode::children_mut).and_then(Vec::pop) {
            self.remove_subtree(child);
        }
        self.roots.retain(|root| *root != node);
        self.z_order.retain(|root| *root != node);
        self.nodes.remove(&node);
    }

    /// Updates the synthetic root-window container behavior for this frame.
    fn configure_root_window(&mut self, scroll_behavior: ScrollBehavior) {
        if let Some(root) = self.roots.first().copied() {
            if let Some(mut container) = self.container_clone(root) {
                container.configure_root_scroll(scroll_behavior);
                self.set_container(root, container);
            }
        }
    }

    /// Returns the synthetic root-window scroll offset.
    fn root_window_scroll_offset(&self) -> Vec2i {
        self.roots
            .first()
            .and_then(|root| self.nodes.get(root))
            .and_then(|node| match &node.data {
                UiNodeData::Container { container, .. } => container.root_scroll_state().map(|(offset, _)| offset),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Returns the synthetic root-window active scrollbar drag axis.
    fn root_window_scroll_drag(&self) -> Option<ScrollAxis> {
        self.roots.first().and_then(|root| self.nodes.get(root)).and_then(|node| match &node.data {
            UiNodeData::Container { container, .. } => container.root_scroll_state().and_then(|(_, drag)| drag),
            _ => None,
        })
    }

    /// Stores synthetic root-window scroll interaction state.
    fn set_root_window_scroll_state(&mut self, offset: Vec2i, drag: Option<ScrollAxis>) {
        if let Some(root) = self.roots.first().copied() {
            if let Some(mut container) = self.container_clone(root) {
                container.set_root_scroll_state(offset, drag);
                self.set_container(root, container);
            }
        }
    }

    /// Lays out root nodes, repeating until current-frame content and scrollbar gutters agree.
    fn layout_roots_until_scrollbars_stable(&mut self, style: &Style, atlas: crate::AtlasHandle, body: Recti, scroll_behavior: ScrollBehavior) {
        let mut content_hint = self.root_window_content_size();
        let mut layout_content = Dimensioni::default();
        for _ in 0..3 {
            let client = self.root_window_view_for_content(body, style, scroll_behavior, content_hint);
            layout_content = self.layout_roots_in_view(style, atlas.clone(), client);
            let next_client = self.root_window_view_for_content(body, style, scroll_behavior, layout_content);
            if same_rect(client, next_client) {
                break;
            }
            content_hint = layout_content;
        }

        let client = self.root_window_view_for_content(body, style, scroll_behavior, layout_content);
        let content = layout_content;
        let mut scroll = self.root_window_scroll_offset();
        scroll.x = scroll.x.clamp(0, scrollbar_max_scroll(content.width, client.width));
        scroll.y = scroll.y.clamp(0, scrollbar_max_scroll(content.height, client.height));
        self.set_root_window_scroll_state(scroll, self.root_window_scroll_drag());
        self.layout_roots_in_view(style, atlas, client);
    }

    /// Lays out root nodes inside an already resolved root client area.
    fn layout_roots_in_view(&mut self, style: &Style, atlas: crate::AtlasHandle, client: Recti) -> Dimensioni {
        let scroll_offset = self.root_window_scroll_offset();
        let mut y = client.y.saturating_sub(scroll_offset.y);
        let mut content_bounds = None;
        for index in 0..self.roots.len() {
            let Some(root) = self.root_at(index) else { continue };
            let content_y = y.saturating_add(scroll_offset.y);
            let remaining_height = (client.y + client.height - content_y).max(0);
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
                Recti::new(client.x.saturating_sub(scroll_offset.x), y, client.width, height)
            };
            self.layout_node(root, style, &atlas, rect, client);
            if let Some(node) = self.nodes.get(&root) {
                let unscrolled = if is_root_window {
                    Recti::new(
                        node.rect.x,
                        node.rect.y,
                        node.rect.width.max(node.content_size.width),
                        node.rect.height.max(node.content_size.height),
                    )
                } else {
                    Recti::new(
                        node.rect.x.saturating_add(scroll_offset.x),
                        node.rect.y.saturating_add(scroll_offset.y),
                        node.rect.width.max(node.content_size.width),
                        node.rect.height.max(node.content_size.height),
                    )
                };
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

    /// Applies wheel scrolling to the root viewport and clamps it to current content.
    fn update_root_window_scroll(&mut self, body: Recti, style: &Style, input: &Input, scroll_behavior: ScrollBehavior) {
        if scroll_behavior.is_no_scroll() {
            self.set_root_window_scroll_state(Vec2i::default(), None);
            return;
        }

        let view = self.root_window_view_for_content(body, style, scroll_behavior, self.root_window_content_size());
        let content = self.root_window_content_size();
        let max_x = (content.width - view.width).max(0);
        let max_y = (content.height - view.height).max(0);
        let scrollbar_size = style.scrollbar_size.max(0);
        let mut scroll_offset = self.root_window_scroll_offset();
        let mut scroll_drag = self.root_window_scroll_drag();
        if input.mouse_down.is_empty() {
            scroll_drag = None;
        } else if self.hover_root_active && input.mouse_pressed.intersects(MouseButton::LEFT) && scrollbar_size > 0 {
            let vertical = scrollbar_base(ScrollAxis::Vertical, view, scrollbar_size);
            let horizontal = scrollbar_base(ScrollAxis::Horizontal, view, scrollbar_size);
            if max_y > 0 && vertical.contains(&input.mouse_pos) {
                scroll_drag = Some(ScrollAxis::Vertical);
            } else if max_x > 0 && horizontal.contains(&input.mouse_pos) {
                scroll_drag = Some(ScrollAxis::Horizontal);
            }
        }

        match scroll_drag {
            Some(ScrollAxis::Vertical) if max_y > 0 => {
                let base = scrollbar_base(ScrollAxis::Vertical, view, scrollbar_size);
                scroll_offset.y = scroll_offset
                    .y
                    .saturating_add(scrollbar_drag_delta(ScrollAxis::Vertical, input.mouse_delta, content.height, base));
            }
            Some(ScrollAxis::Horizontal) if max_x > 0 => {
                let base = scrollbar_base(ScrollAxis::Horizontal, view, scrollbar_size);
                scroll_offset.x = scroll_offset
                    .x
                    .saturating_add(scrollbar_drag_delta(ScrollAxis::Horizontal, input.mouse_delta, content.width, base));
            }
            _ => {}
        }

        if self.hover_root_active {
            scroll_offset.x = scroll_offset.x.saturating_sub(input.scroll_delta.x);
            scroll_offset.y = scroll_offset.y.saturating_sub(input.scroll_delta.y);
        }
        scroll_offset.x = scroll_offset.x.clamp(0, max_x);
        scroll_offset.y = scroll_offset.y.clamp(0, max_y);
        self.set_root_window_scroll_state(scroll_offset, scroll_drag);
    }

    /// Returns the first root content size in the root-window client coordinate space.
    fn root_window_content_size(&self) -> Dimensioni {
        self.roots
            .first()
            .and_then(|root| self.nodes.get(root))
            .map(|node| node.content_size)
            .unwrap_or_default()
    }

    /// Returns the root content viewport with scrollbar gutters reserved inside the root.
    fn root_window_view_for_content(&self, body: Recti, style: &Style, scroll_behavior: ScrollBehavior, content: Dimensioni) -> Recti {
        let mut view = expand_rect(body, -style.padding);
        if scroll_behavior.is_no_scroll() {
            return view;
        }
        let scrollbar_size = style.scrollbar_size.max(0);
        if scrollbar_size <= 0 {
            return view;
        }
        let base = view;
        for _ in 0..3 {
            let needs_vertical = content.height > view.height && view.height > 0;
            let needs_horizontal = content.width > view.width && view.width > 0;
            let mut next = base;
            if needs_vertical {
                next.width = next.width.saturating_sub(scrollbar_size);
            }
            if needs_horizontal {
                next.height = next.height.saturating_sub(scrollbar_size);
            }
            if same_rect(next, view) {
                break;
            }
            view = next;
        }
        view
    }

    /// Measures one node's preferred size in the box-tree layout path.
    fn measure_node(&self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
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
    fn measure_container(&self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        self.container_clone(id)
            .map(|container| container.measure(self, id, style, atlas, available))
            .unwrap_or_default()
    }

    /// Measures a vertical container.
    fn measure_column(&self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let mut width = 0;
        let mut height: i32 = 0;
        for index in 0..self.child_count(id) {
            let Some(child) = self.child_at(id, index) else { continue };
            let child_size = self.measure_node(child, style, atlas, available);
            width = width.max(child_size.width);
            height = height.saturating_add(child_size.height);
            if index + 1 < self.child_count(id) {
                height = height.saturating_add(style.spacing);
            }
        }
        Dimensioni::new(width.max(0), height.max(0))
    }

    /// Measures a row container.
    fn measure_row(&self, id: UiNodeId, height_policy: SizePolicy, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let mut preferred_heights = Vec::new();
        let mut preferred_widths = Vec::new();
        for index in 0..self.child_count(id) {
            let Some(child) = self.child_at(id, index) else { continue };
            let child_size = self.measure_node(child, style, atlas, available);
            preferred_widths.push(child_size.width);
            preferred_heights.push(child_size.height);
        }
        let height = resolve_size(
            height_policy,
            preferred_heights.iter().copied().max().unwrap_or_else(|| default_cell_height(style, atlas)),
            available.height,
            available.height,
            None,
        );
        let spacing = style.spacing.saturating_mul(self.child_count(id).saturating_sub(1) as i32);
        let width = preferred_widths.into_iter().sum::<i32>().saturating_add(spacing).max(0);
        Dimensioni::new(width, height)
    }

    /// Measures a grid container.
    fn measure_grid(
        &self,
        id: UiNodeId,
        widths: &[SizePolicy],
        heights: &[SizePolicy],
        style: &Style,
        atlas: &crate::AtlasHandle,
        available: Dimensioni,
    ) -> Dimensioni {
        let cols = widths.len().max(1);
        let rows = self
            .grid_placements(id, cols)
            .into_iter()
            .map(|placement| placement.row + placement.row_span)
            .max()
            .unwrap_or(0);
        let rows = rows.max(heights.len()).max(1);
        let default_height = default_cell_height(style, atlas);
        let width = available.width;
        let height = if heights.is_empty() {
            default_height
                .saturating_mul(rows as i32)
                .saturating_add(style.spacing.saturating_mul(rows.saturating_sub(1) as i32))
        } else {
            available.height
        };
        Dimensioni::new(width.max(0), height.max(0))
    }

    /// Measures a stack container.
    fn measure_stack(
        &self,
        id: UiNodeId,
        width_policy: SizePolicy,
        height_policy: SizePolicy,
        style: &Style,
        atlas: &crate::AtlasHandle,
        available: Dimensioni,
    ) -> Dimensioni {
        let mut width = 0;
        let mut height: i32 = 0;
        for index in 0..self.child_count(id) {
            let Some(child) = self.child_at(id, index) else { continue };
            let child_size = self.measure_node(child, style, atlas, available);
            width = width.max(resolve_size(width_policy, child_size.width, available.width, available.width, None));
            height = height.saturating_add(resolve_size(height_policy, child_size.height, available.height, available.height, None));
            if index + 1 < self.child_count(id) {
                height = height.saturating_add(style.spacing);
            }
        }
        Dimensioni::new(width.max(0), height.max(0))
    }

    /// Measures a header/tree disclosure container.
    fn measure_disclosure(&self, id: UiNodeId, disclosure: &Disclosure, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let widget = erased_widget_state(disclosure.state.clone());
        let header_size = widget.measure(style, atlas, available);
        if !disclosure.state.read(|state| state.state).is_expanded() {
            return Dimensioni::new(available.width.max(header_size.width), header_size.height);
        }

        let child_available = Dimensioni::new(
            available.width.saturating_sub(disclosure_child_indent(disclosure.indent_children, style)),
            available.height.saturating_sub(header_size.height),
        );
        let child_size = self.measure_column(id, style, atlas, child_available);
        Dimensioni::new(
            available
                .width
                .max(header_size.width)
                .max(child_size.width + disclosure_child_indent(disclosure.indent_children, style)),
            header_size.height.saturating_add(style.spacing).saturating_add(child_size.height),
        )
    }

    /// Lays out one node using the enum-based runtime path.
    fn layout_node(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) -> Dimensioni {
        let kind = self.node_kind_tag(id);
        match kind {
            RuntimeNodeKind::Widget => self.layout_widget(id, style, atlas, rect, clip),
            RuntimeNodeKind::Container => self.layout_container(id, style, atlas, rect, clip),
        }
    }

    /// Returns whether a node is a widget or container without holding a borrow.
    fn node_kind_tag(&self, id: UiNodeId) -> RuntimeNodeKind {
        match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { .. }) => RuntimeNodeKind::Widget,
            _ => RuntimeNodeKind::Container,
        }
    }

    /// Lays out a widget leaf.
    fn layout_widget(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) -> Dimensioni {
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
            node.rect = rect;
            node.client = node.rect;
            node.clip = clip.intersect(&node.rect).unwrap_or_default();
            node.content_size = size;
        }
        size
    }

    /// Lays out a container and descendants.
    fn layout_container(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) -> Dimensioni {
        let node_clip = clip.intersect(&rect).unwrap_or_default();
        if let Some(node) = self.nodes.get_mut(&id) {
            node.rect = rect;
            node.client = rect;
            node.clip = node_clip;
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
    fn layout_container_children(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        if let Some(mut container) = self.container_clone(id) {
            container.layout(self, id, style, atlas, rect, clip);
            self.set_container(id, container);
        }
    }

    /// Lays out children as a vertical column.
    fn layout_column_children(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        let count = self.child_count(id);
        let available_height = rect.height.saturating_sub(style.spacing.saturating_mul(count.saturating_sub(1) as i32));
        let mut preferred = Vec::with_capacity(count);
        let mut policies = Vec::with_capacity(count);
        for index in 0..count {
            let Some(child) = self.child_at(id, index) else { continue };
            let child_size = self.measure_node(child, style, atlas, Dimensioni::new(rect.width, available_height));
            preferred.push(child_size.height);
            policies.push(self.vertical_child_policy(child));
        }
        let heights = resolve_axis_tracks(&policies, &preferred, available_height);
        let mut y = rect.y;
        for index in 0..count {
            let Some(child) = self.child_at(id, index) else { continue };
            let height = heights.get(index).copied().unwrap_or_default();
            let child_rect = Recti::new(rect.x, y, rect.width, height);
            self.layout_node(child, style, atlas, child_rect, clip);
            y = y.saturating_add(height).saturating_add(style.spacing);
        }
    }

    /// Lays out synthetic root-window children inside a fixed viewport using the root scroll offset.
    fn layout_root_window_children(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        let scroll = self.root_window_scroll_offset();
        let scrolled_rect = Recti::new(rect.x.saturating_sub(scroll.x), rect.y.saturating_sub(scroll.y), rect.width, rect.height);
        self.layout_column_children(id, style, atlas, scrolled_rect, clip);

        let content_size = self
            .child_content_bounds(id)
            .map(|bounds| {
                Dimensioni::new(
                    (bounds.x + scroll.x + bounds.width - rect.x).max(0),
                    (bounds.y + scroll.y + bounds.height - rect.y).max(0),
                )
            })
            .unwrap_or_default();
        if let Some(node) = self.nodes.get_mut(&id) {
            node.content_size = content_size;
        }
    }

    /// Lays out a scroll area from retained scroll state and current-frame content size.
    fn layout_scroll_area_children(
        &mut self,
        id: UiNodeId,
        style: &Style,
        atlas: &crate::AtlasHandle,
        rect: Recti,
        clip: Recti,
        handle: &ScrollAreaHandle,
        scroll_behavior: ScrollBehavior,
    ) -> ScrollAreaLayout {
        let mut content_hint = handle.with(|area| area.content_size());
        let requested_scroll = handle.with(|area| area.scroll());
        let mut layout = ScrollAreaLayout {
            body: self.scroll_area_body_for_content(rect, style, scroll_behavior, content_hint),
            content_size: content_hint,
            scroll: requested_scroll,
        };

        for _ in 0..3 {
            layout = self.layout_scroll_area_once(id, style, atlas, rect, clip, scroll_behavior, content_hint, requested_scroll);
            let next_body = self.scroll_area_body_for_content(rect, style, scroll_behavior, layout.content_size);
            if same_rect(next_body, layout.body) {
                break;
            }
            content_hint = layout.content_size;
        }

        let layout_snapshot = NodeLayout::new(rect, layout.body, layout.content_size);
        handle.with_inner_mut(|area| {
            area.apply_viewport_layout(layout_snapshot);
            area.set_scroll(layout.scroll);
        });
        layout
    }

    /// Performs one scroll-area layout pass using `content_hint` to resolve scrollbar gutters.
    fn layout_scroll_area_once(
        &mut self,
        id: UiNodeId,
        style: &Style,
        atlas: &crate::AtlasHandle,
        rect: Recti,
        clip: Recti,
        scroll_behavior: ScrollBehavior,
        content_hint: Dimensioni,
        scroll: Vec2i,
    ) -> ScrollAreaLayout {
        let body = self.scroll_area_body_for_content(rect, style, scroll_behavior, content_hint);
        let padded_hint = add_padding(content_hint, style.padding.max(0));
        let scroll = Vec2i::new(
            scroll.x.clamp(0, scrollbar_max_scroll(padded_hint.width, body.width)),
            scroll.y.clamp(0, scrollbar_max_scroll(padded_hint.height, body.height)),
        );
        let child_clip = clip.intersect(&body).unwrap_or_default();
        let mut child_rect = expand_rect(body, -style.padding);
        child_rect.x = child_rect.x.saturating_sub(scroll.x);
        child_rect.y = child_rect.y.saturating_sub(scroll.y);

        if let Some(node) = self.nodes.get_mut(&id) {
            node.rect = rect;
            node.client = body;
            node.clip = child_clip;
        }

        self.layout_column_children(id, style, atlas, child_rect, child_clip);
        let content_size = self
            .child_content_bounds(id)
            .map(|bounds| {
                Dimensioni::new(
                    (bounds.x + bounds.width - child_rect.x).max(0),
                    (bounds.y + bounds.height - child_rect.y).max(0),
                )
            })
            .unwrap_or_default();

        if let Some(node) = self.nodes.get_mut(&id) {
            node.content_size = Dimensioni::new(rect.width.max(0), rect.height.max(0));
        }

        ScrollAreaLayout { body, content_size, scroll }
    }

    /// Resolves a scroll-area body from a content-size hint.
    fn scroll_area_body_for_content(&self, rect: Recti, style: &Style, scroll_behavior: ScrollBehavior, content_size: Dimensioni) -> Recti {
        if scroll_behavior.is_no_scroll() {
            return rect;
        }
        let scrollbar_size = style.scrollbar_size.max(0);
        if scrollbar_size <= 0 {
            return rect;
        }
        let content = add_padding(content_size, style.padding.max(0));
        let mut body = rect;
        for _ in 0..3 {
            let needs_vertical = content.height > body.height && body.height > 0;
            let needs_horizontal = content.width > body.width && body.width > 0;
            let mut next = rect;
            if needs_vertical {
                next.width = next.width.saturating_sub(scrollbar_size);
            }
            if needs_horizontal {
                next.height = next.height.saturating_sub(scrollbar_size);
            }
            if same_rect(next, body) {
                break;
            }
            body = next;
        }
        body
    }

    /// Lays out children as a horizontal row.
    fn layout_row_children(
        &mut self,
        id: UiNodeId,
        style: &Style,
        atlas: &crate::AtlasHandle,
        rect: Recti,
        clip: Recti,
        widths: &[SizePolicy],
        height_policy: SizePolicy,
    ) {
        let count = self.child_count(id);
        let available_width = rect.width.saturating_sub(style.spacing.saturating_mul(count.saturating_sub(1) as i32));
        let mut preferred = Vec::with_capacity(count);
        let mut policies = Vec::with_capacity(count);
        for index in 0..count {
            let Some(child) = self.child_at(id, index) else { continue };
            let child_size = self.measure_node(child, style, atlas, Dimensioni::new(available_width, rect.height));
            preferred.push(child_size.width);
            policies.push(self.horizontal_track_policy(child, widths.get(index).copied().unwrap_or(SizePolicy::Auto)));
        }
        let child_widths = resolve_axis_tracks(&policies, &preferred, available_width);
        let height = resolve_size(height_policy, rect.height, rect.height, rect.height, None);
        let mut x = rect.x;
        for index in 0..count {
            let Some(child) = self.child_at(id, index) else { continue };
            let width = child_widths.get(index).copied().unwrap_or_default();
            let child_rect = Recti::new(x, rect.y, width, height);
            self.layout_node(child, style, atlas, child_rect, clip);
            x = x.saturating_add(width).saturating_add(style.spacing);
        }
    }

    /// Lays out children as a row-major grid.
    fn layout_grid_children(
        &mut self,
        id: UiNodeId,
        style: &Style,
        atlas: &crate::AtlasHandle,
        rect: Recti,
        clip: Recti,
        widths: &[SizePolicy],
        heights: &[SizePolicy],
    ) {
        let cols = widths.len().max(1);
        let placements = self.grid_placements(id, cols);
        let rows = placements
            .iter()
            .map(|placement| placement.row + placement.row_span)
            .max()
            .unwrap_or(0)
            .max(heights.len())
            .max(1);
        let available_width = rect.width.saturating_sub(style.spacing.saturating_mul(cols.saturating_sub(1) as i32));
        let available_height = rect.height.saturating_sub(style.spacing.saturating_mul(rows.saturating_sub(1) as i32));
        let preferred_widths = vec![default_cell_width(style); cols];
        let preferred_heights = vec![default_cell_height(style, atlas); rows];
        let col_widths = resolve_axis_tracks(&track_policies(widths, cols), &preferred_widths, available_width);
        let row_heights = resolve_axis_tracks(&track_policies(heights, rows), &preferred_heights, available_height);
        for placement in placements {
            let x = rect.x + col_widths.iter().take(placement.col).sum::<i32>() + style.spacing.saturating_mul(placement.col as i32);
            let y = rect.y + row_heights.iter().take(placement.row).sum::<i32>() + style.spacing.saturating_mul(placement.row as i32);
            let width = span_size(&col_widths, placement.col, placement.col_span, style.spacing);
            let height = span_size(&row_heights, placement.row, placement.row_span, style.spacing);
            let child_rect = Recti::new(x, y, width, height);
            self.layout_node(placement.child, style, atlas, child_rect, clip);
        }
    }

    /// Computes row-major child placements while honoring explicit grid spans.
    fn grid_placements(&self, id: UiNodeId, cols: usize) -> Vec<GridPlacement> {
        let cols = cols.max(1);
        let mut occupied: Vec<Vec<bool>> = Vec::new();
        let mut placements = Vec::with_capacity(self.child_count(id));
        let mut search_row = 0;
        let mut search_col = 0;

        for index in 0..self.child_count(id) {
            let Some(child) = self.child_at(id, index) else { continue };
            let (row, col) = first_free_grid_cell(&mut occupied, cols, search_row, search_col);
            let span = self.nodes.get(&child).map(|node| node.grid_span).unwrap_or(GridSpan::ONE);
            let col_span = span.columns.max(1).min(cols.saturating_sub(col).max(1));
            let row_span = span.rows.max(1);
            mark_grid_occupied(&mut occupied, cols, row, col, row_span, col_span);
            placements.push(GridPlacement { child, col, row, col_span, row_span });

            search_row = row;
            search_col = col.saturating_add(col_span);
            while search_col >= cols {
                search_col -= cols;
                search_row += 1;
            }
        }

        placements
    }

    /// Lays out stack children from the chosen vertical anchor.
    fn layout_stack_children(
        &mut self,
        id: UiNodeId,
        style: &Style,
        atlas: &crate::AtlasHandle,
        rect: Recti,
        clip: Recti,
        width_policy: SizePolicy,
        height_policy: SizePolicy,
        direction: StackDirection,
    ) {
        let count = self.child_count(id);
        let mut heights = Vec::with_capacity(count);
        for index in 0..count {
            let Some(child) = self.child_at(id, index) else { continue };
            let child_size = self.measure_node(child, style, atlas, Dimensioni::new(rect.width, rect.height));
            heights.push(resolve_size(height_policy, child_size.height, rect.height, rect.height, None));
        }
        match direction {
            StackDirection::TopToBottom => {
                let mut y = rect.y;
                for index in 0..count {
                    let Some(child) = self.child_at(id, index) else { continue };
                    let height = heights.get(index).copied().unwrap_or_default();
                    let width = resolve_size(width_policy, rect.width, rect.width, rect.width, None);
                    self.layout_node(child, style, atlas, Recti::new(rect.x, y, width, height), clip);
                    y = y.saturating_add(height).saturating_add(style.spacing);
                }
            }
            StackDirection::BottomToTop => {
                let mut y = rect.y + rect.height;
                for index in (0..count).rev() {
                    let Some(child) = self.child_at(id, index) else { continue };
                    let height = heights.get(index).copied().unwrap_or_default();
                    let width = resolve_size(width_policy, rect.width, rect.width, rect.width, None);
                    y = y.saturating_sub(height);
                    self.layout_node(child, style, atlas, Recti::new(rect.x, y, width, height), clip);
                    y = y.saturating_sub(style.spacing);
                }
            }
        }
    }

    /// Returns the union of child rectangles, including each child's measured content overflow.
    fn child_content_bounds(&self, id: UiNodeId) -> Option<Recti> {
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

    /// Lays out a header/tree disclosure row and optional children.
    fn layout_disclosure_children(&mut self, id: UiNodeId, disclosure: &Disclosure, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        let widget = erased_widget_state(disclosure.state.clone());
        let header_preferred = widget.measure(style, atlas, Dimensioni::new(rect.width, rect.height));
        let header_height = header_preferred.height.max(default_cell_height(style, atlas)).min(rect.height.max(0));
        let header_rect = Recti::new(rect.x, rect.y, rect.width, header_height);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.client = header_rect;
        }

        if !disclosure.state.read(|state| state.state).is_expanded() {
            return;
        }

        let indent = disclosure_child_indent(disclosure.indent_children, style);
        let child_rect = Recti::new(
            rect.x + indent,
            rect.y + header_height + style.spacing,
            rect.width.saturating_sub(indent),
            rect.height.saturating_sub(header_height).saturating_sub(style.spacing),
        );
        self.layout_column_children(id, style, atlas, child_rect, clip);
    }

    /// Returns the effective vertical placement policy for a child in a column.
    fn vertical_child_policy(&self, child: UiNodeId) -> SizePolicy {
        let policy = self.nodes.get(&child).map(|node| node.policy.height).unwrap_or(SizePolicy::Auto);
        if policy != SizePolicy::Auto {
            return policy;
        }
        self.container_clone(child)
            .and_then(|container| container.vertical_child_policy())
            .unwrap_or(SizePolicy::Auto)
    }

    /// Returns the effective horizontal placement policy for a child in a row.
    fn horizontal_track_policy(&self, child: UiNodeId, track: SizePolicy) -> SizePolicy {
        let policy = self.nodes.get(&child).map(|node| node.policy.width).unwrap_or(SizePolicy::Auto);
        if policy != SizePolicy::Auto { policy } else { track }
    }

    /// Updates one node and descendants.
    fn update_node(
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
                    .map(|container| container.update(self, root_id, id, style, atlas.clone(), input, results))
                    .unwrap_or(true);
                if traverse_children {
                    for index in 0..self.child_count(id) {
                        let Some(child) = self.child_at(id, index) else { continue };
                        self.update_node(root_id, root_name, child, style, atlas.clone(), input, results);
                    }
                }
                if let Some(container) = container {
                    self.set_container(id, container);
                }
            }
        }
    }

    /// Updates a header/tree disclosure widget stored on a container node.
    fn update_disclosure_widget(
        &mut self,
        root_id: crate::RootId,
        id: UiNodeId,
        disclosure: &Disclosure,
        style: &Style,
        atlas: crate::AtlasHandle,
        input: &Input,
        results: &mut FrameResults,
    ) {
        let rect = self.nodes.get(&id).map(|node| node.client).unwrap_or_default();
        let widget = erased_widget_state(disclosure.state.clone());
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let control = self.control_for(id, rect, input, opt, scroll_behavior, focus_policy);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.control = control;
        }

        let mut focus_slot = self.focus.map(RetainedId::node);
        let mut focus_seen = self.updated_focus;
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
            None,
        );
        let result = widget.update(&mut ctx, &control);
        self.focus = retained_focus_to_node(focus_slot);
        self.updated_focus = focus_seen;

        results.record_retained_with_context(
            RetainedId::root_node(root_id, id),
            id,
            disclosure.state.id(),
            result,
            format!("ui node disclosure {:?}", id),
        );
    }

    /// Updates a widget leaf.
    fn update_widget(
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
            id,
            widget_handle_id,
            result,
            format!("root {root_name:?} ui node {:?}", id),
        );
    }

    /// Computes control state from node geometry and shared input.
    fn control_for(
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
    fn dispatch_scroll_input(&mut self, style: &Style, input: &Input) -> bool {
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
    fn dispatch_scroll_input_to_node(&mut self, id: UiNodeId, style: &Style, input: &Input) -> bool {
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
                let consumed = container.dispatch_scroll(self, id, style, input);
                self.set_container(id, container);
                consumed
            }
            _ => false,
        }
    }

    /// Updates scroll-area scrollbar drag and wheel state. Returns true if this area owns the event.
    fn dispatch_scroll_area_input(&mut self, id: UiNodeId, scroll_area: &mut ScrollArea, style: &Style, input: &Input) -> bool {
        let Some((clip, body)) = self.nodes.get(&id).map(|node| (node.clip, node.client)) else {
            return false;
        };
        let content_size = scroll_area.content_size;
        let handle = scroll_area.handle.clone();
        let scroll_behavior = scroll_area.scroll_behavior;
        let mut scroll_drag = scroll_area.scroll_drag;
        if scroll_behavior.is_no_scroll() {
            return false;
        }

        let content = add_padding(content_size, style.padding.max(0));
        let max_x = scrollbar_max_scroll(content.width, body.width);
        let max_y = scrollbar_max_scroll(content.height, body.height);
        let scrollbar_size = style.scrollbar_size.max(0);
        let vertical = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
        let horizontal = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
        let hovered_body = body.contains(&input.mouse_pos) && clip.contains(&input.mouse_pos);
        let hovered_vertical = max_y > 0 && vertical.contains(&input.mouse_pos);
        let hovered_horizontal = max_x > 0 && horizontal.contains(&input.mouse_pos);
        let wheel_input = input.scroll_delta.x != 0 || input.scroll_delta.y != 0;
        let mut owns_event = hovered_body && wheel_input;
        let mut scroll = handle.with(|area| area.scroll());
        if input.mouse_down.is_empty() {
            scroll_drag = None;
        } else if input.mouse_pressed.intersects(MouseButton::LEFT) && scrollbar_size > 0 {
            if hovered_vertical {
                scroll_drag = Some(ScrollAxis::Vertical);
                owns_event = true;
            } else if hovered_horizontal {
                scroll_drag = Some(ScrollAxis::Horizontal);
                owns_event = true;
            }
        }

        match scroll_drag {
            Some(ScrollAxis::Vertical) if max_y > 0 => {
                let base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
                scroll.y = scroll
                    .y
                    .saturating_add(scrollbar_drag_delta(ScrollAxis::Vertical, input.mouse_delta, content.height, base));
            }
            Some(ScrollAxis::Horizontal) if max_x > 0 => {
                let base = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
                scroll.x = scroll
                    .x
                    .saturating_add(scrollbar_drag_delta(ScrollAxis::Horizontal, input.mouse_delta, content.width, base));
            }
            _ => {}
        }

        if hovered_body {
            scroll.x = scroll.x.saturating_sub(input.scroll_delta.x);
            scroll.y = scroll.y.saturating_sub(input.scroll_delta.y);
        }
        scroll.x = scroll.x.clamp(0, max_x);
        scroll.y = scroll.y.clamp(0, max_y);

        handle.with_inner_mut(|area| area.set_scroll(scroll));
        scroll_area.scroll_offset = scroll;
        scroll_area.scroll_drag = scroll_drag;
        owns_event || scroll_drag.is_some()
    }

    /// Paints one node and descendants.
    fn paint_node(&mut self, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
        let kind = self.node_kind_tag(id);
        match kind {
            RuntimeNodeKind::Widget => self.paint_widget(id, style, atlas, input),
            RuntimeNodeKind::Container => {
                let mut container = self.container_clone(id);
                let traverse_children = container
                    .as_mut()
                    .map(|container| container.paint_before_children(self, id, style, atlas.clone()))
                    .unwrap_or(true);
                if traverse_children {
                    for index in 0..self.child_count(id) {
                        let Some(child) = self.child_at(id, index) else { continue };
                        self.paint_node(child, style, atlas.clone(), input);
                    }
                }
                if let Some(container) = container.as_mut() {
                    container.paint_after_children(self, id, style, atlas);
                }
                if let Some(container) = container {
                    self.set_container(id, container);
                }
            }
        }
    }

    /// Paints a header/tree disclosure widget stored on a container node.
    fn paint_disclosure_widget(&mut self, id: UiNodeId, disclosure: &Disclosure, style: &Style, atlas: crate::AtlasHandle) {
        let rect = self.nodes.get(&id).map(|node| node.client).unwrap_or_default();
        let control = self.nodes.get(&id).map(|node| node.control).unwrap_or_default();
        let widget = erased_widget_state(disclosure.state.clone());
        let mut focus_slot = self.focus.map(RetainedId::node);
        let mut focus_seen = self.updated_focus;
        self.push_node_clip(id);
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
            None,
        );
        widget.paint(&mut ctx, &control);
        self.pop_node_clip();
        self.focus = retained_focus_to_node(focus_slot);
        self.updated_focus = focus_seen;
    }

    /// Paints a widget leaf.
    fn paint_widget(&mut self, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
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
    fn current_clip_rect(&self) -> Recti {
        self.clip_stack.last().copied().unwrap_or(UNCLIPPED_RECT)
    }

    /// Pushes a node's effective clip for widget drawing.
    fn push_node_clip(&mut self, id: UiNodeId) {
        let clip = self.nodes.get(&id).map(|node| node.clip).unwrap_or(UNCLIPPED_RECT);
        let current = self.current_clip_rect();
        let effective = current.intersect(&clip).unwrap_or_default();
        self.clip_stack.push(effective);
        self.commands.push(Command::PushClip { rect: effective });
    }

    /// Pops a node clip pushed by [`Self::push_node_clip`].
    fn pop_node_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
        }
        self.commands.push(Command::PopClip);
    }

    /// Paints a retained scroll-area panel while native scrollbars are still pending.
    fn paint_scroll_area_panel(&mut self, id: UiNodeId, opt: ContainerOption, style: &Style, atlas: &crate::AtlasHandle) {
        let Some(rect) = self.nodes.get(&id).map(|node| node.rect) else {
            return;
        };
        if !opt.intersects(ContainerOption::NO_FRAME) {
            let mut draw = DrawCtx::new(&mut self.commands, &mut self.triangle_vertices, &mut self.clip_stack, style, atlas);
            draw.draw_frame(rect, ControlColor::PanelBG);
        }
    }

    /// Paints scroll-area scrollbars from the resolved body/content snapshot.
    fn paint_scroll_area_scrollbars(&mut self, id: UiNodeId, scroll_area: &ScrollArea, style: &Style, atlas: &crate::AtlasHandle) {
        let Some(body) = self.nodes.get(&id).map(|node| node.client) else {
            return;
        };
        let content_size = scroll_area.content_size;
        let scroll_offset = scroll_area.scroll_offset;
        let scroll_behavior = scroll_area.scroll_behavior;
        if scroll_behavior.is_no_scroll() {
            return;
        }
        let scrollbar_size = style.scrollbar_size.max(0);
        if scrollbar_size <= 0 {
            return;
        }
        let content = add_padding(content_size, style.padding.max(0));
        let mut draw = DrawCtx::new(&mut self.commands, &mut self.triangle_vertices, &mut self.clip_stack, style, atlas);
        if content.height > body.height {
            let base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
            let thumb = scrollbar_thumb(ScrollAxis::Vertical, base, body.height, content.height, scroll_offset.y, scrollbar_size);
            draw.draw_frame(base, ControlColor::Base);
            draw.draw_frame(thumb, ControlColor::Button);
        }
        if content.width > body.width {
            let base = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
            let thumb = scrollbar_thumb(ScrollAxis::Horizontal, base, body.width, content.width, scroll_offset.x, scrollbar_size);
            draw.draw_frame(base, ControlColor::Base);
            draw.draw_frame(thumb, ControlColor::Button);
        }
    }
}

/// Coarse node kind used to route passes without holding a node borrow.
#[derive(Copy, Clone)]
enum RuntimeNodeKind {
    /// Leaf widget.
    Widget,
    /// Container node.
    Container,
}

/// Concrete row-major placement of one child inside a grid.
struct GridPlacement {
    /// Child node being placed.
    child: UiNodeId,
    /// Starting column.
    col: usize,
    /// Starting row.
    row: usize,
    /// Number of columns occupied.
    col_span: usize,
    /// Number of rows occupied.
    row_span: usize,
}

/// Resolved scroll-area geometry for one layout pass.
struct ScrollAreaLayout {
    /// Visible body excluding scrollbar gutters.
    body: Recti,
    /// Measured child content size.
    content_size: Dimensioni,
    /// Clamped scroll offset.
    scroll: Vec2i,
}

/// Clone support for boxed container behavior objects.
pub(crate) trait ContainerClone {
    /// Clones this container into a boxed trait object.
    fn clone_box(&self) -> Box<dyn ContainerTrait>;
}

impl<T> ContainerClone for T
where
    T: ContainerTrait + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn ContainerTrait> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn ContainerTrait> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Common internal behavior interface for child-owning nodes.
pub(crate) trait ContainerTrait: ContainerClone {
    /// Measures the preferred size for a container node.
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni;

    /// Assigns rectangles to children and recursively lays them out.
    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti);

    /// Updates the container's own retained widget, if any, and returns whether children should be traversed.
    fn update(
        &mut self,
        _runtime: &mut UiRuntime,
        _root_id: crate::RootId,
        _id: UiNodeId,
        _style: &Style,
        _atlas: crate::AtlasHandle,
        _input: &Input,
        _results: &mut FrameResults,
    ) -> bool {
        true
    }

    /// Paints any container-owned widget/background before children and returns whether children should be painted.
    fn paint_before_children(&mut self, _runtime: &mut UiRuntime, _id: UiNodeId, _style: &Style, _atlas: crate::AtlasHandle) -> bool {
        true
    }

    /// Paints any container-owned overlay after children.
    fn paint_after_children(&mut self, _runtime: &mut UiRuntime, _id: UiNodeId, _style: &Style, _atlas: crate::AtlasHandle) {}

    /// Dispatches scroll input owned by this container.
    fn dispatch_scroll(&mut self, _runtime: &mut UiRuntime, _id: UiNodeId, _style: &Style, _input: &Input) -> bool {
        false
    }

    /// Returns an implicit vertical child policy this container contributes when it is a child.
    fn vertical_child_policy(&self) -> Option<SizePolicy> {
        None
    }

    /// Whether this container is the synthetic root-window body.
    fn is_root_window(&self) -> bool {
        false
    }

    /// Whether this container is a scrollable child viewport.
    fn is_scroll_area(&self) -> bool {
        false
    }

    /// Current root-window scroll state, if this is a root-window container.
    fn root_scroll_state(&self) -> Option<(Vec2i, Option<ScrollAxis>)> {
        None
    }

    /// Configures root-window scrolling for this frame.
    fn configure_root_scroll(&mut self, _scroll_behavior: ScrollBehavior) {}

    /// Stores root-window scroll state.
    fn set_root_scroll_state(&mut self, _offset: Vec2i, _drag: Option<ScrollAxis>) {}
}

impl ContainerTrait for RootWindow {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime.measure_column(id, style, atlas, available)
    }

    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        runtime.layout_root_window_children(id, style, atlas, rect, clip);
    }

    fn is_root_window(&self) -> bool {
        true
    }

    fn root_scroll_state(&self) -> Option<(Vec2i, Option<ScrollAxis>)> {
        Some((self.scroll_offset, self.scroll_drag))
    }

    fn configure_root_scroll(&mut self, scroll_behavior: ScrollBehavior) {
        self.scroll_behavior = scroll_behavior;
    }

    fn set_root_scroll_state(&mut self, offset: Vec2i, drag: Option<ScrollAxis>) {
        self.scroll_offset = offset;
        self.scroll_drag = drag;
    }
}

impl ContainerTrait for ScrollArea {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime.measure_column(id, style, atlas, available)
    }

    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        let layout = runtime.layout_scroll_area_children(id, style, atlas, rect, clip, &self.handle, self.scroll_behavior);
        self.content_size = layout.content_size;
        self.scroll_offset = layout.scroll;
    }

    fn dispatch_scroll(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, input: &Input) -> bool {
        runtime.dispatch_scroll_area_input(id, self, style, input)
    }

    fn is_scroll_area(&self) -> bool {
        true
    }

    fn paint_before_children(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle) -> bool {
        runtime.paint_scroll_area_panel(id, self.opt, style, &atlas);
        runtime.push_node_clip(id);
        true
    }

    fn paint_after_children(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle) {
        runtime.pop_node_clip();
        runtime.paint_scroll_area_scrollbars(id, self, style, &atlas);
    }
}

impl ContainerTrait for Disclosure {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime.measure_disclosure(id, self, style, atlas, available)
    }

    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        runtime.layout_disclosure_children(id, self, style, atlas, rect, clip);
    }

    fn update(
        &mut self,
        runtime: &mut UiRuntime,
        root_id: crate::RootId,
        id: UiNodeId,
        style: &Style,
        atlas: crate::AtlasHandle,
        input: &Input,
        results: &mut FrameResults,
    ) -> bool {
        runtime.update_disclosure_widget(root_id, id, self, style, atlas, input, results);
        self.state.read(|state| state.state).is_expanded()
    }

    fn paint_before_children(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle) -> bool {
        runtime.paint_disclosure_widget(id, self, style, atlas);
        self.state.read(|state| state.state).is_expanded()
    }
}

impl ContainerTrait for Column {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime.measure_column(id, style, atlas, available)
    }

    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        runtime.layout_column_children(id, style, atlas, rect, clip);
    }
}

impl ContainerTrait for Row {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime.measure_row(id, self.height, style, atlas, available)
    }

    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        runtime.layout_row_children(id, style, atlas, rect, clip, &self.widths, self.height);
    }

    fn vertical_child_policy(&self) -> Option<SizePolicy> {
        Some(self.height)
    }
}

impl ContainerTrait for Grid {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime.measure_grid(id, &self.widths, &self.heights, style, atlas, available)
    }

    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        runtime.layout_grid_children(id, style, atlas, rect, clip, &self.widths, &self.heights);
    }
}

impl ContainerTrait for Stack {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime.measure_stack(id, self.width, self.height, style, atlas, available)
    }

    fn layout(&mut self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        runtime.layout_stack_children(id, style, atlas, rect, clip, self.width, self.height, self.direction);
    }
}

/// Internal node context used by future container passes.
pub(crate) struct NodeCtx<'a> {
    /// Runtime owning the node graph.
    runtime: &'a mut UiRuntime,
    /// Current node id.
    id: UiNodeId,
}

impl<'a> NodeCtx<'a> {
    /// Returns the current node id.
    pub(crate) fn id(&self) -> UiNodeId {
        self.id
    }

    /// Returns the parent node id, if any.
    pub(crate) fn parent(&self) -> Option<UiNodeId> {
        self.runtime.nodes.get(&self.id).and_then(|node| node.parent)
    }

    /// Returns the current container children, or an empty slice for leaf widgets.
    pub(crate) fn children(&self) -> &[UiNodeId] {
        self.runtime.nodes.get(&self.id).map(UiNode::children).unwrap_or(&[])
    }

    /// Returns the current full node rect.
    pub(crate) fn rect(&self) -> Recti {
        self.runtime.nodes.get(&self.id).map(|node| node.rect).unwrap_or_default()
    }

    /// Returns the current node client rect.
    pub(crate) fn client(&self) -> Recti {
        self.runtime.nodes.get(&self.id).map(|node| node.client).unwrap_or_default()
    }

    /// Returns the current effective clip rect.
    pub(crate) fn clip(&self) -> Recti {
        self.runtime.nodes.get(&self.id).map(|node| node.clip).unwrap_or_default()
    }

    /// Updates the current full node rect.
    pub(crate) fn set_rect(&mut self, value: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.rect = value;
        }
    }

    /// Updates the current node client rect.
    pub(crate) fn set_client(&mut self, value: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.client = value;
        }
    }

    /// Updates the current effective clip rect.
    pub(crate) fn set_clip(&mut self, value: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.clip = value;
        }
    }

    /// Updates the current measured content size.
    pub(crate) fn set_content_size(&mut self, value: Dimensioni) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.content_size = value;
        }
    }

    /// Defers a topology/data operation until traversal completes.
    pub(crate) fn defer(&mut self, op: TreeOp) {
        self.runtime.deferred.push(op);
    }
}

/// Computes titlebar height from style minimums and current title font metrics.
fn root_titlebar_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.title_font) as i32;
    let padding = style.padding.max(0);
    let min_title_h = font_height + (padding / 2).max(1) * 2;
    style.title_height.max(min_title_h)
}

/// Returns the union of two rectangles.
fn union_rect(a: Recti, b: Recti) -> Recti {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);
    Recti::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Compares rectangle components directly.
fn same_rect(a: Recti, b: Recti) -> bool {
    (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height)
}

/// Adds symmetric style padding to content size for scrollbar range checks.
fn add_padding(size: Dimensioni, padding: i32) -> Dimensioni {
    Dimensioni::new(
        size.width.saturating_add(padding.saturating_mul(2)),
        size.height.saturating_add(padding.saturating_mul(2)),
    )
}

/// Returns the child indentation for a disclosure container.
fn disclosure_child_indent(indent_children: bool, style: &Style) -> i32 {
    if indent_children { style.indent.max(0) } else { 0 }
}

/// Returns the fallback control width for auto-sized cells.
fn default_cell_width(style: &Style) -> i32 {
    style.default_cell_width.saturating_add(style.padding.max(0) * 2).max(0)
}

/// Returns the fallback control height for auto-sized cells.
fn default_cell_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.font) as i32;
    font_height.saturating_add(style.padding.max(0) * 2).max(style.padding.max(0) * 2)
}

/// Expands a possibly shorter track-policy slice to a requested count.
fn track_policies(policies: &[SizePolicy], count: usize) -> Vec<SizePolicy> {
    (0..count).map(|index| policies.get(index).copied().unwrap_or(SizePolicy::Auto)).collect()
}

/// Returns the first free cell in a row-major occupancy grid, extending rows as needed.
fn first_free_grid_cell(occupied: &mut Vec<Vec<bool>>, cols: usize, mut row: usize, mut col: usize) -> (usize, usize) {
    loop {
        while occupied.len() <= row {
            occupied.push(vec![false; cols]);
        }
        while col < cols {
            if !occupied[row][col] {
                return (row, col);
            }
            col += 1;
        }
        row += 1;
        col = 0;
    }
}

/// Marks a rectangular cell range as occupied, extending rows as needed.
fn mark_grid_occupied(occupied: &mut Vec<Vec<bool>>, cols: usize, row: usize, col: usize, row_span: usize, col_span: usize) {
    for y in row..row.saturating_add(row_span.max(1)) {
        while occupied.len() <= y {
            occupied.push(vec![false; cols]);
        }
        for x in col..col.saturating_add(col_span.max(1)).min(cols) {
            occupied[y][x] = true;
        }
    }
}

/// Sums a track span, including the spacing between spanned tracks.
fn span_size(tracks: &[i32], start: usize, span: usize, spacing: i32) -> i32 {
    let span = span.max(1);
    let size = tracks.iter().skip(start).take(span).copied().sum::<i32>();
    size.saturating_add(spacing.saturating_mul(span.saturating_sub(1) as i32))
}

/// Resolves one size policy against a preferred size, available space, and optional weight context.
fn resolve_size(policy: SizePolicy, preferred: i32, available: i32, reference: i32, total_weight: Option<f32>) -> i32 {
    let resolved = match policy {
        SizePolicy::Auto => preferred,
        SizePolicy::Fixed(value) => value,
        SizePolicy::Fraction(value) => {
            let fraction = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
            ((reference.max(0) as f32) * fraction).floor() as i32
        }
        SizePolicy::Weight(value) => {
            let weight = if value.is_finite() { value.max(0.0) } else { 0.0 };
            if weight <= 0.0 {
                0
            } else {
                let denom = total_weight.filter(|total| total.is_finite() && *total > 0.0).unwrap_or(weight);
                ((reference.max(0) as f32) * (weight / denom)).floor() as i32
            }
        }
        SizePolicy::Remainder(margin) => available.saturating_sub(margin),
    };
    resolved.max(0)
}

/// Resolves the available size passed into a child measurement from explicit placement policy.
fn measure_axis_available(policy: SizePolicy, available: i32) -> i32 {
    match policy {
        SizePolicy::Fixed(value) => value.max(0),
        SizePolicy::Fraction(value) => resolve_size(SizePolicy::Fraction(value), 0, available, available, None),
        SizePolicy::Remainder(margin) => available.saturating_sub(margin.max(0)).max(0),
        SizePolicy::Auto | SizePolicy::Weight(_) => available.max(0),
    }
}

/// Resolves a node inside an already allocated parent slot.
fn resolve_allocated_size(policy: SizePolicy, preferred: i32, allocated: i32, reference: i32, total_weight: Option<f32>) -> i32 {
    match policy {
        SizePolicy::Auto => allocated.max(preferred).max(0),
        _ => resolve_size(policy, preferred, allocated, reference, total_weight),
    }
}

/// Resolves sibling tracks in one axis using fixed, auto, fraction, weight, and remainder policies.
fn resolve_axis_tracks(policies: &[SizePolicy], preferred: &[i32], available: i32) -> Vec<i32> {
    if policies.is_empty() {
        return Vec::new();
    }

    let available = available.max(0);
    let mut sizes = vec![0; policies.len()];
    let has_remainder = policies.iter().any(|policy| matches!(policy, SizePolicy::Remainder(_)));
    let total_weight = policies
        .iter()
        .filter_map(|policy| match *policy {
            SizePolicy::Weight(value) if value.is_finite() => Some(value.max(0.0)),
            _ => None,
        })
        .sum::<f32>();
    let reserved_for_weight = if has_remainder {
        0
    } else {
        policies
            .iter()
            .copied()
            .enumerate()
            .map(|(index, policy)| match policy {
                SizePolicy::Auto => preferred.get(index).copied().unwrap_or_default().max(0),
                SizePolicy::Fixed(value) => value.max(0),
                SizePolicy::Fraction(value) => resolve_size(SizePolicy::Fraction(value), 0, available, available, None),
                SizePolicy::Weight(_) | SizePolicy::Remainder(_) => 0,
            })
            .sum::<i32>()
    };
    let weight_reference = if total_weight > 0.0 {
        if has_remainder {
            available
        } else {
            available.saturating_sub(reserved_for_weight)
        }
    } else {
        0
    };

    let mut used: i32 = 0;
    for (index, policy) in policies.iter().copied().enumerate() {
        let remaining = available.saturating_sub(used);
        match policy {
            SizePolicy::Auto => {
                sizes[index] = preferred.get(index).copied().unwrap_or_default().max(0);
            }
            SizePolicy::Fixed(value) => {
                sizes[index] = value.max(0);
            }
            SizePolicy::Fraction(value) => {
                sizes[index] = resolve_size(SizePolicy::Fraction(value), 0, available, available, None);
            }
            SizePolicy::Weight(value) => {
                sizes[index] = resolve_size(SizePolicy::Weight(value), 0, remaining, weight_reference, Some(total_weight));
            }
            SizePolicy::Remainder(margin) => {
                sizes[index] = remaining.saturating_sub(margin.max(0)).max(0);
            }
        }
        used = used.saturating_add(sizes[index]);
    }
    sizes
}

/// Returns the screen-space rectangle occupied by a child and any overflow content it measured.
fn child_content_rect(node: &UiNode) -> Recti {
    Recti::new(
        node.rect.x,
        node.rect.y,
        node.rect.width.max(node.content_size.width),
        node.rect.height.max(node.content_size.height),
    )
}

/// Captures immutable input state for widgets that request full input.
fn snapshot_from_input(input: &Input) -> InputSnapshot {
    InputSnapshot {
        mouse_pos: input.mouse_pos,
        mouse_delta: input.mouse_delta,
        mouse_down: input.mouse_down,
        mouse_pressed: input.mouse_pressed,
        key_mods: input.key_down,
        key_pressed: input.key_pressed,
        key_codes: input.key_code_down,
        key_code_pressed: input.key_code_pressed,
        text_input: input.input_text.clone(),
    }
}

/// Converts the retained focus slot used by `WidgetCtx` back to a node id.
fn retained_focus_to_node(focus: Option<RetainedId>) -> Option<UiNodeId> {
    match focus {
        Some(RetainedId::Node(id)) => Some(id),
        _ => None,
    }
}

/// Converts a global input snapshot into widget-local mouse event semantics.
fn input_to_mouse_event(control: &ControlState, input: &InputSnapshot, rect: Recti) -> MouseEvent {
    let origin = Vec2i::new(rect.x, rect.y);
    let prev_pos = input.mouse_pos - input.mouse_delta - origin;
    let curr_pos = input.mouse_pos - origin;

    if control.focused && input.mouse_down.intersects(MouseButton::LEFT) {
        return MouseEvent::Drag { prev_pos, curr_pos };
    }
    if control.hovered && input.mouse_pressed.intersects(MouseButton::LEFT) {
        return MouseEvent::Click(curr_pos);
    }
    if control.hovered {
        return MouseEvent::Move(curr_pos);
    }
    MouseEvent::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    use crate::{
        color4b, rect, AtlasHandle, AtlasSource, Button, Canvas, CharEntry, Custom, FontEntry, Image, Input, ListItem, Policy, RendererHandle,
        ScrollArea as LegacyScrollArea, ScrollAreaHandle, SourceFormat, Textbox, WidgetFillOption, UiNodeBuilder, widget_handle,
    };
    use crate::test_support::{test_atlas, NoopRenderer};

    #[test]
    fn ui_node_set_conversion_keeps_container_children_off_leaf_widgets() {
        let button = widget_handle(Button::new("child"));
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(10, 20))).column(|tree| {
                tree.widget(button.clone());
            });
        });

        let runtime = UiRuntime::from_ui_nodes(tree);
        let root = runtime.roots[0];
        let root_node = runtime.nodes.get(&root).expect("root node missing");
        let column = root_node.children()[0];
        let column_node = runtime.nodes.get(&column).expect("column node missing");
        let child = column_node.children()[0];
        let child_node = runtime.nodes.get(&child).expect("child node missing");

        assert!(matches!(root_node.data, UiNodeData::Container { .. }));
        assert!(matches!(column_node.data, UiNodeData::Container { .. }));
        assert!(matches!(child_node.data, UiNodeData::Widget { .. }));
        assert!(child_node.children().is_empty());
    }

    #[test]
    fn deferred_move_updates_parent_membership() {
        let first = UiNode::new(
            Id::new(1),
            None,
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Container {
                container: Box::new(Column),
                children: Vec::new(),
            },
        );
        let second = UiNode::new(
            Id::new(2),
            None,
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Container {
                container: Box::new(Column),
                children: Vec::new(),
            },
        );
        let child = UiNode::new(
            Id::new(3),
            Some(Id::new(1)),
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Container {
                container: Box::new(Column),
                children: Vec::new(),
            },
        );

        let mut runtime = UiRuntime::new();
        runtime.nodes.insert(first.id, first);
        runtime.nodes.insert(second.id, second);
        runtime.nodes.insert(child.id, child);
        runtime.nodes.get_mut(&Id::new(1)).unwrap().children_mut().unwrap().push(Id::new(3));
        runtime.deferred.push(TreeOp::MoveNode {
            node: Id::new(3),
            new_parent: Id::new(2),
            index: 0,
        });

        runtime.apply_deferred();

        assert!(runtime.nodes.get(&Id::new(1)).unwrap().children().is_empty());
        assert_eq!(runtime.nodes.get(&Id::new(2)).unwrap().children(), &[Id::new(3)]);
        assert_eq!(runtime.nodes.get(&Id::new(3)).unwrap().parent, Some(Id::new(2)));
    }

    #[test]
    fn node_window_chrome_offsets_layout_body() {
        let button = widget_handle(Button::new("bbbb"));
        let tree = UiNodeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                tree.widget(button.clone());
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(400, 500));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(40, 40 + style.title_height, 300, 450 - style.title_height),
            ScrollBehavior::NONE,
            true,
        );

        let button_node = runtime
            .nodes
            .values()
            .find(|node| matches!(node.data, UiNodeData::Widget { .. }))
            .expect("button node missing");
        assert!(button_node.rect.y > 40 + style.title_height);
        assert_eq!(button_node.rect.x, 40 + style.padding);
        assert!(button_node.rect.width > 250);
    }

    #[test]
    fn node_calculator_grid_uses_weighted_tracks() {
        let display = widget_handle(Textbox::with_opt("0", WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT));
        let buttons: Vec<_> = (0..20).map(|_| widget_handle(Button::new("b"))).collect();
        let button_ids = std::cell::RefCell::new(Vec::new());
        let tree = UiNodeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Fraction(0.20), |tree| {
                tree.widget(&display);
            });
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |tree| {
                tree.column(|tree| {
                    let columns = [SizePolicy::Weight(1.0); 4];
                    let rows = [SizePolicy::Weight(1.0); 5];
                    tree.grid(&columns, &rows, |tree| {
                        for button in &buttons {
                            button_ids.borrow_mut().push(tree.widget(button));
                        }
                    });
                });
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(320, 420));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 320, 420),
            ScrollBehavior::NONE,
            true,
        );

        let ids = button_ids.borrow();
        let first = runtime.nodes.get(&ids[0]).unwrap().rect;
        let fourth = runtime.nodes.get(&ids[3]).unwrap().rect;
        let fifth = runtime.nodes.get(&ids[4]).unwrap().rect;
        assert!(first.width > 60);
        assert_eq!(first.y, fourth.y);
        assert!(fourth.x > first.x);
        assert!(fifth.y > first.y);
    }

    #[test]
    fn node_row_remainder_tracks_resolve_left_to_right() {
        let label = widget_handle(ListItem::with_opt("Test buttons 2:", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME));
        let middle = widget_handle(Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER));
        let right = widget_handle(Button::with_opt("Popup", WidgetOption::ALIGN_CENTER));
        let mut middle_id = Id::new(0);
        let mut right_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            let widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
            tree.row(&widths, SizePolicy::Auto, |tree| {
                tree.widget(&label);
                middle_id = tree.widget(&middle);
                right_id = tree.widget(&right);
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(320, 120));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 300, 100),
            ScrollBehavior::NONE,
            true,
        );

        let middle_rect = runtime.nodes.get(&middle_id).unwrap().rect;
        let right_rect = runtime.nodes.get(&right_id).unwrap().rect;
        assert!(middle_rect.width > 0);
        assert!(right_rect.width > middle_rect.width);
        assert!(right_rect.x > middle_rect.x + middle_rect.width);
    }

    #[test]
    fn node_content_size_includes_nested_stack_overflow() {
        let small = widget_handle(Button::new("slot"));
        let image = widget_handle(Button::new("image"));
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(67), StackDirection::TopToBottom, |tree| {
                tree.widget(&small);
                tree.stack(SizePolicy::Fixed(256), SizePolicy::Fixed(256), StackDirection::TopToBottom, |tree| {
                    tree.widget(&image);
                });
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(320, 160));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 300, 120),
            ScrollBehavior::NONE,
            true,
        );

        let root = runtime.roots[0];
        let root_node = runtime.nodes.get(&root).unwrap();
        assert!(root_node.content_size.height >= 67 + style.spacing + 256);
    }

    #[test]
    fn node_scroll_area_keeps_handle_content_and_scroll_state() {
        let atlas = test_atlas();
        let style = Rc::new(Style::default());
        let scroll_area = ScrollAreaHandle::new(LegacyScrollArea::new("node scroll"));
        let first = widget_handle(Button::new("first"));
        let rest: Vec<_> = (0..5).map(|_| widget_handle(Button::new("row"))).collect();
        let mut first_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(120, 48)))
                .scroll_area(&scroll_area, ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(24), StackDirection::TopToBottom, |tree| {
                        first_id = tree.widget(&first);
                        for button in &rest {
                            tree.widget(button);
                        }
                    });
                });
        });
        scroll_area.with_mut(|area| area.set_scroll(Vec2i::new(0, 36)));
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(180, 100));
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            style.as_ref(),
            &Input::default(),
            &mut results,
            rect(0, 0, 160, 80),
            ScrollBehavior::NONE,
            true,
        );

        let first_rect = runtime.nodes.get(&first_id).unwrap().rect;
        let body = scroll_area.with(|area| area.body());
        let content = scroll_area.with(|area| area.content_size());
        let scroll = scroll_area.with(|area| area.scroll());
        assert!(content.height > body.height);
        assert!(scroll.y > 0);
        assert!(first_rect.y < body.y);
    }

    #[test]
    fn node_root_scrollbar_view_is_stable_for_identical_size() {
        let buttons: Vec<_> = (0..6).map(|_| widget_handle(Button::new("wide row"))).collect();
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Fixed(150), SizePolicy::Fixed(24), StackDirection::TopToBottom, |tree| {
                for button in &buttons {
                    tree.widget(button);
                }
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(220, 160));
        let mut style = Style::default();
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        let body = rect(0, 0, 100, 80);

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            body,
            ScrollBehavior::NONE,
            true,
        );
        let first_client = runtime.nodes.get(&runtime.roots[0]).unwrap().client;
        let first_content = runtime.nodes.get(&runtime.roots[0]).unwrap().content_size;

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            body,
            ScrollBehavior::NONE,
            true,
        );
        let second_client = runtime.nodes.get(&runtime.roots[0]).unwrap().client;
        let second_content = runtime.nodes.get(&runtime.roots[0]).unwrap().content_size;

        assert!(first_content.width > first_client.width || first_content.height > first_client.height);
        assert!(same_rect(first_client, second_client));
        assert_eq!(first_content.width, second_content.width);
        assert_eq!(first_content.height, second_content.height);
    }

    #[test]
    fn node_root_full_viewport_custom_render_does_not_overflow_from_padding() {
        let custom = widget_handle(Custom::new("viewport"));
        let mut custom_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                custom_id = tree.custom_render(&custom, |_dim, _args| {});
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(220, 160));
        let mut style = Style::default();
        style.padding = 6;
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        let body = rect(0, 0, 120, 90);

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            body,
            ScrollBehavior::NONE,
            true,
        );

        let root = runtime.nodes.get(&runtime.roots[0]).unwrap();
        let custom_rect = runtime.nodes.get(&custom_id).unwrap().rect;
        assert_eq!(root.client.width, body.width - style.padding * 2);
        assert_eq!(root.client.height, body.height - style.padding * 2);
        assert_eq!(custom_rect.width, root.client.width);
        assert_eq!(custom_rect.height, root.client.height);
        assert!(root.content_size.width <= root.client.width);
        assert!(root.content_size.height <= root.client.height);
    }

    #[test]
    fn node_root_paints_slot_button_after_scrolling_to_slot_section() {
        let pixels = [255, 255, 255, 255];
        let chars = [(
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &chars,
            },
        )];
        let icons = [("white", Recti::new(0, 0, 1, 1))];
        let slots = [Recti::new(0, 0, 1, 1)];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &slots,
        });
        let slot = atlas.clone_slot_table()[0];
        let paint_count = Rc::new(std::cell::Cell::new(0));
        let paint_count_for_slot = paint_count.clone();
        let slot_button = widget_handle(Button::with_slot(
            "slot",
            slot,
            Rc::new(move |_x, _y| {
                paint_count_for_slot.set(paint_count_for_slot.get() + 1);
                color4b(255, 0, 0, 255)
            }),
            WidgetOption::NONE,
            WidgetFillOption::ALL,
        ));
        let filler = widget_handle(Button::new("filler"));
        let mut slot_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 180))).widget(filler.clone());
            slot_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 40))).widget(slot_button.clone());
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(140, 80));
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 120, 70),
            ScrollBehavior::NONE,
            true,
        );
        assert_eq!(paint_count.get(), 0);

        runtime.set_root_window_scroll_state(Vec2i::new(0, 160), None);
        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 120, 70),
            ScrollBehavior::NONE,
            true,
        );
        let root = runtime.nodes.get(&runtime.roots[0]).unwrap();
        let slot_node = runtime.nodes.get(&slot_id).unwrap();
        assert!(
            paint_count.get() > 0,
            "slot not painted; root client {:?} content {:?} slot rect {:?} clip {:?}",
            root.client,
            root.content_size,
            slot_node.rect,
            slot_node.clip
        );
    }

    #[test]
    fn node_fixed_width_image_button_derives_height_from_aspect_ratio() {
        let pixels = vec![255; 80 * 80 * 4];
        let chars = [(
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &chars,
            },
        )];
        let icons = [("white", Recti::new(0, 0, 1, 1))];
        let slots = [Recti::new(0, 0, 64, 64)];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 80,
            height: 80,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &slots,
        });
        let slot = atlas.clone_slot_table()[0];
        let button = widget_handle(Button::with_scaled_image(
            "image",
            Some(Image::Slot(slot)),
            WidgetOption::NONE,
            WidgetFillOption::ALL,
        ));
        let mut button_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                button_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed_width(48))).widget(&button);
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(200, 140));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 160, 120),
            ScrollBehavior::NONE,
            true,
        );

        let button_rect = runtime.nodes.get(&button_id).unwrap().rect;
        assert_eq!(button_rect.width, 48);
        assert_eq!(button_rect.height, 48);
    }

    #[test]
    fn node_fixed_width_regular_slot_button_keeps_inline_height() {
        let pixels = vec![255; 80 * 80 * 4];
        let chars = [(
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &chars,
            },
        )];
        let icons = [("white", Recti::new(0, 0, 1, 1))];
        let slots = [Recti::new(0, 0, 64, 64)];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 80,
            height: 80,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &slots,
        });
        let slot = atlas.clone_slot_table()[0];
        let button = widget_handle(Button::with_image("image", Some(Image::Slot(slot)), WidgetOption::NONE, WidgetFillOption::ALL));
        let mut button_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                button_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed_width(256))).widget(&button);
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(300, 160));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 280, 140),
            ScrollBehavior::NONE,
            true,
        );

        let button_rect = runtime.nodes.get(&button_id).unwrap().rect;
        assert_eq!(button_rect.width, 256);
        assert!(button_rect.height < 100, "regular slot button should stay inline-sized, got {:?}", button_rect);
    }

    #[test]
    fn node_grid_honors_explicit_child_spans() {
        let first = widget_handle(Button::new("a"));
        let second = widget_handle(Button::new("b"));
        let third = widget_handle(Button::new("c"));
        let mut first_id = Id::new(0);
        let mut second_id = Id::new(0);
        let mut third_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            let columns = [SizePolicy::Fixed(40), SizePolicy::Fixed(50), SizePolicy::Fixed(60)];
            let rows = [SizePolicy::Fixed(20), SizePolicy::Fixed(20)];
            tree.grid(&columns, &rows, |tree| {
                first_id = tree.node(crate::NodeOptions::with_policy(Policy::fill()).grid_span(2, 1)).widget(first.clone());
                second_id = tree.node(crate::NodeOptions::with_policy(Policy::fill())).widget(second.clone());
                third_id = tree.node(crate::NodeOptions::with_policy(Policy::fill())).widget(third.clone());
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(220, 80));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 220, 80),
            ScrollBehavior::NONE,
            true,
        );

        let first_rect = runtime.nodes.get(&first_id).unwrap().rect;
        let second_rect = runtime.nodes.get(&second_id).unwrap().rect;
        let third_rect = runtime.nodes.get(&third_id).unwrap().rect;
        assert_eq!(first_rect.width, 40 + style.spacing + 50);
        assert_eq!(second_rect.width, 60);
        assert!(second_rect.x > first_rect.x);
        assert_eq!(third_rect.x, first_rect.x);
        assert!(third_rect.y > first_rect.y);
    }
}
