//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
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

//! Typed semantic appearances and interaction states shared by flat and image themes.

use std::sync::Arc;

use crate::{Color, ControlColor, NinePatch, SliceInsets};

/// Semantic background or chrome part painted by the built-in UI.
///
/// Roles describe meaning rather than concrete widget Rust types. Composite controls can therefore
/// select separate track, thumb, popup, and caption-button appearances without inventing an erased
/// style lookup protocol.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum AppearanceRole {
    /// Generic frame requested through [`crate::WidgetOption::FRAME`].
    GenericFrame,
    /// Content panel or scroll-area viewport background.
    Panel,
    /// Ordinary command button or button-like list box.
    Button,
    /// Unchecked checkbox square.
    Checkbox,
    /// Checked checkbox square.
    CheckboxChecked,
    /// Single-line, multiline, or numeric text input background.
    TextInput,
    /// Unselected list row.
    ListItem,
    /// Semantically selected list row.
    ListItemSelected,
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
    /// Disclosure header or tree row.
    DisclosureHeader,
    /// Menu bar spanning a window.
    MenuBar,
    /// Menu-bar title that does not own an open popup.
    MenuTitle,
    /// Menu-bar title whose popup is open.
    MenuTitleOpen,
    /// Popup-menu panel.
    MenuPopup,
    /// Ordinary menu item row.
    MenuItem,
    /// Checked or radio-selected menu item row.
    MenuItemSelected,
    /// Inactive window outer frame and body.
    WindowFrame,
    /// Active window outer frame and body.
    WindowFrameActive,
    /// Inactive window title background.
    WindowTitle,
    /// Active window title background.
    WindowTitleActive,
    /// Window close caption button.
    WindowCloseButton,
    /// Window minimize caption button.
    WindowMinimizeButton,
    /// Window maximize caption button.
    WindowMaximizeButton,
    /// Window restore caption button used while maximized.
    WindowRestoreButton,
    /// Visible bottom-right resize grip.
    WindowResizeGrip,
}

impl AppearanceRole {
    /// Number of role slots retained by [`AppearanceCatalog`].
    pub const COUNT: usize = Self::WindowResizeGrip as usize + 1;

    /// Returns the stable snake-case JSON key for this role.
    pub const fn json_name(self) -> &'static str {
        // Keeping the complete conversion exhaustive makes a newly added role fail compilation
        // until its theme-file spelling is deliberately chosen.
        match self {
            Self::GenericFrame => "generic_frame",
            Self::Panel => "panel",
            Self::Button => "button",
            Self::Checkbox => "checkbox",
            Self::CheckboxChecked => "checkbox_checked",
            Self::TextInput => "text_input",
            Self::ListItem => "list_item",
            Self::ListItemSelected => "list_item_selected",
            Self::Combo => "combo",
            Self::SliderTrack => "slider_track",
            Self::SliderThumb => "slider_thumb",
            Self::ScrollbarTrack => "scrollbar_track",
            Self::ScrollbarThumb => "scrollbar_thumb",
            Self::DisclosureHeader => "disclosure_header",
            Self::MenuBar => "menu_bar",
            Self::MenuTitle => "menu_title",
            Self::MenuTitleOpen => "menu_title_open",
            Self::MenuPopup => "menu_popup",
            Self::MenuItem => "menu_item",
            Self::MenuItemSelected => "menu_item_selected",
            Self::WindowFrame => "window_frame",
            Self::WindowFrameActive => "window_frame_active",
            Self::WindowTitle => "window_title",
            Self::WindowTitleActive => "window_title_active",
            Self::WindowCloseButton => "window_close_button",
            Self::WindowMinimizeButton => "window_minimize_button",
            Self::WindowMaximizeButton => "window_maximize_button",
            Self::WindowRestoreButton => "window_restore_button",
            Self::WindowResizeGrip => "window_resize_grip",
        }
    }

    /// Parses one exact snake-case JSON role name.
    #[cfg(feature = "theme-json")]
    pub(crate) fn from_json_name(name: &str) -> Option<Self> {
        // Match exact schema spellings so misspelled theme roles produce a useful load error rather
        // than silently creating an unused entry.
        Self::ALL.into_iter().find(|role| role.json_name() == name)
    }

    /// Complete role list in catalog index order.
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::GenericFrame,
        Self::Panel,
        Self::Button,
        Self::Checkbox,
        Self::CheckboxChecked,
        Self::TextInput,
        Self::ListItem,
        Self::ListItemSelected,
        Self::Combo,
        Self::SliderTrack,
        Self::SliderThumb,
        Self::ScrollbarTrack,
        Self::ScrollbarThumb,
        Self::DisclosureHeader,
        Self::MenuBar,
        Self::MenuTitle,
        Self::MenuTitleOpen,
        Self::MenuPopup,
        Self::MenuItem,
        Self::MenuItemSelected,
        Self::WindowFrame,
        Self::WindowFrameActive,
        Self::WindowTitle,
        Self::WindowTitleActive,
        Self::WindowCloseButton,
        Self::WindowMinimizeButton,
        Self::WindowMaximizeButton,
        Self::WindowRestoreButton,
        Self::WindowResizeGrip,
    ];
}

/// Mutually exclusive interaction state used to select one appearance PNG or flat fallback.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum VisualState {
    /// Enabled widget with no hover, focus, or active press.
    Normal,
    /// Pointer is over an enabled widget.
    Hovered,
    /// Pointer capture is pressed over an enabled widget.
    Pressed,
    /// Keyboard focus is visible without pointer hover.
    Focused,
    /// Keyboard focus and pointer hover are both present.
    HoveredFocused,
    /// Keyboard focus and a pointer press are both present.
    PressedFocused,
    /// Widget or menu item is disabled regardless of pointer position.
    Disabled,
}

impl VisualState {
    /// Number of state slots retained for every semantic role.
    pub const COUNT: usize = Self::Disabled as usize + 1;

    /// Complete state list in catalog index order.
    #[cfg(feature = "theme-json")]
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::Normal,
        Self::Hovered,
        Self::Pressed,
        Self::Focused,
        Self::HoveredFocused,
        Self::PressedFocused,
        Self::Disabled,
    ];

    /// Resolves one exact state from concrete interaction facts.
    pub const fn from_interaction(enabled: bool, hovered: bool, focused: bool, pressed: bool) -> Self {
        // Disabled wins first, then a visible press, then hover and focus combinations. Keeping the
        // precedence here prevents widgets from implementing subtly different state ladders.
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

/// Complete state table for one semantic appearance role.
#[derive(Copy, Clone)]
pub struct StatefulAppearance {
    /// Fixed state-indexed patch table.
    patches: [NinePatch; VisualState::COUNT],
}

impl StatefulAppearance {
    /// Creates a state table that initially uses one patch for every interaction state.
    pub const fn all(patch: NinePatch) -> Self {
        // A repeated flat default makes every state total before JSON replaces selected entries.
        Self { patches: [patch; VisualState::COUNT] }
    }

    /// Creates an explicit state table in enum order.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        normal: NinePatch,
        hovered: NinePatch,
        pressed: NinePatch,
        focused: NinePatch,
        hovered_focused: NinePatch,
        pressed_focused: NinePatch,
        disabled: NinePatch,
    ) -> Self {
        // The named parameters make programmatic construction readable while the retained array
        // keeps lookup branch-free and allocation-free.
        Self {
            patches: [normal, hovered, pressed, focused, hovered_focused, pressed_focused, disabled],
        }
    }

    /// Returns the exact patch assigned to `state`.
    pub const fn get(self, state: VisualState) -> NinePatch {
        // `VisualState` uses a contiguous private representation whose final variant defines COUNT.
        self.patches[state as usize]
    }

    /// Replaces one exact state patch.
    pub fn set(&mut self, state: VisualState, patch: NinePatch) {
        // Mutation remains typed by the enum; callers cannot address an invalid numeric slot.
        self.patches[state as usize] = patch;
    }

    /// Returns every state patch in stable enum order for resource validation.
    pub(crate) fn patches(self) -> [NinePatch; VisualState::COUNT] {
        // Copy the small fixed table so callers do not receive mutable access to catalog storage.
        self.patches
    }
}

/// Cheaply cloneable typed catalog containing every built-in semantic appearance.
#[derive(Clone)]
pub struct AppearanceCatalog {
    /// Copy-on-write role table shared by cloned styles and local style overrides.
    entries: Arc<[StatefulAppearance; AppearanceRole::COUNT]>,
}

impl AppearanceCatalog {
    /// Creates a complete catalog using one state table for every role.
    pub fn new(default: StatefulAppearance) -> Self {
        // The fixed array guarantees total role lookup without a hash map, string key, or erased
        // payload. Arc keeps ordinary Style cloning cheap while preserving value semantics.
        Self {
            entries: Arc::new([default; AppearanceRole::COUNT]),
        }
    }

    /// Returns the exact state table for one semantic role.
    pub fn get(&self, role: AppearanceRole) -> StatefulAppearance {
        // Role discriminants are contiguous and private catalog construction always has COUNT slots.
        self.entries[role as usize]
    }

    /// Replaces one role's complete state table using copy-on-write storage.
    pub fn set(&mut self, role: AppearanceRole, appearance: StatefulAppearance) {
        // Clone the fixed catalog only when a shared Style is actually customized.
        Arc::make_mut(&mut self.entries)[role as usize] = appearance;
    }

    /// Returns one exact role and interaction patch.
    pub fn resolve(&self, role: AppearanceRole, state: VisualState) -> NinePatch {
        // Both indices are typed and total, so resolution cannot fail at paint time.
        self.get(role).get(state)
    }

    /// Returns every retained patch in role-major, state-minor order.
    pub(crate) fn patches(&self) -> impl Iterator<Item = NinePatch> + '_ {
        // Copy each fixed state table before flattening so the iterator never exposes catalog
        // internals or requires dynamic dispatch.
        self.entries.iter().copied().flat_map(StatefulAppearance::patches)
    }

    /// Returns every role's normalized destination insets for retained measurement keys.
    pub(crate) fn measurement_insets(&self) -> [[i32; 4]; AppearanceRole::COUNT] {
        // A container measurement can observe any descendant role, so cache identity must include
        // all structural patch geometry rather than only the generic frame. Flat colors and image
        // payloads remain paint-only and deliberately do not invalidate preferred dimensions.
        std::array::from_fn(|index| {
            let role = AppearanceRole::ALL[index];
            let insets = self.resolve(role, VisualState::Normal).insets.normalized();
            [insets.left, insets.top, insets.right, insets.bottom]
        })
    }

    /// Builds the default catalog whose flat patches preserve the original Style presentation.
    pub(crate) fn from_flat_palette(frame_insets: SliceInsets, colors: [Color; 12], focus: Color, window_focus: Color, menu_background: Color) -> Self {
        // Resolve named palette entries once, then assemble role tables from concrete NinePatch
        // values. This is also the fallback rebuilt after a JSON theme changes flat colors.
        let border = colors[ControlColor::Border as usize];
        let transparent = Color { r: 0, g: 0, b: 0, a: 0 };
        let framed = |fill| NinePatch::framed(frame_insets, border, Some(fill));
        let hollow = NinePatch::framed(frame_insets, border, None);
        let solid = NinePatch::solid;
        let states = |normal, hovered, pressed, focused, disabled| StatefulAppearance::new(normal, hovered, pressed, focused, focused, pressed, disabled);
        let button = states(
            framed(colors[ControlColor::Button as usize]),
            framed(colors[ControlColor::ButtonHover as usize]),
            framed(colors[ControlColor::Base as usize]),
            framed(focus),
            framed(colors[ControlColor::Button as usize]),
        );
        let input = states(
            framed(colors[ControlColor::Base as usize]),
            framed(colors[ControlColor::BaseHover as usize]),
            framed(colors[ControlColor::BaseHover as usize]),
            framed(focus),
            framed(colors[ControlColor::Base as usize]),
        );
        let highlight = states(
            solid(transparent),
            solid(colors[ControlColor::ButtonHover as usize]),
            solid(colors[ControlColor::Button as usize]),
            solid(focus),
            solid(transparent),
        );
        let selected = states(
            solid(focus),
            solid(colors[ControlColor::ButtonHover as usize]),
            solid(colors[ControlColor::Button as usize]),
            solid(focus),
            solid(focus),
        );
        let window = states(
            framed(colors[ControlColor::WindowBG as usize]),
            framed(colors[ControlColor::WindowBG as usize]),
            framed(colors[ControlColor::WindowBG as usize]),
            framed(colors[ControlColor::WindowBG as usize]),
            framed(colors[ControlColor::WindowBG as usize]),
        );

        let mut catalog = Self::new(StatefulAppearance::all(NinePatch::solid(transparent)));
        catalog.set(AppearanceRole::GenericFrame, StatefulAppearance::all(hollow));
        catalog.set(AppearanceRole::Panel, StatefulAppearance::all(framed(colors[ControlColor::PanelBG as usize])));
        catalog.set(AppearanceRole::Button, button);
        catalog.set(AppearanceRole::Checkbox, input);
        catalog.set(AppearanceRole::CheckboxChecked, input);
        catalog.set(AppearanceRole::TextInput, input);
        catalog.set(AppearanceRole::ListItem, highlight);
        catalog.set(AppearanceRole::ListItemSelected, selected);
        catalog.set(AppearanceRole::Combo, button);
        catalog.set(AppearanceRole::SliderTrack, input);
        catalog.set(AppearanceRole::SliderThumb, button);
        catalog.set(
            AppearanceRole::ScrollbarTrack,
            StatefulAppearance::all(solid(colors[ControlColor::ScrollBase as usize])),
        );
        catalog.set(
            AppearanceRole::ScrollbarThumb,
            StatefulAppearance::all(solid(colors[ControlColor::ScrollThumb as usize])),
        );
        catalog.set(AppearanceRole::DisclosureHeader, highlight);
        catalog.set(AppearanceRole::MenuBar, StatefulAppearance::all(solid(menu_background)));
        catalog.set(AppearanceRole::MenuTitle, highlight);
        catalog.set(AppearanceRole::MenuTitleOpen, selected);
        catalog.set(AppearanceRole::MenuPopup, StatefulAppearance::all(solid(menu_background)));
        catalog.set(AppearanceRole::MenuItem, highlight);
        catalog.set(AppearanceRole::MenuItemSelected, selected);
        catalog.set(AppearanceRole::WindowFrame, window);
        catalog.set(
            AppearanceRole::WindowFrameActive,
            StatefulAppearance::all(NinePatch::framed(
                frame_insets.at_least(1),
                window_focus,
                Some(colors[ControlColor::WindowBG as usize]),
            )),
        );
        catalog.set(
            AppearanceRole::WindowTitle,
            StatefulAppearance::all(solid(colors[ControlColor::TitleBG as usize])),
        );
        catalog.set(AppearanceRole::WindowTitleActive, StatefulAppearance::all(solid(window_focus)));
        catalog.set(AppearanceRole::WindowCloseButton, button);
        catalog.set(AppearanceRole::WindowMinimizeButton, button);
        catalog.set(AppearanceRole::WindowMaximizeButton, button);
        catalog.set(AppearanceRole::WindowRestoreButton, button);
        catalog.set(AppearanceRole::WindowResizeGrip, input);
        catalog
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;

    /// Verifies the shared interaction resolver keeps every combined state independently addressable.
    #[test]
    fn visual_state_resolution_preserves_disabled_and_combined_states() {
        assert_eq!(VisualState::from_interaction(false, true, true, true), VisualState::Disabled);
        assert_eq!(VisualState::from_interaction(true, true, true, true), VisualState::PressedFocused);
        assert_eq!(VisualState::from_interaction(true, true, true, false), VisualState::HoveredFocused);
        assert_eq!(VisualState::from_interaction(true, false, true, false), VisualState::Focused);
        assert_eq!(VisualState::from_interaction(true, true, false, false), VisualState::Hovered);
        assert_eq!(VisualState::from_interaction(true, false, false, false), VisualState::Normal);
    }

    /// Verifies a cloned catalog shares storage until one exact typed role is replaced.
    #[test]
    fn catalog_mutation_has_copy_on_write_value_semantics() {
        let normal = NinePatch::solid(color(1, 2, 3, 255));
        let replacement = NinePatch::solid(color(4, 5, 6, 255));
        let mut original = AppearanceCatalog::new(StatefulAppearance::all(normal));
        let cloned = original.clone();

        original.set(AppearanceRole::Button, StatefulAppearance::all(replacement));

        assert!(matches!(
            original.resolve(AppearanceRole::Button, VisualState::Normal).content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if color.r == 4)
        ));
        assert!(matches!(
            cloned.resolve(AppearanceRole::Button, VisualState::Normal).content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if color.r == 1)
        ));
    }
}
