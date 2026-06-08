use crate::window_manager::{erased_widget_state, TreeCustomRender, WidgetStateHandleDyn};
use crate::sizing::SizePolicy;
use crate::{CustomRenderArgs, Dimensioni, FrameResults, Input, KeyCode, KeyMode, MouseButton, Node, Recti, RetainedId, Style, Vec2i, WidgetHandle};

use super::{
    custom_render_events_from_input, events_key_codes, events_key_mods, events_text, frame_events_from_input, measure_axis_available, resolve_allocated_size,
    resolve_size, retained_focus_to_node, NodeCustomRenderCommand, NodeLayout, TraversalState, UiNode, UiNodeId, UiRuntime, WidgetCtx,
};
use crate::render_command::Command;

mod column;
mod disclosure;
mod grid;
mod root_window;
mod row;
mod scroll_area;
mod stack;

pub(crate) use column::Column;
pub(crate) use disclosure::Disclosure;
pub(crate) use grid::Grid;
pub(crate) use root_window::RootWindow;
pub(crate) use row::Row;
pub(crate) use scroll_area::{scroll_viewport_node, scrollbar_nodes, shared_scroll_area_state, ScrollArea};
#[cfg(test)]
pub(crate) use scroll_area::ScrollAreaState;
pub(crate) use stack::Stack;

/// Clone support for boxed node behavior objects.
pub(crate) trait NodeBehaviorClone {
    /// Clones this behavior into a boxed trait object.
    fn clone_box(&self) -> Box<dyn NodeBehavior>;
}

impl<T> NodeBehaviorClone for T
where
    T: NodeBehavior + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn NodeBehavior> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn NodeBehavior> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Common internal behavior interface for retained nodes.
pub(crate) trait NodeBehavior: NodeBehaviorClone {
    /// Measures the preferred size for a node.
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni;

    /// Assigns rectangles to the node and, for containers, its children.
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti);

    /// Updates this node and returns whether children should be traversed.
    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _id: UiNodeId) -> bool {
        true
    }

    /// Paints this node and returns whether children should be painted.
    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _id: UiNodeId) -> bool {
        true
    }

    /// Updates this node in response to one routed input event.
    fn update_on(&mut self, _ctx: &mut InputCtx<'_>, _id: UiNodeId, _event: &UiInputEvent) -> InputResult {
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

/// Retained widget adapter behind the internal node behavior interface.
pub(crate) struct WidgetNode {
    /// Type-erased retained widget state.
    pub(crate) widget: Box<dyn WidgetStateHandleDyn>,
    /// Optional custom backend render callback for custom-render leaves.
    pub(crate) custom_render: Option<TreeCustomRender>,
    /// Focus-owned one-frame input events routed to this widget.
    pub(crate) pending_events: Vec<UiInputEvent>,
}

impl Clone for WidgetNode {
    fn clone(&self) -> Self {
        Self {
            widget: self.widget.clone_box(),
            custom_render: self.custom_render.clone(),
            pending_events: self.pending_events.clone(),
        }
    }
}

impl NodeBehavior for WidgetNode {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        let policy = ctx.runtime.nodes.get(&id).map(|node| node.policy).unwrap_or_else(crate::Policy::auto);
        let measure_available = Dimensioni::new(
            measure_axis_available(policy.width, available.width),
            measure_axis_available(policy.height, available.height),
        );
        let preferred = self.widget.measure(ctx.style, ctx.atlas, measure_available);
        Dimensioni::new(
            resolve_size(policy.width, preferred.width, available.width, available.width, None),
            resolve_size(policy.height, preferred.height, available.height, available.height, None),
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let policy = ctx.runtime.nodes.get(&id).map(|node| node.policy).unwrap_or_else(crate::Policy::auto);
        let measure_available = Dimensioni::new(
            measure_axis_available(policy.width, rect.width),
            measure_axis_available(policy.height, rect.height),
        );
        let preferred = self.widget.measure(ctx.style, ctx.atlas, measure_available);
        let rect = Recti::new(
            rect.x,
            rect.y,
            resolve_allocated_size(policy.width, preferred.width, rect.width, rect.width, None),
            resolve_allocated_size(policy.height, preferred.height, rect.height, rect.height, None),
        );
        let content_size = Dimensioni::new(rect.width.max(preferred.width), rect.height.max(preferred.height));
        if let Some(node) = ctx.runtime.nodes.get_mut(&id) {
            node.set_layout_from_rect(rect, clip, content_size);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, id: UiNodeId) -> bool {
        let rect = ctx.node_rect(id).unwrap_or_default();
        let (hovered, focused, clicked, active, scroll_delta) = ctx.runtime.interaction_for(
            id,
            rect,
            ctx.node_clip(),
            ctx.input,
            self.widget.effective_widget_opt(),
            self.widget.effective_scroll_behavior(),
            self.widget.focus_policy(),
        );
        if let Some(node) = ctx.runtime.nodes.get_mut(&id) {
            node.hovered = hovered;
            node.focused = focused;
            node.clicked = clicked;
            node.active = active;
            node.scroll_delta = scroll_delta;
        }

        let mut focus_slot = ctx.runtime.focus.map(RetainedId::node);
        let mut focus_seen = ctx.runtime.updated_focus;
        let mut events = frame_events_from_input(ctx.input);
        events.extend(self.pending_events.iter().cloned());
        let mut widget_ctx = WidgetCtx::new_with_interaction(
            RetainedId::node(id),
            rect,
            &mut ctx.runtime.commands,
            &mut ctx.runtime.triangle_vertices,
            &mut ctx.runtime.clip_stack,
            ctx.style,
            &ctx.atlas,
            &mut focus_slot,
            &mut focus_seen,
            ctx.runtime.hover_root_active,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
            events,
        );
        let result = self.widget.update(&mut widget_ctx);
        self.pending_events.clear();
        ctx.runtime.focus = retained_focus_to_node(focus_slot);
        ctx.runtime.updated_focus = focus_seen;

        ctx.results.record_retained_with_context(
            RetainedId::root_node(ctx.root_id, id),
            self.widget.widget_handle_id(),
            result,
            format!("root {:?} ui node {:?}", ctx.root_name, id),
        );
        false
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, id: UiNodeId) -> bool {
        let rect = ctx.node_rect(id).unwrap_or_default();
        let (hovered, focused, clicked, active, scroll_delta) = ctx
            .runtime
            .nodes
            .get(&id)
            .map(|node| (node.hovered, node.focused, node.clicked, node.active, node.scroll_delta))
            .unwrap_or((false, false, false, false, None));
        let mut focus_slot = ctx.runtime.focus.map(RetainedId::node);
        let mut focus_seen = ctx.runtime.updated_focus;
        let events = frame_events_from_input(ctx.input);
        let node_clip = ctx.node_clip();
        ctx.push_node_clip(id);
        let mut widget_ctx = WidgetCtx::new_with_interaction(
            RetainedId::node(id),
            rect,
            &mut ctx.runtime.commands,
            &mut ctx.runtime.triangle_vertices,
            &mut ctx.runtime.clip_stack,
            ctx.style,
            &ctx.atlas,
            &mut focus_slot,
            &mut focus_seen,
            true,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
            events,
        );
        self.widget.paint(&mut widget_ctx);
        ctx.pop_node_clip();
        ctx.runtime.focus = retained_focus_to_node(focus_slot);
        ctx.runtime.updated_focus = focus_seen;

        if let Some(render) = self.custom_render.clone() {
            let events = custom_render_events_from_input(ctx.input, focused);
            let input_events = WidgetCtx::localize_events(rect, events.clone());
            let view = node_clip.intersect(&rect).unwrap_or_else(|| Recti::new(rect.x, rect.y, 0, 0));
            let cra = CustomRenderArgs {
                content_area: rect,
                view,
                input_events,
                scroll_delta,
                widget_opt: self.widget.effective_widget_opt(),
                scroll_behavior: self.widget.effective_scroll_behavior(),
                key_mods: if focused { events_key_mods(&events) } else { KeyMode::NONE },
                key_codes: if focused { events_key_codes(&events) } else { KeyCode::NONE },
                text_input: if focused { events_text(&events) } else { String::new() },
            };
            ctx.runtime
                .commands
                .push(Command::BackendCustomRender(cra, Box::new(NodeCustomRenderCommand { render })));
        }
        false
    }

    fn update_on(&mut self, ctx: &mut InputCtx<'_>, id: UiNodeId, event: &UiInputEvent) -> InputResult {
        if event.is_focus_input() {
            self.pending_events.push(event.clone());
            return InputResult::Consumed;
        }
        let UiInputEvent::Scroll { pos, delta } = event else {
            return InputResult::Ignored;
        };
        let rect = ctx.node_rect(id).unwrap_or_default();
        let clip = ctx.node_clip();
        let hovered = rect.contains(&pos) && clip.contains(&pos);
        if hovered && self.widget.effective_scroll_behavior().is_grab_scroll() && (delta.x != 0 || delta.y != 0) {
            InputResult::Consumed
        } else {
            InputResult::Ignored
        }
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
            Self::KeyDown { .. } | Self::KeyUp { .. } | Self::KeyCodeDown { .. } | Self::KeyCodeUp { .. } | Self::Text { .. }
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

/// Read-only services available while a container measures itself.
///
/// Measurement must not mutate child topology.
pub(crate) struct MeasureCtx<'a> {
    pub(crate) runtime: &'a UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) atlas: &'a crate::AtlasHandle,
}

impl MeasureCtx<'_> {
    pub(crate) fn child_count(&self, id: UiNodeId) -> usize {
        self.runtime.child_count(id)
    }

    pub(crate) fn child_at(&self, id: UiNodeId, index: usize) -> Option<UiNodeId> {
        self.runtime.child_at(id, index)
    }

    pub(crate) fn measure_node(&self, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        self.runtime.measure_node(id, self.style, self.atlas, available)
    }
}

/// Mutable geometry services available while a container lays out its children.
///
/// Layout may update rectangles, clips, and content sizes, but child topology is read-only.
pub(crate) struct LayoutCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) atlas: &'a crate::AtlasHandle,
}

impl LayoutCtx<'_> {
    pub(crate) fn child_count(&self, id: UiNodeId) -> usize {
        self.runtime.child_count(id)
    }

    pub(crate) fn child_at(&self, id: UiNodeId, index: usize) -> Option<UiNodeId> {
        self.runtime.child_at(id, index)
    }

    pub(crate) fn measure_node(&self, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        self.runtime.measure_node(id, self.style, self.atlas, available)
    }

    pub(crate) fn layout_node(&mut self, id: UiNodeId, rect: Recti, clip: Recti) -> Dimensioni {
        self.runtime.layout_node(id, self.style, self.atlas, rect, clip)
    }

    pub(crate) fn vertical_child_policy(&self, child: UiNodeId) -> SizePolicy {
        self.runtime.vertical_child_policy(child)
    }

    pub(crate) fn horizontal_track_policy(&self, child: UiNodeId, track: SizePolicy) -> SizePolicy {
        self.runtime.horizontal_track_policy(child, track)
    }

    pub(crate) fn child_content_bounds(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.child_content_bounds(id)
    }

    pub(crate) fn set_client(&mut self, id: UiNodeId, client: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            node.set_layout(node.layout.with_control(client));
        }
    }

    pub(crate) fn set_content_size(&mut self, id: UiNodeId, content_size: Dimensioni) {
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            let layout = node.layout.with_content_size(content_size);
            node.set_layout(layout);
        }
    }

    pub(crate) fn set_child_overflow_propagation(&mut self, id: UiNodeId, propagate_child_overflow: bool) {
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            let layout = node.layout.with_child_overflow_propagation(propagate_child_overflow);
            node.set_layout(layout);
        }
    }

    pub(crate) fn set_content_space_geometry(
        &mut self,
        id: UiNodeId,
        rect: Recti,
        control: Recti,
        viewport: Recti,
        parent_clip: Recti,
        virtual_size: Dimensioni,
        content_to_parent_translation: Vec2i,
    ) {
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            let mut layout = NodeLayout::from_parts(rect, control, viewport, parent_clip, virtual_size, node.layout.content_size);
            layout.content.content_to_parent_translation = content_to_parent_translation;
            node.set_layout(layout);
        }
    }

    pub(crate) fn grid_span(&self, id: UiNodeId) -> crate::GridSpan {
        self.runtime.nodes.get(&id).map(|node| node.grid_span).unwrap_or(crate::GridSpan::ONE)
    }
}

/// Services available while a container updates its own interactive state.
///
/// `UpdateCtx` is the only traversal context allowed to mutate topology. It supports immediate
/// add-new-child and remove-child operations; reparenting is intentionally unsupported so a child
/// can have only one parent for its lifetime.
pub(crate) struct UpdateCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) root_id: crate::RootId,
    pub(crate) root_name: &'a str,
    pub(crate) style: &'a Style,
    pub(crate) atlas: crate::AtlasHandle,
    pub(crate) input: &'a Input,
    pub(crate) results: &'a mut FrameResults,
    pub(crate) traversal: TraversalState,
}

impl UpdateCtx<'_> {
    /// Inserts `child` under `parent` immediately and returns its id.
    ///
    /// The child must be new to this runtime and must not already have a parent. The updated tree is
    /// visible to the same frame's post-update layout and paint passes.
    pub(crate) fn add_child(&mut self, parent: UiNodeId, child: UiNode, index: usize) -> Option<UiNodeId> {
        self.runtime.insert_child_immediate(parent, child, index)
    }

    /// Removes a direct child and its subtree immediately.
    pub(crate) fn remove_child(&mut self, parent: UiNodeId, child: UiNodeId) -> bool {
        self.runtime.remove_child_immediate(parent, child)
    }

    pub(crate) fn node_rect(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.nodes.get(&id).map(|node| self.traversal.screen_frame(node.layout))
    }

    pub(crate) fn node_control(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.nodes.get(&id).map(|node| self.traversal.screen_rect(node.layout.control))
    }

    pub(crate) fn node_clip(&self) -> Recti {
        self.traversal.screen_clip
    }

    pub(crate) fn update_container_widget(&mut self, id: UiNodeId, handle: WidgetHandle<Node>, label: &str) {
        let rect = self.node_control(id).unwrap_or_default();
        let widget = erased_widget_state(handle.clone());
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let (hovered, focused, clicked, active, scroll_delta) =
            self.runtime
                .interaction_for(id, rect, self.node_clip(), self.input, opt, scroll_behavior, focus_policy);
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            node.hovered = hovered;
            node.focused = focused;
            node.clicked = clicked;
            node.active = active;
            node.scroll_delta = scroll_delta;
        }

        let mut focus_slot = self.runtime.focus.map(RetainedId::node);
        let mut focus_seen = self.runtime.updated_focus;
        let mut ctx = WidgetCtx::new_with_interaction(
            RetainedId::node(id),
            rect,
            &mut self.runtime.commands,
            &mut self.runtime.triangle_vertices,
            &mut self.runtime.clip_stack,
            self.style,
            &self.atlas,
            &mut focus_slot,
            &mut focus_seen,
            self.runtime.hover_root_active,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
            frame_events_from_input(self.input),
        );
        let result = widget.update(&mut ctx);
        self.runtime.focus = retained_focus_to_node(focus_slot);
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
    pub(crate) traversal: TraversalState,
}

impl InputCtx<'_> {
    pub(crate) fn node_rect(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.nodes.get(&id).map(|node| self.traversal.screen_frame(node.layout))
    }

    pub(crate) fn node_clip_and_control(&self, id: UiNodeId) -> Option<(Recti, Recti)> {
        self.runtime
            .nodes
            .get(&id)
            .map(|node| (self.traversal.screen_clip, self.traversal.screen_rect(node.layout.control)))
    }

    pub(crate) fn node_clip(&self) -> Recti {
        self.traversal.screen_clip
    }
}

/// Services available while a container paints its own surface.
///
/// Painting may record commands and clip changes, but child topology is read-only.
pub(crate) struct PaintCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) atlas: crate::AtlasHandle,
    pub(crate) input: &'a Input,
    pub(crate) traversal: TraversalState,
}

impl PaintCtx<'_> {
    pub(crate) fn node_rect(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.nodes.get(&id).map(|node| self.traversal.screen_frame(node.layout))
    }

    pub(crate) fn node_control(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.nodes.get(&id).map(|node| self.traversal.screen_rect(node.layout.control))
    }

    pub(crate) fn node_clip(&self) -> Recti {
        self.traversal.screen_clip
    }

    pub(crate) fn push_node_clip(&mut self, id: UiNodeId) {
        self.runtime.push_node_clip_for_traversal(id, self.traversal);
    }

    pub(crate) fn pop_node_clip(&mut self) {
        self.runtime.pop_node_clip();
    }

    pub(crate) fn draw_frame(&mut self, rect: Recti, color: crate::ControlColor) {
        let mut draw = crate::draw_context::DrawCtx::new(
            &mut self.runtime.commands,
            &mut self.runtime.triangle_vertices,
            &mut self.runtime.clip_stack,
            self.style,
            &self.atlas,
        );
        draw.draw_frame(rect, color);
    }

    pub(crate) fn paint_container_widget(&mut self, id: UiNodeId, handle: WidgetHandle<Node>) {
        let rect = self.node_control(id).unwrap_or_default();
        let (hovered, focused, clicked, active, scroll_delta) = self
            .runtime
            .nodes
            .get(&id)
            .map(|node| (node.hovered, node.focused, node.clicked, node.active, node.scroll_delta))
            .unwrap_or((false, false, false, false, None));
        let widget = erased_widget_state(handle);
        let mut focus_slot = self.runtime.focus.map(RetainedId::node);
        let mut focus_seen = self.runtime.updated_focus;
        self.push_node_clip(id);
        let mut ctx = WidgetCtx::new_with_interaction(
            RetainedId::node(id),
            rect,
            &mut self.runtime.commands,
            &mut self.runtime.triangle_vertices,
            &mut self.runtime.clip_stack,
            self.style,
            &self.atlas,
            &mut focus_slot,
            &mut focus_seen,
            true,
            hovered,
            focused,
            clicked,
            active,
            scroll_delta,
            frame_events_from_input(self.input),
        );
        widget.paint(&mut ctx);
        self.pop_node_clip();
        self.runtime.focus = retained_focus_to_node(focus_slot);
        self.runtime.updated_focus = focus_seen;
    }
}
