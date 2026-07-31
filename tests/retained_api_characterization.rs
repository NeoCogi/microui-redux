//! P0.0 characterization through the downstream public API surface.

use std::{cell::RefCell, rc::Rc};

use microui_redux::{prelude::*, render::Vertex, AtlasSource};

struct MarkerBackend {
    atlas: AtlasHandle,
    log: Rc<RefCell<Vec<&'static str>>>,
}

#[must_use]
struct MarkerFrame<'a> {
    log: &'a Rc<RefCell<Vec<&'static str>>>,
}

impl MarkerFrame<'_> {
    fn mark(&mut self, event: &'static str) {
        self.log.borrow_mut().push(event);
    }
}

impl RendererFrame for MarkerFrame<'_> {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {}
    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}
    fn flush(&mut self) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

impl RendererBackend for MarkerBackend {
    type Frame<'a> = MarkerFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(MarkerFrame { log: &self.log })
    }

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

struct P1Parameters {
    value: usize,
    log: Rc<RefCell<Vec<&'static str>>>,
}

impl WidgetParameters for P1Parameters {}

struct P1State {
    value: usize,
}

impl WidgetState for P1State {}

struct P1Builder;

struct P1Widget {
    state: Rc<RefCell<P1State>>,
    log: Rc<RefCell<Vec<&'static str>>>,
    opt: WidgetOption,
}

impl Widget for P1Widget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        self.state
            .try_borrow()
            .map(|_| Dimensioni::new(24, 12))
            .expect("state access closures must finish before retained traversal")
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        self.state.try_borrow_mut().expect("state access closures must finish before update").value += 1;
        self.log.borrow_mut().push("p1-update");
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        let _state = self.state.try_borrow().expect("state access closures must finish before paint");
        self.log.borrow_mut().push("p1-paint");
    }
}

impl WidgetStateOwner for P1Widget {
    type State = P1State;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl WidgetBuilder for P1Builder {
    type Parameters = P1Parameters;
    type W = P1Widget;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        P1Widget {
            state: Rc::new(RefCell::new(P1State { value: parameters.value })),
            log: parameters.log,
            opt: WidgetOption::NONE,
        }
    }
}

struct UnitParameters {
    log: Rc<RefCell<Vec<&'static str>>>,
}

impl WidgetParameters for UnitParameters {}

struct UnitBuilder;

struct UnitWidget {
    state: Rc<RefCell<()>>,
    log: Rc<RefCell<Vec<&'static str>>>,
    opt: WidgetOption,
}

impl Widget for UnitWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        self.state
            .try_borrow()
            .map(|_| Dimensioni::new(12, 8))
            .expect("state access closures must finish before retained traversal")
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        let _state = self.state.try_borrow_mut().expect("state access closures must finish before update");
        self.log.borrow_mut().push("unit-update");
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        let _state = self.state.try_borrow().expect("state access closures must finish before paint");
        self.log.borrow_mut().push("unit-paint");
    }
}

impl WidgetStateOwner for UnitWidget {
    type State = ();

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl WidgetBuilder for UnitBuilder {
    type Parameters = UnitParameters;
    type W = UnitWidget;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        UnitWidget {
            state: Rc::new(RefCell::new(())),
            log: parameters.log,
            opt: WidgetOption::NONE,
        }
    }
}

struct ExternalContainerParameters {
    children: Children,
}

impl WidgetParameters for ExternalContainerParameters {}

struct ExternalContainerState {
    children: Children,
    measure_calls: usize,
    layout_calls: usize,
}

impl WidgetState for ExternalContainerState {}
impl ContainerState for ExternalContainerState {}

struct ExternalContainerBuilder;

struct ExternalContainer {
    state: Rc<RefCell<ExternalContainerState>>,
    opt: WidgetOption,
}

impl Widget for ExternalContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        let mut state = self.state.try_borrow_mut().expect("external container state unavailable during measure");
        state.measure_calls += 1;
        let mut preferred = Dimensioni::default();
        for index in 0..state.children.len() {
            let child = state.children.measure_child(index, style, atlas, available).unwrap_or_default();
            preferred.width = preferred.width.max(child.width);
            preferred.height = preferred.height.saturating_add(child.height);
        }
        preferred
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl WidgetStateOwner for ExternalContainer {
    type State = ExternalContainerState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Container for ExternalContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        let state = self.state.try_borrow().expect("external container state unavailable during traversal");
        visitor.visit(&state.children);
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        let mut state = self
            .state
            .try_borrow_mut()
            .expect("external container state unavailable during mutable traversal");
        visitor.visit(&mut state.children);
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        let mut state = self.state.try_borrow_mut().expect("external container state unavailable during layout");
        state.layout_calls += 1;
        let count = state.children.len().max(1) as i32;
        let child_height = rect.height / count;
        for index in 0..state.children.len() {
            let y = rect.y + (index as i32) * child_height;
            let _ = ctx.layout_child(&mut state.children, index, Recti::new(rect.x, y, rect.width, child_height));
        }
    }
}

impl ContainerBuilder for ExternalContainerBuilder {
    type Parameters = ExternalContainerParameters;
    type W = ExternalContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        ExternalContainer {
            state: Rc::new(RefCell::new(ExternalContainerState {
                children: parameters.children,
                measure_calls: 0,
                layout_calls: 0,
            })),
            opt: WidgetOption::NONE,
        }
    }
}

fn atlas() -> AtlasHandle {
    let pixels = [0xFF; 4];
    let icons = [
        ("white", rect(0, 0, 1, 1)),
        ("close", rect(0, 0, 1, 1)),
        ("expand", rect(0, 0, 1, 1)),
        ("collapse", rect(0, 0, 1, 1)),
        ("check", rect(0, 0, 1, 1)),
        ("expand_down", rect(0, 0, 1, 1)),
    ];
    let characters = [(
        'a',
        CharEntry {
            offset: vec2(0, 0),
            advance: vec2(8, 0),
            rect: rect(0, 0, 1, 1),
        },
    )];
    let fonts = [(
        "default",
        FontEntry {
            line_size: 10,
            baseline: 8,
            font_size: 10,
            entries: &characters,
        },
    )];
    AtlasHandle::from(&AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
    })
}

#[test]
fn downstream_four_role_widget_path_uses_the_runtime_owned_state_cell() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let widget = P1Builder::create_widget(P1Parameters { value: 41, log: log.clone() });
    let state = widget.state_handle();
    let unit_widget = UnitBuilder::create_widget(UnitParameters { log: log.clone() });

    assert_eq!(state.try_read(|state| state.value), Some(41));
    let tree = UiNodeBuilder::build(|tree| {
        tree.widget(widget);
        tree.widget(unit_widget);
    });
    let backend = MarkerBackend {
        atlas: atlas(),
        log: Rc::new(RefCell::new(Vec::new())),
    };
    let mut ctx = Context::new(backend);
    ctx.create_window("p1 widget roles", rect(0, 0, 100, 70), tree);
    let info = FrameInfo::try_new(Dimensioni::new(120, 90), color(0, 0, 0, 0)).unwrap();
    assert_eq!(state.try_update(|state| state.value += 1), Some(()));
    let frame = ctx.frame(info);
    assert_eq!(state.try_read(|state| state.value), Some(42));
    assert_eq!(state.try_update(|state| state.value += 1), Some(()));
    frame.render_ui().unwrap();

    assert_eq!(state.try_read(|state| state.value), Some(44));
    let log = log.borrow();
    assert!(log.contains(&"p1-update"));
    assert!(log.contains(&"p1-paint"));
    assert!(log.contains(&"unit-update"));
    assert!(log.contains(&"unit-paint"));
    drop(log);

    drop(ctx);
    assert_eq!(state.try_read(|_| ()), None);
}

#[test]
fn downstream_owning_node_custom_render_and_disclosure_are_public() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let geometry = Rc::new(RefCell::new(Vec::new()));
    let backend = MarkerBackend { atlas: atlas(), log: log.clone() };
    let mut ctx = Context::new(backend);
    let custom = ctx
        .register_custom_renderer({
            let log = log.clone();
            let geometry = geometry.clone();
            move |frame: &mut MarkerFrame<'_>, args: CustomRenderArgs| {
                frame.mark("custom");
                assert!(Rc::ptr_eq(&log, frame.log));
                geometry.borrow_mut().push((
                    (args.dimensions.width, args.dimensions.height),
                    (args.content_area.x, args.content_area.y, args.content_area.width, args.content_area.height),
                    (args.view.x, args.view.y, args.view.width, args.view.height),
                ));
            }
        })
        .unwrap();

    let widget = P1Builder::create_widget(P1Parameters { value: 1, log: log.clone() });
    let widget_state = widget.state_handle();
    widget_state.try_update(|state| state.value = 2).unwrap();

    let (disclosure_state, disclosure_node) = Disclosure::create(DisclosureParameters::header("section", false, std::iter::empty()));
    assert_eq!(disclosure_state.try_read(DisclosureState::is_collapsed), Some(true));
    drop(disclosure_node);
    assert!(!disclosure_state.is_alive());

    // The finished owning node enters a root directly; no raw runtime box or projection adapter is
    // involved in the public custom-render path.
    let node = Node::custom_render(widget, custom);
    ctx.create_window("downstream", rect(0, 0, 100, 70), node.into());
    let info = FrameInfo::try_new(Dimensioni::new(120, 90), color(0, 0, 0, 0)).unwrap();
    ctx.frame(info).render_ui().unwrap();

    assert_eq!(widget_state.try_read(|state| state.value), Some(3));
    let log = log.borrow();
    let update = log.iter().position(|event| *event == "p1-update").unwrap();
    let paint = log.iter().position(|event| *event == "p1-paint").unwrap();
    let custom = log.iter().position(|event| *event == "custom").unwrap();
    assert!(update < paint && paint < custom, "unexpected downstream phase order: {log:?}");
    let geometry = geometry.borrow();
    let [(dimensions, content, view)] = geometry.as_slice() else {
        panic!("expected one custom-render geometry record, got {geometry:?}");
    };
    assert_eq!(*dimensions, (120, 90));
    assert_eq!(content, view);
    assert!(content.0 >= 0 && content.1 >= 0 && content.2 > 0 && content.3 == 12);
    assert!(content.0 + content.2 <= dimensions.0 && content.1 + content.3 <= dimensions.1);
}

#[test]
fn downstream_container_uses_only_the_public_state_owned_child_contract() {
    let (column_state, empty_column) = Column::create(ColumnParameters::default());
    assert_eq!(column_state.try_read(ColumnState::is_empty), Some(true));
    drop(empty_column);
    assert!(!column_state.is_alive());

    let log = Rc::new(RefCell::new(Vec::new()));
    let child = P1Builder::create_widget(P1Parameters { value: 7, log: log.clone() });
    let child_state = child.state_handle();
    let children: Children = [Node::widget(child)].into_iter().collect();

    let container = ExternalContainerBuilder::create_container(ExternalContainerParameters { children });
    let container_state = container.state_handle();
    let node = Node::container(container).with_policy(Policy::fixed(80, 24));

    let backend = MarkerBackend {
        atlas: atlas(),
        log: Rc::new(RefCell::new(Vec::new())),
    };
    let mut ctx = Context::new(backend);
    ctx.create_window("custom container", rect(0, 0, 100, 70), node.into());
    let info = FrameInfo::try_new(Dimensioni::new(120, 90), color(0, 0, 0, 0)).unwrap();
    ctx.frame(info).render_ui().unwrap();

    let (measure_calls, layout_calls, child_count) = container_state
        .try_read(|state| (state.measure_calls, state.layout_calls, state.children.len()))
        .expect("custom container state must remain owned by its runtime");
    assert!(measure_calls > 0);
    assert!(layout_calls > 0);
    assert_eq!(child_count, 1);
    assert_eq!(child_state.try_read(|state| state.value), Some(8));

    drop(ctx);
    assert!(!container_state.is_alive());
    assert!(!child_state.is_alive());
}

#[test]
fn downstream_grid_owns_placement_without_polluting_node_or_container_context() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let child = P1Builder::create_widget(P1Parameters { value: 10, log });
    let child_state = child.state_handle();
    let item = GridItem::spanned(Node::widget(child), 2, 0);
    assert_eq!(item.span(), GridSpan::new(2, 1));

    let (grid_state, grid_node) = Grid::create(GridParameters::new(
        [SizePolicy::Fixed(30), SizePolicy::Fixed(40)],
        [SizePolicy::Fixed(16)],
        [item],
    ));
    assert_eq!(grid_state.try_read(GridState::len), Some(1));
    assert_eq!(grid_state.try_read(|state| state.span(0)), Some(Some(GridSpan::new(2, 1))));
    assert_eq!(grid_state.try_update(|state| state.set_span(0, GridSpan::ONE)), Some(true));

    let backend = MarkerBackend {
        atlas: atlas(),
        log: Rc::new(RefCell::new(Vec::new())),
    };
    let mut ctx = Context::new(backend);
    ctx.create_window("public Grid", rect(0, 0, 100, 70), grid_node.with_policy(Policy::fixed(74, 16)).into());
    let info = FrameInfo::try_new(Dimensioni::new(120, 90), color(0, 0, 0, 0)).unwrap();
    ctx.frame(info).render_ui().unwrap();

    assert_eq!(child_state.try_read(|state| state.value), Some(11));
    assert_eq!(grid_state.try_read(|state| state.span(0)), Some(Some(GridSpan::ONE)));
    drop(ctx);
    assert!(!grid_state.is_alive());
    assert!(!child_state.is_alive());
}
