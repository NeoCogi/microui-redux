//! Cross-phase runtime characterization.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use crate::test_support::test_atlas;
use crate::{
    Children, ChildrenVisitor, ChildrenVisitorMut, ContainerState, Widget, WidgetPaintCtx, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};
use crate::input::Input;

#[derive(Default)]
struct ProbeCounts {
    measures: Cell<usize>,
    updates: Cell<usize>,
    paints: Cell<usize>,
    routed_events: Cell<usize>,
}

struct Probe {
    state: Rc<RefCell<()>>,
    name: &'static str,
    counts: Rc<ProbeCounts>,
    log: Rc<RefCell<Vec<String>>>,
    opt: WidgetOption,
}

impl Probe {
    fn new(name: &'static str, log: Rc<RefCell<Vec<String>>>) -> (Self, Rc<ProbeCounts>) {
        let counts = Rc::new(ProbeCounts::default());
        (
            Self {
                state: Rc::new(RefCell::new(())),
                name,
                counts: counts.clone(),
                log,
                opt: WidgetOption::NONE,
            },
            counts,
        )
    }
}

impl WidgetStateOwner for Probe {
    type State = ();

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for Probe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
        self.counts.measures.set(self.counts.measures.get() + 1);
        Dimensioni::new(17, 13)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.counts.updates.set(self.counts.updates.get() + 1);
        self.counts.routed_events.set(self.counts.routed_events.get() + usize::from(input.is_some()));
        self.log.borrow_mut().push(format!("{}:update", self.name));
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.counts.paints.set(self.counts.paints.get() + 1);
        self.log.borrow_mut().push(format!("{}:paint", self.name));
    }
}

struct HoldFocusProbe {
    state: Rc<RefCell<()>>,
    opt: WidgetOption,
}

impl WidgetStateOwner for HoldFocusProbe {
    type State = ();

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for HoldFocusProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 20)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::HoldUntilBlur
    }
}

struct TraversalState {
    children: Children,
    visible: bool,
}

impl WidgetState for TraversalState {}
impl ContainerState for TraversalState {}

struct TraversalContainer {
    state: Rc<RefCell<TraversalState>>,
    hide_during_update: bool,
    log: Rc<RefCell<Vec<String>>>,
    opt: WidgetOption,
}

impl TraversalContainer {
    fn new(children: impl IntoIterator<Item = Node>, hide_during_update: bool, log: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            state: Rc::new(RefCell::new(TraversalState {
                children: children.into_iter().collect(),
                visible: true,
            })),
            hide_during_update,
            log,
            opt: WidgetOption::NONE,
        }
    }
}

impl WidgetStateOwner for TraversalContainer {
    type State = TraversalState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for TraversalContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        let state = self.state.try_borrow().expect("traversal state must be available during measure");
        (0..state.children.len())
            .filter_map(|index| state.children.measure_child(index, style, atlas, available))
            .fold(Dimensioni::default(), |size, child| {
                Dimensioni::new(size.width.max(child.width), size.height.max(child.height))
            })
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        self.log.borrow_mut().push("container:update".to_owned());
        if self.hide_during_update {
            self.state.try_borrow_mut().expect("traversal state must be available during update").visible = false;
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.log.borrow_mut().push("container:paint".to_owned());
    }
}

impl Container for TraversalContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        let state = self.state.try_borrow().expect("traversal state must be available during immutable visitation");
        visitor.visit(&state.children);
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        let mut state = self
            .state
            .try_borrow_mut()
            .expect("traversal state must be available during mutable visitation");
        visitor.visit(&mut state.children);
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        let mut state = self.state.try_borrow_mut().expect("traversal state must be available during layout");
        if state.visible {
            for index in 0..state.children.len() {
                let _ = ctx.layout_child(&mut state.children, index, rect);
            }
        }
    }

    fn children_visible(&self) -> bool {
        self.state
            .try_borrow()
            .expect("traversal state must be available for the visibility gate")
            .visible
    }
}

struct CaptureState {
    children: Children,
    active: bool,
    losses: usize,
    drags: usize,
    saw_capture_during_drag: bool,
}

impl WidgetState for CaptureState {}
impl ContainerState for CaptureState {}

struct CaptureContainer {
    state: Rc<RefCell<CaptureState>>,
    opt: WidgetOption,
}

impl CaptureContainer {
    fn new() -> (Self, Rc<RefCell<CaptureState>>) {
        let state = Rc::new(RefCell::new(CaptureState {
            children: Children::new(),
            active: false,
            losses: 0,
            drags: 0,
            saw_capture_during_drag: false,
        }));
        (
            Self {
                state: state.clone(),
                opt: WidgetOption::NONE,
            },
            state,
        )
    }
}

impl WidgetStateOwner for CaptureContainer {
    type State = CaptureState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for CaptureContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 20)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let mut state = self.state.try_borrow_mut().expect("capture state must be available during update");
        if let Some(event) = input {
            match event {
                UiInputEvent::MouseDown { button, .. } if button.intersects(MouseButton::LEFT) => state.active = true,
                UiInputEvent::MouseDrag { buttons, .. } if buttons.intersects(MouseButton::LEFT) && state.active => state.drags += 1,
                UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT) => state.active = false,
                _ => {}
            }
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::DragCapture
    }
}

impl Container for CaptureContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        let state = self.state.try_borrow().expect("capture state must be available during visitation");
        visitor.visit(&state.children);
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        let mut state = self.state.try_borrow_mut().expect("capture state must be available during mutable visitation");
        visitor.visit(&mut state.children);
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
    }

    fn retains_pointer_capture(&self) -> bool {
        self.state.try_borrow().expect("capture state must be available for retention").active
    }

    fn on_pointer_capture_lost(&mut self) {
        let mut state = self.state.try_borrow_mut().expect("capture state must be available for loss notification");
        state.active = false;
        state.losses += 1;
    }

    fn route_input(&mut self, ctx: &mut ContainerInputCtx<'_>, event: &UiInputEvent) -> ContainerInputResult {
        if matches!(event, UiInputEvent::MouseDrag { .. }) && ctx.has_pointer_capture() {
            self.state
                .try_borrow_mut()
                .expect("capture state must be available during routing")
                .saw_capture_during_drag = true;
        }
        ctx.route_widget(event, self.opt)
    }
}

struct CrossSubtreeRemover {
    state: Rc<RefCell<()>>,
    target: Rc<RefCell<TraversalState>>,
    removed: bool,
    opt: WidgetOption,
}

impl WidgetStateOwner for CrossSubtreeRemover {
    type State = ();

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for CrossSubtreeRemover {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(10, 10)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        if !self.removed {
            self.target
                .try_borrow_mut()
                .expect("cross-subtree target state must be independently available")
                .children
                .clear();
            self.removed = true;
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

fn layout_root(runtime: &mut UiRuntime, root: &mut Node, style: &Style, atlas: crate::AtlasHandle) {
    runtime.layout_tree_root(root, style, atlas, Recti::new(10, 20, 80, 60), Recti::new(0, 0, 320, 240));
}

fn empty_input() -> InputSnapshot {
    Input::default().snapshot()
}

fn next_input(input: &mut Input) -> (UiInputEvent, InputSnapshot) {
    let event = input.pop_event().expect("test input must contain one queued event");
    (event, input.snapshot())
}

#[test]
fn leaf_layout_reuses_one_authoritative_widget_measurement() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (probe, counts) = Probe::new("leaf", log);
    let mut root = Node::widget(probe);
    let mut runtime = UiRuntime::new();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &Style::default(), test_atlas());

    assert_eq!(counts.measures.get(), 1);
    assert_eq!(runtime.debug_metrics().measures, 1);
    assert_eq!((root.state.layout.content_size.width, root.state.layout.content_size.height), (80, 60));
}

#[test]
fn common_phases_are_parent_first_and_siblings_are_forward() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, first_counts) = Probe::new("first", log.clone());
    let (second, second_counts) = Probe::new("second", log.clone());
    let container = TraversalContainer::new([Node::widget(first), Node::widget(second)], false, log.clone());
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    log.borrow_mut().clear();
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas);

    let expected = [
        "container:update",
        "first:update",
        "second:update",
        "container:paint",
        "first:paint",
        "second:paint",
    ]
    .map(str::to_owned);
    assert_eq!(log.borrow().as_slice(), expected.as_slice());
    assert_eq!((first_counts.updates.get(), first_counts.paints.get()), (1, 1));
    assert_eq!((second_counts.updates.get(), second_counts.paints.get()), (1, 1));
}

#[test]
fn post_update_visibility_gate_suppresses_descendants_in_the_same_frame() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (child, child_counts) = Probe::new("child", log.clone());
    let container = TraversalContainer::new([Node::widget(child)], true, log.clone());
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    log.borrow_mut().clear();
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas);

    let expected = ["container:update", "container:paint"].map(str::to_owned);
    assert_eq!(log.borrow().as_slice(), expected.as_slice());
    assert_eq!((child_counts.updates.get(), child_counts.paints.get()), (0, 0));
}

#[test]
fn overlapping_pointer_routing_visits_siblings_in_reverse_z_order() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, first_counts) = Probe::new("first", log.clone());
    let (second, second_counts) = Probe::new("second", log.clone());
    let container = TraversalContainer::new([Node::widget(first), Node::widget(second)], false, log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    let event = UiInputEvent::MouseDown {
        pos: Vec2i::new(20, 30),
        button: MouseButton::LEFT,
    };
    runtime.begin_input_event(true, &event);
    let routed = runtime.route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &event);
    assert_eq!(routed.map(|(_, result)| result), Some(ContainerInputResult::Captured));
    runtime.update_tree_root(&mut root, &style, atlas, empty_input());

    assert_eq!(first_counts.routed_events.get(), 0);
    assert_eq!(second_counts.routed_events.get(), 1);
}

#[test]
fn pointer_hit_selection_uses_reverse_sibling_paint_order() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, _) = Probe::new("first", log.clone());
    let (second, _) = Probe::new("second", log.clone());
    let first_id = Node::widget(first);
    let first_id_value = first_id.id();
    let second_id = Node::widget(second);
    let second_id_value = second_id.id();
    let container = TraversalContainer::new([first_id, second_id], false, log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    let hit = runtime.hit_test_pointer_node_ref(&root, runtime.root_transform(), &style, Vec2i::new(20, 30));
    assert_eq!(hit, Some(second_id_value));
    assert_ne!(hit, Some(first_id_value));
}

#[test]
fn no_interact_node_is_transparent_to_pointer_hit_selection() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, _) = Probe::new("first", log.clone());
    let (mut second, _) = Probe::new("second", log.clone());
    second.opt = WidgetOption::NO_INTERACT;
    let first = Node::widget(first);
    let first_id = first.id();
    let container = TraversalContainer::new([first, Node::widget(second)], false, log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    assert_eq!(
        runtime.hit_test_pointer_node_ref(&root, runtime.root_transform(), &style, Vec2i::new(20, 30)),
        Some(first_id)
    );
}

#[test]
fn ignored_topmost_pointer_target_never_exposes_a_covered_sibling() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (mut lower, lower_counts) = Probe::new("lower", log.clone());
    lower.opt = WidgetOption::GRAB_SCROLL;
    let (upper, upper_counts) = Probe::new("upper", log.clone());
    let container = TraversalContainer::new([Node::widget(lower), Node::widget(upper)], false, log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    let event = UiInputEvent::Scroll {
        pos: Vec2i::new(20, 30),
        delta: Vec2i::new(0, 1),
    };
    runtime.begin_input_event(true, &event);
    let routed = runtime.route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &event);
    assert_eq!(routed.map(|(_, result)| result), Some(ContainerInputResult::Ignored));
    runtime.update_tree_root(&mut root, &style, atlas, empty_input());

    assert_eq!(upper_counts.routed_events.get(), 0, "unsupported events are not delivered to the target update");
    assert_eq!(
        lower_counts.routed_events.get(),
        0,
        "the covered sibling must never be considered after the hit"
    );
}

#[test]
fn widget_focus_policy_is_authoritative_after_container_routing_cleanup() {
    let mut root = Node::widget(HoldFocusProbe {
        state: Rc::new(RefCell::new(())),
        opt: WidgetOption::NONE,
    });
    let id = root.id();
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());

    let mut input = Input::default();
    input.mousedown(20, 30, MouseButton::LEFT);
    let (down, down_state) = next_input(&mut input);
    runtime.begin_input_event(true, &down);
    let (owner, result) = runtime
        .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &down)
        .expect("pointer-down must route to the focus probe");
    runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
    runtime.update_tree_root(&mut root, &style, atlas.clone(), down_state);
    assert_eq!(runtime.focus, Some(id));

    input.mouseup(20, 30, MouseButton::LEFT);
    let (release, release_state) = next_input(&mut input);
    runtime.begin_input_event(true, &release);
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, release_state.mouse_buttons, &release),
        Some(true)
    );
    runtime.update_tree_root(&mut root, &style, atlas, release_state);

    assert_eq!(runtime.capture, None);
    assert_eq!(runtime.focus, Some(id), "the Widget override, not a routing helper argument, must retain focus");
}

#[test]
fn captured_container_reports_local_retention_and_receives_loss_notification() {
    let (container, state) = CaptureContainer::new();
    let mut root = Node::container(container);
    let id = root.id();
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());

    let mut input = Input::default();
    input.mousedown(20, 30, MouseButton::LEFT);
    let (down, down_state) = next_input(&mut input);
    runtime.begin_input_event(true, &down);
    let (owner, result) = runtime
        .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &down)
        .expect("container pointer-down must route");
    runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
    assert_eq!(runtime.capture, Some(id));
    runtime.update_tree_root(&mut root, &style, atlas.clone(), down_state);
    assert!(state.borrow().active);

    input.mousemove(200, 180);
    let (drag, drag_state) = next_input(&mut input);
    runtime.begin_input_event(true, &drag);
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, drag_state.mouse_buttons, &drag,),
        Some(true)
    );
    assert!(state.borrow().saw_capture_during_drag);

    runtime.update_tree_root(&mut root, &style, atlas, drag_state);
    assert_eq!(runtime.capture, Some(id));
    assert!(state.borrow().active);
    assert_eq!(state.borrow().drags, 1);

    state.borrow_mut().active = false;
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    assert_eq!(runtime.capture, None);
    assert_eq!(state.borrow().losses, 1);
    assert!(!state.borrow().active);
}

#[test]
fn routing_time_release_defers_loss_until_that_event_update_finishes() {
    let (container, state) = CaptureContainer::new();
    state.borrow_mut().active = true;
    let mut root = Node::container(container);
    let id = root.id();
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.capture = Some(id);

    let mut release_input = Input::default();
    release_input.mousedown(20, 30, MouseButton::LEFT);
    let _ = release_input.pop_event();
    release_input.mouseup(200, 180, MouseButton::LEFT);
    let (release, release_state) = next_input(&mut release_input);
    runtime.begin_input_event(true, &release);
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, release_state.mouse_buttons, &release,),
        Some(true)
    );
    assert_eq!(runtime.capture, None);
    assert_eq!(runtime.capture_loss_after_update, Some(id));
    assert!(state.borrow().active, "loss must wait until the release update");
    assert_eq!(state.borrow().losses, 0);

    runtime.update_tree_root(&mut root, &style, atlas, release_state);
    assert!(!state.borrow().active);
    assert_eq!(state.borrow().losses, 1);
    assert_eq!(runtime.capture_loss_after_update, None);
}

#[test]
fn a_new_press_after_release_starts_a_distinct_capture_event() {
    let (container, state) = CaptureContainer::new();
    state.borrow_mut().active = true;
    let mut root = Node::container(container);
    let id = root.id();
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.capture = Some(id);

    let mut release_input = Input::default();
    release_input.mousedown(20, 30, MouseButton::LEFT);
    let _ = release_input.pop_event();
    release_input.mouseup(20, 30, MouseButton::LEFT);
    let (release, release_state) = next_input(&mut release_input);
    runtime.begin_input_event(true, &release);
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, release_state.mouse_buttons, &release,),
        Some(true)
    );
    runtime.update_tree_root(&mut root, &style, atlas.clone(), release_state);
    assert_eq!(runtime.capture, None);
    assert_eq!(state.borrow().losses, 1);

    let mut down_input = Input::default();
    down_input.mousedown(20, 30, MouseButton::LEFT);
    let (down, down_state) = next_input(&mut down_input);
    runtime.begin_input_event(true, &down);
    let (owner, result) = runtime
        .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &down)
        .expect("same target must reacquire capture");
    runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
    assert_eq!(runtime.capture, Some(id));

    runtime.update_tree_root(&mut root, &style, atlas, down_state);
    assert_eq!(runtime.capture, Some(id));
    assert!(state.borrow().active);
    assert_eq!(state.borrow().losses, 1);
}

#[test]
fn ancestor_gate_clears_all_descendant_targets_and_local_capture_mode() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (captured, capture_state) = CaptureContainer::new();
    capture_state.borrow_mut().active = true;
    let captured = Node::container(captured);
    let captured_id = captured.id();
    let gate = TraversalContainer::new([captured], false, log);
    let gate_state = gate.state.clone();
    let mut root = Node::container(gate);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    runtime.focus = Some(captured_id);
    runtime.hover = Some(captured_id);
    runtime.capture = Some(captured_id);
    runtime.push_routed_event(
        captured_id,
        UiInputEvent::MouseMove {
            pos: Vec2i::new(20, 30),
            delta: Vec2i::default(),
        },
    );

    gate_state.borrow_mut().visible = false;
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    assert_eq!((runtime.focus, runtime.hover, runtime.capture), (None, None, None));
    assert!(runtime.take_routed_event(captured_id).is_none());
    assert!(!capture_state.borrow().active);
    assert_eq!(capture_state.borrow().losses, 1);

    gate_state.borrow_mut().visible = true;
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    assert_eq!(runtime.capture, None, "expansion must not restore old capture");
    assert!(!capture_state.borrow().active, "expansion must not restore old local mode");
}

#[test]
fn removed_target_does_not_notify_or_transfer_state_to_same_index_replacement() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (removed, removed_state) = CaptureContainer::new();
    removed_state.borrow_mut().active = true;
    let removed = Node::container(removed);
    let removed_id = removed.id();
    let (replacement, replacement_state) = CaptureContainer::new();
    let replacement = Node::container(replacement);
    let replacement_id = replacement.id();
    let parent = TraversalContainer::new([removed], false, log);
    let parent_state = parent.state.clone();
    let mut root = Node::container(parent);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    runtime.focus = Some(removed_id);
    runtime.hover = Some(removed_id);
    runtime.capture = Some(removed_id);
    runtime.push_routed_event(
        removed_id,
        UiInputEvent::MouseMove {
            pos: Vec2i::new(20, 30),
            delta: Vec2i::default(),
        },
    );

    parent_state.borrow_mut().children.replace([replacement]);
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    assert_eq!((runtime.focus, runtime.hover, runtime.capture), (None, None, None));
    assert!(runtime.take_routed_event(removed_id).is_none());
    assert!(runtime.take_routed_event(replacement_id).is_none());
    assert_eq!(removed_state.borrow().losses, 0, "removed runtimes are dropped rather than notified");
    assert!(!replacement_state.borrow().active);
    assert_eq!(replacement_state.borrow().losses, 0);

    let mut drag_input = Input::default();
    drag_input.mousedown(20, 30, MouseButton::LEFT);
    let drag = UiInputEvent::MouseDrag {
        pos: Vec2i::new(20, 30),
        delta: Vec2i::new(2, 3),
        buttons: MouseButton::LEFT,
    };
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, MouseButton::LEFT, &drag),
        Some(false),
        "the stale drag must be swallowed while awaiting its release"
    );
    assert!(runtime.discard_invalidated_capture_events);

    let mut release_input = Input::default();
    release_input.mouseup(20, 30, MouseButton::LEFT);
    let release = UiInputEvent::MouseUp {
        pos: Vec2i::new(20, 30),
        button: MouseButton::LEFT,
    };
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, MouseButton::NONE, &release),
        Some(false),
        "the stale release must be swallowed instead of falling back to replacement hit routing"
    );
    assert!(!runtime.discard_invalidated_capture_events);
    assert!(runtime.take_routed_event(replacement_id).is_none());
}

#[test]
fn cross_subtree_removal_during_update_sanitizes_before_later_delivery() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (captured, captured_state) = CaptureContainer::new();
    captured_state.borrow_mut().active = true;
    let captured = Node::container(captured);
    let captured_id = captured.id();
    let target_parent = TraversalContainer::new([captured], false, log.clone());
    let target_state = target_parent.state.clone();
    let remover = CrossSubtreeRemover {
        state: Rc::new(RefCell::new(())),
        target: target_state,
        removed: false,
        opt: WidgetOption::NONE,
    };
    let root_container = TraversalContainer::new([Node::widget(remover), Node::container(target_parent)], false, log);
    let mut root = Node::container(root_container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.focus = Some(captured_id);
    runtime.hover = Some(captured_id);
    runtime.capture = Some(captured_id);
    runtime.push_routed_event(
        captured_id,
        UiInputEvent::MouseMove {
            pos: Vec2i::new(20, 30),
            delta: Vec2i::default(),
        },
    );

    runtime.update_tree_root(&mut root, &style, atlas, empty_input());

    assert_eq!((runtime.focus, runtime.hover, runtime.capture), (None, None, None));
    assert!(runtime.take_routed_event(captured_id).is_none());
    assert_eq!(captured_state.borrow().losses, 0, "removed runtime must not receive a loss callback");
}
