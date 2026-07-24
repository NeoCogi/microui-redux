//! Characterization tests for single-pass Canvas execution and resource ownership.

use super::*;
use crate::render::{
    DisplayList, Painter,
    geometry::{SolidTriangle, SolidVertex},
};
use crate::test_support::{RecordedVertex, RenderEvent, recording_renderer};
use crate::{AtlasSource, CharEntry, CLOSE_ICON, FontEntry, SourceFormat, color, color4b};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct CountingRenderer {
    atlas: AtlasHandle,
    atlas_reads: Cell<usize>,
    quads: usize,
}

impl Renderer for CountingRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas_reads.set(self.atlas_reads.get() + 1);
        self.atlas.clone()
    }

    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}

    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {
        self.quads += 1;
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

fn make_atlas() -> AtlasHandle {
    let pixels = [0xFF; 8 * 8 * 4];
    let icons = [("white", Recti::new(0, 0, 1, 1)), ("close", Recti::new(4, 0, 4, 4))];
    let entries = [
        (
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(4, 0),
                rect: Recti::new(0, 4, 4, 4),
            },
        ),
        (
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(4, 0),
                rect: Recti::new(0, 4, 4, 4),
            },
        ),
    ];
    let fonts = [(
        "default",
        FontEntry {
            line_size: 4,
            baseline: 4,
            font_size: 4,
            entries: &entries,
        },
    )];
    let slots = [Recti::new(4, 4, 4, 4)];
    AtlasHandle::from(&AtlasSource {
        width: 8,
        height: 8,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
        slots: &slots,
    })
}

fn viewport() -> Recti {
    Recti::new(0, 0, 32, 32)
}

fn painter<'a>(list: &'a mut DisplayList, clip: Recti) -> Painter<'a> {
    Painter::new(list, Vec2i::new(0, 0), viewport(), clip)
}

fn assert_position(vertex: RecordedVertex, expected: [f32; 2]) {
    assert!((vertex.position[0] - expected[0]).abs() < 1.0e-6);
    assert!((vertex.position[1] - expected[1]).abs() < 1.0e-6);
}

fn assert_uv(vertex: RecordedVertex, expected: [f32; 2]) {
    assert!((vertex.tex_coord[0] - expected[0]).abs() < 1.0e-6);
    assert!((vertex.tex_coord[1] - expected[1]).abs() < 1.0e-6);
}

#[test]
fn textured_rectangle_clipping_projects_into_source_coordinates() {
    let (dst, src) = clip_textured_rect(Recti::new(0, 0, 100, 100), Recti::new(0, 0, 50, 50), Recti::new(20, 20, 40, 40)).unwrap();
    assert_eq!((dst.x, dst.y, dst.width, dst.height), (20, 20, 40, 40));
    assert_eq!((src.x, src.y, src.width, src.height), (10, 10, 20, 20));
    assert!(clip_textured_rect(Recti::new(0, 0, 10, 10), Recti::new(0, 0, 10, 10), Recti::new(50, 50, 10, 10)).is_none());
}

#[test]
fn semantic_atlas_operations_use_the_cached_atlas_and_one_executor() {
    let renderer = RendererHandle::new(CountingRenderer {
        atlas: make_atlas(),
        atlas_reads: Cell::new(0),
        quads: 0,
    });
    let mut canvas = Canvas::new(renderer.clone(), Dimensioni::new(32, 32));
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    {
        let mut painter = painter(&mut list, viewport());
        painter.fill_rect(Recti::new(0, 0, 1, 1), white);
        painter.text(FontId::default(), "aa", Vec2i::new(0, 0), white);
        painter.icon(WHITE_ICON, Recti::new(0, 0, 3, 3), white);
        painter.image(Image::Slot(SlotId::default()), Recti::new(0, 0, 4, 4), white);
        painter.redraw_slot(SlotId::default(), Recti::new(0, 0, 4, 4), white, Rc::new(|_, _| color4b(255, 255, 255, 255)));
    }

    canvas.render(&mut list);

    assert!(list.is_empty());
    renderer.scope(|renderer| {
        assert_eq!(renderer.atlas_reads.get(), 1);
        assert_eq!(renderer.quads, 6);
    });
}

#[test]
fn operation_clip_is_intersected_with_viewport_for_every_quad_kind() {
    let (renderer, log) = recording_renderer(make_atlas());
    let mut canvas = Canvas::new(renderer, Dimensioni::new(8, 8));
    let white = color(255, 255, 255, 255);
    let clip = Recti::new(2, 0, 20, 4);
    let mut list = DisplayList::new();
    {
        let mut painter = Painter::new(&mut list, Vec2i::new(0, 0), viewport(), clip);
        painter.fill_rect(Recti::new(0, 0, 10, 4), white);
        painter.text(FontId::default(), "a", Vec2i::new(0, 0), white);
        painter.icon(CLOSE_ICON, Recti::new(0, 0, 4, 4), white);
        painter.image(Image::Slot(SlotId::default()), Recti::new(0, 0, 4, 4), white);
    }

    canvas.render(&mut list);

    let quads: Vec<_> = log
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            RenderEvent::AtlasQuad(vertices) => Some(vertices),
            _ => None,
        })
        .collect();
    assert_eq!(quads.len(), 4);
    for quad in &quads {
        assert!(
            quad.iter()
                .all(|vertex| { vertex.position[0] >= 2.0 && vertex.position[0] <= 8.0 && vertex.position[1] >= 0.0 && vertex.position[1] <= 4.0 })
        );
    }
}

#[test]
fn external_texture_clipping_preserves_uv_mapping_and_stream_order() {
    let (renderer, log) = recording_renderer(make_atlas());
    let mut canvas = Canvas::new(renderer, Dimensioni::new(32, 32));
    let texture = canvas.try_load_texture_rgba(20, 20, &[0xFF; 20 * 20 * 4]).unwrap();
    log.clear();
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).fill_rect(Recti::new(0, 0, 1, 1), white);
    painter(&mut list, Recti::new(5, 5, 10, 10)).image(Image::Texture(texture), Recti::new(0, 0, 20, 20), white);
    painter(&mut list, viewport()).fill_rect(Recti::new(20, 0, 1, 1), white);

    canvas.render(&mut list);

    let events = log.snapshot();
    assert!(matches!(events[0], RenderEvent::AtlasQuad(_)));
    let RenderEvent::ExternalTexture { id, vertices } = &events[1] else {
        panic!("external texture must remain between atlas operations");
    };
    assert_eq!(*id, texture);
    assert_position(vertices[0], [5.0, 5.0]);
    assert_position(vertices[2], [15.0, 15.0]);
    assert_uv(vertices[0], [0.25, 0.25]);
    assert_uv(vertices[2], [0.75, 0.75]);
    assert!(matches!(events[2], RenderEvent::AtlasQuad(_)));
}

#[test]
fn solid_triangles_are_clipped_only_during_execution_and_interpolate_color() {
    let (renderer, log) = recording_renderer(make_atlas());
    let mut canvas = Canvas::new(renderer, Dimensioni::new(20, 20));
    let triangle = SolidTriangle::from([
        SolidVertex {
            position: Vec2f::new(-10.0, 0.0),
            color: color4b(255, 0, 0, 255),
        },
        SolidVertex {
            position: Vec2f::new(10.0, 0.0),
            color: color4b(0, 0, 255, 255),
        },
        SolidVertex {
            position: Vec2f::new(0.0, 10.0),
            color: color4b(0, 255, 0, 255),
        },
    ]);
    let mut list = DisplayList::new();
    list.push_solid_triangles(Recti::new(0, 0, 10, 10), &[triangle]);

    canvas.render(&mut list);

    let triangles: Vec<_> = log
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            RenderEvent::Triangle(vertices) => Some(vertices),
            _ => None,
        })
        .collect();
    assert_eq!(triangles.len(), 1);
    let vertices: Vec<_> = triangles.into_iter().flatten().collect();
    assert!(
        vertices
            .iter()
            .all(|vertex| { vertex.position[0] >= 0.0 && vertex.position[0] <= 10.0 && vertex.position[1] >= 0.0 && vertex.position[1] <= 10.0 })
    );
    assert!(
        vertices
            .iter()
            .any(|vertex| vertex.position == [0.0, 0.0] && vertex.color == [128, 0, 128, 255])
    );
}

#[test]
fn custom_barrier_flushes_releases_lock_clips_and_preserves_order() {
    let (renderer, log) = recording_renderer(make_atlas());
    let mut canvas = Canvas::new(renderer.clone(), Dimensioni::new(20, 20));
    let observed = Rc::new(RefCell::new(Vec::new()));
    let callback_observed = observed.clone();
    let mut callback_renderer = renderer.clone();
    let callback = move |dim: Dimensioni, args: &CustomRenderArgs| {
        callback_observed
            .borrow_mut()
            .push(((dim.width, dim.height), (args.view.x, args.view.y, args.view.width, args.view.height)));
        callback_renderer.scope_mut(|renderer| renderer.record_marker("custom"));
    };
    let mut list = DisplayList::new();
    list.push_fill_rect(viewport(), Recti::new(0, 0, 4, 4), color(255, 0, 0, 255));
    list.push_custom(
        Recti::new(0, 0, 40, 40),
        CustomRenderArgs {
            content_area: Recti::new(0, 0, 40, 40),
            view: Recti::new(10, 10, 20, 20),
        },
        Box::new(callback),
    );
    list.push_fill_rect(viewport(), Recti::new(4, 0, 4, 4), color(0, 0, 255, 255));

    canvas.render(&mut list);

    assert_eq!(*observed.borrow(), vec![((20, 20), (10, 10, 10, 10))]);
    let events = log.snapshot();
    assert_eq!(events.len(), 5);
    assert!(matches!(events[0], RenderEvent::AtlasQuad(_)));
    assert_eq!(events[1], RenderEvent::Flush);
    assert_eq!(events[2], RenderEvent::Marker(String::from("custom")));
    assert_eq!(events[3], RenderEvent::Flush);
    assert!(matches!(events[4], RenderEvent::AtlasQuad(_)));
}

#[test]
fn dynamic_slot_payload_runs_before_the_slot_quad() {
    let atlas = make_atlas();
    let update_before = atlas.get_last_update_id();
    let (renderer, log) = recording_renderer(atlas.clone());
    let mut canvas = Canvas::new(renderer, Dimensioni::new(20, 20));
    let payload_log = log.clone();
    let called = Rc::new(Cell::new(false));
    let callback_called = called.clone();
    let payload = Rc::new(move |_, _| {
        if !callback_called.replace(true) {
            payload_log.record_marker("slot-payload");
        }
        color4b(7, 8, 9, 255)
    });
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).redraw_slot(SlotId::default(), Recti::new(0, 0, 4, 4), color(255, 255, 255, 255), payload);

    canvas.render(&mut list);

    assert_eq!(atlas.get_last_update_id(), update_before.wrapping_add(1));
    assert!(called.get());
    let events = log.snapshot();
    assert_eq!(events[0], RenderEvent::Marker(String::from("slot-payload")));
    assert!(matches!(events[1], RenderEvent::AtlasQuad(_)));
}

#[test]
fn texture_upload_validation_and_backend_failure_do_not_consume_ids() {
    let mut renderer = RendererHandle::new(TextureUploadRenderer {
        atlas: make_atlas(),
        create_calls: 0,
        destroy_calls: 0,
        fail_upload: false,
    });
    let mut canvas = Canvas::new(renderer.clone(), Dimensioni::new(16, 16));
    let error = canvas.try_load_texture_rgba(2, 2, &[0xFF; 4]).unwrap_err();
    assert_eq!(error, "Expected 16 RGBA bytes, received 4");
    assert_eq!(canvas.next_texture_id, 1);
    assert!(canvas.textures.is_empty());
    renderer.scope(|renderer| assert_eq!(renderer.create_calls, 0));

    renderer.scope_mut(|renderer| renderer.fail_upload = true);
    let error = canvas.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap_err();
    assert_eq!(error, "backend rejected texture");
    assert_eq!(canvas.next_texture_id, 1);
    assert!(canvas.textures.is_empty());
    renderer.scope(|renderer| assert_eq!(renderer.create_calls, 1));
}

#[test]
fn unknown_and_freed_textures_are_noops_and_drop_destroys_owned_textures_once() {
    let (renderer, log) = recording_renderer(make_atlas());
    let mut canvas = Canvas::new(renderer, Dimensioni::new(32, 32));
    let first = canvas.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    let second = canvas.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    let third = canvas.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    log.clear();
    canvas.free_texture(first);
    canvas.free_texture(first);
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).image(Image::Texture(first), Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    painter(&mut list, viewport()).image(Image::Texture(TextureId::new(999, 1, 1)), Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    canvas.render(&mut list);
    drop(canvas);

    let events = log.snapshot();
    assert!(!events.iter().any(|event| matches!(event, RenderEvent::ExternalTexture { .. })));
    let mut destroyed: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            RenderEvent::DestroyTexture(id) => Some(id.raw()),
            _ => None,
        })
        .collect();
    destroyed.sort_unstable();
    assert_eq!(destroyed, vec![first.raw(), second.raw(), third.raw()]);
}

#[test]
fn frame_lifecycle_is_forwarded_in_order() {
    let (renderer, log) = recording_renderer(make_atlas());
    let mut canvas = Canvas::new(renderer, Dimensioni::new(1, 1));

    canvas.begin(20, 10, color(1, 2, 3, 4));
    canvas.flush();
    canvas.end();

    assert_eq!(
        log.snapshot(),
        vec![
            RenderEvent::Begin {
                width: 20,
                height: 10,
                clear: [1, 2, 3, 4],
            },
            RenderEvent::Flush,
            RenderEvent::End,
        ]
    );
}
