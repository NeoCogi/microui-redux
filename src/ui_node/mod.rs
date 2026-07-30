//! Common runtime node model used by the next retained traversal path.
//!
//! Internal retained UI node runtime.
//! It gives the crate one node representation that can own either a widget or a framework
//! container.
//!
//! Topology is built by the window-manager/builder path and then traversed by input, update,
//! measure, layout, and paint passes. Runtime traversal does not mutate child membership.
//!
//! Each node retains an allocation in its parent's child coordinates plus a node-local child
//! offset and clip. Recursive passes carry one stack-only [`Transform`]. Resolved outer rectangles
//! and outer clips remain runtime stack locals; phase contexts expose node-local content geometry.
//! [`NodeBehavior`] is the internal retained-node contract; [`WidgetNode`] adapts the public
//! [`crate::Widget`] contract to it.
#![allow(dead_code)]

use crate::render::DisplayList;
#[cfg(test)]
use crate::render::RendererBackend;
use crate::{expand_rect, Dimensioni, FrameResults, Input, MouseButton, Recti, Style, Vec2i, UNCLIPPED_RECT};
#[cfg(test)]
use crate::UiNodeSet;
use crate::{WidgetOption, WindowOption};
use crate::sizing::SizePolicy;
use crate::widget::FocusPolicy;

mod node;
pub(crate) use node::{NodeLayout, Transform, UiNode, UiNodeData, UiNodeId, UiNodeState};
mod runtime;
pub(crate) use runtime::UiRuntime;
#[cfg(test)]
pub(crate) use runtime::RuntimeMetrics;
mod containers;
pub(crate) use containers::{
    scroll_viewport_node, scrollbar_nodes, shared_scroll_area_state, Column, Container, Disclosure, Grid, InputCtx, InputResult, LayoutCtx, MeasureCtx,
    NodeBehavior, PaintCtx, Row, ScrollArea, Stack, UpdateCtx, WidgetNode,
};
#[cfg(test)]
pub(crate) use containers::{scroll_area_state, set_scroll_area_scroll};
pub use containers::UiInputEvent;
pub use containers::ScrollAreaOption;

/// Computes titlebar height from style minimums and current title font metrics.
fn root_titlebar_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.title_font) as i32;
    let padding = style.padding.max(0);
    let min_title_h = font_height + (padding / 2).max(1) * 2;
    style.title_height.max(min_title_h)
}

/// Returns the union of two rectangles.
fn union_rect(a: Recti, b: Recti) -> Recti {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);
    Recti::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Compares rectangle components directly.
fn same_rect(a: Recti, b: Recti) -> bool {
    (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height)
}

/// Adds symmetric style padding to content size for scrollbar range checks.
fn add_padding(size: Dimensioni, padding: i32) -> Dimensioni {
    Dimensioni::new(
        size.width.saturating_add(padding.saturating_mul(2)),
        size.height.saturating_add(padding.saturating_mul(2)),
    )
}

/// Returns the child indentation for a disclosure container.
fn disclosure_child_indent(indent_children: bool, style: &Style) -> i32 {
    if indent_children { style.indent.max(0) } else { 0 }
}

/// Returns the fallback control width for auto-sized cells.
fn default_cell_width(style: &Style) -> i32 {
    style.default_cell_width.saturating_add(style.padding.max(0) * 2).max(0)
}

/// Returns the fallback control height for auto-sized cells.
fn default_cell_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.font) as i32;
    font_height.saturating_add(style.padding.max(0) * 2).max(style.padding.max(0) * 2)
}

/// Expands a possibly shorter track-policy slice to a requested count.
fn track_policies(policies: &[SizePolicy], count: usize) -> Vec<SizePolicy> {
    (0..count).map(|index| policies.get(index).copied().unwrap_or(SizePolicy::Auto)).collect()
}

/// Returns the first free cell in a row-major occupancy grid, extending rows as needed.
fn first_free_grid_cell(occupied: &mut Vec<Vec<bool>>, cols: usize, mut row: usize, mut col: usize) -> (usize, usize) {
    loop {
        while occupied.len() <= row {
            occupied.push(vec![false; cols]);
        }
        while col < cols {
            if !occupied[row][col] {
                return (row, col);
            }
            col += 1;
        }
        row += 1;
        col = 0;
    }
}

/// Marks a rectangular cell range as occupied, extending rows as needed.
fn mark_grid_occupied(occupied: &mut Vec<Vec<bool>>, cols: usize, row: usize, col: usize, row_span: usize, col_span: usize) {
    for y in row..row.saturating_add(row_span.max(1)) {
        while occupied.len() <= y {
            occupied.push(vec![false; cols]);
        }
        for x in col..col.saturating_add(col_span.max(1)).min(cols) {
            occupied[y][x] = true;
        }
    }
}

/// Sums a track span, including the spacing between spanned tracks.
fn span_size(tracks: &[i32], start: usize, span: usize, spacing: i32) -> i32 {
    let span = span.max(1);
    let size = tracks.iter().skip(start).take(span).copied().sum::<i32>();
    size.saturating_add(spacing.saturating_mul(span.saturating_sub(1) as i32))
}

/// Resolves one size policy against a preferred size, available space, and optional weight context.
fn resolve_size(policy: SizePolicy, preferred: i32, available: i32, reference: i32, total_weight: Option<f32>) -> i32 {
    let resolved = match policy {
        SizePolicy::Auto => preferred,
        SizePolicy::Fixed(value) => value,
        SizePolicy::Fraction(value) => {
            let fraction = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
            ((reference.max(0) as f32) * fraction).floor() as i32
        }
        SizePolicy::Weight(value) => {
            let weight = if value.is_finite() { value.max(0.0) } else { 0.0 };
            if weight <= 0.0 {
                0
            } else {
                let denom = total_weight.filter(|total| total.is_finite() && *total > 0.0).unwrap_or(weight);
                ((reference.max(0) as f32) * (weight / denom)).floor() as i32
            }
        }
        SizePolicy::Remainder(margin) => available.saturating_sub(margin),
    };
    resolved.max(0)
}

/// Resolves the available size passed into a child measurement from explicit placement policy.
fn measure_axis_available(policy: SizePolicy, available: i32) -> i32 {
    match policy {
        SizePolicy::Fixed(value) => value.max(0),
        SizePolicy::Fraction(value) => resolve_size(SizePolicy::Fraction(value), 0, available, available, None),
        SizePolicy::Remainder(margin) => available.saturating_sub(margin.max(0)).max(0),
        SizePolicy::Auto | SizePolicy::Weight(_) => available.max(0),
    }
}

/// Resolves a node inside an already allocated parent slot.
fn resolve_allocated_size(policy: SizePolicy, preferred: i32, allocated: i32, reference: i32, total_weight: Option<f32>) -> i32 {
    match policy {
        SizePolicy::Auto => allocated.max(0),
        _ => resolve_size(policy, preferred, allocated, reference, total_weight),
    }
}

/// Resolves sibling tracks in one axis using fixed, auto, fraction, weight, and remainder policies.
fn resolve_axis_tracks(policies: &[SizePolicy], preferred: &[i32], available: i32) -> Vec<i32> {
    if policies.is_empty() {
        return Vec::new();
    }

    let available = available.max(0);
    let mut sizes = vec![0; policies.len()];
    let has_remainder = policies.iter().any(|policy| matches!(policy, SizePolicy::Remainder(_)));
    let total_weight = policies
        .iter()
        .filter_map(|policy| match *policy {
            SizePolicy::Weight(value) if value.is_finite() => Some(value.max(0.0)),
            _ => None,
        })
        .sum::<f32>();
    let reserved_for_weight = if has_remainder {
        0
    } else {
        policies
            .iter()
            .copied()
            .enumerate()
            .map(|(index, policy)| match policy {
                SizePolicy::Auto => preferred.get(index).copied().unwrap_or_default().max(0),
                SizePolicy::Fixed(value) => value.max(0),
                SizePolicy::Fraction(value) => resolve_size(SizePolicy::Fraction(value), 0, available, available, None),
                SizePolicy::Weight(_) | SizePolicy::Remainder(_) => 0,
            })
            .sum::<i32>()
    };
    let weight_reference = if total_weight > 0.0 {
        if has_remainder {
            available
        } else {
            available.saturating_sub(reserved_for_weight)
        }
    } else {
        0
    };

    let mut used: i32 = 0;
    for (index, policy) in policies.iter().copied().enumerate() {
        let remaining = available.saturating_sub(used);
        match policy {
            SizePolicy::Auto => {
                sizes[index] = preferred.get(index).copied().unwrap_or_default().max(0);
            }
            SizePolicy::Fixed(value) => {
                sizes[index] = value.max(0);
            }
            SizePolicy::Fraction(value) => {
                sizes[index] = resolve_size(SizePolicy::Fraction(value), 0, available, available, None);
            }
            SizePolicy::Weight(value) => {
                sizes[index] = resolve_size(SizePolicy::Weight(value), 0, remaining, weight_reference, Some(total_weight));
            }
            SizePolicy::Remainder(margin) => {
                sizes[index] = remaining.saturating_sub(margin.max(0)).max(0);
            }
        }
        used = used.saturating_add(sizes[index]);
    }
    sizes
}

/// Returns the screen-space rectangle occupied by a child and any overflow content it measured.
fn child_content_rect(node: &UiNode) -> Recti {
    let allocation = node.state.layout.allocation;
    let content_size = node.state.layout.content_size;
    Recti::new(
        allocation.x,
        allocation.y,
        allocation.width.max(content_size.width),
        allocation.height.max(content_size.height),
    )
}

/// Builds pointer events from raw frame input.
pub(crate) fn pointer_events_from_input(input: &Input) -> Vec<UiInputEvent> {
    let mut events = Vec::new();
    if !input.mouse_pressed.is_empty() {
        events.push(UiInputEvent::MouseDown {
            pos: input.mouse_pos,
            button: input.mouse_pressed,
        });
    }
    if !input.mouse_released.is_empty() {
        events.push(UiInputEvent::MouseUp {
            pos: input.mouse_pos,
            button: input.mouse_released,
        });
    }
    if input.mouse_delta.x != 0 || input.mouse_delta.y != 0 {
        if input.mouse_down.is_empty() {
            events.push(UiInputEvent::MouseMove {
                pos: input.mouse_pos,
                delta: input.mouse_delta,
            });
        } else {
            events.push(UiInputEvent::MouseDrag {
                pos: input.mouse_pos,
                delta: input.mouse_delta,
                buttons: input.mouse_down,
            });
        }
    }
    if input.scroll_delta.x != 0 || input.scroll_delta.y != 0 {
        events.push(UiInputEvent::Scroll {
            pos: input.mouse_pos,
            delta: input.scroll_delta,
        });
    }
    events
}

/// Builds focus transition events from raw frame input.
pub(crate) fn focus_events_from_input(input: &Input) -> Vec<UiInputEvent> {
    let mut events = Vec::new();
    if !input.key_pressed.is_empty() {
        events.push(UiInputEvent::KeyDown { key: input.key_pressed });
    }
    if !input.key_released.is_empty() {
        events.push(UiInputEvent::KeyUp { key: input.key_released });
    }
    if !input.key_code_pressed.is_empty() {
        events.push(UiInputEvent::KeyCodeDown { code: input.key_code_pressed });
    }
    if !input.key_code_released.is_empty() {
        events.push(UiInputEvent::KeyCodeUp { code: input.key_code_released });
    }
    if !input.input_text.is_empty() {
        events.push(UiInputEvent::Text { text: input.input_text.clone() });
    }
    events
}

/// Builds held input state events for the current frame.
pub(super) fn held_events_from_input(input: &Input) -> Vec<UiInputEvent> {
    let mut events = Vec::new();
    if !input.key_down.is_empty() {
        events.push(UiInputEvent::KeyState { keys: input.key_down });
    }
    if !input.key_code_down.is_empty() {
        events.push(UiInputEvent::KeyCodeState { codes: input.key_code_down });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    use crate::render::Renderer;
    use crate::{
        rect, AtlasHandle, AtlasSource, Button, CharEntry, Custom, FontEntry, Id, Input, KeyMode, ListItem, NodeOptions, Policy, ResourceState, SourceFormat,
        StackDirection, Textbox, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx, UiNodeBuilder, widget_handle,
    };
    use crate::test_support::{test_atlas, NoopRenderer};

    struct TestRuntime {
        runtime: UiRuntime,
        display_list: DisplayList,
        roots: Vec<UiNode>,
        z_order: Vec<UiNodeId>,
    }

    impl std::ops::Deref for TestRuntime {
        type Target = UiRuntime;

        fn deref(&self) -> &Self::Target {
            &self.runtime
        }
    }

    impl std::ops::DerefMut for TestRuntime {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.runtime
        }
    }

    impl TestRuntime {
        fn new() -> Self {
            Self {
                runtime: UiRuntime::new(),
                display_list: DisplayList::new(),
                roots: Vec::new(),
                z_order: Vec::new(),
            }
        }

        fn from_ui_nodes(tree: UiNodeSet) -> Self {
            let roots = tree.into_roots();
            let z_order = roots.iter().map(UiNode::id).collect();
            Self {
                runtime: UiRuntime::new(),
                display_list: DisplayList::new(),
                roots,
                z_order,
            }
        }

        fn node(&self, id: UiNodeId) -> Option<&UiNode> {
            self.runtime.node(&self.roots, id)
        }

        fn contains_node(&self, id: UiNodeId) -> bool {
            self.runtime.contains_node(&self.roots, id)
        }

        fn parent_of(&self, id: UiNodeId) -> Option<UiNodeId> {
            self.runtime.parent_of(&self.roots, id)
        }

        fn transform_for_node(&self, id: UiNodeId) -> Transform {
            self.runtime.transform_for_node(&self.roots, id)
        }

        fn node_screen_rect(&self, id: UiNodeId) -> Option<Recti> {
            self.node(id)
                .map(|node| self.runtime.parent_transform_for_node(&self.roots, id).resolve(node.state.layout.allocation))
        }

        fn replace_ui_nodes(&mut self, tree: UiNodeSet) {
            let previous_roots = std::mem::replace(&mut self.roots, tree.into_roots());
            self.z_order = self.roots.iter().map(UiNode::id).collect();
            let mut next_runtime = UiRuntime::new();
            next_runtime.transfer_runtime_state_from(&mut self.roots, &previous_roots, &self.runtime);
            self.runtime = next_runtime;
        }

        fn route_input_events(&mut self, style: &Style, input: &Input) -> bool {
            let mut consumed = false;
            if self.runtime.accepts_pointer_input() || self.runtime.capture.is_some() {
                for event in pointer_events_from_input(input) {
                    if let Some(captured) = self.runtime.route_captured_pointer_input_event(&mut self.roots, style, input, &event) {
                        consumed |= captured;
                        continue;
                    }
                    if !self.runtime.accepts_pointer_input() {
                        continue;
                    }
                    let mut routed = None;
                    let root_transform = self.runtime.root_transform();
                    for root in self.z_order.iter().copied().rev() {
                        let Some(root) = self.roots.iter_mut().find(|node| node.id() == root) else {
                            continue;
                        };
                        routed = self.runtime.route_input_event_to_node_ref(root, root_transform, style, &event);
                        if routed.is_some() {
                            break;
                        }
                    }
                    if let Some((owner, result)) = routed {
                        self.runtime.update_pointer_capture(owner, result, &event, input);
                        consumed |= result.is_consumed();
                    }
                }
            }
            consumed | self.runtime.route_focus_input_events(&mut self.roots, style, input)
        }

        fn render_frame<B: RendererBackend>(
            &mut self,
            root_id: crate::RootId,
            root_name: &str,
            renderer: &mut Renderer<B>,
            style: &Style,
            input: &Input,
            results: &mut FrameResults,
            body: Recti,
            pointer_input_enabled: bool,
        ) {
            self.runtime.begin_frame(pointer_input_enabled);
            self.runtime.layout_frame_roots(&mut self.roots, style, renderer.atlas(), body);
            self.route_input_events(style, input);
            let atlas = renderer.atlas();
            self.runtime
                .update_paint_frame(&mut self.roots, root_id, root_name, &mut self.display_list, atlas, style, input, results, body);
            renderer.render_test(&mut self.display_list);
        }
    }

    #[derive(Clone)]
    struct RecordingBehavior {
        log: Rc<RefCell<Vec<(UiNodeId, &'static str)>>>,
        result: InputResult,
    }

    impl RecordingBehavior {
        fn new(log: Rc<RefCell<Vec<(UiNodeId, &'static str)>>>, result: InputResult) -> Self {
            Self { log, result }
        }
    }

    impl NodeBehavior for RecordingBehavior {
        fn measure(&self, _ctx: &MeasureCtx<'_>, _state: &UiNodeState, _available: Dimensioni) -> Dimensioni {
            Dimensioni::default()
        }

        fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _state: &mut UiNodeState, _rect: Recti) {}

        fn update_on(&mut self, _ctx: &mut InputCtx<'_>, state: &mut UiNodeState, event: &UiInputEvent) -> InputResult {
            let id = state.id();
            let name = match event {
                UiInputEvent::MouseMove { .. } => "mouse_move",
                UiInputEvent::MouseDrag { .. } => "mouse_drag",
                UiInputEvent::MouseDown { .. } => "mouse_down",
                UiInputEvent::MouseUp { .. } => "mouse_up",
                UiInputEvent::Scroll { .. } => "scroll",
                UiInputEvent::KeyDown { .. } => "key_down",
                UiInputEvent::KeyState { .. } => "key_state",
                UiInputEvent::KeyUp { .. } => "key_up",
                UiInputEvent::KeyCodeDown { .. } => "key_code_down",
                UiInputEvent::KeyCodeState { .. } => "key_code_state",
                UiInputEvent::KeyCodeUp { .. } => "key_code_up",
                UiInputEvent::Text { .. } => "text",
            };
            self.log.borrow_mut().push((id, name));
            self.result
        }
    }

    struct EventRecorder {
        seen: Rc<RefCell<Vec<Vec<UiInputEvent>>>>,
        opt: WidgetOption,
    }

    impl EventRecorder {
        fn new(seen: Rc<RefCell<Vec<Vec<UiInputEvent>>>>) -> Self {
            Self { seen, opt: WidgetOption::NONE }
        }

        fn with_opt(seen: Rc<RefCell<Vec<Vec<UiInputEvent>>>>, opt: WidgetOption) -> Self {
            Self { seen, opt }
        }
    }

    impl crate::Widget for EventRecorder {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            Dimensioni::new(10, 10)
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Vec<UiInputEvent>) -> ResourceState {
            self.seen.borrow_mut().push(input);
            ResourceState::NONE
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
    }

    struct ScrollRecorder {
        seen: Rc<RefCell<Vec<Option<Vec2i>>>>,
        opt: WidgetOption,
    }

    impl crate::Widget for ScrollRecorder {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            Dimensioni::new(10, 10)
        }

        fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
            self.seen.borrow_mut().push(ctx.scroll_delta());
            ResourceState::NONE
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
    }

    struct FrameToggle {
        opt: WidgetOption,
        painted: Rc<RefCell<Vec<Recti>>>,
    }

    impl crate::Widget for FrameToggle {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            Dimensioni::new(10, 8)
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
            self.opt.insert(WidgetOption::FRAME);
            ResourceState::CHANGE
        }

        fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
            self.painted.borrow_mut().push(ctx.screen_content_rect());
        }
    }

    #[test]
    fn ui_node_set_conversion_keeps_container_children_off_leaf_widgets() {
        let button = widget_handle(Button::new("child"));
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(10, 20))).column(|tree| {
                tree.widget(button.clone());
            });
        });

        let runtime = TestRuntime::from_ui_nodes(tree);
        let column_node = runtime.roots.first().expect("column node missing");
        let child_node = column_node.children().first().expect("child node missing");

        assert!(matches!(column_node.data, UiNodeData::Container(_)));
        assert!(matches!(child_node.data, UiNodeData::Widget(_)));
        assert!(child_node.children().is_empty());
    }

    #[test]
    fn framed_widget_owns_inside_border_content_clip_and_outer_hit_box() {
        let button = widget_handle(Button::new("framed"));
        let mut button_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            button_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed(20, 12))).widget(&button);
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let style = Style { padding: 0, ..Style::default() };
        let body = rect(50, 60, 20, 12);
        let mut input = Input::default();
        input.mousemove(body.x, body.y + 5);
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.runtime.begin_frame(true);
        runtime.runtime.layout_frame_roots(&mut runtime.roots, &style, atlas.clone(), body);
        runtime.route_input_events(&style, &input);
        runtime.runtime.update_paint_frame(
            &mut runtime.roots,
            crate::RootId::from_raw(1),
            "frame-test",
            &mut runtime.display_list,
            atlas,
            &style,
            &input,
            &mut results,
            body,
        );

        let node = runtime.node(button_id).expect("button node missing");
        assert_eq!(rect_key(node.state.layout.allocation), (0, 0, 20, 12));
        assert_eq!(rect_key(node.state.layout.children.clip), (1, 1, 18, 10));
        assert!(node.state.hovered, "the inside border remains part of the hit target");

        let border = style.colors[crate::ControlColor::Border as usize];
        let border_ops: Vec<_> = runtime
            .display_list
            .debug_fill_rects()
            .into_iter()
            .filter(|(_, _, color)| color_key(*color) == color_key(border))
            .collect();
        assert_eq!(border_ops.len(), 4);
        assert_eq!(
            border_ops.iter().map(|(rect, _, _)| rect_key(*rect)).collect::<Vec<_>>(),
            vec![(50, 60, 20, 1), (50, 71, 20, 1), (50, 61, 1, 10), (69, 61, 1, 10)]
        );
        assert!(border_ops.iter().all(|(_, clip, _)| rect_key(*clip) == rect_key(body)));
    }

    #[test]
    fn frame_option_changes_preferred_outer_size_but_none_keeps_full_content() {
        let framed = widget_handle(Button::with_opt("size", WidgetOption::FRAME));
        let flat = widget_handle(Button::with_opt("size", WidgetOption::NONE));
        let tree = UiNodeBuilder::build(|tree| {
            tree.widget(&framed);
            tree.widget(&flat);
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let style = Style::default();
        let available = Dimensioni::new(100, 100);
        let framed_size = runtime.runtime.measure_node_ref(&runtime.roots[0], &style, &atlas, available);
        let flat_size = runtime.runtime.measure_node_ref(&runtime.roots[1], &style, &atlas, available);
        assert_eq!(framed_size.width, flat_size.width + style.frame_border_width * 2);
        assert_eq!(framed_size.height, flat_size.height + style.frame_border_width * 2);

        runtime.runtime.layout_node_ref(&mut runtime.roots[0], &style, &atlas, rect(7, 9, 30, 14));
        runtime.runtime.layout_node_ref(&mut runtime.roots[1], &style, &atlas, rect(7, 30, 30, 14));
        assert_eq!(rect_key(runtime.roots[0].state.layout.children.clip), (1, 1, 28, 12));
        assert_eq!(rect_key(runtime.roots[1].state.layout.children.clip), (0, 0, 30, 14));
    }

    #[test]
    fn framed_scroll_area_intersects_its_child_viewport_with_frame_content() {
        let mut scroll_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            scroll_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed(40, 24))).scroll_area(
                ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
                |tree| {
                    tree.text("inside");
                },
            );
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let style = Style::default();
        runtime.runtime.layout_node_ref(&mut runtime.roots[0], &style, &atlas, rect(4, 6, 40, 24));

        let scroll = runtime.node(scroll_id).expect("scroll area missing");
        assert_eq!(rect_key(scroll.state.layout.allocation), (4, 6, 40, 24));
        assert_eq!(rect_key(scroll.state.layout.children.clip), (1, 1, 38, 22));
        let viewport = scroll.children().first().expect("scroll viewport missing");
        assert!(viewport.state.layout.allocation.x >= 1);
        assert!(viewport.state.layout.allocation.y >= 1);
        assert!(viewport.state.layout.allocation.x + viewport.state.layout.allocation.width <= 39);
        assert!(viewport.state.layout.allocation.y + viewport.state.layout.allocation.height <= 23);
    }

    #[test]
    fn scroll_area_without_frame_keeps_the_complete_allocation_as_content() {
        let mut scroll_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            scroll_id = tree
                .node(crate::NodeOptions::with_policy(Policy::fixed(40, 24)))
                .scroll_area(ScrollAreaOption::ENABLE_SCROLL, |_| {});
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let style = Style::default();
        runtime.runtime.layout_node_ref(&mut runtime.roots[0], &style, &atlas, rect(4, 6, 40, 24));

        let scroll = runtime.node(scroll_id).expect("scroll area missing");
        assert_eq!(rect_key(scroll.state.layout.allocation), (4, 6, 40, 24));
        assert_eq!(rect_key(scroll.state.layout.children.clip), (0, 0, 40, 24));
    }

    #[test]
    fn framed_custom_renderer_receives_derived_content_area() {
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(120, 100));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let callback_seen = seen.clone();
        let custom_renderer = renderer
            .register_custom_renderer(move |_frame, args| {
                callback_seen.borrow_mut().push((rect_key(args.content_area), rect_key(args.view)));
            })
            .expect("custom renderer registration");
        let state = widget_handle(Custom::with_opt("custom", WidgetOption::FRAME));
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(20, 12)))
                .custom_render(&state, custom_renderer);
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let style = Style { padding: 0, ..Style::default() };
        let mut results = FrameResults::default();
        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "custom-frame-test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(50, 60, 20, 12),
            true,
        );

        assert_eq!(&*seen.borrow(), &[((51, 61, 18, 10), (51, 61, 18, 10))]);
    }

    #[test]
    fn post_update_layout_observes_a_changed_frame_option_before_paint() {
        let painted = Rc::new(RefCell::new(Vec::new()));
        let state = widget_handle(FrameToggle {
            opt: WidgetOption::NONE,
            painted: painted.clone(),
        });
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(20, 12))).widget(&state);
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(120, 100));
        let style = Style { padding: 0, ..Style::default() };
        let mut results = FrameResults::default();
        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "dynamic-frame-test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(50, 60, 20, 12),
            true,
        );

        let painted = painted.borrow();
        assert_eq!(painted.len(), 1);
        assert_eq!(rect_key(painted[0]), (51, 61, 18, 10));
    }

    #[test]
    fn framed_widget_receives_content_local_pointer_coordinates() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let widget = widget_handle(EventRecorder::with_opt(seen.clone(), WidgetOption::FRAME));
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(20, 12))).widget(&widget);
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(120, 100));
        let style = Style { padding: 0, ..Style::default() };
        let input = Input {
            mouse_pos: Vec2i::new(55, 65),
            mouse_down: MouseButton::LEFT,
            mouse_pressed: MouseButton::LEFT,
            ..Input::default()
        };
        let mut results = FrameResults::default();
        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "content-local-input-test",
            &mut renderer,
            &style,
            &input,
            &mut results,
            rect(50, 60, 20, 12),
            true,
        );

        assert!(
            seen.borrow()[0]
                .iter()
                .any(|event| { matches!(event, UiInputEvent::MouseDown { pos, .. } if (pos.x, pos.y) == (4, 4)) })
        );
    }

    #[test]
    fn grab_scroll_widget_option_delivers_hovered_scroll_delta() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let widget = widget_handle(ScrollRecorder {
            seen: seen.clone(),
            opt: WidgetOption::GRAB_SCROLL,
        });
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(20, 12))).widget(&widget);
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(120, 100));
        let style = Style { padding: 0, ..Style::default() };
        let input = Input {
            mouse_pos: Vec2i::new(55, 65),
            scroll_delta: Vec2i::new(3, -7),
            ..Input::default()
        };
        let mut results = FrameResults::default();
        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "grab-scroll-input-test",
            &mut renderer,
            &style,
            &input,
            &mut results,
            rect(50, 60, 20, 12),
            true,
        );

        let seen = seen.borrow();
        let delta = seen[0].expect("grab-scroll widget did not receive a scroll delta");
        assert_eq!((delta.x, delta.y), (3, -7));
    }

    fn rect_key(rect: Recti) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.width, rect.height)
    }

    fn color_key(color: crate::Color) -> (u8, u8, u8, u8) {
        (color.r, color.g, color.b, color.a)
    }

    #[test]
    fn key_text_input_routes_only_to_focused_node() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = TestRuntime::new();
        runtime.roots.push(UiNode::new(
            Id::new(1),
            crate::Policy::auto(),
            UiNodeData::Widget(Box::new(RecordingBehavior::new(log.clone(), InputResult::Consumed))),
        ));
        runtime.roots.push(UiNode::new(
            Id::new(2),
            crate::Policy::auto(),
            UiNodeData::Widget(Box::new(RecordingBehavior::new(log.clone(), InputResult::Consumed))),
        ));
        runtime.focus = Some(Id::new(2));

        let mut input = Input::default();
        input.text("x");
        input.keydown(KeyMode::CTRL);
        assert!(runtime.route_input_events(&Style::default(), &input));

        assert_eq!(&*log.borrow(), &[(Id::new(2), "key_down"), (Id::new(2), "text"), (Id::new(2), "key_state")]);
    }

    #[test]
    fn retained_widget_key_text_comes_from_focused_routed_event() {
        let focused_seen = Rc::new(RefCell::new(Vec::new()));
        let unfocused_seen = Rc::new(RefCell::new(Vec::new()));
        let focused_widget = widget_handle(EventRecorder::new(focused_seen.clone()));
        let unfocused_widget = widget_handle(EventRecorder::new(unfocused_seen.clone()));
        let mut focused_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.column(|tree| {
                tree.widget(&unfocused_widget);
                focused_id = tree.widget(&focused_widget);
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        runtime.focus = Some(focused_id);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(120, 80));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();
        let mut input = Input::default();
        input.text("x");
        input.keydown(KeyMode::CTRL);

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &input,
            &mut results,
            rect(0, 0, 120, 80),
            true,
        );

        let focused = focused_seen.borrow();
        assert!(focused[0].iter().any(|event| matches!(event, UiInputEvent::Text { text } if text == "x")));
        assert!(
            focused[0]
                .iter()
                .any(|event| matches!(event, UiInputEvent::KeyDown { key } if key.intersects(KeyMode::CTRL)))
        );

        let unfocused = unfocused_seen.borrow();
        assert!(!unfocused[0].iter().any(UiInputEvent::is_focus_input));
    }

    #[test]
    fn pointer_capture_routes_without_hover_and_clears_on_release() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = TestRuntime::new();
        let child_a = UiNode::new(
            Id::new(2),
            crate::Policy::auto(),
            UiNodeData::Widget(Box::new(RecordingBehavior::new(log.clone(), InputResult::Captured))),
        );
        let child_b = UiNode::new(
            Id::new(3),
            crate::Policy::auto(),
            UiNodeData::Widget(Box::new(RecordingBehavior::new(log.clone(), InputResult::Ignored))),
        );
        runtime.roots.push(UiNode::new(
            Id::new(1),
            crate::Policy::auto(),
            UiNodeData::Container(Box::new(Column { children: vec![child_a, child_b] })),
        ));
        runtime.z_order.push(Id::new(1));
        runtime.pointer_input_enabled = true;

        let mut input = Input::default();
        input.mousedown(10, 10, MouseButton::LEFT);
        assert!(runtime.route_input_events(&Style::default(), &input));
        assert_eq!(runtime.capture, Some(Id::new(2)));
        input.epilogue();

        runtime.pointer_input_enabled = false;
        input.mousemove(20, 10);
        input.prelude();
        assert!(runtime.route_input_events(&Style::default(), &input));
        assert_eq!(runtime.capture, Some(Id::new(2)));
        input.epilogue();

        input.mouseup(20, 10, MouseButton::LEFT);
        assert!(runtime.route_input_events(&Style::default(), &input));
        assert_eq!(runtime.capture, None);

        assert_eq!(
            &*log.borrow(),
            &[
                (Id::new(3), "mouse_down"),
                (Id::new(2), "mouse_down"),
                (Id::new(2), "mouse_drag"),
                (Id::new(2), "mouse_up"),
            ]
        );
    }

    #[test]
    fn replacing_projection_drops_absent_nodes_and_transient_state() {
        let button = widget_handle(Button::new("removed"));
        let mut removed_id = Id::default();
        let first = UiNodeBuilder::build(|tree| {
            removed_id = tree.widget(button.clone());
        });
        let mut runtime = TestRuntime::from_ui_nodes(first);
        runtime.focus = Some(removed_id);
        runtime.hover = Some(removed_id);
        runtime.capture = Some(removed_id);

        runtime.replace_ui_nodes(UiNodeBuilder::build(|_| {}));

        assert!(!runtime.contains_node(removed_id));
        assert_eq!(runtime.focus, None);
        assert_eq!(runtime.hover, None);
        assert_eq!(runtime.capture, None);
    }

    #[test]
    fn final_root_honors_auto_and_remainder_height_policies() {
        let style = Style::default();
        let atlas = test_atlas();
        let client = rect(7, 11, 180, 120);

        let auto_button = widget_handle(Button::new("auto"));
        let auto_tree = UiNodeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                tree.widget(&auto_button);
            });
        });
        let mut auto_runtime = TestRuntime::from_ui_nodes(auto_tree);
        let auto_preferred = auto_runtime
            .runtime
            .measure_node_ref(&auto_runtime.roots[0], &style, &atlas, Dimensioni::new(client.width, client.height));
        auto_runtime
            .runtime
            .layout_roots_in_view(&mut auto_runtime.roots, &style, atlas.clone(), client);

        assert!(auto_preferred.height < client.height, "test requires spare client height");
        assert_eq!(auto_runtime.roots[0].state.layout.allocation.height, auto_preferred.height);

        let fill_button = widget_handle(Button::new("fill"));
        let fill_tree = UiNodeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |tree| {
                tree.widget(&fill_button);
            });
        });
        let mut fill_runtime = TestRuntime::from_ui_nodes(fill_tree);
        fill_runtime.runtime.layout_roots_in_view(&mut fill_runtime.roots, &style, atlas, client);

        assert_eq!(fill_runtime.roots[0].state.layout.allocation.height, client.height);
    }

    #[test]
    fn node_window_chrome_offsets_layout_body() {
        let button = widget_handle(Button::new("bbbb"));
        let mut button_id = Id::default();
        let tree = UiNodeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                button_id = tree.widget(button.clone());
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(400, 500));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(40, 40 + style.title_height, 300, 450 - style.title_height),
            true,
        );

        let button_rect = runtime.node_screen_rect(button_id).expect("button node missing");
        let button_layout = runtime.node(button_id).expect("button node missing").state.layout.allocation;
        assert_eq!(button_layout.x, 0, "child allocation is local to its parent node");
        assert!(button_rect.y > 40 + style.title_height);
        assert_eq!(button_rect.x, 40 + style.padding);
        assert!(button_rect.width > 250);
    }

    #[test]
    fn node_calculator_grid_uses_weighted_tracks() {
        let display = widget_handle(Textbox::with_opt(
            "0",
            WidgetOption::FRAME | WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT,
        ));
        let buttons: Vec<_> = (0..20).map(|_| widget_handle(Button::new("b"))).collect();
        let button_ids = std::cell::RefCell::new(Vec::new());
        let tree = UiNodeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Fraction(0.20), |tree| {
                tree.widget(&display);
            });
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |tree| {
                tree.column(|tree| {
                    let columns = [SizePolicy::Weight(1.0); 4];
                    let rows = [SizePolicy::Weight(1.0); 5];
                    tree.grid(&columns, &rows, |tree| {
                        for button in &buttons {
                            button_ids.borrow_mut().push(tree.widget(button));
                        }
                    });
                });
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(320, 420));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 320, 420),
            true,
        );

        let ids = button_ids.borrow();
        let first = runtime.node_screen_rect(ids[0]).unwrap();
        let fourth = runtime.node_screen_rect(ids[3]).unwrap();
        let fifth = runtime.node_screen_rect(ids[4]).unwrap();
        assert!(first.width > 60);
        assert_eq!(first.y, fourth.y);
        assert!(fourth.x > first.x);
        assert!(fifth.y > first.y);
    }

    #[test]
    fn node_row_remainder_tracks_resolve_left_to_right() {
        let label = widget_handle(ListItem::with_opt("Test buttons 2:", WidgetOption::NO_INTERACT));
        let middle = widget_handle(Button::with_opt("Button 3", WidgetOption::FRAME | WidgetOption::ALIGN_CENTER));
        let right = widget_handle(Button::with_opt("Popup", WidgetOption::FRAME | WidgetOption::ALIGN_CENTER));
        let mut middle_id = Id::new(0);
        let mut right_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            let widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
            tree.row(&widths, SizePolicy::Auto, |tree| {
                tree.widget(&label);
                middle_id = tree.widget(&middle);
                right_id = tree.widget(&right);
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(320, 120));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 235, 100),
            true,
        );

        let middle_rect = runtime.node_screen_rect(middle_id).unwrap();
        let right_rect = runtime.node_screen_rect(right_id).unwrap();
        assert!(middle_rect.width > 0);
        assert!(right_rect.width > middle_rect.width);
        assert!(right_rect.x >= middle_rect.x + middle_rect.width + style.spacing);
    }

    #[test]
    fn node_content_size_includes_nested_stack_overflow() {
        let small = widget_handle(Button::new("slot"));
        let image = widget_handle(Button::new("image"));
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(67), StackDirection::TopToBottom, |tree| {
                tree.widget(&small);
                tree.stack(SizePolicy::Fixed(256), SizePolicy::Fixed(256), StackDirection::TopToBottom, |tree| {
                    tree.widget(&image);
                });
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(320, 160));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 300, 120),
            true,
        );

        let root_node = &runtime.roots[0];
        assert!(root_node.state.layout.content_size.height >= 67 + style.spacing + 256);
    }

    #[test]
    fn node_scroll_area_keeps_runtime_content_and_scroll_state() {
        let atlas = test_atlas();
        let style = Rc::new(Style::default());
        let first = widget_handle(Button::new("first"));
        let rest: Vec<_> = (0..5).map(|_| widget_handle(Button::new("row"))).collect();
        let mut first_id = Id::new(0);
        let mut scroll_area_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            scroll_area_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed(120, 48))).scroll_area(
                ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
                |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(24), StackDirection::TopToBottom, |tree| {
                        first_id = tree.widget(&first);
                        for button in &rest {
                            tree.widget(button);
                        }
                    });
                },
            );
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        set_scroll_area_scroll(&mut runtime.roots, scroll_area_id, Vec2i::new(0, 36));
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(180, 100));
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            style.as_ref(),
            &Input::default(),
            &mut results,
            rect(0, 0, 160, 80),
            true,
        );

        let first_node = runtime.node(first_id).unwrap();
        let first_rect = first_node.state.layout.allocation;
        let first_screen_rect = runtime.node_screen_rect(first_id).unwrap();
        let scroll_state = scroll_area_state(&runtime.roots, scroll_area_id).unwrap();
        let body = scroll_state.body;
        let content = scroll_state.content_size;
        let scroll = scroll_state.scroll;
        assert!(content.height > body.height);
        assert!(scroll.y > 0);
        assert!(first_rect.y >= 0);
        assert!(first_screen_rect.y < body.y);
        let scroll_node = runtime.node(scroll_area_id).unwrap();
        assert_eq!(scroll_node.children().len(), 4);
        let viewport_node = &scroll_node.children()[0];
        assert_eq!(viewport_node.children().len(), 1);
    }

    #[test]
    fn root_body_view_is_stable_for_identical_size_without_root_scrollbars() {
        let buttons: Vec<_> = (0..6).map(|_| widget_handle(Button::new("wide row"))).collect();
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(NodeOptions::with_policy(Policy::new(SizePolicy::Auto, SizePolicy::Remainder(0))))
                .stack(SizePolicy::Fixed(150), SizePolicy::Fixed(24), StackDirection::TopToBottom, |tree| {
                    for button in &buttons {
                        tree.widget(button);
                    }
                });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(220, 160));
        let mut style = Style::default();
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        let body = rect(0, 0, 100, 80);

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            body,
            true,
        );
        let first_client = runtime.roots[0].state.layout.allocation;
        let first_content = runtime.roots[0].state.layout.content_size;

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            body,
            true,
        );
        let second_client = runtime.roots[0].state.layout.allocation;
        let second_content = runtime.roots[0].state.layout.content_size;

        assert!(same_rect(first_client, second_client));
        assert_eq!(first_client.width, body.width - style.padding * 2);
        assert_eq!(first_client.height, body.height - style.padding * 2);
        assert!(first_content.width > first_client.width || first_content.height > first_client.height);
        assert_eq!(first_content.width, second_content.width);
        assert_eq!(first_content.height, second_content.height);
    }

    #[test]
    fn root_full_viewport_custom_render_does_not_overflow_from_padding() {
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(220, 160));
        let custom_renderer = renderer.register_custom_renderer(|_frame, _args| {}).unwrap();
        let custom = widget_handle(Custom::new("viewport"));
        let mut custom_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                custom_id = tree.custom_render(&custom, custom_renderer);
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let mut style = Style::default();
        style.padding = 6;
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        let body = rect(0, 0, 120, 90);

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            body,
            true,
        );

        let root = &runtime.roots[0];
        let custom_rect = runtime.node_screen_rect(custom_id).unwrap();
        assert_eq!(root.state.layout.allocation.width, body.width - style.padding * 2);
        assert_eq!(root.state.layout.allocation.height, body.height - style.padding * 2);
        assert_eq!(custom_rect.width, root.state.layout.allocation.width);
        assert_eq!(custom_rect.height, root.state.layout.allocation.height);
        assert!(root.state.layout.content_size.width <= root.state.layout.allocation.width);
        assert!(root.state.layout.content_size.height <= root.state.layout.allocation.height);
    }

    #[test]
    fn scroll_area_reveals_icon_button_after_scrolling_to_icon_section() {
        let pixels = [255, 255, 255, 255];
        let chars = [(
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &chars,
            },
        )];
        let icons = [("white", Recti::new(0, 0, 1, 1))];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
        });
        let icon_button = widget_handle(Button::with_icon("icon", crate::WHITE_ICON, WidgetOption::FRAME, WidgetFillOption::ALL));
        let filler = widget_handle(Button::new("filler"));
        let mut scroll_area_id = Id::new(0);
        let mut icon_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            scroll_area_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 70))).scroll_area(
                ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
                |tree| {
                    tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 180))).widget(filler.clone());
                    icon_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 40))).widget(icon_button.clone());
                },
            );
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(140, 80));
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 120, 70),
            true,
        );

        set_scroll_area_scroll(&mut runtime.roots, scroll_area_id, Vec2i::new(0, 160));
        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 120, 70),
            true,
        );
        let root = &runtime.roots[0];
        let icon_node = runtime.node(icon_id).unwrap();
        let icon_rect = runtime.node_screen_rect(icon_id).unwrap();
        let scroll_state = scroll_area_state(&runtime.roots, scroll_area_id).unwrap();
        let body = scroll_state.body;
        let scroll = scroll_state.scroll;
        let icon_is_visible = icon_rect.x < body.x + body.width
            && icon_rect.x + icon_rect.width > body.x
            && icon_rect.y < body.y + body.height
            && icon_rect.y + icon_rect.height > body.y;
        assert!(
            icon_is_visible,
            "icon not visible; root client {:?} content {:?} scroll body {:?} scroll {:?} icon rect {:?} clip {:?}",
            root.state.layout.allocation, root.state.layout.content_size, body, scroll, icon_rect, icon_node.state.layout.children.clip
        );
    }

    #[test]
    fn node_fixed_width_image_button_derives_height_from_aspect_ratio() {
        let pixels = vec![255; 80 * 80 * 4];
        let chars = [(
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &chars,
            },
        )];
        let icons = [("white", Recti::new(0, 0, 1, 1))];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 80,
            height: 80,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
        });
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(200, 140));
        let texture = renderer.try_load_texture_rgba(64, 64, &[255; 64 * 64 * 4]).unwrap();
        let button = widget_handle(Button::with_scaled_image("image", Some(texture), WidgetOption::FRAME, WidgetFillOption::ALL));
        let mut button_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                button_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed_width(48))).widget(&button);
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 160, 120),
            true,
        );

        let button_rect = runtime.node_screen_rect(button_id).unwrap();
        assert_eq!(button_rect.width, 48);
        assert_eq!(button_rect.height, 48);
    }

    #[test]
    fn node_fixed_width_regular_texture_button_keeps_inline_height() {
        let pixels = vec![255; 80 * 80 * 4];
        let chars = [(
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &chars,
            },
        )];
        let icons = [("white", Recti::new(0, 0, 1, 1))];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 80,
            height: 80,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
        });
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(300, 160));
        let texture = renderer.try_load_texture_rgba(64, 64, &[255; 64 * 64 * 4]).unwrap();
        let button = widget_handle(Button::with_image("image", Some(texture), WidgetOption::FRAME, WidgetFillOption::ALL));
        let mut button_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                button_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed_width(256))).widget(&button);
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 280, 140),
            true,
        );

        let button_rect = runtime.node_screen_rect(button_id).unwrap();
        assert_eq!(button_rect.width, 256);
        assert!(
            button_rect.height < 100,
            "regular texture button should stay inline-sized, got {:?}",
            button_rect
        );
    }

    #[test]
    fn node_grid_honors_explicit_child_spans() {
        let first = widget_handle(Button::new("a"));
        let second = widget_handle(Button::new("b"));
        let third = widget_handle(Button::new("c"));
        let mut first_id = Id::new(0);
        let mut second_id = Id::new(0);
        let mut third_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            let columns = [SizePolicy::Fixed(40), SizePolicy::Fixed(50), SizePolicy::Fixed(60)];
            let rows = [SizePolicy::Fixed(20), SizePolicy::Fixed(20)];
            tree.grid(&columns, &rows, |tree| {
                first_id = tree.node(crate::NodeOptions::with_policy(Policy::fill()).grid_span(2, 1)).widget(first.clone());
                second_id = tree.node(crate::NodeOptions::with_policy(Policy::fill())).widget(second.clone());
                third_id = tree.node(crate::NodeOptions::with_policy(Policy::fill())).widget(third.clone());
            });
        });
        let mut runtime = TestRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut renderer = Renderer::new_test(backend, Dimensioni::new(220, 80));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut renderer,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 220, 80),
            true,
        );

        let first_rect = runtime.node_screen_rect(first_id).unwrap();
        let second_rect = runtime.node_screen_rect(second_id).unwrap();
        let third_rect = runtime.node_screen_rect(third_id).unwrap();
        assert_eq!(first_rect.width, 40 + style.spacing + 50);
        assert_eq!(second_rect.width, 60);
        assert!(second_rect.x > first_rect.x);
        assert_eq!(third_rect.x, first_rect.x);
        assert!(third_rect.y > first_rect.y);
    }
}
