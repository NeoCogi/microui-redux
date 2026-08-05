//! Runtime node identity, derived layout, and traversal transforms.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Dimensioni, Recti, Vec2i};

/// Layout-authored participation of one retained child and its subtree.
///
/// This value deliberately separates responsive placement from application widget options. A
/// layout may retain a child while removing it from rendering and dispatch, or keep it visible
/// while preventing activation. The dispatcher remains the sole consumer that turns this data
/// into event eligibility.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ChildParticipation {
    /// The child is updated, painted, and eligible for dispatcher targeting.
    #[default]
    Active,
    /// The child remains visible and updated, but its complete subtree rejects input and focus.
    Disabled,
    /// The child remains owned but is excluded from update, paint, input, and focus traversal.
    Hidden,
}

impl ChildParticipation {
    /// Returns whether ordinary update and paint traversal should visit the child.
    pub(crate) const fn is_visible(self) -> bool {
        !matches!(self, Self::Hidden)
    }

    /// Returns whether the dispatcher may target the child or anything below it.
    pub(crate) const fn accepts_input(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Process-wide source of runtime-only node identity.
///
/// Relaxed ordering is sufficient: the counter establishes uniqueness and does not publish any
/// node memory or synchronize traversal.
static NEXT_RUNTIME_NODE_ID: AtomicU64 = AtomicU64::new(1);

pub(super) const fn advance_runtime_node_id(current: u64) -> Option<u64> {
    current.checked_add(1)
}

/// Runtime-private identity assigned exactly once when an owning [`crate::Node`] is created.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RuntimeNodeId(pub(super) NonZeroU64);

impl RuntimeNodeId {
    pub(super) fn allocate() -> Self {
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
