use std::{cell::RefCell, rc::Rc};

use microui_redux::render::{FrameError, FrameInfo, RendererBackend, RendererFrame, Vertex};
use microui_redux::retained::*;
use microui_redux::prelude::{Dimensioni, Recti};
use microui_redux::{
    rect, AtlasHandle, AtlasSource, Column, ColumnParameters, Context, Disclosure, DisclosureParameters, Grid, GridParameters, RootMutationError, Row,
    RowParameters, ScrollArea, ScrollAreaOption, ScrollAreaParameters, SizePolicy, SourceFormat, Stack, StackDirection, StackParameters, Style, TextureId,
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
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &[],
        format: SourceFormat::Raw,
    };
    Context::new(TestBackend { atlas: AtlasHandle::from(&source) })
}

#[test]
fn every_builtin_container_returns_a_typed_handle_and_completed_node() {
    let (row, row_node) = Row::create(RowParameters::new([], SizePolicy::Auto, []));
    let (grid, grid_node) = Grid::create(GridParameters::new([], [], std::iter::empty::<Node>()));
    let (stack, stack_node) = Stack::create(StackParameters::new(SizePolicy::Auto, SizePolicy::Auto, StackDirection::TopToBottom, []));
    let (scroll, scroll_node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, []));
    let (column, column_node) = Column::create(ColumnParameters::new([row_node, grid_node, stack_node, scroll_node]));
    let (disclosure, root_node) = Disclosure::create(DisclosureParameters::header("group", true, [column_node]));

    assert_eq!(row.try_read(|state| state.len()), Some(0));
    assert_eq!(grid.try_read(|state| state.len()), Some(0));
    assert_eq!(stack.try_read(|state| state.len()), Some(0));
    assert_eq!(scroll.try_read(|state| state.len()), Some(0));
    assert_eq!(column.try_read(|state| state.len()), Some(4));
    assert_eq!(disclosure.try_read(|state| state.len()), Some(1));
    drop(root_node);
    assert!(!row.is_alive());
    assert!(!disclosure.is_alive());
}

struct ExternalParameters {
    children: Children,
}

impl WidgetParameters for ExternalParameters {}

struct ExternalState {
    children: Children,
}

impl WidgetState for ExternalState {}
impl ContainerState for ExternalState {}

struct ExternalContainer {
    state: Rc<RefCell<ExternalState>>,
    options: WidgetOption,
}

impl WidgetStateOwner for ExternalContainer {
    type State = ExternalState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for ExternalContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.options
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(20, 20)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) {}
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl Container for ExternalContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        let state = self.state.try_borrow().expect("external state must not be reentered");
        visitor.visit(&state.children);
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        let mut state = self.state.try_borrow_mut().expect("external state must not be reentered");
        visitor.visit(&mut state.children);
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        let mut state = self.state.try_borrow_mut().expect("external state must not be reentered");
        if !state.children.is_empty() {
            let _ = ctx.layout_child(&mut state.children, 0, rect);
        }
    }
}

struct ExternalBuilder;

impl ContainerBuilder for ExternalBuilder {
    type Parameters = ExternalParameters;
    type W = ExternalContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        ExternalContainer {
            state: Rc::new(RefCell::new(ExternalState { children: parameters.children })),
            options: WidgetOption::NONE,
        }
    }
}

#[test]
fn downstream_custom_container_uses_the_same_unique_node_boundary() {
    let runtime = ExternalBuilder::create_container(ExternalParameters { children: Children::new() });
    let state = runtime.state_handle();
    let node = Node::container(runtime);
    assert!(state.is_alive());
    drop(node);
    assert!(!state.is_alive());
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
