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

use fontdue::*;
use png::BitDepth;
use png::ColorType;
use rect_packer::*;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::path::*;
use std::io::{Result, BufWriter};
use std::fs::*;

use super::*;

#[derive(Debug, Clone)]
pub struct CharEntry {
    pub offset: Vec2i,
    pub advance: Vec2i,
    pub rect: Recti, // coordinates in the atlas
}

#[derive(Clone)]
struct Font {
    line_size: usize,                  // line size
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
            "Font {{ line_size: {}, font_size: {}, entries: [{}] }}",
            self.line_size, self.font_size, entries
        ))
    }
}

#[derive(Default, Copy, Clone)]
pub struct FontId(usize);

#[derive(Default, Copy, Clone)]
pub struct IconId(usize);

impl Into<u32> for IconId {
    fn into(self) -> u32 {
        self.0 as _
    }
}

#[derive(Debug, Clone)]
struct Icon {
    rect: Recti,
}

#[derive(Clone)]
pub struct Config {
    pub texture_width: usize,
    pub texture_height: usize,
    pub white_icon: String,
    pub close_icon: String,
    pub expand_icon: String,
    pub collapse_icon: String,
    pub check_icon: String,
    pub default_font: String,
    pub default_font_size: usize,
}

pub struct Atlas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    fonts: Vec<(String, Font)>,
    icons: Vec<(String, Icon)>,

    packer: Packer,
}

pub const WHITE_ICON: IconId = IconId(0);
pub const CLOSE_ICON: IconId = IconId(1);
pub const EXPAND_ICON: IconId = IconId(2);
pub const COLLAPSE_ICON: IconId = IconId(3);
pub const CHECK_ICON: IconId = IconId(4);

impl Atlas {
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn pixels(&self) -> &Vec<u8> {
        &self.pixels
    }
    pub fn from_config(config: &Config) -> Result<Self> {
        let rp_config = rect_packer::Config {
            width: config.texture_width as _,
            height: config.texture_height as _,

            border_padding: 1,
            rectangle_padding: 1,
        };

        let mut atlas = Self {
            width: config.texture_width,
            height: config.texture_height,
            pixels: vec![0; config.texture_height * config.texture_width],
            fonts: Vec::new(),
            icons: Vec::new(),

            packer: Packer::new(rp_config),
        };

        atlas.add_icon(&config.white_icon)?;
        atlas.add_icon(&config.close_icon)?;
        atlas.add_icon(&config.expand_icon)?;
        atlas.add_icon(&config.collapse_icon)?;
        atlas.add_icon(&config.check_icon)?;
        atlas.add_font(&config.default_font, config.default_font_size)?;

        Ok(atlas)
    }

    pub fn add_icon(&mut self, path: &str) -> Result<IconId> {
        let (width, height, pixels) = Self::load_icon(path)?;
        let rect = self.add_tile(width, height, pixels.as_slice())?;
        let id = self.icons.len();
        let icon = Icon { rect };
        self.icons.push((Self::format_path(&path), icon.clone()));
        Ok(IconId(id))
    }

    pub fn add_font(&mut self, path: &str, size: usize) -> Result<FontId> {
        let font = Self::load_font(path)?;
        let mut entries = HashMap::new();
        let mut min_y = i32::MAX;
        let mut max_y = -i32::MAX;
        for i in 32..127 {
            // Rasterize and get the layout metrics for the letter at font size.
            let ch = i as u8 as char;
            let (metrics, bitmap) = font.rasterize(ch, size as f32);
            let rect = self.add_tile(metrics.width as _, metrics.height as _, bitmap.as_slice())?;
            let ce = CharEntry {
                offset: Vec2i::new(metrics.xmin, metrics.ymin),
                advance: Vec2i::new(metrics.advance_width as _, metrics.advance_height as _),
                rect,
            };
            entries.insert(i as u8 as char, ce);
            min_y = min_y.min(size as i32 - metrics.ymin - metrics.height as i32);
            max_y = max_y.max(size as i32 - metrics.ymin - metrics.height as i32);
        }

        let id = self.fonts.len();
        let font = Font {
            line_size: (max_y - min_y) as usize,
            font_size: size,
            entries,
        };
        self.fonts.push((Self::format_path(path), font.clone()));
        Ok(FontId(id))
    }

    pub fn save_png_image(&self, path: &str) -> Result<()> {
        // png writer
        let file = File::create(path)?;
        let ref mut w = BufWriter::new(file);

        let mut encoder = png::Encoder::new(w, self.width as _, self.height as _); // Width is 2 pixels and height is 1.
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header()?;

        writer.write_image_data(self.pixels.as_slice())?;
        Ok(())
    }

    pub fn get_char_entry(&self, font: FontId, c: char) -> CharEntry {
        self.fonts[font.0].1.entries[&c].clone()
    }

    pub fn get_font_height(&self, font: FontId) -> usize {
        self.fonts[font.0].1.line_size
    }

    pub fn get_icon_size(&self, icon: IconId) -> Dimensioni {
        let r = self.icons[icon.0].1.rect;
        Dimensioni::new(r.width, r.height)
    }

    pub fn get_icon_rect(&self, icon: IconId) -> Recti {
        self.icons[icon.0].1.rect
    }

    pub fn get_texture_dimension(&self) -> Dimensioni {
        Dimension::new(self.width as _, self.height as _)
    }

    pub fn draw_string<DrawFunction: FnMut(char, Vec2i, Recti, Recti)>(&self, font: FontId, text: &str, mut f: DrawFunction) {
        let mut dst = Recti { x: 0, y: 0, width: 0, height: 0 };
        let fh = self.get_font_height(font) as i32;
        let mut acc_x = 0;
        let mut acc_y = 0;
        for chr in text.chars() {
            let src = self.get_char_entry(font, chr);

            // string could be empty
            if acc_y == 0 {
                acc_y = fh
            }

            if chr == '\n' {
                acc_x = 0;
                acc_y += fh;
            }

            dst.width = src.rect.width;
            dst.height = src.rect.height;
            dst.x = acc_x + src.offset.x;
            dst.y = acc_y - src.offset.y - src.rect.height;
            f(chr, src.advance, dst, src.rect);
            acc_x += src.advance.x;
        }
    }

    pub fn get_text_size(&self, font: FontId, text: &str) -> Dimensioni {
        let mut res = Dimensioni::new(0, 0);
        self.draw_string(font, text, |_, advance, dst, _| {
            res.width = max(res.width, dst.x + max(advance.x, dst.width));
            res.height = max(res.height, dst.y + dst.height);
        });
        res
    }
}

////////////////////////////////////////////////////////////////////////////////
/// Private methods
////////////////////////////////////////////////////////////////////////////////
impl Atlas {
    fn add_tile(&mut self, width: usize, height: usize, pixels: &[u8]) -> Result<Recti> {
        let rect = self.packer.pack(width as _, height as _, false);
        match rect {
            Some(r) => {
                for y in 0..height {
                    for x in 0..width {
                        self.pixels[(r.x + x as i32 + (r.y + y as i32) * self.width as i32) as usize] = pixels[x + y * width];
                    }
                }
                Ok(Recti::new(r.x, r.y, r.width, r.height))
            }
            None if width != 0 && height != 0 => {
                let error = format!("Bitmap size of {}x{} is not enough to hold the atlas, please resize", self.width, self.height);
                Err(Error::new(ErrorKind::Other, error))
            }
            _ => Ok(Recti::new(0, 0, 0, 0)),
        }
    }

    fn load_icon(path: &str) -> Result<(usize, usize, Vec<u8>)> {
        let mut decoder = png::Decoder::new(File::open(path)?);
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().unwrap();
        let mut img_data = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut img_data)?;

        assert_eq!(info.bit_depth, BitDepth::Eight);

        let pixel_size = match info.color_type {
            ColorType::Grayscale => 1,
            ColorType::GrayscaleAlpha => 2,
            ColorType::Indexed => 1,
            ColorType::Rgb => 3,
            ColorType::Rgba => 4,
        };

        let mut pixels = vec![0u8; (info.width * info.height) as usize];
        let line_size = info.line_size;
        for y in 0..info.height {
            let line = &img_data[(y as usize * line_size)..((y as usize + 1) * line_size)];

            for x in 0..info.width {
                let xx = (x * pixel_size) as usize;
                let color = match info.color_type {
                    ColorType::Grayscale => line[xx],
                    ColorType::GrayscaleAlpha => line[xx + 1],
                    ColorType::Indexed => todo!(),
                    ColorType::Rgb => ((line[xx] as u32 + line[xx + 1] as u32 + line[xx + 2] as u32) / 3) as u8,
                    ColorType::Rgba => line[xx + 3],
                };
                pixels[(x + y * info.width) as usize] = color;
            }
        }

        Ok((info.width as _, info.height as _, pixels))
    }

    fn load_font(path: &str) -> Result<fontdue::Font> {
        let mut data = Vec::new();
        File::open(path).unwrap().read_to_end(&mut data).unwrap();

        let font = fontdue::Font::from_bytes(data, FontSettings::default()).map_err(|error| Error::new(ErrorKind::Other, format!("{}", error)))?;
        Ok(font)
    }

    fn strip_path_to_file(path: &str) -> String {
        let p = Path::new(path);
        p.file_name().unwrap().to_str().unwrap().to_string()
    }

    fn strip_extension(path: &str) -> String {
        let p = Path::new(path);
        p.with_extension("").to_str().unwrap().to_string()
    }

    fn format_path(path: &str) -> String {
        Self::strip_extension(&Self::strip_path_to_file(path))
    }
}
