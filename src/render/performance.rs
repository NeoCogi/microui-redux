//! Reproducible render microbenchmark and allocation instrumentation.
//!
//! The benchmark is ignored during the normal test suite. Run it serially in release mode:
//!
//! `cargo test --release render_performance_baseline -- --ignored --nocapture --test-threads=1`

use super::{BackendHandle, CustomRenderArgs, DisplayList, Painter, Renderer, RendererBackend, Vertex};
use crate::{AtlasSource, CharEntry, FontEntry, FontId, Image, SourceFormat, TextureId, color};
use rs_math3d::{Dimensioni, Recti, Vec2f, Vec2i};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

const VIEW_SIZE: i32 = 4_096;
const RECTANGLE_COUNT: usize = 4_096;
const GLYPH_COUNT: usize = 4_096;
const MIXED_PAIR_COUNT: usize = 2_048;
const NESTED_CLIP_DEPTH: usize = 32;
const EXTERNAL_TEXTURE_PAIR_COUNT: usize = 2_048;
const CUSTOM_BARRIER_COUNT: usize = 8;
const ITERATIONS: u64 = 200;

/// Allocator wrapper enabled only around one benchmark measurement window.
struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

static MEASURE_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATION_EVENTS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        count_allocation(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        count_allocation(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, new_size) };
        count_allocation(pointer, new_size);
        pointer
    }
}

/// Counts one successful allocation or reallocation during an active measurement.
fn count_allocation(pointer: *mut u8, bytes: usize) {
    if !pointer.is_null() && MEASURE_ALLOCATIONS.load(Ordering::Relaxed) {
        ALLOCATION_EVENTS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

/// Starts an isolated allocation measurement window.
fn begin_allocation_measurement() {
    ALLOCATION_EVENTS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    MEASURE_ALLOCATIONS.store(true, Ordering::Release);
}

/// Ends the current allocation measurement window.
fn end_allocation_measurement() -> AllocationCount {
    MEASURE_ALLOCATIONS.store(false, Ordering::Release);
    AllocationCount {
        events: ALLOCATION_EVENTS.load(Ordering::Relaxed),
        bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

/// Aggregate allocation activity observed by the counting allocator.
#[derive(Clone, Copy)]
struct AllocationCount {
    /// Allocation and reallocation calls.
    events: u64,
    /// Bytes requested by those calls.
    bytes: u64,
}

/// Backend submission counters for one scenario.
#[derive(Clone, Copy, Default)]
struct SubmissionCount {
    /// Atlas-backed quads.
    quads: u64,
    /// Solid triangles.
    triangles: u64,
    /// External texture quads.
    texture_quads: u64,
    /// Explicit flush calls.
    flushes: u64,
}

impl SubmissionCount {
    /// Returns the total final vertices delivered to the backend.
    fn vertices(self) -> u64 {
        self.quads * 4 + self.triangles * 3 + self.texture_quads * 4
    }
}

/// Minimal backend that retains only submission counts.
struct MeasurementBackend {
    /// Atlas used by the renderer.
    atlas: crate::AtlasHandle,
    /// Submission counters.
    submissions: SubmissionCount,
}

impl RendererBackend for MeasurementBackend {
    fn get_atlas(&self) -> crate::AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, _width: i32, _height: i32, _clear: crate::Color) {}

    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {
        self.submissions.quads += 1;
    }

    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {
        self.submissions.triangles += 1;
    }

    fn flush(&mut self) {
        self.submissions.flushes += 1;
    }

    fn end(&mut self) {}

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}

    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {
        self.submissions.texture_quads += 1;
    }
}

/// Representative renderer workload.
#[derive(Clone, Copy)]
enum Scenario {
    /// Thousands of independent semantic rectangles.
    Rectangles,
    /// One text operation expanded into thousands of glyphs.
    GlyphText,
    /// Alternating semantic rectangles and concave solid polygons.
    MixedGeometry,
    /// Rectangles recorded through deeply nested clip scopes.
    NestedClipping,
    /// Alternating atlas and external-texture operations.
    ExternalTextures,
    /// Normal segments separated by several custom callbacks.
    CustomBarriers,
}

impl Scenario {
    /// Stable report label.
    const fn name(self) -> &'static str {
        match self {
            Self::Rectangles => "rectangles",
            Self::GlyphText => "glyph text",
            Self::MixedGeometry => "mixed geometry",
            Self::NestedClipping => "nested clipping",
            Self::ExternalTextures => "external textures",
            Self::CustomBarriers => "custom barriers",
        }
    }

    /// Expected renderer write locks per execution after recording.
    const fn expected_locks(self) -> u64 {
        match self {
            Self::CustomBarriers => (CUSTOM_BARRIER_COUNT as u64) * 3 + 1,
            _ => 1,
        }
    }

    /// Expected warmed allocation events per frame.
    const fn expected_allocations(self) -> u64 {
        match self {
            // DisplayList owns text after recording, so one String snapshot is intentionally
            // allocated per text operation. Glyph count does not affect this allocation count.
            Self::GlyphText => 1,
            _ => 0,
        }
    }
}

/// One frame's opaque recording counts.
#[derive(Clone, Copy)]
struct RecordingCount {
    /// Display-list operations.
    operations: usize,
    /// Typed solid triangles.
    triangles: usize,
}

/// Normalized benchmark result for one scenario.
struct ScenarioResult {
    /// Scenario identifier.
    scenario: Scenario,
    /// Display-list shape.
    recording: RecordingCount,
    /// Average exclusive backend locks per frame.
    locks: u64,
    /// Average allocation events per frame.
    allocations: u64,
    /// Average allocated bytes per frame.
    allocated_bytes: u64,
    /// Average final vertices per frame.
    vertices: u64,
    /// Average elapsed nanoseconds per frame.
    nanoseconds: u128,
}

/// Creates the small atlas shared by all benchmark scenarios.
fn make_atlas() -> crate::AtlasHandle {
    let pixels = [0xFF; 4];
    let icons = [("white", Recti::new(0, 0, 1, 1))];
    let entries = [(
        'a',
        CharEntry {
            offset: Vec2i::new(0, 0),
            advance: Vec2i::new(1, 0),
            rect: Recti::new(0, 0, 1, 1),
        },
    )];
    let fonts = [(
        "default",
        FontEntry {
            line_size: 1,
            baseline: 1,
            font_size: 1,
            entries: &entries,
        },
    )];
    crate::AtlasHandle::from(&AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
        slots: &[],
    })
}

/// Returns the shared recording bounds and viewport.
fn viewport() -> Recti {
    Recti::new(0, 0, VIEW_SIZE, VIEW_SIZE)
}

/// Produces a stable grid rectangle for a scenario item.
fn item_rect(index: usize) -> Recti {
    Recti::new(((index % 64) * 16) as i32, ((index / 64) * 16) as i32, 8, 8)
}

/// Records one representative frame and reports its opaque list shape.
fn record_scenario(scenario: Scenario, list: &mut DisplayList, font: FontId, text: &str, texture: TextureId) -> RecordingCount {
    let clip = viewport();
    let white = color(255, 255, 255, 255);

    match scenario {
        Scenario::Rectangles => {
            let mut painter = Painter::new(list, Vec2i::default(), clip, clip);
            for index in 0..RECTANGLE_COUNT {
                painter.fill_rect(item_rect(index), white);
            }
        }
        Scenario::GlyphText => {
            Painter::new(list, Vec2i::default(), clip, clip).text(font, text, Vec2i::default(), white);
        }
        Scenario::MixedGeometry => {
            let mut painter = Painter::new(list, Vec2i::default(), clip, clip);
            for index in 0..MIXED_PAIR_COUNT {
                let rect = item_rect(index);
                painter.fill_rect(rect, white);
                let x = rect.x as f32;
                let y = rect.y as f32;
                painter.fill_polygon(
                    &[
                        Vec2f::new(x, y),
                        Vec2f::new(x + 8.0, y),
                        Vec2f::new(x + 8.0, y + 8.0),
                        Vec2f::new(x + 4.0, y + 4.0),
                        Vec2f::new(x, y + 8.0),
                    ],
                    white,
                );
            }
        }
        Scenario::NestedClipping => {
            let mut painter = Painter::new(list, Vec2i::default(), clip, clip);
            record_nested_rectangles(&mut painter, NESTED_CLIP_DEPTH, white);
        }
        Scenario::ExternalTextures => {
            let mut painter = Painter::new(list, Vec2i::default(), clip, clip);
            for index in 0..EXTERNAL_TEXTURE_PAIR_COUNT {
                let rect = item_rect(index);
                painter.fill_rect(rect, white);
                painter.image(Image::Texture(texture), rect, white);
            }
        }
        Scenario::CustomBarriers => {
            for segment in 0..=CUSTOM_BARRIER_COUNT {
                let start = segment * RECTANGLE_COUNT / (CUSTOM_BARRIER_COUNT + 1);
                let end = (segment + 1) * RECTANGLE_COUNT / (CUSTOM_BARRIER_COUNT + 1);
                {
                    let mut painter = Painter::new(list, Vec2i::default(), clip, clip);
                    for index in start..end {
                        painter.fill_rect(item_rect(index), white);
                    }
                }
                if segment < CUSTOM_BARRIER_COUNT {
                    list.push_custom(
                        clip,
                        CustomRenderArgs { content_area: clip, view: clip },
                        Box::new(|_: Dimensioni, _: &CustomRenderArgs| {}),
                    );
                }
            }
        }
    }

    RecordingCount {
        operations: list.debug_operation_count(),
        triangles: list.debug_triangle_count(),
    }
}

/// Recursively narrows the painter before recording the nested-clipping workload.
fn record_nested_rectangles(painter: &mut Painter<'_>, depth: usize, color: crate::Color) {
    if depth == 0 {
        for index in 0..RECTANGLE_COUNT {
            painter.fill_rect(item_rect(index), color);
        }
        return;
    }

    let inset = depth as i32;
    painter.with_clip(Recti::new(inset, inset, VIEW_SIZE - inset * 2, VIEW_SIZE - inset * 2), |painter| {
        record_nested_rectangles(painter, depth - 1, color)
    });
}

/// Measures one scenario after warming all retained allocations.
fn measure_scenario(scenario: Scenario, text: &str) -> ScenarioResult {
    let atlas = make_atlas();
    let font = atlas.font_id("default").expect("benchmark atlas must contain its default font");
    let mut backend = BackendHandle::new(MeasurementBackend {
        atlas,
        submissions: SubmissionCount::default(),
    });
    let mut renderer = Renderer::new(backend.clone(), Dimensioni::new(VIEW_SIZE, VIEW_SIZE));
    let texture = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).expect("benchmark texture upload must succeed");
    let mut list = DisplayList::new();

    let recording = record_scenario(scenario, &mut list, font, text, texture);
    renderer.render(&mut list);
    backend.scope_mut(|backend| backend.submissions = SubmissionCount::default());

    let locks_before = backend.debug_write_acquisition_count() as u64;
    let started = Instant::now();
    begin_allocation_measurement();
    for _ in 0..ITERATIONS {
        let current = record_scenario(scenario, &mut list, font, text, texture);
        black_box((current.operations, current.triangles));
        renderer.render(&mut list);
    }
    let allocations = end_allocation_measurement();
    let elapsed = started.elapsed();
    let locks = backend.debug_write_acquisition_count() as u64 - locks_before;
    let submissions = backend.scope(|backend| backend.submissions);
    assert_eq!(locks, scenario.expected_locks() * ITERATIONS);
    assert_eq!(allocations.events, scenario.expected_allocations() * ITERATIONS);

    ScenarioResult {
        scenario,
        recording,
        locks: locks / ITERATIONS,
        allocations: allocations.events / ITERATIONS,
        allocated_bytes: allocations.bytes / ITERATIONS,
        vertices: submissions.vertices() / ITERATIONS,
        nanoseconds: elapsed.as_nanos() / u128::from(ITERATIONS),
    }
}

#[test]
#[ignore = "manual release-mode render performance baseline"]
fn render_performance_baseline() {
    let text = "a".repeat(GLYPH_COUNT);
    let scenarios = [
        Scenario::Rectangles,
        Scenario::GlyphText,
        Scenario::MixedGeometry,
        Scenario::NestedClipping,
        Scenario::ExternalTextures,
        Scenario::CustomBarriers,
    ];
    let results: Vec<_> = scenarios.into_iter().map(|scenario| measure_scenario(scenario, &text)).collect();

    println!("| scenario | ops | triangles | locks | allocs | bytes | vertices | ns/frame |");
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for result in &results {
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            result.scenario.name(),
            result.recording.operations,
            result.recording.triangles,
            result.locks,
            result.allocations,
            result.allocated_bytes,
            result.vertices,
            result.nanoseconds,
        );

        assert_eq!(result.locks, result.scenario.expected_locks());
        assert_eq!(result.allocations, result.scenario.expected_allocations());
    }
}
