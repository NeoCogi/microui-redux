# Layout redesign

This document defines the retained layout contract, its deliberate scope, and the implementation
decisions that keep measurement, parent-owned sizing, and exact allocation separate.

## Goals

- [x] **Represent finite and intrinsic measurement requests without sentinels.** A constraint must
  say explicitly whether each axis is bounded or unbounded, and a bounded value of zero must remain
  a real zero-pixel offer. Every measurement path, including composite widgets and scroll probes,
  must preserve that distinction instead of repairing zero into one or interpreting a remembered
  rectangle as an intrinsic request.

- [x] **Keep desired measurement separate from exact allocation.** Measurement reports the content
  extent a widget would like under the supplied information; it does not silently accept all
  available space unless an explicit policy says that occupying the bound is the desired behavior.
  Placement is the later operation that receives an exact rectangle and commits descendant
  geometry. Tests must exercise both phases so a correct measurement cannot hide an incorrect
  allocation, or vice versa.

- [x] **Make the parent the sole owner of each child slot.** Main-axis tracks, grid spans, fixed
  cross extents, and spacing are parent-child relationship data and must stay beside the container
  that interprets them. `Node` must not acquire a fallback size policy, and the runtime must not
  inspect concrete layout widget types to decide how much space a child receives.

- [x] **Resolve every sizing rule exactly once.** A container first resolves content, fixed, and
  flexible relationships into exact extents and then passes rectangles containing those extents to
  the runtime. Descendant allocation must not reapply the track rule, redistribute remainder, or
  infer a second answer from the child's desired size. Overflow therefore remains attributable to
  one parent decision.

- [x] **Use one concrete retained widget for all one-dimensional layout.** Horizontal and vertical
  sequences must be configurations of a public `Linear` widget rather than separate `Row` and
  `Column` widget types forwarding to private `LinearState`. The concrete object must directly own
  its weak topology capability, index-matched item specifications, direction, cross-axis behavior,
  and reusable placement scratch. Horizontal and vertical names may remain constructor vocabulary,
  but they must not create different runtime state types or typed-handle APIs.

- [x] **Express direction and cross-axis behavior without orientation-specific special cases.** A
  linear direction must identify both its axis and its leading edge, so left-to-right,
  right-to-left, top-to-bottom, and bottom-to-top placement use one model. Cross sizing must use a
  shared vocabulary for content sizing, stretching to the assigned allocation, and an exact fixed
  extent. The old Row-only height type and Column-only reverse flag must disappear rather than be
  carried as optional arguments through the shared solver.

- [x] **Use the same deterministic track solver for Linear and Grid.** Both containers must reserve
  content, fixed extents, and non-negative gaps before dividing bounded remainder among valid flex
  tracks. Invalid flex weights receive no share, rounding gives indivisible pixels to earlier
  tracks, and unbounded flex behaves as content. The common arithmetic must remain independently
  testable instead of being duplicated inside either container.

- [x] **Select scrollbars from measurements and allocate content once.** `ScrollArea` may evaluate
  the finite set of candidate viewport states, but candidate evaluation must remain measurement
  only. Once scrollbar visibility converges, the surface, bars, and application content receive one
  committed set of rectangles. Linear unification must not introduce a special runtime hook for
  scrolling or cause speculative child placement.

- [x] **Give empty generic containers zero intrinsic size.** An empty `Linear` or `Grid` contributes
  no content merely because style contains padding, a font height, or a historical default cell
  size. Explicit fixed tracks may still contribute their stated extents when the container model can
  represent a track without a child; otherwise size must arise from actual retained content.

- [x] **Expose overflow without perturbing sibling placement.** Content and fixed tracks retain
  their resolved size when they exceed a bound, while flexible tracks collapse to the remaining
  non-negative space. A child's overflow may enlarge reported content bounds, but it must never
  move a later sibling backward, cause implicit overlap, or rewrite the exact slot already assigned
  by the parent.

- [x] **Preserve representative committed geometry during the type unification.** Exact regressions
  must continue to cover the `demo-full` button row, calculator display/keypad split, log region,
  and weighted Grid example. The API migration is allowed to break source compatibility, but a new
  type name is not permission to change visibly correct placement. Final visual comparison remains
  a manual release check because raster output is outside the retained layout contract.

- [x] **Keep warm retained layout allocation-free.** Mutable placement may retain vectors used for
  resolved track extents and Grid occupancy, and repeated layout must reuse their capacity.
  Immutable measurement must continue using scalar replay because `ContainerWidget::measure`
  receives `&self`. The final validation must run the allocation regression after all call sites use
  `Linear`, proving that removing the forwarding widgets did not trade type simplicity for per-frame
  heap work.

### Linear unification execution checklist

- [x] **Define the public model before migrating consumers.** Add `LinearDirection`,
  `LinearCrossSize`, and one construction parameter type with documented defaults for horizontal
  and vertical sequences. Direction owns reversal for both axes. Cross sizing owns whether the
  shared line uses desired content, the exact assigned cross extent, or a fixed value. This step is
  complete only when each public constructor and mutator explains normalization, ownership, and
  measurement invalidation in both API documentation and implementation comments.

- [x] **Promote retained state into the concrete widget.** Rename and expand `LinearState` into the
  `Linear` object stored behind `ContainerWidget`. Move creation, topology mutation, measurement,
  placement, and the inert `Widget` surface onto that object. Remove orientation and Row-height
  parameters from private layout calls; those decisions must be read from the concrete object so
  invalid combinations cannot be assembled by an internal caller.

- [x] **Migrate every typed handle and construction site.** Built-ins, examples, window-manager
  fixtures, integration tests, and public exports must use `TypedWidgetHandle<Linear>` and the new
  parameter vocabulary. Disclosure's private body and FileDialog's mutable lists are important
  acceptance cases because they exercise topology mutation through a retained typed handle, not
  merely one-shot construction. Migration is incomplete while any production or test source names
  the removed Row/Column types.

- [x] **Delete the obsolete façade surface.** Remove the Row and Column modules, their parameter
  types, `RowHeight`, old exports, and documentation that describes two concrete linear widgets.
  Search the complete repository, excluding build artifacts, for obsolete identifiers and repair
  conceptual prose as well as code. No compatibility aliases are retained: a successful build must
  prove that all consumers genuinely use the new model.

- [x] **Validate behavior, documentation, and warm-state properties.** Run formatting, all-target
  checking, API documentation, all-target tests, feature-gated example checks, and the repository's
  allocation regression. Record exact results in this document only after they have run. Any
  geometry expectation changed solely to make a test pass must be explained against the desired
  contract; otherwise the old committed rectangles remain the acceptance baseline.

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

enum LinearDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

enum LinearCrossSize {
    Content,
    Stretch,
    Fixed(i32),
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

`Linear` is the only concrete one-dimensional container. `LinearParameters::horizontal` and
`LinearParameters::vertical` are constructor vocabulary, not different retained types. Each child
has one main-axis `TrackSize`; horizontal directions interpret main as width, and vertical
directions interpret main as height. An explicit per-item fixed cross extent remains available for
the small number of interfaces that require one child not to use the shared line extent.

`LinearDirection` combines axis and leading edge. This makes left-to-right, right-to-left,
top-to-bottom, and bottom-to-top placement ordinary configurations of the same object. Reversal
changes cursor origin and advancement only: item ownership, index order, measurement order, event
traversal, painting order, and resolved track sizes are unchanged.

`LinearCrossSize` describes the single shared line without pretending it participates in weighted
sibling distribution. `Content` reports and places at the maximum desired child cross extent.
`Stretch` reports the same desired content extent during measurement but consumes the exact cross
extent during placement. `Fixed(n)` reports and places at the normalized exact value while exposing
larger child content as overflow. Horizontal construction defaults to `Content`; vertical
construction defaults to `Stretch`.

The former `Stack`, `Row`, and `Column` types represented overlapping subsets of this state space.
They and their parameter types were removed rather than retained as aliases, ensuring every typed
handle exposes the same direction, cross-size, track, and topology mutation API.

The public linear syntax attaches sizing to the parent-child edge where it is interpreted:

```rust
Linear::create(LinearParameters::horizontal(
    [
        LinearItem::fixed(label, 86),
        LinearItem::flex(body, 1.0),
        LinearItem::fixed(action, 109),
    ],
))
```

Plain `Node` values still mean `LinearItem::content(node)`. There is no node-global fallback and no
runtime dispatch on a direction-specific concrete type.

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

Disclosure body inputs are `LinearItem` values because the private body is an ordinary vertical
`Linear`. The relationship does not leak onto the disclosure node or require a
disclosure-specific runtime hook.

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
- A trailing-edge `LinearDirection` changes only placement origin and cursor advancement. Sizing,
  item order, mutation, traversal, and painting retain their ordinary forward order.
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
- Added container-level coverage proving that a bounded measurement does not make a content-only
  Linear or Grid claim unused space.

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
  behavior, and the common axis-neutral algorithm remain intact.

### 3. Give the shared cross axis explicit semantics

- Replaced orientation-specific line-height and stretch behavior with `LinearCrossSize`, which
  exposes only content size, exact fixed size, or stretching across the assigned cross extent.
- Kept content cross size as the horizontal constructor default and stretch as the vertical
  constructor default, preserving existing committed geometry while making both choices available
  to every direction.
- Migrated examples, built-ins, tests, and documentation without changing committed geometry.

### 4. Promote shared state into one concrete widget

- Promoted the weak `ChildrenHandle`, index-matched specifications, direction, cross-size policy,
  and retained exact extents into public `Linear`; there is no subordinate `LinearState` object.
- Implemented `Widget` and `ContainerWidget` directly on `Linear`, so the object behind
  `TypedWidgetHandle<Linear>` is the object measured and placed by runtime traversal.
- Migrated Disclosure's private body, FileDialog's mutable lists, examples, baselines, integration
  tests, and dynamic topology tests before deleting the old façade modules. This order proved that
  live mutation behavior moved successfully rather than only making construction syntax compile.
- Removed direction-specific parameter and widget types without aliases. Repository source now has
  one typed topology API for every one-dimensional retained tree.

### 5. Reduce permanent process documentation

- Kept the layout contract, enduring design decisions, this implementation plan, and a concise
  validation summary in this document.
- Removed branch chronology, transient command history, and duplicated completed-work narration;
  commit history remains the authoritative work log.

### Validation gates

- `cargo fmt --all -- --check`, `cargo check --all-targets`, `cargo doc --no-deps`, and
  `cargo test --all-targets` pass. The test run contains 236 passing library tests, four passing
  downstream API tests, and three intentionally ignored manual baselines. The two tests formerly
  housed beside the horizontal façade now exercise the concrete `Linear` widget directly, keeping
  topology/track synchronization and bounded content measurement covered after module deletion.
- The exact geometry regressions cover the 86/flex/109 button row, the calculator's 104/312 split,
  the 276-pixel log region, and the Weight Demo Grid's 44/88 split. The warm retained-layout
  regression remains allocation-free. These assertions ran after every construction site and typed
  mutation handle had moved to `Linear`, so they validate the final ownership path rather than a
  transitional adapter.
- `demo-full`, `calculator`, and `simple` pass `cargo check` with the `example-wgpu` feature.
- `cargo clippy --all-targets` completes with the repository's existing diagnostics in atlas,
  image, rendering, style, and large-error test closures; it reports no diagnostic in the changed
  `Linear` implementation. The library run reports 32 warnings and the test build reports 43 after
  duplicates, all outside the promoted layout widget.
- A repository source search finds no use of the removed widget or parameter identifiers. Remaining
  occurrences of the historical names are confined to this migration explanation, human-facing
  demo labels, and ordinary Grid row/column terminology; there is no compatibility alias or hidden
  direction-specific retained type.
