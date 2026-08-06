//! Rust source export for serialized atlases.

use super::*;
#[cfg(feature = "png_source")]
use png::{BitDepth, ColorType};
use std::{fs::File, io::Result, io::Write, str::FromStr};

impl AtlasHandle {
    /// Serializes the atlas into Rust source files for reuse at build time.
    pub fn to_rust_files(&self, atlas_name: &str, format: SourceFormat, path: &str) -> Result<()> {
        let mut font_meta = String::new();
        font_meta.push_str(
            format!(
                "use microui_redux::prelude::*; use microui_redux::AtlasSource; pub const {} : AtlasSource = AtlasSource {{\n",
                atlas_name
            )
            .as_str(),
        );
        font_meta.push_str(format!("width: {}, height: {},\n", self.width(), self.height()).as_str());
        let mut icons = String::from_str("&[\n").unwrap();
        for (i, r) in &self.0.icons {
            icons.push_str(
                format!(
                    "(\"{}\", Rect {{ x: {}, y: {}, width: {}, height: {} }}),",
                    i, r.rect.x, r.rect.y, r.rect.width, r.rect.height,
                )
                .as_str(),
            );
        }
        icons.push_str("]");
        let mut fonts = String::from_str("&[\n").unwrap();
        for (n, f) in &self.0.fonts {
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
        let (source_pixels, source_format) = match format {
            SourceFormat::Raw => (
                self.0.pixels.iter().map(|p| [p.x, p.y, p.z, p.w]).flatten().collect::<Vec<_>>(),
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

    #[cfg(feature = "png_source")]
    fn png_image_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let pixels = self.0.pixels.iter().map(|c| [c.x, c.y, c.z, c.w]).flatten().collect::<Vec<_>>();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width() as _, self.height() as _);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(pixels.as_slice())?;
        }
        Ok(bytes)
    }
}
