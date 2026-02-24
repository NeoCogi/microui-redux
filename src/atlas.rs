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

#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{BitDepth, ColorType, Decoder, Transformations};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
#[cfg(any(feature = "builder", feature = "save-to-rust"))]
use std::fs::*;
#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;
use std::io::Error;
use std::io::ErrorKind;
#[cfg(feature = "builder")]
use std::io::Read;
#[cfg(feature = "builder")]
use std::io::BufWriter;
#[cfg(any(feature = "builder", feature = "save-to-rust"))]
use std::io::Result;
#[cfg(any(feature = "builder", feature = "save-to-rust"))]
use std::io::Write;
#[cfg(feature = "builder")]
use std::path::*;

#[cfg(feature = "save-to-rust")]
use std::str::FromStr;

use super::*;
use crate::ImageSource;

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
struct Font {
    line_size: usize,                  // line size
    baseline: i32,                     // distance from top of line to baseline
    font_size: usize,                  // font size in pixels
    entries: HashMap<char, CharEntry>, // all printable chars [32-127]
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

#[derive(Default, Copy, Clone)]
/// Handle referencing a font stored in the atlas.
pub struct FontId(usize);

#[derive(Default, Copy, Clone)]
/// Handle referencing a bitmap icon stored in the atlas.
pub struct IconId(usize);

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
/// Handle referencing an arbitrary image slot stored in the atlas.
pub struct SlotId(usize);

impl Into<u32> for IconId {
    fn into(self) -> u32 {
        self.0 as _
    }
}

impl Into<u32> for SlotId {
    fn into(self) -> u32 {
        self.0 as _
    }
}

#[derive(Debug, Clone)]
struct Icon {
    rect: Recti,
}

struct Atlas {
    width: usize,
    height: usize,
    pixels: Vec<Color4b>,
    fonts: Vec<(String, Font)>,
    icons: Vec<(String, Icon)>,
    slots: Vec<Recti>,
    last_update_id: usize,
}

#[derive(Clone)]
/// Shared handle exposing read/write access to the atlas.
pub struct AtlasHandle(Rc<RefCell<Atlas>>);

/// Identifier of the solid white icon baked into the default atlas.
pub const WHITE_ICON: IconId = IconId(0);
/// Identifier of the close icon baked into the default atlas.
pub const CLOSE_ICON: IconId = IconId(1);
/// Identifier of the expand icon baked into the default atlas.
pub const EXPAND_ICON: IconId = IconId(2);
/// Identifier of the collapse icon baked into the default atlas.
pub const COLLAPSE_ICON: IconId = IconId(3);
/// Identifier of the checkbox icon baked into the default atlas.
pub const CHECK_ICON: IconId = IconId(4);
/// Identifier of the combo-box expand icon baked into the default atlas.
pub const EXPAND_DOWN_ICON: IconId = IconId(5);
/// Identifier of the open-folder icon baked into the default atlas.
pub const OPEN_FOLDER_16_ICON: IconId = IconId(6);
/// Identifier of the closed-folder icon baked into the default atlas.
pub const CLOSED_FOLDER_16_ICON: IconId = IconId(7);
/// Identifier of the file icon baked into the default atlas.
pub const FILE_16_ICON: IconId = IconId(8);

/// Decodes image data into 32-bit pixels according to `source`.
/// Grayscale and RGB PNG inputs are expanded to opaque RGBA (alpha = 255).
pub fn load_image_bytes(source: ImageSource) -> std::io::Result<(usize, usize, Vec<Color4b>)> {
    match source {
        ImageSource::Raw { width, height, pixels } => {
            if width <= 0 || height <= 0 {
                return Err(Error::new(ErrorKind::Other, "Image dimensions must be positive"));
            }
            let width_usize = width as usize;
            let height_usize = height as usize;
            let expected = width_usize * height_usize * 4;
            if pixels.len() != expected {
                return Err(Error::new(
                    ErrorKind::Other,
                    format!("Expected {} RGBA bytes, found {}", expected, pixels.len()),
                ));
            }
            let mut colors = Vec::with_capacity(width_usize * height_usize);
            for chunk in pixels.chunks_exact(4) {
                colors.push(color4b(chunk[0], chunk[1], chunk[2], chunk[3]));
            }
            Ok((width_usize, height_usize, colors))
        }
        #[cfg(any(feature = "builder", feature = "png_source"))]
        ImageSource::Png { bytes } => decode_png_to_colors(bytes),
    }
}

#[cfg(any(feature = "builder", feature = "png_source"))]
fn decode_png_to_colors(bytes: &[u8]) -> std::io::Result<(usize, usize, Vec<Color4b>)> {
    let mut cursor = Cursor::new(bytes);
    let mut decoder = Decoder::new(&mut cursor);
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::new(ErrorKind::Other, format!("PNG decode error: {}", e)))?;
    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::new(ErrorKind::Other, "PNG decoder did not report output size"))?;
    let mut img_data = vec![0; buf_size];
    let info = reader.next_frame(&mut img_data)?;

    if info.bit_depth != BitDepth::Eight {
        return Err(Error::new(ErrorKind::Other, format!("Unsupported PNG bit depth: {:?}", info.bit_depth)));
    }

    let pixel_size = match info.color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Indexed => 1,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
    };

    let mut pixels = vec![Color4b::default(); (info.width * info.height) as usize];
    let line_size = info.line_size;
    for y in 0..info.height {
        let line = &img_data[(y as usize * line_size)..((y as usize + 1) * line_size)];

        for x in 0..info.width {
            let xx = (x * pixel_size) as usize;
            let color = match info.color_type {
                ColorType::Grayscale => {
                    let v = line[xx];
                    color4b(v, v, v, 0xFF)
                }
                ColorType::GrayscaleAlpha => {
                    let c = line[xx];
                    let a = line[xx + 1];
                    color4b(c, c, c, a)
                }
                ColorType::Indexed => {
                    return Err(Error::new(ErrorKind::Other, "Indexed PNGs are not supported"));
                }
                ColorType::Rgb => color4b(line[xx], line[xx + 1], line[xx + 2], 0xFF),
                ColorType::Rgba => {
                    let r = line[xx];
                    let g = line[xx + 1];
                    let b = line[xx + 2];
                    let a = line[xx + 3];
                    color4b(r, g, b, a)
                }
            };
            pixels[(x + y * info.width) as usize] = color;
        }
    }

    Ok((info.width as _, info.height as _, pixels))
}

#[cfg(feature = "builder")]
/// Helpers for constructing atlas textures at build time.
pub mod builder {
    use std::io::Seek;

    use super::*;
    use fontdue::*;

    use rect_packer::*;

    /// Incrementally constructs an atlas by packing fonts, icons, and slots.
    pub struct Builder {
        packer: Packer,
        atlas: Atlas,
    }

    #[derive(Clone)]
    /// Configuration for constructing an atlas from disk assets.
    #[cfg(feature = "builder")]
    pub struct Config<'a> {
        /// Width of the atlas texture in pixels.
        pub texture_width: usize,
        /// Height of the atlas texture in pixels.
        pub texture_height: usize,
        /// Path to the solid white icon.
        pub white_icon: String,
        /// Path to the close icon.
        pub close_icon: String,
        /// Path to the expand icon.
        pub expand_icon: String,
        /// Path to the collapse icon.
        pub collapse_icon: String,
        /// Path to the checkbox icon.
        pub check_icon: String,
        /// Path to the combo box expand icon.
        pub expand_down_icon: String,
        /// Path to the open-folder icon.
        pub open_folder_16_icon: String,
        /// Path to the closed-folder icon.
        pub closed_folder_16_icon: String,
        /// Path to the file icon.
        pub file_16_icon: String,
        /// Path to the default font file.
        pub default_font: String,
        /// Size of the default font.
        pub default_font_size: usize,
        /// Dimensions of additional slots to reserve in the atlas.
        pub slots: &'a [Dimensioni],
    }

    impl Builder {
        /// Creates a builder using the provided configuration and assets.
        #[cfg(feature = "builder")]
        pub fn from_config<'a>(config: &'a Config) -> Result<Builder> {
            let rp_config = rect_packer::Config {
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
                slots: Vec::new(),
                last_update_id: 0,
            };

            let mut builder = Builder { atlas, packer: Packer::new(rp_config) };

            builder.add_icon(&config.white_icon)?;
            builder.add_icon(&config.close_icon)?;
            builder.add_icon(&config.expand_icon)?;
            builder.add_icon(&config.collapse_icon)?;
            builder.add_icon(&config.check_icon)?;
            builder.add_icon(&config.expand_down_icon)?;
            builder.add_icon(&config.open_folder_16_icon)?;
            builder.add_icon(&config.closed_folder_16_icon)?;
            builder.add_icon(&config.file_16_icon)?;
            builder.add_font(&config.default_font, config.default_font_size)?;

            for slot in config.slots {
                builder.add_slot(*slot)?;
            }

            Ok(builder)
        }

        /// Adds an icon from the given image path and returns its [`IconId`].
        pub fn add_icon(&mut self, path: &str) -> Result<IconId> {
            let (width, height, pixels) = Self::load_icon(path)?;
            let rect = self.add_tile(width, height, pixels.as_slice())?;
            let id = self.atlas.icons.len();
            let icon = Icon { rect };
            self.atlas.icons.push((Self::format_path(&path), icon.clone()));
            Ok(IconId(id))
        }

        /// Adds a font at the requested size and returns its [`FontId`].
        pub fn add_font(&mut self, path: &str, size: usize) -> Result<FontId> {
            let font = Self::load_font(path)?;
            let mut entries = HashMap::new();
            let mut min_y = i32::MAX;
            let mut max_y = -i32::MAX;
            for i in 32..127 {
                // Rasterize and get the layout metrics for the letter at font size.
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
            self.atlas.fonts.push((Self::format_path(path), font.clone()));
            Ok(FontId(id))
        }

        /// Serializes the atlas texture into PNG bytes.
        pub fn png_image_bytes(atlas: AtlasHandle) -> Result<Vec<u8>> {
            let mut w: Vec<u8> = Vec::new();
            let mut cursor = Cursor::new(Vec::new());
            {
                let mut encoder = png::Encoder::new(&mut cursor, atlas.width() as _, atlas.height() as _); // Width is 2 pixels and height is 1.
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);

                let mut writer = encoder.write_header()?;

                writer.write_image_data(
                    atlas
                        .0
                        .borrow()
                        .pixels
                        .iter()
                        .map(|c| [c.x, c.y, c.z, c.w])
                        .flatten()
                        .collect::<Vec<u8>>()
                        .as_slice(),
                )?;
            }
            cursor.seek(std::io::SeekFrom::Start(0))?;
            cursor.read_to_end(&mut w)?;
            Ok(w)
        }

        /// Writes the atlas texture to disk as a PNG.
        pub fn save_png_image(atlas: AtlasHandle, path: &str) -> Result<()> {
            // png writer
            let file = File::create(path)?;
            let ref mut w = BufWriter::new(file);
            let bytes = Self::png_image_bytes(atlas)?;
            w.write_all(bytes.as_slice())?;
            Ok(())
        }

        #[cfg(any(feature = "builder", feature = "png_source"))]
        fn load_icon(path: &str) -> Result<(usize, usize, Vec<Color4b>)> {
            let mut f = File::open(path)?;
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes)?;
            load_image_bytes(ImageSource::Png { bytes: bytes.as_slice() })
        }

        fn add_slot(&mut self, slot: Dimensioni) -> Result<Recti> {
            let rect = self.packer.pack(slot.width, slot.height, false);
            match rect {
                Some(r) => {
                    self.atlas.slots.push(r);
                    Ok(r)
                }
                None => {
                    let error = format!(
                        "Bitmap size of {}x{} is not enough to hold the atlas, please resize",
                        self.atlas.width, self.atlas.height
                    );
                    Err(Error::new(ErrorKind::Other, error))
                }
            }
        }

        fn add_tile(&mut self, width: usize, height: usize, pixels: &[Color4b]) -> Result<Recti> {
            let rect = self.packer.pack(width as _, height as _, false);
            match rect {
                Some(r) => {
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
                    Err(Error::new(ErrorKind::Other, error))
                }
                _ => Ok(Recti::new(0, 0, 0, 0)),
            }
        }

        fn load_font(path: &str) -> Result<fontdue::Font> {
            let mut data = Vec::new();
            File::open(path)
                .map_err(|e| Error::new(ErrorKind::Other, format!("Cannot open font file '{}': {}", path, e)))?
                .read_to_end(&mut data)
                .map_err(|e| Error::new(ErrorKind::Other, format!("Cannot read font file '{}': {}", path, e)))?;

            let font = fontdue::Font::from_bytes(data, FontSettings::default()).map_err(|error| Error::new(ErrorKind::Other, format!("{}", error)))?;
            Ok(font)
        }

        fn strip_path_to_file(path: &str) -> String {
            let p = Path::new(path);
            p.file_name().and_then(|n| n.to_str()).unwrap_or(path).to_string()
        }

        fn strip_extension(path: &str) -> String {
            let p = Path::new(path);
            p.with_extension("").to_str().unwrap_or(path).to_string()
        }

        fn format_path(path: &str) -> String {
            Self::strip_extension(&Self::strip_path_to_file(path))
        }

        /// Consumes the builder and returns an [`AtlasHandle`].
        pub fn to_atlas(self) -> AtlasHandle {
            AtlasHandle(Rc::new(RefCell::new(self.atlas)))
        }
    }
}

/// Describes a font baked into an [`AtlasSource`].
pub struct FontEntry<'a> {
    /// Distance between baselines in pixels.
    pub line_size: usize, // line size
    /// Offset from the top of the line to the baseline.
    pub baseline: i32,
    /// Requested pixel size.
    pub font_size: usize, // font size in pixels
    /// Glyph metadata table.
    pub entries: &'a [(char, CharEntry)], // all printable chars [32-127]
}

/// Encodes how atlas pixel data is stored.
pub enum SourceFormat {
    /// Raw RGBA byte array.
    Raw,
    #[cfg(feature = "png_source")]
    /// PNG-formatted byte array.
    Png,
}

/// Serializable representation of an atlas that can be shipped with the binary.
pub struct AtlasSource<'a> {
    /// Width of the atlas texture.
    pub width: usize,
    /// Height of the atlas texture.
    pub height: usize,
    /// Pixel data matching [`AtlasSource::format`].
    pub pixels: &'a [u8],
    /// Icon lookup table.
    pub icons: &'a [(&'a str, Recti)],
    /// Fonts baked into the atlas.
    pub fonts: &'a [(&'a str, FontEntry<'a>)],
    /// Encoding of [`AtlasSource::pixels`].
    pub format: SourceFormat,
    /// Slot rectangles reserved in the atlas.
    pub slots: &'a [Recti],
}

impl AtlasHandle {
    fn from_parts<'a>(source: &AtlasSource<'a>, pixels: Vec<Color4b>) -> Self {
        let icons: Vec<(String, Icon)> = source
            .icons
            .iter()
            .map(|(name, rect)| (name.to_string(), Icon { rect: rect.clone() }))
            .collect();
        let fonts: Vec<(String, Font)> = source
            .fonts
            .iter()
            .map(|(name, f)| {
                let font = Font {
                    line_size: f.line_size,
                    baseline: f.baseline,
                    font_size: f.font_size,
                    entries: f.entries.iter().map(|(ch, e)| (ch.clone(), e.clone())).collect(),
                };
                (name.to_string(), font)
            })
            .collect();
        let slots: Vec<Recti> = source.slots.iter().map(|p| *p).collect();

        Self(Rc::new(RefCell::new(Atlas {
            width: source.width,
            height: source.height,
            icons,
            fonts,
            slots,
            pixels,
            last_update_id: 0,
        })))
    }

    /// Reconstructs an atlas from a serialized [`AtlasSource`].
    ///
    /// This method panics when decoding fails. Use [`AtlasHandle::try_from`] to handle
    /// failures explicitly, or [`AtlasHandle::from_lossy`] to preserve the previous
    /// "blank atlas on error" fallback behavior.
    pub fn from<'a>(source: &AtlasSource<'a>) -> Self {
        Self::try_from(source).unwrap_or_else(|err| panic!("Atlas decode failed: {}", err))
    }

    /// Reconstructs an atlas from a serialized [`AtlasSource`], falling back to a blank atlas
    /// if decoding fails.
    pub fn from_lossy<'a>(source: &AtlasSource<'a>) -> Self {
        match Self::try_from(source) {
            Ok(atlas) => atlas,
            Err(err) => {
                debug_assert!(false, "Atlas decode failed: {}", err);
                let pixel_count = source.width.saturating_mul(source.height);
                Self::from_parts(source, vec![Color4b::default(); pixel_count])
            }
        }
    }

    /// Attempts to reconstruct an atlas from a serialized [`AtlasSource`].
    pub fn try_from<'a>(source: &AtlasSource<'a>) -> std::io::Result<Self> {
        let width = i32::try_from(source.width).map_err(|_| Error::new(ErrorKind::Other, "Atlas width exceeds i32::MAX"))?;
        let height = i32::try_from(source.height).map_err(|_| Error::new(ErrorKind::Other, "Atlas height exceeds i32::MAX"))?;
        let pixels = match source.format {
            SourceFormat::Raw => {
                let (raw_width, raw_height, pixels) = load_image_bytes(ImageSource::Raw { width, height, pixels: source.pixels })?;
                if raw_width != source.width || raw_height != source.height {
                    return Err(Error::new(ErrorKind::Other, "Atlas dimensions do not match raw data"));
                }
                pixels
            }
            #[cfg(feature = "png_source")]
            SourceFormat::Png => {
                let (png_width, png_height, pixels) = load_image_bytes(ImageSource::Png { bytes: source.pixels })?;
                if png_width != source.width || png_height != source.height {
                    return Err(Error::new(ErrorKind::Other, "Atlas dimensions do not match PNG data"));
                }
                pixels
            }
        };
        Ok(Self::from_parts(source, pixels))
    }

    #[cfg(feature = "save-to-rust")]
    /// Serializes the atlas into Rust source files for reuse at build time.
    pub fn to_rust_files(&self, atlas_name: &str, format: SourceFormat, path: &str) -> Result<()> {
        let mut font_meta = String::new();
        font_meta.push_str(format!("use microui_redux::*; pub const {} : AtlasSource = AtlasSource {{\n", atlas_name).as_str());
        font_meta.push_str(format!("width: {}, height: {},\n", self.width(), self.height()).as_str());
        let mut icons = String::from_str("&[\n").unwrap();
        for (i, r) in &self.0.borrow().icons {
            icons.push_str(
                format!(
                    "(\"{}\", Rect {{ x: {}, y: {}, width: {}, height: {} }}),",
                    i, r.rect.x, r.rect.y, r.rect.width, r.rect.height,
                )
                .as_str(),
            );
        }
        icons.push_str("]");
        let mut slots = String::from_str("&[\n").unwrap();
        for r in &self.0.borrow().slots {
            slots.push_str(format!("Rect {{ x: {}, y: {}, width: {}, height: {} }},", r.x, r.y, r.width, r.height,).as_str());
        }
        slots.push_str("]");
        let mut fonts = String::from_str("&[\n").unwrap();
        for (n, f) in &self.0.borrow().fonts {
            let mut char_entries = String::from_str("&[\n").unwrap();
            for (ch, entry) in &f.entries {
                let str = match ch {
                    '\'' => String::from_str("\\'").unwrap(),
                    '\\' => String::from_str("\\\\").unwrap(),
                    _ => format!("{}", ch),
                };
                char_entries.push_str(
                    format!(
                        "('{}', CharEntry {{ offset: Vec2i {{ x: {}, y:{} }}, advance: Vec2i {{ x:{}, y: {} }}, rect: Recti {{x: {}, y: {}, width: {}, height: {} }}, }}),\n",
                        str, entry.offset.x, entry.offset.y, entry.advance.x, entry.advance.y, entry.rect.x, entry.rect.y, entry.rect.width, entry.rect.height,
                    )
                        .as_str(),
                );
            }
            char_entries.push_str("]\n");
            fonts.push_str(
                format!(
                    "(\"{}\", FontEntry {{ line_size: {}, baseline: {}, font_size: {}, entries: {} }}),\n",
                    n, f.line_size, f.baseline, f.font_size, char_entries
                )
                .as_str(),
            );
        }
        fonts.push_str("]");
        font_meta.push_str(format!("icons: {},\n", icons).as_str());
        font_meta.push_str(format!("fonts: {},\n", fonts).as_str());
        font_meta.push_str(format!("slots: {},\n", slots).as_str());
        let (source_pixels, source_format) = match format {
            SourceFormat::Raw => (
                self.0.borrow().pixels.iter().map(|p| [p.x, p.y, p.z, p.w]).flatten().collect::<Vec<_>>(),
                "SourceFormat::Raw",
            ),
            #[cfg(feature = "png_source")]
            SourceFormat::Png => (self.png_image_bytes()?, "SourceFormat::Png"),
        };

        let mut pixels = String::from_str("&[\n").unwrap();
        for p in source_pixels {
            pixels.push_str(format!("0x{:02x},", p).as_str());
        }
        pixels.push_str("]\n");
        font_meta.push_str(format!("format: {},\n", source_format).as_str());
        font_meta.push_str(format!("pixels: {},\n", pixels).as_str());
        font_meta.push_str("};");
        let mut f = File::create(path)?;
        write!(f, "{}", font_meta)
    }

    #[cfg(all(feature = "save-to-rust", feature = "png_source"))]
    fn png_image_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let pixels = self.0.borrow().pixels.iter().map(|c| [c.x, c.y, c.z, c.w]).flatten().collect::<Vec<_>>();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width() as _, self.height() as _);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(pixels.as_slice())?;
        }
        Ok(bytes)
    }

    /// Returns the atlas texture width in pixels.
    pub fn width(&self) -> usize {
        self.0.borrow().width
    }
    /// Returns the atlas texture height in pixels.
    pub fn height(&self) -> usize {
        self.0.borrow().height
    }
    /// Returns a clone of the atlas pixel data.
    pub fn pixels_clone(&self) -> Vec<Color4b> {
        self.0.borrow().pixels.clone()
    }

    /// Executes a closure with shared access to the atlas pixels.
    pub fn apply_pixels<F: FnMut(usize, usize, &Vec<Color4b>)>(&self, mut f: F) {
        let s = self.0.borrow();
        f(s.width, s.height, &s.pixels);
    }

    /// Returns a mapping from icon names to their identifiers.
    pub fn clone_icon_table(&self) -> Vec<(String, IconId)> {
        self.0.borrow().icons.iter().enumerate().map(|(i, icon)| (icon.0.clone(), IconId(i))).collect()
    }

    /// Returns a mapping from font names to their identifiers.
    pub fn clone_font_table(&self) -> Vec<(String, FontId)> {
        self.0.borrow().fonts.iter().enumerate().map(|(i, font)| (font.0.clone(), FontId(i))).collect()
    }

    /// Returns a list of available slot identifiers.
    pub fn clone_slot_table(&self) -> Vec<SlotId> {
        self.0.borrow().slots.iter().enumerate().map(|(i, _)| SlotId(i)).collect()
    }

    /// Returns glyph metrics for the specified character, if available.
    pub fn get_char_entry(&self, font: FontId, c: char) -> Option<CharEntry> {
        self.0.borrow().fonts[font.0].1.entries.get(&c).map(|x| x.clone())
    }

    /// Returns the line height for the specified font.
    pub fn get_font_height(&self, font: FontId) -> usize {
        self.0.borrow().fonts[font.0].1.line_size
    }

    /// Returns the baseline offset (in pixels) for the specified font.
    pub fn get_font_baseline(&self, font: FontId) -> i32 {
        self.0.borrow().fonts[font.0].1.baseline
    }

    /// Returns the dimensions of an icon.
    pub fn get_icon_size(&self, icon: IconId) -> Dimensioni {
        let r = self.0.borrow().icons[icon.0].1.rect;
        Dimensioni::new(r.width, r.height)
    }

    /// Returns the atlas rectangle storing an icon.
    pub fn get_icon_rect(&self, icon: IconId) -> Recti {
        self.0.borrow().icons[icon.0].1.rect
    }

    /// Returns the dimensions of a slot.
    pub fn get_slot_size(&self, slot: SlotId) -> Dimensioni {
        let r = self.0.borrow().slots[slot.0];
        Dimension::new(r.width, r.height)
    }

    /// Returns the atlas rectangle storing a slot.
    pub(crate) fn get_slot_rect(&self, slot: SlotId) -> Recti {
        self.0.borrow().slots[slot.0]
    }

    /// Returns the atlas texture dimensions.
    pub fn get_texture_dimension(&self) -> Dimensioni {
        Dimension::new(self.0.borrow().width as _, self.0.borrow().height as _)
    }

    /// Internal helper that walks glyphs applying baseline-aware placement.
    fn walk_glyphs<F>(&self, font: FontId, text: &str, mut f: F)
    where
        F: FnMut(char, Vec2i, Recti, Recti, i32),
    {
        let mut dst = Recti { x: 0, y: 0, width: 0, height: 0 };
        let line_height = self.get_font_height(font) as i32;
        let baseline = self.get_font_baseline(font);
        let mut baseline_y = baseline;
        let mut pen_x = 0;

        for chr in text.chars() {
            if chr == '\n' || chr == '\r' {
                pen_x = 0;
                baseline_y += line_height;
                continue;
            }

            let src = self.get_char_entry(font, chr).or_else(|| self.get_char_entry(font, '_')).unwrap_or(CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 8, 8),
            });

            dst.width = src.rect.width;
            dst.height = src.rect.height;
            dst.x = pen_x + src.offset.x;
            dst.y = baseline_y - src.offset.y - src.rect.height;

            f(chr, src.advance, dst, src.rect, baseline_y);
            pen_x += src.advance.x;
        }
    }

    /// Walks each glyph in the string and invokes the closure with draw information.
    pub fn draw_string<DrawFunction: FnMut(char, Vec2i, Recti, Recti)>(&self, font: FontId, text: &str, mut f: DrawFunction) {
        self.walk_glyphs(font, text, |chr, advance, dst, src, _| f(chr, advance, dst, src));
    }

    /// Measures the bounding box of the provided text.
    pub fn get_text_size(&self, font: FontId, text: &str) -> Dimensioni {
        let mut res = Dimensioni::new(0, 0);
        let line_height = self.get_font_height(font) as i32;
        let baseline = self.get_font_baseline(font);
        let descent = (line_height - baseline).max(0);
        let mut max_line_bottom = 0;
        let mut saw_glyph = false;

        self.walk_glyphs(font, text, |_, advance, dst, _, baseline_y| {
            saw_glyph = true;
            res.width = max(res.width, dst.x + max(advance.x, dst.width));
            res.height = max(res.height, dst.y + dst.height);
            max_line_bottom = max(max_line_bottom, baseline_y + descent);
        });

        if saw_glyph {
            res.height = max(res.height, max_line_bottom);
        }
        res
    }

    /// Renders into a slot using the provided callback and bumps the update counter.
    pub fn render_slot(&mut self, slot: SlotId, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let slot_rect = match self.0.borrow().slots.get(slot.0) {
            Some(rect) => *rect,
            None => return,
        };
        let width = self.width();
        let height = self.height();
        {
            let pixels = &mut self.0.borrow_mut().pixels;
            let max_y = (slot_rect.y + slot_rect.height).min(height as i32);
            let max_x = (slot_rect.x + slot_rect.width).min(width as i32);
            for y in slot_rect.y.max(0)..max_y {
                for x in slot_rect.x.max(0)..max_x {
                    let index = (x + y * (width as i32)) as usize;
                    if index < pixels.len() {
                        pixels[index] = f(x as _, y as _)
                    }
                }
            }
        }
        let last_update = self.0.borrow().last_update_id;
        self.0.borrow_mut().last_update_id = last_update.wrapping_add(1);
    }

    /// Returns a monotonically increasing value that changes whenever slot pixels are modified.
    pub fn get_last_update_id(&self) -> usize {
        self.0.borrow().last_update_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "builder", feature = "png_source"))]
    use png::Encoder;
    #[cfg(any(feature = "builder", feature = "png_source"))]
    use std::fmt::Write;

    #[cfg(any(feature = "builder", feature = "png_source"))]
    fn encode_png(color_type: ColorType, data: &[u8], width: u32, height: u32, palette: Option<&[u8]>) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut encoder = Encoder::new(&mut buffer, width, height);
            encoder.set_color(color_type);
            encoder.set_depth(BitDepth::Eight);
            if let Some(palette) = palette {
                encoder.set_palette(palette);
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(data).unwrap();
        }
        buffer
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    #[test]
    fn png_decode_error_returns_err() {
        let res = load_image_bytes(ImageSource::Png { bytes: &[] });
        assert!(res.is_err());
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    #[test]
    fn png_decode_rgb_expands_alpha() {
        let bytes = encode_png(ColorType::Rgb, &[10, 20, 30], 1, 1, None);
        let (width, height, pixels) = load_image_bytes(ImageSource::Png { bytes: &bytes }).unwrap();

        assert_eq!(width, 1);
        assert_eq!(height, 1);
        assert_eq!(pixels.len(), 1);
        let pixel = pixels[0];
        assert_eq!((pixel.x, pixel.y, pixel.z, pixel.w), (10, 20, 30, 0xFF));
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    #[test]
    fn png_decode_indexed_uses_palette() {
        let palette = [0x01, 0x02, 0x03];
        let bytes = encode_png(ColorType::Indexed, &[0], 1, 1, Some(&palette));
        let (width, height, pixels) = load_image_bytes(ImageSource::Png { bytes: &bytes }).unwrap();

        assert_eq!(width, 1);
        assert_eq!(height, 1);
        assert_eq!(pixels.len(), 1);

        let pixel = pixels[0];
        let mut message = String::new();
        let _ = write!(&mut message, "{},{},{},{}", pixel.x, pixel.y, pixel.z, pixel.w);
        assert_eq!(message, "1,2,3,255");
    }
}
