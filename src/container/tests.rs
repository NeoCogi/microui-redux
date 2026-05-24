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
//! Tests for traversal-host layout, retained traversal, scroll areas, focus, and draw command behavior.
use super::*;
use crate::test_support::{test_atlas, NoopRenderer};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn make_container() -> TraversalHost {
    let atlas = test_atlas();
    let input = Rc::new(RefCell::new(Input::default()));
    let mut container = TraversalHost::new("test", atlas, Rc::new(Style::default()), input);
    container.interaction.in_hover_root = true;
    container.push_container_body(rect(0, 0, 100, 30), ContainerOption::NONE, ScrollBehavior::NONE);
    container
}

fn begin_test_frame(container: &mut TraversalHost, body: Recti) {
    container.prepare();
    container.set_rect(body);
    container.set_content_size(Dimensioni::default());
    container.push_container_body(body, ContainerOption::NONE, ScrollBehavior::NONE);
}

fn update_control_for_widget<W: Widget + ?Sized>(container: &mut TraversalHost, node_id: NodeId, rect: Recti, state: &W) -> ControlState {
    container.update_control_for_node(
        node_id,
        rect,
        state.effective_widget_opt(),
        state.effective_scroll_behavior(),
        state.focus_policy(),
    )
}

fn widget_ctx_for_node<'a>(container: &'a mut TraversalHost, node_id: NodeId, rect: Recti, input: Option<Rc<InputSnapshot>>) -> WidgetCtx<'a> {
    let interaction_id = container.retained_id_for_node(node_id);
    container.widget_ctx_for(interaction_id, rect, input)
}

fn make_scroll_area_handle(container: &TraversalHost, name: &str) -> ScrollAreaHandle {
    ScrollAreaHandle::new(ScrollArea::new(name, container.atlas.clone(), container.style.clone(), container.input.clone()))
}

struct TraceWidget {
    name: &'static str,
    log: Rc<RefCell<Vec<String>>>,
    opt: WidgetOption,
    scroll_behavior: ScrollBehavior,
}

struct FocusProbe {
    focused: Rc<Cell<bool>>,
    opt: WidgetOption,
    scroll_behavior: ScrollBehavior,
}

impl FocusProbe {
    fn new(focused: Rc<Cell<bool>>) -> Self {
        Self {
            focused,
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }
}

impl Widget for FocusProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.scroll_behavior
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        Dimensioni::new(30, 10)
    }

    fn update(&mut self, _ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        self.focused.set(control.focused);
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) {}

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::HoldUntilBlur
    }
}

impl TraceWidget {
    fn new(name: &'static str, log: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            name,
            log,
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }
}

impl Widget for TraceWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.scroll_behavior
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        self.log.borrow_mut().push(format!("measure {}", self.name));
        Dimensioni::new(10, 10)
    }

    fn update(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        self.log.borrow_mut().push(format!("update {}", self.name));
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) {
        self.log.borrow_mut().push(format!("paint {}", self.name));
    }
}

#[test]
fn scrollbars_use_current_body() {
    let mut container = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    container.style = Rc::new(style);

    container.set_body(rect(0, 0, 1, 1));
    container.set_content_size(Dimensioni::new(0, 0));

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

    container.set_content_size(Dimensioni::new(200, 200));

    let mut body = rect(0, 0, 100, 100);
    container.scrollbars(&mut body);

    assert_eq!(body.width, 90);
    assert_eq!(body.height, 90);
}

#[test]
fn vertical_scrollbar_can_force_horizontal_scrollbar_gutter() {
    let mut container = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    container.style = Rc::new(style);

    container.set_content_size(Dimensioni::new(95, 200));

    let original = rect(0, 0, 100, 100);
    let mut body = original;
    container.scrollbars(&mut body);

    let (_, horizontal_scrollbar) = container.scrollbar_node_ids_for_test();
    let scrollbar = container
        .current_node_layout(horizontal_scrollbar)
        .expect("horizontal scrollbar layout missing");

    assert_eq!(body.width, 90);
    assert_eq!(body.height, 90);
    assert_eq!(scrollbar.rect.y, body.y + body.height);
    assert_eq!(scrollbar.rect.y + scrollbar.rect.height, original.y + original.height);
}

#[test]
fn horizontal_scrollbar_can_force_vertical_scrollbar_gutter() {
    let mut container = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    container.style = Rc::new(style);

    container.set_content_size(Dimensioni::new(200, 95));

    let original = rect(0, 0, 100, 100);
    let mut body = original;
    container.scrollbars(&mut body);

    let (vertical_scrollbar, _) = container.scrollbar_node_ids_for_test();
    let scrollbar = container.current_node_layout(vertical_scrollbar).expect("vertical scrollbar layout missing");

    assert_eq!(body.width, 90);
    assert_eq!(body.height, 90);
    assert_eq!(scrollbar.rect.x, body.x + body.width);
    assert_eq!(scrollbar.rect.x + scrollbar.rect.width, original.x + original.width);
}

#[test]
fn textbox_left_moves_over_multibyte() {
    let mut container = make_container();
    let input = container.input.clone();
    let mut state = Textbox::new("a\u{1F600}b");
    let textbox_id = NodeId::new(0x1001);
    container.set_focus_node(textbox_id);
    state.set_cursor(5);

    input.borrow_mut().keydown_code(KeyCode::LEFT);
    let rect = container.layout.next();
    let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.scroll_behavior);
    let control = update_control_for_widget(&mut container, textbox_id, rect, &control_state);
    let input = container.snapshot_input();
    let mut ctx = widget_ctx_for_node(&mut container, textbox_id, rect, Some(input));
    state.update(&mut ctx, &control);
    assert_eq!(state.cursor(), 1);
}

#[test]
fn textbox_backspace_removes_multibyte() {
    let mut container = make_container();
    let input = container.input.clone();
    let mut state = Textbox::new("a\u{1F600}b");
    let textbox_id = NodeId::new(0x1002);
    container.set_focus_node(textbox_id);
    state.set_cursor(5);

    input.borrow_mut().keydown(KeyMode::BACKSPACE);
    let rect = container.layout.next();
    let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.scroll_behavior);
    let control = update_control_for_widget(&mut container, textbox_id, rect, &control_state);
    let input = container.snapshot_input();
    let mut ctx = widget_ctx_for_node(&mut container, textbox_id, rect, Some(input));
    state.update(&mut ctx, &control);
    assert_eq!(state.text(), "ab");
    assert_eq!(state.cursor(), 1);
}

#[test]
fn node_run_updates_expansion_after_click() {
    let mut container = make_container();
    let mut state = Node::header("Header", NodeStateValue::Closed);
    let node_id = NodeId::new(0x1003);
    let rect = container.layout.next();
    let control = ControlState {
        hovered: true,
        focused: true,
        clicked: true,
        active: true,
        scroll_delta: None,
    };
    let mut ctx = widget_ctx_for_node(&mut container, node_id, rect, None);
    let res = state.update(&mut ctx, &control);

    assert!(res.is_changed());
    assert!(state.is_expanded());
}

#[test]
fn clicking_away_does_not_refocus_stale_hover_widget() {
    let mut container = make_container();
    let input = container.input.clone();
    let button = Button::new("A");
    let button_id = NodeId::new(0x1004);
    let button_rect = rect(0, 0, 50, 20);

    input.borrow_mut().mousemove(10, 10);
    let control = update_control_for_widget(&mut container, button_id, button_rect, &button);
    assert!(control.hovered);
    assert!(!control.focused);

    {
        let mut input = input.borrow_mut();
        input.mousemove(80, 10);
        input.mousedown(80, 10, MouseButton::LEFT);
        input.mouseup(80, 10, MouseButton::LEFT);
    }
    let control = update_control_for_widget(&mut container, button_id, button_rect, &button);
    assert!(!control.hovered);
    assert!(!control.focused);
}

#[test]
fn focus_policy_holds_focus_without_hold_focus_widget_option() {
    let mut container = make_container();
    let focused = Rc::new(Cell::new(false));
    let probe = FocusProbe::new(focused);
    let probe_id = NodeId::new(0x1005);
    container.set_focus_node(probe_id);

    let control = update_control_for_widget(&mut container, probe_id, rect(0, 0, 50, 20), &probe);

    assert!(control.focused);
    assert!(!probe.widget_opt().is_holding_focus());
}

#[test]
fn widget_tree_records_leaf_states() {
    let mut container = make_container();
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));
    let mut results = FrameResults::default();
    let mut button_a_node = NodeId::new(0);
    let mut button_b_node = NodeId::new(0);

    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Auto, SizePolicy::Auto], SizePolicy::Auto, |tree| {
            button_a_node = tree.widget(button_a.clone());
            button_b_node = tree.widget(button_b.clone());
        });
    });
    container.widget_tree(&mut results, &tree);

    assert!(results.current().state_of_node(button_a_node).is_none());
    assert!(results.current().state_of_node(button_b_node).is_none());
}

#[test]
fn widget_node_policy_overrides_auto_cell_size() {
    let mut container = make_container();
    let button = widget_handle(Button::new("A"));
    let mut results = FrameResults::default();
    let mut node_id = NodeId::new(0);

    begin_test_frame(&mut container, rect(0, 0, 100, 40));
    let tree = WidgetTreeBuilder::build(|tree| {
        node_id = tree.node(NodeOptions::with_policy(Policy::fixed(42, 13))).widget(button.clone());
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
        row_id = tree.node(NodeOptions::with_policy(Policy::fixed(60, 20))).row(
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
fn widget_tree_measures_and_updates_all_nodes_before_painting() {
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
            "update first".to_string(),
            "update second".to_string(),
            "paint first".to_string(),
            "paint second".to_string(),
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

    let retained_checkbox_id = container.retained_id_for_node(checkbox_node_id);
    assert!(results.current().state_of_retained(retained_checkbox_id).is_changed());
    assert!(checkbox.read(|checkbox| checkbox.value));
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

    assert!(checkbox.read(|checkbox| checkbox.value));
}

#[test]
fn widget_tree_dispatches_scroll_area_children() {
    let mut parent = make_container();
    let scroll_area = make_scroll_area_handle(&parent, "scroll area");
    let button = widget_handle(Button::new("inside"));
    let mut results = FrameResults::default();
    let mut button_node_id = NodeId::default();

    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
            tree.scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                button_node_id = tree.widget(button.clone());
            });
        });
    });
    parent.widget_tree(&mut results, &tree);

    assert!(results.current().state_of_node(button_node_id).is_none());
}

#[test]
fn measurement_tree_does_not_mutate_live_root_or_scroll_area_state() {
    let mut parent = make_container();
    let scroll_area = make_scroll_area_handle(&parent, "scroll area");
    let text = widget_handle(TextBlock::new("scroll area child"));
    let results = FrameResults::default();
    let mut scroll_area_node_id = NodeId::new(0);

    begin_test_frame(&mut parent, rect(0, 0, 80, 30));
    let tree = WidgetTreeBuilder::build(|tree| {
        scroll_area_node_id = tree.scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
            tree.widget(text.clone());
        });
    });

    let measured = parent.measure_widget_tree_content(&results, &tree);

    assert!(measured.width > 0);
    assert!(measured.height > 0);
    assert!(parent.current_node_layout(scroll_area_node_id).is_none());
    let scroll_area = scroll_area.inner();
    let rect = scroll_area.rect();
    let body = scroll_area.body();
    let content_size = scroll_area.content_size();
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (0, 0, 0, 0));
    assert_eq!((body.x, body.y, body.width, body.height), (0, 0, 0, 0));
    assert_eq!((content_size.width, content_size.height), (0, 0));
}

#[test]
fn embedded_scroll_area_render_command_preserves_tree_order() {
    let mut parent = make_container();
    let scroll_area = make_scroll_area_handle(&parent, "scroll area");
    let before = widget_handle(TextBlock::new("before"));
    let inside = widget_handle(TextBlock::new("inside"));
    let after = widget_handle(TextBlock::new("after"));
    let mut results = FrameResults::default();

    begin_test_frame(&mut parent, rect(0, 0, 120, 60));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.widget(before.clone());
        tree.scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
            tree.widget(inside.clone());
        });
        tree.widget(after.clone());
    });
    parent.widget_tree(&mut results, &tree);

    let before_idx = parent
        .draw
        .commands
        .iter()
        .position(|cmd| matches!(cmd, Command::Text { text, .. } if text == "before"))
        .expect("before text missing");
    let scroll_area_idx = parent
        .draw
        .commands
        .iter()
        .position(|cmd| matches!(cmd, Command::RetainedScrollArea { .. }))
        .expect("scroll area command missing");
    let after_idx = parent
        .draw
        .commands
        .iter()
        .position(|cmd| matches!(cmd, Command::Text { text, .. } if text == "after"))
        .expect("after text missing");

    assert!(before_idx < scroll_area_idx);
    assert!(scroll_area_idx < after_idx);
}

#[test]
fn retained_scroll_area_scope_is_stable_and_parent_scoped() {
    let mut first_parent = make_container();
    let mut second_parent = make_container();
    first_parent.set_internal_id_seed(Id::new(11));
    second_parent.set_internal_id_seed(Id::new(22));

    let scroll_area_node = NodeId::new(100);
    let child_node = NodeId::new(200);
    let first_scope = first_parent.scroll_area_scope_id(scroll_area_node);
    let second_scope = second_parent.scroll_area_scope_id(scroll_area_node);

    assert_eq!(first_scope, first_parent.scroll_area_scope_id(scroll_area_node));
    assert_ne!(first_scope, second_scope);
    assert_ne!(
        RetainedId::scoped_node(first_scope, child_node),
        RetainedId::scoped_node(second_scope, child_node)
    );
}

#[test]
fn retained_scrollbar_control_ids_are_stable_and_container_scoped() {
    let mut first_parent = make_container();
    let mut second_parent = make_container();
    first_parent.set_internal_id_seed(Id::new(11));
    second_parent.set_internal_id_seed(Id::new(22));

    let first_ids = first_parent.scrollbar_node_ids_for_test();
    let second_ids = second_parent.scrollbar_node_ids_for_test();

    assert_eq!(first_ids, first_parent.scrollbar_node_ids_for_test());
    assert_ne!(first_ids.0, first_ids.1);
    assert_ne!(first_ids, second_ids);
}

#[test]
fn partial_draw_clip_commands_are_balanced_push_pop() {
    let mut container = make_container();
    begin_test_frame(&mut container, rect(0, 0, 80, 30));

    container.push_clip_rect(rect(0, 0, 8, 30));
    container.draw_text(FontId::default(), "aaaa", vec2(0, 0), color(255, 255, 255, 255));
    container.pop_clip_rect();

    let push_idx = container
        .draw
        .commands
        .iter()
        .position(|cmd| matches!(cmd, Command::PushClip { .. }))
        .expect("push clip missing");
    let text_idx = container
        .draw
        .commands
        .iter()
        .position(|cmd| matches!(cmd, Command::Text { text, .. } if text == "aaaa"))
        .expect("text command missing");
    let pop_idx = container
        .draw
        .commands
        .iter()
        .position(|cmd| matches!(cmd, Command::PopClip))
        .expect("pop clip missing");

    assert!(push_idx < text_idx);
    assert!(text_idx < pop_idx);
}

#[test]
fn set_clip_updates_live_clip_state() {
    let mut container = make_container();
    begin_test_frame(&mut container, rect(0, 0, 80, 30));

    container.set_clip(rect(0, 0, 10, 10));

    assert!(container.check_clip(rect(12, 0, 4, 4)) == Clip::All);
    container.draw_rect(rect(12, 0, 4, 4), color(255, 255, 255, 255));
    assert!(
        !container
            .draw
            .commands
            .iter()
            .any(|cmd| matches!(cmd, Command::Recti { rect, .. } if rect.x == 12))
    );

    container.draw_rect(rect(2, 2, 4, 4), color(255, 255, 255, 255));
    assert!(
        container
            .draw
            .commands
            .iter()
            .any(|cmd| matches!(cmd, Command::Recti { rect, .. } if rect.x == 2 && rect.y == 2))
    );
}

#[test]
fn retained_focus_follows_stable_node_id_when_widget_handle_changes() {
    let mut container = make_container();
    let mut results = FrameResults::default();
    let second_focus = Rc::new(Cell::new(false));
    let mut focused_node_id = NodeId::new(0);

    begin_test_frame(&mut container, rect(0, 0, 80, 30));
    let first = widget_handle(FocusProbe::new(Rc::new(Cell::new(false))));
    let tree = WidgetTreeBuilder::build(|tree| {
        focused_node_id = tree.node(NodeOptions::keyed("stable-probe")).widget(first.clone());
    });
    container.widget_tree(&mut results, &tree);
    container.set_focus_node(focused_node_id);
    container.finish();

    results.begin_frame();
    begin_test_frame(&mut container, rect(0, 0, 80, 30));
    let second = widget_handle(FocusProbe::new(second_focus.clone()));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.node(NodeOptions::keyed("stable-probe")).widget(second.clone());
    });
    container.widget_tree(&mut results, &tree);

    assert!(second_focus.get());
}

#[test]
fn retained_text_inside_scroll_area_grows_content_height() {
    let mut parent = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    parent.style = Rc::new(style);

    let scroll_area = make_scroll_area_handle(&parent, "scroll area");
    let text = widget_handle(TextBlock::new("a\na\na\na"));
    let mut results = FrameResults::default();

    begin_test_frame(&mut parent, rect(0, 0, 60, 20));
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(60)], SizePolicy::Fixed(20), |tree| {
            tree.scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.widget(text.clone());
            });
        });
    });
    parent.widget_tree(&mut results, &tree);

    let scroll_area = scroll_area.inner();
    assert!(scroll_area.content_size().height > scroll_area.body().height);
}

#[test]
fn retained_scroll_area_reserves_horizontal_gutter_when_vertical_scrollbar_forces_overflow() {
    let mut parent = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    parent.style = Rc::new(style);

    let scroll_area = make_scroll_area_handle(&parent, "scroll area");
    let child = widget_handle(Button::new("child"));
    let mut results = FrameResults::default();
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.node(NodeOptions::with_policy(Policy::fixed(100, 100)))
            .scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.node(NodeOptions::with_policy(Policy::fixed(95, 200))).widget(child.clone());
            });
    });

    results.begin_frame();
    begin_test_frame(&mut parent, rect(0, 0, 120, 120));
    parent.widget_tree(&mut results, &tree);
    parent.finish();
    results.finish_frame();

    results.begin_frame();
    begin_test_frame(&mut parent, rect(0, 0, 120, 120));
    parent.widget_tree(&mut results, &tree);

    let scroll_area = scroll_area.inner();
    let (_, horizontal_scrollbar) = scroll_area.scrollbar_node_ids_for_test();
    let scrollbar = scroll_area
        .previous_node_layout(horizontal_scrollbar)
        .expect("horizontal scrollbar layout missing");
    let rect = scroll_area.rect();
    let body = scroll_area.body();

    assert_eq!(body.height, rect.height - style.scrollbar_size);
    assert_eq!(scrollbar.rect.y, body.y + body.height);
    assert_eq!(scrollbar.rect.y + scrollbar.rect.height, rect.y + rect.height);
}

#[test]
fn retained_scroll_area_resize_does_not_reserve_transient_horizontal_gutter_for_fitting_content() {
    let mut parent = make_container();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 10;
    parent.style = Rc::new(style);

    let scroll_area = make_scroll_area_handle(&parent, "scroll area");
    let child = widget_handle(Button::new("child"));
    let tree_100 = WidgetTreeBuilder::build(|tree| {
        tree.node(NodeOptions::with_policy(Policy::fixed(100, 100)))
            .scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(200), StackDirection::TopToBottom, |tree| {
                    tree.widget(child.clone());
                });
            });
    });
    let tree_99 = WidgetTreeBuilder::build(|tree| {
        tree.node(NodeOptions::with_policy(Policy::fixed(99, 100)))
            .scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(200), StackDirection::TopToBottom, |tree| {
                    tree.widget(child.clone());
                });
            });
    });
    let mut results = FrameResults::default();

    for _ in 0..3 {
        results.begin_frame();
        begin_test_frame(&mut parent, rect(0, 0, 120, 120));
        parent.widget_tree(&mut results, &tree_100);
        parent.finish();
        results.finish_frame();
    }

    results.begin_frame();
    begin_test_frame(&mut parent, rect(0, 0, 120, 120));
    parent.widget_tree(&mut results, &tree_99);
    parent.finish();
    results.finish_frame();

    let scroll_area = scroll_area.inner();
    let (_, horizontal_scrollbar) = scroll_area.scrollbar_node_ids_for_test();
    let rect = scroll_area.rect();
    let body = scroll_area.body();

    assert!(scroll_area.previous_node_layout(horizontal_scrollbar).is_none());
    assert_eq!(body.width, rect.width - style.scrollbar_size);
    assert_eq!(body.height, rect.height);
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
    assert!(header.read(Node::is_closed));
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
    assert!(header.read(Node::is_expanded));
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
    assert!(header.read(Node::is_expanded));
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
    assert!(header.read(Node::is_closed));
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
    assert!(header.read(Node::is_closed));
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
    assert!(header.read(Node::is_expanded));
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
    assert!(header.read(Node::is_expanded));
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
    assert!(header.read(Node::is_expanded));
    assert!(container.current_node_layout(child_node_id).is_some());
}

#[test]
fn scroll_area_hover_root_switches_between_siblings_on_next_frame() {
    let mut parent = make_container();
    let input = parent.input.clone();
    let left = make_scroll_area_handle(&parent, "left");
    let right = make_scroll_area_handle(&parent, "right");
    let mut results = FrameResults::default();
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
            tree.scroll_area(left.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |_| {});
            tree.scroll_area(right.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |_| {});
        });
    });

    input.borrow_mut().mousemove(75, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(!left.inner().interaction.in_hover_root);
    assert!(!right.inner().interaction.in_hover_root);
    parent.finish();

    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(!left.inner().interaction.in_hover_root);
    assert!(right.inner().interaction.in_hover_root);
    parent.finish();

    input.borrow_mut().mousemove(25, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(!left.inner().interaction.in_hover_root);
    assert!(right.inner().interaction.in_hover_root);
    parent.finish();

    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    assert!(left.inner().interaction.in_hover_root);
    assert!(!right.inner().interaction.in_hover_root);
}

#[test]
fn nested_scroll_areas_preserve_focus_hover_scroll_and_content_size() {
    let mut parent = make_container();
    let input = parent.input.clone();
    let mut style = Style::default();
    style.padding = 0;
    style.scrollbar_size = 8;
    parent.style = Rc::new(style);

    let outer = make_scroll_area_handle(&parent, "outer");
    let inner = make_scroll_area_handle(&parent, "inner");
    let focused = Rc::new(Cell::new(false));
    let probe = widget_handle(FocusProbe::new(focused.clone()));
    let text = widget_handle(TextBlock::new("a\na\na\na\na\na\na\na"));
    let mut outer_node_id = NodeId::new(0);
    let mut inner_node_id = NodeId::new(0);
    let mut probe_node_id = NodeId::new(0);
    let mut results = FrameResults::default();

    let tree = WidgetTreeBuilder::build(|tree| {
        outer_node_id =
            tree.node(NodeOptions::with_policy(Policy::fixed(80, 40)))
                .scroll_area(outer.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                    inner_node_id = tree.node(NodeOptions::with_policy(Policy::fixed(60, 18))).scroll_area(
                        inner.clone(),
                        ContainerOption::NONE,
                        ScrollBehavior::NONE,
                        |tree| {
                            probe_node_id = tree.widget(probe.clone());
                            tree.widget(text.clone());
                        },
                    );
                });
    });

    input.borrow_mut().mousemove(5, 5);
    for _ in 0..3 {
        results.begin_frame();
        begin_test_frame(&mut parent, rect(0, 0, 100, 60));
        parent.widget_tree(&mut results, &tree);
        parent.finish();
        results.finish_frame();
    }

    assert_eq!(parent.interaction.hover_root_child, Some(parent.retained_id_for_node(outer_node_id)));
    assert!(outer.inner().interaction.in_hover_root);
    let expected_inner_hover = outer.inner().retained_id_for_node(inner_node_id);
    assert_eq!(outer.inner().interaction.hover_root_child, Some(expected_inner_hover));
    assert!(inner.inner().interaction.in_hover_root);

    let inner_focus = inner.clone();
    inner_focus.with_mut(|scroll_area| scroll_area.set_focus_node(probe_node_id));
    focused.set(false);
    results.begin_frame();
    begin_test_frame(&mut parent, rect(0, 0, 100, 60));
    parent.widget_tree(&mut results, &tree);
    assert!(focused.get());
    parent.finish();
    results.finish_frame();

    let content_before_scroll = inner.inner().content_size();
    let body_before_scroll = inner.inner().body();
    let scroll_before = inner.inner().scroll();
    assert!(content_before_scroll.height > body_before_scroll.height);

    results.begin_frame();
    begin_test_frame(&mut parent, rect(0, 0, 100, 60));
    parent.seed_pending_scroll(Some(vec2(0, 24)));
    parent.widget_tree(&mut results, &tree);
    parent.finish();
    results.finish_frame();

    let scroll_after = inner.inner().scroll();
    assert!(scroll_after.y > scroll_before.y);
    assert_eq!(inner.inner().content_size().height, content_before_scroll.height);

    focused.set(false);
    results.begin_frame();
    begin_test_frame(&mut parent, rect(0, 0, 100, 60));
    parent.widget_tree(&mut results, &tree);
    assert!(focused.get());
    parent.finish();
    results.finish_frame();

    assert_eq!(inner.inner().scroll().y, scroll_after.y);
    assert_eq!(inner.inner().content_size().height, content_before_scroll.height);
}

#[test]
fn parent_widgets_are_only_blocked_while_mouse_is_inside_active_child_rect() {
    let mut parent = make_container();
    let input = parent.input.clone();
    let scroll_area = make_scroll_area_handle(&parent, "scroll area");
    let mut results = FrameResults::default();
    let tree = WidgetTreeBuilder::build(|tree| {
        tree.row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
            tree.scroll_area(scroll_area.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |_| {});
        });
    });

    input.borrow_mut().mousemove(10, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    parent.widget_tree(&mut results, &tree);
    parent.finish();

    let blocked_button = Button::new("blocked");
    input.borrow_mut().mousemove(10, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    let blocked = update_control_for_widget(&mut parent, NodeId::new(0x1006), rect(0, 0, 40, 20), &blocked_button);
    assert!(!blocked.hovered);
    parent.widget_tree(&mut results, &tree);
    parent.finish();

    let free_button = Button::new("free");
    input.borrow_mut().mousemove(75, 10);
    begin_test_frame(&mut parent, rect(0, 0, 100, 20));
    let free = update_control_for_widget(&mut parent, NodeId::new(0x1007), rect(60, 0, 30, 20), &free_button);
    assert!(free.hovered);
}

#[test]
fn scoped_row_restores_template_but_preserves_cursor_advancement() {
    let mut container = make_container();
    begin_test_frame(&mut container, rect(0, 0, 120, 80));
    container.layout.row(&[SizePolicy::Fixed(20), SizePolicy::Fixed(30)], SizePolicy::Fixed(10));

    let first = container.layout.next();
    let mut scoped = Vec::new();
    container.with_row(&[SizePolicy::Fixed(15), SizePolicy::Fixed(15)], SizePolicy::Fixed(10), |ui| {
        scoped.push(ui.layout.next());
        scoped.push(ui.layout.next());
    });
    let after_scope = container.layout.next();
    let next_after_scope = container.layout.next();

    assert_eq!(first.width, 20);
    assert_eq!(scoped[0].width, 15);
    assert_eq!(scoped[1].width, 15);
    assert_eq!(after_scope.width, 20);
    assert_eq!(next_after_scope.width, 30);
    assert_eq!(after_scope.y, scoped[0].y);
    assert!(after_scope.x >= scoped[1].x + scoped[1].width + container.style().spacing);
}

#[test]
fn scoped_stack_restores_previous_row_template_after_advancing_layout() {
    let mut container = make_container();
    begin_test_frame(&mut container, rect(0, 0, 120, 80));
    container.layout.row(&[SizePolicy::Fixed(22), SizePolicy::Fixed(33)], SizePolicy::Fixed(10));

    let mut stacked = Vec::new();
    container.stack_with_width_direction(SizePolicy::Fixed(44), SizePolicy::Fixed(8), StackDirection::TopToBottom, |ui| {
        stacked.push(ui.layout.next());
        stacked.push(ui.layout.next());
    });
    let after_scope = container.layout.next();
    let next_after_scope = container.layout.next();

    assert_eq!(stacked[0].width, 44);
    assert_eq!(stacked[1].width, 44);
    assert_eq!(after_scope.width, 22);
    assert_eq!(next_after_scope.width, 33);
    assert!(after_scope.y >= stacked[1].y + stacked[1].height + container.style().spacing);
}

#[test]
fn retained_image_button_emits_separate_image_and_text_commands() {
    let mut container = make_container();
    let texture = TextureId::new(42, 13, 5);
    let button = widget_handle(Button::with_image(
        "aa",
        Some(Image::Texture(texture)),
        WidgetOption::NONE,
        WidgetFillOption::ALL,
    ));
    let mut results = FrameResults::default();
    let mut button_node = NodeId::new(0);

    begin_test_frame(&mut container, rect(0, 0, 120, 40));
    let tree = WidgetTreeBuilder::build(|tree| {
        button_node = tree.widget(button.clone());
    });
    container.widget_tree(&mut results, &tree);

    let layout = container.current_node_layout(button_node).expect("button layout missing");
    let padding = container.style().padding;
    assert_eq!(layout.rect.width, padding * 2 + texture.width() + padding + 16);
    assert_eq!(layout.rect.height, 14);

    let image_rect = container
        .draw
        .commands
        .iter()
        .find_map(|cmd| match cmd {
            Command::Image { rect, image: Image::Texture(id), .. } if *id == texture => Some(*rect),
            _ => None,
        })
        .expect("image command missing");
    let text_pos = container
        .draw
        .commands
        .iter()
        .find_map(|cmd| match cmd {
            Command::Text { text, pos, .. } if text == "aa" => Some(*pos),
            _ => None,
        })
        .expect("text command missing");

    assert!(image_rect.x + image_rect.width <= text_pos.x);
}

#[test]
fn retained_custom_render_callback_receives_content_clipped_view() {
    let mut container = make_container();
    let custom = widget_handle(Custom::new("custom"));
    let observed = Rc::new(RefCell::new(None));
    let observed_for_render = observed.clone();
    let mut results = FrameResults::default();

    begin_test_frame(&mut container, rect(0, 0, 120, 40));
    container.push_clip_rect(rect(8, 8, 16, 8));
    let tree = WidgetTreeBuilder::build(move |tree| {
        tree.node(NodeOptions::with_policy(Policy::fixed(40, 20)))
            .custom_render(custom.clone(), move |_dim, args| {
                *observed_for_render.borrow_mut() = Some((args.content_area, args.view));
            });
    });
    container.widget_tree(&mut results, &tree);
    container.pop_clip_rect();

    let mut canvas = Canvas::from(RendererHandle::new(NoopRenderer { atlas: container.atlas.clone() }), Dimensioni::new(120, 40));
    container.render(&mut canvas);

    let (content_area, view) = observed.borrow().as_ref().copied().expect("custom render callback was not invoked");
    let expected_view = content_area.intersect(&rect(8, 8, 16, 8)).unwrap();
    assert_eq!(content_area.width, 40);
    assert_eq!(content_area.height, 20);
    assert_eq!(
        (view.x, view.y, view.width, view.height),
        (expected_view.x, expected_view.y, expected_view.width, expected_view.height)
    );
}
