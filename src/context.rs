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
use std::{cell::RefCell, rc::Rc};

#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;

#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{ColorType, Decoder};

use crate::{
    rect, Canvas, Color, Container, ContainerHandle, ContainerOption, Dimensioni, FrameResultGeneration, ImageSource, Input, Recti, Renderer, KeyCode, KeyMode,
    MouseButton, RendererHandle, Style, TextureId, UNCLIPPED_RECT, WidgetBehaviourOption, WidgetTree, WindowHandle, WindowState, FrameResults,
};

#[cfg(test)]
use crate::Vec2i;

/// Primary entry point used to drive the UI over a renderer implementation.
pub struct Context<R: Renderer> {
    canvas: Canvas<R>,
    style: Rc<Style>,

    last_zindex: i32,
    frame: usize,
    hover_root: Option<WindowHandle>,
    next_hover_root: Option<WindowHandle>,
    scroll_target: Option<WindowHandle>,

    root_list: Vec<WindowHandle>,
    frame_results: FrameResults,

    /// Shared pointer to the input state driving this context.
    pub input: Rc<RefCell<Input>>,
}

impl<R: Renderer> Context<R> {
    /// Creates a new UI context around the provided renderer and dimensions.
    pub fn new(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        Self {
            canvas: Canvas::from(renderer, dim),
            style: Rc::new(Style::default()),
            last_zindex: 0,
            frame: 0,
            hover_root: None,
            next_hover_root: None,
            scroll_target: None,

            root_list: Vec::default(),
            frame_results: FrameResults::default(),

            input: Rc::new(RefCell::new(Input::default())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        panic::{catch_unwind, AssertUnwindSafe},
    };

    use super::*;
    use crate::{
        container::Command, widget::widget_id_of_handle, widget_handle, AtlasHandle, AtlasSource, CharEntry, ControlState, FontEntry, ResourceState, SourceFormat,
        TextBlock, Widget, WidgetCtx, WidgetOption, WidgetTreeBuilder,
    };

    const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

    struct NoopRenderer {
        atlas: AtlasHandle,
    }

    impl Renderer for NoopRenderer {
        fn get_atlas(&self) -> AtlasHandle {
            self.atlas.clone()
        }
        fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}
        fn push_quad_vertices(&mut self, _v0: &crate::canvas::Vertex, _v1: &crate::canvas::Vertex, _v2: &crate::canvas::Vertex, _v3: &crate::canvas::Vertex) {}
        fn flush(&mut self) {}
        fn end(&mut self) {}
        fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) {}
        fn destroy_texture(&mut self, _id: TextureId) {}
        fn draw_texture(&mut self, _id: TextureId, _vertices: [crate::canvas::Vertex; 4]) {}
    }

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

    fn panic_message(payload: Box<dyn Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&str>() {
            return (*message).to_string();
        }
        "<non-string panic payload>".to_string()
    }

    struct AlwaysSubmitWidget {
        label: &'static str,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    }

    impl AlwaysSubmitWidget {
        fn new(label: &'static str) -> Self {
            Self {
                label,
                opt: WidgetOption::NONE,
                bopt: WidgetBehaviourOption::NONE,
            }
        }
    }

    impl Widget for AlwaysSubmitWidget {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn behaviour_opt(&self) -> &WidgetBehaviourOption {
            &self.bopt
        }

        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            Dimensioni::new((self.label.len() as i32 * 8).max(8), 12)
        }

        fn run(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
            ResourceState::SUBMIT
        }
    }

    #[test]
    fn root_windows_render_scrollbars_after_content_size_is_known() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        ctx.set_style(&style);

        let mut window = ctx.new_window("window", rect(0, 0, 60, 30));
        let text = widget_handle(TextBlock::new("a\na\na\na\na\na"));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.widget(text.clone());
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        let inner = window.inner();
        let body = inner.main.body;
        let has_vertical_scrollbar =
            inner.main.command_list.iter().any(
                |cmd| matches!(cmd, Command::Recti { rect, .. } if rect.x == body.x + body.width && rect.width == style.scrollbar_size && rect.height > 0),
            );

        assert!(has_vertical_scrollbar);
    }

    #[test]
    fn resize_handle_wins_bottom_right_corner_over_window_scrollbars() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 240));
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        ctx.set_style(&style);

        let mut window = ctx.new_window("window", rect(0, 0, 60, 40));
        let text = widget_handle(TextBlock::new("aaaaaaaaaaaaaaaaaaaaaaaa\na\na\na\na\na\na\na"));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.widget(text.clone());
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        let initial_rect = window.rect();
        let corner_x = initial_rect.x + initial_rect.width - 1;
        let corner_y = initial_rect.y + initial_rect.height - 1;

        ctx.mousemove(corner_x, corner_y);
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        ctx.mousemove(corner_x + 12, corner_y + 10);
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        let resized = window.rect();
        assert!(resized.width > initial_rect.width);
        assert!(resized.height > initial_rect.height);
    }

    #[test]
    fn context_result_accessors_expose_committed_and_current_generations() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let committed_widget = 1_u8;
        let current_widget = 2_u8;
        let committed_id = (&committed_widget as *const u8).cast::<()>();
        let current_id = (&current_widget as *const u8).cast::<()>();

        ctx.frame_results.record(committed_id, ResourceState::SUBMIT);
        ctx.frame_results.finish_frame();
        ctx.frame_results.begin_frame();
        ctx.frame_results.record(current_id, ResourceState::CHANGE);

        assert!(ctx.committed_results().state(committed_id).is_submitted());
        assert!(ctx.committed_results().state(current_id).is_none());
        assert!(ctx.current_results().state(committed_id).is_none());
        assert!(ctx.current_results().state(current_id).is_changed());
    }

    #[test]
    fn closing_window_resets_transient_render_state() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut window = ctx.new_window("window", rect(0, 0, 80, 40));

        {
            let mut inner = window.inner_mut();
            inner.main.command_list.push(Command::None);
            inner.main.clip_stack.push(UNCLIPPED_RECT);
            inner.main.content_size = Dimensioni::new(11, 17);
            inner.main.scroll = Vec2i::new(3, 5);
        }

        window.close();

        let inner = window.inner();
        assert!(inner.main.command_list.is_empty());
        assert!(inner.main.clip_stack.is_empty());
        assert_eq!(inner.main.content_size.width, 0);
        assert_eq!(inner.main.content_size.height, 0);
        assert_eq!(inner.main.scroll.x, 0);
        assert_eq!(inner.main.scroll.y, 0);
    }

    #[test]
    fn reshown_windows_prepare_on_first_render_after_a_gap() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut window = ctx.new_window("window", rect(20, 20, 80, 40));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.text("hello");
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });
        ctx.frame(|_ui| {});

        {
            let mut inner = window.inner_mut();
            inner.main.command_list.push(Command::None);
        }

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        let inner = window.inner();
        assert!(!inner.main.command_list.iter().any(|cmd| matches!(cmd, Command::None)));
    }

    #[test]
    #[should_panic(expected = "rendered more than once in frame")]
    fn rendering_same_window_twice_in_one_frame_panics() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut window = ctx.new_window("window", rect(0, 0, 80, 40));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.text("hello");
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });
    }

    #[test]
    fn reopening_dialog_replaces_old_commands_with_current_frame_commands() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut dialog = ctx.new_dialog("dialog", rect(10, 10, 80, 40));
        let first_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("before");
        });
        let second_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("after");
        });
        let opt = ContainerOption::NO_TITLE | ContainerOption::NO_CLOSE | ContainerOption::NO_RESIZE;

        dialog.open();
        ctx.frame(|ui| {
            ui.dialog(&mut dialog, opt, WidgetBehaviourOption::NONE, &first_tree);
        });

        dialog.close();

        dialog.open();
        ctx.frame(|ui| {
            ui.dialog(&mut dialog, opt, WidgetBehaviourOption::NONE, &second_tree);
        });

        let inner = dialog.inner();
        let texts: Vec<String> = inner
            .main
            .command_list
            .iter()
            .filter_map(|cmd| match cmd {
                Command::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();

        assert!(texts.iter().any(|text| text == "after"));
        assert!(!texts.iter().any(|text| text == "before"));
    }

    #[test]
    fn reshown_roots_drop_stale_panel_handles_after_a_gap() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut window = ctx.new_window("window", rect(0, 0, 100, 80));
        let panel = ctx.new_panel("panel");
        let tree_with_panel = WidgetTreeBuilder::build({
            let panel = panel.clone();
            move |tree| {
                tree.container(panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                    tree.text("panel child");
                });
            }
        });
        let tree_without_panel = WidgetTreeBuilder::build(|tree| {
            tree.text("root only");
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree_with_panel);
        });
        assert_eq!(window.inner().main.panel_count(), 1);

        ctx.frame(|_ui| {});

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree_without_panel);
        });

        assert_eq!(window.inner().main.panel_count(), 0);
    }

    #[test]
    fn newly_opened_popup_auto_sizes_on_first_frame() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut popup = ctx.new_popup("popup");
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.text("hello popup");
        });

        ctx.mousemove(40, 50);
        ctx.open_popup(&mut popup);
        ctx.frame(|ui| {
            ui.popup(&mut popup, WidgetBehaviourOption::NONE, &tree);
        });

        let inner = popup.inner();
        assert!(inner.main.rect.width > 1);
        assert!(inner.main.rect.height > 1);
        assert!(inner.main.body.width > 0);
        assert!(inner.main.body.height > 0);
    }

    #[test]
    fn auto_sized_titled_window_uses_current_frame_content_size() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut window = ctx.new_window("window", rect(0, 0, 1, 1));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.text("hello\nhello\nhello");
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::AUTO_SIZE, WidgetBehaviourOption::NONE, &tree);
        });

        let inner = window.inner();
        assert!(inner.main.rect.width > 1);
        assert!(inner.main.rect.height > 1);
        assert!(inner.main.body.y > inner.main.rect.y);
        assert!(inner.main.body.height > 0);
    }

    #[test]
    fn popup_content_changes_resize_without_a_frame_lag() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut popup = ctx.new_popup("popup");
        let short_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("a");
        });
        let long_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("hello popup with much longer content");
        });

        ctx.mousemove(60, 60);
        ctx.open_popup(&mut popup);
        ctx.frame(|ui| {
            ui.popup(&mut popup, WidgetBehaviourOption::NONE, &short_tree);
        });
        let first_width = popup.rect().width;

        ctx.frame(|ui| {
            ui.popup(&mut popup, WidgetBehaviourOption::NONE, &long_tree);
        });

        assert!(popup.rect().width > first_width);
    }

    #[test]
    fn auto_sized_titled_window_body_fits_current_content_same_frame() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        ctx.set_style(&style);
        let mut window = ctx.new_window("window", rect(0, 0, 1, 1));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.text("hello\nhello\nhello\nhello");
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::AUTO_SIZE, WidgetBehaviourOption::NONE, &tree);
        });

        let inner = window.inner();
        assert!(inner.main.body.width >= inner.main.content_size.width);
        assert!(inner.main.body.height >= inner.main.content_size.height);
        assert!(inner.main.body.height > 0);
        assert!(inner.main.body.y > inner.main.rect.y);

        let body = inner.main.body;
        let has_vertical_scrollbar =
            inner.main.command_list.iter().any(
                |cmd| matches!(cmd, Command::Recti { rect, .. } if rect.x == body.x + body.width && rect.width == style.scrollbar_size && rect.height > 0),
            );

        assert!(!has_vertical_scrollbar);
    }

    #[test]
    fn title_option_controls_root_window_title_bar_geometry() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let mut titled = ctx.new_window("titled", rect(0, 0, 80, 40));
        let mut plain = ctx.new_window("plain", rect(100, 0, 80, 40));
        let tree = WidgetTreeBuilder::build(|_tree| {});

        ctx.frame(|ui| {
            ui.window(&mut titled, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
            ui.window(&mut plain, ContainerOption::NO_TITLE, WidgetBehaviourOption::NONE, &tree);
        });

        let titled_inner = titled.inner();
        assert!(titled_inner.main.body.y > titled_inner.main.rect.y);
        assert!(titled_inner.main.body.height < titled_inner.main.rect.height);
        let titled_texts: Vec<String> = titled_inner
            .main
            .command_list
            .iter()
            .filter_map(|cmd| match cmd {
                Command::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(titled_texts.iter().any(|text| text == "titled"));

        let plain_inner = plain.inner();
        assert_eq!(plain_inner.main.body.y, plain_inner.main.rect.y);
        assert_eq!(plain_inner.main.body.height, plain_inner.main.rect.height);
        assert_eq!(plain_inner.main.body.x, plain_inner.main.rect.x);
        assert_eq!(plain_inner.main.body.width, plain_inner.main.rect.width);
        let plain_texts: Vec<String> = plain_inner
            .main
            .command_list
            .iter()
            .filter_map(|cmd| match cmd {
                Command::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(!plain_texts.iter().any(|text| text == "plain"));
    }

    #[test]
    fn duplicate_widget_dispatch_in_same_tree_panics_with_context() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut window = ctx.new_window("primary", rect(0, 0, 80, 40));
        let shared = widget_handle(AlwaysSubmitWidget::new("shared"));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.widget(shared.clone());
            tree.widget(shared.clone());
        });

        let panic = catch_unwind(AssertUnwindSafe(|| {
            ctx.frame(|ui| {
                ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
            });
        }))
        .expect_err("duplicate widget handle should panic");
        let message = panic_message(panic);

        assert!(message.contains("duplicate widget dispatch"));
        assert!(message.contains("WidgetHandle"));
        assert!(message.contains("primary"));
        assert!(message.contains("tree node"));
    }

    #[test]
    fn duplicate_widget_dispatch_across_windows_panics() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut left = ctx.new_window("left", rect(0, 0, 80, 40));
        let mut right = ctx.new_window("right", rect(90, 0, 80, 40));
        let shared = widget_handle(AlwaysSubmitWidget::new("shared"));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.widget(shared.clone());
        });

        let panic = catch_unwind(AssertUnwindSafe(|| {
            ctx.frame(|ui| {
                ui.window(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
                ui.window(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
            });
        }))
        .expect_err("rendering one widget handle in two windows should panic");
        let message = panic_message(panic);

        assert!(message.contains("duplicate widget dispatch"));
        assert!(message.contains("left"));
        assert!(message.contains("right"));
    }

    #[test]
    fn distinct_widget_handles_with_identical_labels_render_normally() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut window = ctx.new_window("window", rect(0, 0, 80, 40));
        let first = widget_handle(AlwaysSubmitWidget::new("same"));
        let second = widget_handle(AlwaysSubmitWidget::new("same"));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.widget(first.clone());
            tree.widget(second.clone());
        });

        assert_ne!(widget_id_of_handle(&first), widget_id_of_handle(&second));

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, WidgetBehaviourOption::NONE, &tree);
        });

        assert!(ctx.committed_results().state_of_handle(&first).is_submitted());
        assert!(ctx.committed_results().state_of_handle(&second).is_submitted());
    }
}

impl<R: Renderer> Context<R> {
    /// Begins a new draw pass on the underlying canvas.
    pub fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.canvas.begin(width, height, clr);
    }

    /// Flushes recorded draw commands to the renderer and ends the draw pass.
    pub fn end(&mut self) {
        for r in &mut self.root_list {
            r.render(&mut self.canvas);
        }
        self.canvas.end()
    }

    /// Returns a handle to the underlying renderer.
    pub fn renderer_handle(&self) -> RendererHandle<R> {
        self.canvas.renderer_handle()
    }

    /// Updates the current mouse pointer position.
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.input.borrow_mut().mousemove(x, y);
    }

    /// Records that the specified mouse button was pressed.
    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input.borrow_mut().mousedown(x, y, btn);
    }

    /// Records that the specified mouse button was released.
    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input.borrow_mut().mouseup(x, y, btn);
    }

    /// Accumulates scroll wheel movement.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.input.borrow_mut().scroll(x, y);
    }

    /// Records that a modifier key was pressed.
    pub fn keydown(&mut self, key: KeyMode) {
        self.input.borrow_mut().keydown(key);
    }

    /// Records that a modifier key was released.
    pub fn keyup(&mut self, key: KeyMode) {
        self.input.borrow_mut().keyup(key);
    }

    /// Records that a navigation key was pressed.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.input.borrow_mut().keydown_code(code);
    }

    /// Records that a navigation key was released.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.input.borrow_mut().keyup_code(code);
    }

    /// Appends UTF-8 text to the input buffer.
    pub fn text(&mut self, text: &str) {
        self.input.borrow_mut().text(text);
    }

    #[inline(never)]
    fn frame_begin(&mut self) {
        self.scroll_target = None;
        self.frame_results.begin_frame();
        self.input.borrow_mut().prelude();
        self.frame += 1;
        self.root_list.clear();
    }

    #[inline(never)]
    fn frame_end(&mut self) {
        for r in &mut self.root_list {
            r.finish();
        }
        self.frame_results.finish_frame();

        let mouse_pressed = self.input.borrow().mouse_pressed;
        match (mouse_pressed.is_none(), &self.next_hover_root) {
            (false, Some(next_hover_root)) if next_hover_root.zindex() < self.last_zindex && next_hover_root.zindex() >= 0 => {
                self.bring_to_front(&mut next_hover_root.clone());
            }
            _ => (),
        }

        self.input.borrow_mut().epilogue();

        // prepare the next frame
        self.hover_root = self.next_hover_root.clone();
        self.next_hover_root = None;
        for r in &mut self.root_list {
            r.inner_mut().main.in_hover_root = false;
        }
        match &mut self.hover_root {
            Some(window) => window.inner_mut().main.in_hover_root = true,
            _ => (),
        }

        // sort all windows
        self.root_list.sort_by(|a, b| a.zindex().cmp(&b.zindex()));
    }

    /// Runs the UI for a single frame by wrapping input/layout bookkeeping.
    /// Rendering still requires calling [`Context::begin`] and [`Context::end`].
    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.frame_begin();

        // execute the frame function
        f(self);

        self.frame_end();
    }

    /// Creates a new movable window rooted at the provided rectangle.
    pub fn new_window(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let mut window = WindowHandle::window(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect);
        self.bring_to_front(&mut window);
        window
    }

    /// Creates a modal dialog window.
    pub fn new_dialog(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        WindowHandle::dialog(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect)
    }

    /// Creates a popup window that appears under the mouse cursor.
    pub fn new_popup(&mut self, name: &str) -> WindowHandle {
        WindowHandle::popup(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone())
    }

    /// Creates a standalone panel that can be embedded inside other windows.
    pub fn new_panel(&mut self, name: &str) -> ContainerHandle {
        ContainerHandle::new(Container::new(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone()))
    }

    /// Bumps the window's Z order so it renders above others.
    pub fn bring_to_front(&mut self, window: &mut WindowHandle) {
        self.last_zindex += 1;
        window.inner_mut().main.zindex = self.last_zindex;
    }

    #[inline(never)]
    fn begin_root_container(&mut self, window: &mut WindowHandle) {
        window.prepare_for_frame(self.frame);
        self.root_list.push(window.clone());

        if window.inner().main.rect.contains(&self.input.borrow().mouse_pos)
            && (self.next_hover_root.is_none() || window.zindex() > self.next_hover_root.as_ref().unwrap().zindex())
        {
            self.next_hover_root = Some(window.clone());
        }
        let container = &mut window.inner_mut().main;
        let scroll_delta = self.input.borrow().scroll_delta;
        let pending_scroll = if container.in_hover_root && (scroll_delta.x != 0 || scroll_delta.y != 0) {
            Some(scroll_delta)
        } else {
            None
        };
        container.seed_pending_scroll(pending_scroll);
        container.clip_stack.push(UNCLIPPED_RECT);
    }

    #[inline(never)]
    fn end_root_container(&mut self, window: &mut WindowHandle) {
        let container = &mut window.inner_mut().main;
        container.pop_clip_rect();

        let layout_body = container.layout.current_body();
        match container.layout.current_max() {
            None => (),
            Some(lm) => container.content_size = Dimensioni::new(lm.x - layout_body.x, lm.y - layout_body.y),
        }
        container.render_active_scrollbars();
        container.consume_pending_scroll();
        container.layout.pop_scope();
    }

    #[inline(never)]
    #[must_use]
    fn begin_window(&mut self, window: &mut WindowHandle, opt: ContainerOption, bopt: WidgetBehaviourOption) -> bool {
        if !window.is_open() {
            return false;
        }

        self.begin_root_container(window);
        window.begin_window(opt, bopt);

        true
    }

    fn end_window(&mut self, window: &mut WindowHandle, opt: ContainerOption) {
        window.end_window();
        self.end_root_container(window);
        window.finish_resize(opt);
    }

    fn render_window_tree(&mut self, window: &mut WindowHandle, opt: ContainerOption, bopt: WidgetBehaviourOption, tree: &WidgetTree) {
        if window.is_open() {
            window.inner_mut().main.style = self.style.clone();
            if opt.is_auto_sizing() {
                window.measure_auto_size(&self.frame_results, opt, bopt, tree);
            }
        }

        if self.begin_window(window, opt, bopt) {
            {
                let mut inner = window.inner_mut();
                inner.main.widget_tree(&mut self.frame_results, tree);
            }
            self.end_window(window, opt);

            if !window.is_open() {
                window.reset_after_close();
            }
        }
    }

    /// Opens a window and renders the provided retained widget tree into it.
    pub fn window(&mut self, window: &mut WindowHandle, opt: ContainerOption, bopt: WidgetBehaviourOption, tree: &WidgetTree) {
        self.render_window_tree(window, opt, bopt, tree);
    }

    /// Marks a dialog window as open for the next frame.
    pub fn open_dialog(&mut self, window: &mut WindowHandle) {
        window.inner_mut().win_state = WindowState::Open;
    }

    /// Renders a dialog window if it is currently open.
    pub fn dialog(&mut self, window: &mut WindowHandle, opt: ContainerOption, bopt: WidgetBehaviourOption, tree: &WidgetTree) {
        if window.is_open() {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            window.inner_mut().main.in_hover_root = true;
            self.bring_to_front(window);

            self.render_window_tree(window, opt, bopt, tree);
        }
    }

    /// Shows a popup at the mouse cursor position.
    pub fn open_popup(&mut self, window: &mut WindowHandle) {
        let was_open = window.is_open();
        let mouse_pos = self.input.borrow().mouse_pos;
        {
            let mut inner = window.inner_mut();
            if was_open {
                let mut rect = inner.main.rect;
                rect.x = mouse_pos.x;
                rect.y = mouse_pos.y;
                inner.main.rect = rect;
            } else {
                inner.main.rect = rect(mouse_pos.x, mouse_pos.y, 1, 1);
                inner.win_state = WindowState::Open;
                inner.main.in_hover_root = true;
                inner.main.popup_just_opened = true;
            }
        }
        if !was_open {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            self.bring_to_front(window);
        }
    }

    /// Shows a popup anchored at the provided rectangle instead of the mouse cursor.
    pub fn open_popup_at(&mut self, window: &mut WindowHandle, anchor: Recti) {
        let was_open = window.is_open();
        {
            let mut inner = window.inner_mut();
            if was_open {
                let mut rect = inner.main.rect;
                rect.x = anchor.x;
                rect.y = anchor.y;
                rect.width = anchor.width;
                inner.main.rect = rect;
            } else {
                inner.main.rect = anchor;
                inner.win_state = WindowState::Open;
                inner.main.in_hover_root = true;
                inner.main.popup_just_opened = true;
            }
        }
        if !was_open {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            self.bring_to_front(window);
        }
    }

    /// Opens a popup window with default options and renders a retained tree into it.
    pub fn popup(&mut self, window: &mut WindowHandle, bopt: WidgetBehaviourOption, tree: &WidgetTree) {
        let opt = ContainerOption::AUTO_SIZE | ContainerOption::NO_RESIZE | ContainerOption::NO_TITLE;
        self.render_window_tree(window, opt, bopt, tree);
    }

    /// Returns the previous frame's published widget results.
    ///
    /// This is the public business-logic view of retained interaction state.
    /// App code should react to this generation after rendering, accepting the
    /// one-frame delay as part of the retained pipeline contract.
    pub fn committed_results(&self) -> FrameResultGeneration<'_> {
        self.frame_results.committed()
    }

    /// Returns the in-progress result generation being written by the current frame.
    ///
    /// This is mainly useful for framework internals or advanced debugging.
    /// Normal application/business logic should prefer [`Context::committed_results`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current_results(&self) -> FrameResultGeneration<'_> {
        self.frame_results.current()
    }

    /// Replaces the current UI style.
    pub fn set_style(&mut self, style: &Style) {
        self.style = Rc::new(style.clone())
    }

    /// Returns the underlying canvas used for rendering.
    pub fn canvas(&self) -> &Canvas<R> {
        &self.canvas
    }

    /// Uploads an RGBA image to the renderer and returns its [`TextureId`].
    pub fn load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId {
        self.canvas.load_texture_rgba(width, height, pixels)
    }

    /// Deletes a previously uploaded texture.
    pub fn free_image(&mut self, id: TextureId) {
        self.canvas.free_texture(id);
    }

    /// Uploads texture data described by `source`. PNG decoding is only available when the
    /// `png_source` (or `builder`) feature is enabled.
    pub fn load_image_from(&mut self, source: ImageSource) -> Result<TextureId, String> {
        match source {
            ImageSource::Raw { width, height, pixels } => {
                Self::assert_rgba_len(width, height, pixels.len())?;
                Ok(self.load_image_rgba(width, height, pixels))
            }
            #[cfg(any(feature = "builder", feature = "png_source"))]
            ImageSource::Png { bytes } => {
                let (width, height, rgba) = Self::decode_png(bytes)?;
                Ok(self.load_image_rgba(width, height, rgba.as_slice()))
            }
        }
    }

    fn assert_rgba_len(width: i32, height: i32, len: usize) -> Result<(), String> {
        if width <= 0 || height <= 0 {
            return Err(String::from("Image dimensions must be positive"));
        }
        let expected = width as usize * height as usize * 4;
        if len != expected {
            return Err(format!("Expected {} RGBA bytes, received {}", expected, len));
        }
        Ok(())
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    fn decode_png(bytes: &[u8]) -> Result<(i32, i32, Vec<u8>), String> {
        let cursor = Cursor::new(bytes);
        let decoder = Decoder::new(cursor);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let buf_size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG decoder did not report output size".to_string())?;
        let mut buf = vec![0; buf_size];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let raw = &buf[..info.buffer_size()];
        let mut rgba = Vec::with_capacity((info.width as usize) * (info.height as usize) * 4);
        match info.color_type {
            ColorType::Rgba => rgba.extend_from_slice(raw),
            ColorType::Rgb => {
                for chunk in raw.chunks(3) {
                    rgba.extend_from_slice(chunk);
                    rgba.push(0xFF);
                }
            }
            ColorType::Grayscale => {
                for &v in raw {
                    rgba.extend_from_slice(&[v, v, v, 0xFF]);
                }
            }
            ColorType::GrayscaleAlpha => {
                for chunk in raw.chunks(2) {
                    let v = chunk[0];
                    let a = chunk[1];
                    rgba.extend_from_slice(&[v, v, v, a]);
                }
            }
            _ => {
                return Err("Unsupported PNG color type".into());
            }
        }
        Ok((info.width as i32, info.height as i32, rgba))
    }
}
