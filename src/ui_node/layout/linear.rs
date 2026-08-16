//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without modification, are permitted
// provided that the conditions in the project LICENSE are met.
//

//! One concrete retained widget for horizontal and vertical linear layout.

use std::{cell::RefCell, rc::Rc};

use crate::ui_node::children::ChildrenHandle;
use crate::{
    AtlasHandle, AvailableSpace, Children, Constraints, Container, ContainerLayoutCtx, ContainerWidget, Dimensioni, MeasureCtx, Node, Recti, Style, TrackSize,
    TypedWidgetHandle, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};

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

/// Leading-edge direction of a one-dimensional retained layout.
///
/// Direction deliberately combines axis and reversal. This prevents the old design from storing a
/// vertical-only `reversed` flag in otherwise axis-neutral retained state, and it makes trailing-edge
/// horizontal layout available without introducing another concrete widget type.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum LinearDirection {
    /// Places child zero at the left edge and advances toward the right.
    #[default]
    LeftToRight,
    /// Places child zero at the right edge and advances toward the left.
    RightToLeft,
    /// Places child zero at the top edge and advances downward.
    TopToBottom,
    /// Places child zero at the bottom edge and advances upward.
    BottomToTop,
}

impl LinearDirection {
    /// Returns whether main-axis geometry uses horizontal coordinates.
    pub const fn is_horizontal(self) -> bool {
        // Match the named directions rather than relying on enum layout, which keeps this semantic
        // query correct if representation or variant order changes later.
        matches!(self, Self::LeftToRight | Self::RightToLeft)
    }

    /// Returns whether placement starts at the allocation's trailing edge.
    pub const fn is_reversed(self) -> bool {
        // Reversal changes only origins and cursor advancement; retained ownership order is stable.
        matches!(self, Self::RightToLeft | Self::BottomToTop)
    }

    /// Returns the opposite leading edge on the same axis.
    pub const fn reversed(self) -> Self {
        // Keep reversal total for every public direction so builders never need an axis-specific
        // boolean or a fallible conversion.
        match self {
            Self::LeftToRight => Self::RightToLeft,
            Self::RightToLeft => Self::LeftToRight,
            Self::TopToBottom => Self::BottomToTop,
            Self::BottomToTop => Self::TopToBottom,
        }
    }

    fn axis(self) -> LinearAxis {
        // The private geometry adapter needs only the axis; leading-edge behavior remains on the
        // public direction and is consulted separately during placement.
        if self.is_horizontal() { LinearAxis::Horizontal } else { LinearAxis::Vertical }
    }
}

/// Shared cross-axis sizing behavior for a [`Linear`] container.
///
/// Cross sizing describes one shared line. Individual children may still request an exact cross
/// extent through [`LinearItem::with_fixed_cross`], but they do not change the line's own desired or
/// assigned size.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum LinearCrossSize {
    /// Uses the largest desired child cross extent and places the line at that extent.
    #[default]
    Content,
    /// Reports desired content during measurement and stretches the line across exact allocation.
    Stretch,
    /// Uses an exact non-negative cross extent and reports larger content as overflow.
    Fixed(i32),
}

impl LinearCrossSize {
    /// Creates an exact cross extent, normalizing negative public input to zero.
    pub const fn fixed(extent: i32) -> Self {
        // Normalize at the named construction boundary while still defending against callers that
        // directly construct `Fixed` with a negative value in the private resolver.
        Self::Fixed(if extent < 0 { 0 } else { extent })
    }
}

/// One-shot construction input for a [`Linear`] container.
///
/// Horizontal construction defaults to content cross sizing, matching an ordinary control row.
/// Vertical construction defaults to stretch, so children receive the exact width assigned by the
/// parent while the container still reports its widest desired child during measurement.
pub struct LinearParameters {
    items: Vec<LinearItem>,
    direction: LinearDirection,
    cross_size: LinearCrossSize,
}

impl WidgetParameters for LinearParameters {}

impl LinearParameters {
    /// Creates a left-to-right sequence with content-derived line height.
    pub fn horizontal<T>(items: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<LinearItem>,
    {
        // Consume each unique unmounted node exactly once into construction input; ownership is not
        // transferred to retained `Children` until `Linear::create` succeeds.
        Self {
            items: items.into_iter().map(Into::into).collect(),
            direction: LinearDirection::LeftToRight,
            cross_size: LinearCrossSize::Content,
        }
    }

    /// Creates a top-to-bottom sequence that stretches children across its assigned width.
    pub fn vertical<T>(items: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<LinearItem>,
    {
        // Vertical defaults encode the former Column behavior using the same cross-size vocabulary
        // available to every direction.
        Self {
            items: items.into_iter().map(Into::into).collect(),
            direction: LinearDirection::TopToBottom,
            cross_size: LinearCrossSize::Stretch,
        }
    }

    /// Replaces the complete direction without changing item order or sizing metadata.
    pub const fn with_direction(mut self, direction: LinearDirection) -> Self {
        // Track extents remain main-axis values and are intentionally reinterpreted on the new axis.
        self.direction = direction;
        self
    }

    /// Selects the opposite leading edge while preserving the current axis.
    pub const fn reversed(mut self) -> Self {
        // Store the complete resulting direction so the retained widget has no second reverse flag.
        self.direction = self.direction.reversed();
        self
    }

    /// Replaces the shared cross-axis sizing behavior.
    pub const fn with_cross_size(mut self, cross_size: LinearCrossSize) -> Self {
        // This is one-shot input, so changing the policy cannot invalidate retained measurement yet.
        self.cross_size = cross_size;
        self
    }

    /// Stretches the shared line across its exact allocated cross extent.
    pub const fn stretch_cross(self) -> Self {
        // Delegate to the general setter so constructor vocabulary and stored policy cannot diverge.
        self.with_cross_size(LinearCrossSize::Stretch)
    }

    /// Uses an exact non-negative shared cross extent.
    pub const fn fixed_cross(self, extent: i32) -> Self {
        // Normalize through the public value constructor before storing the policy.
        self.with_cross_size(LinearCrossSize::fixed(extent))
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

/// Complete retained widget for one-dimensional child measurement and placement.
///
/// `specs` is the only metadata collection and is index-matched to the concrete container's
/// authoritative `Children`. `resolved_main` is mutable placement scratch: it retains capacity
/// between commits but is never read by immutable measurement or exposed as semantic state.
pub struct Linear {
    children: ChildrenHandle,
    specs: Vec<LinearItemSpec>,
    direction: LinearDirection,
    cross_size: LinearCrossSize,
    resolved_main: Vec<i32>,
}

impl Linear {
    /// Mounts ordered input and returns both a weak typed widget handle and its owning node.
    pub fn create(parameters: LinearParameters) -> (TypedWidgetHandle<Self>, Node) {
        // Split the one-shot parameters only at the ownership boundary: the generic Container keeps
        // the strong child collection while Linear keeps its weak topology mutation capability.
        let (children, widget) = Self::mount(parameters);
        let (handle, container) = Container::from_shared(children, widget);
        (handle, Node::container(container))
    }

    /// Mounts ordered items for the concrete widget and transitional built-in adapters.
    pub(in crate::ui_node) fn mount(parameters: LinearParameters) -> (Rc<RefCell<Children>>, Self) {
        // Build children and relationship metadata together before either can become observable.
        let (children, specs) = collect_items(parameters.items);
        let children = Rc::new(RefCell::new(children));
        let widget = Self {
            children: ChildrenHandle::new(&children),
            specs,
            direction: parameters.direction,
            cross_size: parameters.cross_size,
            resolved_main: Vec::new(),
        };
        widget.debug_assert_synchronized(children.borrow().len());
        (children, widget)
    }

    /// Returns the mounted child count while the concrete collection is available.
    pub fn len(&self) -> Option<usize> {
        // The weak capability makes node lifetime and active traversal borrows visible as absence.
        self.children.len()
    }

    /// Returns whether the mounted collection is empty while topology is available.
    pub fn is_empty(&self) -> Option<bool> {
        // Reuse the handle's checked read so liveness and borrow-conflict behavior stays consistent.
        self.children.is_empty()
    }

    /// Appends one node and its relationship metadata as one observable topology mutation.
    #[allow(clippy::result_large_err)] // Failure preserves the unique node and its edge metadata.
    pub fn push(&mut self, item: impl Into<LinearItem>) -> Result<(), LinearItem> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        // Acquire the authoritative collection before consuming the unique input, then update both
        // parallel collections inside one checked borrow.
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
    pub fn insert(&mut self, index: usize, item: LinearItem) -> Result<(), LinearItem> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        // The closure returns a reconstructed LinearItem when the concrete collection rejects the
        // index, preserving unique node ownership all the way back to the caller.
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
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        // Keep the child and metadata removals inside the same topology borrow so traversal can
        // never observe one collection after only half of the mutation.
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
    pub fn clear(&mut self) -> Option<()> {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        // Clear vectors rather than replacing them so a subsequent warm layout can reuse capacity.
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
    pub fn replace<T, I>(&mut self, replacement: I) -> Result<(), I>
    where
        T: Into<LinearItem>,
        I: IntoIterator<Item = T>,
    {
        let specs = &mut self.specs;
        let resolved_main = &mut self.resolved_main;
        // Do not advance a lazy replacement iterator until the authoritative topology borrow has
        // succeeded; failure therefore returns the exact unconsumed iterator.
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
    pub fn track(&self, index: usize) -> Option<TrackSize> {
        // Specifications are immutable during traversal, so an ordinary indexed read is sufficient.
        self.specs.get(index).map(|spec| spec.main)
    }

    /// Replaces one main-axis track and discards any resolved placement derived from the old track.
    pub fn set_track(&mut self, index: usize, track: TrackSize) -> bool {
        let Some(spec) = self.specs.get_mut(index) else {
            return false;
        };
        // Retained exact extents are placement scratch derived from the old track and cannot survive
        // a semantic mutation, although their vector capacity remains reusable.
        spec.main = track;
        self.resolved_main.clear();
        true
    }

    /// Returns the complete main-axis direction.
    pub const fn direction(&self) -> LinearDirection {
        // Return the value by copy; callers never receive access to retained placement scratch.
        self.direction
    }

    /// Replaces direction without changing item order or parent-owned tracks.
    pub fn set_direction(&mut self, direction: LinearDirection) {
        // Resolved main extents are scalar sizes and could be reused across a simple reversal, but an
        // axis change reinterprets them. Clearing consistently keeps both changes unambiguous.
        self.direction = direction;
        self.resolved_main.clear();
    }

    /// Returns the shared cross-axis sizing behavior.
    pub const fn cross_size(&self) -> LinearCrossSize {
        // Cross policy is semantic state and contains no reference to the child collection.
        self.cross_size
    }

    /// Replaces the shared cross-axis sizing behavior.
    pub fn set_cross_size(&mut self, cross_size: LinearCrossSize) {
        // Cross sizing does not alter main extents, but clear scratch so every geometry-affecting
        // public mutation has the same conservative invalidation boundary.
        self.cross_size = cross_size;
        self.resolved_main.clear();
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

/// Measures the retained direction and cross policy without mutating per-pass geometry.
pub(in crate::ui_node) fn measure(ctx: &mut MeasureCtx<'_>, state: &Linear, constraints: Constraints) -> Dimensioni {
    // Direction is retained semantic state, so private callers cannot accidentally pair horizontal
    // geometry with a vertical widget or pass a Row-only optional policy.
    let orientation = state.direction.axis();
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
    let cross_content = cross_content.max(minimum_content_cross(state.direction, ctx.style(), ctx.atlas()));
    let cross = resolve_measured_cross(state.cross_size, cross_content);
    orientation.size(resolver.extent(), cross)
}

fn track_resolver_for_measure(
    ctx: &mut MeasureCtx<'_>,
    state: &Linear,
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
pub(in crate::ui_node) fn place(ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, state: &mut Linear, rect: Recti) {
    // Snapshot axis and policy before mutably borrowing scratch fields below; these copied values
    // also make it explicit that one retained configuration drives the complete placement pass.
    let orientation = state.direction.axis();
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
    let cross_content = cross_content.max(minimum_content_cross(state.direction, ctx.style(), ctx.atlas()));
    let line_cross = resolve_placed_cross(state.cross_size, orientation.cross_extent(rect), cross_content);

    let reversed = state.direction.is_reversed();
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

impl ContainerWidget for Linear {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        // Route the public widget contract through the scalar immutable pass; placement scratch is
        // intentionally unavailable through this shared reference.
        measure(ctx, self, constraints)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // The generic Container lends its authoritative child collection only for this call, while
        // Linear supplies the index-matched relationship metadata and reusable exact extents.
        place(ctx, children, self, rect);
    }
}

impl Widget for Linear {
    fn widget_opt(&self) -> &WidgetOption {
        // Linear is a geometry-only branch surface; interaction belongs to retained descendants.
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // Linear owns no event-driven semantic state, so runtime update traversal only visits its
        // descendants after this intentionally empty surface callback.
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        // The container emits no drawing commands of its own; child widgets paint their allocations.
    }
}

/// Returns the style-derived cross-axis minimum for an ordinary horizontal control line.
fn minimum_content_cross(direction: LinearDirection, style: &Style, atlas: &AtlasHandle) -> i32 {
    // Vertical sequences have no manufactured minimum width. Horizontal sequences retain the
    // established control-row convention of one font line plus symmetric style padding.
    if !direction.is_horizontal() {
        return 0;
    }
    let padding = style.padding.max(0);
    (atlas.get_font_height(style.font) as i32)
        .saturating_add(padding.saturating_mul(2))
        .max(padding.saturating_mul(2))
}

/// Resolves desired cross size without treating a finite constraint as an allocation.
fn resolve_measured_cross(cross_size: LinearCrossSize, content: i32) -> i32 {
    let content = content.max(0);
    match cross_size {
        // Stretch affects only exact placement. During measurement it reports the same desired
        // content as Content, preserving the desired-size/allocation boundary.
        LinearCrossSize::Content | LinearCrossSize::Stretch => content,
        LinearCrossSize::Fixed(extent) => extent.max(0),
    }
}

/// Resolves the shared line extent for one exact placement allocation.
fn resolve_placed_cross(cross_size: LinearCrossSize, allocated: i32, content: i32) -> i32 {
    let content = content.max(0);
    match cross_size {
        LinearCrossSize::Content => content,
        LinearCrossSize::Stretch => allocated.max(0),
        LinearCrossSize::Fixed(extent) => extent.max(0),
    }
}

#[cfg(test)]
mod cross_size_tests {
    use super::*;

    #[test]
    fn cross_size_separates_desired_measurement_from_exact_placement() {
        // Stretch remains content-sized during measurement and consumes only an exact allocation;
        // Fixed is exact in both phases and its named constructor normalizes hostile input.
        assert_eq!(resolve_measured_cross(LinearCrossSize::Content, 20), 20);
        assert_eq!(resolve_measured_cross(LinearCrossSize::Stretch, 20), 20);
        assert_eq!(resolve_placed_cross(LinearCrossSize::Content, 80, 20), 20);
        assert_eq!(resolve_placed_cross(LinearCrossSize::Stretch, 80, 20), 80);
        assert_eq!(resolve_measured_cross(LinearCrossSize::Fixed(12), 20), 12);
        assert_eq!(resolve_placed_cross(LinearCrossSize::Fixed(12), 80, 20), 12);
        assert_eq!(LinearCrossSize::fixed(-7), LinearCrossSize::Fixed(0));
    }
}
