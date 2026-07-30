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
    Button, Checkbox, CheckboxParameters, CheckboxState, ColorSwatch, Combo, ListBox, ListItem, Node, NodeStateValue, Number, ResourceState, RetainedId,
    ScrollAreaOption, SizePolicy, StackDirection, TextArea, TextBlock, Textbox, UiInputEvent, Widget, WidgetFillOption, WidgetOption, WidgetPaintCtx,
    WidgetUpdateCtx, color, widget_handle,
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
}

impl PhaseWidget {
    fn new(phases: Rc<RefCell<Vec<Phase>>>) -> Self {
        Self { phases, opt: WidgetOption::NONE }
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
    let widget = widget_handle(PhaseWidget::new(phases.clone()));
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(&widget);
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
    let button = widget_handle(Button::new("before"));
    let list_item = widget_handle(ListItem::new("before"));
    let list_box = widget_handle(ListBox::new("before", None));
    let combo = widget_handle(Combo::new());
    let text = widget_handle(TextBlock::new("before"));
    let swatch = widget_handle(ColorSwatch::new(color(1, 2, 3, 255)));
    let slider = widget_handle(crate::Slider::new(0.0, 0.0, 10.0));
    let number = widget_handle(Number::new(0.0, 1.0, 0));
    let textbox = widget_handle(Textbox::new("before"));
    let text_area = widget_handle(TextArea::new("before"));
    let disclosure = widget_handle(Node::header("section", NodeStateValue::Closed));

    checkbox.try_update(CheckboxState::check).unwrap();
    button.update(|state| {
        state.content = crate::ButtonContent::Text { label: "after".into(), icon: None };
        state.fill = WidgetFillOption::HOVER;
    });
    list_item.update(|state| state.label = "after item".into());
    list_box.update(|state| state.label = "after box".into());
    combo.update(|state| {
        state.open_popup();
        assert_eq!(state.select(1, &["zero", "one"]), Some("one".into()));
    });
    text.update(|state| state.text = "after text".into());
    swatch.update(|state| {
        state.fill = color(9, 8, 7, 255);
        state.label = "after swatch".into();
    });
    slider.update(|state| state.set_value(7.0));
    number.update(|state| state.set_value(8.0));
    textbox.update(|state| state.set_text("after textbox"));
    text_area.update(|state| {
        state.set_text("after area");
        state.set_cursor(5);
        state.set_scroll(crate::vec2(3, 4));
    });
    disclosure.update(|state| state.state = NodeStateValue::Expanded);

    assert_eq!(checkbox.try_read(CheckboxState::checked), Some(true));
    assert_eq!(combo.read(Combo::selected), 1);
    assert_eq!(slider.read(crate::Slider::value), 7.0);
    assert_eq!(number.read(Number::value), 8.0);
    assert_eq!(textbox.read(|state| state.text().to_owned()), "after textbox");
    let (area_text, area_cursor, area_scroll) = text_area.read(|state| (state.text().to_owned(), state.cursor(), state.scroll()));
    assert_eq!(area_text, "after area");
    assert_eq!(area_cursor, 5);
    assert_eq!((area_scroll.x, area_scroll.y), (3, 4));
    assert!(disclosure.read(Node::is_expanded));

    let tree = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            tree.widget(&button);
            tree.widget(&list_item);
            tree.widget(&list_box);
            tree.widget(&text);
            tree.widget(&swatch);
            tree.widget(&textbox);
            tree.widget(&text_area);
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
        checkbox_id = tree.state_widget(widget);
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
        tree.state_widget(widget);
    });
    let mut ctx = context(160, 100);
    ctx.create_window("reentrant checkbox", rect(0, 0, 120, 70), tree);

    let _ = checkbox.try_update(|_| ctx.update_ui());
}

#[test]
fn p0_committed_button_and_textbox_submissions_follow_focus() {
    let button = widget_handle(Button::new("submit"));
    let textbox = widget_handle(Textbox::new(""));
    let mut button_id = NodeId::default();
    let mut textbox_id = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            button_id = tree.widget(&button);
            textbox_id = tree.widget(&textbox);
        });
    });
    let mut ctx = context(220, 140);
    let root = ctx.create_window("input", rect(0, 0, 180, 110), tree);
    ctx.update_ui();

    let (button_x, button_y) = center(ctx.debug_root_node_rect(root, button_id).unwrap());
    ctx.mousedown(button_x, button_y, MouseButton::LEFT);
    ctx.update_ui();
    assert!(ctx.committed_results().state_of_retained(RetainedId::root_node(root, button_id)).is_submitted());
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
    assert_eq!(textbox.read(|state| state.text().to_owned()), "typed");
}

#[test]
fn p0_container_disclosure_scroll_and_dynamic_list_outcomes_are_stable() {
    let disclosure = widget_handle(Node::header("section", NodeStateValue::Closed));
    let first = widget_handle(ListItem::new("first"));
    let second = widget_handle(ListItem::new("second"));
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
                        tree.node(NodeOptions::keyed("first")).widget(&first);
                        second_id = tree.node(NodeOptions::keyed("second")).widget(&second);
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
fn p0_keyed_dynamic_list_reorder_preserves_typed_state_and_visual_order() {
    let first = widget_handle(ListItem::new("first"));
    let second = widget_handle(ListItem::new("second"));
    let mut first_id = NodeId::default();
    let mut second_id = NodeId::default();
    let initial = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            first_id = tree.node(NodeOptions::keyed("first")).widget(&first);
            second_id = tree.node(NodeOptions::keyed("second")).widget(&second);
        });
    });
    let mut ctx = context(180, 140);
    let root = ctx.create_window("dynamic", rect(0, 0, 140, 110), initial);
    ctx.update_ui();
    assert!(ctx.debug_root_node_rect(root, first_id).unwrap().y < ctx.debug_root_node_rect(root, second_id).unwrap().y);

    first.update(|state| state.label = "first retained".into());
    let reordered = UiNodeBuilder::build(|tree| {
        tree.column(|tree| {
            second_id = tree.node(NodeOptions::keyed("second")).widget(&second);
            first_id = tree.node(NodeOptions::keyed("first")).widget(&first);
        });
    });
    ctx.set_root_nodes(root, reordered);
    ctx.update_ui();

    assert!(ctx.debug_root_node_rect(root, second_id).unwrap().y < ctx.debug_root_node_rect(root, first_id).unwrap().y);
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
    let widget = widget_handle(TextBlock::new("owned twice"));
    assert_eq!(widget.debug_strong_count(), 1);
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(&widget);
    });
    assert_eq!(widget.debug_strong_count(), 2);
    let roots = tree.into_roots();
    assert_eq!(roots.iter().map(UiNode::debug_erased_adapter_count).sum::<usize>(), 1);

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
    let probe = widget_handle(ConstraintProbe {
        heights: heights.clone(),
        opt: WidgetOption::NONE,
    });
    let mut probe_id = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        probe_id = tree.widget(&probe);
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

    assert_eq!((one.nodes, one.erased_adapters), (1, 1));
    assert_eq!((hundred.nodes, hundred.erased_adapters), (100, 99));
    assert_eq!((scroll.nodes, scroll.erased_adapters), (25, 20));
}
