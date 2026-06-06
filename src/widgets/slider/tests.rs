//! Tests for slider and numeric editing behavior.

use super::*;
use crate::test_support::test_atlas as make_test_atlas;
use crate::ui_node::UiInputEvent;

fn run_slider_once(slider: &mut Slider, rect: Recti, events: Vec<UiInputEvent>, control: ControlState) -> ResourceState {
    let atlas = make_test_atlas();
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut clip_stack = Vec::new();
    let mut focus = None;
    let mut updated_focus = false;
    let mut ctx = WidgetCtx::new_with_interaction(
        RetainedId::node(Id::new(1)),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut clip_stack,
        &style,
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        events,
    );
    slider.update(&mut ctx, &control)
}

fn assert_real_close(actual: Real, expected: Real) {
    assert!((actual - expected).abs() < 1.0e-5, "expected {expected}, got {actual}");
}

#[test]
fn slider_zero_range_keeps_value() {
    let atlas = make_test_atlas();
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut clip_stack = Vec::new();
    let mut focus = None;
    let mut updated_focus = false;

    let mut slider = Slider::new(5.0, 5.0, 5.0);
    let rect = rect(0, 0, 100, 20);
    let input = vec![UiInputEvent::MouseDrag {
        pos: vec2(50, 10),
        delta: vec2(5, 0),
        buttons: MouseButton::LEFT,
    }];
    let mut ctx = WidgetCtx::new_with_interaction(
        RetainedId::node(Id::new(2)),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut clip_stack,
        &style,
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        input,
    );
    let control = ControlState {
        hovered: true,
        focused: true,
        clicked: false,
        active: true,
        scroll_delta: None,
    };

    let res = slider.update(&mut ctx, &control);

    assert!(res.is_active());
    assert!(slider.value.is_finite());
    assert_eq!(slider.value, 5.0);
    assert_eq!(slider.value, 5.0);
}

#[test]
fn slider_wheel_snaps_fractional_step_from_lower_bound() {
    let mut slider = Slider::with_opt(1.15, 1.0, 2.0, 0.2, 2, WidgetOption::NONE);
    let control = ControlState {
        hovered: true,
        focused: false,
        clicked: false,
        active: false,
        scroll_delta: Some(vec2(0, 1)),
    };

    let res = run_slider_once(&mut slider, rect(0, 0, 100, 20), Vec::new(), control);

    assert!(res.is_changed());
    assert_real_close(slider.value, 1.4);
}

#[test]
fn slider_drag_snaps_fractional_step_from_lower_bound() {
    let mut slider = Slider::with_opt(10.0, 10.0, 20.0, 0.25, 2, WidgetOption::NONE);
    let input = vec![UiInputEvent::MouseDrag {
        pos: vec2(33, 10),
        delta: Vec2i::default(),
        buttons: MouseButton::LEFT,
    }];
    let control = ControlState {
        hovered: true,
        focused: true,
        clicked: false,
        active: true,
        scroll_delta: None,
    };

    let res = run_slider_once(&mut slider, rect(0, 0, 100, 20), input, control);

    assert!(res.is_changed());
    assert_real_close(slider.value, 13.25);
}

#[test]
fn slider_uses_widget_local_mouse_position() {
    let atlas = make_test_atlas();
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut clip_stack = Vec::new();
    let mut focus = None;
    let mut updated_focus = false;

    let mut slider = Slider::new(0.0, 0.0, 100.0);
    let rect = rect(40, 20, 100, 20);
    let input = vec![UiInputEvent::MouseDrag {
        pos: vec2(90, 30),
        delta: Vec2i::default(),
        buttons: MouseButton::LEFT,
    }];
    let mut ctx = WidgetCtx::new_with_interaction(
        RetainedId::node(Id::new(3)),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut clip_stack,
        &style,
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        input,
    );
    let control = ControlState {
        hovered: true,
        focused: true,
        clicked: false,
        active: true,
        scroll_delta: None,
    };

    let res = slider.update(&mut ctx, &control);

    assert!(!res.is_none());
    assert_eq!(slider.value, 50.0);
}
