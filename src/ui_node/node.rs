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

use crate::math::RectExt;
use crate::{AtlasHandle, Constraints, Dimensioni, FontId, IconId, LeafWidget, Recti, SliceInsets, Style, TypedWidgetHandle, Widget};

use super::{ChildParticipation, Children, Container, NodeLayout, RuntimeNodeId, WidgetStorage};

#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) struct MeasurementStyleKey {
    font: FontId,
    small_font: FontId,
    title_font: FontId,
    heading_font: FontId,
    mono_font: FontId,
    expand_icon: IconId,
    expand_down_icon: IconId,
    check_icon: IconId,
    default_cell_width: i32,
    padding: i32,
    spacing: i32,
    indent: i32,
    title_height: i32,
    scrollbar_size: i32,
    thumb_size: i32,
    /// Normal-state destination insets for every semantic appearance role.
    appearance_insets: [[i32; 4]; crate::AppearanceRole::COUNT],
}

impl MeasurementStyleKey {
    pub(crate) fn new(style: &Style) -> Self {
        // Cache only measurement-observable style values. Colors and cell payloads affect paint,
        // while every role's normalized insets can affect a framed descendant's constraints.
        Self {
            font: style.font,
            small_font: style.small_font,
            title_font: style.title_font,
            heading_font: style.heading_font,
            mono_font: style.mono_font,
            expand_icon: style.icons.expand,
            expand_down_icon: style.icons.expand_down,
            check_icon: style.icons.check,
            default_cell_width: style.default_cell_width,
            padding: style.padding,
            spacing: style.spacing,
            indent: style.indent,
            title_height: style.title_height,
            scrollbar_size: style.scrollbar_size,
            thumb_size: style.thumb_size,
            appearance_insets: style.visuals.measurement_insets(),
        }
    }
}

struct MeasurementEntry {
    constraints: Constraints,
    style: MeasurementStyleKey,
    atlas: AtlasHandle,
    preferred: Dimensioni,
}

/// Bounded preferred-size cache owned directly by one retained node.
///
/// Four retained entries cover the small set of constraints produced by responsive parent flows.
/// Clearing preserves the vector's capacity, so warmed invalidation reuses its storage.
struct MeasurementCache {
    entries: Vec<MeasurementEntry>,
    layout_dirty: bool,
}

impl MeasurementCache {
    fn new() -> Self {
        Self { entries: Vec::new(), layout_dirty: true }
    }

    fn invalidate(&mut self) {
        self.entries.clear();
        self.layout_dirty = true;
    }

    fn invalidate_layout(&mut self) {
        self.layout_dirty = true;
    }

    fn layout_is_dirty(&self) -> bool {
        self.layout_dirty
    }

    fn validate_layout(&mut self) {
        self.layout_dirty = false;
    }

    fn lookup(&self, constraints: Constraints, style: MeasurementStyleKey, atlas: &AtlasHandle) -> Option<Dimensioni> {
        self.entries
            .iter()
            .find(|cached| cached.constraints == constraints && cached.style == style && cached.atlas.ptr_eq(atlas))
            .map(|cached| cached.preferred)
    }

    fn insert(&mut self, entry: MeasurementEntry) {
        if self.entries.len() == 4 {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }
}

/// Reports a typed-handle borrow that escaped into retained traversal.
///
/// Keep the diagnostic path out of the normal phase-dispatch code: successful traversal is the
/// overwhelmingly common case, while a conflict is an application contract violation that always
/// terminates the current operation.
#[cold]
#[inline(never)]
fn widget_borrow_conflict() -> ! {
    panic!("retained widget invariant violated: a typed access closure must finish before runtime traversal")
}

#[cfg(test)]
use super::node_layout::advance_runtime_node_id;
use super::Transform;
#[cfg(test)]
use crate::Vec2i;

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
    /// Parent-layout result consumed uniformly by traversal and dispatch.
    pub(crate) participation: ChildParticipation,
    /// Persistent preferred sizes owned directly by this retained node.
    measurement: MeasurementCache,
    /// Whether this node has received its first authoritative allocation.
    placed: bool,
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

    pub(crate) fn invalidate_measurement(&mut self) {
        self.measurement.invalidate();
    }

    pub(crate) fn invalidate_layout(&mut self) {
        self.measurement.invalidate_layout();
    }

    pub(crate) fn layout_is_dirty(&self) -> bool {
        self.measurement.layout_is_dirty()
    }

    pub(crate) fn validate_layout(&mut self) {
        self.measurement.validate_layout();
        self.placed = true;
    }
}

/// Unique owning retained-tree node.
///
/// A `Node` owns exactly one concrete widget or container runtime. It is intentionally not
/// cloneable: successful insertion transfers ownership into one [`Children`] collection. Its
/// process-unique identity is runtime-private, unrelated to application-facing window handles, and
/// is never stored in or exposed through a [`crate::TypedWidgetHandle`]. Attached nodes cannot be
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
    pub fn widget<W: LeafWidget + 'static>(widget: W) -> Self {
        Self::mount_widget(widget, None).1
    }

    /// Creates a leaf and a weak typed handle to the same retained widget allocation.
    pub fn typed_widget<W: LeafWidget + 'static>(widget: W) -> (TypedWidgetHandle<W>, Self) {
        Self::mount_widget(widget, None)
    }

    /// Creates a framework-internal leaf without returning a typed application handle.
    pub(crate) fn widget_internal<W: LeafWidget + 'static>(widget: W) -> Self {
        Self::widget(widget)
    }

    /// Creates a leaf node with a backend-typed custom-render callback.
    ///
    /// The backend-specific handle is erased only after this checked public boundary; the stored
    /// key remains private and is validated by the renderer registry before use.
    pub fn custom_render<B, W>(widget: W, renderer: CustomRenderHandle<B>) -> Self
    where
        B: RendererBackend,
        W: LeafWidget + 'static,
    {
        Self::mount_widget(widget, Some(renderer.key)).1
    }

    /// Creates a custom-render leaf while preserving weak typed application access.
    pub fn typed_custom_render<B, W>(widget: W, renderer: CustomRenderHandle<B>) -> (TypedWidgetHandle<W>, Self)
    where
        B: RendererBackend,
        W: LeafWidget + 'static,
    {
        Self::mount_widget(widget, Some(renderer.key))
    }

    fn mount_widget<W: LeafWidget + 'static>(widget: W, custom_render: Option<crate::render::CustomRenderKey>) -> (TypedWidgetHandle<W>, Self) {
        // Allocate once while the concrete type is known, then erase only the strong reference
        // retained by the node. Both references therefore address the same RefCell allocation.
        let widget = Rc::new(RefCell::new(WidgetStorage::new(widget)));
        let handle = TypedWidgetHandle::new(&widget);
        let widget: Rc<RefCell<WidgetStorage<dyn LeafWidget>>> = widget;
        let node = Self::from_kind(NodeKind::Widget(WidgetNode::new(widget, custom_render)));
        (handle, node)
    }

    /// Creates a branch node from one complete concrete container owner.
    pub fn container(container: Container) -> Self {
        // Container is already the complete child/layout owner; Node adds only common runtime state.
        Self::from_kind(NodeKind::Container(container))
    }

    /// Installs a cascading style override before this node is mounted.
    ///
    /// On a leaf the override affects only that widget. On a container the style is also inherited
    /// by every descendant until another descendant supplies its own override.
    pub fn with_style_override(mut self, style_override: Style) -> Self {
        self.set_style_override(style_override);
        self
    }

    /// Replaces this unmounted node's cascading style override.
    ///
    /// Once ownership has moved into a retained tree, use the node's [`TypedWidgetHandle`] to
    /// change the override.
    pub fn set_style_override(&mut self, style_override: Style) {
        self.data.set_style_override(Some(style_override));
    }

    /// Clears this unmounted node's override so it inherits its parent style again.
    pub fn clear_style_override(&mut self) {
        self.data.set_style_override(None);
    }

    /// Returns this node's local style override, if one is installed.
    pub fn style_override(&self) -> Option<Style> {
        self.data.style_override()
    }

    /// Resolves this node's local override against the inherited style.
    pub(crate) fn resolve_style(&self, inherited: &Style) -> Style {
        self.data.resolve_style(inherited)
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
                participation: ChildParticipation::Active,
                measurement: MeasurementCache::new(),
                placed: false,
            },
            data: kind,
        }
    }

    /// Returns this node's private identity for runtime-only traversal.
    pub(crate) fn id(&self) -> RuntimeNodeId {
        self.state.id()
    }

    /// Tests retained bounds against an inherited screen-space viewport.
    pub(crate) fn intersects_clip(&self, parent_transform: Transform) -> bool {
        let allocation = self.state.layout.allocation;
        if !self.state.placed || self.data.is_measurement_dirty() {
            // New nodes and directly mutated later subtrees participate before layout commits
            // their resulting geometry.
            return true;
        }
        let content = self.state.layout.content_size;
        let bounds = Recti::new(
            allocation.x,
            allocation.y,
            allocation.width.max(content.width),
            allocation.height.max(content.height),
        );
        parent_transform.resolve(bounds).overlaps(parent_transform.clip)
    }

    /// Measures this node's desired outer size. Its parent assigns the later exact allocation.
    pub(crate) fn measure(&mut self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.measure_with_cache_status(style, atlas, constraints).0
    }

    /// Measures and reports whether the retained result satisfied this exact query.
    pub(crate) fn measure_with_cache_status(&mut self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> (Dimensioni, bool) {
        let style = self.resolve_style(style);
        let style = &style;
        let style_key = MeasurementStyleKey::new(style);
        if let Some(cached) = self.state.measurement.lookup(constraints, style_key, atlas) {
            return (cached, true);
        }

        // Frame geometry is intrinsic to the widget, so remove it from the content bound before
        // dispatch and add it back to the returned content preference afterward.
        // Resolve frame policy and preferred size under one scoped widget borrow. Measurement is a
        // dominant retained-layout path, so reacquiring the same RefCell merely to read options is
        // both redundant and measurably expensive for large leaf trees.
        let (frame_insets, measured_content) = match &mut self.data {
            NodeKind::Widget(node) => {
                let widget = node.widget.try_borrow().unwrap_or_else(|_| widget_borrow_conflict());
                let frame_insets = if widget.widget.effective_widget_opt().intersects(crate::WidgetOption::FRAME) {
                    style
                        .appearance(widget.widget.frame_appearance_role(), crate::VisualState::Normal)
                        .insets
                        .normalized()
                } else {
                    SliceInsets::ZERO
                };
                let measured_content = widget
                    .widget
                    .measure(style, atlas, crate::ui_node::frame::content_constraints(constraints, frame_insets));
                (frame_insets, measured_content)
            }
            NodeKind::Container(container) => container.measure_content_with_frame(style, atlas, constraints),
        };
        // Widgets cannot return negative geometry. Node placement policy is intentionally absent:
        // the parent consumes this desired size while resolving its own child relationship.
        let preferred_content = Dimensioni::new(measured_content.width.max(0), measured_content.height.max(0));
        let preferred = crate::ui_node::frame::outer_preferred(preferred_content, frame_insets);
        self.state.measurement.insert(MeasurementEntry {
            constraints,
            style: style_key,
            atlas: atlas.clone(),
            preferred,
        });
        (preferred, false)
    }

    /// Consumes widget mutation markers and invalidates each changed node's ancestor path.
    pub(crate) fn synchronize_measurement_invalidation(&mut self) -> bool {
        let mut measurement_dirty = self.data.take_measurement_dirty();
        self.with_children_mut(|children| {
            for child in children.iter_mut() {
                measurement_dirty |= child.synchronize_measurement_invalidation();
            }
        });
        if measurement_dirty {
            self.state.invalidate_measurement();
        }
        measurement_dirty
    }

    /// Invalidates preferred-size entries for this complete retained subtree.
    ///
    /// Global style replacement uses this downward traversal because every field of [`Style`] is
    /// observable by the public [`LeafWidget::measure`] contract. A parent-first style change must
    /// therefore discard descendant entries even when the changed value is purely visual to the
    /// built-in widgets or the subtree is currently hidden.
    pub(crate) fn invalidate_measurement_subtree(&mut self) {
        // Clear the local bounded cache before descending. The traversal owns every child mutably,
        // so no parallel generation counter or incomplete projection of Style is needed.
        self.state.invalidate_measurement();
        self.with_children_mut(|children| {
            for child in children.iter_mut() {
                child.invalidate_measurement_subtree();
            }
        });
    }

    /// Reports whether this node or any descendant has uncommitted widget measurement state.
    pub(crate) fn has_measurement_dirty(&self) -> bool {
        self.data.is_measurement_dirty() || self.with_children(|children| children.iter().any(Self::has_measurement_dirty))
    }

    /// Marks this node as the local source of a preferred-size invalidation.
    pub(crate) fn mark_measurement_dirty(&mut self) {
        self.data.mark_measurement_dirty();
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
    pub(crate) widget: Rc<RefCell<WidgetStorage<dyn LeafWidget>>>,
    /// Optional custom backend render callback for custom-render leaves.
    custom_render: Option<CustomRenderKey>,
}

impl WidgetNode {
    /// Erases one concrete runtime at the retained leaf boundary.
    pub(crate) fn new(widget: Rc<RefCell<WidgetStorage<dyn LeafWidget>>>, custom_render: Option<CustomRenderKey>) -> Self {
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
    /// Direct concrete container owner with one erased geometry implementation.
    Container(Container),
}

impl NodeKind {
    pub(crate) fn style_override(&self) -> Option<Style> {
        match self {
            Self::Widget(node) => node.widget.try_borrow().unwrap_or_else(|_| widget_borrow_conflict()).style_override(),
            Self::Container(container) => container.style_override(),
        }
    }

    pub(crate) fn set_style_override(&mut self, style_override: Option<Style>) {
        match self {
            Self::Widget(node) => node
                .widget
                .try_borrow_mut()
                .unwrap_or_else(|_| widget_borrow_conflict())
                .set_style_override(style_override),
            Self::Container(container) => container.set_style_override(style_override),
        }
    }

    pub(crate) fn resolve_style(&self, inherited: &Style) -> Style {
        match self {
            Self::Widget(node) => node.widget.try_borrow().unwrap_or_else(|_| widget_borrow_conflict()).resolve_style(inherited),
            Self::Container(container) => container.resolve_style(inherited),
        }
    }

    /// Runs a read-only operation against the common widget phase object.
    pub(crate) fn with_widget<R>(&self, f: impl FnOnce(&dyn Widget) -> R) -> R {
        match self {
            Self::Widget(node) => {
                let widget = node.widget.try_borrow().unwrap_or_else(|_| widget_borrow_conflict());
                f(&widget.widget)
            }
            Self::Container(container) => container.with_widget(f),
        }
    }

    /// Runs a mutable operation against the common widget phase object.
    pub(crate) fn with_widget_mut<R>(&mut self, f: impl FnOnce(&mut dyn Widget) -> R) -> R {
        match self {
            Self::Widget(node) => {
                let mut widget = node.widget.try_borrow_mut().unwrap_or_else(|_| widget_borrow_conflict());
                f(&mut widget.widget)
            }
            Self::Container(container) => container.with_widget_mut(f),
        }
    }

    pub(crate) fn is_measurement_dirty(&self) -> bool {
        match self {
            Self::Widget(node) => node.widget.try_borrow().unwrap_or_else(|_| widget_borrow_conflict()).is_measurement_dirty(),
            Self::Container(container) => container.is_measurement_dirty(),
        }
    }

    pub(crate) fn take_measurement_dirty(&mut self) -> bool {
        match self {
            Self::Widget(node) => node
                .widget
                .try_borrow_mut()
                .unwrap_or_else(|_| widget_borrow_conflict())
                .take_measurement_dirty(),
            Self::Container(container) => container.take_measurement_dirty(),
        }
    }

    fn mark_measurement_dirty(&mut self) {
        match self {
            Self::Widget(node) => node
                .widget
                .try_borrow_mut()
                .unwrap_or_else(|_| widget_borrow_conflict())
                .mark_measurement_dirty(),
            Self::Container(container) => container.mark_measurement_dirty(),
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

        let mut children = Children::new();
        let rejected = children.insert(1, first).expect_err("out-of-range insertion must reject the exact owner");
        assert_eq!(rejected.state.id.0.get(), first_id);
        assert!(children.is_empty());

        children.push(rejected);
        assert_eq!(children.nodes[0].state.id.0.get(), first_id);
    }

    #[test]
    fn measurement_reports_content_independently_of_exact_parent_allocation() {
        let (_, mut node) = text_node("same content");
        let atlas = test_atlas();
        // Resolve the measurement style from the exact handle passed to both measurement and
        // layout so same-slot resources from another atlas can never satisfy this regression.
        let style = crate::test_support::test_style(&atlas);

        let preferred = node.measure(&style, &atlas, Constraints::unbounded());
        let mut runtime = crate::ui_node::UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut node, &style, atlas.clone(), Recti::new(0, 0, 300, 200), crate::UNCLIPPED_RECT);
        let measured_again = node.measure(&style, &atlas, Constraints::unbounded());

        assert_eq!((measured_again.width, measured_again.height), (preferred.width, preferred.height));
        let allocation = node.state.layout.allocation;
        assert_eq!((allocation.x, allocation.y, allocation.width, allocation.height), (0, 0, 300, 200));
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
    #[allow(clippy::result_large_err)] // The test verifies that failed mutation returns the exact owner.
    fn ownership_moving_widget_access_returns_the_same_unmounted_node_on_failure() {
        let (column_state, column_node) = crate::Linear::create(crate::LinearParameters::vertical(std::iter::empty::<Node>()));
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
