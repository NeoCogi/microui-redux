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

/// Describes the interface the atlas uses to query font metadata.
pub trait Font {
    /// Returns the font's display name.
    fn name(&self) -> &str;
    /// Returns the base pixel size of the font.
    fn get_size(&self) -> usize;
    /// Returns the pixel width and height for a specific character.
    fn get_char_size(&self, c: char) -> (usize, usize);
}
