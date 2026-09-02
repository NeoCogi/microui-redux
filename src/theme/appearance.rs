//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Complete background and foreground visuals compiled for each appearance family.

use crate::{ChromeRole, ChromeState, ControlRole, ControlState, MenuRole, MenuState, PointerState, SurfaceRole, SurfaceState};

use crate::{Color, FlatPalette, NinePatch, SliceInsets};

use super::catalog::AppearanceCatalog;

/// Complete paint description selected for one semantic role and meaningful state.
///
/// The background patch and its adjacent text or glyph color intentionally travel together.
/// Keeping them in one concrete value prevents independently mutated catalogs from describing two
/// different states for the same control.
#[derive(Copy, Clone)]
pub struct Visual {
    /// Background, border, or image-backed nine-patch painted for the visual.
    pub patch: NinePatch,
    /// Foreground color used for text and semantic glyphs over the patch.
    ///
    /// A transparent value naturally suppresses separately drawn semantic content when the patch
    /// already contains the control's complete label or symbol.
    pub foreground: Color,
}

impl Visual {
    /// Creates one complete visual from its background and foreground values.
    pub const fn new(patch: NinePatch, foreground: Color) -> Self {
        // Requiring both halves at construction keeps a visual complete at every API boundary.
        Self { patch, foreground }
    }
}

/// Builds the complete family catalogs used by default and authored skins.
pub(crate) fn visuals_from_flat_palette(frame_insets: SliceInsets, palette: &FlatPalette) -> AppearanceCatalog {
    // Resolve named palette values once. Every family builder below exhaustively assigns its own
    // states, so no universal fallback or cross-family state conversion can hide a missing policy.
    let border = palette.border;
    let text = palette.text;
    let transparent = Color { r: 0, g: 0, b: 0, a: 0 };
    let framed = |fill| NinePatch::framed(frame_insets, border, Some(fill));
    let hollow = NinePatch::framed(frame_insets, border, None);
    let solid = NinePatch::solid;

    AppearanceCatalog::from_fns(
        |role, state| {
            // Structural surfaces react only to inherited availability.
            let patch = match (role, state) {
                (SurfaceRole::GenericFrame, _) => hollow,
                (SurfaceRole::Panel, SurfaceState::Normal) => framed(palette.panel_background),
                (SurfaceRole::Panel, SurfaceState::Disabled) => framed(palette.disabled_background),
            };
            let foreground = match state {
                SurfaceState::Normal => text,
                SurfaceState::Disabled => palette.disabled_foreground,
            };
            Visual::new(patch, foreground)
        },
        |role, state| {
            // Control families share one nested state domain, while each role still owns its exact
            // patch and foreground policy. Exhaustive matching prevents a new role from silently
            // inheriting a button or text-input appearance.
            let patch = match role {
                ControlRole::Button
                | ControlRole::Combo
                | ControlRole::SliderThumb
                | ControlRole::CloseButton
                | ControlRole::MinimizeButton
                | ControlRole::MaximizeButton
                | ControlRole::RestoreButton => match state {
                    ControlState::Disabled => framed(palette.disabled_background),
                    ControlState::Enabled(PointerState::Normal) => framed(palette.button),
                    ControlState::Enabled(PointerState::Hovered) => framed(palette.button_hovered),
                    ControlState::Enabled(PointerState::Pressed) => framed(palette.input),
                    ControlState::Focused(PointerState::Normal | PointerState::Hovered) => framed(palette.control_focus),
                    ControlState::Focused(PointerState::Pressed) => framed(palette.input),
                },
                ControlRole::Checkbox | ControlRole::TextInput | ControlRole::SliderTrack | ControlRole::ResizeGrip => match state {
                    ControlState::Disabled => framed(palette.disabled_background),
                    ControlState::Enabled(PointerState::Normal) => framed(palette.input),
                    ControlState::Enabled(PointerState::Hovered | PointerState::Pressed) => framed(palette.input_hovered),
                    ControlState::Focused(PointerState::Normal | PointerState::Hovered) => framed(palette.control_focus),
                    ControlState::Focused(PointerState::Pressed) => framed(palette.input_hovered),
                },
                ControlRole::Item => match state {
                    ControlState::Disabled | ControlState::Enabled(PointerState::Normal) => solid(transparent),
                    ControlState::Enabled(PointerState::Hovered | PointerState::Pressed)
                    | ControlState::Focused(PointerState::Normal | PointerState::Hovered | PointerState::Pressed) => solid(palette.selection_background),
                },
                ControlRole::ScrollbarTrack => match state {
                    ControlState::Disabled => solid(palette.disabled_background),
                    ControlState::Enabled(_) | ControlState::Focused(_) => solid(palette.scrollbar_track),
                },
                ControlRole::ScrollbarThumb => match state {
                    ControlState::Disabled => solid(palette.disabled_background),
                    ControlState::Enabled(_) | ControlState::Focused(_) => solid(palette.scrollbar_thumb),
                },
            };
            let foreground = match role {
                ControlRole::Item => match state {
                    ControlState::Disabled => palette.disabled_foreground,
                    ControlState::Enabled(PointerState::Normal) => text,
                    ControlState::Enabled(PointerState::Hovered | PointerState::Pressed)
                    | ControlState::Focused(PointerState::Normal | PointerState::Hovered | PointerState::Pressed) => palette.selection_foreground,
                },
                ControlRole::CloseButton | ControlRole::MinimizeButton | ControlRole::MaximizeButton | ControlRole::RestoreButton => match state {
                    ControlState::Disabled => palette.disabled_title_foreground,
                    ControlState::Enabled(_) | ControlState::Focused(_) => palette.title_foreground,
                },
                _ => match state {
                    ControlState::Disabled => palette.disabled_foreground,
                    ControlState::Enabled(_) | ControlState::Focused(_) => text,
                },
            };
            Visual::new(patch, foreground)
        },
        |role, state| {
            // Open is a menu state rather than a duplicate title role. Popup owners and selected
            // submenu entries can therefore use the same explicit precedence and appearance.
            let selected = matches!(state, MenuState::Hovered | MenuState::Pressed | MenuState::Focused | MenuState::Open);
            let patch = match role {
                MenuRole::Bar => match state {
                    MenuState::Disabled => solid(palette.disabled_background),
                    _ => solid(palette.menu_background),
                },
                MenuRole::Title | MenuRole::Item => {
                    if selected {
                        solid(palette.selection_background)
                    } else {
                        solid(transparent)
                    }
                }
                MenuRole::Popup => match state {
                    MenuState::Disabled => framed(palette.disabled_background),
                    _ => framed(palette.menu_background),
                },
            };
            let foreground = match state {
                MenuState::Disabled => palette.disabled_foreground,
                _ if selected && matches!(role, MenuRole::Title | MenuRole::Item) => palette.selection_foreground,
                _ => palette.menu_foreground,
            };
            Visual::new(patch, foreground)
        },
        |role, state| {
            // Window activation belongs solely to chrome. Control descendants continue resolving
            // their own enabled, focus, and pointer states independently of the active window.
            let patch = match role {
                ChromeRole::WindowFrame | ChromeRole::DialogFrame => match state {
                    ChromeState::Base => framed(palette.window_background),
                    ChromeState::Active => NinePatch::framed(frame_insets.at_least(1), palette.window_active, Some(palette.window_background)),
                    ChromeState::Disabled => framed(palette.disabled_background),
                },
                ChromeRole::Title => match state {
                    ChromeState::Base => solid(palette.title_background),
                    ChromeState::Active => solid(palette.window_active),
                    ChromeState::Disabled => solid(palette.title_background),
                },
            };
            let foreground = match (role, state) {
                (ChromeRole::Title, ChromeState::Disabled) => palette.disabled_title_foreground,
                (ChromeRole::Title, ChromeState::Base | ChromeState::Active) => palette.title_foreground,
                (_, ChromeState::Disabled) => palette.disabled_foreground,
                (_, ChromeState::Base | ChromeState::Active) => text,
            };
            Visual::new(patch, foreground)
        },
    )
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

    /// Verifies a dark control-focus accent cannot darken an independently selected item.
    #[test]
    fn control_focus_and_item_selection_compile_to_independent_visuals() {
        let palette = FlatPalette {
            control_focus: color(0, 0, 0, 255),
            selection_background: color(0, 0, 170, 255),
            selection_foreground: color(255, 255, 255, 255),
            ..FlatPalette::default()
        };

        // Compile once through the production fallback builder so the assertion covers the exact
        // family/state mapping responsible for focused tree and list rows.
        let visuals = visuals_from_flat_palette(SliceInsets::uniform(1), &palette);
        let state = ControlState::Focused(PointerState::Normal);
        let control = visuals.control(ControlRole::Button, state);
        let item = visuals.control(ControlRole::Item, state);

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
