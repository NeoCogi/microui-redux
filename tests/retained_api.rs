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

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use microui_redux::render::{FrameError, FrameInfo, RendererBackend, RendererFrame, Vertex};
use microui_redux::retained::*;
use microui_redux::prelude::{Dimensioni, FileDialogRequest, FileDialogStatus, Recti};
use microui_redux::{
    color, rect, AtlasHandle, AtlasSource, Column, ColumnParameters, Context, Disclosure, DisclosureParameters, FontEntry, Grid, GridParameters, Policy,
    RootMutationError, Row, RowParameters, ScrollArea, ScrollAreaOption, ScrollAreaParameters, SizePolicy, SourceFormat, Stack, StackDirection,
    StackParameters, Style, TextureId,
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

fn context() -> Context<TestBackend> {
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
    Context::new(TestBackend { atlas: AtlasHandle::from(&source) })
}

#[test]
fn downstream_file_dialog_session_is_polled_and_cancelled_without_widget_access() {
    let mut context = context();
    let request: microui_redux::FileDialogRequest = FileDialogRequest::default();
    let session = context.open_file_dialog(request);

    assert_eq!(session.status(), FileDialogStatus::Pending);
    assert!(context.cancel_file_dialog(&session));
    assert_eq!(session.status(), FileDialogStatus::Cancelled);
    assert!(!context.cancel_file_dialog(&session));
}

#[test]
fn every_builtin_container_returns_a_typed_handle_and_completed_node() {
    let (row, row_node) = Row::create(RowParameters::new([], SizePolicy::Auto, []));
    let (grid, grid_node) = Grid::create(GridParameters::new([], [], std::iter::empty::<Node>()));
    let (stack, stack_node) = Stack::create(StackParameters::new(SizePolicy::Auto, SizePolicy::Auto, StackDirection::TopToBottom, []));
    let (scroll, scroll_node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, []));
    let (column, column_node) = Column::create(ColumnParameters::new([row_node, grid_node, stack_node, scroll_node]));
    let (disclosure, root_node) = Disclosure::create(DisclosureParameters::header("group", true, [column_node]));

    assert_eq!(row.try_read(|state| state.len()), Some(Some(0)));
    assert_eq!(grid.try_read(|state| state.len()), Some(Some(0)));
    assert_eq!(stack.try_read(|state| state.len()), Some(Some(0)));
    assert_eq!(scroll.try_read(|state| state.len()), Some(Some(0)));
    assert_eq!(column.try_read(|state| state.len()), Some(Some(4)));
    assert_eq!(disclosure.try_read(|state| state.len()), Some(Some(1)));
    drop(root_node);
    assert!(!row.is_alive());
    assert!(!disclosure.is_alive());
}

struct ExternalState {
    measure_calls: Cell<usize>,
    layout_calls: Cell<usize>,
    observed_policy: Cell<Option<Policy>>,
}

impl WidgetState for ExternalState {}

struct ExternalLayout {
    state: Rc<RefCell<ExternalState>>,
}

impl Layout for ExternalLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        let state = self.state.try_borrow().expect("external state must not be reentered");
        state.measure_calls.set(state.measure_calls.get() + 1);
        state.observed_policy.set(children.child_policy(0));
        children.measure_child(0, style, atlas, available).unwrap_or_else(|| Dimensioni::new(20, 20))
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        let state = self.state.try_borrow().expect("external state must not be reentered");
        state.layout_calls.set(state.layout_calls.get() + 1);
        assert!(ctx.child_policy(children, usize::MAX).is_none());
        assert!(ctx.layout_child(children, usize::MAX, rect).is_none());
        state.observed_policy.set(ctx.child_policy(children, 0));
        if !children.is_empty() {
            let _ = ctx.layout_child(children, 0, rect);
        }
    }
}

fn external_container(children: impl IntoIterator<Item = Node>) -> (WidgetStateHandle<ExternalState>, Container) {
    let state = Rc::new(RefCell::new(ExternalState {
        measure_calls: Cell::new(0),
        layout_calls: Cell::new(0),
        observed_policy: Cell::new(None),
    }));
    let handle = WidgetStateHandle::new(&state);
    let container = Container::new(ExternalLayout { state }, WidgetOption::NONE, children);
    (handle, container)
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

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(12, 9)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

#[test]
fn downstream_custom_container_measures_and_lays_out_through_public_scoped_apis() {
    let (child_state, child) = ExternalLeaf::create();
    let policy = Policy::fixed(24, 18);
    let children = [child.with_policy(policy)];
    let (state, runtime) = external_container(children);
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
    assert_eq!(state.try_read(|state| state.observed_policy.get()), Some(Some(policy)));
    assert!(child_state.is_alive());
    assert!(ctx.destroy_root(root.id()));
    assert!(!state.is_alive());
    assert!(!child_state.is_alive());
}

#[test]
fn root_creation_and_lifecycle_need_no_projection_or_generated_node_identity() {
    let mut ctx = context();
    let content = Column::create(ColumnParameters::default()).1;
    let root = ctx.create_window("root", rect(10, 20, 100, 80), content);
    let id = root.id();

    assert_eq!(root.state().try_read(|state| state.is_visible()), Some(true));
    ctx.set_root_visible(id, false).unwrap();
    assert_eq!(root.state().try_read(|state| state.is_visible()), Some(false));
    assert!(ctx.destroy_root(id));
    assert!(!root.state().is_alive());
    assert_eq!(ctx.set_root_rect(id, rect(0, 0, 1, 1)), Err(RootMutationError::UnknownRoot));
}
