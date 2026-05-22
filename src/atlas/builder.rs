//! Build-time atlas construction helpers.

use super::*;
use crate::{rect_packer::*, ImageSource};
use fontdue::*;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufWriter, Cursor, Error, ErrorKind, Read, Result, Seek, Write},
    path::Path,
};

/// Incrementally constructs an atlas by packing fonts, icons, and slots.
pub struct Builder {
    packer: Packer,
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
    /// Legacy fallback font path used when [`Config::fonts`] is empty.
    pub default_font: String,
    /// Legacy fallback font size used when [`Config::fonts`] is empty.
    pub default_font_size: usize,
    /// Fonts baked into the atlas.
    ///
    /// Use the conventional keys `body`, `small`, `title`, `heading`, and `mono`
    /// to populate the built-in semantic roles through [`Style::bind_named_fonts`].
    /// When this slice is empty, [`Config::default_font`] and
    /// [`Config::default_font_size`] are used instead for single-font atlases.
    pub fonts: &'a [FontAsset<'a>],
    /// Dimensions of additional slots to reserve in the atlas.
    pub slots: &'a [Dimensioni],
}

impl Builder {
    /// Creates a builder using the provided configuration and assets.
    #[cfg(feature = "builder")]
    pub fn from_config<'a>(config: &'a Config) -> Result<Builder> {
        let rp_config = crate::rect_packer::Config {
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
        if config.fonts.is_empty() {
            if config.default_font.is_empty() {
                return Err(Error::new(ErrorKind::Other, "Atlas config must provide either `fonts` or `default_font`"));
            }
            builder.add_font(config.default_font.as_str(), config.default_font_size)?;
        } else {
            for font in config.fonts {
                builder.add_font_named(font.name, font.path, font.size)?;
            }
        }

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
        self.atlas.icons.push((Self::format_path(path), icon.clone()));
        Ok(IconId(id))
    }

    /// Adds a font at the requested size and returns its [`FontId`].
    pub fn add_font(&mut self, path: &str, size: usize) -> Result<FontId> {
        let name = format!("{}-{}", Self::format_path(path), size);
        self.add_font_named(name.as_str(), path, size)
    }

    /// Adds a font with an explicit atlas key and returns its [`FontId`].
    pub fn add_font_named(&mut self, name: &str, path: &str, size: usize) -> Result<FontId> {
        if self.atlas.fonts.iter().any(|(existing, _)| existing == name) {
            return Err(Error::new(ErrorKind::Other, format!("Font name '{}' already exists in the atlas", name)));
        }
        let font = Self::load_font(path)?;
        let mut entries = HashMap::new();
        let mut min_y = i32::MAX;
        let mut max_y = -i32::MAX;
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
