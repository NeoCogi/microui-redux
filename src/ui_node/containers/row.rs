//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without modification, are permitted
// provided that the conditions in the project LICENSE are met.
//

use crate::{
    Children, Constraints, Container, ContainerLayoutCtx, ContainerWidget, Dimensioni, Linear, LinearCrossSize, LinearParameters, MeasureCtx, Node, Recti,
    TrackSize, TypedWidgetHandle, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};

use crate::ui_node::layout::RowHeight;
use crate::ui_node::layout::linear::{self as linear_layout, LinearItem};

/// One-shot construction input for a horizontal [`Row`].
///
/// Each item owns its width track. The row uses content height by default; named configuration can
/// select an exact height or fill a finite height supplied by the Row's parent.
pub struct RowParameters {
    items: Vec<LinearItem>,
    height: RowHeight,
}

impl WidgetParameters for RowParameters {}

impl RowParameters {
    /// Creates a content-height row from ordered child items.
    ///
    /// Plain [`Node`] values convert to content-width items. Use [`LinearItem`] only for a fixed or
    /// flexible width, or for an explicit fixed child height.
    pub fn new<T>(items: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<LinearItem>,
    {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            height: RowHeight::Content,
        }
    }

    /// Replaces the shared line-height behavior.
    pub const fn with_height(mut self, height: RowHeight) -> Self {
        self.height = height;
        self
    }

    /// Uses an exact non-negative shared line height.
    pub const fn fixed_height(self, extent: i32) -> Self {
        self.with_height(RowHeight::fixed(extent))
    }

    /// Stretches the shared line across a bounded parent height.
    pub const fn fill_height(self) -> Self {
        self.with_height(RowHeight::Fill)
    }
}

impl Default for RowParameters {
    fn default() -> Self {
        Self::new(std::iter::empty::<LinearItem>())
    }
}

/// A horizontal sequence whose child widths are parent-owned [`LinearItem`] tracks.
///
/// Row and [`crate::Column`] use the same private measurement and allocation implementation. This
/// type owns only the horizontal name, the shared line-height rule, and mutation methods that keep
/// child ownership synchronized with its edge metadata.
pub struct Row {
    linear: Linear,
}

impl Row {
    /// Returns the number of owned children, or `None` while topology is unavailable.
    pub fn len(&self) -> Option<usize> {
        self.linear.len()
    }

    /// Returns whether the row is empty, or `None` while topology is unavailable.
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

    /// Drops one child and its width metadata, reporting whether the index existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        self.linear.remove_drop(index)
    }

    /// Drops all children and width metadata.
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

    /// Returns one child's width track.
    pub fn track(&self, index: usize) -> Option<TrackSize> {
        self.linear.track(index)
    }

    /// Replaces one existing child's width track without replacing the child.
    pub fn set_track(&mut self, index: usize, track: TrackSize) -> bool {
        self.linear.set_track(index, track)
    }

    /// Returns the shared line-height rule.
    pub const fn height(&self) -> RowHeight {
        match self.linear.cross_size() {
            LinearCrossSize::Content => RowHeight::Content,
            LinearCrossSize::Stretch => RowHeight::Fill,
            LinearCrossSize::Fixed(extent) => RowHeight::Fixed(extent),
        }
    }

    /// Replaces the shared line-height rule.
    pub fn set_height(&mut self, height: RowHeight) {
        self.linear.set_cross_size(match height {
            RowHeight::Content => LinearCrossSize::Content,
            RowHeight::Fixed(extent) => LinearCrossSize::Fixed(extent),
            RowHeight::Fill => LinearCrossSize::Stretch,
        });
    }

    /// Creates a child-owning row and a weak typed handle to its mounted state.
    pub fn create(parameters: RowParameters) -> (TypedWidgetHandle<Self>, Node) {
        let cross_size = match parameters.height {
            RowHeight::Content => LinearCrossSize::Content,
            RowHeight::Fixed(extent) => LinearCrossSize::Fixed(extent),
            RowHeight::Fill => LinearCrossSize::Stretch,
        };
        let (children, linear) = Linear::mount(LinearParameters::horizontal(parameters.items).with_cross_size(cross_size));
        let widget = Self { linear };
        let (handle, container) = Container::from_shared(children, widget);
        (handle, Node::container(container))
    }
}

impl ContainerWidget for Row {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        linear_layout::measure(ctx, &self.linear, constraints)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        linear_layout::place(ctx, children, &mut self.linear, rect);
    }
}

impl Widget for Row {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;
    use crate::{AvailableSpace, Custom, CustomParameters, Style};

    /// Measures one ordinary content-width row through its public container contract.
    ///
    /// Keeping this fixture at the Row boundary ensures the shared scalar solver cannot report
    /// unused bounded space even when its direct arithmetic tests continue to pass.
    fn measure_content_row(width: AvailableSpace) -> Dimensioni {
        let child = Node::widget(Custom::create(CustomParameters::new("content")));
        let (children, linear) = Linear::mount(LinearParameters::horizontal([child]));
        let row = Row { linear };
        let style = Style::default();
        let atlas = test_atlas();
        let mut children = children.borrow_mut();
        let mut ctx = MeasureCtx::new(&style, &atlas, &mut children);

        // Height remains unbounded so this probe isolates the main-axis bounded measurement rule.
        row.measure(&mut ctx, Constraints::new(width, AvailableSpace::Unbounded))
    }

    #[test]
    fn row_mutations_keep_nodes_and_tracks_synchronized() {
        let first = Custom::create(CustomParameters::new("first"));
        let (first_state, first) = Node::typed_widget(first);
        let (row, node) = Row::create(RowParameters::new([LinearItem::fixed(first, 20)]));

        row.try_update(|state| {
            assert!(
                state
                    .push(LinearItem::flex(Node::widget(Custom::create(CustomParameters::new("second"))), 2.0))
                    .is_ok()
            );
            assert!(state.set_track(0, TrackSize::Flex(1.0)));
            state.set_height(RowHeight::fixed(24));
        })
        .unwrap();

        assert_eq!(row.try_read(|state| state.track(0)), Some(Some(TrackSize::Flex(1.0))));
        assert_eq!(row.try_read(Row::height), Some(RowHeight::Fixed(24)));
        assert_eq!(row.try_read(Row::len), Some(Some(2)));
        assert_eq!(row.try_update(|state| state.remove_drop(0)), Some(Some(true)));
        assert!(!first_state.is_alive());
        drop(node);
        assert!(!row.is_alive());
    }

    #[test]
    fn bounded_content_row_reports_its_desired_width() {
        let desired = measure_content_row(AvailableSpace::Unbounded);
        let bounded = measure_content_row(AvailableSpace::bounded(desired.width.saturating_add(100)));

        assert_eq!(bounded.width, desired.width);
        assert_eq!(bounded.height, desired.height);
    }
}
