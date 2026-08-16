# Layout redesign

This document is the working record for replacing the inherited microui sizing rules with one
retained layout contract. It is updated in the same commits as the implementation.

## Goals

- [x] A constraint has an explicit bounded or unbounded state; zero is an ordinary bound.
- [x] Measurement reports content requirements. Allocation assigns an exact rectangle.
- [x] The parent is the only owner of a child's slot size.
- [x] Allocation never reapplies a sizing rule already resolved by the parent.
- [x] Row and Column use one linear algorithm with no container-specific branches in the runtime.
- [x] Grid uses the same track solver as linear layout.
- [x] Scrollbars are selected from child measurements and content is allocated once.
- [x] Empty Row, Column, and Grid containers have zero intrinsic size unless they contain explicit
      fixed tracks.
- [x] Linear and Grid overflow is explicit and never changes sibling placement.
- [ ] The existing `demo-full` geometry and appearance are preserved intentionally. Representative
      exact geometry is checked; final visual comparison remains.
- [x] Warm retained layout remains allocation-free after correctness and clarity are established.

## Non-goals

- [x] Do not implement CSS Flexbox or CSS Grid.
- [x] Do not add speculative alignment, minimum-size, baseline, or absolute-positioning APIs.
- [x] Do not add hooks for ScrollArea, root chrome, or another built-in to general traits.
- [x] Do not preserve the pre-1.0 layout API when a smaller contract is clearer.

## Contract

The complete general model is:

```text
constraints down -> desired size up -> exact rectangles down
```

The types must keep those stages distinct:

```rust
enum AvailableSpace {
    Bounded(i32),
    Unbounded,
}

struct Constraints {
    width: AvailableSpace,
    height: AvailableSpace,
}

enum TrackSize {
    Content,
    Fixed(i32),
    Flex(f32),
}
```

`TrackSize` belongs to a parent-child relationship. It is not stored on `Node`, and it is not
interpreted by the runtime. A container resolves tracks, then gives the runtime exact child
rectangles.

### Track rules

For a bounded axis:

1. Remove inter-track gaps once.
2. Resolve `Content` from the child's desired extent and `Fixed` from its value.
3. Divide non-negative remaining space among `Flex` tracks by weight.
4. Give indivisible pixels to earlier flex tracks so the result is deterministic.
5. If content and fixed tracks exceed the bound, keep their sizes and expose overflow. Flex tracks
   receive zero; siblings never overlap merely because the parent is too small.

For an unbounded axis, every non-fixed track contributes its content extent. Unbounded measurement
does not manufacture space from a remembered rectangle or an arbitrary probe.

### Linear items

Row and Column are orientation-specific constructors over one linear implementation. Each child has
one main-axis `TrackSize`; children fill the cross axis unless an explicit fixed cross extent is
needed by an existing interface. Reverse direction affects origins only, never sizing.

The former vertical `Stack` duplicated Column. Its uses moved to Column; the name was not retained
for a non-overlapping layout.

The public linear syntax attaches sizing to the parent-child edge where it is interpreted:

```rust
RowParameters::new(
    TrackSize::Content,
    [
        LinearItem::fixed(label, 86),
        LinearItem::flex(body, 1.0),
        LinearItem::fixed(action, 109),
    ],
)
```

Plain `Node` values still mean `LinearItem::content(node)`. There is no node-global fallback and no
runtime dispatch on the Row or Column type.

### Legacy policy migration

```text
Auto                 -> Content
Fixed(n)             -> Fixed(n)
Weight(w)            -> Flex(w)
Remainder(0)         -> Flex(1)
Remainder(m), last+1 -> Flex(1), followed by Fixed(m)
Fraction(a) + fill   -> proportional Flex weights
```

There is no general `Remainder(m)` replacement because it encodes the size of later siblings. Each
call site must state that relationship directly.

The migration table is historical only. `Policy` and `SizePolicy` no longer exist in the public API
or on `Node`.

### Composite rules

Composites use the same contract without adding cases to `Node`, `ContainerWidget`, or the runtime:

```text
Disclosure: measure header + optional body -> assign two exact role rectangles
RootChrome: subtract chrome from constraints -> measure body -> assign exact body rectangle
ScrollArea: measure candidate viewports -> choose bars -> configure bars -> place surface once
ScrollSurface: desired content -> max(view width, desired width) x desired height
```

Disclosure body inputs are `LinearItem` values because the private body is an ordinary Column. The
relationship does not leak onto the disclosure node or require a disclosure-specific runtime hook.

## Work log

### Phase 1: characterize and specify

- [x] Created branch `layout-model-cleanup` from `extract-window-manager`.
- [x] Recorded the target contract and explicit non-goals.
- [x] Added direct tests for bounded, unbounded, overflowing, and rounded track allocation.
- [x] Recorded exact rectangles for the three-button remainder row, calculator
      fraction/remainder column, and the log panel's bottom-margin stack before changing behavior.

### Phase 2: core sizing

- [x] Introduced `AvailableSpace`, `Constraints`, and `TrackSize` without changing existing callers.
- [x] Changed the public leaf/container measurement APIs, runtime traversal, frame inset, and
      measurement-cache keys to carry `Constraints` directly. A bounded zero now reaches a leaf as
      `Bounded(0)` instead of being rewritten to an arbitrary positive pixel.
- [x] Added the pure replacement track solver alongside the legacy cursor so container migration
      could happen in independently testable commits.
- [x] Removed the crate-private sentinel adapters after the last container migration.
- [x] Removed generic `Policy` and `SizePolicy` from `Node`, runtime allocation, public exports, and
      downstream container APIs. `ContainerLayoutCtx::layout_child` now always assigns an exact
      parent-owned rectangle.

### Phase 3: containers

- [x] Moved Row and Column to one orientation-parameterized implementation. Their public APIs now
      accept `LinearItem`, and retained mutations keep nodes and their edge tracks synchronized.
- [x] Migrated all vertical Stack uses to Column and removed Stack instead of maintaining two names
      for the same non-overlapping behavior. Reverse placement is a Column property.
- [x] Moved Grid to the same `TrackResolver` used by linear layout and replaced both Grid track
      vectors and mutation APIs with `TrackSize`.
- [x] Made empty Row, Column, and Grid sizing zero. Row retains the font-derived standard control
      height only when it actually contains children.

### Phase 4: composites and validation

- [x] Made ScrollArea converge its four possible bar states using measurement only, configure the
      resulting bars, and then allocate the scrolling surface once. A placement-count regression
      test rejects speculative content layout.
- [x] Kept root chrome as an ordinary exact-allocation container. It shrinks bounded constraints by
      its chrome occupancy and gives its one application child the committed body rectangle.
- [x] Migrated every example, test, file-dialog caller, Disclosure body, ScrollArea test fixture, and
      root call site away from node-global sizing.
- [x] Re-ran the frozen `demo-full` rectangles after the linear/Grid migration: the 86/flex/109
      button row, calculator 104/312 split, and 276-pixel log region remain exact.
- [x] Ran formatting, `cargo test --all-targets`, `cargo doc --no-deps`, and wgpu builds for
      `demo-full`, `calculator`, and `simple` without warnings.
- [x] Ran a strict whole-repository Clippy audit. `-D warnings` still stops on pre-existing
      atlas/image/style diagnostics outside this redesign; the production layout diagnostics it
      exposed were corrected or documented where returning the exact unique owner is intentional.
- [x] Built and launched the current `demo-full` with the glow backend on the available Wayland
      display. GNOME denied programmatic screenshot access from this session, so pixel comparison
      remains the one manual validation item; exact geometry assertions are the automated
      compatibility gate.

## Decisions and observations

- [x] The old `AxisSlot::offered`/`advance` split is not part of the new contract. It exists only
      because the old node policy and parent track could both resolve the same dimension.
- [x] Style-owned default cell sizes belong to widgets or explicit fixed tracks, not empty generic
      containers.
- [x] Temporary resolved-track storage is acceptable. It will be reused through layout scratch or
      retained container capacity rather than replaced by repeated measurement.
- [x] Geometry compatibility is checked at the committed rectangles. Keeping old enum names while
      changing their meaning would not count as compatibility.
- [x] The new bounded solver reserves content, fixed tracks, and gaps before distributing flex
      space. Content overflow remains visible and flex collapses to zero instead of overlapping an
      earlier sibling.
- [x] Responsive leaf measurement receives constraints as information, not as an allocation. A
      leaf still reports desired content; only its parent may assign its final rectangle.
- [x] A linear item stores only relationship metadata (`TrackSize` and an optional fixed cross
      extent). It is consumed at insertion and never becomes a second node or runtime abstraction.
- [x] Track resolution is replayable: construction summarizes reservations and weights, and a
      caller replays the same track/content pairs to obtain exact extents. This keeps immutable
      measurement free of scratch allocation while sharing the identical arithmetic with Grid.
- [x] Column reverse direction changes only the placement origin. Sizing, item order, mutation, and
      traversal remain the same as an ordinary Column.
- [x] `AvailableSpace::shrink` is the only inset primitive composites need. It preserves `Unbounded`
      and treats bounded zero as bounded zero, so none of the old `max(1)` sentinel repairs remain.
- [x] Scroll content has one deliberately local rule: it fills at least the viewport width, retains
      wider intrinsic overflow, and keeps its desired height. This behavior belongs to ScrollSurface
      and is not a general trait specialization.
- [x] Public custom containers now observe only desired child measurements and exact allocations;
      child-policy inspection was removed with the policy itself.
- [x] `cargo test --all-targets` passes after this phase (231 library tests and four downstream API
      tests, with the three pre-existing manual baselines ignored). The `example-wgpu` demo build
      also succeeds.
