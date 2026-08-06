//! Cross-phase runtime characterization.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::*;
use crate::test_support::test_atlas;
use crate::ui_node::children::ChildrenHandle;
use crate::{ChildParticipation, Children, ContainerSurface, Layout, Widget, WidgetPaintCtx, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx};
use crate::input::Input;

#[derive(Default)]
struct ProbeCounts {
    measures: Cell<usize>,
    updates: Cell<usize>,
    paints: Cell<usize>,
    routed_events: Cell<usize>,
    hovered: Cell<bool>,
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

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        self.counts.updates.set(self.counts.updates.get() + 1);
        self.counts.routed_events.set(self.counts.routed_events.get() + usize::from(input.is_some()));
        self.counts.hovered.set(ctx.hovered());
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
    children: ChildrenHandle,
    visible: bool,
}

impl WidgetState for TraversalState {}

struct TraversalLayout {
    state: Rc<RefCell<TraversalState>>,
}

struct TraversalSurface {
    state: Weak<RefCell<TraversalState>>,
    hide_during_update: bool,
    log: Rc<RefCell<Vec<String>>>,
    opt: WidgetOption,
}

struct TraversalContainer;

impl TraversalContainer {
    fn new(children: impl IntoIterator<Item = Node>, hide_during_update: bool, log: Rc<RefCell<Vec<String>>>) -> (Container, Rc<RefCell<TraversalState>>) {
        let children = Rc::new(RefCell::new(children.into_iter().collect()));
        let state = Rc::new(RefCell::new(TraversalState {
            children: ChildrenHandle::new(&children),
            visible: true,
        }));
        let container = Container::from_shared(children, TraversalLayout { state: state.clone() }, WidgetOption::NONE);
        let surface = TraversalSurface {
            state: Rc::downgrade(&state),
            hide_during_update,
            log,
            opt: WidgetOption::NONE,
        };
        (container.with_surface(surface), state)
    }
}

impl Layout for TraversalLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        (0..children.len())
            .filter_map(|index| children.measure_child(index, style, atlas, available))
            .fold(Dimensioni::default(), |size, child| {
                Dimensioni::new(size.width.max(child.width), size.height.max(child.height))
            })
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        let visible = self.state.try_borrow().expect("traversal state must be available during layout").visible;
        for index in 0..children.len() {
            let participation = if visible { ChildParticipation::Active } else { ChildParticipation::Hidden };
            let _ = ctx.set_child_participation(children, index, participation);
            if visible {
                let _ = ctx.layout_child(children, index, rect);
            }
        }
    }
}

impl Widget for TraversalSurface {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::default()
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        self.log.borrow_mut().push("container:update".to_owned());
        if self.hide_during_update
            && let Some(state) = self.state.upgrade()
        {
            state.try_borrow_mut().expect("traversal state must be available during update").visible = false;
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.log.borrow_mut().push("container:paint".to_owned());
    }
}

impl ContainerSurface for TraversalSurface {}

struct CaptureState {
    active: bool,
    drags: usize,
    saw_capture_during_drag: bool,
}

impl WidgetState for CaptureState {}

struct CaptureLayout {
    _state: Rc<RefCell<CaptureState>>,
}

struct CaptureSurface {
    state: Weak<RefCell<CaptureState>>,
    opt: WidgetOption,
}

struct CaptureContainer;

impl CaptureContainer {
    fn new() -> (Container, Rc<RefCell<CaptureState>>) {
        let state = Rc::new(RefCell::new(CaptureState {
            active: false,
            drags: 0,
            saw_capture_during_drag: false,
        }));
        let layout = CaptureLayout { _state: state.clone() };
        let surface = CaptureSurface {
            state: Rc::downgrade(&state),
            opt: WidgetOption::NONE,
        };
        (Container::new(layout, WidgetOption::NONE, []).with_surface(surface), state)
    }
}

impl Layout for CaptureLayout {
    fn measure(&self, _children: &Children, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 20)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, _children: &mut Children, rect: Recti) {
        ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
    }
}

impl Widget for CaptureSurface {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::default()
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let state = self.state.upgrade().expect("capture state must outlive its surface");
        let mut state = state.try_borrow_mut().expect("capture state must be available during update");
        // Mirror the public contract: reconcile private drag state from runtime-owned activity on
        // every update, including an eventless update after capture invalidation.
        state.active = ctx.active();
        if let Some(event) = input {
            match event {
                UiInputEvent::MouseDrag { .. } if ctx.active() => {
                    state.saw_capture_during_drag = ctx.focused();
                    state.drags += 1;
                }
                _ => {}
            }
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::DragCapture
    }
}

impl ContainerSurface for CaptureSurface {}

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
                .try_clear()
                .expect("target topology must be available during sibling update");
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
    let (container, _) = TraversalContainer::new([Node::widget(first), Node::widget(second)], false, log.clone());
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
fn layout_participation_filters_descendants_after_state_changes() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (child, child_counts) = Probe::new("child", log.clone());
    let (container, _) = TraversalContainer::new([Node::widget(child)], true, log.clone());
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    log.borrow_mut().clear();
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    // Visibility is a layout result, so commit the state change before paint consumes the flag.
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas);

    let expected = ["container:update", "child:update", "container:paint"].map(str::to_owned);
    assert_eq!(log.borrow().as_slice(), expected.as_slice());
    assert_eq!((child_counts.updates.get(), child_counts.paints.get()), (1, 0));
}

#[test]
fn overlapping_pointer_routing_visits_siblings_in_reverse_z_order() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, first_counts) = Probe::new("first", log.clone());
    let (second, second_counts) = Probe::new("second", log.clone());
    let (container, _) = TraversalContainer::new([Node::widget(first), Node::widget(second)], false, log);
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
    assert_eq!(routed.map(|(_, result)| result), Some(DispatchResult::Captured));
    runtime.update_tree_root(&mut root, &style, atlas, empty_input());

    assert_eq!(first_counts.routed_events.get(), 0);
    assert_eq!(second_counts.routed_events.get(), 1);
    assert!(!first_counts.hovered.get());
    assert!(second_counts.hovered.get());
}

#[test]
fn pointer_target_selection_uses_reverse_sibling_paint_order() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, _) = Probe::new("first", log.clone());
    let (second, _) = Probe::new("second", log.clone());
    let first_id = Node::widget(first);
    let first_id_value = first_id.id();
    let second_id = Node::widget(second);
    let second_id_value = second_id.id();
    let (container, _) = TraversalContainer::new([first_id, second_id], false, log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    let event = UiInputEvent::MouseMove {
        pos: Vec2i::new(20, 30),
        delta: Vec2i::default(),
    };
    runtime.begin_input_event(true, &event);
    let target = runtime
        .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &event)
        .map(|(owner, _)| owner);
    assert_eq!(target, Some(second_id_value));
    assert_ne!(target, Some(first_id_value));
}

#[test]
fn no_interact_node_is_transparent_to_pointer_target_selection() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, _) = Probe::new("first", log.clone());
    let (mut second, _) = Probe::new("second", log.clone());
    second.opt = WidgetOption::NO_INTERACT;
    let first = Node::widget(first);
    let first_id = first.id();
    let (container, _) = TraversalContainer::new([first, Node::widget(second)], false, log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    let event = UiInputEvent::MouseMove {
        pos: Vec2i::new(20, 30),
        delta: Vec2i::default(),
    };
    runtime.begin_input_event(true, &event);
    assert_eq!(
        runtime
            .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &event)
            .map(|(owner, _)| owner),
        Some(first_id)
    );
}

#[test]
fn composite_header_is_targeted_as_a_real_child_surface() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (lower, lower_counts) = Probe::new("lower", log.clone());
    let lower = Node::widget(lower).with_policy(Policy::fill());
    let lower_id = lower.id();
    let (_, disclosure) = crate::Disclosure::create(crate::DisclosureParameters::header("Header", true, std::iter::empty()));
    let disclosure = disclosure.with_policy(Policy::fill());
    let disclosure_id = disclosure.id();
    let (container, _) = TraversalContainer::new([lower, disclosure], false, log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());

    let disclosure_rect = runtime
        .debug_node_rect(std::slice::from_ref(&root), disclosure_id)
        .expect("laid-out disclosure must retain a screen allocation");
    let event = UiInputEvent::MouseMove {
        // The header is now a concrete child widget placed at the top of the allocation.
        pos: Vec2i::new(disclosure_rect.x + 1, disclosure_rect.y + 1),
        delta: Vec2i::default(),
    };
    runtime.begin_input_event(true, &event);
    let routed = runtime.route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &event);
    assert!(routed.is_some(), "the dispatcher must route to the explicit header child");
    assert_ne!(runtime.hover, Some(disclosure_id), "the structural disclosure must not impersonate its header");
    assert_ne!(runtime.hover, Some(lower_id), "the covered sibling must remain occluded by the header child");
    runtime.update_tree_root(&mut root, &style, atlas, empty_input());
    assert_eq!(lower_counts.routed_events.get(), 0, "a covered sibling must not receive the bubbled event");
}

#[test]
fn ignored_topmost_pointer_target_never_exposes_a_covered_sibling() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (mut lower, lower_counts) = Probe::new("lower", log.clone());
    lower.opt = WidgetOption::GRAB_SCROLL;
    let (upper, upper_counts) = Probe::new("upper", log.clone());
    let (container, _) = TraversalContainer::new([Node::widget(lower), Node::widget(upper)], false, log);
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
    assert_eq!(routed.map(|(_, result)| result), Some(DispatchResult::Ignored));
    runtime.update_tree_root(&mut root, &style, atlas, empty_input());

    assert_eq!(upper_counts.routed_events.get(), 0, "unsupported events are not delivered to the target update");
    assert_eq!(
        lower_counts.routed_events.get(),
        0,
        "the covered sibling must never be considered after the hit"
    );
    assert!(upper_counts.hovered.get(), "the geometric target remains hovered when its event bubbles");
    assert!(!lower_counts.hovered.get());
}

#[test]
fn widget_focus_policy_is_authoritative_after_dispatch_cleanup() {
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
fn captured_container_receives_direct_drag_while_capture_is_active() {
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
    assert_eq!(runtime.hover, Some(id));
    runtime.update_tree_root(&mut root, &style, atlas.clone(), down_state);
    assert!(state.borrow().active);

    input.mousemove(200, 180);
    let (drag, drag_state) = next_input(&mut input);
    runtime.begin_input_event(true, &drag);
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, drag_state.mouse_buttons, &drag,),
        Some(true)
    );
    assert_eq!(runtime.hover, None, "capture delivery outside the owner's pure surface must not imply hover");

    runtime.update_tree_root(&mut root, &style, atlas, drag_state);
    assert_eq!(runtime.capture, Some(id));
    assert!(state.borrow().active);
    assert!(state.borrow().saw_capture_during_drag);
    assert_eq!(state.borrow().drags, 1);
}

#[test]
fn routing_time_release_exposes_inactive_state_during_that_event_update() {
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
    assert!(state.borrow().active, "local state changes only during the ordered update traversal");

    runtime.update_tree_root(&mut root, &style, atlas, release_state);
    assert!(!state.borrow().active);
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
    assert!(!state.borrow().active);

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
}

#[test]
fn ancestor_gate_clears_targets_and_next_active_update_reconciles_local_mode() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (captured, capture_state) = CaptureContainer::new();
    capture_state.borrow_mut().active = true;
    let captured = Node::container(captured);
    let captured_id = captured.id();
    let (gate, gate_state) = TraversalContainer::new([captured], false, log);
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
    assert!(capture_state.borrow().active, "a gated widget is not mutated outside ordered update");

    gate_state.borrow_mut().visible = true;
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    assert_eq!(runtime.capture, None, "expansion must not restore old capture");
    runtime.update_tree_root(&mut root, &style, test_atlas(), empty_input());
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
    let (parent, parent_state) = TraversalContainer::new([removed], false, log);
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

    assert!(parent_state.borrow_mut().children.try_replace([replacement]).is_ok());
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    assert_eq!((runtime.focus, runtime.hover, runtime.capture), (None, None, None));
    assert!(runtime.take_routed_event(removed_id).is_none());
    assert!(runtime.take_routed_event(replacement_id).is_none());
    assert!(
        removed_state.borrow().active,
        "removed runtimes are dropped rather than mutated through a callback"
    );
    assert!(!replacement_state.borrow().active);

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
    let (target_parent, target_state) = TraversalContainer::new([captured], false, log.clone());
    let remover = CrossSubtreeRemover {
        state: Rc::new(RefCell::new(())),
        target: target_state,
        removed: false,
        opt: WidgetOption::NONE,
    };
    let (root_container, _) = TraversalContainer::new([Node::widget(remover), Node::container(target_parent)], false, log);
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
    assert!(captured_state.borrow().active, "removed runtime must not receive out-of-band mutation");
}
