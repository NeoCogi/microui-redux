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

use crate::{Dimensioni, Recti, Style, UiInputEvent, Vec2i, Widget, WidgetOption};

use super::{ChildParticipation, Children, NodeLayout, NodeRuntime, UiRuntime};

/// Interactive behavior installed on a [`Container`]'s own surface.
///
/// Ordinary leaf widgets need only [`Widget`]. A container surface has the additional ability to
/// decline a dispatcher-selected event so that it can bubble through structural ancestors. The
/// dispatcher still owns target selection, generic option checks, focus, and pointer capture.
pub trait ContainerSurface: Widget {
    /// Returns whether this surface supports one already-selected event.
    ///
    /// Returning `false` does not expose a covered sibling; it permits only ancestor bubbling.
    /// Captured drag and release delivery bypass this query because capture identity is already an
    /// authoritative runtime decision. Generic scroll eligibility remains derived from
    /// [`WidgetOption::GRAB_SCROLL`] before this surface-specific query is considered.
    fn accepts_event(&self, _event: &UiInputEvent) -> bool {
        // Most surfaces support every event admitted by generic dispatcher options. Specialized
        // surfaces override only when committed local state narrows that set further.
        true
    }
}

/// Geometry policy installed in a retained [`Container`].
///
/// A layout can measure the owner's children and commit their rectangles, viewport, logical
/// content extent, and participation. It receives no update, paint, focus, capture, event, or
/// topology-routing capability; those remain widget and dispatcher responsibilities.
pub trait Layout: 'static {
    /// Measures the preferred content extent for the current child collection.
    fn measure(&self, children: &Children, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni;

    /// Places retained children inside the container's local content rectangle.
    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti);
}

/// Concrete retained owner for one geometry policy and an opaque ordered child collection.
///
/// This is the only strong owner of its direct [`Node`](crate::Node) values. Its boxed layout can
/// borrow children only during measure or placement. An optional ordinary widget supplies behavior
/// for the container's own surface; it never receives child access.
pub struct Container {
    /// One authoritative shared cell for mounted descendants.
    children: Rc<RefCell<Children>>,
    /// Dynamic geometry policy for the authoritative collection.
    layout: Box<dyn Layout>,
    /// Static options for a container with no installed surface widget.
    opt: WidgetOption,
    /// Optional interactive or painted behavior for this container's own surface.
    surface: Option<Box<dyn ContainerSurface>>,
}

impl Container {
    /// Creates an unmounted owner from one layout and initial child sequence.
    ///
    /// This is the complete public construction path. The iterator is consumed exactly once, the
    /// resulting collection becomes private, and the concrete layout is boxed at the point where
    /// heterogeneous containers enter [`crate::Node`]. A container without a surface is geometry
    /// only and therefore receives `NO_INTERACT` automatically.
    pub fn new<L>(layout: L, opt: WidgetOption, children: impl IntoIterator<Item = super::Node>) -> Self
    where
        L: Layout,
    {
        // Collect once at the ownership boundary; no strong collection handle leaves this value.
        Self {
            children: Rc::new(RefCell::new(children.into_iter().collect())),
            // Only the Layout value is dynamically dispatched; Container itself stays concrete.
            layout: Box::new(layout),
            // Geometry-only containers must be transparent to pointer target selection.
            opt: opt | WidgetOption::NO_INTERACT,
            surface: None,
        }
    }

    /// Creates a container around child storage already prepared by a built-in constructor.
    ///
    /// Built-in mutable state needs a weak [`super::children::ChildrenHandle`] pointing at the same
    /// allocation the container will own. Those constructors create the cell, derive the weak
    /// capability, and then move the sole persistent strong reference here. Keeping this function
    /// crate-private prevents downstream code from creating a second child owner.
    pub(crate) fn from_shared<L>(children: Rc<RefCell<Children>>, layout: L, opt: WidgetOption) -> Self
    where
        L: Layout,
    {
        // The caller has already collected children and installed any weak typed-state capability.
        Self {
            children,
            layout: Box::new(layout),
            opt: opt | WidgetOption::NO_INTERACT,
            surface: None,
        }
    }

    /// Installs ordinary widget behavior on this container's own surface.
    ///
    /// Surface behavior is optional and independent of layout: it can paint or receive routed
    /// input, but it has no access to the child collection. This keeps interaction composition from
    /// becoming a second container interface. A type with ordinary surface behavior can opt into
    /// this role with an empty [`ContainerSurface`] implementation; state-dependent event filters
    /// override [`ContainerSurface::accepts_event`].
    pub fn with_surface<S: ContainerSurface + 'static>(mut self, surface: S) -> Self {
        // Descendants remain child-first during hit testing. The dispatcher reaches this surface
        // only after it has selected the container's geometry or bubbled from a selected child.
        self.surface = Some(Box::new(surface));
        self
    }

    /// Returns whether the installed surface supports one dispatcher-selected event.
    pub(crate) fn accepts_event(&self, event: &UiInputEvent) -> bool {
        // A geometry-only container has no event-receiving surface. An installed surface owns only
        // its additional state-dependent filter; generic option policy remains in the dispatcher.
        self.surface.as_ref().is_some_and(|surface| surface.accepts_event(event))
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
        // Layout receives the authoritative collection only for this call. A typed state mutation
        // attempted against the same container while this borrow is active is rejected cleanly by
        // ChildrenHandle rather than invalidating indices during placement.
        let mut children = self
            .children
            .try_borrow_mut()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is already borrowed during layout"));
        self.layout.place(ctx, &mut children, rect);
    }
}

impl Widget for Container {
    fn widget_opt(&self) -> &WidgetOption {
        // Surface options are authoritative when behavior exists; otherwise use the inert fallback.
        self.surface.as_ref().map_or(&self.opt, |surface| surface.widget_opt())
    }

    fn measure(&self, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        // Measurement is read-only and delegates exclusively to Layout. Surface widgets cannot
        // introduce a competing size calculation for the same container.
        let children = self
            .children
            .try_borrow()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is mutably borrowed during measurement"));
        self.layout.measure(&children, style, atlas, available)
    }

    fn update(&mut self, ctx: &mut crate::WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // A geometry-only container has no local update work; descendants are traversed separately
        // by UiRuntime after this method returns.
        if let Some(surface) = &mut self.surface {
            // The dispatcher has already selected and localized this one event.
            surface.update(ctx, input);
        }
    }

    fn paint(&mut self, ctx: &mut crate::WidgetPaintCtx<'_>) {
        // Paint the local surface before the runtime paints children in forward sibling order.
        if let Some(surface) = &mut self.surface {
            surface.paint(ctx);
        }
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        // Dynamic surface options (for example scroll enablement) override the static fallback.
        self.surface.as_ref().map_or(self.opt, |surface| surface.effective_widget_opt())
    }

    fn focus_policy(&self) -> crate::FocusPolicy {
        // Preserve ordinary Widget defaults for inert containers and delegate richer behavior.
        self.surface
            .as_ref()
            .map_or_else(|| crate::FocusPolicy::from_widget_options(self.opt), |surface| surface.focus_policy())
    }
}

/// Framework-scoped geometry services available to one active [`Layout`] call.
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

    impl Layout for GeometryOnly {
        fn measure(&self, children: &Children, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
            Dimensioni::new(children.len() as i32, 1)
        }

        fn place(&mut self, _ctx: &mut ContainerLayoutCtx<'_>, _children: &mut Children, _rect: Recti) {}
    }

    /// Creates one real unique node for ownership checks.
    fn text_node(label: &str) -> super::super::Node {
        let (_, widget) = crate::TextBlock::create(crate::TextBlockParameters::new(label));
        super::super::Node::widget(widget)
    }

    #[test]
    fn concrete_container_owns_one_collection_beside_its_layout() {
        let children = Rc::new(RefCell::new([text_node("first")].into_iter().collect()));
        let handle = crate::ui_node::children::ChildrenHandle::new(&children);
        let owner = Container::from_shared(children, GeometryOnly, WidgetOption::NONE);

        assert_eq!(owner.with_children(Children::len), 1);
        owner.with_children_mut(|children| children.push(text_node("second")));
        assert_eq!(handle.len(), Some(2));
        drop(owner);
        assert_eq!(handle.len(), None);
    }
}
