//! Opaque ownership of retained child nodes.

use crate::Dimensioni;

use super::Node;

/// Opaque ordered owner of unique retained child nodes.
///
/// Public code can transfer new nodes in or drop existing owners but cannot borrow attached nodes,
/// recover a removed owner, inspect runtime identity, or reparent a child. Framework-created
/// [`crate::ChildrenVisitor`] values provide scoped traversal to custom containers without
/// weakening those ownership rules.
pub struct Children {
    pub(super) nodes: Vec<Node>,
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

    /// Measures one child's preferred content without applying its placement policy.
    ///
    /// Use [`Self::child_policy`] separately when the container's slot calculation needs it.
    pub fn measure_child(&self, index: usize, style: &crate::Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Option<Dimensioni> {
        self.nodes.get(index).map(|node| node.measure(style, atlas, available))
    }

    /// Returns one child's placement policy without exposing the child itself.
    pub fn child_policy(&self, index: usize) -> Option<crate::Policy> {
        self.nodes.get(index).map(|node| node.state.policy)
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

    /// Returns one child for framework-only inspection.
    #[cfg(test)]
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
