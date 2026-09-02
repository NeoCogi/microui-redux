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

use crate::{Color, ControlColor, NinePatch, SliceInsets};

use super::{AppearanceRole, RoleTable, StateTable, VisualState};

/// Complete state table for one semantic appearance role.
#[derive(Copy, Clone)]
pub struct StatefulAppearance {
    /// Fixed state-indexed patch table.
    patches: StateTable<NinePatch>,
}

/// Complete foreground-color table for one semantic appearance role.
///
/// Text, semantic icons, check marks, and menu arrows use the same role and interaction state as
/// their adjacent nine-patch. Keeping the foreground table concrete and fixed-size prevents a
/// selected menu row, disabled input, or disabled caption from falling back to unrelated global
/// color exceptions.
#[derive(Copy, Clone)]
pub struct StatefulColor {
    /// Fixed state-indexed color table stored in [`VisualState`] discriminant order.
    colors: StateTable<Color>,
}

impl StatefulColor {
    /// Creates a complete state table that initially uses `color` for every interaction state.
    pub const fn all(color: Color) -> Self {
        // Repetition makes every programmatically constructed role total before selected states
        // are customized by application code or a theme document.
        Self {
            colors: StateTable::new([color; VisualState::COUNT]),
        }
    }

    /// Returns the exact foreground assigned to `state`.
    pub const fn get(self, state: VisualState) -> Color {
        // VisualState is contiguous and its final variant defines COUNT, so every enum value is a
        // valid fixed-array index without a string lookup or fallible branch.
        *self.colors.get(state)
    }

    /// Replaces one exact state foreground.
    pub fn set(&mut self, state: VisualState, color: Color) {
        // The typed enum keeps invalid numeric state slots outside the public API.
        self.colors.set(state, color);
    }
}

/// Cheaply cloneable typed catalog containing foreground colors for every appearance role.
#[derive(Clone)]
pub struct ForegroundCatalog {
    /// Copy-on-write role table shared by cloned styles and detached theme variants.
    entries: RoleTable<StatefulColor>,
}

impl ForegroundCatalog {
    /// Creates a complete catalog using one state table for every semantic role.
    pub fn new(default: StatefulColor) -> Self {
        // A fixed array guarantees total role lookup, while Arc preserves cheap Style cloning and
        // copy-on-write value semantics for live style editors.
        Self { entries: RoleTable::filled(default) }
    }

    /// Returns the complete foreground table for `role`.
    pub fn get(&self, role: AppearanceRole) -> StatefulColor {
        // AppearanceRole discriminants are contiguous and catalog construction always allocates
        // exactly COUNT entries.
        *self.entries.get(role)
    }

    /// Replaces one role's complete foreground table.
    pub fn set(&mut self, role: AppearanceRole, colors: StatefulColor) {
        // Clone shared storage only when a caller actually customizes a style.
        self.entries.set(role, colors);
    }

    /// Replaces one exact role/state foreground without exposing catalog storage.
    pub fn set_state(&mut self, role: AppearanceRole, state: VisualState, color: Color) {
        // Read-modify-write remains a single typed operation and preserves every sibling state.
        let mut colors = self.get(role);
        colors.set(state, color);
        self.set(role, colors);
    }

    /// Resolves one exact role and interaction-state foreground.
    pub fn resolve(&self, role: AppearanceRole, state: VisualState) -> Color {
        // Both indices are exhaustive enums, so paint-time foreground resolution cannot fail.
        self.get(role).get(state)
    }

    /// Builds the flat fallback catalog used before optional JSON state overrides are applied.
    pub(crate) fn from_flat_palette(text: Color, title_text: Color, menu_text: Color, disabled_text: Color, disabled_title_text: Color) -> Self {
        // Explicit disabled colors keep ordinary controls, menus, and chrome consistent without
        // inferring disabled presentation from window activation or alpha arithmetic.
        let mut body = StatefulColor::all(text);
        body.set(VisualState::Disabled, disabled_text);
        let mut menu = StatefulColor::all(menu_text);
        menu.set(VisualState::Disabled, disabled_text);
        let mut title = StatefulColor::all(title_text);
        title.set(VisualState::Disabled, disabled_title_text);

        let mut catalog = Self::new(body);
        // Menu roles share one fallback family but remain independent catalog entries so a theme
        // can change a selected row without recoloring the bar, popup, or separator.
        for role in [
            AppearanceRole::MenuBar,
            AppearanceRole::MenuTitle,
            AppearanceRole::MenuTitleOpen,
            AppearanceRole::MenuPopup,
            AppearanceRole::MenuItem,
            AppearanceRole::MenuItemSelected,
        ] {
            catalog.set(role, menu);
        }
        // Title strips and caption buttons use chrome contrast rather than client-area text.
        for role in [
            AppearanceRole::WindowTitle,
            AppearanceRole::WindowTitleActive,
            AppearanceRole::WindowCloseButton,
            AppearanceRole::WindowMinimizeButton,
            AppearanceRole::WindowMaximizeButton,
            AppearanceRole::WindowRestoreButton,
            AppearanceRole::WindowCloseGlyph,
            AppearanceRole::WindowMinimizeGlyph,
            AppearanceRole::WindowMaximizeGlyph,
            AppearanceRole::WindowRestoreGlyph,
        ] {
            catalog.set(role, title);
        }
        catalog
    }
}

impl StatefulAppearance {
    /// Creates a state table that initially uses one patch for every interaction state.
    pub const fn all(patch: NinePatch) -> Self {
        // A repeated flat default makes every state total before JSON replaces selected entries.
        Self {
            patches: StateTable::new([patch; VisualState::COUNT]),
        }
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
            patches: StateTable::new([normal, hovered, pressed, focused, hovered_focused, pressed_focused, disabled]),
        }
    }

    /// Returns the exact patch assigned to `state`.
    pub const fn get(self, state: VisualState) -> NinePatch {
        // `VisualState` uses a contiguous private representation whose final variant defines COUNT.
        *self.patches.get(state)
    }

    /// Replaces one exact state patch.
    pub fn set(&mut self, state: VisualState, patch: NinePatch) {
        // Mutation remains typed by the enum; callers cannot address an invalid numeric slot.
        self.patches.set(state, patch);
    }

    /// Returns every state patch in stable enum order for resource validation.
    pub(crate) fn patches(self) -> [NinePatch; VisualState::COUNT] {
        // Copy the small fixed table so callers do not receive mutable access to catalog storage.
        self.patches.into_array()
    }
}

/// Cheaply cloneable typed catalog containing every built-in semantic appearance.
#[derive(Clone)]
pub struct AppearanceCatalog {
    /// Copy-on-write role table shared by cloned styles and local style overrides.
    entries: RoleTable<StatefulAppearance>,
}

impl AppearanceCatalog {
    /// Creates a complete catalog using one state table for every role.
    pub fn new(default: StatefulAppearance) -> Self {
        // The fixed array guarantees total role lookup without a hash map, string key, or erased
        // payload. Arc keeps ordinary Style cloning cheap while preserving value semantics.
        Self { entries: RoleTable::filled(default) }
    }

    /// Returns the exact state table for one semantic role.
    pub fn get(&self, role: AppearanceRole) -> StatefulAppearance {
        // Role discriminants are contiguous and private catalog construction always has COUNT slots.
        *self.entries.get(role)
    }

    /// Replaces one role's complete state table using copy-on-write storage.
    pub fn set(&mut self, role: AppearanceRole, appearance: StatefulAppearance) {
        // Clone the fixed catalog only when a shared Style is actually customized.
        self.entries.set(role, appearance);
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
    pub(crate) fn from_flat_palette(
        frame_insets: SliceInsets,
        colors: [Color; 12],
        focus: Color,
        window_focus: Color,
        menu_background: Color,
        disabled_background: Color,
    ) -> Self {
        // Resolve named palette entries once, then assemble role tables from concrete NinePatch
        // values. This is also the fallback rebuilt after a JSON theme changes flat colors.
        let border = colors[ControlColor::Border as usize];
        let transparent = Color { r: 0, g: 0, b: 0, a: 0 };
        let framed = |fill| NinePatch::framed(frame_insets, border, Some(fill));
        let hollow = NinePatch::framed(frame_insets, border, None);
        let solid = NinePatch::solid;
        let states = |normal, hovered, pressed, focused, disabled| {
            // Combined focus/pointer states reuse the authored focus and press patches, while the
            // final slot remains an explicit disabled fallback independent from activation.
            StatefulAppearance::new(normal, hovered, pressed, focused, focused, pressed, disabled)
        };
        let with_disabled = |mut appearance: StatefulAppearance, disabled| {
            // Passive roles normally use one patch for every interaction state. Replacing their
            // disabled slot gives explicit window or ancestor disabling the same total catalog.
            appearance.set(VisualState::Disabled, disabled);
            appearance
        };
        let button = states(
            framed(colors[ControlColor::Button as usize]),
            framed(colors[ControlColor::ButtonHover as usize]),
            framed(colors[ControlColor::Base as usize]),
            framed(focus),
            framed(disabled_background),
        );
        let input = states(
            framed(colors[ControlColor::Base as usize]),
            framed(colors[ControlColor::BaseHover as usize]),
            framed(colors[ControlColor::BaseHover as usize]),
            framed(focus),
            framed(disabled_background),
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
            solid(disabled_background),
        );
        let window = states(
            framed(colors[ControlColor::WindowBG as usize]),
            framed(colors[ControlColor::WindowBG as usize]),
            framed(colors[ControlColor::WindowBG as usize]),
            framed(colors[ControlColor::WindowBG as usize]),
            framed(disabled_background),
        );

        let mut catalog = Self::new(StatefulAppearance::all(NinePatch::solid(transparent)));
        catalog.set(AppearanceRole::GenericFrame, StatefulAppearance::all(hollow));
        catalog.set(
            AppearanceRole::Panel,
            with_disabled(
                StatefulAppearance::all(framed(colors[ControlColor::PanelBG as usize])),
                framed(disabled_background),
            ),
        );
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
            with_disabled(
                StatefulAppearance::all(solid(colors[ControlColor::ScrollBase as usize])),
                solid(disabled_background),
            ),
        );
        catalog.set(
            AppearanceRole::ScrollbarThumb,
            with_disabled(
                StatefulAppearance::all(solid(colors[ControlColor::ScrollThumb as usize])),
                solid(disabled_background),
            ),
        );
        catalog.set(AppearanceRole::DisclosureHeader, highlight);
        catalog.set(
            AppearanceRole::MenuBar,
            with_disabled(StatefulAppearance::all(solid(menu_background)), solid(disabled_background)),
        );
        catalog.set(AppearanceRole::MenuTitle, highlight);
        catalog.set(AppearanceRole::MenuTitleOpen, selected);
        catalog.set(
            AppearanceRole::MenuPopup,
            // Popup insets are structural shell space, not transparent padding. Give every flat
            // theme an explicit border and independently filled center so JSON can enlarge the
            // frame without exposing whatever window happens to lie beneath the menu edges.
            with_disabled(StatefulAppearance::all(framed(menu_background)), framed(disabled_background)),
        );
        catalog.set(AppearanceRole::MenuItem, highlight);
        // A checked or radio-selected menu item differs from an ordinary item by its marker, not
        // by permanent keyboard-selection paint. Reuse the ordinary interaction ladder so its
        // normal state exposes the popup background and only hover/focus highlights the row.
        catalog.set(AppearanceRole::MenuItemSelected, highlight);
        let active_window = StatefulAppearance::all(NinePatch::framed(
            frame_insets.at_least(1),
            window_focus,
            Some(colors[ControlColor::WindowBG as usize]),
        ));
        // Windows and modal dialogs share sensible flat fallbacks but retain independent typed
        // roles. A theme can consequently give dialogs a solid focus frame without changing the
        // ordinary window edge or requiring paint-time knowledge of theme-specific conventions.
        catalog.set(AppearanceRole::WindowFrame, window);
        catalog.set(AppearanceRole::WindowFrameActive, active_window);
        catalog.set(AppearanceRole::DialogFrame, window);
        catalog.set(AppearanceRole::DialogFrameActive, active_window);
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
