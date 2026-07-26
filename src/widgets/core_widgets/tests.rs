//! Tests for basic widget sizing and state behavior.

use super::*;
use crate::render::DisplayList;
use crate::test_support::test_atlas as make_test_atlas;
use std::rc::Rc;

#[test]
fn image_widgets_measure_external_texture_dimensions() {
    let atlas = make_test_atlas();
    let style = Style::default();
    let texture = TextureId::new(7, 13, 5);

    let button = Button::with_image("aa", Some(texture), WidgetOption::FRAME, WidgetFillOption::ALL);
    let button_size = button.measure(&style, &atlas, Dimensioni::default());

    assert_eq!(button_size.width, style.padding * 2 + texture.width() + style.padding + 16);
    assert_eq!(button_size.height, 14);

    let list_box = ListBox::new("a", Some(texture));
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
    let mut combo = Combo::new();
    let mut display_list = DisplayList::new();
    let mut focus = None;
    let mut updated_focus = false;
    let rect = rect(0, 0, 100, 20);
    let mut ctx = WidgetCtx::new_with_interaction(
        Id::new(1),
        rect,
        &mut display_list,
        rect,
        style.as_ref(),
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        true,
        true,
        true,
        true,
        None,
    );

    combo.update(&mut ctx, Vec::new());
    assert!(combo.is_open());

    combo.open_popup();
    let mut ctx = WidgetCtx::new_with_interaction(
        Id::new(1),
        rect,
        &mut display_list,
        rect,
        style.as_ref(),
        &atlas,
        &mut focus,
        &mut updated_focus,
        true,
        true,
        true,
        true,
        true,
        None,
    );
    combo.update(&mut ctx, Vec::new());
    assert!(!combo.is_open());
}

#[test]
fn combo_select_updates_label_and_closes_popup() {
    let mut combo = Combo::new();
    let items = ["Apple", "Banana", "Cherry"];

    combo.open_popup();
    let selected = combo.select(1, &items);

    assert_eq!(selected.as_deref(), Some("Banana"));
    assert_eq!(combo.selected(), 1);
    assert!(!combo.is_open());
}

#[test]
fn convenience_constructors_store_explicit_outer_frame_policy() {
    assert!(Button::new("button").widget_opt().intersects(WidgetOption::FRAME));
    assert!(Combo::new().widget_opt().intersects(WidgetOption::FRAME));
    assert!(crate::Textbox::new("").widget_opt().intersects(WidgetOption::FRAME));
    assert!(crate::TextArea::new("").widget_opt().intersects(WidgetOption::FRAME));
    assert!(crate::Slider::new(0.0, 0.0, 1.0).widget_opt().intersects(WidgetOption::FRAME));
    assert!(crate::Number::new(0.0, 1.0, 0).widget_opt().intersects(WidgetOption::FRAME));
    assert!(crate::ColorSwatch::new(color(0, 0, 0, 255)).widget_opt().intersects(WidgetOption::FRAME));
    assert!(
        crate::Node::header("header", crate::NodeStateValue::Closed)
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );

    assert!(!Checkbox::new("checkbox", false).widget_opt().intersects(WidgetOption::FRAME));
    assert!(!ListItem::new("item").widget_opt().intersects(WidgetOption::FRAME));
    assert!(!ListBox::new("item", None).widget_opt().intersects(WidgetOption::FRAME));
    assert!(!Custom::new("custom").widget_opt().intersects(WidgetOption::FRAME));
    assert!(!crate::TextBlock::new("text").widget_opt().intersects(WidgetOption::FRAME));
    assert!(
        !crate::Node::tree("tree", crate::NodeStateValue::Closed)
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );

    assert!(
        !Button::with_opt("flat", WidgetOption::ALIGN_CENTER)
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
}
