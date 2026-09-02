//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Concrete, deterministic partial edits for resolved skins.

use crate::{Color, NinePatch, SliceInsets};

use super::{AppearanceRole, RoleTable, Skin, StateTable, VisualCatalog, VisualState, WindowChromeSkin};

/// Optional scalar geometry edits applied to [`crate::SkinMetrics`].
///
/// Every field corresponds one-to-one with the resolved metrics structure. `None` preserves the
/// destination value, while `Some` replaces it exactly; no sentinel numbers or string keys exist.
#[derive(Copy, Clone, Default)]
pub struct SkinMetricsPatch {
    /// Optional replacement for the default unconstrained cell width.
    pub default_cell_width: Option<i32>,
    /// Optional replacement for ordinary widget inner padding.
    pub padding: Option<i32>,
    /// Optional replacement for the four application-body window insets.
    pub window_content_insets: Option<SliceInsets>,
    /// Optional replacement for spacing between layout cells.
    pub spacing: Option<i32>,
    /// Optional replacement for nested-content indentation.
    pub indent: Option<i32>,
    /// Optional replacement for the minimum window-title height.
    pub title_height: Option<i32>,
    /// Optional replacement for structural window border thickness.
    pub window_border: Option<SliceInsets>,
    /// Optional replacement for scrollbar width.
    pub scrollbar_size: Option<i32>,
    /// Optional replacement for minimum scrollbar and slider thumb size.
    pub thumb_size: Option<i32>,
}

impl SkinMetricsPatch {
    /// Reports whether this group preserves every resolved metric.
    pub fn is_empty(&self) -> bool {
        // Keep the condition exhaustive so adding a metric patch field requires updating merge and
        // application behavior rather than silently leaving the field inert.
        self.default_cell_width.is_none()
            && self.padding.is_none()
            && self.window_content_insets.is_none()
            && self.spacing.is_none()
            && self.indent.is_none()
            && self.title_height.is_none()
            && self.window_border.is_none()
            && self.scrollbar_size.is_none()
            && self.thumb_size.is_none()
    }

    /// Applies every present value to one resolved metrics structure.
    fn apply_to(&self, metrics: &mut crate::SkinMetrics) {
        // Explicit field mapping is intentionally repetitive: the compiler catches type drift, and
        // readers can audit the complete merge contract without reflection or erased values.
        assign_if_some(&mut metrics.default_cell_width, self.default_cell_width);
        assign_if_some(&mut metrics.padding, self.padding);
        assign_if_some(&mut metrics.window_content_insets, self.window_content_insets);
        assign_if_some(&mut metrics.spacing, self.spacing);
        assign_if_some(&mut metrics.indent, self.indent);
        assign_if_some(&mut metrics.title_height, self.title_height);
        assign_if_some(&mut metrics.window_border, self.window_border);
        assign_if_some(&mut metrics.scrollbar_size, self.scrollbar_size);
        assign_if_some(&mut metrics.thumb_size, self.thumb_size);
    }

    /// Merges `later` over this group using last-present-value wins semantics.
    fn merge_later(&mut self, later: &Self) {
        // The same field order as apply_to makes precedence deterministic and easy to compare.
        replace_option(&mut self.default_cell_width, later.default_cell_width);
        replace_option(&mut self.padding, later.padding);
        replace_option(&mut self.window_content_insets, later.window_content_insets);
        replace_option(&mut self.spacing, later.spacing);
        replace_option(&mut self.indent, later.indent);
        replace_option(&mut self.title_height, later.title_height);
        replace_option(&mut self.window_border, later.window_border);
        replace_option(&mut self.scrollbar_size, later.scrollbar_size);
        replace_option(&mut self.thumb_size, later.thumb_size);
    }
}

/// Optional edits for one exact appearance role and visual state.
///
/// Patch and foreground remain adjacent in this concrete value, matching [`crate::Visual`]. Either
/// component may be omitted without creating a second parallel role/state catalog.
#[derive(Copy, Clone, Default)]
pub struct VisualPatch {
    /// Optional replacement for background, border, or image-backed artwork.
    pub patch: Option<NinePatch>,
    /// Optional replacement for text and semantic-glyph color over that artwork.
    pub foreground: Option<Color>,
}

impl VisualPatch {
    /// Creates an edit replacing both halves of one complete visual.
    pub const fn complete(patch: NinePatch, foreground: Color) -> Self {
        // Keep both values together at construction, mirroring the resolved Visual invariant.
        Self {
            patch: Some(patch),
            foreground: Some(foreground),
        }
    }

    /// Creates an edit replacing only the visual's patch component.
    pub const fn patch(patch: NinePatch) -> Self {
        // Foreground remains explicitly absent rather than copied from an unrelated default.
        Self { patch: Some(patch), foreground: None }
    }

    /// Creates an edit replacing only the visual's foreground component.
    pub const fn foreground(foreground: Color) -> Self {
        // Artwork remains explicitly absent so application preserves the destination patch.
        Self {
            patch: None,
            foreground: Some(foreground),
        }
    }

    /// Reports whether both visual components are omitted.
    pub const fn is_empty(self) -> bool {
        // This exact predicate is shared by catalog emptiness and sparse application traversal.
        self.patch.is_none() && self.foreground.is_none()
    }

    /// Merges a later edit into this exact role/state cell.
    fn merge_later(&mut self, later: Self) {
        // Components have independent presence, but never leave their typed visual cell.
        replace_option(&mut self.patch, later.patch);
        replace_option(&mut self.foreground, later.foreground);
    }
}

/// Sparse typed overrides for the exhaustive appearance role/state domain.
///
/// Storage uses the same closed enum-indexed tables as [`VisualCatalog`]. `Option` appears only
/// inside [`VisualPatch`] fields, so there are no typeless maps, numeric indices, or downcasts.
#[derive(Clone)]
pub struct VisualPatchCatalog {
    /// Complete role table containing complete state tables of concrete optional edits.
    entries: RoleTable<StateTable<VisualPatch>>,
}

impl Default for VisualPatchCatalog {
    /// Creates a catalog that preserves every resolved visual.
    fn default() -> Self {
        // Complete empty cells keep lookup total while representing sparse author input.
        Self {
            entries: RoleTable::filled(StateTable::filled(VisualPatch::default())),
        }
    }
}

impl VisualPatchCatalog {
    /// Returns the exact optional edit stored for one role and state.
    pub fn get(&self, role: AppearanceRole, state: VisualState) -> VisualPatch {
        // Both enum lookups are total and copy only one two-field value.
        *self.entries.get(role).get(state)
    }

    /// Replaces the optional edit for one exact role and state.
    pub fn set(&mut self, role: AppearanceRole, state: VisualState, patch: VisualPatch) {
        // Read-modify-write preserves sibling states while RoleTable retains copy-on-write clones.
        let mut states = *self.entries.get(role);
        states.set(state, patch);
        self.entries.set(role, states);
    }

    /// Reports whether the catalog preserves every resolved visual component.
    pub fn is_empty(&self) -> bool {
        // Fixed table traversal is deterministic role-major/state-minor and allocates nothing.
        self.entries.iter().flat_map(StateTable::iter).all(|patch| patch.is_empty())
    }

    /// Applies present cells to one complete resolved visual catalog.
    fn apply_to(&self, visuals: &mut VisualCatalog) {
        for role in AppearanceRole::ALL {
            for state in VisualState::ALL {
                let patch = self.get(role, state);
                if patch.is_empty() {
                    // Sparse patches do not detach the destination catalog for absent cells.
                    continue;
                }
                let mut visual = visuals.resolve(role, state);
                assign_if_some(&mut visual.patch, patch.patch);
                assign_if_some(&mut visual.foreground, patch.foreground);
                visuals.set_state(role, state, visual);
            }
        }
    }

    /// Merges `later` over this catalog using component-wise last-present-value wins semantics.
    fn merge_later(&mut self, later: &Self) {
        for role in AppearanceRole::ALL {
            for state in VisualState::ALL {
                let later_patch = later.get(role, state);
                if later_patch.is_empty() {
                    // Omitted later cells preserve this catalog without forcing copy-on-write.
                    continue;
                }
                let mut merged = self.get(role, state);
                merged.merge_later(later_patch);
                self.set(role, state, merged);
            }
        }
    }
}

/// Optional replacements for non-catalog paint effects.
#[derive(Copy, Clone, Default)]
pub struct SkinEffectsPatch {
    /// Optional replacement for the universal keyboard-focus outline color.
    pub focus_outline: Option<Color>,
    /// Optional replacement for the active-window outline color.
    pub window_activation: Option<Color>,
}

impl SkinEffectsPatch {
    /// Reports whether this group preserves both resolved effects.
    pub const fn is_empty(self) -> bool {
        // Both concrete fields participate in the group contract.
        self.focus_outline.is_none() && self.window_activation.is_none()
    }

    /// Applies every present effect value.
    fn apply_to(self, effects: &mut crate::SkinEffects) {
        // Map each optional field directly to its resolved counterpart.
        assign_if_some(&mut effects.focus_outline, self.focus_outline);
        assign_if_some(&mut effects.window_activation, self.window_activation);
    }

    /// Merges later present effect values over this group.
    fn merge_later(&mut self, later: Self) {
        // Omitted later values intentionally preserve earlier authoring layers.
        replace_option(&mut self.focus_outline, later.focus_outline);
        replace_option(&mut self.window_activation, later.window_activation);
    }
}

/// Optional replacements for manager-owned window chrome policy.
#[derive(Copy, Clone, Default)]
pub struct WindowChromePatch {
    /// Optional replacement for the complete data-driven chrome recipe.
    pub recipe: Option<WindowChromeSkin>,
}

impl WindowChromePatch {
    /// Reports whether this group preserves all resolved chrome policy.
    pub const fn is_empty(self) -> bool {
        // The recipe is one internally coherent concrete value with no hidden mode discriminator.
        self.recipe.is_none()
    }

    /// Applies every present chrome value.
    fn apply_to(self, chrome: &mut crate::WindowChromeSkin) {
        // Whole-recipe replacement prevents partially applying mutually dependent bank geometry.
        assign_if_some(chrome, self.recipe);
    }

    /// Merges later present chrome values over this group.
    fn merge_later(&mut self, later: Self) {
        // Last-present-value wins matches metrics, effects, and visuals.
        replace_option(&mut self.recipe, later.recipe);
    }
}

/// Complete typed partial edit for one resolved [`Skin`].
///
/// The patch mirrors the structured runtime skin and uses one precedence rule at every leaf:
/// omitted values preserve earlier data and present values replace it. Applying merged patches is
/// therefore deterministic and produces the same result as applying their layers in order.
#[derive(Clone, Default)]
pub struct SkinPatch {
    /// Optional scalar geometry edits.
    pub metrics: SkinMetricsPatch,
    /// Optional role/state patch and foreground edits.
    pub visuals: VisualPatchCatalog,
    /// Optional focus and activation effect edits.
    pub effects: SkinEffectsPatch,
    /// Optional manager-owned window chrome edits.
    pub chrome: WindowChromePatch,
}

impl SkinPatch {
    /// Reports whether applying this patch would preserve every resolved skin value.
    pub fn is_empty(&self) -> bool {
        // Delegate to all four concrete groups so new skin categories cannot be silently omitted.
        self.metrics.is_empty() && self.visuals.is_empty() && self.effects.is_empty() && self.chrome.is_empty()
    }

    /// Merges `later` over this patch using one deterministic precedence rule at every leaf.
    pub fn merge_later(&mut self, later: &Self) {
        // Group order has no semantic effect because each group owns disjoint resolved fields. It
        // remains fixed for auditability and matches Skin's public field order.
        self.metrics.merge_later(&later.metrics);
        self.visuals.merge_later(&later.visuals);
        self.effects.merge_later(later.effects);
        self.chrome.merge_later(later.chrome);
    }

    /// Applies this sparse authoring layer to one complete resolved skin.
    pub(crate) fn apply_to(&self, skin: &mut Skin) {
        // Apply groups in resolved Skin order. Visual component edits resolve against the current
        // destination, making partial patch/foreground changes safe after earlier layers.
        self.metrics.apply_to(&mut skin.metrics);
        self.visuals.apply_to(&mut skin.visuals);
        self.effects.apply_to(&mut skin.effects);
        self.chrome.apply_to(&mut skin.chrome);
    }
}

/// Assigns one present copy value to a complete resolved destination.
fn assign_if_some<T: Copy>(destination: &mut T, value: Option<T>) {
    // This helper contains no storage or type erasure; monomorphization retains each concrete type.
    if let Some(value) = value {
        *destination = value;
    }
}

/// Replaces one optional earlier value only when a later layer is present.
fn replace_option<T: Copy>(earlier: &mut Option<T>, later: Option<T>) {
    // The helper encodes the sole merge rule shared across concrete patch leaf types.
    if later.is_some() {
        *earlier = later;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Converts a concrete color into an equality-friendly channel tuple for assertions.
    fn channels(color: Color) -> (u8, u8, u8, u8) {
        // Color intentionally exposes plain fields without deriving comparison traits.
        (color.r, color.g, color.b, color.a)
    }

    /// Converts concrete insets into an equality-friendly edge tuple for assertions.
    fn edges(insets: SliceInsets) -> (i32, i32, i32, i32) {
        // Preserve the public left/top/right/bottom order used by geometry and JSON documents.
        (insets.left, insets.top, insets.right, insets.bottom)
    }

    /// Verifies a sparse patch changes only its exact concrete leaves.
    #[test]
    fn sparse_patch_preserves_unmentioned_skin_values() {
        let atlas = crate::test_support::test_atlas();
        let mut skin = Skin::from_atlas(&atlas);
        let old_spacing = skin.metrics.spacing;
        let old_foreground = skin.foreground(AppearanceRole::Button, VisualState::Hovered);
        let replacement = Color { r: 1, g: 2, b: 3, a: 255 };
        let mut patch = SkinPatch::default();
        patch.metrics.padding = Some(17);
        patch
            .visuals
            .set(AppearanceRole::Button, VisualState::Hovered, VisualPatch::foreground(replacement));

        skin.apply_patch(&patch);

        assert_eq!(skin.metrics.padding, 17);
        assert_eq!(skin.metrics.spacing, old_spacing);
        assert_eq!(channels(skin.foreground(AppearanceRole::Button, VisualState::Hovered)), channels(replacement));
        assert_ne!(channels(old_foreground), channels(replacement));
        assert!(skin.appearance(AppearanceRole::Button, VisualState::Hovered).is_visible());
    }

    /// Verifies merged layers exactly match sequential application and use later precedence.
    #[test]
    fn merged_patch_matches_ordered_application() {
        let atlas = crate::test_support::test_atlas();
        let base = Skin::from_atlas(&atlas);
        let first_color = Color { r: 10, g: 20, b: 30, a: 255 };
        let later_color = Color { r: 40, g: 50, b: 60, a: 255 };
        let mut first = SkinPatch::default();
        first.metrics.padding = Some(8);
        first.effects.focus_outline = Some(first_color);
        first
            .visuals
            .set(AppearanceRole::TextInput, VisualState::Focused, VisualPatch::foreground(first_color));
        let mut later = SkinPatch::default();
        later.metrics.spacing = Some(11);
        later.effects.focus_outline = Some(later_color);
        later.visuals.set(
            AppearanceRole::TextInput,
            VisualState::Focused,
            VisualPatch::patch(NinePatch::solid(later_color)),
        );

        let mut sequential = base.clone();
        sequential.apply_patch(&first);
        sequential.apply_patch(&later);
        let mut merged_patch = first;
        merged_patch.merge_later(&later);
        let mut merged = base;
        merged.apply_patch(&merged_patch);

        assert_eq!(merged.metrics.padding, sequential.metrics.padding);
        assert_eq!(merged.metrics.spacing, sequential.metrics.spacing);
        assert_eq!(channels(merged.effects.focus_outline), channels(later_color));
        assert_eq!(
            channels(merged.foreground(AppearanceRole::TextInput, VisualState::Focused)),
            channels(sequential.foreground(AppearanceRole::TextInput, VisualState::Focused))
        );
        assert_eq!(
            edges(merged.appearance(AppearanceRole::TextInput, VisualState::Focused).insets),
            edges(sequential.appearance(AppearanceRole::TextInput, VisualState::Focused).insets)
        );
    }

    /// Verifies the default patch has no hidden mutation or synthesized fallback values.
    #[test]
    fn default_patch_is_empty() {
        // A total typed table represents absence without allocating maps or manufacturing sentinels.
        assert!(SkinPatch::default().is_empty());
    }
}
