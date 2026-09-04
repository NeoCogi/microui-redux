//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Semantic appearance roles and the state domains that are meaningful for each family.

/// Defines a fieldless semantic role whose declaration order indexes one family catalog.
///
/// The macro is private because only the four appearance families are storage domains. Keeping
/// count, traversal, and indexing beside each declaration prevents hand-maintained offsets or a
/// flattened cross-family role list from reappearing.
macro_rules! indexed_role {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        #[cfg_attr(feature = "theme-json", derive(serde::Deserialize))]
        #[cfg_attr(feature = "theme-json", serde(rename_all = "snake_case"))]
        #[repr(u8)]
        pub enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        impl $name {
            /// Number of roles in this closed semantic family.
            pub const COUNT: usize = [$(Self::$variant),+].len();

            /// Every role in declaration and family-catalog order.
            pub const ALL: [Self; Self::COUNT] = [$(Self::$variant),+];

            /// Converts this closed role into its family-local array index.
            pub(crate) const fn index(self) -> usize {
                // The macro emits contiguous variants without explicit discriminants, so the cast
                // cannot escape the array whose length is generated from the same declaration.
                self as usize
            }
        }
    };
}

indexed_role! {
    /// Structural surfaces whose presentation depends only on availability.
    pub enum SurfaceRole {
        /// Window or modal-dialog background below application content and chrome.
        Window,
        /// Generic frame requested through [`crate::WidgetOption::FRAME`].
        GenericFrame,
        /// Content panel or scroll-area viewport background.
        Panel,
    }
}

indexed_role! {
    /// Interactive control parts sharing pointer and keyboard-focus behavior.
    pub enum ControlRole {
        /// Ordinary command button or button-like list box.
        Button,
        /// Checkbox square; checked state is represented by an independently painted glyph.
        Checkbox,
        /// Single-line, multiline, or numeric text input background.
        TextInput,
        /// Generic item or tree row whose state supplies transient selection.
        Item,
        /// Combo-box header.
        Combo,
        /// Slider background track.
        SliderTrack,
        /// Slider value thumb.
        SliderThumb,
        /// Scrollbar background track.
        ScrollbarTrack,
        /// Scrollbar movable thumb.
        ScrollbarThumb,
        /// Window close caption button.
        CloseButton,
        /// Window minimize caption button.
        MinimizeButton,
        /// Window maximize caption button.
        MaximizeButton,
        /// Window restore caption button used while maximized.
        RestoreButton,
        /// Visible bottom-right resize grip.
        ResizeGrip,
    }
}

indexed_role! {
    /// Menu-owned surfaces and entries with menu-specific open-selection behavior.
    pub enum MenuRole {
        /// Menu bar spanning a window.
        Bar,
        /// Menu-bar title; [`MenuState::Open`] represents ownership of an open popup.
        Title,
        /// Popup-menu panel.
        Popup,
        /// Ordinary menu item or submenu row.
        Item,
    }
}

indexed_role! {
    /// Manager-owned window chrome whose presentation follows window activation.
    pub enum ChromeRole {
        /// Ordinary-window outer frame decoration.
        WindowFrame,
        /// Modal-dialog outer frame decoration.
        DialogFrame,
        /// Window title patch and semantic-content color.
        Title,
    }
}

/// Appearance family used by the retained node frame contract.
///
/// This sum is deliberately limited to the two families a widget frame can use. It is not a skin
/// catalog key: lookup remains family-specific, and menu or chrome roles cannot enter generic node
/// layout by accident.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FrameRole {
    /// Structural frame whose pointer and focus state is irrelevant.
    Surface(SurfaceRole),
    /// Interactive control frame resolved from the owning widget's state.
    Control(ControlRole),
}

/// Pointer interaction nested inside an enabled control state.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PointerState {
    /// Pointer is not hovering or pressing the control.
    Normal,
    /// Pointer is hovering the control without an active press.
    Hovered,
    /// Pointer capture is pressed while the pointer remains over the control.
    Pressed,
}

impl PointerState {
    /// Number of pointer states in each enabled control branch.
    pub const COUNT: usize = 3;

    /// Every pointer state in family-catalog order.
    pub const ALL: [Self; Self::COUNT] = [Self::Normal, Self::Hovered, Self::Pressed];

    /// Converts this closed pointer state into its branch-local array index.
    const fn index(self) -> usize {
        // The explicit match remains valid independently of enum representation choices.
        match self {
            Self::Normal => 0,
            Self::Hovered => 1,
            Self::Pressed => 2,
        }
    }
}

/// State of an interactive control, excluding impossible disabled interaction combinations.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ControlState {
    /// Control is unavailable; pointer and keyboard-focus presentation no longer apply.
    Disabled,
    /// Control is available without keyboard focus and has the enclosed pointer state.
    Enabled(PointerState),
    /// Control owns visible keyboard focus and has the enclosed pointer state.
    Focused(PointerState),
}

impl ControlState {
    /// Number of concrete control states stored for each control role.
    pub const COUNT: usize = 1 + PointerState::COUNT * 2;

    /// Every concrete control state in family-catalog order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Disabled,
        Self::Enabled(PointerState::Normal),
        Self::Enabled(PointerState::Hovered),
        Self::Enabled(PointerState::Pressed),
        Self::Focused(PointerState::Normal),
        Self::Focused(PointerState::Hovered),
        Self::Focused(PointerState::Pressed),
    ];

    /// Resolves one exact control state from retained interaction facts.
    pub const fn from_interaction(enabled: bool, hovered: bool, focused: bool, pressed: bool) -> Self {
        if !enabled {
            // Disabled is terminal: a disabled control cannot expose misleading hover or press art.
            return Self::Disabled;
        }
        let pointer = if pressed {
            PointerState::Pressed
        } else if hovered {
            PointerState::Hovered
        } else {
            PointerState::Normal
        };
        if focused { Self::Focused(pointer) } else { Self::Enabled(pointer) }
    }

    /// Converts this closed state into its control-catalog array index.
    pub(crate) const fn index(self) -> usize {
        // Disabled occupies one slot; the two enabled branches then occupy equal contiguous spans.
        match self {
            Self::Disabled => 0,
            Self::Enabled(pointer) => 1 + pointer.index(),
            Self::Focused(pointer) => 1 + PointerState::COUNT + pointer.index(),
        }
    }
}

/// Availability state of a noninteractive structural surface.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SurfaceState {
    /// Surface belongs to an enabled retained subtree.
    Normal,
    /// Surface belongs to a disabled retained subtree.
    Disabled,
}

impl SurfaceState {
    /// Number of concrete surface states stored for each surface role.
    pub const COUNT: usize = 2;

    /// Every surface state in family-catalog order.
    pub const ALL: [Self; Self::COUNT] = [Self::Normal, Self::Disabled];

    /// Resolves surface availability without accepting pointer or focus facts.
    pub const fn from_enabled(enabled: bool) -> Self {
        // The binary domain keeps structural surfaces independent from widget interaction.
        if enabled { Self::Normal } else { Self::Disabled }
    }

    /// Converts this closed state into its surface-catalog array index.
    pub(crate) const fn index(self) -> usize {
        // The match documents storage order rather than relying on a representation cast.
        match self {
            Self::Normal => 0,
            Self::Disabled => 1,
        }
    }
}

/// State of a menu surface or entry after menu-specific precedence is resolved.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum MenuState {
    /// Enabled entry or surface without transient selection.
    Normal,
    /// Pointer is over an enabled entry.
    Hovered,
    /// Pointer capture is pressed over an enabled entry.
    Pressed,
    /// Keyboard navigation selected an enabled entry.
    Focused,
    /// Entry owns the popup currently open beneath it.
    Open,
    /// Entry or surface is unavailable.
    Disabled,
}

impl MenuState {
    /// Number of concrete menu states stored for each menu role.
    pub const COUNT: usize = 6;

    /// Every menu state in family-catalog order.
    pub const ALL: [Self; Self::COUNT] = [Self::Normal, Self::Hovered, Self::Pressed, Self::Focused, Self::Open, Self::Disabled];

    /// Resolves menu selection with one explicit precedence order.
    pub const fn from_interaction(enabled: bool, open: bool, hovered: bool, focused: bool, pressed: bool) -> Self {
        // Open selection remains stable while moving through the owned popup; ordinary pointer and
        // keyboard selection matter only when this entry does not own that popup.
        if !enabled {
            Self::Disabled
        } else if open {
            Self::Open
        } else if pressed {
            Self::Pressed
        } else if hovered {
            Self::Hovered
        } else if focused {
            Self::Focused
        } else {
            Self::Normal
        }
    }

    /// Converts this closed state into its menu-catalog array index.
    pub(crate) const fn index(self) -> usize {
        // The explicit mapping keeps serialized field order irrelevant to runtime storage.
        match self {
            Self::Normal => 0,
            Self::Hovered => 1,
            Self::Pressed => 2,
            Self::Focused => 3,
            Self::Open => 4,
            Self::Disabled => 5,
        }
    }
}

/// State of manager-owned chrome after activation and availability are resolved.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ChromeState {
    /// Enabled chrome for a window that does not own activation.
    Base,
    /// Enabled chrome for the active window.
    Active,
    /// Chrome belonging to a disabled window.
    Disabled,
}

impl ChromeState {
    /// Number of concrete chrome states stored for each chrome role.
    pub const COUNT: usize = 3;

    /// Every chrome state in family-catalog order.
    pub const ALL: [Self; Self::COUNT] = [Self::Base, Self::Active, Self::Disabled];

    /// Resolves chrome state from availability and window activation.
    pub const fn from_window(enabled: bool, active: bool) -> Self {
        // A disabled window never presents active chrome even if it remains manager-selected.
        if !enabled {
            Self::Disabled
        } else if active {
            Self::Active
        } else {
            Self::Base
        }
    }

    /// Converts this closed state into its chrome-catalog array index.
    pub(crate) const fn index(self) -> usize {
        // The explicit mapping avoids giving the word "inactive" two unrelated UI meanings.
        match self {
            Self::Base => 0,
            Self::Active => 1,
            Self::Disabled => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps every closed state domain synchronized with its family-local storage index.
    #[test]
    fn state_metadata_is_complete_and_index_ordered() {
        for (index, state) in ControlState::ALL.into_iter().enumerate() {
            assert_eq!(state.index(), index);
        }
        for (index, state) in SurfaceState::ALL.into_iter().enumerate() {
            assert_eq!(state.index(), index);
        }
        for (index, state) in MenuState::ALL.into_iter().enumerate() {
            assert_eq!(state.index(), index);
        }
        for (index, state) in ChromeState::ALL.into_iter().enumerate() {
            assert_eq!(state.index(), index);
        }
    }

    /// Verifies disabled, focus, and pointer facts resolve without representable contradictions.
    #[test]
    fn control_state_nests_pointer_interaction_only_inside_enabled_branches() {
        assert_eq!(ControlState::from_interaction(false, true, true, true), ControlState::Disabled);
        assert_eq!(
            ControlState::from_interaction(true, true, false, false),
            ControlState::Enabled(PointerState::Hovered)
        );
        assert_eq!(
            ControlState::from_interaction(true, true, true, true),
            ControlState::Focused(PointerState::Pressed)
        );
    }
}
