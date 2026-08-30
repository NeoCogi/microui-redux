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

//! Cross-phase runtime characterization.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use crate::test_support::test_atlas;
use crate::ui_node::children::ChildrenHandle;
use crate::{ChildParticipation, Children, ContainerWidget, TypedWidgetHandle, Widget, WidgetPaintCtx, WidgetUpdateCtx};
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
    name: &'static str,
    counts: Rc<ProbeCounts>,
    log: Rc<RefCell<Vec<String>>>,
    opt: WidgetOption,
    keyboard: KeyboardBehavior,
}

#[derive(Default)]
struct StyleObservations {
    measure: Cell<(i32, i32)>,
    update: Cell<(i32, i32)>,
    paint: Cell<(i32, i32)>,
}

struct StyleProbe {
    observations: Rc<StyleObservations>,
    opt: WidgetOption,
}

impl StyleProbe {
    fn observe(style: &Style) -> (i32, i32) {
        (style.padding, style.spacing)
    }
}

impl Widget for StyleProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        self.observations.update.set(Self::observe(ctx.style()));
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.observations.paint.set(Self::observe(ctx.style()));
    }
}

impl crate::LeafWidget for StyleProbe {
    fn measure(&self, style: &Style, _atlas: &crate::AtlasHandle, _constraints: Constraints) -> Dimensioni {
        self.observations.measure.set(Self::observe(style));
        Dimensioni::new(10, 10)
    }
}

impl Probe {
    fn new(name: &'static str, log: Rc<RefCell<Vec<String>>>) -> (Self, Rc<ProbeCounts>) {
        let counts = Rc::new(ProbeCounts::default());
        (
            Self {
                name,
                counts: counts.clone(),
                log,
                opt: WidgetOption::NONE,
                keyboard: KeyboardBehavior::FOCUSABLE,
            },
            counts,
        )
    }
}

impl Widget for Probe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
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

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        self.keyboard
    }
}

impl crate::LeafWidget for Probe {
    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _constraints: Constraints) -> Dimensioni {
        self.counts.measures.set(self.counts.measures.get() + 1);
        Dimensioni::new(17, 13)
    }
}

struct FocusProbe {
    opt: WidgetOption,
}

impl Widget for FocusProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::TAB_STOP
    }
}

impl crate::LeafWidget for FocusProbe {
    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(20, 20)
    }
}

struct TraversalContainer {
    children: ChildrenHandle,
    measurements: Cell<usize>,
    visible: bool,
    log: Rc<RefCell<Vec<String>>>,
    opt: WidgetOption,
}

impl TraversalContainer {
    fn new(children: impl IntoIterator<Item = Node>, log: Rc<RefCell<Vec<String>>>) -> (Container, TypedWidgetHandle<Self>) {
        let children = Rc::new(RefCell::new(children.into_iter().collect()));
        let widget = TraversalContainer {
            children: ChildrenHandle::new(&children),
            measurements: Cell::new(0),
            visible: true,
            log,
            opt: WidgetOption::NONE,
        };
        let (handle, container) = Container::from_shared(children, widget);
        (container, handle)
    }
}

impl ContainerWidget for TraversalContainer {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        self.measurements.set(self.measurements.get() + 1);
        (0..ctx.child_count())
            .filter_map(|index| ctx.measure_child(index, constraints))
            .fold(Dimensioni::default(), |size, child| {
                Dimensioni::new(size.width.max(child.width), size.height.max(child.height))
            })
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        let visible = self.visible;
        for index in 0..children.len() {
            let participation = if visible { ChildParticipation::Active } else { ChildParticipation::Hidden };
            let _ = ctx.set_child_participation(children, index, participation);
            if visible {
                let _ = ctx.layout_child(children, index, rect);
            }
        }
    }
}

impl Widget for TraversalContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        self.log.borrow_mut().push("container:update".to_owned());
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.log.borrow_mut().push("container:paint".to_owned());
    }
}

struct CaptureContainer {
    active: bool,
    drags: usize,
    saw_capture_during_drag: bool,
    opt: WidgetOption,
}

impl CaptureContainer {
    fn new() -> (Container, TypedWidgetHandle<Self>) {
        let widget = CaptureContainer {
            active: false,
            drags: 0,
            saw_capture_during_drag: false,
            opt: WidgetOption::NONE,
        };
        let (handle, container) = Container::new(widget, []);
        (container, handle)
    }
}

impl ContainerWidget for CaptureContainer {
    fn measure(&self, _ctx: &mut MeasureCtx<'_>, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(20, 20)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, _children: &mut Children, rect: Recti) {
        ctx.set_content_size(Dimensioni::new(rect.width.max(0), rect.height.max(0)));
    }
}

impl Widget for CaptureContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Mirror the public contract: reconcile private drag state from runtime-owned activity on
        // every update, including an eventless update after capture invalidation.
        self.active = ctx.active();
        if let Some(event) = input {
            match event {
                UiInputEvent::MouseDrag { .. } if ctx.active() => {
                    self.saw_capture_during_drag = ctx.focused();
                    self.drags += 1;
                }
                _ => {}
            }
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        // This overloaded container accepts pointer drag focus but is not a sequential Tab stop.
        KeyboardBehavior::FOCUSABLE
    }
}

struct CrossSubtreeRemover {
    target: TypedWidgetHandle<TraversalContainer>,
    removed: bool,
    opt: WidgetOption,
}

impl Widget for CrossSubtreeRemover {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        if !self.removed {
            self.target
                .try_update(|target| target.children.try_clear().expect("target topology must be available during sibling update"))
                .expect("cross-subtree target widget must be independently available");
            self.removed = true;
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for CrossSubtreeRemover {
    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(10, 10)
    }
}

fn layout_root(runtime: &mut UiRuntime, root: &mut Node, style: &Style, atlas: crate::AtlasHandle) {
    runtime.layout_tree_root(root, style, atlas, Recti::new(10, 20, 80, 60), Recti::new(0, 0, 320, 240));
}

/// Creates the fixed-size leaf used by layout characterization tests and returns its runtime id.
fn layout_probe(name: &'static str) -> (RuntimeNodeId, Node) {
    let (probe, _) = Probe::new(name, Rc::new(RefCell::new(Vec::new())));
    let node = Node::widget(probe);
    (node.id(), node)
}

fn layout_button(label: &'static str) -> (RuntimeNodeId, Node) {
    let (_, node) = crate::Button::create(crate::ButtonParameters::new(label));
    (node.id(), node)
}

fn committed_rect(runtime: &UiRuntime, root: &Node, id: RuntimeNodeId) -> Recti {
    runtime
        .node_rect(std::slice::from_ref(root), id)
        .expect("the characterized child must have a committed rectangle")
}

fn rect_components(rect: Recti) -> (i32, i32, i32, i32) {
    (rect.x, rect.y, rect.width, rect.height)
}

fn empty_input() -> InputSnapshot {
    Input::default().snapshot()
}

fn next_input(input: &mut Input) -> (UiInputEvent, InputSnapshot) {
    let event = input.pop_event().expect("test input must contain one queued event");
    (event, input.snapshot())
}

#[test]
fn demo_flex_row_geometry_is_preserved_as_an_explicit_baseline() {
    // `demo-full` uses this sequence for its three-button rows. Explicit fixed edge tracks leave
    // the center item the remaining space; spacing is outside all three tracks.
    let (label_id, label) = layout_button("label");
    let (middle_id, middle) = layout_button("middle");
    let (last_id, last) = layout_button("last");
    let (_, mut root) = crate::Linear::create(crate::LinearParameters::horizontal([
        LinearItem::fixed(label, 86),
        LinearItem::flex(middle, 1.0),
        LinearItem::fixed(last, 109),
    ]));
    let style = Style { spacing: 4, ..Style::default() };
    let mut runtime = UiRuntime::new();
    runtime.begin_update();
    runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(0, 0, 400, 40), Recti::new(0, 0, 400, 40));

    assert_eq!(rect_components(committed_rect(&runtime, &root, label_id)), (0, 0, 86, 20));
    assert_eq!(rect_components(committed_rect(&runtime, &root, middle_id)), (90, 0, 197, 20));
    assert_eq!(rect_components(committed_rect(&runtime, &root, last_id)), (291, 0, 109, 20));
}

#[test]
fn calculator_flex_geometry_is_preserved_as_an_explicit_baseline() {
    // The calculator assigns one quarter of the gap-adjusted column to the display and gives the
    // exact remaining height to the keypad. This records the visible result independently of the
    // retired policy names that happened to produce it.
    let (display_id, display) = layout_probe("display");
    let (keypad_id, keypad) = layout_probe("keypad");
    let (_, mut root) = crate::Linear::create(crate::LinearParameters::vertical([
        LinearItem::flex(display, 1.0),
        LinearItem::flex(keypad, 3.0),
    ]));
    let style = Style { spacing: 4, ..Style::default() };
    let mut runtime = UiRuntime::new();
    runtime.begin_update();
    runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(0, 0, 320, 420), Recti::new(0, 0, 320, 420));

    assert_eq!(rect_components(committed_rect(&runtime, &root, display_id)), (0, 0, 320, 104));
    assert_eq!(rect_components(committed_rect(&runtime, &root, keypad_id)), (0, 108, 320, 312));
}

#[test]
fn calculator_keypad_grid_fills_its_nested_linear_allocation() {
    let (display_id, display) = layout_probe("display");
    let buttons = (0..20).map(|_| layout_probe("key").1).collect::<Vec<_>>();
    let first_button_id = buttons.first().expect("calculator keypad has buttons").id();
    let last_button_id = buttons.last().expect("calculator keypad has buttons").id();
    let (_, grid) = crate::Grid::create(crate::GridParameters::new([TrackSize::Flex(1.0); 4], [TrackSize::Flex(1.0); 5], buttons));
    let (_, keypad_column) = crate::Linear::create(crate::LinearParameters::vertical([LinearItem::flex(grid, 1.0)]));
    let (_, keypad_row) = crate::Linear::create(crate::LinearParameters::horizontal([LinearItem::flex(keypad_column, 1.0)]).stretch_cross());
    let (_, mut root) = crate::Linear::create(crate::LinearParameters::vertical([
        LinearItem::flex(display, 0.2),
        LinearItem::flex(keypad_row, 0.8),
    ]));
    let style = Style { spacing: 4, ..Style::default() };
    let mut runtime = UiRuntime::new();
    runtime.begin_update();
    runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(0, 0, 320, 420), Recti::new(0, 0, 320, 420));

    assert_eq!(rect_components(committed_rect(&runtime, &root, display_id)), (0, 0, 320, 84));
    assert_eq!(rect_components(committed_rect(&runtime, &root, first_button_id)), (0, 88, 77, 64));
    assert_eq!(rect_components(committed_rect(&runtime, &root, last_button_id)), (243, 357, 77, 63));
}

#[test]
fn demo_weight_grid_fills_remaining_height_and_preserves_one_to_two_heights() {
    // This is the layout structure used by demo-full's Weight Demo. The Grid is the outer vertical Linear's
    // flexible item, so it receives the resolved remaining height directly. A content-sized
    // container inserted between those nodes would create a different edge and correctly shrink
    // the Grid back to its intrinsic height.
    fn row(node: Node, cross_size: crate::LinearCrossSize) -> Node {
        crate::Linear::create(crate::LinearParameters::horizontal([LinearItem::flex(node, 1.0)]).with_cross_size(cross_size)).1
    }

    let (_, row_label) = crate::ListItem::create(crate::ListItemParameters::with_opt("Row weights 1 : 2 : 3", WidgetOption::NO_INTERACT));
    let row_label = row(row_label, crate::LinearCrossSize::Content);

    let row_buttons = [layout_button("w1").1, layout_button("w2").1, layout_button("w3").1];
    let (_, row_buttons) = crate::Linear::create(
        crate::LinearParameters::horizontal(
            row_buttons
                .into_iter()
                .zip([TrackSize::Flex(1.0), TrackSize::Flex(2.0), TrackSize::Flex(3.0)])
                .map(|(button, width)| LinearItem::new(button, width)),
        )
        .fixed_cross(28),
    );

    let (_, grid_label) = crate::ListItem::create(crate::ListItemParameters::with_opt("Grid weights rows 1 : 2", WidgetOption::NO_INTERACT));
    let grid_label = row(grid_label, crate::LinearCrossSize::Content);

    let (g1_id, g1) = layout_button("g1");
    let (g2_id, g2) = layout_button("g2");
    let (g3_id, g3) = layout_button("g3");
    let (g4_id, g4) = layout_button("g4");
    let (g5_id, g5) = layout_button("g5");
    let (g6_id, g6) = layout_button("g6");
    let (_, grid) = crate::Grid::create(crate::GridParameters::new(
        [TrackSize::Flex(1.0); 3],
        [TrackSize::Flex(1.0), TrackSize::Flex(2.0)],
        [g1, g2, g3, g4, g5, g6],
    ));
    let (_, mut root) = crate::Linear::create(crate::LinearParameters::vertical([
        LinearItem::content(row_label),
        LinearItem::fixed(row_buttons, 28),
        LinearItem::content(grid_label),
        LinearItem::flex(grid, 1.0),
    ]));
    let style = Style { spacing: 4, ..Style::default() };
    let mut runtime = UiRuntime::new();
    runtime.begin_update();
    runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(0, 0, 268, 216), Recti::new(0, 0, 268, 216));

    // The content labels consume 20 pixels each, the explicit button row consumes 28, and the
    // outer three gaps consume 12. The Grid therefore owns all 136 remaining pixels. Its own
    // four-pixel gap leaves 132 pixels, split exactly 44:88 by the 1:2 row weights.
    assert_eq!(rect_components(committed_rect(&runtime, &root, g1_id)), (0, 80, 87, 44));
    assert_eq!(rect_components(committed_rect(&runtime, &root, g2_id)), (91, 80, 87, 44));
    assert_eq!(rect_components(committed_rect(&runtime, &root, g3_id)), (182, 80, 86, 44));
    assert_eq!(rect_components(committed_rect(&runtime, &root, g4_id)), (0, 128, 87, 88));
    assert_eq!(rect_components(committed_rect(&runtime, &root, g5_id)), (91, 128, 87, 88));
    assert_eq!(rect_components(committed_rect(&runtime, &root, g6_id)), (182, 128, 86, 88));
}

#[test]
fn demo_column_bottom_margin_geometry_is_preserved_as_an_explicit_baseline() {
    // The log panel uses one flexible vertical Linear item followed by an explicit 24-pixel spacer below
    // its scrolling child for the submission row that follows it.
    let (content_id, content) = layout_probe("content");
    let spacer = Node::widget(WidgetOption::NONE);
    let (_, mut root) = crate::Linear::create(crate::LinearParameters::vertical([
        LinearItem::flex(content, 1.0),
        LinearItem::fixed(spacer, 24),
    ]));
    let mut runtime = UiRuntime::new();
    runtime.begin_update();
    runtime.layout_tree_root(
        &mut root,
        &Style { spacing: 0, ..Style::default() },
        test_atlas(),
        Recti::new(0, 0, 400, 300),
        Recti::new(0, 0, 400, 300),
    );

    assert_eq!(rect_components(committed_rect(&runtime, &root, content_id)), (0, 0, 400, 276));
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
fn subtree_measurement_is_reused_within_one_layout_pass() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (probe, counts) = Probe::new("leaf", log.clone());
    let (container, _) = TraversalContainer::new([Node::widget(probe)], log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();

    layout_root(&mut runtime, &mut root, &Style::default(), test_atlas());

    assert_eq!(counts.measures.get(), 1, "placement must reuse the identical recursive measurement");
}

#[test]
fn retained_measurement_cache_survives_layout_passes_and_invalidates_ancestors() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (probe, counts) = Probe::new("leaf", log.clone());
    let (probe, probe_node) = Node::typed_widget(probe);
    let (container, container_state) = TraversalContainer::new([probe_node], log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    assert_eq!(counts.measures.get(), 1, "an unchanged retained leaf must reuse its measurement across passes");
    assert_eq!(container_state.try_read(|state| state.measurements.get()), Some(1));

    probe.try_update(|_| {}).unwrap();
    layout_root(&mut runtime, &mut root, &style, atlas);
    assert_eq!(counts.measures.get(), 2, "typed mutation must invalidate the retained leaf measurement");
    assert_eq!(
        container_state.try_read(|state| state.measurements.get()),
        Some(2),
        "child invalidation must clear its dependent container cache"
    );
}

#[test]
fn child_topology_mutation_invalidates_its_container_through_the_typed_update() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, first_counts) = Probe::new("first", log.clone());
    let (container, container_state) = TraversalContainer::new([Node::widget(first)], log.clone());
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    assert_eq!(container_state.try_read(|state| state.measurements.get()), Some(1));
    assert_eq!(first_counts.measures.get(), 1);

    let (second, second_counts) = Probe::new("second", log);
    assert_eq!(
        container_state.try_update(|state| state.children.try_push(Node::widget(second)).is_ok()),
        Some(true)
    );
    layout_root(&mut runtime, &mut root, &style, atlas);

    assert_eq!(container_state.try_read(|state| state.measurements.get()), Some(2));
    assert_eq!(first_counts.measures.get(), 1, "unchanged descendants retain their own cached measurements");
    assert_eq!(second_counts.measures.get(), 1);
}

#[test]
fn measurement_cache_keeps_constraints_distinct_within_one_pass() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (probe, counts) = Probe::new("leaf", log);
    let (_, mut root) = crate::Linear::create(crate::LinearParameters::vertical([Node::widget(probe)]));
    let mut runtime = UiRuntime::new();

    layout_root(&mut runtime, &mut root, &Style::default(), test_atlas());

    assert_eq!(
        counts.measures.get(),
        2,
        "the intrinsic main-axis query and exact final slot must remain distinct cache keys"
    );
}

#[test]
fn common_phases_are_parent_first_and_siblings_are_forward() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, first_counts) = Probe::new("first", log.clone());
    let (second, second_counts) = Probe::new("second", log.clone());
    let (container, _) = TraversalContainer::new([Node::widget(first), Node::widget(second)], log.clone());
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    log.borrow_mut().clear();
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas, true);

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
fn container_style_cascades_and_child_override_replaces_it_in_every_phase() {
    let observations = Rc::new(StyleObservations::default());
    let child = StyleProbe {
        observations: observations.clone(),
        opt: WidgetOption::NONE,
    };
    let (child, child_node) = Node::typed_widget(child);
    let (container, _) = TraversalContainer::new([child_node], Rc::new(RefCell::new(Vec::new())));
    let container_style = Style {
        padding: 17,
        spacing: 19,
        ..Style::default()
    };
    let mut root = Node::container(container).with_style_override(container_style);

    let style = Style {
        padding: 3,
        spacing: 5,
        ..Style::default()
    };
    let atlas = test_atlas();
    let mut runtime = UiRuntime::new();
    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas.clone(), true);

    assert_eq!(observations.measure.get(), (17, 19));
    assert_eq!(observations.update.get(), (17, 19));
    assert_eq!(observations.paint.get(), (17, 19));

    let child_style = Style {
        padding: 29,
        spacing: 31,
        ..Style::default()
    };
    child.try_set_style_override(child_style).unwrap();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    assert_eq!(observations.measure.get(), (29, 31));
    assert_eq!(child.try_style_override().flatten().unwrap().spacing, 31);

    child.try_clear_style_override().unwrap();
    layout_root(&mut runtime, &mut root, &style, atlas);
    assert_eq!(
        observations.measure.get(),
        (17, 19),
        "clearing a child override must reveal its container style"
    );
}

#[test]
fn consumed_event_conservatively_invalidates_recipient_and_ancestors() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (probe, counts) = Probe::new("leaf", log.clone());
    let (container, container_state) = TraversalContainer::new([Node::widget(probe)], log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    let leaf_measurements = counts.measures.get();
    let container_measurements = container_state.try_read(|state| state.measurements.get()).unwrap();

    let event = UiInputEvent::MouseMove {
        pos: Vec2i::new(20, 30),
        delta: Vec2i::new(1, 0),
    };
    runtime.begin_input_event(true, &event);
    assert!(runtime.route_input_event_to_node_ref(&mut root, &style, &event).is_some());
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    layout_root(&mut runtime, &mut root, &style, atlas);

    assert_eq!(counts.measures.get(), leaf_measurements + 1);
    assert_eq!(container_state.try_read(|state| state.measurements.get()), Some(container_measurements + 1));
}

#[test]
fn layout_participation_filters_descendants_after_state_changes() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (child, child_counts) = Probe::new("child", log.clone());
    let (container, container_state) = TraversalContainer::new([Node::widget(child)], log.clone());
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    log.borrow_mut().clear();
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    container_state.try_update(|state| state.visible = false).unwrap();
    // Visibility is a layout result, so commit the state change before paint consumes the flag.
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.paint_tree_root(&mut root, &mut DisplayList::default(), &style, atlas, true);

    let expected = ["container:update", "child:update", "container:paint"].map(str::to_owned);
    assert_eq!(log.borrow().as_slice(), expected.as_slice());
    assert_eq!((child_counts.updates.get(), child_counts.paints.get()), (1, 0));
}

#[test]
fn overlapping_pointer_routing_visits_siblings_in_reverse_z_order() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, first_counts) = Probe::new("first", log.clone());
    let (second, second_counts) = Probe::new("second", log.clone());
    let (container, _) = TraversalContainer::new([Node::widget(first), Node::widget(second)], log);
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
    let routed = runtime.route_input_event_to_node_ref(&mut root, &style, &event);
    assert_eq!(routed.map(|(_, result)| result), Some(RouteResult::Captured));
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
    let (container, _) = TraversalContainer::new([first_id, second_id], log);
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
    let target = runtime.route_input_event_to_node_ref(&mut root, &style, &event).map(|(owner, _)| owner);
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
    let (container, _) = TraversalContainer::new([first, Node::widget(second)], log);
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
        runtime.route_input_event_to_node_ref(&mut root, &style, &event).map(|(owner, _)| owner),
        Some(first_id)
    );
}

#[test]
fn composite_header_is_targeted_as_a_real_child_surface() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (lower, lower_counts) = Probe::new("lower", log.clone());
    let lower = Node::widget(lower);
    let lower_id = lower.id();
    let (_, disclosure) = crate::Disclosure::create(crate::DisclosureParameters::header("Header", true, std::iter::empty::<crate::LinearItem>()));
    let disclosure_id = disclosure.id();
    let (container, _) = TraversalContainer::new([lower, disclosure], log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());

    let disclosure_rect = runtime
        .node_rect(std::slice::from_ref(&root), disclosure_id)
        .expect("laid-out disclosure must retain a screen allocation");
    let event = UiInputEvent::MouseMove {
        // The header is now a concrete child widget placed at the top of the allocation.
        pos: Vec2i::new(disclosure_rect.x + 1, disclosure_rect.y + 1),
        delta: Vec2i::default(),
    };
    runtime.begin_input_event(true, &event);
    let routed = runtime.route_input_event_to_node_ref(&mut root, &style, &event);
    assert!(routed.is_some(), "the input router must target the explicit header child");
    assert_ne!(
        runtime.debug_hover_target(),
        Some(disclosure_id),
        "the structural disclosure must not impersonate its header"
    );
    assert_ne!(
        runtime.debug_hover_target(),
        Some(lower_id),
        "the covered sibling must remain occluded by the header child"
    );
    runtime.update_tree_root(&mut root, &style, atlas, empty_input());
    assert_eq!(lower_counts.routed_events.get(), 0, "a covered sibling must not receive the bubbled event");
}

#[test]
fn ignored_topmost_pointer_target_never_exposes_a_covered_sibling() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (mut lower, lower_counts) = Probe::new("lower", log.clone());
    lower.opt = WidgetOption::GRAB_SCROLL;
    let (upper, upper_counts) = Probe::new("upper", log.clone());
    let (container, container_state) = TraversalContainer::new([Node::widget(lower), Node::widget(upper)], log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    let lower_measurements = lower_counts.measures.get();
    let upper_measurements = upper_counts.measures.get();
    let container_measurements = container_state.try_read(|state| state.measurements.get()).unwrap();
    let event = UiInputEvent::Scroll {
        pos: Vec2i::new(20, 30),
        delta: Vec2i::new(0, 1),
    };
    runtime.begin_input_event(true, &event);
    let routed = runtime.route_input_event_to_node_ref(&mut root, &style, &event);
    assert_eq!(routed.map(|(_, result)| result), Some(RouteResult::Ignored));
    runtime.update_tree_root(&mut root, &style, atlas.clone(), empty_input());
    layout_root(&mut runtime, &mut root, &style, atlas);

    assert_eq!(upper_counts.routed_events.get(), 0, "unsupported events are not delivered to the target update");
    assert_eq!(
        lower_counts.routed_events.get(),
        0,
        "the covered sibling must never be considered after the hit"
    );
    assert!(upper_counts.hovered.get(), "the geometric target remains hovered when its event bubbles");
    assert!(!lower_counts.hovered.get());
    assert_eq!(lower_counts.measures.get(), lower_measurements);
    assert_eq!(upper_counts.measures.get(), upper_measurements);
    assert_eq!(container_state.try_read(|state| state.measurements.get()), Some(container_measurements));
}

#[test]
fn focusable_widget_retains_focus_after_pointer_capture_ends() {
    let mut root = Node::widget(FocusProbe { opt: WidgetOption::NONE });
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
        .route_input_event_to_node_ref(&mut root, &style, &down)
        .expect("pointer-down must route to the focus probe");
    runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
    runtime.update_tree_root(&mut root, &style, atlas.clone(), down_state);
    assert_eq!(runtime.debug_focus_target(), Some(id));

    input.mouseup(20, 30, MouseButton::LEFT);
    let (release, release_state) = next_input(&mut input);
    runtime.begin_input_event(true, &release);
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, release_state.mouse_buttons, &release),
        Some(true)
    );
    runtime.update_tree_root(&mut root, &style, atlas, release_state);

    assert_eq!(runtime.debug_capture_target(), None);
    assert_eq!(
        runtime.debug_focus_target(),
        Some(id),
        "keyboard focus must remain independent from the completed pointer capture"
    );
}

#[test]
fn sequential_focus_uses_retained_order_wraps_and_skips_ineligible_nodes() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let first = Node::widget(FocusProbe { opt: WidgetOption::NONE });
    let first_id = first.id();

    // A focusable surface need not be a sequential stop. This distinction lets custom controls
    // accept explicit pointer focus without unexpectedly joining the window's Tab order.
    let (mut pointer_focus_only, _) = Probe::new("pointer-focus-only", log.clone());
    pointer_focus_only.keyboard = KeyboardBehavior::FOCUSABLE;
    let pointer_focus_only = Node::widget(pointer_focus_only);

    // Disabled nodes remain in the retained tree but are excluded from every input path.
    let disabled = Node::widget(FocusProbe { opt: WidgetOption::NO_INTERACT });

    // Hidden descendants retain their state and relative position without participating until
    // their structural parent makes them active again.
    let hidden = Node::widget(FocusProbe { opt: WidgetOption::NONE });
    let hidden_id = hidden.id();
    let (hidden_branch, hidden_branch_state) = TraversalContainer::new([hidden], log.clone());
    hidden_branch_state.try_update(|branch| branch.visible = false).unwrap();

    let last = Node::widget(FocusProbe { opt: WidgetOption::NONE });
    let last_id = last.id();
    let (container, _) = TraversalContainer::new([first, pointer_focus_only, disabled, Node::container(hidden_branch), last], log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    assert!(runtime.advance_focus(std::slice::from_mut(&mut root), false));
    assert_eq!(runtime.debug_focus_target(), Some(first_id));
    assert_eq!(runtime.debug_focus_depth(), 2, "the root container and focused child form one retained path");
    assert!(runtime.advance_focus(std::slice::from_mut(&mut root), false));
    assert_eq!(runtime.debug_focus_target(), Some(last_id));
    assert!(runtime.advance_focus(std::slice::from_mut(&mut root), false));
    assert_eq!(runtime.debug_focus_target(), Some(first_id), "forward traversal must wrap");
    assert!(runtime.advance_focus(std::slice::from_mut(&mut root), true));
    assert_eq!(runtime.debug_focus_target(), Some(last_id), "reverse traversal must wrap");
    assert_ne!(runtime.debug_focus_target(), Some(hidden_id));
}

/// Verifies that pointer-only capture and persistent keyboard focus can belong to sibling widgets.
#[test]
fn focus_preserving_pointer_target_captures_without_replacing_keyboard_focus() {
    // Overlap two children so reverse paint-order targeting selects the second child while the
    // first child remains a valid retained keyboard-focus owner.
    let log = Rc::new(RefCell::new(Vec::new()));
    let first = Node::widget(FocusProbe { opt: WidgetOption::NONE });
    let first_id = first.id();
    let (mut pointer_only, _) = Probe::new("pointer-only", log.clone());
    pointer_only.keyboard = KeyboardBehavior::NONE;
    let pointer_only = Node::widget(pointer_only);
    let pointer_only_id = pointer_only.id();
    let (container, _) = TraversalContainer::new([first, pointer_only], log);
    let mut root = Node::container(container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    // Commit geometry before installing a valid existing focus identity and routing the press.
    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.debug_set_transient_targets(std::slice::from_ref(&root), Some(first_id), None, None);

    let mut input = Input::default();
    input.mousedown(20, 30, MouseButton::LEFT);
    let (down, down_state) = next_input(&mut input);
    runtime.begin_input_event(true, &down);
    let (owner, result) = runtime
        .route_input_event_to_node_ref(&mut root, &style, &down)
        .expect("overlapping pointer-only child must receive the press");
    runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
    runtime.update_tree_root(&mut root, &style, atlas, down_state);

    // Pointer capture belongs to the clicked child, but text and keyboard input continue to route
    // to the original hold-focus widget.
    assert_eq!(owner, pointer_only_id);
    assert_eq!(runtime.debug_capture_target(), Some(pointer_only_id));
    assert_eq!(runtime.debug_focus_target(), Some(first_id));
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
        .route_input_event_to_node_ref(&mut root, &style, &down)
        .expect("container pointer-down must route");
    runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
    assert_eq!(runtime.debug_capture_target(), Some(id));
    assert_eq!(runtime.debug_hover_target(), Some(id));
    runtime.update_tree_root(&mut root, &style, atlas.clone(), down_state);
    assert_eq!(state.try_read(|state| state.active), Some(true));

    input.mousemove(200, 180);
    let (drag, drag_state) = next_input(&mut input);
    runtime.begin_input_event(true, &drag);
    assert_eq!(
        runtime.route_captured_pointer_input_event(std::slice::from_mut(&mut root), &style, drag_state.mouse_buttons, &drag,),
        Some(true)
    );
    assert_eq!(
        runtime.debug_hover_target(),
        None,
        "capture delivery outside the owner's pure surface must not imply hover"
    );

    runtime.update_tree_root(&mut root, &style, atlas, drag_state);
    assert_eq!(runtime.debug_capture_target(), Some(id));
    assert_eq!(state.try_read(|state| state.active), Some(true));
    assert_eq!(state.try_read(|state| state.saw_capture_during_drag), Some(true));
    assert_eq!(state.try_read(|state| state.drags), Some(1));
}

#[test]
fn routing_time_release_exposes_inactive_state_during_that_event_update() {
    let (container, state) = CaptureContainer::new();
    state.try_update(|state| state.active = true).unwrap();
    let mut root = Node::container(container);
    let id = root.id();
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.debug_set_capture_target(Some(id));

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
    assert_eq!(runtime.debug_capture_target(), None);
    assert_eq!(
        state.try_read(|state| state.active),
        Some(true),
        "local state changes only during the ordered update traversal"
    );

    runtime.update_tree_root(&mut root, &style, atlas, release_state);
    assert_eq!(state.try_read(|state| state.active), Some(false));
}

#[test]
fn a_new_press_after_release_starts_a_distinct_capture_event() {
    let (container, state) = CaptureContainer::new();
    state.try_update(|state| state.active = true).unwrap();
    let mut root = Node::container(container);
    let id = root.id();
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.debug_set_capture_target(Some(id));

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
    assert_eq!(runtime.debug_capture_target(), None);
    assert_eq!(state.try_read(|state| state.active), Some(false));

    let mut down_input = Input::default();
    down_input.mousedown(20, 30, MouseButton::LEFT);
    let (down, down_state) = next_input(&mut down_input);
    runtime.begin_input_event(true, &down);
    let (owner, result) = runtime
        .route_input_event_to_node_ref(&mut root, &style, &down)
        .expect("same target must reacquire capture");
    runtime.update_pointer_capture(owner, result, &down, down_state.mouse_buttons);
    assert_eq!(runtime.debug_capture_target(), Some(id));

    runtime.update_tree_root(&mut root, &style, atlas, down_state);
    assert_eq!(runtime.debug_capture_target(), Some(id));
    assert_eq!(state.try_read(|state| state.active), Some(true));
}

#[test]
fn ancestor_gate_clears_targets_and_next_active_update_reconciles_local_mode() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (captured, capture_state) = CaptureContainer::new();
    capture_state.try_update(|state| state.active = true).unwrap();
    let captured = Node::container(captured);
    let captured_id = captured.id();
    let (gate, gate_state) = TraversalContainer::new([captured], log);
    let mut root = Node::container(gate);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    runtime.debug_set_transient_targets(std::slice::from_ref(&root), Some(captured_id), Some(captured_id), Some(captured_id));
    runtime.push_routed_event(
        captured_id,
        UiInputEvent::MouseMove {
            pos: Vec2i::new(20, 30),
            delta: Vec2i::default(),
        },
    );

    gate_state.try_update(|state| state.visible = false).unwrap();
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    assert_eq!(
        (runtime.debug_focus_target(), runtime.debug_hover_target(), runtime.debug_capture_target()),
        (None, None, None)
    );
    assert!(runtime.take_routed_event(captured_id).is_none());
    assert_eq!(
        capture_state.try_read(|state| state.active),
        Some(true),
        "a gated widget is not mutated outside ordered update"
    );

    gate_state.try_update(|state| state.visible = true).unwrap();
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    assert_eq!(runtime.debug_capture_target(), None, "expansion must not restore old capture");
    runtime.update_tree_root(&mut root, &style, test_atlas(), empty_input());
    assert_eq!(
        capture_state.try_read(|state| state.active),
        Some(false),
        "expansion must not restore old local mode"
    );
}

#[test]
fn removed_target_does_not_notify_or_transfer_state_to_same_index_replacement() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (removed, removed_state) = CaptureContainer::new();
    removed_state.try_update(|state| state.active = true).unwrap();
    let removed = Node::container(removed);
    let removed_id = removed.id();
    let (replacement, replacement_state) = CaptureContainer::new();
    let replacement = Node::container(replacement);
    let replacement_id = replacement.id();
    let (parent, parent_state) = TraversalContainer::new([removed], log);
    let mut root = Node::container(parent);
    let mut runtime = UiRuntime::new();
    let style = Style::default();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, test_atlas());
    runtime.debug_set_transient_targets(std::slice::from_ref(&root), Some(removed_id), Some(removed_id), Some(removed_id));
    runtime.push_routed_event(
        removed_id,
        UiInputEvent::MouseMove {
            pos: Vec2i::new(20, 30),
            delta: Vec2i::default(),
        },
    );

    assert_eq!(parent_state.try_update(|state| state.children.try_replace([replacement]).is_ok()), Some(true));
    layout_root(&mut runtime, &mut root, &style, test_atlas());

    assert_eq!(
        (runtime.debug_focus_target(), runtime.debug_hover_target(), runtime.debug_capture_target()),
        (None, None, None)
    );
    assert!(runtime.take_routed_event(removed_id).is_none());
    assert!(runtime.take_routed_event(replacement_id).is_none());
    assert!(!removed_state.is_alive(), "removed runtimes are dropped instead of receiving a callback");
    assert_eq!(replacement_state.try_read(|state| state.active), Some(false));

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
    assert!(runtime.debug_discards_invalidated_capture_events());

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
    assert!(!runtime.debug_discards_invalidated_capture_events());
    assert!(runtime.take_routed_event(replacement_id).is_none());
}

#[test]
fn cross_subtree_removal_during_update_sanitizes_before_later_delivery() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (captured, captured_state) = CaptureContainer::new();
    captured_state.try_update(|state| state.active = true).unwrap();
    let captured = Node::container(captured);
    let captured_id = captured.id();
    let (target_parent, target_state) = TraversalContainer::new([captured], log.clone());
    let remover = CrossSubtreeRemover {
        target: target_state,
        removed: false,
        opt: WidgetOption::NONE,
    };
    let (root_container, _) = TraversalContainer::new([Node::widget(remover), Node::container(target_parent)], log);
    let mut root = Node::container(root_container);
    let mut runtime = UiRuntime::new();
    let style = Style::default();
    let atlas = test_atlas();

    runtime.begin_update();
    layout_root(&mut runtime, &mut root, &style, atlas.clone());
    runtime.debug_set_transient_targets(std::slice::from_ref(&root), Some(captured_id), Some(captured_id), Some(captured_id));
    runtime.push_routed_event(
        captured_id,
        UiInputEvent::MouseMove {
            pos: Vec2i::new(20, 30),
            delta: Vec2i::default(),
        },
    );

    runtime.update_tree_root(&mut root, &style, atlas, empty_input());

    assert_eq!(
        (runtime.debug_focus_target(), runtime.debug_hover_target(), runtime.debug_capture_target()),
        (None, None, None)
    );
    assert!(runtime.take_routed_event(captured_id).is_none());
    assert_eq!(
        captured_state.try_read(|state| state.active),
        None,
        "removed runtime must expire without out-of-band mutation"
    );
}
