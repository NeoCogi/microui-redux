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

use super::{appearance::visuals_from_flat_palette, AppearanceRole, Color, FlatPalette, FontRef, FontRole, IconRole, RoleTable, StateTable, Visual, VisualState};
use crate::atlas::{AtlasHandle, FontId};
use crate::render::{NinePatch, SliceInsets};

/// Opaque identity of one immutable skin value installed through a [`crate::SkinBundle`].
///
/// Retained measurement caches compare this concrete token rather than mirroring selected Skin
/// fields or holding atlas pointers. The process-wide allocator never reuses a value, so an old
/// cache entry cannot match a later skin even when both happen to contain equal metrics.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct SkinRevision(
    /// Non-reused process identity kept private to the skinning subsystem.
    crate::identity::ProcessUniqueId,
);

impl SkinRevision {
    /// Allocates a revision for one newly constructed or newly bundled skin value.
    fn allocate() -> Self {
        // The identity source is shared with other capability domains but the wrapper prevents a
        // renderer, node, or surface identity from entering measurement-cache comparisons.
        Self(crate::identity::ProcessUniqueId::allocate())
    }
}

/// Horizontal alignment policy for manager-owned window title text.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum WindowTitleAlignment {
    /// Uses the available title span from its leading edge.
    #[default]
    Leading,
    /// Reserves symmetric caption banks and centers text in the complete title.
    Centered,
}

/// Edge selected for one manager-owned caption button.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum CaptionButtonSide {
    /// Allocates the button from the title's leading edge.
    Leading,
    /// Allocates the button from the title's trailing edge.
    #[default]
    Trailing,
}

/// Data recipe controlling manager-owned caption button geometry and presentation.
#[derive(Copy, Clone)]
pub struct CaptionButtonsSkin {
    /// Edge used by the close button.
    pub close_side: CaptionButtonSide,
    /// Edge used by the minimize button.
    pub minimize_side: CaptionButtonSide,
    /// Edge used by the maximize or restore button.
    pub maximize_side: CaptionButtonSide,
    /// Total pixels removed from title height to obtain each square caption extent.
    pub extent_inset: i32,
    /// Minimum square caption extent after applying the inset.
    pub minimum_extent: i32,
    /// Whether caption controls remain visible and hittable on inactive windows.
    pub show_when_inactive: bool,
    /// Whether a separate semantic glyph layer is painted over each caption button face.
    pub draw_separate_glyphs: bool,
}

/// Optional flat field painted behind active centered title text.
#[derive(Copy, Clone)]
pub struct TitleBackdropSkin {
    /// Flat background color interrupting title artwork behind the measured text.
    pub color: Color,
    /// Horizontal pixels added on each side of the measured title text.
    pub horizontal_padding: i32,
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
    /// Alignment and caption-bank reservation policy for title text.
    pub title_alignment: WindowTitleAlignment,
    /// Independent placement, extent, visibility, and fallback policy for caption buttons.
    pub captions: CaptionButtonsSkin,
    /// Optional active-title field painted behind measured text.
    pub active_title_backdrop: Option<TitleBackdropSkin>,
}

impl WindowChromeSkin {
    /// Creates a leading-title recipe with every caption button on the trailing edge.
    pub const fn trailing_buttons() -> Self {
        // This conventional recipe retains visible inactive controls and manager-drawn fallback
        // glyphs when a skin supplies only button backgrounds.
        Self {
            title_alignment: WindowTitleAlignment::Leading,
            captions: CaptionButtonsSkin {
                close_side: CaptionButtonSide::Trailing,
                minimize_side: CaptionButtonSide::Trailing,
                maximize_side: CaptionButtonSide::Trailing,
                extent_inset: 0,
                minimum_extent: 0,
                show_when_inactive: true,
                draw_separate_glyphs: true,
            },
            active_title_backdrop: None,
        }
    }

    /// Creates a centered-title recipe with split compact caption banks.
    pub const fn classic_mac(title_backdrop: Color) -> Self {
        // The recipe describes each independent behavior directly; manager code contains no
        // Classic-Mac mode branch and can also represent mixed application-defined arrangements.
        Self {
            title_alignment: WindowTitleAlignment::Centered,
            captions: CaptionButtonsSkin {
                close_side: CaptionButtonSide::Leading,
                minimize_side: CaptionButtonSide::Trailing,
                maximize_side: CaptionButtonSide::Trailing,
                extent_inset: 6,
                minimum_extent: 1,
                show_when_inactive: false,
                draw_separate_glyphs: false,
            },
            active_title_backdrop: Some(TitleBackdropSkin {
                color: title_backdrop,
                horizontal_padding: 4,
            }),
        }
    }

    /// Updates an existing optional title backdrop without changing chrome geometry policy.
    pub(crate) fn set_backdrop_color(&mut self, color: Color) {
        // Conventional recipes have no backdrop and therefore remain unchanged by palette edits.
        if let Some(backdrop) = &mut self.active_title_backdrop {
            backdrop.color = color;
        }
    }
}

impl Default for WindowChromeSkin {
    /// Returns the conventional trailing-caption recipe.
    fn default() -> Self {
        // Keep Skin::from_atlas and programmatic WindowChromeSkin defaults identical.
        Self::trailing_buttons()
    }
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
    /// Exact generation used by retained caches; refreshed whenever a bundle takes ownership.
    revision: SkinRevision,
    /// Layout and widget geometry values.
    pub metrics: SkinMetrics,
    /// Complete background and foreground visuals indexed by semantic role and interaction state.
    ///
    /// Storage stays private so every public read and replacement crosses the API as one `Visual`.
    /// This prevents callers from temporarily or permanently pairing a new background with the
    /// foreground belonging to an unrelated state.
    visuals: RoleTable<StateTable<Visual>>,
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
        let visuals = visuals_from_flat_palette(SliceInsets::uniform(1), &palette);
        Self {
            revision: SkinRevision::allocate(),
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
            chrome: WindowChromeSkin::trailing_buttons(),
        }
    }

    /// Returns the opaque generation associated with this complete skin value.
    pub(crate) const fn revision(&self) -> SkinRevision {
        // Copies are safe because a revision is an immutable identity, not a mutable counter.
        self.revision
    }

    /// Assigns a fresh generation when a bundle takes ownership of this skin.
    pub(crate) fn refresh_revision(&mut self) {
        // Public Skin fields remain ordinary concrete values. The bundle boundary is the one place
        // that turns their completed combination into a new cacheable immutable generation.
        self.revision = SkinRevision::allocate();
    }

    /// Replaces flat visual fallbacks and related effects from one authored palette.
    ///
    /// This operation is intended for builders and live skin editors. It compiles the palette
    /// immediately into the resolved catalog rather than retaining the palette as shadow state.
    pub fn apply_flat_palette(&mut self, palette: FlatPalette) {
        // Preserve the currently selected generic frame geometry while replacing all flat paint
        // values. Callers that need different frame geometry can update that typed visual after.
        let frame_insets = self.frame_insets();
        self.visuals = visuals_from_flat_palette(frame_insets, &palette);
        self.effects.focus_outline = palette.focus;
        self.effects.window_activation = palette.window_focus;
        self.chrome.set_backdrop_color(palette.title_background);
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
            .iter()
            .flat_map(StateTable::iter)
            .map(|visual| visual.patch)
            .filter_map(NinePatch::image_content)
            .all(|image| atlas.contains_icon(image.icon))
    }

    /// Returns normalized structural frame insets shared by measurement and placement.
    pub(crate) fn frame_insets(&self) -> SliceInsets {
        // NinePatch owns normalization so layout and renderer geometry cannot disagree on negative
        // application-provided style components.
        self.visual(AppearanceRole::GenericFrame, VisualState::Normal).patch.insets.normalized()
    }

    /// Returns the complete visual assigned to one semantic role and interaction state.
    pub fn visual(&self, role: AppearanceRole, state: VisualState) -> Visual {
        // Both indices are closed enums and both tables are total, so lookup cannot fail or require
        // a string key, type erasure, fallback branch, or runtime downcast.
        self.visuals[role][state]
    }

    /// Replaces one complete visual while preserving every sibling role and state.
    pub fn set_visual(&mut self, role: AppearanceRole, state: VisualState, visual: Visual) {
        // Copy the small fixed state table, update its typed slot, then replace the role atomically.
        // No API exists for publishing only the patch or foreground half of the new value.
        let mut states = self.role_visuals(role);
        states.set(state, visual);
        self.visuals.set(role, states);
    }

    /// Replaces the complete state table for one semantic role.
    pub fn set_role_visuals(&mut self, role: AppearanceRole, visuals: StateTable<Visual>) {
        // RoleTable supplies copy-on-write value semantics when a cloned Skin is customized.
        self.visuals.set(role, visuals);
    }

    /// Returns a copy of every complete interaction-state visual for one semantic role.
    pub(crate) fn role_visuals(&self, role: AppearanceRole) -> StateTable<Visual> {
        // Returning the concrete table by value lets builders prepare a coherent replacement
        // without exposing the private table shared by cloned skins.
        self.visuals[role]
    }

    /// Rebuilds all visual roles from a flat palette and a shared fallback frame geometry.
    pub(crate) fn replace_flat_visuals(&mut self, frame_insets: SliceInsets, palette: &FlatPalette) {
        // Theme loading compiles authoring data into the same private concrete table as defaults.
        // The serialized representation is never retained as a second source of runtime truth.
        self.visuals = visuals_from_flat_palette(frame_insets, palette);
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

    /// Verifies one-pass skin construction resolves named roles and uses body for missing roles.
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
        crate::test_support::replace_skin_patches(
            &mut candidate,
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

    /// Verifies standard skin construction never substitutes a positional font for missing body.
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
        let mut style = Skin::from_atlas(&atlas);
        crate::test_support::replace_skin_patches(
            &mut style,
            AppearanceRole::GenericFrame,
            crate::StateTable::filled(NinePatch::framed(SliceInsets::new(-4, 2, -3, 5), Color { r: 10, g: 20, b: 30, a: 255 }, None)),
        );
        let insets = style.frame_insets();

        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (0, 2, 0, 5));
    }
}
