//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without modification, are permitted
// provided that the conditions in the project LICENSE are met.
//

//! Pure allocation of parent-owned linear and grid tracks.

use crate::{AvailableSpace, TrackSize};

/// Replaces measured content extents with resolved track extents and returns their complete span.
///
/// `content` determines the number of tracks. Missing declarations are `Content`; extra
/// declarations are ignored. Gaps are outside tracks and are deducted exactly once.
pub(super) fn resolve_tracks(available: AvailableSpace, gap: i32, tracks: &[TrackSize], content: &mut [i32]) -> i32 {
    let gap = gap.max(0);
    let gap_count = content.len().saturating_sub(1) as i32;
    let total_gap = gap.saturating_mul(gap_count);
    content.iter_mut().for_each(|extent| *extent = (*extent).max(0));

    let Some(bound) = available.bound() else {
        for (index, extent) in content.iter_mut().enumerate() {
            if let TrackSize::Fixed(value) = track(tracks, index) {
                *extent = value.max(0);
            }
        }
        return sum(content).saturating_add(total_gap);
    };

    let usable = bound.saturating_sub(total_gap).max(0);
    let mut reserved = 0_i32;
    let mut total_flex = 0.0_f64;
    for (index, extent) in content.iter_mut().enumerate() {
        match track(tracks, index) {
            TrackSize::Content => reserved = reserved.saturating_add(*extent),
            TrackSize::Fixed(value) => {
                *extent = value.max(0);
                reserved = reserved.saturating_add(*extent);
            }
            TrackSize::Flex(weight) => {
                *extent = 0;
                total_flex += valid_weight(weight);
            }
        }
    }

    let flexible_space = usable.saturating_sub(reserved).max(0);
    if flexible_space > 0 && total_flex > 0.0 {
        // Cumulative ceilings assign indivisible pixels to earlier tracks and guarantee that the
        // final positive flex track ends exactly at `flexible_space`.
        let mut cumulative_weight = 0.0_f64;
        let mut allocated = 0_i32;
        for (index, extent) in content.iter_mut().enumerate() {
            let TrackSize::Flex(weight) = track(tracks, index) else { continue };
            let weight = valid_weight(weight);
            if weight == 0.0 {
                continue;
            }
            cumulative_weight += weight;
            let cumulative_pixels = ((f64::from(flexible_space) * cumulative_weight / total_flex).ceil() as i64).clamp(0, i64::from(flexible_space)) as i32;
            *extent = cumulative_pixels.saturating_sub(allocated);
            allocated = cumulative_pixels;
        }
    }

    sum(content).saturating_add(total_gap)
}

fn track(tracks: &[TrackSize], index: usize) -> TrackSize {
    tracks.get(index).copied().unwrap_or(TrackSize::Content)
}

fn valid_weight(weight: f32) -> f64 {
    if weight.is_finite() && weight > 0.0 { f64::from(weight) } else { 0.0 }
}

fn sum(extents: &[i32]) -> i32 {
    extents.iter().fold(0_i32, |total, extent| total.saturating_add(*extent))
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
