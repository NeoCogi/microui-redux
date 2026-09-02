//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Unified background and foreground visuals for every semantic role and interaction state.

use crate::{Color, FlatPalette, NinePatch, SliceInsets};

use super::{AppearanceRole, RoleTable, StateTable, VisualState};

/// Complete paint description selected for one semantic role and interaction state.
///
/// The background patch and its adjacent text or glyph color intentionally travel together.
/// Keeping them in one concrete value prevents independently mutated catalogs from describing two
/// different states for the same widget.
#[derive(Copy, Clone)]
pub struct Visual {
    /// Background, border, or image-backed nine-patch painted for the visual.
    pub patch: NinePatch,
    /// Foreground color used for text and semantic glyphs over the patch.
    pub foreground: Color,
}

impl Visual {
    /// Creates one complete visual from its background and foreground values.
    pub const fn new(patch: NinePatch, foreground: Color) -> Self {
        // Requiring both halves at construction keeps a visual complete at every API boundary.
        Self { patch, foreground }
    }
}

/// Complete state table for one semantic appearance role.
pub type StatefulVisual = StateTable<Visual>;

/// Cheaply cloneable catalog containing one complete visual for every role and state.
#[derive(Clone)]
pub struct VisualCatalog {
    /// Copy-on-write role table whose entries are total fixed-size state tables.
    entries: RoleTable<StatefulVisual>,
}

impl VisualCatalog {
    /// Creates a complete catalog using one visual for every role and state.
    pub fn filled(default: Visual) -> Self {
        // Both table layers are concrete and total; paint-time resolution cannot fail or downcast.
        Self {
            entries: RoleTable::filled(StateTable::filled(default)),
        }
    }

    /// Returns a copy of the complete visual-state table for `role`.
    pub fn get(&self, role: AppearanceRole) -> StatefulVisual {
        // State tables contain small Copy paint values, so returning a value keeps mutation local.
        *self.entries.get(role)
    }

    /// Replaces every interaction state for exactly one semantic role.
    pub fn set(&mut self, role: AppearanceRole, states: StatefulVisual) {
        // RoleTable preserves copy-on-write value semantics for cloned skins.
        self.entries.set(role, states);
    }

    /// Replaces one complete role/state visual.
    pub fn set_state(&mut self, role: AppearanceRole, state: VisualState, visual: Visual) {
        // Read-modify-write preserves all sibling states without exposing catalog storage.
        let mut states = self.get(role);
        states.set(state, visual);
        self.set(role, states);
    }

    /// Replaces one role/state patch while preserving its matching foreground.
    pub fn set_patch(&mut self, role: AppearanceRole, state: VisualState, patch: NinePatch) {
        // Partial authoring changes still update the single unified runtime catalog.
        let mut visual = self.resolve(role, state);
        visual.patch = patch;
        self.set_state(role, state, visual);
    }

    /// Replaces every state patch for a role while retaining all foreground colors.
    pub fn set_patches(&mut self, role: AppearanceRole, patches: StateTable<NinePatch>) {
        // This helper supports programmatic skins without recreating unchanged foreground values.
        let mut visuals = self.get(role);
        for state in VisualState::ALL {
            let mut visual = *visuals.get(state);
            visual.patch = *patches.get(state);
            visuals.set(state, visual);
        }
        self.set(role, visuals);
    }

    /// Replaces one role/state foreground while preserving its matching patch.
    pub fn set_foreground(&mut self, role: AppearanceRole, state: VisualState, foreground: Color) {
        // Foreground customization cannot drift into a separate interaction-state table.
        let mut visual = self.resolve(role, state);
        visual.foreground = foreground;
        self.set_state(role, state, visual);
    }

    /// Replaces every state foreground for a role while retaining all background patches.
    pub fn set_foregrounds(&mut self, role: AppearanceRole, foregrounds: StateTable<Color>) {
        // This helper is the typed counterpart of `set_patches` for programmatic skin builders.
        let mut visuals = self.get(role);
        for state in VisualState::ALL {
            let mut visual = *visuals.get(state);
            visual.foreground = *foregrounds.get(state);
            visuals.set(state, visual);
        }
        self.set(role, visuals);
    }

    /// Resolves one exact semantic role and interaction state.
    pub fn resolve(&self, role: AppearanceRole, state: VisualState) -> Visual {
        // Both indices are exhaustive enums, making lookup total and allocation-free.
        *self.entries.get(role).get(state)
    }

    /// Iterates over every retained patch in role-major, state-minor order.
    pub(crate) fn patches(&self) -> impl Iterator<Item = NinePatch> + '_ {
        // Only patches carry atlas image capabilities; foreground colors require no validation.
        self.entries.iter().flat_map(StateTable::iter).map(|visual| visual.patch)
    }

    /// Returns each role's normalized normal-state insets for retained measurement identity.
    pub(crate) fn measurement_insets(&self) -> [[i32; 4]; AppearanceRole::COUNT] {
        // A container can measure descendants using any role, so the current cache contract needs
        // all structural patch geometry even though colors and image pixels remain paint-only.
        std::array::from_fn(|index| {
            let role = AppearanceRole::ALL[index];
            let insets = self.resolve(role, VisualState::Normal).patch.insets.normalized();
            [insets.left, insets.top, insets.right, insets.bottom]
        })
    }

    /// Builds the complete flat fallback catalog used by default and authored skins.
    pub(crate) fn from_flat_palette(frame_insets: SliceInsets, palette: &FlatPalette) -> Self {
        // Resolve named palette values once and assemble complete Visual values directly. No
        // parallel appearance or foreground catalog exists before or after this construction.
        let border = palette.border;
        let text = palette.text;
        let title_text = palette.title_foreground;
        let transparent = Color { r: 0, g: 0, b: 0, a: 0 };
        let framed = |fill| NinePatch::framed(frame_insets, border, Some(fill));
        let hollow = NinePatch::framed(frame_insets, border, None);
        let solid = NinePatch::solid;

        let mut body_foregrounds = StateTable::filled(text);
        body_foregrounds.set(VisualState::Disabled, palette.disabled_foreground);
        let mut menu_foregrounds = StateTable::filled(palette.menu_foreground);
        menu_foregrounds.set(VisualState::Disabled, palette.disabled_foreground);
        let mut title_foregrounds = StateTable::filled(title_text);
        title_foregrounds.set(VisualState::Disabled, palette.disabled_title_foreground);

        let foregrounds_for = |role| {
            if matches!(
                role,
                AppearanceRole::MenuBar
                    | AppearanceRole::MenuTitle
                    | AppearanceRole::MenuTitleOpen
                    | AppearanceRole::MenuPopup
                    | AppearanceRole::MenuItem
                    | AppearanceRole::MenuItemSelected
            ) {
                menu_foregrounds
            } else if matches!(
                role,
                AppearanceRole::WindowTitle
                    | AppearanceRole::WindowTitleActive
                    | AppearanceRole::WindowCloseButton
                    | AppearanceRole::WindowMinimizeButton
                    | AppearanceRole::WindowMaximizeButton
                    | AppearanceRole::WindowRestoreButton
                    | AppearanceRole::WindowCloseGlyph
                    | AppearanceRole::WindowMinimizeGlyph
                    | AppearanceRole::WindowMaximizeGlyph
                    | AppearanceRole::WindowRestoreGlyph
            ) {
                title_foregrounds
            } else {
                body_foregrounds
            }
        };

        let combine = |patches: StateTable<NinePatch>, foregrounds: StateTable<Color>| {
            StateTable::new(std::array::from_fn(|index| {
                let state = VisualState::ALL[index];
                Visual::new(*patches.get(state), *foregrounds.get(state))
            }))
        };
        let patch_states = |normal, hovered, pressed, focused, disabled| StateTable::new([normal, hovered, pressed, focused, focused, pressed, disabled]);
        let with_disabled = |mut patches: StateTable<NinePatch>, disabled| {
            patches.set(VisualState::Disabled, disabled);
            patches
        };

        let button = patch_states(
            framed(palette.button),
            framed(palette.button_hovered),
            framed(palette.input),
            framed(palette.focus),
            framed(palette.disabled_background),
        );
        let input = patch_states(
            framed(palette.input),
            framed(palette.input_hovered),
            framed(palette.input_hovered),
            framed(palette.focus),
            framed(palette.disabled_background),
        );
        let highlight = patch_states(
            solid(transparent),
            solid(palette.button_hovered),
            solid(palette.button),
            solid(palette.focus),
            solid(transparent),
        );
        let selected = patch_states(
            solid(palette.focus),
            solid(palette.button_hovered),
            solid(palette.button),
            solid(palette.focus),
            solid(palette.disabled_background),
        );
        let window = patch_states(
            framed(palette.window_background),
            framed(palette.window_background),
            framed(palette.window_background),
            framed(palette.window_background),
            framed(palette.disabled_background),
        );

        let default = Visual::new(NinePatch::solid(transparent), text);
        let mut catalog = Self::filled(default);
        let mut assign = |role, patches| catalog.set(role, combine(patches, foregrounds_for(role)));

        assign(AppearanceRole::GenericFrame, StateTable::filled(hollow));
        assign(
            AppearanceRole::Panel,
            with_disabled(StateTable::filled(framed(palette.panel_background)), framed(palette.disabled_background)),
        );
        assign(AppearanceRole::Button, button);
        assign(AppearanceRole::Checkbox, input);
        assign(AppearanceRole::CheckboxChecked, input);
        assign(AppearanceRole::TextInput, input);
        assign(AppearanceRole::ListItem, highlight);
        assign(AppearanceRole::ListItemSelected, selected);
        assign(AppearanceRole::Combo, button);
        assign(AppearanceRole::SliderTrack, input);
        assign(AppearanceRole::SliderThumb, button);
        assign(
            AppearanceRole::ScrollbarTrack,
            with_disabled(StateTable::filled(solid(palette.scrollbar_track)), solid(palette.disabled_background)),
        );
        assign(
            AppearanceRole::ScrollbarThumb,
            with_disabled(StateTable::filled(solid(palette.scrollbar_thumb)), solid(palette.disabled_background)),
        );
        assign(AppearanceRole::DisclosureHeader, highlight);
        assign(
            AppearanceRole::MenuBar,
            with_disabled(StateTable::filled(solid(palette.menu_background)), solid(palette.disabled_background)),
        );
        assign(AppearanceRole::MenuTitle, highlight);
        assign(AppearanceRole::MenuTitleOpen, selected);
        assign(
            AppearanceRole::MenuPopup,
            with_disabled(StateTable::filled(framed(palette.menu_background)), framed(palette.disabled_background)),
        );
        assign(AppearanceRole::MenuItem, highlight);
        // Marker state and interaction state are independent, so a checked row is not permanently
        // highlighted merely because its marker is visible.
        assign(AppearanceRole::MenuItemSelected, highlight);

        let active_window = StateTable::filled(NinePatch::framed(
            frame_insets.at_least(1),
            palette.window_focus,
            Some(palette.window_background),
        ));
        assign(AppearanceRole::WindowFrame, window);
        assign(AppearanceRole::WindowFrameActive, active_window);
        assign(AppearanceRole::DialogFrame, window);
        assign(AppearanceRole::DialogFrameActive, active_window);
        assign(AppearanceRole::WindowTitle, StateTable::filled(solid(palette.title_background)));
        assign(AppearanceRole::WindowTitleActive, StateTable::filled(solid(palette.window_focus)));
        assign(AppearanceRole::WindowCloseButton, button);
        assign(AppearanceRole::WindowMinimizeButton, button);
        assign(AppearanceRole::WindowMaximizeButton, button);
        assign(AppearanceRole::WindowRestoreButton, button);
        assign(AppearanceRole::WindowResizeGrip, input);
        catalog
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;

    /// Converts a color into comparable channel data without changing the render value API.
    fn channels(value: Color) -> (u8, u8, u8, u8) {
        // Tests care about exact authored channels rather than requiring Color to implement Eq.
        (value.r, value.g, value.b, value.a)
    }

    /// Verifies cloned catalogs detach one role without changing their source value.
    #[test]
    fn visual_catalog_mutation_has_copy_on_write_value_semantics() {
        let normal = Visual::new(NinePatch::solid(color(1, 2, 3, 255)), color(4, 5, 6, 255));
        let replacement = Visual::new(NinePatch::solid(color(7, 8, 9, 255)), color(10, 11, 12, 255));
        let original = VisualCatalog::filled(normal);
        let mut changed = original.clone();
        changed.set_state(AppearanceRole::Button, VisualState::Pressed, replacement);

        assert_eq!(
            channels(original.resolve(AppearanceRole::Button, VisualState::Pressed).foreground),
            channels(normal.foreground)
        );
        assert_eq!(
            channels(changed.resolve(AppearanceRole::Button, VisualState::Pressed).foreground),
            channels(replacement.foreground)
        );
        assert_eq!(
            channels(changed.resolve(AppearanceRole::Button, VisualState::Normal).foreground),
            channels(normal.foreground)
        );
    }

    /// Verifies patch-only and foreground-only edits converge on one role/state value.
    #[test]
    fn partial_visual_edits_preserve_the_other_half() {
        let first_patch = NinePatch::solid(color(1, 2, 3, 255));
        let second_patch = NinePatch::solid(color(4, 5, 6, 255));
        let first_foreground = color(7, 8, 9, 255);
        let second_foreground = color(10, 11, 12, 255);
        let mut catalog = VisualCatalog::filled(Visual::new(first_patch, first_foreground));

        catalog.set_patch(AppearanceRole::Checkbox, VisualState::Hovered, second_patch);
        catalog.set_foreground(AppearanceRole::Checkbox, VisualState::Hovered, second_foreground);

        let visual = catalog.resolve(AppearanceRole::Checkbox, VisualState::Hovered);
        assert_eq!(
            (
                visual.patch.insets.left,
                visual.patch.insets.top,
                visual.patch.insets.right,
                visual.patch.insets.bottom
            ),
            (
                second_patch.insets.left,
                second_patch.insets.top,
                second_patch.insets.right,
                second_patch.insets.bottom,
            )
        );
        assert_eq!(channels(visual.foreground), channels(second_foreground));
        assert_eq!(
            channels(catalog.resolve(AppearanceRole::Checkbox, VisualState::Normal).foreground),
            channels(first_foreground)
        );
    }
}
