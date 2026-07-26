//! Characterization tests for single-pass Renderer execution and resource ownership.

use super::*;
use crate::render::{
    DisplayList, FrameInfoError, Painter,
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
    stats: Rc<CountingStats>,
}

#[derive(Default)]
struct CountingStats {
    atlas_reads: Cell<usize>,
    quads: Cell<usize>,
    texture_quads: Cell<usize>,
    frames: Cell<usize>,
    drops: Cell<usize>,
}

#[must_use]
struct CountingFrame<'a> {
    backend: &'a mut CountingRenderer,
}

impl RendererFrame for CountingFrame<'_> {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {
        self.backend.stats.quads.set(self.backend.stats.quads.get() + 1);
    }

    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}

    fn flush(&mut self) {}

    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {
        self.backend.stats.texture_quads.set(self.backend.stats.texture_quads.get() + 1);
    }
}

impl Drop for CountingFrame<'_> {
    fn drop(&mut self) {
        self.backend.stats.drops.set(self.backend.stats.drops.get() + 1);
    }
}

impl RendererBackend for CountingRenderer {
    type Frame<'a> = CountingFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.stats.atlas_reads.set(self.stats.atlas_reads.get() + 1);
        self.atlas.clone()
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        self.stats.frames.set(self.stats.frames.get() + 1);
        Ok(CountingFrame { backend: self })
    }

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

struct TextureUploadRenderer {
    atlas: AtlasHandle,
    create_calls: Rc<Cell<usize>>,
    destroy_calls: Rc<Cell<usize>>,
    fail_upload: Rc<Cell<bool>>,
}

#[must_use]
struct EmptyFrame;

impl RendererFrame for EmptyFrame {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {}
    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}
    fn flush(&mut self) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

impl RendererBackend for TextureUploadRenderer {
    type Frame<'a> = EmptyFrame;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(EmptyFrame)
    }

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        self.create_calls.set(self.create_calls.get() + 1);
        if self.fail_upload.get() {
            Err(String::from("backend rejected texture"))
        } else {
            Ok(())
        }
    }

    fn destroy_texture(&mut self, _id: TextureId) {
        self.destroy_calls.set(self.destroy_calls.get() + 1);
    }
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
    AtlasHandle::from(&AtlasSource {
        width: 8,
        height: 8,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
    })
}

fn viewport() -> Recti {
    Recti::new(0, 0, 32, 32)
}

fn frame_info(width: i32, height: i32) -> FrameInfo {
    FrameInfo::try_new(Dimensioni::new(width, height), color(0, 0, 0, 0)).unwrap()
}

#[test]
fn frame_info_rejects_every_non_positive_dimension() {
    for dimensions in [
        Dimensioni::new(0, 1),
        Dimensioni::new(1, 0),
        Dimensioni::new(-1, 1),
        Dimensioni::new(1, -1),
        Dimensioni::new(-1, -1),
    ] {
        assert!(matches!(
            FrameInfo::try_new(dimensions, color(1, 2, 3, 4)),
            Err(FrameInfoError::NonPositiveDimensions(rejected))
                if (rejected.width, rejected.height) == (dimensions.width, dimensions.height)
        ));
    }
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
    let stats = Rc::new(CountingStats::default());
    let backend = CountingRenderer {
        atlas: make_atlas(),
        stats: stats.clone(),
    };
    let mut renderer = Renderer::new(backend);
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    {
        let mut painter = painter(&mut list, viewport());
        painter.fill_rect(Recti::new(0, 0, 1, 1), white);
        painter.text(FontId::default(), "aa", Vec2i::new(0, 0), white);
        painter.icon(WHITE_ICON, Recti::new(0, 0, 3, 3), white);
    }

    renderer.render(frame_info(32, 32), &mut list).unwrap();

    assert!(list.is_empty());
    assert_eq!(stats.atlas_reads.get(), 1);
    assert_eq!(stats.quads.get(), 4);
    assert_eq!(stats.texture_quads.get(), 0);
    assert_eq!(stats.frames.get(), 1);
    assert_eq!(stats.drops.get(), 1);
}

#[test]
fn one_backend_frame_owns_normal_operations_and_custom_barriers() {
    let stats = Rc::new(CountingStats::default());
    let backend = CountingRenderer {
        atlas: make_atlas(),
        stats: stats.clone(),
    };
    let mut renderer = Renderer::new(backend);
    let custom_calls = Rc::new(Cell::new(0));
    let callback_calls = custom_calls.clone();
    let custom_renderer = renderer
        .register_custom_renderer(move |_frame, _args| callback_calls.set(callback_calls.get() + 1))
        .unwrap();
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();

    {
        let mut painter = painter(&mut list, viewport());
        for index in 0..4_096 {
            painter.fill_rect(Recti::new(index % 32, (index / 32) % 32, 1, 1), white);
        }
    }
    renderer.render(frame_info(32, 32), &mut list).unwrap();
    assert_eq!(stats.frames.get(), 1);
    assert_eq!(stats.drops.get(), 1);

    for segment in 0..=3 {
        {
            let mut painter = painter(&mut list, viewport());
            for index in 0..1_024 {
                painter.fill_rect(Recti::new(index % 32, (index / 32) % 32, 1, 1), white);
            }
        }
        if segment < 3 {
            list.push_custom(viewport(), custom_renderer.key, viewport());
        }
    }

    renderer.render(frame_info(32, 32), &mut list).unwrap();
    assert_eq!(stats.frames.get(), 2);
    assert_eq!(stats.drops.get(), 2);
    assert_eq!(custom_calls.get(), 3);
}

#[test]
fn renderer_and_display_list_reuse_text_and_clipping_scratch_after_execution() {
    let backend = CountingRenderer {
        atlas: make_atlas(),
        stats: Rc::new(CountingStats::default()),
    };
    let mut renderer = Renderer::new(backend);
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    let long_text = "a".repeat(256);

    painter(&mut list, viewport()).text(FontId::default(), &long_text, Vec2i::new(0, 0), white);
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(-8.0, -8.0), Vec2f::new(40.0, 0.0), Vec2f::new(0.0, 40.0)], white);
    renderer.render(frame_info(32, 32), &mut list).unwrap();

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
    renderer.render(frame_info(32, 32), &mut list).unwrap();

    assert_eq!(list.debug_operation_capacity(), operation_capacity);
    assert_eq!(list.debug_triangle_capacity(), triangle_capacity);
    assert_eq!(renderer.rect_batch.capacity(), glyph_capacity);
    assert_eq!(renderer.clipped_triangles.capacity(), clipped_capacity);
}

#[test]
fn operation_clip_is_intersected_with_viewport_for_every_quad_kind() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let white = color(255, 255, 255, 255);
    let clip = Recti::new(2, 0, 20, 4);
    let mut list = DisplayList::new();
    {
        let mut painter = Painter::new(&mut list, Vec2i::new(0, 0), viewport(), clip);
        painter.fill_rect(Recti::new(0, 0, 10, 4), white);
        painter.text(FontId::default(), "a", Vec2i::new(0, 0), white);
        painter.icon(CLOSE_ICON, Recti::new(0, 0, 4, 4), white);
    }

    renderer.render(frame_info(8, 8), &mut list).unwrap();

    let quads: Vec<_> = log
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            RenderEvent::AtlasQuad(vertices) => Some(vertices),
            _ => None,
        })
        .collect();
    assert_eq!(quads.len(), 3);
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
    let mut renderer = Renderer::new(backend);
    let texture = renderer.try_load_texture_rgba(20, 20, &[0xFF; 20 * 20 * 4]).unwrap();
    log.clear();
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).fill_rect(Recti::new(0, 0, 1, 1), white);
    painter(&mut list, Recti::new(5, 5, 10, 10)).image(texture, Recti::new(0, 0, 20, 20), white);
    painter(&mut list, viewport()).fill_rect(Recti::new(20, 0, 1, 1), white);

    renderer.render(frame_info(32, 32), &mut list).unwrap();

    let events = log.snapshot();
    assert!(matches!(events[0], RenderEvent::Begin { .. }));
    assert!(matches!(events[1], RenderEvent::AtlasQuad(_)));
    assert_eq!(events[2], RenderEvent::Flush);
    let RenderEvent::ExternalTexture { id, vertices } = &events[3] else {
        panic!("external texture must remain between atlas operations");
    };
    assert_eq!(*id, texture);
    assert_position(vertices[0], [5.0, 5.0]);
    assert_position(vertices[2], [15.0, 15.0]);
    assert_uv(vertices[0], [0.25, 0.25]);
    assert_uv(vertices[2], [0.75, 0.75]);
    assert!(matches!(events[4], RenderEvent::AtlasQuad(_)));
}

#[test]
fn solid_triangles_are_clipped_only_during_execution_and_interpolate_color() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
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

    renderer.render(frame_info(20, 20), &mut list).unwrap();

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
    let mut renderer = Renderer::new(backend);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let callback_observed = observed.clone();
    let custom_renderer = renderer
        .register_custom_renderer(move |frame: &mut crate::test_support::RecordingFrame<'_>, args: CustomRenderArgs| {
            callback_observed.borrow_mut().push((
                (args.dimensions.width, args.dimensions.height),
                (args.view.x, args.view.y, args.view.width, args.view.height),
            ));
            frame.record_marker("custom");
        })
        .unwrap();
    let mut list = DisplayList::new();
    list.push_fill_rect(viewport(), Recti::new(0, 0, 4, 4), color(255, 0, 0, 255));
    list.push_custom(Recti::new(10, 10, 20, 20), custom_renderer.key, Recti::new(0, 0, 40, 40));
    list.push_fill_rect(viewport(), Recti::new(4, 0, 4, 4), color(0, 0, 255, 255));

    renderer.render(frame_info(20, 20), &mut list).unwrap();

    assert_eq!(*observed.borrow(), vec![((20, 20), (10, 10, 10, 10))]);
    let events = log.snapshot();
    assert_eq!(events.len(), 7);
    assert!(matches!(events[0], RenderEvent::Begin { .. }));
    assert!(matches!(events[1], RenderEvent::AtlasQuad(_)));
    assert_eq!(events[2], RenderEvent::Flush);
    assert_eq!(events[3], RenderEvent::Marker(String::from("custom")));
    assert!(matches!(events[4], RenderEvent::AtlasQuad(_)));
    assert_eq!(events[5], RenderEvent::Flush);
    assert_eq!(events[6], RenderEvent::End);
}

#[test]
fn texture_upload_validation_and_backend_failure_do_not_consume_ids() {
    let create_calls = Rc::new(Cell::new(0));
    let destroy_calls = Rc::new(Cell::new(0));
    let fail_upload = Rc::new(Cell::new(false));
    let backend = TextureUploadRenderer {
        atlas: make_atlas(),
        create_calls: create_calls.clone(),
        destroy_calls,
        fail_upload: fail_upload.clone(),
    };
    let mut renderer = Renderer::new(backend);
    let error = renderer.try_load_texture_rgba(2, 2, &[0xFF; 4]).unwrap_err();
    assert_eq!(error, "Expected 16 RGBA bytes, received 4");
    assert_eq!(renderer.next_texture_id, 1);
    assert!(renderer.textures.is_empty());
    assert_eq!(create_calls.get(), 0);

    fail_upload.set(true);
    let error = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap_err();
    assert_eq!(error, "backend rejected texture");
    assert_eq!(renderer.next_texture_id, 1);
    assert!(renderer.textures.is_empty());
    assert_eq!(create_calls.get(), 1);
}

#[test]
fn unknown_and_freed_textures_fail_preflight_and_drop_destroys_owned_textures_once() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let first = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    let second = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    let third = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    log.clear();
    renderer.free_texture(first);
    renderer.free_texture(first);
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).image(first, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    painter(&mut list, viewport()).image(TextureId::new(999, 1, 1), Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    let error = renderer.render(frame_info(32, 32), &mut list).unwrap_err();
    assert!(matches!(error, RenderError::UnknownTexture { id, operation_index: 0 } if id == first));
    assert!(list.is_empty());
    assert!(!log.snapshot().iter().any(|event| matches!(event, RenderEvent::Begin { .. })));
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
    let mut renderer = Renderer::new(backend);
    let mut list = DisplayList::new();
    let info = FrameInfo::try_new(Dimensioni::new(20, 10), color(1, 2, 3, 4)).unwrap();
    renderer.render(info, &mut list).unwrap();

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

#[test]
fn frame_acquisition_failure_discards_the_list_without_finalization() {
    struct FailingBackend {
        atlas: AtlasHandle,
        attempts: Rc<Cell<usize>>,
    }

    impl RendererBackend for FailingBackend {
        type Frame<'a> = EmptyFrame;

        fn get_atlas(&self) -> AtlasHandle {
            self.atlas.clone()
        }

        fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
            self.attempts.set(self.attempts.get() + 1);
            Err(FrameError::new("acquire failed"))
        }

        fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
            Ok(())
        }

        fn destroy_texture(&mut self, _id: TextureId) {}
    }

    let atlas = make_atlas();
    let attempts = Rc::new(Cell::new(0));
    let mut renderer = Renderer::new(FailingBackend {
        atlas: atlas.clone(),
        attempts: attempts.clone(),
    });
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).fill_rect(Recti::new(0, 0, 4, 4), color(255, 255, 255, 255));

    assert_eq!(
        renderer.render(frame_info(32, 32), &mut list),
        Err(RenderError::Frame(FrameError::new("acquire failed")))
    );
    assert_eq!(attempts.get(), 1);
    assert!(list.is_empty());
}

#[test]
fn removed_custom_renderer_fails_preflight_before_backend_acquisition() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let callback = renderer.register_custom_renderer(|_frame, _args| {}).unwrap();
    let mut list = DisplayList::new();
    list.push_custom(viewport(), callback.key, viewport());
    renderer.unregister_custom_renderer(callback).unwrap();

    assert_eq!(
        renderer.render(frame_info(32, 32), &mut list),
        Err(RenderError::UnknownCustomRenderer { operation_index: 0 })
    );
    assert!(list.is_empty());
    assert!(log.snapshot().is_empty());
}

#[test]
fn panicking_custom_callback_still_drops_the_backend_frame_without_double_panicking() {
    let atlas = make_atlas();
    let (backend, log) = recording_backend(atlas.clone());
    let mut renderer = Renderer::new(backend);
    let callback = renderer.register_custom_renderer(|_frame, _args| panic!("custom render panic")).unwrap();
    let mut list = DisplayList::new();
    list.push_custom(viewport(), callback.key, viewport());

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = renderer.render(frame_info(32, 32), &mut list);
    }));
    assert!(panic.is_err());
    assert!(list.is_empty());
    assert!(log.snapshot().iter().any(|event| matches!(event, RenderEvent::End)));
}
