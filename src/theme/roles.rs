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
    /// Semantic visual part painted by the built-in user interface.
    ///
    /// Roles describe meaning rather than widget Rust types. Composite controls can consequently
    /// select separate track, thumb, popup, and caption visuals without erased style lookup.
    pub enum AppearanceRole {
        /// Generic frame requested through [`crate::WidgetOption::FRAME`].
        GenericFrame => "generic_frame",
        /// Content panel or scroll-area viewport background.
        Panel => "panel",
        /// Ordinary command button or button-like list box.
        Button => "button",
        /// Unchecked checkbox square.
        Checkbox => "checkbox",
        /// Checked checkbox square.
        CheckboxChecked => "checkbox_checked",
        /// Single-line, multiline, or numeric text input background.
        TextInput => "text_input",
        /// Unselected list row.
        ListItem => "list_item",
        /// Semantically selected list row.
        ListItemSelected => "list_item_selected",
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
        /// Disclosure header or tree row.
        DisclosureHeader => "disclosure_header",
        /// Menu bar spanning a window.
        MenuBar => "menu_bar",
        /// Menu-bar title that does not own an open popup.
        MenuTitle => "menu_title",
        /// Menu-bar title whose popup is open.
        MenuTitleOpen => "menu_title_open",
        /// Popup-menu panel.
        MenuPopup => "menu_popup",
        /// Ordinary menu item row.
        MenuItem => "menu_item",
        /// Checked or radio-selected menu item row.
        MenuItemSelected => "menu_item_selected",
        /// Passive window outer frame and body.
        WindowFrame => "window_frame",
        /// Active window outer frame and body.
        WindowFrameActive => "window_frame_active",
        /// Passive modal-dialog outer frame and body.
        DialogFrame => "dialog_frame",
        /// Active modal-dialog outer frame and body.
        DialogFrameActive => "dialog_frame_active",
        /// Passive window title background.
        WindowTitle => "window_title",
        /// Active window title background.
        WindowTitleActive => "window_title_active",
        /// Window close caption button.
        WindowCloseButton => "window_close_button",
        /// Window minimize caption button.
        WindowMinimizeButton => "window_minimize_button",
        /// Window maximize caption button.
        WindowMaximizeButton => "window_maximize_button",
        /// Window restore caption button used while maximized.
        WindowRestoreButton => "window_restore_button",
        /// Visible bottom-right resize grip.
        WindowResizeGrip => "window_resize_grip",
        /// Optional themed glyph painted inside the window close button.
        WindowCloseGlyph => "window_close_glyph",
        /// Optional themed glyph painted inside the window minimize button.
        WindowMinimizeGlyph => "window_minimize_glyph",
        /// Optional themed glyph painted inside the window maximize button.
        WindowMaximizeGlyph => "window_maximize_glyph",
        /// Optional themed glyph painted inside the window restore button.
        WindowRestoreGlyph => "window_restore_glyph",
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
