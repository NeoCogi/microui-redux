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

use crate::{Dimensioni, Recti, Style, TypedWidgetHandle, UiInputEvent, Vec2i, Widget, WidgetOption};

use super::{ChildParticipation, Children, NodeLayout, NodeRuntime, UiRuntime};

/// Complete behavior of one retained branch widget.
///
/// The concrete widget owns semantic, interaction, and layout policy while [`Container`] owns the
/// heterogeneous child collection separately. Runtime calls never retain the widget borrow while
/// recursively visiting descendants.
pub trait ContainerWidget: Widget {
    /// Measures preferred content from the authoritative child collection.
    fn measure(&self, children: &Children, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni;

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
/// independently allocated so runtime phases can borrow it only for the current operation and
/// release it before recursive child traversal.
pub struct Container {
    /// One authoritative shared cell for mounted descendants.
    children: Rc<RefCell<Children>>,
    /// Sole persistent strong owner of the concrete typed container widget.
    widget: Rc<RefCell<dyn ContainerWidget>>,
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
        let widget = Rc::new(RefCell::new(widget));
        Self::from_shared_owner(children, widget)
    }

    /// Adopts a concrete widget allocation prepared for internal weak composition links.
    pub(crate) fn from_shared_owner<W>(children: Rc<RefCell<Children>>, widget: Rc<RefCell<W>>) -> (TypedWidgetHandle<W>, Self)
    where
        W: ContainerWidget + 'static,
    {
        let handle = TypedWidgetHandle::new(&widget);
        let widget: Rc<RefCell<dyn ContainerWidget>> = widget;
        (handle, Self { children, widget })
    }

    /// Returns whether the installed surface supports one dispatcher-selected event.
    pub(crate) fn accepts_event(&self, event: &UiInputEvent) -> bool {
        // A geometry-only container has no event-receiving surface. An installed surface owns only
        // its additional state-dependent filter; generic option policy remains in the dispatcher.
        self.widget
            .try_borrow()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
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

    /// Invokes the geometry policy for one placement pass.
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
            .place(ctx, &mut children, rect);
    }

    /// Resolves frame width and measures content under one typed-runtime borrow.
    pub(crate) fn measure_content_with_frame(&self, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> (i32, Dimensioni) {
        let children = self
            .children
            .try_borrow()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is mutably borrowed during measurement"));
        let widget = self.widget.try_borrow().unwrap_or_else(|_| typed_container_borrow_conflict());
        let border_width = if widget.effective_widget_opt().intersects(WidgetOption::FRAME) {
            style.frame_border().width.max(0)
        } else {
            0
        };
        let measured = ContainerWidget::measure(&*widget, &children, style, atlas, super::frame::content_available(available, border_width));
        (border_width, measured)
    }

    /// Returns the concrete widget's effective options.
    pub(crate) fn effective_widget_opt(&self) -> WidgetOption {
        self.widget
            .try_borrow()
            .unwrap_or_else(|_| typed_container_borrow_conflict())
            .effective_widget_opt()
    }

    /// Runs one common widget query against the concrete container object.
    pub(crate) fn with_widget<R>(&self, f: impl FnOnce(&dyn Widget) -> R) -> R {
        let widget = self.widget.try_borrow().unwrap_or_else(|_| typed_container_borrow_conflict());
        f(&*widget)
    }

    /// Runs one common mutable widget phase against the concrete container object.
    pub(crate) fn with_widget_mut<R>(&mut self, f: impl FnOnce(&mut dyn Widget) -> R) -> R {
        let mut widget = self.widget.try_borrow_mut().unwrap_or_else(|_| typed_container_borrow_conflict());
        f(&mut *widget)
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
    runtime: &'a mut UiRuntime,
    style: &'a Style,
    atlas: &'a crate::AtlasHandle,
    content: Recti,
    current: &'a mut NodeRuntime,
}

impl ContainerLayoutCtx<'_> {
    /// Creates one runtime-scoped placement context.
    pub(crate) fn new<'a>(
        runtime: &'a mut UiRuntime,
        style: &'a Style,
        atlas: &'a crate::AtlasHandle,
        content: Recti,
        current: &'a mut NodeRuntime,
    ) -> ContainerLayoutCtx<'a> {
        // All references share one placement lifetime, preventing the context from escaping the
        // runtime call that owns the mutable current-node state.
        ContainerLayoutCtx { runtime, style, atlas, content, current }
    }

    /// Returns the active UI style.
    pub fn style(&self) -> &Style {
        self.style
    }

    /// Returns the active atlas.
    pub fn atlas(&self) -> &crate::AtlasHandle {
        self.atlas
    }

    /// Returns one child's parent-owned placement policy.
    pub fn child_policy(&self, children: &Children, index: usize) -> Option<crate::Policy> {
        // Delegate through Children so no Node reference crosses the public layout boundary.
        children.child_policy(index)
    }

    /// Assigns one indexed child rectangle and returns its resulting allocated size.
    pub fn layout_child(&mut self, children: &mut Children, index: usize, rect: Recti) -> Option<Dimensioni> {
        // Resolve the child internally, recurse immediately, and return only derived geometry.
        let node = children.get_mut(index)?;
        Some(self.runtime.layout_node_ref(node, self.style, self.atlas, rect))
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
            .intersect(&self.content)
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

    /// Minimal policy proving that concrete ownership needs geometry only.
    struct GeometryOnly;

    impl Widget for GeometryOnly {
        fn widget_opt(&self) -> &WidgetOption {
            &WidgetOption::NO_INTERACT
        }

        fn update(&mut self, _ctx: &mut crate::WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

        fn paint(&mut self, _ctx: &mut crate::WidgetPaintCtx<'_>) {}
    }

    impl ContainerWidget for GeometryOnly {
        fn measure(&self, children: &Children, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
            Dimensioni::new(children.len() as i32, 1)
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
