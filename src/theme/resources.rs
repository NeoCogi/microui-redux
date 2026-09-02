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

//! Immutable application resources from which every runtime skin is derived.

use crate::AtlasHandle;

use super::{FontRef, IconRef, SkinBundle};

/// The stable font and icon catalog supplied by the renderer when a context is created.
///
/// A [`crate::Context`] never replaces this catalog when it installs a [`SkinBundle`]. Theme
/// loaders consequently rebuild from the same application-owned inputs every time instead of
/// treating the previously selected theme's private artwork as new source material. Named
/// references created here remain meaningful for every correctly derived theme bundle.
#[derive(Clone)]
pub struct ResourceCatalog {
    /// Original immutable atlas containing application fonts and icons but no later theme assets.
    atlas: AtlasHandle,
}

impl ResourceCatalog {
    /// Captures the renderer's original immutable application-resource atlas.
    ///
    /// This constructor deliberately retains one concrete [`AtlasHandle`]; it does not erase
    /// resource types or accept an open-ended value bag. Creating a default skin bundle later
    /// validates the required semantic `body` font and built-in icon names.
    pub fn new(atlas: AtlasHandle) -> Self {
        // AtlasHandle has already crossed the shared structural validation boundary. Keeping its
        // allocation alive is sufficient to make this catalog immutable and independent of the
        // backend's subsequently active atlas.
        Self { atlas }
    }

    /// Returns a stable named font reference when this catalog contains `name`.
    pub fn font_ref(&self, name: &str) -> Option<FontRef> {
        // Validate the spelling now, but retain only the name. The short-lived concrete FontId is
        // intentionally discarded because a derived theme owns a different atlas allocation.
        self.atlas.font_id(name).map(|_| FontRef::named(name.to_owned()))
    }

    /// Returns a stable named icon reference when this catalog contains `name`.
    pub fn icon_ref(&self, name: &str) -> Option<IconRef> {
        // As with fonts, catalog membership is checked against the source allocation while the
        // returned reference remains allocation-independent until measurement or paint.
        self.atlas.icon_id(name).map(|_| IconRef::named(name.to_owned()))
    }

    /// Builds the standard skin paired with this catalog's exact atlas.
    ///
    /// # Panics
    ///
    /// Panics when the catalog lacks the required `body` font or a built-in semantic icon.
    pub fn default_skin_bundle(&self) -> SkinBundle {
        // Clone only the immutable handle. SkinBundle performs the complete semantic-resource and
        // image-capability validation before the pair can become active.
        SkinBundle::from_atlas(self.atlas.clone())
    }

    /// Borrows the pristine source atlas for internal deterministic theme derivation.
    #[cfg(feature = "theme-json")]
    pub(crate) fn atlas(&self) -> &AtlasHandle {
        // Restrict raw source-atlas access to this crate so applications cannot accidentally draw
        // its allocation-bound IDs after another bundle has become active.
        &self.atlas
    }
}

#[cfg(test)]
mod tests {
    //! Stable resource-catalog construction and lookup tests.

    use super::*;

    /// Verifies catalog lookups return typed stable references and reject unknown names.
    #[test]
    fn named_references_are_created_only_for_catalog_members() {
        let catalog = ResourceCatalog::new(crate::test_support::test_atlas());
        let bundle = catalog.default_skin_bundle();

        let body = catalog.font_ref("body").expect("the fixture catalog must expose its body font");
        let close = catalog.icon_ref("close").expect("the fixture catalog must expose its close icon");

        assert_eq!(body.resolve(bundle.skin(), bundle.atlas()), bundle.atlas().font_id("body").unwrap());
        assert_eq!(close.resolve(bundle.atlas()), bundle.atlas().icon_id("close").unwrap());
        assert!(catalog.font_ref("missing-font").is_none());
        assert!(catalog.icon_ref("missing-icon").is_none());
    }
}
