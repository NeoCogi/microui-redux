use super::*;

use crate::test_support::{recording_backend, test_atlas, NoopRenderer, RenderEvent};
use crate::{
    rect, Button, ButtonParameters, ButtonState, Column, ColumnParameters, ColumnState, Custom, CustomParameters, Dimensioni, MouseButton, Node,
    WidgetStateHandle,
};

fn context() -> Context<NoopRenderer> {
    Context::new_test(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240))
}

fn empty_content() -> Node {
    Column::create(ColumnParameters::default()).1
}

fn button_content(label: &str) -> (WidgetStateHandle<ButtonState>, Node) {
    let (state, widget) = Button::create(ButtonParameters::new(label));
    (state, Node::widget(widget))
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
    assert_eq!(ctx.debug_root_structure(root.id()), Some((2, 0)));
}

#[test]
fn hide_and_show_preserve_root_and_descendant_state() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.create_window("window", rect(10, 10, 120, 90), content);

    ctx.set_root_visible(root.id(), false).unwrap();
    ctx.update_ui();
    assert_eq!(root.state().try_read(RootState::is_visible), Some(false));
    assert!(button.is_alive());

    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_ui();
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
fn destruction_waits_for_an_active_state_upgrade() {
    let mut ctx = context();
    let (child, content) = button_content("child");
    let root = ctx.create_window("window", rect(0, 0, 100, 80), content);
    let state = root.state().clone();

    state
        .try_update(|_| {
            assert!(ctx.destroy_root(root.id()));
            assert!(state.is_alive());
            assert!(child.is_alive());
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

    column.try_update(|column| column.push(Node::widget(widget))).unwrap();
    ctx.update_ui();
    assert_eq!(root.id(), root_id);
    assert!(button.is_alive());
    assert_eq!(ctx.debug_root_structure(root_id), Some((3, 0)));

    assert_eq!(column.try_update(|column: &mut ColumnState| column.remove_drop(0)), Some(true));
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
    ctx.update_ui();

    ctx.mousedown(200, 180, MouseButton::LEFT);
    ctx.update_ui();

    assert_eq!(popup.state().try_read(RootState::is_visible), Some(false));
    ctx.set_root_visible(popup.id(), true).unwrap();
    ctx.set_root_visible(popup.id(), false).unwrap();
    assert_eq!(popup.state().try_update(RootState::take_submitted), Some(true));
    assert_eq!(popup.state().try_update(RootState::take_submitted), Some(false));
}

#[test]
fn chrome_and_content_run_once_per_phase_in_one_persistent_tree() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(10, 10, 120, 90), empty_content());

    ctx.update_ui();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 2);
    assert_eq!(metrics.updates, 2);
    assert_eq!(metrics.paints, 2);
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
fn title_drag_and_close_record_typed_root_events() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), empty_content());
    ctx.update_ui();
    let (title, _, _) = ctx.debug_root_chrome(root.id()).unwrap();
    let title = title.unwrap();
    let drag_x = title.x + 2;
    let drag_y = title.y + 2;

    ctx.mousedown(drag_x, drag_y, MouseButton::LEFT);
    ctx.update_ui();
    assert_eq!(root.state().try_read(RootState::is_moving), Some(true));
    assert_eq!(root.state().try_update(RootState::take_changed), Some(true));

    ctx.mouseup(drag_x, drag_y, MouseButton::LEFT);
    ctx.update_ui();
    assert_eq!(root.state().try_read(RootState::is_active), Some(false));

    let close = ctx.debug_root_chrome(root.id()).unwrap().1.unwrap();
    let close_x = close.x + close.width / 2;
    let close_y = close.y + close.height / 2;
    ctx.mousedown(close_x, close_y, MouseButton::LEFT);
    ctx.update_ui();
    assert_eq!(root.state().try_read(RootState::is_visible), Some(false));
    assert_eq!(root.state().try_update(RootState::take_submitted), Some(true));
}

#[test]
fn hiding_and_showing_root_does_not_restore_chrome_capture() {
    let mut ctx = context();
    let root = ctx.create_window("window", rect(30, 30, 140, 100), empty_content());
    ctx.update_ui();
    let title = ctx.debug_root_chrome(root.id()).unwrap().0.unwrap();

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.update_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(true));
    assert_eq!(root.state().try_read(RootState::is_moving), Some(true));

    ctx.set_root_visible(root.id(), false).unwrap();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(root.state().try_read(RootState::is_active), Some(false));

    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(root.state().try_read(RootState::is_active), Some(false));
}

#[test]
fn chrome_geometry_exposes_one_body_and_auto_size_tracks_content() {
    let mut ctx = context();
    let (_, text) = crate::TextBlock::create(crate::TextBlockParameters::new("window content"));
    let root = ctx.create_popup("popup", Node::widget(text));
    ctx.set_root_visible(root.id(), true).unwrap();
    ctx.update_ui();

    let body = ctx.debug_root_body(root.id()).unwrap();
    let content = ctx.debug_root_content_size(root.id()).unwrap();
    let outer = root.state().try_read(RootState::rect).unwrap();
    assert!(body.width > 0 && body.height > 0);
    assert!(content.width > 0 && content.height > 0);
    assert!(outer.width >= body.width && outer.height >= body.height);
}

#[test]
fn body_input_falls_through_chrome_to_the_application_node() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.create_window("window", rect(20, 20, 140, 100), content);
    ctx.update_ui();
    let body = ctx.debug_root_body(root.id()).unwrap();

    ctx.mousedown(body.x + body.width / 2, body.y + body.height / 2, MouseButton::LEFT);
    ctx.update_ui();

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

    ctx.update_ui();
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
