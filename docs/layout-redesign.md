# Layout redesign

This document is the working record for replacing the inherited microui sizing rules with one
retained layout contract. It is updated in the same commits as the implementation.

## Goals

- [ ] A constraint has an explicit bounded or unbounded state; zero is an ordinary bound.
- [ ] Measurement reports content requirements. Allocation assigns an exact rectangle.
- [ ] The parent is the only owner of a child's slot size.
- [ ] Allocation never reapplies a policy already resolved by the parent.
- [ ] Row and Column use one linear algorithm with no container-specific branches in the runtime.
- [ ] Grid uses the same track solver as linear layout.
- [ ] Scrollbars are selected from measurement and content is allocated once.
- [ ] Empty containers have zero intrinsic size unless they contain explicit fixed tracks.
- [ ] Overflow is explicit and never changes sibling placement.
- [ ] The existing `demo-full` geometry and appearance are preserved intentionally.
- [ ] Warm retained layout remains allocation-free after correctness and clarity are established.

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

The current vertical `Stack` duplicates Column. Its uses will move to Column; the name will not be
retained for a non-overlapping layout.

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

## Work log

### Phase 1: characterize and specify

- [x] Created branch `layout-model-cleanup` from `extract-window-manager`.
- [x] Recorded the target contract and explicit non-goals.
- [ ] Add direct tests for bounded, unbounded, overflowing, and rounded track allocation.
- [ ] Record representative demo layout geometry before changing behavior.

### Phase 2: core sizing

- [ ] Introduce `AvailableSpace`, `Constraints`, and `TrackSize`.
- [ ] Replace the ordered `Remainder` cursor with one pure track solver.
- [ ] Remove generic `Policy` from `Node` and runtime allocation.

### Phase 3: containers

- [ ] Move Row and Column to the shared linear implementation.
- [ ] Migrate and remove the current vertical Stack.
- [ ] Move Grid to the common track solver.
- [ ] Make empty-container sizing consistent.

### Phase 4: composites and validation

- [ ] Make ScrollArea choose bars during measurement and allocate content once.
- [ ] Keep root chrome as an ordinary exact-allocation container.
- [ ] Migrate every example, test, and file-dialog caller.
- [ ] Compare `demo-full` geometry/appearance with the baseline.
- [ ] Run formatting, all-target tests, and relevant feature builds.

## Decisions and observations

- [x] The old `AxisSlot::offered`/`advance` split is not part of the new contract. It exists only
      because the old node policy and parent track could both resolve the same dimension.
- [x] Style-owned default cell sizes belong to widgets or explicit fixed tracks, not empty generic
      containers.
- [x] Temporary resolved-track storage is acceptable. It will be reused through layout scratch or
      retained container capacity rather than replaced by repeated measurement.
- [x] Geometry compatibility is checked at the committed rectangles. Keeping old enum names while
      changing their meaning would not count as compatibility.

