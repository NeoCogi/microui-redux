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
    color, rect, AtlasHandle, Button, ButtonParameters, ButtonSubmitted, Checkbox, CheckboxParameters, Combo, ComboParameters, ComboSubmitted, Custom,
    CustomParameters, Constraints, Context, Dimensioni, Disclosure, DisclosureParameters, Ui, Grid, GridParameters, KeyMode, Linear, LinearItem,
    LinearParameters, Menu, MenuBar, MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted, MouseButton, Node, ScrollArea, ScrollAreaOption, ListItem,
    ListItemParameters, ScrollAreaParameters, Style, Textbox, TextboxChanged, TextBlock, TextBlockParameters, TextboxParameters, TrackSize, TypedWidgetHandle,
    UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx,
};
use crate::render::{FrameInfo, RenderError};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn context() -> Context<NoopRenderer> {
    Context::new_test(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240))
}

/// Delivers one complete left-button click to the center of a committed test rectangle.
fn click_rect(context: &mut Context<NoopRenderer>, target: Recti) {
    // Use the center rather than an edge so adjacent menu rows and popup borders cannot receive the
    // test input because of an inclusive/exclusive boundary detail.
    let x = target.x + target.width / 2;
    let y = target.y + target.height / 2;
    context.mousedown(x, y, MouseButton::LEFT);
    context.update_and_render_ui();
    context.mouseup(x, y, MouseButton::LEFT);
    context.update_and_render_ui();
}

fn empty_content() -> Node {
    Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1
}

fn frame_info(dimensions: Dimensioni) -> FrameInfo {
    FrameInfo::try_new(dimensions, color(0, 0, 0, 255)).unwrap()
}

/// Leaf used when a test needs content with an exact intrinsic size.
struct DesiredSize(Dimensioni);

impl Widget for DesiredSize {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NONE
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for DesiredSize {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        self.0
    }
}

fn desired_size_node(width: i32, height: i32) -> Node {
    Node::widget(DesiredSize(Dimensioni::new(width, height)))
}

fn increment_event_counter<E>(count: &mut usize, _: &E) {
    *count += 1;
}

fn event_counter<E: crate::WidgetEvent>(port: crate::WidgetEventPortHandle<E>) -> crate::event::WidgetEventDispatcher<usize> {
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    dispatcher.subscribe(port, increment_event_counter::<E>).unwrap();
    dispatcher
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
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
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
        if event.is_some()
            && let Some(grow_to) = self.grow_to.take()
        {
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
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
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
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
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
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

struct TopologyMutator {
    same_container_blocked: bool,
    other_container_changed: bool,
    same_container: Rc<RefCell<Option<TypedWidgetHandle<Linear>>>>,
    other_container: TypedWidgetHandle<Linear>,
    candidate: Option<Node>,
    opt: WidgetOption,
}

impl Widget for TopologyMutator {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    #[allow(clippy::result_large_err)] // Failed topology changes must preserve the unique node owner.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        let Some(candidate) = self.candidate.take() else { return };
        let same_container = self
            .same_container
            .borrow()
            .as_ref()
            .expect("outer container handle must be installed before traversal")
            .clone();
        let same_container_blocked = same_container.try_update(|state| state.remove_drop(usize::MAX)).flatten().is_none();
        let other_container_changed = match self
            .other_container
            .try_update_with(candidate, |state, node| state.push(node).map_err(crate::LinearItem::into_node))
        {
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
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

#[test]
fn routed_recipient_gets_one_event_while_every_node_still_updates_in_fifo_order() {
    let (state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 100, 80), probe));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
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
    assert_eq!(metrics.updates, 6, "the application node receives each event exactly once");
}

#[test]
fn update_drains_each_input_into_one_full_update_and_one_followup_layout() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));
    let dimensions = Dimensioni::new(320, 240);

    ctx.mousemove(20, 20);
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("x");
    ctx.update_ui(dimensions);

    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 4, "one synchronization layout plus one per input event");
    assert_eq!(metrics.updates, 3, "three events update the application tree");
    assert_eq!(metrics.paints, 0, "update_ui must not paint");

    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 4, "render must not lay out");
    assert_eq!(metrics.updates, 3, "render must not update widgets");
    assert_eq!(metrics.paints, 1, "render paints the application tree exactly once");
}

#[test]
fn render_preflight_requires_a_matching_commit_and_never_acquires_backend_on_error() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::<_>::new(backend);
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));
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

    ctx.ui().set_window_rect(&root, rect(20, 20, 120, 90)).unwrap();
    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());
}

#[test]
fn invalid_update_dimensions_panic_before_dequeue_and_preserve_pending_input() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));
    ctx.text("still pending");

    // Catch only to keep exercising the same context after the rejected commit; the companion
    // `should_panic` test below verifies the diagnostic without inspecting an erased payload.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.update_ui(Dimensioni::new(0, 240));
    }));
    assert!(result.is_err(), "invalid dimensions must panic");

    ctx.update_ui(Dimensioni::new(320, 240));
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 2);
    assert_eq!(metrics.updates, 1, "the event queued before the panic must still be drained");
}

#[test]
#[should_panic(expected = "update_ui dimensions must be positive")]
fn invalid_update_dimensions_reports_the_expected_diagnostic() {
    // Drive the public commit entry point directly so `should_panic` checks the emitted text while
    // keeping the test independent of panic-payload representation.
    context().update_ui(Dimensioni::new(0, 240));
}

#[test]
fn every_event_layout_commit_updates_hit_geometry_for_the_next_queued_event() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
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

    assert_eq!(ctx.debug_root_visible(root.id()), Some(false));
}

#[test]
fn disclosure_update_commits_child_geometry_before_the_next_queued_press() {
    let (button, child) = button_content("child");
    let mut dispatcher = event_counter(button);
    let mut submissions = 0;
    let (disclosure, node) = Disclosure::create(DisclosureParameters::header("section", false, [child]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), node));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.mousedown(10, 28, MouseButton::LEFT);
    ctx.update_ui(dimensions);

    assert_eq!(disclosure.try_read(Disclosure::is_expanded), Some(true));
    assert!(dispatcher.dispatch(&mut submissions));
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
    let (disclosure, content) = Disclosure::create(DisclosureParameters::header(
        "section",
        true,
        [LinearItem::content(probe_node), LinearItem::fixed(custom_node, 10).with_fixed_cross(20)],
    ));
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 160, 140), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
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
    let inner_content = desired_size_node(50, 180);
    let (inner, inner_node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, inner_content));
    let inner_id = inner_node.id();
    let outer_tail = Node::widget(Custom::create(CustomParameters::new("outer tail")));
    let (_, outer_content) = Linear::create(LinearParameters::vertical([
        LinearItem::fixed(inner_node, 60).with_fixed_cross(60),
        LinearItem::fixed(outer_tail, 120).with_fixed_cross(60),
    ]));
    let (outer, content) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, outer_content));

    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
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
    let (_, content) = Linear::create(LinearParameters::vertical([growing, target]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
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
    let (_, content) = Linear::create(LinearParameters::vertical([earlier, later, already_updated, late_mutator]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
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
    let (other_container, other_node) = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>()));
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
    let (outer_container, content) = Linear::create(LinearParameters::vertical([mutator, other_node]));
    *same_container.borrow_mut() = Some(outer_container.clone());
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(5, 5);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(
        mutator_state.try_read(|state| (state.same_container_blocked, state.other_container_changed)),
        Some((true, true))
    );
    assert_eq!(outer_container.try_read(Linear::len), Some(Some(2)));
    assert_eq!(other_container.try_read(Linear::len), Some(Some(1)));
    assert_eq!(inserted_updates.get(), 1, "the newly inserted later descendant participates in the same update");
}

#[test]
#[allow(clippy::result_large_err)] // The assertion exercises the ownership-preserving mutation result.
fn programmatic_topology_mutation_needs_only_an_empty_queue_layout_commit() {
    let (_, first, _) = CommitProbe::new(10, None);
    let (column, content) = Linear::create(LinearParameters::vertical([first]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
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
fn traversal_recovers_after_widget_access_closure_borrows_are_released() {
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), widget));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);

    // Catch only so this test can prove the context remains usable after the diagnostic. Dedicated
    // `should_panic` tests below verify the diagnostic text without examining an erased payload.
    let update_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.update_ui(dimensions));
    }));
    assert!(update_result.is_err(), "layout must diagnose the active TextBlock borrow");

    // Once the access closure has unwound and released its borrow, the same commit is valid.
    ctx.update_ui(dimensions);
    let paint_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.frame(frame_info(dimensions)).render_ui().unwrap());
    }));
    assert!(paint_result.is_err(), "paint must diagnose the active TextBlock borrow");

    ctx.frame(frame_info(dimensions)).render_ui().unwrap();

    // A shared access closure is likewise incompatible when the routed update needs to mutate the
    // same cell, even though the synchronization layout's shared reads are allowed by RefCell.
    let (checkbox, checkbox_node) = Checkbox::create(CheckboxParameters::new("checkbox", false));
    let checkbox_id = checkbox_node.id();
    let mut checkbox_ctx = context();
    let checkbox_root = checkbox_ctx.ui().create_window(Window::new("checkbox", rect(0, 0, 140, 100), checkbox_node));
    checkbox_ctx
        .ui()
        .set_window_options(&checkbox_root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    checkbox_ctx.update_ui(dimensions);
    let checkbox_rect = checkbox_ctx.debug_root_node_rect(checkbox_root.id(), checkbox_id).unwrap();
    checkbox_ctx.mousedown(checkbox_rect.x + 1, checkbox_rect.y + 1, MouseButton::LEFT);
    let read_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        checkbox.try_read(|_| checkbox_ctx.update_ui(dimensions));
    }));
    assert!(read_result.is_err(), "Checkbox::update must diagnose the active shared Checkbox borrow");
}

#[test]
#[should_panic(expected = "typed access closure must finish before runtime traversal")]
fn update_traversal_reports_an_active_typed_access_closure() {
    // Hold the concrete widget's mutable access guard across update traversal to provoke the
    // invariant diagnostic at the same boundary used by applications.
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), widget));
    text.try_update(|_| ctx.update_ui(Dimensioni::new(320, 240)));
}

#[test]
#[should_panic(expected = "retained widget invariant violated")]
fn paint_traversal_reports_an_active_typed_access_closure() {
    // Commit once before holding the concrete widget's mutable access guard across paint traversal,
    // ensuring the panic comes from painting rather than update preflight.
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), widget));
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    text.try_update(|_| ctx.frame(frame_info(dimensions)).render_ui().unwrap());
}

#[test]
#[should_panic(expected = "typed access closure must finish before runtime traversal")]
fn routed_update_reports_an_active_shared_access_closure() {
    // Queue a routed checkbox event, then retain a concrete shared access guard while update needs
    // mutable access to that same widget state.
    let (checkbox, checkbox_node) = Checkbox::create(CheckboxParameters::new("checkbox", false));
    let checkbox_id = checkbox_node.id();
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("checkbox", rect(0, 0, 140, 100), checkbox_node));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    let checkbox_rect = ctx.debug_root_node_rect(root.id(), checkbox_id).unwrap();
    ctx.mousedown(checkbox_rect.x + 1, checkbox_rect.y + 1, MouseButton::LEFT);
    checkbox.try_read(|_| ctx.update_ui(dimensions));
}

fn button_content(label: &str) -> (crate::WidgetEventPortHandle<ButtonSubmitted>, Node) {
    let (widget, node) = Button::create(ButtonParameters::new(label));
    (widget.submitted(), node)
}

#[test]
fn widget_handle_events_invoke_state_methods_without_polling() {
    #[derive(Default)]
    struct Model {
        submissions: Vec<&'static str>,
    }

    impl Model {
        fn record(&mut self, label: &&'static str, _: &ButtonSubmitted) {
            self.submissions.push(*label);
        }
    }

    let (first_widget, first) = Button::create(ButtonParameters::new("first"));
    let first_submitted = first_widget.submitted();
    let first_id = first.id();
    let (second_widget, second) = Button::create(ButtonParameters::new("second"));
    let second_submitted = second_widget.submitted();
    let second_id = second.id();
    let (_, content) = Linear::create(LinearParameters::horizontal([LinearItem::fixed(first, 60), LinearItem::fixed(second, 60)]));
    let mut ctx: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240));
    let root = ctx.ui().create_window(Window::new("signal", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    let mut model = Model::default();
    ctx.update_ui_state(dimensions, &mut model);
    let first_rect = ctx.debug_root_node_rect(root.id(), first_id).unwrap();
    let second_rect = ctx.debug_root_node_rect(root.id(), second_id).unwrap();

    ctx.subscribe_with(first_submitted, "first", Model::record).unwrap();
    ctx.subscribe_with(second_submitted, "second", Model::record).unwrap();
    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);

    assert_eq!(model.submissions, ["first", "second", "first"]);
}

#[test]
fn context_aware_handler_creates_and_mutates_every_root_kind_before_layout() {
    #[derive(Default)]
    struct Model {
        window: Option<WindowHandle>,
        dialog: Option<WindowHandle>,
        popup: Option<PopupHandle>,
    }

    impl Model {
        fn create_roots(&mut self, context: &mut Ui<'_>, _: &ButtonSubmitted) {
            // Construct every root kind through the same dispatch capability. The nodes move into
            // Context ownership exactly as they do through the ordinary Context façade.
            let window = context.create_window(Window::new("event window", rect(30, 40, 90, 70), empty_content()));
            let dialog = context
                .create_dialog(&window, Window::new("event dialog", rect(50, 60, 100, 80), empty_content()))
                .unwrap();
            let popup = context.create_popup(&dialog, "event popup", empty_content()).unwrap();

            // Exercise generic root mutation while the event boundary owns exclusive WindowManager
            // access. The layout following dispatch must observe every change.
            context.set_window_size(&window, Dimensioni::new(110, 75)).unwrap();
            context.set_window_visible(&dialog, true).unwrap();
            context.show_popup_at(&popup, rect(180, 30, 1, 1)).unwrap();

            self.window = Some(window);
            self.dialog = Some(dialog);
            self.popup = Some(popup);
        }
    }

    let (submitted, button) = button_content("create roots");
    let button_id = button.id();
    let dimensions = Dimensioni::new(320, 240);
    let mut context: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions);
    let source = context.ui().create_window(Window::new("source", rect(0, 0, 140, 100), button));
    context
        .ui()
        .set_window_options(&source, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    context.subscribe_context(submitted, Model::create_roots).unwrap();
    let mut model = Model::default();
    context.update_ui_state(dimensions, &mut model);
    let button_rect = context.debug_root_node_rect(source.id(), button_id).unwrap();

    context.mousedown(button_rect.x + 1, button_rect.y + 1, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);

    let window = model.window.as_ref().expect("event handler must retain the weak window handle");
    let dialog = model.dialog.as_ref().expect("event handler must retain the weak dialog handle");
    let popup = model.popup.as_ref().expect("event handler must retain the weak popup handle");
    assert_eq!(
        context.debug_root_rect(window.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((30, 40, 110, 75))
    );
    assert_eq!(context.debug_root_visible(dialog.id()), Some(true));
    assert_eq!(context.debug_popup_visible(popup), Some(true));
    assert_eq!(context.debug_popup_rect(popup).map(|rect| (rect.x, rect.y)), Some((180, 30)));
    assert!(context.debug_root_node_count(window.id()).is_some());
    assert!(context.debug_root_node_count(dialog.id()).is_some());
    assert!(popup.is_alive());
}

#[test]
fn typed_events_keep_composed_combo_and_popup_state_synchronized() {
    struct Model {
        combo: TypedWidgetHandle<Combo>,
        popup: PopupHandle,
        submitted_anchor: Option<Recti>,
    }

    impl Model {
        fn combo_submitted(&mut self, context: &mut Ui<'_>, event: &ComboSubmitted) {
            // Compose the semantic Combo with an ordinary window-owned popup.
            if event.open {
                context.show_popup_at(&self.popup, event.anchor).unwrap();
            } else {
                context.hide_popup(&self.popup).unwrap();
            }
            self.submitted_anchor = Some(event.anchor);
        }

        fn popup_submitted(&mut self, event: &PopupEvent) {
            // Popup dismissal becomes typed application input after traversal, where the
            // composed widget can safely reconcile its retained semantic state.
            if matches!(event, PopupEvent::Dismissed) {
                self.combo.close_popup().expect("mounted combo must remain available");
            }
        }
    }

    let dimensions = Dimensioni::new(320, 240);
    let mut context: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions);
    let (combo, combo_node) = Combo::create(ComboParameters::new());
    let combo_id = combo_node.id();
    let source = context.ui().create_window(Window::new("combo source", rect(10, 10, 140, 90), combo_node));
    context.ui().set_window_options(&source, WindowOption::FRAME).unwrap();
    let popup = context
        .ui()
        .create_popup(&source, "combo choices", Node::widget(DesiredSize(Dimensioni::new(100, 60))))
        .unwrap();
    let replacement = context
        .ui()
        .create_popup(&source, "replacement", Node::widget(DesiredSize(Dimensioni::new(80, 40))))
        .unwrap();
    let mut model = Model {
        combo: combo.clone(),
        popup: popup.clone(),
        submitted_anchor: None,
    };
    context.subscribe_context(combo.submitted(), Model::combo_submitted).unwrap();
    context.subscribe(popup.clone(), Model::popup_submitted).unwrap();
    context.update_ui_state(dimensions, &mut model);
    let combo_rect = context.debug_root_node_rect(source.id(), combo_id).unwrap();

    context.mousedown(combo_rect.x + 1, combo_rect.y + 1, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);

    let anchor = model.submitted_anchor.expect("combo submission must carry its routed anchor");
    assert_eq!(context.debug_popup_visible(&popup), Some(true));
    assert_eq!(context.debug_popup_rect(&popup).map(|rect| (rect.x, rect.y)), Some((anchor.x, anchor.y)));
    assert_eq!(combo.is_open(), Some(true));

    // Root chrome is outside the composed popup. Its press dismisses the popup and reconciles the
    // shared Combo state before the subsequent drag can move the source window.
    let title = context.debug_root_chrome(source.id()).unwrap().0.unwrap();
    let title_x = title.x + title.width / 2;
    let title_y = title.y + title.height / 2;
    context.mousedown(title_x, title_y, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(combo.is_open(), Some(false));
    assert_eq!(context.debug_root_moving(source.id()), Some(true));

    context.mousemove(title_x + 15, title_y + 10);
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_root_rect(source.id()).map(|rect| (rect.x, rect.y)), Some((25, 20)));
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(combo.is_open(), Some(false));

    // Reopen the composed popup, then replace its owner's active popup. The displaced popup's
    // dismissal event must close Combo's semantic state in the same update transaction.
    context.mouseup(title_x + 15, title_y + 10, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);
    let moved_combo_rect = context.debug_root_node_rect(source.id(), combo_id).unwrap();
    context.mousedown(moved_combo_rect.x + 1, moved_combo_rect.y + 1, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(true));
    assert_eq!(combo.is_open(), Some(true));

    context.ui().show_popup(&replacement).unwrap();
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(context.debug_popup_visible(&replacement), Some(true));
    assert_eq!(combo.is_open(), Some(false));
}

#[test]
fn textbox_handle_event_dispatches_a_complete_snapshot_to_state() {
    #[derive(Default)]
    struct Model {
        changes: Vec<(String, usize)>,
    }

    impl Model {
        fn changed(&mut self, event: &TextboxChanged) {
            self.changes.push((event.text.clone(), event.cursor));
        }
    }

    let (widget, node) = Textbox::create(TextboxParameters::new(""));
    let changed = widget.changed();
    let node_id = node.id();
    let mut ctx: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240));
    let root = ctx.ui().create_window(Window::new("textbox signal", rect(0, 0, 140, 100), node));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    let mut model = Model::default();
    ctx.update_ui_state(dimensions, &mut model);
    let textbox_rect = ctx.debug_root_node_rect(root.id(), node_id).unwrap();

    ctx.subscribe(changed, Model::changed).unwrap();
    ctx.mousedown(textbox_rect.x + 1, textbox_rect.y + 1, MouseButton::LEFT);
    ctx.text("é");
    ctx.update_ui_state(dimensions, &mut model);

    assert_eq!(model.changes, [(String::from("é"), "é".len())]);
}

#[test]
fn creation_returns_persistent_root_handle() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(20, 30, 120, 90), empty_content()));

    assert!(root.is_alive());
    assert_eq!(ctx.debug_root_name(root.id()), Some("window".to_owned()));
    assert_eq!(
        ctx.debug_root_rect(root.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((20, 30, 120, 90))
    );
    assert_eq!(ctx.debug_root_visible(root.id()), Some(true));
    assert_eq!(ctx.debug_root_node_count(root.id()), Some(1));
}

#[test]
fn one_child_scroll_area_retains_its_three_structural_children() {
    let child = Node::widget(Custom::create(CustomParameters::new("content")));
    let (_, content) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("scroll", rect(0, 0, 100, 80), content));

    // The application child lives below a scroll surface, beside two real scrollbar widgets.
    assert_eq!(ctx.debug_root_node_count(root.id()), Some(5));
}

#[test]
fn hide_and_show_preserve_root_and_descendant_state() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), content));

    ctx.ui().set_window_visible(&root, false).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(root.id()), Some(false));
    assert!(button.is_alive());

    ctx.ui().set_window_visible(&root, true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(root.id()), Some(true));
    assert!(button.is_alive());
}

#[test]
fn destroy_expires_handles_and_ids_are_never_reused() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), content));
    let destroyed_id = root.id();

    assert_eq!(ctx.ui().destroy_window(&root), Ok(()));
    assert_eq!(ctx.ui().destroy_window(&root), Err(SurfaceMutationError::UnknownWindow));
    assert_eq!(ctx.ui().bring_window_to_front(&root), Err(SurfaceMutationError::UnknownWindow));
    assert_eq!(ctx.ui().set_window_rect(&root, rect(1, 2, 3, 4)), Err(SurfaceMutationError::UnknownWindow));
    assert!(!root.is_alive());
    assert!(!button.is_alive());

    let replacement = ctx.ui().create_window(Window::new("second", rect(0, 0, 100, 80), empty_content()));
    assert_ne!(replacement.id(), destroyed_id);
}

#[test]
fn window_and_popup_capabilities_cannot_resolve_another_contexts_surfaces() {
    let mut first = context();
    let first_window = first.ui().create_window(Window::new("first window", rect(0, 0, 100, 80), empty_content()));
    let first_popup = first.ui().create_popup(&first_window, "first popup", empty_content()).unwrap();

    let mut second = context();
    let second_window = second.ui().create_window(Window::new("second window", rect(0, 0, 100, 80), empty_content()));
    let second_popup = second.ui().create_popup(&second_window, "second popup", empty_content()).unwrap();

    // Each manager begins its private counters at the same values, so these checks specifically
    // prove that a capability's concrete event allocation authenticates its originating Context;
    // comparing only the manager-local numeric ids would incorrectly accept both foreign handles.
    assert_eq!(
        second.ui().set_window_rect(&first_window, rect(1, 2, 3, 4)),
        Err(SurfaceMutationError::UnknownWindow)
    );
    assert_eq!(second.ui().show_popup(&first_popup), Err(SurfaceMutationError::UnknownPopup));

    // Rejected foreign mutations leave the local records fully usable, demonstrating that failed
    // authentication neither selects nor partially changes the numerically colliding surface.
    second.ui().set_window_rect(&second_window, rect(5, 6, 70, 60)).unwrap();
    second.ui().show_popup(&second_popup).unwrap();
    assert_eq!(
        second.debug_root_rect(second_window.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((5, 6, 70, 60))
    );
    assert_eq!(second.debug_popup_visible(&second_popup), Some(true));
}

#[test]
#[allow(clippy::result_large_err)] // The assertion exercises the ownership-preserving mutation result.
fn dynamic_container_root_changes_descendants_without_replacing_the_root() {
    let mut ctx = context();
    let (column, content) = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>()));
    let root = ctx.ui().create_window(Window::new("dynamic", rect(0, 0, 140, 100), content));
    let root_id = root.id();
    let (button, widget) = Button::create(ButtonParameters::new("new child"));

    let inserted = column.try_update(|column| column.push(widget)).unwrap();
    assert!(inserted.is_ok());
    ctx.update_and_render_ui();
    assert_eq!(root.id(), root_id);
    assert!(button.is_alive());
    assert_eq!(ctx.debug_root_node_count(root_id), Some(2));

    assert_eq!(column.try_update(|linear: &mut Linear| linear.remove_drop(0)), Some(Some(true)));
    assert!(!button.is_alive());
    assert!(root.is_alive());
}

#[test]
fn showing_a_popup_atomically_hides_and_dismisses_the_previous_one() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let first = ctx.ui().create_popup(&source, "first", empty_content()).unwrap();
    let second = ctx.ui().create_popup(&source, "second", empty_content()).unwrap();
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    fn record(events: &mut Vec<PopupEvent>, event: &PopupEvent) {
        events.push(*event);
    }
    dispatcher.subscribe(first.clone(), record).unwrap();
    let mut submissions = Vec::new();

    ctx.ui().show_popup_at(&first, rect(12, 18, 90, 1)).unwrap();
    assert_eq!(ctx.debug_popup_visible(&first), Some(true));
    ctx.ui().show_popup_at(&second, rect(40, 55, 120, 1)).unwrap();

    assert_eq!(ctx.debug_popup_visible(&first), Some(false));
    assert_eq!(ctx.debug_popup_visible(&second), Some(true));
    assert_eq!(
        ctx.debug_popup_rect(&second).map(|anchor| (anchor.x, anchor.y, anchor.width, anchor.height)),
        Some((40, 55, 120, 1))
    );
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed]);
}

#[test]
fn hiding_a_window_hides_its_active_popup() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let first = ctx.ui().create_popup(&source, "first", empty_content()).unwrap();
    let second = ctx.ui().create_popup(&source, "second", empty_content()).unwrap();
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    fn record(events: &mut Vec<PopupEvent>, event: &PopupEvent) {
        events.push(*event);
    }
    dispatcher.subscribe(second.clone(), record).unwrap();
    let mut submissions = Vec::new();

    ctx.ui().show_popup(&first).unwrap();
    ctx.ui().hide_popup(&first).unwrap();
    ctx.ui().show_popup(&second).unwrap();
    ctx.ui().set_window_visible(&source, false).unwrap();

    assert_eq!(ctx.debug_popup_visible(&first), Some(false));
    assert_eq!(ctx.debug_popup_visible(&second), Some(false));
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed]);
}

#[test]
fn stale_popup_mutations_fail_after_owner_destruction() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();
    assert_eq!(ctx.ui().destroy_window(&source), Ok(()));

    // Popup definitions have no independent destruction operation. Destroying the owning window
    // expires its handles and makes every typed mutation fail consistently.
    assert_eq!(ctx.ui().show_popup_at(&popup, rect(20, 30, 40, 1)), Err(SurfaceMutationError::UnknownPopup));
    assert_eq!(ctx.ui().set_popup_options(&popup, WindowOption::FRAME), Err(SurfaceMutationError::UnknownPopup));
    assert_eq!(ctx.ui().hide_popup(&popup), Err(SurfaceMutationError::UnknownPopup));
    assert!(!popup.is_alive());
}

#[test]
fn outside_popup_press_hides_and_records_typed_submission() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();
    ctx.ui()
        .set_popup_options(&popup, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().show_popup_at(&popup, rect(20, 20, 80, 60)).unwrap();
    let mut widget_event_dispatcher = crate::event::WidgetEventDispatcher::new();
    fn record(events: &mut Vec<PopupEvent>, event: &PopupEvent) {
        events.push(*event);
    }
    widget_event_dispatcher.subscribe(popup.clone(), record).unwrap();
    let mut submissions = Vec::new();
    ctx.update_and_render_ui();

    ctx.mousedown(200, 180, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert!(widget_event_dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed]);
    ctx.ui().show_popup(&popup).unwrap();
    ctx.ui().hide_popup(&popup).unwrap();
    assert!(widget_event_dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed, PopupEvent::Dismissed]);
}

#[test]
fn outside_popup_press_dismisses_then_routes_once_to_the_revealed_root() {
    let mut ctx = context();
    let (button, content) = button_content("behind");
    let mut dispatcher = event_counter(button);
    let mut submissions = 0;
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 180, 120), content));
    ctx.ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let popup = ctx.ui().create_popup(&window, "popup", empty_content()).unwrap();
    ctx.ui()
        .set_popup_options(&popup, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().show_popup_at(&popup, rect(80, 60, 60, 40)).unwrap();
    ctx.update_ui(Dimensioni::new(320, 240));

    ctx.mousedown(15, 15, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, 1);
}

#[test]
fn layout_only_update_and_paint_have_separate_phase_counts() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));

    ctx.update_and_render_ui();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 1);
    assert_eq!(metrics.updates, 0);
    assert_eq!(metrics.paints, 1);
}

#[test]
fn warmed_container_measurement_and_layout_allocate_nothing() {
    let child = |name| Node::widget(Custom::create(CustomParameters::new(name)));
    let (_, row) = Linear::create(LinearParameters::horizontal([LinearItem::flex(child("row"), 1.0)]));
    let (_, grid) = Grid::create(GridParameters::new([TrackSize::Flex(1.0)], [TrackSize::Content], [child("grid")]));
    let (_, fixed_column) = Linear::create(LinearParameters::vertical([LinearItem::fixed(child("column"), 20)]));
    let (_, disclosure) = Disclosure::create(DisclosureParameters::header("expanded", true, [child("disclosure")]));
    let (_, scroll) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child("scroll")));
    let (_, content) = Linear::create(LinearParameters::vertical([row, grid, fixed_column, disclosure, scroll]));
    let mut ctx = context();
    ctx.ui().create_window(Window::new("allocation probe", rect(10, 10, 300, 220), content));
    let dimensions = Dimensioni::new(640, 480);

    ctx.update_ui(dimensions);
    ctx.update_ui(dimensions);
    let measurement = AllocationMeasurement::begin();
    ctx.update_ui(dimensions);
    let allocations = measurement.finish();

    assert_eq!(allocations.events, 0, "steady measurement/layout allocated {} bytes", allocations.bytes);
}

#[test]
fn warmed_recursive_menu_layout_reuses_every_slot_and_path_allocation() {
    let (_, recent_item) = MenuItem::create(MenuItemParameters::new("Recent project"));
    let menu_bar = MenuBar::new([Menu::new("File").submenu(Menu::new("Recent").item(recent_item))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("allocation menu", rect(10, 10, 300, 220), empty_content()).menu_bar(menu_bar));
    let dimensions = Dimensioni::new(640, 480);

    // Open both levels once so bar slots, popup slots, visible order, and popup-path scratch all
    // reach their steady capacities before the allocation counter begins.
    ctx.update_ui(dimensions);
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    let submenu = ctx.debug_active_menu_row_rects()[0][0];
    click_rect(&mut ctx, submenu);
    assert_eq!(ctx.debug_active_menu_row_rects().len(), 2);
    ctx.update_ui(dimensions);

    let measurement = AllocationMeasurement::begin();
    ctx.update_ui(dimensions);
    let allocations = measurement.finish();

    assert_eq!(allocations.events, 0, "steady menu layout allocated {} bytes", allocations.bytes);
}

#[test]
fn fronting_changes_only_cross_root_z_order() {
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()));
    let _second = ctx.ui().create_window(Window::new("second", rect(20, 20, 100, 80), empty_content()));
    assert_eq!(ctx.debug_rendered_root_names(), ["first", "second"]);

    ctx.ui().bring_window_to_front(&first).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["second", "first"]);
}

#[test]
fn fixed_layers_validate_and_managed_roots_reject_direct_assignment() {
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()));
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(20, 20, 100, 80), empty_content()))
        .unwrap();

    assert_eq!(ctx.ui().window_layer(&window), Ok(LayerBinding::Fixed(DEFAULT_LAYER)));
    assert_eq!(ctx.ui().window_layer(&dialog), Ok(LayerBinding::Modal));
    assert_eq!(
        ctx.ui().set_window_layer(&window, MAX_LAYER + 1),
        Err(SurfaceMutationError::InvalidLayer(MAX_LAYER + 1))
    );
    assert_eq!(ctx.ui().set_window_layer(&dialog, 3), Err(SurfaceMutationError::ManagedLayer));

    ctx.ui().set_window_layer(&window, MIN_LAYER).unwrap();
    assert_eq!(ctx.ui().window_layer(&window), Ok(LayerBinding::Fixed(MIN_LAYER)));
}

#[test]
fn raising_reorders_only_inside_a_fixed_layer() {
    let mut ctx = context();
    let _high = ctx.ui().create_window(Window::new("high", rect(0, 0, 100, 80), empty_content()));
    let low_first = ctx.ui().create_window(Window::new("low first", rect(0, 0, 100, 80), empty_content()));
    let low_second = ctx.ui().create_window(Window::new("low second", rect(0, 0, 100, 80), empty_content()));
    ctx.ui().set_window_layer(&low_first, 2).unwrap();
    ctx.ui().set_window_layer(&low_second, 2).unwrap();

    assert_eq!(ctx.debug_rendered_root_names(), ["low first", "low second", "high"]);
    ctx.ui().bring_window_to_front(&low_first).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["low second", "low first", "high"]);
}

#[test]
fn popup_inherits_its_parent_layer_and_uses_only_that_layers_transient_tier() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let same_layer = ctx.ui().create_window(Window::new("same layer", rect(0, 0, 100, 80), empty_content()));
    let higher = ctx.ui().create_window(Window::new("higher", rect(0, 0, 100, 80), empty_content()));
    ctx.ui().set_window_layer(&source, 2).unwrap();
    ctx.ui().set_window_layer(&same_layer, 2).unwrap();
    ctx.ui().set_window_layer(&higher, 3).unwrap();
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();

    ctx.ui().show_popup_at(&popup, rect(10, 10, 60, 40)).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["source", "same layer", "popup", "higher"]);
    assert_eq!(ctx.debug_active_popup_names(), ["popup"]);
    ctx.update_and_render_ui();

    // Window ownership remains live while the popup is visible: moving the owner moves its active
    // popup to that window's transient tier without giving it an independent layer.
    ctx.ui().set_window_layer(&source, 4).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["same layer", "higher", "source", "popup"]);
    assert_eq!(ctx.debug_active_popup_names(), ["popup"]);
}

#[test]
fn higher_layer_window_occludes_lower_popup_for_outside_dismissal() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let higher = ctx.ui().create_window(Window::new("higher", rect(0, 0, 100, 80), empty_content()));
    let chromeless = WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE;
    ctx.ui().set_window_options(&source, chromeless).unwrap();
    ctx.ui().set_window_options(&higher, chromeless).unwrap();
    ctx.ui().set_window_layer(&source, 2).unwrap();
    ctx.ui().set_window_layer(&higher, 3).unwrap();
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();
    ctx.ui().set_popup_options(&popup, chromeless).unwrap();
    ctx.ui().show_popup_at(&popup, rect(10, 10, 60, 40)).unwrap();
    ctx.update_and_render_ui();

    // The point lies inside the popup rectangle, but the higher-layer window is the visible input
    // target there. Treating raw popup bounds as visibility would leave an occluded menu open.
    ctx.mousedown(20, 20, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert_eq!(ctx.debug_active_root(), Some(higher.id()));
}

#[test]
fn owner_destruction_expires_all_popup_handles() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();

    // Popup definitions remain alive while hidden and expire only with their owning window.
    ctx.ui().show_popup(&popup).unwrap();
    ctx.ui().hide_popup(&popup).unwrap();
    assert!(popup.is_alive());

    assert_eq!(ctx.ui().destroy_window(&source), Ok(()));
    assert!(!popup.is_alive());
}

#[test]
fn active_root_routes_keyboard_without_crossing_layer_boundaries() {
    let (low_state, low_content) = OrderedProbe::create(WidgetOption::HOLD_FOCUS);
    let mut ctx = context();
    let low = ctx.ui().create_window(Window::new("low", rect(0, 0, 100, 80), low_content));
    let _high = ctx.ui().create_window(Window::new("high", rect(160, 120, 100, 80), empty_content()));
    ctx.ui().set_window_layer(&low, 0).unwrap();
    ctx.update_and_render_ui();
    let low_body = ctx.debug_root_body(low.id()).unwrap();

    ctx.mousedown(low_body.x + 1, low_body.y + 1, MouseButton::LEFT);
    ctx.mouseup(low_body.x + 1, low_body.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(low.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["low", "high"]);

    // The higher-layer window remains visually frontmost, but keyboard input belongs to the root
    // explicitly activated by the completed pointer press.
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("low layer");
    ctx.update_and_render_ui();
    assert_eq!(low_state.try_read(|state| state.events.clone()), Some(vec!["down", "up", "key-down", "text"]));
}

#[test]
fn blank_root_press_confines_drag_to_the_pressed_root() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()));
    let second = ctx.ui().create_window(Window::new("second", rect(160, 120, 100, 80), probe));
    for window in [&first, &second] {
        ctx.ui()
            .set_window_options(window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
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
fn scroll_and_new_press_reach_a_hovered_lower_layer_independently_of_activation() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::GRAB_SCROLL);
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()));
    let second = ctx.ui().create_window(Window::new("second", rect(160, 120, 100, 80), probe));
    for window in [&first, &second] {
        ctx.ui()
            .set_window_options(window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
            .unwrap();
    }
    ctx.ui().set_window_layer(&second, MIN_LAYER).unwrap();
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
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["move", "scroll"]));

    ctx.mousedown(second_point.x, second_point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["move", "scroll", "down"]));
}

#[test]
fn pointer_captured_root_remains_the_keyboard_and_text_input_root() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), probe));
    let second = ctx.ui().create_window(Window::new("second", rect(160, 120, 100, 80), empty_content()));
    ctx.update_and_render_ui();

    let first_body = ctx.debug_root_body(first.id()).unwrap();
    ctx.mousedown(first_body.x + 1, first_body.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(first.id()), Some(true));

    ctx.ui().bring_window_to_front(&second).unwrap();
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("captured");
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "key-down", "text"]));
}

#[test]
fn visible_dialog_is_the_sole_pointer_root_and_remains_frontmost() {
    let mut ctx = context();
    let (behind_button, behind_content) = button_content("behind");
    let mut behind_dispatcher = event_counter(behind_button);
    let mut behind_submissions = 0;
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), behind_content));
    ctx.ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let (dialog_button, dialog_content) = button_content("dialog");
    let mut dialog_dispatcher = event_counter(dialog_button);
    let mut dialog_submissions = 0;
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(120, 100, 100, 80), dialog_content))
        .unwrap();
    ctx.ui()
        .set_window_options(&dialog, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().set_window_visible(&dialog, true).unwrap();
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_modal_root(), Some(dialog.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog"]);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(!behind_dispatcher.dispatch(&mut behind_submissions));

    ctx.mousedown(130, 110, MouseButton::LEFT);
    ctx.mouseup(130, 110, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(dialog_dispatcher.dispatch(&mut dialog_submissions));
    assert_eq!(dialog_submissions, 1);

    ctx.ui().bring_window_to_front(&window).unwrap();
    ctx.ui().set_window_visible(&window, true).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog"]);
    // Numeric z-indices are local ordering values; the modal band remains structurally above the
    // fixed band even after the ordinary window receives the newer number.
    assert_eq!(ctx.debug_modal_root(), Some(dialog.id()));

    ctx.ui().set_window_visible(&dialog, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), None);
    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(behind_dispatcher.dispatch(&mut behind_submissions));
    assert_eq!(behind_submissions, 1);
}

#[test]
fn active_dialog_accepts_only_its_own_popup_in_the_modal_input_group() {
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()));
    let window_popup = ctx.ui().create_popup(&window, "window popup", empty_content()).unwrap();
    let (popup_button, popup_content) = button_content("popup");
    let mut dispatcher = event_counter(popup_button);
    let mut submissions = 0;
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(120, 100, 100, 80), empty_content()))
        .unwrap();
    let popup = ctx.ui().create_popup(&dialog, "popup", popup_content).unwrap();
    ctx.ui()
        .set_popup_options(&popup, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui()
        .set_window_options(&dialog, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().set_window_visible(&dialog, true).unwrap();
    ctx.mousemove(10, 10);
    ctx.update_and_render_ui();

    assert_eq!(ctx.ui().show_popup(&window_popup), Err(SurfaceMutationError::InvalidPopupParent));
    ctx.ui().show_popup_at(&popup, rect(0, 0, 100, 80)).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog", "popup"]);
    assert_eq!(ctx.debug_active_popup_names(), ["popup"]);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, 1);

    ctx.ui().set_window_visible(&dialog, false).unwrap();
    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
}

#[test]
fn modal_activation_clears_underlying_focus_and_blocks_keyboard_input() {
    let (state, probe) = OrderedProbe::create(WidgetOption::HOLD_FOCUS);
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), probe));
    ctx.ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(120, 100, 100, 80), empty_content()))
        .unwrap();
    ctx.ui()
        .set_window_options(&dialog, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.ui().set_window_visible(&dialog, true).unwrap();
    let updates_before_modal_input = state.try_read(|state| state.updates).unwrap();
    ctx.keydown(KeyMode::SHIFT);
    ctx.text("blocked");
    ctx.keyup(KeyMode::SHIFT);
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.updates), Some(updates_before_modal_input));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.ui().set_window_visible(&dialog, false).unwrap();
    ctx.text("still unfocused");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.text("accepted");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up", "down", "text"]));
}

#[test]
fn modal_routing_revokes_underlying_chrome_capture_before_drag_continues() {
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(170, 120, 100, 80), empty_content()))
        .unwrap();
    ctx.update_and_render_ui();
    let title = ctx.debug_root_chrome(window.id()).unwrap().0.unwrap();

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(window.id()), Some(true));
    assert_eq!(ctx.debug_root_moving(window.id()), Some(true));
    let before = ctx.debug_root_rect(window.id()).unwrap();

    ctx.ui().set_window_visible(&dialog, true).unwrap();
    ctx.mousemove(title.x + 20, title.y + 20);
    ctx.mouseup(title.x + 20, title.y + 20, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(window.id()), Some(false));
    assert_eq!(ctx.debug_root_active(window.id()), Some(false));
    assert_eq!(
        ctx.debug_root_rect(window.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((before.x, before.y, before.width, before.height))
    );
}

#[test]
fn hiding_or_destroying_the_front_dialog_reveals_the_next_visible_dialog() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(0, 0, 10, 10), empty_content()));
    let first = ctx
        .ui()
        .create_dialog(&owner, Window::new("first", rect(20, 20, 120, 90), empty_content()))
        .unwrap();
    let second = ctx
        .ui()
        .create_dialog(&owner, Window::new("second", rect(40, 40, 120, 90), empty_content()))
        .unwrap();

    ctx.ui().set_window_visible(&first, true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    ctx.ui().set_window_visible(&second, true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));

    ctx.ui().set_window_visible(&second, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    ctx.ui().set_window_visible(&second, true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));

    ctx.update_and_render_ui();
    let close = ctx.debug_root_chrome(second.id()).unwrap().1.unwrap();
    ctx.mousedown(close.x + close.width / 2, close.y + close.height / 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(second.id()), Some(false));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.ui().set_window_visible(&second, true).unwrap();
    assert_eq!(ctx.ui().destroy_window(&second), Ok(()));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert_eq!(ctx.ui().destroy_window(&first), Ok(()));
    assert_eq!(ctx.debug_modal_root(), None);
}

#[test]
fn fronting_a_visible_dialog_makes_it_the_active_modal() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(0, 0, 10, 10), empty_content()));
    let first = ctx
        .ui()
        .create_dialog(&owner, Window::new("first", rect(20, 20, 120, 90), empty_content()))
        .unwrap();
    let middle = ctx
        .ui()
        .create_dialog(&owner, Window::new("middle", rect(30, 30, 120, 90), empty_content()))
        .unwrap();
    let second = ctx
        .ui()
        .create_dialog(&owner, Window::new("second", rect(40, 40, 120, 90), empty_content()))
        .unwrap();
    ctx.ui().set_window_visible(&first, true).unwrap();
    ctx.ui().set_window_visible(&middle, true).unwrap();
    ctx.ui().set_window_visible(&second, true).unwrap();

    ctx.ui().bring_window_to_front(&first).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["owner", "middle", "second", "first"]);
    // Raising the ordinary owner moves only its fixed-band subtree and leaves modal sibling order
    // untouched.
    ctx.ui().bring_window_to_front(&owner).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.ui().set_window_visible(&first, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));
    ctx.ui().set_window_visible(&second, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(middle.id()));
    ctx.ui().set_window_visible(&second, true).unwrap();

    ctx.ui().bring_window_to_front(&second).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));
}

#[test]
fn fronting_a_dialog_closes_the_previous_modal_groups_popup() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(0, 0, 10, 10), empty_content()));
    let first = ctx
        .ui()
        .create_dialog(&owner, Window::new("first", rect(20, 20, 120, 90), empty_content()))
        .unwrap();
    let second = ctx
        .ui()
        .create_dialog(&owner, Window::new("second", rect(40, 40, 120, 90), empty_content()))
        .unwrap();
    ctx.ui().set_window_visible(&first, true).unwrap();
    ctx.ui().set_window_visible(&second, true).unwrap();
    let popup = ctx.ui().create_popup(&second, "popup", empty_content()).unwrap();
    ctx.ui().show_popup_at(&popup, rect(50, 50, 40, 30)).unwrap();

    // Switching dialog groups dismisses the transient that belonged to the previously active group
    // before changing modal z-order.
    ctx.ui().bring_window_to_front(&first).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
}

#[test]
fn title_drag_and_close_record_typed_window_events() {
    /// Preserves the exact order in which one window's combined event stream is dispatched.
    fn record(events: &mut Vec<WindowEvent>, event: &WindowEvent) {
        // Copy the concrete event value so the assertions cover both variants without an erased or
        // auxiliary test-only representation.
        events.push(*event);
    }

    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    dispatcher.subscribe(root.clone(), record).unwrap();
    let mut events = Vec::new();
    ctx.update_and_render_ui();
    let (title, _, _) = ctx.debug_root_chrome(root.id()).unwrap();
    let title = title.unwrap();
    let drag_x = title.x + 2;
    let drag_y = title.y + 2;

    ctx.mousedown(drag_x, drag_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_moving(root.id()), Some(true));
    assert!(!dispatcher.dispatch(&mut events));

    ctx.mousemove(drag_x + 10, drag_y + 8);
    ctx.update_and_render_ui();
    assert!(dispatcher.dispatch(&mut events));
    assert!(matches!(
        events.as_slice(),
        [WindowEvent::GeometryChanged { rect }]
            if (rect.x, rect.y, rect.width, rect.height) == (40, 38, 140, 100)
    ));

    ctx.mouseup(drag_x + 10, drag_y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_active(root.id()), Some(false));

    let close = ctx.debug_root_chrome(root.id()).unwrap().1.unwrap();
    let close_x = close.x + close.width / 2;
    let close_y = close.y + close.height / 2;
    ctx.mousedown(close_x, close_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(root.id()), Some(false));
    assert!(dispatcher.dispatch(&mut events));
    assert!(matches!(
        events.as_slice(),
        [WindowEvent::GeometryChanged { rect }, WindowEvent::CloseRequested]
            if (rect.x, rect.y, rect.width, rect.height) == (40, 38, 140, 100)
    ));
}

#[test]
fn resize_overlay_preempts_content_where_the_grip_overlaps_the_root_body() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), probe));
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

    let before = ctx.debug_root_rect(root.id()).unwrap();
    ctx.mousedown(press.x, press.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_resizing(root.id()), Some(true));
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    ctx.mousemove(press.x + 12, press.y + 8);
    ctx.mouseup(press.x + 12, press.y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(
        ctx.debug_root_rect(root.id()).map(|rect| (rect.width, rect.height)),
        Some((before.width + 12, before.height + 8))
    );
}

#[test]
fn content_capture_remains_exclusive_while_dragging_across_root_chrome() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), probe));
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
    assert_eq!(ctx.debug_root_resizing(root.id()), Some(false));

    ctx.mouseup(over_chrome.x, over_chrome.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "drag", "up"]));
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
}

#[test]
fn hiding_and_showing_root_does_not_restore_chrome_capture() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
    ctx.update_and_render_ui();
    let title = ctx.debug_root_chrome(root.id()).unwrap().0.unwrap();

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(true));
    assert_eq!(ctx.debug_root_moving(root.id()), Some(true));

    ctx.ui().set_window_visible(&root, false).unwrap();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(ctx.debug_root_active(root.id()), Some(false));

    ctx.ui().set_window_visible(&root, true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(ctx.debug_root_active(root.id()), Some(false));
}

/// Verifies that menu placement remains a live surface-slot relationship after owner movement.
#[test]
fn declarative_menu_popups_follow_heading_and_submenu_edges_when_the_window_moves() {
    // A nested File menu exercises both relationship variants. Edit supplies another top-level
    // heading, ensuring the helper's declaration order matches the compiled popup order.
    let (_, open_item) = MenuItem::create(MenuItemParameters::new("Open"));
    let (_, recent_item) = MenuItem::create(MenuItemParameters::new("Recent Project"));
    let (_, copy_item) = MenuItem::create(MenuItemParameters::new("Copy"));
    let menu_bar = MenuBar::new([
        Menu::new("File").item(open_item).submenu(Menu::new("Recent").item(recent_item)),
        Menu::new("Edit").item(copy_item),
    ]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("menu geometry", rect(30, 25, 190, 140), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // The first compiled relationship belongs to File. Opening it must use the current heading
    // slot's exact left and bottom edges, not a caller-computed or creation-time screen rectangle.
    let anchors = ctx.debug_menu_anchor_rects(root.id()).unwrap();
    assert_eq!(anchors.len(), 3);
    let file_heading = anchors[0].expect("the bar surface's File slot must have committed geometry");
    click_rect(&mut ctx, file_heading);
    assert_eq!(ctx.debug_active_popup_names(), ["menu geometry File Menu"]);
    let file_popup = ctx.debug_active_popup_rects()[0];
    assert_eq!(file_popup.x, file_heading.x);
    assert_eq!(file_popup.y, file_heading.y + file_heading.height);

    // The parent popup surface commits the Recent row slot. Opening that row must place the child
    // at its exact right edge and preserve its y coordinate.
    let recent_row = ctx.debug_menu_anchor_rects(root.id()).unwrap()[1].expect("an active parent surface must lay out its submenu slot");
    click_rect(&mut ctx, recent_row);
    assert_eq!(ctx.debug_active_popup_names(), ["menu geometry File Menu", "menu geometry Recent Menu"]);
    let open_popups = ctx.debug_active_popup_rects();
    assert_eq!(open_popups[1].x, recent_row.x + recent_row.width);
    assert_eq!(open_popups[1].y, recent_row.y);

    // Move the owner programmatically while both levels remain open. A fresh layout must translate
    // headings, submenu rows, and both popup surfaces by the same delta while preserving the two
    // edge equations above.
    let delta = crate::vec2(37, 29);
    let original_window = ctx.debug_root_rect(root.id()).unwrap();
    ctx.ui()
        .set_window_rect(
            &root,
            Recti::new(
                original_window.x + delta.x,
                original_window.y + delta.y,
                original_window.width,
                original_window.height,
            ),
        )
        .unwrap();
    ctx.update_and_render_ui();

    let moved_anchors = ctx.debug_menu_anchor_rects(root.id()).unwrap();
    let moved_file_heading = moved_anchors[0].unwrap();
    let moved_recent_row = moved_anchors[1].unwrap();
    let moved_popups = ctx.debug_active_popup_rects();
    assert_eq!(
        (moved_file_heading.x, moved_file_heading.y),
        (file_heading.x + delta.x, file_heading.y + delta.y)
    );
    assert_eq!((moved_recent_row.x, moved_recent_row.y), (recent_row.x + delta.x, recent_row.y + delta.y));
    assert_eq!((moved_popups[0].x, moved_popups[0].y), (file_popup.x + delta.x, file_popup.y + delta.y));
    assert_eq!((moved_popups[1].x, moved_popups[1].y), (open_popups[1].x + delta.x, open_popups[1].y + delta.y));
    assert_eq!(moved_popups[0].x, moved_file_heading.x);
    assert_eq!(moved_popups[0].y, moved_file_heading.y + moved_file_heading.height);
    assert_eq!(moved_popups[1].x, moved_recent_row.x + moved_recent_row.width);
    assert_eq!(moved_popups[1].y, moved_recent_row.y);
}

/// Verifies that live marker-role changes remeasure only their owning menu surface.
#[test]
fn changing_item_marker_role_reflows_parent_and_reanchors_open_child() {
    // Keep the mutable item in the parent while the child owns an unrelated fixed-size item. The
    // application retains only the weak handle needed to change the parent's shared marker gutter.
    let (item, direct_item) = MenuItem::create(MenuItemParameters::new("Direct action"));
    let (_, child_item) = MenuItem::create(MenuItemParameters::new("Child action"));
    let menu_bar = MenuBar::new([Menu::new("File").item(direct_item).submenu(Menu::new("Child").item(child_item))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("live marker geometry", rect(24, 20, 220, 150), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Open both levels and retain their unmarked baseline. The child begins exactly at the parent
    // menu surface's right edge inside its one-pixel popup frame, which distinguishes resizing
    // from stale anchoring without conflating content geometry with window chrome.
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    let child_trigger = ctx.debug_active_menu_row_rects()[0][1];
    click_rect(&mut ctx, child_trigger);
    assert_eq!(
        ctx.debug_active_popup_names(),
        ["live marker geometry File Menu", "live marker geometry Child Menu"]
    );
    let baseline_popups = ctx.debug_active_popup_rects();
    let baseline_rows = ctx.debug_active_menu_row_rects();
    // Recti intentionally has no equality implementation. Compare compact scalar snapshots when a
    // phase requires exact geometry rather than one relational edge assertion.
    let popup_geometry = |rects: &[Recti]| rects.iter().map(|rect| (rect.x, rect.y, rect.width, rect.height)).collect::<Vec<_>>();
    let row_geometry = |surfaces: &[Vec<Recti>]| {
        surfaces
            .iter()
            .map(|rows| rows.iter().map(|rect| (rect.x, rect.y, rect.width, rect.height)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    assert_eq!(baseline_popups[1].x, baseline_rows[0][1].x + baseline_rows[0][1].width);
    assert_eq!(baseline_popups[1].y, baseline_rows[0][1].y);

    // Introducing an unchecked role must still allocate the shared marker gutter. A normal update
    // remeasures the parent, widens every parent row, and repositions the already-open child from
    // the freshly committed submenu slot without altering the child's own intrinsic dimensions.
    ctx.ui().menu_item_mut(&item).unwrap().mark = MenuItemMark::Checked(false);
    ctx.update_and_render_ui();
    let unchecked_popups = ctx.debug_active_popup_rects();
    let unchecked_rows = ctx.debug_active_menu_row_rects();
    assert!(unchecked_popups[0].width > baseline_popups[0].width);
    assert_eq!(unchecked_popups[0].height, baseline_popups[0].height);
    assert!(unchecked_rows[0][0].width > baseline_rows[0][0].width);
    assert!(unchecked_rows[0][1].width > baseline_rows[0][1].width);
    assert_eq!(unchecked_popups[1].x, unchecked_rows[0][1].x + unchecked_rows[0][1].width);
    assert_eq!(unchecked_popups[1].y, unchecked_rows[0][1].y);
    assert_eq!(
        (unchecked_popups[1].width, unchecked_popups[1].height),
        (baseline_popups[1].width, baseline_popups[1].height)
    );

    // Toggling only the boolean paints a check but preserves its role, so no geometry on either
    // surface may move. This guards against measuring marker visibility instead of marker role.
    ctx.ui().menu_item_mut(&item).unwrap().mark = MenuItemMark::Checked(true);
    ctx.update_and_render_ui();
    assert_eq!(popup_geometry(&ctx.debug_active_popup_rects()), popup_geometry(&unchecked_popups));
    assert_eq!(row_geometry(&ctx.debug_active_menu_row_rects()), row_geometry(&unchecked_rows));

    // Removing the role releases the gutter. The parent and child must return to their exact
    // baseline rectangles while the open path remains intact and the child keeps its own size.
    ctx.ui().menu_item_mut(&item).unwrap().mark = MenuItemMark::None;
    ctx.update_and_render_ui();
    assert_eq!(popup_geometry(&ctx.debug_active_popup_rects()), popup_geometry(&baseline_popups));
    assert_eq!(row_geometry(&ctx.debug_active_menu_row_rects()), row_geometry(&baseline_rows));
    assert_eq!(
        ctx.debug_active_popup_names(),
        ["live marker geometry File Menu", "live marker geometry Child Menu"]
    );
}

/// Verifies that a heading toggles its own menu and switches directly to a sibling heading.
#[test]
fn menu_heading_clicks_toggle_and_switch_the_single_active_popup_path() {
    // Each top-level menu owns one concrete item so both popup surfaces have nonzero geometry.
    let (_, new_item) = MenuItem::create(MenuItemParameters::new("New"));
    let (_, undo_item) = MenuItem::create(MenuItemParameters::new("Undo"));
    let menu_bar = MenuBar::new([Menu::new("File").item(new_item), Menu::new("Edit").item(undo_item)]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("menu switching", rect(15, 20, 180, 120), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();
    let anchors = ctx.debug_menu_anchor_rects(root.id()).unwrap();
    let file_heading = anchors[0].unwrap();
    let edit_heading = anchors[1].unwrap();

    // Repeating the same heading press closes the existing top-level path instead of reopening it
    // after outside-dismissal policy runs for that press.
    click_rect(&mut ctx, file_heading);
    assert_eq!(ctx.debug_active_popup_names(), ["menu switching File Menu"]);
    click_rect(&mut ctx, file_heading);
    assert!(ctx.debug_active_popup_names().is_empty());

    // A different heading replaces the path in one input transaction, and its popup uses that new
    // heading's exact below-edge relationship. It likewise toggles closed on repetition.
    click_rect(&mut ctx, file_heading);
    click_rect(&mut ctx, edit_heading);
    assert_eq!(ctx.debug_active_popup_names(), ["menu switching Edit Menu"]);
    let edit_popup = ctx.debug_active_popup_rects()[0];
    assert_eq!((edit_popup.x, edit_popup.y), (edit_heading.x, edit_heading.y + edit_heading.height));
    click_rect(&mut ctx, edit_heading);
    assert!(ctx.debug_active_popup_names().is_empty());
}

/// Verifies that choosing a sibling submenu removes every deeper popup from the old branch.
#[test]
fn opening_sibling_submenu_replaces_the_complete_descendant_suffix() {
    // Alpha owns a second descendant level so replacing it exercises more than the immediate child.
    // Concrete leaf items keep both terminal popup surfaces measurable without contributing row
    // nodes to the retained tree.
    let (_, alpha_leaf) = MenuItem::create(MenuItemParameters::new("Alpha leaf"));
    let (_, beta_leaf) = MenuItem::create(MenuItemParameters::new("Beta leaf"));
    let menu_bar = MenuBar::new([Menu::new("File")
        .submenu(Menu::new("Alpha").submenu(Menu::new("Deep").item(alpha_leaf)))
        .submenu(Menu::new("Beta").item(beta_leaf))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("submenu switching", rect(20, 20, 220, 150), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Open File, then follow Alpha into Deep. Row rectangles come from the active compact menu
    // surfaces in parent-first order, while each inner vector preserves declaration order.
    let file_heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, file_heading);
    let alpha_row = ctx.debug_active_menu_row_rects()[0][0];
    click_rect(&mut ctx, alpha_row);
    let deep_row = ctx.debug_active_menu_row_rects()[1][0];
    click_rect(&mut ctx, deep_row);
    assert_eq!(
        ctx.debug_active_popup_names(),
        ["submenu switching File Menu", "submenu switching Alpha Menu", "submenu switching Deep Menu",]
    );

    // Beta is a sibling of Alpha in the still-open File surface. Its press must replace both Alpha
    // and Deep atomically, leaving the common File prefix followed only by the new Beta branch.
    let beta_row = ctx.debug_active_menu_row_rects()[0][1];
    click_rect(&mut ctx, beta_row);
    assert_eq!(ctx.debug_active_popup_names(), ["submenu switching File Menu", "submenu switching Beta Menu"]);
    assert_eq!(ctx.debug_active_menu_row_rects().len(), 2);
}

/// Verifies that menu bar, submenu, and item presses preserve application keyboard focus.
#[test]
fn menu_pointer_operations_preserve_preexisting_application_keyboard_focus() {
    // HOLD_FOCUS gives the application body a persistent keyboard target. Every menu surface uses
    // PRESERVE_FOCUS, so its independent pointer capture must never replace this retained identity.
    let (probe, body) = OrderedProbe::create(WidgetOption::HOLD_FOCUS);
    let body_id = body.id();
    let (_, invoke_item) = MenuItem::create(MenuItemParameters::new("Invoke"));
    let menu_bar = MenuBar::new([Menu::new("Actions").submenu(Menu::new("More").item(invoke_item))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("menu focus", rect(20, 20, 220, 150), body).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Establish application focus before any menu becomes visible, then verify the bar press leaves
    // text routing on that body even while the top-level popup is active.
    let body_rect = ctx.debug_root_node_rect(root.id(), body_id).unwrap();
    click_rect(&mut ctx, body_rect);
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    ctx.text("after heading");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["menu focus Actions Menu"]);

    // Opening a nested popup exercises pointer capture on a popup-local MenuSurface. Keyboard input
    // must still bypass both menu surfaces and reach the original application focus owner.
    let submenu_row = ctx.debug_active_menu_row_rects()[0][0];
    click_rect(&mut ctx, submenu_row);
    ctx.text("after submenu");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["menu focus Actions Menu", "menu focus More Menu"]);

    // Invoking the leaf dismisses the complete popup path during MouseDown. Neither dismissal nor
    // the swallowed release tail may clear the application focus used by the following text event.
    let item_row = ctx.debug_active_menu_row_rects()[1][0];
    click_rect(&mut ctx, item_row);
    ctx.text("after item");
    ctx.update_and_render_ui();
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up", "text", "text", "text"]));
}

/// Verifies direct item events, dispatch ordering, and disabled-row menu policy together.
#[test]
fn menu_items_dispatch_directly_after_close_while_disabled_items_leave_the_menu_open() {
    /// Application state observed by the two concrete menu-item event subscriptions.
    #[derive(Default)]
    struct Model {
        /// Number of enabled-item submissions delivered by its native typed port.
        enabled_submissions: usize,
        /// Number of disabled-item submissions, expected to remain zero.
        disabled_submissions: usize,
        /// Whether enabled dispatch observed manager policy's already-closed popup path.
        enabled_observed_closed_menu: bool,
    }

    impl Model {
        /// Records the enabled item and inspects ordering through the safe event capability.
        fn enabled_submitted(&mut self, context: &mut Ui<'_>, _: &MenuItemSubmitted) {
            // The surface records an invocation and manager policy closes the path before the
            // item's typed event reaches application dispatch. The handler therefore sees no popup
            // without issuing a separate close command.
            self.enabled_submissions += 1;
            self.enabled_observed_closed_menu = context.debug_active_popup_names().is_empty();
        }

        /// Records any erroneous event emitted by the disabled concrete item.
        fn disabled_submitted(&mut self, _context: &mut Ui<'_>, _: &MenuItemSubmitted) {
            // The surface routes the row but resolves its disabled item to no invocation action.
            self.disabled_submissions += 1;
        }
    }

    // Each uniquely owned value moves into the compact menu data, while its handle remains with the
    // test so subscriptions can address the item's concrete event source after that move.
    let (enabled, enabled_item) = MenuItem::create(MenuItemParameters::new("Run"));
    let (disabled, disabled_item) = MenuItem::create(MenuItemParameters::new("Unavailable").disabled());
    let menu_bar = MenuBar::new([Menu::new("Actions").item(enabled_item).item(disabled_item)]);
    let dimensions = Dimensioni::new(320, 240);
    let mut ctx: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions);
    let root = ctx
        .ui()
        .create_window(Window::new("menu events", rect(20, 25, 180, 120), empty_content()).menu_bar(menu_bar));
    ctx.subscribe_context(enabled.clone(), Model::enabled_submitted).unwrap();
    ctx.subscribe_context(disabled.clone(), Model::disabled_submitted).unwrap();
    let mut model = Model::default();
    ctx.update_ui_state(dimensions, &mut model);

    // Open Actions and press its enabled item. The item's own port fires once, and the handler sees
    // the menu closed in the same update transaction.
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    let heading_point = crate::vec2(heading.x + heading.width / 2, heading.y + heading.height / 2);
    ctx.mousedown(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    // Rows no longer have RuntimeNodeIds. The test-only accessor exposes the same rectangles used
    // by the active popup surface for hit testing, indexed in declaration order.
    let enabled_rect = ctx.debug_active_menu_row_rects()[0][0];
    let enabled_point = crate::vec2(enabled_rect.x + enabled_rect.width / 2, enabled_rect.y + enabled_rect.height / 2);
    ctx.mousedown(enabled_point.x, enabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(enabled_point.x, enabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    assert_eq!(model.enabled_submissions, 1);
    assert!(model.enabled_observed_closed_menu);
    assert!(ctx.debug_active_popup_names().is_empty());

    // Reopen Actions and press the disabled row. The compact surface produces no invocation action,
    // so neither a typed event nor the manager's close policy runs and the popup remains active.
    ctx.mousedown(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    let disabled_rect = ctx.debug_active_menu_row_rects()[0][1];
    let disabled_point = crate::vec2(disabled_rect.x + disabled_rect.width / 2, disabled_rect.y + disabled_rect.height / 2);
    ctx.mousedown(disabled_point.x, disabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(disabled_point.x, disabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    assert_eq!(model.enabled_submissions, 1);
    assert_eq!(model.disabled_submissions, 0);
    assert_eq!(ctx.debug_active_popup_names(), ["menu events Actions Menu"]);
}

/// Verifies that closing a menu cannot redirect the remainder of its pointer gesture.
#[test]
fn menu_item_dismissal_swallows_the_invalidated_popup_capture_tail() {
    // Place an event-recording application surface directly beneath the menu popup. This makes a
    // stale drag or release observable instead of relying on a Button, which would usually ignore
    // an unmatched release and conceal the cross-surface routing error.
    let (body_state, body) = OrderedProbe::create(WidgetOption::NONE);
    let (_, invoke_item) = MenuItem::create(MenuItemParameters::new("Invoke"));
    let menu_bar = MenuBar::new([Menu::new("Actions").item(invoke_item)]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("capture tail", rect(20, 25, 220, 160), body).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Opening the popup completes the heading's independent captured gesture. Pressing its sole
    // enabled row then closes the popup during MouseDown, while that now-inactive surface still
    // owns the physical gesture that began the invocation.
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    let row = ctx.debug_active_menu_row_rects()[0][0];
    let point = crate::vec2(row.x + row.width / 2, row.y + row.height / 2);
    ctx.mousedown(point.x, point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(body_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    // Neither a drag nor the final release may fall through to the newly revealed body. The
    // suppression ends with that release, so the following fresh click must route normally.
    ctx.mousemove(point.x + 1, point.y + 1);
    ctx.update_and_render_ui();
    ctx.mouseup(point.x + 1, point.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(body_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    ctx.mousedown(point.x, point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    ctx.mouseup(point.x, point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(body_state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
}

/// Verifies that ordinary programmatic popups retain explicit screen-space placement semantics.
#[test]
fn generic_screen_anchored_popup_does_not_follow_its_moved_owner() {
    // This popup deliberately uses the public generic API rather than a declarative menu relation.
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(20, 30, 120, 90), empty_content()));
    let popup = ctx.ui().create_popup(&source, "screen popup", desired_size_node(70, 45)).unwrap();
    let explicit_anchor = rect(210, 35, 1, 1);
    ctx.ui().show_popup_at(&popup, explicit_anchor).unwrap();
    ctx.update_and_render_ui();
    let before_move = ctx.debug_popup_rect(&popup).unwrap();
    assert_eq!((before_move.x, before_move.y), (explicit_anchor.x, explicit_anchor.y));

    // Moving the owner changes stacking ownership only; the exact screen anchor and auto-sized
    // popup extent remain unchanged across the following layout.
    ctx.ui().set_window_rect(&source, rect(85, 95, 120, 90)).unwrap();
    ctx.update_and_render_ui();
    let after_move = ctx.debug_popup_rect(&popup).unwrap();
    assert_eq!(
        (after_move.x, after_move.y, after_move.width, after_move.height),
        (before_move.x, before_move.y, before_move.width, before_move.height)
    );
    assert_eq!(ctx.debug_active_popup_names(), ["screen popup"]);
}

#[test]
fn popup_auto_size_tracks_content() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(200, 160, 100, 70), empty_content()));
    let (_, text) = crate::TextBlock::create(crate::TextBlockParameters::new("window content"));
    let popup = ctx.ui().create_popup(&source, "popup", text).unwrap();
    ctx.ui().show_popup(&popup).unwrap();
    ctx.update_and_render_ui();

    let content = ctx.debug_popup_content_size(&popup).unwrap();
    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert!(content.width > 0 && content.height > 0);
    assert!(outer.width >= content.width && outer.height >= content.height);
}

#[test]
fn auto_sized_window_includes_menu_bar_and_exposes_application_body_below_it() {
    let (_, action) = MenuItem::create(MenuItemParameters::new("Action"));
    let menu_bar = MenuBar::new([Menu::new("File").item(action)]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("auto menu", rect(20, 30, 1, 1), desired_size_node(90, 24)).menu_bar(menu_bar));
    ctx.ui()
        .set_window_options(
            &root,
            WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE | WindowOption::AUTO_SIZE,
        )
        .unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_root_rect(root.id()).unwrap();
    let bar = ctx.debug_menu_bar_rect(root.id()).unwrap();
    let body = ctx.debug_root_body(root.id()).unwrap();
    assert!(bar.height > 0, "a non-empty menu must contribute intrinsic height");
    assert_eq!((bar.x, bar.width), (body.x, body.width), "the bar must fill the complete client width");
    assert_eq!(bar.y + bar.height, body.y, "application content must begin immediately below the bar");
    assert_eq!(body.height, 24, "auto height must retain the application's intrinsic extent below the bar");
    assert!(
        outer.height >= bar.height + body.height,
        "outer auto-size must include menu and application content"
    );
}

#[test]
fn live_but_unmounted_menu_item_is_unknown_to_ui() {
    let (handle, item) = MenuItem::create(MenuItemParameters::new("Unmounted"));
    let mut ctx = context();

    assert!(handle.is_alive(), "the declaration still owns its typed event source");
    assert!(matches!(ctx.ui().menu_item(&handle), Err(crate::MenuItemAccessError::UnknownItem)));
    assert!(matches!(ctx.ui().menu_item_mut(&handle), Err(crate::MenuItemAccessError::UnknownItem)));

    drop(item);
    assert!(!handle.is_alive(), "dropping the unmounted declaration expires its weak capability");
}

#[test]
fn menu_item_event_capability_cannot_resolve_another_contexts_record() {
    let (first_handle, first_item) = MenuItem::create(MenuItemParameters::new("First"));
    let (second_handle, second_item) = MenuItem::create(MenuItemParameters::new("Second"));
    let mut first = context();
    let mut second = context();
    first
        .ui()
        .create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(first_item)])));
    second
        .ui()
        .create_window(Window::new("second", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(second_item)])));

    // The weak event endpoint is both the subscription capability and the item identity. Matching
    // its allocation prevents equal presentation values from becoming cross-manager authority.
    assert_eq!(first.ui().menu_item(&first_handle).unwrap().label, "First");
    assert!(matches!(first.ui().menu_item(&second_handle), Err(crate::MenuItemAccessError::UnknownItem)));
    assert_eq!(second.ui().menu_item(&second_handle).unwrap().label, "Second");
    assert!(matches!(second.ui().menu_item(&first_handle), Err(crate::MenuItemAccessError::UnknownItem)));
}

#[test]
fn no_padding_option_makes_chromeless_root_content_edge_to_edge() {
    let mut ctx = context();
    let outer = rect(0, 0, 320, 240);
    let root = ctx.ui().create_window(Window::new("surface", outer, empty_content()));
    ctx.ui()
        .set_window_options(&root, WindowOption::NO_TITLE | WindowOption::NO_RESIZE | WindowOption::NO_PADDING)
        .unwrap();

    ctx.update_and_render_ui();

    let body = ctx.debug_root_body(root.id()).unwrap();
    assert_eq!((body.x, body.y, body.width, body.height), (outer.x, outer.y, outer.width, outer.height));
    let (title, close, resize) = ctx.debug_root_chrome(root.id()).unwrap();
    assert!(title.is_none() && close.is_none() && resize.is_none());
}

#[test]
fn auto_height_preserves_popup_width_and_stretches_column_items() {
    let mut item_ids = Vec::new();
    let items = ["Apple", "Banana", "Cherry", "Date"]
        .into_iter()
        .map(|label| {
            let (_, node) = ListItem::create(ListItemParameters::new(label));
            item_ids.push(node.id());
            node
        })
        .collect::<Vec<_>>();
    let (_, content) = Linear::create(LinearParameters::vertical(items));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "combo", content).unwrap();
    let anchor = rect(20, 30, 180, 1);
    ctx.ui()
        .set_popup_options(
            &popup,
            WindowOption::FRAME | WindowOption::AUTO_HEIGHT | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
        )
        .unwrap();
    ctx.ui().show_popup_at(&popup, anchor).unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert_eq!(outer.x, anchor.x);
    assert_eq!(outer.y, anchor.y);
    assert_eq!(outer.width, anchor.width, "AUTO_HEIGHT must retain the programmed width");
    assert!(outer.height > anchor.height, "popup height must still follow its items");
    let item_rects = item_ids
        .into_iter()
        .map(|item| ctx.debug_popup_node_rect(&popup, item).unwrap())
        .collect::<Vec<_>>();
    let first = item_rects[0];
    for item in item_rects {
        assert_eq!((item.x, item.width), (first.x, first.width));
    }
}

#[test]
fn auto_width_preserves_programmed_height() {
    let (_, item) = ListItem::create(ListItemParameters::new("intrinsic width"));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "horizontal", item).unwrap();
    let programmed = rect(20, 30, 1, 120);
    ctx.ui()
        .set_popup_options(
            &popup,
            WindowOption::FRAME | WindowOption::AUTO_WIDTH | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
        )
        .unwrap();
    ctx.ui().show_popup_at(&popup, programmed).unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert!(outer.width > programmed.width, "AUTO_WIDTH must derive width from content");
    assert_eq!(outer.height, programmed.height, "AUTO_WIDTH must retain the programmed height");
}

#[test]
fn auto_width_consumes_typed_measurement_invalidation_before_intrinsic_measurement() {
    let (text, content) = TextBlock::create(TextBlockParameters::new("x"));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "dynamic width", content).unwrap();
    let programmed = rect(20, 30, 1, 80);
    ctx.ui()
        .set_popup_options(
            &popup,
            WindowOption::FRAME | WindowOption::AUTO_WIDTH | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
        )
        .unwrap();
    ctx.ui().show_popup_at(&popup, programmed).unwrap();

    ctx.update_ui(Dimensioni::new(320, 240));
    let before = ctx.debug_popup_rect(&popup).unwrap();

    text.set_text("a substantially wider retained text block").unwrap();
    ctx.update_ui(Dimensioni::new(320, 240));
    let after = ctx.debug_popup_rect(&popup).unwrap();

    assert!(after.width > before.width, "auto-width measurement must observe the typed mutation");
    assert_eq!(after.height, programmed.height);
}

#[test]
fn auto_size_ignores_the_previous_rect_for_flexible_linear_and_grid_tracks() {
    let row_children = (0..5)
        .map(|index| Node::widget(Custom::create(CustomParameters::new(format!("row {index}")))))
        .collect::<Vec<_>>();
    let (_, row) = Linear::create(
        LinearParameters::horizontal(row_children.into_iter().enumerate().map(|(index, child)| match index {
            0 => LinearItem::fixed(child, 18),
            1 => LinearItem::content(child),
            2 | 3 => LinearItem::flex(child, 1.0),
            _ => LinearItem::fixed(child, 4),
        }))
        .stretch_cross(),
    );
    let grid_items = (0..5)
        .map(|index| Node::widget(Custom::create(CustomParameters::new(format!("grid {index}")))))
        .collect::<Vec<_>>();
    let (_, grid) = Grid::create(GridParameters::new(
        [
            TrackSize::Fixed(18),
            TrackSize::Content,
            TrackSize::Flex(1.0),
            TrackSize::Flex(1.0),
            TrackSize::Fixed(4),
        ],
        [TrackSize::Flex(1.0)],
        grid_items,
    ));
    let (_, flexible_column) = Linear::create(LinearParameters::vertical([
        LinearItem::flex(Node::widget(Custom::create(CustomParameters::new("column first"))), 1.0),
        LinearItem::flex(Node::widget(Custom::create(CustomParameters::new("column second"))), 1.0),
    ]));
    let (_, content) = Linear::create(LinearParameters::vertical([row, grid, flexible_column]));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "intrinsic", content).unwrap();
    ctx.ui().show_popup_at(&popup, rect(20, 30, 2_000, 3_000)).unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert!(
        outer.width < 1_000 && outer.height < 1_000,
        "AUTO_SIZE must derive both axes from content: {outer:?}"
    );
}

#[test]
fn body_input_falls_through_chrome_to_the_application_node() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.ui().create_window(Window::new("window", rect(20, 20, 140, 100), content));
    let mut button_dispatcher = event_counter(button);
    let mut root_dispatcher = event_counter(root.clone());
    let mut button_submissions = 0;
    let mut root_submissions = 0;
    ctx.update_and_render_ui();
    let body = ctx.debug_root_body(root.id()).unwrap();

    ctx.mousedown(body.x + body.width / 2, body.y + body.height / 2, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert!(button_dispatcher.dispatch(&mut button_submissions));
    assert_eq!(button_submissions, 1);
    assert!(!root_dispatcher.dispatch(&mut root_submissions));
}

#[test]
fn post_tree_chrome_overlay_is_submitted_after_custom_descendant_rendering() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new_test(backend, Dimensioni::new(320, 240));
    let custom = ctx.register_custom_renderer(|frame, _args| frame.record_marker("application content")).unwrap();
    let content = Node::custom_render(Custom::create(CustomParameters::new("custom")), custom);
    let _root = ctx.ui().create_window(Window::new("window", rect(20, 20, 140, 100), content));

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
