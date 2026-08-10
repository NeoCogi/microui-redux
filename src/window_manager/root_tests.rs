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
    color, rect, AtlasHandle, Button, ButtonParameters, ButtonSubmitted, Checkbox, CheckboxParameters, Column, ColumnParameters, Custom, CustomParameters,
    Dimensioni, Disclosure, DisclosureParameters, Grid, GridParameters, KeyMode, MouseButton, Node, Policy, Row, RowParameters, ScrollArea, ScrollAreaOption,
    ListItem, ListItemParameters, ScrollAreaParameters, SizePolicy, Stack, StackDirection, StackParameters, Style, Textbox, TextboxChanged, TextboxParameters,
    TypedWidgetHandle, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx,
};
use crate::render::{FrameInfo, RenderError};
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

fn event_counter<E: crate::WidgetEvent>(event: crate::WidgetEventHandle<E>) -> crate::Session<usize, ()> {
    let mut session = crate::Session::new();
    session.connect(event, |_| ()).unwrap();
    session.subscribe(|count: &mut usize, _: &(), _| *count += 1);
    session
}

struct OrderedProbe {
    events: Vec<&'static str>,
    held_buttons: Vec<u32>,
    held_keys: Vec<u32>,
    measures: Cell<usize>,
    updates: usize,
    paints: usize,
    hovered: bool,
    opt: WidgetOption,
}

impl OrderedProbe {
    fn create(opt: WidgetOption) -> (TypedWidgetHandle<Self>, Node) {
        Node::typed_widget(Self {
            events: Vec::new(),
            held_buttons: Vec::new(),
            held_keys: Vec::new(),
            measures: Cell::new(0),
            updates: 0,
            paints: 0,
            hovered: false,
            opt,
        })
    }
}

impl Widget for OrderedProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, event: Option<&UiInputEvent>) {
        self.updates += 1;
        self.held_buttons.push(ctx.mouse_buttons().bits());
        self.held_keys.push(ctx.key_modes().bits());
        self.hovered = ctx.hovered();
        if let Some(event) = event {
            self.events.push(match event {
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
        self.paints += 1;
    }
}

impl crate::LeafWidget for OrderedProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        self.measures.set(self.measures.get() + 1);
        Dimensioni::new(80, 60)
    }
}

struct CommitProbe {
    intrinsic_height: i32,
    grow_to: Option<i32>,
    presses: usize,
    painted_rects: Rc<RefCell<Vec<Recti>>>,
    opt: WidgetOption,
}

impl CommitProbe {
    fn new(intrinsic_height: i32, grow_to: Option<i32>) -> (TypedWidgetHandle<Self>, Node, Rc<RefCell<Vec<Recti>>>) {
        let painted_rects = Rc::new(RefCell::new(Vec::new()));
        let probe = Self {
            intrinsic_height,
            grow_to,
            presses: 0,
            painted_rects: painted_rects.clone(),
            opt: WidgetOption::NONE,
        };
        let (handle, node) = Node::typed_widget(probe);
        (handle, node, painted_rects)
    }
}

impl Widget for CommitProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, event: Option<&UiInputEvent>) {
        if let Some(grow_to) = self.grow_to.take() {
            self.intrinsic_height = grow_to;
        }
        if matches!(event, Some(UiInputEvent::MouseDown { .. })) {
            self.presses += 1;
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // This is deliberately a rendering-only test cache: semantic state remains observational.
        self.painted_rects.borrow_mut().push(ctx.screen_content_rect());
    }
}

impl crate::LeafWidget for CommitProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(40, self.intrinsic_height)
    }
}

struct SiblingMutationProbe {
    value: i32,
    observed_during_update: Vec<i32>,
    target: Option<(TypedWidgetHandle<SiblingMutationProbe>, i32)>,
    opt: WidgetOption,
}

impl SiblingMutationProbe {
    fn new(value: i32, target: Option<(TypedWidgetHandle<SiblingMutationProbe>, i32)>) -> (TypedWidgetHandle<Self>, Node) {
        let probe = Self {
            value,
            observed_during_update: Vec::new(),
            target,
            opt: WidgetOption::NONE,
        };
        Node::typed_widget(probe)
    }
}

impl Widget for SiblingMutationProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        if let Some((target, value)) = &self.target {
            target
                .try_update(|state| state.value = *value)
                .expect("the sibling target must not be borrowed yet or anymore");
        }
        self.observed_during_update.push(self.value);
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for SiblingMutationProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

struct CountedProbe {
    updates: Rc<Cell<usize>>,
    opt: WidgetOption,
}

impl CountedProbe {
    fn new(updates: Rc<Cell<usize>>) -> Self {
        Self { updates, opt: WidgetOption::NONE }
    }
}

impl Widget for CountedProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        self.updates.set(self.updates.get() + 1);
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for CountedProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

struct TopologyMutator {
    same_container_blocked: bool,
    other_container_changed: bool,
    same_container: Rc<RefCell<Option<TypedWidgetHandle<Column>>>>,
    other_container: TypedWidgetHandle<Column>,
    candidate: Option<Node>,
    opt: WidgetOption,
}

impl Widget for TopologyMutator {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
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
        self.same_container_blocked = same_container_blocked;
        self.other_container_changed = other_container_changed;
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for TopologyMutator {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

#[test]
fn routed_recipient_gets_one_event_while_every_node_still_updates_in_fifo_order() {
    let (state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.create_window("window", rect(10, 10, 100, 80), probe);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(20, 20);
    ctx.mousedown(20, 20, MouseButton::LEFT);
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("x");
    ctx.keyup(KeyMode::SHIFT);
    ctx.mouseup(20, 20, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));

    state
        .try_read(|state| {
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
        })
        .unwrap();
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

    assert_eq!(root.widget().try_read(RootChrome::is_visible), Some(false));
}

#[test]
fn disclosure_update_commits_child_geometry_before_the_next_queued_press() {
    let (button, child) = button_content("child");
    let mut session = event_counter(button);
    let mut submissions = 0;
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

    assert_eq!(disclosure.try_read(Disclosure::is_expanded), Some(true));
    assert!(session.dispatch(&mut submissions));
    assert_eq!(submissions, 1);
}

#[test]
fn collapsed_disclosure_skips_descendant_phases_and_drops_targets_only_on_removal() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new_test(backend, Dimensioni::new(320, 240));

    let (probe_state, probe_node) = OrderedProbe::create(WidgetOption::HOLD_FOCUS);
    let probe_id = probe_node.id();

    let custom = ctx
        .register_custom_renderer(|frame, _args| frame.record_marker("disclosure custom child"))
        .unwrap();
    let custom_runtime = Custom::create(CustomParameters::new("custom"));
    let (custom_state, custom_node) = Node::typed_custom_render(custom_runtime, custom);
    let custom_node = custom_node.with_policy(Policy::fixed(20, 10));

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
        .try_read(|state| (state.measures.get(), state.updates, state.paints, state.events.clone()))
        .unwrap();
    assert_eq!(visible_counts.3, ["down", "up"]);

    disclosure.try_update(Disclosure::collapse).unwrap();
    log.clear();
    ctx.mousemove(probe_rect.x + 2, probe_rect.y + 2);
    ctx.update_and_render_ui();

    assert_eq!(
        probe_state.try_read(|state| (state.measures.get(), state.updates, state.paints)),
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

    disclosure.try_update(Disclosure::expand).unwrap();
    log.clear();
    ctx.text("focus must not return");
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
    assert!(
        log.snapshot()
            .iter()
            .any(|event| matches!(event, RenderEvent::Marker(name) if name == "disclosure custom child"))
    );

    disclosure.try_update(Disclosure::clear).unwrap();
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
    let growing_id = growing.id();
    let (target_state, target, _) = CommitProbe::new(10, None);
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
    let (_, content) = Column::create(ColumnParameters::new([earlier, later, already_updated, late_mutator]));
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
        same_container_blocked: false,
        other_container_changed: false,
        same_container: same_container.clone(),
        other_container: other_container.clone(),
        candidate: Some(candidate),
        opt: WidgetOption::NONE,
    };
    let (mutator_state, mutator) = Node::typed_widget(mutator);
    let (outer_container, content) = Column::create(ColumnParameters::new([mutator, other_node]));
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
    assert_eq!(outer_container.try_read(Column::len), Some(Some(2)));
    assert_eq!(other_container.try_read(Column::len), Some(Some(1)));
    assert_eq!(inserted_updates.get(), 1, "the newly inserted later descendant participates in the same update");
}

#[test]
fn programmatic_topology_mutation_needs_only_an_empty_queue_layout_commit() {
    let (_, first, _) = CommitProbe::new(10, None);
    let (column, content) = Column::create(ColumnParameters::new([first]));
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), content);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    let (_, appended, _) = CommitProbe::new(18, None);
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
fn traversal_reaching_widget_borrowed_by_an_access_closure_reports_the_runtime_diagnostic() {
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 140, 100), widget);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);

    let update_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.update_ui(dimensions));
    }))
    .expect_err("layout must diagnose the active TextBlock borrow");
    let update_message = panic_message(update_panic.as_ref());
    assert!(update_message.contains("retained widget invariant violated"));
    assert!(update_message.contains("typed access closure must finish before runtime traversal"));

    // Once the access closure has unwound and released its borrow, the same commit is valid.
    ctx.update_ui(dimensions);
    let paint_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.frame(frame_info(dimensions)).render_ui().unwrap());
    }))
    .expect_err("paint must diagnose the active TextBlock borrow");
    let paint_message = panic_message(paint_panic.as_ref());
    assert!(paint_message.contains("retained widget invariant violated"));

    ctx.frame(frame_info(dimensions)).render_ui().unwrap();

    // A shared access closure is likewise incompatible when the routed update needs to mutate the
    // same cell, even though the synchronization layout's shared reads are allowed by RefCell.
    let (checkbox, checkbox_node) = Checkbox::create(CheckboxParameters::new("checkbox", false));
    let checkbox_id = checkbox_node.id();
    let mut checkbox_ctx = context();
    let checkbox_root = checkbox_ctx.create_window("checkbox", rect(0, 0, 140, 100), checkbox_node);
    checkbox_ctx
        .set_root_options(checkbox_root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    checkbox_ctx.update_ui(dimensions);
    let checkbox_rect = checkbox_ctx.debug_root_node_rect(checkbox_root.id(), checkbox_id).unwrap();
    checkbox_ctx.mousedown(checkbox_rect.x + 1, checkbox_rect.y + 1, MouseButton::LEFT);
    let read_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        checkbox.try_read(|_| checkbox_ctx.update_ui(dimensions));
    }))
    .expect_err("Checkbox::update must diagnose the active shared Checkbox borrow");
    let read_message = panic_message(read_panic.as_ref());
    assert!(read_message.contains("retained widget invariant violated"));
    assert!(read_message.contains("typed access closure must finish before runtime traversal"));
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

fn button_content(label: &str) -> (crate::WidgetEventHandle<ButtonSubmitted>, Node) {
    let (widget, node) = Button::create(ButtonParameters::new(label));
    (widget.submitted(), node)
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

    let (first_widget, first) = Button::create(ButtonParameters::new("first"));
    let first_submitted = first_widget.submitted();
    let first_id = first.id();
    let (second_widget, second) = Button::create(ButtonParameters::new("second"));
    let second_submitted = second_widget.submitted();
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
    session.connect(first_submitted, |_| Message::FirstSubmitted).unwrap();
    session.connect(second_submitted, |_| Message::SecondSubmitted).unwrap();
    session.subscribe(|model: &mut Model, message: &Message, _emit| match message {
        Message::FirstSubmitted => model.submissions.push("first"),
        Message::SecondSubmitted => model.submissions.push("second"),
    });
    let mut model = Model::default();

    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.update_ui_session(dimensions, &mut session, &mut model);

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

    let (widget, node) = Textbox::create(TextboxParameters::new(""));
    let changed = widget.changed();
    let node_id = node.id();
    let mut ctx = context();
    let root = ctx.create_window("textbox signal", rect(0, 0, 140, 100), node);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    let textbox_rect = ctx.debug_root_node_rect(root.id(), node_id).unwrap();

    let mut session = crate::Session::new();
    session.connect(changed, Message::Changed).unwrap();
    session.subscribe(|model: &mut Model, message: &Message, _emit| match message {
        Message::Changed(event) => model.changes.push((event.text.clone(), event.cursor)),
    });
    let mut model = Model::default();

    ctx.mousedown(textbox_rect.x + 1, textbox_rect.y + 1, MouseButton::LEFT);
    ctx.text("é");
    ctx.update_ui_session(dimensions, &mut session, &mut model);

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
    session.subscribe(|count: &mut usize, message: &Message, _emit| match message {
        Message::Increment => *count += 1,
    });
    let mut count = 0;

    ctx.update_ui_session(Dimensioni::new(320, 240), &mut session, &mut count);

    assert_eq!(count, 1);
}

#[test]
fn creation_returns_typed_persistent_root_widget() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(20, 30, 120, 90), empty_content());

    assert!(root.widget().is_alive());
    assert_eq!(root.widget().try_read(|state| state.name().to_owned()), Some("window".to_owned()));
    assert_eq!(
        root.widget().try_read(|state| {
            let rect = state.rect();
            (rect.x, rect.y, rect.width, rect.height)
        }),
        Some((20, 30, 120, 90))
    );
    assert_eq!(root.widget().try_read(RootChrome::is_visible), Some(true));
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
    assert_eq!(root.widget().try_read(RootChrome::is_visible), Some(false));
    assert!(button.is_alive());

    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(root.widget().try_read(RootChrome::is_visible), Some(true));
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
    assert!(!root.widget().is_alive());
    assert!(!button.is_alive());

    let replacement = ctx.create_window("second", rect(0, 0, 100, 80), empty_content());
    assert_ne!(replacement.id(), destroyed_id);
}

#[test]
fn active_state_upgrade_does_not_keep_destroyed_root_topology_alive() {
    let mut ctx = context();
    let (child, content) = button_content("child");
    let root = ctx.create_window("window", rect(0, 0, 100, 80), content);
    let state = root.widget().clone();

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
fn same_widget_setter_conflict_is_checked() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(0, 0, 100, 80), empty_content());
    let mut conflict = None;

    root.widget()
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

    first.widget().try_update(|_| result = Some(ctx.set_root_visible(second.id(), true))).unwrap();

    assert_eq!(result, Some(Err(RootMutationError::Borrowed)));
    assert_eq!(first.widget().try_read(RootChrome::is_visible), Some(true));
    assert_eq!(second.widget().try_read(RootChrome::is_visible), Some(false));
}

#[test]
fn dynamic_container_root_changes_descendants_without_replacing_the_root() {
    let mut ctx = context();
    let (column, content) = Column::create(ColumnParameters::default());
    let root = ctx.create_window("dynamic", rect(0, 0, 140, 100), content);
    let root_id = root.id();
    let (button, widget) = Button::create(ButtonParameters::new("new child"));

    let inserted = column.try_update(|column| column.push(widget)).unwrap();
    assert!(inserted.is_ok());
    ctx.update_and_render_ui();
    assert_eq!(root.id(), root_id);
    assert!(button.is_alive());
    assert_eq!(ctx.debug_root_node_count(root_id), Some(3));

    assert_eq!(column.try_update(|column: &mut Column| column.remove_drop(0)), Some(Some(true)));
    assert!(!button.is_alive());
    assert!(root.widget().is_alive());
}

#[test]
fn showing_a_popup_atomically_hides_the_previous_one() {
    let mut ctx = context();
    let first = ctx.create_popup("first", empty_content());
    let second = ctx.create_popup("second", empty_content());
    let mut session = event_counter(first.submitted());
    let mut submissions = 0;

    ctx.set_root_visible(first.id(), true).unwrap();
    assert_eq!(first.widget().try_read(RootChrome::is_visible), Some(true));
    ctx.set_root_visible(second.id(), true).unwrap();

    assert_eq!(first.widget().try_read(RootChrome::is_visible), Some(false));
    assert_eq!(second.widget().try_read(RootChrome::is_visible), Some(true));
    assert!(!session.dispatch(&mut submissions));
    assert_eq!(submissions, 0);
}

#[test]
fn outside_popup_press_hides_and_records_typed_submission() {
    let mut ctx = context();
    let popup = ctx.create_popup("popup", empty_content());
    ctx.set_root_options(popup.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.set_root_visible(popup.id(), true).unwrap();
    ctx.set_root_rect(popup.id(), rect(20, 20, 80, 60)).unwrap();
    let mut event_session = crate::Session::new();
    event_session.connect(popup.submitted(), |event| event).unwrap();
    event_session.subscribe(|events: &mut Vec<RootSubmitted>, event: &RootSubmitted, _| events.push(*event));
    let mut submissions = Vec::new();
    ctx.update_and_render_ui();

    ctx.mousedown(200, 180, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(popup.widget().try_read(RootChrome::is_visible), Some(false));
    assert!(event_session.dispatch(&mut submissions));
    assert_eq!(submissions, [RootSubmitted::PopupDismissed]);
    ctx.set_root_visible(popup.id(), true).unwrap();
    ctx.set_root_visible(popup.id(), false).unwrap();
    assert!(!event_session.dispatch(&mut submissions));
}

#[test]
fn outside_popup_press_dismisses_then_routes_once_to_the_revealed_root() {
    let mut ctx = context();
    let (button, content) = button_content("behind");
    let mut session = event_counter(button);
    let mut submissions = 0;
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

    assert_eq!(popup.widget().try_read(RootChrome::is_visible), Some(false));
    assert!(session.dispatch(&mut submissions));
    assert_eq!(submissions, 1);
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
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let first = ctx.create_window("first", rect(0, 0, 100, 80), empty_content());
    let second = ctx.create_window("second", rect(160, 120, 100, 80), probe.with_policy(Policy::fill()));
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
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::GRAB_SCROLL);
    let mut ctx = context();
    let first = ctx.create_window("first", rect(0, 0, 100, 80), empty_content());
    let second = ctx.create_window("second", rect(160, 120, 100, 80), probe.with_policy(Policy::fill()));
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
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let first = ctx.create_window("first", rect(0, 0, 100, 80), probe.with_policy(Policy::fill()));
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
    let mut behind_session = event_counter(behind_button);
    let mut behind_submissions = 0;
    let window = ctx.create_window("window", rect(0, 0, 100, 80), behind_content);
    ctx.set_root_options(window.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let (dialog_button, dialog_content) = button_content("dialog");
    let mut dialog_session = event_counter(dialog_button);
    let mut dialog_submissions = 0;
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
    assert!(!behind_session.dispatch(&mut behind_submissions));

    ctx.mousedown(130, 110, MouseButton::LEFT);
    ctx.mouseup(130, 110, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(dialog_session.dispatch(&mut dialog_submissions));
    assert_eq!(dialog_submissions, 1);

    assert!(ctx.bring_root_to_front(window.id()));
    ctx.set_root_visible(window.id(), true).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog"]);
    assert!(ctx.debug_root_zindex(dialog.id()).unwrap() > ctx.debug_root_zindex(window.id()).unwrap());

    ctx.set_root_visible(dialog.id(), false).unwrap();
    assert_eq!(ctx.debug_modal_root(), None);
    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(behind_session.dispatch(&mut behind_submissions));
    assert_eq!(behind_submissions, 1);
}

#[test]
fn active_dialog_keeps_a_visible_popup_below_and_input_blocked() {
    let mut ctx = context();
    let (popup_button, popup_content) = button_content("popup");
    let mut session = event_counter(popup_button);
    let mut submissions = 0;
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
    assert!(!session.dispatch(&mut submissions));

    ctx.set_root_visible(dialog.id(), false).unwrap();
    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(session.dispatch(&mut submissions));
    assert_eq!(submissions, 1);
}

#[test]
fn modal_activation_clears_underlying_focus_and_blocks_keyboard_input() {
    let (state, probe) = OrderedProbe::create(WidgetOption::HOLD_FOCUS);
    let mut ctx = context();
    let window = ctx.create_window("window", rect(0, 0, 100, 80), probe);
    ctx.set_root_options(window.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dialog = ctx.create_dialog("dialog", rect(120, 100, 100, 80), empty_content());
    ctx.set_root_options(dialog.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.set_root_visible(dialog.id(), true).unwrap();
    let updates_before_modal_input = state.try_read(|state| state.updates).unwrap();
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("blocked");
    ctx.keyup(KeyMode::SHIFT);
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.updates), Some(updates_before_modal_input));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.set_root_visible(dialog.id(), false).unwrap();
    ctx.text("still unfocused");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.text("accepted");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up", "down", "text"]));
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
    assert_eq!(window.widget().try_read(RootChrome::is_moving), Some(true));
    let before = window.widget().try_read(RootChrome::rect).unwrap();

    ctx.set_root_visible(dialog.id(), true).unwrap();
    assert_eq!(ctx.debug_root_has_pointer_capture(window.id()), Some(false));
    assert_eq!(window.widget().try_read(RootChrome::is_active), Some(false));
    ctx.mousemove(title.x + 20, title.y + 20);
    ctx.mouseup(title.x + 20, title.y + 20, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(
        window.widget().try_read(|state| {
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
    assert_eq!(second.widget().try_read(RootChrome::is_visible), Some(false));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.set_root_visible(second.id(), true).unwrap();
    assert!(ctx.destroy_root(second.id()));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert!(ctx.destroy_root(first.id()));
    assert_eq!(ctx.debug_modal_root(), None);
}

#[test]
fn fronting_remains_widget_borrow_independent_and_does_not_replace_the_active_modal() {
    let mut ctx = context();
    let first = ctx.create_dialog("first", rect(20, 20, 120, 90), empty_content());
    let middle = ctx.create_dialog("middle", rect(30, 30, 120, 90), empty_content());
    let second = ctx.create_dialog("second", rect(40, 40, 120, 90), empty_content());
    ctx.set_root_visible(first.id(), true).unwrap();
    ctx.set_root_visible(middle.id(), true).unwrap();
    ctx.set_root_visible(second.id(), true).unwrap();

    first.widget().try_update(|_| assert!(ctx.bring_root_to_front(first.id()))).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));
    assert!(ctx.debug_root_zindex(second.id()).unwrap() > ctx.debug_root_zindex(first.id()).unwrap());

    ctx.set_root_visible(second.id(), false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(middle.id()));
    ctx.set_root_visible(second.id(), true).unwrap();

    second.widget().try_update(|_| assert!(ctx.bring_root_to_front(second.id()))).unwrap();
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

    first.widget().try_update(|_| ctx.set_root_visible(second.id(), false).unwrap()).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.set_root_visible(second.id(), true).unwrap();
    first.widget().try_update(|_| assert!(ctx.destroy_root(second.id()))).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    first.widget().try_update(|_| assert!(ctx.destroy_root(window.id()))).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
}

#[test]
fn title_drag_and_close_record_typed_root_events() {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    enum Event {
        Changed(i32, i32, i32, i32),
        Submitted(RootSubmitted),
    }

    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), empty_content());
    let mut session = crate::Session::new();
    session
        .connect(root.changed(), |event| {
            Event::Changed(event.rect.x, event.rect.y, event.rect.width, event.rect.height)
        })
        .unwrap();
    session.connect(root.submitted(), Event::Submitted).unwrap();
    session.subscribe(|events: &mut Vec<Event>, event: &Event, _| events.push(*event));
    let mut events = Vec::new();
    ctx.update_and_render_ui();
    let (title, _, _) = ctx.debug_root_chrome(root.id()).unwrap();
    let title = title.unwrap();
    let drag_x = title.x + 2;
    let drag_y = title.y + 2;

    ctx.mousedown(drag_x, drag_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.widget().try_read(RootChrome::is_moving), Some(true));
    assert!(!session.dispatch(&mut events));

    ctx.mousemove(drag_x + 10, drag_y + 8);
    ctx.update_and_render_ui();
    assert!(session.dispatch(&mut events));
    assert_eq!(events, [Event::Changed(40, 38, 140, 100)]);

    ctx.mouseup(drag_x + 10, drag_y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.widget().try_read(RootChrome::is_active), Some(false));

    let close = ctx.debug_root_chrome(root.id()).unwrap().1.unwrap();
    let close_x = close.x + close.width / 2;
    let close_y = close.y + close.height / 2;
    ctx.mousedown(close_x, close_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.widget().try_read(RootChrome::is_visible), Some(false));
    assert!(session.dispatch(&mut events));
    assert_eq!(events, [Event::Changed(40, 38, 140, 100), Event::Submitted(RootSubmitted::Close)]);
}

#[test]
fn resize_overlay_preempts_content_where_the_grip_overlaps_the_root_body() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), probe.with_policy(Policy::fill()));
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

    let before = root.widget().try_read(RootChrome::rect).unwrap();
    ctx.mousedown(press.x, press.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(root.widget().try_read(RootChrome::is_resizing), Some(true));
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    ctx.mousemove(press.x + 12, press.y + 8);
    ctx.mouseup(press.x + 12, press.y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(
        root.widget().try_read(|state| (state.rect().width, state.rect().height)),
        Some((before.width + 12, before.height + 8))
    );
}

#[test]
fn content_capture_remains_exclusive_while_dragging_across_root_chrome() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), probe.with_policy(Policy::fill()));
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
    assert_eq!(root.widget().try_read(RootChrome::is_resizing), Some(false));

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
    assert_eq!(root.widget().try_read(RootChrome::is_moving), Some(true));

    ctx.set_root_visible(root.id(), false).unwrap();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(root.widget().try_read(RootChrome::is_active), Some(false));

    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(root.widget().try_read(RootChrome::is_active), Some(false));
}

#[test]
fn chrome_geometry_exposes_one_body_and_auto_size_tracks_content() {
    let mut ctx = context();
    let (_, text) = crate::TextBlock::create(crate::TextBlockParameters::new("window content"));
    let root = ctx.create_popup("popup", text);
    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_and_render_ui();

    let body = ctx.debug_root_body(root.id()).unwrap();
    let content = ctx.debug_root_content_size(root.id()).unwrap();
    let outer = root.widget().try_read(RootChrome::rect).unwrap();
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
            let (_, node) = ListItem::create(ListItemParameters::new(label));
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

    let outer = root.widget().try_read(RootChrome::rect).unwrap();
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
    let root = ctx.create_popup("horizontal", item);
    let programmed = rect(20, 30, 1, 120);
    ctx.set_root_options(
        root.id(),
        WindowOption::FRAME | WindowOption::AUTO_WIDTH | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
    )
    .unwrap();
    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.set_root_rect(root.id(), programmed).unwrap();

    ctx.update_and_render_ui();

    let outer = root.widget().try_read(RootChrome::rect).unwrap();
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

    let outer = root.widget().try_read(RootChrome::rect).unwrap();
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
    let mut button_session = event_counter(button);
    let mut root_session = event_counter(root.submitted());
    let mut button_submissions = 0;
    let mut root_submissions = 0;
    ctx.update_and_render_ui();
    let body = ctx.debug_root_body(root.id()).unwrap();

    ctx.mousedown(body.x + body.width / 2, body.y + body.height / 2, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert!(button_session.dispatch(&mut button_submissions));
    assert_eq!(button_submissions, 1);
    assert!(!root_session.dispatch(&mut root_submissions));
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
