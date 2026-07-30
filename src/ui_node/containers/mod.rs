use crate::WidgetOption;
use crate::render::{CustomRenderKey, DisplayList, Painter};
use crate::widget_ctx::{localize_events, WidgetPaintCtx, WidgetUpdateCtx};
use crate::window_manager::{erased_widget_state, WidgetStateHandleDyn};
use crate::{Dimensioni, FocusPolicy, FrameResults, Input, KeyCode, KeyMode, MouseButton, Node, Recti, RetainedId, Style, Vec2i, WidgetHandle};

use super::{NodeLayout, UiNode, UiNodeId, UiNodeState, UiRuntime};

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

/// Internal behavior interface for widgets that own child nodes.
pub(crate) trait Container: NodeBehavior {
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

impl NodeBehavior for WidgetNode {
    #[cfg(test)]
    fn debug_is_erased_widget_adapter(&self) -> bool {
        true
    }

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
        let mut focus_seen = ctx.runtime.updated_focus;
        let events = localize_events(ctx.content_rect, ctx.runtime.take_routed_events(id));
        let accepts_pointer_input = ctx.runtime.accepts_pointer_input();
        let content_rect = ctx.screen_rect(ctx.content_rect);
        let content_clip = ctx.screen_clip();
        let mut widget_ctx = WidgetUpdateCtx::new_with_content_geometry(
            id,
            content_rect,
            content_clip,
            ctx.style,
            &ctx.atlas,
            &mut ctx.runtime.focus,
            &mut focus_seen,
            accepts_pointer_input,
            state.hovered,
            state.focused,
            state.clicked,
            state.active,
            state.scroll_delta,
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

    pub(crate) fn update_container_widget_in_rect(&mut self, state: &mut UiNodeState, local_rect: Recti, handle: WidgetHandle<Node>, label: &str) {
        let id = state.id();
        let rect = self.screen_rect(local_rect);
        let widget = erased_widget_state(handle.clone());
        let opt = widget.effective_widget_opt();
        let local_content_rect = crate::frame::frame_geometry(local_rect, opt.intersects(WidgetOption::FRAME), self.style).content_or_empty();
        let content_rect = self.screen_rect(local_content_rect);
        let focus_policy = widget.focus_policy();
        let content_clip = self.screen_clip();
        let (hovered, focused, clicked, active, scroll_delta) = self.runtime.interaction_for(id, rect, content_clip, self.input, opt, focus_policy);
        state.hovered = hovered;
        state.focused = focused;
        state.clicked = clicked;
        state.active = active;
        state.scroll_delta = scroll_delta;

        let mut focus_seen = self.runtime.updated_focus;
        let accepts_pointer_input = self.runtime.accepts_pointer_input();
        let events = localize_events(local_content_rect, self.runtime.take_routed_events(id));
        let mut ctx = WidgetUpdateCtx::new_with_content_geometry(
            id,
            content_rect,
            content_clip,
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

    pub(crate) fn paint_container_widget_in_rect(&mut self, state: &UiNodeState, local_rect: Recti, handle: WidgetHandle<Node>) {
        let (hovered, focused, clicked, active, scroll_delta) = (state.hovered, state.focused, state.clicked, state.active, state.scroll_delta);
        let widget = erased_widget_state(handle);
        let framed = widget.effective_widget_opt().intersects(WidgetOption::FRAME);
        let geometry = crate::frame::frame_geometry(local_rect, framed, self.style);
        if framed {
            let screen_rect = self.screen_rect(local_rect);
            let border = self.style.frame_border();
            let mut painter = self.painter();
            crate::frame::paint_internal_frame(&mut painter, screen_rect, None, border);
        }
        let content_rect = self.screen_rect(geometry.content_or_empty());
        let content_clip = self.screen_clip();
        let mut ctx = WidgetPaintCtx::new_with_content_geometry(
            content_rect,
            &mut *self.display_list,
            content_clip,
            self.style,
            &self.atlas,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
        );
        widget.paint(&mut ctx);
    }
}
