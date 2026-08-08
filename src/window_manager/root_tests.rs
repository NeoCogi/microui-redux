//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

use super::*;

use crate::test_support::{AllocationMeasurement, NoopRenderer, RenderEvent, recording_backend, test_atlas};
use crate::{
    color, rect, AtlasHandle, Button, ButtonParameters, ButtonState, Column, ColumnParameters, ColumnState, Custom, CustomParameters, Dimensioni, Disclosure,
    DisclosureParameters, DisclosureState, Grid, GridParameters, KeyMode, MouseButton, Node, Policy, Row, RowParameters, ScrollArea, ScrollAreaOption,
    ListItem, ListItemParameters, ScrollAreaParameters, SizePolicy, Stack, StackDirection, StackParameters, Style, Textbox, TextboxChanged, TextboxParameters,
    UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};
use crate::render::{FrameInfo, RenderError};
use crate::ui_node::{runtime_read_state, runtime_update_state};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn context() -> Context<NoopRenderer> {
    Context::new_test(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240))
}

fn empty_content() -> Node {
    Column::create(ColumnParameters::default()).1
}

fn frame_info(dimensions: Dimensioni) -> FrameInfo {
    FrameInfo::try_new(dimensions, color(0, 0, 0, 255)).unwrap()
}

#[derive(Default)]
struct OrderedProbeState {
    events: Vec<&'static str>,
    held_buttons: Vec<u32>,
    held_keys: Vec<u32>,
    measures: usize,
    updates: usize,
    paints: usize,
    hovered: bool,
}

impl WidgetState for OrderedProbeState {}

struct OrderedProbe {
    state: Rc<RefCell<OrderedProbeState>>,
    opt: WidgetOption,
}

impl WidgetStateOwner for OrderedProbe {
    type State = OrderedProbeState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for OrderedProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        self.state.borrow_mut().measures += 1;
        Dimensioni::new(80, 60)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, event: Option<&UiInputEvent>) {
        let mut state = self.state.borrow_mut();
        state.updates += 1;
        state.held_buttons.push(ctx.mouse_buttons().bits());
        state.held_keys.push(ctx.key_modes().bits());
        state.hovered = ctx.hovered();
        if let Some(event) = event {
            state.events.push(match event {
                UiInputEvent::MouseMove { .. } => "move",
                UiInputEvent::MouseDrag { .. } => "drag",
                UiInputEvent::MouseDown { .. } => "down",
                UiInputEvent::MouseUp { .. } => "up",
                UiInputEvent::Scroll { .. } => "scroll",
                UiInputEvent::KeyDown { .. } => "key-down",
                UiInputEvent::KeyUp { .. } => "key-up",
                UiInputEvent::KeyCodeDown { .. } => "code-down",
                UiInputEvent::KeyCodeUp { .. } => "code-up",
                UiInputEvent::Text { .. } => "text",
            });
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.state.borrow_mut().paints += 1;
    }
}

struct CommitProbeState {
    intrinsic_height: i32,
    grow_to: Option<i32>,
    presses: usize,
}

impl WidgetState for CommitProbeState {}

struct CommitProbe {
    state: Rc<RefCell<CommitProbeState>>,
    painted_rects: Rc<RefCell<Vec<Recti>>>,
    opt: WidgetOption,
}

impl CommitProbe {
    fn new(intrinsic_height: i32, grow_to: Option<i32>) -> (WidgetStateHandle<CommitProbeState>, Self, Rc<RefCell<Vec<Recti>>>) {
        let painted_rects = Rc::new(RefCell::new(Vec::new()));
        let probe = Self {
            state: Rc::new(RefCell::new(CommitProbeState { intrinsic_height, grow_to, presses: 0 })),
            painted_rects: painted_rects.clone(),
            opt: WidgetOption::NONE,
        };
        let state = probe.state_handle();
        (state, probe, painted_rects)
    }
}

impl WidgetStateOwner for CommitProbe {
    type State = CommitProbeState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for CommitProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "CommitProbe::measure", |state| Dimensioni::new(40, state.intrinsic_height))
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, event: Option<&UiInputEvent>) {
        runtime_update_state(&self.state, "CommitProbe::update", |state| {
            if let Some(grow_to) = state.grow_to.take() {
                state.intrinsic_height = grow_to;
            }
            if matches!(event, Some(UiInputEvent::MouseDown { .. })) {
                state.presses += 1;
            }
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // This is deliberately a rendering-only test cache: semantic state remains observational.
        self.painted_rects.borrow_mut().push(ctx.screen_content_rect());
    }
}

#[derive(Default)]
struct SiblingMutationState {
    value: i32,
    observed_during_update: Vec<i32>,
}

impl WidgetState for SiblingMutationState {}

struct SiblingMutationProbe {
    state: Rc<RefCell<SiblingMutationState>>,
    target: Option<(WidgetStateHandle<SiblingMutationState>, i32)>,
    opt: WidgetOption,
}

impl SiblingMutationProbe {
    fn new(value: i32, target: Option<(WidgetStateHandle<SiblingMutationState>, i32)>) -> (WidgetStateHandle<SiblingMutationState>, Self) {
        let probe = Self {
            state: Rc::new(RefCell::new(SiblingMutationState {
                value,
                observed_during_update: Vec::new(),
            })),
            target,
            opt: WidgetOption::NONE,
        };
        let state = probe.state_handle();
        (state, probe)
    }
}

impl WidgetStateOwner for SiblingMutationProbe {
    type State = SiblingMutationState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for SiblingMutationProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 10)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        if let Some((target, value)) = &self.target {
            target
                .try_update(|state| state.value = *value)
                .expect("the sibling target must not be borrowed yet or anymore");
        }
        runtime_update_state(&self.state, "SiblingMutationProbe::update", |state| {
            state.observed_during_update.push(state.value);
        });
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

struct CountedProbe {
    state: Rc<RefCell<()>>,
    updates: Rc<Cell<usize>>,
    opt: WidgetOption,
}

impl CountedProbe {
    fn new(updates: Rc<Cell<usize>>) -> Self {
        Self {
            state: Rc::new(RefCell::new(())),
            updates,
            opt: WidgetOption::NONE,
        }
    }
}

impl WidgetStateOwner for CountedProbe {
    type State = ();

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for CountedProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 10)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        self.updates.set(self.updates.get() + 1);
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

#[derive(Default)]
struct TopologyMutationState {
    same_container_blocked: bool,
    other_container_changed: bool,
}

impl WidgetState for TopologyMutationState {}

struct TopologyMutator {
    state: Rc<RefCell<TopologyMutationState>>,
    same_container: Rc<RefCell<Option<WidgetStateHandle<ColumnState>>>>,
    other_container: WidgetStateHandle<ColumnState>,
    candidate: Option<Node>,
    opt: WidgetOption,
}

impl WidgetStateOwner for TopologyMutator {
    type State = TopologyMutationState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for TopologyMutator {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 10)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        let Some(candidate) = self.candidate.take() else { return };
        let same_container = self
            .same_container
            .borrow()
            .as_ref()
            .expect("outer container handle must be installed before traversal")
            .clone();
        let same_container_blocked = same_container.try_update(|state| state.remove_drop(usize::MAX)).flatten().is_none();
        let other_container_changed = match self.other_container.try_update_with(candidate, |state, node| state.push(node)) {
            Ok(Ok(())) => true,
            Ok(Err(candidate)) | Err(candidate) => {
                self.candidate = Some(candidate);
                false
            }
        };
        runtime_update_state(&self.state, "TopologyMutator::update", |state| {
            state.same_container_blocked = same_container_blocked;
            state.other_container_changed = other_container_changed;
        });
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

#[test]
fn routed_recipient_gets_one_event_while_every_node_still_updates_in_fifo_order() {
    let state = Rc::new(RefCell::new(OrderedProbeState::default()));
    let probe = OrderedProbe {
        state: state.clone(),
        opt: WidgetOption::NONE,
    };
    let mut ctx = context();
    let root = ctx.create_window("window", rect(10, 10, 100, 80), Node::widget(probe));
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(20, 20);
    ctx.mousedown(20, 20, MouseButton::LEFT);
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("x");
    ctx.keyup(KeyMode::SHIFT);
    ctx.mouseup(20, 20, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));

    let state = state.borrow();
    assert_eq!(state.updates, 6);
    assert_eq!(state.events, ["move", "down", "key-down", "text", "key-up", "up"]);
    assert_eq!(
        state.held_buttons,
        [
            MouseButton::NONE.bits(),
            MouseButton::LEFT.bits(),
            MouseButton::LEFT.bits(),
            MouseButton::LEFT.bits(),
            MouseButton::LEFT.bits(),
            MouseButton::NONE.bits(),
        ]
    );
    assert_eq!(
        state.held_keys,
        [
            KeyMode::NONE.bits(),
            KeyMode::NONE.bits(),
            KeyMode::SHIFT.bits(),
            KeyMode::SHIFT.bits(),
            KeyMode::NONE.bits(),
            KeyMode::NONE.bits(),
        ]
    );
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.updates, 12, "chrome receives None for each event while the probe receives Some");
}

#[test]
fn update_drains_each_input_into_one_full_update_and_one_followup_layout() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(10, 10, 120, 90), empty_content());
    let dimensions = Dimensioni::new(320, 240);

    ctx.mousemove(20, 20);
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("x");
    ctx.update_ui(dimensions);

    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 4, "one synchronization layout plus one per input event");
    assert_eq!(metrics.updates, 6, "three events update both chrome and content");
    assert_eq!(metrics.paints, 0, "update_ui must not paint");

    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 4, "render must not lay out");
    assert_eq!(metrics.updates, 6, "render must not update widgets");
    assert_eq!(metrics.paints, 2, "render paints chrome and content exactly once");
}

#[test]
fn render_preflight_requires_a_matching_commit_and_never_acquires_backend_on_error() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new(backend);
    let root = ctx.create_window("window", rect(10, 10, 120, 90), empty_content());
    let dimensions = Dimensioni::new(320, 240);

    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());

    ctx.update_ui(dimensions);
    ctx.text("pending");
    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());

    ctx.update_ui(dimensions);
    let other = Dimensioni::new(640, 480);
    assert_eq!(ctx.frame(frame_info(other)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());

    ctx.set_root_rect(root.id(), rect(20, 20, 120, 90)).unwrap();
    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());
}

#[test]
fn invalid_update_dimensions_panic_before_dequeue_and_preserve_pending_input() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(10, 10, 120, 90), empty_content());
    ctx.text("still pending");

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctx.update_ui(Dimensioni::new(0, 240)))).expect_err("invalid dimensions must panic");
    let message = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str));
    assert_eq!(message, Some("update_ui dimensions must be positive"));

    ctx.update_ui(Dimensioni::new(320, 240));
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 2);
    assert_eq!(metrics.updates, 2, "the event queued before the panic must still be drained");
}

#[test]
fn every_event_layout_commit_updates_hit_geometry_for_the_next_queued_event() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), empty_content());
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    let (title, close, _) = ctx.debug_root_chrome(root.id()).unwrap();
    let title = title.unwrap();
    let close = close.unwrap();
    let shift = crate::vec2(18, 11);

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.mousemove(title.x + 2 + shift.x, title.y + 2 + shift.y);
    ctx.mouseup(title.x + 2 + shift.x, title.y + 2 + shift.y, MouseButton::LEFT);
    ctx.mousedown(close.x + close.width / 2 + shift.x, close.y + close.height / 2 + shift.y, MouseButton::LEFT);
    ctx.update_ui(dimensions);

    assert_eq!(root.state().try_read(RootState::is_visible), Some(false));
}

#[test]
fn disclosure_update_commits_child_geometry_before_the_next_queued_press() {
    let (button, child) = button_content("child");
    let (disclosure, node) = Disclosure::create(DisclosureParameters::header("section", false, [child]));
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), node);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.mousedown(10, 28, MouseButton::LEFT);
    ctx.update_ui(dimensions);

    assert_eq!(disclosure.try_read(DisclosureState::is_expanded), Some(true));
    assert_eq!(button.try_update(ButtonState::take_submitted), Some(true));
}

#[test]
fn collapsed_disclosure_skips_descendant_phases_and_drops_targets_only_on_removal() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new_test(backend, Dimensioni::new(320, 240));

    let probe = OrderedProbe {
        state: Rc::new(RefCell::new(OrderedProbeState::default())),
        opt: WidgetOption::HOLD_FOCUS,
    };
    let probe_state = probe.state_handle();
    let probe_node = Node::widget(probe);
    let probe_id = probe_node.id();

    let custom = ctx
        .register_custom_renderer(|frame, _args| frame.record_marker("disclosure custom child"))
        .unwrap();
    let custom_runtime = Custom::create(CustomParameters::new("custom"));
    let custom_state = custom_runtime.state_handle();
    let custom_node = Node::custom_render(custom_runtime, custom).with_policy(Policy::fixed(20, 10));

    let (disclosure, content) = Disclosure::create(DisclosureParameters::header("section", true, [probe_node, custom_node]));
    let root = ctx.create_window("window", rect(0, 0, 160, 140), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.update_and_render_ui();
    let probe_rect = ctx.debug_root_node_rect(root.id(), probe_id).unwrap();

    ctx.mousedown(probe_rect.x + 2, probe_rect.y + 2, MouseButton::LEFT);
    ctx.mouseup(probe_rect.x + 2, probe_rect.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    let visible_counts = probe_state
        .try_read(|state| (state.measures, state.updates, state.paints, state.events.clone()))
        .unwrap();
    assert_eq!(visible_counts.3, ["down", "up"]);

    disclosure.try_update(DisclosureState::collapse).unwrap();
    log.clear();
    ctx.mousemove(probe_rect.x + 2, probe_rect.y + 2);
    ctx.update_and_render_ui();

    assert_eq!(
        probe_state.try_read(|state| (state.measures, state.updates, state.paints)),
        Some((visible_counts.0, visible_counts.1, visible_counts.2)),
        "collapsed descendants must skip measure, update, and paint"
    );
    assert!(
        !log.snapshot()
            .iter()
            .any(|event| matches!(event, RenderEvent::Marker(name) if name == "disclosure custom child")),
        "collapsed descendants must skip custom rendering"
    );
    assert!(probe_state.is_alive() && custom_state.is_alive(), "collapse must retain descendant ownership");

    disclosure.try_update(DisclosureState::expand).unwrap();
    log.clear();
    ctx.text("focus must not return");
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
    assert!(
        log.snapshot()
            .iter()
            .any(|event| matches!(event, RenderEvent::Marker(name) if name == "disclosure custom child"))
    );

    disclosure.try_update(DisclosureState::clear).unwrap();
    assert!(!probe_state.is_alive() && !custom_state.is_alive(), "removal must drop descendant runtimes");
}

#[test]
fn nested_scroll_bubbles_at_the_inner_boundary_and_moves_only_the_outer_area() {
    let inner_content = Node::widget(Custom::create(CustomParameters::new("inner content"))).with_policy(Policy::fixed(50, 180));
    let (inner, inner_node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [inner_content]));
    let inner_node = inner_node.with_policy(Policy::fixed(60, 60));
    let inner_id = inner_node.id();
    let outer_tail = Node::widget(Custom::create(CustomParameters::new("outer tail"))).with_policy(Policy::fixed(60, 120));
    let (outer, content) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [inner_node, outer_tail]));

    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 100, 100), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    inner.try_update(|state| state.set_offset(crate::vec2(0, i32::MAX))).unwrap();
    ctx.update_ui(dimensions);
    let inner_offset = inner.try_read(|state| state.offset()).unwrap();
    assert!(inner_offset.y > 0);
    let inner_rect = ctx.debug_root_node_rect(root.id(), inner_id).unwrap();

    ctx.mousemove(inner_rect.x + 5, inner_rect.y + 5);
    ctx.scroll(0, 12);
    ctx.update_ui(dimensions);

    assert_eq!(
        inner.try_read(|state| (state.offset().x, state.offset().y)),
        Some((inner_offset.x, inner_offset.y))
    );
    assert_eq!(outer.try_read(|state| state.offset().y), Some(12));
}

#[test]
fn intrinsic_mutation_is_laid_out_before_the_next_event_and_painted_from_that_commit() {
    let (growing_state, growing, growing_paints) = CommitProbe::new(10, Some(30));
    let growing = Node::widget(growing);
    let growing_id = growing.id();
    let (target_state, target, _) = CommitProbe::new(10, None);
    let target = Node::widget(target);
    let target_id = target.id();
    let (_, content) = Column::create(ColumnParameters::new([growing, target]));
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    let growing_before = ctx.debug_root_node_rect(root.id(), growing_id).unwrap();
    let target_before = ctx.debug_root_node_rect(root.id(), target_id).unwrap();
    assert_eq!(growing_before.height, 10);

    // The move grows the first sibling. Its post-event layout moves the second sibling before the
    // queued press is routed, so this point is inside only the new target rectangle.
    ctx.mousemove(growing_before.x + 1, growing_before.y + 1);
    ctx.mousedown(target_before.x + 1, target_before.y + 21, MouseButton::LEFT);
    ctx.update_ui(dimensions);

    let target_after = ctx.debug_root_node_rect(root.id(), target_id).unwrap();
    assert_eq!(growing_state.try_read(|state| state.intrinsic_height), Some(30));
    assert_eq!(target_after.y, target_before.y + 20);
    assert_eq!(target_state.try_read(|state| state.presses), Some(1));

    let committed_metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    let rendered_metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(rendered_metrics.tree_layouts, committed_metrics.tree_layouts);
    assert_eq!(rendered_metrics.updates, committed_metrics.updates);
    assert_eq!(growing_paints.borrow().last().map(|rect| rect.height), Some(30));
}

#[test]
fn sibling_mutation_observes_parent_first_forward_traversal_without_reruns() {
    let (later_state, later) = SiblingMutationProbe::new(0, None);
    let (earlier_state, earlier) = SiblingMutationProbe::new(0, Some((later_state.clone(), 11)));
    let (already_updated_state, already_updated) = SiblingMutationProbe::new(0, None);
    let (_late_mutator_state, late_mutator) = SiblingMutationProbe::new(0, Some((already_updated_state.clone(), 22)));
    let (_, content) = Column::create(ColumnParameters::new([
        Node::widget(earlier),
        Node::widget(later),
        Node::widget(already_updated),
        Node::widget(late_mutator),
    ]));
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(5, 5);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(earlier_state.try_read(|state| state.observed_during_update.clone()), Some(vec![0]));
    assert_eq!(later_state.try_read(|state| state.observed_during_update.clone()), Some(vec![11]));
    assert_eq!(already_updated_state.try_read(|state| state.value), Some(22));
    assert_eq!(
        already_updated_state.try_read(|state| state.observed_during_update.clone()),
        Some(vec![0]),
        "the earlier sibling changes but is not rerun after the later sibling mutates it"
    );
}

#[test]
fn topology_mutation_is_blocked_for_the_active_container_and_visible_in_a_later_subtree() {
    let inserted_updates = Rc::new(Cell::new(0));
    let candidate = Node::widget(CountedProbe::new(inserted_updates.clone()));
    let (other_container, other_node) = Column::create(ColumnParameters::default());
    let same_container = Rc::new(RefCell::new(None));
    let mutator = TopologyMutator {
        state: Rc::new(RefCell::new(TopologyMutationState::default())),
        same_container: same_container.clone(),
        other_container: other_container.clone(),
        candidate: Some(candidate),
        opt: WidgetOption::NONE,
    };
    let mutator_state = mutator.state_handle();
    let (outer_container, content) = Column::create(ColumnParameters::new([Node::widget(mutator), other_node]));
    *same_container.borrow_mut() = Some(outer_container.clone());
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(5, 5);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(
        mutator_state.try_read(|state| (state.same_container_blocked, state.other_container_changed)),
        Some((true, true))
    );
    assert_eq!(outer_container.try_read(ColumnState::len), Some(Some(2)));
    assert_eq!(other_container.try_read(ColumnState::len), Some(Some(1)));
    assert_eq!(inserted_updates.get(), 1, "the newly inserted later descendant participates in the same update");
}

#[test]
fn programmatic_topology_mutation_needs_only_an_empty_queue_layout_commit() {
    let (_, first, _) = CommitProbe::new(10, None);
    let (column, content) = Column::create(ColumnParameters::new([Node::widget(first)]));
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    let (_, appended, _) = CommitProbe::new(18, None);
    let appended = Node::widget(appended);
    let appended_id = appended.id();
    assert!(
        column.try_update_with(appended, |state, node| state.push(node)).is_ok(),
        "the programmatic topology mutation must commit before traversal"
    );
    ctx.update_ui(dimensions);

    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    let appended_rect = ctx.debug_root_node_rect(root.id(), appended_id).unwrap();
    assert_eq!(metrics.tree_layouts, 1);
    assert_eq!(metrics.updates, 0);
    assert_eq!(appended_rect.height, 18);
}

#[test]
fn traversal_reaching_state_borrowed_by_an_access_closure_reports_the_runtime_diagnostic() {
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), Node::widget(widget));
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);

    let update_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.update_ui(dimensions));
    }))
    .expect_err("layout must diagnose the active TextBlockState borrow");
    let update_message = panic_message(update_panic.as_ref());
    assert!(update_message.contains("retained widget state invariant violated during TextBlock::measure"));
    assert!(update_message.contains("state-access closures must finish before retained update, layout, or paint traversal"));

    // Once the access closure has unwound and released its borrow, the same commit is valid.
    ctx.update_ui(dimensions);
    let paint_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.frame(frame_info(dimensions)).render_ui().unwrap());
    }))
    .expect_err("paint must diagnose the active TextBlockState borrow");
    let paint_message = panic_message(paint_panic.as_ref());
    assert!(paint_message.contains("retained widget state invariant violated during TextBlock::paint"));

    ctx.frame(frame_info(dimensions)).render_ui().unwrap();

    // A shared access closure is likewise incompatible when the routed update needs to mutate the
    // same cell, even though the synchronization layout's shared reads are allowed by RefCell.
    let (button, button_node) = button_content("button");
    let button_id = button_node.id();
    let mut button_ctx = context();
    let button_root = button_ctx.create_window("button", rect(0, 0, 140, 100), button_node);
    button_ctx
        .set_root_options(button_root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    button_ctx.update_ui(dimensions);
    let button_rect = button_ctx.debug_root_node_rect(button_root.id(), button_id).unwrap();
    button_ctx.mousedown(button_rect.x + 1, button_rect.y + 1, MouseButton::LEFT);
    let read_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        button.try_read(|_| button_ctx.update_ui(dimensions));
    }))
    .expect_err("Button::update must diagnose the active shared ButtonState borrow");
    let read_message = panic_message(read_panic.as_ref());
    assert!(read_message.contains("retained widget state invariant violated during Button::update"));
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

fn button_content(label: &str) -> (WidgetStateHandle<ButtonState>, Node) {
    let (state, widget) = Button::create(ButtonParameters::new(label));
    (state, Node::widget(widget))
}

#[test]
fn widget_handle_events_map_into_one_typed_session_without_state_polling() {
    #[derive(Default)]
    struct Model {
        submissions: Vec<&'static str>,
    }

    enum Message {
        FirstSubmitted,
        SecondSubmitted,
    }

    let (first_state, first_widget) = Button::create(ButtonParameters::new("first"));
    let first = Node::widget(first_widget);
    let first_id = first.id();
    let (second_state, second_widget) = Button::create(ButtonParameters::new("second"));
    let second = Node::widget(second_widget);
    let second_id = second.id();
    let (_, content) = Row::create(RowParameters::new(
        [SizePolicy::Fixed(60), SizePolicy::Fixed(60)],
        SizePolicy::Auto,
        [first, second],
    ));
    let mut ctx = context();
    let root = ctx.create_window("signal", rect(0, 0, 140, 100), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    let first_rect = ctx.debug_root_node_rect(root.id(), first_id).unwrap();
    let second_rect = ctx.debug_root_node_rect(root.id(), second_id).unwrap();

    let mut session = crate::Session::new();
    session.connect(first_state.submitted(), |_| Message::FirstSubmitted).unwrap();
    session.connect(second_state.submitted(), |_| Message::SecondSubmitted).unwrap();
    let mut subscribers = crate::Subscribers::new();
    subscribers.subscribe(|model: &mut Model, message: &Message, _emit| match message {
        Message::FirstSubmitted => model.submissions.push("first"),
        Message::SecondSubmitted => model.submissions.push("second"),
    });
    let mut model = Model::default();

    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.update_ui_session(dimensions, &mut session, &mut model, &mut subscribers);

    assert_eq!(model.submissions, ["first", "second", "first"]);
}

#[test]
fn textbox_handle_event_maps_a_complete_snapshot_into_the_session() {
    #[derive(Default)]
    struct Model {
        changes: Vec<(String, usize)>,
    }

    enum Message {
        Changed(TextboxChanged),
    }

    let (textbox_state, widget) = Textbox::create(TextboxParameters::new(""));
    let node = Node::widget(widget);
    let node_id = node.id();
    let mut ctx = context();
    let root = ctx.create_window("textbox signal", rect(0, 0, 140, 100), node);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    let textbox_rect = ctx.debug_root_node_rect(root.id(), node_id).unwrap();

    let mut session = crate::Session::new();
    session.connect(textbox_state.changed(), Message::Changed).unwrap();
    let mut subscribers = crate::Subscribers::new();
    subscribers.subscribe(|model: &mut Model, message: &Message, _emit| match message {
        Message::Changed(event) => model.changes.push((event.text.clone(), event.cursor)),
    });
    let mut model = Model::default();

    ctx.mousedown(textbox_rect.x + 1, textbox_rect.y + 1, MouseButton::LEFT);
    ctx.text("é");
    ctx.update_ui_session(dimensions, &mut session, &mut model, &mut subscribers);

    assert_eq!(model.changes, [(String::from("é"), "é".len())]);
}

#[test]
fn session_dispatches_application_messages_without_raw_input() {
    enum Message {
        Increment,
    }

    let mut ctx = context();
    let mut session = crate::Session::new();
    session.emit(Message::Increment);
    let mut subscribers = crate::Subscribers::new();
    subscribers.subscribe(|count: &mut usize, message: &Message, _emit| match message {
        Message::Increment => *count += 1,
    });
    let mut count = 0;

    ctx.update_ui_session(Dimensioni::new(320, 240), &mut session, &mut count, &mut subscribers);

    assert_eq!(count, 1);
}

#[test]
fn creation_returns_typed_persistent_root_state() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(20, 30, 120, 90), empty_content());

    assert!(root.state().is_alive());
    assert_eq!(root.state().try_read(|state| state.name().to_owned()), Some("window".to_owned()));
    assert_eq!(
        root.state().try_read(|state| {
            let rect = state.rect();
            (rect.x, rect.y, rect.width, rect.height)
        }),
        Some((20, 30, 120, 90))
    );
    assert_eq!(root.state().try_read(RootState::is_visible), Some(true));
    assert_eq!(ctx.debug_root_node_count(root.id()), Some(2));
}

#[test]
fn every_root_kind_adds_exactly_one_private_chrome_node() {
    let mut ctx = context();
    let window = ctx.create_window("window", rect(0, 0, 100, 80), empty_content());
    let dialog = ctx.create_dialog("dialog", rect(10, 10, 100, 80), empty_content());
    let popup = ctx.create_popup("popup", empty_content());

    // Each application tree contains one empty Column node. The second retained node is the one
    // private root Container; title, close, and resize regions are geometry, not child nodes.
    assert_eq!(ctx.debug_root_node_count(window.id()), Some(2));
    assert_eq!(ctx.debug_root_node_count(dialog.id()), Some(2));
    assert_eq!(ctx.debug_root_node_count(popup.id()), Some(2));
}

#[test]
fn one_child_scroll_area_retains_its_three_structural_children() {
    let child = Node::widget(Custom::create(CustomParameters::new("content")));
    let (_, content) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [child]));
    let mut ctx = context();
    let root = ctx.create_window("scroll", rect(0, 0, 100, 80), content);

    // The application child lives below a virtual surface, beside two real scrollbar widgets.
    // Root chrome is the sixth retained node and remains separate from the application composite.
    assert_eq!(ctx.debug_root_node_count(root.id()), Some(6));
}

#[test]
fn hide_and_show_preserve_root_and_descendant_state() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.create_window("window", rect(10, 10, 120, 90), content);

    ctx.set_root_visible(root.id(), false).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(root.state().try_read(RootState::is_visible), Some(false));
    assert!(button.is_alive());

    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(root.state().try_read(RootState::is_visible), Some(true));
    assert!(button.is_alive());
}

#[test]
fn destroy_expires_handles_and_ids_are_never_reused() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.create_window("first", rect(0, 0, 100, 80), content);
    let destroyed_id = root.id();

    assert!(ctx.destroy_root(destroyed_id));
    assert!(!ctx.destroy_root(destroyed_id));
    assert!(!ctx.bring_root_to_front(destroyed_id));
    assert_eq!(ctx.set_root_rect(destroyed_id, rect(1, 2, 3, 4)), Err(RootMutationError::UnknownRoot));
    assert!(!root.state().is_alive());
    assert!(!button.is_alive());

    let replacement = ctx.create_window("second", rect(0, 0, 100, 80), empty_content());
    assert_ne!(replacement.id(), destroyed_id);
}

#[test]
fn active_state_upgrade_does_not_keep_destroyed_root_topology_alive() {
    let mut ctx = context();
    let (child, content) = button_content("child");
    let root = ctx.create_window("window", rect(0, 0, 100, 80), content);
    let state = root.state().clone();

    state
        .try_update(|_| {
            assert!(ctx.destroy_root(root.id()));
            assert!(state.is_alive());
            assert!(!child.is_alive(), "root state access is not a second strong child owner");
        })
        .unwrap();

    assert!(!state.is_alive());
    assert!(!child.is_alive());
}

#[test]
fn same_state_setter_conflict_is_checked() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 100, 80), empty_content());
    let mut conflict = None;

    root.state()
        .try_update(|_| conflict = Some(ctx.set_root_rect(root.id(), rect(10, 10, 100, 80))))
        .unwrap();

    assert_eq!(conflict, Some(Err(RootMutationError::Borrowed)));
}

#[test]
fn popup_switch_is_atomic_when_the_visible_popup_state_is_borrowed() {
    let mut ctx = context();
    let first = ctx.create_popup("first", empty_content());
    let second = ctx.create_popup("second", empty_content());
    ctx.set_root_visible(first.id(), true).unwrap();
    let mut result = None;

    first.state().try_update(|_| result = Some(ctx.set_root_visible(second.id(), true))).unwrap();

    assert_eq!(result, Some(Err(RootMutationError::Borrowed)));
    assert_eq!(first.state().try_read(RootState::is_visible), Some(true));
    assert_eq!(second.state().try_read(RootState::is_visible), Some(false));
}

#[test]
fn dynamic_container_root_changes_descendants_without_replacing_the_root() {
    let mut ctx = context();
    let (column, content) = Column::create(ColumnParameters::default());
    let root = ctx.create_window("dynamic", rect(0, 0, 140, 100), content);
    let root_id = root.id();
    let (button, widget) = Button::create(ButtonParameters::new("new child"));

    let inserted = column.try_update(|column| column.push(Node::widget(widget))).unwrap();
    assert!(inserted.is_ok());
    ctx.update_and_render_ui();
    assert_eq!(root.id(), root_id);
    assert!(button.is_alive());
    assert_eq!(ctx.debug_root_node_count(root_id), Some(3));

    assert_eq!(column.try_update(|column: &mut ColumnState| column.remove_drop(0)), Some(Some(true)));
    assert!(!button.is_alive());
    assert!(root.state().is_alive());
}

#[test]
fn showing_a_popup_atomically_hides_the_previous_one() {
    let mut ctx = context();
    let first = ctx.create_popup("first", empty_content());
    let second = ctx.create_popup("second", empty_content());

    ctx.set_root_visible(first.id(), true).unwrap();
    assert_eq!(first.state().try_read(RootState::is_visible), Some(true));
    ctx.set_root_visible(second.id(), true).unwrap();

    assert_eq!(first.state().try_read(RootState::is_visible), Some(false));
    assert_eq!(second.state().try_read(RootState::is_visible), Some(true));
    assert_eq!(first.state().try_update(RootState::take_submitted), Some(false));
}

#[test]
fn outside_popup_press_hides_and_records_typed_submission() {
    let mut ctx = context();
    let popup = ctx.create_popup("popup", empty_content());
    ctx.set_root_options(popup.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.set_root_visible(popup.id(), true).unwrap();
    ctx.set_root_rect(popup.id(), rect(20, 20, 80, 60)).unwrap();
    ctx.update_and_render_ui();

    ctx.mousedown(200, 180, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(popup.state().try_read(RootState::is_visible), Some(false));
    ctx.set_root_visible(popup.id(), true).unwrap();
    ctx.set_root_visible(popup.id(), false).unwrap();
    assert_eq!(popup.state().try_update(RootState::take_submitted), Some(true));
    assert_eq!(popup.state().try_update(RootState::take_submitted), Some(false));
}

#[test]
fn outside_popup_press_dismisses_then_routes_once_to_the_revealed_root() {
    let mut ctx = context();
    let (button, content) = button_content("behind");
    let window = ctx.create_window("window", rect(0, 0, 180, 120), content);
    ctx.set_root_options(window.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let popup = ctx.create_popup("popup", empty_content());
    ctx.set_root_options(popup.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.set_root_visible(popup.id(), true).unwrap();
    ctx.set_root_rect(popup.id(), rect(80, 60, 60, 40)).unwrap();
    ctx.update_ui(Dimensioni::new(320, 240));

    ctx.mousedown(15, 15, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(popup.state().try_read(RootState::is_visible), Some(false));
    assert_eq!(button.try_update(ButtonState::take_submitted), Some(true));
    assert_eq!(button.try_update(ButtonState::take_submitted), Some(false));
}

#[test]
fn layout_only_update_and_paint_have_separate_phase_counts() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(10, 10, 120, 90), empty_content());

    ctx.update_and_render_ui();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 1);
    assert_eq!(metrics.updates, 0);
    assert_eq!(metrics.paints, 2);
}

#[test]
fn warmed_container_measurement_and_layout_allocate_nothing() {
    let child = |name| Node::widget(Custom::create(CustomParameters::new(name)));
    let (_, row) = Row::create(RowParameters::new([SizePolicy::Weight(1.0)], SizePolicy::Auto, [child("row")]));
    let (_, grid) = Grid::create(GridParameters::new([SizePolicy::Weight(1.0)], [SizePolicy::Auto], [child("grid")]));
    let (_, stack) = Stack::create(StackParameters::new(
        SizePolicy::Remainder(0),
        SizePolicy::Fixed(20),
        StackDirection::TopToBottom,
        [child("stack")],
    ));
    let (_, disclosure) = Disclosure::create(DisclosureParameters::header("expanded", true, [child("disclosure")]));
    let (_, scroll) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [child("scroll")]));
    let (_, content) = Column::create(ColumnParameters::new([row, grid, stack, disclosure, scroll]));
    let mut ctx = context();
    ctx.create_window("allocation probe", rect(10, 10, 300, 220), content);
    let dimensions = Dimensioni::new(640, 480);

    ctx.update_ui(dimensions);
    ctx.update_ui(dimensions);
    let measurement = AllocationMeasurement::begin();
    ctx.update_ui(dimensions);
    let allocations = measurement.finish();

    assert_eq!(allocations.events, 0, "steady measurement/layout allocated {} bytes", allocations.bytes);
}

#[test]
fn fronting_changes_only_cross_root_z_order() {
    let mut ctx = context();
    let first = ctx.create_window("first", rect(0, 0, 100, 80), empty_content());
    let second = ctx.create_window("second", rect(20, 20, 100, 80), empty_content());
    assert_eq!(ctx.debug_rendered_root_names(), ["first", "second"]);

    assert!(ctx.bring_root_to_front(first.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["second", "first"]);
    assert!(ctx.debug_root_zindex(first.id()).unwrap() > ctx.debug_root_zindex(second.id()).unwrap());
}

#[test]
fn blank_root_press_confines_drag_to_the_pressed_root() {
    let probe = OrderedProbe {
        state: Rc::new(RefCell::new(OrderedProbeState::default())),
        opt: WidgetOption::NONE,
    };
    let probe_state = probe.state_handle();
    let mut ctx = context();
    let first = ctx.create_window("first", rect(0, 0, 100, 80), empty_content());
    let second = ctx.create_window("second", rect(160, 120, 100, 80), Node::widget(probe).with_policy(Policy::fill()));
    for root in [first.id(), second.id()] {
        ctx.set_root_options(root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
            .unwrap();
    }
    ctx.update_and_render_ui();

    let first_body = ctx.debug_root_body(first.id()).unwrap();
    let second_body = ctx.debug_root_body(second.id()).unwrap();
    let first_point = crate::vec2(first_body.x + 1, first_body.y + 1);
    let second_point = crate::vec2(second_body.x + 1, second_body.y + 1);

    ctx.mousedown(first_point.x, first_point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(first.id()), Some(false));

    ctx.mousemove(second_point.x, second_point.y);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));
}

#[test]
fn active_root_confines_scroll_while_hover_and_press_remain_hit_routed() {
    let probe = OrderedProbe {
        state: Rc::new(RefCell::new(OrderedProbeState::default())),
        opt: WidgetOption::GRAB_SCROLL,
    };
    let probe_state = probe.state_handle();
    let mut ctx = context();
    let first = ctx.create_window("first", rect(0, 0, 100, 80), empty_content());
    let second = ctx.create_window("second", rect(160, 120, 100, 80), Node::widget(probe).with_policy(Policy::fill()));
    for root in [first.id(), second.id()] {
        ctx.set_root_options(root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
            .unwrap();
    }
    ctx.update_and_render_ui();

    let first_body = ctx.debug_root_body(first.id()).unwrap();
    let second_body = ctx.debug_root_body(second.id()).unwrap();
    let first_point = crate::vec2(first_body.x + 1, first_body.y + 1);
    let second_point = crate::vec2(second_body.x + 1, second_body.y + 1);

    ctx.mousedown(first_point.x, first_point.y, MouseButton::LEFT);
    ctx.mouseup(first_point.x, first_point.y, MouseButton::LEFT);
    ctx.mousemove(second_point.x, second_point.y);
    ctx.scroll(0, 1);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["move"]));

    ctx.mousedown(second_point.x, second_point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["move", "down"]));
}

#[test]
fn pointer_captured_root_remains_the_keyboard_and_text_input_root() {
    let probe = OrderedProbe {
        state: Rc::new(RefCell::new(OrderedProbeState::default())),
        opt: WidgetOption::NONE,
    };
    let probe_state = probe.state_handle();
    let mut ctx = context();
    let first = ctx.create_window("first", rect(0, 0, 100, 80), Node::widget(probe).with_policy(Policy::fill()));
    let second = ctx.create_window("second", rect(160, 120, 100, 80), empty_content());
    ctx.update_and_render_ui();

    let first_body = ctx.debug_root_body(first.id()).unwrap();
    ctx.mousedown(first_body.x + 1, first_body.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(first.id()), Some(true));

    assert!(ctx.bring_root_to_front(second.id()));
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("captured");
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "key-down", "text"]));
}

#[test]
fn visible_dialog_is_the_sole_pointer_root_and_remains_frontmost() {
    let mut ctx = context();
    let (behind_button, behind_content) = button_content("behind");
    let window = ctx.create_window("window", rect(0, 0, 100, 80), behind_content);
    ctx.set_root_options(window.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let (dialog_button, dialog_content) = button_content("dialog");
    let dialog = ctx.create_dialog("dialog", rect(120, 100, 100, 80), dialog_content);
    ctx.set_root_options(dialog.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.set_root_visible(dialog.id(), true).unwrap();
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_modal_root(), Some(dialog.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog"]);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(behind_button.try_update(ButtonState::take_submitted), Some(false));

    ctx.mousedown(130, 110, MouseButton::LEFT);
    ctx.mouseup(130, 110, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(dialog_button.try_update(ButtonState::take_submitted), Some(true));

    assert!(ctx.bring_root_to_front(window.id()));
    ctx.set_root_visible(window.id(), true).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog"]);
    assert!(ctx.debug_root_zindex(dialog.id()).unwrap() > ctx.debug_root_zindex(window.id()).unwrap());

    ctx.set_root_visible(dialog.id(), false).unwrap();
    assert_eq!(ctx.debug_modal_root(), None);
    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(behind_button.try_update(ButtonState::take_submitted), Some(true));
}

#[test]
fn active_dialog_keeps_a_visible_popup_below_and_input_blocked() {
    let mut ctx = context();
    let (popup_button, popup_content) = button_content("popup");
    let popup = ctx.create_popup("popup", popup_content);
    ctx.set_root_options(popup.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dialog = ctx.create_dialog("dialog", rect(120, 100, 100, 80), empty_content());
    ctx.set_root_options(dialog.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.set_root_visible(dialog.id(), true).unwrap();
    ctx.mousemove(10, 10);
    ctx.update_and_render_ui();

    ctx.set_root_visible(popup.id(), true).unwrap();
    ctx.set_root_rect(popup.id(), rect(0, 0, 100, 80)).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_rendered_root_names(), ["popup", "dialog"]);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(popup_button.try_update(ButtonState::take_submitted), Some(false));

    ctx.set_root_visible(dialog.id(), false).unwrap();
    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(popup_button.try_update(ButtonState::take_submitted), Some(true));
}

#[test]
fn modal_activation_clears_underlying_focus_and_blocks_keyboard_input() {
    let state = Rc::new(RefCell::new(OrderedProbeState::default()));
    let probe = OrderedProbe {
        state: state.clone(),
        opt: WidgetOption::HOLD_FOCUS,
    };
    let mut ctx = context();
    let window = ctx.create_window("window", rect(0, 0, 100, 80), Node::widget(probe));
    ctx.set_root_options(window.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dialog = ctx.create_dialog("dialog", rect(120, 100, 100, 80), empty_content());
    ctx.set_root_options(dialog.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.borrow().events, ["down", "up"]);

    ctx.set_root_visible(dialog.id(), true).unwrap();
    let updates_before_modal_input = state.borrow().updates;
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("blocked");
    ctx.keyup(KeyMode::SHIFT);
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.borrow().updates, updates_before_modal_input);
    assert_eq!(state.borrow().events, ["down", "up"]);

    ctx.set_root_visible(dialog.id(), false).unwrap();
    ctx.text("still unfocused");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.borrow().events, ["down", "up"]);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.text("accepted");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.borrow().events, ["down", "up", "down", "text"]);
}

#[test]
fn showing_a_dialog_revokes_underlying_chrome_capture() {
    let mut ctx = context();
    let window = ctx.create_window("window", rect(30, 30, 140, 100), empty_content());
    let dialog = ctx.create_dialog("dialog", rect(170, 120, 100, 80), empty_content());
    ctx.update_and_render_ui();
    let title = ctx.debug_root_chrome(window.id()).unwrap().0.unwrap();

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(window.id()), Some(true));
    assert_eq!(window.state().try_read(RootState::is_moving), Some(true));
    let before = window.state().try_read(RootState::rect).unwrap();

    ctx.set_root_visible(dialog.id(), true).unwrap();
    assert_eq!(ctx.debug_root_has_pointer_capture(window.id()), Some(false));
    assert_eq!(window.state().try_read(RootState::is_active), Some(false));
    ctx.mousemove(title.x + 20, title.y + 20);
    ctx.mouseup(title.x + 20, title.y + 20, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(
        window.state().try_read(|state| {
            let rect = state.rect();
            (rect.x, rect.y, rect.width, rect.height)
        }),
        Some((before.x, before.y, before.width, before.height))
    );
}

#[test]
fn hiding_or_destroying_the_active_dialog_restores_the_previous_modal_dialog() {
    let mut ctx = context();
    let first = ctx.create_dialog("first", rect(20, 20, 120, 90), empty_content());
    let second = ctx.create_dialog("second", rect(40, 40, 120, 90), empty_content());

    ctx.set_root_visible(first.id(), true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    ctx.set_root_visible(second.id(), true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));

    ctx.set_root_visible(second.id(), false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    ctx.set_root_visible(second.id(), true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));

    ctx.update_and_render_ui();
    let close = ctx.debug_root_chrome(second.id()).unwrap().1.unwrap();
    ctx.mousedown(close.x + close.width / 2, close.y + close.height / 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(second.state().try_read(RootState::is_visible), Some(false));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.set_root_visible(second.id(), true).unwrap();
    assert!(ctx.destroy_root(second.id()));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert!(ctx.destroy_root(first.id()));
    assert_eq!(ctx.debug_modal_root(), None);
}

#[test]
fn fronting_remains_state_borrow_independent_and_does_not_replace_the_active_modal() {
    let mut ctx = context();
    let first = ctx.create_dialog("first", rect(20, 20, 120, 90), empty_content());
    let middle = ctx.create_dialog("middle", rect(30, 30, 120, 90), empty_content());
    let second = ctx.create_dialog("second", rect(40, 40, 120, 90), empty_content());
    ctx.set_root_visible(first.id(), true).unwrap();
    ctx.set_root_visible(middle.id(), true).unwrap();
    ctx.set_root_visible(second.id(), true).unwrap();

    first.state().try_update(|_| assert!(ctx.bring_root_to_front(first.id()))).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));
    assert!(ctx.debug_root_zindex(second.id()).unwrap() > ctx.debug_root_zindex(first.id()).unwrap());

    ctx.set_root_visible(second.id(), false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(middle.id()));
    ctx.set_root_visible(second.id(), true).unwrap();

    second.state().try_update(|_| assert!(ctx.bring_root_to_front(second.id()))).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));
}

#[test]
fn modal_restoration_keeps_hiding_and_destruction_independent_of_other_root_borrows() {
    let mut ctx = context();
    let window = ctx.create_window("window", rect(0, 0, 100, 80), empty_content());
    let first = ctx.create_dialog("first", rect(20, 20, 120, 90), empty_content());
    let second = ctx.create_dialog("second", rect(40, 40, 120, 90), empty_content());
    ctx.set_root_visible(first.id(), true).unwrap();
    ctx.set_root_visible(second.id(), true).unwrap();

    first.state().try_update(|_| ctx.set_root_visible(second.id(), false).unwrap()).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.set_root_visible(second.id(), true).unwrap();
    first.state().try_update(|_| assert!(ctx.destroy_root(second.id()))).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    first.state().try_update(|_| assert!(ctx.destroy_root(window.id()))).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
}

#[test]
fn title_drag_and_close_record_typed_root_events() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), empty_content());
    ctx.update_and_render_ui();
    let (title, _, _) = ctx.debug_root_chrome(root.id()).unwrap();
    let title = title.unwrap();
    let drag_x = title.x + 2;
    let drag_y = title.y + 2;

    ctx.mousedown(drag_x, drag_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.state().try_read(RootState::is_moving), Some(true));
    assert_eq!(root.state().try_update(RootState::take_changed), Some(false));

    ctx.mousemove(drag_x + 10, drag_y + 8);
    ctx.update_and_render_ui();
    assert_eq!(root.state().try_update(RootState::take_changed), Some(true));

    ctx.mouseup(drag_x + 10, drag_y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.state().try_read(RootState::is_active), Some(false));

    let close = ctx.debug_root_chrome(root.id()).unwrap().1.unwrap();
    let close_x = close.x + close.width / 2;
    let close_y = close.y + close.height / 2;
    ctx.mousedown(close_x, close_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.state().try_read(RootState::is_visible), Some(false));
    assert_eq!(root.state().try_update(RootState::take_submitted), Some(true));
}

#[test]
fn resize_overlay_preempts_content_where_the_grip_overlaps_the_root_body() {
    let probe = OrderedProbe {
        state: Rc::new(RefCell::new(OrderedProbeState::default())),
        opt: WidgetOption::NONE,
    };
    let probe_state = probe.state_handle();
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), Node::widget(probe).with_policy(Policy::fill()));
    ctx.update_and_render_ui();

    let resize = ctx.debug_root_chrome(root.id()).unwrap().2.unwrap();
    let body = ctx.debug_root_body(root.id()).unwrap();
    let press = crate::vec2(resize.x + 1, resize.y + 1);
    assert!(
        resize.contains(&press) && body.contains(&press),
        "the regression requires the painted grip to overlap content"
    );

    ctx.mousemove(press.x, press.y);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.hovered), Some(false));
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));
    ctx.scroll(0, 1);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    let before = root.state().try_read(RootState::rect).unwrap();
    ctx.mousedown(press.x, press.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.state().try_read(RootState::is_resizing), Some(true));
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    ctx.mousemove(press.x + 12, press.y + 8);
    ctx.mouseup(press.x + 12, press.y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(
        root.state().try_read(|state| (state.rect().width, state.rect().height)),
        Some((before.width + 12, before.height + 8))
    );
}

#[test]
fn content_capture_remains_exclusive_while_dragging_across_root_chrome() {
    let probe = OrderedProbe {
        state: Rc::new(RefCell::new(OrderedProbeState::default())),
        opt: WidgetOption::NONE,
    };
    let probe_state = probe.state_handle();
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), Node::widget(probe).with_policy(Policy::fill()));
    ctx.update_and_render_ui();

    let body = ctx.debug_root_body(root.id()).unwrap();
    let resize = ctx.debug_root_chrome(root.id()).unwrap().2.unwrap();
    let press = crate::vec2(body.x + 1, body.y + 1);
    let over_chrome = crate::vec2(resize.x + 1, resize.y + 1);
    assert!(!resize.contains(&press));
    assert!(body.contains(&over_chrome), "the capture regression requires chrome overlapping content");

    ctx.mousedown(press.x, press.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down"]));
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(true));

    ctx.mousemove(over_chrome.x, over_chrome.y);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "drag"]));
    assert_eq!(probe_state.try_read(|state| state.hovered), Some(true));
    assert_eq!(root.state().try_read(RootState::is_resizing), Some(false));

    ctx.mouseup(over_chrome.x, over_chrome.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "drag", "up"]));
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
}

#[test]
fn hiding_and_showing_root_does_not_restore_chrome_capture() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), empty_content());
    ctx.update_and_render_ui();
    let title = ctx.debug_root_chrome(root.id()).unwrap().0.unwrap();

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(true));
    assert_eq!(root.state().try_read(RootState::is_moving), Some(true));

    ctx.set_root_visible(root.id(), false).unwrap();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(root.state().try_read(RootState::is_active), Some(false));

    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(root.state().try_read(RootState::is_active), Some(false));
}

#[test]
fn chrome_geometry_exposes_one_body_and_auto_size_tracks_content() {
    let mut ctx = context();
    let (_, text) = crate::TextBlock::create(crate::TextBlockParameters::new("window content"));
    let root = ctx.create_popup("popup", Node::widget(text));
    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_and_render_ui();

    let body = ctx.debug_root_body(root.id()).unwrap();
    let content = ctx.debug_root_content_size(root.id()).unwrap();
    let outer = root.state().try_read(RootState::rect).unwrap();
    assert!(body.width > 0 && body.height > 0);
    assert!(content.width > 0 && content.height > 0);
    assert!(outer.width >= body.width && outer.height >= body.height);
}

#[test]
fn auto_height_preserves_popup_width_and_stretches_stack_items() {
    let mut item_ids = Vec::new();
    let items = ["Apple", "Banana", "Cherry", "Date"]
        .into_iter()
        .map(|label| {
            let (_, item) = ListItem::create(ListItemParameters::new(label));
            let node = Node::widget(item);
            item_ids.push(node.id());
            node
        })
        .collect::<Vec<_>>();
    let (_, content) = Stack::create(StackParameters::new(
        SizePolicy::Remainder(0),
        SizePolicy::Auto,
        StackDirection::TopToBottom,
        items,
    ));
    let mut ctx = context();
    let root = ctx.create_popup("combo", content);
    let anchor = rect(20, 30, 180, 1);
    ctx.set_root_options(
        root.id(),
        WindowOption::FRAME | WindowOption::AUTO_HEIGHT | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
    )
    .unwrap();
    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.set_root_rect(root.id(), anchor).unwrap();

    ctx.update_and_render_ui();

    let outer = root.state().try_read(RootState::rect).unwrap();
    let body = ctx.debug_root_body(root.id()).unwrap();
    assert_eq!(outer.x, anchor.x);
    assert_eq!(outer.y, anchor.y);
    assert_eq!(outer.width, anchor.width, "AUTO_HEIGHT must retain the programmed width");
    assert!(outer.height > anchor.height, "popup height must still follow its items");
    for item in item_ids {
        let item = ctx.debug_root_node_rect(root.id(), item).unwrap();
        assert_eq!((item.x, item.width), (body.x, body.width));
    }
}

#[test]
fn auto_width_preserves_programmed_height() {
    let (_, item) = ListItem::create(ListItemParameters::new("intrinsic width"));
    let mut ctx = context();
    let root = ctx.create_popup("horizontal", Node::widget(item));
    let programmed = rect(20, 30, 1, 120);
    ctx.set_root_options(
        root.id(),
        WindowOption::FRAME | WindowOption::AUTO_WIDTH | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
    )
    .unwrap();
    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.set_root_rect(root.id(), programmed).unwrap();

    ctx.update_and_render_ui();

    let outer = root.state().try_read(RootState::rect).unwrap();
    assert!(outer.width > programmed.width, "AUTO_WIDTH must derive width from content");
    assert_eq!(outer.height, programmed.height, "AUTO_WIDTH must retain the programmed height");
}

#[test]
fn auto_size_ignores_the_previous_rect_for_flexible_row_grid_and_stack_tracks() {
    let row_children = (0..5)
        .map(|index| Node::widget(Custom::create(CustomParameters::new(format!("row {index}")))))
        .collect::<Vec<_>>();
    let (_, row) = Row::create(RowParameters::new(
        [
            SizePolicy::Fixed(18),
            SizePolicy::Auto,
            SizePolicy::Fraction(0.5),
            SizePolicy::Weight(1.0),
            SizePolicy::Remainder(4),
        ],
        SizePolicy::Weight(1.0),
        row_children,
    ));
    let grid_items = (0..5)
        .map(|index| Node::widget(Custom::create(CustomParameters::new(format!("grid {index}")))))
        .collect::<Vec<_>>();
    let (_, grid) = Grid::create(GridParameters::new(
        [
            SizePolicy::Fixed(18),
            SizePolicy::Auto,
            SizePolicy::Fraction(0.5),
            SizePolicy::Weight(1.0),
            SizePolicy::Remainder(4),
        ],
        [SizePolicy::Weight(1.0)],
        grid_items,
    ));
    let (_, stack) = Stack::create(StackParameters::new(
        SizePolicy::Fraction(0.5),
        SizePolicy::Weight(1.0),
        StackDirection::TopToBottom,
        [
            Node::widget(Custom::create(CustomParameters::new("stack first"))),
            Node::widget(Custom::create(CustomParameters::new("stack second"))),
        ],
    ));
    let (_, content) = Column::create(ColumnParameters::new([row, grid, stack]));
    let mut ctx = context();
    let root = ctx.create_popup("intrinsic", content);
    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.set_root_rect(root.id(), rect(20, 30, 2_000, 3_000)).unwrap();

    ctx.update_and_render_ui();

    let outer = root.state().try_read(RootState::rect).unwrap();
    assert!(
        outer.width < 1_000 && outer.height < 1_000,
        "AUTO_SIZE must derive both axes from content: {outer:?}"
    );
}

#[test]
fn body_input_falls_through_chrome_to_the_application_node() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.create_window("window", rect(20, 20, 140, 100), content);
    ctx.update_and_render_ui();
    let body = ctx.debug_root_body(root.id()).unwrap();

    ctx.mousedown(body.x + body.width / 2, body.y + body.height / 2, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(button.try_update(ButtonState::take_submitted), Some(true));
    assert_eq!(root.state().try_update(RootState::take_submitted), Some(false));
}

#[test]
fn post_tree_chrome_overlay_is_submitted_after_custom_descendant_rendering() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new_test(backend, Dimensioni::new(320, 240));
    let custom = ctx.register_custom_renderer(|frame, _args| frame.record_marker("application content")).unwrap();
    let content = Node::custom_render(Custom::create(CustomParameters::new("custom")), custom);
    let _root = ctx.create_window("window", rect(20, 20, 140, 100), content);

    ctx.update_and_render_ui();
    let events = log.snapshot();
    let marker = events
        .iter()
        .position(|event| matches!(event, RenderEvent::Marker(name) if name == "application content"))
        .expect("custom descendant marker missing");
    let final_atlas_quad = events
        .iter()
        .rposition(|event| matches!(event, RenderEvent::AtlasQuad(_)))
        .expect("root overlay quad missing");
    assert!(final_atlas_quad > marker);
}
