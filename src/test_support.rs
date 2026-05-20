use crate::{AtlasHandle, AtlasSource, CharEntry, FontEntry, Recti, SourceFormat, Vec2i};

const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

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
