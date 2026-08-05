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
    pub fn new<L>(layout: L, opt: WidgetOption, children: impl IntoIterator<Item = super::Node>) -> Self
    where
        L: Layout,
    {
        // Collect once at the ownership boundary; no strong collection handle leaves this value.
        Self {
            children: Rc::new(RefCell::new(children.into_iter().collect())),
            layout: Box::new(layout),
            opt: opt | WidgetOption::NO_INTERACT,
            surface: None,
        }
    }

    pub(crate) fn from_shared<L>(children: Rc<RefCell<Children>>, layout: L, opt: WidgetOption) -> Self
    where
        L: Layout,
    {
        Self {
            children,
            layout: Box::new(layout),
            opt: opt | WidgetOption::NO_INTERACT,
            surface: None,
        }
    }

    /// Installs ordinary widget behavior on this container's own surface.
    pub fn with_surface<W: Widget + 'static>(mut self, surface: W) -> Self {
        // Descendants remain child-first during hit testing. The dispatcher reaches this surface
        // only after it has selected the container's geometry or bubbled from a selected child.
        self.surface = Some(Box::new(surface));
        self
    }

    /// Runs one immutable framework traversal without exposing child storage publicly.
    pub(crate) fn with_children<R>(&self, f: impl FnOnce(&Children) -> R) -> R {
        let children = self
            .children
            .try_borrow()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is mutably borrowed during immutable traversal"));
        f(&children)
    }

    /// Runs one mutable framework traversal while preventing concurrent topology mutation.
    pub(crate) fn with_children_mut<R>(&self, f: impl FnOnce(&mut Children) -> R) -> R {
        let mut children = self
            .children
            .try_borrow_mut()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is already borrowed during mutable traversal"));
        f(&mut children)
    }

    /// Invokes the geometry policy for one placement pass.
    pub(crate) fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        let mut children = self
            .children
            .try_borrow_mut()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is already borrowed during layout"));
        self.layout.place(ctx, &mut children, rect);
    }
}

impl Widget for Container {
    fn widget_opt(&self) -> &WidgetOption {
        self.surface.as_ref().map_or(&self.opt, |surface| surface.widget_opt())
    }

    fn measure(&self, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let children = self
            .children
            .try_borrow()
            .unwrap_or_else(|_| panic!("retained child invariant violated: collection is mutably borrowed during measurement"));
        self.layout.measure(&children, style, atlas, available)
    }

    fn update(&mut self, ctx: &mut crate::WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        if let Some(surface) = &mut self.surface {
            // The dispatcher has already selected and localized this one event.
            surface.update(ctx, input);
        }
    }

    fn paint(&mut self, ctx: &mut crate::WidgetPaintCtx<'_>) {
        if let Some(surface) = &mut self.surface {
            surface.paint(ctx);
        }
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        self.surface.as_ref().map_or(self.opt, |surface| surface.effective_widget_opt())
    }

    fn accepts_scroll(&self, delta: Vec2i) -> bool {
        self.surface.as_ref().is_some_and(|surface| surface.accepts_scroll(delta))
    }

    fn accepts_event(&self, event: &UiInputEvent) -> bool {
        self.surface.as_ref().is_some_and(|surface| surface.accepts_event(event))
    }

    fn focus_policy(&self) -> crate::FocusPolicy {
        self.surface
            .as_ref()
            .map_or_else(|| crate::FocusPolicy::from_widget_options(self.opt), |surface| surface.focus_policy())
    }

    fn keeps_pointer_capture(&self) -> bool {
        self.surface.as_ref().is_none_or(|surface| surface.keeps_pointer_capture())
    }

    fn pointer_capture_lost(&mut self) {
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
    if opt.intersects(WidgetOption::NO_INTERACT) || !accepts_event {
        return DispatchResult::Ignored;
    }

    let id = state.id();
    if event.is_focus_input() {
        if runtime.focus != Some(id) {
            return DispatchResult::Ignored;
        }
        runtime.push_routed_event(id, event.clone());
        return DispatchResult::Consumed;
    }

    let captured = runtime.capture == Some(id);
    let event_hits_rect = event.position().is_some_and(|pos| rect.contains(&pos) && clip.contains(&pos));
    match event {
        UiInputEvent::MouseDown { button, .. } if event_hits_rect => {
            runtime.claim_pointer_focus(id, *button);
            runtime.push_routed_event(id, event.clone());
            DispatchResult::Captured
        }
        UiInputEvent::MouseDrag { .. } if captured || event_hits_rect => {
            runtime.push_routed_event(id, event.clone());
            if captured { DispatchResult::Captured } else { DispatchResult::Consumed }
        }
        UiInputEvent::MouseUp { .. } if captured || event_hits_rect => {
            runtime.push_routed_event(id, event.clone());
            DispatchResult::Consumed
        }
        UiInputEvent::MouseMove { .. } | UiInputEvent::Scroll { .. } if event_hits_rect => {
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
        children.child_policy(index)
    }

    /// Assigns one indexed child rectangle and returns its resulting allocated size.
    pub fn layout_child(&mut self, children: &mut Children, index: usize, rect: Recti) -> Option<Dimensioni> {
        let node = children.get_mut(index)?;
        Some(self.runtime.layout_node_ref(node, self.style, self.atlas, rect))
    }

    /// Reads one child's content extent from its most recent placement in this pass.
    pub fn child_content_size(&self, children: &Children, index: usize) -> Option<Dimensioni> {
        children.get(index).map(|node| node.state.layout.content_size)
    }

    /// Commits whether one retained child participates after this layout pass.
    pub fn set_child_participation(&mut self, children: &mut Children, index: usize, participation: ChildParticipation) -> bool {
        let Some(node) = children.get_mut(index) else { return false };
        node.state.participation = participation;
        true
    }

    /// Replaces the current container's derived logical content size.
    pub fn set_content_size(&mut self, size: Dimensioni) {
        self.current.set_layout(self.current.layout.with_content_size(size));
    }

    /// Installs the current container's descendant viewport and translation.
    pub fn set_children_viewport(&mut self, viewport: Recti, offset: Vec2i) {
        let viewport = viewport
            .intersect(&self.content)
            .unwrap_or_else(|| Recti::new(self.content.x, self.content.y, 0, 0));
        let outer = self.current.layout.allocation;
        let mut layout = NodeLayout::from_parts(outer, viewport, self.current.layout.content_size);
        layout.children.offset = offset;
        self.current.set_layout(layout);
    }

    /// Controls whether descendant overflow contributes to the parent-visible extent.
    pub fn set_child_overflow_propagation(&mut self, propagate: bool) {
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
