use crate::input::{ScrollBehavior, WidgetOption};
use crate::render::{CustomRenderKey, DisplayList, Painter};
use crate::widget_ctx::localize_events;
use crate::window_manager::{erased_widget_state, WidgetStateHandleDyn};
use crate::{Dimensioni, FrameResults, Input, KeyCode, KeyMode, MouseButton, Node, Recti, RetainedId, Style, Vec2i, WidgetHandle};

use super::{NodeLayout, TraversalState, UiNode, UiNodeId, UiNodeState, UiRuntime, WidgetCtx};

mod column;
mod disclosure;
mod grid;
mod row;
mod scroll_area;
mod stack;

pub(crate) use column::Column;
pub(crate) use disclosure::Disclosure;
pub(crate) use grid::Grid;
pub(crate) use row::Row;
pub(crate) use scroll_area::{scroll_viewport_node, scrollbar_nodes, shared_scroll_area_state, ScrollArea};
#[cfg(test)]
pub(crate) use scroll_area::{scroll_area_state, set_scroll_area_scroll, ScrollAreaState};
pub(crate) use stack::Stack;

/// Common internal behavior interface for retained nodes.
pub(crate) trait Widget {
    /// Returns whether the runtime owns an outer frame for this node.
    fn is_framed(&self) -> bool {
        false
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

/// Internal behavior interface for widgets that own child nodes.
pub(crate) trait Container: Widget {
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

/// Retained widget adapter behind the internal node behavior interface.
pub(crate) struct WidgetNode {
    /// Type-erased retained widget state.
    pub(crate) widget: Box<dyn WidgetStateHandleDyn>,
    /// Optional custom backend render callback for custom-render leaves.
    pub(crate) custom_render: Option<CustomRenderKey>,
}

impl Clone for WidgetNode {
    fn clone(&self) -> Self {
        Self {
            widget: self.widget.clone_box(),
            custom_render: self.custom_render.clone(),
        }
    }
}

impl Widget for WidgetNode {
    fn is_framed(&self) -> bool {
        self.widget.effective_widget_opt().intersects(WidgetOption::FRAME)
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
        let rect = ctx.node_rect(state);
        let (hovered, focused, clicked, active, scroll_delta) = ctx.runtime.interaction_for(
            id,
            rect,
            ctx.node_clip(),
            ctx.input,
            self.widget.effective_widget_opt(),
            self.widget.effective_scroll_behavior(),
            self.widget.focus_policy(),
        );
        state.hovered = hovered;
        state.focused = focused;
        state.clicked = clicked;
        state.active = active;
        state.scroll_delta = scroll_delta;

        let mut focus_seen = ctx.runtime.updated_focus;
        let events = localize_events(rect, ctx.runtime.take_routed_events(id));
        let accepts_pointer_input = ctx.runtime.accepts_pointer_input();
        let node_clip = ctx.content_clip();
        let mut widget_ctx = WidgetCtx::new_with_frame_geometry(
            id,
            rect,
            ctx.content_rect,
            &mut *ctx.display_list,
            node_clip,
            ctx.style,
            &ctx.atlas,
            &mut ctx.runtime.focus,
            &mut focus_seen,
            accepts_pointer_input,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        );
        let result = self.widget.update(&mut widget_ctx, events);
        ctx.runtime.updated_focus = focus_seen;

        ctx.results.record_retained_with_context(
            RetainedId::root_node(ctx.root_id, id),
            self.widget.widget_handle_id(),
            result,
            format!("root {:?} ui node {:?}", ctx.root_name, id),
        );
        false
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, state: &mut UiNodeState) -> bool {
        let id = state.id();
        let rect = ctx.node_content_rect(state);
        let (hovered, focused, clicked, active, scroll_delta) = (state.hovered, state.focused, state.clicked, state.active, state.scroll_delta);
        let mut focus_seen = ctx.runtime.updated_focus;
        let node_clip = ctx.node_clip();
        let mut widget_ctx = WidgetCtx::new_with_frame_geometry(
            id,
            ctx.frame_rect,
            rect,
            &mut *ctx.display_list,
            node_clip,
            ctx.style,
            &ctx.atlas,
            &mut ctx.runtime.focus,
            &mut focus_seen,
            true,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        );
        self.widget.paint(&mut widget_ctx);
        ctx.runtime.updated_focus = focus_seen;

        if let Some(renderer) = self.custom_render {
            ctx.display_list.push_custom(node_clip, renderer, rect);
        }
        false
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
        let rect = ctx.node_rect(state);
        route_public_widget_input(
            ctx,
            state,
            rect,
            self.widget.effective_widget_opt(),
            self.widget.effective_scroll_behavior(),
            event,
        )
    }
}

/// Input event routed to retained node behavior.
#[derive(Clone, Debug)]
pub enum UiInputEvent {
    /// Pointer moved without any mouse button held.
    MouseMove {
        /// Current pointer position in screen coordinates.
        pos: Vec2i,
        /// Pointer movement since the previous frame.
        delta: Vec2i,
    },
    /// Pointer moved while one or more mouse buttons are held.
    MouseDrag {
        /// Current pointer position in screen coordinates.
        pos: Vec2i,
        /// Pointer movement since the previous frame.
        delta: Vec2i,
        /// Mouse buttons held during the drag.
        buttons: MouseButton,
    },
    /// One or more mouse buttons were pressed.
    MouseDown {
        /// Current pointer position in screen coordinates.
        pos: Vec2i,
        /// Buttons pressed during this frame.
        button: MouseButton,
    },
    /// One or more mouse buttons were released.
    MouseUp {
        /// Current pointer position in screen coordinates.
        pos: Vec2i,
        /// Buttons released during this frame.
        button: MouseButton,
    },
    /// Scroll wheel or equivalent high-level scroll input.
    Scroll {
        /// Pointer position used for hit routing.
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

/// Result of routing one input event to a node behavior.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum InputResult {
    /// The node ignored the event.
    Ignored,
    /// The node consumed the event.
    Consumed,
    /// The node consumed the event and should keep receiving related pointer input.
    Captured,
}

impl InputResult {
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
        | UiInputEvent::KeyState { .. }
        | UiInputEvent::KeyUp { .. }
        | UiInputEvent::KeyCodeDown { .. }
        | UiInputEvent::KeyCodeState { .. }
        | UiInputEvent::KeyCodeUp { .. }
        | UiInputEvent::Text { .. } => None,
    }
}

pub(super) fn route_public_widget_input(
    ctx: &mut InputCtx<'_>,
    state: &UiNodeState,
    rect: Recti,
    opt: WidgetOption,
    scroll_behavior: ScrollBehavior,
    event: &UiInputEvent,
) -> InputResult {
    if opt.intersects(WidgetOption::NO_INTERACT) {
        return InputResult::Ignored;
    }

    let id = state.id();
    if event.is_focus_input() {
        ctx.runtime.push_routed_event(id, event.clone());
        return InputResult::Consumed;
    }

    let captured = ctx.runtime.capture == Some(id);
    let hovered = event_position(event)
        .map(|pos| rect.contains(&pos) && ctx.node_clip().contains(&pos))
        .unwrap_or(false);

    match event {
        UiInputEvent::MouseDown { .. } if hovered => {
            ctx.runtime.push_routed_event(id, event.clone());
            InputResult::Captured
        }
        UiInputEvent::MouseDrag { .. } if captured || state.focused || hovered => {
            ctx.runtime.push_routed_event(id, event.clone());
            if captured { InputResult::Captured } else { InputResult::Consumed }
        }
        UiInputEvent::MouseUp { .. } if captured || state.focused || hovered => {
            ctx.runtime.push_routed_event(id, event.clone());
            InputResult::Consumed
        }
        UiInputEvent::MouseMove { .. } if hovered => {
            ctx.runtime.push_routed_event(id, event.clone());
            InputResult::Consumed
        }
        UiInputEvent::Scroll { delta, .. } if hovered => {
            ctx.runtime.push_routed_event(id, event.clone());
            if scroll_behavior.is_grab_scroll() && (delta.x != 0 || delta.y != 0) {
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

    pub(crate) fn set_content_space_geometry(
        &mut self,
        state: &mut UiNodeState,
        _rect: Recti,
        viewport: Recti,
        virtual_size: Dimensioni,
        content_to_parent_translation: Vec2i,
    ) {
        let viewport = viewport
            .intersect(&self.content)
            .unwrap_or_else(|| Recti::new(self.content.x, self.content.y, 0, 0));
        let mut layout = NodeLayout::from_parts(self.outer, viewport, virtual_size, state.layout.content_size);
        layout.content.content_to_parent_translation = content_to_parent_translation;
        state.set_layout(layout);
    }
}

/// Services available while a container updates its own interactive state.
///
/// `UpdateCtx` may mutate runtime interaction state and frame results. Child topology is owned by
/// the window-manager/builder path and remains stable during runtime traversal.
pub(crate) struct UpdateCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) display_list: &'a mut DisplayList,
    pub(crate) root_id: crate::RootId,
    pub(crate) root_name: &'a str,
    pub(crate) style: &'a Style,
    pub(crate) atlas: crate::AtlasHandle,
    pub(crate) input: &'a Input,
    pub(crate) results: &'a mut FrameResults,
    pub(crate) frame_rect: Recti,
    pub(crate) content_rect: Recti,
    pub(crate) frame_clip: Recti,
    pub(crate) parent_traversal: TraversalState,
    pub(crate) traversal: TraversalState,
}

impl UpdateCtx<'_> {
    pub(crate) fn node_rect(&self, state: &UiNodeState) -> Recti {
        let _ = state;
        self.frame_rect
    }

    pub(crate) fn node_clip(&self) -> Recti {
        self.frame_clip
    }

    pub(crate) fn content_clip(&self) -> Recti {
        self.traversal.screen_clip
    }

    pub(crate) fn update_container_widget_in_rect(&mut self, state: &mut UiNodeState, local_rect: Recti, handle: WidgetHandle<Node>, label: &str) {
        let id = state.id();
        let rect = self.parent_traversal.screen_rect(local_rect);
        let widget = erased_widget_state(handle.clone());
        let opt = widget.effective_widget_opt();
        let content_rect = crate::frame::frame_geometry(rect, opt.intersects(WidgetOption::FRAME), self.style).content_or_empty();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let (hovered, focused, clicked, active, scroll_delta) =
            self.runtime
                .interaction_for(id, rect, self.node_clip(), self.input, opt, scroll_behavior, focus_policy);
        state.hovered = hovered;
        state.focused = focused;
        state.clicked = clicked;
        state.active = active;
        state.scroll_delta = scroll_delta;

        let mut focus_seen = self.runtime.updated_focus;
        let accepts_pointer_input = self.runtime.accepts_pointer_input();
        let events = localize_events(rect, self.runtime.take_routed_events(id));
        let node_clip = self.node_clip();
        let mut ctx = WidgetCtx::new_with_frame_geometry(
            id,
            rect,
            content_rect,
            &mut *self.display_list,
            node_clip,
            self.style,
            &self.atlas,
            &mut self.runtime.focus,
            &mut focus_seen,
            accepts_pointer_input,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        );
        let result = widget.update(&mut ctx, events);
        self.runtime.updated_focus = focus_seen;

        self.results
            .record_retained_with_context(RetainedId::root_node(self.root_id, id), handle.id(), result, format!("{label} {:?}", id));
    }
}

/// Services available while a node handles a routed input event.
///
/// Input routing may mutate node behavior state, but child topology is read-only.
pub(crate) struct InputCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) input: &'a Input,
    pub(crate) parent_traversal: TraversalState,
    pub(crate) traversal: TraversalState,
}

impl InputCtx<'_> {
    pub(crate) fn node_rect(&self, state: &UiNodeState) -> Recti {
        self.parent_traversal.screen_frame(state.layout)
    }

    pub(crate) fn node_clip_and_rect(&self, local_rect: Recti) -> (Recti, Recti) {
        (self.traversal.screen_clip, self.parent_traversal.screen_rect(local_rect))
    }

    pub(crate) fn node_clip(&self) -> Recti {
        self.parent_traversal.screen_clip
    }
}

/// Services available while a container paints its own surface.
///
/// Painting appends display-list operations under the traversal-derived clip; child topology is
/// read-only.
pub(crate) struct PaintCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) display_list: &'a mut DisplayList,
    pub(crate) style: &'a Style,
    pub(crate) atlas: crate::AtlasHandle,
    pub(crate) parent_traversal: TraversalState,
    pub(crate) frame_rect: Recti,
    pub(crate) content_rect: Recti,
    pub(crate) traversal: TraversalState,
}

impl PaintCtx<'_> {
    pub(crate) fn node_rect(&self, state: &UiNodeState) -> Recti {
        let _ = state;
        self.frame_rect
    }

    pub(crate) fn node_content_rect(&self, state: &UiNodeState) -> Recti {
        let _ = state;
        self.content_rect
    }

    pub(crate) fn node_clip(&self) -> Recti {
        self.traversal.screen_clip
    }

    pub(crate) fn draw_internal_frame(&mut self, rect: Recti, color: crate::ControlColor) -> Option<Recti> {
        let fill = self.style.colors[color as usize];
        let local_bounds = Recti::new(0, 0, i32::MAX, i32::MAX);
        let node_clip = self.node_clip();
        let mut painter = Painter::new(&mut *self.display_list, Vec2i::default(), local_bounds, node_clip);
        crate::frame::paint_internal_frame(&mut painter, rect, Some(fill), self.style.frame_border())
    }

    pub(crate) fn draw_flat_rect(&mut self, rect: Recti, color: crate::ControlColor) {
        let fill = self.style.colors[color as usize];
        let local_bounds = Recti::new(0, 0, i32::MAX, i32::MAX);
        let node_clip = self.node_clip();
        Painter::new(&mut *self.display_list, Vec2i::default(), local_bounds, node_clip).fill_rect(rect, fill);
    }

    pub(crate) fn paint_container_widget_in_rect(&mut self, state: &UiNodeState, local_rect: Recti, handle: WidgetHandle<Node>) {
        let id = state.id();
        let rect = self.parent_traversal.screen_rect(local_rect);
        let (hovered, focused, clicked, active, scroll_delta) = (state.hovered, state.focused, state.clicked, state.active, state.scroll_delta);
        let widget = erased_widget_state(handle);
        let framed = widget.effective_widget_opt().intersects(WidgetOption::FRAME);
        let geometry = crate::frame::frame_geometry(rect, framed, self.style);
        if framed {
            let local_bounds = Recti::new(0, 0, i32::MAX, i32::MAX);
            let node_clip = self.node_clip();
            let mut painter = Painter::new(&mut *self.display_list, Vec2i::default(), local_bounds, node_clip);
            crate::frame::paint_internal_frame(&mut painter, rect, None, self.style.frame_border());
        }
        let content_rect = geometry.content_or_empty();
        let mut focus_seen = self.runtime.updated_focus;
        let node_clip = self.node_clip();
        let mut ctx = WidgetCtx::new_with_frame_geometry(
            id,
            rect,
            content_rect,
            &mut *self.display_list,
            node_clip,
            self.style,
            &self.atlas,
            &mut self.runtime.focus,
            &mut focus_seen,
            true,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        );
        widget.paint(&mut ctx);
        self.runtime.updated_focus = focus_seen;
    }
}
