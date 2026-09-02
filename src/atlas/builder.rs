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

//! Build-time atlas construction helpers.
//!
//! Each configured font is rasterized for printable ASCII (`U+0020` through `U+007E`) only. Use a
//! serialized [`super::AtlasSource`] when an application needs a different or broader glyph set.
//! Individual image and font files, as well as decoded atlas storage, are bounded by
//! [`crate::image::MAX_DECODED_RGBA_BYTES`] before the builder allocates from their contents.

use super::*;
mod packer;
use packer::Packer;
use crate::image::{ImageError, ImageSource, MAX_DECODED_RGBA_BYTES, load_image_bytes};
use fontdue::{Font as RasterFont, FontSettings, Metrics};
use std::{
    error::Error,
    fmt::{Display, Formatter},
    fs::File,
    io::{self, Read},
    path::Path,
};

/// Incrementally constructs an atlas by packing fonts and named bitmap icons.
pub struct Builder {
    /// Rectangle packer used to reserve atlas regions.
    packer: Packer,
    /// Unvalidated atlas data finalized through the same boundary as serialized sources.
    candidate: AtlasCandidate,
}

/// One glyph whose rectangle has been reserved in a temporary packer but whose bitmap has not yet
/// been rasterized or copied into the atlas.
///
/// Keeping the plan concrete makes font insertion transactional: every printable glyph must fit
/// before an external rasterizer allocates bitmap data, and the live builder is changed only after
/// all rasterized output agrees with this plan.
struct PlannedGlyph {
    /// Unicode scalar value represented by this glyph.
    character: char,
    /// Geometry reported by fontdue's allocation-free metrics pass.
    metrics: Metrics,
    /// Rectangle reserved in the cloned next-state packer, or an empty origin rectangle for a
    /// glyph such as space that has no bitmap pixels.
    rectangle: Recti,
    /// Exact bitmap length implied by `metrics.width * metrics.height`.
    pixel_count: usize,
}

/// Concrete failure returned while loading, packing, or finalizing a build-time atlas.
///
/// Structural atlas and image-decoding failures retain their matchable concrete errors. File,
/// font, and packer failures use an I/O-oriented error because they originate in heterogeneous
/// operational APIs.
#[derive(Debug)]
pub enum BuilderError {
    /// Candidate dimensions or finalized atlas metadata violate the runtime atlas contract.
    Atlas {
        /// Concrete structural validation failure.
        source: AtlasError,
    },
    /// An icon asset could be read but could not be decoded as a supported static image.
    Image {
        /// Path of the invalid icon asset.
        path: String,
        /// Concrete image validation or decoding failure.
        source: ImageError,
    },
    /// An asset could not be read, rasterized, or packed.
    Asset {
        /// Concrete operational error produced by the builder pipeline.
        source: io::Error,
    },
}

impl Display for BuilderError {
    /// Delegates to the retained concrete cause without flattening it into stored text.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        // Every variant already owns a complete diagnostic. Only image decoding adds the asset
        // path that its lower-level source cannot know.
        match self {
            Self::Atlas { source } => Display::fmt(source, formatter),
            Self::Image { path, source } => write!(formatter, "cannot decode icon asset `{path}`: {source}"),
            Self::Asset { source } => Display::fmt(source, formatter),
        }
    }
}

impl Error for BuilderError {
    /// Exposes the retained atlas or asset error through the standard cause chain.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Every variant retains one concrete lower-level cause.
        match self {
            Self::Atlas { source } => Some(source),
            Self::Image { source, .. } => Some(source),
            Self::Asset { source } => Some(source),
        }
    }
}

impl From<AtlasError> for BuilderError {
    /// Preserves a structural validation error without converting it to I/O text.
    fn from(source: AtlasError) -> Self {
        // This conversion is used by both early dimension checks and final build validation.
        Self::Atlas { source }
    }
}

impl From<io::Error> for BuilderError {
    /// Preserves one operational asset-pipeline failure.
    fn from(source: io::Error) -> Self {
        // File, PNG, font, and packer operations share one operational error boundary.
        Self::Asset { source }
    }
}

#[derive(Clone)]
/// Configuration for constructing an atlas from disk assets.
pub struct FontAsset<'a> {
    /// Stable font key stored in the atlas font table.
    pub name: &'a str,
    /// Path to the source font file.
    pub path: &'a str,
    /// Pixel size baked into the atlas.
    pub size: usize,
}

#[derive(Clone)]
/// Named bitmap icon included in a constructed atlas.
pub struct IconAsset<'a> {
    /// Stable icon key stored in the atlas icon table.
    pub name: &'a str,
    /// Path to the source PNG file.
    pub path: &'a str,
}

/// Configuration for constructing an atlas from disk assets.
pub struct Config<'a> {
    /// Width of the atlas texture in pixels.
    pub texture_width: usize,
    /// Height of the atlas texture in pixels.
    pub texture_height: usize,
    /// Path to the solid white icon.
    pub white_icon: String,
    /// Named semantic or application icons packed after the white rendering tile.
    pub icons: &'a [IconAsset<'a>],
    /// Fonts baked into the atlas for the printable ASCII range.
    ///
    /// A standard Context atlas must include the `body` key. The optional conventional keys
    /// `small`, `title`, `heading`, and `mono` populate their corresponding skin roles; missing
    /// optional roles use `body`.
    pub fonts: &'a [FontAsset<'a>],
}

impl Builder {
    /// Creates a builder using the provided configuration and assets.
    ///
    /// # Errors
    ///
    /// Returns an error before reading any assets when the texture dimensions cannot be represented
    /// safely. Also returns an error when a configured font or icon cannot be read, decoded,
    /// rasterized, or packed into those dimensions. A fontless configuration is valid for a
    /// low-level renderer atlas; [`crate::Context`] separately requires its semantic `body` font.
    /// Call [`Builder::build`] to perform structural validation after all assets have been added.
    pub fn from_config(config: &Config) -> Result<Builder, BuilderError> {
        // Validate dimensions before multiplying, allocating, or converting usize coordinates to
        // i32. BuilderError retains both operational asset failures and the original typed
        // AtlasError for structural failures rather than flattening either category into text.
        let candidate = AtlasCandidate::blank(config.texture_width, config.texture_height)?;
        let mut builder = Builder {
            candidate,
            // Atlas packing has one private one-pixel border/inter-rectangle policy.
            packer: Packer::new(config.texture_width as i32, config.texture_height as i32),
        };

        builder.add_icon_named("white", &config.white_icon)?;
        for icon in config.icons {
            builder.add_icon_named(icon.name, icon.path)?;
        }
        for font in config.fonts {
            builder.add_font_named(font.name, font.path, font.size)?;
        }

        Ok(builder)
    }

    /// Creates an empty-font builder by copying every named icon from an existing atlas.
    ///
    /// Theme font recipes use this constructor to rebuild typography without requiring the theme
    /// to repeat application icon paths. Icons are inserted in their existing table order, which
    /// preserves deterministic packing for atlases originally built with this builder and keeps
    /// semantic icon lookup independent from numeric slots.
    ///
    /// # Errors
    ///
    /// Returns an error if the source dimensions cannot initialize a candidate or an extracted
    /// icon cannot be represented or packed. A successful result contains no fonts; callers add
    /// the complete desired font set before [`Builder::build`].
    pub fn from_atlas_icons(atlas: &AtlasHandle) -> Result<Builder, BuilderError> {
        Self::from_atlas_icons_with_size(atlas, atlas.width(), atlas.height())
    }

    /// Creates an empty-font builder of an explicit size by copying an existing atlas's icons.
    ///
    /// This form lets a theme select enough texture capacity for its declared font sizes even when
    /// the source atlas is a compact or prebuilt allocation. The requested dimensions still pass
    /// the common atlas image limits before icon pixels are extracted or allocated.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested dimensions are invalid or cannot contain every copied
    /// icon. A failure leaves the immutable source atlas untouched.
    pub fn from_atlas_icons_with_size(atlas: &AtlasHandle, width: usize, height: usize) -> Result<Builder, BuilderError> {
        let mut builder = Builder {
            candidate: AtlasCandidate::blank(width, height)?,
            // Reconstruct packing state solely from concrete icon rectangles; font glyphs from the
            // source are intentionally not retained.
            packer: Packer::new(width as i32, height as i32),
        };
        let source_pixels = atlas.pixels_clone();
        let source_width = atlas.width();
        for (name, icon) in atlas.clone_icon_table() {
            let rectangle = atlas.get_icon_rect(icon);
            let left = usize::try_from(rectangle.x).expect("validated atlas icon x must be nonnegative");
            let top = usize::try_from(rectangle.y).expect("validated atlas icon y must be nonnegative");
            let width = usize::try_from(rectangle.width).expect("validated atlas icon width must be positive");
            let height = usize::try_from(rectangle.height).expect("validated atlas icon height must be positive");
            let mut pixels = Vec::with_capacity(width * height);
            for row in 0..height {
                // Atlas validation proves each exact row is in bounds; copy only the icon tile so
                // old font pixels and unused padding cannot leak into the rebuilt candidate.
                let start = (top + row) * source_width + left;
                pixels.extend_from_slice(&source_pixels[start..start + width]);
            }
            builder.add_icon_pixels_named(name.as_str(), width, height, pixels.as_slice())?;
        }
        Ok(builder)
    }

    /// Creates a builder by repacking every named icon and baked font from an existing atlas.
    ///
    /// Theme definitions that retain the application's typography still need a new immutable atlas
    /// when they add state artwork. Repacking both resource tables gives those themes room for new
    /// bitmap tiles without requiring the original font files or weakening atlas-ID provenance.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested dimensions are invalid or cannot contain all copied
    /// icons, glyph bitmaps, and metadata. The source atlas remains immutable on every failure.
    pub fn from_atlas_with_size(atlas: &AtlasHandle, width: usize, height: usize) -> Result<Builder, BuilderError> {
        // Ordinary rebuilds preserve every named font. Theme recipes use the narrower internal
        // helper below to replace only their semantic roles while retaining application fonts.
        Self::from_atlas_with_size_excluding_fonts(atlas, width, height, &[])
    }

    /// Creates a builder by repacking all source resources except explicitly replaced fonts.
    ///
    /// The exclusion list is intentionally a concrete slice of atlas names. Theme loading has
    /// exactly five semantic roles to replace and does not need a callback, erased predicate, or
    /// second resource-copy abstraction.
    pub(crate) fn from_atlas_with_size_excluding_fonts(
        atlas: &AtlasHandle,
        width: usize,
        height: usize,
        excluded_fonts: &[&str],
    ) -> Result<Builder, BuilderError> {
        // Begin with the existing icon-copy path so required semantic icon names, including the
        // opaque white tile, retain their table order and exact pixels in the new allocation.
        let mut builder = Self::from_atlas_icons_with_size(atlas, width, height)?;
        let source_pixels = atlas.pixels_clone();
        let source_width = atlas.width();

        for (name, source_font) in &atlas.0.fonts {
            if excluded_fonts.contains(&name.as_str()) {
                // A later recipe will insert this same name. Omitting it now avoids a duplicate
                // while every unrelated application font follows the normal exact-copy path.
                continue;
            }
            // HashMap traversal is intentionally normalized by Unicode scalar value. Stable copy
            // order keeps atlas output reproducible and gives the rectangle packer deterministic
            // input even when the source atlas originated from serialized metadata.
            let mut source_entries = source_font.entries.iter().collect::<Vec<_>>();
            source_entries.sort_unstable_by_key(|(character, _)| **character);
            let mut entries = Vec::with_capacity(source_entries.len());
            for (&character, source_entry) in source_entries {
                let source_rectangle = source_entry.rect;
                let glyph_width = usize::try_from(source_rectangle.width).expect("validated source glyph width must be nonnegative");
                let glyph_height = usize::try_from(source_rectangle.height).expect("validated source glyph height must be nonnegative");
                let mut glyph_pixels = Vec::with_capacity(glyph_width.saturating_mul(glyph_height));
                let source_left = usize::try_from(source_rectangle.x).expect("validated source glyph x must be nonnegative");
                let source_top = usize::try_from(source_rectangle.y).expect("validated source glyph y must be nonnegative");
                for row in 0..glyph_height {
                    // Atlas validation proves every source row is in bounds. Copying the exact
                    // glyph rectangle prevents unrelated neighboring atlas pixels from leaking.
                    let start = (source_top + row) * source_width + source_left;
                    glyph_pixels.extend_from_slice(&source_pixels[start..start + glyph_width]);
                }
                let rectangle = builder.add_tile(glyph_width, glyph_height, glyph_pixels.as_slice())?;
                entries.push((
                    character,
                    CharEntry {
                        offset: source_entry.offset,
                        advance: source_entry.advance,
                        rect: rectangle,
                    },
                ));
            }

            let candidate = FontCandidate {
                line_size: source_font.line_size,
                baseline: source_font.baseline,
                font_size: source_font.font_size,
                entries,
            };
            builder.candidate.validate_font(name, &candidate)?;
            builder.candidate.fonts.push((name.clone(), candidate));
        }
        Ok(builder)
    }

    /// Adds an icon from the given image path and returns its [`IconId`].
    ///
    /// # Errors
    ///
    /// Returns an error when the PNG exceeds the builder input limit, cannot be read or decoded, its
    /// dimensions cannot be packed, or its normalized pixels are inconsistent.
    pub fn add_icon(&mut self, path: &str) -> Result<IconId, BuilderError> {
        let name = Self::format_path(path);
        self.add_icon_named(&name, path)
    }

    /// Adds an icon under a stable lookup key and returns its [`IconId`].
    ///
    /// The name is retained verbatim. A duplicate name is rejected before the path is opened and
    /// leaves the builder unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error when the PNG exceeds the builder input limit, cannot be read or decoded, its
    /// dimensions cannot be packed, or its normalized pixels are inconsistent.
    pub fn add_icon_named(&mut self, name: &str, path: &str) -> Result<IconId, BuilderError> {
        if self.candidate.icons.iter().any(|(existing, _)| existing == name) {
            // Preserve the documented file-I/O boundary: a duplicate name is known entirely from
            // candidate metadata and must not attempt to open an irrelevant path.
            return Err(AtlasError::DuplicateIconName { name: name.to_string() }.into());
        }
        let (width, height, pixels) = Self::load_icon(path)?;
        self.add_icon_pixels_named(name, width, height, pixels.as_slice())
    }

    /// Adds one already-decoded icon tile under a stable name.
    ///
    /// File-backed and atlas-copy insertion share this boundary so duplicate checks, transactional
    /// packing, candidate metadata, and capability provenance cannot diverge.
    pub(crate) fn add_icon_pixels_named(&mut self, name: &str, width: usize, height: usize, pixels: &[Color4b]) -> Result<IconId, BuilderError> {
        if self.candidate.icons.iter().any(|(existing, _)| existing == name) {
            // Name lookup would make one of two equal entries unreachable. Detect the conflict
            // before packing so a failed insertion has no side effects.
            return Err(AtlasError::DuplicateIconName { name: name.to_string() }.into());
        }
        let rect = self.add_tile(width, height, pixels)?;
        let slot = self.candidate.icons.len();
        self.candidate.icons.push((name.to_string(), Icon { rect }));
        // The returned capability already belongs to the candidate moved through build.
        Ok(IconId::new(self.candidate.id, slot))
    }

    /// Adds the printable ASCII range of a font at the requested size and returns its [`FontId`].
    ///
    /// # Errors
    ///
    /// Returns an error when `size` is outside `1..=i32::MAX`, the font exceeds the builder input
    /// limit or cannot be read/parsed, or any glyph cannot fit the remaining texture space.
    pub fn add_font(&mut self, path: &str, size: usize) -> Result<FontId, BuilderError> {
        let name = format!("{}-{}", Self::format_path(path), size);
        self.add_font_named(name.as_str(), path, size)
    }

    /// Adds the printable ASCII range of a font under an explicit atlas key and returns its
    /// [`FontId`].
    ///
    /// The name is retained verbatim. A duplicate name is rejected before the path is opened and
    /// leaves the builder unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error when `size` is outside `1..=i32::MAX`, the font exceeds the builder input
    /// limit or cannot be read/parsed, or any glyph cannot fit the remaining texture space.
    pub fn add_font_named(&mut self, name: &str, path: &str, size: usize) -> Result<FontId, BuilderError> {
        if self.candidate.fonts.iter().any(|(existing, _)| existing == name) {
            // A repeated key is an input error, not a second resource that should consume texture
            // space. Check it before size validation and file I/O to keep failure transactional.
            return Err(AtlasError::DuplicateFontName { name: name.to_string() }.into());
        }
        let maximum = i32::MAX as usize;
        if size == 0 || size > maximum {
            // Match the serialized AtlasSource contract before converting the requested size to
            // runtime coordinates. Per-glyph metrics are checked against the live packer below
            // before fontdue is allowed to allocate any raster bitmap.
            return Err(AtlasError::InvalidFontSize { font: name.to_string(), font_size: size }.into());
        }
        let font = Self::load_font(path)?;
        let mut next_packer = self.packer.clone();
        let mut planned_glyphs = Vec::with_capacity(95);
        let mut min_y = i64::MAX;
        let mut max_y = i64::MIN;
        // Plan the complete printable-ASCII font against a cloned packer. A late packing failure
        // therefore cannot consume rectangles in the live builder, and no bitmap allocation occurs
        // until every glyph has a destination.
        for i in 32..127 {
            let ch = i as u8 as char;
            let metrics = font.metrics(ch, size as f32);
            let pixel_count = metrics
                .width
                .checked_mul(metrics.height)
                .ok_or_else(|| io::Error::other(format!("Font `{name}` glyph {ch:?} dimensions overflow pixel count")))?;
            let rectangle = Self::reserve_tile(
                &mut next_packer,
                metrics.width,
                metrics.height,
                self.candidate.dimensions.width,
                self.candidate.dimensions.height,
            )
            .map_err(|source| io::Error::other(format!("Font `{name}` glyph {ch:?}: {source}")))?;
            planned_glyphs.push(PlannedGlyph {
                character: ch,
                metrics,
                rectangle,
                pixel_count,
            });
            // Font metrics come from an external file. Wider arithmetic avoids wrapping while the
            // fallback line height is accumulated; final atlas validation still owns its runtime
            // i32 representability rule.
            let glyph_height =
                i64::try_from(metrics.height).map_err(|_| io::Error::other(format!("Font `{name}` glyph {ch:?} height cannot be represented")))?;
            let glyph_bottom = i64::try_from(size).expect("validated builder font size fits i32") - i64::from(metrics.ymin) - glyph_height;
            min_y = min_y.min(glyph_bottom);
            max_y = max_y.max(glyph_bottom);
        }

        let line_metrics = font.horizontal_line_metrics(size as f32);
        let line_size = line_metrics.as_ref().map(|m| m.new_line_size.round() as usize).unwrap_or_else(|| {
            // Printable ASCII always contributes entries, so these sentinels have been
            // replaced. Saturation defers an out-of-contract metric to typed final validation.
            usize::try_from(max_y.saturating_sub(min_y)).unwrap_or(usize::MAX)
        });
        let baseline = line_metrics
            .as_ref()
            .map(|m| m.ascent.round() as i32)
            .unwrap_or_else(|| i32::try_from(line_size).unwrap_or(i32::MAX));
        let font_candidate = FontCandidate {
            line_size,
            baseline,
            font_size: size,
            entries: planned_glyphs
                .iter()
                .map(|glyph| {
                    let metrics = glyph.metrics;
                    (
                        glyph.character,
                        CharEntry {
                            offset: Vec2i::new(metrics.xmin, metrics.ymin),
                            advance: Vec2i::new(metrics.advance_width as i32, metrics.advance_height as i32),
                            rect: glyph.rectangle,
                        },
                    )
                })
                .collect(),
        };
        // Reuse the same structural validator as serialized atlases before rasterizing or changing
        // live state. This catches unusable line metrics and fallback metadata at insertion time.
        self.candidate.validate_font(name, &font_candidate)?;

        let mut rasterized_pixels = Vec::with_capacity(planned_glyphs.len());
        for glyph in &planned_glyphs {
            let (actual_metrics, bitmap) = font.rasterize(glyph.character, size as f32);
            if actual_metrics != glyph.metrics || bitmap.len() != glyph.pixel_count {
                // The allocation-free metrics query and rasterizer must describe exactly the same
                // bitmap. Reject inconsistent dependency output while the builder is untouched.
                return Err(io::Error::other(format!(
                    "Font `{name}` glyph {:?} changed between measurement and rasterization",
                    glyph.character
                ))
                .into());
            }
            rasterized_pixels.push(bitmap.into_iter().map(|alpha| color4b(0xFF, 0xFF, 0xFF, alpha)).collect::<Vec<_>>());
        }

        // All fallible work is complete. Copy the staged bitmaps, then publish the cloned packer
        // and metadata as one logical commit to the builder.
        for (glyph, pixels) in planned_glyphs.iter().zip(&rasterized_pixels) {
            Self::copy_tile(&mut self.candidate.pixels, self.candidate.dimensions.width, glyph.rectangle, pixels);
        }
        self.packer = next_packer;
        let slot = self.candidate.fonts.len();
        self.candidate.fonts.push((name.to_string(), font_candidate));
        // The returned capability already belongs to the same candidate moved through build.
        Ok(FontId::new(self.candidate.id, slot))
    }

    /// Loads an icon image from disk and normalizes it to RGBA pixels.
    fn load_icon(path: &str) -> Result<(usize, usize, Vec<Color4b>), BuilderError> {
        let bytes = Self::read_asset_file(path)?;
        // ImageError already owns the precise format/storage classification. Add only the asset
        // path at this boundary instead of converting it through an unrelated io::Error kind.
        load_image_bytes(ImageSource::Png { bytes: bytes.as_slice() }).map_err(|source| BuilderError::Image { path: path.to_string(), source })
    }

    /// Reads one builder asset while bounding compressed images and font files before allocation.
    fn read_asset_file(path: &str) -> Result<Vec<u8>, BuilderError> {
        let file = File::open(path).map_err(|source| io::Error::new(source.kind(), format!("Cannot open asset file `{path}`: {source}")))?;
        Self::read_bounded(file, MAX_DECODED_RGBA_BYTES)
            .map_err(|source| io::Error::new(source.kind(), format!("Cannot read asset file `{path}`: {source}")).into())
    }

    /// Reads at most `maximum_bytes + 1` bytes and rejects a source that crosses that boundary.
    fn read_bounded(reader: impl Read, maximum_bytes: usize) -> io::Result<Vec<u8>> {
        // Taking one byte beyond the accepted limit distinguishes an exact-boundary file from a
        // larger one without trusting file metadata or allowing read_to_end to grow indefinitely.
        let read_limit = maximum_bytes
            .checked_add(1)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "builder input limit cannot be represented by Read::take"))?;
        let mut reader = reader.take(read_limit);
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        if bytes.len() > maximum_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("asset exceeds the {maximum_bytes}-byte builder input limit"),
            ));
        }
        Ok(bytes)
    }

    /// Packs one populated bitmap transactionally and copies it into the texture buffer.
    fn add_tile(&mut self, width: usize, height: usize, pixels: &[Color4b]) -> Result<Recti, BuilderError> {
        let expected = width
            .checked_mul(height)
            .ok_or_else(|| io::Error::other(format!("Tile dimensions {width}x{height} overflow pixel count")))?;
        if pixels.len() != expected {
            return Err(io::Error::other(format!("Tile dimensions {width}x{height} require {expected} pixels, received {}", pixels.len())).into());
        }
        let mut next_packer = self.packer.clone();
        let rectangle = Self::reserve_tile(
            &mut next_packer,
            width,
            height,
            self.candidate.dimensions.width,
            self.candidate.dimensions.height,
        )?;
        // Reservation and all bounds checks succeeded against a cloned packer. Pixel copying is now
        // infallible under those checked preconditions, after which the new packer state is exposed.
        Self::copy_tile(&mut self.candidate.pixels, self.candidate.dimensions.width, rectangle, pixels);
        self.packer = next_packer;
        Ok(rectangle)
    }

    /// Reserves one bitmap rectangle in a caller-selected packer without touching atlas pixels.
    fn reserve_tile(packer: &mut Packer, width: usize, height: usize, atlas_width: usize, atlas_height: usize) -> io::Result<Recti> {
        let width_i32 = i32::try_from(width).map_err(|_| io::Error::other(format!("Tile width {width} exceeds i32::MAX")))?;
        let height_i32 = i32::try_from(height).map_err(|_| io::Error::other(format!("Tile height {height} exceeds i32::MAX")))?;
        match packer.pack(width_i32, height_i32, false) {
            Some(rectangle) => {
                // The packer is private, but validate its result at this boundary before its state
                // or coordinates can influence the atlas allocation.
                let left = usize::try_from(rectangle.x).map_err(|_| io::Error::other("Packer returned a negative tile x coordinate"))?;
                let top = usize::try_from(rectangle.y).map_err(|_| io::Error::other("Packer returned a negative tile y coordinate"))?;
                let right = left.checked_add(width).ok_or_else(|| io::Error::other("Packed tile right edge overflowed"))?;
                let bottom = top.checked_add(height).ok_or_else(|| io::Error::other("Packed tile bottom edge overflowed"))?;
                if rectangle.width != width_i32 || rectangle.height != height_i32 || right > atlas_width || bottom > atlas_height {
                    return Err(io::Error::other("Packer returned a tile outside the requested atlas region"));
                }
                Ok(rectangle)
            }
            None if width != 0 && height != 0 => {
                let error = format!("Bitmap size of {atlas_width}x{atlas_height} is not enough to hold the atlas, please resize");
                Err(io::Error::other(error))
            }
            _ => Ok(Recti::new(0, 0, 0, 0)),
        }
    }

    /// Copies one already-validated bitmap into its reserved row-major atlas rectangle.
    fn copy_tile(atlas_pixels: &mut [Color4b], atlas_width: usize, rectangle: Recti, pixels: &[Color4b]) {
        // reserve_tile and the raster-size checks prove every conversion and slice range below.
        // Retaining debug assertions documents those assumptions without introducing a fallible
        // operation after a transaction starts committing pixels.
        let left = usize::try_from(rectangle.x).expect("reserved tile x coordinate must be nonnegative");
        let top = usize::try_from(rectangle.y).expect("reserved tile y coordinate must be nonnegative");
        let width = usize::try_from(rectangle.width).expect("reserved tile width must be nonnegative");
        let height = usize::try_from(rectangle.height).expect("reserved tile height must be nonnegative");
        debug_assert_eq!(pixels.len(), width * height);
        debug_assert!(top.saturating_add(height).saturating_mul(atlas_width) <= atlas_pixels.len());
        for row in 0..height {
            let source_start = row * width;
            let destination_start = (top + row) * atlas_width + left;
            atlas_pixels[destination_start..destination_start + width].copy_from_slice(&pixels[source_start..source_start + width]);
        }
    }

    /// Loads and parses a font file using `fontdue`.
    fn load_font(path: &str) -> Result<RasterFont, BuilderError> {
        let data = Self::read_asset_file(path)?;
        let font =
            RasterFont::from_bytes(data, FontSettings::default()).map_err(|error| io::Error::other(format!("Cannot parse font asset `{path}`: {error}")))?;
        Ok(font)
    }

    /// Returns the final path segment, preserving the original value when it is not valid UTF-8.
    fn strip_path_to_file(path: &str) -> String {
        let p = Path::new(path);
        p.file_name().and_then(|n| n.to_str()).unwrap_or(path).to_string()
    }

    /// Removes a file extension from a path-like string.
    fn strip_extension(path: &str) -> String {
        let p = Path::new(path);
        p.with_extension("").to_str().unwrap_or(path).to_string()
    }

    /// Converts an asset path into the stable atlas key used for generated sources.
    fn format_path(path: &str) -> String {
        Self::strip_extension(&Self::strip_path_to_file(path))
    }

    /// Validates and consumes the populated builder, returning an immutable [`AtlasHandle`].
    ///
    /// # Errors
    ///
    /// Returns [`BuilderError::Atlas`] with the concrete [`AtlasError`] when generated or supplied
    /// assets violate the same structural contract enforced for serialized [`AtlasSource`] values.
    pub fn build(self) -> Result<AtlasHandle, BuilderError> {
        // Builder finalization shares the only AtlasHandle construction boundary and preserves the
        // owner identity already copied into every ID returned by add_font or add_icon.
        AtlasHandle::finish(self.candidate).map_err(BuilderError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const WHITE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/WHITE.png");
    const FONT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/NORMAL.ttf");
    const FILE_ICON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/FILE_16.png");

    /// Verifies the builder supports the same fontless low-level atlas contract as AtlasSource.
    #[test]
    fn fontless_config_builds_a_low_level_renderer_atlas() {
        let config = Config {
            texture_width: 32,
            texture_height: 32,
            white_icon: String::from(WHITE_PATH),
            icons: &[],
            fonts: &[],
        };

        let atlas = Builder::from_config(&config)
            .expect("fontless renderer configuration must load")
            .build()
            .expect("white-only renderer atlas must validate");
        assert!(atlas.clone_font_table().is_empty());
        assert_eq!(atlas.clone_icon_table(), vec![(String::from("white"), atlas.white_icon())]);
    }

    /// Verifies invalid dimensions fail before the builder reads any configured asset path.
    #[test]
    fn config_validates_dimensions_before_allocating_or_loading_assets() {
        let fonts = [FontAsset {
            name: "body",
            path: "path-that-must-not-be-read",
            size: 10,
        }];
        let config = Config {
            texture_width: 0,
            texture_height: 32,
            white_icon: String::from("path-that-must-not-be-read"),
            icons: &[],
            fonts: &fonts,
        };

        let error = Builder::from_config(&config).err().expect("zero width must be rejected before asset loading");
        assert!(matches!(
            error,
            BuilderError::Atlas {
                source: AtlasError::Image {
                    source: ImageError::Storage {
                        source: crate::image::ImageStorageError::DimensionsOutOfRange { width: 0, height: 32 },
                    },
                },
            }
        ));
    }

    /// Verifies successful file I/O followed by invalid PNG bytes retains ImageError and the path.
    #[test]
    fn invalid_icon_bytes_use_the_composed_image_error_variant() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let error = Builder::load_icon(path).expect_err("Cargo.toml bytes cannot decode as a PNG icon");

        assert!(matches!(
            &error,
            BuilderError::Image {
                path: actual_path,
                source: ImageError::Decode { .. },
            } if actual_path == path
        ));
        assert!(std::error::Error::source(&error).is_some());
    }

    /// Verifies unrepresentable rasterization sizes fail concretely before fontdue reads the font.
    #[test]
    fn font_size_is_bounded_by_runtime_metadata_coordinates() {
        let fonts = [FontAsset {
            name: "body",
            path: "path-that-must-not-be-read",
            size: i32::MAX as usize + 1,
        }];
        let config = Config {
            texture_width: 32,
            texture_height: 32,
            white_icon: String::from(WHITE_PATH),
            icons: &[],
            fonts: &fonts,
        };

        let error = Builder::from_config(&config).err().expect("oversized font must fail before file loading");
        assert!(matches!(
            error,
            BuilderError::Atlas {
                source: AtlasError::InvalidFontSize { font, font_size },
            } if font == "body" && font_size == i32::MAX as usize + 1
        ));
    }

    /// Verifies builder output crosses the shared opaque-white validation boundary.
    #[test]
    fn build_rejects_a_non_white_image_assigned_to_the_white_role() {
        let fonts = [FontAsset { name: "body", path: FONT_PATH, size: 10 }];
        let config = Config {
            texture_width: 512,
            texture_height: 256,
            // The close glyph is a valid PNG and packs successfully, but it is not a solid opaque
            // white tile. Reaching build isolates structural validation from asset I/O and packing.
            white_icon: String::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/CLOSE.png")),
            icons: &[],
            fonts: &fonts,
        };
        let builder = Builder::from_config(&config).expect("non-white fixture assets must still load and pack");

        let error = builder.build().err().expect("shared finalization must inspect every white-tile pixel");
        assert!(matches!(
            error,
            BuilderError::Atlas {
                source: AtlasError::WhiteIconNotOpaqueWhite { .. },
            }
        ));
    }

    /// Verifies capabilities returned during construction retain the finalized atlas owner.
    #[test]
    fn builder_resource_ids_remain_valid_after_finalization() {
        let fonts = [FontAsset { name: "body", path: FONT_PATH, size: 10 }];
        let config = Config {
            texture_width: 512,
            texture_height: 256,
            white_icon: String::from(WHITE_PATH),
            icons: &[],
            fonts: &fonts,
        };
        let mut builder = Builder::from_config(&config).expect("fixture assets must build an atlas");

        // These IDs are minted before build moves the candidate into its validated shared handle.
        let icon = builder.add_icon_named("extra", WHITE_PATH).expect("extra icon must fit");
        let font = builder.add_font_named("extra", FONT_PATH, 8).expect("extra font must fit");
        let atlas = builder.build().expect("builder output must pass shared atlas validation");

        assert!(atlas.contains_icon(icon));
        assert!(atlas.contains_font(font));
        assert_eq!(atlas.icon_id("extra"), Some(icon));
        assert_eq!(atlas.font_id("extra"), Some(font));
    }

    /// Verifies a theme-style rebuild retains exact icon names and pixels while replacing all
    /// source fonts with the caller's explicitly added set.
    #[test]
    fn atlas_icon_copy_rebuilds_fonts_without_losing_semantic_images() {
        let fonts = [FontAsset { name: "body", path: FONT_PATH, size: 10 }];
        let icons = [IconAsset { name: "file", path: FILE_ICON_PATH }];
        let config = Config {
            texture_width: 512,
            texture_height: 256,
            white_icon: String::from(WHITE_PATH),
            icons: &icons,
            fonts: &fonts,
        };
        let source = Builder::from_config(&config)
            .expect("source atlas assets must pack")
            .build()
            .expect("source atlas must validate");
        let source_file = source.icon_id("file").expect("source file icon must exist");
        let source_file_rectangle = source.get_icon_rect(source_file);

        let mut replacement = Builder::from_atlas_icons(&source).expect("validated source icons must copy");
        replacement
            .add_font_named("body", FONT_PATH, 14)
            .expect("replacement body font must fit beside copied icons");
        let replacement = replacement.build().expect("replacement atlas must validate");

        assert_eq!(
            replacement.clone_icon_table().iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
            ["white", "file"]
        );
        let replacement_file_rectangle = replacement.get_icon_rect(replacement.icon_id("file").unwrap());
        assert_eq!(
            (
                replacement_file_rectangle.x,
                replacement_file_rectangle.y,
                replacement_file_rectangle.width,
                replacement_file_rectangle.height,
            ),
            (
                source_file_rectangle.x,
                source_file_rectangle.y,
                source_file_rectangle.width,
                source_file_rectangle.height
            )
        );
        assert_eq!(replacement.get_font_size(replacement.font_id("body").unwrap()), 14);
        assert_eq!(replacement.clone_font_table().len(), 1);
    }

    /// Verifies an artwork-only rebuild preserves baked glyph metrics without original font files.
    #[test]
    fn complete_atlas_copy_repacks_existing_fonts_and_accepts_new_artwork() {
        let source = crate::test_support::test_atlas();
        let source_font = source.font_id("body").expect("source atlas must contain its body font");
        let mut builder = Builder::from_atlas_with_size(&source, 64, 64).expect("copied test resources must fit the expanded atlas");
        let pixels = [color4b(7, 8, 9, 255); 4];
        let artwork = builder
            .add_icon_pixels_named("@theme/test", 2, 2, &pixels)
            .expect("theme artwork must fit beside copied resources");
        let rebuilt = builder.build().expect("complete copied atlas must pass shared validation");

        let rebuilt_font = rebuilt.font_id("body").expect("copied body font name must survive");
        assert_eq!(rebuilt.get_font_height(rebuilt_font), source.get_font_height(source_font));
        assert_eq!(rebuilt.get_font_baseline(rebuilt_font), source.get_font_baseline(source_font));
        assert!(rebuilt.get_char_entry(rebuilt_font, 'a').is_some());
        assert!(rebuilt.contains_icon(artwork));
        let artwork_size = rebuilt.get_icon_size(artwork);
        assert_eq!((artwork_size.width, artwork_size.height), (2, 2));
    }

    /// Verifies semantic font replacement retains unrelated application typography by name.
    #[test]
    fn selective_font_copy_replaces_roles_without_dropping_application_fonts() {
        let fonts = [
            FontAsset { name: "body", path: FONT_PATH, size: 10 },
            FontAsset {
                name: "application-code",
                path: FONT_PATH,
                size: 8,
            },
        ];
        let config = Config {
            texture_width: 512,
            texture_height: 256,
            white_icon: String::from(WHITE_PATH),
            icons: &[],
            fonts: &fonts,
        };
        let source = Builder::from_config(&config)
            .expect("source typography must fit the fixture atlas")
            .build()
            .expect("source typography must satisfy atlas validation");
        let mut replacement =
            Builder::from_atlas_with_size_excluding_fonts(&source, 512, 256, &["body"]).expect("the application font must copy without the replaced body role");
        replacement
            .add_font_named("body", FONT_PATH, 14)
            .expect("the replacement body recipe must fit beside the application font");
        let replacement = replacement.build().expect("selectively rebuilt atlas must validate");

        assert_eq!(replacement.get_font_size(replacement.font_id("body").unwrap()), 14);
        assert_eq!(replacement.get_font_size(replacement.font_id("application-code").unwrap()), 8);
        assert_eq!(replacement.clone_font_table().len(), 2);
    }

    /// Verifies duplicate resource keys are structural errors detected before opening the supplied
    /// path, so they neither mask as I/O failures nor consume atlas space.
    #[test]
    fn duplicate_names_fail_before_asset_io() {
        let fonts = [FontAsset { name: "body", path: FONT_PATH, size: 10 }];
        let config = Config {
            texture_width: 512,
            texture_height: 256,
            white_icon: String::from(WHITE_PATH),
            icons: &[],
            fonts: &fonts,
        };
        let mut builder = Builder::from_config(&config).expect("fixture assets must build an atlas candidate");

        let icon_error = builder
            .add_icon_named("white", "path-that-must-not-be-read")
            .expect_err("the existing white key must be rejected");
        assert!(matches!(
            icon_error,
            BuilderError::Atlas {
                source: AtlasError::DuplicateIconName { name },
            } if name == "white"
        ));

        let font_error = builder
            .add_font_named("body", "path-that-must-not-be-read", 0)
            .expect_err("the existing body key must be rejected");
        assert!(matches!(
            font_error,
            BuilderError::Atlas {
                source: AtlasError::DuplicateFontName { name },
            } if name == "body"
        ));
    }

    /// Verifies builder file reads accept the exact byte boundary and inspect no more than one byte
    /// beyond it when distinguishing oversized input.
    #[test]
    fn asset_reads_are_bounded_before_decode_or_font_parsing() {
        let exact = Builder::read_bounded(Cursor::new([1_u8, 2, 3]), 3).expect("exact-boundary input must be accepted");
        assert_eq!(exact, [1, 2, 3]);

        let error = Builder::read_bounded(Cursor::new([1_u8, 2, 3, 4, 5]), 3).expect_err("one excess byte must reject the input");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("3-byte builder input limit"));
    }

    /// Verifies a font that runs out of texture space late in planning leaves pixels, metadata, and
    /// future packer placement identical to a builder that never attempted the failed insertion.
    #[test]
    fn failed_font_insertion_is_transactional() {
        let config = Config {
            texture_width: 64,
            texture_height: 32,
            white_icon: String::from(WHITE_PATH),
            icons: &[],
            fonts: &[],
        };
        let mut attempted = Builder::from_config(&config).expect("small fixture atlas must contain its white tile");
        let mut untouched = Builder::from_config(&config).expect("control atlas must contain its white tile");
        let pixels_before = attempted.candidate.pixels.clone();

        let error = attempted
            .add_font_named("too-large", FONT_PATH, 10)
            .expect_err("printable ASCII must not fit beside the white tile");
        assert!(matches!(&error, BuilderError::Asset { .. }));
        assert!(
            error.to_string().contains("glyph '4'"),
            "fixture must fail only after earlier glyphs were reserved"
        );
        assert!(attempted.candidate.fonts.is_empty());
        assert!(
            attempted
                .candidate
                .pixels
                .iter()
                .zip(&pixels_before)
                .all(|(left, right)| (left.x, left.y, left.z, left.w) == (right.x, right.y, right.z, right.w))
        );

        // Identical placement of a subsequent real asset proves the cloned planning packer was not
        // published when the font failed after reserving earlier glyphs.
        attempted
            .add_icon_named("after-failure", FILE_ICON_PATH)
            .expect("control icon must fit after failed font");
        untouched
            .add_icon_named("after-failure", FILE_ICON_PATH)
            .expect("control icon must fit in untouched builder");
        let attempted_rect = attempted.candidate.icons.last().unwrap().1.rect;
        let untouched_rect = untouched.candidate.icons.last().unwrap().1.rect;
        assert_eq!(
            (attempted_rect.x, attempted_rect.y, attempted_rect.width, attempted_rect.height),
            (untouched_rect.x, untouched_rect.y, untouched_rect.width, untouched_rect.height)
        );
        assert!(
            attempted
                .candidate
                .pixels
                .iter()
                .zip(&untouched.candidate.pixels)
                .all(|(left, right)| (left.x, left.y, left.z, left.w) == (right.x, right.y, right.z, right.w))
        );
    }
}
