use std::{cell::RefCell, rc::Rc};

use crate::{Dimensioni, Recti, Style, UiInputEvent, Vec2i, Widget, WidgetOption};

use super::{ChildParticipation, Children, NodeLayout, NodeRuntime, UiRuntime};

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
    surface: Option<Box<dyn Widget>>,
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
    /// becoming a second container interface.
    pub fn with_surface<W: Widget + 'static>(mut self, surface: W) -> Self {
        // Descendants remain child-first during hit testing. The dispatcher reaches this surface
        // only after it has selected the container's geometry or bubbled from a selected child.
        self.surface = Some(Box::new(surface));
        self
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

    fn accepts_event(&self, event: &UiInputEvent) -> bool {
        // Event eligibility belongs entirely to the optional surface widget.
        self.surface.as_ref().is_some_and(|surface| surface.accepts_event(event))
    }

    fn focus_policy(&self) -> crate::FocusPolicy {
        // Preserve ordinary Widget defaults for inert containers and delegate richer behavior.
        self.surface
            .as_ref()
            .map_or_else(|| crate::FocusPolicy::from_widget_options(self.opt), |surface| surface.focus_policy())
    }

    fn keeps_pointer_capture(&self) -> bool {
        // No surface means there is no local interaction state capable of cancelling capture.
        self.surface.as_ref().is_none_or(|surface| surface.keeps_pointer_capture())
    }

    fn pointer_capture_lost(&mut self) {
        // Loss notification is local; descendants own and receive their own capture lifecycle.
        if let Some(surface) = &mut self.surface {
            surface.pointer_capture_lost();
        }
    }
}

/// Dispatcher-internal result of delivering one event to an already-selected node.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum DispatchResult {
    /// The selected widget declined the event, allowing ancestor-only bubbling.
    Ignored,
    /// The widget consumed the event without acquiring pointer capture.
    Consumed,
    /// The widget consumed the event and requests dispatcher-owned pointer capture.
    Captured,
}

impl DispatchResult {
    /// Returns whether dispatch and ancestor bubbling should stop.
    pub(crate) fn is_consumed(self) -> bool {
        matches!(self, Self::Consumed | Self::Captured)
    }
}

/// Delivers one already-targeted event through common dispatcher interaction rules.
pub(super) fn dispatch_widget_input(
    runtime: &mut UiRuntime,
    state: &NodeRuntime,
    rect: Recti,
    clip: Recti,
    opt: WidgetOption,
    accepts_event: bool,
    event: &UiInputEvent,
) -> DispatchResult {
    // Target selection may reach a geometrically matching node whose dynamic options or widget
    // policy decline this specific event. Such a node remains available for ancestor bubbling.
    if opt.intersects(WidgetOption::NO_INTERACT) || !accepts_event {
        return DispatchResult::Ignored;
    }

    let id = state.id();
    if event.is_focus_input() {
        // Keyboard/text events have no meaningful rectangle. Deliver them only to the dispatcher-
        // owned focus identity selected by an earlier pointer or programmatic transition.
        if runtime.focus != Some(id) {
            return DispatchResult::Ignored;
        }
        runtime.push_routed_event(id, event.clone());
        return DispatchResult::Consumed;
    }

    // Capture lets drag/release escape the original rectangle; all other pointer events still need
    // to hit both the node allocation and its effective clip.
    let captured = runtime.capture == Some(id);
    let event_hits_rect = event.position().is_some_and(|pos| rect.contains(&pos) && clip.contains(&pos));
    match event {
        UiInputEvent::MouseDown { button, .. } if event_hits_rect => {
            // Press establishes focus/click state and asks routing to acquire pointer capture.
            runtime.claim_pointer_focus(id, *button);
            runtime.push_routed_event(id, event.clone());
            DispatchResult::Captured
        }
        UiInputEvent::MouseDrag { .. } if captured || event_hits_rect => {
            // An uncaptured drag can be consumed under the pointer but does not create capture.
            runtime.push_routed_event(id, event.clone());
            if captured { DispatchResult::Captured } else { DispatchResult::Consumed }
        }
        UiInputEvent::MouseUp { .. } if captured || event_hits_rect => {
            // Routing releases capture after the recipient observes this event during update.
            runtime.push_routed_event(id, event.clone());
            DispatchResult::Consumed
        }
        UiInputEvent::MouseMove { .. } | UiInputEvent::Scroll { .. } if event_hits_rect => {
            // Hover and wheel delivery are hit-based and never acquire capture.
            runtime.push_routed_event(id, event.clone());
            DispatchResult::Consumed
        }
        _ => DispatchResult::Ignored,
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
