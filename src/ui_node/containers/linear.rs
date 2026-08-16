//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without modification, are permitted
// provided that the conditions in the project LICENSE are met.
//

//! One parent-owned model shared by horizontal rows and vertical columns.

use std::{cell::RefCell, rc::Rc};

use crate::ui_node::children::ChildrenHandle;
use crate::{AvailableSpace, Children, Constraints, ContainerLayoutCtx, Dimensioni, MeasureCtx, Node, Recti, TrackSize};

use super::tracks::TrackResolver;

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

    fn into_parts(self) -> (Node, LinearItemLayout) {
        (
            self.node,
            LinearItemLayout {
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
struct LinearItemLayout {
    main: TrackSize,
    fixed_cross: Option<i32>,
}

/// Collects construction or replacement input into the runtime's split ownership representation.
///
/// The concrete container remains the sole strong owner of `Children`; linear state retains only
/// the index-matched relationship metadata that Row or Column interprets. Building both outputs in
/// one function keeps their order and length identical before either becomes observable.
fn collect_items<T>(items: impl IntoIterator<Item = T>) -> (Children, Vec<LinearItemLayout>)
where
    T: Into<LinearItem>,
{
    let mut children = Children::new();
    let mut layouts = Vec::new();
    for item in items {
        // Split each unique unmounted owner exactly once, preserving iterator order in both
        // adjacent collections.
        let (node, layout) = item.into().into_parts();
        children.push(node);
        layouts.push(layout);
    }
    debug_assert_eq!(children.len(), layouts.len());
    (children, layouts)
}

/// Complete retained linear state shared structurally by Row and Column.
///
/// `layouts` is the only metadata collection and is index-matched to the concrete container's
/// authoritative `Children`. `resolved_main` is mutable placement scratch: it retains capacity
/// between commits but is never read by immutable measurement or exposed as semantic state.
pub(super) struct LinearState {
    children: ChildrenHandle,
    layouts: Vec<LinearItemLayout>,
    reversed: bool,
    resolved_main: Vec<i32>,
}

impl LinearState {
    /// Mounts ordered linear items and installs one weak topology capability beside their layouts.
    pub(super) fn mount<T>(items: impl IntoIterator<Item = T>) -> (Rc<RefCell<Children>>, Self)
    where
        T: Into<LinearItem>,
    {
        let (children, layouts) = collect_items(items);
        let children = Rc::new(RefCell::new(children));
        let state = Self {
            children: ChildrenHandle::new(&children),
            layouts,
            reversed: false,
            resolved_main: Vec::new(),
        };
        state.debug_assert_synchronized(children.borrow().len());
        (children, state)
    }

    /// Returns the mounted child count while the concrete collection is available.
    pub(super) fn len(&self) -> Option<usize> {
        self.children.len()
    }

    /// Returns whether the mounted collection is empty while topology is available.
    pub(super) fn is_empty(&self) -> Option<bool> {
        self.children.is_empty()
    }

    /// Appends one node and its relationship metadata as one observable topology mutation.
    #[allow(clippy::result_large_err)] // Failure preserves the unique node and its edge metadata.
    pub(super) fn push(&mut self, item: impl Into<LinearItem>) -> Result<(), LinearItem> {
        let layouts = &mut self.layouts;
        let resolved_main = &mut self.resolved_main;
        self.children.try_update_with(item.into(), |children, item| {
            debug_assert_eq!(children.len(), layouts.len());
            let (node, layout) = item.into_parts();
            children.push(node);
            layouts.push(layout);
            // No resolved extent may survive a topology change, but retaining capacity keeps the
            // next committed placement allocation-free when the previous capacity is sufficient.
            resolved_main.clear();
            debug_assert_eq!(children.len(), layouts.len());
        })
    }

    /// Inserts one node and layout together, reconstructing the exact input if insertion fails.
    #[allow(clippy::result_large_err)]
    pub(super) fn insert(&mut self, index: usize, item: LinearItem) -> Result<(), LinearItem> {
        let layouts = &mut self.layouts;
        let resolved_main = &mut self.resolved_main;
        self.children.try_update_with(item, |children, item| {
            debug_assert_eq!(children.len(), layouts.len());
            let (node, layout) = item.into_parts();
            match children.insert(index, node) {
                Ok(()) => {
                    layouts.insert(index, layout);
                    resolved_main.clear();
                    debug_assert_eq!(children.len(), layouts.len());
                    Ok(())
                }
                Err(node) => Err(LinearItem {
                    node,
                    main: layout.main,
                    fixed_cross: layout.fixed_cross,
                }),
            }
        })?
    }

    /// Drops one indexed child owner and its relationship metadata if that index exists.
    pub(super) fn remove_drop(&mut self, index: usize) -> Option<bool> {
        let layouts = &mut self.layouts;
        let resolved_main = &mut self.resolved_main;
        self.children
            .try_update_with((), |children, ()| {
                debug_assert_eq!(children.len(), layouts.len());
                if index >= children.len() {
                    return false;
                }
                let removed = children.remove_drop(index);
                debug_assert!(removed, "linear state validated the child index before removal");
                layouts.remove(index);
                resolved_main.clear();
                debug_assert_eq!(children.len(), layouts.len());
                true
            })
            .ok()
    }

    /// Drops all child owners and metadata while retaining reusable vector allocations.
    pub(super) fn clear(&mut self) -> Option<()> {
        let layouts = &mut self.layouts;
        let resolved_main = &mut self.resolved_main;
        self.children
            .try_update_with((), |children, ()| {
                children.clear();
                layouts.clear();
                resolved_main.clear();
                debug_assert_eq!(children.len(), layouts.len());
            })
            .ok()
    }

    /// Replaces the complete child/layout sequence without exposing an intermediate mismatch.
    pub(super) fn replace<T, I>(&mut self, replacement: I) -> Result<(), I>
    where
        T: Into<LinearItem>,
        I: IntoIterator<Item = T>,
    {
        let layouts = &mut self.layouts;
        let resolved_main = &mut self.resolved_main;
        self.children.try_update_with(replacement, |children, replacement| {
            // Construct both replacement collections before assigning either authoritative field.
            let (replacement, replacement_layouts) = collect_items(replacement);
            *children = replacement;
            *layouts = replacement_layouts;
            resolved_main.clear();
            debug_assert_eq!(children.len(), layouts.len());
        })
    }

    /// Returns one child's main-axis relationship when its metadata index exists.
    pub(super) fn main(&self, index: usize) -> Option<TrackSize> {
        self.layouts.get(index).map(|layout| layout.main)
    }

    /// Replaces one main-axis track and discards any resolved placement derived from the old track.
    pub(super) fn set_main(&mut self, index: usize, track: TrackSize) -> bool {
        let Some(layout) = self.layouts.get_mut(index) else {
            return false;
        };
        layout.main = track;
        self.resolved_main.clear();
        true
    }

    /// Returns whether main-axis origins are committed from the trailing edge.
    pub(super) const fn reversed(&self) -> bool {
        self.reversed
    }

    /// Selects forward or trailing-edge placement without changing item order or sizing.
    pub(super) fn set_reversed(&mut self, reversed: bool) {
        self.reversed = reversed;
    }

    /// Returns one index-matched layout, defaulting defensively for a release-build mismatch.
    fn layout(&self, index: usize) -> LinearItemLayout {
        self.layouts.get(index).copied().unwrap_or(LinearItemLayout {
            main: TrackSize::Content,
            fixed_cross: None,
        })
    }

    /// Checks the only retained parallel-collection invariant at mutation and traversal boundaries.
    fn debug_assert_synchronized(&self, child_count: usize) {
        debug_assert_eq!(
            child_count,
            self.layouts.len(),
            "linear child and layout collections must remain index-synchronized"
        );
    }
}

#[derive(Copy, Clone)]
pub(super) enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
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
/// `line_track` is Row's shared height rule. Column passes `None`: its desired width is content,
/// while placement stretches children to the exact width assigned by Column's parent.
pub(super) fn measure_linear(
    ctx: &mut MeasureCtx<'_>,
    state: &LinearState,
    orientation: Orientation,
    line_track: Option<TrackSize>,
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
        let placement = state.layout(index);
        let initial = measure_child(ctx, orientation, index, AvailableSpace::Unbounded, cross_space, placement);
        let main = resolver.next(placement.main, orientation.main(initial));
        let child = measure_child(ctx, orientation, index, AvailableSpace::Bounded(main), cross_space, placement);
        cross_content = cross_content.max(placement.fixed_cross.unwrap_or_else(|| orientation.cross(child)).max(0));
    }
    let cross = resolve_line_cross(line_track, cross_space, cross_content.max(minimum_cross.max(0)));
    orientation.size(resolver.extent(), cross)
}

fn track_resolver_for_measure(
    ctx: &mut MeasureCtx<'_>,
    state: &LinearState,
    orientation: Orientation,
    main_space: AvailableSpace,
    cross_space: AvailableSpace,
    gap: i32,
) -> TrackResolver {
    TrackResolver::new(
        main_space,
        gap,
        ctx.child_count(),
        (0..ctx.child_count()).map(|index| {
            let placement = state.layout(index);
            let child = measure_child(ctx, orientation, index, AvailableSpace::Unbounded, cross_space, placement);
            (placement.main, orientation.main(child))
        }),
    )
}

fn measure_child(
    ctx: &mut MeasureCtx<'_>,
    orientation: Orientation,
    index: usize,
    main: AvailableSpace,
    cross: AvailableSpace,
    placement: LinearItemLayout,
) -> Dimensioni {
    let cross = placement.fixed_cross.map(AvailableSpace::Bounded).unwrap_or(cross);
    ctx.measure_child(index, orientation.constraints(main, cross)).unwrap_or_default()
}

/// Resolves and commits one exact linear allocation.
pub(super) fn layout_linear(
    ctx: &mut ContainerLayoutCtx<'_>,
    children: &mut Children,
    state: &mut LinearState,
    orientation: Orientation,
    line_track: Option<TrackSize>,
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
    let layouts = &state.layouts;
    let resolved_main = &mut state.resolved_main;
    resolved_main.clear();
    for (index, layout) in layouts.iter().copied().enumerate() {
        let content = if matches!(layout.main, TrackSize::Content) {
            let desired = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Unbounded, cross_space, layout);
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
        layouts.iter().zip(resolved_main.iter()).map(|(layout, content)| (layout.main, *content)),
    );
    for (layout, extent) in layouts.iter().zip(resolved_main.iter_mut()) {
        *extent = resolver.next(layout.main, *extent);
    }
    let extent = resolver.extent();

    // Responsive children are measured once at their exact main-axis extent. This pass determines
    // the shared Row height or Column overflow width and also warms the exact measurement consumed
    // by the runtime when each child rectangle is committed below.
    let mut cross_content = 0;
    for (index, (layout, main)) in layouts.iter().copied().zip(resolved_main.iter().copied()).enumerate() {
        let child = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Bounded(main), cross_space, layout);
        cross_content = cross_content.max(layout.fixed_cross.unwrap_or_else(|| orientation.cross(child)).max(0));
    }
    let cross_content = cross_content.max(minimum_cross.max(0));
    let line_cross = line_track
        .map(|track| resolve_line_cross(Some(track), cross_space, cross_content))
        .unwrap_or_else(|| orientation.cross_extent(rect).max(0));

    let reversed = state.reversed;
    let mut cursor = if reversed {
        orientation.main_origin(rect).saturating_add(orientation.main_extent(rect))
    } else {
        orientation.main_origin(rect)
    };
    for (index, (layout, main)) in layouts.iter().copied().zip(resolved_main.iter().copied()).enumerate() {
        if reversed {
            cursor = cursor.saturating_sub(main);
        }
        let cross = layout.fixed_cross.unwrap_or(line_cross);
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
    orientation: Orientation,
    index: usize,
    main: AvailableSpace,
    cross: AvailableSpace,
    placement: LinearItemLayout,
) -> Dimensioni {
    let cross = placement.fixed_cross.map(AvailableSpace::Bounded).unwrap_or(cross);
    ctx.measure_child(children, index, orientation.constraints(main, cross)).unwrap_or_default()
}

fn resolve_line_cross(track: Option<TrackSize>, available: AvailableSpace, content: i32) -> i32 {
    let Some(track) = track else {
        return content.max(0);
    };
    let mut resolver = TrackResolver::new(available, 0, 1, [(track, content)]);
    resolver.next(track, content)
}
