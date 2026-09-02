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

//! Atomic ownership of one resolved skin and the immutable atlas it references.

use crate::AtlasHandle;

use super::{IconRole, Skin};

/// One resolved [`Skin`] paired with the exact immutable [`AtlasHandle`] it uses.
///
/// Image-backed nine patches still contain low-level [`crate::IconId`] capabilities because the
/// renderer needs exact atlas rectangles. Keeping the skin and atlas behind this single concrete
/// value prevents callers and retained runtime owners from replacing either half independently.
/// Stable [`crate::FontRef`] and [`crate::IconRef`] values resolve through this same pair.
#[derive(Clone)]
pub struct SkinBundle {
    /// Immutable pixels, font metrics, and icon rectangles used by the paired skin.
    atlas: AtlasHandle,
    /// Complete resolved geometry and visuals whose image capabilities belong to `atlas`.
    skin: Skin,
}

impl SkinBundle {
    /// Pairs an already resolved skin with its exact atlas after validating the full contract.
    ///
    /// # Panics
    ///
    /// Panics when the atlas lacks a required semantic resource or an image-backed skin visual
    /// contains a capability minted by another atlas allocation.
    pub fn new(atlas: AtlasHandle, mut skin: Skin) -> Self {
        // Validate stable semantic resource names even though they are resolved lazily. This keeps
        // a successfully constructed bundle total for every built-in measure and paint operation.
        let _ = skin.resolve_font_role(&atlas, crate::FontRole::Body);
        for role in IconRole::ALL {
            let _ = role.resolve(&atlas);
        }
        // Image visuals are the only allocation-bound values retained inside Skin after stable
        // font/icon references replaced widget-owned IDs.
        assert!(skin.belongs_to(&atlas), "skin contains image capabilities from another atlas");
        // Bundle construction publishes a completed concrete skin value. Give that value a fresh
        // identity so retained caches need no partial field fingerprint or atlas-pointer key.
        skin.refresh_revision();
        Self { atlas, skin }
    }

    /// Builds the default resolved skin for `atlas` and returns the validated pair.
    ///
    /// # Panics
    ///
    /// Panics when `atlas` does not satisfy the standard semantic resource contract.
    pub fn from_atlas(atlas: AtlasHandle) -> Self {
        // Construct before moving the handle so Skin::from_atlas and the resulting bundle validate
        // the exact same immutable allocation.
        let skin = Skin::from_atlas(&atlas);
        Self::new(atlas, skin)
    }

    /// Borrows the complete resolved runtime skin.
    pub fn skin(&self) -> &Skin {
        // Return a projection of this pair rather than cloning an independently mutable copy.
        &self.skin
    }

    /// Borrows the immutable atlas paired with this skin.
    pub fn atlas(&self) -> &AtlasHandle {
        // The handle exposes immutable pixels and metadata, so a shared borrow is sufficient for
        // layout, paint, renderer replacement, and application name lookup.
        &self.atlas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, ControlRole, IconRole, NinePatch, NinePatchImage, SliceInsets};

    /// Verifies the pair rejects the only allocation-bound value still retained by a skin.
    #[test]
    #[should_panic(expected = "skin contains image capabilities from another atlas")]
    fn bundle_rejects_foreign_image_visuals() {
        let local_atlas = crate::test_support::test_atlas();
        let foreign_atlas = crate::test_support::test_atlas();
        let mut skin = Skin::from_atlas(&local_atlas);
        let foreign_icon = IconRole::Close.resolve(&foreign_atlas);
        crate::test_support::replace_control_patches(&mut skin, ControlRole::Button, |_| {
            NinePatch::image(
                SliceInsets::ZERO,
                NinePatchImage::new(foreign_icon, SliceInsets::ZERO, Color { r: 255, g: 255, b: 255, a: 255 }),
            )
        });

        // Construction is the sole public pairing boundary, so malformed ownership cannot enter a
        // LoadedTheme or WindowManager and fail later during rendering.
        let _ = SkinBundle::new(local_atlas, skin);
    }

    /// Verifies bundle clones identify one value while a new pairing starts a new generation.
    #[test]
    fn bundle_construction_assigns_one_complete_non_reused_skin_revision() {
        let atlas = crate::test_support::test_atlas();
        let first = SkinBundle::from_atlas(atlas.clone());
        let first_clone = first.clone();
        let second = SkinBundle::new(atlas, first.skin().clone());

        assert_eq!(first.skin().revision(), first_clone.skin().revision());
        assert_ne!(first.skin().revision(), second.skin().revision());
    }
}
