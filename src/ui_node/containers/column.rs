//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without modification, are permitted
// provided that the conditions in the project LICENSE are met.
//

use crate::{
    Children, Constraints, Container, ContainerLayoutCtx, ContainerWidget, Dimensioni, MeasureCtx, Node, Recti, TrackSize, TypedWidgetHandle, UiInputEvent,
    Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};

use super::linear::{LinearItem, LinearState, Orientation, layout_linear, measure_linear};

/// One-shot construction input for a vertical [`Column`].
pub struct ColumnParameters {
    items: Vec<LinearItem>,
    reversed: bool,
}

impl WidgetParameters for ColumnParameters {}

impl ColumnParameters {
    /// Creates a top-to-bottom column.
    ///
    /// Plain [`Node`] values convert to content-height items. Use [`LinearItem`] for fixed or flex
    /// height and for the uncommon fixed child width.
    pub fn new<T>(items: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<LinearItem>,
    {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            reversed: false,
        }
    }

    /// Places child zero at the bottom, followed by later children above it.
    pub const fn reversed(mut self) -> Self {
        self.reversed = true;
        self
    }
}

impl Default for ColumnParameters {
    fn default() -> Self {
        Self::new(std::iter::empty::<LinearItem>())
    }
}

/// A vertical sequence whose child heights are parent-owned [`LinearItem`] tracks.
pub struct Column {
    linear: LinearState,
}

impl Column {
    /// Returns the number of owned children, or `None` while topology is unavailable.
    pub fn len(&self) -> Option<usize> {
        self.linear.len()
    }

    /// Returns whether the column is empty, or `None` while topology is unavailable.
    pub fn is_empty(&self) -> Option<bool> {
        self.linear.is_empty()
    }

    /// Appends one unmounted item, preserving it on failure.
    #[allow(clippy::result_large_err)] // Failure returns the exact unique node and its edge metadata.
    pub fn push(&mut self, item: impl Into<LinearItem>) -> Result<(), LinearItem> {
        self.linear.push(item)
    }

    /// Inserts one item, preserving it when the index or topology is unavailable.
    #[allow(clippy::result_large_err)]
    pub fn insert(&mut self, index: usize, item: LinearItem) -> Result<(), LinearItem> {
        self.linear.insert(index, item)
    }

    /// Drops one child and its height metadata, reporting whether the index existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        self.linear.remove_drop(index)
    }

    /// Drops all children and height metadata.
    pub fn clear(&mut self) -> Option<()> {
        self.linear.clear()
    }

    /// Replaces the complete ordered item sequence.
    pub fn replace<T, I>(&mut self, items: I) -> Result<(), I>
    where
        T: Into<LinearItem>,
        I: IntoIterator<Item = T>,
    {
        self.linear.replace(items)
    }

    /// Returns one child's height track.
    pub fn track(&self, index: usize) -> Option<TrackSize> {
        self.linear.main(index)
    }

    /// Replaces one existing child's height track without replacing the child.
    pub fn set_track(&mut self, index: usize, track: TrackSize) -> bool {
        self.linear.set_main(index, track)
    }

    /// Returns whether child zero is anchored at the bottom of the allocation.
    pub const fn is_reversed(&self) -> bool {
        self.linear.reversed()
    }

    /// Changes placement direction without changing item order or sizing.
    pub fn set_reversed(&mut self, reversed: bool) {
        self.linear.set_reversed(reversed);
    }

    /// Creates a child-owning column and a weak typed handle to its mounted state.
    pub fn create(parameters: ColumnParameters) -> (TypedWidgetHandle<Self>, Node) {
        let (children, mut linear) = LinearState::mount(parameters.items);
        linear.set_reversed(parameters.reversed);
        let (handle, container) = Container::from_shared(children, Self { linear });
        (handle, Node::container(container))
    }
}

impl ContainerWidget for Column {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        // A Column reports its widest desired child but stretches children across its exact width
        // during placement. Main-axis sizing is otherwise identical to Row.
        measure_linear(ctx, &self.linear, Orientation::Vertical, None, 0, constraints)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        layout_linear(ctx, children, &mut self.linear, Orientation::Vertical, None, 0, rect);
    }
}

impl Widget for Column {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}
