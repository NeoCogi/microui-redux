//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without modification, are permitted
// provided that the conditions in the project LICENSE are met.
//

//! Shared retained layout for horizontal rows and vertical columns.

use std::{cell::RefCell, rc::Rc};

use crate::ui_node::children::ChildrenHandle;
use crate::{AvailableSpace, Children, Constraints, ContainerLayoutCtx, Dimensioni, MeasureCtx, Node, Recti, TrackSize};

use super::TrackResolver;

/// Shared cross-axis sizing for one horizontal [`crate::Row`].
///
/// A Row has exactly one line, so weighted distribution has no meaningful sibling context on its
/// height axis. The shared linear algorithm owns these three behaviors; Row exposes them as its
/// public configuration vocabulary.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum RowHeight {
    /// Uses the tallest child's desired height, including the standard non-empty control minimum.
    #[default]
    Content,
    /// Uses an exact non-negative line height and allows taller child content to overflow.
    Fixed(i32),
    /// Fills a bounded height supplied by the Row's parent and uses content when unbounded.
    Fill,
}

impl RowHeight {
    /// Creates an exact line height, normalizing a negative public extent to zero.
    pub const fn fixed(extent: i32) -> Self {
        // Normalize at this named constructor so ordinary callers establish the documented
        // non-negative invariant before the value reaches measurement or placement.
        Self::Fixed(if extent < 0 { 0 } else { extent })
    }
}

/// An unmounted node paired with its main-axis relationship to a Row or Column.
///
/// The item is construction and mutation input, not another runtime node. Row interprets `main` as
/// width; Column interprets it as height. The optional fixed cross extent is likewise interpreted by
/// the owning container, so no container-specific policy is stored on [`Node`].
pub struct LinearItem {
    node: Node,
    main: TrackSize,
    fixed_cross: Option<i32>,
}

impl LinearItem {
    /// Creates an item with an explicit main-axis track that stretches across the container's line.
    pub fn new(node: Node, main: TrackSize) -> Self {
        Self { node, main, fixed_cross: None }
    }

    /// Creates a content-sized item.
    pub fn content(node: Node) -> Self {
        Self::new(node, TrackSize::Content)
    }

    /// Creates an item with an exact non-negative main-axis extent.
    pub fn fixed(node: Node, extent: i32) -> Self {
        Self::new(node, TrackSize::Fixed(extent))
    }

    /// Creates an item that receives a weighted share of bounded remaining main-axis space.
    pub fn flex(node: Node, weight: f32) -> Self {
        Self::new(node, TrackSize::Flex(weight))
    }

    /// Uses an exact non-negative cross-axis extent instead of stretching across the line.
    pub fn with_fixed_cross(mut self, extent: i32) -> Self {
        self.fixed_cross = Some(extent.max(0));
        self
    }

    /// Returns the parent-owned main-axis track.
    pub const fn main(&self) -> TrackSize {
        self.main
    }

    /// Returns the explicit cross-axis extent, or `None` when the item stretches across its line.
    pub const fn fixed_cross(&self) -> Option<i32> {
        self.fixed_cross
    }

    /// Recovers the still-unmounted node and discards its linear-container relationship.
    pub fn into_node(self) -> Node {
        self.node
    }

    fn into_parts(self) -> (Node, LinearItemSpec) {
        (
            self.node,
            LinearItemSpec {
                main: self.main,
                fixed_cross: self.fixed_cross,
            },
        )
    }
}

impl From<Node> for LinearItem {
    fn from(node: Node) -> Self {
        Self::content(node)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct LinearItemSpec {
    main: TrackSize,
    fixed_cross: Option<i32>,
}

/// Collects construction or replacement input into the runtime's split ownership representation.
///
/// The concrete container remains the sole strong owner of `Children`; linear state retains only
/// the index-matched relationship metadata that Row or Column interprets. Building both outputs in
/// one function keeps their order and length identical before either becomes observable.
fn collect_items<T>(items: impl IntoIterator<Item = T>) -> (Children, Vec<LinearItemSpec>)
where
    T: Into<LinearItem>,
{
    let mut children = Children::new();
    let mut specs = Vec::new();
    for item in items {
        // Split each unique unmounted owner exactly once, preserving iterator order in both
        // adjacent collections.
        let (node, spec) = item.into().into_parts();
        children.push(node);
        specs.push(spec);
    }
    debug_assert_eq!(children.len(), specs.len());
    (children, specs)
}

/// Complete retained linear state shared structurally by Row and Column.
///
/// `specs` is the only metadata collection and is index-matched to the concrete container's
/// authoritative `Children`. `resolved_main` is mutable placement scratch: it retains capacity
/// between commits but is never read by immutable measurement or exposed as semantic state.
pub(in crate::ui_node) struct LinearState {
    children: ChildrenHandle,
    specs: Vec<LinearItemSpec>,
    reversed: bool,
    resolved_main: Vec<i32>,
}

impl LinearState {
    /// Mounts ordered linear items and installs one weak topology capability beside their specs.
    pub(in crate::ui_node) fn mount<T>(items: impl IntoIterator<Item = T>) -> (Rc<RefCell<Children>>, Self)
    where
        T: Into<LinearItem>,
    {
        let (children, specs) = collect_items(items);
        let children = Rc::new(RefCell::new(children));
        let state = Self {
            children: ChildrenHandle::new(&children),
            specs,
            reversed: false,
            resolved_main: Vec::new(),
        };
        state.debug_assert_synchronized(children.borrow().len());
        (children, state)
    }

    /// Returns the mounted child count while the concrete collection is available.
    pub(in crate::ui_node) fn len(&self) -> Option<usize> {
        self.children.len()
    }

    /// Returns whether the mounted collection is empty while topology is available.
    pub(in crate::ui_node) fn is_empty(&self) -> Option<bool> {
        self.children.is_empty()
    }

    /// Appends one node and its relationship metadata as one observable topology mutation.
    #[allow(clippy::result_large_err)] // Failure preserves the unique node and its edge metadata.
    pub(in crate::ui_node) fn push(&mut self, item: impl Into<LinearItem>) -> Result<(), LinearItem> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        self.children.try_update_with(item.into(), |children, item| {
            debug_assert_eq!(children.len(), specs.len());
            let (node, spec) = item.into_parts();
            children.push(node);
            specs.push(spec);
            // No resolved extent may survive a topology change, but retaining capacity keeps the
            // next committed placement allocation-free when the previous capacity is sufficient.
            resolved_main.clear();
            debug_assert_eq!(children.len(), specs.len());
        })
    }

    /// Inserts one node and relationship specification, reconstructing the input on failure.
    #[allow(clippy::result_large_err)]
    pub(in crate::ui_node) fn insert(&mut self, index: usize, item: LinearItem) -> Result<(), LinearItem> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        self.children.try_update_with(item, |children, item| {
            debug_assert_eq!(children.len(), specs.len());
            let (node, spec) = item.into_parts();
            match children.insert(index, node) {
                Ok(()) => {
                    specs.insert(index, spec);
                    resolved_main.clear();
                    debug_assert_eq!(children.len(), specs.len());
                    Ok(())
                }
                Err(node) => Err(LinearItem {
                    node,
                    main: spec.main,
                    fixed_cross: spec.fixed_cross,
                }),
            }
        })?
    }

    /// Drops one indexed child owner and its relationship metadata if that index exists.
    pub(in crate::ui_node) fn remove_drop(&mut self, index: usize) -> Option<bool> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        self.children
            .try_update_with((), |children, ()| {
                debug_assert_eq!(children.len(), specs.len());
                if index >= children.len() {
                    return false;
                }
                let removed = children.remove_drop(index);
                debug_assert!(removed, "linear state validated the child index before removal");
                specs.remove(index);
                resolved_main.clear();
                debug_assert_eq!(children.len(), specs.len());
                true
            })
            .ok()
    }

    /// Drops all child owners and metadata while retaining reusable vector allocations.
    pub(in crate::ui_node) fn clear(&mut self) -> Option<()> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        self.children
            .try_update_with((), |children, ()| {
                children.clear();
                specs.clear();
                resolved_main.clear();
                debug_assert_eq!(children.len(), specs.len());
            })
            .ok()
    }

    /// Replaces the complete child/specification sequence without exposing an intermediate mismatch.
    pub(in crate::ui_node) fn replace<T, I>(&mut self, replacement: I) -> Result<(), I>
    where
        T: Into<LinearItem>,
        I: IntoIterator<Item = T>,
    {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        self.children.try_update_with(replacement, |children, replacement| {
            // Construct both replacement collections before assigning either authoritative field.
            let (replacement, replacement_specs) = collect_items(replacement);
            *children = replacement;
            *specs = replacement_specs;
            resolved_main.clear();
            debug_assert_eq!(children.len(), specs.len());
        })
    }

    /// Returns one child's main-axis relationship when its metadata index exists.
    pub(in crate::ui_node) fn main(&self, index: usize) -> Option<TrackSize> {
        self.specs.get(index).map(|spec| spec.main)
    }

    /// Replaces one main-axis track and discards any resolved placement derived from the old track.
    pub(in crate::ui_node) fn set_main(&mut self, index: usize, track: TrackSize) -> bool {
        let Some(spec) = self.specs.get_mut(index) else {
            return false;
        };
        spec.main = track;
        self.resolved_main.clear();
        true
    }

    /// Returns whether main-axis origins are committed from the trailing edge.
    pub(in crate::ui_node) const fn reversed(&self) -> bool {
        self.reversed
    }

    /// Selects forward or trailing-edge placement without changing item order or sizing.
    pub(in crate::ui_node) fn set_reversed(&mut self, reversed: bool) {
        self.reversed = reversed;
    }

    /// Returns one index-matched specification, defaulting for a release-build mismatch.
    fn spec(&self, index: usize) -> LinearItemSpec {
        self.specs.get(index).copied().unwrap_or(LinearItemSpec {
            main: TrackSize::Content,
            fixed_cross: None,
        })
    }

    /// Checks the only retained parallel-collection invariant at mutation and traversal boundaries.
    fn debug_assert_synchronized(&self, child_count: usize) {
        debug_assert_eq!(
            child_count,
            self.specs.len(),
            "linear child and specification collections must remain index-synchronized"
        );
    }
}

#[derive(Copy, Clone)]
pub(in crate::ui_node) enum LinearAxis {
    Horizontal,
    Vertical,
}

impl LinearAxis {
    fn main_space(self, constraints: Constraints) -> AvailableSpace {
        match self {
            Self::Horizontal => constraints.width,
            Self::Vertical => constraints.height,
        }
    }

    fn cross_space(self, constraints: Constraints) -> AvailableSpace {
        match self {
            Self::Horizontal => constraints.height,
            Self::Vertical => constraints.width,
        }
    }

    fn constraints(self, main: AvailableSpace, cross: AvailableSpace) -> Constraints {
        match self {
            Self::Horizontal => Constraints::new(main, cross),
            Self::Vertical => Constraints::new(cross, main),
        }
    }

    fn main(self, size: Dimensioni) -> i32 {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }

    fn cross(self, size: Dimensioni) -> i32 {
        match self {
            Self::Horizontal => size.height,
            Self::Vertical => size.width,
        }
    }

    fn size(self, main: i32, cross: i32) -> Dimensioni {
        match self {
            Self::Horizontal => Dimensioni::new(main, cross),
            Self::Vertical => Dimensioni::new(cross, main),
        }
    }

    fn main_origin(self, rect: Recti) -> i32 {
        match self {
            Self::Horizontal => rect.x,
            Self::Vertical => rect.y,
        }
    }

    fn cross_origin(self, rect: Recti) -> i32 {
        match self {
            Self::Horizontal => rect.y,
            Self::Vertical => rect.x,
        }
    }

    fn main_extent(self, rect: Recti) -> i32 {
        match self {
            Self::Horizontal => rect.width,
            Self::Vertical => rect.height,
        }
    }

    fn cross_extent(self, rect: Recti) -> i32 {
        match self {
            Self::Horizontal => rect.height,
            Self::Vertical => rect.width,
        }
    }

    fn rect(self, main: i32, cross: i32, main_extent: i32, cross_extent: i32) -> Recti {
        match self {
            Self::Horizontal => Recti::new(main, cross, main_extent, cross_extent),
            Self::Vertical => Recti::new(cross, main, cross_extent, main_extent),
        }
    }
}

/// Measures one orientation without retaining per-pass geometry.
///
/// `row_height` is Row's shared height rule. Column passes `None`: its desired width is content,
/// while placement stretches children to the exact width assigned by Column's parent.
pub(in crate::ui_node) fn measure(
    ctx: &mut MeasureCtx<'_>,
    state: &LinearState,
    orientation: LinearAxis,
    row_height: Option<RowHeight>,
    minimum_cross: i32,
    constraints: Constraints,
) -> Dimensioni {
    let count = ctx.child_count();
    if count == 0 {
        return Dimensioni::default();
    }
    state.debug_assert_synchronized(count);
    let gap = ctx.style().spacing.max(0);
    let main_space = orientation.main_space(constraints);
    let cross_space = orientation.cross_space(constraints);
    let mut resolver = track_resolver_for_measure(ctx, state, orientation, main_space, cross_space, gap);
    let mut cross_content = 0;
    for index in 0..count {
        let placement = state.spec(index);
        let initial = measure_child(ctx, orientation, index, AvailableSpace::Unbounded, cross_space, placement);
        let main = resolver.next(placement.main, orientation.main(initial));
        let child = measure_child(ctx, orientation, index, AvailableSpace::Bounded(main), cross_space, placement);
        cross_content = cross_content.max(placement.fixed_cross.unwrap_or_else(|| orientation.cross(child)).max(0));
    }
    let cross_content = cross_content.max(minimum_cross.max(0));
    let cross = row_height
        .map(|height| resolve_row_height(height, cross_space, cross_content))
        .unwrap_or(cross_content);
    orientation.size(resolver.extent(), cross)
}

fn track_resolver_for_measure(
    ctx: &mut MeasureCtx<'_>,
    state: &LinearState,
    orientation: LinearAxis,
    main_space: AvailableSpace,
    cross_space: AvailableSpace,
    gap: i32,
) -> TrackResolver {
    TrackResolver::new(
        main_space,
        gap,
        ctx.child_count(),
        (0..ctx.child_count()).map(|index| {
            let placement = state.spec(index);
            let child = measure_child(ctx, orientation, index, AvailableSpace::Unbounded, cross_space, placement);
            (placement.main, orientation.main(child))
        }),
    )
}

fn measure_child(
    ctx: &mut MeasureCtx<'_>,
    orientation: LinearAxis,
    index: usize,
    main: AvailableSpace,
    cross: AvailableSpace,
    placement: LinearItemSpec,
) -> Dimensioni {
    let cross = placement.fixed_cross.map(AvailableSpace::Bounded).unwrap_or(cross);
    ctx.measure_child(index, orientation.constraints(main, cross)).unwrap_or_default()
}

/// Resolves and commits one exact linear allocation.
pub(in crate::ui_node) fn place(
    ctx: &mut ContainerLayoutCtx<'_>,
    children: &mut Children,
    state: &mut LinearState,
    orientation: LinearAxis,
    row_height: Option<RowHeight>,
    minimum_cross: i32,
    rect: Recti,
) {
    let count = children.len();
    if count == 0 {
        ctx.set_content_size(Dimensioni::default());
        return;
    }
    state.debug_assert_synchronized(count);
    let gap = ctx.style().spacing.max(0);
    let main_space = AvailableSpace::Bounded(orientation.main_extent(rect).max(0));
    let cross_space = AvailableSpace::Bounded(orientation.cross_extent(rect).max(0));

    // Resolve every main-axis extent once into retained scratch. Only Content needs an intrinsic
    // main-axis measurement under a bounded allocation: Fixed ignores content and Flex divides the
    // remaining bound. The subsequent bounded measurement supplies responsive cross-axis content.
    let specs = &state.specs;
    let resolved_main = &mut state.resolved_main;
    resolved_main.clear();
    for (index, spec) in specs.iter().copied().enumerate() {
        let content = if matches!(spec.main, TrackSize::Content) {
            let desired = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Unbounded, cross_space, spec);
            orientation.main(desired)
        } else {
            0
        };
        resolved_main.push(content.max(0));
    }

    // TrackResolver reads the complete immutable summary before this loop replaces desired content
    // with exact allocated extents. The retained vector then drives both cross measurement and
    // placement, avoiding a second resolver and another intrinsic child-measurement replay.
    let mut resolver = TrackResolver::new(
        main_space,
        gap,
        count,
        specs.iter().zip(resolved_main.iter()).map(|(spec, content)| (spec.main, *content)),
    );
    for (spec, extent) in specs.iter().zip(resolved_main.iter_mut()) {
        *extent = resolver.next(spec.main, *extent);
    }
    let extent = resolver.extent();

    // Responsive children are measured once at their exact main-axis extent. This pass determines
    // the shared Row height or Column overflow width and also warms the exact measurement consumed
    // by the runtime when each child rectangle is committed below.
    let mut cross_content = 0;
    for (index, (spec, main)) in specs.iter().copied().zip(resolved_main.iter().copied()).enumerate() {
        let child = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Bounded(main), cross_space, spec);
        cross_content = cross_content.max(spec.fixed_cross.unwrap_or_else(|| orientation.cross(child)).max(0));
    }
    let cross_content = cross_content.max(minimum_cross.max(0));
    let line_cross = row_height
        .map(|height| resolve_row_height(height, cross_space, cross_content))
        .unwrap_or_else(|| orientation.cross_extent(rect).max(0));

    let reversed = state.reversed;
    let mut cursor = if reversed {
        orientation.main_origin(rect).saturating_add(orientation.main_extent(rect))
    } else {
        orientation.main_origin(rect)
    };
    for (index, (spec, main)) in specs.iter().copied().zip(resolved_main.iter().copied()).enumerate() {
        if reversed {
            cursor = cursor.saturating_sub(main);
        }
        let cross = spec.fixed_cross.unwrap_or(line_cross);
        let child_rect = orientation.rect(cursor, orientation.cross_origin(rect), main, cross);
        let _ = ctx.layout_child(children, index, child_rect);
        if reversed {
            cursor = cursor.saturating_sub(gap);
        } else {
            cursor = cursor.saturating_add(main).saturating_add(gap);
        }
    }
    ctx.set_content_size(orientation.size(extent, line_cross.max(cross_content)));
}

fn measure_layout_child(
    ctx: &mut ContainerLayoutCtx<'_>,
    children: &mut Children,
    orientation: LinearAxis,
    index: usize,
    main: AvailableSpace,
    cross: AvailableSpace,
    placement: LinearItemSpec,
) -> Dimensioni {
    let cross = placement.fixed_cross.map(AvailableSpace::Bounded).unwrap_or(cross);
    ctx.measure_child(children, index, orientation.constraints(main, cross)).unwrap_or_default()
}

/// Resolves the one shared Row line without routing it through weighted sibling allocation.
fn resolve_row_height(height: RowHeight, available: AvailableSpace, content: i32) -> i32 {
    let content = content.max(0);
    match height {
        RowHeight::Content => content,
        RowHeight::Fixed(extent) => extent.max(0),
        // Fill requires a finite parent extent. During intrinsic measurement there is nothing to
        // fill, so the row contributes the same desired height as a content-height row.
        RowHeight::Fill => available.bound().unwrap_or(content).max(0),
    }
}

#[cfg(test)]
mod row_height_tests {
    use super::*;

    #[test]
    fn row_height_exposes_only_content_fixed_and_fill_behavior() {
        // Content remains desired size under either constraint, while Fixed is exact and Fill uses
        // a finite parent extent without manufacturing height for an intrinsic query.
        assert_eq!(resolve_row_height(RowHeight::Content, AvailableSpace::bounded(80), 20), 20);
        assert_eq!(resolve_row_height(RowHeight::Fixed(12), AvailableSpace::bounded(80), 20), 12);
        assert_eq!(resolve_row_height(RowHeight::Fill, AvailableSpace::bounded(80), 20), 80);
        assert_eq!(resolve_row_height(RowHeight::Fill, AvailableSpace::Unbounded, 20), 20);
        assert_eq!(RowHeight::fixed(-7), RowHeight::Fixed(0));
    }
}
