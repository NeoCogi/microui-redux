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

//! Allocation arithmetic shared by the built-in linear and grid containers.
//!
//! [`Widget::measure`](crate::Widget::measure) is an immutable preferred-size query. It therefore
//! cannot fill a retained vector or mutate a cache. `Axis` keeps that contract honest: it is a
//! short-lived, scalar cursor created inside one measurement or layout call. It never owns child
//! data and is never stored behind interior mutability.
//!
//! Axis allocation has two logical passes:
//!
//! 1. [`Axis::new`] summarizes the sibling policies and preferred extents. This determines the
//!    intrinsic total and the shared reference used by weighted tracks.
//! 2. For a bounded axis, the caller visits the same siblings in the same order and calls
//!    [`Axis::next`] to resolve each slot. Only scalar running state is retained between siblings.
//!    An unbounded measurement can use [`Axis::intrinsic_extent`] immediately and skip this replay.
//!
//! This type knows nothing about nodes, widgets, rows, columns, or rendering. Containers remain
//! responsible for measuring their children and for deciding which policy applies to each slot.

use crate::ui_node::sizing::{SizePolicy, scaled};

/// Per-call scalar state for resolving one ordered sibling axis.
///
/// A non-positive public measurement bound is normalized to `available == 0`, meaning
/// "unbounded/preferred size". Positive values represent a real allocation bound.
pub(super) struct Axis {
    /// Normalized allocation bound; zero denotes an unbounded preferred-size query.
    available: i32,
    /// Sum of every item's unbounded contribution, excluding inter-item spacing.
    intrinsic: i32,
    /// Sum of finite, non-negative weights used to normalize individual weighted slots.
    total_weight: f32,
    /// Pixel range shared by weighted slots after the non-weight reservation rule is applied.
    weight_reference: i32,
    /// Resolved pixels consumed by prior calls to [`Axis::next`], excluding spacing.
    used: i32,
}

/// One resolved sibling slot.
///
/// `advance` moves the parent's sibling cursor. `offered` is the extent passed to child layout.
/// They differ when child layout will apply the node policy itself: a fractional child must be
/// offered the full reference axis, and a remainder child must be offered its margin as well. This
/// prevents those policies from being applied twice while preserving the resolved sibling spacing.
#[derive(Copy, Clone, Default)]
pub(super) struct AxisSlot {
    /// Extent offered to the child layout operation.
    pub(super) offered: i32,
    /// Extent consumed before placing the next sibling.
    pub(super) advance: i32,
}

impl Axis {
    /// Summarizes one axis without retaining its item sequence.
    ///
    /// Each item is `(parent_policy, measured_preferred_extent)`. Callers that subsequently use
    /// [`Self::next`] must replay those items in the same order because `Remainder` depends on the
    /// pixels consumed by earlier siblings.
    pub(super) fn new(available: i32, items: impl IntoIterator<Item = (SizePolicy, i32)>) -> Self {
        // The public measurement convention uses zero for an unbounded axis. Negative dimensions
        // carry the same meaning, so normalize them once at this private boundary.
        let available = available.max(0);
        let mut intrinsic = 0_i32;
        let mut total_weight = 0.0;
        let mut reserved = 0_i32;
        let mut has_remainder = false;

        for (policy, preferred) in items {
            // Intrinsic size is independent of the current bound: Fixed forces its value, while
            // every flexible policy contributes the measured content preference.
            // intrinsic_total = previous_intrinsic_total + policy_intrinsic_extent.
            intrinsic = intrinsic.saturating_add(policy.intrinsic_extent(preferred));

            // Bounded weight allocation needs two aggregate facts before any individual slot can
            // be resolved: space reserved by non-weight tracks and the total valid weight.
            // reserved_total = previous_reserved_total + resolved_non_weight_extent.
            match policy {
                SizePolicy::Auto => reserved = reserved.saturating_add(preferred.max(0)),
                SizePolicy::Fixed(value) => reserved = reserved.saturating_add(value.max(0)),
                SizePolicy::Fraction(value) => reserved = reserved.saturating_add(scaled(available, value.clamp(0.0, 1.0))),
                SizePolicy::Weight(value) if value.is_finite() => total_weight += value.max(0.0),
                SizePolicy::Weight(_) => {}
                SizePolicy::Remainder(_) => has_remainder = true,
            }
        }

        Self {
            available,
            intrinsic,
            total_weight,
            // Without a remainder track, weights divide only the space left after Auto, Fixed,
            // and Fraction tracks. With a remainder track, weights use the complete reference and
            // Remainder consumes whatever is still available when its ordered turn is reached.
            // weight_reference = available_extent - reserved_extent when no remainder exists.
            weight_reference: if has_remainder { available } else { available.saturating_sub(reserved) },
            used: 0,
        }
    }

    /// Resolves the next item and advances the scalar cursor.
    ///
    /// This method performs no allocation and stores no item result. The caller consumes the
    /// returned slot immediately, which is why measurement can remain immutable and layout can
    /// place arbitrarily many children without a temporary per-child collection.
    pub(super) fn next(&mut self, policy: SizePolicy, preferred: i32) -> AxisSlot {
        let advance = if self.available == 0 {
            // In an unbounded query only Fixed overrides measured content. Fraction, Weight, and
            // Remainder have no finite reference from which to manufacture a preferred extent.
            policy.intrinsic_extent(preferred)
        } else {
            // Resolve the parent-owned slot policy exactly once against the shared axis bound.
            // fraction_extent = available_extent * clamp(fraction, 0, 1).
            // weight_extent = weight_reference * item_weight / total_weight.
            // remainder_extent = available_extent - used_extent - margin.
            match policy {
                SizePolicy::Auto => preferred.max(0),
                SizePolicy::Fixed(value) => value.max(0),
                SizePolicy::Fraction(value) => scaled(self.available, value.clamp(0.0, 1.0)),
                SizePolicy::Weight(value) if self.total_weight > 0.0 => scaled(self.weight_reference, value.max(0.0) / self.total_weight),
                SizePolicy::Weight(_) => 0,
                SizePolicy::Remainder(margin) => self.available.saturating_sub(self.used).saturating_sub(margin.max(0)),
            }
        };

        // Remainder observes this running total, so update it only after resolving the current
        // sibling and never include visual spacing in it.
        // used_extent = previous_used_extent + current_advance.
        self.used = self.used.saturating_add(advance);

        // Node layout applies its own Fraction/Remainder policy to the offered rectangle. Rebuild
        // that pre-policy offer here while keeping `advance` as the already-resolved cursor step.
        let offered = match policy {
            SizePolicy::Fraction(_) if self.available > 0 => self.available,
            // offered_extent = resolved_advance + margin.
            SizePolicy::Remainder(margin) if self.available > 0 => advance.saturating_add(margin.max(0)),
            _ => advance,
        };
        AxisSlot { offered, advance }
    }

    /// Returns the resolved item total plus the gaps between `count` siblings.
    ///
    /// Call this after replaying all bounded items through [`Self::next`]. Spacing is added here,
    /// rather than to `used`, so `Remainder` sees only space consumed by actual tracks.
    pub(super) fn extent(&self, count: usize, spacing: i32) -> i32 {
        // gap_count = item_count - 1; spacing_total = spacing * gap_count.
        let gap_count = count.saturating_sub(1) as i32;
        let spacing_total = spacing.max(0).saturating_mul(gap_count);
        // extent = used_track_extent + spacing_total.
        self.used.saturating_add(spacing_total)
    }

    /// Returns the unbounded preferred total plus the gaps between `count` siblings.
    ///
    /// This value was completed by [`Self::new`], so an immutable unbounded measurement does not
    /// need to measure and replay its children a second time merely to populate `used`.
    pub(super) fn intrinsic_extent(&self, count: usize, spacing: i32) -> i32 {
        // gap_count = item_count - 1; spacing_total = spacing * gap_count.
        let gap_count = count.saturating_sub(1) as i32;
        let spacing_total = spacing.max(0).saturating_mul(gap_count);
        // intrinsic_extent = intrinsic_track_extent + spacing_total.
        self.intrinsic.saturating_add(spacing_total)
    }
}
