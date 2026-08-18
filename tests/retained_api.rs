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

use std::cell::Cell;

use microui_redux::render::{FrameError, FrameInfo, RendererBackend, RendererFrame, Vertex};
use microui_redux::retained::*;
use microui_redux::prelude::{Dimensioni, FileDialogRequest, FileDialogStatus, Recti};
use microui_redux::{
    color, rect, AtlasHandle, AtlasSource, Constraints, Context, Disclosure, DisclosureParameters, FontEntry, Grid, GridParameters, Linear, LinearParameters,
    RootMutationError, ScrollArea, ScrollAreaOption, ScrollAreaParameters, SourceFormat, Style, TextureId,
};

struct TestBackend {
    atlas: AtlasHandle,
}

struct TestFrame;

impl RendererFrame for TestFrame {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {}
    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}
    fn flush(&mut self) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

impl RendererBackend for TestBackend {
    type Frame<'a> = TestFrame;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(TestFrame)
    }

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

fn context_with_state<State: 'static>() -> Context<TestBackend, State> {
    // Build the smallest public atlas accepted by the downstream test renderer.
    let pixels = [255, 255, 255, 255];
    let icons = [("white", Recti::new(0, 0, 1, 1))];
    let font = FontEntry {
        line_size: 10,
        baseline: 8,
        font_size: 10,
        entries: &[],
    };
    let fonts = [("default", font)];
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
    };
    // Context infers the application state type from this helper's return value.
    Context::new(TestBackend { atlas: AtlasHandle::from(&source) })
}

fn context() -> Context<TestBackend> {
    // Most downstream tests use the polling-only unit application state.
    context_with_state()
}

#[derive(Default)]
struct FileDialogModel {
    /// Live session retained until its matching event reaches application state.
    session: Option<FileDialogSession>,
    /// Terminal result observed only through the Context-owned typed source.
    completion: Option<FileDialogStatus>,
}

impl FileDialogModel {
    /// Applies the completion belonging to this downstream model's live session.
    fn file_dialog_completed(&mut self, event: &FileDialogCompleted) {
        // Ignore another concurrent operation's event without inspecting either session status.
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if !event.is_for(session) {
            return;
        }

        // Retain the terminal payload and release the completed session in the callback itself.
        self.completion = Some(event.status().clone());
        self.session = None;
    }
}

#[test]
fn downstream_file_dialog_completion_is_subscriber_driven_without_widget_access() {
    let mut context = context_with_state::<FileDialogModel>();
    let completed = context.file_dialog_completed();
    context.subscribe(completed, FileDialogModel::file_dialog_completed).unwrap();
    let mut model = FileDialogModel {
        session: Some(context.open_file_dialog(FileDialogRequest::default())),
        ..FileDialogModel::default()
    };

    // Explicit cancellation queues one event; the retained update delivers it without frame polling.
    assert!(context.cancel_file_dialog(model.session.as_ref().unwrap()));
    assert!(model.completion.is_none());
    context.update_ui_state(Dimensioni::new(320, 240), &mut model);
    assert_eq!(model.completion, Some(FileDialogStatus::Cancelled));
    assert!(model.session.is_none());
}

#[test]
fn every_builtin_container_returns_a_typed_handle_and_completed_node() {
    let (row, row_node) = Linear::create(LinearParameters::horizontal(std::iter::empty::<Node>()));
    let (grid, grid_node) = Grid::create(GridParameters::new([], [], std::iter::empty::<Node>()));
    let (_, scroll_content) = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>()));
    let (scroll, scroll_node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, scroll_content));
    let (column, column_node) = Linear::create(LinearParameters::vertical([row_node, grid_node, scroll_node]));
    let (disclosure, root_node) = Disclosure::create(DisclosureParameters::header("group", true, [column_node]));

    assert_eq!(row.try_read(|state| state.len()), Some(Some(0)));
    assert_eq!(grid.try_read(|state| state.len()), Some(Some(0)));
    assert!(scroll.is_alive());
    assert_eq!(column.try_read(|state| state.len()), Some(Some(3)));
    assert_eq!(disclosure.try_read(|state| state.len()), Some(Some(1)));
    drop(root_node);
    assert!(!row.is_alive());
    assert!(!disclosure.is_alive());
}

struct ExternalContainer {
    measure_calls: Cell<usize>,
    layout_calls: Cell<usize>,
    allocated_child: Cell<Option<Dimensioni>>,
    options: WidgetOption,
}

impl ContainerWidget for ExternalContainer {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        self.measure_calls.set(self.measure_calls.get() + 1);
        ctx.measure_child(0, constraints).unwrap_or_else(|| Dimensioni::new(20, 20))
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        self.layout_calls.set(self.layout_calls.get() + 1);
        assert!(ctx.layout_child(children, usize::MAX, rect).is_none());
        if !children.is_empty() {
            self.allocated_child.set(ctx.layout_child(children, 0, rect));
        }
    }
}

impl Widget for ExternalContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.options
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

fn external_container(children: impl IntoIterator<Item = Node>) -> (TypedWidgetHandle<ExternalContainer>, Container) {
    Container::new(
        ExternalContainer {
            measure_calls: Cell::new(0),
            layout_calls: Cell::new(0),
            allocated_child: Cell::new(None),
            options: WidgetOption::NO_INTERACT,
        },
        children,
    )
}

struct ExternalLeaf {
    options: WidgetOption,
}

impl ExternalLeaf {
    fn create() -> (TypedWidgetHandle<Self>, Node) {
        Node::typed_widget(Self { options: WidgetOption::NONE })
    }
}

impl Widget for ExternalLeaf {
    fn widget_opt(&self) -> &WidgetOption {
        &self.options
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl LeafWidget for ExternalLeaf {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(12, 9)
    }
}

#[test]
fn downstream_custom_container_measures_and_lays_out_through_public_scoped_apis() {
    let (child_state, child) = ExternalLeaf::create();
    let (state, runtime) = external_container([child]);
    let node = Node::container(runtime);
    let mut ctx = context();
    let root = ctx.create_window("external", rect(10, 20, 100, 80), node);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let frame = FrameInfo::try_new(Dimensioni::new(320, 240), color(0, 0, 0, 255)).unwrap();

    ctx.update_ui(Dimensioni::new(320, 240));
    ctx.frame(frame).render_ui().unwrap();

    assert!(state.try_read(|state| state.measure_calls.get()).unwrap() > 0);
    assert!(state.try_read(|state| state.layout_calls.get()).unwrap() > 0);
    let allocated = state.try_read(|state| state.allocated_child.get()).flatten().unwrap();
    assert!(allocated.width > 12 && allocated.height > 9, "the parent rectangle must be authoritative");
    assert!(child_state.is_alive());
    assert!(ctx.destroy_root(root.id()));
    assert!(!state.is_alive());
    assert!(!child_state.is_alive());
}

#[test]
fn root_creation_and_lifecycle_need_no_projection_or_generated_node_identity() {
    let mut ctx = context();
    let content = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1;
    let root = ctx.create_window("root", rect(10, 20, 100, 80), content);
    let id = root.id();

    assert_eq!(root.widget().try_read(|widget| widget.is_visible()), Some(true));
    ctx.set_root_visible(id, false).unwrap();
    assert_eq!(root.widget().try_read(|widget| widget.is_visible()), Some(false));
    assert!(ctx.destroy_root(id));
    assert!(!root.widget().is_alive());
    assert_eq!(ctx.set_root_rect(id, rect(0, 0, 1, 1)), Err(RootMutationError::UnknownRoot));
}
