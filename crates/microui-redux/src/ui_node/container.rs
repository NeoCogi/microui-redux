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

use std::{cell::RefCell, rc::Rc};

use crate::math::RectExt;
use crate::{Constraints, Dimensioni, Recti, SliceInsets, Skin, TypedWidgetHandle, UiInputEvent, Vec2i, Widget, WidgetOption};

use super::{ChildParticipation, Children, NodeLayout, NodeRuntime, UiRuntime, WidgetStorage};

/// Scoped services for measuring one retained container's children.
///
/// The context owns the mutable child access required by node-local measurement caches but exposes
/// only derived geometry. Neither a [`Node`](crate::Node) nor the child collection crosses the
/// public container-widget boundary.
pub struct MeasureCtx<'a> {
    /// Complete context skin used by the actively measured container.
    skin: &'a Skin,
    /// Immutable atlas paired with `skin` for concrete font and icon resolution.
    atlas: &'a crate::AtlasHandle,
    /// Authoritative direct children available only for this scoped measure call.
    children: &'a mut Children,
}

impl<'a> MeasureCtx<'a> {
    /// Creates one runtime-scoped measurement context.
    pub(crate) fn new(skin: &'a Skin, atlas: &'a crate::AtlasHandle, children: &'a mut Children) -> Self {
        // Store the complete resolved skin and matching atlas borrowed for this measurement scope.
        Self { skin, atlas, children }
    }

    /// Returns the active UI skin.
    pub fn skin(&self) -> &Skin {
        self.skin
    }

    /// Returns the active atlas.
    pub fn atlas(&self) -> &crate::AtlasHandle {
        self.atlas
    }

    /// Returns the number of direct retained children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Measures one indexed child's desired size under explicit constraints.
    pub fn measure_child(&mut self, index: usize, constraints: Constraints) -> Option<Dimensioni> {
        // Reborrow exactly one node for the recursive call; only copied geometry leaves this scope.
        let node = self.children.get_mut(index)?;
        Some(node.measure(self.skin, self.atlas, constraints))
    }
}

/// Complete behavior of one retained branch widget.
///
/// The concrete widget owns semantic, interaction, and layout behavior while [`Container`] owns the
/// heterogeneous child collection separately. Runtime calls never retain the widget borrow while
/// recursively visiting descendants.
pub trait ContainerWidget: Widget {
    /// Measures preferred content from the authoritative child collection.
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni;

    /// Places retained children and commits descendant viewport/content geometry.
    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti);

    /// Returns whether this container accepts an already-selected event.
    ///
    /// Returning `false` permits ancestor bubbling but never exposes a covered sibling.
    fn accepts_event(&self, _event: &UiInputEvent) -> bool {
        true
    }
}

/// Retained owner for one erased typed branch widget and an opaque ordered child collection.
///
/// This is the only strong owner of its direct [`Node`](crate::Node) values. The concrete widget is
/// independently allocated so runtime phases can scope widget and child access to one operation
/// without exposing either retained allocation through the public container API.
pub struct Container {
    /// One authoritative shared cell for mounted descendants.
    children: Rc<RefCell<Children>>,
    /// Sole persistent strong owner of the concrete typed container widget.
    widget: Rc<RefCell<WidgetStorage<dyn ContainerWidget>>>,
}

impl Container {
    /// Creates an unmounted typed branch widget and its weak concrete handle.
    pub fn new<W>(widget: W, children: impl IntoIterator<Item = super::Node>) -> (TypedWidgetHandle<W>, Self)
    where
        W: ContainerWidget + 'static,
    {
        Self::from_shared(Rc::new(RefCell::new(children.into_iter().collect())), widget)
    }

    /// Creates a typed branch around child storage prepared by a built-in constructor.
    pub(crate) fn from_shared<W>(children: Rc<RefCell<Children>>, widget: W) -> (TypedWidgetHandle<W>, Self)
    where
        W: ContainerWidget + 'static,
    {
        let widget = Rc::new(RefCell::new(WidgetStorage::new(widget)));
        Self::from_shared_owner(children, widget)
    }

    /// Adopts a concrete widget allocation prepared for internal weak composition links.
    pub(crate) fn from_shared_owner<W>(children: Rc<RefCell<Children>>, widget: Rc<RefCell<WidgetStorage<W>>>) -> (TypedWidgetHandle<W>, Self)
    where
        W: ContainerWidget + 'static,
    {
        let handle = TypedWidgetHandle::new(&widget);
        let widget: Rc<RefCell<WidgetStorage<dyn ContainerWidget>>> = widget;
        (handle, Self { children, widget })
    }

    /// Returns whether the installed surface supports one dispatcher-selected event.
    pub(crate) fn accepts_event(&self, event: &UiInputEvent) -> bool {
        // A geometry-only container has no event-receiving surface. An installed surface owns only
        // its additional state-dependent filter; generic option policy remains in the dispatcher.
        self.widget
            .try_borrow()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
            .widget
            .accepts_event(event)
    }

    /// Runs one immutable framework traversal without exposing child storage publicly.
    pub(crate) fn with_children<R>(&self, f: impl FnOnce(&Children) -> R) -> R {
        // A failed borrow is a phase-ordering bug, not a recoverable absence of children. Panicking
        // here reports the invariant violation at the ownership boundary instead of deeper in
        // recursive traversal.
        let children = self
            .children
            .try_borrow()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is mutably borrowed during immutable traversal"));
        f(&children)
    }

    /// Runs one mutable framework traversal while preventing concurrent topology mutation.
    pub(crate) fn with_children_mut<R>(&self, f: impl FnOnce(&mut Children) -> R) -> R {
        // Hold the borrow for the complete callback so topology cannot change while a recursive
        // runtime pass is using node references from this collection.
        let mut children = self
            .children
            .try_borrow_mut()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is already borrowed during mutable traversal"));
        f(&mut children)
    }

    /// Invokes the geometry implementation for one placement pass.
    pub(crate) fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        // The widget receives the authoritative collection only for this call. A typed mutation
        // attempted against the same container while this borrow is active is rejected cleanly by
        // ChildrenHandle rather than invalidating indices during placement.
        let mut children = self
            .children
            .try_borrow_mut()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is already borrowed during layout"));
        self.widget
            .try_borrow_mut()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
            .widget
            .place(ctx, &mut children, rect);
    }

    /// Resolves frame insets and measures content under one typed-runtime borrow.
    pub(crate) fn measure_content_with_frame(&mut self, style: &Skin, atlas: &crate::AtlasHandle, constraints: Constraints) -> (SliceInsets, Dimensioni) {
        let mut children = self
            .children
            .try_borrow_mut()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is already borrowed during measurement"));
        let widget = self.widget.try_borrow().unwrap_or_else(|_| typed_container_borrow_conflict());
        let frame_insets = if widget.widget.effective_widget_opt().intersects(WidgetOption::FRAME) {
            super::frame::normal_frame_visual(style, widget.widget.frame_appearance_role())
                .patch
                .insets
                .normalized()
        } else {
            SliceInsets::ZERO
        };
        let mut ctx = MeasureCtx::new(style, atlas, &mut children);
        let measured = ContainerWidget::measure(&widget.widget, &mut ctx, super::frame::content_constraints(constraints, frame_insets));
        (frame_insets, measured)
    }

    /// Returns the concrete widget's effective options.
    pub(crate) fn effective_widget_opt(&self) -> WidgetOption {
        self.widget
            .try_borrow()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
            .widget
            .effective_widget_opt()
    }

    /// Runs one common widget query against the concrete container object.
    pub(crate) fn with_widget<R>(&self, f: impl FnOnce(&dyn Widget) -> R) -> R {
        let widget = self.widget.try_borrow().unwrap_or_else(|_| typed_container_borrow_conflict());
        f(&widget.widget)
    }

    /// Runs one common mutable widget phase against the concrete container object.
    pub(crate) fn with_widget_mut<R>(&mut self, f: impl FnOnce(&mut dyn Widget) -> R) -> R {
        let mut widget = self.widget.try_borrow_mut().unwrap_or_else(|_| typed_container_borrow_conflict());
        f(&mut widget.widget)
    }

    pub(crate) fn is_measurement_dirty(&self) -> bool {
        self.widget
            .try_borrow()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
            .is_measurement_dirty()
    }

    pub(crate) fn take_measurement_dirty(&mut self) -> bool {
        self.widget
            .try_borrow_mut()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
            .take_measurement_dirty()
    }

    pub(crate) fn mark_measurement_dirty(&mut self) {
        self.widget
            .try_borrow_mut()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
            .mark_measurement_dirty();
    }
}

/// Reports application access that overlaps a typed container runtime phase.
#[cold]
#[inline(never)]
fn typed_container_borrow_conflict() -> ! {
    panic!("retained widget invariant violated: a typed access closure must finish before runtime traversal")
}

/// Framework-scoped geometry services available to one active [`ContainerWidget::place`] call.
///
/// The context measures or places indexed children and commits derived viewport/participation
/// results. It never lends a node or permits topology mutation.
pub struct ContainerLayoutCtx<'a> {
    /// Runtime recursion services used to measure and place selected direct children.
    runtime: &'a mut UiRuntime,
    /// Complete context skin used by this active placement call.
    skin: &'a Skin,
    /// Immutable atlas paired with `skin` for child measurement and placement.
    atlas: &'a crate::AtlasHandle,
    /// Exact node-local content rectangle assigned to the container.
    content: Recti,
    /// Common runtime state on which this container publishes derived geometry.
    current: &'a mut NodeRuntime,
}

impl ContainerLayoutCtx<'_> {
    /// Creates one runtime-scoped placement context.
    pub(crate) fn new<'a>(
        runtime: &'a mut UiRuntime,
        skin: &'a Skin,
        atlas: &'a crate::AtlasHandle,
        content: Recti,
        current: &'a mut NodeRuntime,
    ) -> ContainerLayoutCtx<'a> {
        // All references share one placement lifetime, preventing the context from escaping the
        // runtime call that owns the mutable current-node state.
        ContainerLayoutCtx { runtime, skin, atlas, content, current }
    }

    /// Returns the active UI skin.
    pub fn skin(&self) -> &Skin {
        self.skin
    }

    /// Returns the active atlas.
    pub fn atlas(&self) -> &crate::AtlasHandle {
        self.atlas
    }

    /// Measures one indexed child under `available` during placement.
    pub fn measure_child(&mut self, children: &mut Children, index: usize, constraints: Constraints) -> Option<Dimensioni> {
        // Placement participates in the same runtime epoch as the measure phase immediately before
        // it, so identical child queries reuse the node-local preferred-size result.
        let node = children.get_mut(index)?;
        Some(self.runtime.measure_node(node, self.skin, self.atlas, constraints))
    }

    /// Assigns one exact indexed child rectangle and returns its allocated size.
    ///
    /// A container must resolve every content, fixed, or flexible relationship before this call.
    /// Runtime does not reinterpret the rectangle or inspect the concrete parent type.
    pub fn layout_child(&mut self, children: &mut Children, index: usize, rect: Recti) -> Option<Dimensioni> {
        // Resolve the child internally, recurse immediately, and return only copied geometry.
        let node = children.get_mut(index)?;
        Some(self.runtime.layout_node_ref(node, self.skin, self.atlas, rect))
    }

    /// Reads one child's content extent from its most recent placement in this pass.
    pub fn child_content_size(&self, children: &Children, index: usize) -> Option<Dimensioni> {
        // Content size is a copy of committed layout output, never a borrow of child runtime state.
        children.get(index).map(|node| node.state.layout.content_size)
    }

    /// Commits whether one retained child participates after this layout pass.
    pub fn set_child_participation(&mut self, children: &mut Children, index: usize, participation: ChildParticipation) -> bool {
        // Participation is parent-authored derived state and remains attached to the child between
        // passes so update, paint, hit testing, and target sanitation observe the same decision.
        let Some(node) = children.get_mut(index) else { return false };
        node.state.participation = participation;
        true
    }

    /// Replaces the current container's derived logical content size.
    pub fn set_content_size(&mut self, size: Dimensioni) {
        // Preserve allocation and viewport while replacing only the logical overflow extent.
        self.current.set_layout(self.current.layout.with_content_size(size));
    }

    /// Installs the current container's descendant viewport and translation.
    pub fn set_children_viewport(&mut self, viewport: Recti, offset: Vec2i) {
        // A layout may narrow its descendant viewport but cannot reveal pixels outside its framed
        // content rectangle. Disjoint input is represented by a stable empty rectangle at content
        // origin rather than by an invalid rectangle.
        let viewport = viewport
            .positive_intersection(self.content)
            .unwrap_or_else(|| Recti::new(self.content.x, self.content.y, 0, 0));
        // Rebuild the child transform while retaining the current logical content size.
        let outer = self.current.layout.allocation;
        let mut layout = NodeLayout::from_parts(outer, viewport, self.current.layout.content_size);
        layout.children.offset = offset;
        self.current.set_layout(layout);
    }

    /// Controls whether descendant overflow contributes to the parent-visible extent.
    pub fn set_child_overflow_propagation(&mut self, propagate: bool) {
        // This flag affects only the parent's derived content bounds; it does not clip children.
        self.current.set_layout(self.current.layout.with_child_overflow_propagation(propagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal layout implementation proving that concrete ownership needs geometry only.
    struct GeometryOnly;

    impl Widget for GeometryOnly {
        fn widget_opt(&self) -> &WidgetOption {
            &WidgetOption::NO_INTERACT
        }

        fn update(&mut self, _ctx: &mut crate::WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

        fn paint(&mut self, _ctx: &mut crate::WidgetPaintCtx<'_>) {}
    }

    impl ContainerWidget for GeometryOnly {
        fn measure(&self, ctx: &mut MeasureCtx<'_>, _constraints: Constraints) -> Dimensioni {
            Dimensioni::new(ctx.child_count() as i32, 1)
        }

        fn place(&mut self, _ctx: &mut ContainerLayoutCtx<'_>, _children: &mut Children, _rect: Recti) {}
    }

    /// Creates one real unique node for ownership checks.
    fn text_node(label: &str) -> super::super::Node {
        crate::TextBlock::create(crate::TextBlockParameters::new(label)).1
    }

    #[test]
    fn concrete_container_owns_one_collection_beside_its_widget() {
        let children = Rc::new(RefCell::new([text_node("first")].into_iter().collect()));
        let handle = crate::ui_node::children::ChildrenHandle::new(&children);
        let (_, owner) = Container::from_shared(children, GeometryOnly);

        assert_eq!(owner.with_children(Children::len), 1);
        owner.with_children_mut(|children| children.push(text_node("second")));
        assert_eq!(handle.len(), Some(2));
        drop(owner);
        assert_eq!(handle.len(), None);
    }
}
