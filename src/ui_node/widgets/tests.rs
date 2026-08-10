//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Tests for behavior shared across built-in widgets.

use super::*;
use crate::test_support::test_atlas as make_test_atlas;
use std::rc::Rc;

fn run_click<W: Widget>(widget: &mut W) {
    let atlas = make_test_atlas();
    let style = Style::default();
    let bounds = rect(0, 0, 100, 20);
    let mut ctx = WidgetUpdateCtx::new_with_interaction(
        bounds,
        bounds,
        &style,
        &atlas,
        true,
        true,
        true,
        true,
        true,
        MouseButton::LEFT,
        KeyMode::NONE,
        KeyCode::NONE,
    );
    widget.update(&mut ctx, None)
}

#[test]
fn image_widgets_measure_external_texture_dimensions() {
    let atlas = make_test_atlas();
    let style = Style::default();
    let texture = TextureId::new(7, 13, 5);

    let button = ButtonBuilder::create_widget(ButtonParameters::with_image("aa", Some(texture), WidgetOption::FRAME, WidgetFillOption::ALL));
    let button_size = button.measure(&style, &atlas, Dimensioni::default());

    assert_eq!(button_size.width, style.padding * 2 + texture.width() + style.padding + 16);
    assert_eq!(button_size.height, 14);

    let list_box = ListBoxBuilder::create_widget(ListBoxParameters::new("a", Some(texture)));
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
    let mut combo = ComboBuilder::create_widget(ComboParameters::new());
    let rect = rect(0, 0, 100, 20);
    let mut ctx = WidgetUpdateCtx::new_with_interaction(
        rect,
        rect,
        style.as_ref(),
        &atlas,
        true,
        true,
        true,
        true,
        true,
        MouseButton::LEFT,
        KeyMode::NONE,
        KeyCode::NONE,
    );

    combo.update(&mut ctx, None);
    assert!(combo.is_open());

    combo.open_popup();
    let mut ctx = WidgetUpdateCtx::new_with_interaction(
        rect,
        rect,
        style.as_ref(),
        &atlas,
        true,
        true,
        true,
        true,
        true,
        MouseButton::LEFT,
        KeyMode::NONE,
        KeyCode::NONE,
    );
    combo.update(&mut ctx, None);
    assert!(!combo.is_open());
}

#[test]
fn combo_select_updates_label_and_closes_popup() {
    let mut combo = ComboBuilder::create_widget(ComboParameters::new());
    let items = ["Apple", "Banana", "Cherry"];

    combo.open_popup();
    let selected = combo.select(1, &items);

    assert_eq!(selected.as_deref(), Some("Banana"));
    assert_eq!(combo.selected(), 1);
    assert!(!combo.is_open());
}

#[test]
fn convenience_constructors_store_explicit_outer_frame_policy() {
    fn has_option<W: Widget + 'static>(handle: &TypedWidgetHandle<W>, option: WidgetOption) -> bool {
        handle.try_read(|widget| widget.widget_opt().intersects(option)).unwrap()
    }

    let (button, _button_node) = Button::create(ButtonParameters::new("button"));
    let (combo, _combo_node) = Combo::create(ComboParameters::new());
    let (textbox, _textbox_node) = crate::Textbox::create(crate::TextboxParameters::new(""));
    let (text_area, _text_area_node) = crate::TextArea::create(crate::TextAreaParameters::new(""));
    let (slider, _slider_node) = crate::Slider::create(crate::SliderParameters::new(0.0, 0.0, 1.0));
    let (number, _number_node) = crate::Number::create(crate::NumberParameters::new(0.0, 1.0, 0));
    let (swatch, _swatch_node) = crate::ColorSwatch::create(crate::ColorSwatchParameters::new(color(0, 0, 0, 255)));
    assert!(has_option(&button, WidgetOption::FRAME));
    assert!(has_option(&combo, WidgetOption::FRAME));
    assert!(has_option(&textbox, WidgetOption::FRAME));
    assert!(has_option(&text_area, WidgetOption::FRAME));
    assert!(has_option(&slider, WidgetOption::FRAME));
    assert!(has_option(&number, WidgetOption::FRAME));
    assert!(has_option(&swatch, WidgetOption::FRAME));
    let header = crate::Disclosure::create(crate::DisclosureParameters::header("header", false, std::iter::empty())).1;
    assert!(!header.data.with_widget(|widget| widget.effective_widget_opt().intersects(WidgetOption::FRAME)));

    let (checkbox, _checkbox_node) = Checkbox::create(CheckboxParameters::new("checkbox", false));
    let (item, _item_node) = ListItem::create(ListItemParameters::new("item"));
    let (list, _list_node) = ListBox::create(ListBoxParameters::new("item", None));
    assert!(!has_option(&checkbox, WidgetOption::FRAME));
    assert!(!has_option(&item, WidgetOption::FRAME));
    assert!(!has_option(&list, WidgetOption::FRAME));
    assert!(!Custom::create(CustomParameters::new("custom")).widget_opt().intersects(WidgetOption::FRAME));
    let (text, _text_node) = crate::TextBlock::create(crate::TextBlockParameters::new("text"));
    assert!(!has_option(&text, WidgetOption::FRAME));
    let tree = crate::Disclosure::create(crate::DisclosureParameters::tree("tree", false, std::iter::empty())).1;
    assert!(!tree.data.with_widget(|widget| widget.effective_widget_opt().intersects(WidgetOption::FRAME)));

    let (flat, _flat_node) = Button::create(ButtonParameters::with_opt("flat", WidgetOption::ALIGN_CENTER));
    assert!(!has_option(&flat, WidgetOption::FRAME));
}

#[derive(Debug, Eq, PartialEq)]
enum ClickMessage {
    Checkbox(bool),
    Button,
    Item(String),
    ListBox,
    Combo(bool),
}

#[test]
fn typed_click_events_dispatch_every_occurrence_in_order() {
    let mut checkbox = CheckboxBuilder::create_widget(CheckboxParameters::new("check", false));
    let mut button = ButtonBuilder::create_widget(ButtonParameters::new("button"));
    let mut item = ListItemBuilder::create_widget(ListItemParameters::new("item"));
    let mut list = ListBoxBuilder::create_widget(ListBoxParameters::new("list", None));
    let mut combo = ComboBuilder::create_widget(ComboParameters::new());

    let mut session = crate::Session::new();
    session.connect(checkbox.changed(), |event| ClickMessage::Checkbox(event.checked)).unwrap();
    session.connect(button.submitted(), |_| ClickMessage::Button).unwrap();
    session.connect(item.submitted(), |event| ClickMessage::Item(event.label)).unwrap();
    session.connect(list.submitted(), |_| ClickMessage::ListBox).unwrap();
    session.connect(combo.submitted(), |event| ClickMessage::Combo(event.open)).unwrap();
    session.subscribe(|events: &mut Vec<ClickMessage>, event: &ClickMessage, _| {
        events.push(match event {
            ClickMessage::Checkbox(value) => ClickMessage::Checkbox(*value),
            ClickMessage::Button => ClickMessage::Button,
            ClickMessage::Item(label) => ClickMessage::Item(label.clone()),
            ClickMessage::ListBox => ClickMessage::ListBox,
            ClickMessage::Combo(open) => ClickMessage::Combo(*open),
        });
    });

    for _ in 0..2 {
        run_click(&mut checkbox);
        run_click(&mut button);
        run_click(&mut item);
        run_click(&mut list);
        run_click(&mut combo);
    }

    let mut events = Vec::new();
    assert!(session.dispatch(&mut events));
    assert_eq!(
        events,
        [
            ClickMessage::Checkbox(true),
            ClickMessage::Button,
            ClickMessage::Item("item".to_owned()),
            ClickMessage::ListBox,
            ClickMessage::Combo(true),
            ClickMessage::Checkbox(false),
            ClickMessage::Button,
            ClickMessage::Item("item".to_owned()),
            ClickMessage::ListBox,
            ClickMessage::Combo(false),
        ]
    );
}

#[test]
fn combo_selection_and_item_clamping_emit_only_value_changes() {
    let (state, _node) = Combo::create(ComboParameters::new());
    let labels = ["zero", "one", "two"];

    let mut session = crate::Session::new();
    session.connect(state.changed(), |event| (event.selected, event.label)).unwrap();
    session.subscribe(|events: &mut Vec<(usize, String)>, event: &(usize, String), _| events.push(event.clone()));
    let mut events = Vec::new();

    state
        .try_update(|combo| {
            combo.open_popup();
            combo.close_popup();
            assert_eq!(combo.select(2, &labels).as_deref(), Some("two"));
        })
        .unwrap();
    assert!(session.dispatch(&mut events));
    assert_eq!(events, [(2, "two".to_owned())]);

    state.try_update(|combo| combo.update_items(&labels[..1])).unwrap();
    assert_eq!(state.try_read(Combo::selected), Some(0));
    assert!(session.dispatch(&mut events));
    assert_eq!(events, [(2, "two".to_owned()), (0, "zero".to_owned())]);

    state.try_update(|combo| combo.update_items(&labels[..1])).unwrap();
    assert!(!session.dispatch(&mut events));
}

#[test]
fn checkbox_and_list_item_programmatic_setters_are_silent() {
    let (checkbox, _checkbox_node) = Checkbox::create(CheckboxParameters::new("check", false));
    let (item, _item_node) = ListItem::create(ListItemParameters::new("before"));
    let mut session = crate::Session::<(), ()>::new();
    session.connect(checkbox.changed(), |_| ()).unwrap();
    session.connect(item.submitted(), |_| ()).unwrap();

    checkbox
        .try_update(|state| {
            state.check();
            state.uncheck();
            state.set_checked(true);
        })
        .unwrap();
    item.try_update(|state| state.set_label("after")).unwrap();
    assert!(!session.dispatch(&mut ()));
}

#[test]
fn connecting_an_event_does_not_borrow_semantic_widget_state() {
    let (button, _node) = Button::create(ButtonParameters::new("button"));
    let submitted = button.submitted();
    let mut session = crate::Session::<(), ()>::new();

    let connected = button
        .try_update(|_| session.connect(submitted, |_| ()))
        .expect("button state should remain available");

    assert_eq!(connected, Ok(()));
}

#[test]
fn custom_widgets_need_no_unit_state_allocation() {
    let (handle, node) = Node::typed_widget(Custom::create(CustomParameters::new("custom")));
    assert!(handle.is_alive());
    drop(node);
    assert!(!handle.is_alive());
}
