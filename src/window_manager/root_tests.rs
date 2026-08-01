use super::*;

use crate::test_support::{recording_backend, test_atlas, NoopRenderer, RenderEvent};
use crate::{
    color, rect, AtlasHandle, Button, ButtonParameters, ButtonState, Column, ColumnParameters, ColumnState, Custom, CustomParameters, Dimensioni, Disclosure,
    DisclosureParameters, DisclosureState, KeyMode, MouseButton, Node, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetState,
    WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};
use crate::render::{FrameInfo, RenderError};
use std::{cell::RefCell, rc::Rc};

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
    updates: usize,
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
        Dimensioni::new(80, 60)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, event: Option<&UiInputEvent>) {
        let mut state = self.state.borrow_mut();
        state.updates += 1;
        state.held_buttons.push(ctx.mouse_buttons().bits());
        state.held_keys.push(ctx.key_modes().bits());
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
    ctx.create_window("window", rect(10, 10, 120, 90), empty_content());
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
    ctx.update_and_render_ui();
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
