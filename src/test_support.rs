//! Shared fixtures and no-op helpers used by unit tests.

use crate::{AtlasHandle, AtlasSource, CharEntry, Color, FontEntry, Recti, Renderer, SourceFormat, TextureId, Vec2i, Vertex};

const ICON_NAMES: [&str; 9] = [
    "white",
    "close",
    "expand",
    "collapse",
    "check",
    "expand_down",
    "open_folder",
    "closed_folder",
    "file",
];

pub(crate) fn test_atlas() -> AtlasHandle {
    test_atlas_with_font_sizes(&[("default", 10)])
}

pub(crate) fn test_atlas_with_font_sizes(fonts: &[(&str, usize)]) -> AtlasHandle {
    let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
    let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
    let entries = vec![
        (
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
        (
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
        (
            'b',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        ),
    ];
    let fonts: Vec<(&str, FontEntry<'_>)> = fonts
        .iter()
        .map(|(name, size)| {
            (
                *name,
                FontEntry {
                    line_size: *size,
                    baseline: (*size as i32 * 4) / 5,
                    font_size: *size,
                    entries: &entries,
                },
            )
        })
        .collect();
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
        slots: &[],
    };
    AtlasHandle::from(&source)
}

pub(crate) struct NoopRenderer {
    pub(crate) atlas: AtlasHandle,
}

impl Renderer for NoopRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}

    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {}

    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {}

    fn flush(&mut self) {}

    fn end(&mut self) {}

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}

    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}
