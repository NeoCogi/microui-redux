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

    let (_, button) = Button::create(ButtonParameters::with_image("aa", Some(texture), WidgetOption::FRAME, WidgetFillOption::ALL));
    let button_size = button.measure(&style, &atlas, Dimensioni::default());

    assert_eq!(button_size.width, style.padding * 2 + texture.width() + style.padding + 16);
    assert_eq!(button_size.height, 14);

    let (_, list_box) = ListBox::create(ListBoxParameters::new("a", Some(texture)));
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
    let (combo_state, mut combo) = Combo::create(ComboParameters::new());
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
    assert_eq!(combo_state.try_read(ComboState::is_open), Some(true));

    assert_eq!(combo_state.try_update(ComboState::open_popup), Some(()));
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
    assert_eq!(combo_state.try_read(ComboState::is_open), Some(false));
}

#[test]
fn combo_select_updates_label_and_closes_popup() {
    let (combo_state, _combo) = Combo::create(ComboParameters::new());
    let items = ["Apple", "Banana", "Cherry"];

    combo_state.try_update(ComboState::open_popup).unwrap();
    let selected = combo_state.try_update(|combo| combo.select(1, &items)).unwrap();

    assert_eq!(selected.as_deref(), Some("Banana"));
    assert_eq!(combo_state.try_read(ComboState::selected), Some(1));
    assert_eq!(combo_state.try_read(ComboState::is_open), Some(false));
}

#[test]
fn convenience_constructors_store_explicit_outer_frame_policy() {
    assert!(Button::create(ButtonParameters::new("button")).1.widget_opt().intersects(WidgetOption::FRAME));
    assert!(Combo::create(ComboParameters::new()).1.widget_opt().intersects(WidgetOption::FRAME));
    assert!(
        crate::Textbox::create(crate::TextboxParameters::new(""))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
    assert!(
        crate::TextArea::create(crate::TextAreaParameters::new(""))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
    assert!(
        crate::Slider::create(crate::SliderParameters::new(0.0, 0.0, 1.0))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
    assert!(
        crate::Number::create(crate::NumberParameters::new(0.0, 1.0, 0))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
    assert!(
        crate::ColorSwatch::create(crate::ColorSwatchParameters::new(color(0, 0, 0, 255)))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
    let header = crate::Disclosure::create(crate::DisclosureParameters::header("header", false, std::iter::empty())).1;
    assert!(!header.data.widget().effective_widget_opt().intersects(WidgetOption::FRAME));

    let (_, checkbox) = Checkbox::create(CheckboxParameters::new("checkbox", false));
    assert!(!checkbox.widget_opt().intersects(WidgetOption::FRAME));
    assert!(!ListItem::create(ListItemParameters::new("item")).1.widget_opt().intersects(WidgetOption::FRAME));
    assert!(
        !ListBox::create(ListBoxParameters::new("item", None))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
    assert!(!Custom::create(CustomParameters::new("custom")).widget_opt().intersects(WidgetOption::FRAME));
    assert!(
        !crate::TextBlock::create(crate::TextBlockParameters::new("text"))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
    let tree = crate::Disclosure::create(crate::DisclosureParameters::tree("tree", false, std::iter::empty())).1;
    assert!(!tree.data.widget().effective_widget_opt().intersects(WidgetOption::FRAME));

    assert!(
        !Button::create(ButtonParameters::with_opt("flat", WidgetOption::ALIGN_CENTER))
            .1
            .widget_opt()
            .intersects(WidgetOption::FRAME)
    );
}

#[test]
fn typed_click_events_accumulate_and_consume_one_occurrence_at_a_time() {
    let (checkbox_state, mut checkbox) = Checkbox::create(CheckboxParameters::new("check", false));
    let (button_state, mut button) = Button::create(ButtonParameters::new("button"));
    let (item_state, mut item) = ListItem::create(ListItemParameters::new("item"));
    let (list_state, mut list) = ListBox::create(ListBoxParameters::new("list", None));
    let (combo_state, mut combo) = Combo::create(ComboParameters::new());

    for _ in 0..2 {
        run_click(&mut checkbox);
        run_click(&mut button);
        run_click(&mut item);
        run_click(&mut list);
        run_click(&mut combo);
    }

    for _ in 0..2 {
        assert_eq!(checkbox_state.try_update(CheckboxState::take_changed), Some(true));
    }
    assert_eq!(checkbox_state.try_update(CheckboxState::take_changed), Some(false));

    assert_eq!(button_state.try_update(ButtonState::take_submitted), Some(true));
    assert_eq!(button_state.try_update(ButtonState::take_submitted), Some(true));
    assert_eq!(button_state.try_update(ButtonState::take_submitted), Some(false));
    assert_eq!(item_state.try_update(ListItemState::take_submitted), Some(true));
    assert_eq!(item_state.try_update(ListItemState::take_submitted), Some(true));
    assert_eq!(item_state.try_update(ListItemState::take_submitted), Some(false));
    assert_eq!(list_state.try_update(ListBoxState::take_submitted), Some(true));
    assert_eq!(list_state.try_update(ListBoxState::take_submitted), Some(true));
    assert_eq!(list_state.try_update(ListBoxState::take_submitted), Some(false));
    assert_eq!(combo_state.try_update(ComboState::take_submitted), Some(true));
    assert_eq!(combo_state.try_update(ComboState::take_submitted), Some(true));
    assert_eq!(combo_state.try_update(ComboState::take_submitted), Some(false));
}

#[test]
fn combo_programmatic_operations_are_silent_except_item_clamping() {
    let (state, _runtime) = Combo::create(ComboParameters::new());
    let labels = ["zero", "one", "two"];

    state
        .try_update(|combo| {
            combo.open_popup();
            combo.close_popup();
            assert_eq!(combo.select(2, &labels).as_deref(), Some("two"));
        })
        .unwrap();
    assert_eq!(state.try_update(ComboState::take_changed), Some(false));
    assert_eq!(state.try_update(ComboState::take_submitted), Some(false));

    state.try_update(|combo| combo.update_items(&labels[..1])).unwrap();
    assert_eq!(state.try_read(ComboState::selected), Some(0));
    assert_eq!(state.try_update(ComboState::take_changed), Some(true));
    assert_eq!(state.try_update(ComboState::take_changed), Some(false));

    state.try_update(|combo| combo.update_items(&labels[..1])).unwrap();
    assert_eq!(state.try_update(ComboState::take_changed), Some(false));
}

#[test]
fn checkbox_and_list_item_programmatic_setters_are_silent() {
    let (checkbox, _checkbox_runtime) = Checkbox::create(CheckboxParameters::new("check", false));
    checkbox
        .try_update(|state| {
            state.check();
            state.uncheck();
            state.set_checked(true);
        })
        .unwrap();
    assert_eq!(checkbox.try_update(CheckboxState::take_changed), Some(false));

    let (item, _item_runtime) = ListItem::create(ListItemParameters::new("before"));
    item.try_update(|state| state.set_label("after")).unwrap();
    assert_eq!(item.try_update(ListItemState::take_submitted), Some(false));
}

#[test]
fn pending_event_counters_saturate_without_wrapping() {
    let mut pending = u32::MAX;
    crate::widgets::record_pending_event(&mut pending);
    assert_eq!(pending, u32::MAX);
    assert!(crate::widgets::take_pending_event(&mut pending));
    assert_eq!(pending, u32::MAX - 1);
}

#[test]
fn custom_constructor_returns_only_the_runtime_and_retains_unit_state() {
    fn assert_runtime(_: Custom) {}

    let runtime = Custom::create(CustomParameters::new("custom"));
    let state = runtime.state_handle();
    assert!(state.is_alive());
    assert_runtime(runtime);
    assert!(!state.is_alive());
}
