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

//! Stable semantic and named font references for retained UI state.

use std::sync::Arc;

use crate::atlas::{AtlasHandle, FontId};

use super::Skin;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
/// Semantic font roles used by built-in widgets and skins.
pub enum FontRole {
    /// Default body text used by most widgets.
    #[default]
    Body,
    /// Compact supporting text.
    Small,
    /// Window titles and similar chrome text.
    Title,
    /// Larger display text.
    Heading,
    /// Monospace-style text.
    Mono,
}

impl FontRole {
    /// Every semantic font role in declaration order.
    pub const ALL: [Self; 5] = [Self::Body, Self::Small, Self::Title, Self::Heading, Self::Mono];

    /// Returns the conventional atlas font name resolved by [`Skin::from_atlas`].
    pub const fn atlas_name(self) -> &'static str {
        // Exhaustive matching keeps the typed role catalog and atlas spellings synchronized when
        // theme loaders need to replace only these semantic entries.
        match self {
            Self::Body => "body",
            Self::Small => "small",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Mono => "mono",
        }
    }
}

/// Stable reference to a font used by retained UI state.
///
/// Unlike [`FontId`], this value does not belong to one atlas allocation. A retained widget can
/// therefore keep it while the active skin and atlas are replaced together. Semantic roles let a
/// skin choose its standard typography, while named references address application fonts copied
/// into every derived theme atlas.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FontRef {
    /// Resolves through one standard semantic font role.
    Role(FontRole),
    /// Resolves one exact atlas font name owned by the application resource catalog.
    Named(Arc<str>),
}

impl Default for FontRef {
    /// Selects ordinary body typography for a newly constructed widget.
    fn default() -> Self {
        // Body is the only required semantic font and is therefore the unsurprising default.
        Self::Role(FontRole::Body)
    }
}

impl From<FontRole> for FontRef {
    /// Converts a semantic role without resolving it against the current atlas.
    fn from(role: FontRole) -> Self {
        // Preserve the role so a later skin replacement can select its corresponding font.
        Self::Role(role)
    }
}

impl FontRef {
    /// Creates a semantic font selection.
    pub fn role(role: FontRole) -> Self {
        // Keep this named constructor beside `named` so call sites never construct variants only
        // to communicate whether a resource is semantic or application-owned.
        Self::Role(role)
    }

    /// Creates a stable reference to one exact atlas font name.
    pub fn named(name: impl Into<Arc<str>>) -> Self {
        let name = name.into();
        // Empty names cannot identify atlas entries and would postpone an obvious configuration
        // error until layout. Reject them at the retained-state construction boundary instead.
        assert!(!name.is_empty(), "font reference name must not be empty");
        Self::Named(name)
    }

    /// Resolves this stable reference into a capability owned by `atlas`.
    ///
    /// # Panics
    ///
    /// Panics when a named font is absent or when the atlas violates the required `body` font
    /// contract used for optional semantic-role fallback.
    pub fn resolve(&self, skin: &Skin, atlas: &AtlasHandle) -> FontId {
        // Skin owns semantic selection policy; exact names bypass roles but still resolve only at
        // the short-lived layout or paint boundary where the matching atlas is available.
        match self {
            Self::Role(role) => skin.resolve_font_role(atlas, *role),
            Self::Named(name) => atlas.font_id(name).unwrap_or_else(|| panic!("atlas does not contain referenced font `{name}`")),
        }
    }
}
