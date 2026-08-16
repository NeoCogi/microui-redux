# Layout redesign

This document defines the retained layout contract, its deliberate scope, and the implementation
decisions that keep measurement, parent-owned sizing, and exact allocation separate.

## Goals

- A constraint has an explicit bounded or unbounded state; zero is an ordinary bound.
- Measurement reports content requirements. Allocation assigns an exact rectangle.
- The parent is the only owner of a child's slot size.
- Allocation never reapplies a sizing rule already resolved by the parent.
- Row and Column use one linear algorithm with no container-specific branches in the runtime.
- Grid uses the same track solver as linear layout.
- Scrollbars are selected from child measurements and content is allocated once.
- Empty Row, Column, and Grid containers have zero intrinsic size unless they contain explicit
  fixed tracks.
- Linear and Grid overflow is explicit and never changes sibling placement.
- Representative `demo-full` geometry is preserved through exact automated assertions. Final
  appearance comparison remains a manual release check.
- Warm retained layout remains allocation-free.

## Non-goals

- Do not implement CSS Flexbox or CSS Grid.
- Do not add speculative alignment, minimum-size, baseline, or absolute-positioning APIs.
- Do not add hooks for ScrollArea, root chrome, or another built-in to general traits.
- Do not preserve the pre-1.0 layout API when a smaller contract is clearer.

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

enum RowHeight {
    Content,
    Fixed(i32),
    Fill,
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
needed by an existing interface. Row's single shared line uses `RowHeight`, because a weighted flex
value has no sibling meaning on that axis. Reverse direction affects origins only, never sizing.

The former vertical `Stack` duplicated Column. Its uses moved to Column; the name was not retained
for a non-overlapping layout.

The public linear syntax attaches sizing to the parent-child edge where it is interpreted:

```rust
RowParameters::new(
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

## Decisions and observations

- The old `AxisSlot::offered`/`advance` split is not part of the contract. It existed only because
  the old node policy and parent track could both resolve the same dimension.
- Style-owned default cell sizes belong to widgets or explicit fixed tracks, not empty generic
  containers.
- Mutable layout may retain resolved-track scratch and reuse its capacity. Immutable measurement
  instead replays scalar track state because `ContainerWidget::measure` receives `&self`.
- Geometry compatibility is checked at committed rectangles. Keeping old enum names while changing
  their meaning would not count as compatibility.
- The bounded solver reserves content, fixed tracks, and gaps before distributing flex space. It
  reports only space owned by resolved tracks: unused bounded remainder is not desired content.
- Content overflow remains visible and flex collapses to zero instead of overlapping an earlier
  sibling.
- Responsive leaf measurement receives constraints as information, not as an allocation. A leaf
  still reports desired content; only its parent may assign its final rectangle.
- A linear item stores only relationship metadata (`TrackSize` and an optional fixed cross extent).
  It is consumed at insertion and never becomes a second node or runtime abstraction.
- Column reverse direction changes only the placement origin. Sizing, item order, mutation, and
  traversal remain the same as an ordinary Column.
- `AvailableSpace::shrink` preserves `Unbounded` and treats bounded zero as bounded zero, so none of
  the old `max(1)` sentinel repairs remain.
- Scroll content fills at least the viewport width, retains wider intrinsic overflow, and keeps its
  desired height. This local behavior belongs to ScrollSurface rather than a general trait hook.
- Public custom containers observe only desired child measurements and exact allocations;
  child-policy inspection was removed with the policy itself.

## Completed simplification plan

The parent-owned layout contract remains the architectural boundary. This follow-up does not
restore node-global sizing policy, allow allocation to reinterpret a resolved track, or change the
frozen example geometry. It reduces the machinery used to implement that contract and closes gaps
where the implementation does not yet match its documented desired-size semantics.

### 1. Make bounded track extents exact

- Added direct regressions for bounded axes containing only `Content` or `Fixed` tracks, invalid flex
  weights, and empty configured grids.
- Bounded remaining space enters the reported track extent only when at least one valid `Flex`
  track receives it.
- Content and fixed overflow remain visible, and flex-pixel rounding remains deterministic.
- Added container-level coverage proving that a bounded measurement does not make a content-only Row
  or Grid claim unused space.

### 2. Simplify retained linear state and placement

- Collapsed the separate metadata-owner layer into one retained linear state with one index-matched
  layout vector.
- Retained a reusable main-axis extent buffer for mutable placement, following Grid's existing
  scratch-buffer pattern.
- Mutable placement measures intrinsic main-axis requirements once, resolves them through the shared
  solver, measures cross-axis requirements once at those resolved extents, and places directly from
  the retained results.
- Immutable preferred-size measurement remains scalar and allocation-free, because
  `ContainerWidget::measure` receives `&self` and must not mutate retained scratch.
- Synchronized topology mutation, exact failure-value ownership, reverse placement, warm allocation
  behavior, and the common Row/Column algorithm remain intact.

### 3. Give Row height explicit semantics

- Replaced the single-track use of weighted `TrackSize::Flex(f32)` with `RowHeight`, which exposes
  only content height, exact fixed height, or filling the assigned height.
- Made content height the ordinary constructor default and added named configuration for fixed and
  fill height behavior.
- Migrated examples, built-ins, tests, and documentation without changing committed geometry.

### 4. Reduce permanent process documentation

- Kept the layout contract, enduring design decisions, this implementation plan, and a concise
  validation summary in this document.
- Removed branch chronology, transient command history, and duplicated completed-work narration;
  commit history remains the authoritative work log.

### Validation gates

- `cargo fmt --all -- --check`, `cargo check --all-targets`, `cargo doc --no-deps`, and
  `cargo test --all-targets` pass. The test run contains 236 passing library tests, four passing
  downstream API tests, and three intentionally ignored manual baselines.
- The exact geometry regressions cover the 86/flex/109 button row, the calculator's 104/312 split,
  the 276-pixel log region, and the Weight Demo Grid's 44/88 split. The warm retained-layout
  regression remains allocation-free.
- `demo-full`, `calculator`, and `simple` pass `cargo check` with the `example-wgpu` feature.
- `cargo clippy --all-targets` completes with the repository's existing diagnostics in atlas,
  image, style, and test code; it reports no new diagnostic in the changed layout implementation.
- The final diff is reviewed against both `layout-model-cleanup` and `extract-window-manager`, so
  this simplification remains distinguishable from the original layout migration.
