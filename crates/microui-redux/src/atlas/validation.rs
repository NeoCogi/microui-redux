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

//! Structural validation shared by serialized and builder-produced atlases.

use super::*;
use crate::image::{CheckedImageDimensions, ImageError};
use std::{
    collections::HashSet,
    error::Error,
    fmt::{Display, Formatter},
};

/// Concrete failure returned when atlas bytes or metadata cannot form a safe runtime atlas.
///
/// Variants preserve the invalid value and its resource name where applicable. Callers can match
/// failures without parsing an I/O error string or accepting a partially initialized fallback.
#[derive(Debug)]
pub enum AtlasError {
    /// Atlas texture dimensions or pixel data violate the shared image contract.
    Image {
        /// Concrete dimension, storage, raw-buffer, or decoder failure from the image layer.
        source: ImageError,
    },
    /// Two icons use the same lookup key.
    DuplicateIconName {
        /// Repeated icon name.
        name: String,
    },
    /// Two fonts use the same lookup key.
    DuplicateFontName {
        /// Repeated font name.
        name: String,
    },
    /// Two entries in one font describe the same Unicode scalar value.
    DuplicateGlyph {
        /// Name of the font containing the duplicate.
        font: String,
        /// Repeated Unicode scalar value.
        character: char,
    },
    /// No icon has the exact rendering-tile name `white`.
    MissingWhiteIcon,
    /// An icon rectangle is empty, negative, or outside the texture.
    InvalidIconRectangle {
        /// Name of the icon owning the invalid rectangle.
        name: String,
        /// Invalid rectangle in atlas pixel coordinates.
        rectangle: Recti,
    },
    /// A pixel inside the `white` icon is not opaque white.
    WhiteIconNotOpaqueWhite {
        /// Horizontal coordinate of the first invalid pixel.
        x: usize,
        /// Vertical coordinate of the first invalid pixel.
        y: usize,
        /// Actual red, green, blue, and alpha channels.
        rgba: [u8; 4],
    },
    /// A font line height is zero or cannot be represented by runtime text coordinates.
    InvalidLineSize {
        /// Name of the invalid font.
        font: String,
        /// Invalid distance between baselines.
        line_size: usize,
    },
    /// A font baseline lies outside its line box.
    InvalidBaseline {
        /// Name of the invalid font.
        font: String,
        /// Invalid top-to-baseline distance.
        baseline: i32,
        /// Validated line height used as the upper bound.
        line_size: usize,
    },
    /// A requested font size is zero or cannot be represented by runtime text coordinates.
    InvalidFontSize {
        /// Name of the invalid font.
        font: String,
        /// Invalid requested pixel size.
        font_size: usize,
    },
    /// A font has no underscore glyph to use for missing characters.
    MissingFallbackGlyph {
        /// Name of the font missing `_`.
        font: String,
    },
    /// A glyph rectangle has a negative component or extends outside the texture.
    InvalidGlyphRectangle {
        /// Name of the font containing the invalid glyph.
        font: String,
        /// Unicode scalar value owning the invalid rectangle.
        character: char,
        /// Invalid rectangle in atlas pixel coordinates.
        rectangle: Recti,
    },
}

impl Display for AtlasError {
    /// Formats one stable diagnostic without discarding matchable error structure.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        // Keep diagnostics next to their concrete variants so adding a validation rule requires a
        // deliberate public message instead of falling back to an opaque debug dump.
        match self {
            Self::Image { source } => write!(formatter, "atlas image is invalid: {source}"),
            Self::DuplicateIconName { name } => write!(formatter, "atlas icon name `{name}` is duplicated"),
            Self::DuplicateFontName { name } => write!(formatter, "atlas font name `{name}` is duplicated"),
            Self::DuplicateGlyph { font, character } => {
                write!(formatter, "atlas font `{font}` contains duplicate glyph {character:?}")
            }
            Self::MissingWhiteIcon => formatter.write_str("atlas does not contain the required icon named `white`"),
            Self::InvalidIconRectangle { name, rectangle } => {
                write!(formatter, "atlas icon `{name}` has invalid rectangle {rectangle:?}")
            }
            Self::WhiteIconNotOpaqueWhite { x, y, rgba } => {
                write!(formatter, "atlas `white` icon pixel at ({x}, {y}) is {rgba:?}, expected [255, 255, 255, 255]")
            }
            Self::InvalidLineSize { font, line_size } => {
                write!(formatter, "atlas font `{font}` has invalid line size {line_size}")
            }
            Self::InvalidBaseline { font, baseline, line_size } => {
                write!(formatter, "atlas font `{font}` baseline {baseline} is outside 0..={line_size}")
            }
            Self::InvalidFontSize { font, font_size } => {
                write!(formatter, "atlas font `{font}` has invalid requested size {font_size}")
            }
            Self::MissingFallbackGlyph { font } => write!(formatter, "atlas font `{font}` does not contain the required `_` fallback glyph"),
            Self::InvalidGlyphRectangle { font, character, rectangle } => {
                write!(formatter, "atlas font `{font}` glyph {character:?} has invalid rectangle {rectangle:?}")
            }
        }
    }
}

impl Error for AtlasError {
    /// Exposes the concrete image error for failures originating below atlas validation.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Structural failures are fully represented by this enum. Image validation keeps its own
        // storage or decoder cause so callers can continue down the standard error chain.
        match self {
            Self::Image { source } => Some(source),
            _ => None,
        }
    }
}

impl From<ImageError> for AtlasError {
    /// Retains one image-layer failure as the atlas error's concrete source.
    fn from(source: ImageError) -> Self {
        // Composition keeps every typed image field and its error chain available without
        // repeating the same variants in the atlas vocabulary.
        Self::Image { source }
    }
}

/// Applies the image layer's single dimension and allocation policy to an atlas extent.
pub(super) fn checked_atlas_dimensions(width: usize, height: usize) -> Result<CheckedImageDimensions, AtlasError> {
    // The checked image token remains the sole owner of axis, arithmetic, addressability, and
    // allocation-budget validation. AtlasError composes that concrete failure rather than
    // duplicating its fields under atlas-specific variant names.
    CheckedImageDimensions::try_new(width, height)
        .map_err(ImageError::from)
        .map_err(AtlasError::from)
}

impl AtlasCandidate {
    /// Creates a blank candidate for the incremental builder after validating allocation sizes.
    #[cfg(feature = "builder")]
    pub(super) fn blank(width: usize, height: usize) -> Result<Self, AtlasError> {
        let dimensions = checked_atlas_dimensions(width, height)?;
        // Allocate only after dimensions and byte counts pass all representability checks.
        let pixels = vec![Color4b::default(); dimensions.pixel_count];
        Ok(Self {
            dimensions,
            pixels,
            fonts: Vec::new(),
            icons: Vec::new(),
        })
    }

    /// Creates a source-backed candidate while preserving all metadata until validation.
    pub(super) fn from_decoded(
        dimensions: CheckedImageDimensions,
        pixels: Vec<Color4b>,
        fonts: Vec<(String, FontCandidate)>,
        icons: Vec<(String, Icon)>,
    ) -> Self {
        // Preserve the decoder output unchanged. Producers already guarantee the exact normalized
        // pixel count; the sole finalizer below owns every metadata invariant.
        Self { dimensions, pixels, fonts, icons }
    }

    /// Checks every structural invariant before runtime lookup tables are materialized.
    fn validate(&self) -> Result<(), AtlasError> {
        // Both candidate producers allocate or decode exactly the validated pixel count. Keep the
        // assertion local to their shared boundary without exposing an error callers cannot cause.
        debug_assert_eq!(self.pixels.len(), self.dimensions.pixel_count);

        let mut icon_names = HashSet::with_capacity(self.icons.len());
        let mut white_rectangle = None;
        for (name, icon) in &self.icons {
            if !icon_names.insert(name.as_str()) {
                return Err(AtlasError::DuplicateIconName { name: name.clone() });
            }
            if !rectangle_fits(icon.rect, &self.dimensions, false) {
                return Err(AtlasError::InvalidIconRectangle { name: name.clone(), rectangle: icon.rect });
            }
            if name == "white" {
                // Duplicate-name validation guarantees at most one exact rendering tile.
                white_rectangle = Some(icon.rect);
            }
        }
        let white_rectangle = white_rectangle.ok_or(AtlasError::MissingWhiteIcon)?;
        self.validate_white_pixels(white_rectangle)?;

        let mut font_names = HashSet::with_capacity(self.fonts.len());
        for (name, font) in &self.fonts {
            if !font_names.insert(name.as_str()) {
                return Err(AtlasError::DuplicateFontName { name: name.clone() });
            }
            self.validate_font(name, font)?;
        }
        Ok(())
    }

    /// Verifies one font's metrics, unique fallback, and glyph rectangles.
    pub(super) fn validate_font(&self, name: &str, font: &FontCandidate) -> Result<(), AtlasError> {
        if font.line_size == 0 || font.line_size > i32::MAX as usize {
            return Err(AtlasError::InvalidLineSize {
                font: name.to_string(),
                line_size: font.line_size,
            });
        }
        if font.baseline < 0 || font.baseline as usize > font.line_size {
            return Err(AtlasError::InvalidBaseline {
                font: name.to_string(),
                baseline: font.baseline,
                line_size: font.line_size,
            });
        }
        if font.font_size == 0 || font.font_size > i32::MAX as usize {
            return Err(AtlasError::InvalidFontSize {
                font: name.to_string(),
                font_size: font.font_size,
            });
        }

        let mut characters = HashSet::with_capacity(font.entries.len());
        let mut has_fallback = false;
        for (character, entry) in &font.entries {
            if !characters.insert(*character) {
                return Err(AtlasError::DuplicateGlyph {
                    font: name.to_string(),
                    character: *character,
                });
            }
            if !rectangle_fits(entry.rect, &self.dimensions, true) {
                return Err(AtlasError::InvalidGlyphRectangle {
                    font: name.to_string(),
                    character: *character,
                    rectangle: entry.rect,
                });
            }
            has_fallback |= *character == '_';
        }
        if !has_fallback {
            return Err(AtlasError::MissingFallbackGlyph { font: name.to_string() });
        }
        Ok(())
    }

    /// Verifies every pixel covered by the required rendering tile is opaque white.
    fn validate_white_pixels(&self, rectangle: Recti) -> Result<(), AtlasError> {
        // Icon rectangle validation already proved every coordinate and row-major index is inside
        // the pixel buffer, so these casts and additions are bounded.
        let left = rectangle.x as usize;
        let top = rectangle.y as usize;
        let right = left + rectangle.width as usize;
        let bottom = top + rectangle.height as usize;
        for y in top..bottom {
            for x in left..right {
                let pixel = self.pixels[x + y * self.dimensions.width];
                let rgba = [pixel.x, pixel.y, pixel.z, pixel.w];
                if rgba != [0xFF; 4] {
                    return Err(AtlasError::WhiteIconNotOpaqueWhite { x, y, rgba });
                }
            }
        }
        Ok(())
    }
}

impl AtlasHandle {
    /// Finalizes one candidate after all shared validation succeeds.
    pub(super) fn finish(candidate: AtlasCandidate) -> Result<Self, AtlasError> {
        candidate.validate()?;
        // Destructure only after validation so duplicate glyphs remain observable until rejection.
        let AtlasCandidate { dimensions, pixels, fonts, icons } = candidate;
        let fonts = fonts
            .into_iter()
            .map(|(name, font)| {
                let font = Font {
                    id: font.id,
                    line_size: font.line_size,
                    baseline: font.baseline,
                    font_size: font.font_size,
                    entries: font.entries.into_iter().collect(),
                };
                (name, font)
            })
            .collect::<Vec<_>>();
        // Stable resource identities are private and can enter a candidate only by allocation or
        // exact copying. Building explicit local indices here keeps metric and paint lookup O(1)
        // even when a derived atlas reordered resources while replacing semantic fonts.
        let font_slots = fonts.iter().enumerate().map(|(slot, (_, font))| (font.id, slot)).collect::<HashMap<_, _>>();
        let icon_slots = icons.iter().enumerate().map(|(slot, (_, icon))| (icon.id, slot)).collect::<HashMap<_, _>>();
        debug_assert_eq!(font_slots.len(), fonts.len(), "candidate contains duplicate internal font identities");
        debug_assert_eq!(icon_slots.len(), icons.len(), "candidate contains duplicate internal icon identities");
        let white_icon = icons
            .iter()
            .find_map(|(name, icon)| (name == "white").then_some(icon.id))
            .expect("validated candidate must contain the required white icon");
        let atlas = Atlas {
            width: dimensions.width,
            height: dimensions.height,
            pixels,
            fonts,
            icons,
            white_icon,
            font_slots,
            icon_slots,
        };
        // This is the only direct AtlasHandle construction site in production code.
        Ok(Self(Rc::new(atlas)))
    }
}

/// Reports whether a rectangle is nonnegative, optionally empty, and completely in the texture.
fn rectangle_fits(rectangle: Recti, dimensions: &CheckedImageDimensions, allow_empty: bool) -> bool {
    // i64 arithmetic makes even i32::MAX + i32::MAX representable, so malformed metadata returns
    // a typed error instead of overflowing in debug builds or wrapping in release builds.
    let x = i64::from(rectangle.x);
    let y = i64::from(rectangle.y);
    let width = i64::from(rectangle.width);
    let height = i64::from(rectangle.height);
    if x < 0 || y < 0 || width < 0 || height < 0 {
        return false;
    }
    if !allow_empty && (width == 0 || height == 0) {
        return false;
    }
    x + width <= dimensions.width as i64 && y + height <= dimensions.height as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageStorageError;

    /// Returns a small pixel buffer whose second pixel is the required opaque-white rendering
    /// source while the other pixels remain visibly distinct.
    fn valid_pixels() -> [u8; 16] {
        // A non-white first pixel proves validation follows the named rectangle instead of assuming
        // the rendering tile occupies slot or pixel zero.
        [1, 2, 3, 4, 0xFF, 0xFF, 0xFF, 0xFF, 5, 6, 7, 8, 9, 10, 11, 12]
    }

    /// Returns named icon metadata with `white` deliberately stored after another icon.
    fn valid_icons() -> [(&'static str, Recti); 2] {
        // Both rectangles have positive area and remain inside the shared two-by-two fixture.
        [("sample", Recti::new(0, 0, 1, 1)), ("white", Recti::new(1, 0, 1, 1))]
    }

    /// Creates ordinary horizontal glyph metadata for one caller-selected atlas rectangle.
    fn glyph(rectangle: Recti) -> CharEntry {
        // Unit advance and zero bearing keep positive fixtures easy to measure while rectangle edge
        // cases remain the only variable under test.
        CharEntry {
            offset: Vec2i::new(0, 0),
            advance: Vec2i::new(1, 0),
            rect: rectangle,
        }
    }

    /// Returns one valid font entry over the supplied duplicate-preserving glyph slice.
    fn valid_font(entries: &[(char, CharEntry)]) -> FontEntry<'_> {
        // A baseline in the middle of a positive two-pixel line satisfies every metric invariant.
        FontEntry {
            line_size: 2,
            baseline: 1,
            font_size: 1,
            entries,
        }
    }

    /// Borrows caller-owned fixture slices into one raw serialized atlas description.
    fn raw_source<'source>(
        width: usize,
        height: usize,
        pixels: &'source [u8],
        icons: &'source [(&'source str, Recti)],
        fonts: &'source [(&'source str, FontEntry<'source>)],
    ) -> AtlasSource<'source> {
        // Keeping this helper borrow-only avoids hiding copies, leaks, or alternate construction
        // paths from tests that are meant to exercise the public TryFrom boundary.
        AtlasSource {
            width,
            height,
            pixels,
            icons,
            fonts,
            format: SourceFormat::Raw,
        }
    }

    /// Extracts the typed failure returned by the public atlas construction boundary.
    fn construction_error(source: &AtlasSource<'_>) -> AtlasError {
        // A successful result is always a test-fixture bug; matching explicitly avoids requiring
        // AtlasHandle to implement Debug solely for Result::unwrap_err.
        match AtlasHandle::try_from(source) {
            Ok(_) => panic!("invalid atlas fixture unexpectedly passed validation"),
            Err(error) => error,
        }
    }

    /// Verifies the smallest complete source accepts name-based white lookup and an empty glyph.
    #[test]
    fn minimal_valid_source_accepts_reordered_white_icon_and_zero_area_space() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let entries = [
            ('_', glyph(Recti::new(0, 1, 1, 1))),
            // Empty glyph bitmaps such as space are valid even at the inclusive lower-right edge.
            (' ', glyph(Recti::new(2, 2, 0, 0))),
        ];
        let fonts = [("body", valid_font(&entries))];
        let source = raw_source(2, 2, &pixels, &icons, &fonts);

        let atlas = AtlasHandle::try_from(&source).expect("the complete minimal atlas must validate");
        let white = atlas.get_icon_rect(atlas.white_icon());
        let space = atlas
            .get_char_entry(atlas.font_id("body").expect("body font must survive finalization"), ' ')
            .expect("zero-area space glyph must survive finalization");

        assert_eq!((white.x, white.y, white.width, white.height), (1, 0, 1, 1));
        assert_eq!((space.rect.x, space.rect.y, space.rect.width, space.rect.height), (2, 2, 0, 0));
    }

    /// Verifies zero and coordinate-unrepresentable dimensions fail before pixel decoding.
    #[test]
    fn dimensions_must_be_positive_and_fit_runtime_rectangles() {
        let cases = [(0, 1), (1, 0), (i32::MAX as usize + 1, 1), (1, i32::MAX as usize + 1)];

        for (width, height) in cases {
            let error = match checked_atlas_dimensions(width, height) {
                Ok(_) => panic!("invalid dimensions {width}x{height} unexpectedly validated"),
                Err(error) => error,
            };
            assert!(matches!(
                error,
                AtlasError::Image {
                    source: ImageError::Storage {
                        source: ImageStorageError::DimensionsOutOfRange {
                            width: actual_width,
                            height: actual_height,
                        },
                    },
                } if actual_width == width && actual_height == height
            ));
        }
    }

    /// Verifies accepted component dimensions cannot imply an unaddressable allocation.
    #[test]
    fn pixel_storage_overflow_is_rejected_without_allocating() {
        let width = i32::MAX as usize;
        let height = i32::MAX as usize;

        // Both axes pass the rectangle-domain check, so this specifically exercises checked pixel,
        // RGBA-byte, and isize allocation bounds without constructing a correspondingly large Vec.
        let error = match checked_atlas_dimensions(width, height) {
            Ok(_) => panic!("unaddressable atlas storage unexpectedly validated"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            AtlasError::Image {
                source: ImageError::Storage {
                    source: ImageStorageError::Overflow {
                        width: actual_width,
                        height: actual_height,
                    },
                },
            } if actual_width == width && actual_height == height
        ));
    }

    /// Verifies serialized atlas dimensions cannot exceed the fixed decoded-image budget.
    #[test]
    fn pixel_storage_budget_is_rejected_before_raw_length_validation() {
        let source = raw_source(4096, 4097, &[], &[], &[]);

        // Empty bytes would otherwise report a raw-length mismatch. The budget error proves the
        // public source boundary validates its extent before inspecting or allocating pixel data.
        let error = construction_error(&source);
        assert!(matches!(
            error,
            AtlasError::Image {
                source: ImageError::Storage {
                    source: ImageStorageError::TooLarge {
                        width: 4096,
                        height: 4097,
                        required_bytes: 67_125_248,
                        maximum_bytes: crate::image::MAX_DECODED_RGBA_BYTES,
                    },
                },
            }
        ));
    }

    /// Verifies the blank builder candidate applies the same budget before allocating its pixels.
    #[cfg(feature = "builder")]
    #[test]
    fn blank_candidate_shares_the_decoded_image_storage_budget() {
        let error = match AtlasCandidate::blank(4096, 4097) {
            Ok(_) => panic!("an oversized blank atlas unexpectedly allocated"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AtlasError::Image {
                source: ImageError::Storage {
                    source: ImageStorageError::TooLarge {
                        width: 4096,
                        height: 4097,
                        required_bytes: 67_125_248,
                        maximum_bytes: crate::image::MAX_DECODED_RGBA_BYTES,
                    },
                },
            }
        ));
    }

    /// Verifies raw RGBA input rejects both truncation and ignored trailing bytes.
    #[test]
    fn raw_pixel_bytes_must_match_the_checked_length_exactly() {
        let short = [0_u8; 15];
        let long = [0_u8; 17];

        for pixels in [&short[..], &long[..]] {
            let source = raw_source(2, 2, pixels, &[], &[]);
            let error = construction_error(&source);
            assert!(matches!(
                error,
                AtlasError::Image {
                    source: ImageError::RawPixelLengthMismatch { expected: 16, actual },
                } if actual == pixels.len()
            ));
        }
    }

    /// Verifies duplicate icon names remain observable instead of resolving to the first slot.
    #[test]
    fn duplicate_icon_names_are_rejected_before_lookup_tables_are_built() {
        let pixels = valid_pixels();
        let icons = [
            ("white", Recti::new(1, 0, 1, 1)),
            ("sample", Recti::new(0, 0, 1, 1)),
            ("sample", Recti::new(0, 1, 1, 1)),
        ];
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];
        let fonts = [("body", valid_font(&entries))];
        let source = raw_source(2, 2, &pixels, &icons, &fonts);

        let error = construction_error(&source);
        assert!(matches!(error, AtlasError::DuplicateIconName { name } if name == "sample"));
    }

    /// Verifies duplicate font names cannot leave one same-name table entry unreachable.
    #[test]
    fn duplicate_font_names_are_rejected_before_lookup_tables_are_built() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];
        let fonts = [("body", valid_font(&entries)), ("body", valid_font(&entries))];
        let source = raw_source(2, 2, &pixels, &icons, &fonts);

        let error = construction_error(&source);
        assert!(matches!(error, AtlasError::DuplicateFontName { name } if name == "body"));
    }

    /// Verifies repeated glyph keys are rejected before HashMap collection could replace one.
    #[test]
    fn duplicate_glyphs_are_rejected_with_their_font_and_character() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let entries = [
            ('_', glyph(Recti::new(0, 1, 1, 1))),
            ('a', glyph(Recti::new(0, 1, 1, 1))),
            ('a', glyph(Recti::new(1, 1, 1, 1))),
        ];
        let fonts = [("body", valid_font(&entries))];
        let source = raw_source(2, 2, &pixels, &icons, &fonts);

        let error = construction_error(&source);
        assert!(matches!(
            error,
            AtlasError::DuplicateGlyph { font, character: 'a' } if font == "body"
        ));
    }

    /// Verifies every invalid icon rectangle shape returns one typed error without i32 overflow.
    #[test]
    fn icon_rectangles_reject_negative_empty_out_of_bounds_and_overflowing_values() {
        let cases = [
            Recti::new(-1, 0, 1, 1),
            Recti::new(0, 0, -1, 1),
            Recti::new(0, 0, 0, 1),
            Recti::new(2, 0, 1, 1),
            Recti::new(i32::MAX, 0, i32::MAX, 1),
        ];
        let pixels = valid_pixels();
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];
        let fonts = [("body", valid_font(&entries))];

        for rectangle in cases {
            let icons = [("white", rectangle)];
            let source = raw_source(2, 2, &pixels, &icons, &fonts);
            let error = construction_error(&source);
            match error {
                AtlasError::InvalidIconRectangle { name, rectangle: actual } => {
                    assert_eq!(name, "white");
                    assert_eq!(
                        (actual.x, actual.y, actual.width, actual.height),
                        (rectangle.x, rectangle.y, rectangle.width, rectangle.height)
                    );
                }
                other => panic!("expected InvalidIconRectangle, received {other:?}"),
            }
        }
    }

    /// Verifies malformed glyph rectangles fail while empty bitmaps remain an explicit valid case.
    #[test]
    fn glyph_rectangles_reject_invalid_values_but_allow_zero_area() {
        let invalid = [
            Recti::new(-1, 0, 1, 1),
            Recti::new(0, 0, -1, 1),
            Recti::new(2, 0, 1, 1),
            Recti::new(i32::MAX, 0, i32::MAX, 1),
        ];
        let pixels = valid_pixels();
        let icons = valid_icons();

        for rectangle in invalid {
            let entries = [('_', glyph(rectangle))];
            let fonts = [("body", valid_font(&entries))];
            let source = raw_source(2, 2, &pixels, &icons, &fonts);
            let error = construction_error(&source);
            match error {
                AtlasError::InvalidGlyphRectangle { font, character: '_', rectangle: actual } => {
                    assert_eq!(font, "body");
                    assert_eq!(
                        (actual.x, actual.y, actual.width, actual.height),
                        (rectangle.x, rectangle.y, rectangle.width, rectangle.height)
                    );
                }
                other => panic!("expected InvalidGlyphRectangle, received {other:?}"),
            }
        }

        // A fallback with no bitmap remains structurally usable because its advance still supplies
        // deterministic text layout and no pixel is sampled from its boundary coordinate.
        let entries = [('_', glyph(Recti::new(2, 2, 0, 0)))];
        let fonts = [("body", valid_font(&entries))];
        let source = raw_source(2, 2, &pixels, &icons, &fonts);
        assert!(AtlasHandle::try_from(&source).is_ok());
    }

    /// Verifies the required rendering tile uses one exact case-sensitive semantic name.
    #[test]
    fn white_icon_requires_the_exact_lowercase_name() {
        let pixels = valid_pixels();
        let icons = [("WHITE", Recti::new(1, 0, 1, 1))];
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];
        let fonts = [("body", valid_font(&entries))];
        let source = raw_source(2, 2, &pixels, &icons, &fonts);

        assert!(matches!(construction_error(&source), AtlasError::MissingWhiteIcon));
    }

    /// Verifies every covered pixel and every RGBA channel participates in white-tile validation.
    #[test]
    fn white_icon_checks_rgb_alpha_and_pixels_after_the_first() {
        let cases = [
            (
                [1, 2, 3, 4, 0xFE, 0xFF, 0xFF, 0xFF, 5, 6, 7, 8, 9, 10, 11, 12],
                Recti::new(1, 0, 1, 1),
                (1, 0, [0xFE, 0xFF, 0xFF, 0xFF]),
            ),
            (
                [1, 2, 3, 4, 0xFF, 0xFF, 0xFF, 0xFE, 5, 6, 7, 8, 9, 10, 11, 12],
                Recti::new(1, 0, 1, 1),
                (1, 0, [0xFF, 0xFF, 0xFF, 0xFE]),
            ),
            (
                [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xFF, 0xFF, 5, 6, 7, 8, 9, 10, 11, 12],
                Recti::new(0, 0, 2, 1),
                (1, 0, [0xFF, 0xFE, 0xFF, 0xFF]),
            ),
            (
                [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xFF, 5, 6, 7, 8, 9, 10, 11, 12],
                Recti::new(0, 0, 2, 1),
                (1, 0, [0xFF, 0xFF, 0xFE, 0xFF]),
            ),
        ];
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];
        let fonts = [("body", valid_font(&entries))];

        for (pixels, rectangle, (expected_x, expected_y, expected_rgba)) in cases {
            let icons = [("white", rectangle)];
            let source = raw_source(2, 2, &pixels, &icons, &fonts);
            let error = construction_error(&source);
            assert!(matches!(
                error,
                AtlasError::WhiteIconNotOpaqueWhite { x, y, rgba }
                    if x == expected_x && y == expected_y && rgba == expected_rgba
            ));
        }
    }

    /// Verifies zero and coordinate-unrepresentable line heights are rejected distinctly.
    #[test]
    fn line_size_must_be_positive_and_fit_runtime_coordinates() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];

        for line_size in [0, i32::MAX as usize + 1] {
            let fonts = [(
                "body",
                FontEntry {
                    line_size,
                    baseline: 0,
                    font_size: 1,
                    entries: &entries,
                },
            )];
            let source = raw_source(2, 2, &pixels, &icons, &fonts);
            assert!(matches!(
                construction_error(&source),
                AtlasError::InvalidLineSize { font, line_size: actual }
                    if font == "body" && actual == line_size
            ));
        }
    }

    /// Verifies a baseline is constrained to the inclusive validated line box.
    #[test]
    fn baseline_must_lie_inside_the_line_box() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];

        for baseline in [-1, 3] {
            let fonts = [(
                "body",
                FontEntry {
                    line_size: 2,
                    baseline,
                    font_size: 1,
                    entries: &entries,
                },
            )];
            let source = raw_source(2, 2, &pixels, &icons, &fonts);
            assert!(matches!(
                construction_error(&source),
                AtlasError::InvalidBaseline {
                    font,
                    baseline: actual,
                    line_size: 2,
                } if font == "body" && actual == baseline
            ));
        }
    }

    /// Verifies requested font sizes remain positive and representable by runtime coordinates.
    #[test]
    fn font_size_must_be_positive_and_fit_runtime_coordinates() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];

        for font_size in [0, i32::MAX as usize + 1] {
            let fonts = [(
                "body",
                FontEntry {
                    line_size: 2,
                    baseline: 1,
                    font_size,
                    entries: &entries,
                },
            )];
            let source = raw_source(2, 2, &pixels, &icons, &fonts);
            assert!(matches!(
                construction_error(&source),
                AtlasError::InvalidFontSize { font, font_size: actual }
                    if font == "body" && actual == font_size
            ));
        }
    }

    /// Verifies every nonempty font provides the fallback required by runtime text lookup.
    #[test]
    fn font_requires_an_underscore_fallback_glyph() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let entries = [('a', glyph(Recti::new(0, 1, 1, 1)))];
        let fonts = [("body", valid_font(&entries))];
        let source = raw_source(2, 2, &pixels, &icons, &fonts);

        assert!(matches!(
            construction_error(&source),
            AtlasError::MissingFallbackGlyph { font } if font == "body"
        ));
    }

    /// Verifies a low-level atlas may intentionally contain no font table at all.
    #[test]
    fn no_font_low_level_atlas_remains_valid() {
        let pixels = valid_pixels();
        let icons = valid_icons();
        let source = raw_source(2, 2, &pixels, &icons, &[]);

        let atlas = AtlasHandle::try_from(&source).expect("standalone render atlases may omit fonts");
        assert!(atlas.clone_font_table().is_empty());
        assert_eq!(atlas.clone_icon_table().len(), icons.len());
    }

    /// Encodes a compact RGBA PNG for checked atlas-header tests.
    #[cfg(feature = "png_source")]
    fn encode_rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        // The encoder is scoped so its mutable borrow ends before the owned byte vector is returned.
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("test PNG header must encode");
            writer.write_image_data(pixels).expect("test PNG pixels must encode");
        }
        encoded
    }

    /// Encodes one animated PNG frame so static-atlas rejection is exercised with valid bytes.
    #[cfg(feature = "png_source")]
    fn encode_rgba_apng(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_animated(1, 0).expect("test animation metadata must encode");
            let mut writer = encoder.write_header().expect("test APNG header must encode");
            writer.write_image_data(pixels).expect("test APNG frame must encode");
        }
        encoded
    }

    /// Verifies a valid static PNG reaches complete metadata validation and runtime finalization.
    #[cfg(feature = "png_source")]
    #[test]
    fn valid_png_source_constructs_the_complete_atlas() {
        let pixels = valid_pixels();
        let encoded = encode_rgba_png(2, 2, &pixels);
        let icons = valid_icons();
        let entries = [('_', glyph(Recti::new(0, 1, 1, 1)))];
        let fonts = [("body", valid_font(&entries))];
        let source = AtlasSource {
            width: 2,
            height: 2,
            pixels: &encoded,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Png,
        };

        let atlas = AtlasHandle::try_from(&source).expect("valid static PNG bytes and metadata must finalize");

        assert_eq!((atlas.width(), atlas.height()), (2, 2));
        assert!(atlas.font_id("body").is_some());
        assert!(atlas.icon_id("white").is_some());
    }

    /// Verifies malformed compressed bytes retain a concrete decoder source.
    #[cfg(feature = "png_source")]
    #[test]
    fn malformed_png_returns_a_typed_decode_error() {
        let source = AtlasSource {
            width: 1,
            height: 1,
            pixels: &[],
            icons: &[],
            fonts: &[],
            format: SourceFormat::Png,
        };

        assert!(matches!(construction_error(&source), AtlasError::Image { source: ImageError::Decode { .. } }));
    }

    /// Verifies APNG input is rejected by format rather than misread as a partial static texture.
    #[cfg(feature = "png_source")]
    #[test]
    fn animated_png_is_rejected_with_its_own_typed_error() {
        let encoded = encode_rgba_apng(1, 1, &[0xFF; 4]);
        let source = AtlasSource {
            width: 1,
            height: 1,
            pixels: &encoded,
            icons: &[],
            fonts: &[],
            format: SourceFormat::Png,
        };

        assert!(matches!(
            construction_error(&source),
            AtlasError::Image {
                source: ImageError::AnimatedPngUnsupported,
            }
        ));
    }

    /// Verifies PNG dimensions are compared at the header boundary before normalized allocation.
    #[cfg(feature = "png_source")]
    #[test]
    fn png_header_dimensions_must_match_serialized_metadata() {
        let encoded = encode_rgba_png(1, 1, &[0xFF; 4]);
        let source = AtlasSource {
            width: 2,
            height: 1,
            pixels: &encoded,
            icons: &[],
            fonts: &[],
            format: SourceFormat::Png,
        };

        assert!(matches!(
            construction_error(&source),
            AtlasError::Image {
                source: ImageError::DimensionMismatch {
                    expected_width: 2,
                    expected_height: 1,
                    actual_width: 1,
                    actual_height: 1,
                },
            }
        ));
    }
}
