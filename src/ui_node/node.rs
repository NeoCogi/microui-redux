use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::render::{CustomRenderHandle, RendererBackend};
use crate::{Dimensioni, Recti, Vec2i, Widget, WidgetStateOwner};

use super::containers::{with_container_children, with_container_children_mut};
use super::{Container, WidgetNode};

/// Process-wide source of runtime-only node identity.
///
/// Relaxed ordering is sufficient: the counter establishes uniqueness and does not publish any
/// node memory or synchronize traversal.
static NEXT_RUNTIME_NODE_ID: AtomicU64 = AtomicU64::new(1);

const fn advance_runtime_node_id(current: u64) -> Option<u64> {
    current.checked_add(1)
}

/// Runtime-private identity assigned exactly once when an owning [`Node`] is created.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RuntimeNodeId(NonZeroU64);

impl RuntimeNodeId {
    fn allocate() -> Self {
        let raw = NEXT_RUNTIME_NODE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, advance_runtime_node_id)
            .expect("RuntimeNodeId space exhausted");
        Self(NonZeroU64::new(raw).expect("RuntimeNodeId allocator returned zero"))
    }
}

/// Persistent layout result for one runtime node.
#[derive(Copy, Clone, Debug)]
pub(crate) struct NodeLayout {
    /// Allocation assigned by the parent, in the parent's content coordinate space.
    pub(crate) allocation: Recti,
    /// Node-local transform and viewport exposed to children.
    pub(crate) children: ChildLayout,
    /// Measured/assigned virtual content size in content coordinates.
    pub(crate) content_size: Dimensioni,
    /// Whether child overflow contributes to this node's parent-visible content size.
    pub(crate) propagate_child_overflow: bool,
}

impl Default for NodeLayout {
    fn default() -> Self {
        Self {
            allocation: Recti::default(),
            children: ChildLayout::default(),
            content_size: Dimensioni::default(),
            propagate_child_overflow: true,
        }
    }
}

impl NodeLayout {
    /// Builds a simple non-scrolled layout.
    pub(crate) fn from_rect(rect: Recti, content_size: Dimensioni) -> Self {
        Self::from_parts(rect, Recti::new(0, 0, rect.width.max(0), rect.height.max(0)), content_size)
    }

    /// Builds a layout from an outer allocation and node-local child viewport.
    pub(crate) fn from_parts(allocation: Recti, child_clip: Recti, content_size: Dimensioni) -> Self {
        Self {
            allocation,
            children: ChildLayout::new(child_clip),
            content_size,
            propagate_child_overflow: true,
        }
    }

    /// Returns this layout with an updated content size.
    pub(crate) fn with_content_size(mut self, content_size: Dimensioni) -> Self {
        self.content_size = content_size;
        self
    }

    /// Returns this layout with updated child-overflow propagation.
    pub(crate) fn with_child_overflow_propagation(mut self, propagate_child_overflow: bool) -> Self {
        self.propagate_child_overflow = propagate_child_overflow;
        self
    }
}

/// Node-local transform and viewport exposed to child nodes.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct ChildLayout {
    /// Translation from child content coordinates into this node's local coordinates.
    pub(crate) offset: Vec2i,
    /// Visible child viewport in this node's local coordinates.
    pub(crate) clip: Recti,
}

impl ChildLayout {
    /// Builds child-layout data from a node-local viewport.
    pub(crate) fn new(clip: Recti) -> Self {
        Self { offset: Vec2i::default(), clip }
    }
}

/// Stack-only transform derived while walking the node tree.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Transform {
    /// Translation from the current content coordinate space to screen coordinates.
    pub(crate) offset: Vec2i,
    /// Inherited effective clip in screen coordinates.
    pub(crate) clip: Recti,
}

impl Transform {
    /// Creates a root transform.
    pub(crate) fn root(screen_clip: Recti) -> Self {
        Self {
            offset: Vec2i::default(),
            clip: screen_clip,
        }
    }

    /// Creates a root transform with a screen-space origin.
    pub(crate) fn root_at(origin: Vec2i, screen_clip: Recti) -> Self {
        Self { offset: origin, clip: screen_clip }
    }

    /// Pushes a node's child coordinate system onto the transform stack.
    pub(crate) fn push(self, layout: NodeLayout) -> Self {
        let node_origin = self.offset + Vec2i::new(layout.allocation.x, layout.allocation.y);
        let screen_clip = translate_rect(layout.children.clip, node_origin);
        Self {
            offset: node_origin + layout.children.offset,
            clip: self.clip.intersect(&screen_clip).unwrap_or_default(),
        }
    }

    /// Resolves a parent-local allocation into screen coordinates.
    pub(crate) fn resolve(self, allocation: Recti) -> Recti {
        translate_rect(allocation, self.offset)
    }
}

fn translate_rect(rect: Recti, offset: Vec2i) -> Recti {
    Recti::new(rect.x + offset.x, rect.y + offset.y, rect.width, rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_node(label: &str) -> (crate::WidgetStateHandle<crate::TextBlockState>, Node) {
        let (state, runtime) = crate::TextBlock::create(crate::TextBlockParameters::new(label));
        (state, Node::widget(runtime))
    }

    #[test]
    fn pushing_a_node_uses_node_local_child_geometry() {
        let mut layout = NodeLayout::from_parts(Recti::new(10, 20, 30, 40), Recti::new(2, 3, 20, 10), Dimensioni::new(30, 40));
        layout.children.offset = Vec2i::new(4, -5);

        let parent = Transform::root_at(Vec2i::new(100, 200), Recti::new(0, 0, 1000, 1000));
        let child = parent.push(layout);

        assert_eq!((child.offset.x, child.offset.y), (114, 215));
        assert_eq!((child.clip.x, child.clip.y, child.clip.width, child.clip.height), (112, 223, 20, 10));
    }

    #[test]
    fn owning_nodes_allocate_once_and_preserve_identity_while_moved_or_rejected() {
        let (_, first) = text_node("first");
        let first_id = first.state.id.0.get();
        let (_, second) = text_node("second");
        let second_id = second.state.id.0.get();
        assert!(second_id > first_id, "the process-wide allocator must increase monotonically");

        let configured = first.with_policy(crate::Policy::fixed(17, 23));
        assert_eq!(configured.state.id.0.get(), first_id);
        assert_eq!(configured.state.policy, crate::Policy::fixed(17, 23));

        let mut children = Children::new();
        let rejected = children.insert(1, configured).expect_err("out-of-range insertion must reject the exact owner");
        assert_eq!(rejected.state.id.0.get(), first_id);
        assert!(children.is_empty());

        children.push(rejected);
        assert_eq!(children.nodes[0].state.id.0.get(), first_id);
    }

    #[test]
    fn runtime_identity_exhaustion_is_detected_without_mutating_the_global_allocator() {
        assert_eq!(advance_runtime_node_id(1), Some(2));
        assert_eq!(advance_runtime_node_id(u64::MAX), None);
    }

    #[test]
    fn children_topology_operations_drop_only_the_replaced_owners() {
        let (first_state, first) = text_node("first");
        let (second_state, second) = text_node("second");
        let (third_state, third) = text_node("third");
        let mut children: Children = [first, second].into_iter().collect();

        assert_eq!(children.len(), 2);
        assert!(children.remove_drop(0));
        assert!(!first_state.is_alive());
        assert!(second_state.is_alive());

        children.replace([third]);
        assert!(!second_state.is_alive());
        assert!(third_state.is_alive());

        children.clear();
        assert!(!third_state.is_alive());
        assert!(!children.remove_drop(0));
    }

    #[test]
    fn ownership_moving_state_access_returns_the_same_unmounted_node_on_failure() {
        let (column_state, column_node) = crate::Column::create(crate::ColumnParameters::default());
        let (_, candidate) = text_node("candidate");
        let candidate_id = candidate.state.id.0.get();

        let rejected = column_state
            .try_read(|_| {
                column_state
                    .try_update_with(candidate, |column, node| column.push(node))
                    .expect_err("the active read must prevent mutation")
            })
            .expect("column read must be available");
        assert_eq!(rejected.state.id.0.get(), candidate_id);

        drop(column_node);
        let rejected = column_state
            .try_update_with(rejected, |column, node| column.push(node))
            .expect_err("expired state must return the input owner");
        assert_eq!(rejected.state.id.0.get(), candidate_id);
    }
}

/// Common derived and transient runtime state shared by widgets and containers.
///
/// This state deliberately has no generic visibility bit. Root visibility and container-owned
/// descendant gating are separate mechanisms with different owners.
pub(crate) struct NodeRuntime {
    /// Private process-unique runtime identity.
    id: RuntimeNodeId,
    /// Persistent layout result for traversal.
    pub(crate) layout: NodeLayout,
    /// Cursor is hovering this node.
    pub(crate) hovered: bool,
    /// This node currently owns focus.
    pub(crate) focused: bool,
    /// Mouse was pressed on this node during the current frame.
    pub(crate) clicked: bool,
    /// Mouse is held down while this node owns focus.
    pub(crate) active: bool,
    /// Scroll delta consumed by this node during the current frame.
    /// Placement policy used by runtime layout passes.
    pub(crate) policy: crate::Policy,
}

impl NodeRuntime {
    /// Returns this node's private process-unique identity.
    pub(crate) fn id(&self) -> RuntimeNodeId {
        self.id
    }

    /// Writes layout as the source of truth.
    pub(crate) fn set_layout(&mut self, layout: NodeLayout) {
        self.layout = layout;
    }

    /// Writes a simple non-scrolled layout.
    pub(crate) fn set_layout_from_rect(&mut self, rect: Recti, content_size: Dimensioni) {
        self.set_layout(NodeLayout::from_rect(rect, content_size));
    }
}

/// Unique owning retained-tree node.
///
/// A `Node` owns exactly one concrete widget or container runtime. It is intentionally not
/// cloneable: successful insertion transfers ownership into one [`Children`] collection. Its
/// process-unique identity is runtime-private, unrelated to public [`crate::RootId`] values, and is
/// never stored in or exposed through a [`crate::WidgetStateHandle`].
pub struct Node {
    /// Common state for layout, identity, and interaction.
    pub(crate) state: NodeRuntime,
    /// Node-specific payload.
    pub(crate) data: NodeKind,
}

impl Node {
    /// Creates a leaf node from one concrete state-owning widget runtime.
    pub fn widget<W: WidgetStateOwner>(widget: W) -> Self {
        Self::widget_with_custom_render(widget, None)
    }

    /// Creates a leaf node with a backend-typed custom-render callback.
    ///
    /// The backend-specific handle is erased only after this checked public boundary; the stored
    /// key remains private and is validated by the renderer registry before use.
    pub fn custom_render<B, W>(widget: W, renderer: CustomRenderHandle<B>) -> Self
    where
        B: RendererBackend,
        W: WidgetStateOwner,
    {
        Self::widget_with_custom_render(widget, Some(renderer.key))
    }

    fn widget_with_custom_render<W: WidgetStateOwner>(widget: W, custom_render: Option<crate::render::CustomRenderKey>) -> Self {
        Self::from_kind(NodeKind::Widget(WidgetNode::new(widget, custom_render)))
    }

    /// Creates a container node from one concrete state-owning container runtime.
    pub fn container<C>(container: C) -> Self
    where
        C: Container + WidgetStateOwner,
    {
        Self::from_kind(NodeKind::Container(Box::new(container)))
    }

    /// Replaces this still-unmounted node's parent placement policy.
    pub fn with_policy(mut self, policy: crate::Policy) -> Self {
        self.state.policy = policy;
        self
    }

    fn from_kind(kind: NodeKind) -> Self {
        Self {
            state: NodeRuntime {
                id: RuntimeNodeId::allocate(),
                layout: NodeLayout::default(),
                hovered: false,
                focused: false,
                clicked: false,
                active: false,
                policy: crate::Policy::auto(),
            },
            data: kind,
        }
    }

    /// Returns this node's private identity for runtime-only traversal.
    pub(crate) fn id(&self) -> RuntimeNodeId {
        self.state.id()
    }

    /// Measures this node without requiring a Context or runtime registry.
    ///
    /// Public `Children::measure_child` uses this path so downstream containers can implement the
    /// inherited `Widget::measure` contract while holding their own state borrow.
    pub(crate) fn measure(&self, style: &crate::Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> NodeMeasurement {
        let framed = self.data.widget().effective_widget_opt().intersects(crate::WidgetOption::FRAME);
        let border_width = if framed { style.frame_border().width } else { 0 };
        let policy = self.state.policy;
        let outer_available = Dimensioni::new(
            super::measure_axis_available(policy.width, available.width),
            super::measure_axis_available(policy.height, available.height),
        );
        let content_available = crate::frame::content_available(outer_available, border_width);
        let preferred_content = self.data.widget().measure(style, atlas, content_available);
        let preferred_outer = crate::frame::outer_preferred(preferred_content, border_width);
        let resolved_outer = Dimensioni::new(
            super::resolve_size(policy.width, preferred_outer.width, available.width, available.width, None),
            super::resolve_size(policy.height, preferred_outer.height, available.height, available.height, None),
        );
        NodeMeasurement { resolved_outer, preferred_outer }
    }

    /// Writes layout as the source of truth.
    pub(crate) fn set_layout(&mut self, layout: NodeLayout) {
        self.state.set_layout(layout);
    }

    /// Writes a simple non-scrolled layout.
    pub(crate) fn set_layout_from_rect(&mut self, rect: Recti, content_size: Dimensioni) {
        self.set_layout(NodeLayout::from_rect(rect, content_size));
    }

    /// Returns the node's children when it accepts children.
    pub(crate) fn with_children<R>(&self, f: impl FnOnce(&[Node]) -> R) -> R {
        match &self.data {
            NodeKind::Widget(_) => f(&[]),
            NodeKind::Container(container) => with_container_children(&**container, |children| f(children.as_slice())),
        }
    }

    /// Runs `f` with this node's authoritative child collection when it is a container.
    pub(crate) fn with_children_mut<R>(&mut self, f: impl FnOnce(&mut [Node]) -> R) -> Option<R> {
        match &mut self.data {
            NodeKind::Widget(_) => None,
            NodeKind::Container(container) => Some(with_container_children_mut(&mut **container, |children| f(children.as_mut_slice()))),
        }
    }

    /// Returns whether this node is a container.
    pub(crate) fn is_container(&self) -> bool {
        matches!(self.data, NodeKind::Container(_))
    }

    /// Counts this node and every retained descendant.
    #[cfg(test)]
    pub(crate) fn debug_node_count(&self) -> usize {
        self.with_children(|children| 1 + children.iter().map(Self::debug_node_count).sum::<usize>())
    }

    /// Counts old erased public-widget adapters in this subtree.
    #[cfg(test)]
    pub(crate) fn debug_erased_adapter_count(&self) -> usize {
        self.with_children(|children| children.iter().map(Self::debug_erased_adapter_count).sum())
    }

    /// Runs `f` against one matching node without returning a borrow through the opaque visitor.
    pub(crate) fn with_node<R>(&self, id: RuntimeNodeId, f: impl FnOnce(&Node) -> R) -> Option<R> {
        let mut f = Some(f);
        self.with_node_inner(id, &mut f)
    }

    fn with_node_inner<R, F>(&self, id: RuntimeNodeId, f: &mut Option<F>) -> Option<R>
    where
        F: FnOnce(&Node) -> R,
    {
        if self.id() == id {
            return Some(f.take().expect("node visitor invoked twice")(self));
        }
        self.with_children(|children| children.iter().find_map(|child| child.with_node_inner(id, f)))
    }

    /// Runs `f` mutably against one matching node without exposing attached storage.
    pub(crate) fn with_node_mut<R>(&mut self, id: RuntimeNodeId, f: impl FnOnce(&mut Node) -> R) -> Option<R> {
        let mut f = Some(f);
        self.with_node_mut_inner(id, &mut f)
    }

    fn with_node_mut_inner<R, F>(&mut self, id: RuntimeNodeId, f: &mut Option<F>) -> Option<R>
    where
        F: FnOnce(&mut Node) -> R,
    {
        if self.id() == id {
            return Some(f.take().expect("mutable node visitor invoked twice")(self));
        }
        self.with_children_mut(|children| children.iter_mut().find_map(|child| child.with_node_mut_inner(id, f)))?
    }

    /// Collects this node id and all descendant ids.
    pub(crate) fn collect_ids(&self, ids: &mut Vec<RuntimeNodeId>) {
        ids.push(self.id());
        self.with_children(|children| {
            for child in children.iter() {
                child.collect_ids(ids);
            }
        });
    }
}

/// Private runtime payload for an owning [`Node`].
pub(crate) enum NodeKind {
    /// Direct state-owning leaf runtime.
    Widget(WidgetNode),
    /// Direct state-owning public container runtime.
    Container(Box<dyn Container>),
}

impl NodeKind {
    /// Returns the one common runtime phase object for either node variant.
    pub(crate) fn widget(&self) -> &dyn Widget {
        match self {
            Self::Widget(node) => &*node.widget,
            Self::Container(container) => &**container,
        }
    }

    /// Returns the one mutable common runtime phase object for either node variant.
    pub(crate) fn widget_mut(&mut self) -> &mut dyn Widget {
        match self {
            Self::Widget(node) => &mut *node.widget,
            Self::Container(container) => &mut **container,
        }
    }

    /// Returns the container-specific runtime when this node owns children.
    pub(crate) fn container(&self) -> Option<&dyn Container> {
        match self {
            Self::Widget(_) => None,
            Self::Container(container) => Some(&**container),
        }
    }

    /// Returns the mutable container-specific runtime for lifecycle notification.
    pub(crate) fn container_mut(&mut self) -> Option<&mut dyn Container> {
        match self {
            Self::Widget(_) => None,
            Self::Container(container) => Some(&mut **container),
        }
    }
}

/// One authoritative node measurement reused by parent allocation and leaf content sizing.
pub(crate) struct NodeMeasurement {
    /// Policy-resolved preferred outer size returned to the parent.
    pub(crate) resolved_outer: Dimensioni,
    /// Intrinsic outer size before parent allocation clamps/fills it.
    pub(crate) preferred_outer: Dimensioni,
}

/// Opaque ordered owner of unique retained child nodes.
pub struct Children {
    nodes: Vec<Node>,
}

impl Children {
    /// Creates an empty child collection.
    pub const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Returns the number of owned child nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns whether this collection owns no children.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Measures one child by index without exposing the child itself.
    pub fn measure_child(&self, index: usize, style: &crate::Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Option<Dimensioni> {
        self.nodes.get(index).map(|node| node.measure(style, atlas, available).resolved_outer)
    }

    /// Appends one still-unmounted node and commits this collection as its owner.
    pub fn push(&mut self, node: Node) {
        self.nodes.push(node);
    }

    /// Inserts a node at `index`, returning it unchanged when the index exceeds `len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        if index > self.nodes.len() {
            return Err(node);
        }
        self.nodes.insert(index, node);
        Ok(())
    }

    /// Drops the indexed child owner and reports whether one existed.
    pub fn remove_drop(&mut self, index: usize) -> bool {
        if index >= self.nodes.len() {
            return false;
        }
        self.nodes.remove(index);
        true
    }

    /// Drops every currently owned child.
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Replaces all children in iterator order, dropping the previous owners.
    pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>) {
        self.nodes = nodes.into_iter().collect();
    }

    /// Iterates children for framework traversal without making attached nodes public.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Node> {
        self.nodes.iter()
    }

    /// Iterates children mutably for framework traversal only.
    pub(crate) fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Node> {
        self.nodes.iter_mut()
    }

    pub(crate) fn as_slice(&self) -> &[Node] {
        &self.nodes
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [Node] {
        &mut self.nodes
    }
}

impl Default for Children {
    fn default() -> Self {
        Self::new()
    }
}

impl FromIterator<Node> for Children {
    fn from_iter<T: IntoIterator<Item = Node>>(iter: T) -> Self {
        Self { nodes: iter.into_iter().collect() }
    }
}
