//! Tests for slider and numeric editing behavior.

use super::*;
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
    let mut ctx = WidgetUpdateCtx::new_with_interaction(rect, rect, &style, &atlas, true, hovered, focused, false, active, scroll_delta);
    let mut events = localize_events(rect, events);
    if let Some(delta) = scroll_delta {
        events.push(UiInputEvent::Scroll { pos: Vec2i::default(), delta });
    }
    slider.update(&mut ctx, events)
}

fn run_number_once(number: &mut Number, events: Vec<UiInputEvent>) -> ResourceState {
    let atlas = make_test_atlas();
    let style = Style::default();
    let bounds = rect(0, 0, 100, 20);
    let mut ctx = WidgetUpdateCtx::new_with_interaction(bounds, bounds, &style, &atlas, true, true, true, false, true, None);
    number.update(&mut ctx, events)
}

fn assert_real_close(actual: Real, expected: Real) {
    assert!((actual - expected).abs() < 1.0e-5, "expected {expected}, got {actual}");
}

#[test]
fn slider_zero_range_keeps_value() {
    let atlas = make_test_atlas();
    let style = Style::default();

    let (state, mut slider) = Slider::create(SliderParameters::new(5.0, 5.0, 5.0));
    let rect = rect(0, 0, 100, 20);
    let input = vec![UiInputEvent::MouseDrag {
        pos: vec2(50, 10),
        delta: vec2(5, 0),
        buttons: MouseButton::LEFT,
    }];
    let mut ctx = WidgetUpdateCtx::new_with_interaction(rect, rect, &style, &atlas, true, true, true, false, true, None);

    let res = slider.update(&mut ctx, localize_events(rect, input));

    assert!(res.is_active());
    assert_eq!(state.try_read(|state| state.value().is_finite()), Some(true));
    assert_eq!(state.try_read(SliderState::value), Some(5.0));
    assert_eq!(state.try_update(SliderState::take_changed), Some(false));
}

#[test]
fn slider_wheel_snaps_fractional_step_from_lower_bound() {
    let (state, mut slider) = Slider::create(SliderParameters::with_opt(1.15, 1.0, 2.0, 0.2, 2, WidgetOption::FRAME));
    let res = run_slider_once(&mut slider, rect(0, 0, 100, 20), Vec::new(), true, false, false, Some(vec2(0, 1)));

    assert!(res.is_changed());
    assert_real_close(state.try_read(SliderState::value).unwrap(), 1.4);
    assert_eq!(state.try_update(SliderState::take_changed), Some(true));
    assert_eq!(state.try_update(SliderState::take_changed), Some(false));
}

#[test]
fn slider_drag_snaps_fractional_step_from_lower_bound() {
    let (state, mut slider) = Slider::create(SliderParameters::with_opt(10.0, 10.0, 20.0, 0.25, 2, WidgetOption::FRAME));
    let input = vec![UiInputEvent::MouseDrag {
        pos: vec2(33, 10),
        delta: Vec2i::default(),
        buttons: MouseButton::LEFT,
    }];
    let res = run_slider_once(&mut slider, rect(0, 0, 100, 20), input, true, true, true, None);

    assert!(res.is_changed());
    assert_real_close(state.try_read(SliderState::value).unwrap(), 13.25);
    assert_eq!(state.try_update(SliderState::take_changed), Some(true));
}

#[test]
fn slider_uses_widget_local_mouse_position() {
    let atlas = make_test_atlas();
    let style = Style::default();

    let (state, mut slider) = Slider::create(SliderParameters::new(0.0, 0.0, 100.0));
    let rect = rect(40, 20, 100, 20);
    let input = vec![UiInputEvent::MouseDrag {
        pos: vec2(90, 30),
        delta: Vec2i::default(),
        buttons: MouseButton::LEFT,
    }];
    let mut ctx = WidgetUpdateCtx::new_with_interaction(rect, rect, &style, &atlas, true, true, true, false, true, None);

    let res = slider.update(&mut ctx, localize_events(rect, input));

    assert!(!res.is_none());
    assert_eq!(state.try_read(SliderState::value), Some(50.0));
    assert_eq!(state.try_update(SliderState::take_changed), Some(true));
}

#[test]
fn number_drag_records_a_typed_change_and_programmatic_setter_is_silent() {
    let (state, mut number) = Number::create(NumberParameters::new(0.0, 2.0, 0));
    state.try_update(|state| state.set_value(4.0)).unwrap();
    assert_eq!(state.try_update(NumberState::take_changed), Some(false));

    let result = run_number_once(
        &mut number,
        vec![UiInputEvent::MouseDrag {
            pos: vec2(10, 10),
            delta: vec2(3, 0),
            buttons: MouseButton::LEFT,
        }],
    );
    assert!(result.is_changed());
    assert_eq!(state.try_read(NumberState::value), Some(10.0));
    assert_eq!(state.try_update(NumberState::take_changed), Some(true));
    assert_eq!(state.try_update(NumberState::take_changed), Some(false));
}

#[test]
fn slider_programmatic_setter_is_silent() {
    let (state, _slider) = Slider::create(SliderParameters::new(0.0, -5.0, 5.0));
    state.try_update(|state| state.set_value(4.0)).unwrap();
    assert_eq!(state.try_read(SliderState::value), Some(4.0));
    assert_eq!(state.try_update(SliderState::take_changed), Some(false));
}
