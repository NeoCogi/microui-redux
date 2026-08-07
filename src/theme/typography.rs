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

//! Semantic font selection for UI styles.

use crate::atlas::FontId;

use super::Style;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
/// Semantic font roles used by the built-in widgets and default style.
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
    /// Returns the conventional atlas font name used by [`Style::bind_named_fonts`].
    pub fn atlas_name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Small => "small",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Mono => "mono",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Selects either a semantic font role from [`Style`] or a specific [`FontId`].
pub enum FontChoice {
    /// Resolve through a [`FontRole`] stored on the style.
    Role(FontRole),
    /// Use the provided concrete font directly.
    Id(FontId),
}

impl Default for FontChoice {
    fn default() -> Self {
        Self::Role(FontRole::Body)
    }
}

impl From<FontRole> for FontChoice {
    fn from(role: FontRole) -> Self {
        Self::Role(role)
    }
}

impl From<FontId> for FontChoice {
    fn from(font: FontId) -> Self {
        Self::Id(font)
    }
}

impl FontChoice {
    /// Creates a semantic font selection.
    pub fn role(role: FontRole) -> Self {
        Self::Role(role)
    }

    /// Creates a concrete font selection.
    pub fn id(font: FontId) -> Self {
        Self::Id(font)
    }

    /// Resolves this choice against `style`.
    pub fn resolve(self, style: &Style) -> FontId {
        style.resolve_font_choice(self)
    }
}
