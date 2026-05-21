//! Tests for basic widget sizing and state behavior.

use super::*;
use crate::{test_support::test_atlas as make_test_atlas, Input};
use std::{cell::RefCell, rc::Rc};

#[test]
fn image_widgets_measure_external_texture_dimensions() {
    let atlas = make_test_atlas();
    let style = Style::default();
    let texture = TextureId::new(7, 13, 5);

    let button = Button::with_image("aa", Some(Image::Texture(texture)), WidgetOption::NONE, WidgetFillOption::ALL);
    let button_size = button.measure(&style, &atlas, Dimensioni::default());

    assert_eq!(button_size.width, style.padding * 2 + texture.width() + style.padding + 16);
    assert_eq!(button_size.height, 14);

    let list_box = ListBox::new("a", Some(Image::Texture(texture)));
    let list_size = list_box.measure(&style, &atlas, Dimensioni::default());

    assert_eq!(list_size.width, style.padding * 2 + texture.width() + style.padding + 8);
    assert_eq!(list_size.height, 14);
}

#[test]
fn inline_image_layout_keeps_visual_and_text_rects_separate() {
    let style = Style::default();
    let layout = layout_inline_content(rect(10, 20, 60, 18), &style, "aa", Some(Dimensioni::new(13, 5)));
    let visual = layout.visual.expect("visual rect");

    assert_eq!(visual.x, 10 + style.padding);
    assert_eq!(visual.width, 13);
    assert!(layout.text.x >= visual.x + visual.width);
    assert!(layout.text.width > 0);
}

#[test]
fn combo_run_toggles_open_state() {
    let atlas = make_test_atlas();
    let style = Rc::new(Style::default());
    let input = Rc::new(RefCell::new(Input::default()));
    let popup = WindowHandle::popup(RootId::from_raw(1), "combo", atlas.clone(), style.clone(), input);
    let mut combo = Combo::new(popup);
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut clip_stack = Vec::new();
    let mut focus = None;
    let mut updated_focus = false;
    let rect = rect(0, 0, 100, 20);
    let control = ControlState {
        hovered: true,
        focused: true,
        clicked: true,
        active: true,
        scroll_delta: None,
    };
    let mut ctx = WidgetCtx::new_with_interaction(
        RetainedId::node(Id::new(1)),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut clip_stack,
        style.as_ref(),
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        None,
    );

    combo.update(&mut ctx, &control);
    assert!(combo.is_open());

    combo.popup.open();
    let mut ctx = WidgetCtx::new_with_interaction(
        RetainedId::node(Id::new(1)),
        rect,
        &mut commands,
        &mut triangle_vertices,
        &mut clip_stack,
        style.as_ref(),
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        None,
    );
    combo.update(&mut ctx, &control);
    assert!(!combo.is_open());
    assert!(!combo.popup.is_open());
}

#[test]
fn combo_select_updates_label_and_closes_popup() {
    let atlas = make_test_atlas();
    let style = Rc::new(Style::default());
    let input = Rc::new(RefCell::new(Input::default()));
    let popup = WindowHandle::popup(RootId::from_raw(1), "combo", atlas, style, input);
    let mut combo = Combo::new(popup);
    let items = ["Apple", "Banana", "Cherry"];

    combo.open = true;
    combo.popup.open();
    let selected = combo.select(1, &items);

    assert_eq!(selected.as_deref(), Some("Banana"));
    assert_eq!(combo.selected, 1);
    assert!(!combo.is_open());
    assert!(!combo.popup.is_open());
}
