//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Atlas-independent flat colors consumed while constructing a resolved [`crate::Skin`].

use crate::Color;

/// Named flat colors used as inputs to visual-catalog construction.
///
/// A palette is an authoring value, not retained runtime skin state. Compiling it eagerly into
/// complete [`crate::Visual`] values avoids the former shadow color array that could disagree with
/// what widgets actually painted.
#[derive(Copy, Clone)]
pub struct FlatPalette {
    /// Ordinary control text and semantic glyph color.
    pub text: Color,
    /// Generic flat frame and control border color.
    pub border: Color,
    /// Window and dialog client-area background.
    pub window_background: Color,
    /// Passive window title background.
    pub title_background: Color,
    /// Window title and caption foreground.
    pub title_foreground: Color,
    /// Panel and scroll-viewport background.
    pub panel_background: Color,
    /// Ordinary raised-control fill.
    pub button: Color,
    /// Hovered raised-control fill.
    pub button_hovered: Color,
    /// Ordinary recessed-control fill.
    pub input: Color,
    /// Hovered recessed-control fill.
    pub input_hovered: Color,
    /// Scrollbar track fill.
    pub scrollbar_track: Color,
    /// Scrollbar thumb fill.
    pub scrollbar_thumb: Color,
    /// Focused control border or fill accent.
    pub control_focus: Color,
    /// Background used by hovered, keyboard-focused, open, or otherwise selected items.
    pub selection_background: Color,
    /// Text and glyph color paired with [`Self::selection_background`].
    pub selection_foreground: Color,
    /// Active window frame and title accent.
    pub window_active: Color,
    /// Menu text and marker foreground.
    pub menu_foreground: Color,
    /// Menu bar and popup background.
    pub menu_background: Color,
    /// Disabled control and client-area background.
    pub disabled_background: Color,
    /// Disabled body, menu, and control foreground.
    pub disabled_foreground: Color,
    /// Disabled title and caption foreground.
    pub disabled_title_foreground: Color,
}

impl Default for FlatPalette {
    /// Returns the crate's neutral dark flat-skin recipe.
    fn default() -> Self {
        // Every value is named at construction, preventing positional palette indices from leaking
        // into loaders, editors, or widget paint code.
        Self {
            text: Color { r: 230, g: 230, b: 230, a: 255 },
            border: Color { r: 25, g: 25, b: 25, a: 255 },
            window_background: Color { r: 50, g: 50, b: 50, a: 255 },
            title_background: Color { r: 25, g: 25, b: 25, a: 255 },
            title_foreground: Color { r: 240, g: 240, b: 240, a: 255 },
            panel_background: Color { r: 0, g: 0, b: 0, a: 0 },
            button: Color { r: 75, g: 75, b: 75, a: 255 },
            button_hovered: Color { r: 95, g: 95, b: 95, a: 255 },
            input: Color { r: 30, g: 30, b: 30, a: 255 },
            input_hovered: Color { r: 35, g: 35, b: 35, a: 255 },
            scrollbar_track: Color { r: 43, g: 43, b: 43, a: 255 },
            scrollbar_thumb: Color { r: 30, g: 30, b: 30, a: 255 },
            control_focus: Color { r: 0, g: 120, b: 215, a: 255 },
            selection_background: Color { r: 0, g: 120, b: 215, a: 255 },
            selection_foreground: Color { r: 255, g: 255, b: 255, a: 255 },
            window_active: Color { r: 0, g: 120, b: 215, a: 255 },
            menu_foreground: Color { r: 230, g: 230, b: 230, a: 255 },
            menu_background: Color { r: 50, g: 50, b: 50, a: 255 },
            disabled_background: Color { r: 50, g: 50, b: 50, a: 255 },
            disabled_foreground: Color { r: 230, g: 230, b: 230, a: 255 },
            disabled_title_foreground: Color { r: 240, g: 240, b: 240, a: 255 },
        }
    }
}
