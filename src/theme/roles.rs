//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Exhaustive semantic indices used by the concrete skin tables.
//!
//! This module deliberately defines each indexed enum and its serialized spelling in one place.
//! Adding a role or state therefore changes the enum, its count, its ordered traversal, and its
//! external name together instead of relying on several manually synchronized match expressions.

/// Defines a fieldless enum whose declaration order is also its fixed-table index order.
///
/// The macro is private because the generated enums are the public contract; exposing the macro
/// would let downstream crates create unrelated index domains that the skin tables cannot store.
macro_rules! indexed_enum {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $external_name:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        #[repr(u8)]
        pub enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        impl $name {
            /// Number of values in this closed index domain.
            pub const COUNT: usize = [$(Self::$variant),+].len();

            /// Every value in discriminant and fixed-table order.
            pub const ALL: [Self; Self::COUNT] = [$(Self::$variant),+];

            /// Returns the stable snake-case spelling used by authored skin documents.
            pub const fn json_name(self) -> &'static str {
                // Exhaustive matching makes the compiler require an external spelling whenever a
                // new semantic value is introduced.
                match self {
                    $(Self::$variant => $external_name,)+
                }
            }

            /// Parses one exact authored spelling into its concrete enum value.
            #[cfg(feature = "theme-json")]
            #[allow(dead_code, reason = "every generated domain owns its parser before all document maps use it")]
            pub(crate) fn from_json_name(name: &str) -> Option<Self> {
                // The closed ordered array is small and parsing happens only while compiling a
                // skin, so a linear scan is clearer than maintaining another lookup structure.
                Self::ALL.into_iter().find(|value| value.json_name() == name)
            }

            /// Converts the value into its checked fixed-table index.
            pub(crate) const fn index(self) -> usize {
                // The representation is contiguous because the macro emits no explicit
                // discriminants and callers cannot construct an enum outside these variants.
                self as usize
            }
        }
    };
}

indexed_enum! {
    /// Structural surface painted without knowledge of a concrete container implementation.
    pub enum SurfaceRole {
        /// Generic frame requested through [`crate::WidgetOption::FRAME`].
        GenericFrame => "generic_frame",
        /// Content panel or scroll-area viewport background.
        Panel => "panel",
    }
}

indexed_enum! {
    /// Interactive control part shared by built-in widgets with equivalent visual behavior.
    pub enum ControlRole {
        /// Ordinary command button or button-like list box.
        Button => "button",
        /// Checkbox square; checked state is represented by the independently painted check glyph.
        Checkbox => "checkbox",
        /// Single-line, multiline, or numeric text input background.
        TextInput => "text_input",
        /// Generic item or tree row whose interaction state supplies transient selection.
        Item => "item",
        /// Combo-box header.
        Combo => "combo",
        /// Slider background track.
        SliderTrack => "slider_track",
        /// Slider value thumb.
        SliderThumb => "slider_thumb",
        /// Scrollbar background track.
        ScrollbarTrack => "scrollbar_track",
        /// Scrollbar movable thumb.
        ScrollbarThumb => "scrollbar_thumb",
    }
}

indexed_enum! {
    /// Menu-owned surface or entry independent of retained widget implementation types.
    pub enum MenuRole {
        /// Menu bar spanning a window.
        Bar => "menu_bar",
        /// Menu-bar title that does not own an open popup.
        Title => "menu_title",
        /// Menu-bar title whose popup is open.
        TitleOpen => "menu_title_open",
        /// Popup-menu panel.
        Popup => "menu_popup",
        /// Ordinary menu item row.
        Item => "menu_item",
    }
}

indexed_enum! {
    /// Manager-owned window frame, title, caption, glyph, or resize visual.
    pub enum ChromeRole {
        /// Normal window outer frame and body.
        WindowFrame => "window_frame",
        /// Active window outer frame and body.
        WindowFrameActive => "window_frame_active",
        /// Normal modal-dialog outer frame and body.
        DialogFrame => "dialog_frame",
        /// Active modal-dialog outer frame and body.
        DialogFrameActive => "dialog_frame_active",
        /// Normal window title background.
        Title => "window_title",
        /// Active window title background.
        TitleActive => "window_title_active",
        /// Window close caption button.
        CloseButton => "window_close_button",
        /// Window minimize caption button.
        MinimizeButton => "window_minimize_button",
        /// Window maximize caption button.
        MaximizeButton => "window_maximize_button",
        /// Window restore caption button used while maximized.
        RestoreButton => "window_restore_button",
        /// Visible bottom-right resize grip.
        ResizeGrip => "window_resize_grip",
        /// Optional themed glyph painted inside the window close button.
        CloseGlyph => "window_close_glyph",
        /// Optional themed glyph painted inside the window minimize button.
        MinimizeGlyph => "window_minimize_glyph",
        /// Optional themed glyph painted inside the window maximize button.
        MaximizeGlyph => "window_maximize_glyph",
        /// Optional themed glyph painted inside the window restore button.
        RestoreGlyph => "window_restore_glyph",
    }
}

/// Semantic family and visual part painted by the built-in user interface.
///
/// The outer category describes a rendering domain, while each nested enum names only parts in
/// that domain. This prevents the loader and skin catalog from acquiring knowledge of concrete
/// widget or container Rust types without introducing erased storage or dynamic dispatch.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AppearanceRole {
    /// Structural frame or panel presentation.
    Surface(SurfaceRole),
    /// Interactive control presentation.
    Control(ControlRole),
    /// Compact menu presentation.
    Menu(MenuRole),
    /// Manager-owned window chrome presentation.
    Chrome(ChromeRole),
}

impl AppearanceRole {
    /// Number of values across all closed appearance families.
    pub const COUNT: usize = SurfaceRole::COUNT + ControlRole::COUNT + MenuRole::COUNT + ChromeRole::COUNT;

    /// Every categorized role in fixed-table order.
    pub const ALL: [Self; Self::COUNT] = Self::collect_all();

    /// Builds [`Self::ALL`] from each family's independently generated ordered values.
    const fn collect_all() -> [Self; Self::COUNT] {
        // Populate one concrete array in family order so declarations remain authoritative and no
        // manually duplicated flattened role list can drift from a nested enum.
        let mut roles = [Self::Surface(SurfaceRole::GenericFrame); Self::COUNT];
        let mut destination = 0;
        let mut source = 0;
        while source < SurfaceRole::COUNT {
            roles[destination] = Self::Surface(SurfaceRole::ALL[source]);
            destination += 1;
            source += 1;
        }
        source = 0;
        while source < ControlRole::COUNT {
            roles[destination] = Self::Control(ControlRole::ALL[source]);
            destination += 1;
            source += 1;
        }
        source = 0;
        while source < MenuRole::COUNT {
            roles[destination] = Self::Menu(MenuRole::ALL[source]);
            destination += 1;
            source += 1;
        }
        source = 0;
        while source < ChromeRole::COUNT {
            roles[destination] = Self::Chrome(ChromeRole::ALL[source]);
            destination += 1;
            source += 1;
        }
        roles
    }

    /// Returns the current flat theme spelling for the categorized role.
    pub const fn json_name(self) -> &'static str {
        // Flat spellings remain only until the following schema commit replaces the document
        // shape. Delegation keeps this independently buildable commit free of new aliases.
        match self {
            Self::Surface(role) => role.json_name(),
            Self::Control(role) => role.json_name(),
            Self::Menu(role) => role.json_name(),
            Self::Chrome(role) => role.json_name(),
        }
    }

    /// Parses one current flat theme spelling into its categorized role.
    #[cfg(feature = "theme-json")]
    pub(crate) fn from_json_name(name: &str) -> Option<Self> {
        // Each family owns disjoint spellings in the current document, so concrete attempts require
        // neither a string registry nor knowledge of widget implementation types.
        if let Some(role) = SurfaceRole::from_json_name(name) {
            Some(Self::Surface(role))
        } else if let Some(role) = ControlRole::from_json_name(name) {
            Some(Self::Control(role))
        } else if let Some(role) = MenuRole::from_json_name(name) {
            Some(Self::Menu(role))
        } else {
            ChromeRole::from_json_name(name).map(Self::Chrome)
        }
    }

    /// Converts one categorized role into its checked flattened table index.
    pub(crate) const fn index(self) -> usize {
        // Fixed family offsets preserve one compact exhaustive catalog without exposing a second
        // storage abstraction for each category.
        match self {
            Self::Surface(role) => role.index(),
            Self::Control(role) => SurfaceRole::COUNT + role.index(),
            Self::Menu(role) => SurfaceRole::COUNT + ControlRole::COUNT + role.index(),
            Self::Chrome(role) => SurfaceRole::COUNT + ControlRole::COUNT + MenuRole::COUNT + role.index(),
        }
    }
}

indexed_enum! {
    /// Mutually exclusive interaction state used to select one visual.
    pub enum VisualState {
        /// Enabled widget with no hover, focus, or active press.
        Normal => "normal",
        /// Pointer is over an enabled widget.
        Hovered => "hovered",
        /// Pointer capture is pressed over an enabled widget.
        Pressed => "pressed",
        /// Keyboard focus is visible without pointer hover.
        Focused => "focused",
        /// Keyboard focus and pointer hover are both present.
        HoveredFocused => "hovered_focused",
        /// Keyboard focus and a pointer press are both present.
        PressedFocused => "pressed_focused",
        /// Widget or menu item is disabled regardless of pointer position.
        Disabled => "disabled",
    }
}

impl VisualState {
    /// Resolves one exact state from concrete widget interaction facts.
    pub const fn from_interaction(enabled: bool, hovered: bool, focused: bool, pressed: bool) -> Self {
        // Disabled wins before the ordinary press, hover, and focus ladder. Window activation is
        // deliberately represented by distinct appearance roles rather than smuggled into state.
        if !enabled {
            Self::Disabled
        } else if pressed && focused {
            Self::PressedFocused
        } else if pressed {
            Self::Pressed
        } else if hovered && focused {
            Self::HoveredFocused
        } else if hovered {
            Self::Hovered
        } else if focused {
            Self::Focused
        } else {
            Self::Normal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps declaration order, table indices, and authored spellings synchronized.
    #[test]
    fn generated_metadata_is_complete_and_index_ordered() {
        for (index, role) in AppearanceRole::ALL.into_iter().enumerate() {
            assert_eq!(role.index(), index);
            assert!(!role.json_name().is_empty());
        }
        for (index, state) in VisualState::ALL.into_iter().enumerate() {
            assert_eq!(state.index(), index);
            assert!(!state.json_name().is_empty());
        }
    }
}
