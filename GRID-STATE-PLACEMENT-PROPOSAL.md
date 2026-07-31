# Grid-owned placement state proposal

Status: accepted and implemented for the current retained/runtime phase on 2026-07-30.
The private projection-builder edge bridge remains intentionally temporary until P3.0 deletes that
builder.

## Goal

Make grid placement an explicit property of the relationship between a `Grid` and
one of its children.

The current `NodeRuntime::grid_span` field makes every node carry metadata that is
meaningful only when that node happens to be owned by a grid. It also exposes that
grid-specific concern through the otherwise generic `ContainerLayoutCtx`.

This proposal removes that leak and gives `GridState` one authoritative home for:

- owned grid children;
- each child's grid span;
- column track definitions; and
- row track definitions.

The repository is unreleased, so this is a contract correction rather than a
compatibility migration. We should optimize for the smallest coherent retained
model, not preserve the current transitional API.

## Non-goals

- Do not introduce a generic `Any`, metadata map, or parent-placement enum on
  `Node`.
- Do not introduce a mounted wrapper node merely to represent a grid item.
- Do not expose attached `Node` references or raw mutable slices from `Children`.
- Do not change stable child identity when only a span or a track definition
  changes.
- Do not make non-grid containers understand or preserve grid spans.
- Do not keep the projection builder's representation as part of the final
  retained API.

## Current ownership problem

Today, grid placement crosses several abstraction boundaries:

```text
NodeOptions.grid_span
        |
        v
NodeRuntime.grid_span       <- every Node carries Grid-specific state
        |
        +--> ContainerLayoutCtx::child_grid_span(...)
        |
        v
legacy Grid.spans           <- Grid eventually copies the value here
```

This creates two conceptual owners:

1. the child node temporarily owns its span; and
2. the grid later owns a parallel span entry.

Neither is the right semantic model. A span is not intrinsic node state: the same
node could occupy one cell under one grid parent and multiple cells under another.
The value belongs to the parent-child edge.

The current model also makes span mutation awkward. Changing a grid placement
property should not require replacing the child node or changing its identity.

## Recommended target model

```text
GridParameters
    |
    v
GridContainer
    |
    +--> Rc<RefCell<GridState>>
              |
              +--> GridItems
              |      +--> Children
              |      `--> Vec<GridSpan>
              |
              +--> column_tracks: Vec<SizePolicy>
              `--> row_tracks: Vec<SizePolicy>

NodeRuntime
    +--> id
    +--> layout
    +--> transient runtime flags
    `--> policy                  <- generic parent-placement policy remains
```

`GridItems` is a private invariant-maintaining collection. Its split internal
storage is intentional: the current traversal interfaces operate on `Children`,
while grid measurement and placement also need indexed span data. Keeping both
inside one private type makes that representation an implementation detail rather
than a public synchronization obligation.

### Why `Policy` remains on `Node`

`Policy` is consumed by the common parent layout contract and is meaningful across
containers. `GridSpan` is consumed only by `Grid`. Treating these fields alike
would create superficial structural symmetry while preserving the abstraction
leak.

## Public construction types

### `GridSpan`

Define `GridSpan` in `src/ui_node/containers/grid.rs`, alongside the only
container that interprets it. Re-export it through `ui_node`, the crate root,
`retained`, and the prelude; remove the window-manager definition and re-export:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridSpan {
    columns: usize,
    rows: usize,
}

impl GridSpan {
    pub const ONE: Self = Self {
        columns: 1,
        rows: 1,
    };

    pub const fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns: if columns == 0 { 1 } else { columns },
            rows: if rows == 0 { 1 } else { rows },
        }
    }

    pub const fn columns(self) -> usize {
        self.columns
    }

    pub const fn rows(self) -> usize {
        self.rows
    }
}
```

The fields are private so the type's non-zero invariant is established once.
Grid layout may still clamp a span to the available grid width; that is a
placement rule, not input validation.

### `GridItem`

Use a small unmounted insertion value to carry a node and the placement metadata
for its future grid edge:

```rust
pub struct GridItem {
    node: Node,
    span: GridSpan,
}

impl GridItem {
    pub fn new(node: Node) -> Self {
        Self {
            node,
            span: GridSpan::ONE,
        }
    }

    pub fn spanned(node: Node, columns: usize, rows: usize) -> Self {
        Self {
            node,
            span: GridSpan::new(columns, rows),
        }
    }

    pub fn span(&self) -> GridSpan {
        self.span
    }

    pub fn into_node(self) -> Node {
        self.node
    }
}

impl From<Node> for GridItem {
    fn from(node: Node) -> Self {
        Self::new(node)
    }
}
```

`GridItem` is not another runtime tree level and receives no ID, layout, or
widget state. It exists only before insertion, so `into_node` is safe and useful
when an insertion cannot be completed.

This keeps the common case concise:

```rust
grid.push(label);
grid.push(GridItem::spanned(editor, 2, 1));
```

It also makes the uncommon data honest at the call site. A grid span is no longer
disguised as a property of `editor`.

## Private synchronized storage

```rust
struct GridItems {
    children: Children,
    spans: Vec<GridSpan>,
}
```

Every topology operation is implemented once on this type:

```rust
impl GridItems {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;

    fn push(&mut self, item: GridItem);
    fn insert(&mut self, index: usize, item: GridItem)
        -> Result<(), GridItem>;
    fn replace(&mut self, items: impl IntoIterator<Item = GridItem>);
    fn remove_drop(&mut self, index: usize) -> bool;
    fn clear(&mut self);

    fn span(&self, index: usize) -> Option<GridSpan>;
    fn set_span(&mut self, index: usize, span: GridSpan) -> bool;

    fn children(&self) -> &Children;
    fn spans(&self) -> &[GridSpan];
}
```

Required invariants:

1. `children.len() == spans.len()` after every public or private operation.
2. The span at index `i` always describes the child at index `i`.
3. Failed insertion returns the original unmounted `GridItem`.
4. Batch replacement builds the complete new child/span pair before committing
   either collection.
5. Removing, clearing, or replacing items follows the normal `Children` drop
   semantics.
6. Changing a span never replaces, detaches, or re-identifies the child.
7. Debug builds assert the length invariant at every mutation boundary.

The exact helper visibility can be narrower than the sketch. In particular,
`GridItems` must not offer a general-purpose `children_mut()` accessor.
`GridContainer` submits the private collection directly to the scoped framework
visitor, and Grid layout assigns rectangles through a narrow private helper.
Neither path exposes topology mutation to `GridState` callers.

## `GridState` API

```rust
pub struct GridState {
    items: GridItems,
    column_tracks: Vec<SizePolicy>,
    row_tracks: Vec<SizePolicy>,
}

impl GridState {
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;

    pub fn push(&mut self, item: impl Into<GridItem>);
    pub fn insert(
        &mut self,
        index: usize,
        item: GridItem,
    ) -> Result<(), GridItem>;
    pub fn replace(
        &mut self,
        items: impl IntoIterator<Item = GridItem>,
    );
    pub fn remove_drop(&mut self, index: usize) -> bool;
    pub fn clear(&mut self);

    pub fn span(&self, index: usize) -> Option<GridSpan>;
    pub fn set_span(&mut self, index: usize, span: GridSpan) -> bool;

    pub fn column_tracks(&self) -> &[SizePolicy];
    pub fn set_column_tracks(&mut self, tracks: Vec<SizePolicy>);
    pub fn row_tracks(&self) -> &[SizePolicy];
    pub fn set_row_tracks(&mut self, tracks: Vec<SizePolicy>);
}
```

The precise error type should follow the repository's existing `Children`
contract. The important contract is that a failed operation preserves ownership
of the input `GridItem`.

`push` accepts `Into<GridItem>` for the ordinary one-cell case. `insert` takes
`GridItem` explicitly because its failure path must return the same input type.
This also composes with `WidgetStateHandle::try_update_with`: borrow failure and
an invalid index both preserve the same owned value. Callers can write
`GridItem::new(node)` when no custom span is needed.

`GridParameters` accepts the same retained concepts:

```rust
pub struct GridParameters {
    pub items: Vec<GridItem>,
    pub column_tracks: Vec<SizePolicy>,
    pub row_tracks: Vec<SizePolicy>,
}
```

Convenience constructors may accept
`IntoIterator<Item = T> where T: Into<GridItem>` so a grid of one-cell nodes does
not require wrapper boilerplate.

## Unification achieved

| Concern | Current representation | Proposed representation |
| --- | --- | --- |
| Child ownership | `Children`/legacy `Vec<UiNode>` | `GridState.items.children` |
| Child span | generic `NodeRuntime`, then copied to Grid | `GridState.items.spans` |
| Track definitions | Grid-specific mutable state | same `GridState` |
| Span mutation | node replacement or no retained path | `GridState::set_span` |
| Span validation | repeated defensive clamping | once in `GridSpan::new` |
| Generic container access | `child_grid_span` on `ContainerLayoutCtx` | no Grid-specific method |
| Builder transport | generic `Node` field | temporary builder edge field |
| Measurement/placement input | potentially separate paths | one ordered Grid item list |

The major unification is not merely moving a field. All persistent inputs to grid
layout become properties of one state object, and all child-edge invariants become
properties of one collection.

## Simplification and deletion inventory

Delete from the final retained model:

- `NodeRuntime::grid_span`;
- `Node::with_grid_span`;
- default `GridSpan::ONE` initialization for every node;
- `ContainerLayoutCtx::child_grid_span`;
- the requirement that custom containers preserve or query grid metadata;
- documentation and tests asserting that non-grid parents ignore a span;
- public `GridSpan` fields and repeated `.max(1)` input normalization in Grid;
- `NodeOptions::grid_span` when the projection builder is removed; and
- any builder helper that treats span as an intrinsic node option.

Keep:

- `NodeRuntime::policy`, because it belongs to the generic parent placement
  contract;
- `ContainerLayoutCtx::child_policy`;
- `ContainerLayoutCtx::layout_child`; and
- the layout-result setters, which are generic container output operations rather
  than parent-specific child metadata.

Expected concrete effects:

- generic nodes no longer store two grid-only `usize` values;
- custom container authors see a smaller, honest layout API;
- grids can mutate spans without remounting children;
- Grid measurement and final placement consume the same indexed item source;
- span/child synchronization logic exists in one private implementation; and
- the projection builder no longer dictates the retained runtime design.

Any `Node` size reduction must be measured after implementation because Rust
padding may affect the final struct-size delta.

## Rejected alternatives

### Keep `grid_span` on `Node`

This preserves the superficially uniform `push(Node)` API, but every node carries
Grid semantics and the generic layout context remains polluted. It also models
edge data as node data and gives span mutation the wrong identity consequences.

### Put `Children` and `Vec<GridSpan>` directly on `GridState`

This removes the node leak but scatters synchronization across every `GridState`
method and its layout implementation. A private `GridItems` wrapper makes the
parallel representation locally provable and independently testable.

### Store `Vec<GridItem>` directly

This looks attractive, but the retained traversal and layout APIs deliberately
operate on opaque `Children`. Replacing that mechanism would enlarge the change
into a generic traversal redesign. `GridItems` preserves the safe traversal
contract while hiding the necessary split storage.

### Add generic parent metadata to `Node`

An enum, type map, or `Any` payload would make the generic node API nominally
extensible while retaining the underlying ownership error. It adds dynamic
branching or downcasting for a single known need and makes unrelated containers
responsible for opaque data.

### Add a runtime `GridItem` node

An extra semantic node would complicate identity, layout, hit testing, and
inspection. The proposed `GridItem` is only an unmounted input value and
disappears into `GridState` at insertion.

## Projection-builder bridge

The projection builder may still need to carry a span before P3 removes it. That
temporary path must not keep Grid state on generic retained nodes.

Use builder-edge metadata instead:

```rust
struct BuilderChild {
    node: Node,
    grid_span: GridSpan,
}
```

During the bridge:

1. `NodeOptions` may temporarily continue collecting the declarative span.
2. Child construction moves that value into `BuilderChild`, not `NodeRuntime`.
3. Grid projection converts each `BuilderChild` into a `GridItem`.
4. Non-grid projection continues to discard the temporary grid-only option,
   preserving current builder behavior until that API is deleted.
5. P3 deletes `NodeOptions::grid_span`, `BuilderChild::grid_span`, and the
   projection helpers together.

This bridge is deliberately local and disposable. It must not be exported or used
by direct retained construction.

## Changes to the existing refactor plan

If this proposal is accepted, amend `UI-NODE-REFACTOR-PLAN.md` in these places:

1. Replace the final `NodeRuntime` and `ContainerLayoutCtx` sketches with the
   grid-free versions.
2. Change the ownership table so `GridState` owns children, spans, and tracks.
3. Remove “mutating attached GridSpan” from the non-goals; it becomes a supported
   `GridState` operation.
4. Amend P0 characterization tests to distinguish generic `Policy` from
   Grid-owned placement data.
5. Correct P1.3 completion evidence: its current Grid field and context method are
   transitional, not final.
6. Make the `GridItem`/`GridItems` conversion part of P2.0 so Grid is never
   migrated into an already-obsolete retained shape.
7. Remove the P2.3 requirement that downstream custom containers read
   `child_grid_span`.
8. In P3, describe the builder-edge bridge and its mandatory deletion.
9. In P4.2, define one ordered Grid item list as the input to measurement,
   placement, and both axis solvers.
10. Add the deletion inventory and absence checks to P5 completion criteria.

## Implementation sequence

The move should be one compile-safe correction within the existing phase order,
not a prolonged dual-authority migration.

### G0 — Amend contracts and characterize behavior

- [x] Amend the main plan sections listed above.
- [x] Add tests for `GridSpan` non-zero normalization.
- [x] Record current grid ordering, wrapping, spanning, and overflow behavior.
- [x] Record child identity/state across ordinary Grid layout.
- [x] Add compile-time/API checks that custom containers need no Grid metadata.

Completion evidence:

- The retained target has exactly one documented owner for span data.
- Characterization tests distinguish behavior to preserve from representation to
  delete.

### G1 — Introduce Grid-owned inputs and state

- [x] Move `GridSpan` to `src/ui_node/containers/grid.rs` and re-export it through
  `ui_node`, the crate root, `retained`, and the prelude.
- [x] Make its fields private and add `columns()`/`rows()` accessors.
- [x] Add unmounted `GridItem`.
- [x] Add private `GridItems` with centralized topology and span operations.
- [x] Add `GridState` containing items plus column and row tracks.
- [x] Add `GridParameters` construction from `GridItem` and plain `Node`.
- [x] Convert Grid measurement and placement to read spans from `GridItems`.

Completion evidence:

- Grid children, spans, and tracks are all reachable through one `GridState`.
- Span changes preserve child ID, widget state, and mounted topology.
- Every `GridItems` mutation preserves its length/index invariant.

### G2 — Remove generic runtime leakage

- [x] Delete `NodeRuntime::grid_span`.
- [x] Delete `Node::with_grid_span`.
- [x] Delete `ContainerLayoutCtx::child_grid_span`.
- [x] Update internal and downstream custom containers to the smaller context.
- [x] Retain `child_policy` and document why it remains generic.
- [x] Delete obsolete node-span identity and non-grid-ignore tests.

This should land in the same compile-safe batch as G1. Do not add a second
fallback read path from nodes.

Completion evidence:

- Searching generic node and container-context code finds no Grid-specific field,
  accessor, import, or branch.
- Custom containers compile without importing `GridSpan`.

### G3 — Isolate and delete builder transport

- [x] If P3 has not landed, carry declarative span only on private
  `BuilderChild`.
- [x] Convert builder Grid children into `GridItem` at the projection boundary.
- [x] Add a source comment marking the field for P3 deletion.
- [ ] When direct retained construction replaces projection, delete the complete
  builder span path.

Completion evidence:

- Before P3, only the Grid module and the private builder bridge mention
  `grid_span`.
- After P3, only the Grid-owned retained API mentions `GridSpan`.

### G4 — Share placement input with the axis solvers

- [x] Build ordered placement lists from the single `GridItems` source.
- [x] Use the same placement derivation for intrinsic measurement and final child rectangles.
- [x] Feed the same normalized spans into the column and row solvers.
- [x] Remove repeated zero-span normalization.
- [x] Preserve explicit overflow behavior when spans exceed available columns.

Completion evidence:

- Measurement and layout cannot observe different child/span pairings.
- Column and row allocation share one placement source and one span invariant.

### G5 — Cleanup, documentation, and absence checks

- [x] Update README and API examples to construct spans with `GridItem`.
- [x] Document span changes as topology-preserving state updates.
- [ ] Measure and record final `Node` size rather than inferring it.
- [x] Search for all deleted symbols and stale “node-owned span” language.
- [x] Run the full validation matrix.

## Required tests

### Value and construction tests

- `GridSpan::new(0, 0) == GridSpan::ONE`.
- Mixed zero/non-zero inputs normalize independently.
- `Node.into()` creates a one-cell `GridItem`.
- `GridItem::spanned` preserves the normalized requested span.
- A failed indexed insertion returns the original `GridItem`.

### Collection invariant tests

For `push`, front/middle/end `insert`, batch `replace`, `remove_drop`, and
`clear`:

- child and span lengths remain equal;
- index ordering remains equal;
- batch replacement preserves iterator order; and
- invalid indices do not partially mutate either collection.

### Identity and state tests

- `set_span` preserves the child's node ID.
- `set_span` preserves widget state.
- changing track definitions preserves all child IDs and state;
- batch replacement follows the ordinary `Children::replace` ownership
  semantics; and
- removal still drops the removed child's owned state exactly once.

### Layout tests

- default one-cell placement;
- horizontal and vertical spans;
- spans at row boundaries;
- a span wider than available columns;
- zero inputs after constructor normalization;
- empty Grid;
- no column tracks;
- mixed fixed, content, and stretch tracks;
- span change followed by remeasure and relayout;
- track change followed by remeasure and relayout;
- descendant overflow and viewport behavior; and
- measurement and layout using the same child/span ordering.

### API boundary tests

- a custom non-grid container compiles using only generic
  `ContainerLayoutCtx` operations;
- a direct retained Grid accepts both `Node` and `GridItem`;
- generic `Node` construction exposes no span setter;
- private `GridSpan` fields prevent invalid literal construction; and
- no public API exposes attached nodes or mutable raw item slices.

## Defect-prevention matrix

| Risk | Prevention | Evidence |
| --- | --- | --- |
| Child/span vectors drift | all mutations live in `GridItems` | invariant tests for every topology verb |
| Failed insertion loses a node | return original `GridItem` | invalid-index and borrow-failure tests |
| Span mutation remounts child | indexed metadata update only | ID/state preservation tests |
| Builder becomes second authority | private, temporary edge bridge | P3 deletion and source absence check |
| Measurement/layout disagree | one ordered item/placement source | paired layout regression tests |
| Invalid zero span reappears | private fields, normalizing constructor | construction and compile-fail checks |
| Custom containers depend on Grid | remove context accessor | downstream compile test |
| Oversized span panics | explicit Grid placement clamp/overflow rule | oversized-span test |

## Validation

Run after each compile-safe batch:

```text
cargo fmt --all -- --check
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -W clippy::all
cargo check --no-default-features
cargo doc --no-deps
cargo check --examples --features example-glow
cargo check --examples --features example-vulkan
cargo check --examples --features example-wgpu
```

Also run source absence checks for:

```text
NodeRuntime::grid_span
with_grid_span
ContainerLayoutCtx::child_grid_span
NodeOptions::grid_span
```

The last symbol is permitted only in the explicitly temporary projection-builder
bridge and must be absent after P3.

## Completion criteria

This proposal is complete when:

1. Grid placement metadata has exactly one retained owner: `GridState`.
2. `Node` and `ContainerLayoutCtx` contain no Grid-specific state or methods.
3. Grid spans can change without replacing a child or changing its state/identity.
4. Grid topology methods cannot desynchronize children from spans.
5. Track definitions, spans, and owned children are mutated through the same
   state handle.
6. Measurement and final layout consume the same ordered child/span source.
7. The projection bridge is either isolated as specified or fully deleted.
8. Direct retained examples use `Node` for one-cell children and `GridItem` only
   where grid placement metadata is needed.
9. All behavior, invariant, API-boundary, and validation checks pass.
