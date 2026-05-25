//! Retained tree measurement without live draw or interaction state.

use super::*;

#[derive(Clone)]
/// Snapshot of viewport state needed by layout-only measurement.
struct MeasurementViewport {
    /// Outer viewport rectangle assigned to the measured container.
    rect: Recti,
    /// Inner body rectangle available to children after chrome/scrollbar gutters.
    body: Recti,
    /// Content dimensions discovered by the latest layout pass.
    content_size: Dimensioni,
    /// Scroll offset used while measuring scrolled child content.
    scroll: Vec2i,
    /// Whether scrollbars and scroll clamping are enabled.
    scroll_enabled: bool,
}

impl From<&TraversalHost> for MeasurementViewport {
    fn from(host: &TraversalHost) -> Self {
        Self {
            rect: host.viewport.rect,
            body: host.viewport.body,
            content_size: host.viewport.content_size,
            scroll: host.viewport.scroll.offset(),
            scroll_enabled: host.viewport.scroll.enabled(),
        }
    }
}

/// Layout-only context used for auto-size and nested scroll-area measurement.
pub(crate) struct MeasurementContext {
    /// Atlas used for widget intrinsic measurement.
    atlas: AtlasHandle,
    /// Style snapshot used by layout policy resolution.
    style: Rc<Style>,
    /// Scope seed for deterministic retained ids during measurement.
    internal_id_seed: Id,
    /// Viewport snapshot being measured.
    viewport: MeasurementViewport,
    /// Layout engine cloned from the live traversal host.
    layout: LayoutManager,
    /// Per-measurement frame cache for retained node layouts.
    tree_cache: WidgetTreeCache,
}

impl MeasurementContext {
    /// Creates a layout-only context from a live traversal host.
    pub(crate) fn from_host(host: &TraversalHost) -> Self {
        Self {
            atlas: host.atlas.clone(),
            style: host.style.clone(),
            internal_id_seed: host.internal_id_seed,
            viewport: MeasurementViewport::from(host),
            layout: host.layout.clone(),
            tree_cache: WidgetTreeCache::default(),
        }
    }

    /// Copies style and scope identity inherited from a parent measurement context.
    fn apply_parent_state(&mut self, parent: &Self, scope: Id) {
        self.internal_id_seed = scope;
        self.style = parent.style.clone();
    }

    /// Clamps `x` into the inclusive range `[a, b]`.
    fn clamp(x: i32, a: i32, b: i32) -> i32 {
        min(max(x, a), b)
    }

    /// Compares rectangle components without relying on external trait behavior.
    fn same_rect(a: Recti, b: Recti) -> bool {
        (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height)
    }

    /// Resets measured content and scroll state before a fresh auto-size pass.
    pub(crate) fn clear_content_and_scroll(&mut self) {
        self.viewport.content_size = Dimensioni::default();
        self.viewport.scroll = Vec2i::default();
    }

    /// Sets the outer viewport rectangle.
    fn set_rect(&mut self, rect: Recti) {
        self.viewport.rect = rect;
    }

    /// Returns the outer viewport rectangle.
    fn rect(&self) -> Recti {
        self.viewport.rect
    }

    /// Returns the child body rectangle.
    fn body(&self) -> Recti {
        self.viewport.body
    }

    /// Returns the last measured content size.
    fn content_size(&self) -> Dimensioni {
        self.viewport.content_size
    }

    /// Stores the last measured content size.
    fn set_content_size(&mut self, content_size: Dimensioni) {
        self.viewport.content_size = content_size;
    }

    /// Applies the retained scroll behavior to the measurement viewport.
    fn apply_scroll_behavior(&mut self, scroll_behavior: ScrollBehavior) {
        self.viewport.scroll_enabled = !scroll_behavior.is_no_scroll();
    }

    /// Adds style padding to content size before evaluating scrollbar ranges.
    fn padded_scrollbar_content_size(mut content_size: Dimensioni, padding: i32) -> Dimensioni {
        content_size.width += padding * 2;
        content_size.height += padding * 2;
        content_size
    }

    /// Resolves the body rectangle after vertical and horizontal scrollbar gutters are considered.
    fn resolved_scrollbar_body(body: Recti, content_size: Dimensioni, scrollbar_size: i32) -> Recti {
        let scrollbar_size = scrollbar_size.max(0);
        let mut resolved = body;
        // A vertical bar can make a horizontal bar necessary and vice versa; a few passes converge
        // because the body can only shrink along each axis.
        for _ in 0..3 {
            let needs_vertical = content_size.height > resolved.height && resolved.height > 0;
            let needs_horizontal = content_size.width > resolved.width && resolved.width > 0;
            let mut next = body;
            if needs_vertical {
                next.width = next.width.saturating_sub(scrollbar_size).max(0);
            }
            if needs_horizontal {
                next.height = next.height.saturating_sub(scrollbar_size).max(0);
            }
            if (next.x, next.y, next.width, next.height) == (resolved.x, resolved.y, resolved.width, resolved.height) {
                break;
            }
            resolved = next;
        }
        resolved
    }

    /// Shrinks `body` for visible scrollbars and clamps the measurement scroll offset.
    fn resolve_scrollbars(&mut self, body: &mut Recti) {
        let (scrollbar_size, padding) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding)
        };
        let cs = Self::padded_scrollbar_content_size(self.viewport.content_size, padding);
        *body = Self::resolved_scrollbar_body(*body, cs, scrollbar_size);
        let body = *body;
        let maxscroll_y = crate::scrollbar::scrollbar_max_scroll(cs.height, body.height);
        self.viewport.scroll.y = if maxscroll_y > 0 && body.height > 0 {
            Self::clamp(self.viewport.scroll.y, 0, maxscroll_y)
        } else {
            0
        };

        let maxscroll_x = crate::scrollbar::scrollbar_max_scroll(cs.width, body.width);
        self.viewport.scroll.x = if maxscroll_x > 0 && body.width > 0 {
            Self::clamp(self.viewport.scroll.x, 0, maxscroll_x)
        } else {
            0
        };
    }

    /// Resolves child body dimensions for a content hint without mutating this context.
    fn resolved_body_for_content(&self, body: Recti, scroll_behavior: ScrollBehavior, content_size: Dimensioni) -> Recti {
        if scroll_behavior.is_no_scroll() {
            return body;
        }
        let style = self.style.as_ref();
        let content_size = Self::padded_scrollbar_content_size(content_size, style.padding);
        Self::resolved_scrollbar_body(body, content_size, style.scrollbar_size)
    }

    /// Configures the active body layout scope and default cell metrics.
    pub(crate) fn configure_container_body(&mut self, body: Recti, scroll_behavior: ScrollBehavior) {
        let mut body = body;
        self.apply_scroll_behavior(scroll_behavior);
        if self.viewport.scroll_enabled {
            self.resolve_scrollbars(&mut body);
        }
        let (layout_padding, style_padding, font, style_clone) = {
            let style = self.style.as_ref();
            (-style.padding, style.padding, style.font, *style)
        };
        self.layout.reset(expand_rect(body, layout_padding), self.viewport.scroll);
        self.layout.style = style_clone;
        let font_height = self.atlas.get_font_height(font) as i32;
        let vertical_pad = crate::text_layout::vertical_text_padding(style_padding);
        let icon_height = self.atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let default_height = max(font_height + vertical_pad * 2, icon_height);
        self.layout.set_default_cell_height(default_height);
        self.viewport.body = body;
    }

    /// Commits current layout extents into the measurement viewport.
    fn commit_active_layout_content_size(&mut self) -> Dimensioni {
        let layout_body = self.layout.current_body();
        let content_size = self
            .layout
            .current_max()
            .map(|lm| Dimensioni::new(lm.x - layout_body.x, lm.y - layout_body.y))
            .unwrap_or_default();
        self.set_content_size(content_size);
        content_size
    }

    /// Finishes the active body layout scope and returns the resulting node layout.
    fn finish_body_layout_scope(&mut self) -> NodeLayout {
        self.commit_active_layout_content_size();
        let layout = NodeLayout::new(self.rect(), self.body(), self.content_size());
        self.layout.pop_scope();
        layout
    }

    /// Measures a body until scrollbar-dependent body size stops changing.
    fn layout_body_until_scrollbars_stable(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        rect: Recti,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        self.layout_viewport_body_until_scrollbars_stable(results, resources, rect, rect, scroll_behavior, children)
    }

    /// Measures an inner body within an outer viewport until scrollbar gutters stabilize.
    fn layout_viewport_body_until_scrollbars_stable(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        viewport_rect: Recti,
        body_rect: Recti,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        let mut content_hint = self.content_size();
        let mut layout = self.layout_body_with_content_hint(results, resources, viewport_rect, body_rect, scroll_behavior, content_hint, children);
        // Re-run layout when a changed content size changes scrollbar visibility and therefore
        // the child body dimensions.
        for _ in 0..3 {
            let resolved_body = self.resolved_body_for_content(body_rect, scroll_behavior, layout.content_size);
            if Self::same_rect(resolved_body, layout.body) {
                return layout;
            }
            content_hint = layout.content_size;
            layout = self.layout_body_with_content_hint(results, resources, viewport_rect, body_rect, scroll_behavior, content_hint, children);
        }
        layout
    }

    /// Performs one layout pass using `content_hint` to seed scrollbar decisions.
    fn layout_body_with_content_hint(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        viewport_rect: Recti,
        body_rect: Recti,
        scroll_behavior: ScrollBehavior,
        content_hint: Dimensioni,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        self.tree_cache.begin_frame();
        self.set_rect(viewport_rect);
        self.set_content_size(content_hint);
        self.configure_container_body(body_rect, scroll_behavior);
        self.layout_tree_nodes(results, resources, children);
        self.finish_body_layout_scope()
    }

    /// Measures all roots in a retained tree and returns their aggregate content size.
    pub(crate) fn measure_widget_tree_content(&mut self, results: &FrameResults, tree: &WidgetTree) -> Dimensioni {
        self.layout_tree_nodes(results, tree.resources(), tree.roots());
        match self.layout.current_max() {
            Some(max_rect) => {
                let body = self.layout.current_body();
                Dimensioni::new(max_rect.x - body.x, max_rect.y - body.y)
            }
            None => Dimensioni::default(),
        }
    }

    /// Builds the deterministic scope id for a nested retained scroll area.
    fn scroll_area_scope_id(&self, node_id: NodeId) -> Id {
        crate::id::IdNamespace::SCROLL_AREA_SCOPE.id([self.internal_id_seed.raw() as u64, node_id.raw() as u64])
    }

    /// Measures one erased widget and advances the current layout flow.
    fn measure_widget_rect_dyn_with_policy(&mut self, widget: &dyn WidgetStateHandleDyn, policy: Policy) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = widget.measure(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_policies(preferred, policy.width, policy.height)
    }

    /// Records a resolved node layout in the measurement cache.
    fn record_tree_layout(&mut self, node_id: NodeId, layout: NodeLayout) {
        self.tree_cache.record_layout(node_id, layout);
    }

    /// Records a structural node layout from the bounds of its already-measured children.
    fn record_tree_group_from_children(&mut self, node_id: NodeId, children: &[WidgetTreeNode]) {
        let mut bounds: Option<Recti> = None;
        for child in children {
            if let Some(child_state) = self.tree_cache.current_layout(child.id()) {
                bounds = Some(match bounds {
                    Some(existing_rect) => {
                        let min_x = existing_rect.x.min(child_state.rect.x);
                        let min_y = existing_rect.y.min(child_state.rect.y);
                        let max_x = (existing_rect.x + existing_rect.width).max(child_state.rect.x + child_state.rect.width);
                        let max_y = (existing_rect.y + existing_rect.height).max(child_state.rect.y + child_state.rect.height);
                        rect(min_x, min_y, max_x - min_x, max_y - min_y)
                    }
                    None => child_state.rect,
                });
            }
        }

        if let Some(rect) = bounds {
            self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::new(rect.width, rect.height)));
        }
    }

    /// Visits each sibling node in order using the active layout flow.
    fn layout_tree_nodes(&mut self, results: &FrameResults, resources: &WidgetTreeResources, nodes: &[WidgetTreeNode]) {
        for node in nodes {
            self.layout_tree_node(results, resources, node);
        }
    }

    /// Dispatches measurement for one retained tree node kind.
    fn layout_tree_node(&mut self, results: &FrameResults, resources: &WidgetTreeResources, node: &WidgetTreeNode) {
        let (node_id, kind, children) = node.parts();
        let policy = node.policy();
        match kind {
            WidgetTreeNodeKind::Widget { resource } => self.layout_tree_widget(node_id, policy, resources.widget(*resource)),
            WidgetTreeNodeKind::CustomRender { resource } => {
                let (state, _) = resources.custom_render(*resource);
                self.layout_tree_custom_render(node_id, policy, state);
            }
            WidgetTreeNodeKind::ScrollArea { resource, scroll_behavior, .. } => {
                let layout = resources
                    .scroll_area(*resource)
                    .inner()
                    .measure_children(self, results, resources, node_id, policy, *scroll_behavior, children);
                self.record_tree_layout(node_id, layout);
            }
            WidgetTreeNodeKind::Header { resource } => {
                self.layout_tree_node_scope_children(results, resources, node_id, policy, resources.node(*resource), children, false)
            }
            WidgetTreeNodeKind::Tree { resource } => {
                self.layout_tree_node_scope_children(results, resources, node_id, policy, resources.node(*resource), children, true)
            }
            WidgetTreeNodeKind::Row { widths, height } => self.visit_tree_row(results, resources, node_id, policy, children, widths, *height),
            WidgetTreeNodeKind::Grid { widths, heights } => self.visit_tree_grid(results, resources, node_id, policy, children, widths, heights),
            WidgetTreeNodeKind::Column => self.visit_tree_column(results, resources, node_id, policy, children),
            WidgetTreeNodeKind::Stack { width, height, direction } => {
                self.visit_tree_stack(results, resources, node_id, policy, children, *width, *height, *direction)
            }
        }
    }

    /// Measures a regular widget node.
    fn layout_tree_widget(&mut self, node_id: NodeId, policy: Policy, widget: &dyn WidgetStateHandleDyn) {
        let rect = self.measure_widget_rect_dyn_with_policy(widget, policy);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::default()));
    }

    /// Measures a custom-render node through its retained widget state.
    fn layout_tree_custom_render(&mut self, node_id: NodeId, policy: Policy, state: &WidgetHandle<Custom>) {
        let widget = erased_widget_state(state.clone());
        let rect = self.measure_widget_rect_dyn_with_policy(&*widget, policy);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::default()));
    }

    /// Measures an expandable node header and returns its stable expanded/collapsed state.
    fn layout_tree_node_scope(&mut self, node_id: NodeId, policy: Policy, state: &WidgetHandle<Node>) -> NodeStateValue {
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let widget = erased_widget_state(state.clone());
        let rect = self.measure_widget_rect_dyn_with_policy(&*widget, policy);
        let stable_state = state.read(|state| state.state);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::default()));
        stable_state
    }

    /// Measures children for an expandable node when it is currently expanded.
    fn layout_tree_node_scope_children(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        policy: Policy,
        state: &WidgetHandle<Node>,
        children: &[WidgetTreeNode],
        indent_children: bool,
    ) {
        if !self.layout_tree_node_scope(node_id, policy, state).is_expanded() {
            return;
        }

        if indent_children {
            let indent_size = self.style.as_ref().indent;
            self.layout.adjust_indent(indent_size);
            self.layout_tree_nodes(results, resources, children);
            self.layout.adjust_indent(-indent_size);
        } else {
            self.layout_tree_nodes(results, resources, children);
        }
    }

    /// Measures a structural policy group and records either child bounds or scoped content size.
    fn layout_policy_group<F: FnOnce(&mut Self)>(&mut self, node_id: NodeId, policy: Policy, children: &[WidgetTreeNode], f: F) {
        if policy == Policy::auto() {
            f(self);
            self.record_tree_group_from_children(node_id, children);
            return;
        }

        let rect = self.layout.begin_node_scope_with_policies(Dimensioni::default(), policy.width, policy.height);
        f(self);
        let content_size = self.layout.end_node_scope();
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, content_size));
    }

    /// Measures a row node with explicit width tracks.
    fn visit_tree_row(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        policy: Policy,
        children: &[WidgetTreeNode],
        widths: &[SizePolicy],
        height: SizePolicy,
    ) {
        self.layout_policy_group(node_id, policy, children, |ctx| {
            ctx.with_row(widths, height, |ctx| {
                ctx.layout_tree_nodes(results, resources, children);
            });
        });
    }

    /// Measures a grid node with explicit width and height tracks.
    fn visit_tree_grid(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        policy: Policy,
        children: &[WidgetTreeNode],
        widths: &[SizePolicy],
        heights: &[SizePolicy],
    ) {
        self.layout_policy_group(node_id, policy, children, |ctx| {
            ctx.with_grid(widths, heights, |ctx| {
                ctx.layout_tree_nodes(results, resources, children);
            });
        });
    }

    /// Measures a column node by temporarily entering column layout mode.
    fn visit_tree_column(&mut self, results: &FrameResults, resources: &WidgetTreeResources, node_id: NodeId, policy: Policy, children: &[WidgetTreeNode]) {
        if policy == Policy::auto() {
            self.column(|ctx| {
                ctx.layout_tree_nodes(results, resources, children);
            });
            self.record_tree_group_from_children(node_id, children);
        } else {
            let rect = self.layout.begin_node_scope_with_policies(Dimensioni::default(), policy.width, policy.height);
            self.layout_tree_nodes(results, resources, children);
            let content_size = self.layout.end_node_scope();
            self.record_tree_layout(node_id, NodeLayout::new(rect, rect, content_size));
        }
    }

    /// Measures a stack node using the requested overlay/stack direction.
    fn visit_tree_stack(
        &mut self,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        policy: Policy,
        children: &[WidgetTreeNode],
        width: SizePolicy,
        height: SizePolicy,
        direction: StackDirection,
    ) {
        self.layout_policy_group(node_id, policy, children, |ctx| {
            ctx.stack_with_width_direction(width, height, direction, |ctx| {
                ctx.layout_tree_nodes(results, resources, children);
            });
        });
    }

    /// Runs child measurement under a temporary row flow.
    fn with_row<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], height: SizePolicy, f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.row(widths, height);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Runs child measurement under a temporary grid flow.
    fn with_grid<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.grid(widths, heights);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Runs child measurement under a temporary stack flow.
    fn stack_with_width_direction<F: FnOnce(&mut Self)>(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        if direction == StackDirection::TopToBottom {
            self.layout.stack(width, height);
        } else {
            self.layout.stack_with_direction(width, height, direction);
        }
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Runs child measurement inside a temporary column scope.
    fn column<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.layout.begin_column();
        f(self);
        self.layout.end_column();
    }
}

impl ScrollArea {
    /// Measures scroll-area children by creating a nested measurement context.
    pub(crate) fn measure_children(
        &self,
        parent: &mut MeasurementContext,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        policy: Policy,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        let mut child = MeasurementContext::from_host(self.host());
        child.apply_parent_state(parent, parent.scroll_area_scope_id(node_id));
        let rect = parent.layout.next_with_policies(Dimensioni::default(), policy.width, policy.height);
        child.set_rect(rect);
        child.layout_body_until_scrollbars_stable(results, resources, rect, scroll_behavior, children)
    }
}
