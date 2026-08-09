//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

use crate::render::{CustomRenderHandle, CustomRenderKey, RendererBackend};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{Dimensioni, TypedWidgetHandle, Widget};

use super::{ChildParticipation, Children, Container, NodeLayout, RuntimeNodeId};

#[cfg(test)]
use super::node_layout::advance_runtime_node_id;
#[cfg(test)]
use super::Transform;
#[cfg(test)]
use crate::{Recti, Vec2i};

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
    /// Mouse was pressed on this node during the current input transaction.
    pub(crate) clicked: bool,
    /// Mouse is held down while this node owns focus.
    pub(crate) active: bool,
    /// Placement policy used by runtime layout passes.
    pub(crate) policy: crate::Policy,
    /// Parent-layout result consumed uniformly by traversal and dispatch.
    pub(crate) participation: ChildParticipation,
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
}

/// Unique owning retained-tree node.
///
/// A `Node` owns exactly one concrete widget or container runtime. It is intentionally not
/// cloneable: successful insertion transfers ownership into one [`Children`] collection. Its
/// process-unique identity is runtime-private, unrelated to public [`crate::RootId`] values, and is
/// never stored in or exposed through a [`crate::TypedWidgetHandle`]. Attached nodes cannot be
/// detached or reparented: topology APIs either keep ownership in place or drop the removed
/// runtime. Build a replacement node when content must move to another parent.
pub struct Node {
    /// Common state for layout, identity, and interaction.
    pub(crate) state: NodeRuntime,
    /// Node-specific payload.
    pub(crate) data: NodeKind,
}

impl Node {
    /// Creates a leaf node without retaining a typed application handle.
    pub fn widget<W: Widget + 'static>(widget: W) -> Self {
        Self::mount_widget(widget, None).1
    }

    /// Creates a leaf and a weak typed handle to the same retained widget allocation.
    pub fn typed_widget<W: Widget + 'static>(widget: W) -> (TypedWidgetHandle<W>, Self) {
        Self::mount_widget(widget, None)
    }

    /// Creates a framework-internal leaf without returning a typed application handle.
    pub(crate) fn widget_internal<W: Widget + 'static>(widget: W) -> Self {
        Self::widget(widget)
    }

    /// Creates a leaf node with a backend-typed custom-render callback.
    ///
    /// The backend-specific handle is erased only after this checked public boundary; the stored
    /// key remains private and is validated by the renderer registry before use.
    pub fn custom_render<B, W>(widget: W, renderer: CustomRenderHandle<B>) -> Self
    where
        B: RendererBackend,
        W: Widget + 'static,
    {
        Self::mount_widget(widget, Some(renderer.key)).1
    }

    /// Creates a custom-render leaf while preserving weak typed application access.
    pub fn typed_custom_render<B, W>(widget: W, renderer: CustomRenderHandle<B>) -> (TypedWidgetHandle<W>, Self)
    where
        B: RendererBackend,
        W: Widget + 'static,
    {
        Self::mount_widget(widget, Some(renderer.key))
    }

    fn mount_widget<W: Widget + 'static>(widget: W, custom_render: Option<crate::render::CustomRenderKey>) -> (TypedWidgetHandle<W>, Self) {
        // Allocate once while the concrete type is known, then erase only the strong reference
        // retained by the node. Both references therefore address the same RefCell allocation.
        let widget = Rc::new(RefCell::new(widget));
        let handle = TypedWidgetHandle::new(&widget);
        let widget: Rc<RefCell<dyn Widget>> = widget;
        let node = Self::from_kind(NodeKind::Widget(WidgetNode::new(widget, custom_render)));
        (handle, node)
    }

    /// Creates a branch node from one complete concrete container owner.
    pub fn container(container: Container) -> Self {
        // Container is already the complete child/layout owner; Node adds only common runtime state.
        Self::from_kind(NodeKind::Container(container))
    }

    /// Replaces this still-unmounted node's generic parent placement policy.
    ///
    /// Grid spans are separate parent-owned metadata supplied by [`crate::GridItem`] and take
    /// precedence for cell occupancy; this policy still controls sizing within the assigned area.
    pub fn with_policy(mut self, policy: crate::Policy) -> Self {
        self.state.policy = policy;
        self
    }

    fn from_kind(kind: NodeKind) -> Self {
        // Identity is allocated once at the final owning boundary and survives every subsequent move
        // of the non-Clone Node through unmounted construction and retained insertion.
        Self {
            state: NodeRuntime {
                id: RuntimeNodeId::allocate(),
                layout: NodeLayout::default(),
                hovered: false,
                focused: false,
                clicked: false,
                active: false,
                policy: crate::Policy::auto(),
                participation: ChildParticipation::Active,
            },
            data: kind,
        }
    }

    /// Returns this node's private identity for runtime-only traversal.
    pub(crate) fn id(&self) -> RuntimeNodeId {
        self.state.id()
    }

    /// Measures this node's preferred outer size. Placement policy is applied later by layout.
    pub(crate) fn measure(&self, style: &crate::Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        // Frame geometry is intrinsic to the widget, so remove it from the content bound before
        // dispatch and add it back to the returned content preference afterward.
        let framed = self
            .data
            .with_widget(|widget| widget.effective_widget_opt().intersects(crate::WidgetOption::FRAME));
        let border_width = if framed { style.frame_border().width.max(0) } else { 0 };
        // Both leaves and containers expose one Widget measurement entry point through NodeKind.
        let measured_content = self
            .data
            .with_widget(|widget| widget.measure(style, atlas, crate::ui_node::frame::content_available(available, border_width)));
        // Widgets cannot return negative geometry. Node placement policy is intentionally absent:
        // the parent applies it later when allocating this preferred outer size.
        let preferred_content = Dimensioni::new(measured_content.width.max(0), measured_content.height.max(0));
        crate::ui_node::frame::outer_preferred(preferred_content, border_width)
    }

    /// Writes layout as the source of truth.
    pub(crate) fn set_layout(&mut self, layout: NodeLayout) {
        self.state.set_layout(layout);
    }

    /// Runs `f` with this node's opaque authoritative child collection.
    pub(crate) fn with_children<R>(&self, f: impl FnOnce(&Children) -> R) -> R {
        // Treat a leaf as an empty collection for read-only generic traversal. This avoids a second
        // visitor abstraction while preserving the fact that only Container stores descendants.
        match &self.data {
            NodeKind::Widget(_) => f(&Children::new()),
            NodeKind::Container(container) => container.with_children(f),
        }
    }

    /// Runs `f` with this node's authoritative child collection when it is a container.
    pub(crate) fn with_children_mut<R>(&mut self, f: impl FnOnce(&mut Children) -> R) -> Option<R> {
        // Mutable traversal distinguishes leaves explicitly because there is no persistent empty
        // collection that could safely accept topology changes.
        match &mut self.data {
            NodeKind::Widget(_) => None,
            NodeKind::Container(container) => Some(container.with_children_mut(f)),
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

    /// Runs a test-only immutable visitor against one matching retained node.
    #[cfg(test)]
    pub(crate) fn with_node<R>(&self, id: RuntimeNodeId, f: impl FnOnce(&Node) -> R) -> Option<R> {
        // Keep the one-shot visitor in an Option so recursive descent can move it exactly once when
        // the requested identity is reached without returning a borrow from opaque child storage.
        let mut f = Some(f);
        self.with_node_inner(id, &mut f)
    }

    #[cfg(test)]
    fn with_node_inner<R, F>(&self, id: RuntimeNodeId, f: &mut Option<F>) -> Option<R>
    where
        F: FnOnce(&Node) -> R,
    {
        if self.id() == id {
            // Runtime IDs are unique, so taking the callback here proves it cannot run twice.
            return Some(f.take().expect("node visitor invoked twice")(self));
        }
        // The child borrow remains scoped to this search and cannot escape through the callback.
        self.with_children(|children| children.iter().find_map(|child| child.with_node_inner(id, f)))
    }
}

/// Thin retained leaf owner for one erased widget and optional custom-render metadata.
pub(crate) struct WidgetNode {
    /// Sole persistent strong widget owner, erased without changing its allocation.
    pub(crate) widget: Rc<RefCell<dyn Widget>>,
    /// Optional custom backend render callback for custom-render leaves.
    custom_render: Option<CustomRenderKey>,
}

impl WidgetNode {
    /// Erases one concrete runtime at the retained leaf boundary.
    pub(crate) fn new(widget: Rc<RefCell<dyn Widget>>, custom_render: Option<CustomRenderKey>) -> Self {
        Self { widget, custom_render }
    }

    /// Returns the private custom-render callback key, when one was supplied at construction.
    pub(crate) fn custom_render(&self) -> Option<CustomRenderKey> {
        self.custom_render
    }
}

/// Private runtime payload for an owning [`Node`].
pub(crate) enum NodeKind {
    /// Direct erased leaf widget runtime.
    Widget(WidgetNode),
    /// Direct concrete container owner with one erased geometry policy.
    Container(Container),
}

impl NodeKind {
    /// Runs a read-only operation against the common widget phase object.
    pub(crate) fn with_widget<R>(&self, f: impl FnOnce(&dyn Widget) -> R) -> R {
        match self {
            Self::Widget(node) => {
                let widget = node
                    .widget
                    .try_borrow()
                    .expect("retained widget invariant violated: a typed access closure must finish before runtime traversal");
                f(&*widget)
            }
            Self::Container(container) => f(container),
        }
    }

    /// Runs a mutable operation against the common widget phase object.
    pub(crate) fn with_widget_mut<R>(&mut self, f: impl FnOnce(&mut dyn Widget) -> R) -> R {
        match self {
            Self::Widget(node) => {
                let mut widget = node
                    .widget
                    .try_borrow_mut()
                    .expect("retained widget invariant violated: a typed access closure must finish before runtime traversal");
                f(&mut *widget)
            }
            Self::Container(container) => f(container),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;

    fn text_node(label: &str) -> (crate::TypedWidgetHandle<crate::TextBlock>, Node) {
        crate::TextBlock::create(crate::TextBlockParameters::new(label))
    }

    #[test]
    fn pushing_a_node_uses_node_local_child_geometry() {
        let mut layout = NodeLayout::from_parts(Recti::new(10, 20, 30, 40), Recti::new(2, 3, 20, 10), Dimensioni::new(30, 40));
        layout.children.offset = Vec2i::new(4, -5);

        let parent = Transform {
            offset: Vec2i::new(100, 200),
            clip: Recti::new(0, 0, 1000, 1000),
        };
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
    fn measurement_reports_content_and_leaves_placement_policy_to_the_parent() {
        let (_, plain) = text_node("same content");
        let (_, fixed) = text_node("same content");
        let fixed = fixed.with_policy(crate::Policy::fixed(300, 200));
        let style = crate::Style::default();
        let atlas = test_atlas();

        let plain = plain.measure(&style, &atlas, Dimensioni::default());
        let fixed_measurement = fixed.measure(&style, &atlas, Dimensioni::default());
        assert_eq!((plain.width, plain.height), (fixed_measurement.width, fixed_measurement.height));
        let children: Children = [fixed].into_iter().collect();
        assert_eq!(children.child_policy(0), Some(crate::Policy::fixed(300, 200)));
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
            .try_read(|_| match column_state.try_update_with(candidate, |column, node| column.push(node)) {
                Err(candidate) => candidate,
                Ok(_) => panic!("the active read must prevent mutation"),
            })
            .expect("column read must be available");
        assert_eq!(rejected.state.id.0.get(), candidate_id);

        drop(column_node);
        let rejected = match column_state.try_update_with(rejected, |column, node| column.push(node)) {
            Err(rejected) => rejected,
            Ok(_) => panic!("expired state must return the input owner"),
        };
        assert_eq!(rejected.state.id.0.get(), candidate_id);
    }
}
