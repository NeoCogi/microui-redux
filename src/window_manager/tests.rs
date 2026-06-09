//! Tests for context root registration, visibility, image loading, and retained results.

use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};

use super::*;
use crate::{
    test_support::{test_atlas as make_test_atlas, test_atlas_with_font_sizes, NoopRenderer},
    widget_handle, AtlasHandle, Button, Combo, ListItem, Node, NodeId, NodeOptions, NodeStateValue, Policy, ResourceState, RetainedId, SizePolicy,
    ScrollBehavior, StackDirection, TextBlock, Widget, WidgetCtx, WidgetHandle, WidgetOption, UiNodeBuilder,
};

fn make_named_font_test_atlas() -> AtlasHandle {
    test_atlas_with_font_sizes(&[("small", 10), ("body", 12), ("title", 16)])
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    "<non-string panic payload>".to_string()
}

fn root_texts(ctx: &Context<NoopRenderer>, root: RootId) -> Vec<String> {
    ctx.debug_root_texts(root)
}

fn rendered_root_names(ctx: &Context<NoopRenderer>) -> Vec<String> {
    ctx.debug_rendered_root_names()
}

fn rect_key(rect: Option<Recti>) -> Option<(i32, i32, i32, i32)> {
    rect.map(|rect| (rect.x, rect.y, rect.width, rect.height))
}

fn chrome_key(
    chrome: (Option<Recti>, Option<Recti>, Option<Recti>),
) -> (Option<(i32, i32, i32, i32)>, Option<(i32, i32, i32, i32)>, Option<(i32, i32, i32, i32)>) {
    (rect_key(chrome.0), rect_key(chrome.1), rect_key(chrome.2))
}

struct AlwaysSubmitWidget {
    label: &'static str,
    opt: WidgetOption,
    scroll_behavior: ScrollBehavior,
}

impl AlwaysSubmitWidget {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }
}

impl Widget for AlwaysSubmitWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.scroll_behavior
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        Dimensioni::new((self.label.len() as i32 * 8).max(8), 12)
    }

    fn update(&mut self, _ctx: &mut WidgetCtx<'_>) -> ResourceState {
        ResourceState::SUBMIT
    }

    fn paint(&mut self, _ctx: &mut WidgetCtx<'_>) {}
}

#[test]
fn root_windows_do_not_render_scrollbars_for_overflow_content() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let text = widget_handle(TextBlock::new("a\na\na\na\na\na"));
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(text.clone());
    });
    let root = ctx.create_window("window", rect(0, 0, 60, 30), tree);

    ctx.update_ui();

    let body = ctx.debug_root_body(root).unwrap();
    let has_vertical_scrollbar = ctx
        .debug_root_rects(root)
        .unwrap()
        .iter()
        .any(|rect| rect.x == body.x + body.width && rect.width == style.scrollbar_size && rect.height > 0);
    assert!(!has_vertical_scrollbar);
}

#[test]
fn set_style_rebinds_default_font_fields_from_named_atlas() {
    let atlas = make_named_font_test_atlas();
    let body = atlas.font_id("body").unwrap();
    let small = atlas.font_id("small").unwrap();
    let title = atlas.font_id("title").unwrap();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));

    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    assert_eq!(ctx.style.font, body);
    assert_eq!(ctx.style.small_font, small);
    assert_eq!(ctx.style.title_font, title);
}

#[test]
fn resize_handle_wins_bottom_right_corner_over_window_scrollbars() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 240));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let text = widget_handle(TextBlock::new("aaaaaaaaaaaaaaaaaaaaaaaa\na\na\na\na\na\na\na"));
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(text.clone());
    });
    let root = ctx.create_window("window", rect(0, 0, 60, 40), tree);

    ctx.update_ui();
    ctx.update_ui();

    let initial_rect = ctx.root_rect(root).unwrap();
    let corner_x = initial_rect.x + initial_rect.width - 1;
    let corner_y = initial_rect.y + initial_rect.height - 1;

    ctx.mousemove(corner_x, corner_y);
    ctx.update_ui();

    ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
    ctx.update_ui();

    ctx.mousemove(corner_x + 12, corner_y + 10);
    ctx.update_ui();

    let resized = ctx.root_rect(root).unwrap();
    assert!(resized.width > initial_rect.width);
    assert!(resized.height > initial_rect.height);
}

#[test]
fn active_resize_updates_scroll_area_scrollbars_in_same_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 240));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let mut scroll_area = NodeId::default();
    let child = widget_handle(Button::new("child"));
    let tree = UiNodeBuilder::build(|tree| {
        scroll_area = tree
            .node(NodeOptions::with_policy(Policy::fill()))
            .scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.node(NodeOptions::with_policy(Policy::fixed(95, 200))).widget(child.clone());
            });
    });
    let root = ctx.create_window("window", rect(0, 0, 100, 100), tree);
    ctx.set_root_options(root, ContainerOption::NO_TITLE);

    ctx.update_ui();
    ctx.update_ui();

    assert!(ctx.scroll_area_content_size(root, scroll_area).unwrap().height > 0);

    let initial_rect = ctx.root_rect(root).unwrap();
    let corner_x = initial_rect.x + initial_rect.width - 1;
    let corner_y = initial_rect.y + initial_rect.height - 1;

    ctx.mousemove(corner_x, corner_y);
    ctx.update_ui();
    ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(corner_x + 10, corner_y);
    ctx.update_ui();

    let resized = ctx.root_rect(root).unwrap();
    assert!(resized.width > initial_rect.width);
    assert!(ctx.scroll_area_body(root, scroll_area).unwrap().width >= 95);
}

#[test]
fn title_drag_does_not_route_pointer_to_scroll_area() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(260, 240));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let mut scroll_area = NodeId::default();
    let child = widget_handle(Button::new("child"));
    let tree = UiNodeBuilder::build(|tree| {
        scroll_area = tree
            .node(NodeOptions::with_policy(Policy::fill()))
            .scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.node(NodeOptions::with_policy(Policy::fixed(95, 220))).widget(child.clone());
            });
    });
    let root = ctx.create_window("window", rect(20, 20, 120, 100), tree);

    ctx.update_ui();
    ctx.update_ui();

    let body = ctx.scroll_area_body(root, scroll_area).unwrap();
    let scrollbar_x = body.x + body.width + 1;
    let scrollbar_y = body.y + 6;
    ctx.mousemove(scrollbar_x, scrollbar_y);
    ctx.update_ui();
    ctx.mousedown(scrollbar_x, scrollbar_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(scrollbar_x, scrollbar_y + 10);
    ctx.update_ui();
    ctx.mouseup(scrollbar_x, scrollbar_y + 10, MouseButton::LEFT);
    ctx.update_ui();

    let scroll_after_scrollbar_drag = ctx.scroll_area_scroll(root, scroll_area).unwrap();
    assert!(scroll_after_scrollbar_drag.y > 0);

    let title_x = ctx.root_rect(root).unwrap().x + 10;
    let title_y = ctx.root_rect(root).unwrap().y + 6;
    ctx.mousemove(title_x, title_y);
    ctx.update_ui();
    ctx.mousedown(title_x, title_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(title_x + 18, title_y + 12);
    ctx.update_ui();

    assert_eq!(ctx.scroll_area_scroll(root, scroll_area).unwrap().y, scroll_after_scrollbar_drag.y);
    assert!(ctx.root_rect(root).unwrap().x > 20);
}

#[test]
fn resize_handle_geometry_matches_scrollbar_corner_size() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 240));
    let mut style = Style::default();
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let root = ctx.create_window(
        "window",
        rect(4, 6, 80, 50),
        UiNodeBuilder::build(|tree| {
            tree.text("body");
        }),
    );
    ctx.update_ui();

    let (_, _, resize) = ctx.debug_root_chrome(root).unwrap();
    let resize = resize.expect("resize chrome rect missing");
    let rect = ctx.root_rect(root).unwrap();
    assert_eq!(resize.width, style.scrollbar_size);
    assert_eq!(resize.height, style.scrollbar_size);
    assert_eq!(resize.x + resize.width, rect.x + rect.width);
    assert_eq!(resize.y + resize.height, rect.y + rect.height);
}

#[test]
fn context_result_accessors_expose_committed_and_current_generations() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let committed_id = NodeId::new(1);
    let current_id = NodeId::new(2);

    ctx.frame_results
        .record_node_with_context(RetainedId::node(committed_id), ResourceState::SUBMIT, "committed");
    ctx.frame_results.finish_frame();
    ctx.frame_results.begin_frame();
    ctx.frame_results
        .record_node_with_context(RetainedId::node(current_id), ResourceState::CHANGE, "current");

    assert!(ctx.committed_results().state_of_retained(RetainedId::node(committed_id)).is_submitted());
    assert!(ctx.committed_results().state_of_retained(RetainedId::node(current_id)).is_none());
    assert!(ctx.current_results().state_of_retained(RetainedId::node(committed_id)).is_none());
    assert!(ctx.current_results().state_of_retained(RetainedId::node(current_id)).is_changed());
}

#[test]
fn closing_window_resets_transient_render_state() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let root = ctx.create_window("window", rect(0, 0, 80, 40), UiNodeSet::default());

    ctx.update_ui();
    ctx.set_root_visible(root, false);
    ctx.update_ui();

    assert_eq!(ctx.root_visible(root), Some(false));
    assert!(rendered_root_names(&ctx).is_empty());
}

#[test]
fn reshown_windows_prepare_on_first_render_after_a_gap() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = UiNodeBuilder::build(|tree| {
        tree.text("hello");
    });
    let root = ctx.create_window("window", rect(20, 20, 80, 40), tree);

    ctx.update_ui();
    ctx.set_root_visible(root, false);
    ctx.update_ui();

    ctx.set_root_visible(root, true);
    ctx.update_ui();

    assert_eq!(ctx.root_visible(root), Some(true));
    assert!(ctx.debug_root_content_size(root).unwrap().height > 0);
}

#[test]
fn reopening_dialog_replaces_old_commands_with_current_frame_commands() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let first_tree = UiNodeBuilder::build(|tree| {
        tree.text("before");
    });
    let second_tree = UiNodeBuilder::build(|tree| {
        tree.text("after");
    });
    let opt = ContainerOption::NO_TITLE | ContainerOption::NO_CLOSE | ContainerOption::NO_RESIZE;
    let root = ctx.create_dialog("dialog", rect(10, 10, 80, 40), first_tree);
    ctx.set_root_options(root, opt);

    ctx.set_root_visible(root, true);
    ctx.update_ui();

    ctx.set_root_visible(root, false);
    ctx.update_ui();

    ctx.set_root_nodes(root, second_tree);
    ctx.set_root_visible(root, true);
    ctx.update_ui();

    let texts = root_texts(&ctx, root);

    assert!(texts.iter().any(|text| text == "after"));
    assert!(!texts.iter().any(|text| text == "before"));
}

#[test]
fn open_dialog_does_not_bump_zindex_every_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let background_tree = UiNodeBuilder::build(|tree| {
        tree.text("background");
    });
    let dialog_tree = UiNodeBuilder::build(|tree| {
        tree.text("dialog");
    });
    let background = ctx.create_window("background", rect(0, 0, 100, 80), background_tree);
    let dialog = ctx.create_dialog("dialog", rect(10, 10, 80, 40), dialog_tree);

    ctx.set_root_visible(dialog, true);
    ctx.update_ui();

    let first_zindex = ctx.debug_root_zindex(dialog).unwrap();
    assert!(first_zindex > ctx.debug_root_zindex(background).unwrap());

    ctx.update_ui();

    assert_eq!(ctx.debug_root_zindex(dialog).unwrap(), first_zindex);
}

#[test]
fn reshown_roots_drop_stale_scroll_area_state_after_a_gap() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let mut scroll_area = NodeId::default();
    let tree_with_scroll_area = UiNodeBuilder::build(|tree| {
        scroll_area = tree.scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
            tree.text("scroll area child");
        });
    });
    let tree_without_scroll_area = UiNodeBuilder::build(|tree| {
        tree.text("root only");
    });
    let root = ctx.create_window("window", rect(0, 0, 100, 80), tree_with_scroll_area);

    ctx.update_ui();
    assert!(ctx.scroll_area_content_size(root, scroll_area).unwrap().height > 0);

    ctx.set_root_visible(root, false);
    ctx.update_ui();

    ctx.set_root_nodes(root, tree_without_scroll_area);
    ctx.set_root_visible(root, true);
    ctx.update_ui();

    assert!(ctx.debug_root_content_size(root).unwrap().height > 0);
}

#[test]
fn scroll_area_node_renders_scroll_area_node() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let mut scroll_area = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        scroll_area = tree.scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
            tree.text("scroll area child");
        });
    });
    let root = ctx.create_window("window", rect(0, 0, 100, 80), tree);

    ctx.update_ui();

    assert!(ctx.scroll_area_content_size(root, scroll_area).unwrap().height > 0);
}

#[test]
fn scroll_area_paints_disclosure_headers_in_screen_space() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 160));
    let header = widget_handle(Node::header("Visible Header", NodeStateValue::Expanded));
    let tree_node = widget_handle(Node::tree("Visible Tree", NodeStateValue::Expanded));
    let tree = UiNodeBuilder::build(|tree| {
        tree.node(NodeOptions::with_policy(Policy::fixed(180, 100)))
            .scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.header(&header, |tree| {
                    tree.tree_node(&tree_node, |tree| {
                        tree.text("Visible Child");
                    });
                });
            });
    });
    let root = ctx.create_window("window", rect(0, 0, 200, 130), tree);
    ctx.set_root_options(root, ContainerOption::NO_TITLE);

    ctx.update_ui();

    let texts = root_texts(&ctx, root);
    assert!(texts.iter().any(|text| text == "Visible Header"), "{texts:?}");
    assert!(texts.iter().any(|text| text == "Visible Tree"), "{texts:?}");
}

#[test]
fn newly_opened_popup_auto_sizes_on_first_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = UiNodeBuilder::build(|tree| {
        tree.text("hello popup");
    });
    let popup = ctx.create_popup("popup", tree);

    ctx.mousemove(40, 50);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();

    let rect = ctx.root_rect(popup).unwrap();
    let body = ctx.debug_root_body(popup).unwrap();
    assert!(rect.width > 1);
    assert!(rect.height > 1);
    assert!(body.width > 0);
    assert!(body.height > 0);
}

#[test]
fn auto_sized_titled_window_uses_current_frame_content_size() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = UiNodeBuilder::build(|tree| {
        tree.text("hello\nhello\nhello");
    });
    let root = ctx.create_window("window", rect(0, 0, 1, 1), tree);
    ctx.set_root_options(root, ContainerOption::AUTO_SIZE);

    ctx.update_ui();

    let rect = ctx.root_rect(root).unwrap();
    let body = ctx.debug_root_body(root).unwrap();
    assert!(rect.width > 1);
    assert!(rect.height > 1);
    assert!(body.y > rect.y);
    assert!(body.height > 0);
}

#[test]
fn popup_content_changes_resize_without_a_frame_lag() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let short_tree = UiNodeBuilder::build(|tree| {
        tree.text("a");
    });
    let long_tree = UiNodeBuilder::build(|tree| {
        tree.text("hello popup with much longer content");
    });
    let popup = ctx.create_popup("popup", short_tree);

    ctx.mousemove(60, 60);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();
    let first_width = ctx.root_rect(popup).unwrap().width;

    ctx.set_root_nodes(popup, long_tree);
    ctx.update_ui();

    assert!(ctx.root_rect(popup).unwrap().width > first_width);
}

#[test]
fn auto_sized_titled_window_body_fits_current_content_same_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);
    let tree = UiNodeBuilder::build(|tree| {
        tree.text("hello\nhello\nhello\nhello");
    });
    let root = ctx.create_window("window", rect(0, 0, 1, 1), tree);
    ctx.set_root_options(root, ContainerOption::AUTO_SIZE);

    ctx.update_ui();

    let body = ctx.debug_root_body(root).unwrap();
    let content = ctx.debug_root_content_size(root).unwrap();
    let rect = ctx.root_rect(root).unwrap();
    assert!(body.width >= content.width);
    assert!(body.height >= content.height);
    assert!(body.height > 0);
    assert!(body.y > rect.y);
}

#[test]
fn title_option_controls_root_window_title_bar_geometry() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let titled = ctx.create_window("titled", rect(0, 0, 80, 40), UiNodeBuilder::build(|_tree| {}));
    let plain = ctx.create_window("plain", rect(100, 0, 80, 40), UiNodeBuilder::build(|_tree| {}));
    ctx.set_root_options(plain, ContainerOption::NO_TITLE);
    ctx.update_ui();

    let titled_rect = ctx.root_rect(titled).unwrap();
    let titled_body = ctx.debug_root_body(titled).unwrap();
    assert!(titled_body.y > titled_rect.y);
    assert!(titled_body.height < titled_rect.height);

    let plain_rect = ctx.root_rect(plain).unwrap();
    let plain_body = ctx.debug_root_body(plain).unwrap();
    assert_eq!(plain_body.y, plain_rect.y);
    assert_eq!(plain_body.height, plain_rect.height);
    assert_eq!(plain_body.x, plain_rect.x);
    assert_eq!(plain_body.width, plain_rect.width);
    let plain_texts = root_texts(&ctx, plain);
    assert!(!plain_texts.iter().any(|text| text == "plain"));
}

#[test]
fn duplicate_widget_dispatch_in_same_tree_panics_with_context() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let shared = widget_handle(AlwaysSubmitWidget::new("shared"));
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(shared.clone());
        tree.widget(shared.clone());
    });
    ctx.create_window("primary", rect(0, 0, 80, 40), tree);

    let panic = catch_unwind(AssertUnwindSafe(|| {
        ctx.update_ui();
    }))
    .expect_err("duplicate widget handle should panic");
    let message = panic_message(panic);

    assert!(message.contains("duplicate widget dispatch"));
    assert!(message.contains("WidgetHandle"));
    assert!(message.contains("primary"));
    assert!(message.contains("ui node"));
}

#[test]
fn duplicate_widget_dispatch_across_windows_panics() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let shared = widget_handle(AlwaysSubmitWidget::new("shared"));
    let left_tree = UiNodeBuilder::build(|tree| {
        tree.widget(shared.clone());
    });
    let right_tree = UiNodeBuilder::build(|tree| {
        tree.widget(shared.clone());
    });
    ctx.create_window("left", rect(0, 0, 80, 40), left_tree);
    ctx.create_window("right", rect(90, 0, 80, 40), right_tree);

    let panic = catch_unwind(AssertUnwindSafe(|| {
        ctx.update_ui();
    }))
    .expect_err("rendering one widget handle in two windows should panic");
    let message = panic_message(panic);

    assert!(message.contains("duplicate widget dispatch"));
    assert!(message.contains("left"));
    assert!(message.contains("right"));
}

#[test]
fn distinct_widget_handles_with_identical_labels_render_normally() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let first = widget_handle(AlwaysSubmitWidget::new("same"));
    let second = widget_handle(AlwaysSubmitWidget::new("same"));
    let mut first_id = NodeId::default();
    let mut second_id = NodeId::default();
    let tree = UiNodeBuilder::build(|tree| {
        first_id = tree.widget(first.clone());
        second_id = tree.widget(second.clone());
    });

    let root = ctx.create_window("window", rect(0, 0, 80, 40), tree);
    ctx.update_ui();

    assert!(ctx.committed_results().state_of_retained(RetainedId::root_node(root, first_id)).is_submitted());
    assert!(ctx.committed_results().state_of_retained(RetainedId::root_node(root, second_id)).is_submitted());
}

#[test]
fn registered_window_renders_across_frames_without_resubmission() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let text = widget_handle(TextBlock::new("before"));
    let tree = UiNodeBuilder::build({
        let text = text.clone();
        move |tree| {
            tree.widget(text.clone());
        }
    });
    let root = ctx.create_window("retained", rect(0, 0, 90, 50), tree);

    ctx.update_ui();
    assert!(root_texts(&ctx, root).iter().any(|text| text == "before"));

    text.update(|text| {
        text.text = "after".to_string();
    });
    ctx.update_ui();
    let texts = root_texts(&ctx, root);
    assert!(texts.iter().any(|text| text == "after"));
    assert!(!texts.iter().any(|text| text == "before"));
}

#[test]
fn retained_root_visibility_controls_rendering() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = UiNodeBuilder::build(|tree| {
        tree.text("visible");
    });
    let root = ctx.create_window("retained", rect(0, 0, 90, 50), tree);

    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["retained"]);

    ctx.set_root_visible(root, false);
    ctx.update_ui();
    assert!(rendered_root_names(&ctx).is_empty());
    assert_eq!(ctx.root_visible(root), Some(false));

    ctx.set_root_visible(root, true);
    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["retained"]);
    assert!(root_texts(&ctx, root).iter().any(|text| text == "visible"));
}

#[test]
fn retained_root_nodes_can_be_replaced_after_registration() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let first_tree = UiNodeBuilder::build(|tree| {
        tree.text("first");
    });
    let second_tree = UiNodeBuilder::build(|tree| {
        tree.text("second");
    });
    let root = ctx.create_window("retained", rect(0, 0, 90, 50), first_tree);

    ctx.update_ui();
    assert!(root_texts(&ctx, root).iter().any(|text| text == "first"));

    ctx.set_root_nodes(root, second_tree);
    ctx.update_ui();
    let texts = root_texts(&ctx, root);
    assert!(texts.iter().any(|text| text == "second"));
    assert!(!texts.iter().any(|text| text == "first"));
}

#[test]
fn root_ids_preserve_z_order_and_fronting() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let left_tree = UiNodeBuilder::build(|tree| {
        tree.text("left");
    });
    let right_tree = UiNodeBuilder::build(|tree| {
        tree.text("right");
    });
    let left = ctx.create_window("left", rect(0, 0, 90, 60), left_tree);
    let _right = ctx.create_window("right", rect(20, 0, 90, 60), right_tree);

    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["left", "right"]);

    ctx.bring_root_to_front(left);
    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["right", "left"]);
}

#[test]
fn retained_dialog_becomes_front_root_when_shown() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let background = ctx.create_window(
        "background",
        rect(0, 0, 120, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("background");
        }),
    );
    let dialog = ctx.create_dialog(
        "dialog",
        rect(10, 10, 90, 50),
        UiNodeBuilder::build(|tree| {
            tree.text("dialog");
        }),
    );

    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["background"]);

    ctx.set_root_visible(dialog, true);
    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["background", "dialog"]);
    assert!(ctx.debug_root_zindex(dialog).unwrap() > ctx.debug_root_zindex(background).unwrap());
}

#[test]
fn retained_popup_opens_at_mouse_and_auto_sizes_on_first_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup = ctx.create_popup(
        "popup",
        UiNodeBuilder::build(|tree| {
            tree.text("hello popup");
        }),
    );

    ctx.mousemove(40, 50);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();

    let rect = ctx.root_rect(popup).unwrap();
    assert_eq!(rect.x, 40);
    assert_eq!(rect.y, 50);
    assert!(rect.width > 1);
    assert!(rect.height > 1);
    assert_eq!(rendered_root_names(&ctx), vec!["popup"]);
}

#[test]
fn retained_root_hover_selection_uses_registered_root_z_order() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let left = ctx.create_window(
        "left",
        rect(0, 0, 100, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("left");
        }),
    );
    let _right = ctx.create_window(
        "right",
        rect(0, 0, 100, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("right");
        }),
    );

    ctx.mousemove(20, 20);
    ctx.update_ui();
    let left_entry = ctx.roots.iter().find(|entry| entry.id == left).unwrap();
    assert!(!left_entry.runtime.accepts_pointer_input());

    ctx.bring_root_to_front(left);
    ctx.update_ui();
    let left_entry = ctx.roots.iter().find(|entry| entry.id == left).unwrap();
    assert!(left_entry.runtime.accepts_pointer_input());
}

#[test]
fn root_hover_selection_uses_root_z_order() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let left_button = widget_handle(Button::new("left"));
    let right_button = widget_handle(Button::new("right"));
    let left = ctx.create_window(
        "left",
        rect(0, 0, 120, 80),
        UiNodeBuilder::build(|tree| {
            tree.widget(left_button.clone());
        }),
    );
    let right = ctx.create_window(
        "right",
        rect(40, 0, 120, 80),
        UiNodeBuilder::build(|tree| {
            tree.widget(right_button.clone());
        }),
    );

    ctx.mousemove(50, 30);
    ctx.update_ui();

    let left_entry = ctx.roots.iter().find(|entry| entry.id == left).unwrap();
    let right_entry = ctx.roots.iter().find(|entry| entry.id == right).unwrap();
    assert!(!left_entry.runtime.accepts_pointer_input());
    assert!(right_entry.runtime.accepts_pointer_input());

    ctx.mousedown(20, 30, MouseButton::LEFT);
    ctx.update_ui();

    let left_entry = ctx.roots.iter().find(|entry| entry.id == left).unwrap();
    let right_entry = ctx.roots.iter().find(|entry| entry.id == right).unwrap();
    assert!(left_entry.runtime.accepts_pointer_input());
    assert!(!right_entry.runtime.accepts_pointer_input());
    assert!(left_entry.z_index > right_entry.z_index);
}

#[test]
fn root_title_drag_moves_window() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 160));
    let root = ctx.create_window(
        "node",
        rect(10, 10, 120, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("body");
        }),
    );

    ctx.mousemove(30, 18);
    ctx.update_ui();
    ctx.mousedown(30, 18, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(45, 28);
    ctx.update_ui();

    let moved = ctx.root_rect(root).unwrap();
    assert_eq!(moved.x, 25);
    assert_eq!(moved.y, 20);
}

#[test]
fn root_resize_handle_resizes_window() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 160));
    let root = ctx.create_window(
        "node",
        rect(10, 10, 120, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("body");
        }),
    );

    ctx.mousemove(126, 86);
    ctx.update_ui();
    ctx.mousedown(126, 86, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(146, 101);
    ctx.update_ui();

    let resized = ctx.root_rect(root).unwrap();
    assert_eq!(resized.width, 140);
    assert_eq!(resized.height, 95);
}

#[test]
fn root_close_button_hides_window() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 160));
    let root = ctx.create_window(
        "node",
        rect(10, 10, 120, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("body");
        }),
    );

    ctx.mousemove(118, 18);
    ctx.update_ui();
    ctx.mousedown(118, 18, MouseButton::LEFT);
    ctx.update_ui();

    assert_eq!(ctx.root_visible(root), Some(false));
}

#[test]
fn retained_chrome_node_ids_are_root_derived_and_stable() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "retained",
        rect(0, 0, 100, 70),
        UiNodeBuilder::build(|tree| {
            tree.text("before");
        }),
    );
    let other = ctx.create_window(
        "other",
        rect(120, 0, 100, 70),
        UiNodeBuilder::build(|tree| {
            tree.text("other");
        }),
    );

    let chrome = chrome_key(ctx.debug_root_chrome(root).unwrap());
    assert_ne!(chrome, chrome_key(ctx.debug_root_chrome(other).unwrap()));

    ctx.update_ui();
    ctx.set_root_nodes(
        root,
        UiNodeBuilder::build(|tree| {
            tree.text("after");
        }),
    );
    ctx.update_ui();

    assert_eq!(chrome_key(ctx.debug_root_chrome(root).unwrap()), chrome);
}

#[test]
fn retained_chrome_nodes_are_recorded_in_root_cache() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "Typography Demo Window Title",
        rect(10, 12, 20, 40),
        UiNodeBuilder::build(|tree| {
            tree.text("body");
        }),
    );
    ctx.update_ui();

    let (title, close, resize) = ctx.debug_root_chrome(root).unwrap();
    let title = title.expect("title chrome rect missing");
    let close = close.expect("close chrome rect missing");
    let resize = resize.expect("resize chrome rect missing");
    let root_rect = ctx.root_rect(root).unwrap();

    assert_eq!(title.x, 10);
    assert_eq!(title.y, 12);
    assert_eq!(title.width, root_rect.width);
    assert!(title.width > 96);
    assert!(title.height > 0);
    assert!(close.x >= title.x);
    assert!(close.x > title.x + title.width / 2);
    assert!(resize.x >= title.x);
    assert!(resize.y >= title.y);
}

#[test]
fn retained_title_drag_uses_chrome_node_after_tree_update() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "retained",
        rect(10, 10, 100, 70),
        UiNodeBuilder::build(|tree| {
            tree.text("before");
        }),
    );
    ctx.update_ui();
    ctx.set_root_nodes(
        root,
        UiNodeBuilder::build(|tree| {
            tree.text("after");
        }),
    );
    ctx.update_ui();

    let chrome = chrome_key(ctx.debug_root_chrome(root).unwrap());
    let initial = ctx.root_rect(root).unwrap();
    let title_x = initial.x + 10;
    let title_y = initial.y + 6;
    ctx.mousemove(title_x, title_y);
    ctx.update_ui();
    ctx.update_ui();
    ctx.mousedown(title_x, title_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(title_x + 12, title_y + 7);
    ctx.update_ui();

    let moved = ctx.root_rect(root).unwrap();
    assert!(moved.x > initial.x);
    assert!(moved.y > initial.y);
    assert_ne!(chrome_key(ctx.debug_root_chrome(root).unwrap()), chrome);
}

#[test]
fn retained_close_button_closes_root_and_records_chrome_result() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "retained",
        rect(10, 10, 100, 70),
        UiNodeBuilder::build(|tree| {
            tree.text("body");
        }),
    );
    let close_x = 10 + 100 - 12;
    let close_y = 10 + 6;
    ctx.mousemove(close_x, close_y);
    ctx.update_ui();
    ctx.update_ui();
    assert_eq!(ctx.root_visible(root), Some(true));

    ctx.mousedown(close_x, close_y, MouseButton::LEFT);
    ctx.update_ui();
    assert_eq!(ctx.root_visible(root), Some(false));

    ctx.mouseup(close_x, close_y, MouseButton::LEFT);
    ctx.update_ui();
    assert!(rendered_root_names(&ctx).is_empty());
}

#[test]
fn retained_resize_handle_wins_bottom_right_corner_over_window_scrollbars() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 240));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let text = widget_handle(TextBlock::new("aaaaaaaaaaaaaaaaaaaaaaaa\na\na\na\na\na\na\na"));
    let tree = UiNodeBuilder::build({
        let text = text.clone();
        move |tree| {
            tree.widget(text.clone());
        }
    });
    let root = ctx.create_window("retained", rect(0, 0, 60, 40), tree);
    ctx.update_ui();
    ctx.update_ui();

    let initial_rect = ctx.root_rect(root).unwrap();
    let corner_x = initial_rect.x + initial_rect.width - 1;
    let corner_y = initial_rect.y + initial_rect.height - 1;

    ctx.mousemove(corner_x, corner_y);
    ctx.update_ui();
    ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(corner_x + 12, corner_y + 10);
    ctx.update_ui();

    let resized = ctx.root_rect(root).unwrap();
    assert!(resized.width > initial_rect.width);
    assert!(resized.height > initial_rect.height);
}

#[test]
fn retained_popup_closes_from_root_state_when_clicking_outside() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup = ctx.create_popup(
        "popup",
        UiNodeBuilder::build(|tree| {
            tree.text("popup");
        }),
    );

    ctx.mousemove(20, 20);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();
    assert_eq!(ctx.root_visible(popup), Some(true));

    ctx.mousemove(200, 100);
    ctx.update_ui();
    ctx.mousedown(200, 100, MouseButton::LEFT);
    ctx.update_ui();

    assert_eq!(ctx.root_visible(popup), Some(false));
    assert!(rendered_root_names(&ctx).is_empty());
}

fn run_combo_frame(
    ctx: &mut Context<NoopRenderer>,
    popup_root: RootId,
    combo: &WidgetHandle<Combo>,
    items: &[WidgetHandle<ListItem>; 2],
    item_ids: &[NodeId; 2],
) -> Option<String> {
    let labels: Vec<String> = items.iter().map(|item| item.read(|item| item.label.clone())).collect();
    combo.update(|combo| combo.update_items(&labels));
    let combo_anchor = combo.read(Combo::anchor);

    let mut selected_label = None;
    let results = ctx.committed_results();
    for (idx, node_id) in item_ids.iter().enumerate() {
        if results.state_of_retained(RetainedId::root_node(popup_root, *node_id)).is_submitted() {
            selected_label = combo.update(|combo| combo.select(idx, &labels));
            break;
        }
    }

    if combo.read(Combo::is_open) {
        ctx.set_root_visible(popup_root, true);
        ctx.set_root_rect(popup_root, combo_anchor);
    } else {
        ctx.set_root_visible(popup_root, false);
    }

    ctx.update_ui();
    selected_label
}

#[test]
fn retained_combo_popup_stays_closed_after_mouse_selection() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup_root = ctx.create_popup("combo popup", UiNodeSet::default());
    ctx.set_root_options(popup_root, ContainerOption::AUTO_SIZE | ContainerOption::NO_RESIZE | ContainerOption::NO_TITLE);
    let combo = widget_handle(Combo::new());
    let items = [widget_handle(ListItem::new("Apple")), widget_handle(ListItem::new("Banana"))];
    let mut item_ids = [NodeId::default(); 2];
    let main_root = ctx.create_window(
        "combo window",
        rect(0, 0, 120, 80),
        UiNodeBuilder::build({
            let combo = combo.clone();
            move |tree| {
                tree.row(&[SizePolicy::Fixed(80)], SizePolicy::Auto, |tree| {
                    tree.widget(combo.clone());
                });
            }
        }),
    );
    ctx.set_root_options(main_root, ContainerOption::NO_TITLE | ContainerOption::NO_RESIZE);
    let popup_items = items.clone();
    ctx.set_root_nodes(
        popup_root,
        UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                for (index, item) in popup_items.iter().enumerate() {
                    item_ids[index] = tree.widget(item.clone());
                }
            });
        }),
    );

    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    ctx.mousemove(10, 10);
    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    ctx.mousedown(10, 10, MouseButton::LEFT);
    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    assert!(combo.read(Combo::is_open));
    assert_eq!(ctx.root_visible(popup_root), Some(true));

    let popup_rect = ctx.root_rect(popup_root).unwrap();
    assert!(popup_rect.width < 200);
    let item_x = popup_rect.x + 12;
    let item_y = popup_rect.y + 12;
    ctx.mousemove(item_x, item_y);
    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    ctx.mousedown(item_x, item_y, MouseButton::LEFT);
    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    ctx.mouseup(item_x, item_y, MouseButton::LEFT);
    let selected = run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);

    assert_eq!(selected.as_deref(), Some("Apple"));
    assert!(!combo.read(Combo::is_open));
    assert_eq!(ctx.root_visible(popup_root), Some(false));

    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    assert!(!combo.read(Combo::is_open));
    assert_eq!(ctx.root_visible(popup_root), Some(false));
}

#[test]
fn node_popup_auto_size_is_stable_with_remainder_stack() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup = ctx.create_popup(
        "popup",
        UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                tree.text("Apple");
                tree.text("Banana");
            });
        }),
    );
    ctx.set_root_rect(popup, rect(20, 20, 80, 1));
    ctx.set_root_visible(popup, true);

    ctx.update_ui();
    let first = ctx.root_rect(popup).unwrap();
    ctx.update_ui();
    let second = ctx.root_rect(popup).unwrap();
    ctx.update_ui();
    let third = ctx.root_rect(popup).unwrap();

    assert_eq!(second.width, first.width);
    assert_eq!(third.width, first.width);
    assert_eq!(second.height, first.height);
    assert_eq!(third.height, first.height);
}

#[test]
fn node_popup_auto_size_fits_stacked_buttons() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let hello = widget_handle(Button::with_opt("Hello", WidgetOption::ALIGN_CENTER));
    let world = widget_handle(Button::with_opt("World", WidgetOption::ALIGN_CENTER));
    let mut hello_id = NodeId::default();
    let mut world_id = NodeId::default();
    let popup = ctx.create_popup(
        "popup",
        UiNodeBuilder::build(|tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                hello_id = tree.widget(hello.clone());
                world_id = tree.widget(world.clone());
            });
        }),
    );
    ctx.set_root_rect(popup, rect(20, 20, 80, 1));
    ctx.set_root_visible(popup, true);

    ctx.update_ui();

    let body = ctx.debug_root_body(popup).unwrap();
    let hello_rect = ctx.debug_root_node_rect(popup, hello_id).unwrap();
    let world_rect = ctx.debug_root_node_rect(popup, world_id).unwrap();
    assert_eq!(body.y + body.height, world_rect.y + world_rect.height + ctx.style.padding);
    assert_eq!(hello_rect.y + hello_rect.height + ctx.style.spacing, world_rect.y);
}

#[test]
fn node_popup_closes_when_clicking_outside() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup = ctx.create_popup(
        "popup",
        UiNodeBuilder::build(|tree| {
            tree.text("popup");
        }),
    );
    ctx.set_root_rect(popup, rect(20, 20, 80, 1));
    ctx.set_root_visible(popup, true);

    ctx.update_ui();
    assert_eq!(ctx.root_visible(popup), Some(true));

    ctx.mousemove(200, 100);
    ctx.update_ui();
    ctx.mousedown(200, 100, MouseButton::LEFT);
    ctx.update_ui();

    assert_eq!(ctx.root_visible(popup), Some(false));
}

#[test]
fn node_popup_closes_when_clicking_another_node_window() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let window = ctx.create_window(
        "window",
        rect(120, 10, 90, 80),
        UiNodeBuilder::build(|tree| {
            tree.text("window");
        }),
    );
    let popup = ctx.create_popup(
        "popup",
        UiNodeBuilder::build(|tree| {
            tree.text("popup");
        }),
    );
    ctx.set_root_rect(popup, rect(20, 20, 80, 1));
    ctx.set_root_visible(popup, true);

    ctx.update_ui();
    assert_eq!(ctx.root_visible(popup), Some(true));

    let target = ctx.root_rect(window).unwrap();
    ctx.mousemove(target.x + 10, target.y + 10);
    ctx.update_ui();
    ctx.mousedown(target.x + 10, target.y + 10, MouseButton::LEFT);
    ctx.update_ui();

    assert_eq!(ctx.root_visible(popup), Some(false));
}

#[test]
fn node_scroll_area_consumes_wheel_without_root_scroll_fallback() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 160));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let mut scroll_area = NodeId::default();
    let inner = widget_handle(Button::new("inner"));
    let bottom = widget_handle(Button::new("bottom"));
    let tree = UiNodeBuilder::build(|tree| {
        scroll_area = tree
            .node(NodeOptions::with_policy(Policy::fixed(90, 40)))
            .scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.node(NodeOptions::with_policy(Policy::fixed(180, 140))).widget(inner.clone());
            });
        tree.node(NodeOptions::with_policy(Policy::fixed(90, 180))).widget(bottom.clone());
    });
    let root = ctx.create_window("window", rect(0, 0, 110, 90), tree);
    ctx.set_root_options(root, ContainerOption::NO_TITLE);

    ctx.update_ui();
    ctx.update_ui();

    ctx.mousemove(10, 10);
    ctx.update_ui();
    ctx.scroll(0, -24);
    ctx.update_ui();

    let nested_scroll = ctx.scroll_area_scroll(root, scroll_area).unwrap();
    assert!(nested_scroll.y > 0);
    let body = ctx.scroll_area_body(root, scroll_area).unwrap();
    ctx.mousemove(body.x + 2, body.y + body.height + 2);
    ctx.update_ui();
    ctx.scroll(-24, 0);
    ctx.update_ui();

    let scroll = ctx.scroll_area_scroll(root, scroll_area).unwrap();
    assert!(scroll.x > 0);
    assert!(scroll.y > 0);
}

#[test]
fn node_scroll_area_internal_overflow_does_not_expand_root_content() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 160));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let mut scroll_area = NodeId::default();
    let inner = widget_handle(Button::new("inner"));
    let tree = UiNodeBuilder::build(|tree| {
        scroll_area = tree
            .node(NodeOptions::with_policy(Policy::fixed(90, 40)))
            .scroll_area(ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.node(NodeOptions::with_policy(Policy::fixed(80, 140))).widget(inner.clone());
            });
    });
    let root = ctx.create_window("window", rect(0, 0, 110, 90), tree);
    ctx.set_root_options(root, ContainerOption::NO_TITLE);

    ctx.update_ui();
    ctx.update_ui();

    let root_entry = ctx.roots.iter().find(|entry| entry.id == root).unwrap();
    let root_node = root_entry.roots.first().unwrap();
    assert!(ctx.scroll_area_content_size(root, scroll_area).unwrap().height > ctx.scroll_area_body(root, scroll_area).unwrap().height);
    assert!(root_node.layout.content_size.height <= root_node.layout.frame.height);
}
