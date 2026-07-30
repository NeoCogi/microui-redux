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

struct DownstreamWidget {
    value: usize,
    log: Rc<RefCell<Vec<&'static str>>>,
    opt: WidgetOption,
}

impl Widget for DownstreamWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        Dimensioni::new(32, 16)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        self.log.borrow_mut().push("update");
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.log.borrow_mut().push("paint");
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
fn downstream_widget_custom_render_and_legacy_node_are_public() {
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

    let widget = widget_handle(DownstreamWidget {
        value: 1,
        log: log.clone(),
        opt: WidgetOption::NONE,
    });
    widget.update(|state| state.value = 2);

    let legacy_node: Node = Node::header("legacy", NodeStateValue::Closed);
    assert!(legacy_node.is_header());

    let tree = UiNodeBuilder::build(|tree| {
        tree.custom_render(&widget, custom);
    });
    ctx.create_window("downstream", rect(0, 0, 100, 70), tree);
    let info = FrameInfo::try_new(Dimensioni::new(120, 90), color(0, 0, 0, 0)).unwrap();
    ctx.frame(info).render_ui().unwrap();

    assert_eq!(widget.read(|state| state.value), 2);
    let log = log.borrow();
    let update = log.iter().position(|event| *event == "update").unwrap();
    let paint = log.iter().position(|event| *event == "paint").unwrap();
    let custom = log.iter().position(|event| *event == "custom").unwrap();
    assert!(update < paint && paint < custom, "unexpected downstream phase order: {log:?}");
    let geometry = geometry.borrow();
    let [(dimensions, content, view)] = geometry.as_slice() else {
        panic!("expected one custom-render geometry record, got {geometry:?}");
    };
    assert_eq!(*dimensions, (120, 90));
    assert_eq!(content, view);
    assert!(content.0 >= 0 && content.1 >= 0 && content.2 > 0 && content.3 == 16);
    assert!(content.0 + content.2 <= dimensions.0 && content.1 + content.3 <= dimensions.1);
}
