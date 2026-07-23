//! Tests for slider and numeric editing behavior.

use super::*;
use crate::render::geometry::SolidGeometry;
use crate::test_support::test_atlas as make_test_atlas;
use crate::ui_node::UiInputEvent;
use crate::widget_ctx::localize_events;

fn run_slider_once(
    slider: &mut Slider,
    rect: Recti,
    events: Vec<UiInputEvent>,
    hovered: bool,
    focused: bool,
    active: bool,
    scroll_delta: Option<Vec2i>,
) -> ResourceState {
    let atlas = make_test_atlas();
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut solid_geometry = SolidGeometry::new();
    let mut clip_stack = Vec::new();
    let mut focus = None;
    let mut updated_focus = false;
    let mut ctx = WidgetCtx::new_with_interaction(
        Id::new(1),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut solid_geometry,
        &mut clip_stack,
        &style,
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        hovered,
        focused,
        false,
        active,
        scroll_delta,
    );
    let mut events = localize_events(rect, events);
    if let Some(delta) = scroll_delta {
        events.push(UiInputEvent::Scroll { pos: Vec2i::default(), delta });
    }
    slider.update(&mut ctx, events)
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
    let mut solid_geometry = SolidGeometry::new();
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
        Id::new(2),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut solid_geometry,
        &mut clip_stack,
        &style,
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        true,
        true,
        false,
        true,
        None,
    );

    let res = slider.update(&mut ctx, localize_events(rect, input));

    assert!(res.is_active());
    assert!(slider.value.is_finite());
    assert_eq!(slider.value, 5.0);
    assert_eq!(slider.value, 5.0);
}

#[test]
fn slider_wheel_snaps_fractional_step_from_lower_bound() {
    let mut slider = Slider::with_opt(1.15, 1.0, 2.0, 0.2, 2, WidgetOption::NONE);
    let res = run_slider_once(&mut slider, rect(0, 0, 100, 20), Vec::new(), true, false, false, Some(vec2(0, 1)));

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
    let res = run_slider_once(&mut slider, rect(0, 0, 100, 20), input, true, true, true, None);

    assert!(res.is_changed());
    assert_real_close(slider.value, 13.25);
}

#[test]
fn slider_uses_widget_local_mouse_position() {
    let atlas = make_test_atlas();
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut solid_geometry = SolidGeometry::new();
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
        Id::new(3),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut solid_geometry,
        &mut clip_stack,
        &style,
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        true,
        true,
        false,
        true,
        None,
    );

    let res = slider.update(&mut ctx, localize_events(rect, input));

    assert!(!res.is_none());
    assert_eq!(slider.value, 50.0);
}
