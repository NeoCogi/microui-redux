//! Font and icon IDs are obtained from one concrete runtime atlas owner.

use microui_redux::{AtlasHandle, FontId, IconId};

#[allow(dead_code)]
fn resolve_owned_ids(atlas: &AtlasHandle) -> Option<(FontId, IconId)> {
    // Both IDs carry the atlas provenance supplied by their public lookup boundary.
    Some((atlas.font_id("body")?, atlas.icon_id("white")?))
}

fn main() {}
