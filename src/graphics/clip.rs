//! Software clipping for retained triangle geometry.

use crate::*;

/// Floating-point tolerance used by clipping predicates.
const CLIP_EPS: f32 = 1.0e-5;
/// Squared clipping tolerance used by duplicate-vertex checks.
const CLIP_EPS_SQ: f32 = CLIP_EPS * CLIP_EPS;

#[derive(Copy, Clone)]
/// One boundary of the rectangular clipping region.
enum RectClipEdge {
    /// Left x boundary.
    Left,
    /// Right x boundary.
    Right,
    /// Top y boundary.
    Top,
    /// Bottom y boundary.
    Bottom,
}

/// Returns the signed 2D cross product.
fn cross2(a: Vec2f, b: Vec2f) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Returns squared distance between two points.
fn distance_sq(a: Vec2f, b: Vec2f) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// Linearly interpolates two 2D vectors.
fn lerp_vec2(a: Vec2f, b: Vec2f, t: f32) -> Vec2f {
    let omt = 1.0 - t;
    Vec2f::new(a.x * omt + b.x * t, a.y * omt + b.y * t)
}

/// Linearly interpolates two byte channels and clamps to the channel range.
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    ((a as f32) + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

/// Linearly interpolates two packed RGBA colors.
fn lerp_color4b(a: Color4b, b: Color4b, t: f32) -> Color4b {
    color4b(lerp_u8(a.x, b.x, t), lerp_u8(a.y, b.y, t), lerp_u8(a.z, b.z, t), lerp_u8(a.w, b.w, t))
}

/// Linearly interpolates position, UV, and color for a clipped vertex.
fn lerp_vertex(a: Vertex, b: Vertex, t: f32) -> Vertex {
    Vertex::new(
        lerp_vec2(a.position(), b.position(), t),
        lerp_vec2(a.tex_coord(), b.tex_coord(), t),
        lerp_color4b(a.color(), b.color(), t),
    )
}

/// Returns whether `point` is inside one edge half-plane.
fn point_inside_clip_edge(point: Vec2f, edge: RectClipEdge, clip: Recti) -> bool {
    let left = clip.x as f32;
    let right = (clip.x + clip.width) as f32;
    let top = clip.y as f32;
    let bottom = (clip.y + clip.height) as f32;
    match edge {
        RectClipEdge::Left => point.x >= left - CLIP_EPS,
        RectClipEdge::Right => point.x <= right + CLIP_EPS,
        RectClipEdge::Top => point.y >= top - CLIP_EPS,
        RectClipEdge::Bottom => point.y <= bottom + CLIP_EPS,
    }
}

/// Finds the normalized segment parameter where `a..b` intersects an edge boundary.
fn intersection_t_for_edge(a: Vec2f, b: Vec2f, edge: RectClipEdge, clip: Recti) -> f32 {
    let (start, delta, boundary) = match edge {
        RectClipEdge::Left => (a.x, b.x - a.x, clip.x as f32),
        RectClipEdge::Right => (a.x, b.x - a.x, (clip.x + clip.width) as f32),
        RectClipEdge::Top => (a.y, b.y - a.y, clip.y as f32),
        RectClipEdge::Bottom => (a.y, b.y - a.y, (clip.y + clip.height) as f32),
    };

    if delta.abs() <= CLIP_EPS {
        0.0
    } else {
        ((boundary - start) / delta).clamp(0.0, 1.0)
    }
}

/// Computes the interpolated vertex at a segment/edge intersection.
fn intersect_vertex_edge(a: Vertex, b: Vertex, edge: RectClipEdge, clip: Recti) -> Vertex {
    let t = intersection_t_for_edge(a.position(), b.position(), edge, clip);
    lerp_vertex(a, b, t)
}

/// Pushes a vertex unless it duplicates the previous output vertex.
fn push_unique_vertex(dst: &mut [Vertex; 8], count: &mut usize, vertex: Vertex) {
    if *count > 0 && distance_sq(dst[*count - 1].position(), vertex.position()) <= CLIP_EPS_SQ {
        dst[*count - 1] = vertex;
        return;
    }

    debug_assert!(*count < dst.len(), "rect-clipped triangle exceeded fixed vertex budget");
    dst[*count] = vertex;
    *count += 1;
}

/// Clips a convex polygon against one rectangular edge using Sutherland-Hodgman clipping.
fn clip_polygon_against_edge(input: &[Vertex; 8], input_count: usize, edge: RectClipEdge, clip: Recti, output: &mut [Vertex; 8]) -> usize {
    if input_count == 0 {
        return 0;
    }

    let mut out_count = 0;
    let mut prev = input[input_count - 1];
    let mut prev_inside = point_inside_clip_edge(prev.position(), edge, clip);

    for curr in input.iter().copied().take(input_count) {
        let curr_inside = point_inside_clip_edge(curr.position(), edge, clip);

        if curr_inside != prev_inside {
            let intersection = intersect_vertex_edge(prev, curr, edge, clip);
            push_unique_vertex(output, &mut out_count, intersection);
        }
        if curr_inside {
            push_unique_vertex(output, &mut out_count, curr);
        }

        prev = curr;
        prev_inside = curr_inside;
    }

    if out_count > 1 && distance_sq(output[0].position(), output[out_count - 1].position()) <= CLIP_EPS_SQ {
        out_count -= 1;
    }

    out_count
}

/// Computes signed area for a vertex polygon.
fn signed_area_vertices(points: &[Vertex]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }

    let mut area = 0.0;
    for idx in 0..points.len() {
        let curr = points[idx].position();
        let next = points[(idx + 1) % points.len()].position();
        area += curr.x * next.y - next.x * curr.y;
    }
    area * 0.5
}

/// Clips one triangle against `clip` and emits zero or more fully clipped triangles.
pub(crate) fn clip_triangle_vertices_to_rect<F>(v0: Vertex, v1: Vertex, v2: Vertex, clip: Recti, mut emit: F)
where
    F: FnMut(Vertex, Vertex, Vertex),
{
    if clip.width <= 0 || clip.height <= 0 {
        return;
    }

    let mut input = [Vertex::default(); 8];
    let mut output = [Vertex::default(); 8];
    input[0] = v0;
    input[1] = v1;
    input[2] = v2;
    let mut input_count = 3usize;

    for edge in [RectClipEdge::Left, RectClipEdge::Right, RectClipEdge::Top, RectClipEdge::Bottom] {
        let output_count = clip_polygon_against_edge(&input, input_count, edge, clip, &mut output);
        if output_count < 3 {
            return;
        }
        input_count = output_count;
        std::mem::swap(&mut input, &mut output);
    }

    if signed_area_vertices(&input[..input_count]).abs() <= CLIP_EPS {
        return;
    }

    for idx in 1..input_count - 1 {
        let a = input[0];
        let b = input[idx];
        let c = input[idx + 1];
        let tri_area = cross2(b.position() - a.position(), c.position() - a.position());
        if tri_area.abs() > CLIP_EPS {
            emit(a, b, c);
        }
    }
}
