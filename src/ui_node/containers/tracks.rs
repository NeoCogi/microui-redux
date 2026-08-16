//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without modification, are permitted
// provided that the conditions in the project LICENSE are met.
//

//! Pure allocation of parent-owned linear and grid tracks.

use crate::{AvailableSpace, TrackSize};

/// Replayable resolution of one ordered set of tracks.
///
/// Construction summarizes reservations and flex weights without retaining the input sequence.
/// Callers then replay the same `(track, content)` pairs through [`Self::next`]. This keeps
/// immutable measurement allocation-free and gives layout exact per-child extents without a
/// second policy application in the runtime.
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

        let flexible_space = available
            .bound()
            .map(|bound| bound.saturating_sub(total_gap).saturating_sub(reserved).max(0))
            .unwrap_or(0);
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

/// Replaces measured content extents with resolved track extents and returns their complete span.
///
/// This slice helper is retained for Grid's reusable placement buffers and direct solver tests.
pub(super) fn resolve_tracks(available: AvailableSpace, gap: i32, tracks: &[TrackSize], content: &mut [i32]) -> i32 {
    content.iter_mut().for_each(|extent| *extent = (*extent).max(0));
    let mut resolver = TrackResolver::new(
        available,
        gap,
        content.len(),
        content.iter().enumerate().map(|(index, extent)| (track(tracks, index), *extent)),
    );
    for (index, extent) in content.iter_mut().enumerate() {
        *extent = resolver.next(track(tracks, index), *extent);
    }
    resolver.extent()
}

fn track(tracks: &[TrackSize], index: usize) -> TrackSize {
    tracks.get(index).copied().unwrap_or(TrackSize::Content)
}

fn valid_weight(weight: f32) -> f64 {
    if weight.is_finite() && weight > 0.0 { f64::from(weight) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbounded_tracks_use_content_except_for_fixed_extents() {
        let mut content = [10, 20, 30, 40];
        let extent = resolve_tracks(
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
        let extent = resolve_tracks(
            AvailableSpace::bounded(40),
            2,
            &[TrackSize::Content, TrackSize::Fixed(8), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [10, 8, 18]);
        assert_eq!(extent, 40);
    }

    #[test]
    fn rounding_pixels_are_deterministic_and_keep_the_exact_bound() {
        let mut content = [0; 3];
        let extent = resolve_tracks(
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
        let extent = resolve_tracks(
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
        let extent = resolve_tracks(AvailableSpace::bounded(0), 0, &[TrackSize::Flex(1.0)], &mut content);
        assert_eq!(content, [0]);
        assert_eq!(extent, 0);
    }
}
