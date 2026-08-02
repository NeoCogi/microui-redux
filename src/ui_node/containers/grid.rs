use std::{cell::RefCell, rc::Rc};

use crate::sizing::SizePolicy;
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
    WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Axis, Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

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

    /// Splits insertion-only syntax into the parallel values retained by [`GridItems`].
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
    /// Unique child owners in public traversal order.
    children: Children,
    /// Grid-owned spans matched one-for-one with `children`.
    spans: Vec<GridSpan>,
    /// Row-major placement derived from `spans` and `columns`.
    placements: Vec<GridPlacement>,
    /// Reused occupancy bitmap needed only while rebuilding `placements`.
    occupied: Vec<bool>,
    /// Effective column count used by the current derived placements.
    columns: usize,
}

impl GridItems {
    /// Collects unmounted items and establishes the initial placement invariant.
    fn from_items<T>(items: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<GridItem>,
    {
        // Split ownership and edge metadata once, behind the type that keeps them synchronized.
        let items = items.into_iter().map(Into::into);
        let (nodes, spans): (Vec<_>, Vec<_>) = items.map(GridItem::into_parts).unzip();
        let mut result = Self {
            children: nodes.into_iter().collect(),
            spans,
            placements: Vec::new(),
            occupied: Vec::new(),
            columns: 1,
        };
        result.rebuild_placements();
        result.debug_assert_synchronized();
        result
    }

    /// Returns the synchronized child/span/placement count.
    fn len(&self) -> usize {
        self.debug_assert_synchronized();
        self.children.len()
    }

    /// Returns whether the synchronized collections contain no child.
    fn is_empty(&self) -> bool {
        self.debug_assert_synchronized();
        self.children.is_empty()
    }

    /// Appends one child/span pair and refreshes its derived row-major placement.
    fn push(&mut self, item: GridItem) {
        self.debug_assert_synchronized();
        let (node, span) = item.into_parts();
        self.children.push(node);
        self.spans.push(span);
        // Placement is derived from ordered topology, so every successful topology change rebuilds
        // it before the state can be observed again.
        self.rebuild_placements();
        self.debug_assert_synchronized();
    }

    /// Inserts ownership and placement metadata as one logical operation.
    #[allow(clippy::result_large_err)] // Failure must preserve the exact unmounted owner and span.
    fn insert(&mut self, index: usize, item: GridItem) -> Result<(), GridItem> {
        self.debug_assert_synchronized();
        let (node, span) = item.into_parts();
        match self.children.insert(index, node) {
            Ok(()) => {
                self.spans.insert(index, span);
                self.rebuild_placements();
                self.debug_assert_synchronized();
                Ok(())
            }
            Err(node) => {
                self.debug_assert_synchronized();
                Err(GridItem { node, span })
            }
        }
    }

    /// Removes one synchronized child/span pair and reports whether `index` existed.
    fn remove_drop(&mut self, index: usize) -> bool {
        self.debug_assert_synchronized();
        if index >= self.children.len() {
            return false;
        }
        let removed = self.children.remove_drop(index);
        debug_assert!(removed, "GridItems checked the child index before removal");
        self.spans.remove(index);
        self.rebuild_placements();
        self.debug_assert_synchronized();
        true
    }

    /// Clears owned and derived state together while retaining reusable allocation.
    fn clear(&mut self) {
        self.children.clear();
        self.spans.clear();
        self.rebuild_placements();
        self.debug_assert_synchronized();
    }

    /// Replaces topology while retaining the configured column count used for derived placement.
    fn replace<T>(&mut self, items: impl IntoIterator<Item = T>)
    where
        T: Into<GridItem>,
    {
        // Build a complete replacement first. Assignment then swaps both collections as one
        // logical operation instead of exposing an intermediate child/span mismatch.
        let columns = self.columns;
        *self = Self::from_items(items);
        self.set_columns(columns);
    }

    /// Returns the Grid-owned span associated with one child index.
    fn span(&self, index: usize) -> Option<GridSpan> {
        self.debug_assert_synchronized();
        self.spans.get(index).copied()
    }

    /// Updates one span and all placements derived from the ordered span list.
    fn set_span(&mut self, index: usize, span: GridSpan) -> bool {
        self.debug_assert_synchronized();
        let Some(current) = self.spans.get_mut(index) else {
            return false;
        };
        *current = span;
        self.rebuild_placements();
        true
    }

    /// Updates the placement column count and immediately restores the derived-data invariant.
    fn set_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
        self.rebuild_placements();
    }

    /// Recomputes row-major placement into retained buffers.
    ///
    /// This runs only when Grid topology, spans, or column count changes. Immutable measurement
    /// reads the resulting placements and therefore never needs interior mutability or allocation.
    fn rebuild_placements(&mut self) {
        grid_placements_into(self.children.len(), self.columns, &self.spans, &mut self.placements, &mut self.occupied);
    }

    /// Checks the parallel-vector and derived-placement invariants at mutation boundaries.
    fn debug_assert_synchronized(&self) {
        debug_assert_eq!(
            self.children.len(),
            self.spans.len(),
            "Grid child and placement collections must remain index-synchronized"
        );
        debug_assert_eq!(
            self.children.len(),
            self.placements.len(),
            "Grid child and derived placement collections must remain index-synchronized"
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
        let column_tracks = column_tracks.into_iter().collect::<Vec<_>>();
        let mut items = GridItems::from_items(items);
        items.set_columns(column_tracks.len());
        Self {
            items,
            column_tracks,
            row_tracks: row_tracks.into_iter().collect(),
        }
    }
}

/// Application-facing mutable Grid state.
///
/// This is the single retained authority for child ownership, child spans, and both track axes.
/// Span and track changes preserve the identity and widget state of every existing child. Spans
/// are Grid-owned parent-child placement metadata, not generic [`Node`] policy.
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
        self.items.set_columns(self.column_tracks.len());
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
    /// Bound-dependent track extents reused only by the mutable layout phase.
    layout: GridLayout,
    opt: WidgetOption,
}

/// Reusable output buffers for one committed Grid layout.
///
/// Measurement does not read or mutate these buffers. It follows the scalar path below; layout
/// fills them so each resolved track is computed once before placing all children.
#[derive(Default)]
struct GridLayout {
    /// Resolved widths indexed by column.
    columns: Vec<i32>,
    /// Resolved heights indexed by row.
    rows: Vec<i32>,
}

impl Widget for GridContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Grid::measure", |state| grid_size(state, style, atlas, available))
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

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
        // State and layout scratch have disjoint owners, so mutable layout needs neither interior
        // mutability nor a temporary per-frame track collection.
        let layout = &mut self.layout;
        runtime_update_state(&self.state, "Grid::layout", |state| {
            layout_grid(state, ctx, rect, layout);
        });
    }
}

/// Builder associating [`GridParameters`] with [`GridContainer`].
pub struct GridBuilder;

impl ContainerBuilder for GridBuilder {
    type Parameters = GridParameters;
    type W = GridContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        let mut items = parameters.items;
        // Placement depends on the final column topology. Normalize it before the state becomes
        // observable through the returned weak handle.
        items.set_columns(parameters.column_tracks.len());
        GridContainer {
            state: Rc::new(RefCell::new(GridState {
                items,
                column_tracks: parameters.column_tracks,
                row_tracks: parameters.row_tracks,
            })),
            layout: GridLayout::default(),
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

/// Resolves both Grid axes and commits every child at its derived placement.
///
/// Columns are resolved before rows because a child's allocated column span is its text-wrapping
/// bound and therefore affects the preferred height contributed to row tracks.
fn layout_grid(state: &mut GridState, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti, layout: &mut GridLayout) {
    let spacing = ctx.style().spacing.max(0);
    let (columns, rows) = grid_dimensions(state);

    // Resolve and retain each column width once for both row measurement and final placement.
    let available_width = available_tracks(rect.width, columns, spacing);
    resolve_columns_into(state, ctx.style(), ctx.atlas(), available_width, &mut layout.columns);

    // Row preferences are measured using the resolved column spans, then allocated vertically.
    let available_height = available_tracks(rect.height, rows, spacing);
    resolve_rows_into(state, ctx.style(), ctx.atlas(), available_height, &layout.columns, &mut layout.rows);

    // Placement and both track vectors are now authoritative for this layout commit.
    for placement in state.items.placements.iter().copied() {
        let x = rect.x.saturating_add(track_offset(&layout.columns, placement.column, spacing));
        let y = rect.y.saturating_add(track_offset(&layout.rows, placement.row, spacing));
        let width = track_span(&layout.columns, placement.column, placement.column_span, spacing);
        let height = track_span(&layout.rows, placement.row, placement.row_span, spacing);
        let _ = ctx.layout_child(&mut state.items.children, placement.child_index, Recti::new(x, y, width, height));
    }
}

/// Measures Grid preferred size without mutating Grid state or layout buffers.
///
/// The pure measurement path resolves scalar column spans on demand when measuring row content.
/// This costs recomputation but preserves `Widget::measure(&self)` and performs no allocation.
fn grid_size(state: &GridState, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    let spacing = style.spacing.max(0);
    let (columns, rows) = grid_dimensions(state);
    let available_width = available_tracks(available.width, columns, spacing);
    // Column preferences depend only on intrinsic child widths and Grid column spans.
    let width = measured_tracks(&state.column_tracks, columns, available_width, spacing, |index| {
        preferred_column(state, index, style, atlas, spacing)
    });
    let available_height = available_tracks(available.height, rows, spacing);
    // Row preferences additionally depend on each child's resolved column-span width.
    let height = measured_tracks(&state.row_tracks, rows, available_height, spacing, |index| {
        preferred_row(state, index, style, atlas, spacing, |placement| {
            resolved_column_span(state, style, atlas, spacing, available_width, placement)
        })
    });
    Dimensioni::new(width, height)
}

/// Returns the effective column and row counts represented by Grid configuration and placement.
fn grid_dimensions(state: &GridState) -> (usize, usize) {
    // A Grid always exposes at least one track on each axis, even when it has no children or
    // explicit policies. Row spans may extend beyond the explicit row-policy list.
    let columns = state.column_tracks.len().max(1);
    let rows = state
        .items
        .placements
        .iter()
        .map(|placement| placement.row.saturating_add(placement.row_span))
        .max()
        .unwrap_or(0)
        .max(state.row_tracks.len())
        .max(1);
    (columns, rows)
}

/// Removes inter-track spacing from a positive axis bound.
///
/// Zero remains the unbounded-measurement marker; a positive bound remains positive after inset.
fn available_tracks(available: i32, count: usize, spacing: i32) -> i32 {
    if available > 0 {
        available.saturating_sub(spacing.max(0).saturating_mul(count.saturating_sub(1) as i32)).max(1)
    } else {
        0
    }
}

/// Computes one column's intrinsic minimum from its fallback and spanning children.
fn preferred_column(state: &GridState, index: usize, style: &Style, atlas: &AtlasHandle, spacing: i32) -> i32 {
    let fallback = super::default_cell_width(style);
    let mut preferred = track_policy(&state.column_tracks, index).intrinsic_extent(fallback);

    // Only children whose spans cover this column can increase its minimum.
    for placement in state.items.placements.iter().copied() {
        if index < placement.column || index >= placement.column.saturating_add(placement.column_span) {
            continue;
        }
        let minimum = state
            .items
            .children
            .measure_child(placement.child_index, style, atlas, Dimensioni::default())
            .unwrap_or_default()
            .width;
        preferred = preferred.max(contribution_for_track(
            &state.column_tracks,
            index,
            placement.column,
            placement.column_span,
            fallback,
            spacing,
            minimum,
        ));
    }
    preferred
}

/// Computes one row's intrinsic minimum using caller-provided resolved child-span widths.
///
/// Layout supplies widths from its resolved column vector. Immutable measurement supplies a pure
/// scalar resolver. Keeping that choice at the call site prevents this helper from knowing about
/// caches, phases, or mutable container state.
fn preferred_row(state: &GridState, index: usize, style: &Style, atlas: &AtlasHandle, spacing: i32, child_width: impl Fn(GridPlacement) -> i32) -> i32 {
    let fallback = super::default_cell_height(style, atlas);
    let mut preferred = track_policy(&state.row_tracks, index).intrinsic_extent(fallback);
    // Measure only children crossing this row, at the width of their complete column span.
    for placement in state.items.placements.iter().copied() {
        if index < placement.row || index >= placement.row.saturating_add(placement.row_span) {
            continue;
        }
        let width = child_width(placement);
        let width = state
            .items
            .children
            .child_policy(placement.child_index)
            .unwrap_or_else(crate::Policy::auto)
            .width
            .measurement_bound(width);
        let minimum = state
            .items
            .children
            .measure_child(placement.child_index, style, atlas, Dimensioni::new(width.max(1), 0))
            .unwrap_or_default()
            .height;
        preferred = preferred.max(contribution_for_track(
            &state.row_tracks,
            index,
            placement.row,
            placement.row_span,
            fallback,
            spacing,
            minimum,
        ));
    }
    preferred
}

/// Returns the minimum required from `index` for one spanning child contribution.
///
/// The span begins at each track's policy-derived fallback. Any remaining deficit is divided among
/// non-fixed tracks; fixed tracks never grow to hide overflow. Integer remainders are assigned from
/// left to right so the result is deterministic and, when a flexible track exists, the complete
/// span covers the child minimum.
fn contribution_for_track(policies: &[SizePolicy], index: usize, start: usize, span: usize, fallback: i32, spacing: i32, minimum: i32) -> i32 {
    let policy = track_policy(policies, index);
    let base = policy.intrinsic_extent(fallback);
    if matches!(policy, SizePolicy::Fixed(_)) {
        // Fixed is an explicit overflow boundary, not a minimum that content may enlarge.
        return base;
    }

    // Establish how much of the child's minimum is already covered by base tracks and the spacing
    // internal to its span. `rank` identifies this track among only the flexible tracks.
    let end = start.saturating_add(span.max(1));
    let mut initial = spacing.max(0).saturating_mul(span.saturating_sub(1) as i32);
    let mut flexible = 0_i32;
    let mut rank = 0_i32;
    for track in start..end {
        let track_policy = track_policy(policies, track);
        initial = initial.saturating_add(track_policy.intrinsic_extent(fallback));
        if !matches!(track_policy, SizePolicy::Fixed(_)) {
            if track < index {
                rank += 1;
            }
            flexible += 1;
        }
    }
    if flexible == 0 {
        return base;
    }

    // Divide only the uncovered pixels. Earlier flexible tracks receive the indivisible remainder.
    let deficit = minimum.max(0).saturating_sub(initial);
    let increment = deficit / flexible + i32::from(rank < deficit % flexible);
    base.saturating_add(increment)
}

/// Resolves the width of one child's column span during immutable Grid measurement.
///
/// No resolved-width vector is available in `measure(&self)`, so this function replays the scalar
/// column allocator and accumulates only the requested span.
fn resolved_column_span(state: &GridState, style: &Style, atlas: &AtlasHandle, spacing: i32, available_width: i32, placement: GridPlacement) -> i32 {
    let columns = state.column_tracks.len().max(1);
    let mut axis = Axis::new(
        available_width,
        (0..columns).map(|index| (track_policy(&state.column_tracks, index), preferred_column(state, index, style, atlas, spacing))),
    );
    let mut width = 0_i32;
    // All preceding columns must be replayed because Remainder depends on ordered consumption.
    for index in 0..columns {
        let size = axis
            .next(track_policy(&state.column_tracks, index), preferred_column(state, index, style, atlas, spacing))
            .advance;
        if index >= placement.column && index < placement.column.saturating_add(placement.column_span) {
            width = width.saturating_add(size);
        }
    }
    width.saturating_add(spacing.max(0).saturating_mul(placement.column_span.saturating_sub(1) as i32))
}

/// Fills the layout-phase column buffer with resolved widths.
fn resolve_columns_into(state: &GridState, style: &Style, atlas: &AtlasHandle, available_width: i32, columns: &mut Vec<i32>) {
    let count = state.column_tracks.len().max(1);
    columns.clear();
    columns.extend((0..count).map(|index| preferred_column(state, index, style, atlas, style.spacing.max(0))));
    resolve_tracks(&state.column_tracks, available_width, columns);
}

/// Fills the layout-phase row buffer using already resolved column widths.
fn resolve_rows_into(state: &GridState, style: &Style, atlas: &AtlasHandle, available_height: i32, columns: &[i32], rows: &mut Vec<i32>) {
    let (_, count) = grid_dimensions(state);
    let spacing = style.spacing.max(0);
    rows.clear();
    rows.extend((0..count).map(|index| {
        preferred_row(state, index, style, atlas, spacing, |placement| {
            track_span(columns, placement.column, placement.column_span, spacing)
        })
    }));
    resolve_tracks(&state.row_tracks, available_height, rows);
}

/// Measures one complete track axis through the allocation rules shared with layout.
///
/// Unbounded axes can return the intrinsic summary directly. Bounded axes replay preferences in
/// order because `Remainder` depends on the pixels consumed by earlier tracks.
fn measured_tracks(policies: &[SizePolicy], count: usize, available: i32, spacing: i32, mut preferred: impl FnMut(usize) -> i32) -> i32 {
    let mut axis = Axis::new(available, (0..count).map(|index| (track_policy(policies, index), preferred(index))));
    if available == 0 {
        return axis.intrinsic_extent(count, spacing);
    }
    for index in 0..count {
        axis.next(track_policy(policies, index), preferred(index));
    }
    axis.extent(count, spacing)
}

/// Replaces preferred extents in a layout buffer with their resolved track extents.
fn resolve_tracks(policies: &[SizePolicy], available: i32, sizes: &mut [i32]) {
    // Axis construction reads the complete immutable preference slice before in-place replacement.
    let mut axis = Axis::new(
        available,
        sizes
            .iter()
            .copied()
            .enumerate()
            .map(|(index, preferred)| (track_policy(policies, index), preferred)),
    );
    for (index, size) in sizes.iter_mut().enumerate() {
        *size = axis.next(track_policy(policies, index), *size).advance;
    }
}

/// Returns an explicit track policy or `Auto` for an implicit track.
fn track_policy(policies: &[SizePolicy], index: usize) -> SizePolicy {
    policies.get(index).copied().unwrap_or(SizePolicy::Auto)
}

/// Sums a resolved track span including only the gaps internal to that span.
fn track_span(tracks: &[i32], start: usize, span: usize, spacing: i32) -> i32 {
    tracks
        .iter()
        .skip(start)
        .take(span.max(1))
        .fold(0_i32, |sum, size| sum.saturating_add(*size))
        .saturating_add(spacing.max(0).saturating_mul(span.saturating_sub(1) as i32))
}

/// Returns the offset preceding a track after earlier extents and their following gaps.
fn track_offset(tracks: &[i32], count: usize, spacing: i32) -> i32 {
    tracks
        .iter()
        .take(count)
        .fold(0_i32, |sum, size| sum.saturating_add(*size))
        .saturating_add(spacing.max(0).saturating_mul(count as i32))
}

/// Concrete row-major placement of one child inside a Grid.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct GridPlacement {
    /// Index into the authoritative child and span collections.
    child_index: usize,
    /// First occupied column.
    column: usize,
    /// First occupied row.
    row: usize,
    /// Effective column span after clamping at the right Grid edge.
    column_span: usize,
    /// Validated row span, which may extend implicit rows.
    row_span: usize,
}

/// Rebuilds row-major placements and occupancy into reusable Grid-owned buffers.
fn grid_placements_into(child_count: usize, columns: usize, spans: &[GridSpan], placements: &mut Vec<GridPlacement>, occupied: &mut Vec<bool>) {
    let columns = columns.max(1);
    occupied.clear();
    placements.clear();
    placements.reserve(child_count);
    let mut search_row = 0;
    let mut search_column = 0;

    // Continue after each placed span rather than restarting at the origin for every child.
    for index in 0..child_count {
        let (row, column) = first_free_grid_cell(occupied, columns, search_row, search_column);
        let span = spans.get(index).copied().unwrap_or(GridSpan::ONE);
        // A span is intrinsically non-zero. Only the parent-dependent right-edge clamp remains.
        let column_span = span.columns().min(columns.saturating_sub(column).max(1));
        let row_span = span.rows();
        mark_grid_occupied(occupied, columns, row, column, row_span, column_span);
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
}

/// Finds the first unoccupied cell at or after a row-major search position.
fn first_free_grid_cell(occupied: &mut Vec<bool>, columns: usize, mut row: usize, mut column: usize) -> (usize, usize) {
    loop {
        // Grow by complete rows so `row * columns + column` remains a valid flat index.
        let required = row.saturating_add(1).saturating_mul(columns);
        if occupied.len() < required {
            occupied.resize(required, false);
        }
        while column < columns {
            if !occupied[row * columns + column] {
                return (row, column);
            }
            column += 1;
        }
        row += 1;
        column = 0;
    }
}

/// Marks every cell covered by one validated placement span.
fn mark_grid_occupied(occupied: &mut Vec<bool>, columns: usize, row: usize, column: usize, row_span: usize, column_span: usize) {
    // Row spans may extend the current bitmap; column spans are clamped at the right Grid edge.
    let end_row = row.saturating_add(row_span.max(1));
    let required = end_row.saturating_mul(columns);
    if occupied.len() < required {
        occupied.resize(required, false);
    }
    for y in row..end_row {
        for x in column..column.saturating_add(column_span.max(1)).min(columns) {
            occupied[y * columns + x] = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;

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
        let mut placements = Vec::new();
        let mut occupied = Vec::new();
        grid_placements_into(
            3,
            3,
            &[GridSpan::new(2, 1), GridSpan::new(8, 1), GridSpan::new(1, 2)],
            &mut placements,
            &mut occupied,
        );

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

    #[test]
    fn grid_preferred_tracks_include_spans_spacing_and_explicit_empty_tracks() {
        let style = Style {
            padding: 0,
            spacing: 2,
            default_cell_width: 0,
            ..Style::default()
        };
        let columns = [SizePolicy::Fixed(10), SizePolicy::Auto, SizePolicy::Weight(1.0)];
        assert_eq!(contribution_for_track(&columns, 0, 0, 3, 0, style.spacing, 70), 10);
        assert_eq!(contribution_for_track(&columns, 1, 0, 3, 0, style.spacing, 70), 28);
        assert_eq!(contribution_for_track(&columns, 2, 0, 3, 0, style.spacing, 70), 28);
        let rows = [SizePolicy::Fixed(8), SizePolicy::Auto];
        assert_eq!(contribution_for_track(&rows, 0, 0, 2, 0, style.spacing, 40), 8);
        assert_eq!(contribution_for_track(&rows, 1, 0, 2, 0, style.spacing, 40), 30);

        let empty = GridState {
            items: GridItems::default(),
            column_tracks: vec![SizePolicy::Fixed(10), SizePolicy::Auto],
            row_tracks: vec![SizePolicy::Fixed(8), SizePolicy::Auto],
        };
        let measured = grid_size(&empty, &style, &test_atlas(), Dimensioni::default());
        assert_eq!(measured.width, 12);
        assert_eq!(measured.height, 20);
    }

    #[test]
    fn grid_fixed_span_tracks_do_not_grow_to_hide_child_overflow() {
        let tracks = [SizePolicy::Fixed(10), SizePolicy::Fixed(12)];
        assert_eq!(contribution_for_track(&tracks, 0, 0, 2, 0, 2, 50), 10);
        assert_eq!(contribution_for_track(&tracks, 1, 0, 2, 0, 2, 50), 12);
    }
}
