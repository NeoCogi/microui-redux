//! Opaque ownership of retained child nodes.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::Dimensioni;

use super::Node;

/// Opaque ordered owner of unique retained child nodes.
///
/// Public code can transfer new nodes in or drop existing owners but cannot borrow attached nodes,
/// recover a removed owner, inspect runtime identity, or reparent a child. Framework-created
/// [`crate::ContainerLayoutCtx`] provides scoped geometry operations without weakening those
/// ownership rules.
pub struct Children {
    pub(super) nodes: Vec<Node>,
}

/// Crate-private weak access used by built-in mutable container state.
#[derive(Clone)]
pub(crate) struct ChildrenHandle {
    cell: Weak<RefCell<Children>>,
}

impl ChildrenHandle {
    /// Creates a non-owning topology capability for `children`.
    ///
    /// The downgraded reference is intentional: application-facing container state may outlive a
    /// particular access closure, but it must never keep a removed container or its descendants
    /// alive. Every operation therefore upgrades and borrows the collection afresh.
    pub(crate) fn new(children: &Rc<RefCell<Children>>) -> Self {
        // Store only a Weak reference so the Container remains the collection's lifetime owner.
        Self { cell: Rc::downgrade(children) }
    }

    /// Runs a crate-internal atomic metadata/topology update while preserving `input` on failure.
    pub(crate) fn try_update_with<I, R>(&self, input: I, f: impl FnOnce(&mut Children, I) -> R) -> Result<R, I> {
        // Grid uses this scoped operation to update child ownership and its index-matched spans
        // under one state closure. The collection borrow never escapes into public application code.
        let Some(owner) = self.cell.upgrade() else {
            return Err(input);
        };
        let Ok(mut children) = owner.try_borrow_mut() else {
            return Err(input);
        };
        Ok(f(&mut children, input))
    }

    /// Reports the current child count, or `None` when storage is unavailable.
    pub(crate) fn len(&self) -> Option<usize> {
        // Read access is checked because update and paint traversal may hold a mutable collection
        // borrow while visiting descendants.
        let owner = self.cell.upgrade()?;
        let children = owner.try_borrow().ok()?;
        Some(children.len())
    }

    /// Reports whether the collection is empty, or `None` when storage is unavailable.
    pub(crate) fn is_empty(&self) -> Option<bool> {
        // Reuse `len` so liveness and borrow-conflict behavior has one implementation.
        self.len().map(|len| len == 0)
    }

    /// Appends an unmounted node or returns it unchanged when mutation is unavailable.
    pub(crate) fn try_push(&self, node: Node) -> Result<(), Node> {
        // Upgrade and borrow before consuming the node so failure preserves its unique ownership.
        let Some(owner) = self.cell.upgrade() else {
            return Err(node);
        };
        let Ok(mut children) = owner.try_borrow_mut() else {
            return Err(node);
        };
        children.push(node);
        Ok(())
    }

    /// Inserts an unmounted node or returns it unchanged on conflict, expiry, or invalid index.
    pub(crate) fn try_insert(&self, index: usize, node: Node) -> Result<(), Node> {
        // Both capability failure and an out-of-range index have the same lossless recovery value,
        // allowing callers to retry without manufacturing another node.
        let Some(owner) = self.cell.upgrade() else {
            return Err(node);
        };
        let Ok(mut children) = owner.try_borrow_mut() else {
            return Err(node);
        };
        children.insert(index, node)
    }

    /// Drops one indexed owner, returning `None` when mutation is unavailable.
    pub(crate) fn try_remove_drop(&self, index: usize) -> Option<bool> {
        // The boolean distinguishes a missing index from capability failure represented by `None`.
        let owner = self.cell.upgrade()?;
        let mut children = owner.try_borrow_mut().ok()?;
        // Removal is deliberately destructive: attached nodes are never returned for reparenting.
        Some(children.remove_drop(index))
    }

    /// Drops every child, returning `None` when mutation is unavailable.
    pub(crate) fn try_clear(&self) -> Option<()> {
        // Clearing keeps the collection allocation and weak-handle identity stable.
        let owner = self.cell.upgrade()?;
        let mut children = owner.try_borrow_mut().ok()?;
        children.clear();
        Some(())
    }

    /// Replaces every child or returns the unconsumed iterator when mutation is unavailable.
    pub(crate) fn try_replace<I>(&self, nodes: I) -> Result<(), I>
    where
        I: IntoIterator<Item = Node>,
    {
        // Acquire the collection before advancing `nodes`; this preserves every unique node on
        // failure even when the caller supplied a lazy iterator.
        let Some(owner) = self.cell.upgrade() else {
            return Err(nodes);
        };
        let Ok(mut children) = owner.try_borrow_mut() else {
            return Err(nodes);
        };
        children.replace(nodes);
        Ok(())
    }
}

impl Children {
    /// Creates an empty child collection.
    pub const fn new() -> Self {
        // No backing allocation is created until the first node is inserted.
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

    /// Measures one child's preferred content without applying its placement policy.
    ///
    /// Use [`Self::child_policy`] separately when the container's slot calculation needs it.
    pub fn measure_child(&self, index: usize, style: &crate::Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Option<Dimensioni> {
        // Resolve the index internally so callers can inspect geometry without borrowing a Node.
        self.nodes.get(index).map(|node| node.measure(style, atlas, available))
    }

    /// Returns one child's placement policy without exposing the child itself.
    pub fn child_policy(&self, index: usize) -> Option<crate::Policy> {
        // Copy only the parent-owned policy; runtime identity and widget state remain opaque.
        self.nodes.get(index).map(|node| node.state.policy)
    }

    /// Appends one still-unmounted node and commits this collection as its owner.
    pub(crate) fn push(&mut self, node: Node) {
        // Moving the unique Node into the vector establishes this collection as its owner.
        self.nodes.push(node);
    }

    /// Inserts a node at `index`, returning it unchanged when the index exceeds `len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub(crate) fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        // Validate before consuming `node`, preserving the exact owner when the index is invalid.
        if index > self.nodes.len() {
            return Err(node);
        }
        self.nodes.insert(index, node);
        Ok(())
    }

    /// Drops the indexed child owner and reports whether one existed.
    pub(crate) fn remove_drop(&mut self, index: usize) -> bool {
        // Do not manufacture a detached-node path: a successful removal drops the owner in place.
        if index >= self.nodes.len() {
            return false;
        }
        self.nodes.remove(index);
        true
    }

    /// Drops every currently owned child.
    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Replaces all children in iterator order, dropping the previous owners.
    pub(crate) fn replace(&mut self, nodes: impl IntoIterator<Item = Node>) {
        // Collect the replacement sequence once, then drop the previous vector and its subtrees.
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

    /// Returns one child for framework-only derived-layout inspection.
    pub(crate) fn get(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }

    /// Returns one child for framework-only layout/traversal.
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut Node> {
        self.nodes.get_mut(index)
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

#[cfg(test)]
mod handle_tests {
    use super::*;

    /// Builds a small node without depending on container construction during owner tests.
    fn text_node(label: &str) -> Node {
        // TextBlock construction also gives the test a real state-owning widget runtime, ensuring
        // these checks exercise the same drop path used by application nodes.
        let (_, widget) = crate::TextBlock::create(crate::TextBlockParameters::new(label));
        Node::widget(widget)
    }

    #[test]
    fn weak_handle_never_keeps_the_collection_alive() {
        let children = Rc::new(RefCell::new([text_node("child")].into_iter().collect()));
        let handle = ChildrenHandle::new(&children);

        assert_eq!(handle.len(), Some(1));
        drop(children);
        assert_eq!(handle.len(), None);
    }

    #[test]
    fn failed_mutation_preserves_the_exact_node_owner() {
        let children = Rc::new(RefCell::new(Children::new()));
        let handle = ChildrenHandle::new(&children);
        let candidate = text_node("candidate");
        let candidate_id = candidate.id();

        let rejected = {
            let _borrow = children.borrow_mut();
            handle.try_push(candidate).expect_err("an active traversal borrow must reject mounted mutation")
        };

        assert_eq!(rejected.id(), candidate_id);
        assert_eq!(handle.len(), Some(0));
    }
}
