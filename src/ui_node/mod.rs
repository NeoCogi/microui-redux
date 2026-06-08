//! Common runtime node model used by the next retained traversal path.
//!
//! Internal retained UI node runtime.
//! It gives the crate one node representation that can own either a leaf widget or a framework
//! container without introducing a broad container trait before the enum-based passes exist.
//!
//! Topology mutation is immediate but phase-limited. Initialization may build child membership
//! directly, and `UpdateCtx` may add new children or remove direct children/subtrees. Measure,
//! layout, paint, and scroll dispatch observe child membership without mutating it. Reparenting is
//! intentionally unsupported: once a node is attached, it has exactly one parent until removal.
#![allow(dead_code)]

use crate::{
    expand_rect, Canvas, CustomRenderArgs, CustomRenderCommand, Dimensioni, FrameResults, Input, GridSpan, KeyCode, KeyMode, MouseButton, Recti, Renderer,
    RetainedId, Style, UiNodeSet, Vec2i, Vertex, UNCLIPPED_RECT,
};
use crate::render_command::{render_command_stream, Command};
use crate::id::IdNamespace;
use crate::input::{ContainerOption, ScrollBehavior, WidgetOption};
use crate::sizing::SizePolicy;
use crate::widget::FocusPolicy;
use crate::widget_ctx::WidgetCtx;
use crate::window_manager::TreeCustomRender;

mod node;
pub(crate) use node::{ContentSpace, NodeLayout, TraversalState, UiNode, UiNodeData, UiNodeId, UiNodeKind};
mod runtime;
pub(crate) use runtime::UiRuntime;
mod containers;
pub(crate) use containers::{
    scroll_viewport_node, scrollbar_nodes, shared_scroll_area_state, Column, Disclosure, Grid, InputCtx, InputResult, LayoutCtx, MeasureCtx, NodeBehavior,
    PaintCtx, RootWindow, Row, ScrollArea, Stack, UpdateCtx, WidgetNode,
};
#[cfg(test)]
pub(crate) use containers::ScrollAreaState;
pub use containers::UiInputEvent;

/// Command wrapper that lets node-runtime custom render callbacks enter the backend stream.
struct NodeCustomRenderCommand {
    /// Shared retained callback invoked during renderer replay.
    render: TreeCustomRender,
}

impl CustomRenderCommand for NodeCustomRenderCommand {
    fn render(&mut self, dim: Dimensioni, args: &CustomRenderArgs) {
        self.render.borrow_mut().render(dim, args);
    }
}

/// Stable internal id for the synthetic root-window container.
fn runtime_root_id() -> UiNodeId {
    IdNamespace::UINODE_ROOT.id([0])
}

/// Internal node context used by future container passes.
///
/// This context is read/layout-state oriented. Topology mutation is intentionally limited to
/// initialization and `UpdateCtx`; measure, layout, paint, and scroll dispatch should observe child
/// membership without mutating it.
pub(crate) struct NodeCtx<'a> {
    /// Runtime owning the node graph.
    runtime: &'a mut UiRuntime,
    /// Current node id.
    id: UiNodeId,
}

impl<'a> NodeCtx<'a> {
    /// Returns the current node id.
    pub(crate) fn id(&self) -> UiNodeId {
        self.id
    }

    /// Returns the parent node id, if any.
    pub(crate) fn parent(&self) -> Option<UiNodeId> {
        self.runtime.nodes.get(&self.id).and_then(|node| node.parent)
    }

    /// Returns the current container children, or an empty slice for leaf widgets.
    pub(crate) fn children(&self) -> &[UiNodeId] {
        self.runtime.nodes.get(&self.id).map(UiNode::children).unwrap_or(&[])
    }

    /// Returns the current full node rect.
    pub(crate) fn rect(&self) -> Recti {
        self.runtime.nodes.get(&self.id).map(|node| node.rect).unwrap_or_default()
    }

    /// Returns the current node client rect.
    pub(crate) fn client(&self) -> Recti {
        self.runtime.nodes.get(&self.id).map(|node| node.layout.control).unwrap_or_default()
    }

    /// Returns the current effective clip rect.
    pub(crate) fn clip(&self) -> Recti {
        self.runtime.nodes.get(&self.id).map(|node| node.layout.content.viewport).unwrap_or_default()
    }

    /// Updates the current full node rect.
    pub(crate) fn set_rect(&mut self, value: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.rect = value;
            node.set_layout(NodeLayout { frame: value, ..node.layout });
        }
    }

    /// Updates the current node client rect.
    pub(crate) fn set_client(&mut self, value: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.set_layout(node.layout.with_control(value));
        }
    }

    /// Updates the current effective clip rect.
    pub(crate) fn set_clip(&mut self, value: Recti) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.set_layout(NodeLayout {
                content: ContentSpace { viewport: value, ..node.layout.content },
                ..node.layout
            });
        }
    }

    /// Updates the current measured content size.
    pub(crate) fn set_content_size(&mut self, value: Dimensioni) {
        if let Some(node) = self.runtime.nodes.get_mut(&self.id) {
            node.set_layout(node.layout.with_content_size(value));
        }
    }
}

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
    let frame = node.layout.frame;
    let content_size = node.layout.content_size;
    Recti::new(frame.x, frame.y, frame.width.max(content_size.width), frame.height.max(content_size.height))
}

/// Builds pointer events from raw frame input.
pub(super) fn pointer_events_from_input(input: &Input) -> Vec<UiInputEvent> {
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
pub(super) fn focus_events_from_input(input: &Input) -> Vec<UiInputEvent> {
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

/// Builds widget-visible frame events from raw input.
pub(super) fn frame_events_from_input(input: &Input) -> Vec<UiInputEvent> {
    let mut events = pointer_events_from_input(input);
    events.extend(held_events_from_input(input));
    events
}

/// Builds all events relevant to a focused custom render node.
pub(super) fn custom_render_events_from_input(input: &Input, focused: bool) -> Vec<UiInputEvent> {
    let mut events = frame_events_from_input(input);
    if focused {
        events.extend(focus_events_from_input(input));
    }
    events
}

/// Converts the retained focus slot used by `WidgetCtx` back to a node id.
fn retained_focus_to_node(focus: Option<RetainedId>) -> Option<UiNodeId> {
    match focus {
        Some(RetainedId::Node(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn events_key_mods(events: &[UiInputEvent]) -> KeyMode {
    events.iter().fold(KeyMode::NONE, |keys, event| match event {
        UiInputEvent::KeyState { keys: state } => keys | *state,
        _ => keys,
    })
}

pub(super) fn events_key_codes(events: &[UiInputEvent]) -> KeyCode {
    events.iter().fold(KeyCode::NONE, |keys, event| match event {
        UiInputEvent::KeyCodeState { codes } => keys | *codes,
        _ => keys,
    })
}

pub(super) fn events_text(events: &[UiInputEvent]) -> String {
    let mut text = String::new();
    for event in events {
        if let UiInputEvent::Text { text: event_text } = event {
            text.push_str(event_text);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    use crate::{
        color4b, rect, AtlasHandle, AtlasSource, Button, Canvas, CharEntry, Custom, FontEntry, Id, Image, Input, KeyMode, ListItem, Policy, RendererHandle,
        ResourceState, SourceFormat, StackDirection, Textbox, WidgetFillOption, WidgetOption, UiNodeBuilder, widget_handle,
    };
    use crate::test_support::{test_atlas, NoopRenderer};

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
        fn measure(&self, _ctx: &MeasureCtx<'_>, _id: UiNodeId, _available: Dimensioni) -> Dimensioni {
            Dimensioni::default()
        }

        fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _id: UiNodeId, _rect: Recti, _clip: Recti) {}

        fn update_on(&mut self, _ctx: &mut InputCtx<'_>, id: UiNodeId, event: &UiInputEvent) -> InputResult {
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
    }

    impl crate::Widget for EventRecorder {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            Dimensioni::new(10, 10)
        }

        fn update(&mut self, ctx: &mut WidgetCtx<'_>) -> ResourceState {
            self.seen.borrow_mut().push(ctx.input_events().to_vec());
            ResourceState::NONE
        }

        fn paint(&mut self, _ctx: &mut WidgetCtx<'_>) {}
    }

    #[test]
    fn ui_node_set_conversion_keeps_container_children_off_leaf_widgets() {
        let button = widget_handle(Button::new("child"));
        let tree = UiNodeBuilder::build(|tree| {
            tree.node(crate::NodeOptions::with_policy(Policy::fixed(10, 20))).column(|tree| {
                tree.widget(button.clone());
            });
        });

        let runtime = UiRuntime::from_ui_nodes(tree);
        let root = runtime.roots[0];
        let root_node = runtime.nodes.get(&root).expect("root node missing");
        let column = root_node.children()[0];
        let column_node = runtime.nodes.get(&column).expect("column node missing");
        let child = column_node.children()[0];
        let child_node = runtime.nodes.get(&child).expect("child node missing");

        assert!(matches!(root_node.data, UiNodeData::Branch { .. }));
        assert!(matches!(column_node.data, UiNodeData::Branch { .. }));
        assert!(matches!(child_node.data, UiNodeData::Leaf { .. }));
        assert!(child_node.children().is_empty());
    }

    #[test]
    fn immediate_insert_rejects_already_parented_child() {
        let parent = UiNode::new(
            Id::new(1),
            None,
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: Vec::new(),
            },
        );
        let child = UiNode::new(
            Id::new(2),
            Some(Id::new(99)),
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: Vec::new(),
            },
        );

        let mut runtime = UiRuntime::new();
        runtime.nodes.insert(parent.id, parent);

        assert_eq!(runtime.insert_child_immediate(Id::new(1), child, 0), None);
        assert!(runtime.nodes.get(&Id::new(1)).unwrap().children().is_empty());
        assert!(!runtime.nodes.contains_key(&Id::new(2)));
    }

    #[test]
    fn immediate_insert_sets_parent_membership() {
        let parent = UiNode::new(
            Id::new(1),
            None,
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: Vec::new(),
            },
        );
        let child = UiNode::new(
            Id::new(2),
            None,
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: Vec::new(),
            },
        );

        let mut runtime = UiRuntime::new();
        runtime.nodes.insert(parent.id, parent);

        assert_eq!(runtime.insert_child_immediate(Id::new(1), child, 0), Some(Id::new(2)));
        assert_eq!(runtime.nodes.get(&Id::new(1)).unwrap().children(), &[Id::new(2)]);
        assert_eq!(runtime.nodes.get(&Id::new(2)).unwrap().parent, Some(Id::new(1)));
    }

    #[test]
    fn key_text_input_routes_only_to_focused_node() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new();
        runtime.nodes.insert(
            Id::new(1),
            UiNode::new(
                Id::new(1),
                None,
                crate::Policy::auto(),
                GridSpan::ONE,
                UiNodeData::Leaf {
                    behavior: Box::new(RecordingBehavior::new(log.clone(), InputResult::Consumed)),
                },
            ),
        );
        runtime.nodes.insert(
            Id::new(2),
            UiNode::new(
                Id::new(2),
                None,
                crate::Policy::auto(),
                GridSpan::ONE,
                UiNodeData::Leaf {
                    behavior: Box::new(RecordingBehavior::new(log.clone(), InputResult::Consumed)),
                },
            ),
        );
        runtime.focus = Some(Id::new(2));

        let mut input = Input::default();
        input.text("x");
        input.keydown(KeyMode::CTRL);
        assert!(runtime.route_input_events(&Style::default(), &input));

        assert_eq!(&*log.borrow(), &[(Id::new(2), "key_down"), (Id::new(2), "text")]);
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
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        runtime.focus = Some(focused_id);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(120, 80));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();
        let mut input = Input::default();
        input.text("x");
        input.keydown(KeyMode::CTRL);

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
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
        let mut runtime = UiRuntime::new();
        runtime.nodes.insert(
            Id::new(1),
            UiNode::new(
                Id::new(1),
                None,
                crate::Policy::auto(),
                GridSpan::ONE,
                UiNodeData::Branch {
                    behavior: Box::new(RecordingBehavior::new(log.clone(), InputResult::Ignored)),
                    children: vec![Id::new(2), Id::new(3)],
                },
            ),
        );
        runtime.nodes.insert(
            Id::new(2),
            UiNode::new(
                Id::new(2),
                Some(Id::new(1)),
                crate::Policy::auto(),
                GridSpan::ONE,
                UiNodeData::Leaf {
                    behavior: Box::new(RecordingBehavior::new(log.clone(), InputResult::Captured)),
                },
            ),
        );
        runtime.nodes.insert(
            Id::new(3),
            UiNode::new(
                Id::new(3),
                Some(Id::new(1)),
                crate::Policy::auto(),
                GridSpan::ONE,
                UiNodeData::Leaf {
                    behavior: Box::new(RecordingBehavior::new(log.clone(), InputResult::Ignored)),
                },
            ),
        );
        runtime.roots.push(Id::new(1));
        runtime.z_order.push(Id::new(1));
        runtime.hover_root_active = true;

        let mut input = Input::default();
        input.mousedown(10, 10, MouseButton::LEFT);
        assert!(runtime.route_input_events(&Style::default(), &input));
        assert_eq!(runtime.capture, Some(Id::new(2)));
        input.epilogue();

        runtime.hover_root_active = false;
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
    fn immediate_remove_deletes_subtree_and_clears_transient_refs() {
        let parent = UiNode::new(
            Id::new(1),
            None,
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: vec![Id::new(2)],
            },
        );
        let child = UiNode::new(
            Id::new(2),
            Some(Id::new(1)),
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: vec![Id::new(3)],
            },
        );
        let grandchild = UiNode::new(
            Id::new(3),
            Some(Id::new(2)),
            crate::Policy::auto(),
            GridSpan::ONE,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: Vec::new(),
            },
        );

        let mut runtime = UiRuntime::new();
        runtime.nodes.insert(parent.id, parent);
        runtime.nodes.insert(child.id, child);
        runtime.nodes.insert(grandchild.id, grandchild);
        runtime.roots.push(Id::new(1));
        runtime.z_order.push(Id::new(1));
        runtime.focus = Some(Id::new(3));
        runtime.hover = Some(Id::new(2));
        runtime.capture = Some(Id::new(3));
        runtime.hover_root = Some(Id::new(2));
        assert!(runtime.remove_child_immediate(Id::new(1), Id::new(2)));

        assert_eq!(runtime.nodes.get(&Id::new(1)).unwrap().children(), &[]);
        assert!(!runtime.nodes.contains_key(&Id::new(2)));
        assert!(!runtime.nodes.contains_key(&Id::new(3)));
        assert_eq!(runtime.focus, None);
        assert_eq!(runtime.hover, None);
        assert_eq!(runtime.capture, None);
        assert_eq!(runtime.hover_root, None);
    }

    #[test]
    fn replacing_projection_drops_absent_nodes_and_transient_state() {
        let button = widget_handle(Button::new("removed"));
        let mut removed_id = Id::default();
        let first = UiNodeBuilder::build(|tree| {
            removed_id = tree.widget(button.clone());
        });
        let mut runtime = UiRuntime::from_ui_nodes(first);
        runtime.focus = Some(removed_id);
        runtime.hover = Some(removed_id);
        runtime.capture = Some(removed_id);

        runtime.replace_ui_nodes(UiNodeBuilder::build(|_| {}));

        assert!(!runtime.nodes.contains_key(&removed_id));
        assert_eq!(runtime.focus, None);
        assert_eq!(runtime.hover, None);
        assert_eq!(runtime.capture, None);
    }

    #[test]
    fn node_window_chrome_offsets_layout_body() {
        let button = widget_handle(Button::new("bbbb"));
        let tree = UiNodeBuilder::build(|tree| {
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                tree.widget(button.clone());
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(400, 500));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(40, 40 + style.title_height, 300, 450 - style.title_height),
            true,
        );

        let button_node = runtime
            .nodes
            .values()
            .find(|node| matches!(node.data, UiNodeData::Leaf { .. }))
            .expect("button node missing");
        assert!(button_node.rect.y > 40 + style.title_height);
        assert_eq!(button_node.rect.x, 40 + style.padding);
        assert!(button_node.rect.width > 250);
    }

    #[test]
    fn node_calculator_grid_uses_weighted_tracks() {
        let display = widget_handle(Textbox::with_opt("0", WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT));
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
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(320, 420));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 320, 420),
            true,
        );

        let ids = button_ids.borrow();
        let first = runtime.nodes.get(&ids[0]).unwrap().rect;
        let fourth = runtime.nodes.get(&ids[3]).unwrap().rect;
        let fifth = runtime.nodes.get(&ids[4]).unwrap().rect;
        assert!(first.width > 60);
        assert_eq!(first.y, fourth.y);
        assert!(fourth.x > first.x);
        assert!(fifth.y > first.y);
    }

    #[test]
    fn node_row_remainder_tracks_resolve_left_to_right() {
        let label = widget_handle(ListItem::with_opt("Test buttons 2:", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME));
        let middle = widget_handle(Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER));
        let right = widget_handle(Button::with_opt("Popup", WidgetOption::ALIGN_CENTER));
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
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(320, 120));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 235, 100),
            true,
        );

        let middle_rect = runtime.nodes.get(&middle_id).unwrap().rect;
        let right_rect = runtime.nodes.get(&right_id).unwrap().rect;
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
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(320, 160));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 300, 120),
            true,
        );

        let root = runtime.roots[0];
        let root_node = runtime.nodes.get(&root).unwrap();
        assert!(root_node.layout.content_size.height >= 67 + style.spacing + 256);
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
            scroll_area_id =
                tree.node(crate::NodeOptions::with_policy(Policy::fixed(120, 48)))
                    .scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                        tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(24), StackDirection::TopToBottom, |tree| {
                            first_id = tree.widget(&first);
                            for button in &rest {
                                tree.widget(button);
                            }
                        });
                    });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        runtime.set_scroll_area_scroll(scroll_area_id, Vec2i::new(0, 36));
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(180, 100));
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            style.as_ref(),
            &Input::default(),
            &mut results,
            rect(0, 0, 160, 80),
            true,
        );

        let first_node = runtime.nodes.get(&first_id).unwrap();
        let first_rect = first_node.rect;
        let first_screen_rect = runtime.traversal_state_for_node(first_id).screen_frame(first_node.layout);
        let scroll_state = runtime.scroll_area_state(scroll_area_id).unwrap();
        let body = scroll_state.body;
        let content = scroll_state.content_size;
        let scroll = scroll_state.scroll;
        assert!(content.height > body.height);
        assert!(scroll.y > 0);
        assert!(first_rect.y >= 0);
        assert!(first_screen_rect.y < body.y);
        let scroll_node = runtime.nodes.get(&scroll_area_id).unwrap();
        assert_eq!(scroll_node.children().len(), 4);
        let viewport_node = runtime.nodes.get(&scroll_node.children()[0]).unwrap();
        assert_eq!(viewport_node.children().len(), 1);
    }

    #[test]
    fn root_body_view_is_stable_for_identical_size_without_root_scrollbars() {
        let buttons: Vec<_> = (0..6).map(|_| widget_handle(Button::new("wide row"))).collect();
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Fixed(150), SizePolicy::Fixed(24), StackDirection::TopToBottom, |tree| {
                for button in &buttons {
                    tree.widget(button);
                }
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(220, 160));
        let mut style = Style::default();
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        let body = rect(0, 0, 100, 80);

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            body,
            true,
        );
        let first_client = runtime.nodes.get(&runtime.roots[0]).unwrap().layout.control;
        let first_content = runtime.nodes.get(&runtime.roots[0]).unwrap().layout.content_size;

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            body,
            true,
        );
        let second_client = runtime.nodes.get(&runtime.roots[0]).unwrap().layout.control;
        let second_content = runtime.nodes.get(&runtime.roots[0]).unwrap().layout.content_size;

        assert!(same_rect(first_client, second_client));
        assert_eq!(first_client.width, body.width - style.padding * 2);
        assert_eq!(first_client.height, body.height - style.padding * 2);
        assert!(first_content.width > first_client.width || first_content.height > first_client.height);
        assert_eq!(first_content.width, second_content.width);
        assert_eq!(first_content.height, second_content.height);
    }

    #[test]
    fn root_full_viewport_custom_render_does_not_overflow_from_padding() {
        let custom = widget_handle(Custom::new("viewport"));
        let mut custom_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                custom_id = tree.custom_render(&custom, |_dim, _args| {});
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(220, 160));
        let mut style = Style::default();
        style.padding = 6;
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        let body = rect(0, 0, 120, 90);

        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            body,
            true,
        );

        let root = runtime.nodes.get(&runtime.roots[0]).unwrap();
        let custom_rect = runtime.nodes.get(&custom_id).unwrap().rect;
        assert_eq!(root.layout.control.width, body.width - style.padding * 2);
        assert_eq!(root.layout.control.height, body.height - style.padding * 2);
        assert_eq!(custom_rect.width, root.layout.control.width);
        assert_eq!(custom_rect.height, root.layout.control.height);
        assert!(root.layout.content_size.width <= root.layout.control.width);
        assert!(root.layout.content_size.height <= root.layout.control.height);
    }

    #[test]
    fn scroll_area_paints_slot_button_after_scrolling_to_slot_section() {
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
        let slots = [Recti::new(0, 0, 1, 1)];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &slots,
        });
        let slot = atlas.clone_slot_table()[0];
        let paint_count = Rc::new(std::cell::Cell::new(0));
        let paint_count_for_slot = paint_count.clone();
        let slot_button = widget_handle(Button::with_slot(
            "slot",
            slot,
            Rc::new(move |_x, _y| {
                paint_count_for_slot.set(paint_count_for_slot.get() + 1);
                color4b(255, 0, 0, 255)
            }),
            WidgetOption::NONE,
            WidgetFillOption::ALL,
        ));
        let filler = widget_handle(Button::new("filler"));
        let mut scroll_area_id = Id::new(0);
        let mut slot_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            scroll_area_id =
                tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 70)))
                    .scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                        tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 180))).widget(filler.clone());
                        slot_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed(100, 40))).widget(slot_button.clone());
                    });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(140, 80));
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 120, 70),
            true,
        );
        assert_eq!(paint_count.get(), 0);

        runtime.set_scroll_area_scroll(scroll_area_id, Vec2i::new(0, 160));
        results.begin_frame();
        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 120, 70),
            true,
        );
        let root = runtime.nodes.get(&runtime.roots[0]).unwrap();
        let slot_node = runtime.nodes.get(&slot_id).unwrap();
        let scroll_state = runtime.scroll_area_state(scroll_area_id).unwrap();
        let body = scroll_state.body;
        let scroll = scroll_state.scroll;
        assert!(
            paint_count.get() > 0,
            "slot not painted; root client {:?} content {:?} scroll body {:?} scroll {:?} slot rect {:?} clip {:?}",
            root.layout.control,
            root.layout.content_size,
            body,
            scroll,
            slot_node.rect,
            slot_node.layout.content.viewport
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
        let slots = [Recti::new(0, 0, 64, 64)];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 80,
            height: 80,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &slots,
        });
        let slot = atlas.clone_slot_table()[0];
        let button = widget_handle(Button::with_scaled_image(
            "image",
            Some(Image::Slot(slot)),
            WidgetOption::NONE,
            WidgetFillOption::ALL,
        ));
        let mut button_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                button_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed_width(48))).widget(&button);
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(200, 140));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 160, 120),
            true,
        );

        let button_rect = runtime.nodes.get(&button_id).unwrap().rect;
        assert_eq!(button_rect.width, 48);
        assert_eq!(button_rect.height, 48);
    }

    #[test]
    fn node_fixed_width_regular_slot_button_keeps_inline_height() {
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
        let slots = [Recti::new(0, 0, 64, 64)];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 80,
            height: 80,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &slots,
        });
        let slot = atlas.clone_slot_table()[0];
        let button = widget_handle(Button::with_image("image", Some(Image::Slot(slot)), WidgetOption::NONE, WidgetFillOption::ALL));
        let mut button_id = Id::new(0);
        let tree = UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                button_id = tree.node(crate::NodeOptions::with_policy(Policy::fixed_width(256))).widget(&button);
            });
        });
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(300, 160));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 280, 140),
            true,
        );

        let button_rect = runtime.nodes.get(&button_id).unwrap().rect;
        assert_eq!(button_rect.width, 256);
        assert!(button_rect.height < 100, "regular slot button should stay inline-sized, got {:?}", button_rect);
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
        let mut runtime = UiRuntime::from_ui_nodes(tree);
        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut canvas = Canvas::from(renderer, Dimensioni::new(220, 80));
        let style = Style::default();
        let mut results = FrameResults::default();
        results.begin_frame();

        runtime.render_frame(
            crate::RootId::from_raw(1),
            "test",
            &mut canvas,
            &style,
            &Input::default(),
            &mut results,
            rect(0, 0, 220, 80),
            true,
        );

        let first_rect = runtime.nodes.get(&first_id).unwrap().rect;
        let second_rect = runtime.nodes.get(&second_id).unwrap().rect;
        let third_rect = runtime.nodes.get(&third_id).unwrap().rect;
        assert_eq!(first_rect.width, 40 + style.spacing + 50);
        assert_eq!(second_rect.width, 60);
        assert!(second_rect.x > first_rect.x);
        assert_eq!(third_rect.x, first_rect.x);
        assert!(third_rect.y > first_rect.y);
    }
}
