use crate::{Widget, WidgetOption, WidgetParameters, WidgetState, WidgetStateOwner};
use crate::render::{CustomRenderKey, DisplayList, Painter};
use crate::widget_ctx::{localize_events, WidgetPaintCtx, WidgetUpdateCtx};
use crate::{Dimensioni, FocusPolicy, FrameResults, Input, KeyCode, KeyMode, MouseButton, Recti, RetainedId, Style, Vec2i};

use super::{Children, Node, NodeLayout, UiNode, UiNodeId, UiNodeState, UiRuntime};

mod column;
mod disclosure;
mod grid;
mod row;
mod scroll_area;
mod stack;

pub use column::{Column, ColumnBuilder, ColumnContainer, ColumnParameters, ColumnState};
pub(crate) use column::LegacyColumn;
pub use disclosure::{Disclosure, DisclosureBuilder, DisclosureContainer, DisclosureParameters, DisclosureState};
pub use grid::{Grid, GridBuilder, GridContainer, GridItem, GridParameters, GridSpan, GridState};
pub(crate) use row::Row;
pub(crate) use scroll_area::{scroll_viewport_node, scrollbar_nodes, shared_scroll_area_state, ScrollArea};
#[cfg(test)]
pub(crate) use scroll_area::{scroll_area_state, set_scroll_area_scroll, ScrollAreaState};
pub use scroll_area::ScrollAreaOption;
pub(crate) use stack::Stack;

/// Internal runtime behavior for any retained node, including widget adapters and containers.
pub(crate) trait NodeBehavior {
    /// Reports whether this behavior is the erased public-widget adapter used by the old runtime.
    #[cfg(test)]
    fn debug_is_erased_widget_adapter(&self) -> bool {
        false
    }

    /// Returns whether the runtime owns an outer frame for this node.
    fn is_framed(&self) -> bool {
        false
    }

    /// Returns the standard public-widget interaction policy, when this behavior wraps one.
    fn interaction_config(&self) -> Option<(WidgetOption, FocusPolicy)> {
        None
    }

    /// Measures the preferred size for a node.
    fn measure(&self, ctx: &MeasureCtx<'_>, state: &UiNodeState, available: Dimensioni) -> Dimensioni;

    /// Assigns rectangles to the node and, for containers, its children.
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti);

    /// Updates this node and returns whether children should be traversed.
    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _state: &mut UiNodeState) -> bool {
        true
    }

    /// Paints this node and returns whether children should be painted.
    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _state: &mut UiNodeState) -> bool {
        true
    }

    /// Updates this node in response to one routed input event.
    fn update_on(&mut self, _ctx: &mut InputCtx<'_>, _state: &mut UiNodeState, _event: &UiInputEvent) -> InputResult {
        InputResult::Ignored
    }

    /// Returns scroll-area state for tests when this behavior owns it.
    #[cfg(test)]
    fn debug_scroll_area_state(&self) -> Option<ScrollAreaState> {
        None
    }

    /// Replaces scroll-area offset for tests when this behavior owns it.
    #[cfg(test)]
    fn debug_set_scroll_area_scroll(&mut self, _scroll: Vec2i) -> bool {
        false
    }
}

/// Temporary behavior interface for containers not yet migrated to state-owned [`Children`].
///
/// Row/Stack migrate in P2.0 and ScrollArea in P2.2; Grid already uses the public state-owned
/// contract. This trait is never public and is removed with the final legacy container.
pub(crate) trait LegacyContainer: NodeBehavior {
    /// Returns the owned child nodes.
    fn children(&self) -> &[UiNode];

    /// Returns the owned child nodes mutably.
    fn children_mut(&mut self) -> &mut Vec<UiNode>;

    /// Removes and returns an owned child node by id.
    fn remove_child(&mut self, child: UiNodeId) -> Option<UiNode> {
        let index = self.children().iter().position(|node| node.id() == child)?;
        Some(self.children_mut().remove(index))
    }
}

/// Marker for application-facing state owned by a concrete container runtime.
///
/// The marker deliberately grants no generic child access. Built-in state types expose only their
/// topology-safe inherent operations.
pub trait ContainerState: WidgetState {}

/// Associates one-shot parameters with one concrete state-owning container runtime.
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
/// visitor, install an extraction callback, or retain a child borrow after `visit` returns.
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
/// once from both visitor methods.
pub trait Container: Widget {
    /// Supplies the authoritative collection for immutable traversal.
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>);
    /// Supplies the same authoritative collection for mutable traversal.
    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>);

    /// Assigns child rectangles within this container's local content coordinates.
    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti);

    /// Reports whether descendants participate in traversal while remaining owned.
    fn children_visible(&self) -> bool {
        true
    }

    /// Routes one event to this container's own interactive surface.
    fn route_input(&mut self, ctx: &mut ContainerInputCtx<'_>, event: &UiInputEvent) -> ContainerInputResult {
        ctx.route_widget(event, self.effective_widget_opt(), self.focus_policy())
    }
}

/// Retained widget adapter behind the internal node behavior interface.
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
}

impl NodeBehavior for WidgetNode {
    fn is_framed(&self) -> bool {
        self.widget.effective_widget_opt().intersects(WidgetOption::FRAME)
    }

    fn interaction_config(&self) -> Option<(WidgetOption, FocusPolicy)> {
        Some((self.widget.effective_widget_opt(), self.widget.focus_policy()))
    }

    fn measure(&self, ctx: &MeasureCtx<'_>, state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        let _ = state;
        self.widget.measure(ctx.style, ctx.atlas, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        let preferred = self.widget.measure(ctx.style, ctx.atlas, Dimensioni::new(rect.width, rect.height));
        ctx.set_widget_content_size(state, preferred);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, state: &mut UiNodeState) -> bool {
        let id = state.id();
        let events = localize_events(ctx.content_rect, ctx.runtime.take_routed_events(id));
        let accepts_pointer_input = ctx.runtime.accepts_pointer_input();
        let content_rect = ctx.screen_rect(ctx.content_rect);
        let content_clip = ctx.screen_clip();
        let mut widget_ctx = WidgetUpdateCtx::new_with_content_geometry(
            content_rect,
            content_clip,
            ctx.style,
            &ctx.atlas,
            accepts_pointer_input,
            state.hovered,
            state.focused,
            state.clicked,
            state.active,
            state.scroll_delta,
        );
        let result = self.widget.update(&mut widget_ctx, events);

        let retained_id = RetainedId::root_node(ctx.root_id, id);
        let dispatch_site = format!("root {:?} ui node {:?}", ctx.root_name, id);
        ctx.results.record_direct_with_context(retained_id, result, dispatch_site);
        false
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, state: &mut UiNodeState) -> bool {
        let rect = ctx.screen_rect(ctx.content_rect);
        let (hovered, focused, clicked, active, scroll_delta) = (state.hovered, state.focused, state.clicked, state.active, state.scroll_delta);
        let content_clip = ctx.screen_clip();
        let mut widget_ctx = WidgetPaintCtx::new_with_content_geometry(
            rect,
            &mut *ctx.display_list,
            content_clip,
            ctx.style,
            &ctx.atlas,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        );
        self.widget.paint(&mut widget_ctx);

        if let Some(renderer) = self.custom_render {
            ctx.display_list.push_custom(content_clip, renderer, rect);
        }
        false
    }
}

/// Compile-safe P1.3 bridge from the final public container contract to the pre-P2.3 traversal
/// interface. It owns no state and creates no alternate application capability; P2.3 removes
/// `NodeBehavior` and moves this dispatch directly into `NodeKind` traversal.
impl NodeBehavior for dyn Container {
    fn is_framed(&self) -> bool {
        self.effective_widget_opt().intersects(WidgetOption::FRAME)
    }

    fn interaction_config(&self) -> Option<(WidgetOption, FocusPolicy)> {
        Some((self.effective_widget_opt(), self.focus_policy()))
    }

    fn measure(&self, ctx: &MeasureCtx<'_>, _state: &UiNodeState, available: Dimensioni) -> Dimensioni {
        Widget::measure(self, ctx.style, ctx.atlas, available)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, state: &mut UiNodeState, rect: Recti) {
        let mut public_ctx = ContainerLayoutCtx {
            runtime: &mut *ctx.runtime,
            style: ctx.style,
            atlas: ctx.atlas,
            content: ctx.content,
            current: state,
        };
        Container::layout(self, &mut public_ctx, rect);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, state: &mut UiNodeState) -> bool {
        let id = state.id();
        let events = localize_events(ctx.content_rect, ctx.runtime.take_routed_events(id));
        let accepts_pointer_input = ctx.runtime.accepts_pointer_input();
        let content_rect = ctx.screen_rect(ctx.content_rect);
        let content_clip = ctx.screen_clip();
        let mut widget_ctx = WidgetUpdateCtx::new_with_content_geometry(
            content_rect,
            content_clip,
            ctx.style,
            &ctx.atlas,
            accepts_pointer_input,
            state.hovered,
            state.focused,
            state.clicked,
            state.active,
            state.scroll_delta,
        );
        let result = Widget::update(self, &mut widget_ctx, events);
        ctx.results.record_direct_with_context(
            RetainedId::root_node(ctx.root_id, id),
            result,
            format!("root {:?} container node {:?}", ctx.root_name, id),
        );
        Container::children_visible(self)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, state: &mut UiNodeState) -> bool {
        let rect = ctx.screen_rect(ctx.content_rect);
        let content_clip = ctx.screen_clip();
        let mut widget_ctx = WidgetPaintCtx::new_with_content_geometry(
            rect,
            &mut *ctx.display_list,
            content_clip,
            ctx.style,
            &ctx.atlas,
            state.hovered,
            state.focused,
            state.clicked,
            state.active,
            state.scroll_delta,
        );
        Widget::paint(self, &mut widget_ctx);
        Container::children_visible(self)
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        let mut public_ctx = ContainerInputCtx {
            runtime: &mut *ctx.runtime,
            content_rect: ctx.content_rect,
            content_clip: ctx.content_clip,
            current: state,
        };
        Container::route_input(self, &mut public_ctx, event)
    }
}

/// Input event routed to retained node behavior.
#[derive(Clone, Debug)]
pub enum UiInputEvent {
    /// Pointer moved without any mouse button held.
    MouseMove {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Pointer movement since the previous frame.
        delta: Vec2i,
    },
    /// Pointer moved while one or more mouse buttons are held.
    MouseDrag {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Pointer movement since the previous frame.
        delta: Vec2i,
        /// Mouse buttons held during the drag.
        buttons: MouseButton,
    },
    /// One or more mouse buttons were pressed.
    MouseDown {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Buttons pressed during this frame.
        button: MouseButton,
    },
    /// One or more mouse buttons were released.
    MouseUp {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Buttons released during this frame.
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
        /// Modifier/control key bits pressed during this frame.
        key: KeyMode,
    },
    /// Current modifier/control key state for this frame.
    KeyState {
        /// Modifier/control keys currently held.
        keys: KeyMode,
    },
    /// Modifier/control key state was released.
    KeyUp {
        /// Modifier/control key bits released during this frame.
        key: KeyMode,
    },
    /// Navigation key state was pressed.
    KeyCodeDown {
        /// Navigation key bits pressed during this frame.
        code: KeyCode,
    },
    /// Current navigation key state for this frame.
    KeyCodeState {
        /// Navigation keys currently held.
        codes: KeyCode,
    },
    /// Navigation key state was released.
    KeyCodeUp {
        /// Navigation key bits released during this frame.
        code: KeyCode,
    },
    /// UTF-8 text input collected during this frame.
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
            Self::KeyDown { .. }
                | Self::KeyState { .. }
                | Self::KeyUp { .. }
                | Self::KeyCodeDown { .. }
                | Self::KeyCodeState { .. }
                | Self::KeyCodeUp { .. }
                | Self::Text { .. }
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

/// Transitional internal name used by the pre-P2.5 routing implementation.
pub(crate) type InputResult = ContainerInputResult;

fn event_position(event: &UiInputEvent) -> Option<Vec2i> {
    match event {
        UiInputEvent::MouseMove { pos, .. }
        | UiInputEvent::MouseDrag { pos, .. }
        | UiInputEvent::MouseDown { pos, .. }
        | UiInputEvent::MouseUp { pos, .. }
        | UiInputEvent::Scroll { pos, .. } => Some(*pos),
        UiInputEvent::KeyDown { .. }
        | UiInputEvent::KeyState { .. }
        | UiInputEvent::KeyUp { .. }
        | UiInputEvent::KeyCodeDown { .. }
        | UiInputEvent::KeyCodeState { .. }
        | UiInputEvent::KeyCodeUp { .. }
        | UiInputEvent::Text { .. } => None,
    }
}

pub(super) fn route_public_widget_input(
    runtime: &mut UiRuntime,
    state: &UiNodeState,
    rect: Recti,
    clip: Recti,
    opt: WidgetOption,
    event: &UiInputEvent,
) -> InputResult {
    if opt.intersects(WidgetOption::NO_INTERACT) {
        return InputResult::Ignored;
    }

    let id = state.id();
    if event.is_focus_input() {
        // Enforce the router invariant at the final delivery boundary as well as at target lookup.
        if runtime.focus != Some(id) {
            return InputResult::Ignored;
        }
        runtime.push_routed_event(id, event.clone());
        return InputResult::Consumed;
    }

    let captured = runtime.capture == Some(id);
    let hovered = event_position(event).map(|pos| rect.contains(&pos) && clip.contains(&pos)).unwrap_or(false);

    match event {
        UiInputEvent::MouseDown { .. } if hovered => {
            runtime.push_routed_event(id, event.clone());
            InputResult::Captured
        }
        UiInputEvent::MouseDrag { .. } if captured || state.focused || hovered => {
            runtime.push_routed_event(id, event.clone());
            if captured { InputResult::Captured } else { InputResult::Consumed }
        }
        UiInputEvent::MouseUp { .. } if captured || state.focused || hovered => {
            runtime.push_routed_event(id, event.clone());
            InputResult::Consumed
        }
        UiInputEvent::MouseMove { .. } if hovered => {
            runtime.push_routed_event(id, event.clone());
            InputResult::Consumed
        }
        UiInputEvent::Scroll { delta, .. } if hovered => {
            runtime.push_routed_event(id, event.clone());
            if opt.intersects(WidgetOption::GRAB_SCROLL) && (delta.x != 0 || delta.y != 0) {
                InputResult::Consumed
            } else {
                InputResult::Ignored
            }
        }
        _ => InputResult::Ignored,
    }
}

/// Read-only services available while a container measures itself.
///
/// Measurement must not mutate child topology.
pub(crate) struct MeasureCtx<'a> {
    pub(crate) runtime: &'a UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) atlas: &'a crate::AtlasHandle,
}

impl MeasureCtx<'_> {
    pub(crate) fn measure_node_ref(&self, node: &UiNode, available: Dimensioni) -> Dimensioni {
        self.runtime.measure_node_ref(node, self.style, self.atlas, available)
    }
}

/// Mutable geometry services available while a container lays out its children.
///
/// Layout may update rectangles and content sizes, but child topology is read-only.
pub(crate) struct LayoutCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) atlas: &'a crate::AtlasHandle,
    pub(crate) outer: Recti,
    pub(crate) content: Recti,
    pub(crate) border_width: i32,
}

impl LayoutCtx<'_> {
    pub(crate) fn measure_node_ref(&self, node: &UiNode, available: Dimensioni) -> Dimensioni {
        self.runtime.measure_node_ref(node, self.style, self.atlas, available)
    }

    pub(crate) fn layout_node_ref(&mut self, node: &mut UiNode, rect: Recti) -> Dimensioni {
        self.runtime.layout_node_ref(node, self.style, self.atlas, rect)
    }

    pub(crate) fn set_content_size(&mut self, state: &mut UiNodeState, content_size: Dimensioni) {
        let layout = state.layout.with_content_size(content_size);
        state.set_layout(layout);
    }

    pub(crate) fn set_widget_content_size(&mut self, state: &mut UiNodeState, preferred_content: Dimensioni) {
        let preferred_outer = crate::frame::outer_preferred(preferred_content, self.border_width);
        self.set_content_size(
            state,
            Dimensioni::new(
                self.outer.width.max(preferred_outer.width).max(0),
                self.outer.height.max(preferred_outer.height).max(0),
            ),
        );
    }

    pub(crate) fn set_child_overflow_propagation(&mut self, state: &mut UiNodeState, propagate_child_overflow: bool) {
        let layout = state.layout.with_child_overflow_propagation(propagate_child_overflow);
        state.set_layout(layout);
    }

    pub(crate) fn set_content_space_geometry(&mut self, state: &mut UiNodeState, _rect: Recti, viewport: Recti, child_offset: Vec2i) {
        let viewport = viewport
            .intersect(&self.content)
            .unwrap_or_else(|| Recti::new(self.content.x, self.content.y, 0, 0));
        let mut layout = NodeLayout::from_parts(self.outer, viewport, state.layout.content_size);
        layout.children.offset = child_offset;
        state.set_layout(layout);
    }
}

/// Framework-scoped geometry services available to a public [`Container`] implementation.
///
/// The fields and constructor are private so application code cannot use this context to traverse
/// arbitrary trees or mutate attached topology outside the active container call.
pub struct ContainerLayoutCtx<'a> {
    runtime: &'a mut UiRuntime,
    style: &'a Style,
    atlas: &'a crate::AtlasHandle,
    content: Recti,
    current: &'a mut UiNodeState,
}

impl ContainerLayoutCtx<'_> {
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

/// Services available while a container updates its own interactive state.
///
/// `UpdateCtx` may mutate runtime interaction state and frame results. Child topology is owned by
/// the window-manager/builder path and remains stable during runtime traversal. It intentionally
/// contains no display list, making the update traversal structurally unable to record paint work.
pub(crate) struct UpdateCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(super) root_id: crate::RootId,
    pub(super) root_name: &'a str,
    pub(super) style: &'a Style,
    pub(super) atlas: crate::AtlasHandle,
    pub(super) input: &'a Input,
    pub(super) results: &'a mut FrameResults,
    /// Current node origin in screen coordinates, used only by context adapters.
    pub(super) screen_origin: Vec2i,
    /// Content surface in node-local coordinates.
    pub(super) content_rect: Recti,
    /// Effective content clip in node-local coordinates.
    pub(super) content_clip: Recti,
}

impl UpdateCtx<'_> {
    fn screen_rect(&self, local_rect: Recti) -> Recti {
        Recti::new(
            self.screen_origin.x + local_rect.x,
            self.screen_origin.y + local_rect.y,
            local_rect.width,
            local_rect.height,
        )
    }

    fn screen_clip(&self) -> Recti {
        self.screen_rect(self.content_clip)
    }
}

/// Services available while a node handles a routed input event.
///
/// Input routing may mutate node behavior state, but child topology is read-only.
pub(crate) struct InputCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) style: &'a Style,
    /// Content surface in node-local coordinates.
    pub(super) content_rect: Recti,
    /// Effective content clip in node-local coordinates.
    pub(super) content_clip: Recti,
}

impl InputCtx<'_> {
    pub(crate) fn content_rect(&self) -> Recti {
        self.content_rect
    }

    pub(crate) fn contains(&self, rect: Recti, pos: Vec2i) -> bool {
        rect.contains(&pos) && self.content_clip.contains(&pos)
    }

    pub(crate) fn route_widget_input(&mut self, state: &UiNodeState, rect: Recti, opt: WidgetOption, event: &UiInputEvent) -> InputResult {
        route_public_widget_input(self.runtime, state, rect, self.content_clip, opt, event)
    }
}

/// Framework-scoped routed-input services for one public [`Container`] call.
pub struct ContainerInputCtx<'a> {
    runtime: &'a mut UiRuntime,
    content_rect: Recti,
    content_clip: Recti,
    current: &'a UiNodeState,
}

impl ContainerInputCtx<'_> {
    /// Routes through the container's complete local content rectangle.
    pub fn route_widget(&mut self, event: &UiInputEvent, opt: WidgetOption, _focus: FocusPolicy) -> ContainerInputResult {
        route_public_widget_input(self.runtime, self.current, self.content_rect, self.content_clip, opt, event)
    }

    /// Routes through one container-local sub-rectangle intersected with the active clip.
    pub fn route_widget_in_rect(&mut self, event: &UiInputEvent, rect: Recti, opt: WidgetOption, _focus: FocusPolicy) -> ContainerInputResult {
        route_public_widget_input(self.runtime, self.current, rect, self.content_clip, opt, event)
    }
}

/// Services available while a container paints its own surface.
///
/// Painting appends display-list operations under the traversal-derived clip; child topology is
/// read-only.
pub(crate) struct PaintCtx<'a> {
    pub(super) display_list: &'a mut DisplayList,
    pub(crate) style: &'a Style,
    pub(super) atlas: crate::AtlasHandle,
    /// Current node origin in screen coordinates, used only by painting adapters.
    pub(super) screen_origin: Vec2i,
    /// Content surface in node-local coordinates.
    pub(super) content_rect: Recti,
    /// Effective content clip in node-local coordinates.
    pub(super) content_clip: Recti,
}

impl PaintCtx<'_> {
    pub(crate) fn content_rect(&self) -> Recti {
        self.content_rect
    }

    fn screen_rect(&self, local_rect: Recti) -> Recti {
        Recti::new(
            self.screen_origin.x + local_rect.x,
            self.screen_origin.y + local_rect.y,
            local_rect.width,
            local_rect.height,
        )
    }

    fn screen_clip(&self) -> Recti {
        self.screen_rect(self.content_clip)
    }

    fn painter(&mut self) -> Painter<'_> {
        let screen_clip = self.screen_clip();
        Painter::screen_space(&mut *self.display_list, screen_clip)
    }

    pub(crate) fn draw_internal_frame(&mut self, rect: Recti, color: crate::ControlColor) -> Option<Recti> {
        let screen_origin = self.screen_origin;
        let rect = self.screen_rect(rect);
        let fill = self.style.colors[color as usize];
        let border = self.style.frame_border();
        let mut painter = self.painter();
        crate::frame::paint_internal_frame(&mut painter, rect, Some(fill), border)
            .map(|content| Recti::new(content.x - screen_origin.x, content.y - screen_origin.y, content.width, content.height))
    }

    pub(crate) fn draw_flat_rect(&mut self, rect: Recti, color: crate::ControlColor) {
        let rect = self.screen_rect(rect);
        let fill = self.style.colors[color as usize];
        self.painter().fill_rect(rect, fill);
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

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> crate::ResourceState {
            crate::ResourceState::NONE
        }

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
