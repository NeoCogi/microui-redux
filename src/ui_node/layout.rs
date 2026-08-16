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

//! Retained layout contract, parent-owned track resolution, and shared linear layout.
//!
//! # Model overview
//!
//! Layout is deliberately split into three directional stages:
//!
//! ```text
//! constraints flow down -> desired sizes flow up -> exact rectangles flow down
//! ```
//!
//! 1. A parent measures a child with [`Constraints`]. The child reports a desired size; it does
//!    not claim the offered rectangle.
//! 2. The parent combines those desired sizes with its own child relationships, such as
//!    [`TrackSize`], spacing, spans, or chrome.
//! 3. The parent resolves every relationship and assigns each child one exact [`Recti`](crate::Recti)
//!    through [`ContainerLayoutCtx::layout_child`](crate::ContainerLayoutCtx::layout_child).
//!
//! The separation is the central invariant. A measurement bound is information used to answer a
//! responsive size query. It is not an instruction to fill that bound. Conversely, an allocated
//! rectangle is final: the runtime and child never reapply a sizing policy after the parent has
//! resolved it.
//!
//! A sizing relationship therefore belongs to the edge from a container to one of its children,
//! not to [`Node`](crate::Node). The same node may be content-sized in one parent and flexible in
//! another without carrying global layout policy or requiring container-specific runtime dispatch.
//!
//! # Object ownership
//!
//! The layout-relevant retained objects form the following ownership graph. The diagram uses only
//! stored relationships: tree branches are by-value ownership, `==>` is a persistent strong
//! [`Rc`](std::rc::Rc) edge to an independently allocated cell, `-.->` is a non-owning
//! [`Weak`](std::rc::Weak) edge, and `---->` associates parallel data by index without pointing to
//! or owning the object on the right.
//!
//! ```text
//! Context
//! `-- WindowManager
//!     `-- roots: Vec<WindowEntry>
//!         `-- WindowEntry
//!             +-- root_widget: TypedWidgetHandle<RootChrome>
//!             |   `-.-> RootChrome WidgetStorage allocation inside root
//!             `-- tree: WidgetTree
//!                 +-- runtime: UiRuntime
//!                 |   `-- traversal state only; it does not own Node objects
//!                 `-- root: Node
//!                     +-- state: NodeRuntime
//!                     |   +-- layout: NodeLayout
//!                     |   `-- measurement: MeasurementCache
//!                     `-- data: NodeKind (exactly one of the following)
//!                         +-- WidgetNode
//!                         |   `==> Rc<RefCell<WidgetStorage<dyn LeafWidget>>>
//!                         |
//!                         `-- Container
//!                             +==> Rc<RefCell<Children>>
//!                             |        +-- nodes[0]: Node
//!                             |        +-- nodes[1]: Node
//!                             |        `-- ... recursively repeats the Node subtree
//!                             |
//!                             `==> Rc<RefCell<WidgetStorage<dyn ContainerWidget>>>
//!                                          `-- concrete container widget W
//!                                               +-- Row / Column / Grid only
//!                                               |   +-- children: ChildrenHandle
//!                                               |   |   `-.-> same Children cell
//!                                               |   +-- parent-child relationship metadata
//!                                               |   `-- reusable derived layout scratch
//!                                               `-- other containers: widget-specific state
//!
//! Application state (leaf or container):
//! `-- TypedWidgetHandle<W>
//!     `-.-> same WidgetStorage<W> allocation shown above
//!
//! For Row or Column:
//!     LinearState.specs[i] ------------------------> Children.nodes[i]
//!     LinearState.resolved_main[i] -- derived ----> Children.nodes[i]
//!
//! For Grid:
//!     GridItems.spans[i] --------------------------> Children.nodes[i]
//!     GridItems.placements[i] ------- derived ----> Children.nodes[i]
//!     GridLayout.columns / rows = track scratch, not child ownership
//! ```
//!
//! A root `Node` is therefore owned by its `WidgetTree`; every descendant `Node` is owned by
//! exactly one parent [`Children`](crate::Children) collection. [`Node`](crate::Node) is not
//! cloneable, has no parent pointer, and cannot participate in an ownership cycle. Moving an
//! unmounted node into a root or child collection transfers its one owner.
//!
//! A leaf or container `Node` is also the sole persistent strong owner of its widget allocation.
//! The allocation remains concrete as `WidgetStorage<W>` while the node views the same allocation
//! through an erased widget trait object. [`TypedWidgetHandle`](crate::TypedWidgetHandle) stores
//! only `Weak`, so application state can inspect or mutate a live widget but cannot keep a removed
//! widget, its container, or its descendants alive.
//!
//! Containers deliberately separate topology from parent-child relationship metadata. The
//! [`Container`](crate::Container) strongly owns `Children`, while Row, Column, and Grid retain a
//! weak `ChildrenHandle` beside index-matched tracks, spans, placements, and reusable scratch.
//! Topology mutations update the child collection and the parallel metadata in one checked
//! operation. The weak backlink lets typed widget methods perform that operation without forming
//! `Container -> widget -> Children -> Node -> Container` strong cycles.
//!
//! `UiRuntime`, [`MeasureCtx`](crate::MeasureCtx), and
//! [`ContainerLayoutCtx`](crate::ContainerLayoutCtx) only borrow nodes, widget state, and children
//! for the duration of a traversal operation. They never retain those borrows or acquire ownership.
//! `NodeLayout`, the bounded measurement cache, linear resolved extents, Grid placements, and Grid
//! track vectors are derived state owned next to the semantic object that reuses them; none is an
//! alternative tree or a second owner of a child.
//!
//! # Public layout values
//!
//! ## Available space
//!
//! [`AvailableSpace`] represents one axis of a measurement request:
//!
//! - [`AvailableSpace::Unbounded`] asks for intrinsic size without a finite maximum.
//! - [`AvailableSpace::Bounded`] carries a non-negative finite maximum. In particular,
//!   `Bounded(0)` is a real zero-pixel constraint and is not an unbounded sentinel.
//!
//! [`AvailableSpace::bounded`] normalizes negative external values to zero. Insets use
//! [`AvailableSpace::shrink`], which subtracts with saturation from a bounded axis and preserves an
//! unbounded axis. These rules prevent the old class of bugs in which zero, negative values, and
//! unconstrained measurement became indistinguishable.
//!
//! ## Two-dimensional constraints
//!
//! [`Constraints`] stores width and height independently. Either axis may be bounded while the
//! other remains unbounded; text wrapping commonly uses a bounded width with an unbounded height.
//! [`Constraints::unbounded`] requests intrinsic size on both axes, while
//! [`Constraints::bounded`] constructs two finite bounds from a [`Dimensioni`](crate::Dimensioni).
//! The default [`Constraints`] value is unbounded on both axes.
//!
//! Constraints are part of the retained measurement-cache key. Queries that differ on either axis
//! remain distinct even when they happen to produce the same desired size.
//!
//! ## Parent-owned tracks
//!
//! [`TrackSize`] describes one Row, Column, or Grid relationship:
//!
//! - [`TrackSize::Content`] reserves the measured non-negative content extent.
//! - [`TrackSize::Fixed`] reserves an exact non-negative extent. Content may overflow it but cannot
//!   enlarge it.
//! - [`TrackSize::Flex`] receives a weighted share of bounded space left after content, fixed
//!   tracks, and gaps. Under an unbounded constraint it contributes its content extent because
//!   there is no finite remainder to divide.
//!
//! A flex weight participates only when it is finite and strictly positive. Zero, negative,
//! infinite, and NaN weights resolve to zero under a bounded constraint. Public fixed extents and
//! measured content are normalized at use sites, so negative geometry cannot enter placement.
//! The default [`TrackSize`] is Content.
//!
//! [`LinearItem`] is the construction and mutation value that pairs an unmounted node with its
//! parent-owned main-axis track and optional exact cross-axis extent. The concrete Row or Column
//! consumes it, becomes the sole owner of the node, and stores only the relationship metadata.
//! [`RowHeight`] separately describes the one shared Row line: content height, fixed height, or
//! filling a bounded parent height. It intentionally has no weight because one line has no sibling
//! line with which to share space. Its default is content height.
//!
//! # Track-resolution algorithm
//!
//! `TrackResolver` is the private scalar solver shared by linear layout and Grid. It resolves one
//! ordered axis in two passes without retaining the input sequence.
//!
//! Its retained scalar state has one purpose per field: `available` preserves bounded versus
//! unbounded behavior; `flexible_space` stores `F`; `total_flex` stores `W`; `cumulative_flex` and
//! `distributed_flex` advance deterministic replay boundaries; and `extent` stores `E` for the
//! caller's final desired or logical content span.
//!
//! Let:
//!
//! ```text
//! n = number of tracks
//! g = max(requested gap, 0)
//! G = g * max(n - 1, 0)                 total inter-track gap space
//! R = reserved Content and Fixed space
//! W = sum of valid positive Flex weights
//! B = finite bound, when one exists
//! F = max(B - G - R, 0) when W > 0      distributable flex space
//!     0 otherwise
//! E = R + F + G                         resolved axis extent
//! ```
//!
//! Every addition, subtraction, and gap multiplication uses saturating integer arithmetic. This
//! makes extreme public dimensions deterministic rather than allowing debug overflow panics or
//! release wrapping.
//!
//! ## Summary pass
//!
//! Construction walks `(TrackSize, measured_content)` pairs once:
//!
//! - On a bounded axis, Content contributes to `R`, Fixed contributes to `R`, and valid Flex
//!   contributes to `W`.
//! - On an unbounded axis, Content and Flex both contribute their measured content to `R`; Fixed
//!   contributes its explicit extent.
//! - Gaps are counted exactly once from the declared track count, independently of content.
//!
//! A bounded remainder enters `E` only if at least one valid Flex track can own it. Consequently, a
//! content-only sequence measured under 1,000 available pixels still reports its content width,
//! not 1,000 pixels. If content, fixed tracks, and gaps already exceed `B`, `F` becomes zero and
//! `E` remains larger than the bound. That is explicit overflow; earlier tracks are never shrunk
//! and later siblings never overlap merely to hide it.
//!
//! ## Replay pass and pixel rounding
//!
//! Callers replay the same ordered pairs through `TrackResolver::next`. Content and Fixed resolve
//! directly. Unbounded Flex resolves to content. For bounded Flex, the solver uses cumulative
//! weighted boundaries:
//!
//! ```text
//! C_i = sum of valid flex weights through flex track i
//! P_i = ceil(F * C_i / W), clamped to [0, F]
//! size_i = P_i - P_(i-1)
//! ```
//!
//! Cumulative boundaries guarantee that all positive flex tracks sum to exactly `F`; no pixel is
//! created or lost by independently rounding each share. The ceiling deliberately gives
//! indivisible leading pixels to earlier flex tracks, so results are stable and deterministic.
//! Invalid Flex tracks resolve to zero and do not advance the cumulative weight.
//!
//! For example, a 40-pixel bound with two-pixel gaps and tracks
//! `Content(10), Fixed(8), Flex(1)` has `G = 4`, `R = 18`, and `F = 18`, producing
//! `[10, 8, 18]`. Three equal Flex tracks inside 10 pixels produce `[4, 3, 3]`: cumulative
//! rounding gives the leading indivisible pixel to the first track while preserving the exact sum.
//!
//! `TrackResolver::extent` is known after the summary pass, but replay is still required when a
//! caller needs each individual track extent. Callers must replay the same order summarized during
//! construction. The private API keeps that invariant close to Row, Column, and Grid.
//!
//! `resolve_tracks_in_place` is the slice adapter used by Grid. It normalizes a retained content
//! vector, summarizes it, and overwrites each entry with its resolved extent. Missing explicit
//! track metadata defaults to Content, which is also how implicit Grid tracks behave.
//!
//! # Shared linear layout
//!
//! The private `linear` module implements the common algorithm behind [`Row`](crate::Row) and
//! [`Column`](crate::Column). There is intentionally no `Linear` widget. Row and Column are the
//! concrete containers; the module owns their common retained data and axis-independent geometry.
//!
//! ## Retained data
//!
//! Internally, `LinearState` contains:
//!
//! - a weak `ChildrenHandle` used by typed mutation methods;
//! - one `LinearItemSpec` per child, containing its main-axis track and optional fixed cross extent;
//! - a reverse-placement flag used by Column;
//! - `resolved_main`, a reusable vector of exact main-axis extents.
//!
//! The concrete [`Container`](crate::Container) remains the only strong owner of the authoritative
//! [`Children`](crate::Children) collection. Child nodes and specifications are parallel by index.
//! Push, insert, removal, replacement, and clear operations update both collections inside one
//! checked topology closure. Failed insertion reconstructs the exact [`LinearItem`] so unique node
//! ownership is never lost. Debug assertions check synchronization at mutation and traversal
//! boundaries.
//!
//! `LinearAxis` maps width/height and x/y into main/cross operations. The algorithm is therefore
//! written once: Horizontal produces Row geometry and Vertical produces Column geometry. Reverse
//! mode changes only cursor origins and advances; it does not reverse ownership, measurement,
//! traversal, or track resolution.
//!
//! ## Immutable measurement
//!
//! [`ContainerWidget::measure`](crate::ContainerWidget::measure) receives `&self`, so preferred-size
//! measurement cannot mutate retained scratch. For a non-empty linear container it:
//!
//! 1. Reads non-negative style spacing and separates the incoming main and cross constraints.
//! 2. Measures intrinsic main-axis requirements and constructs a scalar `TrackResolver`.
//! 3. Replays each track to obtain its resolved main extent.
//! 4. Measures the child at that exact bounded main extent so responsive content, especially text,
//!    can report the correct cross extent.
//! 5. Takes the maximum cross requirement, applies Row's non-empty control minimum when relevant,
//!    and resolves [`RowHeight`] or Column's content width.
//! 6. Returns the track-sequence extent and resolved cross extent as desired size.
//!
//! Empty linear containers return zero desired size. Repeated intrinsic queries during scalar
//! replay normally hit the child's node-local measurement cache, retaining an allocation-free
//! container algorithm without repeatedly executing leaf measurement.
//!
//! ## Mutable placement
//!
//! Placement receives an exact parent-owned rectangle and mutable retained state:
//!
//! 1. Clear `resolved_main` without releasing its capacity.
//! 2. Measure an intrinsic main extent only for Content tracks. Fixed ignores intrinsic main size,
//!    and bounded Flex depends on the parent's remainder, so measuring those here would be wasted.
//! 3. Construct one `TrackResolver` and overwrite the scratch entries with exact resolved extents.
//! 4. Measure every child once at its exact main extent to determine responsive cross content. If
//!    the offered cross constraint also becomes the child's allocated cross extent—as it does for
//!    Column stretch, Row Fill, and fixed-cross items—the query warms the exact cache entry used
//!    when runtime commits the child rectangle.
//! 5. Resolve the Row line height or use Column's assigned cross extent.
//! 6. Walk the resolved vector once, construct exact child rectangles, and call
//!    [`ContainerLayoutCtx::layout_child`](crate::ContainerLayoutCtx::layout_child).
//! 7. Publish logical content size from the resolved main span and any cross-axis overflow.
//!
//! Forward placement advances from the leading edge by `extent + gap`. Reverse placement begins at
//! the trailing edge, subtracts an extent before placing, then subtracts the gap. Both modes consume
//! the identical resolved vector, so direction cannot change sizing.
//!
//! # Grid integration
//!
//! [`Grid`](crate::Grid) uses the same track resolver on both axes. Grid additionally retains child
//! spans, row-major derived placements, and a reusable occupancy bitmap. Spans are normalized to at
//! least one row and column, and topology mutations rebuild placements before the state is visible.
//!
//! A spanning child's intrinsic deficit is distributed across the non-Fixed tracks in its span.
//! Explicit Fixed tracks remain exact overflow boundaries. Integer deficit remainders go to earlier
//! eligible tracks, matching the system's deterministic leading-pixel convention.
//!
//! Columns resolve before rows because a child's resolved column span is the width constraint used
//! to measure wrapped height. Immutable Grid measurement replays scalar column resolution when a
//! row needs a child's span width. This intentionally trades some cached recomputation for an
//! allocation-free `measure(&self)` path. Mutable placement instead fills retained column and row
//! vectors, resolves each track once, and uses those vectors for every final child rectangle.
//!
//! Grid always has an effective track on each axis, but an implicit empty Content track contributes
//! zero. Empty generic grids therefore remain zero-sized unless explicit Fixed tracks contribute
//! extent.
//!
//! # Runtime integration
//!
//! [`MeasureCtx`](crate::MeasureCtx) exposes indexed child measurement while keeping the retained
//! child collection private. [`ContainerLayoutCtx`](crate::ContainerLayoutCtx) exposes measurement
//! during placement, exact child allocation, content-size publication, child participation, and
//! child viewport configuration. Neither context allows a container to retain node borrows across
//! recursion.
//!
//! The runtime traversal performs the following work around container algorithms:
//!
//! 1. Remove frame insets from constraints before measuring widget content, then add frame geometry
//!    back to the reported outer desired size.
//! 2. Cache each node's desired size by exact constraints, measurement-relevant style fields, and
//!    atlas identity.
//! 3. Before placement, measure under the exact allocated size, normalize the parent's rectangle,
//!    and derive the node-local framed content rectangle.
//! 4. Give containers that exact content rectangle. A container resolves its relationships and
//!    recursively assigns exact child rectangles; runtime never inspects [`TrackSize`].
//! 5. Store committed allocation, descendant viewport/offset, logical content size, and overflow
//!    propagation in the node's private `NodeLayout`.
//! 6. Aggregate visible descendant overflow only when the container permits propagation.
//!
//! Desired size and logical content size are related but different. Desired size is the answer to
//! one constraint query. Logical content size is committed placement output and may exceed the
//! allocation, allowing scrolling and clipping without changing sibling origins.
//!
//! # Allocation and traversal optimizations
//!
//! The implementation favors predictable retained reuse rather than per-pass temporary trees:
//!
//! - `TrackResolver` uses constant auxiliary space. It stores six scalar accumulators rather than
//!   copying track metadata or resolved extents.
//! - Immutable Row, Column, and Grid measurement uses scalar replay and performs no scratch-vector
//!   allocation.
//! - Linear placement reuses `resolved_main`; Grid placement reuses its column and row vectors;
//!   Grid placement rebuilding reuses its occupancy bitmap. Clearing retains capacity.
//! - Each node keeps a bounded four-entry measurement cache. Cache keys include both constraints,
//!   measurement-relevant style values, and atlas identity. Invalidation clears entries but retains
//!   vector capacity.
//! - Exact-main child measurements made while resolving cross-axis content are reused when the
//!   eventual cross allocation carries the same constraint; other responsive probes remain useful
//!   bounded entries in the same node-local cache.
//! - If measurement is cached, allocation is unchanged, and layout is not dirty, runtime skips the
//!   complete placement subtree.
//! - Placement computes every resolved track once before emitting rectangles. It does not rerun
//!   parent sizing rules inside the child runtime.
//!
//! Track resolution is linear in the number of tracks and uses constant temporary space. Linear
//! placement is linear in the number of children with retained `O(n)` output scratch. Grid retains
//! `O(columns + rows)` resolved output; its immutable span measurement may replay track preferences
//! to preserve the no-allocation `&self` contract.
//!
//! # Safety and correctness invariants
//!
//! - Public constructors and internal use sites normalize desired sizes, tracks, gaps, and
//!   allocations to non-negative extents at their ownership boundaries.
//! - Geometry accumulation uses saturating arithmetic.
//! - A bounded axis fills only through a valid Flex track or an explicit Row Fill rule.
//! - Content and Fixed overflow remain visible; Flex collapses to zero when no remainder exists.
//! - Track gaps are counted once and only between tracks.
//! - Parent containers are the sole authority for child rectangles.
//! - Child/specification and child/span collections remain index-synchronized.
//! - Reverse linear direction changes origins only.
//! - Measurement scratch is never required through `&self`; mutable placement scratch retains
//!   capacity and never becomes semantic state.

use crate::Dimensioni;

pub(in crate::ui_node) mod linear;
pub use linear::{LinearItem, RowHeight};

/// Available space on one measurement axis.
///
/// This is deliberately not encoded in a pixel count: a bounded zero-sized surface and an
/// unbounded measurement request are different inputs.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AvailableSpace {
    /// The parent imposes no maximum on this axis.
    Unbounded,
    /// The parent supplies a non-negative maximum extent.
    Bounded(i32),
}

impl AvailableSpace {
    /// Creates a bounded axis, normalizing an invalid negative extent at the public boundary.
    pub const fn bounded(extent: i32) -> Self {
        Self::Bounded(if extent < 0 { 0 } else { extent })
    }

    /// Returns the finite extent when this axis is bounded.
    pub const fn bound(self) -> Option<i32> {
        match self {
            Self::Unbounded => None,
            Self::Bounded(extent) => Some(extent),
        }
    }

    /// Removes a non-negative parent-owned inset while preserving an unbounded axis.
    pub const fn shrink(self, inset: i32) -> Self {
        let inset = if inset < 0 { 0 } else { inset };
        match self {
            Self::Unbounded => Self::Unbounded,
            Self::Bounded(extent) => Self::Bounded(extent.saturating_sub(inset)),
        }
    }
}

/// Independent width and height constraints supplied during measurement.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Constraints {
    /// Horizontal measurement space.
    pub width: AvailableSpace,
    /// Vertical measurement space.
    pub height: AvailableSpace,
}

impl Constraints {
    /// Creates explicit axis constraints.
    pub const fn new(width: AvailableSpace, height: AvailableSpace) -> Self {
        Self { width, height }
    }

    /// Creates an unconstrained preferred-size query.
    pub const fn unbounded() -> Self {
        Self::new(AvailableSpace::Unbounded, AvailableSpace::Unbounded)
    }

    /// Creates a measurement constrained to a non-negative maximum size.
    pub const fn bounded(size: Dimensioni) -> Self {
        Self::new(AvailableSpace::bounded(size.width), AvailableSpace::bounded(size.height))
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Self::unbounded()
    }
}

/// Size of one parent-owned linear or grid track.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum TrackSize {
    /// Uses the measured content extent.
    #[default]
    Content,
    /// Uses an exact non-negative pixel extent.
    Fixed(i32),
    /// Receives a weighted share of bounded space left after content, fixed tracks, and gaps.
    /// Under an unbounded constraint it contributes its measured content extent.
    Flex(f32),
}

/// Replayable resolution of one ordered set of parent-owned tracks.
///
/// Construction summarizes reservations and flex weights without retaining the input sequence.
/// Callers then replay the same `(track, content)` pairs through [`Self::next`]. This keeps
/// immutable measurement allocation-free while giving placement exact per-child extents.
pub(super) struct TrackResolver {
    available: AvailableSpace,
    flexible_space: i32,
    total_flex: f64,
    cumulative_flex: f64,
    distributed_flex: i32,
    extent: i32,
}

impl TrackResolver {
    /// Summarizes an ordered track axis and computes its complete extent including gaps.
    pub(super) fn new(available: AvailableSpace, gap: i32, count: usize, items: impl IntoIterator<Item = (TrackSize, i32)>) -> Self {
        let total_gap = gap.max(0).saturating_mul(count.saturating_sub(1) as i32);
        let mut reserved = 0_i32;
        let mut total_flex = 0.0_f64;
        for (track, content) in items {
            match track {
                TrackSize::Content => reserved = reserved.saturating_add(content.max(0)),
                TrackSize::Fixed(value) => reserved = reserved.saturating_add(value.max(0)),
                TrackSize::Flex(weight) if matches!(available, AvailableSpace::Bounded(_)) => {
                    total_flex += valid_weight(weight);
                }
                TrackSize::Flex(_) => reserved = reserved.saturating_add(content.max(0)),
            }
        }

        // Remaining bounded space belongs to the sequence only when a valid flex track can receive
        // it. Content-only and fixed-only sequences retain their desired extent under larger bounds.
        let flexible_space = if total_flex > 0.0 {
            available
                .bound()
                .map(|bound| bound.saturating_sub(total_gap).saturating_sub(reserved).max(0))
                .unwrap_or(0)
        } else {
            0
        };
        // The summary is exactly the span produced by replay: reserved pixels, distributed flex
        // pixels, and gaps. Invalid flex tracks cannot claim otherwise unowned bounded remainder.
        let extent = reserved.saturating_add(flexible_space).saturating_add(total_gap);
        Self {
            available,
            flexible_space,
            total_flex,
            cumulative_flex: 0.0,
            distributed_flex: 0,
            extent,
        }
    }

    /// Resolves the next replayed track to one exact non-negative extent.
    pub(super) fn next(&mut self, track: TrackSize, content: i32) -> i32 {
        match (self.available, track) {
            (_, TrackSize::Content) => content.max(0),
            (_, TrackSize::Fixed(value)) => value.max(0),
            (AvailableSpace::Unbounded, TrackSize::Flex(_)) => content.max(0),
            (AvailableSpace::Bounded(_), TrackSize::Flex(weight)) => {
                let weight = valid_weight(weight);
                if weight == 0.0 || self.total_flex == 0.0 {
                    return 0;
                }
                self.cumulative_flex += weight;
                // Cumulative ceilings assign indivisible pixels to earlier flex tracks and make
                // the final positive flex track end exactly at `flexible_space`.
                let cumulative_pixels =
                    ((f64::from(self.flexible_space) * self.cumulative_flex / self.total_flex).ceil() as i64).clamp(0, i64::from(self.flexible_space)) as i32;
                let extent = cumulative_pixels.saturating_sub(self.distributed_flex);
                self.distributed_flex = cumulative_pixels;
                extent
            }
        }
    }

    /// Returns the exact resolved track span including gaps.
    pub(super) const fn extent(&self) -> i32 {
        self.extent
    }
}

/// Resolves measured content extents in place and returns their complete track span.
///
/// Grid uses this slice form with retained placement buffers; linear layout replays
/// [`TrackResolver`] directly into its own retained extent buffer.
pub(super) fn resolve_tracks_in_place(available: AvailableSpace, gap: i32, tracks: &[TrackSize], content: &mut [i32]) -> i32 {
    // Normalize caller-owned measurements before the resolver summarizes or overwrites them.
    content.iter_mut().for_each(|extent| *extent = (*extent).max(0));
    let mut resolver = TrackResolver::new(
        available,
        gap,
        content.len(),
        content.iter().enumerate().map(|(index, extent)| (track_at(tracks, index), *extent)),
    );
    for (index, extent) in content.iter_mut().enumerate() {
        *extent = resolver.next(track_at(tracks, index), *extent);
    }
    resolver.extent()
}

/// Returns explicit track metadata or the content-sized default for an omitted index.
fn track_at(tracks: &[TrackSize], index: usize) -> TrackSize {
    tracks.get(index).copied().unwrap_or(TrackSize::Content)
}

/// Converts a finite positive public flex weight into the solver's accumulation precision.
fn valid_weight(weight: f32) -> f64 {
    if weight.is_finite() && weight > 0.0 { f64::from(weight) } else { 0.0 }
}

#[cfg(test)]
mod explicit_constraint_tests {
    use super::*;

    #[test]
    fn bounded_constructor_preserves_zero_and_normalizes_negative_extents() {
        assert_eq!(AvailableSpace::bounded(0), AvailableSpace::Bounded(0));
        assert_eq!(AvailableSpace::bounded(-7), AvailableSpace::Bounded(0));
    }

    #[test]
    fn constraints_keep_axis_bounds_independent() {
        let constraints = Constraints::new(AvailableSpace::Unbounded, AvailableSpace::Bounded(24));
        assert_eq!(constraints.width.bound(), None);
        assert_eq!(constraints.height.bound(), Some(24));
    }
}

#[cfg(test)]
mod track_resolution_tests {
    use super::*;

    #[test]
    fn unbounded_tracks_use_content_except_for_fixed_extents() {
        let mut content = [10, 20, 30, 40];
        let extent = resolve_tracks_in_place(
            AvailableSpace::Unbounded,
            2,
            &[TrackSize::Content, TrackSize::Fixed(7), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [10, 7, 30, 40]);
        assert_eq!(extent, 93);
    }

    #[test]
    fn bounded_tracks_reserve_content_fixed_and_gaps_before_flex() {
        let mut content = [10, 99, 99];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(40),
            2,
            &[TrackSize::Content, TrackSize::Fixed(8), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [10, 8, 18]);
        assert_eq!(extent, 40);
    }

    #[test]
    fn bounded_content_and_fixed_tracks_do_not_claim_unused_space() {
        // A bound informs responsive measurement; without a valid flex recipient it is not itself
        // part of the track sequence's desired size.
        let mut content = [10, 99];
        let extent = resolve_tracks_in_place(AvailableSpace::bounded(100), 2, &[TrackSize::Content, TrackSize::Fixed(8)], &mut content);

        assert_eq!(content, [10, 8]);
        assert_eq!(extent, 20);
    }

    #[test]
    fn invalid_flex_weights_do_not_create_phantom_extent() {
        // Invalid bounded flex tracks resolve to zero. Their surrounding gaps remain real, but the
        // unused bounded remainder has no owner and must stay outside the reported content span.
        let mut content = [10, 20, 30];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(100),
            2,
            &[TrackSize::Content, TrackSize::Flex(0.0), TrackSize::Flex(f32::NAN)],
            &mut content,
        );

        assert_eq!(content, [10, 0, 0]);
        assert_eq!(extent, 14);
    }

    #[test]
    fn rounding_pixels_are_deterministic_and_keep_the_exact_bound() {
        let mut content = [0; 3];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(10),
            0,
            &[TrackSize::Flex(1.0), TrackSize::Flex(1.0), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [4, 3, 3]);
        assert_eq!(extent, 10);
    }

    #[test]
    fn content_overflow_does_not_shrink_or_overlap_tracks() {
        let mut content = [20, 30, 50];
        let extent = resolve_tracks_in_place(
            AvailableSpace::bounded(40),
            3,
            &[TrackSize::Content, TrackSize::Fixed(30), TrackSize::Flex(1.0)],
            &mut content,
        );
        assert_eq!(content, [20, 30, 0]);
        assert_eq!(extent, 56);
    }

    #[test]
    fn bounded_zero_is_not_unbounded() {
        let mut content = [12];
        let extent = resolve_tracks_in_place(AvailableSpace::bounded(0), 0, &[TrackSize::Flex(1.0)], &mut content);
        assert_eq!(content, [0]);
        assert_eq!(extent, 0);
    }
}
