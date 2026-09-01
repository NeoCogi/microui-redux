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

//! Context-owned external texture handles.

use crate::identity::ProcessUniqueId;
use rs_math3d::Dimensioni;

/// Concrete identity assigned once to one context-owned render executor.
///
/// Keeping this wrapper distinct from other process-unique owners prevents an atlas, retained
/// surface, or unrelated registry identity from being used as texture provenance inside the crate.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub(crate) struct RendererId(
    /// Shared non-reused process identity hidden behind the executor-specific type boundary.
    ProcessUniqueId,
);

impl RendererId {
    /// Allocates the identity retained by one newly constructed Context executor.
    pub(crate) fn allocate() -> Self {
        // Context executor construction is the sole allocation boundary. Every later texture
        // copies this value, so provenance never depends on a backend address or local counter.
        Self(ProcessUniqueId::allocate())
    }
}

/// Handle referencing an external texture managed by one Context.
///
/// Equality and hashing include the owning Context executor, its local allocation slot, and the
/// immutable dimensions carried by the handle. Two contexts can therefore issue the same local
/// slot without either accepting, drawing, or destroying the other's texture.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureId {
    /// Process-unique identity of the Context executor that created this texture.
    renderer: RendererId,
    /// Monotonically allocated slot meaningful only within `renderer`.
    slot: u32,
    /// Texture width in pixels.
    width: i32,
    /// Texture height in pixels.
    height: i32,
}

impl TextureId {
    /// Creates one Context-owned texture capability with immutable dimensions.
    pub(crate) fn new(renderer: RendererId, slot: u32, width: i32, height: i32) -> Self {
        // Only the private executor calls this production constructor after validating dimensions
        // and before transferring the complete capability to its backend.
        Self { renderer, slot, width, height }
    }

    /// Returns the texture width in pixels.
    pub fn width(self) -> i32 {
        self.width
    }

    /// Returns the texture height in pixels.
    pub fn height(self) -> i32 {
        self.height
    }

    /// Returns the texture dimensions in pixels.
    pub fn size(self) -> Dimensioni {
        Dimensioni::new(self.width, self.height)
    }

    /// Creates an isolated opaque texture capability for unit tests without a Context.
    #[cfg(test)]
    pub(crate) fn new_test(slot: u32, width: i32, height: i32) -> Self {
        // A fresh owner prevents synthetic handles from accidentally comparing equal across tests;
        // tests that exercise equality construct several IDs from one explicit RendererId instead.
        Self::new(RendererId::allocate(), slot, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Verifies every component of an opaque texture capability participates in identity.
    #[test]
    fn identity_includes_renderer_slot_and_immutable_dimensions() {
        let renderer = RendererId::allocate();
        let other_renderer = RendererId::allocate();
        let texture = TextureId::new(renderer, 7, 32, 16);
        let same = TextureId::new(renderer, 7, 32, 16);
        let foreign = TextureId::new(other_renderer, 7, 32, 16);
        let different_slot = TextureId::new(renderer, 8, 32, 16);
        let different_width = TextureId::new(renderer, 7, 64, 16);
        let different_height = TextureId::new(renderer, 7, 32, 8);

        assert_eq!(texture, same);
        assert_ne!(texture, foreign);
        assert_ne!(texture, different_slot);
        assert_ne!(texture, different_width);
        assert_ne!(texture, different_height);

        let textures = HashSet::from([texture]);
        assert!(textures.contains(&same));
        assert!(!textures.contains(&foreign));
        assert!(!textures.contains(&different_slot));
        assert!(!textures.contains(&different_width));
        assert!(!textures.contains(&different_height));
    }
}
