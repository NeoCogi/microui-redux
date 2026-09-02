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

/// Builds the private, complete visual table used by default and authored skins.
pub(crate) fn visuals_from_flat_palette(frame_insets: SliceInsets, palette: &FlatPalette) -> RoleTable<StateTable<Visual>> {
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
    // The nested fixed tables make every role/state pair present by construction. The table is
    // intentionally private to Skin so public callers cannot split a Visual into two updates.
    let mut catalog = RoleTable::filled(StateTable::filled(default));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;

    /// Converts a color into comparable channel data without changing the render value API.
    fn channels(value: Color) -> (u8, u8, u8, u8) {
        // Tests care about exact authored channels rather than requiring Color to implement Eq.
        (value.r, value.g, value.b, value.a)
    }

    /// Verifies cloned visual tables detach one role without changing their source value.
    #[test]
    fn visual_table_mutation_has_copy_on_write_value_semantics() {
        let normal = Visual::new(NinePatch::solid(color(1, 2, 3, 255)), color(4, 5, 6, 255));
        let replacement = Visual::new(NinePatch::solid(color(7, 8, 9, 255)), color(10, 11, 12, 255));
        let original = RoleTable::filled(StateTable::filled(normal));
        let mut changed = original.clone();
        let mut button = *changed.get(AppearanceRole::Button);
        button.set(VisualState::Pressed, replacement);
        changed.set(AppearanceRole::Button, button);

        assert_eq!(
            channels(original[AppearanceRole::Button][VisualState::Pressed].foreground),
            channels(normal.foreground)
        );
        assert_eq!(
            channels(changed[AppearanceRole::Button][VisualState::Pressed].foreground),
            channels(replacement.foreground)
        );
        assert_eq!(
            channels(changed[AppearanceRole::Button][VisualState::Normal].foreground),
            channels(normal.foreground)
        );
    }
}
