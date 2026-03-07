//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
use super::*;
use crate::{AtlasSource, FontEntry, SourceFormat};
use std::{cell::RefCell, rc::Rc};

const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

fn make_test_atlas() -> AtlasHandle {
    let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
    let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
    let entries = vec![
        (
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
        (
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
        (
            'b',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
    ];
    let fonts = vec![(
        "default",
        FontEntry {
            line_size: 10,
            baseline: 8,
            font_size: 10,
            entries: &entries,
        },
    )];
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
        slots: &[],
    };
    AtlasHandle::from(&source)
}

fn make_container() -> Container {
    let atlas = make_test_atlas();
    let input = Rc::new(RefCell::new(Input::default()));
    let mut container = Container::new("test", atlas, Rc::new(Style::default()), input);
    container.in_hover_root = true;
    container.push_container_body(rect(0, 0, 100, 30), ContainerOption::NONE, WidgetBehaviourOption::NONE);
    container
}

fn begin_test_frame(container: &mut Container, body: Recti) {
    container.prepare();
    container.rect = body;
    container.content_size = Vec2i::default();
    container.push_container_body(body, ContainerOption::NONE, WidgetBehaviourOption::NONE);
}

fn make_panel_handle(container: &Container, name: &str) -> ContainerHandle {
    ContainerHandle::new(Container::new(name, container.atlas.clone(), container.style.clone(), container.input.clone()))
}

#[test]
fn scrollbars_use_current_body() {
    let mut container = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    container.style = Rc::new(style);

    container.body = rect(0, 0, 1, 1);
    container.content_size = Vec2i::new(0, 0);

    let mut body = rect(0, 0, 100, 100);
    container.scrollbars(&mut body);

    assert_eq!(body.width, 100);
    assert_eq!(body.height, 100);
}

#[test]
fn scrollbars_shrink_body_when_needed() {
    let mut container = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    container.style = Rc::new(style);

    container.content_size = Vec2i::new(200, 200);

    let mut body = rect(0, 0, 100, 100);
    container.scrollbars(&mut body);

    assert_eq!(body.width, 90);
    assert_eq!(body.height, 90);
}

#[test]
fn textbox_left_moves_over_multibyte() {
    let mut container = make_container();
    let input = container.input.clone();
    let mut state = Textbox::new("a\u{1F600}b");
    let textbox_id = widget_id_of(&state);
    container.set_focus(Some(textbox_id));
    state.cursor = 5;

    input.borrow_mut().keydown_code(KeyCode::LEFT);
    let rect = container.layout.next();
    let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.bopt);
    let control = container.update_control(textbox_id, rect, &control_state);
    let input = container.snapshot_input();
    let mut ctx = container.widget_ctx(textbox_id, rect, Some(input));
    state.render(&mut ctx, &control);

    assert_eq!(state.cursor, 1);
}

#[test]
fn textbox_backspace_removes_multibyte() {
    let mut container = make_container();
    let input = container.input.clone();
    let mut state = Textbox::new("a\u{1F600}b");
    let textbox_id = widget_id_of(&state);
    container.set_focus(Some(textbox_id));
    state.cursor = 5;

    input.borrow_mut().keydown(KeyMode::BACKSPACE);
    let rect = container.layout.next();
    let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.bopt);
    let control = container.update_control(textbox_id, rect, &control_state);
    let input = container.snapshot_input();
    let mut ctx = container.widget_ctx(textbox_id, rect, Some(input));
    state.render(&mut ctx, &control);

    assert_eq!(state.buf, "ab");
    assert_eq!(state.cursor, 1);
}

#[test]
fn widget_textbox_backspace_removes_multibyte() {
    let mut container = make_container();
    let input = container.input.clone();
    let mut state = Textbox::new("a\u{1F600}b");
    container.set_focus(Some(widget_id_of(&state)));
    state.cursor = 5;
    let mut results = FrameResults::default();
    let state_id = widget_id_of(&state);

    input.borrow_mut().keydown(KeyMode::BACKSPACE);
    let mut runs = [widget_ref(&mut state)];
    container.widgets(&mut results, &mut runs);

    assert!(results.state(state_id).is_changed());
    assert_eq!(state.buf, "ab");
    assert_eq!(state.cursor, 1);
}

#[test]
fn clicking_away_does_not_refocus_stale_hover_widget() {
    let mut container = make_container();
    let input = container.input.clone();
    let button = Button::new("A");
    let button_id = widget_id_of(&button);
    let button_rect = rect(0, 0, 50, 20);

    input.borrow_mut().mousemove(10, 10);
    let control = container.update_control(button_id, button_rect, &button);
    assert!(control.hovered);
    assert!(!control.focused);

    {
        let mut input = input.borrow_mut();
        input.mousemove(80, 10);
        input.mousedown(80, 10, MouseButton::LEFT);
        input.mouseup(80, 10, MouseButton::LEFT);
    }
    let control = container.update_control(button_id, button_rect, &button);
    assert!(!control.hovered);
    assert!(!control.focused);
}

#[test]
fn row_widgets_record_states() {
    let mut container = make_container();
    let mut button_a = Button::new("A");
    let mut button_b = Button::new("B");
    let mut results = FrameResults::default();
    let button_a_id = widget_id_of(&button_a);
    let button_b_id = widget_id_of(&button_b);
    let mut runs = [widget_ref(&mut button_a), widget_ref(&mut button_b)];

    container.row_widgets(&mut results, &[SizePolicy::Auto, SizePolicy::Auto], SizePolicy::Auto, &mut runs);

    assert!(results.state(button_a_id).is_none());
    assert!(results.state(button_b_id).is_none());
}

#[test]
fn widget_tree_records_leaf_states() {
    let mut container = make_container();
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));
    let mut results = FrameResults::default();

    container.build_tree(&mut results, |tree| {
        tree.row(&[SizePolicy::Auto, SizePolicy::Auto], SizePolicy::Auto, |tree| {
            tree.widget(button_a.clone());
            tree.widget(button_b.clone());
        });
    });

    assert!(results.state_of_handle(&button_a).is_none());
    assert!(results.state_of_handle(&button_b).is_none());
}

#[test]
fn widget_tree_dispatches_panel_children() {
    let mut parent = make_container();
    let panel = make_panel_handle(&parent, "panel");
    let button = widget_handle(Button::new("inside"));
    let mut results = FrameResults::default();

    parent.build_tree(&mut results, |tree| {
        tree.row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
            tree.container(panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                tree.widget(button.clone());
            });
        });
    });

    assert!(results.state_of_handle(&button).is_none());
}

#[test]
fn retained_text_inside_panel_grows_content_height() {
    let mut parent = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    parent.style = Rc::new(style);

    let panel = make_panel_handle(&parent, "panel");
    let text = widget_handle(TextBlock::new("a\na\na\na"));
    let mut results = FrameResults::default();

    begin_test_frame(&mut parent, rect(0, 0, 60, 20));
    parent.build_tree(&mut results, |tree| {
        tree.row(&[SizePolicy::Fixed(60)], SizePolicy::Fixed(20), |tree| {
            tree.container(panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                tree.widget(text.clone());
            });
        });
    });

    let panel = panel.inner();
    assert!(panel.content_size().y > panel.body().height);
}

#[test]
fn tree_nodes_expand_children_in_same_frame_from_cached_rects() {
    let mut container = make_container();
    let input = container.input.clone();
    let header = widget_handle(Node::header("Header", NodeStateValue::Closed));
    let child = widget_handle(Button::new("Child"));
    let seed = 0xfeed_face_u64;
    let mut header_node_id = NodeId::new(0);
    let mut child_node_id = NodeId::new(0);

    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let mut results = FrameResults::default();
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        header_node_id = tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(container.current_node_state(header_node_id).is_some());
    assert!(container.current_node_state(child_node_id).is_none());
    container.finish();

    let header_rect = container.previous_node_state(header_node_id).expect("header cache missing").rect;
    {
        let mut input = input.borrow_mut();
        input.mousemove(header_rect.x + 1, header_rect.y + 1);
        input.mousedown(header_rect.x + 1, header_rect.y + 1, MouseButton::LEFT);
    }

    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let mut results = FrameResults::default();
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);

    assert!(header.borrow().is_expanded());
    assert!(container.current_node_state(child_node_id).is_some());
    container.finish();
}

#[test]
fn panel_hover_root_switches_between_siblings_on_next_frame() {
    let mut parent = make_container();
    let input = parent.input.clone();
    let mut left = make_panel_handle(&parent, "left");
    let mut right = make_panel_handle(&parent, "right");

    input.borrow_mut().mousemove(75, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
        container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
        container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
    });
    assert!(!left.inner().in_hover_root);
    assert!(!right.inner().in_hover_root);
    parent.finish();

    let mut left_active = false;
    let mut right_active = false;
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
        container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
            left_active = panel.inner().in_hover_root;
        });
        container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
            right_active = panel.inner().in_hover_root;
        });
    });
    assert!(!left_active);
    assert!(right_active);
    parent.finish();

    input.borrow_mut().mousemove(25, 10);
    left_active = false;
    right_active = false;
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
        container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
            left_active = panel.inner().in_hover_root;
        });
        container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
            right_active = panel.inner().in_hover_root;
        });
    });
    assert!(!left_active);
    assert!(right_active);
    parent.finish();

    left_active = false;
    right_active = false;
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
        container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
            left_active = panel.inner().in_hover_root;
        });
        container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
            right_active = panel.inner().in_hover_root;
        });
    });
    assert!(left_active);
    assert!(!right_active);
}

#[test]
fn parent_widgets_are_only_blocked_while_mouse_is_inside_active_child_rect() {
    let mut parent = make_container();
    let input = parent.input.clone();
    let mut panel = make_panel_handle(&parent, "panel");

    input.borrow_mut().mousemove(10, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.with_row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
        container.panel(&mut panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
    });
    parent.finish();

    let blocked_button = Button::new("blocked");
    input.borrow_mut().mousemove(10, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    let blocked = parent.update_control(widget_id_of(&blocked_button), rect(0, 0, 40, 20), &blocked_button);
    assert!(!blocked.hovered);
    parent.with_row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
        container.panel(&mut panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
    });
    parent.finish();

    let free_button = Button::new("free");
    input.borrow_mut().mousemove(75, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    let free = parent.update_control(widget_id_of(&free_button), rect(60, 0, 30, 20), &free_button);
    assert!(free.hovered);
}
