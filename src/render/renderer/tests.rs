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

//! Characterization tests for single-pass Renderer execution and resource ownership.

use super::*;
use crate::render::{
    DisplayList, FrameInfoError, Painter,
    geometry::{SolidTriangle, SolidVertex},
};
use crate::test_support::{RecordedVertex, RenderEvent, recording_backend};
use crate::{AtlasSource, CharEntry, FontEntry, SourceFormat, color, color4b};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
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

    fn create_texture(&mut self, _id: TextureId, _pixels: &[u8]) -> Result<(), String> {
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

    fn create_texture(&mut self, _id: TextureId, _pixels: &[u8]) -> Result<(), String> {
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
                rect: Recti::new(4, 4, 4, 4),
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
        "body",
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

fn list_capacities(list: &DisplayList) -> (usize, usize, usize) {
    (list.debug_operation_capacity(), list.debug_triangle_capacity(), list.debug_polygon_capacity())
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
    Painter::screen_space(list, clip)
}

fn assert_position(vertex: RecordedVertex, expected: [f32; 2]) {
    assert!((vertex.position[0] - expected[0]).abs() < 1.0e-6);
    assert!((vertex.position[1] - expected[1]).abs() < 1.0e-6);
}

fn assert_uv(vertex: RecordedVertex, expected: [f32; 2]) {
    assert!((vertex.tex_coord[0] - expected[0]).abs() < 1.0e-6);
    assert!((vertex.tex_coord[1] - expected[1]).abs() < 1.0e-6);
}

fn assert_vertex(vertex: Vertex, expected_position: [f32; 2], expected_uv: [f32; 2]) {
    let position = vertex.position();
    let uv = vertex.tex_coord();
    assert!((position.x - expected_position[0]).abs() < 1.0e-6);
    assert!((position.y - expected_position[1]).abs() < 1.0e-6);
    assert!((uv.x - expected_uv[0]).abs() < 1.0e-6);
    assert!((uv.y - expected_uv[1]).abs() < 1.0e-6);
}

#[test]
fn one_texel_scaled_and_clipped_preserves_fractional_uvs() {
    let vertices = clipped_textured_quad(
        Recti::new(0, 0, 100, 100),
        Recti::new(0, 0, 1, 1),
        Dimensioni::new(1, 1),
        color(255, 255, 255, 255),
        Recti::new(20, 20, 40, 40),
    )
    .unwrap();

    assert_vertex(vertices[0], [20.0, 20.0], [0.2, 0.2]);
    assert_vertex(vertices[2], [60.0, 60.0], [0.6, 0.6]);
}

#[test]
fn asymmetric_clipping_projects_u_and_v_independently() {
    let vertices = clipped_textured_quad(
        Recti::new(10, 20, 200, 100),
        Recti::new(40, 20, 80, 40),
        Dimensioni::new(200, 100),
        color(255, 255, 255, 255),
        Recti::new(50, 30, 120, 50),
    )
    .unwrap();

    assert_vertex(vertices[0], [50.0, 30.0], [0.28, 0.24]);
    assert_vertex(vertices[2], [170.0, 80.0], [0.52, 0.44]);
}

#[test]
fn textured_quad_clipping_retains_visible_empty_and_disjoint_behavior() {
    let vertices = clipped_textured_quad(
        Recti::new(2, 3, 4, 5),
        Recti::new(2, 4, 4, 5),
        Dimensioni::new(16, 20),
        color(255, 255, 255, 255),
        Recti::new(0, 0, 20, 20),
    )
    .unwrap();
    assert_vertex(vertices[0], [2.0, 3.0], [0.125, 0.2]);
    assert_vertex(vertices[2], [6.0, 8.0], [0.375, 0.45]);

    assert!(
        clipped_textured_quad(
            Recti::new(0, 0, 0, 10),
            Recti::new(0, 0, 10, 10),
            Dimensioni::new(10, 10),
            color(255, 255, 255, 255),
            Recti::new(0, 0, 10, 10),
        )
        .is_none()
    );
    assert!(
        clipped_textured_quad(
            Recti::new(0, 0, 10, 10),
            Recti::new(0, 0, 10, 10),
            Dimensioni::new(10, 10),
            color(255, 255, 255, 255),
            Recti::new(50, 50, 10, 10),
        )
        .is_none()
    );
}

#[test]
fn semantic_atlas_operations_use_the_cached_atlas_and_one_executor() {
    let stats = Rc::new(CountingStats::default());
    let backend = CountingRenderer {
        atlas: make_atlas(),
        stats: stats.clone(),
    };
    let mut renderer = Renderer::new(backend);
    // Resolve both capabilities from the renderer's cached atlas. This makes ownership explicit
    // while retaining the test's original purpose: one cached read and one execution pass.
    let atlas = renderer.atlas();
    let font = atlas.font_id("body").unwrap();
    let white_icon = atlas.white_icon();
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    {
        let mut painter = painter(&mut list, viewport());
        painter.fill_rect(Recti::new(0, 0, 1, 1), white);
        painter.text(font, "aa", Vec2i::new(0, 0), white);
        painter.icon(white_icon, Recti::new(0, 0, 3, 3), white);
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
fn streamed_glyphs_preserve_order_fallback_and_newline_positioning() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    // The selected font must be minted by the exact atlas used to stream these glyphs.
    let font = renderer.atlas().font_id("body").unwrap();
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).text(font, "a?\na", Vec2i::new(10, 10), color(255, 255, 255, 255));

    renderer.render(frame_info(32, 32), &mut list).unwrap();

    let quads: Vec<_> = log
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            RenderEvent::AtlasQuad(vertices) => Some(vertices),
            _ => None,
        })
        .collect();
    assert_eq!(quads.len(), 3);

    assert_position(quads[0][0], [10.0, 10.0]);
    assert_position(quads[0][2], [14.0, 14.0]);
    assert_uv(quads[0][0], [0.0, 0.5]);
    assert_uv(quads[0][2], [0.5, 1.0]);

    assert_position(quads[1][0], [14.0, 10.0]);
    assert_position(quads[1][2], [18.0, 14.0]);
    assert_uv(quads[1][0], [0.5, 0.5]);
    assert_uv(quads[1][2], [1.0, 1.0]);

    assert_position(quads[2][0], [10.0, 14.0]);
    assert_position(quads[2][2], [14.0, 18.0]);
    assert_uv(quads[2][0], [0.0, 0.5]);
    assert_uv(quads[2][2], [0.5, 1.0]);
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
fn renderer_and_display_list_reuse_recording_and_clipping_storage_after_execution() {
    let backend = CountingRenderer {
        atlas: make_atlas(),
        stats: Rc::new(CountingStats::default()),
    };
    let mut renderer = Renderer::new(backend);
    // Reuse one renderer-owned font capability across both submissions along with the storage the
    // test measures; cloning or reconstructing atlas metadata would create a foreign capability.
    let font = renderer.atlas().font_id("body").unwrap();
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    let long_text = "a".repeat(256);

    painter(&mut list, viewport()).text(font, &long_text, Vec2i::new(0, 0), white);
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(-8.0, -8.0), Vec2f::new(40.0, 0.0), Vec2f::new(0.0, 40.0)], white);
    renderer.render(frame_info(32, 32), &mut list).unwrap();

    let operation_capacity = list.debug_operation_capacity();
    let triangle_capacity = list.debug_triangle_capacity();
    let polygon_capacity = list.debug_polygon_capacity();
    let clipped_capacity = renderer.clipped_triangles.capacity();
    assert!(operation_capacity >= 2);
    assert!(triangle_capacity >= 1);
    assert!(polygon_capacity >= 3);
    assert!(clipped_capacity >= 3);

    painter(&mut list, viewport()).text(font, "a", Vec2i::new(0, 0), white);
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(1.0, 1.0), Vec2f::new(2.0, 1.0), Vec2f::new(1.0, 2.0)], white);
    renderer.render(frame_info(32, 32), &mut list).unwrap();

    assert_eq!(list.debug_operation_capacity(), operation_capacity);
    assert_eq!(list.debug_triangle_capacity(), triangle_capacity);
    assert_eq!(list.debug_polygon_capacity(), polygon_capacity);
    assert_eq!(renderer.clipped_triangles.capacity(), clipped_capacity);
}

#[test]
fn operation_clip_is_intersected_with_viewport_for_every_quad_kind() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    // Text and icon operations intentionally share the renderer's atlas ownership domain.
    let atlas = renderer.atlas();
    let font = atlas.font_id("body").unwrap();
    let close_icon = atlas.icon_id("close").expect("test atlas contains the close icon");
    let white = color(255, 255, 255, 255);
    let clip = Recti::new(2, 0, 20, 4);
    let mut list = DisplayList::new();
    {
        let mut painter = Painter::screen_space(&mut list, clip);
        painter.fill_rect(Recti::new(0, 0, 10, 4), white);
        painter.text(font, "a", Vec2i::new(0, 0), white);
        painter.icon(close_icon, Recti::new(0, 0, 4, 4), white);
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
fn atlas_rectangle_clipping_preserves_fractional_uvs() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let mut list = DisplayList::new();
    painter(&mut list, Recti::new(20, 20, 40, 40)).fill_rect(Recti::new(0, 0, 100, 100), color(255, 255, 255, 255));

    renderer.render(frame_info(100, 100), &mut list).unwrap();

    let vertices = log
        .snapshot()
        .into_iter()
        .find_map(|event| match event {
            RenderEvent::AtlasQuad(vertices) => Some(vertices),
            _ => None,
        })
        .expect("the clipped atlas rectangle should be submitted");
    assert_position(vertices[0], [20.0, 20.0]);
    assert_position(vertices[2], [60.0, 60.0]);
    assert_uv(vertices[0], [0.025, 0.025]);
    assert_uv(vertices[2], [0.075, 0.075]);
}

#[test]
fn external_texture_clipping_preserves_uv_mapping_and_stream_order() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let texture = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    log.clear();
    let white = color(255, 255, 255, 255);
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).fill_rect(Recti::new(0, 0, 1, 1), white);
    painter(&mut list, Recti::new(20, 20, 40, 40)).image(texture, Recti::new(0, 0, 100, 100), white);
    painter(&mut list, viewport()).fill_rect(Recti::new(20, 0, 1, 1), white);

    renderer.render(frame_info(100, 100), &mut list).unwrap();

    let events = log.snapshot();
    assert!(matches!(events[0], RenderEvent::Begin { .. }));
    assert!(matches!(events[1], RenderEvent::AtlasQuad(_)));
    assert_eq!(events[2], RenderEvent::Flush);
    let RenderEvent::ExternalTexture { id, vertices } = &events[3] else {
        panic!("external texture must remain between atlas operations");
    };
    assert_eq!(*id, texture);
    assert_position(vertices[0], [20.0, 20.0]);
    assert_position(vertices[2], [60.0, 60.0]);
    assert_uv(vertices[0], [0.2, 0.2]);
    assert_uv(vertices[2], [0.6, 0.6]);
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
fn custom_barrier_flushes_clips_and_preserves_order() {
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

/// Verifies invalid inputs and failed uploads leave both texture ownership and slot state unchanged.
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
    assert_eq!(renderer.last_texture_slot, 0);
    assert!(renderer.textures.is_empty());
    assert_eq!(create_calls.get(), 0);

    fail_upload.set(true);
    let error = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap_err();
    assert_eq!(error, "backend rejected texture");
    assert_eq!(renderer.last_texture_slot, 0);
    assert!(renderer.textures.is_empty());
    assert_eq!(create_calls.get(), 1);
}

/// Verifies atlas provenance prevents equal local font and icon slots from aliasing across
/// otherwise identical atlas allocations.
#[test]
fn foreign_same_slot_font_and_icon_fail_before_frame_acquisition_without_poisoning_local_ids() {
    // Reconstructing identical source metadata deliberately creates distinct ownership domains.
    // The conventional font and white icon occupy the same local slots and expose identical
    // metrics, leaving atlas provenance as the only distinction between each capability pair.
    let foreign_atlas = make_atlas();
    let local_atlas = make_atlas();
    let foreign_font = foreign_atlas.font_id("body").unwrap();
    let local_font = local_atlas.font_id("body").unwrap();
    let foreign_icon = foreign_atlas.white_icon();
    let local_icon = local_atlas.white_icon();
    assert_ne!(foreign_font, local_font);
    assert_ne!(foreign_icon, local_icon);
    assert_eq!(foreign_atlas.get_font_height(foreign_font), local_atlas.get_font_height(local_font));
    let foreign_icon_rect = foreign_atlas.get_icon_rect(foreign_icon);
    let local_icon_rect = local_atlas.get_icon_rect(local_icon);
    assert_eq!(
        (foreign_icon_rect.x, foreign_icon_rect.y, foreign_icon_rect.width, foreign_icon_rect.height,),
        (local_icon_rect.x, local_icon_rect.y, local_icon_rect.width, local_icon_rect.height)
    );

    let (backend, log) = recording_backend(local_atlas.clone());
    let mut renderer = Renderer::new(backend);
    // Confirm the handles retained by the renderer mint the same local capabilities captured
    // above; the regression must reject only IDs from the separate atlas allocation.
    assert_eq!(renderer.atlas().font_id("body"), Some(local_font));
    assert_eq!(renderer.atlas().white_icon(), local_icon);

    // A foreign font is rejected while walking the opaque display list, before frame acquisition
    // can append a Begin event or submit the preceding valid operation to the backend.
    let mut foreign_font_list = DisplayList::new();
    {
        let mut painter = painter(&mut foreign_font_list, viewport());
        painter.text(local_font, "a", Vec2i::new(0, 0), color(255, 255, 255, 255));
        painter.text(foreign_font, "a", Vec2i::new(4, 0), color(255, 255, 255, 255));
    }
    assert_eq!(
        renderer.render(frame_info(32, 32), &mut foreign_font_list),
        Err(RenderError::UnknownFont { id: foreign_font, operation_index: 1 })
    );
    assert!(foreign_font_list.is_empty());
    assert!(log.snapshot().is_empty(), "foreign font preflight must run before backend acquisition");

    // Repeat the same check independently for icons. Placing a valid local icon first verifies
    // that preflight is atomic for the complete operation stream, not just its first operation.
    let mut foreign_icon_list = DisplayList::new();
    {
        let mut painter = painter(&mut foreign_icon_list, viewport());
        painter.icon(local_icon, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
        painter.icon(foreign_icon, Recti::new(1, 0, 1, 1), color(255, 255, 255, 255));
    }
    assert_eq!(
        renderer.render(frame_info(32, 32), &mut foreign_icon_list),
        Err(RenderError::UnknownIcon { id: foreign_icon, operation_index: 1 })
    );
    assert!(foreign_icon_list.is_empty());
    assert!(log.snapshot().is_empty(), "foreign icon preflight must run before backend acquisition");

    // Rendering both matching local IDs proves that the failed submissions neither acquire the
    // backend nor disturb the renderer's valid atlas resources.
    let mut local_list = DisplayList::new();
    {
        let mut painter = painter(&mut local_list, viewport());
        painter.text(local_font, "a", Vec2i::new(0, 0), color(255, 255, 255, 255));
        painter.icon(local_icon, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    }
    renderer.render(frame_info(32, 32), &mut local_list).unwrap();
    assert!(local_list.is_empty());
    assert_eq!(log.snapshot().iter().filter(|event| matches!(event, RenderEvent::AtlasQuad(_))).count(), 2);
}

/// Verifies renderer provenance prevents equal local slots from aliasing across renderer instances.
#[test]
fn foreign_same_slot_texture_cannot_render_or_destroy_the_local_texture() {
    let (left_backend, _left_log) = recording_backend(make_atlas());
    let mut left = Renderer::new(left_backend);
    let (right_backend, right_log) = recording_backend(make_atlas());
    let mut right = Renderer::new(right_backend);

    // Each renderer deliberately allocates its first local slot with the same dimensions. Only the
    // process-unique renderer identity distinguishes these otherwise identical capabilities.
    let foreign = left.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    let local = right.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();
    assert_eq!(left.last_texture_slot, right.last_texture_slot);
    assert_eq!((foreign.width(), foreign.height()), (local.width(), local.height()));
    assert_ne!(foreign, local);

    right_log.clear();
    let mut foreign_list = DisplayList::new();
    painter(&mut foreign_list, viewport()).image(foreign, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    let error = right.render(frame_info(32, 32), &mut foreign_list).unwrap_err();
    assert!(matches!(error, RenderError::UnknownTexture { id, operation_index: 0 } if id == foreign));
    assert!(right_log.snapshot().is_empty(), "foreign texture preflight must run before backend acquisition");

    // The public lifecycle operation follows the established debug-assert/release-no-op contract.
    // In either mode the foreign capability must leave the matching local slot alive.
    let foreign_free = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| right.free_texture(foreign)));
    if cfg!(debug_assertions) {
        assert!(foreign_free.is_err());
    } else {
        assert!(foreign_free.is_ok());
    }
    assert!(right_log.snapshot().is_empty(), "foreign destruction must not reach the backend");

    // Successful rendering with the local handle proves the rejected destruction did not remove
    // the same-numbered resource from either Renderer tracking or the backend.
    let mut local_list = DisplayList::new();
    painter(&mut local_list, viewport()).image(local, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    right.render(frame_info(32, 32), &mut local_list).unwrap();
    assert!(
        right_log
            .snapshot()
            .iter()
            .any(|event| matches!(event, RenderEvent::ExternalTexture { id, .. } if *id == local))
    );
}

#[cfg(debug_assertions)]
#[test]
fn repeated_texture_destruction_debug_asserts_before_a_second_backend_call() {
    let create_calls = Rc::new(Cell::new(0));
    let destroy_calls = Rc::new(Cell::new(0));
    let backend = TextureUploadRenderer {
        atlas: make_atlas(),
        create_calls,
        destroy_calls: destroy_calls.clone(),
        fail_upload: Rc::new(Cell::new(false)),
    };
    let mut renderer = Renderer::new(backend);
    let texture = renderer.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap();

    renderer.free_texture(texture);
    let repeated = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.free_texture(texture)));

    assert!(repeated.is_err());
    assert_eq!(destroy_calls.get(), 1);
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
    let mut list = DisplayList::new();
    painter(&mut list, viewport()).image(first, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    painter(&mut list, viewport()).image(TextureId::new_test(999, 1, 1), Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(0.0, 0.0), Vec2f::new(8.0, 0.0), Vec2f::new(0.0, 8.0)], color(255, 255, 255, 255));
    let capacities = list_capacities(&list);
    let error = renderer.render(frame_info(32, 32), &mut list).unwrap_err();
    assert!(matches!(error, RenderError::UnknownTexture { id, operation_index: 0 } if id == first));
    assert!(list.is_empty());
    assert_eq!(list_capacities(&list), capacities);
    assert!(!log.snapshot().iter().any(|event| matches!(event, RenderEvent::Begin { .. })));
    drop(renderer);

    let events = log.snapshot();
    assert!(!events.iter().any(|event| matches!(event, RenderEvent::ExternalTexture { .. })));
    let destroyed: HashSet<_> = events
        .iter()
        .filter_map(|event| match event {
            RenderEvent::DestroyTexture(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(destroyed, HashSet::from([first, second, third]));
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

        fn create_texture(&mut self, _id: TextureId, _pixels: &[u8]) -> Result<(), String> {
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
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(0.0, 0.0), Vec2f::new(8.0, 0.0), Vec2f::new(0.0, 8.0)], color(255, 255, 255, 255));
    let capacities = list_capacities(&list);

    assert_eq!(
        renderer.render(frame_info(32, 32), &mut list),
        Err(RenderError::Frame(FrameError::new("acquire failed")))
    );
    assert_eq!(attempts.get(), 1);
    assert!(list.is_empty());
    assert_eq!(list_capacities(&list), capacities);
}

#[test]
fn removed_custom_renderer_fails_preflight_before_backend_acquisition() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let callback = renderer.register_custom_renderer(|_frame, _args| {}).unwrap();
    let mut list = DisplayList::new();
    list.push_custom(viewport(), callback.key, viewport());
    painter(&mut list, viewport()).fill_polygon(&[Vec2f::new(0.0, 0.0), Vec2f::new(8.0, 0.0), Vec2f::new(0.0, 8.0)], color(255, 255, 255, 255));
    let capacities = list_capacities(&list);
    renderer.unregister_custom_renderer(callback).unwrap();

    assert_eq!(
        renderer.render(frame_info(32, 32), &mut list),
        Err(RenderError::UnknownCustomRenderer { operation_index: 0 })
    );
    assert!(list.is_empty());
    assert_eq!(list_capacities(&list), capacities);
    assert!(log.snapshot().is_empty());
}

#[test]
fn foreign_custom_renderer_fails_preflight_before_backend_acquisition() {
    let callback_calls = Rc::new(Cell::new(0));
    let callback_counter = callback_calls.clone();
    let (foreign_backend, foreign_log) = recording_backend(make_atlas());
    let mut foreign_renderer = Renderer::new(foreign_backend);
    let foreign_callback = foreign_renderer
        .register_custom_renderer(move |_frame, _args| callback_counter.set(callback_counter.get() + 1))
        .unwrap();

    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let mut list = DisplayList::new();
    list.push_custom(viewport(), foreign_callback.key, viewport());
    painter(&mut list, viewport()).fill_rect(Recti::new(0, 0, 4, 4), color(255, 255, 255, 255));
    let capacities = list_capacities(&list);

    assert_eq!(
        renderer.render(frame_info(32, 32), &mut list),
        Err(RenderError::UnknownCustomRenderer { operation_index: 0 })
    );
    assert_eq!(callback_calls.get(), 0);
    assert!(list.is_empty());
    assert_eq!(list_capacities(&list), capacities);
    assert!(foreign_log.snapshot().is_empty());
    assert!(log.snapshot().is_empty());
}

#[test]
fn invisible_custom_operations_do_not_invoke_callbacks() {
    let (backend, log) = recording_backend(make_atlas());
    let mut renderer = Renderer::new(backend);
    let callback_calls = Rc::new(Cell::new(0));
    let callback_counter = callback_calls.clone();
    let callback = renderer
        .register_custom_renderer(move |_frame, _args| callback_counter.set(callback_counter.get() + 1))
        .unwrap();

    let cases = [
        // Content is fully outside the operation clip.
        (Recti::new(0, 0, 4, 4), Recti::new(8, 8, 4, 4)),
        // Both rectangles are outside the backend viewport.
        (Recti::new(40, 40, 4, 4), Recti::new(40, 40, 4, 4)),
        (viewport(), Recti::new(4, 4, 0, 8)),
        (viewport(), Recti::new(4, 4, 8, 0)),
    ];

    for (clip, content_area) in cases {
        let mut list = DisplayList::new();
        list.push_custom(clip, callback.key, content_area);
        renderer.render(frame_info(32, 32), &mut list).unwrap();
        assert!(list.is_empty());
    }

    assert_eq!(callback_calls.get(), 0);
    assert_eq!(
        log.snapshot().iter().filter(|event| matches!(event, RenderEvent::Begin { .. })).count(),
        cases.len()
    );
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
    assert!(log.snapshot().iter().any(|event| matches!(event, RenderEvent::End)));
}
