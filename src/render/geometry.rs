//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
//! Stateless tessellation, bounds, translation, and final triangle clipping.

use super::backend::Vertex;
use rs_math3d::{Color4b, FloatVector, Recti, Vec2f, Vec2i};
use std::ops::Range;

/// Floating-point tolerance shared by tessellation and clipping predicates.
pub(crate) const GEOM_EPS: f32 = 1.0e-5;
/// Squared tolerance used for duplicate-position checks.
const GEOM_EPS_SQ: f32 = GEOM_EPS * GEOM_EPS;

/// Texture-independent vertex used by solid geometry.
#[derive(Clone, Copy)]
pub(crate) struct SolidVertex {
    /// Screen-space position.
    pub(crate) position: Vec2f,
    /// Interpolated vertex color.
    pub(crate) color: Color4b,
}

/// Strongly typed, texture-independent solid triangle.
#[derive(Clone, Copy)]
pub(crate) struct SolidTriangle([SolidVertex; 3]);

impl SolidTriangle {
    /// Creates a triangle from three positions sharing one color.
    pub(crate) const fn new(positions: [Vec2f; 3], color: Color4b) -> Self {
        Self([
            SolidVertex { position: positions[0], color },
            SolidVertex { position: positions[1], color },
            SolidVertex { position: positions[2], color },
        ])
    }

    /// Returns the triangle's three vertices in winding order.
    pub(crate) const fn vertices(&self) -> &[SolidVertex; 3] {
        &self.0
    }

    /// Returns this triangle translated by a screen-space offset.
    fn translated(mut self, offset: Vec2f) -> Self {
        for vertex in &mut self.0 {
            vertex.position = vertex.position + offset;
        }
        self
    }
}

/// Validated range inside a [`SolidGeometry`] triangle arena.
pub(crate) struct SolidTriangleRange {
    /// Inclusive triangle index.
    start: usize,
    /// Exclusive triangle index.
    end: usize,
}

impl SolidTriangleRange {
    /// Creates a range only when at least one complete triangle was appended.
    fn new(start: usize, end: usize) -> Option<Self> {
        (start < end).then_some(Self { start, end })
    }

    /// Returns a standard range for read-only execution.
    #[allow(dead_code)]
    pub(crate) fn as_range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// Extends this range over an immediately adjacent range.
    pub(crate) fn extend(&mut self, next: &Self) -> bool {
        if self.end != next.start {
            return false;
        }
        self.end = next.end;
        true
    }
}

/// Retained solid-triangle arena with reusable polygon tessellation storage.
#[derive(Default)]
pub(crate) struct SolidGeometry {
    /// Screen-space triangles referenced by validated ranges.
    triangles: Vec<SolidTriangle>,
    /// Active polygon boundary reused by simplification and ear clipping.
    polygon_boundary: Vec<Vec2f>,
}

impl SolidGeometry {
    /// Creates empty solid geometry.
    pub(crate) const fn new() -> Self {
        Self {
            triangles: Vec::new(),
            polygon_boundary: Vec::new(),
        }
    }

    /// Clears retained and working geometry while preserving allocations.
    pub(crate) fn clear(&mut self) {
        self.triangles.clear();
        self.polygon_boundary.clear();
    }

    /// Returns whether no solid triangles are retained.
    pub(crate) fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// Returns all retained solid triangles.
    pub(crate) fn triangles(&self) -> &[SolidTriangle] {
        &self.triangles
    }

    /// Appends pre-tessellated triangles after applying a screen-space offset.
    pub(crate) fn append_triangles(&mut self, triangles: &[SolidTriangle], offset: Vec2f) -> Option<SolidTriangleRange> {
        let start = self.triangles.len();
        self.triangles.reserve(triangles.len());
        for triangle in triangles {
            self.triangles.push(triangle.translated(offset));
        }
        SolidTriangleRange::new(start, self.triangles.len())
    }

    /// Tessellates and appends one thick line.
    pub(crate) fn append_line(&mut self, from: Vec2f, to: Vec2f, width: f32, color: Color4b, offset: Vec2f) -> Option<SolidTriangleRange> {
        let triangles = Self::line_triangles(from, to, width, color)?;
        self.append_triangles(&triangles, offset)
    }

    /// Cleans, triangulates, and appends one simple polygon.
    ///
    /// Convex polygons use a triangle fan. Concave polygons use ear clipping against the reusable
    /// `polygon_boundary` workspace. Both paths append strongly typed triangles directly, so no
    /// flat triangle-position buffer escapes this object.
    pub(crate) fn append_polygon(&mut self, points: &[Vec2f], color: Color4b, offset: Vec2f) -> Option<SolidTriangleRange> {
        let start = self.triangles.len();
        if points.len() < 3 || !self.prepare_polygon(points) {
            return None;
        }

        let area = self.polygon_signed_area();
        if !area.is_finite() || area.abs() <= GEOM_EPS {
            self.polygon_boundary.clear();
            return None;
        }
        if area < 0.0 {
            self.polygon_boundary.reverse();
        }

        self.triangles.reserve(self.polygon_boundary.len() - 2);
        if self.polygon_is_convex_ccw() {
            for index in 1..self.polygon_boundary.len() - 1 {
                self.triangles.push(
                    SolidTriangle::new(
                        [self.polygon_boundary[0], self.polygon_boundary[index], self.polygon_boundary[index + 1]],
                        color,
                    )
                    .translated(offset),
                );
            }
            self.polygon_boundary.clear();
            return SolidTriangleRange::new(start, self.triangles.len());
        }

        // Ear clipping repeatedly removes one convex corner whose triangle contains no other
        // active polygon vertex. The reusable workspace therefore remains the active boundary
        // throughout the algorithm instead of doubling as an externally visible output buffer.
        while self.polygon_boundary.len() > 3 {
            let active_len = self.polygon_boundary.len();
            let mut ear = None;
            for index in 0..active_len {
                let previous = (index + active_len - 1) % active_len;
                let next = (index + 1) % active_len;
                let a = self.polygon_boundary[previous];
                let b = self.polygon_boundary[index];
                let c = self.polygon_boundary[next];
                if !is_convex_ccw(a, b, c) {
                    continue;
                }

                let contains_other = (0..active_len)
                    .any(|probe| probe != previous && probe != index && probe != next && point_in_triangle_ccw(self.polygon_boundary[probe], a, b, c));
                if !contains_other {
                    ear = Some((index, [a, b, c]));
                    break;
                }
            }

            let Some((index, triangle)) = ear else {
                // A malformed or numerically unstable boundary must not leave a partial polygon
                // in the retained triangle arena.
                self.triangles.truncate(start);
                self.polygon_boundary.clear();
                return None;
            };
            self.polygon_boundary.remove(index);
            self.triangles.push(SolidTriangle::new(triangle, color).translated(offset));
        }

        let final_triangle = [self.polygon_boundary[0], self.polygon_boundary[1], self.polygon_boundary[2]];
        self.triangles.push(SolidTriangle::new(final_triangle, color).translated(offset));
        self.polygon_boundary.clear();
        SolidTriangleRange::new(start, self.triangles.len())
    }

    /// Produces the two triangles forming one valid thick line.
    fn line_triangles(from: Vec2f, to: Vec2f, width: f32, color: Color4b) -> Option<[SolidTriangle; 2]> {
        if !width.is_finite() || width <= 0.0 || !point_is_finite(from) || !point_is_finite(to) {
            return None;
        }

        let delta = to - from;
        let len_sq = delta.length_squared();
        let (p0, p1, p2, p3) = if len_sq <= GEOM_EPS_SQ {
            let half = width * 0.5;
            (
                Vec2f::new(from.x - half, from.y - half),
                Vec2f::new(from.x + half, from.y - half),
                Vec2f::new(from.x + half, from.y + half),
                Vec2f::new(from.x - half, from.y + half),
            )
        } else {
            let inv_len = len_sq.sqrt().recip();
            let normal = Vec2f::new(-delta.y * inv_len, delta.x * inv_len) * (width * 0.5);
            (from + normal, to + normal, to - normal, from - normal)
        };
        Some([SolidTriangle::new([p0, p1, p2], color), SolidTriangle::new([p0, p2, p3], color)])
    }

    /// Copies a finite polygon into the reusable workspace and removes redundant vertices.
    fn prepare_polygon(&mut self, points: &[Vec2f]) -> bool {
        self.polygon_boundary.clear();
        // The second pass appends a simplified boundary behind the copied input, so reserve both
        // spans up front and avoid a growth allocation halfway through tessellation.
        self.polygon_boundary.reserve(points.len().saturating_mul(2));
        for point in points.iter().copied() {
            if !point_is_finite(point) {
                self.polygon_boundary.clear();
                return false;
            }
            if self
                .polygon_boundary
                .last()
                .map(|previous| (*previous - point).length_squared() > GEOM_EPS_SQ)
                .unwrap_or(true)
            {
                self.polygon_boundary.push(point);
            }
        }

        if self.polygon_boundary.len() > 1
            && (self.polygon_boundary[0] - *self.polygon_boundary.last().expect("non-empty polygon")).length_squared() <= GEOM_EPS_SQ
        {
            self.polygon_boundary.pop();
        }
        if self.polygon_boundary.len() < 3 {
            self.polygon_boundary.clear();
            return false;
        }

        // Append the simplified boundary after the copied boundary, then discard the original
        // prefix. This reuses one allocation without requiring a second temporary vector.
        let copied_len = self.polygon_boundary.len();
        for index in 0..copied_len {
            let previous = self.polygon_boundary[(index + copied_len - 1) % copied_len];
            let current = self.polygon_boundary[index];
            let next = self.polygon_boundary[(index + 1) % copied_len];
            if (previous - current).length_squared() <= GEOM_EPS_SQ || (current - next).length_squared() <= GEOM_EPS_SQ {
                continue;
            }
            if cross2(current - previous, next - current).abs() <= GEOM_EPS {
                continue;
            }
            self.polygon_boundary.push(current);
        }
        self.polygon_boundary.drain(..copied_len);
        if self.polygon_boundary.len() < 3 {
            self.polygon_boundary.clear();
            return false;
        }
        true
    }

    /// Computes winding and signed area for the prepared polygon boundary.
    fn polygon_signed_area(&self) -> f32 {
        let mut area = 0.0;
        for index in 0..self.polygon_boundary.len() {
            let current = self.polygon_boundary[index];
            let next = self.polygon_boundary[(index + 1) % self.polygon_boundary.len()];
            area += current.x * next.y - next.x * current.y;
        }
        area * 0.5
    }

    /// Returns whether every prepared polygon corner is convex and counter-clockwise.
    fn polygon_is_convex_ccw(&self) -> bool {
        (0..self.polygon_boundary.len()).all(|index| {
            is_convex_ccw(
                self.polygon_boundary[(index + self.polygon_boundary.len() - 1) % self.polygon_boundary.len()],
                self.polygon_boundary[index],
                self.polygon_boundary[(index + 1) % self.polygon_boundary.len()],
            )
        })
    }

    /// Detaches retained triangles while keeping polygon scratch capacity available for reuse.
    pub(crate) fn take_recorded(&mut self) -> Self {
        self.polygon_boundary.clear();
        Self {
            triangles: std::mem::take(&mut self.triangles),
            polygon_boundary: Vec::new(),
        }
    }

    #[cfg(test)]
    /// Returns retained triangle allocation capacity.
    pub(crate) fn triangle_capacity(&self) -> usize {
        self.triangles.capacity()
    }

    #[cfg(test)]
    /// Returns polygon workspace allocation capacity.
    pub(crate) fn polygon_capacity(&self) -> usize {
        self.polygon_boundary.capacity()
    }

    #[cfg(test)]
    /// Reserves polygon workspace for allocation-reuse tests.
    pub(crate) fn reserve_polygon_capacity(&mut self, additional: usize) {
        self.polygon_boundary.reserve(additional);
    }
}

/// Coordinate axis tested by one rectangular clipping edge.
#[derive(Clone, Copy)]
enum ClipAxis {
    /// Vertical boundary that compares x coordinates.
    X,
    /// Horizontal boundary that compares y coordinates.
    Y,
}

/// Half-plane retained by one rectangular clipping edge.
#[derive(Clone, Copy)]
enum ClipSide {
    /// Retains coordinates greater than or equal to the boundary.
    Minimum,
    /// Retains coordinates less than or equal to the boundary.
    Maximum,
}

/// One fully specified rectangular clipping boundary.
///
/// The edge owns its axis, retained side, and scalar boundary. Consequently, containment and
/// intersection operations do not also need a `ClipRect` argument.
#[derive(Clone, Copy)]
struct ClipEdge {
    /// Coordinate component inspected by this edge.
    axis: ClipAxis,
    /// Which side of `boundary` remains visible.
    side: ClipSide,
    /// Screen-space boundary coordinate.
    boundary: f32,
}

impl ClipEdge {
    /// Creates one clipping boundary with all information needed to operate independently.
    const fn new(axis: ClipAxis, side: ClipSide, boundary: f32) -> Self {
        Self { axis, side, boundary }
    }

    /// Returns the coordinate selected by this edge's axis.
    fn coordinate(self, point: Vec2f) -> f32 {
        match self.axis {
            ClipAxis::X => point.x,
            ClipAxis::Y => point.y,
        }
    }

    /// Returns whether `point` lies in the half-plane retained by this edge.
    fn contains(self, point: Vec2f) -> bool {
        let coordinate = self.coordinate(point);
        match self.side {
            ClipSide::Minimum => coordinate >= self.boundary - GEOM_EPS,
            ClipSide::Maximum => coordinate <= self.boundary + GEOM_EPS,
        }
    }

    /// Returns the segment parameter at which `from..to` intersects this edge.
    fn intersection_t(self, from: Vec2f, to: Vec2f) -> f32 {
        let start = self.coordinate(from);
        let delta = self.coordinate(to) - start;

        // A parallel segment has no unique intersection. Returning its start is safe because this
        // method is only used when the two endpoints disagree about containment.
        if delta.abs() <= GEOM_EPS {
            0.0
        } else {
            ((self.boundary - start) / delta).clamp(0.0, 1.0)
        }
    }

    /// Interpolates all attributes at the point where a segment crosses this edge.
    fn intersect(self, from: Vertex, to: Vertex) -> Vertex {
        Vertex::lerp(from, to, self.intersection_t(from.position(), to.position()))
    }

    /// Clips one convex polygon against this edge and replaces `output`.
    fn clip(self, input: &TriangleClippingResult, output: &mut TriangleClippingResult) {
        output.clear();
        if input.is_empty() {
            return;
        }

        // Sutherland-Hodgman examines each directed segment, including the closing segment from
        // the final vertex back to the first.
        let mut previous = input.last();
        let mut previous_inside = self.contains(previous.position());
        for current in input.vertices() {
            let current_inside = self.contains(current.position());
            if current_inside != previous_inside {
                output.push_unique(self.intersect(previous, current));
            }
            if current_inside {
                output.push_unique(current);
            }
            previous = current;
            previous_inside = current_inside;
        }
        output.remove_duplicate_closing_vertex();
    }
}

/// Fixed-capacity intermediate and final result of clipping one triangle.
///
/// A triangle clipped by an axis-aligned rectangle can contain at most seven vertices, so eight
/// slots leave one defensive spare without allocating in the rendering loop. This type is not a
/// general polygon container; it exists only to carry one triangle's result from edge to edge.
#[derive(Default)]
struct TriangleClippingResult {
    /// Valid vertices stored in winding order.
    vertices: [Vertex; 8],
    /// Number of initialized entries in `vertices`.
    len: usize,
}

impl TriangleClippingResult {
    /// Creates a polygon containing exactly one complete triangle.
    fn from_triangle(triangle: [Vertex; 3]) -> Self {
        let mut polygon = Self::default();
        polygon.vertices[..3].copy_from_slice(&triangle);
        polygon.len = 3;
        polygon
    }

    /// Removes all logical vertices while retaining the fixed backing storage.
    fn clear(&mut self) {
        self.len = 0;
    }

    /// Returns whether the polygon has no vertices.
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the current number of polygon vertices.
    fn len(&self) -> usize {
        self.len
    }

    /// Returns valid vertices in winding order.
    fn vertices(&self) -> impl Iterator<Item = Vertex> + '_ {
        self.vertices[..self.len].iter().copied()
    }

    /// Returns the final vertex.
    fn last(&self) -> Vertex {
        debug_assert!(!self.is_empty(), "an empty clipped polygon has no final vertex");
        self.vertices[self.len - 1]
    }

    /// Appends a vertex unless it duplicates the previous output position.
    fn push_unique(&mut self, vertex: Vertex) {
        if self.len > 0 && (self.vertices[self.len - 1].position() - vertex.position()).length_squared() <= GEOM_EPS_SQ {
            // Keep the latest interpolated attributes at a shared position.
            self.vertices[self.len - 1] = vertex;
            return;
        }
        debug_assert!(self.len < self.vertices.len(), "rect-clipped triangle exceeded fixed vertex capacity");
        self.vertices[self.len] = vertex;
        self.len += 1;
    }

    /// Removes a repeated final vertex that closes the polygon explicitly.
    fn remove_duplicate_closing_vertex(&mut self) {
        if self.len > 1 && (self.vertices[0].position() - self.vertices[self.len - 1].position()).length_squared() <= GEOM_EPS_SQ {
            self.len -= 1;
        }
    }

    /// Computes signed polygon area from the stored vertex positions.
    fn signed_area(&self) -> f32 {
        if self.len < 3 {
            return 0.0;
        }

        let mut area = 0.0;
        for index in 0..self.len {
            let current = self.vertices[index].position();
            let next = self.vertices[(index + 1) % self.len].position();
            area += current.x * next.y - next.x * current.y;
        }
        area * 0.5
    }

    /// Emits a triangle fan while rejecting degenerate output triangles.
    fn triangulate_into(&self, output: &mut Vec<Vertex>) {
        if self.signed_area().abs() <= GEOM_EPS {
            return;
        }

        for index in 1..self.len.saturating_sub(1) {
            let triangle = [self.vertices[0], self.vertices[index], self.vertices[index + 1]];
            let left = triangle[1].position() - triangle[0].position();
            let right = triangle[2].position() - triangle[0].position();
            if cross2(left, right).abs() > GEOM_EPS {
                output.extend_from_slice(&triangle);
            }
        }
    }
}

/// Axis-aligned rectangle prepared for repeated triangle clipping.
///
/// The four edges are constructed once and then iterated by value for every triangle. This keeps
/// the source rectangle and its derived boundary semantics hidden from individual edge operations.
pub(crate) struct ClipRect {
    /// Stable left, right, top, and bottom clipping order.
    edges: [ClipEdge; 4],
}

impl ClipRect {
    /// Creates a non-empty clipping rectangle and precomputes its four edge objects.
    pub(crate) fn new(rect: Recti) -> Option<Self> {
        if rect.width <= 0 || rect.height <= 0 {
            return None;
        }

        let left = rect.x as f32;
        let right = rect.x.saturating_add(rect.width) as f32;
        let top = rect.y as f32;
        let bottom = rect.y.saturating_add(rect.height) as f32;
        Some(Self {
            edges: [
                ClipEdge::new(ClipAxis::X, ClipSide::Minimum, left),
                ClipEdge::new(ClipAxis::X, ClipSide::Maximum, right),
                ClipEdge::new(ClipAxis::Y, ClipSide::Minimum, top),
                ClipEdge::new(ClipAxis::Y, ClipSide::Maximum, bottom),
            ],
        })
    }

    /// Iterates over the rectangle's four owned edges in stable clipping order.
    fn edges(&self) -> impl Iterator<Item = ClipEdge> + '_ {
        self.edges.iter().copied()
    }

    /// Clips one final backend triangle and appends zero or more flat output triangles.
    pub(crate) fn clip_triangle(&self, triangle: [Vertex; 3], output: &mut Vec<Vertex>) {
        let mut input = TriangleClippingResult::from_triangle(triangle);
        let mut scratch = TriangleClippingResult::default();

        // Each edge consumes the previous polygon and writes a new one. Swapping the two fixed
        // buffers avoids allocation and makes the output of one boundary the input of the next.
        for edge in self.edges() {
            edge.clip(&input, &mut scratch);
            if scratch.len() < 3 {
                return;
            }
            std::mem::swap(&mut input, &mut scratch);
        }

        input.triangulate_into(output);
    }
}

/// Applies an integer translation without changing rectangle extents.
pub(crate) fn translate_rect(rect: Recti, offset: Vec2i) -> Recti {
    Recti::new(rect.x.saturating_add(offset.x), rect.y.saturating_add(offset.y), rect.width, rect.height)
}

/// Computes a conservative integer bounding rectangle for finite floating-point positions.
pub(super) fn bounds_for_points(points: &[Vec2f]) -> Option<Recti> {
    let first = *points.first()?;
    if !point_is_finite(first) {
        return None;
    }

    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x;
    let mut max_y = first.y;
    for point in points.iter().copied().skip(1) {
        if !point_is_finite(point) {
            return None;
        }
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }

    let x0 = min_x.floor() as i32;
    let y0 = min_y.floor() as i32;
    let x1 = max_x.ceil() as i32;
    let y1 = max_y.ceil() as i32;
    Some(Recti::new(
        x0,
        y0,
        ((x1 as i64 - x0 as i64).max(0).min(i32::MAX as i64)) as i32,
        ((y1 as i64 - y0 as i64).max(0).min(i32::MAX as i64)) as i32,
    ))
}

/// Computes conservative bounds for one finite thick line.
pub(super) fn bounds_for_line(from: Vec2f, to: Vec2f, width: f32) -> Option<Recti> {
    if !width.is_finite() || width <= 0.0 || !point_is_finite(from) || !point_is_finite(to) {
        return None;
    }

    let half = width * 0.5;
    bounds_for_points(&[
        Vec2f::new(from.x.min(to.x) - half, from.y.min(to.y) - half),
        Vec2f::new(from.x.max(to.x) + half, from.y.max(to.y) + half),
    ])
}

/// Returns whether both point components are finite.
fn point_is_finite(point: Vec2f) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// Returns the signed 2D cross product.
fn cross2(left: Vec2f, right: Vec2f) -> f32 {
    left.x * right.y - left.y * right.x
}

/// Returns whether three points form a convex counter-clockwise corner.
fn is_convex_ccw(previous: Vec2f, current: Vec2f, next: Vec2f) -> bool {
    cross2(current - previous, next - current) > GEOM_EPS
}

/// Returns whether a point lies inside or on a counter-clockwise triangle.
fn point_in_triangle_ccw(point: Vec2f, a: Vec2f, b: Vec2f, c: Vec2f) -> bool {
    let ab = cross2(b - a, point - a);
    let bc = cross2(c - b, point - b);
    let ca = cross2(a - c, point - c);
    ab >= -GEOM_EPS && bc >= -GEOM_EPS && ca >= -GEOM_EPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_math3d::color4b;

    fn white() -> Color4b {
        color4b(255, 255, 255, 255)
    }

    fn test_polygon(points: &[Vec2f]) -> Vec<SolidTriangle> {
        let mut geometry = SolidGeometry::new();
        geometry.append_polygon(points, white(), Vec2f::new(0.0, 0.0));
        geometry.triangles().to_vec()
    }

    #[test]
    fn convex_polygon_uses_triangle_fan() {
        let triangles = test_polygon(&[Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), Vec2f::new(10.0, 10.0), Vec2f::new(0.0, 10.0)]);
        assert_eq!(triangles.len(), 2);
    }

    #[test]
    fn concave_polygon_uses_ear_clipping() {
        let triangles = test_polygon(&[
            Vec2f::new(0.0, 0.0),
            Vec2f::new(10.0, 0.0),
            Vec2f::new(5.0, 5.0),
            Vec2f::new(10.0, 10.0),
            Vec2f::new(0.0, 10.0),
        ]);
        assert_eq!(triangles.len(), 3);
    }

    #[test]
    fn duplicate_and_collinear_points_are_simplified() {
        let triangles = test_polygon(&[
            Vec2f::new(0.0, 0.0),
            Vec2f::new(5.0, 0.0),
            Vec2f::new(10.0, 0.0),
            Vec2f::new(10.0, 10.0),
            Vec2f::new(0.0, 10.0),
            Vec2f::new(0.0, 0.0),
            Vec2f::new(0.0, 0.0),
        ]);
        assert_eq!(triangles.len(), 2);
    }

    #[test]
    fn clockwise_polygon_is_normalized() {
        let triangles = test_polygon(&[Vec2f::new(0.0, 0.0), Vec2f::new(0.0, 10.0), Vec2f::new(10.0, 10.0), Vec2f::new(10.0, 0.0)]);
        assert_eq!(triangles.len(), 2);
    }

    #[test]
    fn degenerate_and_non_finite_polygons_emit_nothing() {
        for points in [
            vec![Vec2f::new(0.0, 0.0), Vec2f::new(1.0, 1.0), Vec2f::new(2.0, 2.0)],
            vec![Vec2f::new(0.0, 0.0), Vec2f::new(f32::NAN, 1.0), Vec2f::new(2.0, 0.0)],
            vec![Vec2f::new(0.0, 0.0), Vec2f::new(1.0, 0.0)],
        ] {
            let triangles = test_polygon(&points);
            assert!(triangles.is_empty());
        }
    }

    #[test]
    fn polygon_triangulation_reuses_solid_geometry_workspace() {
        let mut geometry = SolidGeometry::new();
        geometry.append_polygon(
            &[Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), Vec2f::new(10.0, 10.0), Vec2f::new(0.0, 10.0)],
            white(),
            Vec2f::new(0.0, 0.0),
        );
        let capacity = geometry.polygon_capacity();
        assert_eq!(geometry.triangles().len(), 2);

        geometry.clear();
        geometry.append_polygon(
            &[Vec2f::new(0.0, 0.0), Vec2f::new(5.0, 0.0), Vec2f::new(0.0, 5.0)],
            white(),
            Vec2f::new(0.0, 0.0),
        );
        assert_eq!(geometry.triangles().len(), 1);
        assert_eq!(geometry.polygon_capacity(), capacity);
    }

    #[test]
    fn solid_geometry_owns_translation_ranges_and_polygon_scratch() {
        let mut geometry = SolidGeometry::new();
        let polygon = geometry
            .append_polygon(
                &[Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), Vec2f::new(10.0, 10.0), Vec2f::new(0.0, 10.0)],
                white(),
                Vec2f::new(5.0, 7.0),
            )
            .expect("valid polygon");
        let line = geometry
            .append_line(Vec2f::new(0.0, 20.0), Vec2f::new(10.0, 20.0), 2.0, white(), Vec2f::new(5.0, 7.0))
            .expect("valid line");

        assert_eq!(polygon.as_range(), 0..2);
        assert_eq!(line.as_range(), 2..4);
        assert!(
            geometry
                .triangles()
                .iter()
                .flat_map(|triangle| triangle.vertices())
                .all(|vertex| vertex.position.x >= 5.0 && vertex.position.y >= 7.0)
        );

        let polygon_capacity = geometry.polygon_capacity();
        let recorded = geometry.take_recorded();
        assert!(geometry.is_empty());
        assert_eq!(geometry.polygon_capacity(), polygon_capacity);
        assert_eq!(recorded.triangles().len(), 4);
    }

    #[test]
    fn thick_and_zero_length_lines_emit_complete_quads() {
        for (from, to) in [(Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0)), (Vec2f::new(5.0, 5.0), Vec2f::new(5.0, 5.0))] {
            let triangles = SolidGeometry::line_triangles(from, to, 2.0, white()).expect("valid line");
            assert_eq!(triangles.len(), 2);
            assert!(triangles.iter().all(|triangle| triangle.vertices().len() == 3));
        }
    }

    #[test]
    fn clipped_backend_triangle_interpolates_edge_attributes() {
        let triangle = [
            Vertex::new(Vec2f::new(-10.0, 0.0), Vec2f::new(0.0, 0.0), color4b(255, 0, 0, 255)),
            Vertex::new(Vec2f::new(10.0, 0.0), Vec2f::new(1.0, 0.0), color4b(0, 0, 255, 255)),
            Vertex::new(Vec2f::new(0.0, 10.0), Vec2f::new(0.5, 1.0), color4b(0, 255, 0, 255)),
        ];
        let mut output = Vec::new();
        let clip = ClipRect::new(Recti::new(0, 0, 10, 10)).expect("non-empty clip");
        clip.clip_triangle(triangle, &mut output);

        assert!(!output.is_empty());
        let edge = output
            .iter()
            .find(|vertex| vertex.position().x.abs() <= GEOM_EPS && vertex.position().y.abs() <= GEOM_EPS)
            .expect("clipped edge vertex");
        assert!((edge.tex_coord().x - 0.5).abs() <= GEOM_EPS);
        assert!((edge.color().x as i16 - 128).abs() <= 1);
        assert!((edge.color().z as i16 - 128).abs() <= 1);
    }

    #[test]
    fn empty_rectangles_do_not_create_clippers() {
        assert!(ClipRect::new(Recti::new(0, 0, 0, 10)).is_none());
        assert!(ClipRect::new(Recti::new(0, 0, 10, 0)).is_none());
        assert!(ClipRect::new(Recti::new(0, 0, -1, 10)).is_none());
    }

    #[test]
    fn clip_edges_own_their_boundaries_and_half_planes() {
        let clip = ClipRect::new(Recti::new(10, 20, 30, 40)).expect("non-empty clip");
        let edges: Vec<_> = clip.edges().collect();

        assert_eq!(edges.len(), 4);
        assert!(edges[0].contains(Vec2f::new(10.0, 30.0)));
        assert!(!edges[0].contains(Vec2f::new(9.0, 30.0)));
        assert!(edges[1].contains(Vec2f::new(40.0, 30.0)));
        assert!(!edges[1].contains(Vec2f::new(41.0, 30.0)));
        assert!(edges[2].contains(Vec2f::new(20.0, 20.0)));
        assert!(!edges[2].contains(Vec2f::new(20.0, 19.0)));
        assert!(edges[3].contains(Vec2f::new(20.0, 60.0)));
        assert!(!edges[3].contains(Vec2f::new(20.0, 61.0)));
    }

    #[test]
    fn clipping_appends_survivors_without_clearing_caller_output() {
        let retained = Vertex::new(Vec2f::new(1.0, 1.0), Vec2f::new(0.0, 0.0), white());
        let triangle = [
            Vertex::new(Vec2f::new(20.0, 20.0), Vec2f::new(0.0, 0.0), white()),
            Vertex::new(Vec2f::new(30.0, 20.0), Vec2f::new(0.0, 0.0), white()),
            Vertex::new(Vec2f::new(20.0, 30.0), Vec2f::new(0.0, 0.0), white()),
        ];
        let clip = ClipRect::new(Recti::new(0, 0, 10, 10)).expect("non-empty clip");
        let mut output = vec![retained];

        clip.clip_triangle(triangle, &mut output);

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].position().x, retained.position().x);
        assert_eq!(output[0].position().y, retained.position().y);
    }
}
