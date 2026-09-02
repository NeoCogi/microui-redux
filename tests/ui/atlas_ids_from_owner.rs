//! Font and icon IDs are obtained from validated runtime atlas resources.

use microui_redux::{AtlasHandle, FontId, IconId};

#[allow(dead_code)]
fn resolve_owned_ids(atlas: &AtlasHandle) -> Option<(FontId, IconId)> {
    // Both opaque IDs carry the logical resource identity supplied by public lookup.
    Some((atlas.font_id("body")?, atlas.icon_id("white")?))
}

fn main() {}
