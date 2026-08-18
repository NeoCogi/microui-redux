//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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
//

use std::{cell::RefCell, rc::Rc};

use crate::ui_node::children::ChildrenHandle;
use crate::{
    AvailableSpace, Container, ContainerWidget, Dimensioni, MeasureCtx, Recti, TrackSize, TypedWidgetHandle, UiInputEvent, Widget, WidgetOption,
    WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};

use crate::ui_node::layout::{TrackResolver, resolve_tracks_in_place};
use super::{Children, ContainerLayoutCtx, Node};

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

/// Private owner of Grid edge metadata indexed to an adjacent [`Children`] collection.
///
/// Child ownership is deliberately not stored here. Every mutation receives the authoritative
/// collection explicitly and updates spans before returning. This keeps Grid metadata synchronized
/// while the concrete [`Container`] remains the sole strong owner of retained nodes.
#[derive(Default)]
struct GridItems {
    /// Grid-owned spans matched one-for-one with the adjacent child collection.
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
    fn from_items<T>(items: impl IntoIterator<Item = T>) -> (Children, Self)
    where
        T: Into<GridItem>,
    {
        // Split ownership and edge metadata once, behind the type that keeps them synchronized.
        let items = items.into_iter().map(Into::into);
        let (nodes, spans): (Vec<_>, Vec<_>) = items.map(GridItem::into_parts).unzip();
        let children = nodes.into_iter().collect::<Children>();
        let mut result = Self {
            spans,
            placements: Vec::new(),
            occupied: Vec::new(),
            columns: 1,
        };
        result.rebuild_placements(children.len());
        result.debug_assert_synchronized(children.len());
        (children, result)
    }

    /// Returns the synchronized child/span/placement count.
    #[cfg(test)]
    fn len(&self, children: &Children) -> usize {
        self.debug_assert_synchronized(children.len());
        children.len()
    }

    /// Returns whether the synchronized collections contain no child.
    #[cfg(test)]
    fn is_empty(&self, children: &Children) -> bool {
        self.debug_assert_synchronized(children.len());
        children.is_empty()
    }

    /// Appends one child/span pair and refreshes its derived row-major placement.
    fn push(&mut self, children: &mut Children, item: GridItem) {
        self.debug_assert_synchronized(children.len());
        let (node, span) = item.into_parts();
        children.push(node);
        self.spans.push(span);
        // Placement is derived from ordered topology, so every successful topology change rebuilds
        // it before the state can be observed again.
        self.rebuild_placements(children.len());
        self.debug_assert_synchronized(children.len());
    }

    /// Inserts ownership and placement metadata as one logical operation.
    #[allow(clippy::result_large_err)] // Failure must preserve the exact unmounted owner and span.
    fn insert(&mut self, children: &mut Children, index: usize, item: GridItem) -> Result<(), GridItem> {
        self.debug_assert_synchronized(children.len());
        let (node, span) = item.into_parts();
        match children.insert(index, node) {
            Ok(()) => {
                self.spans.insert(index, span);
                self.rebuild_placements(children.len());
                self.debug_assert_synchronized(children.len());
                Ok(())
            }
            Err(node) => {
                self.debug_assert_synchronized(children.len());
                Err(GridItem { node, span })
            }
        }
    }

    /// Removes one synchronized child/span pair and reports whether `index` existed.
    fn remove_drop(&mut self, children: &mut Children, index: usize) -> bool {
        self.debug_assert_synchronized(children.len());
        if index >= children.len() {
            return false;
        }
        let removed = children.remove_drop(index);
        debug_assert!(removed, "GridItems checked the child index before removal");
        self.spans.remove(index);
        self.rebuild_placements(children.len());
        self.debug_assert_synchronized(children.len());
        true
    }

    /// Clears owned and derived state together while retaining reusable allocation.
    fn clear(&mut self, children: &mut Children) {
        children.clear();
        self.spans.clear();
        self.rebuild_placements(children.len());
        self.debug_assert_synchronized(children.len());
    }

    /// Replaces topology while retaining the configured column count used for derived placement.
    fn replace<T>(&mut self, children: &mut Children, items: impl IntoIterator<Item = T>)
    where
        T: Into<GridItem>,
    {
        // Build a complete replacement first. Assignment then swaps both collections as one
        // logical operation instead of exposing an intermediate child/span mismatch.
        let columns = self.columns;
        let (replacement, metadata) = Self::from_items(items);
        *children = replacement;
        *self = metadata;
        self.set_columns(columns);
    }

    /// Returns the Grid-owned span associated with one child index.
    fn span(&self, index: usize) -> Option<GridSpan> {
        self.debug_assert_synchronized(self.spans.len());
        self.spans.get(index).copied()
    }

    /// Updates one span and all placements derived from the ordered span list.
    fn set_span(&mut self, index: usize, span: GridSpan) -> bool {
        self.debug_assert_synchronized(self.spans.len());
        let Some(current) = self.spans.get_mut(index) else {
            return false;
        };
        *current = span;
        self.rebuild_placements(self.spans.len());
        true
    }

    /// Updates the placement column count and immediately restores the derived-data invariant.
    fn set_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
        self.rebuild_placements(self.spans.len());
    }

    /// Recomputes row-major placement into retained buffers.
    ///
    /// This runs only when Grid topology, spans, or column count changes. Immutable measurement
    /// reads the resulting placements and therefore never needs interior mutability or allocation.
    fn rebuild_placements(&mut self, child_count: usize) {
        grid_placements_into(child_count, self.columns, &self.spans, &mut self.placements, &mut self.occupied);
    }

    /// Checks the parallel-vector and derived-placement invariants at mutation boundaries.
    fn debug_assert_synchronized(&self, child_count: usize) {
        debug_assert_eq!(
            child_count,
            self.spans.len(),
            "Grid child and placement collections must remain index-synchronized"
        );
        debug_assert_eq!(
            child_count,
            self.placements.len(),
            "Grid child and derived placement collections must remain index-synchronized"
        );
    }
}

/// One-shot construction input for a child-owning [`Grid`].
#[derive(Default)]
pub struct GridParameters {
    children: Children,
    items: GridItems,
    column_tracks: Vec<TrackSize>,
    row_tracks: Vec<TrackSize>,
}

impl WidgetParameters for GridParameters {}

impl GridParameters {
    /// Creates a Grid from track policies and ordered unmounted items.
    ///
    /// Plain [`Node`] values convert to one-cell items through [`From<Node>`]; callers only need
    /// [`GridItem`] where a non-default span is meaningful.
    pub fn new<T>(
        column_tracks: impl IntoIterator<Item = TrackSize>,
        row_tracks: impl IntoIterator<Item = TrackSize>,
        items: impl IntoIterator<Item = T>,
    ) -> Self
    where
        T: Into<GridItem>,
    {
        let column_tracks = column_tracks.into_iter().collect::<Vec<_>>();
        let (children, mut items) = GridItems::from_items(items);
        items.set_columns(column_tracks.len());
        Self {
            children,
            items,
            column_tracks,
            row_tracks: row_tracks.into_iter().collect(),
        }
    }
}

/// Application-facing mutable state for a retained two-dimensional container.
///
/// [`Container`] is the sole strong owner of the authoritative [`Children`] collection. `Grid`
/// retains only a weak `ChildrenHandle` for checked topology mutation and is the authority for the
/// parallel child spans, both track axes, derived row-major placements, and reusable layout output.
/// Span and track changes preserve the identity and widget state of every existing child because
/// those relationships are Grid-owned metadata rather than generic [`Node`] policy.
///
/// # Spans and topology
///
/// Every [`GridSpan`] is normalized to at least one row and one column. Push, insert, removal,
/// replacement, span changes, and column-track changes rebuild derived placements before the
/// updated state becomes observable. A retained occupancy bitmap is reused during those rebuilds,
/// and debug assertions keep children, spans, and placements index-synchronized.
///
/// A spanning child's intrinsic deficit is distributed across the non-Fixed tracks in its span.
/// Explicit Fixed tracks remain exact overflow boundaries. Integer deficit remainders go to earlier
/// eligible tracks, matching the track resolver's deterministic leading-pixel convention.
///
/// # Measurement and placement
///
/// Grid uses the shared scalar track resolver independently on both axes. Columns resolve before
/// rows because a child's resolved column span supplies the width constraint used to measure wrapped
/// height. Immutable [`ContainerWidget::measure`] replays scalar column resolution whenever a row
/// needs a child's span width. That deliberate recomputation preserves the allocation-free `&self`
/// measurement contract.
///
/// Mutable placement instead fills retained column and row vectors, resolves every track once, and
/// reuses those exact extents for row measurement and final child rectangles. The vectors retain
/// capacity between commits and are derived scratch rather than semantic Grid state. Grid therefore
/// retains `O(columns + rows)` layout output while the shared scalar resolver itself uses constant
/// temporary space.
///
/// Grid always has an effective track on each axis, but an implicit empty Content track contributes
/// zero. An empty generic Grid remains zero-sized unless an explicit Fixed track contributes extent.
pub struct Grid {
    /// Weak topology access coordinated with Grid-owned edge metadata below.
    children: ChildrenHandle,
    items: GridItems,
    column_tracks: Vec<TrackSize>,
    row_tracks: Vec<TrackSize>,
    /// Bound-dependent track extents reused only by the mutable layout phase.
    layout: GridLayout,
}

impl Grid {
    /// Returns the number of owned Grid children.
    pub fn len(&self) -> Option<usize> {
        self.children.len()
    }

    /// Returns whether the Grid owns no children.
    pub fn is_empty(&self) -> Option<bool> {
        self.children.is_empty()
    }

    /// Appends one still-unmounted node and its Grid placement.
    #[allow(clippy::result_large_err)] // Failure returns the exact unique node and its edge metadata.
    pub fn push(&mut self, item: impl Into<GridItem>) -> Result<(), GridItem> {
        let children = &self.children;
        let items = &mut self.items;
        children.try_update_with(item.into(), |children, item| items.push(children, item))
    }

    /// Inserts an item, returning it unchanged when `index > len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, item: GridItem) -> Result<(), GridItem> {
        let children = &self.children;
        let items = &mut self.items;
        children.try_update_with(item, |children, item| items.insert(children, index, item))?
    }

    /// Drops one indexed child owner and its placement, reporting whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        let items = &mut self.items;
        self.children.try_update_with((), |children, ()| items.remove_drop(children, index)).ok()
    }

    /// Drops every current child owner and placement.
    pub fn clear(&mut self) -> Option<()> {
        let items = &mut self.items;
        self.children.try_update_with((), |children, ()| items.clear(children)).ok()
    }

    /// Replaces all children and placements in iterator order.
    pub fn replace<T, I>(&mut self, items: I) -> Result<(), I>
    where
        T: Into<GridItem>,
        I: IntoIterator<Item = T>,
    {
        let metadata = &mut self.items;
        self.children.try_update_with(items, |children, items| metadata.replace(children, items))
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
    pub fn column_tracks(&self) -> &[TrackSize] {
        &self.column_tracks
    }

    /// Replaces the column track policies without changing child ownership.
    pub fn set_column_tracks(&mut self, tracks: impl IntoIterator<Item = TrackSize>) {
        self.column_tracks = tracks.into_iter().collect();
        self.items.set_columns(self.column_tracks.len());
    }

    /// Returns the row track policies.
    pub fn row_tracks(&self) -> &[TrackSize] {
        &self.row_tracks
    }

    /// Replaces the row track policies without changing child ownership.
    pub fn set_row_tracks(&mut self, tracks: impl IntoIterator<Item = TrackSize>) {
        self.row_tracks = tracks.into_iter().collect();
    }

    /// Creates a Grid and its weak typed widget handle.
    pub fn create(parameters: GridParameters) -> (TypedWidgetHandle<Self>, Node) {
        let mut items = parameters.items;
        items.set_columns(parameters.column_tracks.len());
        let children = Rc::new(RefCell::new(parameters.children));
        let widget = Self {
            children: ChildrenHandle::new(&children),
            items,
            column_tracks: parameters.column_tracks,
            row_tracks: parameters.row_tracks,
            layout: GridLayout::default(),
        };
        let (handle, container) = Container::from_shared(children, widget);
        (handle, Node::container(container))
    }
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

/// Shared child-measurement surface used by Grid's immutable and placement-time solvers.
trait GridMeasureCtx {
    fn spacing(&self) -> i32;
    fn measure_child(&mut self, index: usize, constraints: crate::Constraints) -> Option<Dimensioni>;
}

impl GridMeasureCtx for MeasureCtx<'_> {
    fn spacing(&self) -> i32 {
        self.style().spacing.max(0)
    }

    fn measure_child(&mut self, index: usize, constraints: crate::Constraints) -> Option<Dimensioni> {
        MeasureCtx::measure_child(self, index, constraints)
    }
}

struct GridLayoutMeasureCtx<'a, 'runtime> {
    ctx: &'a mut ContainerLayoutCtx<'runtime>,
    children: &'a mut Children,
}

impl GridMeasureCtx for GridLayoutMeasureCtx<'_, '_> {
    fn spacing(&self) -> i32 {
        self.ctx.style().spacing.max(0)
    }

    fn measure_child(&mut self, index: usize, constraints: crate::Constraints) -> Option<Dimensioni> {
        self.ctx.measure_child(self.children, index, constraints)
    }
}

impl ContainerWidget for Grid {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: crate::Constraints) -> Dimensioni {
        grid_size(ctx, self, constraints)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Move the reusable vectors out temporarily so layout can read the remaining Grid fields
        // without adding another RefCell or allocating per frame.
        let mut layout = std::mem::take(&mut self.layout);
        layout_grid(self, children, ctx, rect, &mut layout);
        self.layout = layout;
    }
}

impl Widget for Grid {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

/// Resolves both Grid axes and commits every child at its derived placement.
///
/// Columns are resolved before rows because a child's allocated column span is its text-wrapping
/// bound and therefore affects the preferred height contributed to row tracks.
fn layout_grid(state: &Grid, children: &mut Children, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti, layout: &mut GridLayout) {
    let spacing = ctx.style().spacing.max(0);
    // Resolve and retain each column width once for both row measurement and final placement.
    let available_width = AvailableSpace::bounded(rect.width);
    let available_height = AvailableSpace::bounded(rect.height);
    {
        let mut measure = GridLayoutMeasureCtx { ctx, children };
        resolve_columns_into(&mut measure, state, available_width, &mut layout.columns);

        // Row preferences are measured using the resolved column spans, then allocated vertically.
        resolve_rows_into(&mut measure, state, available_height, &layout.columns, &mut layout.rows);
    }

    // Placement and both track vectors are now authoritative for this layout commit.
    for placement in state.items.placements.iter().copied() {
        // child_origin = grid_origin + preceding_track_offset.
        let x = rect.x.saturating_add(track_offset(&layout.columns, placement.column, spacing));
        let y = rect.y.saturating_add(track_offset(&layout.rows, placement.row, spacing));
        let width = track_span(&layout.columns, placement.column, placement.column_span, spacing);
        let height = track_span(&layout.rows, placement.row, placement.row_span, spacing);
        let _ = ctx.layout_child(children, placement.child_index, Recti::new(x, y, width, height));
    }
}

/// Measures Grid preferred size without mutating Grid state or layout buffers.
///
/// The pure measurement path resolves scalar column spans on demand when measuring row content.
/// This costs recomputation but preserves `Widget::measure(&self)` and performs no allocation.
fn grid_size(ctx: &mut MeasureCtx<'_>, state: &Grid, constraints: crate::Constraints) -> Dimensioni {
    let spacing = ctx.style().spacing.max(0);
    let (columns, rows) = grid_dimensions(state);
    // Column preferences depend only on intrinsic child widths and Grid column spans.
    let width = measured_tracks(&state.column_tracks, columns, constraints.width, spacing, |index| {
        preferred_column(ctx, state, index, spacing)
    });
    // Row preferences additionally depend on each child's resolved column-span width.
    let height = measured_tracks(&state.row_tracks, rows, constraints.height, spacing, |index| {
        preferred_row(ctx, state, index, spacing, |ctx, placement| {
            resolved_column_span(ctx, state, spacing, constraints.width, placement)
        })
    });
    Dimensioni::new(width, height)
}

/// Returns the effective column and row counts represented by Grid configuration and placement.
fn grid_dimensions(state: &Grid) -> (usize, usize) {
    // A Grid always exposes at least one track on each axis, even when it has no children or
    // explicit tracks. Row spans may extend beyond the explicit row-track list.
    let columns = state.column_tracks.len().max(1);
    let rows = state
        .items
        .placements
        .iter()
        // occupied_row_end = placement_row + row_span.
        .map(|placement| placement.row.saturating_add(placement.row_span))
        .max()
        .unwrap_or(0)
        .max(state.row_tracks.len())
        .max(1);
    (columns, rows)
}

/// Computes one column's intrinsic minimum from its explicit track and spanning children.
fn preferred_column(ctx: &mut impl GridMeasureCtx, state: &Grid, index: usize, spacing: i32) -> i32 {
    let mut preferred = fixed_track_extent(track(&state.column_tracks, index));

    // Only children whose spans cover this column can increase its minimum.
    for placement in state.items.placements.iter().copied() {
        // column_end = first_column + column_span.
        let column_end = placement.column.saturating_add(placement.column_span);
        if index < placement.column || index >= column_end {
            continue;
        }
        let minimum = ctx
            .measure_child(placement.child_index, crate::Constraints::unbounded())
            .unwrap_or_default()
            .width;
        preferred = preferred.max(contribution_for_track(
            &state.column_tracks,
            index,
            placement.column,
            placement.column_span,
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
fn preferred_row<C: GridMeasureCtx>(ctx: &mut C, state: &Grid, index: usize, spacing: i32, mut child_width: impl FnMut(&mut C, GridPlacement) -> i32) -> i32 {
    let mut preferred = fixed_track_extent(track(&state.row_tracks, index));
    // Measure only children crossing this row, at the width of their complete column span.
    for placement in state.items.placements.iter().copied() {
        // row_end = first_row + row_span.
        let row_end = placement.row.saturating_add(placement.row_span);
        if index < placement.row || index >= row_end {
            continue;
        }
        let width = child_width(ctx, placement);
        let minimum = ctx
            .measure_child(
                placement.child_index,
                crate::Constraints::new(AvailableSpace::bounded(width), AvailableSpace::Unbounded),
            )
            .unwrap_or_default()
            .height;
        preferred = preferred.max(contribution_for_track(
            &state.row_tracks,
            index,
            placement.row,
            placement.row_span,
            spacing,
            minimum,
        ));
    }
    preferred
}

/// Returns the minimum required from `index` for one spanning child contribution.
///
/// The span begins at each track's explicit fixed contribution. Any remaining deficit is divided among
/// non-fixed tracks; fixed tracks never grow to hide overflow. Integer remainders are assigned from
/// left to right so the result is deterministic and, when a flexible track exists, the complete
/// span covers the child minimum.
fn contribution_for_track(tracks: &[TrackSize], index: usize, start: usize, span: usize, spacing: i32, minimum: i32) -> i32 {
    let current_track = track(tracks, index);
    let base = fixed_track_extent(current_track);
    if matches!(current_track, TrackSize::Fixed(_)) {
        // Fixed is an explicit overflow boundary, not a minimum that content may enlarge.
        return base;
    }

    // Establish how much of the child's minimum is already covered by base tracks and the spacing
    // internal to its span. `rank` identifies this track among only the flexible tracks.
    // span_end = span_start + max(span, 1).
    let end = start.saturating_add(span.max(1));
    // internal_gap_count = span - 1; initial_coverage starts with spacing * internal_gap_count.
    let mut initial = spacing.max(0).saturating_mul(span.saturating_sub(1) as i32);
    let mut flexible = 0_i32;
    let mut rank = 0_i32;
    for track_index in start..end {
        let track_size = track(tracks, track_index);
        // initial_coverage = previous_coverage + track_intrinsic_extent.
        initial = initial.saturating_add(fixed_track_extent(track_size));
        if !matches!(track_size, TrackSize::Fixed(_)) {
            if track_index < index {
                rank += 1;
            }
            flexible += 1;
        }
    }
    if flexible == 0 {
        return base;
    }

    // Divide only the uncovered pixels. Earlier flexible tracks receive the indivisible remainder.
    // deficit = max(child_minimum - initial_coverage, 0).
    let deficit = minimum.max(0).saturating_sub(initial);
    let increment = deficit / flexible + i32::from(rank < deficit % flexible);
    // contributed_extent = fixed_base_extent + allocated_deficit_increment.
    base.saturating_add(increment)
}

/// Resolves the width of one child's column span during Grid preferred-size measurement.
///
/// No resolved-width vector is available in `measure(&self)`, so this function replays the scalar
/// column allocator and accumulates only the requested span.
fn resolved_column_span(ctx: &mut impl GridMeasureCtx, state: &Grid, spacing: i32, available_width: AvailableSpace, placement: GridPlacement) -> i32 {
    let columns = state.column_tracks.len().max(1);
    let mut resolver = TrackResolver::new(
        available_width,
        spacing,
        columns,
        (0..columns).map(|index| (track(&state.column_tracks, index), preferred_column(ctx, state, index, spacing))),
    );
    let mut width = 0_i32;
    // placement_end = first_column + column_span.
    let placement_end = placement.column.saturating_add(placement.column_span);
    for index in 0..columns {
        let size = resolver.next(track(&state.column_tracks, index), preferred_column(ctx, state, index, spacing));
        if index >= placement.column && index < placement_end {
            // span_width = previous_span_width + resolved_track_width.
            width = width.saturating_add(size);
        }
    }
    // internal_gap_count = column_span - 1; spacing_total = spacing * internal_gap_count.
    let gap_count = placement.column_span.saturating_sub(1) as i32;
    let spacing_total = spacing.max(0).saturating_mul(gap_count);
    // complete_span_width = resolved_track_widths + internal_spacing.
    width.saturating_add(spacing_total)
}

/// Fills the layout-phase column buffer with resolved widths.
fn resolve_columns_into(ctx: &mut impl GridMeasureCtx, state: &Grid, available_width: AvailableSpace, columns: &mut Vec<i32>) {
    let count = state.column_tracks.len().max(1);
    let spacing = ctx.spacing();
    columns.clear();
    columns.extend((0..count).map(|index| preferred_column(ctx, state, index, spacing)));
    resolve_tracks_in_place(available_width, spacing, &state.column_tracks, columns);
}

/// Fills the layout-phase row buffer using already resolved column widths.
fn resolve_rows_into(ctx: &mut impl GridMeasureCtx, state: &Grid, available_height: AvailableSpace, columns: &[i32], rows: &mut Vec<i32>) {
    let (_, count) = grid_dimensions(state);
    let spacing = ctx.spacing();
    rows.clear();
    rows.extend((0..count).map(|index| {
        preferred_row(ctx, state, index, spacing, |_ctx, placement| {
            track_span(columns, placement.column, placement.column_span, spacing)
        })
    }));
    resolve_tracks_in_place(available_height, spacing, &state.row_tracks, rows);
}

/// Measures one complete track axis through the allocation rules shared with layout.
///
/// Unbounded axes can return the intrinsic summary directly. Bounded axes replay preferences in
/// order because `Remainder` depends on the pixels consumed by earlier tracks.
fn measured_tracks(tracks: &[TrackSize], count: usize, available: AvailableSpace, spacing: i32, mut preferred: impl FnMut(usize) -> i32) -> i32 {
    let mut resolver = TrackResolver::new(available, spacing, count, (0..count).map(|index| (track(tracks, index), preferred(index))));
    for index in 0..count {
        resolver.next(track(tracks, index), preferred(index));
    }
    resolver.extent()
}

/// Returns an explicit track or `Content` for an implicit track.
fn track(tracks: &[TrackSize], index: usize) -> TrackSize {
    tracks.get(index).copied().unwrap_or(TrackSize::Content)
}

fn fixed_track_extent(track: TrackSize) -> i32 {
    match track {
        TrackSize::Fixed(extent) => extent.max(0),
        TrackSize::Content | TrackSize::Flex(_) => 0,
    }
}

/// Sums a resolved track span including only the gaps internal to that span.
fn track_span(tracks: &[i32], start: usize, span: usize, spacing: i32) -> i32 {
    // track_total = sum(resolved_track_extents in the requested span).
    let track_total = tracks.iter().skip(start).take(span.max(1)).fold(0_i32, |sum, size| sum.saturating_add(*size));
    // internal_gap_count = span - 1; spacing_total = spacing * internal_gap_count.
    let gap_count = span.saturating_sub(1) as i32;
    let spacing_total = spacing.max(0).saturating_mul(gap_count);
    // span_extent = track_total + spacing_total.
    track_total.saturating_add(spacing_total)
}

/// Returns the offset preceding a track after earlier extents and their following gaps.
fn track_offset(tracks: &[i32], count: usize, spacing: i32) -> i32 {
    // preceding_track_total = sum(resolved extents before the requested track).
    let track_total = tracks.iter().take(count).fold(0_i32, |sum, size| sum.saturating_add(*size));
    // preceding_spacing_total = spacing * preceding_track_count.
    let spacing_total = spacing.max(0).saturating_mul(count as i32);
    // track_offset = preceding_track_total + preceding_spacing_total.
    track_total.saturating_add(spacing_total)
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
        // column_span = min(requested_span, max(column_count - start_column, 1)).
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
        // next_search_column = placed_column + occupied_column_span.
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
        // required_cells = (row + 1) * column_count.
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
    // end_row = start_row + max(row_span, 1); required_cells = end_row * column_count.
    let end_row = row.saturating_add(row_span.max(1));
    let required = end_row.saturating_mul(columns);
    if occupied.len() < required {
        occupied.resize(required, false);
    }
    // end_column = min(start_column + max(column_span, 1), column_count).
    let end_column = column.saturating_add(column_span.max(1)).min(columns);
    for y in row..end_row {
        for x in column..end_column {
            occupied[y * columns + x] = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;
    use crate::{Style, TypedWidgetHandle};

    fn text_node(label: &str) -> (TypedWidgetHandle<crate::TextBlock>, Node) {
        crate::TextBlock::create(crate::TextBlockParameters::new(label))
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
        let (mut children, mut items) = GridItems::from_items([GridItem::spanned(first, 2, 1)]);

        assert_eq!(items.len(&children), 1);
        assert_eq!(items.span(0), Some(GridSpan::new(2, 1)));

        let rejected = items
            .insert(&mut children, 2, GridItem::spanned(second, 3, 2))
            .expect_err("out-of-range insertion must preserve the complete item");
        assert_eq!(rejected.node.id(), second_id);
        assert_eq!(rejected.span(), GridSpan::new(3, 2));
        assert_eq!(items.len(&children), 1);

        assert!(items.insert(&mut children, 1, rejected).is_ok());
        assert_eq!(children.get(0).map(Node::id), Some(first_id));
        assert_eq!(children.get(1).map(Node::id), Some(second_id));
        assert_eq!(items.spans, [GridSpan::new(2, 1), GridSpan::new(3, 2)]);

        assert!(items.remove_drop(&mut children, 0));
        assert!(!first_state.is_alive());
        assert!(second_state.is_alive());
        assert_eq!(children.get(0).map(Node::id), Some(second_id));
        assert_eq!(items.span(0), Some(GridSpan::new(3, 2)));

        let (third_state, third) = text_node("third");
        let third_id = third.id();
        items.replace(&mut children, [GridItem::spanned(third, 4, 1)]);
        assert!(!second_state.is_alive());
        assert!(third_state.is_alive());
        assert_eq!(children.get(0).map(Node::id), Some(third_id));
        assert_eq!(items.span(0), Some(GridSpan::new(4, 1)));
        assert!(!items.remove_drop(&mut children, 1));

        items.clear(&mut children);
        assert!(items.is_empty(&children));
        assert!(!third_state.is_alive());
    }

    #[test]
    fn grid_widget_mutates_placement_and_tracks_without_replacing_children() {
        let (child_state, child) = text_node("stable");
        let (grid_state, grid_node) = Grid::create(GridParameters::new([TrackSize::Fixed(20)], [TrackSize::Fixed(10)], [child]));

        grid_state
            .try_update(|state| {
                assert!(state.set_span(0, GridSpan::new(2, 3)));
                assert!(!state.set_span(1, GridSpan::ONE));
                state.set_column_tracks([TrackSize::Fixed(20), TrackSize::Flex(1.0)]);
                state.set_row_tracks([TrackSize::Fixed(10), TrackSize::Fixed(15)]);
            })
            .expect("Grid state must be available before runtime traversal");

        let (span, columns, rows) = grid_state
            .try_read(|state| (state.span(0), state.column_tracks().to_vec(), state.row_tracks().to_vec()))
            .expect("Grid state must remain owned by its runtime");
        assert_eq!(span, Some(GridSpan::new(2, 3)));
        assert_eq!(columns, [TrackSize::Fixed(20), TrackSize::Flex(1.0)]);
        assert_eq!(rows, [TrackSize::Fixed(10), TrackSize::Fixed(15)]);
        assert!(child_state.is_alive());

        drop(grid_node);
        assert!(!grid_state.is_alive());
        assert!(!child_state.is_alive());
    }

    #[test]
    #[allow(clippy::result_large_err)] // The test verifies that failed mutation returns the exact owner.
    fn grid_widget_handle_preserves_a_grid_item_when_access_fails() {
        let (_, child) = text_node("candidate");
        let child_id = child.id();
        let (grid_state, grid_node) = Grid::create(GridParameters::default());

        let rejected = grid_state
            .try_read(
                |_| match grid_state.try_update_with(GridItem::spanned(child, 2, 3), |state, item| state.push(item)) {
                    Err(item) => item,
                    Ok(_) => panic!("same-cell read must prevent a nested update"),
                },
            )
            .expect("outer read must succeed");

        assert_eq!(rejected.node.id(), child_id);
        assert_eq!(rejected.span(), GridSpan::new(2, 3));
        assert!(matches!(grid_state.try_update_with(rejected, Grid::push), Ok(Ok(()))));
        assert_eq!(grid_state.try_read(Grid::len), Some(Some(1)));
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
        let columns = [TrackSize::Fixed(10), TrackSize::Content, TrackSize::Flex(1.0)];
        assert_eq!(contribution_for_track(&columns, 0, 0, 3, style.spacing, 70), 10);
        assert_eq!(contribution_for_track(&columns, 1, 0, 3, style.spacing, 70), 28);
        assert_eq!(contribution_for_track(&columns, 2, 0, 3, style.spacing, 70), 28);
        let rows = [TrackSize::Fixed(8), TrackSize::Content];
        assert_eq!(contribution_for_track(&rows, 0, 0, 2, style.spacing, 40), 8);
        assert_eq!(contribution_for_track(&rows, 1, 0, 2, style.spacing, 40), 30);

        let mut children = Children::new();
        let topology = Rc::new(RefCell::new(Children::new()));
        let empty = Grid {
            children: ChildrenHandle::new(&topology),
            items: GridItems::default(),
            column_tracks: vec![TrackSize::Fixed(10), TrackSize::Content],
            row_tracks: vec![TrackSize::Fixed(8), TrackSize::Content],
            layout: GridLayout::default(),
        };
        let atlas = test_atlas();
        let mut ctx = MeasureCtx::new(&style, &atlas, &mut children);
        let measured = grid_size(&mut ctx, &empty, crate::Constraints::unbounded());
        assert_eq!(measured.width, 12);
        assert_eq!(measured.height, 10);

        // A larger finite measurement offer does not belong to any content or fixed track. The
        // empty configured Grid therefore reports exactly the same desired track span.
        let bounded = grid_size(&mut ctx, &empty, crate::Constraints::bounded(Dimensioni::new(100, 100)));
        assert_eq!(bounded.width, measured.width);
        assert_eq!(bounded.height, measured.height);
    }

    #[test]
    fn grid_fixed_span_tracks_do_not_grow_to_hide_child_overflow() {
        let tracks = [TrackSize::Fixed(10), TrackSize::Fixed(12)];
        assert_eq!(contribution_for_track(&tracks, 0, 0, 2, 2, 50), 10);
        assert_eq!(contribution_for_track(&tracks, 1, 0, 2, 2, 50), 12);
    }
}
