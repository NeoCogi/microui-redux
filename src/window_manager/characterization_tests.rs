//! P0.0 supported-behavior and structural-cost characterization.

use std::{
    cell::RefCell,
    hint::black_box,
    rc::Rc,
    time::{Duration, Instant},
};

use super::*;
use crate::{
    test_support::{AllocationCount, AllocationMeasurement, NoopRenderer, test_atlas},
    Button, ButtonParameters, Checkbox, CheckboxParameters, CheckboxState, ColorSwatch, ColorSwatchParameters, Combo, ComboParameters, ComboState, ListBox,
    ListBoxParameters, ListItem, ListItemParameters, Node, NodeStateValue, Number, NumberParameters, NumberState, ResourceState, RetainedId, ScrollAreaOption,
    SizePolicy, SliderParameters, SliderState, StackDirection, TextArea, TextAreaParameters, TextBlock, TextBlockParameters, Textbox, TextboxParameters,
    TextboxState, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx, color, widget_handle,
};

fn context(width: i32, height: i32) -> Context<NoopRenderer> {
    Context::new_test(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(width, height))
}

fn center(rect: Recti) -> (i32, i32) {
    (rect.x + rect.width / 2, rect.y + rect.height / 2)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Measure,
    Update,
    Paint,
}

struct PhaseWidget {
    phases: Rc<RefCell<Vec<Phase>>>,
    opt: WidgetOption,
    state: Rc<RefCell<()>>,
}

impl PhaseWidget {
    fn new(phases: Rc<RefCell<Vec<Phase>>>) -> Self {
        Self {
            phases,
            opt: WidgetOption::NONE,
            state: Rc::new(RefCell::new(())),
        }
    }
}

impl crate::WidgetStateOwner for PhaseWidget {
    type State = ();

    fn state_handle(&self) -> crate::WidgetStateHandle<Self::State> {
        crate::WidgetStateHandle::new(&self.state)
    }
}

impl Widget for PhaseWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        self.phases.borrow_mut().push(Phase::Measure);
        Dimensioni::new(24, 12)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        self.phases.borrow_mut().push(Phase::Update);
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.phases.borrow_mut().push(Phase::Paint);
    }
}

#[test]
fn p0_widget_phase_order_and_three_layouts_are_explicit() {
    let phases = Rc::new(RefCell::new(Vec::new()));
    let widget = PhaseWidget::new(phases.clone());
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(widget);
    });
    let mut ctx = context(160, 100);
    let root = ctx.create_window("phase order", rect(0, 0, 120, 70), tree);

    ctx.update_ui();

    assert_eq!(
        *phases.borrow(),
        [
            Phase::Measure,
            Phase::Measure,
            Phase::Measure,
            Phase::Measure,
            Phase::Measure,
            Phase::Measure,
            Phase::Update,
            Phase::Measure,
            Phase::Measure,
            Phase::Measure,
            Phase::Paint,
        ]
    );
    let metrics = ctx.debug_root_runtime_metrics(root).unwrap();
    assert_eq!(metrics.tree_layouts, 3);
    assert_eq!(metrics.measures, 6);
    assert_eq!(metrics.layouts, 3);
    assert_eq!(metrics.updates, 1);
    assert_eq!(metrics.paints, 1);
}

#[test]
fn p0_existing_typed_widget_mutations_remain_observable() {
    let (checkbox, _checkbox_widget) = Checkbox::create(CheckboxParameters::new("check", false));
    let (_, button) = Button::create(ButtonParameters::new("after"));
    let (list_item_state, list_item_runtime) = ListItem::create(ListItemParameters::new("before"));
    let list_item = list_item_runtime;
    let (_, list_box) = ListBox::create(ListBoxParameters::new("after box", None));
    let (combo_state, _combo_runtime) = Combo::create(ComboParameters::new());
    let (text_state, text_runtime) = TextBlock::create(TextBlockParameters::new("before"));
    let text = text_runtime;
    let (swatch_state, swatch_runtime) = ColorSwatch::create(ColorSwatchParameters::new(color(1, 2, 3, 255)));
    let swatch = swatch_runtime;
    let (slider_state, _slider_runtime) = crate::Slider::create(SliderParameters::new(0.0, 0.0, 10.0));
    let (number_state, _number_runtime) = Number::create(NumberParameters::new(0.0, 1.0, 0));
    let (textbox_state, textbox_runtime) = Textbox::create(TextboxParameters::new("before"));
    let textbox = textbox_runtime;
    let (text_area_state, text_area_runtime) = TextArea::create(TextAreaParameters::new("before"));
    let text_area = text_area_runtime;
    let disclosure = widget_handle(Node::header("section", NodeStateValue::Closed));

    checkbox.try_update(CheckboxState::check).unwrap();
    list_item_state.try_update(|state| state.set_label("after item")).unwrap();
    combo_state
        .try_update(|state| {
            state.open_popup();
            assert_eq!(state.select(1, &["zero", "one"]), Some("one".into()));
        })
        .unwrap();
    text_state.try_update(|state| state.set_text("after text")).unwrap();
    swatch_state
        .try_update(|state| {
            state.set_fill(color(9, 8, 7, 255));
            state.set_label("after swatch");
        })
        .unwrap();
    slider_state.try_update(|state| state.set_value(7.0)).unwrap();
    number_state.try_update(|state| state.set_value(8.0)).unwrap();
    textbox_state.try_update(|state| state.set_text("after textbox")).unwrap();
    text_area_state
        .try_update(|state| {
            state.set_text("after area");
            state.set_cursor(5);
            state.set_scroll(crate::vec2(3, 4));
        })
        .unwrap();
    disclosure.update(|state| state.state = NodeStateValue::Expanded);

    assert_eq!(checkbox.try_read(CheckboxState::checked), Some(true));
    assert_eq!(combo_state.try_read(ComboState::selected), Some(1));
    assert_eq!(slider_state.try_read(SliderState::value), Some(7.0));
    assert_eq!(number_state.try_read(NumberState::value), Some(8.0));
    assert_eq!(textbox_state.try_read(|state| state.text().to_owned()).as_deref(), Some("after textbox"));
    let (area_text, area_cursor, area_scroll) = text_area_state
        .try_read(|state| (state.text().to_owned(), state.cursor(), state.scroll()))
        .unwrap();
    assert_eq!(area_text, "after area");
    assert_eq!(area_cursor, 5);
    assert_eq!((area_scroll.x, area_scroll.y), (3, 4));
    assert!(disclosure.read(Node::is_expanded));

    let tree = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            tree.widget(button);
            tree.widget(list_item);
            tree.widget(list_box);
            tree.widget(text);
            tree.widget(swatch);
            tree.widget(textbox);
            tree.widget(text_area);
        });
    });
    let mut ctx = context(300, 240);
    let root = ctx.create_window("typed mutations", rect(0, 0, 260, 220), tree);
    ctx.update_ui();
    let rendered = ctx.debug_root_texts(root);
    for expected in ["after", "after item", "after box", "after text", "after swatch", "after textbox", "after area"] {
        assert!(rendered.iter().any(|text| text == expected), "missing rendered text {expected:?}: {rendered:?}");
    }
}

#[test]
fn p1_checkbox_runtime_preserves_projection_geometry_paint_and_click_behavior() {
    let (checkbox, widget) = Checkbox::create(CheckboxParameters::with_opt("retained checkbox", false, WidgetOption::FRAME));
    let mut checkbox_id = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        checkbox_id = tree.widget(widget);
    });

    let node = tree.node(checkbox_id).expect("checkbox projection node");
    assert_eq!(node.debug_erased_adapter_count(), 0);

    let mut ctx = context(220, 120);
    let root = ctx.create_window("checkbox", rect(0, 0, 180, 90), tree);
    ctx.update_ui();

    let checkbox_rect = ctx.debug_root_node_rect(root, checkbox_id).expect("checkbox geometry");
    assert!(checkbox_rect.width > 0);
    assert!(checkbox_rect.height > 0);
    assert!(ctx.debug_root_texts(root).iter().any(|text| text == "retained checkbox"));
    assert_eq!(checkbox.try_read(CheckboxState::checked), Some(false));

    let (checkbox_x, checkbox_y) = center(checkbox_rect);
    ctx.mousedown(checkbox_x, checkbox_y, MouseButton::LEFT);
    ctx.update_ui();

    assert_eq!(checkbox.try_read(CheckboxState::checked), Some(true));
    assert!(ctx.committed_results().state_of_retained(RetainedId::root_node(root, checkbox_id)).is_changed());
}

#[test]
#[should_panic(expected = "retained widget state invariant violated during Checkbox::paint: associated state is already borrowed")]
fn p1_checkbox_reports_reentrant_rendering_as_a_state_invariant_violation() {
    let (checkbox, widget) = Checkbox::create(CheckboxParameters::new("reentrant", false));
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(widget);
    });
    let mut ctx = context(160, 100);
    ctx.create_window("reentrant checkbox", rect(0, 0, 120, 70), tree);

    let _ = checkbox.try_update(|_| ctx.update_ui());
}

#[test]
fn p0_committed_button_and_textbox_submissions_follow_focus() {
    let (button_state, button_runtime) = Button::create(ButtonParameters::new("submit"));
    let (textbox_state, textbox_runtime) = Textbox::create(TextboxParameters::new(""));
    let mut button_id = NodeId::default();
    let mut textbox_id = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            button_id = tree.widget(button_runtime);
            textbox_id = tree.widget(textbox_runtime);
        });
    });
    let mut ctx = context(220, 140);
    let root = ctx.create_window("input", rect(0, 0, 180, 110), tree);
    ctx.update_ui();

    let (button_x, button_y) = center(ctx.debug_root_node_rect(root, button_id).unwrap());
    ctx.mousedown(button_x, button_y, MouseButton::LEFT);
    ctx.update_ui();
    assert!(ctx.committed_results().state_of_retained(RetainedId::root_node(root, button_id)).is_submitted());
    assert_eq!(button_state.try_update(crate::ButtonState::take_submitted), Some(true));
    ctx.mouseup(button_x, button_y, MouseButton::LEFT);
    ctx.update_ui();

    let (textbox_x, textbox_y) = center(ctx.debug_root_node_rect(root, textbox_id).unwrap());
    ctx.mousedown(textbox_x, textbox_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mouseup(textbox_x, textbox_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.text("typed");
    ctx.keydown(KeyMode::RETURN);
    ctx.update_ui();

    let result = ctx.committed_results().state_of_retained(RetainedId::root_node(root, textbox_id));
    assert!(result.is_changed());
    assert!(result.is_submitted());
    assert_eq!(textbox_state.try_update(TextboxState::take_changed), Some(true));
    assert_eq!(textbox_state.try_update(TextboxState::take_submitted), Some(true));
    assert_eq!(textbox_state.try_read(|state| state.text().to_owned()).as_deref(), Some("typed"));

    ctx.text(" again");
    ctx.update_ui();
    assert_eq!(textbox_state.try_read(|state| state.text().to_owned()).as_deref(), Some("typed again"));
}

#[test]
fn keyboard_text_routes_to_only_the_front_roots_focused_widget() {
    let (first_state, first_runtime) = Textbox::create(TextboxParameters::new(""));
    let (second_state, second_runtime) = Textbox::create(TextboxParameters::new(""));
    let mut first_id = NodeId::default();
    let mut second_id = NodeId::default();
    let first_tree = UiNodeBuilder::build(|tree| {
        first_id = tree.widget(first_runtime);
    });
    let second_tree = UiNodeBuilder::build(|tree| {
        second_id = tree.widget(second_runtime);
    });
    let mut ctx = context(300, 120);
    let first_root = ctx.create_window("first", rect(0, 0, 120, 90), first_tree);
    let second_root = ctx.create_window("second", rect(150, 0, 120, 90), second_tree);
    ctx.update_ui();

    let (first_x, first_y) = center(ctx.debug_root_node_rect(first_root, first_id).unwrap());
    ctx.mousedown(first_x, first_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mouseup(first_x, first_y, MouseButton::LEFT);
    ctx.update_ui();

    let (second_x, second_y) = center(ctx.debug_root_node_rect(second_root, second_id).unwrap());
    ctx.mousedown(second_x, second_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mouseup(second_x, second_y, MouseButton::LEFT);
    ctx.update_ui();

    ctx.text("x");
    ctx.update_ui();

    assert_eq!(first_state.try_read(|state| state.text().to_owned()).as_deref(), Some(""));
    assert_eq!(second_state.try_read(|state| state.text().to_owned()).as_deref(), Some("x"));
}

#[test]
fn p0_container_disclosure_scroll_and_dynamic_list_outcomes_are_stable() {
    let disclosure = widget_handle(Node::header("section", NodeStateValue::Closed));
    let (_, first_runtime) = ListItem::create(ListItemParameters::new("first"));
    let (_, second_runtime) = ListItem::create(ListItemParameters::new("second"));
    let mut disclosure_id = NodeId::default();
    let mut scroll_id = NodeId::default();
    let mut second_id = NodeId::default();
    let mut grid_a_id = NodeId::default();
    let mut grid_b_id = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(70), SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
            tree.grid(&[SizePolicy::Fixed(32), SizePolicy::Remainder(0)], &[SizePolicy::Auto], |tree| {
                grid_a_id = tree.text("grid-a");
                grid_b_id = tree.text("grid-b");
            });
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                disclosure_id = tree.header(&disclosure, |tree| {
                    tree.text("disclosed");
                });
                scroll_id = tree.node(NodeOptions::with_policy(Policy::fixed(100, 50))).scroll_area(
                    ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
                    |tree| {
                        tree.node(NodeOptions::keyed("first")).widget(first_runtime);
                        second_id = tree.node(NodeOptions::keyed("second")).widget(second_runtime);
                        for index in 0..12 {
                            tree.text(format!("row-{index}"));
                        }
                    },
                );
            });
        });
    });
    let mut ctx = context(260, 180);
    let root = ctx.create_window("containers", rect(0, 0, 220, 150), tree);
    ctx.update_ui();

    let rendered = ctx.debug_root_texts(root);
    assert!(rendered.iter().any(|text| text == "grid-a"));
    assert!(rendered.iter().any(|text| text == "grid-b"));
    assert!(!rendered.iter().any(|text| text == "disclosed"));
    let grid_a = ctx.debug_root_node_rect(root, grid_a_id).unwrap();
    let grid_b = ctx.debug_root_node_rect(root, grid_b_id).unwrap();
    assert!(grid_a.x < grid_b.x);
    assert_eq!(grid_a.y, grid_b.y);
    let second_before = ctx.debug_root_node_rect(root, second_id).unwrap();

    let (disclosure_x, disclosure_y) = center(ctx.debug_root_node_rect(root, disclosure_id).unwrap());
    ctx.mousedown(disclosure_x, disclosure_y, MouseButton::LEFT);
    ctx.update_ui();
    assert!(disclosure.read(Node::is_expanded));
    assert!(ctx.debug_root_texts(root).iter().any(|text| text == "disclosed"));
    ctx.mouseup(disclosure_x, disclosure_y, MouseButton::LEFT);
    ctx.update_ui();

    let scroll_body = ctx.scroll_area_body(root, scroll_id).unwrap();
    ctx.mousemove(scroll_body.x + 2, scroll_body.y + 2);
    ctx.scroll(0, 40);
    ctx.update_ui();
    let scroll = ctx.scroll_area_scroll(root, scroll_id).unwrap();
    assert_ne!((scroll.x, scroll.y), (0, 0));
    assert_ne!(ctx.debug_root_node_rect(root, second_id).unwrap().y, second_before.y);
}

#[test]
fn p1_direct_leaf_state_survives_frames_without_projection_rebuild() {
    let (first_state, first_runtime) = ListItem::create(ListItemParameters::new("first"));
    let (_, second_runtime) = ListItem::create(ListItemParameters::new("second"));
    let mut first_id = NodeId::default();
    let mut second_id = NodeId::default();
    let initial = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            first_id = tree.node(NodeOptions::keyed("first")).widget(first_runtime);
            second_id = tree.node(NodeOptions::keyed("second")).widget(second_runtime);
        });
    });
    let mut ctx = context(180, 140);
    let root = ctx.create_window("dynamic", rect(0, 0, 140, 110), initial);
    ctx.update_ui();
    assert!(ctx.debug_root_node_rect(root, first_id).unwrap().y < ctx.debug_root_node_rect(root, second_id).unwrap().y);

    first_state.try_update(|state| state.set_label("first retained")).unwrap();
    ctx.update_ui();

    assert!(ctx.debug_root_node_rect(root, first_id).unwrap().y < ctx.debug_root_node_rect(root, second_id).unwrap().y);
    assert!(ctx.debug_root_texts(root).iter().any(|text| text == "first retained"));
}

#[test]
fn p0_root_lifecycle_and_projection_replacement_are_observable() {
    let mut ctx = context(320, 200);
    let window = ctx.create_window(
        "window",
        rect(0, 0, 100, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("old");
        }),
    );
    let dialog = ctx.create_dialog(
        "dialog",
        rect(20, 20, 100, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("dialog");
        }),
    );
    let popup = ctx.create_popup(
        "popup",
        UiNodeBuilder::build(|tree| {
            tree.text("popup");
        }),
    );

    assert_eq!(ctx.root_visible(window), Some(true));
    assert_eq!(ctx.root_visible(dialog), Some(false));
    assert_eq!(ctx.root_visible(popup), Some(false));
    ctx.update_ui();
    assert!(ctx.debug_root_texts(window).iter().any(|text| text == "old"));

    ctx.set_root_nodes(
        window,
        UiNodeBuilder::build(|tree| {
            tree.text("new");
        }),
    );
    ctx.set_root_visible(dialog, true);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();

    assert_eq!(ctx.debug_root_projection_replacements(), 1);
    assert!(ctx.debug_root_texts(window).iter().any(|text| text == "new"));
    assert!(!ctx.debug_root_texts(window).iter().any(|text| text == "old"));
    assert!(ctx.debug_root_zindex(popup).unwrap() > ctx.debug_root_zindex(dialog).unwrap());
}

#[test]
fn p0_known_structural_costs_are_evidence_not_compatibility() {
    let (state, runtime) = TextBlock::create(TextBlockParameters::new("owned directly"));
    assert!(state.is_alive());
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(runtime);
    });
    assert!(state.is_alive());
    let roots = tree.into_roots();
    assert_eq!(roots.iter().map(UiNode::debug_erased_adapter_count).sum::<usize>(), 0);
    drop(roots);
    assert!(!state.is_alive());

    let scroll = UiNodeBuilder::build(|tree| {
        tree.scroll_area(ScrollAreaOption::ENABLE_SCROLL, |tree| {
            tree.text("content");
        });
    });
    let scroll_roots = scroll.into_roots();
    assert_eq!(
        scroll_roots[0].children().len(),
        4,
        "viewport plus two tracks and one corner are synthetic descendants"
    );
    assert_eq!(scroll_roots[0].debug_node_count(), 6);

    let mut swappable = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            tree.text("left");
        });
        tree.column(|tree| {
            tree.text("right");
        });
    })
    .into_roots();
    let (left, right) = swappable.split_at_mut(1);
    let left_child = left[0].children()[0].id();
    let right_child = right[0].children()[0].id();
    std::mem::swap(left[0].children_mut().unwrap(), right[0].children_mut().unwrap());
    assert_eq!(left[0].children()[0].id(), right_child);
    assert_eq!(right[0].children()[0].id(), left_child);

    let visible_tree = UiNodeBuilder::build(|tree| {
        tree.text("still traversed");
    });
    let mut ctx = context(120, 80);
    let root = ctx.create_window("visible bit", rect(0, 0, 100, 70), visible_tree);
    ctx.roots.iter_mut().find(|entry| entry.id == root).unwrap().roots[0].state.visible = false;
    ctx.update_ui();
    assert!(ctx.debug_root_texts(root).iter().any(|text| text == "still traversed"));
}

struct ConstraintProbe {
    heights: Rc<RefCell<Vec<i32>>>,
    opt: WidgetOption,
    state: Rc<RefCell<()>>,
}

impl crate::WidgetStateOwner for ConstraintProbe {
    type State = ();

    fn state_handle(&self) -> crate::WidgetStateHandle<Self::State> {
        crate::WidgetStateHandle::new(&self.state)
    }
}

impl Widget for ConstraintProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &crate::AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.heights.borrow_mut().push(avail.height);
        Dimensioni::new(20, 10)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

#[test]
fn p0_auto_size_probe_and_raw_routed_duplication_are_measured() {
    let heights = Rc::new(RefCell::new(Vec::new()));
    let probe = ConstraintProbe {
        heights: heights.clone(),
        opt: WidgetOption::NONE,
        state: Rc::new(RefCell::new(())),
    };
    let mut probe_id = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        probe_id = tree.widget(probe);
    });
    let mut ctx = context(160, 100);
    let root = ctx.create_window("probe", rect(0, 0, 100, 70), tree);
    ctx.set_root_options(root, WindowOption::FRAME | WindowOption::AUTO_SIZE);
    ctx.update_ui();
    assert!(heights.borrow().contains(&10_000));

    let target = ctx.debug_root_node_rect(root, probe_id).unwrap();
    let (x, y) = center(target);
    ctx.mousedown(x, y, MouseButton::LEFT);
    ctx.update_ui();
    let metrics = ctx.debug_root_runtime_metrics(root).unwrap();
    assert!(metrics.routed_input_dispatches >= 1);
    assert_eq!(metrics.raw_interaction_derivations, 1);
}

#[derive(Debug)]
struct ScenarioResult {
    name: &'static str,
    construction: AllocationCount,
    steady: AllocationCount,
    elapsed: Duration,
    nodes: usize,
    erased_adapters: usize,
    metrics: crate::ui_node::RuntimeMetrics,
}

fn measure_scenario(name: &'static str, build: impl FnOnce() -> UiNodeSet) -> ScenarioResult {
    let mut ctx = context(640, 480);
    let construction_measurement = AllocationMeasurement::begin();
    let tree = build();
    let root = ctx.create_window(name, rect(0, 0, 600, 440), tree);
    let construction = construction_measurement.finish();

    ctx.update_ui();
    ctx.update_ui();
    let started = Instant::now();
    let steady_measurement = AllocationMeasurement::begin();
    ctx.update_ui();
    let steady = steady_measurement.finish();
    let elapsed = started.elapsed();
    let (nodes, erased_adapters) = ctx.debug_root_structure(root).unwrap();
    let metrics = ctx.debug_root_runtime_metrics(root).unwrap();
    black_box(&ctx);

    ScenarioResult {
        name,
        construction,
        steady,
        elapsed,
        nodes,
        erased_adapters,
        metrics,
    }
}

fn print_scenario(result: &ScenarioResult) {
    println!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        result.name,
        result.nodes,
        result.erased_adapters,
        result.construction.events,
        result.construction.bytes,
        result.steady.events,
        result.steady.bytes,
        result.metrics.tree_layouts,
        result.metrics.measures,
        result.metrics.layouts,
        result.metrics.updates,
        result.metrics.paints,
        result.elapsed.as_nanos(),
    );
}

#[test]
#[ignore = "manual serial release-mode P0/P5 UI-node baseline"]
fn ui_node_p0_baseline_runtime() {
    let one = measure_scenario("one widget", || {
        UiNodeBuilder::build(|tree| {
            tree.text("one");
        })
    });
    let hundred = measure_scenario("100-node tree", || {
        UiNodeBuilder::build(|tree| {
            tree.column(|tree| {
                for index in 0..99 {
                    tree.text(format!("node-{index}"));
                }
            });
        })
    });
    let scroll = measure_scenario("scroll area", || {
        UiNodeBuilder::build(|tree| {
            tree.scroll_area(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, |tree| {
                for index in 0..20 {
                    tree.text(format!("row-{index}"));
                }
            });
        })
    });

    println!(
        "| scenario | nodes | erased adapters | build allocs | build bytes | steady allocs | steady bytes | tree layouts | measures | layouts | updates | paints | ns/frame |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for result in [&one, &hundred, &scroll] {
        print_scenario(result);
        assert_eq!(result.metrics.tree_layouts, 3);
        assert!(result.construction.events > 0);
        assert!(result.metrics.updates > 0);
        assert!(result.metrics.paints > 0);
    }

    assert_eq!((one.nodes, one.erased_adapters), (1, 0));
    assert_eq!((hundred.nodes, hundred.erased_adapters), (100, 0));
    assert_eq!((scroll.nodes, scroll.erased_adapters), (25, 0));
}
