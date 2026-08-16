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

//! Explicit measurement constraints and parent-owned track resolution.

use crate::Dimensioni;

pub(in crate::ui_node) mod linear;
pub use linear::{LinearItem, RowHeight};

/// Available space on one measurement axis.
///
/// This is deliberately not encoded in a pixel count: a bounded zero-sized surface and an
/// unbounded measurement request are different inputs.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AvailableSpace {
    /// The parent imposes no maximum on this axis.
    Unbounded,
    /// The parent supplies a non-negative maximum extent.
    Bounded(i32),
}

impl AvailableSpace {
    /// Creates a bounded axis, normalizing an invalid negative extent at the public boundary.
    pub const fn bounded(extent: i32) -> Self {
        Self::Bounded(if extent < 0 { 0 } else { extent })
    }

    /// Returns the finite extent when this axis is bounded.
    pub const fn bound(self) -> Option<i32> {
        match self {
            Self::Unbounded => None,
            Self::Bounded(extent) => Some(extent),
        }
    }

    /// Removes a non-negative parent-owned inset while preserving an unbounded axis.
    pub const fn shrink(self, inset: i32) -> Self {
        let inset = if inset < 0 { 0 } else { inset };
        match self {
            Self::Unbounded => Self::Unbounded,
            Self::Bounded(extent) => Self::Bounded(extent.saturating_sub(inset)),
        }
    }
}

/// Independent width and height constraints supplied during measurement.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Constraints {
    /// Horizontal measurement space.
    pub width: AvailableSpace,
    /// Vertical measurement space.
    pub height: AvailableSpace,
}

impl Constraints {
    /// Creates explicit axis constraints.
    pub const fn new(width: AvailableSpace, height: AvailableSpace) -> Self {
        Self { width, height }
    }

    /// Creates an unconstrained preferred-size query.
    pub const fn unbounded() -> Self {
        Self::new(AvailableSpace::Unbounded, AvailableSpace::Unbounded)
    }

    /// Creates a measurement constrained to a non-negative maximum size.
    pub const fn bounded(size: Dimensioni) -> Self {
        Self::new(AvailableSpace::bounded(size.width), AvailableSpace::bounded(size.height))
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Self::unbounded()
    }
}

/// Size of one parent-owned linear or grid track.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum TrackSize {
    /// Uses the measured content extent.
    #[default]
    Content,
    /// Uses an exact non-negative pixel extent.
    Fixed(i32),
    /// Receives a weighted share of bounded space left after content, fixed tracks, and gaps.
    /// Under an unbounded constraint it contributes its measured content extent.
    Flex(f32),
}

/// Replayable resolution of one ordered set of parent-owned tracks.
///
/// Construction summarizes reservations and flex weights without retaining the input sequence.
/// Callers then replay the same `(track, content)` pairs through [`Self::next`]. This keeps
/// immutable measurement allocation-free while giving placement exact per-child extents.
pub(super) struct TrackResolver {
    available: AvailableSpace,
    flexible_space: i32,
    total_flex: f64,
    cumulative_flex: f64,
    distributed_flex: i32,
    extent: i32,
}

impl TrackResolver {
    /// Summarizes an ordered track axis and computes its complete extent including gaps.
    pub(super) fn new(available: AvailableSpace, gap: i32, count: usize, items: impl IntoIterator<Item = (TrackSize, i32)>) -> Self {
        let total_gap = gap.max(0).saturating_mul(count.saturating_sub(1) as i32);
        let mut reserved = 0_i32;
        let mut total_flex = 0.0_f64;
        for (track, content) in items {
            match track {
                TrackSize::Content => reserved = reserved.saturating_add(content.max(0)),
                TrackSize::Fixed(value) => reserved = reserved.saturating_add(value.max(0)),
                TrackSize::Flex(weight) if matches!(available, AvailableSpace::Bounded(_)) => {
                    total_flex += valid_weight(weight);
                }
                TrackSize::Flex(_) => reserved = reserved.saturating_add(content.max(0)),
            }
        }

        // Remaining bounded space belongs to the sequence only when a valid flex track can receive
        // it. Content-only and fixed-only sequences retain their desired extent under larger bounds.
        let flexible_space = if total_flex > 0.0 {
            available
                .bound()
                .map(|bound| bound.saturating_sub(total_gap).saturating_sub(reserved).max(0))
                .unwrap_or(0)
        } else {
            0
        };
        // The summary is exactly the span produced by replay: reserved pixels, distributed flex
        // pixels, and gaps. Invalid flex tracks cannot claim otherwise unowned bounded remainder.
        let extent = reserved.saturating_add(flexible_space).saturating_add(total_gap);
        Self {
            available,
            flexible_space,
            total_flex,
            cumulative_flex: 0.0,
            distributed_flex: 0,
            extent,
        }
    }

    /// Resolves the next replayed track to one exact non-negative extent.
    pub(super) fn next(&mut self, track: TrackSize, content: i32) -> i32 {
        match (self.available, track) {
            (_, TrackSize::Content) => content.max(0),
            (_, TrackSize::Fixed(value)) => value.max(0),
            (AvailableSpace::Unbounded, TrackSize::Flex(_)) => content.max(0),
            (AvailableSpace::Bounded(_), TrackSize::Flex(weight)) => {
                let weight = valid_weight(weight);
                if weight == 0.0 || self.total_flex == 0.0 {
                    return 0;
                }
                self.cumulative_flex += weight;
                // Cumulative ceilings assign indivisible pixels to earlier flex tracks and make
                // the final positive flex track end exactly at `flexible_space`.
                let cumulative_pixels =
                    ((f64::from(self.flexible_space) * self.cumulative_flex / self.total_flex).ceil() as i64).clamp(0, i64::from(self.flexible_space)) as i32;
                let extent = cumulative_pixels.saturating_sub(self.distributed_flex);
                self.distributed_flex = cumulative_pixels;
                extent
            }
        }
    }

    /// Returns the exact resolved track span including gaps.
    pub(super) const fn extent(&self) -> i32 {
        self.extent
    }
}

/// Resolves measured content extents in place and returns their complete track span.
///
/// Grid uses this slice form with retained placement buffers; linear layout replays
/// [`TrackResolver`] directly into its own retained extent buffer.
pub(super) fn resolve_tracks_in_place(available: AvailableSpace, gap: i32, tracks: &[TrackSize], content: &mut [i32]) -> i32 {
    // Normalize caller-owned measurements before the resolver summarizes or overwrites them.
    content.iter_mut().for_each(|extent| *extent = (*extent).max(0));
    let mut resolver = TrackResolver::new(
        available,
        gap,
        content.len(),
        content.iter().enumerate().map(|(index, extent)| (track_at(tracks, index), *extent)),
    );
    for (index, extent) in content.iter_mut().enumerate() {
        *extent = resolver.next(track_at(tracks, index), *extent);
    }
    resolver.extent()
}

/// Returns explicit track metadata or the content-sized default for an omitted index.
fn track_at(tracks: &[TrackSize], index: usize) -> TrackSize {
    tracks.get(index).copied().unwrap_or(TrackSize::Content)
}

/// Converts a finite positive public flex weight into the solver's accumulation precision.
fn valid_weight(weight: f32) -> f64 {
    if weight.is_finite() && weight > 0.0 { f64::from(weight) } else { 0.0 }
}

#[cfg(test)]
mod explicit_constraint_tests {
    use super::*;

    #[test]
    fn bounded_constructor_preserves_zero_and_normalizes_negative_extents() {
        assert_eq!(AvailableSpace::bounded(0), AvailableSpace::Bounded(0));
        assert_eq!(AvailableSpace::bounded(-7), AvailableSpace::Bounded(0));
    }

    #[test]
    fn constraints_keep_axis_bounds_independent() {
        let constraints = Constraints::new(AvailableSpace::Unbounded, AvailableSpace::Bounded(24));
        assert_eq!(constraints.width.bound(), None);
        assert_eq!(constraints.height.bound(), Some(24));
    }
}

#[cfg(test)]
mod track_resolution_tests {
    use super::*;

    #[test]
    fn unbounded_tracks_use_content_except_for_fixed_extents() {
        let mut content = [10, 20, 30, 40];
        let extent = resolve_tracks_in_place(
            AvailableSpace::Unbounded,
            2,
            &[TrackSize::Content, TrackSize::Fixed(7), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [10, 7, 30, 40]);
        assert_eq!(extent, 93);
    }

    #[test]
    fn bounded_tracks_reserve_content_fixed_and_gaps_before_flex() {
        let mut content = [10, 99, 99];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(40),
            2,
            &[TrackSize::Content, TrackSize::Fixed(8), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [10, 8, 18]);
        assert_eq!(extent, 40);
    }

    #[test]
    fn bounded_content_and_fixed_tracks_do_not_claim_unused_space() {
        // A bound informs responsive measurement; without a valid flex recipient it is not itself
        // part of the track sequence's desired size.
        let mut content = [10, 99];
        let extent = resolve_tracks_in_place(AvailableSpace::bounded(100), 2, &[TrackSize::Content, TrackSize::Fixed(8)], &mut content);

        assert_eq!(content, [10, 8]);
        assert_eq!(extent, 20);
    }

    #[test]
    fn invalid_flex_weights_do_not_create_phantom_extent() {
        // Invalid bounded flex tracks resolve to zero. Their surrounding gaps remain real, but the
        // unused bounded remainder has no owner and must stay outside the reported content span.
        let mut content = [10, 20, 30];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(100),
            2,
            &[TrackSize::Content, TrackSize::Flex(0.0), TrackSize::Flex(f32::NAN)],
            &mut content,
        );

        assert_eq!(content, [10, 0, 0]);
        assert_eq!(extent, 14);
    }

    #[test]
    fn rounding_pixels_are_deterministic_and_keep_the_exact_bound() {
        let mut content = [0; 3];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(10),
            0,
            &[TrackSize::Flex(1.0), TrackSize::Flex(1.0), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [4, 3, 3]);
        assert_eq!(extent, 10);
    }

    #[test]
    fn content_overflow_does_not_shrink_or_overlap_tracks() {
        let mut content = [20, 30, 50];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(40),
            3,
            &[TrackSize::Content, TrackSize::Fixed(30), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [20, 30, 0]);
        assert_eq!(extent, 56);
    }

    #[test]
    fn bounded_zero_is_not_unbounded() {
        let mut content = [12];
        let extent = resolve_tracks_in_place(AvailableSpace::bounded(0), 0, &[TrackSize::Flex(1.0)], &mut content);
        assert_eq!(content, [0]);
        assert_eq!(extent, 0);
    }
}
