//! Characterization tests for single-pass Renderer execution and resource ownership.

use super::*;
use crate::render::{
    DisplayList, Painter,
    geometry::{SolidTriangle, SolidVertex},
};
use crate::test_support::{RecordedVertex, RenderEvent, recording_backend};
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

impl RendererBackend for CountingRenderer {
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

impl RendererBackend for TextureUploadRenderer {
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
    let backend = BackendHandle::new(CountingRenderer {
        atlas: make_atlas(),
        atlas_reads: Cell::new(0),
        quads: 0,
    });
    let mut renderer = Renderer::new(backend.clone(), Dimensioni::new(32, 32));
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

    renderer.render(&mut list);

    assert!(list.is_empty());
    backend.scope(|backend| {
        assert_eq!(backend.atlas_reads.get(), 1);
        assert_eq!(backend.quads, 6);
    });
}

#[test]
fn backend_write_locks_scale_with_custom_barriers_not_normal_operations() {
    let backend = BackendHandle::new(CountingRenderer {
        atlas: make_atlas(),
        atlas_reads: Cell::new(0),
        quads: 0,
    });
    let mut renderer = Renderer::new(backend.clone(), Dimensioni::new(32, 32));
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();

    {
        let mut painter = painter(&mut list, viewport());
        for index in 0..4_096 {
            painter.fill_rect(Recti::new(index % 32, (index / 32) % 32, 1, 1), white);
        }
    }
    let before_normal = backend.debug_write_acquisition_count();
    renderer.render(&mut list);
    assert_eq!(backend.debug_write_acquisition_count() - before_normal, 1);

    for segment in 0..=3 {
        {
            let mut painter = painter(&mut list, viewport());
            for index in 0..1_024 {
                painter.fill_rect(Recti::new(index % 32, (index / 32) % 32, 1, 1), white);
            }
        }
        if segment < 3 {
            list.push_custom(
                viewport(),
                CustomRenderArgs {
                    content_area: viewport(),
                    view: viewport(),
                },
                Box::new(|_: Dimensioni, _: &CustomRenderArgs| {}),
            );
        }
    }

    let before_barriers = backend.debug_write_acquisition_count();
    renderer.render(&mut list);

    // Four normal segments use four locks. Each of the three barriers uses one lock for the
    // pre-callback flush and one for the post-callback flush: 4 + 2 * 3 = 10.
    assert_eq!(backend.debug_write_acquisition_count() - before_barriers, 10);
}

#[test]
fn renderer_and_display_list_reuse_text_and_clipping_scratch_after_execution() {
    let backend = BackendHandle::new(CountingRenderer {
        atlas: make_atlas(),
        atlas_reads: Cell::new(0),
        quads: 0,
    });
    let mut renderer = Renderer::new(backend, Dimensioni::new(32, 32));
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    let long_text = "a".repeat(256);

    painter(&mut list, viewport()).text(FontId::default(), &long_text, Vec2i::new(0, 0), white);
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(-8.0, -8.0), Vec2f::new(40.0, 0.0), Vec2f::new(0.0, 40.0)], white);
    renderer.render(&mut list);

    let operation_capacity = list.debug_operation_capacity();
    let triangle_capacity = list.debug_triangle_capacity();
    let glyph_capacity = renderer.rect_batch.capacity();
    let clipped_capacity = renderer.clipped_triangles.capacity();
    assert!(operation_capacity >= 2);
    assert!(triangle_capacity >= 1);
    assert!(glyph_capacity >= long_text.len());
    assert!(clipped_capacity >= 3);

    painter(&mut list, viewport()).text(FontId::default(), "a", Vec2i::new(0, 0), white);
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(1.0, 1.0), Vec2f::new(2.0, 1.0), Vec2f::new(1.0, 2.0)], white);
    renderer.render(&mut list);

    assert_eq!(list.debug_operation_capacity(), operation_capacity);
    assert_eq!(list.debug_triangle_capacity(), triangle_capacity);
    assert_eq!(renderer.rect_batch.capacity(), glyph_capacity);
    assert_eq!(renderer.clipped_triangles.capacity(), clipped_capacity);
}

#[test]
fn operation_clip_is_intersected_with_viewport_for_every_quad_kind() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend, Dimensioni::new(8, 8));
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

    renderer.render(&mut list);

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
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend, Dimensioni::new(32, 32));
    let texture = renderer.try_load_texture_rgba(20, 20, &[0xFF; 20 * 20 * 4]).unwrap();
    log.clear();
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).fill_rect(Recti::new(0, 0, 1, 1), white);
    painter(&mut list, Recti::new(5, 5, 10, 10)).image(Image::Texture(texture), Recti::new(0, 0, 20, 20), white);
    painter(&mut list, viewport()).fill_rect(Recti::new(20, 0, 1, 1), white);

    renderer.render(&mut list);

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
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend, Dimensioni::new(20, 20));
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

    renderer.render(&mut list);

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
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend.clone(), Dimensioni::new(20, 20));
    let observed = Rc::new(RefCell::new(Vec::new()));
    let callback_observed = observed.clone();
    let mut callback_backend = backend.clone();
    let callback = move |dim: Dimensioni, args: &CustomRenderArgs| {
        callback_observed
            .borrow_mut()
            .push(((dim.width, dim.height), (args.view.x, args.view.y, args.view.width, args.view.height)));
        callback_backend.scope_mut(|backend| backend.record_marker("custom"));
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

    renderer.render(&mut list);

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
    let (backend, log) = recording_backend(atlas.clone());
    let mut renderer = Renderer::new(backend, Dimensioni::new(20, 20));
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

    renderer.render(&mut list);

    assert_eq!(atlas.get_last_update_id(), update_before.wrapping_add(1));
    assert!(called.get());
    let events = log.snapshot();
    assert_eq!(events[0], RenderEvent::Marker(String::from("slot-payload")));
    assert!(matches!(events[1], RenderEvent::AtlasQuad(_)));
}

#[test]
fn texture_upload_validation_and_backend_failure_do_not_consume_ids() {
    let mut backend = BackendHandle::new(TextureUploadRenderer {
        atlas: make_atlas(),
        create_calls: 0,
        destroy_calls: 0,
        fail_upload: false,
    });
    let mut renderer = Renderer::new(backend.clone(), Dimensioni::new(16, 16));
    let error = renderer.try_load_texture_rgba(2, 2, &[0xFF; 4]).unwrap_err();
    assert_eq!(error, "Expected 16 RGBA bytes, received 4");
    assert_eq!(renderer.next_texture_id, 1);
    assert!(renderer.textures.is_empty());
    backend.scope(|backend| assert_eq!(backend.create_calls, 0));

    backend.scope_mut(|backend| backend.fail_upload = true);
    let error = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap_err();
    assert_eq!(error, "backend rejected texture");
    assert_eq!(renderer.next_texture_id, 1);
    assert!(renderer.textures.is_empty());
    backend.scope(|backend| assert_eq!(backend.create_calls, 1));
}

#[test]
fn unknown_and_freed_textures_are_noops_and_drop_destroys_owned_textures_once() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend, Dimensioni::new(32, 32));
    let first = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    let second = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    let third = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    log.clear();
    renderer.free_texture(first);
    renderer.free_texture(first);
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).image(Image::Texture(first), Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    painter(&mut list, viewport()).image(Image::Texture(TextureId::new(999, 1, 1)), Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    renderer.render(&mut list);
    drop(renderer);

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
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend, Dimensioni::new(1, 1));

    renderer.begin(20, 10, color(1, 2, 3, 4));
    renderer.flush();
    renderer.end();

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
