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

//! Explicit measurement constraints and parent-owned track sizes.

use crate::Dimensioni;

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
