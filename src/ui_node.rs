//! Common runtime node model used by the next retained traversal path.
//!
//! This module is intentionally internal while the existing `TraversalHost` path remains live.
//! It gives the crate one node representation that can own either a leaf widget or a framework
//! container without introducing a broad container trait before the enum-based passes exist.
#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use crate::{
    expand_rect, Canvas, ControlColor, CustomRenderArgs, CustomRenderCommand, Dimensioni, FrameResults, Id, Input, InputSnapshot, KeyCode, KeyMode, GridSpan,
    MouseButton, MouseEvent, Node, Recti, Renderer, RetainedId, ScrollAreaHandle, StackDirection, Style, UNCLIPPED_RECT, Vec2i, Vertex, WidgetHandle,
    WidgetTree,
};
use crate::container::{render_command_stream, Command};
use crate::draw_context::DrawCtx;
use crate::input::{ContainerOption, ControlState, ResourceState, ScrollBehavior, WidgetOption};
use crate::layout::SizePolicy;
use crate::widget::FocusPolicy;
use crate::widget_ctx::WidgetCtx;
use crate::widget_tree::{erased_widget_state, TreeCustomRender, WidgetStateHandleDyn, WidgetTreeNode, WidgetTreeNodeKind, WidgetTreeResource, WidgetTreeResources};

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
    fn new(id: UiNodeId, parent: Option<UiNodeId>, policy: crate::Policy, grid_span: GridSpan, data: UiNodeData) -> Self {
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
    fn children_mut(&mut self) -> Option<&mut Vec<UiNodeId>> {
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
        /// Concrete container behavior and data.
        kind: ContainerKind,
        /// Child membership. Leaf widgets do not carry this allocation.
        children: Vec<UiNodeId>,
    },
}

/// Concrete internal container kinds.
pub(crate) enum ContainerKind {
    /// Top-level window root.
    RootWindow(RootWindowData),
    /// Modal dialog root.
    Dialog(DialogData),
    /// Popup root.
    Popup(PopupData),
    /// Scrollable child region.
    ScrollArea(ScrollAreaData),
    /// Collapsible header with optional children.
    Header(DisclosureData),
    /// Tree node with optional indented children.
    Tree(DisclosureData),
    /// Horizontal row flow group.
    Row(RowData),
    /// Grid flow group.
    Grid(GridData),
    /// Nested column scope.
    Column(ColumnData),
    /// Stack scope.
    Stack(StackData),
}

/// Top-level window root data.
pub(crate) struct RootWindowData {
    /// Root chrome/sizing options.
    pub(crate) opt: ContainerOption,
    /// Scroll behavior applied to the root body.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Root z-order value.
    pub(crate) z_index: i32,
}

impl Default for RootWindowData {
    fn default() -> Self {
        Self {
            opt: ContainerOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            z_index: 0,
        }
    }
}

/// Modal dialog root data.
pub(crate) struct DialogData {
    /// Root chrome/sizing options.
    pub(crate) opt: ContainerOption,
    /// Scroll behavior applied to the dialog body.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Root z-order value.
    pub(crate) z_index: i32,
}

impl Default for DialogData {
    fn default() -> Self {
        Self {
            opt: ContainerOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            z_index: 0,
        }
    }
}

/// Popup root data.
pub(crate) struct PopupData {
    /// Root chrome/sizing options.
    pub(crate) opt: ContainerOption,
    /// Scroll behavior applied to the popup body.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Root z-order value.
    pub(crate) z_index: i32,
    /// Whether the popup was opened this frame.
    pub(crate) just_opened: bool,
}

impl Default for PopupData {
    fn default() -> Self {
        Self {
            opt: ContainerOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            z_index: 0,
            just_opened: false,
        }
    }
}

/// Scroll-area container data.
pub(crate) struct ScrollAreaData {
    /// Legacy retained scroll-area handle kept while existing traversal remains active.
    pub(crate) legacy_handle: Option<ScrollAreaHandle>,
    /// Current scroll offset.
    pub(crate) scroll_offset: Vec2i,
    /// Scroll behavior applied while traversing this node's children.
    pub(crate) scroll_behavior: ScrollBehavior,
    /// Rendering options applied to the scroll-area panel.
    pub(crate) opt: ContainerOption,
}

/// Header/tree disclosure container data.
pub(crate) struct DisclosureData {
    /// Widget state for the disclosure row.
    pub(crate) state: WidgetHandle<Node>,
    /// Whether child layout should be indented when expanded.
    pub(crate) indent_children: bool,
}

/// Row container data.
pub(crate) struct RowData {
    /// Width policies for row tracks.
    pub(crate) widths: Vec<SizePolicy>,
    /// Shared row height policy.
    pub(crate) height: SizePolicy,
}

/// Grid container data.
pub(crate) struct GridData {
    /// Width policies for columns.
    pub(crate) widths: Vec<SizePolicy>,
    /// Height policies for rows.
    pub(crate) heights: Vec<SizePolicy>,
}

/// Column container data.
pub(crate) struct ColumnData;

/// Stack container data.
pub(crate) struct StackData {
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

    /// Builds runtime nodes by consuming a retained widget tree.
    pub(crate) fn from_widget_tree(tree: WidgetTree) -> Self {
        let (roots, resources) = tree.into_parts();
        let mut resources = ResourceStore::new(resources);
        let mut runtime = Self::new();
        for root in roots {
            let root_id = runtime.insert_tree_node(None, root, &mut resources);
            runtime.roots.push(root_id);
            runtime.z_order.push(root_id);
        }
        runtime
    }

    /// Replaces all runtime nodes by consuming a fresh retained tree.
    pub(crate) fn replace_widget_tree(&mut self, tree: WidgetTree) {
        *self = Self::from_widget_tree(tree);
    }

    /// Moves focus to a node in this runtime.
    pub(crate) fn set_focus_node(&mut self, node: UiNodeId) {
        if self.nodes.contains_key(&node) {
            self.focus = Some(node);
            self.updated_focus = true;
        }
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
        canvas: &mut Canvas<R>,
        style: &Style,
        input: &Input,
        results: &mut FrameResults,
        rect: Recti,
        name: &str,
        opt: ContainerOption,
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

        let body = self.root_body_rect(rect, style, canvas.get_atlas(), name, opt, scroll_behavior);
        self.layout_roots(style, canvas.get_atlas(), body);

        let mut root_index = 0;
        while let Some(root) = self.root_at(root_index) {
            self.update_node(root_id, root, style, canvas.get_atlas(), input, results);
            root_index += 1;
        }

        let mut root_index = 0;
        while let Some(root) = self.root_at(root_index) {
            self.paint_node(root, style, canvas.get_atlas(), input);
            root_index += 1;
        }

        if !self.updated_focus {
            self.focus = None;
        }
        self.apply_deferred();
        self.clip_stack.pop();
        render_command_stream(canvas, &mut self.commands, &self.triangle_vertices);
        self.triangle_vertices.clear();
    }

    /// Inserts one retained tree node and descendants.
    fn insert_tree_node(&mut self, parent: Option<UiNodeId>, node: WidgetTreeNode, resources: &mut ResourceStore) -> UiNodeId {
        let (id, policy, grid_span, kind, children) = node.into_parts();
        let mut child_nodes = Vec::new();
        let data = match kind {
            WidgetTreeNodeKind::Widget { resource } => UiNodeData::Widget {
                widget: resources.take_widget(resource),
                custom_render: None,
            },
            WidgetTreeNodeKind::CustomRender { resource } => {
                let (widget, render) = resources.take_custom_render(resource);
                UiNodeData::Widget { widget, custom_render: Some(render) }
            }
            WidgetTreeNodeKind::ScrollArea { resource, opt, scroll_behavior } => UiNodeData::Container {
                kind: ContainerKind::ScrollArea(ScrollAreaData {
                    legacy_handle: Some(resources.take_scroll_area(resource)),
                    scroll_offset: Vec2i::default(),
                    scroll_behavior,
                    opt,
                }),
                children: Vec::new(),
            },
            WidgetTreeNodeKind::Header { resource } => UiNodeData::Container {
                kind: ContainerKind::Header(DisclosureData {
                    state: resources.take_node(resource),
                    indent_children: false,
                }),
                children: Vec::new(),
            },
            WidgetTreeNodeKind::Tree { resource } => UiNodeData::Container {
                kind: ContainerKind::Tree(DisclosureData {
                    state: resources.take_node(resource),
                    indent_children: true,
                }),
                children: Vec::new(),
            },
            WidgetTreeNodeKind::Row { widths, height } => UiNodeData::Container {
                kind: ContainerKind::Row(RowData { widths, height }),
                children: Vec::new(),
            },
            WidgetTreeNodeKind::Grid { widths, heights } => UiNodeData::Container {
                kind: ContainerKind::Grid(GridData { widths, heights }),
                children: Vec::new(),
            },
            WidgetTreeNodeKind::Column => UiNodeData::Container {
                kind: ContainerKind::Column(ColumnData),
                children: Vec::new(),
            },
            WidgetTreeNodeKind::Stack { width, height, direction } => UiNodeData::Container {
                kind: ContainerKind::Stack(StackData { width, height, direction }),
                children: Vec::new(),
            },
        };

        let ui_node = UiNode::new(id, parent, policy, grid_span, data);
        self.nodes.insert(id, ui_node);

        for child in children {
            child_nodes.push(self.insert_tree_node(Some(id), child, resources));
        }

        if let Some(node) = self.nodes.get_mut(&id) {
            if let Some(children) = node.children_mut() {
                *children = child_nodes;
            }
        }
        id
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

    /// Records root chrome and returns the client body used by retained children.
    fn root_body_rect(
        &mut self,
        rect: Recti,
        style: &Style,
        atlas: crate::AtlasHandle,
        name: &str,
        opt: ContainerOption,
        _scroll_behavior: ScrollBehavior,
    ) -> Recti {
        if !opt.intersects(ContainerOption::NO_FRAME) {
            self.draw_frame(rect, style, &atlas, ControlColor::WindowBG);
        }

        let mut body = rect;
        if !opt.intersects(ContainerOption::NO_TITLE) {
            let title_height = root_titlebar_height(style, &atlas);
            let title_rect = Recti::new(rect.x, rect.y, rect.width, title_height);
            self.draw_frame(title_rect, style, &atlas, ControlColor::TitleBG);
            self.draw_text(title_rect, style, &atlas, name, ControlColor::TitleText);
            body.y += title_height;
            body.height = body.height.saturating_sub(title_height);
        }
        body
    }

    /// Records a styled frame command.
    fn draw_frame(&mut self, rect: Recti, style: &Style, atlas: &crate::AtlasHandle, color: ControlColor) {
        let mut draw = DrawCtx::new(&mut self.commands, &mut self.triangle_vertices, &mut self.clip_stack, style, atlas);
        draw.draw_frame(rect, color);
    }

    /// Records root title text.
    fn draw_text(&mut self, rect: Recti, style: &Style, atlas: &crate::AtlasHandle, text: &str, color: ControlColor) {
        let mut draw = DrawCtx::new(&mut self.commands, &mut self.triangle_vertices, &mut self.clip_stack, style, atlas);
        draw.draw_control_text_with_font(style.title_font, text, rect, color, WidgetOption::NONE);
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

    /// Lays out root nodes inside the root client area using container-owned child membership.
    fn layout_roots(&mut self, style: &Style, atlas: crate::AtlasHandle, body: Recti) {
        let client = expand_rect(body, -style.padding);
        let mut y = client.y;
        let mut content_bounds = None;
        for index in 0..self.roots.len() {
            let Some(root) = self.root_at(index) else { continue };
            let remaining_height = (client.y + client.height - y).max(0);
            let preferred = self.measure_node(root, style, &atlas, Dimensioni::new(client.width, remaining_height));
            let height = if index + 1 == self.roots.len() {
                remaining_height
            } else {
                preferred.height.min(remaining_height).max(0)
            };
            let rect = Recti::new(client.x, y, client.width, height);
            self.layout_node(root, style, &atlas, rect, client);
            if let Some(node) = self.nodes.get(&root) {
                content_bounds = Some(match content_bounds {
                    Some(bounds) => union_rect(bounds, node.rect),
                    None => node.rect,
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
    }

    /// Measures one node's preferred size in the box-tree layout path.
    fn measure_node(&self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => {
                let preferred = widget.measure(style, atlas, available);
                let policy = self.nodes.get(&id).map(|node| node.policy).unwrap_or_else(crate::Policy::auto);
                Dimensioni::new(
                    resolve_size(policy.width, preferred.width, available.width, available.width, None),
                    resolve_size(policy.height, preferred.height, available.height, available.height, None),
                )
            }
            Some(UiNodeData::Container {
                kind: ContainerKind::Header(disclosure), ..
            }) => self.measure_disclosure(id, disclosure, style, atlas, available),
            Some(UiNodeData::Container {
                kind: ContainerKind::Tree(disclosure), ..
            }) => self.measure_disclosure(id, disclosure, style, atlas, available),
            Some(UiNodeData::Container { .. }) => self.container_behavior(id).measure(self, id, style, atlas, available),
            None => Dimensioni::default(),
        }
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
        Dimensioni::new(width.min(available.width).max(0), height.min(available.height).max(0))
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
        let width = preferred_widths.into_iter().sum::<i32>().saturating_add(spacing).min(available.width).max(0);
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
        Dimensioni::new(width.min(available.width).max(0), height.min(available.height).max(0))
    }

    /// Measures a header/tree disclosure container.
    fn measure_disclosure(&self, id: UiNodeId, disclosure: &DisclosureData, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
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
        let preferred = match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Widget { widget, .. }) => widget.measure(style, atlas, Dimensioni::new(rect.width.max(0), rect.height.max(0))),
            _ => Dimensioni::default(),
        };
        let policy = self.nodes.get(&id).map(|node| node.policy).unwrap_or_else(crate::Policy::auto);
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
        let disclosure = self.disclosure_behavior(id);
        let behavior = disclosure.is_none().then(|| self.container_behavior(id));
        let node_clip = clip.intersect(&rect).unwrap_or_default();
        if let Some(node) = self.nodes.get_mut(&id) {
            node.rect = rect;
            node.client = rect;
            node.clip = node_clip;
        }

        if let Some(disclosure) = disclosure {
            self.layout_disclosure_children(id, &disclosure, style, atlas, rect, node_clip);
        } else if let Some(behavior) = behavior {
            behavior.layout(self, id, style, atlas, rect, node_clip);
        }

        let content_rect = self.child_bounds(id).unwrap_or(rect);
        let content_size = Dimensioni::new(
            (content_rect.x + content_rect.width - rect.x).max(0),
            (content_rect.y + content_rect.height - rect.y).max(0),
        );
        if let Some(node) = self.nodes.get_mut(&id) {
            node.content_size = content_size;
        }
        Dimensioni::new(rect.width, rect.height)
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

    /// Returns the union of direct child rectangles.
    fn child_bounds(&self, id: UiNodeId) -> Option<Recti> {
        let mut bounds = None;
        for index in 0..self.child_count(id) {
            let child_rect = self.child_at(id, index).and_then(|child| self.nodes.get(&child)).map(|node| node.rect)?;
            bounds = Some(match bounds {
                Some(rect) => union_rect(rect, child_rect),
                None => child_rect,
            });
        }
        bounds
    }

    /// Returns the behavior object for a container node.
    fn container_behavior(&self, id: UiNodeId) -> ContainerBehavior {
        match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Container { kind: ContainerKind::Row(row), .. }) => ContainerBehavior::Row {
                widths: row.widths.clone(),
                height: row.height,
            },
            Some(UiNodeData::Container { kind: ContainerKind::Grid(grid), .. }) => ContainerBehavior::Grid {
                widths: grid.widths.clone(),
                heights: grid.heights.clone(),
            },
            Some(UiNodeData::Container { kind: ContainerKind::Stack(stack), .. }) => ContainerBehavior::Stack {
                width: stack.width,
                height: stack.height,
                direction: stack.direction,
            },
            _ => ContainerBehavior::Column,
        }
    }

    /// Returns disclosure behavior data for header/tree nodes.
    fn disclosure_behavior(&self, id: UiNodeId) -> Option<DisclosureBehavior> {
        match self.nodes.get(&id).map(|node| &node.data) {
            Some(UiNodeData::Container {
                kind: ContainerKind::Header(disclosure), ..
            }) => Some(DisclosureBehavior {
                state: disclosure.state.clone(),
                indent_children: disclosure.indent_children,
            }),
            Some(UiNodeData::Container {
                kind: ContainerKind::Tree(disclosure), ..
            }) => Some(DisclosureBehavior {
                state: disclosure.state.clone(),
                indent_children: disclosure.indent_children,
            }),
            _ => None,
        }
    }

    /// Lays out a header/tree disclosure row and optional children.
    fn layout_disclosure_children(
        &mut self,
        id: UiNodeId,
        disclosure: &DisclosureBehavior,
        style: &Style,
        atlas: &crate::AtlasHandle,
        rect: Recti,
        clip: Recti,
    ) {
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
        match self.nodes.get(&child).map(|node| &node.data) {
            Some(UiNodeData::Container { kind: ContainerKind::Row(row), .. }) => row.height,
            _ => SizePolicy::Auto,
        }
    }

    /// Returns the effective horizontal placement policy for a child in a row.
    fn horizontal_track_policy(&self, child: UiNodeId, track: SizePolicy) -> SizePolicy {
        let policy = self.nodes.get(&child).map(|node| node.policy.width).unwrap_or(SizePolicy::Auto);
        if policy != SizePolicy::Auto { policy } else { track }
    }

    /// Updates one node and descendants.
    fn update_node(&mut self, root_id: crate::RootId, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input, results: &mut FrameResults) {
        let kind = self.node_kind_tag(id);
        match kind {
            RuntimeNodeKind::Widget => self.update_widget(root_id, id, style, atlas, input, results),
            RuntimeNodeKind::Container => {
                let disclosure = self.disclosure_behavior(id);
                let traverse_children = match &disclosure {
                    Some(disclosure) => {
                        self.update_disclosure_widget(root_id, id, disclosure, style, atlas.clone(), input, results);
                        disclosure.state.read(|state| state.state).is_expanded()
                    }
                    None => true,
                };
                if traverse_children {
                    for index in 0..self.child_count(id) {
                        let Some(child) = self.child_at(id, index) else { continue };
                        self.update_node(root_id, child, style, atlas.clone(), input, results);
                    }
                }
            }
        }
    }

    /// Updates a header/tree disclosure widget stored on a container node.
    fn update_disclosure_widget(
        &mut self,
        root_id: crate::RootId,
        id: UiNodeId,
        disclosure: &DisclosureBehavior,
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
    fn update_widget(&mut self, root_id: crate::RootId, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input, results: &mut FrameResults) {
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
        results.record_retained_with_context(RetainedId::root_node(root_id, id), id, widget_handle_id, result, format!("ui node {:?}", id));
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
        if hovered && input.mouse_down.is_empty() {
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

    /// Paints one node and descendants.
    fn paint_node(&mut self, id: UiNodeId, style: &Style, atlas: crate::AtlasHandle, input: &Input) {
        let kind = self.node_kind_tag(id);
        match kind {
            RuntimeNodeKind::Widget => self.paint_widget(id, style, atlas, input),
            RuntimeNodeKind::Container => {
                let is_scroll_area = matches!(
                    self.nodes.get(&id).map(|node| &node.data),
                    Some(UiNodeData::Container { kind: ContainerKind::ScrollArea(_), .. })
                );
                if is_scroll_area {
                    self.paint_scroll_area_panel(id, style, &atlas);
                    self.push_node_clip(id);
                }

                let disclosure = self.disclosure_behavior(id);
                let traverse_children = match &disclosure {
                    Some(disclosure) => {
                        self.paint_disclosure_widget(id, disclosure, style, atlas.clone());
                        disclosure.state.read(|state| state.state).is_expanded()
                    }
                    None => true,
                };
                if traverse_children {
                    for index in 0..self.child_count(id) {
                        let Some(child) = self.child_at(id, index) else { continue };
                        self.paint_node(child, style, atlas.clone(), input);
                    }
                }

                if is_scroll_area {
                    self.pop_node_clip();
                }
            }
        }
    }

    /// Paints a header/tree disclosure widget stored on a container node.
    fn paint_disclosure_widget(&mut self, id: UiNodeId, disclosure: &DisclosureBehavior, style: &Style, atlas: crate::AtlasHandle) {
        let rect = self.nodes.get(&id).map(|node| node.client).unwrap_or_default();
        let control = self.nodes.get(&id).map(|node| node.control).unwrap_or_default();
        let widget = erased_widget_state(disclosure.state.clone());
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
            true,
            None,
        );
        widget.paint(&mut ctx, &control);
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
            let view = self.current_clip_rect().intersect(&rect).unwrap_or_else(|| Recti::new(rect.x, rect.y, 0, 0));
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
    fn paint_scroll_area_panel(&mut self, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle) {
        let Some((rect, opt)) = self.nodes.get(&id).and_then(|node| match &node.data {
            UiNodeData::Container {
                kind: ContainerKind::ScrollArea(scroll), ..
            } => Some((node.rect, scroll.opt)),
            _ => None,
        }) else {
            return;
        };
        if !opt.intersects(ContainerOption::NO_FRAME) {
            let mut draw = DrawCtx::new(&mut self.commands, &mut self.triangle_vertices, &mut self.clip_stack, style, atlas);
            draw.draw_frame(rect, ControlColor::PanelBG);
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

/// Copied disclosure behavior data used outside a node borrow.
struct DisclosureBehavior {
    /// Widget state for the disclosure row.
    state: WidgetHandle<Node>,
    /// Whether child layout should be indented when expanded.
    indent_children: bool,
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

/// Common internal behavior interface for child-owning nodes.
trait ContainerTrait {
    /// Measures the preferred size for a container node.
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni;

    /// Assigns rectangles to children and recursively lays them out.
    fn layout(&self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti);
}

/// Concrete internal container behavior used by the runtime pass.
enum ContainerBehavior {
    /// Column-like vertical layout.
    Column,
    /// Row layout with track policies.
    Row {
        /// Width tracks for row children.
        widths: Vec<SizePolicy>,
        /// Shared row height.
        height: SizePolicy,
    },
    /// Grid layout with column and row policies.
    Grid {
        /// Width tracks for columns.
        widths: Vec<SizePolicy>,
        /// Height tracks for rows.
        heights: Vec<SizePolicy>,
    },
    /// Stack layout with per-item policies.
    Stack {
        /// Width policy for each stack item.
        width: SizePolicy,
        /// Height policy for each stack item.
        height: SizePolicy,
        /// Stack direction.
        direction: StackDirection,
    },
}

impl ContainerTrait for ContainerBehavior {
    fn measure(&self, runtime: &UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        match self {
            Self::Column => runtime.measure_column(id, style, atlas, available),
            Self::Row { widths: _, height } => runtime.measure_row(id, *height, style, atlas, available),
            Self::Grid { widths, heights } => runtime.measure_grid(id, widths, heights, style, atlas, available),
            Self::Stack { width, height, direction: _ } => runtime.measure_stack(id, *width, *height, style, atlas, available),
        }
    }

    fn layout(&self, runtime: &mut UiRuntime, id: UiNodeId, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, clip: Recti) {
        match self {
            Self::Column => runtime.layout_column_children(id, style, atlas, rect, clip),
            Self::Row { widths, height } => runtime.layout_row_children(id, style, atlas, rect, clip, widths, *height),
            Self::Grid { widths, heights } => runtime.layout_grid_children(id, style, atlas, rect, clip, widths, heights),
            Self::Stack { width, height, direction } => runtime.layout_stack_children(id, style, atlas, rect, clip, *width, *height, *direction),
        }
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

/// Owned resource table used while consuming a `WidgetTree`.
struct ResourceStore {
    /// Optional resources so conversion can move each entry exactly once.
    entries: Vec<Option<WidgetTreeResource>>,
}

impl ResourceStore {
    /// Creates a movable resource store.
    fn new(resources: WidgetTreeResources) -> Self {
        Self {
            entries: resources.into_entries().into_iter().map(Some).collect(),
        }
    }

    /// Takes a widget resource.
    fn take_widget(&mut self, id: crate::widget_tree::TreeResourceId) -> Box<dyn WidgetStateHandleDyn> {
        match self.take(id) {
            WidgetTreeResource::Widget(widget) => widget,
            _ => panic!("tree resource {:?} is not a widget", id),
        }
    }

    /// Takes a custom-render resource.
    fn take_custom_render(&mut self, id: crate::widget_tree::TreeResourceId) -> (Box<dyn WidgetStateHandleDyn>, TreeCustomRender) {
        match self.take(id) {
            WidgetTreeResource::CustomRender { state, render } => (crate::widget_tree::erased_widget_state(state), render),
            _ => panic!("tree resource {:?} is not a custom-render resource", id),
        }
    }

    /// Takes a scroll-area resource.
    fn take_scroll_area(&mut self, id: crate::widget_tree::TreeResourceId) -> ScrollAreaHandle {
        match self.take(id) {
            WidgetTreeResource::ScrollArea(handle) => handle,
            _ => panic!("tree resource {:?} is not a scroll-area resource", id),
        }
    }

    /// Takes a disclosure node resource.
    fn take_node(&mut self, id: crate::widget_tree::TreeResourceId) -> WidgetHandle<Node> {
        match self.take(id) {
            WidgetTreeResource::Node(state) => state,
            _ => panic!("tree resource {:?} is not a node resource", id),
        }
    }

    /// Takes a raw resource.
    fn take(&mut self, id: crate::widget_tree::TreeResourceId) -> WidgetTreeResource {
        self.entries
            .get_mut(id.index())
            .and_then(Option::take)
            .unwrap_or_else(|| panic!("tree resource {:?} was missing or already consumed", id))
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
    let mut used: i32 = 0;
    let mut total_weight = 0.0f32;
    let mut remainder = Vec::new();

    for (index, policy) in policies.iter().copied().enumerate() {
        match policy {
            SizePolicy::Auto => {
                sizes[index] = preferred.get(index).copied().unwrap_or_default().max(0);
                used = used.saturating_add(sizes[index]);
            }
            SizePolicy::Fixed(value) => {
                sizes[index] = value.max(0);
                used = used.saturating_add(sizes[index]);
            }
            SizePolicy::Fraction(value) => {
                sizes[index] = resolve_size(SizePolicy::Fraction(value), 0, available, available, None);
                used = used.saturating_add(sizes[index]);
            }
            SizePolicy::Weight(value) => {
                if value.is_finite() {
                    total_weight += value.max(0.0);
                }
            }
            SizePolicy::Remainder(margin) => remainder.push((index, margin.max(0))),
        }
    }

    let mut remaining = available.saturating_sub(used);
    if total_weight > 0.0 {
        for (index, policy) in policies.iter().copied().enumerate() {
            if let SizePolicy::Weight(value) = policy {
                sizes[index] = resolve_size(SizePolicy::Weight(value), 0, remaining, remaining, Some(total_weight));
            }
        }
        used = sizes.iter().copied().sum::<i32>();
        remaining = available.saturating_sub(used);
    }

    if !remainder.is_empty() {
        let each = remaining / remainder.len() as i32;
        for (index, margin) in remainder {
            sizes[index] = each.saturating_sub(margin).max(0);
        }
    }
    sizes
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
    use crate::{rect, Button, Canvas, Policy, RendererHandle, Textbox, WidgetTreeBuilder, widget_handle};
    use crate::test_support::{test_atlas, NoopRenderer};

    #[test]
    fn widget_tree_conversion_keeps_container_children_off_leaf_widgets() {
        let button = widget_handle(Button::new("child"));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(10, 20))).column(|tree| {
                tree.widget(button.clone());
            });
        });

        let runtime = UiRuntime::from_widget_tree(tree);
        let root = runtime.roots[0];
        let root_node = runtime.nodes.get(&root).expect("root node missing");
        let child = root_node.children()[0];
        let child_node = runtime.nodes.get(&child).expect("child node missing");

        assert!(matches!(root_node.data, UiNodeData::Container { .. }));
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
                kind: ContainerKind::Column(ColumnData),
                children: Vec::new(),
            },
        );
        let second = UiNode::new(
            Id::new(2),
            None,
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Container {
                kind: ContainerKind::Column(ColumnData),
                children: Vec::new(),
            },
        );
        let child = UiNode::new(
            Id::new(3),
            Some(Id::new(1)),
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Container {
                kind: ContainerKind::Column(ColumnData),
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
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                tree.widget(button.clone());
            });
        });
        let mut runtime = UiRuntime::from_widget_tree(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(400, 500));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(40, 40, 300, 450),
            "simple",
            ContainerOption::NONE,
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
        let tree = WidgetTreeBuilder::build(|tree| {
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
        let mut runtime = UiRuntime::from_widget_tree(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(320, 420));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 320, 420),
            "calculator",
            ContainerOption::NO_TITLE | ContainerOption::NO_RESIZE,
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
    fn node_grid_honors_explicit_child_spans() {
        let first = widget_handle(Button::new("a"));
        let second = widget_handle(Button::new("b"));
        let third = widget_handle(Button::new("c"));
        let mut first_id = Id::new(0);
        let mut second_id = Id::new(0);
        let mut third_id = Id::new(0);
        let tree = WidgetTreeBuilder::build(|tree| {
            let columns = [SizePolicy::Fixed(40), SizePolicy::Fixed(50), SizePolicy::Fixed(60)];
            let rows = [SizePolicy::Fixed(20), SizePolicy::Fixed(20)];
            tree.grid(&columns, &rows, |tree| {
                first_id = tree.node(crate::NodeOptions::with_policy(Policy::fill()).grid_span(2, 1)).widget(first.clone());
                second_id = tree.node(crate::NodeOptions::with_policy(Policy::fill())).widget(second.clone());
                third_id = tree.node(crate::NodeOptions::with_policy(Policy::fill())).widget(third.clone());
            });
        });
        let mut runtime = UiRuntime::from_widget_tree(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(220, 80));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 220, 80),
            "grid",
            ContainerOption::NO_TITLE | ContainerOption::NO_RESIZE,
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
