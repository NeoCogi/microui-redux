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

use super::*;
mod packer;
use packer::{Config as PackerConfig, Packer};
use crate::image::{ImageSource, load_image_bytes};
use fontdue::*;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufWriter, Cursor, Error, Read, Result, Seek, Write},
    path::Path,
};

/// Incrementally constructs an atlas by packing fonts and named bitmap icons.
pub struct Builder {
    /// Rectangle packer used to reserve atlas regions.
    packer: Packer,
    /// Atlas storage being populated by the builder.
    atlas: Atlas,
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
#[cfg(feature = "builder")]
pub struct Config<'a> {
    /// Width of the atlas texture in pixels.
    pub texture_width: usize,
    /// Height of the atlas texture in pixels.
    pub texture_height: usize,
    /// Path to the solid white icon.
    pub white_icon: String,
    /// Named semantic or application icons packed after the white rendering tile.
    pub icons: &'a [IconAsset<'a>],
    /// Legacy fallback font path used when [`Config::fonts`] is empty.
    pub default_font: String,
    /// Legacy fallback font size used when [`Config::fonts`] is empty.
    pub default_font_size: usize,
    /// Fonts baked into the atlas for the printable ASCII range.
    ///
    /// Use the conventional keys `body`, `small`, `title`, `heading`, and `mono`
    /// to populate the built-in semantic roles through [`Style::bind_named_fonts`].
    /// When this slice is empty, [`Config::default_font`] and
    /// [`Config::default_font_size`] are used instead for single-font atlases.
    pub fonts: &'a [FontAsset<'a>],
}

impl Builder {
    /// Creates a builder using the provided configuration and assets.
    #[cfg(feature = "builder")]
    pub fn from_config(config: &Config) -> Result<Builder> {
        let rp_config = PackerConfig {
            width: config.texture_width as _,
            height: config.texture_height as _,

            border_padding: 1,
            rectangle_padding: 1,
        };

        let atlas = Atlas {
            width: config.texture_width,
            height: config.texture_height,
            pixels: vec![Color4b::default(); config.texture_height * config.texture_width],
            fonts: Vec::new(),
            icons: Vec::new(),
        };

        let mut builder = Builder { atlas, packer: Packer::new(rp_config) };

        builder.add_icon_named("white", &config.white_icon)?;
        for icon in config.icons {
            builder.add_icon_named(icon.name, icon.path)?;
        }
        if config.fonts.is_empty() {
            if config.default_font.is_empty() {
                return Err(Error::other("Atlas config must provide either `fonts` or `default_font`"));
            }
            builder.add_font(config.default_font.as_str(), config.default_font_size)?;
        } else {
            for font in config.fonts {
                builder.add_font_named(font.name, font.path, font.size)?;
            }
        }

        Ok(builder)
    }

    /// Adds an icon from the given image path and returns its [`IconId`].
    pub fn add_icon(&mut self, path: &str) -> Result<IconId> {
        let name = Self::format_path(path);
        self.add_icon_named(&name, path)
    }

    /// Adds an icon under a stable lookup key and returns its [`IconId`].
    pub fn add_icon_named(&mut self, name: &str, path: &str) -> Result<IconId> {
        if self.atlas.icons.iter().any(|(existing, _)| existing == name) {
            return Err(Error::other(format!("Icon name '{}' already exists in the atlas", name)));
        }
        let (width, height, pixels) = Self::load_icon(path)?;
        let rect = self.add_tile(width, height, pixels.as_slice())?;
        let id = self.atlas.icons.len();
        let icon = Icon { rect };
        self.atlas.icons.push((name.to_string(), icon.clone()));
        Ok(IconId(id))
    }

    /// Adds the printable ASCII range of a font at the requested size and returns its [`FontId`].
    pub fn add_font(&mut self, path: &str, size: usize) -> Result<FontId> {
        let name = format!("{}-{}", Self::format_path(path), size);
        self.add_font_named(name.as_str(), path, size)
    }

    /// Adds the printable ASCII range of a font under an explicit atlas key and returns its
    /// [`FontId`].
    pub fn add_font_named(&mut self, name: &str, path: &str, size: usize) -> Result<FontId> {
        if self.atlas.fonts.iter().any(|(existing, _)| existing == name) {
            return Err(Error::other(format!("Font name '{}' already exists in the atlas", name)));
        }
        let font = Self::load_font(path)?;
        let mut entries = HashMap::new();
        let mut min_y = i32::MAX;
        let mut max_y = -i32::MAX;
        // The built-in builder deliberately covers printable ASCII. AtlasSource remains the path
        // for callers that need arbitrary Unicode scalar values.
        for i in 32..127 {
            let ch = i as u8 as char;
            let (metrics, bitmap) = font.rasterize(ch, size as f32);
            let rect = self.add_tile(
                metrics.width as _,
                metrics.height as _,
                bitmap.iter().map(|c| color4b(0xFF, 0xFF, 0xFF, *c)).collect::<Vec<Color4b>>().as_slice(),
            )?;
            let ce = CharEntry {
                offset: Vec2i::new(metrics.xmin, metrics.ymin),
                advance: Vec2i::new(metrics.advance_width as _, metrics.advance_height as _),
                rect,
            };
            entries.insert(i as u8 as char, ce);
            min_y = min_y.min(size as i32 - metrics.ymin - metrics.height as i32);
            max_y = max_y.max(size as i32 - metrics.ymin - metrics.height as i32);
        }

        let id = self.atlas.fonts.len();
        let line_metrics = font.horizontal_line_metrics(size as f32);
        let line_size = line_metrics
            .as_ref()
            .map(|m| m.new_line_size.round() as usize)
            .unwrap_or((max_y - min_y) as usize);
        let baseline = line_metrics.as_ref().map(|m| m.ascent.round() as i32).unwrap_or(line_size as i32);
        let font = super::Font {
            line_size,
            baseline,
            font_size: size,
            entries,
        };
        self.atlas.fonts.push((name.to_string(), font.clone()));
        Ok(FontId(id))
    }

    /// Serializes the atlas texture into PNG bytes.
    pub fn png_image_bytes(atlas: AtlasHandle) -> Result<Vec<u8>> {
        let mut w: Vec<u8> = Vec::new();
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut encoder = png::Encoder::new(&mut cursor, atlas.width() as _, atlas.height() as _);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);

            let mut writer = encoder.write_header()?;

            writer.write_image_data(atlas.0.pixels.iter().flat_map(|c| [c.x, c.y, c.z, c.w]).collect::<Vec<u8>>().as_slice())?;
        }
        cursor.seek(std::io::SeekFrom::Start(0))?;
        cursor.read_to_end(&mut w)?;
        Ok(w)
    }

    /// Writes the atlas texture to disk as a PNG.
    pub fn save_png_image(atlas: AtlasHandle, path: &str) -> Result<()> {
        let file = File::create(path)?;
        let mut w = BufWriter::new(file);
        let bytes = Self::png_image_bytes(atlas)?;
        w.write_all(bytes.as_slice())?;
        Ok(())
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    /// Loads an icon image from disk and normalizes it to RGBA pixels.
    fn load_icon(path: &str) -> Result<(usize, usize, Vec<Color4b>)> {
        let mut f = File::open(path)?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)?;
        load_image_bytes(ImageSource::Png { bytes: bytes.as_slice() })
    }

    /// Packs a populated bitmap into the atlas and copies its pixels into the texture buffer.
    fn add_tile(&mut self, width: usize, height: usize, pixels: &[Color4b]) -> Result<Recti> {
        let rect = self.packer.pack(width as _, height as _, false);
        match rect {
            Some(r) => {
                // Atlas pixels are stored row-major; `r` converts the tile-local coordinate into
                // the destination texture offset.
                for y in 0..height {
                    for x in 0..width {
                        self.atlas.pixels[(r.x + x as i32 + (r.y + y as i32) * self.atlas.width as i32) as usize] = pixels[x + y * width];
                    }
                }
                Ok(Recti::new(r.x, r.y, r.width, r.height))
            }
            None if width != 0 && height != 0 => {
                let error = format!(
                    "Bitmap size of {}x{} is not enough to hold the atlas, please resize",
                    self.atlas.width, self.atlas.height
                );
                Err(Error::other(error))
            }
            _ => Ok(Recti::new(0, 0, 0, 0)),
        }
    }

    /// Loads and parses a font file using `fontdue`.
    fn load_font(path: &str) -> Result<fontdue::Font> {
        let mut data = Vec::new();
        File::open(path)
            .map_err(|e| Error::other(format!("Cannot open font file '{}': {}", path, e)))?
            .read_to_end(&mut data)
            .map_err(|e| Error::other(format!("Cannot read font file '{}': {}", path, e)))?;

        let font = fontdue::Font::from_bytes(data, FontSettings::default()).map_err(|error| Error::other(error.to_string()))?;
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

    /// Consumes the builder and returns an [`AtlasHandle`].
    pub fn to_atlas(self) -> AtlasHandle {
        AtlasHandle(Rc::new(self.atlas))
    }
}
