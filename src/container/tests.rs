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
    container.content_size = Dimensioni::default();
    container.push_container_body(body, ContainerOption::NONE, WidgetBehaviourOption::NONE);
}

fn make_panel_handle(container: &Container, name: &str) -> ContainerHandle {
    ContainerHandle::new(Container::new(name, container.atlas.clone(), container.style.clone(), container.input.clone()))
}

struct TraceWidget {
    name: &'static str,
    log: Rc<RefCell<Vec<String>>>,
    opt: WidgetOption,
    bopt: WidgetBehaviourOption,
}

impl TraceWidget {
    fn new(name: &'static str, log: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            name,
            log,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
        }
    }
}

impl Widget for TraceWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn behaviour_opt(&self) -> &WidgetBehaviourOption {
        &self.bopt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        self.log.borrow_mut().push(format!("measure {}", self.name));
        Dimensioni::new(10, 10)
    }

    fn run(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        self.log.borrow_mut().push(format!("render {}", self.name));
        ResourceState::NONE
    }
}

#[test]
fn scrollbars_use_current_body() {
    let mut container = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    container.style = Rc::new(style);

    container.body = rect(0, 0, 1, 1);
    container.content_size = Dimensioni::new(0, 0);

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

    container.content_size = Dimensioni::new(200, 200);

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
    state.run(&mut ctx, &control);
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
    state.run(&mut ctx, &control);
    assert_eq!(state.buf, "ab");
    assert_eq!(state.cursor, 1);
}

#[test]
fn node_run_updates_expansion_after_click() {
    let mut container = make_container();
    let mut state = Node::header("Header", NodeStateValue::Closed);
    let node_id = widget_id_of(&state);
    let rect = container.layout.next();
    let control = ControlState {
        hovered: true,
        focused: true,
        clicked: true,
        active: true,
        scroll_delta: None,
    };
    let mut ctx = container.widget_ctx(node_id, rect, None);
    let res = state.run(&mut ctx, &control);

    assert!(res.is_changed());
    assert!(state.is_expanded());
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
fn widget_tree_records_leaf_states() {
    let mut container = make_container();
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));
    let mut results = FrameResults::default();

    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Auto, SizePolicy::Auto], SizePolicy::Auto, |tree| {
            tree.widget(button_a.clone());
            tree.widget(button_b.clone());
        });
    });
    container.widget_tree(&mut results, &tree);

    assert!(results.current().state(widget_id_of_handle(&button_a)).is_none());
    assert!(results.current().state(widget_id_of_handle(&button_b)).is_none());
}

#[test]
fn widget_node_policy_overrides_auto_cell_size() {
    let mut container = make_container();
    let button = widget_handle(Button::new("A"));
    let mut results = FrameResults::default();
    let mut node_id = NodeId::new(0);

    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build(|tree| {
        node_id = tree.widget_with(NodeOptions::with_policy(Policy::fixed(42, 13)), button.clone());
    });
    container.widget_tree(&mut results, &tree);

    let layout = container.current_node_layout(node_id).expect("widget layout missing");
    assert_eq!(layout.rect.width, 42);
    assert_eq!(layout.rect.height, 13);
}

#[test]
fn structural_node_policy_allocates_outer_scope() {
    let mut container = make_container();
    let first = widget_handle(Button::new("A"));
    let second = widget_handle(Button::new("B"));
    let after = widget_handle(Button::new("C"));
    let mut results = FrameResults::default();
    let mut row_id = NodeId::new(0);
    let mut first_id = NodeId::new(0);
    let mut second_id = NodeId::new(0);
    let mut after_id = NodeId::new(0);

    begin_test_frame(&mut container, rect(0, 0, 100, 60));
    let tree = WidgetTreeBuilder::build(|tree| {
        row_id = tree.row_with(
            NodeOptions::with_policy(Policy::fixed(60, 20)),
            &[SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)],
            SizePolicy::Fixed(10),
            |tree| {
                first_id = tree.widget(first.clone());
                second_id = tree.widget(second.clone());
            },
        );
        after_id = tree.widget(after.clone());
    });
    container.widget_tree(&mut results, &tree);

    let row_layout = container.current_node_layout(row_id).expect("row layout missing");
    let first_layout = container.current_node_layout(first_id).expect("first layout missing");
    let second_layout = container.current_node_layout(second_id).expect("second layout missing");
    let after_layout = container.current_node_layout(after_id).expect("following widget layout missing");

    assert_eq!(row_layout.rect.width, 60);
    assert_eq!(row_layout.rect.height, 20);
    assert!(first_layout.rect.x >= row_layout.rect.x);
    assert!(second_layout.rect.x + second_layout.rect.width <= row_layout.rect.x + row_layout.rect.width);
    assert!(after_layout.rect.y >= row_layout.rect.y + row_layout.rect.height);
}

#[test]
fn widget_tree_measures_all_nodes_before_rendering() {
    let mut container = make_container();
    let log = Rc::new(RefCell::new(Vec::new()));
    let first = widget_handle(TraceWidget::new("first", log.clone()));
    let second = widget_handle(TraceWidget::new("second", log.clone()));
    let mut results = FrameResults::default();

    begin_test_frame(&mut container, rect(0, 0, 100, 20));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Auto, SizePolicy::Auto], SizePolicy::Auto, |tree| {
            tree.widget(first.clone());
            tree.widget(second.clone());
        });
    });
    container.widget_tree(&mut results, &tree);

    assert_eq!(
        &*log.borrow(),
        &[
            "measure first".to_string(),
            "measure second".to_string(),
            "render first".to_string(),
            "render second".to_string(),
        ]
    );
}

#[test]
fn retained_widget_value_commits_on_next_frame() {
    let mut container = make_container();
    let input = container.input.clone();
    let checkbox = widget_handle(Checkbox::new("Check", false));
    let seed = 0x1234_5678_u64;
    let mut checkbox_node_id = NodeId::new(0);
    let mut results = FrameResults::default();

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 20));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        checkbox_node_id = tree.widget(checkbox.clone());
    });
    container.widget_tree(&mut results, &tree);
    container.finish();
    results.finish_frame();

    let checkbox_rect = container.previous_node_layout(checkbox_node_id).expect("checkbox layout missing").rect;
    {
        let mut input = input.borrow_mut();
        input.mousemove(checkbox_rect.x + 1, checkbox_rect.y + 1);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 20));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        checkbox_node_id = tree.widget(checkbox.clone());
    });
    container.widget_tree(&mut results, &tree);
    container.finish();
    results.finish_frame();

    {
        let mut input = input.borrow_mut();
        input.mousedown(checkbox_rect.x + 1, checkbox_rect.y + 1, MouseButton::LEFT);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 20));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        checkbox_node_id = tree.widget(checkbox.clone());
    });
    container.widget_tree(&mut results, &tree);

    assert!(results.current().state(widget_id_of_handle(&checkbox)).is_changed());
    assert!(checkbox.borrow().value);
    container.finish();
    results.finish_frame();

    {
        let mut input = input.borrow_mut();
        input.epilogue();
        input.mouseup(checkbox_rect.x + 1, checkbox_rect.y + 1, MouseButton::LEFT);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 20));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        tree.widget(checkbox.clone());
    });
    container.widget_tree(&mut results, &tree);

    assert!(checkbox.borrow().value);
}

#[test]
fn widget_tree_dispatches_panel_children() {
    let mut parent = make_container();
    let panel = make_panel_handle(&parent, "panel");
    let button = widget_handle(Button::new("inside"));
    let mut results = FrameResults::default();

    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
            tree.container(panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                tree.widget(button.clone());
            });
        });
    });
    parent.widget_tree(&mut results, &tree);

    assert!(results.current().state(widget_id_of_handle(&button)).is_none());
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
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(60)], SizePolicy::Fixed(20), |tree| {
            tree.container(panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                tree.widget(text.clone());
            });
        });
    });
    parent.widget_tree(&mut results, &tree);

    let panel = panel.inner();
    assert!(panel.content_size().height > panel.body().height);
}

#[test]
fn tree_nodes_expand_children_from_committed_previous_frame_results() {
    let mut container = make_container();
    let input = container.input.clone();
    let header = widget_handle(Node::header("Header", NodeStateValue::Closed));
    let child = widget_handle(Button::new("Child"));
    let seed = 0xfeed_face_u64;
    let mut header_node_id = NodeId::new(0);
    let mut child_node_id = NodeId::new(0);
    let mut results = FrameResults::default();

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        header_node_id = tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(container.current_node_layout(header_node_id).is_some());
    assert!(container.current_node_layout(child_node_id).is_none());
    container.finish();
    results.finish_frame();

    let header_rect = container.previous_node_layout(header_node_id).expect("header cache missing").rect;
    {
        let mut input = input.borrow_mut();
        input.mousemove(header_rect.x + 1, header_rect.y + 1);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_closed());
    assert!(container.current_node_layout(child_node_id).is_none());
    container.finish();
    results.finish_frame();

    {
        let mut input = input.borrow_mut();
        input.mousedown(header_rect.x + 1, header_rect.y + 1, MouseButton::LEFT);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_expanded());
    assert!(container.current_node_layout(child_node_id).is_none());
    container.finish();
    results.finish_frame();

    {
        let mut input = input.borrow_mut();
        input.epilogue();
        input.mouseup(header_rect.x + 1, header_rect.y + 1, MouseButton::LEFT);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_expanded());
    assert!(container.current_node_layout(child_node_id).is_some());
}

#[test]
fn retained_tree_node_stays_expanded_after_click_is_committed() {
    let mut container = make_container();
    let input = container.input.clone();
    let header = widget_handle(Node::header("Header", NodeStateValue::Closed));
    let child = widget_handle(Button::new("Child"));
    let seed = 0xfeed_face_u64;
    let mut header_node_id = NodeId::new(0);
    let mut child_node_id = NodeId::new(0);
    let mut results = FrameResults::default();

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        header_node_id = tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_closed());
    assert!(container.current_node_layout(child_node_id).is_none());
    container.finish();
    results.finish_frame();

    let header_rect = container.previous_node_layout(header_node_id).expect("header cache missing").rect;
    {
        let mut input = input.borrow_mut();
        input.mousemove(header_rect.x + 1, header_rect.y + 1);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        header_node_id = tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_closed());
    assert!(container.current_node_layout(child_node_id).is_none());
    container.finish();
    results.finish_frame();

    {
        let mut input = input.borrow_mut();
        input.mousedown(header_rect.x + 1, header_rect.y + 1, MouseButton::LEFT);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        header_node_id = tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_expanded());
    assert!(container.current_node_layout(child_node_id).is_none());
    container.finish();
    results.finish_frame();

    {
        let mut input = input.borrow_mut();
        input.epilogue();
        input.mouseup(header_rect.x + 1, header_rect.y + 1, MouseButton::LEFT);
    }

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_expanded());
    assert!(container.current_node_layout(child_node_id).is_some());
    container.finish();
    results.finish_frame();

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
        tree.header(header.clone(), |tree| {
            child_node_id = tree.widget(child.clone());
        });
    });
    container.widget_tree(&mut results, &tree);
    assert!(header.borrow().is_expanded());
    assert!(container.current_node_layout(child_node_id).is_some());
}

#[test]
fn panel_hover_root_switches_between_siblings_on_next_frame() {
    let mut parent = make_container();
    let input = parent.input.clone();
    let left = make_panel_handle(&parent, "left");
    let right = make_panel_handle(&parent, "right");
    let mut results = FrameResults::default();
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
            tree.container(left.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
            tree.container(right.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
        });
    });

    input.borrow_mut().mousemove(75, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(!left.inner().in_hover_root);
    assert!(!right.inner().in_hover_root);
    parent.finish();

    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(!left.inner().in_hover_root);
    assert!(right.inner().in_hover_root);
    parent.finish();

    input.borrow_mut().mousemove(25, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(!left.inner().in_hover_root);
    assert!(right.inner().in_hover_root);
    parent.finish();

    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(left.inner().in_hover_root);
    assert!(!right.inner().in_hover_root);
}

#[test]
fn parent_widgets_are_only_blocked_while_mouse_is_inside_active_child_rect() {
    let mut parent = make_container();
    let input = parent.input.clone();
    let panel = make_panel_handle(&parent, "panel");
    let mut results = FrameResults::default();
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
            tree.container(panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
        });
    });

    input.borrow_mut().mousemove(10, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    parent.finish();

    let blocked_button = Button::new("blocked");
    input.borrow_mut().mousemove(10, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    let blocked = parent.update_control(widget_id_of(&blocked_button), rect(0, 0, 40, 20), &blocked_button);
    assert!(!blocked.hovered);
    parent.widget_tree(&mut results, &tree);
    parent.finish();

    let free_button = Button::new("free");
    input.borrow_mut().mousemove(75, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    let free = parent.update_control(widget_id_of(&free_button), rect(60, 0, 30, 20), &free_button);
    assert!(free.hovered);
}
