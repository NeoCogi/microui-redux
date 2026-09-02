//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Concrete generic tables for closed appearance-role and visual-state domains.

use std::{ops::Index, sync::Arc};

use super::{AppearanceRole, VisualState};

/// Complete fixed-size value table indexed by [`VisualState`].
///
/// The generic parameter is a concrete compile-time type. The table cannot contain heterogeneous
/// values, erased payloads, or missing states, which makes lookup total and branch-free.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StateTable<T> {
    /// Values stored in [`VisualState`] declaration order.
    values: [T; VisualState::COUNT],
}

impl<T> StateTable<T> {
    /// Creates a complete table from values already arranged in visual-state order.
    pub const fn new(values: [T; VisualState::COUNT]) -> Self {
        // Keeping the array private preserves the enum-indexed contract after construction.
        Self { values }
    }

    /// Returns a shared reference to the value for `state`.
    pub const fn get(&self, state: VisualState) -> &T {
        // VisualState is a closed contiguous enum generated together with its COUNT constant.
        &self.values[state.index()]
    }

    /// Replaces the value for exactly one typed state.
    pub fn set(&mut self, state: VisualState, value: T) {
        // The enum prevents an invalid numeric slot from entering this API.
        self.values[state.index()] = value;
    }

    /// Returns the complete underlying array when a consumer needs ordered bulk traversal.
    pub fn into_array(self) -> [T; VisualState::COUNT] {
        // Consuming the table prevents callers from mutating storage behind another owner.
        self.values
    }

    /// Iterates over every value in visual-state order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &T> {
        // Array iteration has no allocation and preserves the generated enum order.
        self.values.iter()
    }
}

impl<T: Clone> StateTable<T> {
    /// Creates a complete table by cloning one value into every state.
    pub fn filled(value: T) -> Self {
        // `from_fn` supports concrete non-Copy values without weakening the type contract.
        Self::new(std::array::from_fn(|_| value.clone()))
    }
}

impl<T> Index<VisualState> for StateTable<T> {
    type Output = T;

    /// Provides concise typed indexing without exposing integer indices.
    fn index(&self, state: VisualState) -> &Self::Output {
        // Delegate to the single checked enum-to-index conversion.
        self.get(state)
    }
}

/// Complete copy-on-write table indexed by [`AppearanceRole`].
///
/// Cloning a table shares its fixed storage. Mutating one role detaches that storage, retaining
/// ordinary value semantics without a hash map, string lookup, or typeless extension mechanism.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RoleTable<T> {
    /// Role values stored in [`AppearanceRole`] declaration order.
    values: Arc<[T; AppearanceRole::COUNT]>,
}

impl<T> RoleTable<T> {
    /// Builds a complete table by calling `make` once for every concrete role.
    pub(crate) fn from_fn(mut make: impl FnMut(AppearanceRole) -> T) -> Self {
        // Mapping through ALL couples construction to the same declaration order used by lookup.
        let values = std::array::from_fn(|index| make(AppearanceRole::ALL[index]));
        Self { values: Arc::new(values) }
    }

    /// Returns a shared reference to the value for `role`.
    pub(crate) fn get(&self, role: AppearanceRole) -> &T {
        // AppearanceRole is a closed contiguous enum generated with the table length.
        &self.values[role.index()]
    }

    /// Replaces exactly one role while preserving copy-on-write value semantics.
    pub(crate) fn set(&mut self, role: AppearanceRole, value: T)
    where
        T: Clone,
    {
        // Arc detachment occurs only for an actual mutation of shared skin data.
        Arc::make_mut(&mut self.values)[role.index()] = value;
    }

    /// Iterates over every value in appearance-role order.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &T> {
        // The fixed array provides allocation-free ordered traversal.
        self.values.iter()
    }
}

impl<T: Clone> RoleTable<T> {
    /// Creates a complete table by cloning one value into every role.
    pub(crate) fn filled(value: T) -> Self {
        // Construction through `from_fn` keeps the role-order invariant in one implementation.
        Self::from_fn(|_| value.clone())
    }
}

impl<T> Index<AppearanceRole> for RoleTable<T> {
    type Output = T;

    /// Provides concise typed indexing without exposing numeric catalog slots.
    fn index(&self, role: AppearanceRole) -> &Self::Output {
        // Delegate to the one role lookup implementation.
        self.get(role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ControlRole;

    /// Verifies state tables are total and independently mutable through typed indices.
    #[test]
    fn state_table_uses_the_generated_state_order() {
        let mut table = StateTable::filled(3_u8);
        table.set(VisualState::PressedFocused, 9);

        assert_eq!(table[VisualState::Normal], 3);
        assert_eq!(table[VisualState::PressedFocused], 9);
        assert_eq!(table.iter().count(), VisualState::COUNT);
    }

    /// Verifies cloned role tables detach only when one clone is changed.
    #[test]
    fn role_table_has_copy_on_write_value_semantics() {
        let original = RoleTable::filled(4_u8);
        let mut changed = original.clone();
        changed.set(AppearanceRole::Control(ControlRole::Button), 8);

        assert_eq!(original[AppearanceRole::Control(ControlRole::Button)], 4);
        assert_eq!(changed[AppearanceRole::Control(ControlRole::Button)], 8);
        assert_eq!(changed.iter().count(), AppearanceRole::COUNT);
    }
}
