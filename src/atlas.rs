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
use std::fmt::{Debug, Formatter};

use super::*;
use crate::identity::ProcessUniqueId;

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

#[derive(Clone)]
/// Internal font record stored in the atlas.
struct Font {
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

impl Debug for Font {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        let mut entries = String::new();
        for e in &self.entries {
            entries.write_fmt(format_args!("{:?}, ", e))?;
        }
        f.write_fmt(format_args!(
            "Font {{ line_size: {}, baseline: {}, font_size: {}, entries: [{}] }}",
            self.line_size, self.baseline, self.font_size, entries
        ))
    }
}

/// Concrete identity assigned once to one immutable runtime atlas.
///
/// The wrapper keeps atlas provenance distinct from renderer and retained-object identities even
/// though all of them share the same non-reusing process-wide allocator.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
struct AtlasId(
    /// Shared process identity hidden behind the atlas-specific type boundary.
    ProcessUniqueId,
);

impl AtlasId {
    /// Allocates the owner identity retained by one newly constructed runtime atlas.
    fn allocate() -> Self {
        // Construction is the sole allocation boundary. Every FontId and IconId minted from this
        // atlas copies the identity, so a local table slot is never accepted without provenance.
        Self(ProcessUniqueId::allocate())
    }
}

/// Opaque capability referencing one font in one concrete runtime atlas.
///
/// Passing an ID to metric or drawing methods on another [`AtlasHandle`] is an invariant violation
/// and panics; renderer submission reports the same mistake as a typed render error during
/// preflight.
///
/// IDs cannot be fabricated without an atlas owner:
///
/// ```compile_fail
/// use microui_redux::FontId;
/// let _font = FontId::default();
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontId {
    /// Identity of the atlas that minted this capability.
    atlas: AtlasId,
    /// Font-table slot meaningful only within `atlas`.
    slot: usize,
}

impl FontId {
    /// Creates a font capability at the atlas construction or lookup boundary.
    fn new(atlas: AtlasId, slot: usize) -> Self {
        // Fields stay private so application code can obtain only validated slots from an atlas or
        // from the builder that owns the future atlas.
        Self { atlas, slot }
    }
}

/// Opaque capability referencing one bitmap icon in one concrete runtime atlas.
///
/// Passing an ID to rectangle or size methods on another [`AtlasHandle`] is an invariant violation
/// and panics; renderer submission reports the same mistake as a typed render error during
/// preflight.
///
/// IDs cannot be fabricated without an atlas owner:
///
/// ```compile_fail
/// use microui_redux::IconId;
/// let _icon = IconId::default();
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct IconId {
    /// Identity of the atlas that minted this capability.
    atlas: AtlasId,
    /// Icon-table slot meaningful only within `atlas`.
    slot: usize,
}

impl IconId {
    /// Creates an icon capability at the atlas construction or lookup boundary.
    fn new(atlas: AtlasId, slot: usize) -> Self {
        // Fields stay private so numeric positions cannot be forged or reused with another atlas.
        Self { atlas, slot }
    }
}

#[derive(Debug, Clone)]
/// Internal bitmap icon record stored in the atlas.
struct Icon {
    /// Rectangle occupied by the icon in atlas pixel coordinates.
    rect: Recti,
}

/// Immutable atlas storage shared through [`AtlasHandle`].
struct Atlas {
    /// Process-unique provenance copied into every font and icon capability.
    id: AtlasId,
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
}

#[derive(Clone)]
/// Shared read-only handle to a fully constructed atlas.
pub struct AtlasHandle(Rc<Atlas>);

#[cfg(feature = "builder")]
/// Helpers for constructing atlas textures at build time.
pub mod builder;

mod source;
pub use source::{AtlasSource, FontEntry, SourceFormat};

#[cfg(feature = "save-to-rust")]
mod export;
mod runtime;
