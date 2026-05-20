use super::*;
use std::cell::Cell;

struct NoopRenderer;

impl Renderer for NoopRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        unimplemented!()
    }
    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}
    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {}
    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {}
    fn flush(&mut self) {}
    fn end(&mut self) {}
    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn destroy_texture(&mut self, _id: TextureId) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

struct CountingRenderer {
    atlas: AtlasHandle,
    get_atlas_calls: Cell<usize>,
    quad_count: usize,
}

impl Renderer for CountingRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.get_atlas_calls.set(self.get_atlas_calls.get() + 1);
        self.atlas.clone()
    }
    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}
    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {
        self.quad_count += 1;
    }
    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {}
    fn flush(&mut self) {}
    fn end(&mut self) {}
    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn destroy_texture(&mut self, _id: TextureId) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

struct TextureUploadRenderer {
    atlas: AtlasHandle,
    create_calls: usize,
    destroy_calls: usize,
    fail_upload: bool,
}

impl Renderer for TextureUploadRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }
    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}
    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {}
    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {}
    fn flush(&mut self) {}
    fn end(&mut self) {}
    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        self.create_calls += 1;
        if self.fail_upload {
            Err(String::from("backend rejected texture"))
        } else {
            Ok(())
        }
    }
    fn destroy_texture(&mut self, _id: TextureId) {
        self.destroy_calls += 1;
    }
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

struct TextureDrawRenderer {
    atlas: AtlasHandle,
    drawn: Vec<(TextureId, [Vertex; 4])>,
}

impl Renderer for TextureDrawRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }
    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}
    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {}
    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {}
    fn flush(&mut self) {}
    fn end(&mut self) {}
    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn destroy_texture(&mut self, _id: TextureId) {}
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        self.drawn.push((id, vertices));
    }
}

fn make_test_atlas() -> AtlasHandle {
    let pixels: [u8; 64] = [0xFF; 64];
    let icons = [("white", Recti::new(0, 0, 1, 1))];
    let entries = [
        (
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(1, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
        (
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(1, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
    ];
    let fonts = [(
        "default",
        FontEntry {
            line_size: 1,
            baseline: 1,
            font_size: 1,
            entries: &entries,
        },
    )];
    let slots = [Recti::new(1, 0, 1, 1)];
    let source = AtlasSource {
        width: 4,
        height: 4,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
        slots: &slots,
    };
    AtlasHandle::from(&source)
}

fn assert_rect_eq(actual: Recti, expected: Recti) {
    assert_eq!(
        (actual.x, actual.y, actual.width, actual.height),
        (expected.x, expected.y, expected.width, expected.height)
    );
}

fn assert_vec2f_eq(actual: Vec2f, expected: Vec2f) {
    assert!((actual.x - expected.x).abs() < 1.0e-6, "expected x {}, got {}", expected.x, actual.x);
    assert!((actual.y - expected.y).abs() < 1.0e-6, "expected y {}, got {}", expected.y, actual.y);
}

#[test]
fn clip_rect_passthrough() {
    let dst = Recti::new(0, 0, 10, 10);
    let src = Recti::new(5, 5, 10, 10);
    let clip = Recti::new(0, 0, 20, 20);
    let res = Canvas::<NoopRenderer>::clip_rect(dst, src, clip).unwrap();
    assert_rect_eq(res.0, dst);
    assert_rect_eq(res.1, src);
}

#[test]
fn clip_rect_partial() {
    let dst = Recti::new(0, 0, 100, 100);
    let src = Recti::new(0, 0, 50, 50);
    let clip = Recti::new(20, 20, 40, 40);
    let res = Canvas::<NoopRenderer>::clip_rect(dst, src, clip).unwrap();
    assert_rect_eq(res.0, Recti::new(20, 20, 40, 40));
    assert_rect_eq(res.1, Recti::new(10, 10, 20, 20));
}

#[test]
fn clip_rect_none() {
    let dst = Recti::new(0, 0, 10, 10);
    let src = Recti::new(0, 0, 10, 10);
    let clip = Recti::new(50, 50, 10, 10);
    assert!(Canvas::<NoopRenderer>::clip_rect(dst, src, clip).is_none());
}

#[test]
fn atlas_draw_commands_reuse_canvas_cached_atlas() {
    let renderer = RendererHandle::new(CountingRenderer {
        atlas: make_test_atlas(),
        get_atlas_calls: Cell::new(0),
        quad_count: 0,
    });
    let mut canvas = Canvas::from(renderer, Dimensioni::new(16, 16));

    canvas.draw_rect(Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    canvas.draw_chars(FontId::default(), "aa", Vec2i::new(0, 0), color(255, 255, 255, 255));
    canvas.draw_icon(WHITE_ICON, Recti::new(0, 0, 3, 3), color(255, 255, 255, 255));
    canvas.draw_slot(SlotId::default(), Recti::new(0, 0, 3, 3), color(255, 255, 255, 255));
    canvas.draw_slot_with_function(
        SlotId::default(),
        Recti::new(0, 0, 3, 3),
        color(255, 255, 255, 255),
        Rc::new(|_, _| color4b(255, 255, 255, 255)),
    );

    canvas.renderer_handle().scope(|renderer| {
        assert_eq!(renderer.get_atlas_calls.get(), 1);
        assert_eq!(renderer.quad_count, 6);
    });
}

#[test]
fn texture_upload_rejects_invalid_rgba_before_allocating_id() {
    let renderer = RendererHandle::new(TextureUploadRenderer {
        atlas: make_test_atlas(),
        create_calls: 0,
        destroy_calls: 0,
        fail_upload: false,
    });
    let mut canvas = Canvas::from(renderer.clone(), Dimensioni::new(16, 16));

    let err = canvas.try_load_texture_rgba(2, 2, &[0xFF; 4]).unwrap_err();
    assert_eq!(err, "Expected 16 RGBA bytes, received 4");
    assert_eq!(canvas.next_texture_id, 1);
    assert!(canvas.textures.is_empty());
    renderer.scope(|renderer| {
        assert_eq!(renderer.create_calls, 0);
        assert_eq!(renderer.destroy_calls, 0);
    });
}

#[test]
fn texture_upload_rejects_huge_mismatched_buffer_before_backend_call() {
    let renderer = RendererHandle::new(TextureUploadRenderer {
        atlas: make_test_atlas(),
        create_calls: 0,
        destroy_calls: 0,
        fail_upload: false,
    });
    let mut canvas = Canvas::from(renderer.clone(), Dimensioni::new(16, 16));

    let err = canvas.try_load_texture_rgba(i32::MAX, i32::MAX, &[]).unwrap_err();
    assert!(err.contains("RGBA bytes"));
    assert_eq!(canvas.next_texture_id, 1);
    assert!(canvas.textures.is_empty());
    renderer.scope(|renderer| assert_eq!(renderer.create_calls, 0));
}

#[test]
fn texture_upload_rolls_back_when_backend_rejects_texture() {
    let renderer = RendererHandle::new(TextureUploadRenderer {
        atlas: make_test_atlas(),
        create_calls: 0,
        destroy_calls: 0,
        fail_upload: true,
    });
    let mut canvas = Canvas::from(renderer.clone(), Dimensioni::new(16, 16));

    let err = canvas.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap_err();
    assert_eq!(err, "backend rejected texture");
    assert_eq!(canvas.next_texture_id, 1);
    assert!(canvas.textures.is_empty());
    renderer.scope(|renderer| {
        assert_eq!(renderer.create_calls, 1);
        assert_eq!(renderer.destroy_calls, 0);
    });
}

#[test]
fn external_texture_draw_submits_preclipped_vertices_and_uvs() {
    let renderer = RendererHandle::new(TextureDrawRenderer {
        atlas: make_test_atlas(),
        drawn: Vec::new(),
    });
    let mut canvas = Canvas::from(renderer.clone(), Dimensioni::new(32, 32));
    let texture = canvas.try_load_texture_rgba(20, 20, &[0xFF; 20 * 20 * 4]).unwrap();

    canvas.set_clip_rect(Recti::new(5, 5, 10, 10));
    canvas.draw_image(Image::Texture(texture), Recti::new(0, 0, 20, 20), color(255, 255, 255, 255));

    renderer.scope(|renderer| {
        assert_eq!(renderer.drawn.len(), 1);
        let (id, vertices) = &renderer.drawn[0];
        assert_eq!(*id, texture);
        assert_vec2f_eq(vertices[0].position(), Vec2f::new(5.0, 5.0));
        assert_vec2f_eq(vertices[1].position(), Vec2f::new(15.0, 5.0));
        assert_vec2f_eq(vertices[2].position(), Vec2f::new(15.0, 15.0));
        assert_vec2f_eq(vertices[3].position(), Vec2f::new(5.0, 15.0));
        assert_vec2f_eq(vertices[0].tex_coord(), Vec2f::new(0.25, 0.25));
        assert_vec2f_eq(vertices[2].tex_coord(), Vec2f::new(0.75, 0.75));
    });
}
