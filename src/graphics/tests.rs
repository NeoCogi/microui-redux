//! Tests for widget-local graphics translation, clipping, and batching.

use super::*;
use crate::render::DisplayList;
use crate::render_command::render_command_stream;
use crate::draw_context::clip_relation;
use crate::test_support::{recording_renderer, RenderEvent};

fn assert_rect_eq(actual: Recti, expected: Recti) {
    assert_eq!(
        (actual.x, actual.y, actual.width, actual.height),
        (expected.x, expected.y, expected.width, expected.height)
    );
}

fn assert_vec2_eq(actual: Vec2f, expected: Vec2f) {
    assert!((actual.x - expected.x).abs() <= GEOM_EPS);
    assert!((actual.y - expected.y).abs() <= GEOM_EPS);
}

fn make_vertex(pos: (f32, f32)) -> Vertex {
    Vertex::new(Vec2f::new(pos.0, pos.1), Vec2f::default(), color4b(255, 255, 255, 255))
}

#[test]
fn clip_relation_reports_partial_overlap() {
    let clip = rect(10, 10, 10, 10);
    let bounds = rect(5, 5, 10, 10);
    assert_eq!(clip_relation(bounds, clip) as u32, Clip::Part as u32);
}

#[test]
fn local_rect_translation_is_preserved_in_emitted_vertices() {
    let atlas = AtlasHandle::from(&AtlasSource {
        width: 1,
        height: 1,
        pixels: &[255, 255, 255, 255],
        icons: &[("white", Recti::new(0, 0, 1, 1))],
        fonts: &[],
        format: SourceFormat::Raw,
        slots: &[],
    });
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut solid_geometry = SolidGeometry::new();
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, &mut solid_geometry, rect(20, 30, 50, 50));
        graphics.push_triangle_local(Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), Vec2f::new(0.0, 10.0), color4b(255, 255, 255, 255));
    }

    match &commands[0].kind {
        &CommandKind::Triangle { vertex_start, vertex_count } => {
            let vertices = &triangle_vertices[vertex_start..vertex_start + vertex_count];
            let a = vertices[0].position();
            let b = vertices[1].position();
            let c = vertices[2].position();
            assert_vec2_eq(a, Vec2f::new(20.0, 30.0));
            assert_vec2_eq(b, Vec2f::new(30.0, 30.0));
            assert_vec2_eq(c, Vec2f::new(20.0, 40.0));
        }
        _ => panic!("expected triangle command"),
    }
}

#[test]
fn local_clip_changes_finalize_triangle_ranges_with_distinct_clips() {
    let atlas = AtlasHandle::from(&AtlasSource {
        width: 1,
        height: 1,
        pixels: &[255, 255, 255, 255],
        icons: &[("white", Recti::new(0, 0, 1, 1))],
        fonts: &[],
        format: SourceFormat::Raw,
        slots: &[],
    });
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut solid_geometry = SolidGeometry::new();
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, &mut solid_geometry, rect(0, 0, 50, 50));
        graphics.stroke_line(Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), 2.0, color(255, 0, 0, 255));
        graphics.push_clip_rect(rect(0, 0, 5, 5));
        graphics.stroke_line(Vec2f::new(0.0, 2.0), Vec2f::new(10.0, 2.0), 2.0, color(255, 0, 0, 255));
    }

    let triangle_clips: Vec<_> = commands
        .iter()
        .filter_map(|command| match command.kind {
            CommandKind::Triangle { .. } => Some((command.clip.x, command.clip.y, command.clip.width, command.clip.height)),
            _ => None,
        })
        .collect();
    assert_eq!(triangle_clips, vec![(0, 0, 50, 50), (0, 0, 5, 5)]);
    assert_eq!(commands.len(), 2);
}

#[test]
fn graphics_restores_shared_clip_stack_on_drop() {
    let atlas = AtlasHandle::from(&AtlasSource {
        width: 1,
        height: 1,
        pixels: &[255, 255, 255, 255],
        icons: &[("white", Recti::new(0, 0, 1, 1))],
        fonts: &[],
        format: SourceFormat::Raw,
        slots: &[],
    });
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut solid_geometry = SolidGeometry::new();
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, &mut solid_geometry, rect(20, 30, 50, 50));
        graphics.push_clip_rect(rect(0, 0, 5, 5));
        assert_rect_eq(graphics.current_clip_rect(), rect(0, 0, 5, 5));
    }

    assert_rect_eq(draw.current_clip_rect(), rect(0, 0, 200, 200));
}

#[test]
fn local_triangles_remain_unclipped_until_canvas_execution() {
    let atlas = AtlasHandle::from(&AtlasSource {
        width: 1,
        height: 1,
        pixels: &[255, 255, 255, 255],
        icons: &[("white", Recti::new(0, 0, 1, 1))],
        fonts: &[],
        format: SourceFormat::Raw,
        slots: &[],
    });
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut solid_geometry = SolidGeometry::new();
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, &mut solid_geometry, rect(20, 30, 50, 50));
        graphics.push_clip_rect(rect(0, 0, 5, 5));
        graphics.stroke_line(Vec2f::new(-10.0, 2.0), Vec2f::new(20.0, 2.0), 2.0, color(255, 0, 0, 255));
    }

    match &commands[0].kind {
        &CommandKind::Triangle { vertex_start, vertex_count } => {
            let vertices = &triangle_vertices[vertex_start..vertex_start + vertex_count];
            assert!(!vertices.is_empty());
            assert!(vertices.iter().any(|vertex| vertex.position().x < 20.0));
            assert_rect_eq(commands[0].clip, rect(20, 30, 5, 5));
        }
        _ => panic!("expected triangle command"),
    }

    let (renderer, log) = recording_renderer(atlas);
    let mut canvas = Canvas::new(renderer, Dimensioni::new(200, 200));
    render_command_stream(&mut canvas, &mut DisplayList::new(), &mut commands, &triangle_vertices);
    let rendered: Vec<_> = log
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            RenderEvent::Triangle(vertices) => Some(vertices),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!rendered.is_empty());
    assert!(rendered.iter().all(|vertex| {
        vertex.position[0] >= 20.0 - GEOM_EPS
            && vertex.position[0] <= 25.0 + GEOM_EPS
            && vertex.position[1] >= 30.0 - GEOM_EPS
            && vertex.position[1] <= 35.0 + GEOM_EPS
    }));
}

#[test]
fn helper_vertices_are_constructible() {
    let vertex = make_vertex((1.0, 2.0));
    assert_vec2_eq(vertex.position(), Vec2f::new(1.0, 2.0));
}

#[test]
fn nested_widget_local_clips_bound_final_renderer_geometry() {
    let atlas = AtlasHandle::from(&AtlasSource {
        width: 1,
        height: 1,
        pixels: &[255, 255, 255, 255],
        icons: &[("white", Recti::new(0, 0, 1, 1))],
        fonts: &[],
        format: SourceFormat::Raw,
        slots: &[],
    });
    let style = Style::default();
    let mut commands = Vec::new();
    let mut triangle_vertices = Vec::new();
    let mut solid_geometry = SolidGeometry::new();
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, &mut solid_geometry, rect(10, 20, 20, 20));
        graphics.with_clip(rect(2, 2, 10, 10), |graphics| {
            graphics.with_clip(rect(5, 5, 10, 10), |graphics| {
                graphics.draw_rect(rect(0, 0, 20, 20), color(255, 0, 0, 255));
            });
        });
    }

    let (renderer, log) = recording_renderer(atlas);
    let mut canvas = Canvas::new(renderer, Dimensioni::new(200, 200));
    render_command_stream(&mut canvas, &mut DisplayList::new(), &mut commands, &triangle_vertices);

    let triangles: Vec<_> = log
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            RenderEvent::Triangle(vertices) => Some(vertices),
            _ => None,
        })
        .collect();
    assert!(!triangles.is_empty());
    for vertex in triangles.into_iter().flatten() {
        assert!(
            vertex.position[0] >= 15.0 - GEOM_EPS
                && vertex.position[0] <= 22.0 + GEOM_EPS
                && vertex.position[1] >= 25.0 - GEOM_EPS
                && vertex.position[1] <= 32.0 + GEOM_EPS,
            "nested widget-local clip leaked renderer vertex {:?}",
            vertex.position
        );
    }
}
