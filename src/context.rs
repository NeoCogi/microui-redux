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
    rect, Canvas, Color, Container, ContainerHandle, ContainerOption, Dimensioni, FrameResultGeneration, FrameResults, ImageSource, Input, KeyCode, KeyMode,
    MouseButton, Recti, Renderer, RendererHandle, ScrollBehavior, Style, TextureId, WidgetTree, WindowHandle,
};
use crate::window::WindowChromeIds;

#[cfg(test)]
use crate::{UNCLIPPED_RECT, Vec2i};

/// Opaque identifier for a root window, dialog, or popup registered with [`Context`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RootId(usize);

impl RootId {
    pub(crate) fn raw(self) -> usize {
        self.0
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum RootKind {
    Window,
    Dialog,
    Popup,
}

struct RootEntry {
    id: RootId,
    handle: WindowHandle,
    tree: WidgetTree,
    opt: ContainerOption,
    scroll_behavior: ScrollBehavior,
    visible: bool,
    kind: RootKind,
}

/// Primary entry point used to drive the UI over a renderer implementation.
pub struct Context<R: Renderer> {
    canvas: Canvas<R>,
    style: Rc<Style>,

    last_zindex: i32,
    frame: usize,
    hover_root: Option<WindowHandle>,
    next_hover_root: Option<WindowHandle>,

    root_list: Vec<WindowHandle>,
    retained_roots: Vec<RootEntry>,
    next_root_id: usize,
    frame_results: FrameResults,

    input: Rc<RefCell<Input>>,
}

impl<R: Renderer> Context<R> {
    /// Creates a new UI context around the provided renderer and dimensions.
    pub fn new(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        let canvas = Canvas::from(renderer, dim);
        let style = Style::default().with_named_fonts(&canvas.get_atlas());
        Self {
            canvas,
            style: Rc::new(style),
            last_zindex: 0,
            frame: 0,
            hover_root: None,
            next_hover_root: None,

            root_list: Vec::default(),
            retained_roots: Vec::default(),
            next_root_id: 1,
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
        container::Command, widget::widget_id_of_handle, widget_handle, AtlasHandle, AtlasSource, CharEntry, ControlState, FontEntry, ResourceState,
        SourceFormat, TextBlock, Widget, WidgetCtx, WidgetOption, WidgetTreeBuilder,
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
        fn push_triangle_vertices(&mut self, _v0: &crate::canvas::Vertex, _v1: &crate::canvas::Vertex, _v2: &crate::canvas::Vertex) {}
        fn flush(&mut self) {}
        fn end(&mut self) {}
        fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
            Ok(())
        }
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

    fn make_named_font_test_atlas() -> AtlasHandle {
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
        let fonts = vec![
            (
                "small",
                FontEntry {
                    line_size: 10,
                    baseline: 8,
                    font_size: 10,
                    entries: &entries,
                },
            ),
            (
                "body",
                FontEntry {
                    line_size: 12,
                    baseline: 9,
                    font_size: 12,
                    entries: &entries,
                },
            ),
            (
                "title",
                FontEntry {
                    line_size: 16,
                    baseline: 12,
                    font_size: 16,
                    entries: &entries,
                },
            ),
        ];
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

    fn window_texts(window: &WindowHandle) -> Vec<String> {
        window
            .inner()
            .main
            .debug_commands()
            .iter()
            .filter_map(|cmd| match cmd {
                Command::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn rendered_root_names(ctx: &Context<NoopRenderer>) -> Vec<String> {
        ctx.root_list.iter().map(|window| window.inner().main.name().to_string()).collect()
    }

    struct AlwaysSubmitWidget {
        label: &'static str,
        opt: WidgetOption,
        scroll_behavior: ScrollBehavior,
    }

    impl AlwaysSubmitWidget {
        fn new(label: &'static str) -> Self {
            Self {
                label,
                opt: WidgetOption::NONE,
                scroll_behavior: ScrollBehavior::NONE,
            }
        }
    }

    impl Widget for AlwaysSubmitWidget {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn scroll_behavior(&self) -> ScrollBehavior {
            self.scroll_behavior
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
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });

        let inner = window.inner();
        let body = inner.main.body();
        let has_vertical_scrollbar =
            inner.main.debug_commands().iter().any(
                |cmd| matches!(cmd, Command::Recti { rect, .. } if rect.x == body.x + body.width && rect.width == style.scrollbar_size && rect.height > 0),
            );

        assert!(has_vertical_scrollbar);
    }

    #[test]
    fn set_style_rebinds_default_font_fields_from_named_atlas() {
        let atlas = make_named_font_test_atlas();
        let body = atlas.font_id("body").unwrap();
        let small = atlas.font_id("small").unwrap();
        let title = atlas.font_id("title").unwrap();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));

        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        ctx.set_style(&style);

        assert_eq!(ctx.style.font, body);
        assert_eq!(ctx.style.small_font, small);
        assert_eq!(ctx.style.title_font, title);
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
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });

        let initial_rect = window.rect();
        let corner_x = initial_rect.x + initial_rect.width - 1;
        let corner_y = initial_rect.y + initial_rect.height - 1;

        ctx.mousemove(corner_x, corner_y);
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });

        ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });

        ctx.mousemove(corner_x + 12, corner_y + 10);
        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
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
            inner.main.debug_push_command(Command::None);
            inner.main.debug_push_clip(UNCLIPPED_RECT);
            inner.main.set_content_size(Dimensioni::new(11, 17));
            inner.main.set_scroll(Vec2i::new(3, 5));
        }

        window.close();

        let inner = window.inner();
        assert!(inner.main.debug_commands().is_empty());
        assert_eq!(inner.main.content_size().width, 0);
        assert_eq!(inner.main.content_size().height, 0);
        assert_eq!(inner.main.scroll().x, 0);
        assert_eq!(inner.main.scroll().y, 0);
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
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });
        ctx.frame(|_ui| {});

        {
            let mut inner = window.inner_mut();
            inner.main.debug_push_command(Command::None);
        }

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });

        let inner = window.inner();
        assert!(!inner.main.debug_commands().iter().any(|cmd| matches!(cmd, Command::None)));
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
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
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
            ui.dialog(&mut dialog, opt, ScrollBehavior::NONE, &first_tree);
        });

        dialog.close();

        dialog.open();
        ctx.frame(|ui| {
            ui.dialog(&mut dialog, opt, ScrollBehavior::NONE, &second_tree);
        });

        let inner = dialog.inner();
        let texts: Vec<String> = inner
            .main
            .debug_commands()
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
    fn open_dialog_does_not_bump_zindex_every_frame() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let mut background = ctx.new_window("background", rect(0, 0, 100, 80));
        let mut dialog = ctx.new_dialog("dialog", rect(10, 10, 80, 40));
        let background_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("background");
        });
        let dialog_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("dialog");
        });

        ctx.open_dialog(&mut dialog);
        ctx.frame(|ui| {
            ui.window(&mut background, ContainerOption::NONE, ScrollBehavior::NONE, &background_tree);
            ui.dialog(&mut dialog, ContainerOption::NONE, ScrollBehavior::NONE, &dialog_tree);
        });

        let first_zindex = dialog.zindex();
        assert!(first_zindex > background.zindex());

        ctx.frame(|ui| {
            ui.window(&mut background, ContainerOption::NONE, ScrollBehavior::NONE, &background_tree);
            ui.dialog(&mut dialog, ContainerOption::NONE, ScrollBehavior::NONE, &dialog_tree);
        });

        assert_eq!(dialog.zindex(), first_zindex);
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
                tree.container(panel.clone(), ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                    tree.text("panel child");
                });
            }
        });
        let tree_without_panel = WidgetTreeBuilder::build(|tree| {
            tree.text("root only");
        });

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree_with_panel);
        });
        assert_eq!(window.inner().main.panel_count(), 1);

        ctx.frame(|_ui| {});

        ctx.frame(|ui| {
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree_without_panel);
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
            ui.popup(&mut popup, ScrollBehavior::NONE, &tree);
        });

        let inner = popup.inner();
        assert!(inner.main.rect().width > 1);
        assert!(inner.main.rect().height > 1);
        assert!(inner.main.body().width > 0);
        assert!(inner.main.body().height > 0);
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
            ui.window(&mut window, ContainerOption::AUTO_SIZE, ScrollBehavior::NONE, &tree);
        });

        let inner = window.inner();
        assert!(inner.main.rect().width > 1);
        assert!(inner.main.rect().height > 1);
        assert!(inner.main.body().y > inner.main.rect().y);
        assert!(inner.main.body().height > 0);
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
            ui.popup(&mut popup, ScrollBehavior::NONE, &short_tree);
        });
        let first_width = popup.rect().width;

        ctx.frame(|ui| {
            ui.popup(&mut popup, ScrollBehavior::NONE, &long_tree);
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
            ui.window(&mut window, ContainerOption::AUTO_SIZE, ScrollBehavior::NONE, &tree);
        });

        let inner = window.inner();
        assert!(inner.main.body().width >= inner.main.content_size().width);
        assert!(inner.main.body().height >= inner.main.content_size().height);
        assert!(inner.main.body().height > 0);
        assert!(inner.main.body().y > inner.main.rect().y);

        let body = inner.main.body();
        let has_vertical_scrollbar =
            inner.main.debug_commands().iter().any(
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
            ui.window(&mut titled, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
            ui.window(&mut plain, ContainerOption::NO_TITLE, ScrollBehavior::NONE, &tree);
        });

        let titled_inner = titled.inner();
        assert!(titled_inner.main.body().y > titled_inner.main.rect().y);
        assert!(titled_inner.main.body().height < titled_inner.main.rect().height);
        let titled_texts: Vec<String> = titled_inner
            .main
            .debug_commands()
            .iter()
            .filter_map(|cmd| match cmd {
                Command::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(titled_texts.iter().any(|text| text == "titled"));

        let plain_inner = plain.inner();
        assert_eq!(plain_inner.main.body().y, plain_inner.main.rect().y);
        assert_eq!(plain_inner.main.body().height, plain_inner.main.rect().height);
        assert_eq!(plain_inner.main.body().x, plain_inner.main.rect().x);
        assert_eq!(plain_inner.main.body().width, plain_inner.main.rect().width);
        let plain_texts: Vec<String> = plain_inner
            .main
            .debug_commands()
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
                ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
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
                ui.window(&mut left, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
                ui.window(&mut right, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
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
            ui.window(&mut window, ContainerOption::NONE, ScrollBehavior::NONE, &tree);
        });

        assert!(ctx.committed_results().state_of_handle(&first).is_submitted());
        assert!(ctx.committed_results().state_of_handle(&second).is_submitted());
    }

    #[test]
    fn registered_window_renders_across_frames_without_resubmission() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let text = widget_handle(TextBlock::new("before"));
        let tree = WidgetTreeBuilder::build({
            let text = text.clone();
            move |tree| {
                tree.widget(text.clone());
            }
        });
        let root = ctx.create_window("retained", rect(0, 0, 90, 50), tree);

        ctx.update_ui();
        let handle = ctx.root_handle(root).unwrap();
        assert!(window_texts(&handle).iter().any(|text| text == "before"));

        text.borrow_mut().text = "after".to_string();
        ctx.update_ui();
        let handle = ctx.root_handle(root).unwrap();
        let texts = window_texts(&handle);
        assert!(texts.iter().any(|text| text == "after"));
        assert!(!texts.iter().any(|text| text == "before"));
    }

    #[test]
    fn retained_root_visibility_controls_rendering() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let tree = WidgetTreeBuilder::build(|tree| {
            tree.text("visible");
        });
        let root = ctx.create_window("retained", rect(0, 0, 90, 50), tree);

        ctx.update_ui();
        assert_eq!(rendered_root_names(&ctx), vec!["retained"]);

        ctx.set_root_visible(root, false);
        ctx.update_ui();
        assert!(rendered_root_names(&ctx).is_empty());
        assert!(window_texts(&ctx.root_handle(root).unwrap()).is_empty());

        ctx.set_root_visible(root, true);
        ctx.update_ui();
        assert_eq!(rendered_root_names(&ctx), vec!["retained"]);
        assert!(window_texts(&ctx.root_handle(root).unwrap()).iter().any(|text| text == "visible"));
    }

    #[test]
    fn retained_root_tree_can_be_replaced_after_registration() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(200, 200));
        let first_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("first");
        });
        let second_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("second");
        });
        let root = ctx.create_window("retained", rect(0, 0, 90, 50), first_tree);

        ctx.update_ui();
        assert!(window_texts(&ctx.root_handle(root).unwrap()).iter().any(|text| text == "first"));

        ctx.set_root_tree(root, second_tree);
        ctx.update_ui();
        let texts = window_texts(&ctx.root_handle(root).unwrap());
        assert!(texts.iter().any(|text| text == "second"));
        assert!(!texts.iter().any(|text| text == "first"));
    }

    #[test]
    fn retained_and_compatibility_roots_can_share_a_frame() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let retained_root = ctx.create_window(
            "retained",
            rect(0, 0, 90, 60),
            WidgetTreeBuilder::build(|tree| {
                tree.text("retained");
            }),
        );
        let mut legacy_window = ctx.new_window("legacy", rect(100, 0, 90, 60));
        let legacy_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("legacy");
        });

        ctx.run_ui_frame(|ui| {
            ui.window(&mut legacy_window, ContainerOption::NONE, ScrollBehavior::NONE, &legacy_tree);
        });

        let names = rendered_root_names(&ctx);
        assert!(names.iter().any(|name| name == "retained"));
        assert!(names.iter().any(|name| name == "legacy"));
        assert!(window_texts(&ctx.root_handle(retained_root).unwrap()).iter().any(|text| text == "retained"));
        assert!(window_texts(&legacy_window).iter().any(|text| text == "legacy"));
    }

    #[test]
    fn retained_roots_preserve_z_order_and_fronting() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let left_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("left");
        });
        let right_tree = WidgetTreeBuilder::build(|tree| {
            tree.text("right");
        });
        let left = ctx.create_window("left", rect(0, 0, 90, 60), left_tree);
        let _right = ctx.create_window("right", rect(20, 0, 90, 60), right_tree);

        ctx.update_ui();
        assert_eq!(rendered_root_names(&ctx), vec!["left", "right"]);

        let mut left_handle = ctx.root_handle(left).unwrap();
        ctx.bring_to_front(&mut left_handle);
        ctx.update_ui();
        assert_eq!(rendered_root_names(&ctx), vec!["right", "left"]);
    }

    #[test]
    fn retained_dialog_becomes_front_root_when_shown() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let background = ctx.create_window(
            "background",
            rect(0, 0, 120, 80),
            WidgetTreeBuilder::build(|tree| {
                tree.text("background");
            }),
        );
        let dialog = ctx.create_dialog(
            "dialog",
            rect(10, 10, 90, 50),
            WidgetTreeBuilder::build(|tree| {
                tree.text("dialog");
            }),
        );

        ctx.update_ui();
        assert_eq!(rendered_root_names(&ctx), vec!["background"]);

        ctx.set_root_visible(dialog, true);
        ctx.update_ui();
        assert_eq!(rendered_root_names(&ctx), vec!["background", "dialog"]);
        assert!(ctx.root_handle(dialog).unwrap().zindex() > ctx.root_handle(background).unwrap().zindex());
    }

    #[test]
    fn retained_popup_opens_at_mouse_and_auto_sizes_on_first_frame() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let popup = ctx.create_popup(
            "popup",
            WidgetTreeBuilder::build(|tree| {
                tree.text("hello popup");
            }),
        );

        ctx.mousemove(40, 50);
        ctx.set_root_visible(popup, true);
        ctx.update_ui();

        let handle = ctx.root_handle(popup).unwrap();
        assert_eq!(handle.rect().x, 40);
        assert_eq!(handle.rect().y, 50);
        assert!(handle.rect().width > 1);
        assert!(handle.rect().height > 1);
        assert_eq!(rendered_root_names(&ctx), vec!["popup"]);
    }

    #[test]
    fn retained_root_hover_selection_uses_registered_root_z_order() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let left = ctx.create_window(
            "left",
            rect(0, 0, 100, 80),
            WidgetTreeBuilder::build(|tree| {
                tree.text("left");
            }),
        );
        let _right = ctx.create_window(
            "right",
            rect(0, 0, 100, 80),
            WidgetTreeBuilder::build(|tree| {
                tree.text("right");
            }),
        );

        ctx.mousemove(20, 20);
        ctx.update_ui();
        assert_eq!(ctx.hover_root.as_ref().unwrap().inner().main.name(), "right");

        let mut left_handle = ctx.root_handle(left).unwrap();
        ctx.bring_to_front(&mut left_handle);
        ctx.update_ui();
        assert_eq!(ctx.hover_root.as_ref().unwrap().inner().main.name(), "left");
    }

    #[test]
    fn retained_chrome_node_ids_are_root_derived_and_stable() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let root = ctx.create_window(
            "retained",
            rect(0, 0, 100, 70),
            WidgetTreeBuilder::build(|tree| {
                tree.text("before");
            }),
        );
        let other = ctx.create_window(
            "other",
            rect(120, 0, 100, 70),
            WidgetTreeBuilder::build(|tree| {
                tree.text("other");
            }),
        );

        let ids = ctx.root_handle(root).unwrap().inner().chrome_ids;
        assert_eq!(ids, WindowChromeIds::from_root_seed(root.raw()));
        assert_ne!(ids, ctx.root_handle(other).unwrap().inner().chrome_ids);

        ctx.update_ui();
        ctx.set_root_tree(
            root,
            WidgetTreeBuilder::build(|tree| {
                tree.text("after");
            }),
        );
        ctx.update_ui();

        assert_eq!(ctx.root_handle(root).unwrap().inner().chrome_ids, ids);
    }

    #[test]
    fn retained_chrome_nodes_are_recorded_in_root_cache() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let root = ctx.create_window(
            "retained",
            rect(10, 12, 100, 70),
            WidgetTreeBuilder::build(|tree| {
                tree.text("body");
            }),
        );
        let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids;

        ctx.update_ui();

        let handle = ctx.root_handle(root).unwrap();
        let inner = handle.inner();
        let title = inner.main.previous_node_layout(chrome.title).expect("title chrome node layout missing");
        let close = inner.main.previous_node_layout(chrome.close).expect("close chrome node layout missing");
        let resize = inner.main.previous_node_layout(chrome.resize).expect("resize chrome node layout missing");

        assert_eq!(title.rect.x, 10);
        assert_eq!(title.rect.y, 12);
        assert_eq!(title.rect.width, 100);
        assert!(title.rect.height > 0);
        assert!(close.rect.x >= title.rect.x);
        assert!(resize.rect.x >= title.rect.x);
        assert!(resize.rect.y >= title.rect.y);
    }

    #[test]
    fn retained_title_drag_uses_chrome_node_after_tree_update() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let root = ctx.create_window(
            "retained",
            rect(10, 10, 100, 70),
            WidgetTreeBuilder::build(|tree| {
                tree.text("before");
            }),
        );
        let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids;

        ctx.update_ui();
        ctx.set_root_tree(
            root,
            WidgetTreeBuilder::build(|tree| {
                tree.text("after");
            }),
        );
        ctx.update_ui();

        let initial = ctx.root_handle(root).unwrap().rect();
        let title_x = initial.x + 10;
        let title_y = initial.y + 6;
        ctx.mousemove(title_x, title_y);
        ctx.update_ui();
        ctx.update_ui();
        ctx.mousedown(title_x, title_y, MouseButton::LEFT);
        ctx.update_ui();
        ctx.mousemove(title_x + 12, title_y + 7);
        ctx.update_ui();

        let moved = ctx.root_handle(root).unwrap().rect();
        assert!(moved.x > initial.x);
        assert!(moved.y > initial.y);
        assert!(ctx.committed_results().state_of_node(chrome.title).is_active());
        assert_eq!(ctx.root_handle(root).unwrap().inner().chrome_ids, chrome);
    }

    #[test]
    fn retained_close_button_closes_root_and_records_chrome_result() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let root = ctx.create_window(
            "retained",
            rect(10, 10, 100, 70),
            WidgetTreeBuilder::build(|tree| {
                tree.text("body");
            }),
        );
        let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids;

        let close_x = 10 + 100 - 12;
        let close_y = 10 + 6;
        ctx.mousemove(close_x, close_y);
        ctx.update_ui();
        ctx.update_ui();
        assert!(ctx.root_handle(root).unwrap().is_open());

        ctx.mousedown(close_x, close_y, MouseButton::LEFT);
        ctx.update_ui();
        assert!(ctx.committed_results().state_of_node(chrome.close).is_submitted());
        assert!(!ctx.root_handle(root).unwrap().is_open());

        ctx.mouseup(close_x, close_y, MouseButton::LEFT);
        ctx.update_ui();
        assert!(rendered_root_names(&ctx).is_empty());
    }

    #[test]
    fn retained_resize_handle_wins_bottom_right_corner_over_window_scrollbars() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 240));
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        ctx.set_style(&style);

        let text = widget_handle(TextBlock::new("aaaaaaaaaaaaaaaaaaaaaaaa\na\na\na\na\na\na\na"));
        let tree = WidgetTreeBuilder::build({
            let text = text.clone();
            move |tree| {
                tree.widget(text.clone());
            }
        });
        let root = ctx.create_window("retained", rect(0, 0, 60, 40), tree);
        let chrome = ctx.root_handle(root).unwrap().inner().chrome_ids;

        ctx.update_ui();
        ctx.update_ui();

        let initial_rect = ctx.root_handle(root).unwrap().rect();
        let corner_x = initial_rect.x + initial_rect.width - 1;
        let corner_y = initial_rect.y + initial_rect.height - 1;

        ctx.mousemove(corner_x, corner_y);
        ctx.update_ui();
        ctx.mousedown(corner_x, corner_y, MouseButton::LEFT);
        ctx.update_ui();
        ctx.mousemove(corner_x + 12, corner_y + 10);
        ctx.update_ui();

        let resized = ctx.root_handle(root).unwrap().rect();
        assert!(resized.width > initial_rect.width);
        assert!(resized.height > initial_rect.height);
        assert!(ctx.committed_results().state_of_node(chrome.resize).is_active());
    }

    #[test]
    fn retained_popup_closes_from_root_state_when_clicking_outside() {
        let atlas = make_test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(240, 120));
        let popup = ctx.create_popup(
            "popup",
            WidgetTreeBuilder::build(|tree| {
                tree.text("popup");
            }),
        );

        ctx.mousemove(20, 20);
        ctx.set_root_visible(popup, true);
        ctx.update_ui();
        assert!(ctx.root_handle(popup).unwrap().is_open());

        ctx.mousemove(200, 100);
        ctx.update_ui();
        ctx.mousedown(200, 100, MouseButton::LEFT);
        ctx.update_ui();

        assert!(!ctx.root_handle(popup).unwrap().is_open());
        assert!(rendered_root_names(&ctx).is_empty());
    }
}

impl<R: Renderer> Context<R> {
    /// Begins a renderer draw pass for the current viewport.
    ///
    /// Call this once after determining the viewport size and before presenting UI commands for
    /// the frame. Input events may be collected before or after this call, as long as
    /// [`Context::run_ui_frame`] runs after the input state has been updated.
    pub fn begin_render_frame(&mut self, width: i32, height: i32, clr: Color) {
        self.canvas.begin(width, height, clr);
    }

    /// Flushes recorded root commands to the renderer and ends the draw pass.
    pub fn end_render_frame(&mut self) {
        for r in &mut self.root_list {
            r.render(&mut self.canvas);
        }
        self.canvas.end()
    }

    /// Begins a new draw pass on the underlying canvas.
    ///
    /// Prefer [`Context::begin_render_frame`] in new code; this method remains as a compatibility
    /// alias.
    pub fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.begin_render_frame(width, height, clr);
    }

    /// Flushes recorded draw commands to the renderer and ends the draw pass.
    ///
    /// Prefer [`Context::end_render_frame`] in new code; this method remains as a compatibility
    /// alias.
    pub fn end(&mut self) {
        self.end_render_frame()
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
            r.set_root_hover_active(false);
        }
        match &mut self.hover_root {
            Some(window) => window.set_root_hover_active(true),
            _ => (),
        }

        // sort all windows
        self.root_list.sort_by(|a, b| a.zindex().cmp(&b.zindex()));
    }

    /// Runs the retained UI traversal and publishes frame results.
    ///
    /// This wraps input prelude/epilogue, root bookkeeping, retained layout, widget execution, and
    /// result commit. Renderer presentation remains explicit through
    /// [`Context::begin_render_frame`] and [`Context::end_render_frame`].
    pub fn run_ui_frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.frame_begin();

        f(self);
        self.render_registered_roots();

        self.frame_end();
    }

    /// Runs one UI frame using only roots previously registered with this context.
    ///
    /// This is the retained-root entry point: applications can create roots once with
    /// [`Context::create_window`], [`Context::create_dialog`], or [`Context::create_popup`], mutate
    /// widget handle state over time, and call this method each frame without re-submitting root
    /// trees.
    pub fn update_ui(&mut self) {
        self.frame_begin();
        self.render_registered_roots();
        self.frame_end();
    }

    /// Runs the UI for a single frame by wrapping input/layout bookkeeping.
    ///
    /// Prefer [`Context::run_ui_frame`] in new code; this method remains as a compatibility alias.
    /// Rendering still requires calling [`Context::begin_render_frame`] and
    /// [`Context::end_render_frame`].
    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.run_ui_frame(f);
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

    fn next_root_id(&mut self) -> RootId {
        let id = RootId(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained root id counter overflowed");
        id
    }

    fn register_root(
        &mut self,
        kind: RootKind,
        mut handle: WindowHandle,
        tree: WidgetTree,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        visible: bool,
    ) -> RootId {
        let id = self.next_root_id();
        handle.set_chrome_ids(WindowChromeIds::from_root_seed(id.raw()));
        self.retained_roots.push(RootEntry {
            id,
            handle,
            tree,
            opt,
            scroll_behavior,
            visible,
            kind,
        });
        id
    }

    fn root_entry_mut(&mut self, root: RootId) -> Option<&mut RootEntry> {
        self.retained_roots.iter_mut().find(|entry| entry.id == root)
    }

    /// Registers an open retained window and returns its stable root identifier.
    ///
    /// The window is rendered by subsequent calls to [`Context::update_ui`] or
    /// [`Context::run_ui_frame`] without the application re-submitting its tree.
    pub fn create_window(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        let window = self.new_window(name, rect);
        self.register_root(RootKind::Window, window, tree, ContainerOption::NONE, ScrollBehavior::NONE, true)
    }

    /// Registers a retained dialog root.
    ///
    /// Dialogs start hidden; call [`Context::set_root_visible`] with `true` to open the dialog and
    /// bring it to the front.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        let dialog = self.new_dialog(name, rect);
        self.register_root(RootKind::Dialog, dialog, tree, ContainerOption::NONE, ScrollBehavior::NONE, false)
    }

    /// Registers a retained popup root.
    ///
    /// Popups start hidden; calling [`Context::set_root_visible`] with `true` opens the popup at the
    /// current mouse position.
    pub fn create_popup(&mut self, name: &str, tree: WidgetTree) -> RootId {
        let popup = self.new_popup(name);
        self.register_root(RootKind::Popup, popup, tree, Self::default_popup_options(), ScrollBehavior::NONE, false)
    }

    /// Replaces the retained widget tree for a registered root.
    ///
    /// Invalid root identifiers are ignored.
    pub fn set_root_tree(&mut self, root: RootId, tree: WidgetTree) {
        if let Some(entry) = self.root_entry_mut(root) {
            entry.tree = tree;
        }
    }

    /// Replaces the container options and scroll behavior for a registered root.
    ///
    /// Popups are created with the same default options as [`Context::popup`]. This method can
    /// override those defaults for retained roots that need custom chrome or sizing.
    pub fn set_root_options(&mut self, root: RootId, opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        if let Some(entry) = self.root_entry_mut(root) {
            entry.opt = opt;
            entry.scroll_behavior = scroll_behavior;
        }
    }

    /// Shows or hides a registered retained root.
    ///
    /// Windows reopen in their existing z-order. Dialogs and newly opened popups are brought to the
    /// front, matching the compatibility APIs.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) {
        let Some(index) = self.retained_roots.iter().position(|entry| entry.id == root) else {
            return;
        };
        self.set_root_visible_at(index, visible);
    }

    /// Returns the window handle owned by a registered retained root.
    pub fn root_handle(&self, root: RootId) -> Option<WindowHandle> {
        self.retained_roots.iter().find(|entry| entry.id == root).map(|entry| entry.handle.clone())
    }

    /// Bumps the window's Z order so it renders above others.
    pub fn bring_to_front(&mut self, window: &mut WindowHandle) {
        self.last_zindex += 1;
        window.set_zindex(self.last_zindex);
    }

    fn set_root_visible_at(&mut self, index: usize, visible: bool) {
        let mouse_pos = self.input.borrow().mouse_pos;
        let mut bring_to_front = None;
        let mut hover_root = None;

        {
            let entry = &mut self.retained_roots[index];
            if !visible {
                entry.visible = false;
                entry.handle.close();
                return;
            }

            let was_open = entry.handle.is_open();
            entry.visible = true;
            match entry.kind {
                RootKind::Window => {
                    entry.handle.open();
                }
                RootKind::Dialog => {
                    entry.handle.open();
                    if !was_open {
                        bring_to_front = Some(entry.handle.clone());
                    }
                }
                RootKind::Popup => {
                    if !was_open {
                        entry.handle.set_rect(rect(mouse_pos.x, mouse_pos.y, 1, 1));
                        entry.handle.open();
                        entry.handle.set_root_hover_active(true);
                        entry.handle.mark_popup_just_opened();
                        bring_to_front = Some(entry.handle.clone());
                        hover_root = Some(entry.handle.clone());
                    }
                }
            }
        }

        if let Some(mut window) = bring_to_front {
            self.bring_to_front(&mut window);
        }
        if let Some(window) = hover_root {
            self.next_hover_root = Some(window.clone());
            self.hover_root = Some(window);
        }
    }

    fn bring_to_front_if_behind(&mut self, window: &mut WindowHandle) {
        if window.zindex() < self.last_zindex {
            self.bring_to_front(window);
        }
    }

    #[inline(never)]
    fn begin_root_container(&mut self, window: &mut WindowHandle) {
        window.prepare_for_frame(self.frame);
        self.root_list.push(window.clone());

        if window.root_contains_point(self.input.borrow().mouse_pos)
            && (self.next_hover_root.is_none() || window.zindex() > self.next_hover_root.as_ref().unwrap().zindex())
        {
            self.next_hover_root = Some(window.clone());
        }
        let scroll_delta = self.input.borrow().scroll_delta;
        let pending_scroll = if window.root_in_hover_root() && (scroll_delta.x != 0 || scroll_delta.y != 0) {
            Some(scroll_delta)
        } else {
            None
        };
        window.begin_root_command_scope(pending_scroll);
    }

    #[inline(never)]
    fn end_root_container(&mut self, window: &mut WindowHandle) {
        window.finish_root_command_scope();
    }

    #[inline(never)]
    #[must_use]
    fn begin_window(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior) -> bool {
        if !window.is_open() {
            return false;
        }

        if !self.update_popup_root_state(window) {
            return false;
        }

        self.begin_root_container(window);
        window.begin_window(&mut self.frame_results, opt, scroll_behavior);

        true
    }

    fn end_window(&mut self, window: &mut WindowHandle, opt: ContainerOption) {
        window.end_window();
        self.end_root_container(window);
        window.finish_resize(&mut self.frame_results, opt);
    }

    fn update_popup_root_state(&mut self, window: &mut WindowHandle) -> bool {
        if !window.root_is_popup() {
            return true;
        }

        if window.root_popup_just_opened() {
            window.clear_root_popup_just_opened();
            return true;
        }

        if !self.input.borrow().mouse_pressed.is_none() && !window.root_in_hover_root() {
            window.close();
            return false;
        }

        true
    }

    fn render_window_tree(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        if window.is_open() {
            window.set_root_style(self.style.clone());
            if opt.is_auto_sizing() {
                window.measure_auto_size(&self.frame_results, opt, scroll_behavior, tree);
            }
        }

        if self.begin_window(window, opt, scroll_behavior) {
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

    fn render_dialog_tree(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        if window.is_open() {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            window.set_root_hover_active(true);
            self.bring_to_front_if_behind(window);

            self.render_window_tree(window, opt, scroll_behavior, tree);
        }
    }

    fn render_retained_root(&mut self, entry: &mut RootEntry) {
        if !entry.visible {
            return;
        }
        if !entry.handle.is_open() {
            entry.visible = false;
            return;
        }

        match entry.kind {
            RootKind::Window => self.render_window_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Dialog => self.render_dialog_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Popup => self.render_window_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
        }

        if !entry.handle.is_open() {
            entry.visible = false;
        }
    }

    fn render_registered_roots(&mut self) {
        let mut roots = std::mem::take(&mut self.retained_roots);
        for entry in &mut roots {
            self.render_retained_root(entry);
        }
        self.retained_roots = roots;
    }

    const fn default_popup_options() -> ContainerOption {
        ContainerOption::AUTO_SIZE.union(ContainerOption::NO_RESIZE).union(ContainerOption::NO_TITLE)
    }

    /// Opens a window and renders the provided retained widget tree into it.
    ///
    /// This is the per-frame root-submission compatibility API. New retained-root code should use
    /// [`Context::create_window`] plus [`Context::update_ui`].
    pub fn window(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        self.render_window_tree(window, opt, scroll_behavior, tree);
    }

    /// Marks a dialog window as open for the next frame.
    pub fn open_dialog(&mut self, window: &mut WindowHandle) {
        let was_open = window.is_open();
        window.open();
        if !was_open {
            self.bring_to_front(window);
        }
    }

    /// Renders a dialog window if it is currently open.
    ///
    /// This is the per-frame root-submission compatibility API. New retained-root code should use
    /// [`Context::create_dialog`] plus [`Context::set_root_visible`].
    pub fn dialog(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        self.render_dialog_tree(window, opt, scroll_behavior, tree);
    }

    /// Shows a popup at the mouse cursor position.
    pub fn open_popup(&mut self, window: &mut WindowHandle) {
        let was_open = window.is_open();
        let mouse_pos = self.input.borrow().mouse_pos;
        if was_open {
            let mut rect = window.rect();
            rect.x = mouse_pos.x;
            rect.y = mouse_pos.y;
            window.set_rect(rect);
        } else {
            window.set_rect(rect(mouse_pos.x, mouse_pos.y, 1, 1));
            window.open();
            window.set_root_hover_active(true);
            window.mark_popup_just_opened();
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
        if was_open {
            let mut rect = window.rect();
            rect.x = anchor.x;
            rect.y = anchor.y;
            rect.width = anchor.width;
            window.set_rect(rect);
        } else {
            window.set_rect(anchor);
            window.open();
            window.set_root_hover_active(true);
            window.mark_popup_just_opened();
        }
        if !was_open {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            self.bring_to_front(window);
        }
    }

    /// Opens a popup window with default options and renders a retained tree into it.
    ///
    /// This is the per-frame root-submission compatibility API. New retained-root code should use
    /// [`Context::create_popup`] plus [`Context::set_root_visible`].
    pub fn popup(&mut self, window: &mut WindowHandle, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        let opt = Self::default_popup_options();
        self.render_window_tree(window, opt, scroll_behavior, tree);
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
    ///
    /// Unset/default font fields are rebound automatically from the current atlas when it exposes
    /// the conventional `body` / `small` / `title` / `heading` / `mono` font names. Use
    /// [`Style::with_named_fonts`] or [`Style::bind_named_fonts`] when you want to force all
    /// semantic roles to those atlas bindings explicitly.
    pub fn set_style(&mut self, style: &Style) {
        let mut resolved = style.clone();
        resolved.bind_default_named_fonts(&self.canvas.get_atlas());
        self.style = Rc::new(resolved)
    }

    /// Returns the underlying canvas used for advanced backend inspection.
    ///
    /// Application code should prefer the higher-level context image APIs and retained widget
    /// rendering. Backend tests can name this type as [`crate::backend::Canvas`].
    pub fn canvas(&self) -> &crate::backend::Canvas<R> {
        &self.canvas
    }

    /// Attempts to upload an RGBA image to the renderer and returns its [`TextureId`].
    ///
    /// Dimensions and byte length are validated before an id is allocated. Backend upload errors
    /// are returned without recording texture state in the canvas.
    pub fn try_load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, String> {
        self.canvas.try_load_texture_rgba(width, height, pixels)
    }

    /// Uploads an RGBA image to the renderer and returns its [`TextureId`].
    ///
    /// Panics if the RGBA dimensions/byte length are invalid or the backend rejects the upload.
    /// Prefer [`Context::try_load_image_rgba`] when callers can handle upload failure.
    #[track_caller]
    pub fn load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId {
        self.try_load_image_rgba(width, height, pixels).expect("failed to upload RGBA image")
    }

    /// Deletes a previously uploaded texture.
    pub fn free_image(&mut self, id: TextureId) {
        self.canvas.free_texture(id);
    }

    /// Uploads texture data described by `source`. PNG decoding is only available when the
    /// `png_source` (or `builder`) feature is enabled.
    pub fn load_image_from(&mut self, source: ImageSource) -> Result<TextureId, String> {
        match source {
            ImageSource::Raw { width, height, pixels } => self.try_load_image_rgba(width, height, pixels),
            #[cfg(any(feature = "builder", feature = "png_source"))]
            ImageSource::Png { bytes } => {
                let (width, height, rgba) = Self::decode_png(bytes)?;
                self.try_load_image_rgba(width, height, rgba.as_slice())
            }
        }
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
        let width = i32::try_from(info.width).map_err(|_| String::from("PNG width exceeds supported range"))?;
        let height = i32::try_from(info.height).map_err(|_| String::from("PNG height exceeds supported range"))?;
        let mut rgba = Vec::with_capacity(crate::atlas::checked_rgba_byte_len(width, height)?);
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
        Ok((width, height, rgba))
    }
}
