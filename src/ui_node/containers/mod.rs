use crate::scrollbar::ScrollAxis;
use crate::sizing::SizePolicy;
use crate::context::erased_widget_state;
use crate::{Dimensioni, FrameResults, Input, Node, Recti, RetainedId, Style, Vec2i, WidgetHandle};

use super::{retained_focus_to_node, ClientArea, UiNode, UiNodeId, UiRuntime, WidgetCtx};

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
pub(crate) use scroll_area::ScrollArea;
pub(crate) use stack::Stack;

/// Clone support for boxed container behavior objects.
pub(crate) trait ContainerClone {
    /// Clones this container into a boxed trait object.
    fn clone_box(&self) -> Box<dyn ContainerTrait>;
}

impl<T> ContainerClone for T
where
    T: ContainerTrait + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn ContainerTrait> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn ContainerTrait> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Common internal behavior interface for child-owning nodes.
pub(crate) trait ContainerTrait: ContainerClone {
    /// Measures the preferred size for a container node.
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni;

    /// Assigns rectangles to children and recursively lays them out.
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti);

    /// Updates the container's own retained widget, if any, and returns whether children should be traversed.
    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _id: UiNodeId) -> bool {
        true
    }

    /// Paints any container-owned widget/background before children and returns whether children should be painted.
    fn paint_before_children(&mut self, _ctx: &mut PaintCtx<'_>, _id: UiNodeId) -> bool {
        true
    }

    /// Paints any container-owned overlay after children.
    fn paint_after_children(&mut self, _ctx: &mut PaintCtx<'_>, _id: UiNodeId) {}

    /// Dispatches scroll input owned by this container.
    fn dispatch_scroll(&mut self, _ctx: &mut ScrollDispatchCtx<'_>, _id: UiNodeId) -> bool {
        false
    }

    /// Returns an implicit vertical child policy this container contributes when it is a child.
    fn vertical_child_policy(&self) -> Option<SizePolicy> {
        None
    }

    /// Whether this container is the synthetic root-window body.
    fn is_root_window(&self) -> bool {
        false
    }

    /// Whether this container is a scrollable child viewport.
    fn is_scroll_area(&self) -> bool {
        false
    }

    /// Current scroll-area runtime state, if this is a scroll-area container.
    fn scroll_area_runtime_state(&self) -> Option<(Dimensioni, Vec2i, Option<ScrollAxis>)> {
        None
    }

    /// Stores scroll-area runtime state.
    fn set_scroll_area_runtime_state(&mut self, _content_size: Dimensioni, _offset: Vec2i, _drag: Option<ScrollAxis>) {}

    /// Carries runtime-only state from a previous container with the same node id.
    fn transfer_runtime_state_from(&mut self, previous: &dyn ContainerTrait) {
        if let Some((content_size, offset, drag)) = previous.scroll_area_runtime_state() {
            self.set_scroll_area_runtime_state(content_size, offset, drag);
        }
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
            node.client = client;
            node.client_area.visible_rect = client;
        }
    }

    pub(crate) fn set_content_size(&mut self, id: UiNodeId, content_size: Dimensioni) {
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            node.content_size = content_size;
        }
    }

    pub(crate) fn set_geometry(&mut self, id: UiNodeId, rect: Recti, client: Recti, clip: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            node.rect = rect;
            node.client = client;
            node.clip = clip;
            node.client_area.visible_rect = client;
            node.client_area.virtual_clip = client;
            node.client_area.virtual_size = Dimensioni::new(client.width.max(0), client.height.max(0));
            node.client_area.translation = Vec2i::default();
        }
    }

    pub(crate) fn set_client_area_geometry(&mut self, id: UiNodeId, rect: Recti, client_area: ClientArea, parent_clip: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            node.rect = rect;
            node.client = client_area.visible_rect;
            node.clip = client_area.effective_clip(parent_clip);
            node.client_area = client_area;
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
    pub(crate) style: &'a Style,
    pub(crate) atlas: crate::AtlasHandle,
    pub(crate) input: &'a Input,
    pub(crate) results: &'a mut FrameResults,
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

    pub(crate) fn update_container_widget(&mut self, id: UiNodeId, handle: WidgetHandle<Node>, label: &str) {
        let rect = self.runtime.nodes.get(&id).map(|node| node.client).unwrap_or_default();
        let widget = erased_widget_state(handle.clone());
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let control = self.runtime.control_for(id, rect, self.input, opt, scroll_behavior, focus_policy);
        if let Some(node) = self.runtime.nodes.get_mut(&id) {
            node.control = control;
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
            None,
        );
        let result = widget.update(&mut ctx, &control);
        self.runtime.focus = retained_focus_to_node(focus_slot);
        self.runtime.updated_focus = focus_seen;

        self.results
            .record_retained_with_context(RetainedId::root_node(self.root_id, id), handle.id(), result, format!("{label} {:?}", id));
    }
}

/// Services available while a container handles scroll dispatch.
///
/// Scroll dispatch may mutate scroll state on the receiving container, but child topology is
/// read-only.
pub(crate) struct ScrollDispatchCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) input: &'a Input,
}

impl ScrollDispatchCtx<'_> {
    pub(crate) fn node_clip_and_client(&self, id: UiNodeId) -> Option<(Recti, Recti)> {
        self.runtime.nodes.get(&id).map(|node| (node.clip, node.client))
    }

    pub(crate) fn node_client_area(&self, id: UiNodeId) -> Option<ClientArea> {
        self.runtime.nodes.get(&id).map(|node| node.client_area)
    }
}

/// Services available while a container paints its own surface.
///
/// Painting may record commands and clip changes, but child topology is read-only.
pub(crate) struct PaintCtx<'a> {
    pub(crate) runtime: &'a mut UiRuntime,
    pub(crate) style: &'a Style,
    pub(crate) atlas: crate::AtlasHandle,
}

impl PaintCtx<'_> {
    pub(crate) fn node_rect(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.nodes.get(&id).map(|node| node.rect)
    }

    pub(crate) fn node_client(&self, id: UiNodeId) -> Option<Recti> {
        self.runtime.nodes.get(&id).map(|node| node.client)
    }

    pub(crate) fn node_client_area(&self, id: UiNodeId) -> Option<ClientArea> {
        self.runtime.nodes.get(&id).map(|node| node.client_area)
    }

    pub(crate) fn node_control(&self, id: UiNodeId) -> crate::input::ControlState {
        self.runtime.nodes.get(&id).map(|node| node.control).unwrap_or_default()
    }

    pub(crate) fn push_node_clip(&mut self, id: UiNodeId) {
        self.runtime.push_node_clip(id);
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
        let rect = self.node_client(id).unwrap_or_default();
        let control = self.node_control(id);
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
            None,
        );
        widget.paint(&mut ctx, &control);
        self.pop_node_clip();
        self.runtime.focus = retained_focus_to_node(focus_slot);
        self.runtime.updated_focus = focus_seen;
    }
}
