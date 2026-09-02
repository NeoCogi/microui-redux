//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Unified background and foreground visuals for every semantic role and interaction state.

use crate::{ChromeRole, ControlRole, MenuRole, SurfaceRole};

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
    let interactive_foregrounds = |normal, disabled| {
        StateTable::new([
            normal,
            palette.selection_foreground,
            palette.selection_foreground,
            palette.selection_foreground,
            palette.selection_foreground,
            palette.selection_foreground,
            disabled,
        ])
    };
    let selected_foregrounds = |disabled| {
        StateTable::new([
            palette.selection_foreground,
            palette.selection_foreground,
            palette.selection_foreground,
            palette.selection_foreground,
            palette.selection_foreground,
            palette.selection_foreground,
            disabled,
        ])
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
        framed(palette.control_focus),
        framed(palette.disabled_background),
    );
    let input = patch_states(
        framed(palette.input),
        framed(palette.input_hovered),
        framed(palette.input_hovered),
        framed(palette.control_focus),
        framed(palette.disabled_background),
    );
    let highlight = StateTable::new([
        solid(transparent),
        solid(palette.selection_background),
        solid(palette.selection_background),
        solid(palette.selection_background),
        solid(palette.selection_background),
        solid(palette.selection_background),
        solid(transparent),
    ]);
    let selected = with_disabled(StateTable::filled(solid(palette.selection_background)), solid(palette.disabled_background));
    let window = patch_states(
        framed(palette.window_background),
        framed(palette.window_background),
        framed(palette.window_background),
        framed(palette.window_background),
        framed(palette.disabled_background),
    );

    let active_window = StateTable::filled(NinePatch::framed(
        frame_insets.at_least(1),
        palette.window_active,
        Some(palette.window_background),
    ));

    // Construct each role exactly once, with its background and foreground policy adjacent. The
    // exhaustive family matches make new roles a compile error here instead of silently inheriting
    // the generic default that the former initialize-then-overwrite implementation provided.
    RoleTable::from_fn(|role| {
        let (patches, foregrounds) = match role {
            AppearanceRole::Surface(role) => match role {
                SurfaceRole::GenericFrame => (StateTable::filled(hollow), body_foregrounds),
                SurfaceRole::Panel => (
                    with_disabled(StateTable::filled(framed(palette.panel_background)), framed(palette.disabled_background)),
                    body_foregrounds,
                ),
            },
            AppearanceRole::Control(role) => match role {
                ControlRole::Button | ControlRole::Combo | ControlRole::SliderThumb => (button, body_foregrounds),
                ControlRole::Checkbox | ControlRole::TextInput | ControlRole::SliderTrack => (input, body_foregrounds),
                ControlRole::Item => (highlight, interactive_foregrounds(text, palette.disabled_foreground)),
                ControlRole::ScrollbarTrack => (
                    with_disabled(StateTable::filled(solid(palette.scrollbar_track)), solid(palette.disabled_background)),
                    body_foregrounds,
                ),
                ControlRole::ScrollbarThumb => (
                    with_disabled(StateTable::filled(solid(palette.scrollbar_thumb)), solid(palette.disabled_background)),
                    body_foregrounds,
                ),
            },
            AppearanceRole::Menu(role) => match role {
                MenuRole::Bar => (
                    with_disabled(StateTable::filled(solid(palette.menu_background)), solid(palette.disabled_background)),
                    menu_foregrounds,
                ),
                MenuRole::Title => (highlight, interactive_foregrounds(palette.menu_foreground, palette.disabled_foreground)),
                MenuRole::TitleOpen => (selected, selected_foregrounds(palette.disabled_foreground)),
                MenuRole::Popup => (
                    with_disabled(StateTable::filled(framed(palette.menu_background)), framed(palette.disabled_background)),
                    menu_foregrounds,
                ),
                MenuRole::Item => (highlight, interactive_foregrounds(palette.menu_foreground, palette.disabled_foreground)),
            },
            AppearanceRole::Chrome(role) => match role {
                ChromeRole::WindowFrame | ChromeRole::DialogFrame => (window, body_foregrounds),
                ChromeRole::WindowFrameActive | ChromeRole::DialogFrameActive => (active_window, body_foregrounds),
                ChromeRole::Title => (StateTable::filled(solid(palette.title_background)), title_foregrounds),
                ChromeRole::TitleActive => (StateTable::filled(solid(palette.window_active)), title_foregrounds),
                ChromeRole::CloseButton | ChromeRole::MinimizeButton | ChromeRole::MaximizeButton | ChromeRole::RestoreButton => (button, title_foregrounds),
                ChromeRole::ResizeGrip => (input, body_foregrounds),
                // Flat fallback skins draw caption glyphs procedurally. These transparent values
                // reserve image-backed glyph roles for authored themes without drawing a duplicate.
                ChromeRole::CloseGlyph | ChromeRole::MinimizeGlyph | ChromeRole::MaximizeGlyph | ChromeRole::RestoreGlyph => {
                    (StateTable::filled(solid(transparent)), StateTable::filled(text))
                }
            },
        };
        combine(patches, foregrounds)
    })
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
        let mut button = *changed.get(AppearanceRole::Control(ControlRole::Button));
        button.set(VisualState::Pressed, replacement);
        changed.set(AppearanceRole::Control(ControlRole::Button), button);

        assert_eq!(
            channels(original[AppearanceRole::Control(ControlRole::Button)][VisualState::Pressed].foreground),
            channels(normal.foreground)
        );
        assert_eq!(
            channels(changed[AppearanceRole::Control(ControlRole::Button)][VisualState::Pressed].foreground),
            channels(replacement.foreground)
        );
        assert_eq!(
            channels(changed[AppearanceRole::Control(ControlRole::Button)][VisualState::Normal].foreground),
            channels(normal.foreground)
        );
    }

    /// Verifies a dark control-focus accent cannot darken an independently selected item.
    #[test]
    fn control_focus_and_item_selection_compile_to_independent_visuals() {
        let mut palette = FlatPalette::default();
        palette.control_focus = color(0, 0, 0, 255);
        palette.selection_background = color(0, 0, 170, 255);
        palette.selection_foreground = color(255, 255, 255, 255);

        // Compile once through the production fallback builder so the assertion covers the exact
        // semantic role/state mapping responsible for focused tree and list rows.
        let visuals = visuals_from_flat_palette(SliceInsets::uniform(1), &palette);
        let control = visuals[AppearanceRole::Control(ControlRole::Button)][VisualState::Focused];
        let item = visuals[AppearanceRole::Control(ControlRole::Item)][VisualState::Focused];

        assert!(matches!(
            control.patch.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if channels(color) == (0, 0, 0, 255))
        ));
        assert!(matches!(
            item.patch.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if channels(color) == (0, 0, 170, 255))
        ));
        assert_eq!(channels(item.foreground), (255, 255, 255, 255));
    }
}
