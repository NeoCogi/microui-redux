use crate::render::CustomRenderKey;
use crate::{Dimensioni, KeyCode, KeyMode, MouseButton, Recti, Style, Vec2i};
use crate::{Widget, WidgetOption, WidgetParameters, WidgetState, WidgetStateOwner};

#[cfg(test)]
use crate::widget_ctx::{WidgetPaintCtx, WidgetUpdateCtx};

use super::{Children, Node, NodeLayout, NodeRuntime, UiRuntime};

mod column;
mod disclosure;
mod grid;
mod row;
mod scroll_area;
mod stack;

pub use column::{Column, ColumnBuilder, ColumnContainer, ColumnParameters, ColumnState};
pub use disclosure::{Disclosure, DisclosureBuilder, DisclosureContainer, DisclosureParameters, DisclosureState};
pub use grid::{Grid, GridBuilder, GridContainer, GridItem, GridParameters, GridSpan, GridState};
pub use row::{Row, RowBuilder, RowContainer, RowParameters, RowState};
pub use scroll_area::{ScrollArea, ScrollAreaBuilder, ScrollAreaContainer, ScrollAreaOption, ScrollAreaParameters, ScrollAreaState};
pub use stack::{Stack, StackBuilder, StackContainer, StackParameters, StackState};

/// Marker for application-facing state owned by a concrete container runtime.
///
/// The marker deliberately grants no generic child access. Built-in state types expose only their
/// topology-safe inherent operations. It adds no parallel measurement, update, paint, layout, or
/// input contract; those belong to the runtime's [`Widget`] and [`Container`] implementations.
pub trait ContainerState: WidgetState {}

/// Associates one-shot parameters with one concrete state-owning container runtime.
///
/// Downstream convenience constructors use `create_container`, obtain the runtime's typed weak
/// state handle through [`WidgetStateOwner::state_handle`], and finish ownership with
/// [`Node::container`]. Parameters do not choose whether a handle is returned: built-in concrete
/// constructors always return `(WidgetStateHandle<State>, Node)`.
///
/// ```
/// use microui_redux::{ContainerBuilder, Node, WidgetStateHandle, WidgetStateOwner};
///
/// fn finish<B>(parameters: B::Parameters) -> (WidgetStateHandle<<B::W as WidgetStateOwner>::State>, Node)
/// where
///     B: ContainerBuilder,
/// {
///     let runtime = B::create_container(parameters);
///     let state = runtime.state_handle();
///     (state, Node::container(runtime))
/// }
/// ```
pub trait ContainerBuilder: Sized + 'static {
    /// One-shot construction input shared with the widget builder model.
    type Parameters: WidgetParameters;
    /// Concrete runtime created before generic insertion erases it.
    type W: Container + WidgetStateOwner;

    /// Consumes parameters and creates the concrete container runtime.
    fn create_container(parameters: Self::Parameters) -> Self::W;
}

/// Scoped immutable child visitor constructed only by retained traversal.
///
/// A container must submit exactly one authoritative collection. Ordinary callers cannot create a
/// visitor, install an extraction callback, or retain a child borrow after `visit` returns. The
/// framework may recurse into submitted descendants while the parent state is borrowed; this
/// controlled recursion is the exception to the application rule against keeping state-access
/// closures active across traversal.
pub struct ChildrenVisitor<'a> {
    callback: &'a mut dyn FnMut(&Children),
    submissions: usize,
}

impl ChildrenVisitor<'_> {
    /// Submits this container's authoritative child collection to the active traversal.
    pub fn visit(&mut self, children: &Children) {
        if self.submissions != 0 {
            panic!("Container::visit_children must submit exactly one Children collection");
        }
        self.submissions = 1;
        (self.callback)(children);
    }

    fn finish(&self) {
        if self.submissions != 1 {
            panic!("Container::visit_children must submit exactly one Children collection");
        }
    }
}

/// Scoped mutable child visitor constructed only by retained traversal.
///
/// The collection must be the same authoritative collection submitted by [`ChildrenVisitor`]. A
/// borrow cannot escape `visit`, and attached nodes remain opaque while the framework recurses.
/// The owning container state remains borrowed across that recursion, so a checked application
/// mutation of the same container returns `None`; another currently available state cell can still
/// be changed and is observed according to traversal order.
pub struct ChildrenVisitorMut<'a> {
    callback: &'a mut dyn FnMut(&mut Children),
    submissions: usize,
}

impl ChildrenVisitorMut<'_> {
    /// Submits this container's authoritative child collection to mutable traversal.
    pub fn visit(&mut self, children: &mut Children) {
        if self.submissions != 0 {
            panic!("Container::visit_children_mut must submit exactly one Children collection");
        }
        self.submissions = 1;
        (self.callback)(children);
    }

    fn finish(&self) {
        if self.submissions != 1 {
            panic!("Container::visit_children_mut must submit exactly one Children collection");
        }
    }
}

/// Runs framework work against one immutable collection without returning its borrow.
pub(crate) fn with_container_children<R>(container: &dyn Container, f: impl FnOnce(&Children) -> R) -> R {
    let mut f = Some(f);
    let mut result = None;
    {
        let mut callback = |children: &Children| {
            let f = f.take().expect("Container::visit_children submitted more than once");
            result = Some(f(children));
        };
        let mut visitor = ChildrenVisitor { callback: &mut callback, submissions: 0 };
        container.visit_children(&mut visitor);
        visitor.finish();
    }
    result.expect("Container::visit_children did not submit Children")
}

/// Runs framework work against one mutable collection without returning its borrow.
pub(crate) fn with_container_children_mut<R>(container: &mut dyn Container, f: impl FnOnce(&mut Children) -> R) -> R {
    let mut f = Some(f);
    let mut result = None;
    {
        let mut callback = |children: &mut Children| {
            let f = f.take().expect("Container::visit_children_mut submitted more than once");
            result = Some(f(children));
        };
        let mut visitor = ChildrenVisitorMut { callback: &mut callback, submissions: 0 };
        container.visit_children_mut(&mut visitor);
        visitor.finish();
    }
    result.expect("Container::visit_children_mut did not submit Children")
}

/// Runtime contract for a widget that uniquely owns retained children.
///
/// Common measurement, update, paint, options, and focus behavior remain inherited from
/// [`Widget`]. Implementations must submit the same authoritative [`Children`] collection exactly
/// once from both visitor methods. `layout` and `route_input` are the only container-specific
/// phases.
///
/// Capture responsibilities are deliberately split. The tree runtime owns the private captured
/// node identity. The captured container owns only its local retention predicate and cleanup hook.
/// Ancestors own descendant eligibility through [`Container::children_visible`]; collapsing or
/// removing an ancestor can therefore revoke descendant capture without giving that ancestor the
/// captured identity. Containers with no local capture state use the defaults. A container that
/// can keep capture after routing must override both capture methods consistently.
pub trait Container: Widget {
    /// Supplies the authoritative collection for immutable traversal exactly once.
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>);
    /// Supplies the same authoritative collection for mutable traversal exactly once.
    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>);

    /// Assigns child rectangles within this container's local content coordinates.
    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti);

    /// Reports whether descendants participate in traversal while remaining owned.
    ///
    /// This is an ancestor-owned descendant gate, not generic node visibility. Built-in
    /// [`Disclosure`] uses it for collapsed content; root visibility remains a separate
    /// [`crate::Context::set_root_visible`] operation.
    fn children_visible(&self) -> bool {
        true
    }

    /// Reports whether this container's current local pointer-capture interaction remains active.
    ///
    /// The retained runtime owns the captured node identity. This query can only retain or revoke
    /// capture already owned by this container; it receives no identity or tree capability. The
    /// default keeps an otherwise valid capture and is correct for containers without revocable
    /// local drag state.
    fn retains_pointer_capture(&self) -> bool {
        true
    }

    /// Clears container-local interaction state after the runtime ends this container's capture.
    ///
    /// The default is appropriate for containers without capture-specific local state. The runtime
    /// invokes this only for the captured container itself, never through an ancestor. Override
    /// this together with [`Container::retains_pointer_capture`] when capture owns local state.
    fn on_pointer_capture_lost(&mut self) {}

    /// Routes one event to this container's own interactive surface before that event's update.
    ///
    /// Descendant routing is framework-owned. An override may restrict this container's local hit
    /// surface and then call [`ContainerInputCtx::route_widget`] or
    /// [`ContainerInputCtx::route_widget_in_rect`]. Focus behavior comes only from the inherited
    /// [`Widget::focus_policy`] query.
    fn route_input(&mut self, ctx: &mut ContainerInputCtx<'_>, event: &UiInputEvent) -> ContainerInputResult {
        ctx.route_widget(event, self.effective_widget_opt())
    }
}

/// Thin retained leaf owner for one erased widget and optional custom-render metadata.
pub(crate) struct WidgetNode {
    /// Concrete state-owning runtime erased only after generic insertion validates its owner.
    pub(crate) widget: Box<dyn Widget>,
    /// Optional custom backend render callback for custom-render leaves.
    custom_render: Option<CustomRenderKey>,
}

impl WidgetNode {
    /// Erases one concrete state-owning runtime at the retained leaf boundary.
    pub(crate) fn new<W: WidgetStateOwner>(widget: W, custom_render: Option<CustomRenderKey>) -> Self {
        Self { widget: Box::new(widget), custom_render }
    }

    /// Returns the private custom-render callback key, when one was supplied at construction.
    pub(crate) fn custom_render(&self) -> Option<CustomRenderKey> {
        self.custom_render
    }
}

/// Input event routed to one retained widget or container.
#[derive(Clone, Debug)]
pub enum UiInputEvent {
    /// Pointer moved without any mouse button held.
    MouseMove {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Pointer movement since the previous queued pointer-position event.
        delta: Vec2i,
    },
    /// Pointer moved while one or more mouse buttons are held.
    MouseDrag {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Pointer movement since the previous queued pointer-position event.
        delta: Vec2i,
        /// Mouse buttons held during the drag.
        buttons: MouseButton,
    },
    /// One or more mouse buttons were pressed.
    MouseDown {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Buttons carried by this queued press transition.
        button: MouseButton,
    },
    /// One or more mouse buttons were released.
    MouseUp {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Buttons carried by this queued release transition.
        button: MouseButton,
    },
    /// Scroll wheel or equivalent high-level scroll input.
    Scroll {
        /// Pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Requested scroll delta.
        delta: Vec2i,
    },
    /// Modifier/control key state was pressed.
    KeyDown {
        /// Modifier/control key bits carried by this queued press transition.
        key: KeyMode,
    },
    /// Modifier/control key state was released.
    KeyUp {
        /// Modifier/control key bits carried by this queued release transition.
        key: KeyMode,
    },
    /// Navigation key state was pressed.
    KeyCodeDown {
        /// Navigation key bits carried by this queued press transition.
        code: KeyCode,
    },
    /// Navigation key state was released.
    KeyCodeUp {
        /// Navigation key bits carried by this queued release transition.
        code: KeyCode,
    },
    /// One queued UTF-8 text input transition.
    Text {
        /// Entered text.
        text: String,
    },
}

impl UiInputEvent {
    /// Returns whether this event belongs to pointer routing.
    pub(crate) fn is_pointer(&self) -> bool {
        matches!(
            self,
            Self::MouseMove { .. } | Self::MouseDrag { .. } | Self::MouseDown { .. } | Self::MouseUp { .. } | Self::Scroll { .. }
        )
    }

    /// Returns whether this event should be delivered to the focused node.
    pub(crate) fn is_focus_input(&self) -> bool {
        matches!(
            self,
            Self::KeyDown { .. } | Self::KeyUp { .. } | Self::KeyCodeDown { .. } | Self::KeyCodeUp { .. } | Self::Text { .. }
        )
    }

    /// Returns whether this event ends an active pointer capture when no buttons remain held.
    pub(crate) fn is_pointer_release(&self) -> bool {
        matches!(self, Self::MouseUp { .. })
    }
}

/// Result of routing one input event to a container or leaf surface.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ContainerInputResult {
    /// The node ignored the event.
    Ignored,
    /// The node consumed the event.
    Consumed,
    /// The node consumed the event and should keep receiving related pointer input.
    Captured,
}

impl ContainerInputResult {
    /// Returns whether event traversal should stop.
    pub(crate) fn is_consumed(self) -> bool {
        matches!(self, Self::Consumed | Self::Captured)
    }
}

fn event_position(event: &UiInputEvent) -> Option<Vec2i> {
    match event {
        UiInputEvent::MouseMove { pos, .. }
        | UiInputEvent::MouseDrag { pos, .. }
        | UiInputEvent::MouseDown { pos, .. }
        | UiInputEvent::MouseUp { pos, .. }
        | UiInputEvent::Scroll { pos, .. } => Some(*pos),
        UiInputEvent::KeyDown { .. }
        | UiInputEvent::KeyUp { .. }
        | UiInputEvent::KeyCodeDown { .. }
        | UiInputEvent::KeyCodeUp { .. }
        | UiInputEvent::Text { .. } => None,
    }
}

pub(super) fn route_public_widget_input(
    runtime: &mut UiRuntime,
    state: &NodeRuntime,
    rect: Recti,
    clip: Recti,
    opt: WidgetOption,
    event: &UiInputEvent,
) -> ContainerInputResult {
    if opt.intersects(WidgetOption::NO_INTERACT) {
        return ContainerInputResult::Ignored;
    }

    let id = state.id();
    if event.is_focus_input() {
        // Enforce the router invariant at the final delivery boundary as well as at target lookup.
        if runtime.focus != Some(id) {
            return ContainerInputResult::Ignored;
        }
        runtime.push_routed_event(id, event.clone());
        return ContainerInputResult::Consumed;
    }

    let captured = runtime.capture == Some(id);
    let hovered = event_position(event).map(|pos| rect.contains(&pos) && clip.contains(&pos)).unwrap_or(false);

    match event {
        UiInputEvent::MouseDown { button, .. } if hovered => {
            runtime.claim_pointer_focus(id, *button);
            runtime.push_routed_event(id, event.clone());
            ContainerInputResult::Captured
        }
        UiInputEvent::MouseDrag { .. } if captured || state.focused || hovered => {
            runtime.push_routed_event(id, event.clone());
            if captured {
                ContainerInputResult::Captured
            } else {
                ContainerInputResult::Consumed
            }
        }
        UiInputEvent::MouseUp { .. } if captured || state.focused || hovered => {
            runtime.push_routed_event(id, event.clone());
            ContainerInputResult::Consumed
        }
        UiInputEvent::MouseMove { .. } if hovered => {
            runtime.push_routed_event(id, event.clone());
            ContainerInputResult::Consumed
        }
        UiInputEvent::Scroll { delta, .. } if hovered => {
            runtime.push_routed_event(id, event.clone());
            if opt.intersects(WidgetOption::GRAB_SCROLL) && (delta.x != 0 || delta.y != 0) {
                ContainerInputResult::Consumed
            } else {
                ContainerInputResult::Ignored
            }
        }
        _ => ContainerInputResult::Ignored,
    }
}

/// Framework-scoped geometry services available to a public [`Container`] implementation.
///
/// The fields and constructor are private so application code cannot use this context to traverse
/// arbitrary trees or mutate attached topology outside the active container call. Its public
/// operations measure or lay out indexed children from the container's authoritative collection,
/// configure descendant viewport/overflow, and expose the active style and atlas; it does not lend
/// nodes or permit topology mutation.
pub struct ContainerLayoutCtx<'a> {
    runtime: &'a mut UiRuntime,
    style: &'a Style,
    atlas: &'a crate::AtlasHandle,
    content: Recti,
    current: &'a mut NodeRuntime,
}

impl ContainerLayoutCtx<'_> {
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

    /// Returns one child's pre-insertion placement policy.
    pub fn child_policy(&self, children: &Children, index: usize) -> Option<crate::Policy> {
        children.as_slice().get(index).map(|node| node.state.policy)
    }

    /// Assigns one indexed child rectangle and returns its resulting content size.
    pub fn layout_child(&mut self, children: &mut Children, index: usize, rect: Recti) -> Option<Dimensioni> {
        let node = children.as_mut_slice().get_mut(index)?;
        Some(self.runtime.layout_node_ref(node, self.style, self.atlas, rect))
    }

    /// Replaces the current node's derived content size.
    pub fn set_content_size(&mut self, size: Dimensioni) {
        let layout = self.current.layout.with_content_size(size);
        self.current.set_layout(layout);
    }

    /// Installs the current node's descendant viewport and translation.
    pub fn set_children_viewport(&mut self, viewport: Recti, offset: Vec2i) {
        let viewport = viewport
            .intersect(&self.content)
            .unwrap_or_else(|| Recti::new(self.content.x, self.content.y, 0, 0));
        let outer = self.current.layout.allocation;
        let mut layout = NodeLayout::from_parts(outer, viewport, self.current.layout.content_size);
        layout.children.offset = offset;
        self.current.set_layout(layout);
    }

    /// Controls whether descendant overflow contributes to the parent-visible content extent.
    pub fn set_child_overflow_propagation(&mut self, propagate: bool) {
        let layout = self.current.layout.with_child_overflow_propagation(propagate);
        self.current.set_layout(layout);
    }
}

/// Framework-scoped routed-input services for one public [`Container`] call.
///
/// The context belongs to one normalized event and one current container. It exposes only the
/// current container's capture predicate and scoped routing of that container's own surface. It
/// does not expose captured identity, descendant routing, or a focus-policy override.
pub struct ContainerInputCtx<'a> {
    runtime: &'a mut UiRuntime,
    content_rect: Recti,
    content_clip: Recti,
    current: &'a NodeRuntime,
}

impl ContainerInputCtx<'_> {
    pub(crate) fn new<'a>(runtime: &'a mut UiRuntime, content_rect: Recti, content_clip: Recti, current: &'a NodeRuntime) -> ContainerInputCtx<'a> {
        ContainerInputCtx {
            runtime,
            content_rect,
            content_clip,
            current,
        }
    }

    /// Returns whether the current container owns runtime pointer capture.
    ///
    /// This exposes no node identity and cannot acquire, release, or transfer capture.
    pub fn has_pointer_capture(&self) -> bool {
        self.runtime.capture == Some(self.current.id())
    }

    /// Routes through the container's complete local content rectangle.
    ///
    /// `opt` is the surface's effective interaction option set. The runtime obtains focus behavior
    /// authoritatively from the current container's [`Widget::focus_policy`] implementation.
    pub fn route_widget(&mut self, event: &UiInputEvent, opt: WidgetOption) -> ContainerInputResult {
        route_public_widget_input(self.runtime, self.current, self.content_rect, self.content_clip, opt, event)
    }

    /// Routes through one container-local sub-rectangle intersected with the active clip.
    ///
    /// Use this for chrome such as a disclosure header or scrollbar. `rect` uses the same local
    /// content coordinate system as the event delivered to [`Container::route_input`].
    pub fn route_widget_in_rect(&mut self, event: &UiInputEvent, rect: Recti, opt: WidgetOption) -> ContainerInputResult {
        route_public_widget_input(self.runtime, self.current, rect, self.content_clip, opt, event)
    }
}

#[cfg(test)]
mod visitor_tests {
    use super::*;
    use std::any::Any;

    struct InvalidVisitorContainer {
        children: Children,
        submissions: usize,
        opt: WidgetOption,
    }

    impl Widget for InvalidVisitorContainer {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
            Dimensioni::default()
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
    }

    impl Container for InvalidVisitorContainer {
        fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
            for _ in 0..self.submissions {
                visitor.visit(&self.children);
            }
        }

        fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
            for _ in 0..self.submissions {
                visitor.visit(&mut self.children);
            }
        }

        fn layout(&mut self, _ctx: &mut ContainerLayoutCtx<'_>, _rect: Recti) {}
    }

    fn invalid(submissions: usize) -> InvalidVisitorContainer {
        InvalidVisitorContainer {
            children: Children::new(),
            submissions,
            opt: WidgetOption::NONE,
        }
    }

    fn panic_message(payload: Box<dyn Any + Send>) -> String {
        payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default()
    }

    #[test]
    fn immutable_visitor_requires_exactly_one_authoritative_collection() {
        for submissions in [0, 2] {
            let container = invalid(submissions);
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_container_children(&container, |_| ());
            }))
            .expect_err("invalid immutable visitor submissions must panic");
            let message = panic_message(panic);
            assert!(message.contains("visit_children"), "unexpected diagnostic: {message}");
            assert!(message.contains("exactly one Children collection"), "unexpected diagnostic: {message}");
        }
    }

    #[test]
    fn mutable_visitor_requires_exactly_one_authoritative_collection() {
        for submissions in [0, 2] {
            let mut container = invalid(submissions);
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_container_children_mut(&mut container, |_| ());
            }))
            .expect_err("invalid mutable visitor submissions must panic");
            let message = panic_message(panic);
            assert!(message.contains("visit_children_mut"), "unexpected diagnostic: {message}");
            assert!(message.contains("exactly one Children collection"), "unexpected diagnostic: {message}");
        }
    }
}
