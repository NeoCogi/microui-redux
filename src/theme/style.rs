//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
//! Atlas-bound UI style values used across the crate.

use super::{AppearanceCatalog, AppearanceRole, Color, ControlColor, FontChoice, FontRole, ForegroundCatalog, ThemeIcons, VisualState};
use crate::atlas::{AtlasHandle, FontId};
use crate::render::{NinePatch, SliceInsets};

#[derive(Clone)]
/// Collection of visual constants that drive widget appearance.
pub struct Style {
    /// Default body font used for general text rendering.
    pub font: FontId,
    /// Font used for compact supporting text.
    pub small_font: FontId,
    /// Font used for window titles and similar chrome text.
    pub title_font: FontId,
    /// Font used for larger display text.
    pub heading_font: FontId,
    /// Font used for monospace-style text.
    pub mono_font: FontId,
    /// Semantic icons used by built-in widgets and chrome.
    pub icons: ThemeIcons,
    /// Default width used by layouts when no preferred width is supplied.
    pub default_cell_width: i32,
    /// Inner padding applied to most widgets.
    pub padding: i32,
    /// Spacing between cells in a layout.
    pub spacing: i32,
    /// Indentation applied to nested content.
    pub indent: i32,
    /// Height of window title bars.
    pub title_height: i32,
    /// Structural thickness reserved for each top-level window border edge.
    ///
    /// This is deliberately independent from the fixed corner span in the window-frame
    /// nine-patch. A classic theme can therefore paint long L-shaped corner pieces while keeping
    /// the client inset and one-axis resize hit regions at their actual narrow edge thickness.
    pub window_border: SliceInsets,
    /// Width of scrollbars.
    pub scrollbar_size: i32,
    /// Minimum length of scrollbar thumbs and width of slider thumbs.
    pub thumb_size: i32,
    /// Typed state tables used by built-in widgets, containers, menus, and window chrome.
    pub appearances: AppearanceCatalog,
    /// Typed foreground tables resolved with the same semantic roles and states as appearances.
    ///
    /// A foreground colors text and semantic glyphs; it never changes background geometry or
    /// substitutes for an appearance PNG. Theme files can override either half independently.
    pub foregrounds: ForegroundCatalog,
    /// Accent used for focused widget fills, menu selection, and the universal focus outline.
    ///
    /// Focus is an interaction scope rather than a control-family color, so this named value
    /// replaces the former button/base focus entries in [`Self::colors`].
    pub focus_color: Color,
    /// Accent used for the active window title and framed outer outline.
    ///
    /// Keeping window activation separate lets themes distinguish application chrome from the
    /// focused control within that window even when both defaults use the same accent.
    pub window_focus_color: Color,
    /// Background color used by menu bars, popup menus, and cascading submenus.
    pub menu_background: Color,
    /// Shared flat fallback fill used by disabled backgrounds and controls.
    ///
    /// Image-backed themes normally replace individual Disabled patches, while this value keeps
    /// omitted roles and entirely flat themes visually coherent without requiring a PNG.
    pub disabled_background_color: Color,
    /// Palette of [`crate::ControlColor`] entries.
    pub colors: [Color; 12],
}

impl Style {
    /// Constructs the default visual metrics and resolves every retained asset from `atlas`.
    ///
    /// The required `body` font and semantic theme icon names form the standard Context atlas
    /// contract. Optional font roles fall back to that same atlas's body font.
    ///
    /// # Panics
    ///
    /// Panics when `body` or any icon required by [`ThemeIcons::from_atlas`] is absent.
    pub fn from_atlas(atlas: &AtlasHandle) -> Self {
        // Resolve the required body capability first so every optional role has one valid,
        // owner-matched fallback instead of an ownerless default identifier.
        let font = atlas.font_id(FontRole::Body.atlas_name()).expect("atlas does not contain required font `body`");
        let colors = [
            Color { r: 230, g: 230, b: 230, a: 255 },
            Color { r: 25, g: 25, b: 25, a: 255 },
            Color { r: 50, g: 50, b: 50, a: 255 },
            Color { r: 25, g: 25, b: 25, a: 255 },
            Color { r: 240, g: 240, b: 240, a: 255 },
            Color { r: 0, g: 0, b: 0, a: 0 },
            Color { r: 75, g: 75, b: 75, a: 255 },
            Color { r: 95, g: 95, b: 95, a: 255 },
            Color { r: 30, g: 30, b: 30, a: 255 },
            Color { r: 35, g: 35, b: 35, a: 255 },
            Color { r: 43, g: 43, b: 43, a: 255 },
            Color { r: 30, g: 30, b: 30, a: 255 },
        ];
        let focus_color = Color { r: 0, g: 120, b: 215, a: 255 };
        let window_focus_color = Color { r: 0, g: 120, b: 215, a: 255 };
        let menu_background = Color { r: 50, g: 50, b: 50, a: 255 };
        let disabled_background_color = colors[ControlColor::WindowBG as usize];
        let foregrounds = ForegroundCatalog::from_flat_palette(
            colors[ControlColor::Text as usize],
            colors[ControlColor::TitleText as usize],
            Color { r: 230, g: 230, b: 230, a: 255 },
            colors[ControlColor::Text as usize],
            colors[ControlColor::TitleText as usize],
        );
        // Build the catalog from the same concrete flat values stored below. JSON theme loading
        // follows this identical fallback constructor before replacing explicitly supplied PNGs.
        let appearances = AppearanceCatalog::from_flat_palette(
            SliceInsets::uniform(1),
            colors,
            focus_color,
            window_focus_color,
            menu_background,
            disabled_background_color,
        );
        Self {
            font,
            small_font: atlas.font_id(FontRole::Small.atlas_name()).unwrap_or(font),
            title_font: atlas.font_id(FontRole::Title.atlas_name()).unwrap_or(font),
            heading_font: atlas.font_id(FontRole::Heading.atlas_name()).unwrap_or(font),
            mono_font: atlas.font_id(FontRole::Mono.atlas_name()).unwrap_or(font),
            icons: ThemeIcons::from_atlas(atlas),
            default_cell_width: 68,
            padding: 5,
            spacing: 4,
            indent: 24,
            title_height: 24,
            window_border: SliceInsets::uniform(1),
            scrollbar_size: 12,
            thumb_size: 8,
            appearances,
            foregrounds,
            focus_color,
            window_focus_color,
            menu_background,
            disabled_background_color,
            colors,
        }
    }

    /// Reports whether every retained font and icon capability belongs to `atlas`.
    pub(crate) fn belongs_to(&self, atlas: &AtlasHandle) -> bool {
        // List each concrete field so a new style asset cannot bypass validation through erased or
        // reflective storage. Ordinary scalar theme values need no atlas validation.
        atlas.contains_font(self.font)
            && atlas.contains_font(self.small_font)
            && atlas.contains_font(self.title_font)
            && atlas.contains_font(self.heading_font)
            && atlas.contains_font(self.mono_font)
            && self.icons.belongs_to(atlas)
    }

    /// Returns normalized structural frame insets shared by measurement and placement.
    #[cfg(any(feature = "theme-json", test))]
    pub(crate) fn frame_insets(&self) -> SliceInsets {
        // NinePatch owns normalization so layout and renderer geometry cannot disagree on negative
        // application-provided style components.
        self.appearance(AppearanceRole::GenericFrame, VisualState::Normal).insets.normalized()
    }

    /// Returns the exact patch assigned to one semantic role and visual state.
    pub fn appearance(&self, role: AppearanceRole, state: VisualState) -> NinePatch {
        // AppearanceCatalog guarantees both enum-indexed tables are complete.
        self.appearances.resolve(role, state)
    }

    /// Returns the exact foreground assigned to one semantic role and visual state.
    pub fn foreground(&self, role: AppearanceRole, state: VisualState) -> Color {
        // ForegroundCatalog guarantees the same total enum-indexed lookup contract as appearances.
        self.foregrounds.resolve(role, state)
    }

    /// Returns the concrete font ID for the provided semantic role.
    pub fn resolve_font_role(&self, role: FontRole) -> FontId {
        match role {
            FontRole::Body => self.font,
            FontRole::Small => self.small_font,
            FontRole::Title => self.title_font,
            FontRole::Heading => self.heading_font,
            FontRole::Mono => self.mono_font,
        }
    }

    /// Returns the concrete font ID for `choice`.
    pub fn resolve_font_choice(&self, choice: FontChoice) -> FontId {
        match choice {
            FontChoice::Role(role) => self.resolve_font_role(role),
            FontChoice::Id(font) => font,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas_with_font_sizes as make_test_atlas;

    /// Verifies semantic and explicit font choices preserve their concrete atlas capability.
    #[test]
    fn font_choice_conversions_preserve_selected_font() {
        let atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12), (FontRole::Heading.atlas_name(), 18)]);
        let heading = atlas.font_id(FontRole::Heading.atlas_name()).unwrap();

        assert_eq!(FontChoice::from(FontRole::Heading), FontChoice::role(FontRole::Heading));
        assert_eq!(FontChoice::from(heading), FontChoice::id(heading));
    }

    /// Verifies one-pass style construction resolves named roles and uses body for missing roles.
    #[test]
    fn from_atlas_resolves_named_fonts_and_falls_back_to_body() {
        let atlas = make_test_atlas(&[
            (FontRole::Small.atlas_name(), 10),
            (FontRole::Body.atlas_name(), 12),
            (FontRole::Heading.atlas_name(), 18),
            (FontRole::Title.atlas_name(), 16),
        ]);

        let style = Style::from_atlas(&atlas);

        assert_eq!(style.font, atlas.font_id(FontRole::Body.atlas_name()).unwrap());
        assert_eq!(style.small_font, atlas.font_id(FontRole::Small.atlas_name()).unwrap());
        assert_eq!(style.title_font, atlas.font_id(FontRole::Title.atlas_name()).unwrap());
        assert_eq!(style.heading_font, atlas.font_id(FontRole::Heading.atlas_name()).unwrap());
        assert_eq!(style.mono_font, style.font);
        assert!(style.belongs_to(&atlas));
    }

    /// Verifies every concrete retained capability participates in the ownership predicate.
    #[test]
    fn belongs_to_rejects_each_foreign_font_and_icon_field_individually() {
        let local_atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12)]);
        let foreign_atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12)]);
        let local = Style::from_atlas(&local_atlas);
        let foreign = Style::from_atlas(&foreign_atlas);

        // Each candidate differs from the valid local style in exactly one capability. Keeping the
        // cases explicit makes a newly added field fail this regression until belongs_to validates
        // it, without introducing erased reflection or `Any`-based field traversal.
        let mut candidates: [Style; 13] = std::array::from_fn(|_| local.clone());
        candidates[0].font = foreign.font;
        candidates[1].small_font = foreign.small_font;
        candidates[2].title_font = foreign.title_font;
        candidates[3].heading_font = foreign.heading_font;
        candidates[4].mono_font = foreign.mono_font;
        candidates[5].icons.close = foreign.icons.close;
        candidates[6].icons.expand = foreign.icons.expand;
        candidates[7].icons.collapse = foreign.icons.collapse;
        candidates[8].icons.check = foreign.icons.check;
        candidates[9].icons.expand_down = foreign.icons.expand_down;
        candidates[10].icons.open_folder = foreign.icons.open_folder;
        candidates[11].icons.closed_folder = foreign.icons.closed_folder;
        candidates[12].icons.file = foreign.icons.file;

        for candidate in candidates {
            assert!(!candidate.belongs_to(&local_atlas));
        }
    }

    /// Verifies standard style construction never substitutes a positional font for missing body.
    #[test]
    #[should_panic(expected = "atlas does not contain required font `body`")]
    fn from_atlas_requires_the_exact_body_font_name() {
        let atlas = make_test_atlas(&[("caption", 12)]);

        // Even though slot zero is a valid font, its unrelated name cannot satisfy the body role.
        let _ = Style::from_atlas(&atlas);
    }

    /// Verifies frame normalization remains independent of atlas-bound asset construction.
    #[test]
    fn frame_insets_normalize_each_component_without_changing_cells() {
        let atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12)]);
        let style = Style {
            appearances: {
                let mut appearances = Style::from_atlas(&atlas).appearances;
                appearances.set(
                    AppearanceRole::GenericFrame,
                    crate::StatefulAppearance::all(NinePatch::framed(SliceInsets::new(-4, 2, -3, 5), Color { r: 10, g: 20, b: 30, a: 255 }, None)),
                );
                appearances
            },
            ..Style::from_atlas(&atlas)
        };
        let insets = style.frame_insets();

        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (0, 2, 0, 5));
    }
}
