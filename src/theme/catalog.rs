//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Family-specific storage for resolved runtime appearances.

use std::sync::Arc;

use super::{ChromeRole, ChromeState, ControlRole, ControlState, MenuRole, MenuState, SurfaceRole, SurfaceState, Visual};

/// Complete resolved appearance values grouped by their meaningful role and state domains.
///
/// Cloned skins share this immutable block. The first edit detaches it with ordinary value
/// semantics, while reads remain direct enum-indexed array lookups without maps or type erasure.
#[derive(Clone)]
pub(crate) struct AppearanceCatalog {
    /// Shared family arrays detached only when a cloned skin is customized.
    values: Arc<AppearanceValues>,
}

/// Concrete arrays underlying one appearance catalog.
#[derive(Clone)]
struct AppearanceValues {
    /// Structural surface values indexed by [`SurfaceRole`] and [`SurfaceState`].
    surfaces: [[Visual; SurfaceState::COUNT]; SurfaceRole::COUNT],
    /// Interactive control values indexed by [`ControlRole`] and [`ControlState`].
    controls: [[Visual; ControlState::COUNT]; ControlRole::COUNT],
    /// Menu values indexed by [`MenuRole`] and [`MenuState`].
    menus: [[Visual; MenuState::COUNT]; MenuRole::COUNT],
    /// Window chrome values indexed by [`ChromeRole`] and [`ChromeState`].
    chrome: [[Visual; ChromeState::COUNT]; ChromeRole::COUNT],
}

impl AppearanceCatalog {
    /// Constructs every family array from exhaustive typed builders.
    pub(crate) fn from_fns(
        mut surface: impl FnMut(SurfaceRole, SurfaceState) -> Visual,
        mut control: impl FnMut(ControlRole, ControlState) -> Visual,
        mut menu: impl FnMut(MenuRole, MenuState) -> Visual,
        mut chrome: impl FnMut(ChromeRole, ChromeState) -> Visual,
    ) -> Self {
        // Each nested array is derived from the same ALL/index order used by lookup. Adding a role
        // or state therefore extends its own family only and makes its exhaustive builder fail.
        let surfaces = std::array::from_fn(|role_index| {
            let role = SurfaceRole::ALL[role_index];
            std::array::from_fn(|state_index| surface(role, SurfaceState::ALL[state_index]))
        });
        let controls = std::array::from_fn(|role_index| {
            let role = ControlRole::ALL[role_index];
            std::array::from_fn(|state_index| control(role, ControlState::ALL[state_index]))
        });
        let menus = std::array::from_fn(|role_index| {
            let role = MenuRole::ALL[role_index];
            std::array::from_fn(|state_index| menu(role, MenuState::ALL[state_index]))
        });
        let chrome = std::array::from_fn(|role_index| {
            let role = ChromeRole::ALL[role_index];
            std::array::from_fn(|state_index| chrome(role, ChromeState::ALL[state_index]))
        });
        Self {
            values: Arc::new(AppearanceValues { surfaces, controls, menus, chrome }),
        }
    }

    /// Returns one structural surface visual.
    pub(crate) fn surface(&self, role: SurfaceRole, state: SurfaceState) -> Visual {
        // Both indices belong to the surface family, so unrelated states cannot enter this lookup.
        self.values.surfaces[role.index()][state.index()]
    }

    /// Replaces one structural surface visual.
    pub(crate) fn set_surface(&mut self, role: SurfaceRole, state: SurfaceState, visual: Visual) {
        // Detach shared storage before changing exactly one typed family slot.
        Arc::make_mut(&mut self.values).surfaces[role.index()][state.index()] = visual;
    }

    /// Returns one interactive control visual.
    pub(crate) fn control(&self, role: ControlRole, state: ControlState) -> Visual {
        // Control lookup accepts the nested enabled/focused pointer state and no other domain.
        self.values.controls[role.index()][state.index()]
    }

    /// Replaces one interactive control visual.
    pub(crate) fn set_control(&mut self, role: ControlRole, state: ControlState, visual: Visual) {
        // Detach shared storage before changing exactly one typed family slot.
        Arc::make_mut(&mut self.values).controls[role.index()][state.index()] = visual;
    }

    /// Returns one menu visual.
    pub(crate) fn menu(&self, role: MenuRole, state: MenuState) -> Visual {
        // Menu lookup is the only API that can receive the Open state.
        self.values.menus[role.index()][state.index()]
    }

    /// Replaces one menu visual.
    pub(crate) fn set_menu(&mut self, role: MenuRole, state: MenuState, visual: Visual) {
        // Detach shared storage before changing exactly one typed family slot.
        Arc::make_mut(&mut self.values).menus[role.index()][state.index()] = visual;
    }

    /// Returns one window chrome visual.
    pub(crate) fn chrome(&self, role: ChromeRole, state: ChromeState) -> Visual {
        // Chrome lookup accepts activation state without admitting pointer interaction states.
        self.values.chrome[role.index()][state.index()]
    }

    /// Replaces one window chrome visual.
    pub(crate) fn set_chrome(&mut self, role: ChromeRole, state: ChromeState, visual: Visual) {
        // Detach shared storage before changing exactly one typed family slot.
        Arc::make_mut(&mut self.values).chrome[role.index()][state.index()] = visual;
    }

    /// Iterates over every resolved visual for atlas-ownership validation.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Visual> {
        // Flatten each concrete family locally; no synthetic cross-family role or state is created.
        self.values
            .surfaces
            .iter()
            .flatten()
            .chain(self.values.controls.iter().flatten())
            .chain(self.values.menus.iter().flatten())
            .chain(self.values.chrome.iter().flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NinePatch, PointerState, color};

    /// Verifies cloning and changing one family slot leaves the source catalog untouched.
    #[test]
    fn catalog_has_copy_on_write_value_semantics() {
        let original_visual = Visual::new(NinePatch::solid(color(1, 2, 3, 255)), color(4, 5, 6, 255));
        let replacement = Visual::new(NinePatch::solid(color(7, 8, 9, 255)), color(10, 11, 12, 255));
        let original = AppearanceCatalog::from_fns(|_, _| original_visual, |_, _| original_visual, |_, _| original_visual, |_, _| original_visual);
        let mut changed = original.clone();
        let state = ControlState::Focused(PointerState::Pressed);
        changed.set_control(ControlRole::Button, state, replacement);

        assert_eq!(original.control(ControlRole::Button, state).foreground.r, 4);
        assert_eq!(changed.control(ControlRole::Button, state).foreground.r, 10);
    }
}
