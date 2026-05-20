use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};

use super::*;
use crate::{
    container::Command,
    test_support::{test_atlas as make_test_atlas, test_atlas_with_font_sizes, NoopRenderer},
    widget_handle, AtlasHandle, Combo, ControlState, ListItem, NodeId, ResourceState, RetainedId, SizePolicy, StackDirection, TextBlock, Widget, WidgetCtx,
    WidgetHandle, WidgetOption, WidgetTreeBuilder,
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

fn window_texts(window: &WindowHandle) -> Vec<String> {
    window
        .inner()
        .main
        .debug_commands()
        .iter()
        .filter_map(|cmd| match cmd {
            Command::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn rendered_root_names(ctx: &Context<NoopRenderer>) -> Vec<String> {
    ctx.root_list.iter().map(|window| window.inner().main.name().to_string()).collect()
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

    fn update(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::SUBMIT
    }

    fn paint(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) {}
}

#[test]
fn root_windows_render_scrollbars_after_content_size_is_known() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    ctx.set_style(&style);

    let text = widget_handle(TextBlock::new("a\na\na\na\na\na"));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.widget(text.clone());
    });
    let root = ctx.create_window("window", rect(0, 0, 60, 30), tree);

    ctx.update_ui();
    ctx.update_ui();

    let window = ctx.root_handle(root).unwrap();
    let inner = window.inner();
    let body = inner.main.body();
    let has_vertical_scrollbar = inner
        .main
        .debug_commands()
        .iter()
        .any(|cmd| matches!(cmd, Command::Recti { rect, .. } if rect.x == body.x + body.width && rect.width == style.scrollbar_size && rect.height > 0));

    assert!(has_vertical_scrollbar);
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
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.widget(text.clone());
    });
    let root = ctx.create_window("window", rect(0, 0, 60, 40), tree);

    ctx.update_ui();
    ctx.update_ui();

    let window = ctx.root_handle(root).unwrap();
    let initial_rect = window.rect();
    let corner_x = initial_rect.x + initial_rect.width - 1;
    let corner_y = initial_rect.y + initial_rect.height - 1;

    ctx.mousemove(corner_x, corner_y);
    ctx.update_ui();

    ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
    ctx.update_ui();

    ctx.mousemove(corner_x + 12, corner_y + 10);
    ctx.update_ui();

    let resized = ctx.root_handle(root).unwrap().rect();
    assert!(resized.width > initial_rect.width);
    assert!(resized.height > initial_rect.height);
}

#[test]
fn context_result_accessors_expose_committed_and_current_generations() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let committed_id = NodeId::new(1);
    let current_id = NodeId::new(2);

    ctx.frame_results
        .record_node_with_context(RetainedId::node(committed_id), committed_id, ResourceState::SUBMIT, "committed");
    ctx.frame_results.finish_frame();
    ctx.frame_results.begin_frame();
    ctx.frame_results
        .record_node_with_context(RetainedId::node(current_id), current_id, ResourceState::CHANGE, "current");

    assert!(ctx.committed_results().state_of_node(committed_id).is_submitted());
    assert!(ctx.committed_results().state_of_node(current_id).is_none());
    assert!(ctx.current_results().state_of_node(committed_id).is_none());
    assert!(ctx.current_results().state_of_node(current_id).is_changed());
}

#[test]
fn closing_window_resets_transient_render_state() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let root = ctx.create_window("window", rect(0, 0, 80, 40), WidgetTree::default());
    let mut window = ctx.root_handle(root).unwrap();

    {
        let mut inner = window.inner_mut();
        inner.main.debug_push_command(Command::None);
        inner.main.debug_push_clip(UNCLIPPED_RECT);
        inner.main.set_content_size(Dimensioni::new(11, 17));
        inner.main.set_scroll(Vec2i::new(3, 5));
    }

    window.close();

    let inner = window.inner();
    assert!(inner.main.debug_commands().is_empty());
    assert_eq!(inner.main.content_size().width, 0);
    assert_eq!(inner.main.content_size().height, 0);
    assert_eq!(inner.main.scroll().x, 0);
    assert_eq!(inner.main.scroll().y, 0);
}

#[test]
fn reshown_windows_prepare_on_first_render_after_a_gap() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.text("hello");
    });
    let root = ctx.create_window("window", rect(20, 20, 80, 40), tree);

    ctx.update_ui();
    ctx.set_root_visible(root, false);
    ctx.update_ui();

    {
        let mut window = ctx.root_handle(root).unwrap();
        let mut inner = window.inner_mut();
        inner.main.debug_push_command(Command::None);
    }

    ctx.set_root_visible(root, true);
    ctx.update_ui();

    let window = ctx.root_handle(root).unwrap();
    let inner = window.inner();
    assert!(!inner.main.debug_commands().iter().any(|cmd| matches!(cmd, Command::None)));
}

#[test]
fn reopening_dialog_replaces_old_commands_with_current_frame_commands() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let first_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("before");
    });
    let second_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("after");
    });
    let opt = ContainerOption::NO_TITLE | ContainerOption::NO_CLOSE | ContainerOption::NO_RESIZE;
    let root = ctx.create_dialog("dialog", rect(10, 10, 80, 40), first_tree);
    ctx.set_root_options(root, opt, ScrollBehavior::NONE);

    ctx.set_root_visible(root, true);
    ctx.update_ui();

    ctx.set_root_visible(root, false);
    ctx.update_ui();

    ctx.set_root_tree(root, second_tree);
    ctx.set_root_visible(root, true);
    ctx.update_ui();

    let dialog = ctx.root_handle(root).unwrap();
    let inner = dialog.inner();
    let texts: Vec<String> = inner
        .main
        .debug_commands()
        .iter()
        .filter_map(|cmd| match cmd {
            Command::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();

    assert!(texts.iter().any(|text| text == "after"));
    assert!(!texts.iter().any(|text| text == "before"));
}

#[test]
fn open_dialog_does_not_bump_zindex_every_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let background_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("background");
    });
    let dialog_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("dialog");
    });
    let background = ctx.create_window("background", rect(0, 0, 100, 80), background_tree);
    let dialog = ctx.create_dialog("dialog", rect(10, 10, 80, 40), dialog_tree);

    ctx.set_root_visible(dialog, true);
    ctx.update_ui();

    let first_zindex = ctx.root_handle(dialog).unwrap().zindex();
    assert!(first_zindex > ctx.root_handle(background).unwrap().zindex());

    ctx.update_ui();

    assert_eq!(ctx.root_handle(dialog).unwrap().zindex(), first_zindex);
}

#[test]
fn reshown_roots_drop_stale_panel_handles_after_a_gap() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let panel = ctx.new_panel("panel");
    let tree_with_panel = WidgetTreeBuilder::build({
        let panel = panel.clone();
        move |tree| {
            tree.container(panel.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.text("panel child");
            });
        }
    });
    let tree_without_panel = WidgetTreeBuilder::build(|tree| {
        tree.text("root only");
    });
    let root = ctx.create_window("window", rect(0, 0, 100, 80), tree_with_panel);

    ctx.update_ui();
    let window = ctx.root_handle(root).unwrap();
    assert_eq!(window.inner().main.panel_count(), 1);

    ctx.set_root_visible(root, false);
    ctx.update_ui();

    ctx.set_root_tree(root, tree_without_panel);
    ctx.set_root_visible(root, true);
    ctx.update_ui();

    let window = ctx.root_handle(root).unwrap();
    assert_eq!(window.inner().main.panel_count(), 0);
}

#[test]
fn newly_opened_popup_auto_sizes_on_first_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.text("hello popup");
    });
    let popup = ctx.create_popup("popup", tree);

    ctx.mousemove(40, 50);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();

    let popup = ctx.root_handle(popup).unwrap();
    let inner = popup.inner();
    assert!(inner.main.rect().width > 1);
    assert!(inner.main.rect().height > 1);
    assert!(inner.main.body().width > 0);
    assert!(inner.main.body().height > 0);
}

#[test]
fn auto_sized_titled_window_uses_current_frame_content_size() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.text("hello\nhello\nhello");
    });
    let root = ctx.create_window("window", rect(0, 0, 1, 1), tree);
    ctx.set_root_options(root, ContainerOption::AUTO_SIZE, ScrollBehavior::NONE);

    ctx.update_ui();

    let window = ctx.root_handle(root).unwrap();
    let inner = window.inner();
    assert!(inner.main.rect().width > 1);
    assert!(inner.main.rect().height > 1);
    assert!(inner.main.body().y > inner.main.rect().y);
    assert!(inner.main.body().height > 0);
}

#[test]
fn popup_content_changes_resize_without_a_frame_lag() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let short_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("a");
    });
    let long_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("hello popup with much longer content");
    });
    let popup = ctx.create_popup("popup", short_tree);

    ctx.mousemove(60, 60);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();
    let first_width = ctx.root_handle(popup).unwrap().rect().width;

    ctx.set_root_tree(popup, long_tree);
    ctx.update_ui();

    assert!(ctx.root_handle(popup).unwrap().rect().width > first_width);
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
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.text("hello\nhello\nhello\nhello");
    });
    let root = ctx.create_window("window", rect(0, 0, 1, 1), tree);
    ctx.set_root_options(root, ContainerOption::AUTO_SIZE, ScrollBehavior::NONE);

    ctx.update_ui();

    let window = ctx.root_handle(root).unwrap();
    let inner = window.inner();
    assert!(inner.main.body().width >= inner.main.content_size().width);
    assert!(inner.main.body().height >= inner.main.content_size().height);
    assert!(inner.main.body().height > 0);
    assert!(inner.main.body().y > inner.main.rect().y);

    let body = inner.main.body();
    let has_vertical_scrollbar = inner
        .main
        .debug_commands()
        .iter()
        .any(|cmd| matches!(cmd, Command::Recti { rect, .. } if rect.x == body.x + body.width && rect.width == style.scrollbar_size && rect.height > 0));

    assert!(!has_vertical_scrollbar);
}

#[test]
fn title_option_controls_root_window_title_bar_geometry() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let titled = ctx.create_window("titled", rect(0, 0, 80, 40), WidgetTreeBuilder::build(|_tree| {}));
    let plain = ctx.create_window("plain", rect(100, 0, 80, 40), WidgetTreeBuilder::build(|_tree| {}));
    ctx.set_root_options(plain, ContainerOption::NO_TITLE, ScrollBehavior::NONE);
    ctx.update_ui();

    let titled = ctx.root_handle(titled).unwrap();
    let titled_inner = titled.inner();
    assert!(titled_inner.main.body().y > titled_inner.main.rect().y);
    assert!(titled_inner.main.body().height < titled_inner.main.rect().height);
    let titled_texts: Vec<String> = titled_inner
        .main
        .debug_commands()
        .iter()
        .filter_map(|cmd| match cmd {
            Command::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(titled_texts.iter().any(|text| text == "titled"));

    let plain = ctx.root_handle(plain).unwrap();
    let plain_inner = plain.inner();
    assert_eq!(plain_inner.main.body().y, plain_inner.main.rect().y);
    assert_eq!(plain_inner.main.body().height, plain_inner.main.rect().height);
    assert_eq!(plain_inner.main.body().x, plain_inner.main.rect().x);
    assert_eq!(plain_inner.main.body().width, plain_inner.main.rect().width);
    let plain_texts: Vec<String> = plain_inner
        .main
        .debug_commands()
        .iter()
        .filter_map(|cmd| match cmd {
            Command::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(!plain_texts.iter().any(|text| text == "plain"));
}

#[test]
fn duplicate_widget_dispatch_in_same_tree_panics_with_context() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let shared = widget_handle(AlwaysSubmitWidget::new("shared"));
    let tree = WidgetTreeBuilder::build(|tree| {
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
    assert!(message.contains("tree node"));
}

#[test]
fn duplicate_widget_dispatch_across_windows_panics() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let shared = widget_handle(AlwaysSubmitWidget::new("shared"));
    let left_tree = WidgetTreeBuilder::build(|tree| {
        tree.widget(shared.clone());
    });
    let right_tree = WidgetTreeBuilder::build(|tree| {
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
    let tree = WidgetTreeBuilder::build(|tree| {
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
    let tree = WidgetTreeBuilder::build({
        let text = text.clone();
        move |tree| {
            tree.widget(text.clone());
        }
    });
    let root = ctx.create_window("retained", rect(0, 0, 90, 50), tree);

    ctx.update_ui();
    let handle = ctx.root_handle(root).unwrap();
    assert!(window_texts(&handle).iter().any(|text| text == "before"));

    text.borrow_mut().text = "after".to_string();
    ctx.update_ui();
    let handle = ctx.root_handle(root).unwrap();
    let texts = window_texts(&handle);
    assert!(texts.iter().any(|text| text == "after"));
    assert!(!texts.iter().any(|text| text == "before"));
}

#[test]
fn retained_root_visibility_controls_rendering() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.text("visible");
    });
    let root = ctx.create_window("retained", rect(0, 0, 90, 50), tree);

    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["retained"]);

    ctx.set_root_visible(root, false);
    ctx.update_ui();
    assert!(rendered_root_names(&ctx).is_empty());
    assert!(window_texts(&ctx.root_handle(root).unwrap()).is_empty());

    ctx.set_root_visible(root, true);
    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["retained"]);
    assert!(window_texts(&ctx.root_handle(root).unwrap()).iter().any(|text| text == "visible"));
}

#[test]
fn retained_root_tree_can_be_replaced_after_registration() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
    let first_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("first");
    });
    let second_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("second");
    });
    let root = ctx.create_window("retained", rect(0, 0, 90, 50), first_tree);

    ctx.update_ui();
    assert!(window_texts(&ctx.root_handle(root).unwrap()).iter().any(|text| text == "first"));

    ctx.set_root_tree(root, second_tree);
    ctx.update_ui();
    let texts = window_texts(&ctx.root_handle(root).unwrap());
    assert!(texts.iter().any(|text| text == "second"));
    assert!(!texts.iter().any(|text| text == "first"));
}

#[test]
fn retained_roots_preserve_z_order_and_fronting() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let left_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("left");
    });
    let right_tree = WidgetTreeBuilder::build(|tree| {
        tree.text("right");
    });
    let left = ctx.create_window("left", rect(0, 0, 90, 60), left_tree);
    let _right = ctx.create_window("right", rect(20, 0, 90, 60), right_tree);

    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["left", "right"]);

    let mut left_handle = ctx.root_handle(left).unwrap();
    ctx.bring_to_front(&mut left_handle);
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
        WidgetTreeBuilder::build(|tree| {
            tree.text("background");
        }),
    );
    let dialog = ctx.create_dialog(
        "dialog",
        rect(10, 10, 90, 50),
        WidgetTreeBuilder::build(|tree| {
            tree.text("dialog");
        }),
    );

    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["background"]);

    ctx.set_root_visible(dialog, true);
    ctx.update_ui();
    assert_eq!(rendered_root_names(&ctx), vec!["background", "dialog"]);
    assert!(ctx.root_handle(dialog).unwrap().zindex() > ctx.root_handle(background).unwrap().zindex());
}

#[test]
fn retained_popup_opens_at_mouse_and_auto_sizes_on_first_frame() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup = ctx.create_popup(
        "popup",
        WidgetTreeBuilder::build(|tree| {
            tree.text("hello popup");
        }),
    );

    ctx.mousemove(40, 50);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();

    let handle = ctx.root_handle(popup).unwrap();
    assert_eq!(handle.rect().x, 40);
    assert_eq!(handle.rect().y, 50);
    assert!(handle.rect().width > 1);
    assert!(handle.rect().height > 1);
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
        WidgetTreeBuilder::build(|tree| {
            tree.text("left");
        }),
    );
    let _right = ctx.create_window(
        "right",
        rect(0, 0, 100, 80),
        WidgetTreeBuilder::build(|tree| {
            tree.text("right");
        }),
    );

    ctx.mousemove(20, 20);
    ctx.update_ui();
    assert_eq!(ctx.hover_root.as_ref().unwrap().inner().main.name(), "right");

    let mut left_handle = ctx.root_handle(left).unwrap();
    ctx.bring_to_front(&mut left_handle);
    ctx.update_ui();
    assert_eq!(ctx.hover_root.as_ref().unwrap().inner().main.name(), "left");
}

#[test]
fn retained_chrome_node_ids_are_root_derived_and_stable() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "retained",
        rect(0, 0, 100, 70),
        WidgetTreeBuilder::build(|tree| {
            tree.text("before");
        }),
    );
    let other = ctx.create_window(
        "other",
        rect(120, 0, 100, 70),
        WidgetTreeBuilder::build(|tree| {
            tree.text("other");
        }),
    );

    let ids = ctx.root_handle(root).unwrap().inner().chrome_ids();
    assert_eq!(ids, WindowChromeIds::from_root_seed(root.raw()));
    assert_ne!(ids, ctx.root_handle(other).unwrap().inner().chrome_ids());

    ctx.update_ui();
    ctx.set_root_tree(
        root,
        WidgetTreeBuilder::build(|tree| {
            tree.text("after");
        }),
    );
    ctx.update_ui();

    assert_eq!(ctx.root_handle(root).unwrap().inner().chrome_ids(), ids);
}

#[test]
fn retained_chrome_nodes_are_recorded_in_root_cache() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "retained",
        rect(10, 12, 100, 70),
        WidgetTreeBuilder::build(|tree| {
            tree.text("body");
        }),
    );
    let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids();

    ctx.update_ui();

    let handle = ctx.root_handle(root).unwrap();
    let inner = handle.inner();
    let title = inner.main.previous_node_layout(chrome.title).expect("title chrome node layout missing");
    let close = inner.main.previous_node_layout(chrome.close).expect("close chrome node layout missing");
    let resize = inner.main.previous_node_layout(chrome.resize).expect("resize chrome node layout missing");

    assert_eq!(title.rect.x, 10);
    assert_eq!(title.rect.y, 12);
    assert_eq!(title.rect.width, 100);
    assert!(title.rect.height > 0);
    assert!(close.rect.x >= title.rect.x);
    assert!(resize.rect.x >= title.rect.x);
    assert!(resize.rect.y >= title.rect.y);
}

#[test]
fn retained_title_drag_uses_chrome_node_after_tree_update() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "retained",
        rect(10, 10, 100, 70),
        WidgetTreeBuilder::build(|tree| {
            tree.text("before");
        }),
    );
    let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids();

    ctx.update_ui();
    ctx.set_root_tree(
        root,
        WidgetTreeBuilder::build(|tree| {
            tree.text("after");
        }),
    );
    ctx.update_ui();

    let initial = ctx.root_handle(root).unwrap().rect();
    let title_x = initial.x + 10;
    let title_y = initial.y + 6;
    ctx.mousemove(title_x, title_y);
    ctx.update_ui();
    ctx.update_ui();
    ctx.mousedown(title_x, title_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(title_x + 12, title_y + 7);
    ctx.update_ui();

    let moved = ctx.root_handle(root).unwrap().rect();
    assert!(moved.x > initial.x);
    assert!(moved.y > initial.y);
    assert!(ctx.committed_results().state_of_node(chrome.title).is_active());
    assert_eq!(ctx.root_handle(root).unwrap().inner().chrome_ids(), chrome);
}

#[test]
fn retained_close_button_closes_root_and_records_chrome_result() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let root = ctx.create_window(
        "retained",
        rect(10, 10, 100, 70),
        WidgetTreeBuilder::build(|tree| {
            tree.text("body");
        }),
    );
    let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids();

    let close_x = 10 + 100 - 12;
    let close_y = 10 + 6;
    ctx.mousemove(close_x, close_y);
    ctx.update_ui();
    ctx.update_ui();
    assert!(ctx.root_handle(root).unwrap().is_open());

    ctx.mousedown(close_x, close_y, MouseButton::LEFT);
    ctx.update_ui();
    assert!(ctx.committed_results().state_of_node(chrome.close).is_submitted());
    assert!(!ctx.root_handle(root).unwrap().is_open());

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
    let tree = WidgetTreeBuilder::build({
        let text = text.clone();
        move |tree| {
            tree.widget(text.clone());
        }
    });
    let root = ctx.create_window("retained", rect(0, 0, 60, 40), tree);
    let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids();

    ctx.update_ui();
    ctx.update_ui();

    let initial_rect = ctx.root_handle(root).unwrap().rect();
    let corner_x = initial_rect.x + initial_rect.width - 1;
    let corner_y = initial_rect.y + initial_rect.height - 1;

    ctx.mousemove(corner_x, corner_y);
    ctx.update_ui();
    ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
    ctx.update_ui();
    ctx.mousemove(corner_x + 12, corner_y + 10);
    ctx.update_ui();

    let resized = ctx.root_handle(root).unwrap().rect();
    assert!(resized.width > initial_rect.width);
    assert!(resized.height > initial_rect.height);
    assert!(ctx.committed_results().state_of_node(chrome.resize).is_active());
}

#[test]
fn retained_popup_closes_from_root_state_when_clicking_outside() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup = ctx.create_popup(
        "popup",
        WidgetTreeBuilder::build(|tree| {
            tree.text("popup");
        }),
    );

    ctx.mousemove(20, 20);
    ctx.set_root_visible(popup, true);
    ctx.update_ui();
    assert!(ctx.root_handle(popup).unwrap().is_open());

    ctx.mousemove(200, 100);
    ctx.update_ui();
    ctx.mousedown(200, 100, MouseButton::LEFT);
    ctx.update_ui();

    assert!(!ctx.root_handle(popup).unwrap().is_open());
    assert!(rendered_root_names(&ctx).is_empty());
}

fn run_combo_frame(
    ctx: &mut Context<NoopRenderer>,
    popup_root: RootId,
    combo: &WidgetHandle<Combo>,
    items: &[WidgetHandle<ListItem>; 2],
    item_ids: &[NodeId; 2],
) -> Option<String> {
    let labels: Vec<String> = items.iter().map(|item| item.borrow().label.clone()).collect();
    combo.borrow_mut().update_items(&labels);
    let combo_anchor = combo.borrow().anchor();
    let mut popup = combo.borrow().popup.clone();
    if combo.borrow().is_open() {
        ctx.set_root_visible(popup_root, true);
        popup.set_rect(combo_anchor);
    } else {
        ctx.set_root_visible(popup_root, false);
    }

    let mut selected_label = None;
    let results = ctx.committed_results();
    for (idx, node_id) in item_ids.iter().enumerate() {
        if results.state_of_retained(RetainedId::root_node(popup_root, *node_id)).is_submitted() {
            selected_label = combo.borrow_mut().select(idx, &labels);
            break;
        }
    }

    ctx.update_ui();
    selected_label
}

#[test]
fn retained_combo_popup_stays_closed_after_mouse_selection() {
    let atlas = make_test_atlas();
    let renderer = RendererHandle::new(NoopRenderer { atlas });
    let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
    let popup_root = ctx.create_popup("combo popup", WidgetTree::default());
    ctx.set_root_options(
        popup_root,
        ContainerOption::AUTO_SIZE | ContainerOption::NO_RESIZE | ContainerOption::NO_TITLE,
        ScrollBehavior::NO_SCROLL,
    );
    let popup = ctx.root_handle(popup_root).expect("popup root missing");
    let combo = widget_handle(Combo::new(popup));
    let items = [widget_handle(ListItem::new("Apple")), widget_handle(ListItem::new("Banana"))];
    let mut item_ids = [NodeId::default(); 2];
    let main_root = ctx.create_window(
        "combo window",
        rect(0, 0, 120, 80),
        WidgetTreeBuilder::build({
            let combo = combo.clone();
            move |tree| {
                tree.row(&[SizePolicy::Fixed(80)], SizePolicy::Auto, |tree| {
                    tree.widget(combo.clone());
                });
            }
        }),
    );
    ctx.set_root_options(main_root, ContainerOption::NO_TITLE | ContainerOption::NO_RESIZE, ScrollBehavior::NONE);
    let popup_items = items.clone();
    ctx.set_root_tree(
        popup_root,
        WidgetTreeBuilder::build(|tree| {
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
    assert!(combo.borrow().is_open());
    assert!(ctx.root_handle(popup_root).unwrap().is_open());

    let popup_rect = ctx.root_handle(popup_root).unwrap().rect();
    let item_x = popup_rect.x + 12;
    let item_y = popup_rect.y + 12;
    ctx.mousemove(item_x, item_y);
    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    ctx.mousedown(item_x, item_y, MouseButton::LEFT);
    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    ctx.mouseup(item_x, item_y, MouseButton::LEFT);
    let selected = run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);

    assert_eq!(selected.as_deref(), Some("Apple"));
    assert!(!combo.borrow().is_open());
    assert!(!ctx.root_handle(popup_root).unwrap().is_open());

    run_combo_frame(&mut ctx, popup_root, &combo, &items, &item_ids);
    assert!(!combo.borrow().is_open());
    assert!(!ctx.root_handle(popup_root).unwrap().is_open());
}
