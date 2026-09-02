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
//! Resolved, structured, atlas-bound skin values used across the crate.

use super::{AppearanceRole, Color, FlatPalette, FontRef, FontRole, IconRole, VisualCatalog, VisualState};
use crate::atlas::{AtlasHandle, FontId};
use crate::render::{NinePatch, SliceInsets};

/// Platform-oriented arrangement used for manager-owned window titles and caption buttons.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum WindowChromeLayout {
    /// Places every caption button on the trailing edge and left-aligns title text.
    #[default]
    TrailingButtons,
    /// Uses centered title text, a leading close box, and compact trailing zoom/windowshade boxes.
    ClassicMac,
}

/// Scalar geometry shared by layout and built-in widget measurement.
#[derive(Copy, Clone)]
pub struct SkinMetrics {
    /// Default width used by layouts when no preferred width is supplied.
    pub default_cell_width: i32,
    /// Inner padding applied to most widgets.
    pub padding: i32,
    /// Window-owned inset applied only around the application body after title and menu chrome.
    ///
    /// Keeping this separate from [`Self::padding`] lets a theme place application content flush
    /// against classic window chrome without also collapsing button, input, and title interiors.
    /// [`crate::WindowOption::NO_PADDING`] overrides these four values with zero for one root.
    pub window_content_insets: SliceInsets,
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
}

/// Manager-owned window chrome policy selected by the resolved skin.
#[derive(Copy, Clone)]
pub struct WindowChromeSkin {
    /// Platform-oriented title alignment and caption-button arrangement.
    ///
    /// This controls geometry only. Button faces, title stripes, and every interaction state remain
    /// ordinary typed appearance roles supplied by the active skin.
    pub layout: WindowChromeLayout,
    /// Flat label field painted behind centered active-title text when requested by the layout.
    pub title_backdrop: Color,
}

/// Paint effects that are not themselves semantic role/state visuals.
#[derive(Copy, Clone)]
pub struct SkinEffects {
    /// Accent used by the universal keyboard-focus outline.
    pub focus_outline: Color,
    /// Accent used by manager-owned active-window outlines.
    pub window_activation: Color,
}

/// Complete resolved runtime skin installed with one matching atlas.
///
/// A `Skin` contains only values consumed at runtime. Flat palettes and serialized authoring data
/// are compiled into these concrete fields and are not retained as alternate sources of truth.
#[derive(Clone)]
pub struct Skin {
    /// Layout and widget geometry values.
    pub metrics: SkinMetrics,
    /// Unified background and foreground visuals used by built-in UI parts.
    ///
    /// Each semantic role and interaction state resolves one complete value, so background art
    /// and its adjacent text or glyph color cannot drift into independently configured catalogs.
    pub visuals: VisualCatalog,
    /// Non-catalog focus and activation effects.
    pub effects: SkinEffects,
    /// Window-title and caption-control arrangement.
    pub chrome: WindowChromeSkin,
}

impl Skin {
    /// Constructs the default visual metrics and resolves every retained asset from `atlas`.
    ///
    /// The required `body` font and semantic theme icon names form the standard Context atlas
    /// contract. Optional font roles fall back to that same atlas's body font.
    ///
    /// # Panics
    ///
    /// Panics when `body` or any icon required by [`IconRole::ALL`] is absent.
    pub fn from_atlas(atlas: &AtlasHandle) -> Self {
        // Validate standard names once at construction. The skin retains no allocation-bound font
        // or icon IDs; concrete capabilities are minted only when active layout or paint needs one.
        atlas.font_id(FontRole::Body.atlas_name()).expect("atlas does not contain required font `body`");
        for role in IconRole::ALL {
            role.resolve(atlas);
        }
        let palette = FlatPalette::default();
        // Build complete visuals from the same concrete flat values stored below. JSON theme
        // loading follows this identical fallback constructor before replacing authored states.
        let visuals = VisualCatalog::from_flat_palette(SliceInsets::uniform(1), &palette);
        Self {
            metrics: SkinMetrics {
                default_cell_width: 68,
                padding: 5,
                window_content_insets: SliceInsets::uniform(5),
                spacing: 4,
                indent: 24,
                title_height: 24,
                window_border: SliceInsets::uniform(1),
                scrollbar_size: 12,
                thumb_size: 8,
            },
            visuals,
            effects: SkinEffects {
                focus_outline: palette.focus,
                window_activation: palette.window_focus,
            },
            chrome: WindowChromeSkin {
                layout: WindowChromeLayout::TrailingButtons,
                title_backdrop: palette.title_background,
            },
        }
    }

    /// Replaces flat visual fallbacks and related effects from one authored palette.
    ///
    /// This operation is intended for builders and live skin editors. It compiles the palette
    /// immediately into the resolved catalog rather than retaining the palette as shadow state.
    pub fn apply_flat_palette(&mut self, palette: FlatPalette) {
        // Preserve the currently selected generic frame geometry while replacing all flat paint
        // values. Callers that need different frame geometry can update that typed visual after.
        let frame_insets = self.frame_insets();
        self.visuals = VisualCatalog::from_flat_palette(frame_insets, &palette);
        self.effects.focus_outline = palette.focus;
        self.effects.window_activation = palette.window_focus;
        self.chrome.title_backdrop = palette.title_background;
    }

    /// Applies a concrete metrics edit and returns the resulting skin.
    ///
    /// The closure keeps grouped metrics construction concise in programmatic skins and tests
    /// without restoring a flat duplicate field surface on `Skin`.
    pub fn with_metrics(mut self, configure: impl FnOnce(&mut SkinMetrics)) -> Self {
        // Metrics are plain concrete values, so configuration is immediate and cannot be retained
        // as an erased callback or deferred mutation.
        configure(&mut self.metrics);
        self
    }

    /// Reports whether every image-backed visual capability belongs to `atlas`.
    pub(crate) fn belongs_to(&self, atlas: &AtlasHandle) -> bool {
        // Retained widget resources are stable FontRef/IconRef values and therefore need no owner
        // check. Only compiled image patches contain renderer-facing atlas capabilities.
        self.visuals
            .patches()
            .filter_map(NinePatch::image_content)
            .all(|image| atlas.contains_icon(image.icon))
    }

    /// Returns normalized structural frame insets shared by measurement and placement.
    pub(crate) fn frame_insets(&self) -> SliceInsets {
        // NinePatch owns normalization so layout and renderer geometry cannot disagree on negative
        // application-provided style components.
        self.appearance(AppearanceRole::GenericFrame, VisualState::Normal).insets.normalized()
    }

    /// Returns the exact patch assigned to one semantic role and visual state.
    pub fn appearance(&self, role: AppearanceRole, state: VisualState) -> NinePatch {
        // VisualCatalog guarantees both enum-indexed table layers are complete.
        self.visuals.resolve(role, state).patch
    }

    /// Returns the exact foreground assigned to one semantic role and visual state.
    pub fn foreground(&self, role: AppearanceRole, state: VisualState) -> Color {
        // The foreground is selected from the same concrete value as its background patch.
        self.visuals.resolve(role, state).foreground
    }

    /// Returns the active atlas capability for one semantic font role.
    ///
    /// Optional semantic names fall back to the required body font. This keeps compact atlases
    /// useful without retaining IDs that become stale when the atlas is replaced.
    pub fn resolve_font_role(&self, atlas: &AtlasHandle, role: FontRole) -> FontId {
        let body = atlas.font_id(FontRole::Body.atlas_name()).expect("atlas does not contain required font `body`");
        // The role remains meaningful across themes even when a compact theme omits its dedicated
        // font. Body is already proven to belong to this exact atlas allocation.
        atlas.font_id(role.atlas_name()).unwrap_or(body)
    }

    /// Returns the active atlas capability for one stable retained font reference.
    pub fn resolve_font(&self, atlas: &AtlasHandle, font: &FontRef) -> FontId {
        // Delegate to the typed reference so named and semantic references have one resolution
        // implementation shared by application and built-in widgets.
        font.resolve(self, atlas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas_with_font_sizes as make_test_atlas;

    /// Verifies semantic and named font references resolve through the active atlas.
    #[test]
    fn font_references_resolve_semantic_and_named_fonts() {
        let atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12), (FontRole::Heading.atlas_name(), 18)]);
        let heading = atlas.font_id(FontRole::Heading.atlas_name()).unwrap();
        let style = Skin::from_atlas(&atlas);

        assert_eq!(FontRef::from(FontRole::Heading), FontRef::role(FontRole::Heading));
        assert_eq!(FontRef::role(FontRole::Heading).resolve(&style, &atlas), heading);
        assert_eq!(FontRef::named("heading").resolve(&style, &atlas), heading);
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

        let style = Skin::from_atlas(&atlas);

        let body = atlas.font_id(FontRole::Body.atlas_name()).unwrap();
        assert_eq!(style.resolve_font_role(&atlas, FontRole::Body), body);
        assert_eq!(
            style.resolve_font_role(&atlas, FontRole::Small),
            atlas.font_id(FontRole::Small.atlas_name()).unwrap()
        );
        assert_eq!(
            style.resolve_font_role(&atlas, FontRole::Title),
            atlas.font_id(FontRole::Title.atlas_name()).unwrap()
        );
        assert_eq!(
            style.resolve_font_role(&atlas, FontRole::Heading),
            atlas.font_id(FontRole::Heading.atlas_name()).unwrap()
        );
        assert_eq!(style.resolve_font_role(&atlas, FontRole::Mono), body);
        assert!(style.belongs_to(&atlas));
    }

    /// Verifies every image-backed visual capability participates in the ownership predicate.
    #[test]
    fn belongs_to_rejects_a_foreign_appearance_capability() {
        let local_atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12)]);
        let foreign_atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12)]);
        let local = Skin::from_atlas(&local_atlas);

        // Stable FontRef and IconRef values do not participate in atlas ownership. A compiled
        // image patch remains the sole atlas-bound skin value and must still be rejected.
        let mut candidate = local;
        candidate.visuals.set_patches(
            AppearanceRole::Button,
            crate::StateTable::filled(NinePatch::image(
                SliceInsets::ZERO,
                crate::NinePatchImage::new(
                    IconRole::Close.resolve(&foreign_atlas),
                    SliceInsets::ZERO,
                    Color { r: 255, g: 255, b: 255, a: 255 },
                ),
            )),
        );

        assert!(!candidate.belongs_to(&local_atlas));
    }

    /// Verifies standard style construction never substitutes a positional font for missing body.
    #[test]
    #[should_panic(expected = "atlas does not contain required font `body`")]
    fn from_atlas_requires_the_exact_body_font_name() {
        let atlas = make_test_atlas(&[("caption", 12)]);

        // Even though slot zero is a valid font, its unrelated name cannot satisfy the body role.
        let _ = Skin::from_atlas(&atlas);
    }

    /// Verifies frame normalization remains independent of atlas-bound asset construction.
    #[test]
    fn frame_insets_normalize_each_component_without_changing_cells() {
        let atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12)]);
        let style = Skin {
            visuals: {
                let mut visuals = Skin::from_atlas(&atlas).visuals;
                visuals.set_patches(
                    AppearanceRole::GenericFrame,
                    crate::StateTable::filled(NinePatch::framed(SliceInsets::new(-4, 2, -3, 5), Color { r: 10, g: 20, b: 30, a: 255 }, None)),
                );
                visuals
            },
            ..Skin::from_atlas(&atlas)
        };
        let insets = style.frame_insets();

        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (0, 2, 0, 5));
    }
}
