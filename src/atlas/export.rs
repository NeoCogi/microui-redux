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

//! Rust source export for serialized atlases.

use super::*;
use std::{
    fs::File,
    io::{self, BufWriter, Write},
};

/// Checks the deliberately small grammar accepted for a generated Rust constant name.
fn validate_atlas_name(atlas_name: &str) -> io::Result<()> {
    // Restrict names to conventional ASCII constant identifiers. Besides producing idiomatic
    // output, this grammar excludes every lowercase Rust keyword without maintaining an
    // edition-sensitive keyword table. A lone underscore is not a legal item identifier.
    let mut characters = atlas_name.chars();
    let valid_start = characters.next().is_some_and(|character| character == '_' || character.is_ascii_uppercase());
    let valid_tail = characters.all(|character| character == '_' || character.is_ascii_uppercase() || character.is_ascii_digit());
    if atlas_name != "_" && valid_start && valid_tail {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "atlas name must match [A-Z_][A-Z0-9_]* and must not be `_`",
    ))
}

impl AtlasHandle {
    /// Serializes the atlas into one Rust source file for reuse at build time.
    ///
    /// `atlas_name` must be a conventional ASCII Rust constant identifier matching
    /// `[A-Z_][A-Z0-9_]*`; `_` alone is rejected.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidInput`] for an invalid constant name. Pixel encoding, file
    /// creation, and source writes preserve their underlying I/O errors.
    pub fn to_rust_files(&self, atlas_name: &str, format: SourceFormat, path: &str) -> io::Result<()> {
        // Validate and encode before creating the destination so caller mistakes or PNG failures do
        // not truncate an existing generated source file.
        validate_atlas_name(atlas_name)?;
        let (source_pixels, source_format) = self.source_pixels(format)?;
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        self.write_rust_source(&mut writer, atlas_name, source_format, &source_pixels)?;
        writer.flush()
    }

    /// Produces the encoded byte payload and matching source enum expression before file creation.
    fn source_pixels(&self, format: SourceFormat) -> io::Result<(Vec<u8>, &'static str)> {
        // Both branches normalize to an owned byte vector so source emission has one deterministic
        // path independent of the selected storage format.
        match format {
            SourceFormat::Raw => Ok((
                self.0.pixels.iter().flat_map(|pixel| [pixel.x, pixel.y, pixel.z, pixel.w]).collect(),
                "SourceFormat::Raw",
            )),
            #[cfg(feature = "png_source")]
            SourceFormat::Png => Ok((self.png_image_bytes()?, "SourceFormat::Png")),
        }
    }

    /// Writes one complete, deterministic Rust item to an arbitrary byte sink.
    fn write_rust_source(&self, writer: &mut impl Write, atlas_name: &str, source_format: &str, source_pixels: &[u8]) -> io::Result<()> {
        // Explicit imports keep generated dependencies reviewable and avoid a wildcard whose
        // meaning can change when the application prelude grows.
        writeln!(
            writer,
            "use microui_redux::prelude::{{AtlasSource, CharEntry, FontEntry, Recti, SourceFormat, Vec2i}};"
        )?;
        writeln!(writer)?;
        writeln!(writer, "pub const {atlas_name}: AtlasSource<'static> = AtlasSource {{")?;
        writeln!(writer, "    width: {},", self.width())?;
        writeln!(writer, "    height: {},", self.height())?;

        writeln!(writer, "    icons: &[")?;
        for (name, icon) in &self.0.icons {
            // Debug formatting for strings is Rust-literal compatible and escapes quotes,
            // backslashes, control characters, and embedded newlines.
            writeln!(
                writer,
                "        ({name:?}, Recti {{ x: {}, y: {}, width: {}, height: {} }}),",
                icon.rect.x, icon.rect.y, icon.rect.width, icon.rect.height,
            )?;
        }
        writeln!(writer, "    ],")?;

        writeln!(writer, "    fonts: &[")?;
        for (name, font) in &self.0.fonts {
            writeln!(writer, "        ({name:?}, FontEntry {{")?;
            writeln!(writer, "            line_size: {},", font.line_size)?;
            writeln!(writer, "            baseline: {},", font.baseline)?;
            writeln!(writer, "            font_size: {},", font.font_size)?;
            writeln!(writer, "            entries: &[")?;

            let mut entries = font.entries.iter().collect::<Vec<_>>();
            // HashMap iteration order is intentionally unspecified. Sorting by scalar value makes
            // identical atlases produce byte-identical generated source across processes.
            entries.sort_unstable_by_key(|(character, _)| **character);
            for (character, entry) in entries {
                // Character Debug formatting emits a complete Rust character literal, including
                // correct escaping for quotes, backslashes, controls, and Unicode scalars.
                writeln!(
                    writer,
                    "                ({character:?}, CharEntry {{ offset: Vec2i {{ x: {}, y: {} }}, advance: Vec2i {{ x: {}, y: {} }}, rect: Recti {{ x: {}, y: {}, width: {}, height: {} }} }}),",
                    entry.offset.x, entry.offset.y, entry.advance.x, entry.advance.y, entry.rect.x, entry.rect.y, entry.rect.width, entry.rect.height,
                )?;
            }
            writeln!(writer, "            ],")?;
            writeln!(writer, "        }}),")?;
        }
        writeln!(writer, "    ],")?;
        writeln!(writer, "    format: {source_format},")?;
        writeln!(writer, "    pixels: &[")?;
        for row in source_pixels.chunks(16) {
            write!(writer, "        ")?;
            for byte in row {
                write!(writer, "0x{byte:02x}, ")?;
            }
            writeln!(writer)?;
        }
        writeln!(writer, "    ],")?;
        writeln!(writer, "}};")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns one visible glyph entry reused for every literal-escaping case.
    fn glyph() -> CharEntry {
        // Sharing identical geometry keeps the fixture focused on source syntax and ordering.
        CharEntry {
            offset: Vec2i::new(0, 0),
            advance: Vec2i::new(1, 0),
            rect: Recti::new(0, 0, 1, 1),
        }
    }

    /// Builds an atlas containing source strings and characters that require Rust-literal escaping.
    fn escaping_atlas() -> AtlasHandle {
        let pixels = [0xFF; 8];
        let icons = [("white", Recti::new(0, 0, 1, 1)), ("icon\"\\\n", Recti::new(1, 0, 1, 1))];
        let entries = [('z', glyph()), ('\\', glyph()), ('_', glyph()), ('\n', glyph()), ('\'', glyph())];
        let fonts = [(
            "font\"\\\n",
            FontEntry {
                line_size: 1,
                baseline: 1,
                font_size: 1,
                entries: &entries,
            },
        )];

        // Crossing the public construction boundary proves exporter tests never rely on metadata
        // that strict atlas loading would reject.
        AtlasHandle::try_from(&AtlasSource {
            width: 2,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
        })
        .expect("escaping fixture must satisfy the complete atlas contract")
    }

    /// Verifies the accepted constant-name grammar guarantees a Rust item identifier.
    #[test]
    fn atlas_name_requires_a_conventional_ascii_constant_identifier() {
        for valid in ["ATLAS", "PREBUILT_ATLAS", "_PRIVATE", "A1"] {
            validate_atlas_name(valid).expect("conventional constant identifier must validate");
        }
        for invalid in ["", "_", "atlas", "1_ATLAS", "ATLAS-NAME", "ATLAS NAME", "Δ_ATLAS"] {
            let error = validate_atlas_name(invalid).expect_err("invalid token must be rejected before source emission");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }

    /// Verifies generated literals are escaped and glyphs are ordered by Unicode scalar value.
    #[test]
    fn raw_source_export_escapes_literals_and_sorts_hash_map_glyphs() {
        let atlas = escaping_atlas();
        let (pixels, source_format) = atlas.source_pixels(SourceFormat::Raw).expect("raw pixels must encode");
        let mut generated = Vec::new();
        atlas
            .write_rust_source(&mut generated, "TEST_ATLAS", source_format, &pixels)
            .expect("in-memory source emission must succeed");
        let generated = String::from_utf8(generated).expect("Rust source emission must remain UTF-8");

        assert!(generated.contains("(\"icon\\\"\\\\\\n\", Recti"));
        assert!(generated.contains("(\"font\\\"\\\\\\n\", FontEntry"));
        assert!(generated.contains("    format: SourceFormat::Raw,"));
        assert!(generated.contains("        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,"));
        let newline = generated.find("('\\n', CharEntry").expect("newline character must be escaped");
        let quote = generated.find("('\\'', CharEntry").expect("quote character must be escaped");
        let backslash = generated.find("('\\\\', CharEntry").expect("backslash character must be escaped");
        let fallback = generated.find("('_', CharEntry").expect("fallback character must be emitted");
        let z = generated.find("('z', CharEntry").expect("ordinary character must be emitted");
        assert!(newline < quote && quote < backslash && backslash < fallback && fallback < z);
        assert!(generated.starts_with(
            "use microui_redux::prelude::{AtlasSource, CharEntry, FontEntry, Recti, SourceFormat, Vec2i};\n\npub const TEST_ATLAS: AtlasSource<'static>"
        ));
    }

    /// Verifies the optional PNG path emits its matching format and compressed signature bytes.
    #[cfg(feature = "png_source")]
    #[test]
    fn png_source_export_labels_the_encoded_static_payload() {
        let atlas = escaping_atlas();
        let (pixels, source_format) = atlas.source_pixels(SourceFormat::Png).expect("fixture pixels must encode as PNG");
        assert_eq!(source_format, "SourceFormat::Png");
        assert!(pixels.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));

        // Exercise the shared source writer as well as the encoder-specific byte production.
        let mut generated = Vec::new();
        atlas
            .write_rust_source(&mut generated, "PNG_ATLAS", source_format, &pixels)
            .expect("PNG-backed source emission must succeed");
        let generated = String::from_utf8(generated).expect("Rust source emission must remain UTF-8");
        assert!(generated.contains("    format: SourceFormat::Png,"));
        assert!(generated.contains("        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,"));
    }
}
