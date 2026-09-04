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

//! Texture atlas handles, baked icon/font metadata, and construction helpers.
//!
//! Text is indexed by Rust [`char`] values. The built-in `builder` module bakes printable ASCII only;
//! serialized [`AtlasSource`] values can provide any Unicode scalar values. Runtime measurement
//! and drawing substitute the selected font's underscore entry for a missing character.

use std::collections::HashMap;

use super::*;
use crate::{identity::ProcessUniqueId, image::CheckedImageDimensions};

#[derive(Debug, Clone)]
/// Metrics and atlas coordinates for a glyph.
pub struct CharEntry {
    /// Pixel offset relative to the draw origin.
    pub offset: Vec2i,
    /// Horizontal advance after drawing this glyph.
    pub advance: Vec2i,
    /// Rectangle inside the atlas texture.
    pub rect: Recti, // coordinates in the atlas
}

/// Internal font record stored in the atlas.
struct Font {
    /// Stable identity preserved when this font is copied or replaced in a derived atlas.
    id: FontId,
    /// Distance between text baselines in pixels.
    line_size: usize,
    /// Distance from the top of a line to its baseline.
    baseline: i32,
    /// Requested font size in pixels.
    font_size: usize,
    /// Glyph entries available in this font.
    ///
    /// The built-in builder populates printable ASCII; serialized sources may provide arbitrary
    /// Unicode scalar values.
    entries: HashMap<char, CharEntry>,
}

/// Unvalidated owned font metadata retained until atlas finalization succeeds.
///
/// A vector deliberately preserves duplicate characters from serialized input. Converting to the
/// runtime [`HashMap`] before validation would silently replace one duplicate with another.
struct FontCandidate {
    /// Stable identity published with this font after the candidate passes validation.
    id: FontId,
    /// Distance between text baselines in pixels.
    line_size: usize,
    /// Distance from the top of a line to its baseline.
    baseline: i32,
    /// Requested font size in pixels.
    font_size: usize,
    /// Ordered glyph metadata, including any duplicate keys that validation must reject.
    entries: Vec<(char, CharEntry)>,
}

/// Opaque identity of one logical font resource.
///
/// A builder preserves this identity when it copies or replaces the named font in a derived atlas.
/// Retained UI state can consequently keep the ID across theme changes without retaining a name or
/// depending on a table position. An unrelated atlas does not contain the identity and rejects it
/// before accessing font data.
///
/// IDs cannot be fabricated without an atlas owner. The diagnostic-matched
/// `tests/ui/font_id_default.rs` contract test verifies the absence of a default constructor beside
/// a passing fixture that obtains IDs from an [`AtlasHandle`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontId {
    /// Non-reusing identity allocated when the logical resource first enters an atlas pipeline.
    resource: ProcessUniqueId,
}

impl FontId {
    /// Allocates an identity for one newly introduced font resource.
    fn allocate() -> Self {
        // Atlas loading and successful builder insertion are the only allocation sites. Copying a
        // resource carries this value forward rather than manufacturing replacement identities.
        Self { resource: ProcessUniqueId::allocate() }
    }
}

/// Opaque identity of one logical bitmap icon resource.
///
/// Atlas derivation preserves this value together with the named icon's pixels. Theme-private
/// images receive fresh identities, so an image from one sibling theme cannot alias an unrelated
/// image that happens to occupy the same table position in another atlas.
///
/// IDs cannot be fabricated without an atlas owner. The diagnostic-matched
/// `tests/ui/icon_id_default.rs` contract test verifies the absence of a default constructor beside
/// a passing fixture that obtains IDs from an [`AtlasHandle`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct IconId {
    /// Non-reusing identity allocated when the logical resource first enters an atlas pipeline.
    resource: ProcessUniqueId,
}

impl IconId {
    /// Allocates an identity for one newly introduced icon resource.
    fn allocate() -> Self {
        // Keeping construction private prevents callers from forging identities. Atlas copies use
        // the existing value stored beside the source icon instead of calling this constructor.
        Self { resource: ProcessUniqueId::allocate() }
    }
}

/// Internal bitmap icon record stored in the atlas.
struct Icon {
    /// Stable identity preserved when this icon is copied into a derived atlas.
    id: IconId,
    /// Rectangle occupied by the icon in atlas pixel coordinates.
    rect: Recti,
}

/// Structurally validated immutable atlas storage shared through [`AtlasHandle`].
struct Atlas {
    /// Width of the atlas texture in pixels.
    width: usize,
    /// Height of the atlas texture in pixels.
    height: usize,
    /// RGBA pixel data in row-major order.
    pixels: Vec<Color4b>,
    /// Named fonts available to text layout and rendering.
    fonts: Vec<(String, Font)>,
    /// Named icons available to widgets.
    icons: Vec<(String, Icon)>,
    /// Required opaque-white rendering tile resolved once during finalization.
    white_icon: IconId,
    /// Local font-table positions keyed by stable logical identities.
    ///
    /// Derived atlases may reorder fonts while replacing semantic recipes, so the stable ID cannot
    /// itself be treated as a local vector index.
    font_slots: HashMap<FontId, usize>,
    /// Local icon-table positions keyed by stable logical identities.
    icon_slots: HashMap<IconId, usize>,
}

/// Owned atlas data that has not yet crossed the single validation boundary.
///
/// Serialized sources and the build-time atlas builder both produce this concrete representation.
/// Only atlas validation may convert it into the immutable runtime [`Atlas`].
struct AtlasCandidate {
    /// Prevalidated dimensions and allocation counts for the texture.
    dimensions: CheckedImageDimensions,
    /// Decoded RGBA pixels in row-major order.
    pixels: Vec<Color4b>,
    /// Named fonts whose glyph vectors still preserve duplicate keys.
    fonts: Vec<(String, FontCandidate)>,
    /// Named icons and their proposed atlas rectangles.
    icons: Vec<(String, Icon)>,
}

#[derive(Clone)]
/// Shared read-only handle to a fully validated atlas.
///
/// Construct a handle with [`AtlasHandle::try_from`] and handle the concrete [`AtlasError`]. The
/// crate provides no infallible or lossy source conversion because malformed metadata must never
/// become a renderer-visible resource table.
pub struct AtlasHandle(Rc<Atlas>);

mod validation;
pub use validation::AtlasError;

#[cfg(feature = "builder")]
/// Helpers for constructing atlas textures at build time.
pub mod builder;

mod source;
pub use source::{AtlasSource, FontEntry, SourceFormat};

#[cfg(feature = "save-to-rust")]
mod export;
mod runtime;
