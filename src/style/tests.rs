//! Tests for style font binding and role resolution.

use super::*;
use crate::test_support::test_atlas_with_font_sizes as make_test_atlas;
use std::collections::HashSet;

#[test]
fn texture_id_identity_includes_immutable_dimensions() {
    let texture = TextureId::new(7, 32, 16);
    let same = TextureId::new(7, 32, 16);
    let different_width = TextureId::new(7, 64, 16);
    let different_height = TextureId::new(7, 32, 8);

    assert_eq!(texture, same);
    assert_ne!(texture, different_width);
    assert_ne!(texture, different_height);

    let textures = HashSet::from([texture]);
    assert!(textures.contains(&same));
    assert!(!textures.contains(&different_width));
    assert!(!textures.contains(&different_height));
}

#[test]
fn font_choice_conversions_preserve_selected_font() {
    let atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12), (FontRole::Heading.atlas_name(), 18)]);
    let heading = atlas.font_id(FontRole::Heading.atlas_name()).unwrap();

    assert_eq!(FontChoice::from(FontRole::Heading), FontChoice::role(FontRole::Heading));
    assert_eq!(FontChoice::from(heading), FontChoice::id(heading));
}

#[test]
fn bind_named_fonts_uses_conventional_role_names() {
    let atlas = make_test_atlas(&[
        (FontRole::Body.atlas_name(), 12),
        (FontRole::Small.atlas_name(), 10),
        (FontRole::Title.atlas_name(), 16),
        (FontRole::Heading.atlas_name(), 18),
    ]);

    let style = Style::default().with_named_fonts(&atlas);

    assert_eq!(style.font, atlas.font_id(FontRole::Body.atlas_name()).unwrap());
    assert_eq!(style.small_font, atlas.font_id(FontRole::Small.atlas_name()).unwrap());
    assert_eq!(style.title_font, atlas.font_id(FontRole::Title.atlas_name()).unwrap());
    assert_eq!(style.heading_font, atlas.font_id(FontRole::Heading.atlas_name()).unwrap());
    assert_eq!(style.mono_font, style.font);
}

#[test]
fn bind_default_named_fonts_replaces_unset_font_fields_only() {
    let atlas = make_test_atlas(&[
        (FontRole::Small.atlas_name(), 10),
        (FontRole::Body.atlas_name(), 12),
        (FontRole::Title.atlas_name(), 16),
        (FontRole::Heading.atlas_name(), 18),
    ]);

    let mut style = Style::default();
    style.bind_default_named_fonts(&atlas);
    assert_eq!(style.font, atlas.font_id(FontRole::Body.atlas_name()).unwrap());
    assert_eq!(style.small_font, atlas.font_id(FontRole::Small.atlas_name()).unwrap());
    assert_eq!(style.title_font, atlas.font_id(FontRole::Title.atlas_name()).unwrap());
    assert_eq!(style.heading_font, atlas.font_id(FontRole::Heading.atlas_name()).unwrap());

    let explicit_title = atlas.font_id(FontRole::Title.atlas_name()).unwrap();
    style.font = explicit_title;
    style.bind_default_named_fonts(&atlas);
    assert_eq!(style.font, explicit_title);
}

#[test]
fn frame_border_resolves_geometry_and_color_without_role_policy() {
    let mut style = Style::default();
    style.frame_border_width = -4;
    let border = style.frame_border();
    let expected = style.colors[crate::ControlColor::Border as usize];

    assert_eq!(border.width, 0);
    assert_eq!(
        (border.color.r, border.color.g, border.color.b, border.color.a),
        (expected.r, expected.g, expected.b, expected.a)
    );
}
