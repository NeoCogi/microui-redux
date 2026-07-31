use std::{cell::RefCell, rc::Rc};

use crate::sizing::SizePolicy;
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, ResourceState, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
    WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

/// Validated placement extent for one child owned by a [`Grid`].
///
/// A span describes the relationship between a grid and one child; it is deliberately not stored
/// on [`Node`]. Private fields make the non-zero invariant impossible to bypass through a struct
/// literal, so Grid layout does not need to repeatedly repair invalid input.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct GridSpan {
    columns: usize,
    rows: usize,
}

impl GridSpan {
    /// Default one-cell grid placement.
    pub const ONE: Self = Self { columns: 1, rows: 1 };

    /// Creates a span, normalizing a zero component to one track.
    pub const fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns: if columns == 0 { 1 } else { columns },
            rows: if rows == 0 { 1 } else { rows },
        }
    }

    /// Returns the number of occupied columns.
    pub const fn columns(self) -> usize {
        self.columns
    }

    /// Returns the number of occupied rows.
    pub const fn rows(self) -> usize {
        self.rows
    }
}

impl Default for GridSpan {
    fn default() -> Self {
        Self::ONE
    }
}

/// Unmounted insertion value pairing a unique node with its future Grid placement.
///
/// `GridItem` is not a runtime tree node: it has no identity, layout, or state of its own. A Grid
/// consumes it and stores the node and span in one invariant-maintaining collection.
pub struct GridItem {
    node: Node,
    span: GridSpan,
}

impl GridItem {
    /// Wraps a node with the default one-cell placement.
    pub fn new(node: Node) -> Self {
        Self { node, span: GridSpan::ONE }
    }

    /// Wraps a node with an explicit, normalized placement span.
    pub fn spanned(node: Node, columns: usize, rows: usize) -> Self {
        Self { node, span: GridSpan::new(columns, rows) }
    }

    /// Returns the placement that will be installed when this item is inserted.
    pub const fn span(&self) -> GridSpan {
        self.span
    }

    /// Recovers the still-unmounted node, intentionally discarding Grid-only placement.
    pub fn into_node(self) -> Node {
        self.node
    }

    fn into_parts(self) -> (Node, GridSpan) {
        (self.node, self.span)
    }
}

impl From<Node> for GridItem {
    fn from(node: Node) -> Self {
        Self::new(node)
    }
}

/// Private owner of Grid children and their index-matched placement spans.
///
/// Traversal currently requires an opaque [`Children`] collection, while Grid layout also needs
/// indexed edge metadata. Keeping both vectors behind this type gives them one mutation surface
/// and prevents public code from desynchronizing them.
#[derive(Default)]
struct GridItems {
    children: Children,
    spans: Vec<GridSpan>,
}

impl GridItems {
    fn from_items<T>(items: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<GridItem>,
    {
        let items = items.into_iter().map(Into::into);
        let (nodes, spans): (Vec<_>, Vec<_>) = items.map(GridItem::into_parts).unzip();
        let result = Self {
            children: nodes.into_iter().collect(),
            spans,
        };
        result.debug_assert_synchronized();
        result
    }

    fn len(&self) -> usize {
        self.debug_assert_synchronized();
        self.children.len()
    }

    fn is_empty(&self) -> bool {
        self.debug_assert_synchronized();
        self.children.is_empty()
    }

    fn push(&mut self, item: GridItem) {
        self.debug_assert_synchronized();
        let (node, span) = item.into_parts();
        self.children.push(node);
        self.spans.push(span);
        self.debug_assert_synchronized();
    }

    #[allow(clippy::result_large_err)] // Failure must preserve the exact unmounted owner and span.
    fn insert(&mut self, index: usize, item: GridItem) -> Result<(), GridItem> {
        self.debug_assert_synchronized();
        let (node, span) = item.into_parts();
        match self.children.insert(index, node) {
            Ok(()) => {
                self.spans.insert(index, span);
                self.debug_assert_synchronized();
                Ok(())
            }
            Err(node) => {
                self.debug_assert_synchronized();
                Err(GridItem { node, span })
            }
        }
    }

    fn remove_drop(&mut self, index: usize) -> bool {
        self.debug_assert_synchronized();
        if index >= self.children.len() {
            return false;
        }
        let removed = self.children.remove_drop(index);
        debug_assert!(removed, "GridItems checked the child index before removal");
        self.spans.remove(index);
        self.debug_assert_synchronized();
        true
    }

    fn clear(&mut self) {
        self.children.clear();
        self.spans.clear();
        self.debug_assert_synchronized();
    }

    fn replace<T>(&mut self, items: impl IntoIterator<Item = T>)
    where
        T: Into<GridItem>,
    {
        // Build a complete replacement first. Assignment then swaps both collections as one
        // logical operation instead of exposing an intermediate child/span mismatch.
        *self = Self::from_items(items);
    }

    fn span(&self, index: usize) -> Option<GridSpan> {
        self.debug_assert_synchronized();
        self.spans.get(index).copied()
    }

    fn set_span(&mut self, index: usize, span: GridSpan) -> bool {
        self.debug_assert_synchronized();
        let Some(current) = self.spans.get_mut(index) else {
            return false;
        };
        *current = span;
        true
    }

    fn debug_assert_synchronized(&self) {
        debug_assert_eq!(
            self.children.len(),
            self.spans.len(),
            "Grid child and placement collections must remain index-synchronized"
        );
    }
}

/// One-shot construction input for a state-owned [`Grid`].
#[derive(Default)]
pub struct GridParameters {
    items: GridItems,
    column_tracks: Vec<SizePolicy>,
    row_tracks: Vec<SizePolicy>,
}

impl WidgetParameters for GridParameters {}

impl GridParameters {
    /// Creates a Grid from track policies and ordered unmounted items.
    ///
    /// Plain [`Node`] values convert to one-cell items through [`From<Node>`]; callers only need
    /// [`GridItem`] where a non-default span is meaningful.
    pub fn new<T>(
        column_tracks: impl IntoIterator<Item = SizePolicy>,
        row_tracks: impl IntoIterator<Item = SizePolicy>,
        items: impl IntoIterator<Item = T>,
    ) -> Self
    where
        T: Into<GridItem>,
    {
        Self {
            items: GridItems::from_items(items),
            column_tracks: column_tracks.into_iter().collect(),
            row_tracks: row_tracks.into_iter().collect(),
        }
    }
}

/// Application-facing mutable Grid state.
///
/// This is the single retained authority for child ownership, child spans, and both track axes.
/// Span and track changes preserve the identity and widget state of every existing child.
pub struct GridState {
    items: GridItems,
    column_tracks: Vec<SizePolicy>,
    row_tracks: Vec<SizePolicy>,
}

impl WidgetState for GridState {}
impl ContainerState for GridState {}

impl GridState {
    /// Returns the number of owned Grid children.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns whether the Grid owns no children.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Appends one still-unmounted node and its Grid placement.
    pub fn push(&mut self, item: impl Into<GridItem>) {
        self.items.push(item.into());
    }

    /// Inserts an item, returning it unchanged when `index > len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, item: GridItem) -> Result<(), GridItem> {
        self.items.insert(index, item)
    }

    /// Drops one indexed child owner and its placement, reporting whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> bool {
        self.items.remove_drop(index)
    }

    /// Drops every current child owner and placement.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Replaces all children and placements in iterator order.
    pub fn replace<T>(&mut self, items: impl IntoIterator<Item = T>)
    where
        T: Into<GridItem>,
    {
        self.items.replace(items);
    }

    /// Returns one child's Grid-owned placement.
    pub fn span(&self, index: usize) -> Option<GridSpan> {
        self.items.span(index)
    }

    /// Replaces one child's placement without replacing or re-identifying the child.
    pub fn set_span(&mut self, index: usize, span: GridSpan) -> bool {
        self.items.set_span(index, span)
    }

    /// Returns the column track policies.
    pub fn column_tracks(&self) -> &[SizePolicy] {
        &self.column_tracks
    }

    /// Replaces the column track policies without changing child ownership.
    pub fn set_column_tracks(&mut self, tracks: impl IntoIterator<Item = SizePolicy>) {
        self.column_tracks = tracks.into_iter().collect();
    }

    /// Returns the row track policies.
    pub fn row_tracks(&self) -> &[SizePolicy] {
        &self.row_tracks
    }

    /// Replaces the row track policies without changing child ownership.
    pub fn set_row_tracks(&mut self, tracks: impl IntoIterator<Item = SizePolicy>) {
        self.row_tracks = tracks.into_iter().collect();
    }
}

/// Concrete retained runtime for a row-major Grid.
///
/// The runtime is the sole persistent strong owner of [`GridState`].
pub struct GridContainer {
    state: Rc<RefCell<GridState>>,
    opt: WidgetOption,
}

impl Widget for GridContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Grid::measure", |state| measure_grid(state, style, atlas, available))
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl WidgetStateOwner for GridContainer {
    type State = GridState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Container for GridContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        runtime_read_state(&self.state, "Grid::visit_children", |state| {
            visitor.visit(&state.items.children);
        });
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        runtime_update_state(&self.state, "Grid::visit_children_mut", |state| {
            visitor.visit(&mut state.items.children);
        });
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        runtime_update_state(&self.state, "Grid::layout", |state| {
            layout_grid(state, ctx, rect);
        });
    }
}

/// Builder associating [`GridParameters`] with [`GridContainer`].
pub struct GridBuilder;

impl ContainerBuilder for GridBuilder {
    type Parameters = GridParameters;
    type W = GridContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        GridContainer {
            state: Rc::new(RefCell::new(GridState {
                items: parameters.items,
                column_tracks: parameters.column_tracks,
                row_tracks: parameters.row_tracks,
            })),
            opt: WidgetOption::NONE,
        }
    }
}

/// Convenience constructor namespace for state-owned Grids.
pub struct Grid;

impl Grid {
    /// Creates a Grid and returns its weak state capability plus completed owning node.
    pub fn create(parameters: GridParameters) -> (WidgetStateHandle<GridState>, Node) {
        let container = GridBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}

fn measure_grid(state: &GridState, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    let columns = state.column_tracks.len().max(1);
    let rows = grid_placements(state.items.len(), columns, &state.items.spans)
        .into_iter()
        .map(|placement| placement.row + placement.row_span)
        .max()
        .unwrap_or(0)
        .max(state.row_tracks.len())
        .max(1);
    let default_height = super::super::default_cell_height(style, atlas);
    let width = available.width;
    let height = if state.row_tracks.is_empty() {
        default_height
            .saturating_mul(rows as i32)
            .saturating_add(style.spacing.saturating_mul(rows.saturating_sub(1) as i32))
    } else {
        available.height
    };
    Dimensioni::new(width.max(0), height.max(0))
}

fn layout_grid(state: &mut GridState, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
    let columns = state.column_tracks.len().max(1);
    // Placement is derived once and then shared by both axis calculations and child layout. This
    // makes it impossible for measurement within this pass to pair a child with a different span.
    let placements = grid_placements(state.items.len(), columns, &state.items.spans);
    let rows = placements
        .iter()
        .map(|placement| placement.row + placement.row_span)
        .max()
        .unwrap_or(0)
        .max(state.row_tracks.len())
        .max(1);
    let spacing = ctx.style().spacing;
    let available_width = rect.width.saturating_sub(spacing.saturating_mul(columns.saturating_sub(1) as i32));
    let available_height = rect.height.saturating_sub(spacing.saturating_mul(rows.saturating_sub(1) as i32));
    let preferred_widths = vec![super::super::default_cell_width(ctx.style()); columns];
    let preferred_heights = vec![super::super::default_cell_height(ctx.style(), ctx.atlas()); rows];
    let column_widths = super::super::resolve_axis_tracks(&super::super::track_policies(&state.column_tracks, columns), &preferred_widths, available_width);
    let row_heights = super::super::resolve_axis_tracks(&super::super::track_policies(&state.row_tracks, rows), &preferred_heights, available_height);

    for placement in placements {
        let x = rect.x + column_widths.iter().take(placement.column).sum::<i32>() + spacing.saturating_mul(placement.column as i32);
        let y = rect.y + row_heights.iter().take(placement.row).sum::<i32>() + spacing.saturating_mul(placement.row as i32);
        let width = super::super::span_size(&column_widths, placement.column, placement.column_span, spacing);
        let height = super::super::span_size(&row_heights, placement.row, placement.row_span, spacing);
        let _ = ctx.layout_child(&mut state.items.children, placement.child_index, Recti::new(x, y, width, height));
    }
}

/// Concrete row-major placement of one child inside a Grid.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct GridPlacement {
    child_index: usize,
    column: usize,
    row: usize,
    column_span: usize,
    row_span: usize,
}

fn grid_placements(child_count: usize, columns: usize, spans: &[GridSpan]) -> Vec<GridPlacement> {
    let columns = columns.max(1);
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let mut placements = Vec::with_capacity(child_count);
    let mut search_row = 0;
    let mut search_column = 0;

    for index in 0..child_count {
        let (row, column) = super::super::first_free_grid_cell(&mut occupied, columns, search_row, search_column);
        let span = spans.get(index).copied().unwrap_or(GridSpan::ONE);
        // A span is intrinsically non-zero. Only the parent-dependent right-edge clamp remains.
        let column_span = span.columns().min(columns.saturating_sub(column).max(1));
        let row_span = span.rows();
        super::super::mark_grid_occupied(&mut occupied, columns, row, column, row_span, column_span);
        placements.push(GridPlacement {
            child_index: index,
            column,
            row,
            column_span,
            row_span,
        });

        search_row = row;
        search_column = column.saturating_add(column_span);
        while search_column >= columns {
            search_column -= columns;
            search_row += 1;
        }
    }

    placements
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_node(label: &str) -> (WidgetStateHandle<crate::TextBlockState>, Node) {
        let (state, runtime) = crate::TextBlock::create(crate::TextBlockParameters::new(label));
        (state, Node::widget(runtime))
    }

    #[test]
    fn grid_span_normalizes_zero_components_once() {
        assert_eq!(GridSpan::new(0, 0), GridSpan::ONE);
        assert_eq!(GridSpan::new(0, 3), GridSpan::new(1, 3));
        assert_eq!(GridSpan::new(4, 0), GridSpan::new(4, 1));
        assert_eq!(GridSpan::new(4, 3).columns(), 4);
        assert_eq!(GridSpan::new(4, 3).rows(), 3);
    }

    #[test]
    fn grid_items_keep_child_and_span_topology_synchronized() {
        let (first_state, first) = text_node("first");
        let first_id = first.id();
        let (second_state, second) = text_node("second");
        let second_id = second.id();
        let mut items = GridItems::from_items([GridItem::spanned(first, 2, 1)]);

        assert_eq!(items.len(), 1);
        assert_eq!(items.span(0), Some(GridSpan::new(2, 1)));

        let rejected = items
            .insert(2, GridItem::spanned(second, 3, 2))
            .expect_err("out-of-range insertion must preserve the complete item");
        assert_eq!(rejected.node.id(), second_id);
        assert_eq!(rejected.span(), GridSpan::new(3, 2));
        assert_eq!(items.len(), 1);

        assert!(items.insert(1, rejected).is_ok());
        assert_eq!(items.children.as_slice()[0].id(), first_id);
        assert_eq!(items.children.as_slice()[1].id(), second_id);
        assert_eq!(items.spans, [GridSpan::new(2, 1), GridSpan::new(3, 2)]);

        assert!(items.remove_drop(0));
        assert!(!first_state.is_alive());
        assert!(second_state.is_alive());
        assert_eq!(items.children.as_slice()[0].id(), second_id);
        assert_eq!(items.span(0), Some(GridSpan::new(3, 2)));

        let (third_state, third) = text_node("third");
        let third_id = third.id();
        items.replace([GridItem::spanned(third, 4, 1)]);
        assert!(!second_state.is_alive());
        assert!(third_state.is_alive());
        assert_eq!(items.children.as_slice()[0].id(), third_id);
        assert_eq!(items.span(0), Some(GridSpan::new(4, 1)));
        assert!(!items.remove_drop(1));

        items.clear();
        assert!(items.is_empty());
        assert!(!third_state.is_alive());
    }

    #[test]
    fn grid_state_mutates_placement_and_tracks_without_replacing_children() {
        let (child_state, child) = text_node("stable");
        let child_id = child.id();
        let (grid_state, grid_node) = Grid::create(GridParameters::new([SizePolicy::Fixed(20)], [SizePolicy::Fixed(10)], [child]));

        grid_state
            .try_update(|state| {
                assert!(state.set_span(0, GridSpan::new(2, 3)));
                assert!(!state.set_span(1, GridSpan::ONE));
                state.set_column_tracks([SizePolicy::Fixed(20), SizePolicy::Remainder(0)]);
                state.set_row_tracks([SizePolicy::Fixed(10), SizePolicy::Fixed(15)]);
            })
            .expect("Grid state must be available before runtime traversal");

        let (observed_id, span, columns, rows) = grid_state
            .try_read(|state| {
                (
                    state.items.children.as_slice()[0].id(),
                    state.span(0),
                    state.column_tracks().to_vec(),
                    state.row_tracks().to_vec(),
                )
            })
            .expect("Grid state must remain owned by its runtime");
        assert_eq!(observed_id, child_id);
        assert_eq!(span, Some(GridSpan::new(2, 3)));
        assert_eq!(columns, [SizePolicy::Fixed(20), SizePolicy::Remainder(0)]);
        assert_eq!(rows, [SizePolicy::Fixed(10), SizePolicy::Fixed(15)]);
        assert!(child_state.is_alive());

        drop(grid_node);
        assert!(!grid_state.is_alive());
        assert!(!child_state.is_alive());
    }

    #[test]
    fn grid_state_handle_preserves_a_grid_item_when_access_fails() {
        let (_, child) = text_node("candidate");
        let child_id = child.id();
        let (grid_state, grid_node) = Grid::create(GridParameters::default());

        let rejected = grid_state
            .try_read(|_| {
                grid_state
                    .try_update_with(GridItem::spanned(child, 2, 3), |state, item| state.push(item))
                    .expect_err("same-cell read must prevent a nested update")
            })
            .expect("outer read must succeed");

        assert_eq!(rejected.node.id(), child_id);
        assert_eq!(rejected.span(), GridSpan::new(2, 3));
        assert!(grid_state.try_update_with(rejected, GridState::push).is_ok());
        assert_eq!(grid_state.try_read(GridState::len), Some(1));
        drop(grid_node);
    }

    #[test]
    fn placement_uses_validated_spans_and_clamps_only_at_the_right_edge() {
        let placements = grid_placements(3, 3, &[GridSpan::new(2, 1), GridSpan::new(8, 1), GridSpan::new(1, 2)]);

        assert_eq!(
            placements,
            [
                GridPlacement {
                    child_index: 0,
                    column: 0,
                    row: 0,
                    column_span: 2,
                    row_span: 1,
                },
                GridPlacement {
                    child_index: 1,
                    column: 2,
                    row: 0,
                    column_span: 1,
                    row_span: 1,
                },
                GridPlacement {
                    child_index: 2,
                    column: 0,
                    row: 1,
                    column_span: 1,
                    row_span: 2,
                },
            ]
        );
    }
}
