//! Tests for widget-local graphics translation, clipping, and batching.

use super::*;
use crate::container::Command;
use crate::draw_context::clip_relation;

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
fn triangle_bounds_are_conservative() {
    let bounds = rect_from_points(&[Vec2f::new(1.2, 2.6), Vec2f::new(4.8, 3.1), Vec2f::new(3.0, 9.9)]);
    assert_rect_eq(bounds, rect(1, 2, 4, 8));
}

#[test]
fn polygon_cleanup_removes_duplicate_closing_point() {
    let points = dedupe_and_simplify_polygon(&[
        Vec2f::new(0.0, 0.0),
        Vec2f::new(10.0, 0.0),
        Vec2f::new(10.0, 10.0),
        Vec2f::new(0.0, 10.0),
        Vec2f::new(0.0, 0.0),
    ]);
    assert_eq!(points.len(), 4);
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
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, rect(20, 30, 50, 50));
        graphics.push_triangle_local(Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), Vec2f::new(0.0, 10.0), color4b(255, 255, 255, 255));
    }

    match &commands[0] {
        &Command::Triangle { vertex_start, vertex_count } => {
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
fn local_clip_changes_stay_in_one_triangle_batch() {
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
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, rect(0, 0, 50, 50));
        graphics.stroke_line(Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), 2.0, color(255, 0, 0, 255));
        graphics.push_clip_rect(rect(0, 0, 5, 5));
        graphics.stroke_line(Vec2f::new(0.0, 2.0), Vec2f::new(10.0, 2.0), 2.0, color(255, 0, 0, 255));
    }

    let triangle_count = commands.iter().filter(|cmd| matches!(cmd, Command::Triangle { .. })).count();
    let clip_count = commands.iter().filter(|cmd| matches!(cmd, Command::PushClip { .. } | Command::PopClip)).count();
    assert_eq!(triangle_count, 1);
    assert_eq!(clip_count, 0);
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
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, rect(20, 30, 50, 50));
        graphics.push_clip_rect(rect(0, 0, 5, 5));
        assert_rect_eq(graphics.current_clip_rect(), rect(0, 0, 5, 5));
    }

    assert_rect_eq(draw.current_clip_rect(), rect(0, 0, 200, 200));
}

#[test]
fn local_triangles_are_software_clipped_before_emission() {
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
    let mut clip_stack = vec![rect(0, 0, 200, 200)];
    let mut draw = DrawCtx::new(&mut commands, &mut triangle_vertices, &mut clip_stack, &style, &atlas);
    {
        let mut graphics = Graphics::new(&mut draw, rect(20, 30, 50, 50));
        graphics.push_clip_rect(rect(0, 0, 5, 5));
        graphics.stroke_line(Vec2f::new(-10.0, 2.0), Vec2f::new(20.0, 2.0), 2.0, color(255, 0, 0, 255));
    }

    match &commands[0] {
        &Command::Triangle { vertex_start, vertex_count } => {
            let vertices = &triangle_vertices[vertex_start..vertex_start + vertex_count];
            assert!(!vertices.is_empty());
            for vertex in vertices {
                let pos = vertex.position();
                assert!(pos.x >= 20.0 - GEOM_EPS && pos.x <= 25.0 + GEOM_EPS);
                assert!(pos.y >= 30.0 - GEOM_EPS && pos.y <= 35.0 + GEOM_EPS);
            }
        }
        _ => panic!("expected triangle command"),
    }
}

#[test]
fn point_in_triangle_accepts_boundary_points() {
    let a = Vec2f::new(0.0, 0.0);
    let b = Vec2f::new(10.0, 0.0);
    let c = Vec2f::new(0.0, 10.0);
    assert!(point_in_triangle_ccw(Vec2f::new(5.0, 0.0), a, b, c));
}

#[test]
fn helper_vertices_are_constructible() {
    let vertex = make_vertex((1.0, 2.0));
    assert_vec2_eq(vertex.position(), Vec2f::new(1.0, 2.0));
}
