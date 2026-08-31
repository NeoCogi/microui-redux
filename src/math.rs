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

//! Lightweight geometry conveniences shared across retained layout and rendering.

use rs_math3d::{Recti, Vec2f, Vec2i};

/// Clamps one wider geometry calculation into the crate's signed coordinate domain.
pub(crate) fn clamp_i64_to_i32(value: i64) -> i32 {
    // Multi-term expressions should retain cancellation in i64 and narrow exactly once at the
    // Recti/Vec2i boundary. A cast would wrap values outside the destination range.
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Internal rectangle operations not yet supplied by `rs-math3d`.
pub(crate) trait RectExt: Sized {
    /// Expands the rectangle uniformly on all sides with saturated coordinates and extents.
    fn expanded(self, amount: i32) -> Self;

    /// Translates the rectangle while saturating its origin at the integer bounds.
    fn translated(self, offset: Vec2i) -> Self;

    /// Converts the rectangle into coordinates relative to an origin with saturated subtraction.
    fn relative_to(self, origin: Vec2i) -> Self;

    /// Returns whether both rectangle extents are positive.
    fn has_positive_area(self) -> bool;

    /// Returns whether a point lies inside the positive rectangle using overflow-safe edges.
    fn contains_point(self, point: Vec2i) -> bool;

    /// Returns the positive-area portion shared with another rectangle.
    fn positive_intersection(self, other: Self) -> Option<Self>;

    /// Returns whether two positive-area rectangles overlap.
    fn overlaps(self, other: Self) -> bool;

    /// Returns the smallest rectangle containing both input rectangles.
    fn union(self, other: Self) -> Self;

    /// Returns conservative integer bounds around finite floating-point positions.
    fn from_points(points: &[Vec2f]) -> Option<Self>;

    /// Returns conservative integer bounds around a finite thick line.
    fn from_thick_line(from: Vec2f, to: Vec2f, width: f32) -> Option<Self>;
}

impl RectExt for Recti {
    fn expanded(self, amount: i32) -> Self {
        let extent = amount.saturating_mul(2);
        Self::new(
            self.x.saturating_sub(amount),
            self.y.saturating_sub(amount),
            self.width.saturating_add(extent),
            self.height.saturating_add(extent),
        )
    }

    fn translated(self, offset: Vec2i) -> Self {
        Self::new(self.x.saturating_add(offset.x), self.y.saturating_add(offset.y), self.width, self.height)
    }

    fn relative_to(self, origin: Vec2i) -> Self {
        Self::new(self.x.saturating_sub(origin.x), self.y.saturating_sub(origin.y), self.width, self.height)
    }

    fn has_positive_area(self) -> bool {
        self.width > 0 && self.height > 0
    }

    fn contains_point(self, point: Vec2i) -> bool {
        if !self.has_positive_area() {
            return false;
        }
        let x = i64::from(point.x);
        let y = i64::from(point.y);
        let left = i64::from(self.x);
        let top = i64::from(self.y);
        // Exclusive edges live in i64 so a positive extent at i32::MAX remains queryable.
        x >= left && x < left + i64::from(self.width) && y >= top && y < top + i64::from(self.height)
    }

    fn positive_intersection(self, other: Self) -> Option<Self> {
        if !self.has_positive_area() || !other.has_positive_area() {
            return None;
        }

        // Recti permits an origin at i32::MAX together with a positive extent, so computing an
        // exclusive edge in i32 can overflow even though both rectangles are individually valid.
        // Wider intermediates make clipping total for renderer geometry derived from untrusted
        // atlas metrics; the resulting origin and extent are bounded by the input i32 values.
        let left = i64::from(self.x).max(i64::from(other.x));
        let top = i64::from(self.y).max(i64::from(other.y));
        let right = (i64::from(self.x) + i64::from(self.width)).min(i64::from(other.x) + i64::from(other.width));
        let bottom = (i64::from(self.y) + i64::from(self.height)).min(i64::from(other.y) + i64::from(other.height));
        if right <= left || bottom <= top {
            return None;
        }

        Some(Self::new(left as i32, top as i32, (right - left) as i32, (bottom - top) as i32))
    }

    fn overlaps(self, other: Self) -> bool {
        self.positive_intersection(other).is_some()
    }

    fn union(self, other: Self) -> Self {
        let min_x = i64::from(self.x.min(other.x));
        let min_y = i64::from(self.y.min(other.y));
        let max_x = (i64::from(self.x) + i64::from(self.width)).max(i64::from(other.x) + i64::from(other.width));
        let max_y = (i64::from(self.y) + i64::from(self.height)).max(i64::from(other.y) + i64::from(other.height));
        // Recti cannot represent a union wider than i32::MAX. Preserve the leading origin and
        // saturate the extent instead of wrapping to a negative layout rectangle.
        let width = (max_x - min_x).clamp(0, i64::from(i32::MAX)) as i32;
        let height = (max_y - min_y).clamp(0, i64::from(i32::MAX)) as i32;
        Self::new(min_x as i32, min_y as i32, width, height)
    }

    fn from_points(points: &[Vec2f]) -> Option<Self> {
        let first = *points.first()?;
        if !point_is_finite(first) {
            return None;
        }

        let mut min = first;
        let mut max = first;
        for point in points.iter().copied().skip(1) {
            if !point_is_finite(point) {
                return None;
            }
            min = Vec2f::new(min.x.min(point.x), min.y.min(point.y));
            max = Vec2f::new(max.x.max(point.x), max.y.max(point.y));
        }

        let x0 = min.x.floor() as i32;
        let y0 = min.y.floor() as i32;
        let x1 = max.x.ceil() as i32;
        let y1 = max.y.ceil() as i32;
        let width = (i64::from(x1) - i64::from(x0)).clamp(0, i64::from(i32::MAX)) as i32;
        let height = (i64::from(y1) - i64::from(y0)).clamp(0, i64::from(i32::MAX)) as i32;
        Some(Self::new(x0, y0, width, height))
    }

    fn from_thick_line(from: Vec2f, to: Vec2f, width: f32) -> Option<Self> {
        if !width.is_finite() || width <= 0.0 || !point_is_finite(from) || !point_is_finite(to) {
            return None;
        }

        let half = width * 0.5;
        let min = Vec2f::new(from.x.min(to.x) - half, from.y.min(to.y) - half);
        let max = Vec2f::new(from.x.max(to.x) + half, from.y.max(to.y) + half);
        Self::from_points(&[min, max])
    }
}

/// Returns whether both point components are finite.
fn point_is_finite(point: Vec2f) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// Convenience constructor for [`Vec2i`].
pub fn vec2(x: i32, y: i32) -> Vec2i {
    Vec2i { x, y }
}

/// Convenience constructor for [`Recti`].
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> Recti {
    Recti { x, y, width: w, height: h }
}

/// Expands (or shrinks) a rectangle uniformly on all sides.
pub fn expand_rect(rectangle: Recti, amount: i32) -> Recti {
    rectangle.expanded(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rect(actual: Recti, expected: (i32, i32, i32, i32)) {
        assert_eq!((actual.x, actual.y, actual.width, actual.height), expected);
    }

    #[test]
    fn rectangle_transforms_preserve_extents() {
        let source = Recti::new(10, 20, 30, 40);

        assert_rect(source.expanded(2), (8, 18, 34, 44));
        assert_rect(source.translated(Vec2i::new(-3, 5)), (7, 25, 30, 40));
        assert_rect(source.relative_to(Vec2i::new(3, 5)), (7, 15, 30, 40));
        assert_rect(
            Recti::new(i32::MAX - 1, i32::MIN + 1, 30, 40).translated(Vec2i::new(10, -10)),
            (i32::MAX, i32::MIN, 30, 40),
        );
    }

    #[test]
    fn rectangle_queries_require_positive_shared_area() {
        let left = Recti::new(0, 0, 10, 10);
        let overlapping = Recti::new(5, 4, 10, 3);
        let touching = Recti::new(10, 0, 4, 4);

        assert!(left.has_positive_area());
        assert!(!Recti::new(0, 0, 0, 10).has_positive_area());
        assert_rect(left.positive_intersection(overlapping).expect("rectangles overlap"), (5, 4, 5, 3));
        assert!(left.overlaps(overlapping));
        assert!(!left.overlaps(touching));
        assert!(left.positive_intersection(touching).is_none());
        assert_rect(left.union(overlapping), (0, 0, 15, 10));
        assert!(left.contains_point(Vec2i::new(9, 9)));
        assert!(!left.contains_point(Vec2i::new(10, 10)));
    }

    /// Verifies rectangle queries form exclusive edges in wider arithmetic at coordinate limits.
    #[test]
    fn rectangle_intersection_handles_exclusive_edges_beyond_i32() {
        let extreme = Recti::new(i32::MAX, i32::MIN, 1, 2);

        // An identical rectangle still intersects even though its mathematical right edge is one
        // greater than i32::MAX. A normal viewport remains disjoint without evaluating x + width
        // in the narrower coordinate type.
        assert_rect(
            extreme.positive_intersection(extreme).expect("identical extreme rectangles overlap"),
            (i32::MAX, i32::MIN, 1, 2),
        );
        assert!(extreme.positive_intersection(Recti::new(0, 0, 32, 32)).is_none());
        assert!(extreme.contains_point(Vec2i::new(i32::MAX, i32::MIN)));
    }

    #[test]
    fn floating_geometry_produces_conservative_integer_bounds() {
        assert_rect(
            Recti::from_points(&[Vec2f::new(0.7, -1.2), Vec2f::new(1.2, 3.1)]).expect("finite points have bounds"),
            (0, -2, 2, 6),
        );
        assert!(Recti::from_points(&[]).is_none());
        assert!(Recti::from_points(&[Vec2f::new(f32::NAN, 0.0)]).is_none());
        assert_rect(
            Recti::from_thick_line(Vec2f::new(1.0, 2.0), Vec2f::new(5.0, 4.0), 2.0).expect("finite thick line has bounds"),
            (0, 1, 6, 4),
        );
        assert!(Recti::from_thick_line(Vec2f::new(0.0, 0.0), Vec2f::new(1.0, 1.0), 0.0).is_none());
    }
}
