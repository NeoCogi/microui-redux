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

    fn into_parts(self) -> (Node, LinearPlacement) {
        (
            self.node,
            LinearPlacement {
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
struct LinearPlacement {
    main: TrackSize,
    fixed_cross: Option<i32>,
}

/// Index-matched edge metadata kept adjacent to the authoritative child collection.
#[derive(Default)]
struct LinearItems {
    placements: Vec<LinearPlacement>,
}

impl LinearItems {
    fn from_items<T>(items: impl IntoIterator<Item = T>) -> (Children, Self)
    where
        T: Into<LinearItem>,
    {
        let (nodes, placements): (Vec<_>, Vec<_>) = items.into_iter().map(Into::into).map(LinearItem::into_parts).unzip();
        let children: Children = nodes.into_iter().collect();
        let result = Self { placements };
        result.debug_assert_synchronized(children.len());
        (children, result)
    }

    fn placement(&self, index: usize) -> LinearPlacement {
        self.placements.get(index).copied().unwrap_or(LinearPlacement {
            main: TrackSize::Content,
            fixed_cross: None,
        })
    }

    fn push(&mut self, children: &mut Children, item: LinearItem) {
        self.debug_assert_synchronized(children.len());
        let (node, placement) = item.into_parts();
        children.push(node);
        self.placements.push(placement);
        self.debug_assert_synchronized(children.len());
    }

    #[allow(clippy::result_large_err)] // Failure preserves the unique node and its edge metadata.
    fn insert(&mut self, children: &mut Children, index: usize, item: LinearItem) -> Result<(), LinearItem> {
        self.debug_assert_synchronized(children.len());
        let (node, placement) = item.into_parts();
        match children.insert(index, node) {
            Ok(()) => {
                self.placements.insert(index, placement);
                self.debug_assert_synchronized(children.len());
                Ok(())
            }
            Err(node) => Err(LinearItem {
                node,
                main: placement.main,
                fixed_cross: placement.fixed_cross,
            }),
        }
    }

    fn remove_drop(&mut self, children: &mut Children, index: usize) -> bool {
        self.debug_assert_synchronized(children.len());
        if index >= children.len() {
            return false;
        }
        let removed = children.remove_drop(index);
        debug_assert!(removed, "LinearItems validated the child index before removal");
        self.placements.remove(index);
        self.debug_assert_synchronized(children.len());
        true
    }

    fn clear(&mut self, children: &mut Children) {
        children.clear();
        self.placements.clear();
        self.debug_assert_synchronized(children.len());
    }

    fn replace<T>(&mut self, children: &mut Children, items: impl IntoIterator<Item = T>)
    where
        T: Into<LinearItem>,
    {
        let (replacement, metadata) = Self::from_items(items);
        *children = replacement;
        *self = metadata;
    }

    fn set_main(&mut self, index: usize, track: TrackSize) -> bool {
        let Some(placement) = self.placements.get_mut(index) else {
            return false;
        };
        placement.main = track;
        true
    }

    fn debug_assert_synchronized(&self, child_count: usize) {
        debug_assert_eq!(
            child_count,
            self.placements.len(),
            "linear child and placement collections must remain index-synchronized"
        );
    }
}

/// Mounted state shared structurally by Row and Column.
pub(super) struct LinearState {
    children: ChildrenHandle,
    items: LinearItems,
    reversed: bool,
}

impl LinearState {
    pub(super) fn mount<T>(items: impl IntoIterator<Item = T>) -> (Rc<RefCell<Children>>, Self)
    where
        T: Into<LinearItem>,
    {
        let (children, items) = LinearItems::from_items(items);
        let children = Rc::new(RefCell::new(children));
        let state = Self {
            children: ChildrenHandle::new(&children),
            items,
            reversed: false,
        };
        (children, state)
    }

    pub(super) fn len(&self) -> Option<usize> {
        self.children.len()
    }

    pub(super) fn is_empty(&self) -> Option<bool> {
        self.children.is_empty()
    }

    #[allow(clippy::result_large_err)] // Failure preserves the unique node and its edge metadata.
    pub(super) fn push(&mut self, item: impl Into<LinearItem>) -> Result<(), LinearItem> {
        let items = &mut self.items;
        self.children.try_update_with(item.into(), |children, item| items.push(children, item))
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn insert(&mut self, index: usize, item: LinearItem) -> Result<(), LinearItem> {
        let items = &mut self.items;
        self.children.try_update_with(item, |children, item| items.insert(children, index, item))?
    }

    pub(super) fn remove_drop(&mut self, index: usize) -> Option<bool> {
        let items = &mut self.items;
        self.children.try_update_with((), |children, ()| items.remove_drop(children, index)).ok()
    }

    pub(super) fn clear(&mut self) -> Option<()> {
        let items = &mut self.items;
        self.children.try_update_with((), |children, ()| items.clear(children)).ok()
    }

    pub(super) fn replace<T, I>(&mut self, replacement: I) -> Result<(), I>
    where
        T: Into<LinearItem>,
        I: IntoIterator<Item = T>,
    {
        let items = &mut self.items;
        self.children
            .try_update_with(replacement, |children, replacement| items.replace(children, replacement))
    }

    pub(super) fn main(&self, index: usize) -> Option<TrackSize> {
        self.items.placements.get(index).map(|placement| placement.main)
    }

    pub(super) fn set_main(&mut self, index: usize, track: TrackSize) -> bool {
        self.items.set_main(index, track)
    }

    pub(super) const fn reversed(&self) -> bool {
        self.reversed
    }

    pub(super) fn set_reversed(&mut self, reversed: bool) {
        self.reversed = reversed;
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
    state.items.debug_assert_synchronized(count);
    let gap = ctx.style().spacing.max(0);
    let main_space = orientation.main_space(constraints);
    let cross_space = orientation.cross_space(constraints);
    let mut resolver = track_resolver_for_measure(ctx, state, orientation, main_space, cross_space, gap);
    let mut cross_content = 0;
    for index in 0..count {
        let placement = state.items.placement(index);
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
            let placement = state.items.placement(index);
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
    placement: LinearPlacement,
) -> Dimensioni {
    let cross = placement.fixed_cross.map(AvailableSpace::Bounded).unwrap_or(cross);
    ctx.measure_child(index, orientation.constraints(main, cross)).unwrap_or_default()
}

/// Resolves and commits one exact linear allocation.
pub(super) fn layout_linear(
    ctx: &mut ContainerLayoutCtx<'_>,
    children: &mut Children,
    state: &LinearState,
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
    state.items.debug_assert_synchronized(count);
    let gap = ctx.style().spacing.max(0);
    let main_space = AvailableSpace::Bounded(orientation.main_extent(rect).max(0));
    let cross_space = AvailableSpace::Bounded(orientation.cross_extent(rect).max(0));

    let mut resolver = track_resolver_for_layout(ctx, children, state, orientation, main_space, cross_space, gap);
    let mut cross_content = 0;
    for index in 0..count {
        let placement = state.items.placement(index);
        let initial = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Unbounded, cross_space, placement);
        let main = resolver.next(placement.main, orientation.main(initial));
        let child = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Bounded(main), cross_space, placement);
        cross_content = cross_content.max(placement.fixed_cross.unwrap_or_else(|| orientation.cross(child)).max(0));
    }
    let cross_content = cross_content.max(minimum_cross.max(0));
    let line_cross = line_track
        .map(|track| resolve_line_cross(Some(track), cross_space, cross_content))
        .unwrap_or_else(|| orientation.cross_extent(rect).max(0));

    let extent = resolver.extent();
    let mut resolver = track_resolver_for_layout(ctx, children, state, orientation, main_space, cross_space, gap);
    let mut cursor = if state.reversed {
        orientation.main_origin(rect).saturating_add(orientation.main_extent(rect))
    } else {
        orientation.main_origin(rect)
    };
    for index in 0..count {
        let placement = state.items.placement(index);
        let initial = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Unbounded, cross_space, placement);
        let main = resolver.next(placement.main, orientation.main(initial));
        if state.reversed {
            cursor = cursor.saturating_sub(main);
        }
        let cross = placement.fixed_cross.unwrap_or(line_cross);
        let child_rect = orientation.rect(cursor, orientation.cross_origin(rect), main, cross);
        let _ = ctx.layout_child(children, index, child_rect);
        if state.reversed {
            cursor = cursor.saturating_sub(gap);
        } else {
            cursor = cursor.saturating_add(main).saturating_add(gap);
        }
    }
    ctx.set_content_size(orientation.size(extent, line_cross.max(cross_content)));
}

fn track_resolver_for_layout(
    ctx: &mut ContainerLayoutCtx<'_>,
    children: &mut Children,
    state: &LinearState,
    orientation: Orientation,
    main_space: AvailableSpace,
    cross_space: AvailableSpace,
    gap: i32,
) -> TrackResolver {
    TrackResolver::new(
        main_space,
        gap,
        children.len(),
        (0..children.len()).map(|index| {
            let placement = state.items.placement(index);
            let child = measure_layout_child(ctx, children, orientation, index, AvailableSpace::Unbounded, cross_space, placement);
            (placement.main, orientation.main(child))
        }),
    )
}

fn measure_layout_child(
    ctx: &mut ContainerLayoutCtx<'_>,
    children: &mut Children,
    orientation: Orientation,
    index: usize,
    main: AvailableSpace,
    cross: AvailableSpace,
    placement: LinearPlacement,
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
