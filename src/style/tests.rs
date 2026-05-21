//! Tests for style font binding and role resolution.

use super::*;
use crate::test_support::test_atlas_with_font_sizes as make_test_atlas;

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
fn frame_border_policy_keeps_flat_roles_borderless() {
    let style = Style::default();

    assert!(style.frame_border_color(crate::ControlColor::Button).is_some());
    assert!(style.frame_border_color(crate::ControlColor::ScrollBase).is_none());
    assert!(style.frame_border_color(crate::ControlColor::ScrollThumb).is_none());
    assert!(style.frame_border_color(crate::ControlColor::TitleBG).is_none());
}
