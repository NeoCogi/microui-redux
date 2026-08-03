# Widget/runtime separation and persistent node plan

## Status and scope

Target release: `0.8.0-pre-alpha` (the `0.8` pre-alpha development line).

This is the sole authoritative UI-node migration plan. It supersedes the obsolete, now-removed
`UI-NODE-PLAN.md`. The following corrections and decisions are authoritative:

1. `crate::Widget` remains the sole common widget execution contract, but this breaking migration
   changes `Widget::update` to accept `Option<&UiInputEvent>` and return `()`; typed widget state is
   the only application event surface;
2. `WidgetState` is application-facing data owned and consumed by a concrete widget implementation.
   It is not a renamed widget execution trait;
3. `Container` is a public subtrait of `Widget` for built-in and downstream custom containers,
   while `ContainerState` is its distinct marker-only data role;
4. the public name `Node` is reserved for the unique owning tree node. The currently exported
   header/tree widget `widgets::Node` and `NodeStateValue` are removed and their behavior moves into
   the stateful `Disclosure` container;
5. neither `ContainerState` nor `Container` exposes a raw mutable `Children` borrow. Built-in state
   types expose only topology-safe inherent operations, and generic recursion uses
   framework-created opaque child visitors;
6. root creation consumes one application `Node` and returns a non-owning `RootHandle`; every
   `WidgetTree` is rooted by one private `RootChromeContainer` whose public `RootState` uses the
   same weak typed-state mechanism as every other widget/container;
7. root geometry, visibility, chrome interaction state, and chrome events live in `RootState`.
   `WindowEntry` owns cross-root policy, z-order, backend viewport integration, and the
   framework-internal weak handle used to reach that state;
8. every concrete retained runtime implements `WidgetStateOwner`, owns its one strong
   `Rc<RefCell<State>>` directly, and is boxed only when it enters `Node`; there is no parallel
   `OwnedWidget`/`OwnedContainer` wrapper or framework state-allocation factory;
9. `ResourceState`, `FrameResults`, `FrameResultGeneration`, `RetainedId`, and generic frame-result
   lookup are removed. Typed `WidgetState`/`ContainerState` handles are the sole public observation
   and mutation mechanism, including for root/window chrome; focus remains a `WidgetUpdateCtx`
   operation, and routing owns input consumption/capture;
10. built-in exposure and mounted mutation are fixed by the mapping below. The migration preserves
    demonstrated/intended application state, events, and commands, not arbitrary post-mount
    mutation of every field that was public on a combined widget struct;
11. row/grid/stack layout configuration and scroll enablement are mutable through their exposed
    container states. `Node` policy is pre-insertion-only; Grid spans are Grid-owned child-edge
    state and remain mutable through `GridState` without replacing the child;
12. moving a unique unmounted value through a state access uses the input-preserving
    `WidgetStateHandle::try_update_with` operation, so unavailable access returns the uncommitted
    input directly instead of dropping an uninvoked closure capture;
13. replacing a mounted root's application child while retaining its `RootId` is intentionally
    unsupported. Dynamic root content uses a persistent application container; destroying and
    recreating the root yields a new `RootHandle` with a fresh, never-reused `RootId`;
14. this document is the only UI-node migration plan. It owns every still-applicable correctness
    defect from the removed `UI-NODE-PLAN.md`, including explicit constraints, one authoritative
    ordered input queue, one complete retained-tree update plus layout per dequeued input,
    paint-only rendering, shared axis allocation, scrolling,
    disclosure, and
    window/transform boundary work;
15. P1.2 temporarily compiles the file-dialog feature out so the legacy strong-handle leaf adapter
    can be removed completely without pulling state-owned Row, Stack, and ScrollArea ahead of their
    dependency order. The implementation and tests in `src/file_dialog.rs` stay in place, and the
    `src/lib.rs` module/export edges plus `demo-full` integration are commented out with uniform
    `P1.2 TEMPORARY: restore in P3.2` markers rather than deleted. P3.2 restores and refactors that
    preserved code after P1.3/P2.0/P2.2 provide its owning-node/container prerequisites. The
    restored public boundary is a polling `FileDialogSession`: Context owns and advances the
    dialog, while application code only observes stable pending/accepted/cancelled status. This is
    an internal migration state only: no merge/tag/release may expose a build without the restored
    public file dialog;
16. container pointer capture remains owned by `WidgetTree`, while the captured concrete container
    reports whether its own local captured interaction is still active through the defaulted
    `Container::retains_pointer_capture` query and clears that local interaction through the
    defaulted `Container::on_pointer_capture_lost` notification when the tree ends the lease.
    Ancestors control descendant eligibility only through `children_visible`; there is no
    `ContainerOption::RETAIN_POINTER_CAPTURE`, parent override, concrete-container downcast, or
    state-to-tree callback;
17. input forwarding APIs only enqueue ordered raw events. `Context::update_ui(dimensions)` first
    synchronizes current layout, then drains those events without coalescing; every dequeued event
    runs cross-root dispatch, one full eligible-tree `Widget::update`, target sanitation, and a
    complete layout commit before the next event. An empty queue runs layout synchronization but
    zero widget updates. `ContextFrame::render_ui` performs paint/display-list submission only and
    never drains input, updates widgets, or lays out the tree. There is no timer, tick, or idle
    widget-update path;
18. a visible `WindowKind::Modal` root is modal. Context stores visible dialogs in one private
    `modal_stack: Vec<RootId>`; `modal_stack.last()` is the active dialog and an empty stack means
    ordinary routing. It does not store a `RootHandle`: Context already owns each matching
    `WindowEntry`, while `RootHandle` is an application-facing weak capability and would duplicate
    state access at this internal policy boundary. The ID stack also keeps `bring_root_to_front`
    and `destroy_root` borrow-independent; it is cross-root policy metadata, not another owner or a
    duplicate state cell. The active dialog remains frontmost and is the sole root eligible
    for pointer, keyboard, text, focus, capture, and per-event update routing. Outside pointer input
    is swallowed rather than falling through. Other roots remain visible and continue through
    layout and paint. Showing another dialog activates it and clears transient targets in every
    other root; hiding, closing, or destroying it restores the previous dialog in the stack,
    or ordinary routing if none remains. A popup has no implicit modal ownership relationship and
    is therefore blocked while a dialog is active; modal-owned popup behavior requires a future
    explicit root relationship rather than a cross-root exception.

All common runtime phases belong to `Widget`, including for containers. Public `Container: Widget`
adds only opaque child visitation, layout, descendant-visibility, container-specific routed-input,
and local pointer-capture-lifecycle hooks; application state belongs to concrete
`WidgetState`/marker `ContainerState` types. No adapter or alternate trait may transfer phase
methods onto state. There is also no frame-wide state lock, Context token, or in-frame/out-of-frame
state distinction.

The target keeps the useful part of the prior direction: one persistent, uniquely owned node tree
and typed weak application handles. Each concrete runtime owns the sole persistent strong
`Rc<RefCell<T>>` for its `T: WidgetState` and implements `WidgetStateOwner` to produce the safe weak
capability. A concrete constructor returns that handle when application access is meaningful or
discards it for a `State = ()`/internal runtime. P1 establishes this direct ownership boundary
before the bulk migration so later items implement one ownership model.

Breaking public API changes are expected. The migration does not preserve `widget_handle`, strong
`WidgetHandle<T>`, generated builder identity, `UiNodeBuilder`, `UiNodeSet`, public widget `NodeId`,
`ResourceState`, any frame-result lookup API, the `Widget::update -> ResourceState` return, or
context-level `scroll_delta` accessors. It also does not preserve `WidgetInputEvents`, aggregate
per-render input semantics, implicit widget update/layout inside `render_ui`, or idle per-frame
`Widget::update` calls. Root reads move from Context to `RootState`, and root setters become fallible
through `RootMutationError`.

## Document authority and behavior change control

The **Target architecture** section is the single normative description of the intended final
system. P0 freezes the current supported baseline plus the currently wanted replacement behavior
before production migration begins. P1-P5 name implementation owners and migration-specific
verification; they do not silently redefine P0 or introduce alternate contracts. The final
completion definition is an audit index over those earlier contracts rather than a second source of
truth.

P0's wanted behavior is protected but not immutable. If a later implementation cannot preserve a
listed behavior cleanly, or preservation exposes a material API, architecture, compatibility,
performance, or test cost, stop that item and raise an explicit decision. Present concrete options
for keeping and changing the behavior, with their implementation and user-visible consequences.
Ask the plan owner for clarification rather than selecting a compatibility trade-off implicitly.
Do not silently preserve, weaken, or discard it. Once resolved, update the applicable Target
architecture rule, P0 acceptance criterion, implementation owner, migration notes, and affected
tests together. Known defects remain excluded from the preservation baseline unless an explicit
decision reclassifies one.

### Temporary internal file-dialog availability exception

**Decision needed: No — explicit plan-owner decision for the P1.2 integration boundary**

`FileDialogState` currently depends on reusable strong leaf handles because
`FileDialogState::eval` rebuilds and replaces the complete projection. Strict P1.2 direct boxing
cannot keep that compiled path without either retaining an erased/shared runtime adapter or
recreating controls and weakening cursor, pending-event, scroll, and weak-handle lifetime. Neither
is an accepted migration path.

P1.2 therefore comments out only the file-dialog compilation and integration edges. Preserve
`src/file_dialog.rs` in place with its implementation and tests; do not delete, rename, truncate,
move, replace with a stub, or mechanically rewrite it merely to make the disabled code compile.
Comment out `mod file_dialog` and both `FileDialogState` re-exports in `src/lib.rs`, plus the
file-dialog-owned fields, initialization, evaluation, and visible demo controls in `demo-full`.
Every commented region must carry the exact searchable marker
`P1.2 TEMPORARY: restore in P3.2` and enough adjacent code/context to make its restoration owner
unambiguous. Do not add a Cargo feature whose enabled configuration is known not to compile, and do
not hide deletion behind an empty compatibility type.

P1.3 supplies the owning `Node`/`Children` foundation but does not prematurely re-enable a reduced
or reconstruction-based dialog. P2.0 supplies Row/Stack and P2.2 supplies ScrollArea. P3.2 uses the
preserved source and comment markers as its migration inventory, refactors the implementation in
place to persistent controls plus local child replacement, replaces application-driven
`FileDialogState::eval` with Context-owned polling sessions, restores the crate-root/prelude exports
and `demo-full` integration, re-enables/adapts all file-dialog tests, and removes every temporary
marker. The ordinary validation matrix may exclude file-dialog code only from the completed P1.2
commit through the prerequisites before P3.2. No externally visible release is permitted during
that interval, and disabled tests are not completion evidence for the restored feature.

## Deferred post-refactor redesign: keyboard and pointer focus

**Status: Known defect, explicitly deferred until after P0-P5 complete**

**Decision needed: Yes — only after the final retained tree and routing shape exists**

The current runtime uses one `focus` target for several distinct responsibilities: persistent
keyboard/text routing, pointer-press activity, focused/pressed paint state, and parts of drag
lifetime. `FocusPolicy` and `WidgetOption::HOLD_FOCUS` vary the lifetime of that combined target.
Each visible window-manager root also owns an independent runtime focus slot. The window-manager
dispatcher sends keyboard/text input only through the front visible root's slot, but cross-root
keyboard ownership is not yet stored as one explicit target. These facts are observed migration
state, not an endorsed final focus model.

P1.1 nevertheless fixes one local authority violation without attempting that broader redesign:
widgets cannot assign or clear focus through `WidgetUpdateCtx`, widget state contains no queued
focus command, and textbox submission records its event without releasing focus. The
window-manager dispatcher selects one front visible root, and that root's `UiRuntime` sends each
keyboard/text event only to its current focused node. This removes cooperative widget focus
mutation and the textbox refocus workaround while leaving the combined pointer/keyboard focus
model for the post-refactor redesign.

Do not redesign this subsystem during P0-P5. In particular, this plan does not choose a final
focusability option, focus-target type, handle/identity representation, container-versus-tree
owner, cross-root activation model, or public focus-command API. Choosing those types before
projection builders, compatibility result paths, temporary payload adapters, and the old routing
pipeline are gone would be speculative and could force the same migration twice. The current focus
machinery may change only as required to keep the UI-node migration compile-safe,
preserve demonstrated editing and pointer behavior, route input through the planned authoritative
dispatcher, and sanitize stale targets; such work must not claim to resolve this defect.

After every P0-P5 completion criterion is satisfied, create a separate repository-grounded focus
redesign plan against the resulting code. That plan must re-inspect the final ownership and event
flow before deciding:

- how keyboard/text focus is declared, stored, transferred, cleared, and restored;
- how pointer hover, press, active state, and capture remain independent from keyboard focus;
- how exactly one root and widget receive each keyboard/text event;
- how hidden, gated, disabled, removed, and destroyed targets affect focus;
- how programmatic focus requests interact with traversal order and frame boundaries; and
- which built-in/custom widget APIs, style states, examples, and compatibility behavior survive.

Completion of this UI-node plan deliberately leaves that decision open. References elsewhere in
this document to preserving focus behavior, `FocusPolicy`, or `HOLD_FOCUS` describe the migration's
temporary compatibility boundary, not the approved architecture of the follow-on focus system.

## Goal

Replace projection rebuilding with a persistent retained tree whose application-facing capability is
typed state:

- `Widget` remains the object-safe runtime phase trait for both leaves and containers, with
  `update` simplified to accept one optional current event and return `()`;
- public `Container: Widget` adds only the object-safe opaque-child-visitation, layout,
  descendant-visibility, routed-input, and local pointer-capture-lifecycle contract needed by
  container nodes;
- `WidgetState` marks concrete application state and contains widget-specific operations;
- `WidgetParameters` represents construction input;
- `WidgetStateOwner: Widget` associates each concrete runtime with its one state type and returns a
  safe weak state handle while keeping the strong `Rc<RefCell<T>>` private in that runtime;
- `WidgetBuilder` associates Parameters with one concrete `W: WidgetStateOwner` and constructs that
  runtime directly through `WidgetBuilder::create_widget`;
- final widget construction returns the concrete runtime, or a typed handle plus the runtime when
  application access is meaningful; no optional generic factory result or owner wrapper exists;
- final custom-container construction follows the same runtime-owned state model;
- `WidgetStateHandle<T>` is cloneable without requiring `T: Clone`; every clone remains weak;
- each public dynamic built-in container construction returns its typed state handle and a completed
  `Node`, while downstream custom containers pass their concrete state-owning runtime to public
  `Node::container`;
- built-in container state owns its child `Node` values directly;
- application-mutable container membership is changed through an exposed state handle, while fixed
  containers may hide their state; neither path uses a Context editor or identity token;
- public container-state APIs never return a child or lend the complete mutable child collection;
- same-cell conflicts between checked state-handle access operations return `None` rather than
  panicking;
- outside paint/custom-render callbacks, unrelated state cells may be read or mutated whenever
  their checked borrow is available, including while a `ContextFrame` exists; after a
  layout-affecting mutation the application drops any unsubmitted frame and calls
  `Context::update_ui` again before paint;
- node identity, focus, capture, routing, layout, and painting remain internal runtime concerns;
- removing a node drops its concrete runtime and owned state cell and makes any exposed weak handle
  expire after active access closures release their temporary upgrades;
- every root has one private retained `RootChromeContainer`; its exposed `RootState` reports chrome
  state and consumes chrome events through the same checked weak-handle API as other state;
- root creation returns a cloneable non-owning `RootHandle` that pairs the lifecycle `RootId` with
  the weak `WidgetStateHandle<RootState>`;
- destroying a root removes its `WindowEntry` and releases the private chrome container/application
  subtree; physical state destruction waits only for any already-active temporary state upgrades;
- retaining a `RootId` never permits replacing the chrome container's single application child;
  mutable root content is modeled by a persistent application container;
- owned inputs passed to checked state mutation are recoverable when access fails before the closure
  starts;
- runtime traversal visits retained boxes and child collections directly through opaque scoped
  visitors rather than application-constructible raw child callbacks.

Preserve these capabilities:

- window, dialog, and popup roots;
- built-in and external custom leaf widgets;
- built-in and external custom containers through the public `Container` trait;
- typed mutation of text, values, selection, colors, configuration intended to remain mutable, and
  custom application state;
- row, column, grid, stack, disclosure, and scroll-area composition;
- explicit dynamic child insertion, removal, clearing, and replacement;
- fixed, automatic, weighted, fractional, and remainder sizing;
- framing, clipping, transforms, and backend-specific custom rendering;
- input-transaction update-to-layout consistency followed by paint of the latest committed state;
- focus, hover, pointer capture, and routed input;
- two-axis and nested scrolling;
- correct intrinsic auto-size without a numeric pseudo-unbounded probe;
- one authoritative ordered input-dispatch stream—ordinary interaction through routed events plus
  the explicit cross-root popup-dismissal boundary—and a full retained-tree update/layout commit
  after every dequeued event before the next event is hit-tested;
- deterministic destruction and stale weak-handle behavior.

## Non-goals

This migration does not initially attempt to:

- make nodes, widgets, containers, state handles, or Context `Send` or `Sync`;
- support concurrent traversal from multiple threads;
- impose a global state-mutation boundary around a frame;
- provide order-independent or snapshot semantics for cross-widget mutation;
- detach, return, move, clone, or reparent an attached node;
- keep state alive after its concrete retained runtime and all temporary access upgrades are gone;
- return a strong `Rc`, raw `Weak`, raw pointer, internal runtime ID, or Context identity as an
  application state capability; `WidgetStateOwner::state_handle` returns only the safe typed weak
  wrapper and the concrete runtime keeps the sole persistent strong `Rc<RefCell<T>>` private;
- preserve any generic public frame-result channel when the same observation can live in typed
  widget, container, or root state;
- preserve source compatibility with projection builders or strong handles;
- preserve arbitrary post-mount mutation of every field that was public on a combined widget
  struct; the built-in mapping below is the compatibility boundary;
- preserve the old header/tree `widgets::Node` or `NodeStateValue` names;
- replace a mounted root `Node` while retaining its `RootId`;
- mutate an attached node's `Policy`; Grid placement is not node state and is mutable through its
  owning `GridState`;
- add generic per-node hide/show state or public node-visibility mutation;
- add dirty propagation, retained paint fragments, a node registry, or an arena without measurement;
- synthesize `Widget::update` calls from a timer, animation tick, idle frame, paint, or an empty
  input queue;
- perform input dispatch, widget update, layout, topology mutation, or application-state mutation
  from `ContextFrame::render_ui`; paint/custom-render code may update rendering-only caches but must
  otherwise be observational;
- redesign keyboard focus, pointer activity, or cross-root focus ownership before the final retained
  tree and routed-input architecture exists; that work is deferred by the dedicated section above.

Single-threaded, traversal-ordered execution is a contract. A state or topology mutation succeeds
whenever the target cell is live and its checked borrow is available. If the same cell is currently
borrowed by its widget, container traversal, or another handle closure, ordinary access returns
`None`; ownership-moving `try_update_with` returns its original input in `Err(input)`.
Mutating a different available cell is valid; later work observes the mutation and already-completed
work is not retroactively repeated.

The public state-handle API intentionally does not classify unavailable access. `is_alive()` reports
whether persistent ownership or an active upgrade remains when an application genuinely needs that
liveness fact; the ordinary access result stays `Option`, and `try_update_with` uses its error slot
only to return ownership. Widget phase signatures have no borrow-failure channel, so application
closures passed to `try_read`/`try_update`/`try_update_with` must not invoke
`Context::update_ui`, `ContextFrame::render_ui`, or another top-level retained traversal entry
point. This is an explicit application-reentrancy precondition, not a frame lock: state access while
a `ContextFrame` merely exists remains memory-safe, but a mutation after the last UI commit requires
dropping that unsubmitted frame and committing again before render. Framework-authorized recursion through
`Children::measure_child`, `ContainerLayoutCtx::layout_child`, and the opaque visitors is ordinary
retained traversal and is explicitly exempt.

## Current architecture

The current code combines application state and runtime behavior in the same concrete value:

```text
Application
    -> WidgetHandle<W>
         -> Rc<RefCell<W>>                    strong shared owner
              W: Widget                      state and runtime methods together

UiNodeBuilder
    -> generated NodeId
    -> WidgetNode
         -> Box<dyn WidgetStateHandleDyn>
              -> cloned WidgetHandle<W>      second strong owner
              -> borrows W and dispatches Widget methods

UiNodeSet
    -> Vec<UiNode>
         -> UiNodeState                      ID/layout/interaction
         -> UiNodeData
              -> Widget(Box<dyn NodeBehavior>)
              -> Container(Box<dyn Container>)
                    -> Vec<UiNode>
```

Repository facts that constrain the migration:

- [`src/widget.rs`](src/widget.rs) already defines the public, object-safe `Widget` phase contract.
  It is implemented by built-ins and external custom widgets in the examples.
- [`src/window_manager/retained.rs`](src/window_manager/retained.rs) stores widget values in strong
  `Rc<RefCell<T>>` handles and introduces `WidgetStateHandleDyn` only to erase those handles back
  into runtime phase dispatch.
- [`src/ui_node/containers/mod.rs`](src/ui_node/containers/mod.rs) adds another `WidgetNode` adapter
  behind `NodeBehavior`; current containers own `Vec<UiNode>` directly.
- that adapter's `NodeBehavior::measure`, `update`, and `paint` implementations mostly construct
  contexts and forward to the public `Widget` methods, while containers implement parallel phase
  methods directly. This duplication disappears when public `Container` inherits `Widget`.
- built-in types such as `Checkbox`, `Slider`, `Textbox`, `TextArea`, `Combo`, `TextBlock`, and
  `ColorSwatch` currently mix parameters, mutable application state, cached state, and `Widget`
  methods in one struct.
- `demo-full`, the calculator, and the file dialog use typed methods such as `Slider::set_value`,
  `Textbox::set_text`, `Combo::select`, and direct label/color updates.
- examples also implement `Widget` directly for custom runtime behavior; renaming or privatizing
  that trait would create unnecessary migration and an additional adapter layer.
- `UiNodeBuilder` reconstructs node projections and generated IDs. `Context::set_root_nodes`
  searches the old tree and transfers generic runtime state into the replacement.
- [`src/widgets/nodes.rs`](src/widgets/nodes.rs) and [`src/lib.rs`](src/lib.rs) currently export a
  header/tree widget named `Node` plus `NodeStateValue`, colliding with the owning `Node` required by
  this migration.
- current `UiNodeState::visible` is copied by runtime-state transfer but has no public mutation path
  and does not gate measure/input/update/paint traversal; it is not an existing supported generic
  visibility capability.
- `Context` has no root-destruction operation, and `FrameResultGeneration` exposes only
  `state_of_retained`; the existing `RetainedId::Root` shape is not populated as an application root
  result.
- current widget interaction is queried through `FrameResults` plus public generated identity, so
  application code often stores both a typed widget handle and a `NodeId`.
- `WidgetNode::update` currently records the returned `ResourceState` only in leaf `FrameResults`;
  focus changes flow through `WidgetUpdateCtx`, and input routing/capture use separate mechanisms.
  Removing leaf results therefore leaves no retained-runtime consumer for that return value.
- `UiRuntime::measure_auto_size` currently uses `10_000` as a pseudo-unbounded height, allowing
  flexible grid/row policies to leak the probe into intrinsic window size.
- routed `UiInputEvent` values and raw `Input` are both interpreted: routing queues events, while
  `UiRuntime::interaction_for` separately derives hover/click/active/scroll state during update.
- the window manager performs a pre-input layout, while `UiRuntime::update_paint_frame` performs an
  unconditional pre-update repeat and then a post-update layout. The target replaces all three
  frame-coupled passes with an explicit synchronization layout plus one full update/layout
  transaction per queued input; rendering performs neither.
- row/grid/stack/column intrinsic measurement and allocation currently use independent policy logic;
  row/grid measurement can disagree with the eventual track allocation.
- `ContextFrame` already excludes direct Context method calls through Rust borrowing. It does not
  and should not control independent state-cell access.

The useful current pieces are the `Widget` execution contract, widget-specific typed methods, and
directly owned container children. The accidental pieces are strong application ownership, erased
handle redispatch, the `NodeBehavior` forwarding/parallel-phase layer, projection reconstruction,
parallel state/node identity, and Context-mediated state/topology access.

## Target architecture

### Ownership and construction flow

```text
Checkbox::create(CheckboxParameters)              exposed state, final release shape
    |
    +-- WidgetStateHandle<CheckboxState> ---------- Weak<RefCell<CheckboxState>>
    |
    +-- Checkbox runtime
            +-- state: Rc<RefCell<CheckboxState>>  sole persistent strong owner
            +-- immutable runtime configuration

Node::widget(Checkbox)
    -> private RuntimeNodeId + NodeRuntime + Box<dyn Widget>

Decoration::create(DecorationParameters)           hidden state
    |
    +-- Decoration runtime
            +-- state: Rc<RefCell<()>>             sole persistent strong owner
            +-- public constructor discards its trivial weak handle

Column::create(ColumnParameters { children })      exposed container state
    |
    +-- WidgetStateHandle<ColumnState> ------------ Weak<RefCell<ColumnState>>
    |
    +-- Node
         -> private NodeKind
              -> Container(Box<dyn Container>)     public Container: Widget, private enum variant
                   -> ColumnContainer
                        +-- state: Rc<RefCell<ColumnState>>
                              -> Children(Vec<Node>)

Context
    -> WindowEntry
         +-- RootId / kind / z-order / just-opened / backend policy
         +-- WidgetStateHandle<RootState>       framework-only weak clone
         +-- WidgetTree
              -> Node root
                   -> Container(Box<dyn Container>: private RootChromeContainer)
                        +-- state: Rc<RefCell<RootState>>
                              -> Children(exactly one application Node)
```

### Ownership boundary and compile-safe migration

The architecture above is the only supported result. P1 establishes direct state-owning runtime
insertion before the bulk built-in and container migrations. Public insertion is generic over a
concrete `WidgetStateOwner` (and `Container` for container nodes), then erases the runtime to a
private `Box<dyn Widget>`/`Box<dyn Container>` inside `Node`. No public raw-box overload exists, and
there is no parallel opaque owner wrapper or erased state keep-alive allocation.

Application state access is Context-free:

```text
upgrade weak state cell
    -> unavailable: return None / Err(input)
    -> try_borrow / try_borrow_mut
    -> unavailable: return None / Err(input)
    -> run non-escaping typed closure
```

Container topology uses exactly the same rule:

```text
column_handle.try_update_with(node, |column, node| column.push(node))
column_handle.try_update(|column| column.remove_drop(index))
column_handle.try_update_with(nodes, |column, nodes| column.replace(nodes))
```

There is no Context token, mount token, frame flag, `ContainerEditor`, handle registry, or raw
pointer lookup in either path.

### Keep `Widget` as the sole common runtime trait and simplify update

The public runtime contract remains the one object-safe phase interface, but this migration
intentionally removes the redundant leaf-result return:

```rust
pub trait Widget {
    fn widget_opt(&self) -> &WidgetOption;
    fn measure(
        &self,
        style: &Style,
        atlas: &AtlasHandle,
        avail: Dimensioni,
    ) -> Dimensioni;
    fn update(
        &mut self,
        ctx: &mut WidgetUpdateCtx<'_>,
        input: Option<&UiInputEvent>,
    );
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>);

    fn effective_widget_opt(&self) -> WidgetOption;
    fn focus_policy(&self) -> FocusPolicy;
}
```

Use the actual current default implementations for `effective_widget_opt` and `focus_policy`; the
sketch omits their bodies only for brevity. `Widget::update` records any application-observable
event directly in its typed associated state and returns no generic summary. The option contains
the one localized routed event assigned to this node for the current input transaction; every other
eligible node receives `None`. There is no routed batch or `WidgetInputEvents` aggregation helper.
Do not rename this
trait, add state/mount identity methods to it, or create a second internal trait with the same
responsibility.

`WidgetNode` becomes a thin owner of `Box<dyn Widget>` plus optional custom-render metadata. It
invokes the concrete runtime directly. The current
`WidgetStateHandleDyn` redispatch layer and erased handle cloning disappear. Public
`Node::widget` constructs the `None` metadata path; public backend-typed
`Node::custom_render(widget, CustomRenderHandle<B>)` supplies the `Some` path while keeping
`CustomRenderKey` private.

### Drain ordered input through complete update transactions and keep rendering paint-only

**Decision needed: No — explicit plan-owner decision for P2.5 on 2026-07-31**

The input/render coupling is removed at the Context boundary. Public `mousemove`, `mousedown`,
`mouseup`, `scroll`, `keydown`, `keyup`, `keydown_code`, `keyup_code`, and `text` calls append one
raw event to a private `VecDeque` in call order. They do not hit-test, mutate a tree, update a
widget, lay out, paint, or collapse repeated events into one aggregate state. The queue retains the
payload needed to apply pointer position, button/key held state, modifiers, text, and scroll in that
exact order. Normalization occurs once when an event is popped; later phases never inspect the raw
queue or reconstruct an order from final pressed/released bitsets.

The event-loop-facing boundary is explicit:

```rust
while let Some(event) = event_pump.poll_event() {
    forward_to_context(&mut context, event); // enqueue only
}

context.update_ui(window_dimensions);       // drain/update/layout; never paint
observe_events_and_mutate_typed_state();
context.update_ui(window_dimensions);       // optional layout-only sync; zero widget updates
context.frame(frame_info).render_ui()?;      // paint/submit only
```

`Context::update_ui(dimensions)` requires strictly positive dimensions and validates them before
touching the queue; invalid dimensions panic with an `update_ui dimensions must be positive`
diagnostic, matching the existing invariant that renderable `FrameInfo` has positive dimensions.
It performs one initial layout synchronization so root creation,
viewport changes, auto-size, and application mutations made through weak state handles are reflected
before the first pending input is hit-tested. It then drains the queue. For each dequeued event it
runs this indivisible transaction:

```text
dequeue and normalize exactly one raw input event
    -> sanitize targets against the currently committed tree
    -> apply cross-root/modal/popup policy and choose at most one routed recipient
    -> localize that event using the currently committed geometry
    -> traverse every eligible retained node through Widget::update exactly once
       (recipient: Some(event), all other eligible nodes: None)
    -> finalize focus/capture transitions and sanitize targets/topology
    -> measure and lay out every visible root, including auto-size/chrome normalization
    -> publish that state and geometry as the committed basis for the next queued event
```

The full update is intentionally not target-only. A resize, scroll, disclosure toggle, option
change, or topology change made while applying event N is laid out before event N+1 performs root
selection or hit testing. Thus a later event lands against the UI produced by every earlier input,
matching an SDL-style ordered event pump. A queue containing N events produces N full eligible-tree
widget updates and N post-event layout commits, plus the one initial layout synchronization. An
empty queue produces one layout synchronization and zero `Widget::update` calls. Events are not
coalesced, reordered, or reconstructed; repeated motion, wheel, key, and text events remain distinct
transactions.

The initial layout is necessary because application state handles deliberately have no Context or
dirty callback. That is the remaining explicit precondition rather than a hidden invalidation
channel: after any layout-affecting state/topology/root/style/viewport mutation, the application
must call `update_ui` before painting or forwarding another event that depends on the new geometry.
Calling it with no pending input is layout synchronization, not an idle widget update. Setters and
topology operations must leave their state internally valid immediately; they cannot rely on a
future timer-driven `Widget::update` for reconciliation.

`WidgetUpdateCtx` contains the router-owned interaction snapshot and event-time held state for the
current transaction. Add the exact read-only methods `mouse_buttons() -> MouseButton`,
`key_modes() -> KeyMode`, and `key_codes() -> KeyCode`; remove the synthetic
`UiInputEvent::KeyState`/`KeyCodeState` variants because held state is not a second input event. The
current localized event itself supplies pointer position/delta, pressed/released bits, text, and
scroll. The context exposes no raw `Input`, queue, Context, focus mutation, or duplicate scroll
field. Scroll exists only as the routed `UiInputEvent::Scroll` payload.
Routing may decide consumption/capture and recipient selection but must not apply widget state; the
single full update traversal applies it. Hover, click/active, focus, capture, and wheel behavior are
therefore derived once from the same popped event.

`ContextFrame::render_ui` requires a completed `update_ui` for the same dimensions and no newly
queued input. Add the public unit variant `RenderError::UiUpdateRequired`; return it when no UI
commit exists, the frame dimensions differ from the committed dimensions, or input was enqueued
after that commit. Context/root/style operations known to Context also invalidate the commit;
independent typed-state mutation follows the explicit caller precondition above because it cannot
set that flag. On a valid commit, rendering clears/records the display list by traversing paint
once, appends rendering-only chrome/custom operations, and submits that list once. It never calls
`Widget::update`, layout,
input dispatch, popup policy, or state reconciliation. Pending input or mismatched committed/frame
dimensions is this documented update-before-render error, not a reason for rendering to update
implicitly. Paint remains `&mut self` only for rendering caches and existing backend callback
ergonomics; mutating application state, topology, interaction, or layout during paint/custom render
is outside the contract because the type system cannot prevent mutation through independently held
`Rc<RefCell<_>>` handles.

Options considered and rejected:

1. **One aggregate update and two layouts per rendered frame.** It loses input order and lets event
   N+1 hit-test geometry that event N has already changed logically but not laid out.
2. **Target-only event application between layouts.** It makes routing responsible for part of the
   update semantics and skips cross-widget/container work the full update contract permits.
3. **Timer/idle `Widget::update`.** It makes behavior depend on render cadence or an unrelated clock
   and recreates input/render coupling. Empty-queue work is layout synchronization only.
4. **Layout from `render_ui`.** It hides stale application mutations inside painting and makes the
   render call an update boundary again.
5. **Change `Widget::paint` to `&self` as mutation enforcement.** It would block legitimate
   rendering-cache mutation but still could not prevent application-state mutation through an
   independently held `Rc<RefCell<_>>` or `FnMut` custom renderer. Keep `&mut self` and make
   observational paint a documented conformance rule.
6. **Give weak state handles a dirty callback into Context.** It would couple application state to
   mount/Context identity and recreate the callback/registry boundary this plan removes. The
   explicit initial layout synchronization is the selected cost.
7. **Pass `FrameInfo` into `update_ui`.** It would avoid repeating the dimensions value but couple
   update/layout to render-only clear/backend information. Keep `Dimensioni` as the update input and
   validate equality when the later `FrameInfo` is rendered.

### Keep `Container: Widget` public and `ContainerState` marker-only

`Container` is a public, object-safe subtrait of the final `Widget` trait. `ContainerState` is
the separate marker-only data trait implemented explicitly by concrete states that own `Children`;
it grants no generic child access. Neither runtime trait is a rename or substitute for a state
trait. Do not expose the current private `NodeBehavior`, `UiNode`, `UiNodeState`, runtime IDs, or a
raw child-collection callback. Replace `NodeBehavior` with inherited `Widget` dispatch plus the few
operations that only a container needs:

```rust
pub trait ContainerState: WidgetState {}

// Fields and constructors are private. Only retained traversal can construct these
// capabilities; a downstream Container implementation may only provide its own
// private child collection to the active traversal.
pub struct ChildrenVisitor<'a> { /* private */ }
pub struct ChildrenVisitorMut<'a> { /* private */ }

impl ChildrenVisitor<'_> {
    pub fn visit(&mut self, children: &Children);
}

impl ChildrenVisitorMut<'_> {
    pub fn visit(&mut self, children: &mut Children);
}

pub trait Container: Widget {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>);
    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>);

    fn layout(
        &mut self,
        ctx: &mut ContainerLayoutCtx<'_>,
        rect: Recti,
    );

    fn children_visible(&self) -> bool {
        true
    }

    fn retains_pointer_capture(&self) -> bool {
        true
    }

    fn on_pointer_capture_lost(&mut self) {}

    fn route_input(
        &mut self,
        ctx: &mut ContainerInputCtx<'_>,
        event: &UiInputEvent,
    ) -> ContainerInputResult {
        ctx.route_widget(
            event,
            self.effective_widget_opt(),
        )
    }
}

pub enum ContainerInputResult {
    Ignored,
    Consumed,
    Captured,
}

impl ContainerLayoutCtx<'_> {
    pub fn style(&self) -> &Style;
    pub fn atlas(&self) -> &AtlasHandle;

    pub fn child_policy(
        &self,
        children: &Children,
        index: usize,
    ) -> Option<Policy>;

    pub fn layout_child(
        &mut self,
        children: &mut Children,
        index: usize,
        rect: Recti,
    ) -> Option<Dimensioni>;

    pub fn set_content_size(&mut self, size: Dimensioni);
    pub fn set_children_viewport(&mut self, viewport: Recti, offset: Vec2i);
    pub fn set_child_overflow_propagation(&mut self, propagate: bool);
}

impl ContainerInputCtx<'_> {
    pub fn has_pointer_capture(&self) -> bool;

    pub fn route_widget(
        &mut self,
        event: &UiInputEvent,
        opt: WidgetOption,
    ) -> ContainerInputResult;

    pub fn route_widget_in_rect(
        &mut self,
        event: &UiInputEvent,
        rect: Recti,
        opt: WidgetOption,
    ) -> ContainerInputResult;
}
```

`ContainerInputCtx` does not accept a separate `FocusPolicy`. The retained runtime reads the
authoritative policy from the current node's inherited `Widget::focus_policy` during the same input
transaction. Passing another copy through routing would either be ignored or create two sources of
truth that could disagree.

### Keep pointer-capture ownership in `WidgetTree` and its local lifecycle in the captured container

**Decision needed: No — explicit plan-owner decision for P2.4 on 2026-07-31**

The public routed-input contract can acquire tree-owned capture by returning
`ContainerInputResult::Captured`. Direct state mutation can later invalidate the local interaction
that justified that capture without producing another input event. Conversely, removal, ancestor
gating, root hiding, pointer release, or replacement can end tree capture while the captured
container still has private drag state. The runtime therefore needs a narrow two-way lifecycle at
the same abstraction boundary: ask only the current captured container whether its local
interaction remains active, and notify only that container when the tree ends the lease.

Options considered:

1. **Add defaulted local-retention/loss hooks plus a scoped current-owner query — selected.**
   - Benefits: completes the existing public `Captured` lifecycle at the same abstraction boundary;
     lets built-in and downstream containers derive the answer from their private state; exposes no
     ID, parent, `WidgetTree`, Context, state handle, or concrete type; and leaves existing custom
     implementations source-compatible through the `true`/no-op defaults. The loss notification
     prevents a structurally gated but still-retained child from keeping a stale private drag mode.
   - Consequences: adds two public object-safe methods to `Container` and one read-only bool query to
     `ContainerInputCtx`. A custom container whose local state can revoke an acquired capture must
     override `retains_pointer_capture`; one that holds a private capture-specific mode must also
     clear it in `on_pointer_capture_lost`.

   ```rust
   pub trait Container: Widget {
       // Existing child/layout/visibility/input methods.

       fn retains_pointer_capture(&self) -> bool {
           true
       }

       fn on_pointer_capture_lost(&mut self) {}
   }
   ```

2. **Add a dynamic `ContainerOption::RETAIN_POINTER_CAPTURE` flag — rejected.**
   - Benefits: could encode the same answer if an `effective_container_opt` query recomputed the
     flag from live state.
   - Consequences: introduces a speculative option namespace for one live predicate, confuses
     construction/configuration with current interaction state, and still requires the same dynamic
     object-safe query. A static flag cannot handle disable/re-enable correctly, and neither form
     notifies a retained child when ancestor gating or tree policy ends capture first.

   ```rust
   fn effective_container_opt(&self) -> ContainerOption;
   // RETAIN_POINTER_CAPTURE would have to change with private drag state.
   ```

3. **Let the parent decide whether a child retains capture — rejected.**
   - Benefits: centralizes subtree composition decisions in the parent.
   - Consequences: requires a parent to inspect or identify a child's private interaction state,
     contradicts opaque `Children` and private runtime IDs, and couples reusable containers to the
     concrete behavior of descendants. Parents already have the correct narrower authority:
     `children_visible == false` makes the complete descendant subtree ineligible.

   ```rust
   // Rejected: this would expose child identity/state across the ownership boundary.
   fn child_retains_pointer_capture(&self, child: /* private identity */) -> bool;
   ```

`WidgetTree` remains the sole owner of `capture: Option<RuntimeNodeId>`. At each safe sanitation
boundary it first checks that the target still belongs to the tree and is reachable through every
ancestor `children_visible` gate. If the captured node is a container, it then calls
`retains_pointer_capture` only on that target. A `false` result clears the tree-owned capture before
another pointer event is routed; it never transfers capture to a parent, sibling, replacement, or
new node at the same index.

Every centralized transition away from an existing captured container schedules
`on_pointer_capture_lost` if that runtime still belongs to the tree, including pointer release,
local-retention rejection, ancestor gating, root hiding, explicit transient-target clear, or
replacement by a new capture. Sanitation outside an input transaction invokes it immediately. A
routing-time release clears tree ownership immediately, delivers that one release event to the old
target during the transaction's full update, and invokes the loss hook immediately after that
target's update. No second raw event can enter before this ordering completes, so the final drag or
release cannot be erased prematurely. The transaction may retain at most one private
`capture_loss_after_update` target; there is no routed-event batch, pending-loss list, loss
coalescing, or same-transaction reacquisition sequence.

If the node was removed, dropping its runtime is the cleanup and no callback is possible or
required. The hook receives no ID, reason enum, parent, tree, or Context; it only lets the captured
container clear private capture-specific state. The tree performs this notification directly on the
captured target, never through an ancestor.

Routing and `Widget::update` remain deliberately separated. `ContainerInputCtx::has_pointer_capture`
is a read-only answer about the current container, not a handle or ID. It lets a special container
distinguish direct delivery to its already-captured surface from ordinary hit routing. A pointer-down
that returns `Captured` is delivered during the same one-event full update; sanitation does not call
`retains_pointer_capture` between acquisition and that update establishing the local mode. The
transaction completes its update, sanitation, and layout before the next drag/release event can be
popped.

`ScrollAreaContainer` returns `scrolling_enabled && drag_axis.is_some()` and clears `drag_axis` from
`on_pointer_capture_lost`. Disabling scrolling clears both fields needed for retention, so disabling
and re-enabling before sanitation cannot resurrect the old capture. `RootChromeContainer` returns
whether `RootInteraction` is `Moving` or `Resizing` and clears that mode from its loss hook; option
changes, hiding, release, dismissal, and destruction also clear it through their existing owners.
Containers that never acquire capture or hold no capture-specific local mode may keep both defaults.
`WidgetOption`, `FocusPolicy`, and a new `ContainerOption` do not encode this local capture lease.

The opaque visitors scope the state borrow without returning a borrow guard or exposing child
references to their caller. Their constructors remain crate-private. Each visitor accepts exactly
one `Children` collection per `Container` call. `visit` panics immediately on a second submission
with a diagnostic naming `visit_children` or `visit_children_mut` and stating that exactly one
collection is required. After the method returns, framework `finish` validation panics with the
corresponding diagnostic if no collection was submitted. These are implementation invariant
violations by a custom `Container`, not recoverable input errors.

Both visitor methods must submit the same authoritative `Children` for the lifetime of a retained
container. Safe Rust cannot prove this across two object-safe methods: an incorrect downstream
implementation could privately own two collections and submit a different one from each method.
That same-collection rule is therefore a documented downstream conformance obligation, not an
`unsafe` contract. Every built-in, the downstream example, and conformance tests pin it. A typical
runtime implementation borrows its state only while supplying its private field:

```rust
impl Widget for ColumnContainer {
    // The final Widget methods. `measure` reads ColumnState and calls
    // Children::measure_child; update/paint are ordinary Widget phases.
}

impl Container for ColumnContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        let state = runtime_read(&self.state);
        visitor.visit(&state.children);
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        let mut state = runtime_update(&self.state);
        visitor.visit(&mut state.children);
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        let mut state = runtime_update(&self.state);
        layout_column(ctx, &mut state.children, rect);
    }
}
```

`Children` exposes safe indexed child measurement needed by implementations of the inherited
`Widget::measure`. `ContainerLayoutCtx::child_policy` reads the unique node's generic parent
placement policy without exposing the node. Parent-specific edge metadata belongs to the owning
container state and does not pass through this generic context. `layout_child` assigns one indexed child
rectangle in current-container content coordinates and returns the child's resulting content size,
or `None` for an invalid index. `set_children_viewport` installs a node-local visible viewport and
translation for descendants; the runtime intersects the viewport with the current content clip.
`set_content_size` and `set_child_overflow_propagation` update only the current node's derived layout
state. None of these methods changes topology or returns a child.

`ContainerInputCtx` exists because routing precedes the current input transaction's full
`Widget::update` traversal and must immediately decide deepest-first propagation and pointer
capture. Its default `route_widget` path
performs the same generic geometry/options/focus routing as
a leaf over the current node's complete content rectangle and assigns the accepted event to that
node for this transaction's inherited `Widget::update`. `route_widget_in_rect` applies the same rules to a supplied
container-local sub-rectangle, intersected with the active clip; pointer coordinates remain in the
container's local content coordinate space. `has_pointer_capture` reports only whether the current
container is the tree's captured target so special routing can continue an established gesture.
Disclosure uses sub-rectangle routing for the header, and scroll
area uses it for viewport/scrollbar hit regions. A special container may instead return `Ignored` at
a scroll boundary. Routing never applies the widget/container state change itself. The later
`Container::<Widget>::update` consumes its optional current event and performs the mutation.

Removing this hook would leave nowhere to return `Ignored`/`Consumed`/`Captured` before update:
`Widget::update` is deliberately one-way and routing has already selected the recipient. Doing so
would require reordering updates, invoking them more than once, or adding routing outcomes to the
common Widget contract. None is part of this migration.
The two capture hooks are not a second routing or update phase. `retains_pointer_capture` is a
read-only local-retention query used only when that same container is already the tree's captured
target; `on_pointer_capture_lost` is a one-shot lifecycle notification used only when the tree ends
that existing capture. Acquisition still comes only from `ContainerInputResult::Captured`, pointer
release still belongs to the ordered dispatcher, and removal/ancestor gating still invalidates
capture independently of the container's answer.
`ContainerInputCtx` and both child-visitor fields/constructors remain private and expose no IDs, raw
node storage, Context identity, raw child callback, or unrestricted tree mutation.

Export `Container`, marker `ContainerState`, `ChildrenVisitor`, `ChildrenVisitorMut`,
`ContainerLayoutCtx`, `ContainerInputCtx`, and `ContainerInputResult` from the public retained API
and its prelude. Do not add parallel container measure/update/paint contexts:
containers implement the final `Widget::measure`, `Widget::update`, `Widget::paint`,
`Widget::effective_widget_opt`, and `Widget::focus_policy` methods. `NodeBehavior` is deleted rather
than exported.

Private retained-tree traversal obtains `&dyn Widget`/`&mut dyn Widget` from either `NodeKind`
variant's private runtime box and uses exactly one common Widget
dispatch path per requested measurement, update, or paint invocation. This is not a promise of one
`Widget::measure` call per application loop: the initial synchronization layout and each required
post-input layout, plus an explicitly bounded scroll-constraint convergence, may issue multiple
legitimate measurement requests. The
invariant forbids parallel leaf/container phase paths and duplicate remeasurement inside one
request. Traversal branches to `Container` only for layout, scoped child recursion,
container-owned descendant visibility, special input routing/current-owner inspection, and the
captured target's local retention/loss hooks. Generic `NodeRuntime` has no visibility bit or
hide/show API. There is no shared `NodeBehavior` trait or parallel container phase adapter.

Built-in constructors return the completed `Node` for ergonomics, while downstream code may
construct and insert its own implementation explicitly:

```rust
let (custom_state, custom_container) =
    CustomContainer::create(custom_parameters);
let custom_node = Node::container(custom_container);
```

### Final builder, runtime owner, and stable state-handle contracts

`WidgetState`, `WidgetParameters`, `WidgetStateHandle`, `WidgetStateOwner`, and `WidgetBuilder` are
the complete construction and state-lifetime model implemented by P1 and used thereafter. A
concrete runtime owns its strong state cell directly. `WidgetStateOwner` associates that runtime
with exactly one state type and returns only the safe weak handle; it never returns the underlying
`Rc` or `Weak`. `WidgetBuilder` associates one Parameters type with one concrete state-owning
runtime `W`. A genuinely stateless runtime uses `State = ()` and its public constructor may discard
the trivial handle.

```rust
pub trait WidgetState: 'static {}

pub trait WidgetParameters: 'static {}

impl WidgetState for () {}

pub struct WidgetStateHandle<T: WidgetState> {
    cell: Weak<RefCell<T>>,
}

impl<T: WidgetState> Clone for WidgetStateHandle<T> {
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
        }
    }
}

impl<T: WidgetState> WidgetStateHandle<T> {
    pub fn new(owner: &Rc<RefCell<T>>) -> Self;

    pub fn is_alive(&self) -> bool;

    pub fn try_read<R>(
        &self,
        f: impl FnOnce(&T) -> R,
    ) -> Option<R>;

    pub fn try_update<R>(
        &self,
        f: impl FnOnce(&mut T) -> R,
    ) -> Option<R>;

    pub fn try_update_with<I, R>(
        &self,
        input: I,
        f: impl FnOnce(&mut T, I) -> R,
    ) -> Result<R, I>;
}

pub trait WidgetStateOwner: Widget + 'static {
    type State: WidgetState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State>;
}

pub trait WidgetBuilder: Sized + 'static {
    type Parameters: WidgetParameters;
    type W: WidgetStateOwner;

    fn create_widget(parameters: Self::Parameters) -> Self::W;
}

pub trait ContainerBuilder: Sized + 'static {
    type Parameters: WidgetParameters;
    type W: Container + WidgetStateOwner;

    fn create_container(parameters: Self::Parameters) -> Self::W;
}
```

Each builder consumes Parameters once and returns the concrete runtime; that runtime constructs and
privately retains exactly one `Rc<RefCell<State>>`. Its `state_handle` implementation calls
`WidgetStateHandle::new(&self.state)`, which only downgrades the borrowed owner reference and never
returns a strong pointer or raw `Weak`. The handle's manual `Clone` implementation clones only
`Weak` and deliberately imposes no `T: Clone` bound. Incorrect custom implementations that return a
handle for a different allocation violate the documented safe trait contract; built-in and
downstream conformance tests prove the returned handle observes the same allocation used by runtime
phases. Public `Node` constructors accept concrete state-owning runtimes, not raw trait-object boxes.

`try_read` and `try_update` return `None` without invoking their closure when the weak owner is gone
or the live cell is incompatibly borrowed. `is_alive()` supplies the separate liveness fact when it
is actually needed; no public error taxonomy duplicates information already available from the
handle. `try_update_with` upgrades and successfully borrows the cell before moving `input` into `f`,
so either unavailable condition returns the untouched input directly as `Err(input)`. Ordinary
`try_update` retains normal Rust closure semantics and cannot recover a value moved into an
uninvoked closure; examples must use `try_update_with` whenever a unique `Node`, concrete runtime,
or another non-cloneable input must survive access failure. Once `f` starts, it
owns the input normally. A container `insert` that rejects an index returns its uncommitted node
inside the successful outer access result.

Application handle exposure is expressed by each concrete constructor's return type, not by a
generic `Option` or builder constant. `Checkbox::create` and public dynamic layout containers return
their typed handles directly. A decoration widget or internal fixed container returns only its
runtime/finished `Node` and discards the trivial or internal handle before insertion. Parameters do
not contain an exposure flag, and discarding a weak handle never changes runtime ownership.

### Final Checkbox construction example

The split is data-oriented rather than a rename of today's `Checkbox` struct:

```rust
pub struct CheckboxParameters {
    pub label: String,
    pub checked: bool,
    pub opt: WidgetOption,
}

impl WidgetParameters for CheckboxParameters {}

impl CheckboxParameters {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            opt: WidgetOption::NONE,
        }
    }
}

pub struct CheckboxState {
    checked: bool,
}

impl WidgetState for CheckboxState {}

impl CheckboxState {
    pub fn check(&mut self) {
        self.checked = true;
    }

    pub fn uncheck(&mut self) {
        self.checked = false;
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn checked(&self) -> bool {
        self.checked
    }
}

pub struct CheckboxBuilder;

pub struct Checkbox {
    label: String,
    opt: WidgetOption,
    state: Rc<RefCell<CheckboxState>>,
}

impl Widget for Checkbox {
    // Current Widget methods. measure/paint read state and update mutates it.
}

impl WidgetStateOwner for Checkbox {
    type State = CheckboxState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl WidgetBuilder for CheckboxBuilder {
    type Parameters = CheckboxParameters;
    type W = Checkbox;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Checkbox {
            state: Rc::new(RefCell::new(CheckboxState {
                checked: parameters.checked,
            })),
            label: parameters.label,
            opt: parameters.opt,
        }
    }
}

impl Checkbox {
    pub fn create(
        parameters: CheckboxParameters,
    ) -> (WidgetStateHandle<CheckboxState>, Self) {
        let widget = CheckboxBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }
}
```

Construction and insertion are explicit:

```rust
let (checkbox_state, checkbox) = Checkbox::create(CheckboxParameters::new("Enabled", false));
let checkbox = Node::widget(checkbox);

column_state.try_update_with(checkbox, |column, checkbox| {
    column.push(checkbox);
})?;
checkbox_state
    .try_update(CheckboxState::check)
    .expect("checkbox state unavailable");
```

A widget with no application-visible state still owns its state cell directly. This genuinely
stateless example uses `()` and its public constructor simply does not return the trivial handle:

```rust
struct DecorationWidget {
    state: Rc<RefCell<()>>,
    // Other runtime-only configuration and caches may remain ordinary fields.
}

impl WidgetStateOwner for DecorationWidget {
    type State = ();

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl WidgetBuilder for DecorationBuilder {
    type Parameters = DecorationParameters;
    type W = DecorationWidget;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        DecorationWidget::new(parameters, Rc::new(RefCell::new(())))
    }
}
```

An initialization parameter may seed state, configure the runtime object, or both. Anything the
application must mutate later belongs in the state type. Stable runtime configuration needed by
`Widget::widget_opt`, such as the base `WidgetOption`, remains directly owned by the runtime widget;
`effective_widget_opt` may derive a copied dynamic override from state when required.

### State ownership and access

Every concrete `WidgetStateOwner` owns one persistent strong `Rc<RefCell<T>>` for its
`T: WidgetState`. Any application `WidgetStateHandle<T>` clones are weak and non-owning. A
constructor returning or discarding a weak handle never changes runtime ownership.

An access operation temporarily upgrades the weak pointer. Consequently:

- dropping an uninserted concrete runtime invalidates any exposed state handle;
- removing a node drops its boxed runtime and invalidates handles once any already-running access closure
  releases its temporary strong upgrade;
- handles do not know or care which Context or container owns the widget;
- moving the unique `Node` before insertion does not affect any exposed state handle;
- no frame state is checked;
- same-cell reentrancy through another checked handle access returns `None`;
- cross-cell access succeeds when the other cell is available.

The temporary-upgrade qualification is intentional. Preventing an active access closure from
briefly keeping the cell allocation alive would require another global or per-node lifecycle lock.
The observable contract is instead that no new successful access begins after both the node owner
and all already-active access operations are gone.

### Application state-access closures are not top-level traversal callbacks

Retained traversal is Context-local and never crosses Context boundaries. Entering a top-level
retained update, layout, or paint for the same Context that owns a borrowed state cell from inside application
`WidgetStateHandle::try_read`/`try_update`/`try_update_with` is explicitly unsupported. The runtime
cannot return the public handle's `None`/`Err(input)` outcome through `Widget::measure`, `update`, or
`paint`. Skipping a borrowed widget or painting stale data is also not an acceptable fallback.

This sequence is supported because mutation and its borrow end before the explicit UI commit:

```rust
checkbox_state
    .try_update(CheckboxState::check)
    .expect("checkbox state unavailable");

ctx.update_ui(frame_info.dimensions());
ctx.frame(frame_info).render_ui()?;
```

This sequence violates the API precondition because retained traversal begins while the mutable
state borrow is still held by the closure:

```rust
checkbox_state.try_update(|checkbox| {
    checkbox.check();
    ctx.update_ui(frame_info.dimensions()) // unsupported reentrant traversal
});
```

This compiles because the application-owned Context and weak state handle are independent Rust
values with no static lifetime relationship. `try_update` nevertheless retains the cell's mutable
`RefCell` borrow until its closure returns. If the nested traversal reaches that state, the built-in
runtime requests an incompatible borrow and reports the phase-specific invariant panic. That local
state borrow is the reentrancy guard; the architecture deliberately adds no Context-wide gate.

Do not add a FrameGate, Context token, state-access depth counter, or global “currently borrowed”
flag to detect this condition. Built-in runtimes should use a small internal state-borrow helper that
panics with a precise diagnostic if unsupported reentrant traversal reaches a borrowed state cell;
external custom widgets and containers are bound by the same documented precondition. Ordinary
handle-to-handle borrow conflicts continue to return `None` without invoking the nested closure.

This prohibition does not apply to framework-authorized recursive traversal. A container runtime
may retain its current checked state borrow while calling `Children::measure_child`,
`ContainerLayoutCtx::layout_child`, or supplying that same state's collection through an active
opaque visitor. Those capabilities are constructed only by the framework, recurse into distinct
owned child state cells, and are required by the public custom-container contract. Reentering
`Context::update_ui`, `ContextFrame::render_ui`, or another root traversal from those methods remains
unsupported.

```rust
fn runtime_read<T>(cell: &RefCell<T>) -> Ref<'_, T> {
    cell.try_borrow().expect(
        "widget state is already mutably borrowed; retained traversal from a state-access closure is unsupported",
    )
}

fn runtime_update<T>(cell: &RefCell<T>) -> RefMut<'_, T> {
    cell.try_borrow_mut().expect(
        "widget state is already borrowed; retained traversal from a state-access closure is unsupported",
    )
}
```

### Containers own children in their state

Use the same state-handle model for built-in containers. A separate `ContainerHandle`,
`ContainerCell`, and Context-owned editor are unnecessary.

```rust
pub struct Children {
    nodes: Vec<Node>,
}

impl Children {
    pub fn new() -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn measure_child(
        &self,
        index: usize,
        style: &Style,
        atlas: &AtlasHandle,
        available: Dimensioni,
    ) -> Option<Dimensioni>;
    pub fn child_policy(&self, index: usize) -> Option<Policy>;
    pub fn push(&mut self, node: Node);
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node>;
    pub fn remove_drop(&mut self, index: usize) -> bool;
    pub fn clear(&mut self);
    pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>);

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Node>;
    pub(crate) fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Node>;
}

impl Default for Children;
impl FromIterator<Node> for Children;

pub struct ColumnState {
    children: Children,
}

impl WidgetState for ColumnState {}
impl ContainerState for ColumnState {}

impl ColumnState {
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn push(&mut self, node: Node);
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node>;
    pub fn remove_drop(&mut self, index: usize) -> bool;
    pub fn clear(&mut self);
    pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>);
}
```

`Children` is public but opaque. `new`, `Default`, and `FromIterator<Node>` let downstream custom
containers build a collection before mounting it. `measure_child` lets a custom
`Container: Widget` implement the unchanged `Widget::measure` signature without a second container
measurement trait; it delegates to the child's private content measurement path. `child_policy`
separately exposes indexed placement policy so measurement never has to apply it implicitly. Neither
method exposes node identity or storage. Direct node iteration is crate-private and double-ended, so update/layout/paint can
traverse forward and input routing can traverse in reverse without making attached node references
public.

`ContainerState` is marker-only. Built-in state fields remain private and expose only inherent
operations such as the `ColumnState` methods above; no built-in state returns `Children`,
`&Children`, `&mut Children`, or a successfully removed `Node`. Consequently two attached built-in
containers cannot be reparented with `mem::swap`, `mem::replace`, or `mem::take` through their state
handles. `Children` deliberately provides no public iterator, `remove_and_return`, `detach`, clone,
shared insertion, or reparent operation. Failed `insert` returns the still-unmounted input node
because ownership was never committed; successful removal and replacement drop the previous owner.
Downstream custom-container authors are responsible for preserving the same logical invariant in
their own state APIs; the framework does not mark the safe `Container` trait `unsafe` for a
non-memory-safety contract.

The public `Container: Widget` runtime supplies its private field only to framework-created
`ChildrenVisitor`/`ChildrenVisitorMut` values. Ordinary downstream callers cannot construct those
visitors or install an extraction closure, so the generic runtime boundary also cannot be used to
swap whole attached collections. A traversal borrow of a container prevents that same container's
topology from changing until the visitor returns. Another available container may change, and
phase/traversal order determines whether its old or new children participate in the current input
transaction.
There is no snapshot or rollback: each phase renders or processes the state it observes when it
reaches that node. Work already completed in the update traversal is not repeated; the mandatory
post-event layout observes the resulting topology. Before paint or the next geometry-dependent
input, the next explicit UI commit must be fully stable against every successful mutation.

Public constructors for row, column, grid, stack, disclosure, and scroll area return
`(WidgetStateHandle<SpecificContainerState>, Node)`. Each concrete container runtime owns its state
cell and is immediately moved into `Node::container`, so downstream callers receive the ready node
instead of performing a redundant wrapping step. An internal fixed-composition constructor may
discard its state handle and return only `Node`. External custom leaf and container runtimes use
`WidgetStateOwner`, public `WidgetBuilder`/`ContainerBuilder`, and the generic `Node` constructors.

```rust
impl ContainerBuilder for ColumnBuilder {
    type Parameters = ColumnParameters;
    type W = ColumnContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        ColumnContainer {
            state: Rc::new(RefCell::new(ColumnState {
                children: parameters.children,
            })),
        }
    }
}

impl Column {
    pub fn create(
        parameters: ColumnParameters,
    ) -> (WidgetStateHandle<ColumnState>, Node) {
        let container = ColumnBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}
```

### Mounted container layout configuration

Container-local layout configuration is mutable through the same exposed state handle as child
membership. Parameters seed these values; the mounted APIs are fixed as follows:

```rust
impl RowState {
    pub fn widths(&self) -> &[SizePolicy];
    pub fn set_widths(&mut self, widths: impl IntoIterator<Item = SizePolicy>);
    pub fn item_height(&self) -> SizePolicy;
    pub fn set_item_height(&mut self, height: SizePolicy);
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
    pub fn remove_drop(&mut self, index: usize) -> bool;
    pub fn clear(&mut self);
    pub fn replace<T>(
        &mut self,
        items: impl IntoIterator<Item = T>,
    )
    where
        T: Into<GridItem>;
    pub fn span(&self, index: usize) -> Option<GridSpan>;
    pub fn set_span(&mut self, index: usize, span: GridSpan) -> bool;
    pub fn column_tracks(&self) -> &[SizePolicy];
    pub fn set_column_tracks(&mut self, tracks: impl IntoIterator<Item = SizePolicy>);
    pub fn row_tracks(&self) -> &[SizePolicy];
    pub fn set_row_tracks(&mut self, tracks: impl IntoIterator<Item = SizePolicy>);
}

impl StackState {
    pub fn item_width(&self) -> SizePolicy;
    pub fn set_item_width(&mut self, width: SizePolicy);
    pub fn item_height(&self) -> SizePolicy;
    pub fn set_item_height(&mut self, height: SizePolicy);
    pub fn direction(&self) -> StackDirection;
    pub fn set_direction(&mut self, direction: StackDirection);
}

impl ScrollAreaState {
    pub fn offset(&self) -> Vec2i;
    pub fn set_offset(&mut self, offset: Vec2i);
    pub fn scrolling_enabled(&self) -> bool;
    pub fn set_scrolling_enabled(&mut self, enabled: bool);
}
```

`ColumnState` has no per-container layout configuration beyond membership; its spacing comes from
`Style`. `DisclosureState` exposes expansion and membership. Scroll framing and base widget options
remain immutable Parameters because `Widget::widget_opt` returns a reference to stable runtime
configuration. `set_scrolling_enabled(false)` synchronously clears private drag state and resets
offset to zero; setting an offset while disabled keeps it at zero. Pointer capture belongs to the
owning `WidgetTree`, not `ScrollAreaState`: `ScrollAreaContainer::retains_pointer_capture` reports
`scrolling_enabled && drag_axis.is_some()`, `on_pointer_capture_lost` clears `drag_axis`, and target
sanitization releases tree-owned capture before another event can be routed when that local
predicate becomes false. Re-enabling scrolling does not restore capture because disabling or loss
already cleared `drag_axis`; a later routed pointer-down must acquire a new capture. Derived layout
hides both bars. `set_offset` clamps negative components immediately and clamps the upper bound
during the next layout, when current content and viewport extents are known.

`GridItem` is an unmounted insertion value containing one `Node` and one validated `GridSpan`; it
is not a semantic runtime node and has no ID, layout, or state of its own. `Node` converts to a
one-cell `GridItem`, while `GridItem::spanned(node, columns, rows)` is explicit at the only call
site where Grid placement exists. `GridSpan` is defined by the Grid module, has private fields,
normalizes zero components in `GridSpan::new`, and exposes `columns()`/`rows()` getters.

Privately, `GridState` stores a `GridItems { children: Children, spans: Vec<GridSpan> }` invariant
wrapper. All topology operations update both index-matched collections, and debug assertions pin
equal lengths. This split representation exists only because generic traversal operates on opaque
`Children`; neither collection is publicly exposed. `set_span` changes only edge metadata and
therefore preserves the child's ID, runtime, widget state, and mounted topology.

Row width entries correspond to children by index; a missing entry is `Auto` and excess entries are
ignored. An empty grid column list means one `Auto` column. Explicit extra grid column/row tracks
remain part of grid geometry even when currently empty. Changing grid columns or a child span
reflows row-major placement without changing child IDs or state cells.

Layout precedence is single and directional:

1. the container resolves a slot or shared grid tracks from its state configuration and child
   intrinsic measurements;
2. a grid applies the matching Grid-owned `GridSpan` to form the offered slot;
3. `ContainerLayoutCtx::layout_child` applies the child's pre-insertion `Policy` exactly once to
   that slot;
4. `Auto` fills the offered slot during allocation, while a non-`Auto` policy has final precedence
   for that child's allocation without rewriting shared row/grid track definitions;
5. a smaller child allocation leaves trailing slot space, while a larger allocation participates in
   the explicit overflow contract.

Containers may inspect `child_policy` for measurement and slot planning, but must not resolve it and
then let generic traversal apply it a second time. There is no mounted child-policy setter; changing
policy requires constructing and inserting a replacement node. Grid span is different because it
belongs to the parent-child edge: `GridState::set_span` updates it without replacing the child.
State mutations made before `Context::update_ui` affect its initial synchronization layout and the
first queued input. Mutations during an input transaction affect that transaction's post-event
layout and therefore the next queued input. Mutations made after `update_ui` require another
layout-only `update_ui` call before paint or geometry-dependent input; rendering never reconciles
them implicitly.

### Owning node and internal identity

The public name `Node` is reserved for the unique, opaque owner placed in roots and `Children`.
The existing header/tree widget with that name and its `NodeStateValue` enum are not aliased or
renamed as public compatibility types; their supported behavior is absorbed by `Disclosure` below.

The final opaque `Node` payload stores only erased concrete runtimes:

```rust
pub struct Node {
    id: RuntimeNodeId,
    runtime: NodeRuntime,
    kind: NodeKind,
}

enum NodeKind {
    Widget(WidgetNode),
    Container(Box<dyn Container>),
}

struct WidgetNode {
    widget: Box<dyn Widget>,
    custom_render: Option<CustomRenderKey>,
}

struct NodeRuntime {
    // Derived layout and transient hover/focus/capture-related flags, but no visibility bit.
    policy: Policy,
    /* private derived/transient fields */
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct RuntimeNodeId(NonZeroU64);
```

Leaf construction has two paths. Both keep `CustomRenderKey` private:

```rust
impl Node {
    pub fn widget<W: WidgetStateOwner>(widget: W) -> Self {
        Self::widget_with_custom_render(widget, None)
    }

    pub fn custom_render<B: RendererBackend, W: WidgetStateOwner>(
        widget: W,
        renderer: CustomRenderHandle<B>,
    ) -> Self {
        Self::widget_with_custom_render(widget, Some(renderer.key))
    }

    fn widget_with_custom_render<W: WidgetStateOwner>(
        widget: W,
        custom_render: Option<CustomRenderKey>,
    ) -> Self {
        Self::from_kind(NodeKind::Widget(WidgetNode {
            widget: Box::new(widget),
            custom_render,
        }))
    }

    pub fn container<C>(container: C) -> Self
    where
        C: Container + WidgetStateOwner,
    {
        Self::from_kind(NodeKind::Container(Box::new(container)))
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.runtime.policy = policy;
        self
    }

}
```

`Node::from_kind` initializes `Policy::auto()`. `with_policy` consumes and returns the
still-unmounted unique node. There is no mounted-node policy setter. Every built-in container
follows the single-application policy precedence above. Grid placement is constructed with
`GridItem`, retained by `GridState`, and never appears in generic `NodeRuntime` or
`ContainerLayoutCtx`.

`Node::custom_render` is backend-typed at the public construction boundary because it accepts only
`CustomRenderHandle<B>`, then erases the handle to the existing private `CustomRenderKey` stored in
the node. `Node` itself remains backend-neutral; therefore a handle from another Context/registry
(including one created for another backend and later erased into a Node) is still rejected by the
renderer registry's existing namespace/key preflight before backend acquisition. Achieving a
compile-time `Node<B>`-to-`Context<B>` relationship would require making the whole retained tree
backend-generic and is explicitly not part of this migration.

Construction from a state-exposing custom widget is direct:

```rust
let (triangle_state, triangle_widget) =
    Triangle::create(triangle_parameters);
let triangle_node =
    Node::custom_render(triangle_widget, triangle_renderer);
```

The owning type is named `Node`/`NodeKind`; an ID remains a private scalar. Public `Node::widget`
and `Node::custom_render` create leaf variants from concrete `WidgetStateOwner` runtimes. Public
`Node::container` accepts a concrete `Container + WidgetStateOwner`; no raw-box overload exists.
Built-in container constructors call the
applicable constructor internally and return the finished `Node` for convenience. Every path
assigns a never-reused runtime
ID from one process-wide monotonic allocator. Relaxed atomic allocation is sufficient because the
value is only uniqueness metadata, not synchronization.

IDs are created with nodes, not assigned by Context and not stored in state handles. They support
focus, capture, routed input, liveness validation, and cleanup only. The application never
receives or reconstructs them. Because IDs are globally unique and never reused, a stale internal
target cannot alias a node in another Context; no Context token or mount metadata is required.

`Node` is not `Clone`. Placement policy is configured with the consuming builder method above
before insertion. A successful insertion consumes it; Grid placement is supplied by `GridItem` at
the Grid insertion boundary. Generic node visibility is
absent: there is no `visible` field, `set_visible`, `show`, or `hide` operation on `Node` or
`NodeRuntime`.

### Fixed built-in constructor and compatibility mapping

Every built-in has this fixed constructor result and mounted application surface. “Handle” means
the constructor returns the typed weak handle directly; “runtime only” means it returns only the
runtime/Node and discards its trivial handle:

| Built-in | Constructor result | Mounted application state/events |
|---|---|---|
| `Checkbox` | handle + runtime | checked value and consumable change observation |
| `Button` | handle + runtime | consumable submissions |
| `ListItem` | handle + runtime | mutable label and consumable submissions |
| `ListBox` | handle + runtime | consumable submissions |
| `Combo` | handle + runtime | selected/open state, current label/anchor, selection operations, and consumable change/submit events |
| `TextBlock` | handle + runtime | mutable text |
| `ColorSwatch` | handle + runtime | mutable fill and label |
| `Slider` | handle + runtime | value/editing state and consumable changes |
| `Number` | handle + runtime | value/editing state and consumable changes |
| `Textbox` | handle + runtime | text, cursor, and change/submit events; focus remains router-owned |
| `TextArea` | handle + runtime | text, cursor, scroll, and change/submit events |
| `Custom` | runtime only | no mounted application state; it uses `State = ()` |
| old `widgets::Node`/`NodeStateValue` | retired | replaced by exposed `DisclosureState` |

Every public dynamic built-in container (`Column`, `Row`, `Grid`, `Stack`, `Disclosure`, and
`ScrollArea`) returns its handle directly; explicitly fixed/internal constructors return only the
finished Node. The outcome never depends on parameters or caller-selected flags.

Initialization-only visual and base behavior data moves to Parameters: base font/options/config,
button content/fill, checkbox label, list-item icon, list-box label/image, custom name/options,
text wrapping, slider bounds/step/precision, number step/precision, and disclosure label/visual
variant/options. Current pre-handle mutations of these values become parameter builder methods.
They are not mounted mutation APIs. The migration intentionally does not preserve arbitrary
post-mount mutation of every field that happened to be public on the old combined structs.

P1.1 must include a complete symbol table classifying every old public field and method as
`Parameters`, exposed `State`, runtime-private/derived, or retired. Its acceptance language must not
claim that every formerly mutable public field moves into State.

### Application interaction lives in typed state

The application should not need a second node identity to learn what its widget did. Persistent
values and observations live in the widget-specific state:

- `CheckboxState::checked` exposes the persistent value;
- `SliderState::value` and `set_value` expose numeric state;
- `TextboxState` owns text and cursor; it does not own focus;
- `ComboState` owns open/selected state;
- widget states expose the exact consumable change/submit operations below;
- custom widgets define their own state and observation methods.

Each consumable event kind is a private saturating `u32` pending count. Public
`take_changed() -> bool` or `take_submitted() -> bool` consumes exactly one occurrence and returns
`false` only when none is pending. Events therefore persist across frames and hidden periods until
consumed. A built-in records at most one occurrence of each semantic event kind per input-driven
`Widget::update` invocation. Because one real input produces one update traversal, distinct raw
events are never collapsed into one semantic occurrence; occurrences from separate input
transactions accumulate. The fixed event API and recording points are:

| State | Public event API | Recording point |
|---|---|---|
| `CheckboxState` | `take_changed` | a user click actually toggles the checked value |
| `ButtonState` | `take_submitted` | a user click submits the button |
| `ListItemState` | `take_submitted` | a user click submits the item |
| `ListBoxState` | `take_submitted` | a user click submits the list box |
| `ComboState` | `take_changed`, `take_submitted` | `update_items` clamps a stale selection and records changed; a user header click toggles open and records submitted, matching current `CHANGE`/`SUBMIT` behavior |
| `SliderState` | `take_changed` | pointer, wheel, or text-edit interaction actually changes the value |
| `NumberState` | `take_changed` | pointer or text-edit interaction actually changes the value |
| `TextboxState` | `take_changed`, `take_submitted` | user text editing changes the buffer; the user submits it |
| `TextAreaState` | `take_changed`, `take_submitted` | user text editing changes the buffer; the user submits it |

Programmatic value setters (`set_checked`, `set_value`, `set_text`, cursor setters, and
open/close/select operations) do not record interaction events. This matches current behavior and
prevents feedback loops. `ComboState::update_items` clamping remains the one explicit compatibility
exception because current Combo reports `CHANGE` for that normalization; direct `select` remains
silent because the caller performs it after consuming the submitted popup-item event. `ACTIVE` is
runtime interaction/paint state for ordinary leaf widgets, not an application event, and is not
copied into their typed state. Root chrome separately exposes its persistent moving/resizing mode
through `RootState::is_active` because that mode is itself application-observable window state; it
is still not a consumable event.

`Widget::update` returns `()` and records typed events at the interaction decision point. Focus
selection and keyboard/text delivery remain authoritative router operations; `WidgetUpdateCtx`
exposes only the router-produced focused snapshot and cannot assign or clear focus. Input
consumption and capture come from `ContainerInputResult`/private routing state. Root chrome records
its values and events in `RootState` by exactly the same mechanism. Do not add a replacement result
store, generic event summary, root-only side channel, or widget-state focus command.

### Unified root chrome container, typed state, and lifecycle

A window root is retained UI, so it uses retained widget state. Every `WindowEntry` owns one
`WidgetTree` whose root node is a private `RootChromeContainer`. That container's `RootState` owns
exactly one application-provided child `Node`. Title, close, frame, body, and resize regions are
parts of this one container; they are not synthetic child nodes and do not have independent public
identity or state.

The public capability is:

```rust
#[derive(Clone)]
pub struct RootHandle {
    id: RootId,
    state: WidgetStateHandle<RootState>,
}

impl RootHandle {
    pub fn id(&self) -> RootId;
    pub fn state(&self) -> &WidgetStateHandle<RootState>;
}

pub struct RootState {
    // All fields private:
    // name: String,
    // options: WindowOption,
    // children: Children,              // exactly one application Node
    // rect: Recti,
    // visible: bool,
    // interaction: RootInteraction,    // private None/Moving/Resizing enum
    // pending_changes: u32,
    // pending_submissions: u32,
}

impl WidgetState for RootState {}
impl ContainerState for RootState {}

impl RootState {
    pub fn name(&self) -> &str;
    pub fn options(&self) -> WindowOption;
    pub fn rect(&self) -> Recti;
    pub fn is_visible(&self) -> bool;
    pub fn is_active(&self) -> bool;
    pub fn is_moving(&self) -> bool;
    pub fn is_resizing(&self) -> bool;
    pub fn take_changed(&mut self) -> bool;
    pub fn take_submitted(&mut self) -> bool;
}
```

`RootHandle` is cloneable and non-owning: cloning it clones the weak state capability, not the
window or tree. `RootId` remains the Copy lifecycle key used by Context methods, and callers pass
`handle.id()` to those methods. Root constructors consume the application node and return the pair
as one value:

```rust
impl<B: RendererBackend> Context<B> {
    pub fn create_window(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle;
    pub fn create_dialog(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle;
    pub fn create_popup(&mut self, name: &str, content: Node) -> RootHandle;

    pub fn set_root_rect(
        &mut self,
        root: RootId,
        rect: Recti,
    ) -> Result<(), RootMutationError>;
    pub fn set_root_size(
        &mut self,
        root: RootId,
        size: Dimensioni,
    ) -> Result<(), RootMutationError>;
    pub fn set_root_options(
        &mut self,
        root: RootId,
        options: WindowOption,
    ) -> Result<(), RootMutationError>;
    pub fn set_root_visible(
        &mut self,
        root: RootId,
        visible: bool,
    ) -> Result<(), RootMutationError>;
    pub fn bring_root_to_front(&mut self, root: RootId) -> bool;
    pub fn destroy_root(&mut self, root: RootId) -> bool;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootMutationError {
    UnknownRoot,
    Borrowed,
}
```

Creation preserves the current root-kind defaults: a window starts visible at the supplied rectangle
with `FRAME`; a dialog starts hidden at the supplied rectangle with `FRAME`; a popup starts hidden
at `Recti::default()` with `FRAME | AUTO_SIZE | NO_RESIZE | NO_TITLE`. `name` is immutable after
creation. All three roots create the chrome container and live `RootState` immediately, so their
handles work while hidden and do not depend on a first rendered frame.

The internal ownership shape is:

```text
Context
  -> modal_stack: Vec<RootId>            // last() is the private cross-root routing gate
  -> WindowEntry {
       id: RootId,
       kind / z-order,
       root_state: WidgetStateHandle<RootState>, // framework-only weak clone
       tree: WidgetTree,
     }
       -> Node::Container(private RootChromeContainer runtime)
            -> RootChromeContainer.state      // sole persistent strong RootState owner
                 -> RootState.children         // exactly one application Node
```

The final private builder consumes the unique application node rather than borrowing/cloning it:

```rust
struct RootChromeParameters {
    name: String,
    options: WindowOption,
    rect: Recti,
    visible: bool,
    content: Node,
}

struct RootChromeBuilder;

impl ContainerBuilder for RootChromeBuilder {
    type Parameters = RootChromeParameters;
    type W = RootChromeContainer;

    fn create_container(parameters: RootChromeParameters) -> Self::W {
        let children = core::iter::once(parameters.content).collect();
        RootChromeContainer {
            state: Rc::new(RefCell::new(RootState::new(
                parameters.name,
                parameters.options,
                parameters.rect,
                parameters.visible,
                children,
            ))),
        }
    }
}

impl WidgetStateOwner for RootChromeContainer {
    type State = RootState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}
```

`RootState::new` and `RootChromeParameters` are private. The constructor asserts that `children`
contains exactly one node; no later code changes that count.

`RootState` deliberately has no public child getter or topology mutation API. The internal child is
the immutable application-root slot, while a dynamic application supplies an exposed persistent
`Column`, `Stack`, or another container as that child and mutates its descendants. There is no
`replace_root`, `set_root_node`, or equivalent operation that preserves `RootId`. Changing the
literal application-root type requires `destroy_root` followed by creation of a new `RootHandle`
with a fresh, never-reused `RootId`.

`RootState`'s name/options/rectangle/visibility methods are the only root query APIs. Remove the
parallel `Context::root_rect` and `Context::root_visible` queries; after destruction the weak state
handle is no longer alive and its ordinary access methods return `None`. Root mutation remains on
`Context` because it also
coordinates z-order, front-root selection, backend viewport state, and transient input cleanup:

- `set_root_rect(root, rect)` and `set_root_size(root, size)` mutate `RootState` through
  `WindowEntry.root_state` and emit no typed event. They preserve an in-progress move/resize, with
  the next captured delta applied from the new programmatic rectangle. If `AUTO_SIZE` is enabled,
  the next `update_ui` synchronization measure replaces width/height but preserves the
  programmatic origin;
- `set_root_options(root, options)` mutates the same authoritative state silently. Enabling
  `NO_TITLE` clears a current move; enabling `NO_RESIZE` or `AUTO_SIZE` clears a current resize; the
  matching tree capture is released before another pointer event routes;
- `set_root_visible(root, false)` keeps the tree alive, silently clears moving/resizing state plus
  invalid focus/hover/capture/routed targets, and emits no submission. Hiding the active dialog
  removes it from the stack, exposing the previous dialog or disabling the modal gate if none
  remains;
- `set_root_visible(root, true)` raises the root. A window/dialog keeps its rectangle. Before a
  hidden popup is shown, any other visible popup in that Context is silently hidden and sanitized
  without recording a submission. The new popup is repositioned at the current pointer with a
  `1 x 1` seed rectangle and receives its content-derived auto-size during the next `update_ui`
  synchronization before input drains. Showing keeps application state and pending events but does
  not restore cleared transient
  targets. Switching popups is atomic: Context resolves both entries and obtains the required
  checked state borrows before changing either; if either state is borrowed, the operation returns
  `RootMutationError::Borrowed` and leaves both visibility states unchanged. Showing a dialog also
  makes it the active modal root, clears transient input targets in every other root, and keeps it
  above windows and popups;
- `bring_root_to_front(root)` changes only z-order and emits no typed event. It does not change
  modal ownership; raising any inactive root cannot place it above the active dialog;
- mutation by an unknown/destroyed ID returns `RootMutationError::UnknownRoot`. Trying a Context
  mutation while a state-handle closure currently borrows that same `RootState` returns
  `RootMutationError::Borrowed`; callers finish the closure and retry. A weak-upgrade failure for an
  extant `WindowEntry` is an internal ownership invariant panic. `bring_root_to_front` and
  `destroy_root` need no state borrow and return `false` for an unknown/destroyed ID. Destroying the
  active dialog removes it from the stack, exposing the previous dialog or disabling the modal
  gate.

Modal routing is a strict cross-root gate, not a second input queue or a tree-local option. While
`modal_stack.last()` returns a root, hit testing, pointer capture lookup, keyboard/text selection,
and full input-driven update traversal admit only that root. A pointer event outside its rectangle
selects no recipient and cannot dismiss or activate an underlying popup/window. Activation immediately
sanitizes focus, hover, capture, and routed targets in every other tree, preventing a pre-modal
capture from bypassing the gate. Layout and paint still visit every visible root. The modal root is
re-raised after any attempted fronting/showing of another root so visual and input ordering cannot
diverge.

Chrome events use the same private saturating `u32` counters and one-occurrence `take_*` contract as
built-in widget events:

- `take_submitted` records a left-button press in the title-close rectangle or a pointer-button
  press outside an eligible visible popup. Either action first
  clears active chrome interaction/capture, hides the retained root, and then leaves one pending
  submission for the application to consume;
- `take_changed` records at most one occurrence per `RootChromeContainer::update` invocation when a
  user title drag or resize actually changes `RootState::rect`; constraint normalization that does
  not change the rectangle records nothing;
- `is_active`, `is_moving`, and `is_resizing` are persistent current-state queries, not consumable
  events. Moving and resizing are mutually exclusive, and `is_active()` is exactly their union.
  Matching pointer release, loss of capture during sanitization, incompatible options, hiding, and
  destruction clear them;
- a left press starts move/resize even if no delta follows. Movement applies the routed pointer
  delta with saturating coordinate arithmetic; resize applies it with the shared outer minimum-size
  clamp. Each input-driven update increments `pending_changes` only if that one event changes the
  rectangle; distinct move events run distinct updates and accumulate distinct occurrences;
- programmatic geometry, visibility, z-order, options, creation, and destruction never increment a
  pending event count;
- events persist across frames and hidden periods until consumed, and occurrences from separate
  updates accumulate. A later show does not erase them.

`RootChromeContainer` owns chrome execution rather than delegating it to a parallel window-manager
interaction path. Its phase contract is exact:

| Phase | Root chrome responsibility |
|---|---|
| `Widget::measure` | measure the one application child through `Children::measure_child`, add title/frame/body extents, and enforce the outer minimum size with explicit constraints |
| `Container::layout` | use the shared pure chrome-geometry helper to derive title, close, body, and resize rectangles, then lay out the application child in the body rectangle |
| `Container::route_input` | on left press, give close/title/resize regions precedence, assign the chrome event to this transaction and return `Consumed` for close or `Captured` for title/resize; consume other pointer presses over chrome without starting an action; return `Ignored` for body events so ordinary child routing descends into the application node |
| `Container::retains_pointer_capture` | report whether `RootInteraction` is currently `Moving` or `Resizing`; the tree clears its capture if this local mode was invalidated by options, hiding, release, dismissal, or sanitization |
| `Container::on_pointer_capture_lost` | clear `RootInteraction` when the tree ends capture because of release, hiding, gating, transient-target sanitation, or replacement; receive no target ID or loss reason |
| `Widget::update` | consume the optional routed chrome event for this transaction, hide/submit on close, enter or leave moving/resizing state, mutate the rectangle during captured drag, and record a change only for an actual user-driven rectangle change |
| `Widget::paint` | paint the window/frame background underlay before descendants from the shared geometry; ordinary traversal paints the application child in the body clip |
| `Container::children_visible` | return the current `RootState::is_visible` value, allowing a close handled by the parent update to suppress application-child traversal in that same input transaction |

One pure `root_chrome_geometry` helper is the source of truth for measure/layout, chrome hit-testing,
underlay/overlay paint, outer/body conversion, min-size enforcement, and backend viewport bounds.
Its contract is concrete rather than a collection of phase-local formulas:

```rust
struct RootChromeGeometry {
    outer: Recti,
    client: Recti,
    title: Option<Recti>,
    close: Option<Recti>,
    body: Recti,
    resize: Option<Recti>,
    minimum_outer: Dimensioni,
    intrinsic_outer: Dimensioni,
}

fn root_chrome_geometry(
    outer: Recti,
    child_intrinsic: Dimensioni,
    name: &str,
    options: WindowOption,
    style: &Style,
    atlas: &AtlasHandle,
) -> RootChromeGeometry;
```

It clamps every intermediate rectangle/size to non-negative extents, applies frame insets first,
places an optional title strip inside the client, places the square close region at the title's
trailing edge, and assigns the remaining client to the body. Resize exists only without
`AUTO_SIZE | NO_RESIZE` and occupies the shared bottom-trailing affordance. Before frame insets, the
minimum is `1 x 1` for `AUTO_SIZE` and `96 x 64` otherwise. A title raises width to at least measured
title text plus close width plus twice `Style::padding`; title height is
`max(style.title_height, font_height + 2 * max(style.padding / 2, 1))`, and close width equals that
height when close is enabled. A title raises outer height to title height for
auto-size and title height plus twice padding otherwise. Twice the frame-border width is then added
on each axis when `FRAME` is enabled. Checked inset accumulation overflow is an invariant panic with
a root-chrome diagnostic; returned extents are non-negative. `intrinsic_outer` additionally contains
the measured application child. Close outranks resize, resize outranks title, and all three outrank
body during hit testing. Measurements may call this pure helper before and after child measurement
to convert constraints, but no phase owns a different formula.

After descendant paint, the window compositor reads `RootState` once and appends the title fill/text,
close icon, and resize affordance as a post-tree overlay using that same helper. This preserves the
required z-order without synthetic chrome nodes or a second chrome interaction/update path.

During the initial `update_ui` synchronization and every post-event layout commit, a visible
`AUTO_SIZE` root is measured through the internal chrome node with explicit constraints and its
width/height are silently normalized in `RootState`; this is not a user change event. Each event
routes and updates chrome through the tree, then the mandatory post-event layout commits any
user-mutated rectangle before another event. If the container's own update hides the root on close,
traversal rechecks root visibility before descending, so application-child update and layout are
skipped for that root; the later paint-only render also skips it.

Generic `WidgetTree` capture owns title drag and resizing just as it owns container capture. The
captured private root runtime ID receives matching left-button drag/release events outside its
current bounds without a second hit test. `RootChromeContainer::retains_pointer_capture` reads its
own `RootInteraction`, while the tree remains the sole owner of the captured ID. Every centralized
tree transition away from that capture invokes `on_pointer_capture_lost` if the runtime remains
mounted; release and sanitation therefore clear the tree ID and local interaction without a second
window-manager owner.
The window manager must not keep a second chrome-capture flag or independently reinterpret raw
pointer input. Chrome hit regions outrank the body only within the front eligible root, after
cross-root/modal gating by the window manager.

Popup outside-click is necessarily detected at that cross-root boundary rather than by ordinary
inside-tree hit routing. There is no `just_opened` frame heuristic: the input transaction that
caused application code to show a popup has already completed before the application observes its
typed event, and therefore cannot be delivered again. On any later dequeued press outside the
visible popup, before routing that same press elsewhere, the window manager upgrades its private
weak `RootState` handle and invokes a framework-private dismissal operation that clears active
chrome state/capture, hides the popup, increments the same pending-submission counter, and sanitizes
that tree's transient targets. If application code shows a popup while older input is still queued,
those queued events are intentionally subsequent UI input and receive normal dismissal/routing;
callers that are reacting to a UI event show it only after that event has drained. This is the sole
special boundary hook; it does not create a second result or event mechanism. Failure to upgrade
means the entry is internally inconsistent and is an invariant panic, not a silently missing
application event.

At most one popup is visible in a Context. Showing popup B while popup A is visible silently hides A,
clears A's transient targets and capture, preserves A's state and pending events, and records no
submission because the transition is programmatic. B then receives the ordinary pointer placement,
z-order, and next `update_ui` layout synchronization. Nested popup ownership and dismissal form a separate future
feature and are not inferred from multiple independent popup roots. A later press outside the sole
visible popup dismisses it once, then routes that same press once to the highest eligible remaining
root; the popup cannot receive both a boundary dismissal and an inside-tree chrome event for that
press.

`destroy_root` immediately removes the matching `WindowEntry`, releases retained ownership of the
private chrome container and complete application subtree, and returns `true`. Unknown or
already-destroyed IDs return `false`; IDs are never reused. With no active state access, release
drops the tree synchronously. An already-active access holds only its temporary upgraded `Rc`; a
root-state access can therefore defer physical destruction of `RootState.children` until its closure
returns, but the removed tree is immediately unmounted and can never traverse again. Every clone of
the root and descendant weak state handles become non-live and return `None` after all such active
access closures release their temporary upgrades. Destruction never returns the `Node`, tree, pending events, or
state. Hiding is distinct: title close, popup dismissal, and
`set_root_visible(false)` retain both the tree and weak-handle liveness, and application code may
later destroy the root for permanent removal.

### Visibility boundaries

There is no generic node-visibility feature. Root visibility is the `RootState` value coordinated
through Context/`WindowEntry` policy.
`Container::children_visible` is a narrower traversal gate used by containers such as Disclosure:
when it returns `false`, descendants remain owned and their weak handles remain live, but they make
no measurement/layout contribution and receive no input, update, paint, or custom-render call.
Focus, hover, capture, and any current transaction recipient targeting the hidden descendant subtree are cleared
at the next safe sanitization point and are not automatically restored when traversal resumes. The
container itself still measures, updates, and paints. Its own `Widget::measure`/`layout`
implementation is responsible for omitting hidden descendants consistently with the traversal gate.

### Runtime lifecycle after direct topology mutation

Focus, hover, pointer capture, and the current routed recipient remain private `RuntimeNodeId` values in the owning
`WidgetTree`. Direct state mutation means removal does not call Context cleanup synchronously.
Instead, every target use is liveness-checked against the retained tree, and the normal frame
boundary sanitizes targets that no longer exist before routing new input.

Required rules:

- a missing focus/capture/current-event target is cleared and never redirected;
- IDs are never reused, so stale state cannot target a replacement node;
- there are no public per-node or per-root result entries to preserve or transfer; all application
  observation lives in typed state;
- removal drops the node immediately even if runtime target cleanup occurs at the next safe tree
  boundary;
- mutation of the child collection currently borrowed by traversal returns `None` without invoking
  the mutation closure;
- cross-subtree topology mutation is observed according to deterministic traversal order.

This is safe without a registry or Context callback because runtime targets are scalar IDs, not
pointers. Focused tests must pin removal-before-`update_ui`, removal-during-update in another subtree,
capture removal, and replacement behavior.

### Authoritative ownership

| Datum | Authoritative owner |
|---|---|
| Common measure/update/paint/options/focus behavior | concrete runtime implementing `Widget`, including every `Container: Widget` runtime |
| Container-only child/layout/special-input/local-capture-lifecycle behavior | concrete runtime implementing `Container` |
| Widget/container state lifetime | the concrete `WidgetStateOwner` runtime's private strong `Rc<RefCell<T>>` |
| Hidden runtime-only caches/configuration | ordinary concrete boxed widget/container fields |
| Application state capability | `WidgetStateHandle<T>` returned explicitly by concrete constructors when meaningful |
| Construction input and public return shape | specific `Parameters`; each concrete constructor documents whether it returns a handle with the runtime/finished node or only the runtime/node |
| Container children | `Children` inside the concrete container state cell |
| Generic child placement policy, derived layout, and transient interaction flags | `NodeRuntime`; no generic visibility or Grid field |
| Grid child placement spans and track definitions | `GridState`; private `GridItems` keeps each span index-matched with its owned child |
| Descendant traversal visibility | concrete `Container::children_visible`, principally `Disclosure` |
| Focus/hover/capture/routed input | owning `WidgetTree`, keyed by private `RuntimeNodeId` |
| Whether a captured container's own local interaction remains active, and how that local mode ends when tree capture is lost | that captured concrete container through `Container::retains_pointer_capture` and `Container::on_pointer_capture_lost`; ancestors only gate descendant eligibility |
| Widget action/value observation | widget-specific `WidgetState` |
| Root application content | private `RootState.children`, containing exactly one application `Node` and exposing no public topology mutation |
| Root geometry/visibility/chrome interaction/events | `RootState`, strongly owned by the private `RootChromeContainer` and exposed weakly through `RootHandle` |
| Root lifecycle identity | never-reused `RootId` inside `RootHandle`; `Context`/`WindowEntry` changes lifetime only by creation and `destroy_root` |
| Root cross-window policy/z-order/backend viewport | `WindowEntry`/window manager, using its framework-internal weak `RootState` handle |
| File-dialog UI, navigation, and terminal transition | Context-owned private controller and retained root; application code receives a `FileDialogSession` that can only poll a stable status snapshot |
| File-dialog cancellation | explicit `Context::cancel_file_dialog(&session)`, Cancel action, title-bar close, or abandonment of the last session handle; every terminal transition destroys the retained root automatically |
| State/topology access eligibility | checked borrow of the target cell only |

## What remains complicated

- **Inherent:** retained ownership, recursive layout, deterministic event routing, focus/capture
  cleanup, scroll geometry, clipping, and custom rendering remain real UI-runtime work.
- **Induced by compatibility:** current examples directly mutate structs that presently combine
  parameters and state. Each built-in needs an explicit split and migration guide.
- **Induced by Rust borrowing:** a concrete widget needs a strong state owner while application
  handles are weak; container recursion must scope child borrows so same-cell mutation fails safely.
- **Accidental and removed:** generated projection identity, state reconciliation, strong handle
  cloning, `WidgetStateHandleDyn`, phase/state adapter duplication, mounted state metadata, Context
  tokens, frame mutation flags, Context container editors, and parallel public widget IDs.

## Current inconsistencies and known defects

1. `WidgetHandle<T>` is documented as application ownership and keeps removed widget state alive.
2. Cloning one strong handle permits the same state to be projected into multiple positions, with a
   frame-time duplicate-dispatch panic as the guard.
3. Generated node identity and widget-handle allocation identity represent the same logical widget
   differently.
4. `Context::set_root_nodes` rebuilds complete roots and transfers only generic node runtime state.
5. The file dialog rebuilds its shell and persistent controls when only directory rows change.
6. Current public state access uses infallible `RefCell` borrows and can panic on reentrancy.
7. Current built-in structs mix immutable parameters, mutable application state, rendering caches,
   and runtime phase behavior, obscuring the public state surface.
8. The current erased handle adapter clones strong handles to recover `Widget` dispatch that a
   direct `Box<dyn Widget>` already provides.
9. Scroll area is represented by several synthetic semantic nodes sharing state.
10. Disclosure recreates erased widget adapters across phases.
11. Dynamic unkeyed list interaction follows builder position rather than the logical state object.
12. Public README terminology does not consistently match the current builder API.
13. The crate already exports a header/tree widget named `Node`, colliding with the target owning
    tree node; its `NodeStateValue` API is not classified by the original leaf migration list.
14. A public `ContainerState::children_mut` or raw `Container` callback permits safe
    `mem::swap`/`mem::replace` of complete attached child collections, contradicting the no-reparent
    contract.
15. The target requires typed root state and destruction, but the current API has neither
    `RootHandle`/`RootState` nor `destroy_root`; current chrome interaction is not represented by
    the target typed-state mechanism.
16. Current generic node `visible` state is copied but otherwise unused, leaving an ownership-table
    feature with no mutation or phase semantics.
17. Treating the concrete-runtime owner insertion boundary as late release hardening would force a second construction
    migration; P1 must establish it before the bulk conversion.
18. `ResourceState` currently has no retained-runtime responsibility once result stores are removed;
    focus, capture, routing, widget events, and target root events all have dedicated mechanisms.
19. `UiRuntime::measure_auto_size` uses a `10_000` pseudo-unbounded height that can leak into
    flexible row/grid intrinsic size.
20. Routed events and raw `Input` independently derive interaction during one frame, allowing
    geometry, focus, click, active, or wheel delivery to disagree.
21. The current pipeline performs three tree layouts: pre-input, an unconditional pre-update repeat,
    and post-update.
22. Row/grid/stack/column measurement and allocation do not share one axis solver, so intrinsic size
    can disagree with final tracks and spans.
23. Moving an unmounted `Node` into a closure passed to `try_update` would drop it if state access
    fails before invoking that closure.
24. The two object-safe child visitor methods cannot type-enforce that a downstream implementation
    submits the same authoritative `Children` collection from both methods.

Known defects are not compatibility requirements. Characterization preserves user-visible geometry
and interaction, not projection rebuilding, strong state lifetime, generated IDs, or synthetic
scroll nodes.

## Target invariant index

This is a compact audit index of the normative Target architecture, not an independent place to
change behavior. Amend the defining contract and its P0 criterion first, then update this index.

1. `Widget` is the only common runtime phase contract for leaves and containers, and its final
   `update` method accepts `Option<&UiInputEvent>` and returns `()` rather than a routed batch or
   generic leaf result.
2. Public `Container: Widget` adds only opaque child visitation, layout, descendant visibility,
   special routed-input behavior/current-owner inspection, and the defaulted local
   `retains_pointer_capture`/`on_pointer_capture_lost` hooks and is implementable downstream; it
   does not redeclare measure/update/paint or lend raw child collections to ordinary callers.
   Returning `Captured` acquires tree-owned capture; the hooks can only report or clear that
   container's local mode and cannot acquire, transfer, or identify tree capture.
3. `WidgetState` is data only and `ContainerState: WidgetState` is marker-only; implementing either
   does not implement runtime phases or grant generic child access.
4. `WidgetBuilder` associates one concrete parameter type with one concrete
   `W: WidgetStateOwner` and constructs that concrete runtime directly.
5. Every concrete `WidgetStateOwner` owns the only persistent strong `Rc<RefCell<T>>` for its
   `T: WidgetState`; `Node` erases it only after generic insertion validates the ownership trait.
   There is no parallel owner wrapper or public raw trait-object-box insertion contract.
6. Application state handles are typed, weak, cloneable without `T: Clone`, and contain
   no Context or node identity.
7. State access uses non-escaping closures and checked per-cell borrows; `try_update_with` preserves
   and returns owned input if upgrade/borrow fails before closure invocation.
8. `ContextFrame` existence has no effect on state or container-state borrow eligibility, but a
   state-access closure must finish before retained update/layout/paint traversal begins in the same
   Context. Layout-affecting mutation after the last UI commit requires dropping an unsubmitted
   frame and calling `Context::update_ui` again; retained traversal never crosses Context boundaries.
9. A same-cell conflict between checked handle accesses returns `None`; runtime phase access
   encountered through unsupported top-level render reentrancy may panic with the documented
   diagnostic. Framework-authorized child measurement/layout/visitor recursion is exempt.
10. Each container state's private `Children` field is the unique owner of its child nodes;
    built-in state APIs expose only safe inherent topology operations.
11. Successful insertion consumes a unique `Node`; successful removal drops it and never returns it.
12. No attached node can be cloned, detached, shared, moved, or reparented through framework-provided
    safe APIs; neither built-in state nor ordinary `Container` callers can obtain a raw mutable
    `Children` or any `&mut Node`.
13. `RuntimeNodeId` is private, globally unique for the process lifetime, and never stored in state
    handles.
14. Runtime targets are validated before use and stale IDs never alias replacement nodes.
    Ancestor `children_visible` gates decide descendant eligibility; only the current captured
    container reports its own local continuation state through `retains_pointer_capture` and clears
    it through `on_pointer_capture_lost`, while `WidgetTree` remains the authoritative capture owner.
15. Widget actions, values, and application commands are observed through typed state, not public
    node identity. Built-in event kinds use saturating pending counts consumed one occurrence at a
    time; ordinary programmatic setters are silent.
16. `ResourceState`, frame-result generations, and generic result lookup do not exist. Widget,
    container, and chrome updates return no generic result; application observation is typed state,
    while focus, capture, and routing use their dedicated mechanisms.
17. Every `WindowEntry` owns one `WidgetTree` rooted by the private `RootChromeContainer`; its
    strongly owned `RootState` contains exactly one application child and is exposed through the
    weak state capability inside `RootHandle`.
18. `Context::destroy_root` immediately unmounts/releases the chrome container and application
    subtree, never reuses the `RootId`, and lets all weak handles expire after already-active access
    closures release their temporary upgrades; hiding retains the tree and pending typed events.
19. Public root reads use only `RootState`; Context root mutations coordinate external effects and
    return `RootMutationError::{UnknownRoot, Borrowed}` rather than duplicating state or silently
    losing a same-cell mutation.
20. A mounted root's application child cannot be replaced while retaining `RootId`; dynamic content
    uses an exposed persistent application container.
21. Generic node visibility does not exist. Root visibility and container-owned descendant gating
    are separate, fully specified mechanisms.
22. Traversal follows owned nodes and borrows container children through framework-created opaque
    visitors without a registry, downcast, raw pointer, or Context lookup.
23. Both child visitor methods submit the same authoritative `Children` exactly once. Zero or
    multiple submissions panic with a diagnostic; same-collection consistency is a safe downstream
    conformance obligation.
24. Container layout state may change after mounting only through the fixed Row/Grid/Stack/Scroll
    APIs; attached `Policy` never changes, while Grid-owned `GridSpan` may change through
    `GridState::set_span` without changing child identity.
25. Raw input is queued and normalized once in call order. Ordinary retained interaction derives
    only from the one current routed event; cross-root popup outside dismissal is the sole boundary
    exception and consumes that same event before routing it onward. `Context::update_ui` performs
    one synchronization layout and, for every dequeued input, one full eligible-tree update followed
    by one complete layout before the next event. An empty queue performs zero widget updates.
    `ContextFrame::render_ui` paints/submits only. Intrinsic layout uses explicit bounded/unbounded
    constraints plus shared axis allocation.
26. Root chrome uses the same routed-input, update, capture, typed-state, and pending-event machinery
    as other containers. Cross-root popup dismissal is the only private boundary injection and
    records into that same `RootState`.
27. P1 establishes direct generic insertion of concrete `WidgetStateOwner` runtimes before bulk
    migration; raw trait-object-box insertion is never a public checklist or released compatibility
    boundary.

## Priority and completion rules

- **P0 — Baseline and wanted-behavior freeze:** characterize supported current behavior, freeze the
  currently wanted state/builder/container/event/root contracts, and route any later preservation
  trade-off through explicit change control.
- **P1 — Ownership:** establish the final weak-handle/concrete-runtime ownership traits and direct
  runtime boxing at `Node` before bulk migration or projection deletion.
- **P2 — Runtime mechanics:** migrate layouts, disclosure, scroll, routing, and cleanup onto direct
  state-owned topology.
- **P3 — Application boundary:** migrate roots, examples, custom widgets, and the file dialog to
  fixed constructor-returned state handles and owned nodes.
- **P4 — Correctness:** pin dynamic-mutation semantics and repair scroll, intrinsic measurement,
  axis allocation, window-boundary sizing, and transform edge cases against the simplified
  representation.
- **P5 — Cleanup and measurement:** remove all obsolete adapters/identity/reconciliation and optimize
  only measured hot paths.
- **Release validation:** verify the P1 concrete-runtime ownership boundary, rerun the complete validation
  matrix, and only then permit external release.

An item is complete only when production code, focused tests, affected examples, public docs, and
named obsolete-code removal land together. Temporary adapters must be crate-private and have a named
deletion point inside the same P1 owner item. P0-P5 remain internal integration milestones until the
final validation pass; do not merge/tag/release an incomplete migration or leave two externally
supported widget construction or ownership models. The sole availability exception is the
plan-owner-approved, source-preserving file-dialog disable from P1.2 through P3.2: its implementation
and tests remain tracked in place behind commented compilation/integration edges, and P3.2 must
restore them before any release validation can pass.

## Ordered checklist

### P0 — Baseline and wanted-behavior freeze

P0 records two different kinds of guardrail before migration begins. P0.0 characterizes supported
current behavior with green executable tests and measurements. P0.1-P0.7 freeze the currently
wanted replacement behavior and its acceptance criteria so implementation changes cannot weaken it
implicitly. Those wanted criteria may initially be test specifications rather than passing tests;
their named P1/P2 owners make them executable and green.

This freeze is a change-control baseline, not an immutable promise. If an owner discovers a material
trade-off, it must follow “Document authority and behavior change control” above and obtain an
explicit decision before changing the criterion.

- [x] **P0.0 — Characterize current supported behavior and structural cost**

  **Problem**

  Ownership, widget construction, application observation, topology, and root chrome/lifecycle change
  together. Existing tests often assert generated IDs or rebuilding details.

  **Decision needed: No**

  **Implementation owner: P0.0; rerun and compare in P5.1**

  **Target contract or migration**

  Add deterministic characterization for widget phase order, current built-in typed mutations,
  custom widgets, committed button/text submission, focus, container layouts, dynamic lists,
  disclosure, scrolling, custom rendering, and root lifecycle. Record allocations and phase counts
  for one widget, a 100-node tree, a scroll area, and idle/refresh file-dialog processing.

  **Acceptance tests**

  - `cargo test --all-targets` is green before production migration.
  - Tests assert visible state/geometry/input outcomes rather than builder hashes.
  - Baselines demonstrate current strong-handle duplication, erased adapter count, root rebuilding,
    synthetic scroll-node count, the `10_000` auto-size probe, raw/routed interaction duplication,
    and three tree-layout phases.
  - Characterization records that the old header/tree `Node` is publicly exported, generic
    `UiNodeState::visible` has no behavioral effect, whole `Children` swapping is currently possible
    through the proposed raw mutation API, and no typed root-state/destruction API currently exists.
  - Every known defect is assigned below rather than frozen as desired output.

  **Recorded evidence (2026-07-29)**

  The pre-migration gate passed before the P0.0 additions with 164 tests passed, zero failed, and
  the existing manual render benchmark ignored. The completed characterization adds green tests
  for the current widget phase sequence, typed built-in mutation, committed button/textbox events,
  focus, row/grid/stack geometry, keyed dynamic-list reorder, disclosure gating, scrolling, root
  lifecycle/replacement, and downstream public custom-widget/custom-render integration. These
  tests assert state, geometry, rendered content, event results, and callback order rather than
  builder hash values.

  After adding the characterization, `cargo test --all-targets` passes 172 unit tests and one
  downstream integration test with zero failures; the existing render baseline and two new UI-node
  baselines are intentionally ignored in the ordinary suite.

  Test-only runtime counters record the current common traversal phases without changing production
  control flow. One leaf frame performs three tree layouts, six runtime measurement requests, three
  layout dispatches, one update, and one paint. The concrete old widget adapter performs three
  additional widget measurements from its layout forwarding path, yielding the characterized
  `measure x6 -> update -> measure x3 -> paint` widget-call sequence. A pointer press on that leaf
  produces both routed-event dispatch and a raw-input interaction derivation in the same frame.

  The following release baseline was recorded with Rust 1.97.1
  (`x86_64-unknown-linux-gnu`, LLVM 22.1.6). Allocation columns count successful heap allocation or
  reallocation calls and requested bytes. Construction covers tree/root creation after Context
  setup; steady frames are measured after two warm-up frames. Timing is informational and is not a
  compatibility threshold.

  | Scenario | Nodes | Erased adapters | Build allocs | Build bytes | Steady allocs | Steady bytes | Tree layouts | Measures | Layouts | Updates | Paints | ns/frame |
  |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
  | One widget | 1 | 1 | 14 | 2,821 | 19 | 1,397 | 3 | 6 | 3 | 1 | 1 | 9,451 |
  | 100-node tree | 100 | 99 | 717 | 88,691 | 1,957 | 172,364 | 3 | 1,194 | 300 | 100 | 100 | 1,286,853 |
  | Scroll area with 20 content widgets | 25 | 20 | 258 | 32,749 | 497 | 42,244 | 3 | 324 | 75 | 25 | 25 | 294,298 |

  File-dialog measurements cover `eval` plus the resulting rendered frame. The refresh row also
  includes deterministic directory enumeration after adding one entry. Both paths replace the
  complete root projection once per evaluation.

  | Scenario | Nodes | Erased adapters | Allocs | Bytes | Root rebuilds | Tree layouts | Measures | Layouts | Updates | Paints | ns/eval+frame |
  |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
  | File dialog idle | 41 | 24 | 612 | 57,782 | 1 | 3 | 861 | 123 | 41 | 41 | 874,403 |
  | File dialog refresh | 42 | 25 | 718 | 63,511 | 1 | 3 | 894 | 126 | 42 | 42 | 837,152 |

  The structural characterization separately pins facts that are evidence of migration cost, not
  desired compatibility: projecting one retained application handle raises its strong owner count
  from one to two; each ordinary projected leaf adds one `WidgetStateHandleDyn` adapter; a scroll
  area adds one viewport, two scrollbar tracks, and one corner synthetic descendant; auto-size
  supplies the numeric `10_000` height probe; `UiNodeState::visible = false` does not suppress
  traversal; and the current raw `children_mut` surface permits swapping whole child collections.
  The downstream test confirms that the old header/tree `Node` remains publicly exported. Source
  inspection confirms there is no public typed `RootState`, `RootHandle`, or `destroy_root` API.

  Reproduce the complete P0.0 evidence with:

  ```text
  cargo fmt --all -- --check
  cargo test --all-targets
  cargo test --release ui_node_p0_baseline -- --ignored --nocapture --test-threads=1
  ```

- [x] **P0.1 — Freeze the runtime/state separation contract**

  **Problem**

  Current built-in values combine application data with runtime phase behavior. The target roles
  must be separated without changing which trait owns phase execution.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.0, P1.1, P1.3, P2.3, P2.4, and the P2.5 input-transaction amendment**

  **Wanted behavior and contract**

  Keep `Widget` as the sole common runtime phase trait but change `Widget::update` to accept
  `Option<&UiInputEvent>` and return `()`.
  Add marker `WidgetState` and `WidgetParameters`, `WidgetStateOwner: Widget`, and associated-type
  `WidgetBuilder`. A builder associates `Parameters` with one concrete `W: WidgetStateOwner` and
  `WidgetBuilder::create_widget` returns that runtime directly. The concrete runtime privately owns
  its sole persistent strong `Rc<RefCell<State>>`; its `state_handle` method returns only the typed
  weak capability. Concrete convenience constructors decide through their ordinary return type
  whether to return that handle alongside the runtime. There is no generic optional exposure policy,
  owner wrapper, or framework state-allocation factory. `WidgetState` has no measure/update/paint
  behavior and is never used as a substitute dispatch trait. Do not add identity, mount, child, or
  Context methods to `Widget`.

  Apply the same construction boundary to containers through associated-type `ContainerBuilder`:
  its concrete `W: Container + WidgetStateOwner` owns the strong state cell and is erased directly
  when passed to `Node::container`. Public built-in convenience constructors return their typed state
  handle plus a completed `Node`; internal fixed constructors may discard an internal/trivial handle
  and return only `Node`. `ContainerState: WidgetState` remains marker-only and does not acquire child
  access or runtime behavior.

  Specify the replacement of the current crate-private `Container: NodeBehavior` coupling with the
  final public object-safe
  `Container: Widget`. Keep `ContainerState` as the separate marker-only data trait. Add only the
  container-specific opaque child visitors, exact indexed layout services, descendant visibility,
  full/sub-rectangle routed-input/current-owner hooks, and defaulted local
  pointer-capture-retention/loss hooks shown above; do not
  duplicate `Widget` measurement, update, paint, option, or focus methods. Public `Container` must
  be implementable by downstream crates without exposing private tree machinery; P2.3 deletes
  `NodeBehavior` after direct `NodeKind`/`Widget` dispatch lands. This P0 item freezes those
  signatures but does not publish an unusable partial container surface: public `Container`,
  `ContainerState`, `Children`, visitors, scoped contexts/results, owning `Node`, and downstream
  container compile tests land atomically in the P1.3/P2.1 compile-safe batch.

  **Acceptance tests**

  - Compile-time signature tests pin the public `Widget` methods/defaults and prove `update` takes
    `Option<&UiInputEvent>` and returns `()` with no routed batch or generic result summary.
  - `CheckboxState` can be mutated without exposing measure/update/paint.
  - Concrete `Checkbox` owns its state `Rc` and dispatches after direct generic boxing with no erased
    state-handle trait or parallel owner wrapper.
  - A stateless/internal proof runtime owns `Rc<RefCell<()>>`; discarding its weak handle does not
    change that ownership or runtime behavior.
  - An external custom leaf implements `Widget`, `WidgetState`, `WidgetParameters`, and
    `WidgetBuilder` without private APIs.
  - The P1.3/P2.1 batch's external custom-container test implements `Widget`, `WidgetStateOwner`,
    `Container`, and marker `ContainerState`; proves the concrete runtime retains the sole persistent
    strong state cell and returns its matching weak handle; constructs `Children` through
    `new`/`FromIterator`; supplies the same authoritative collection exactly once through both opaque
    visitors; measures/layouts it through the exact public scoped operations; constructs it through
    `ContainerBuilder::create_container`; and enters the tree through public generic
    `Node::container` without private APIs.
  - Container conformance tests panic with the specified diagnostics when either opaque visitor
    receives zero or multiple `Children` submissions; downstream documentation states the
    same-authoritative-collection obligation that safe Rust cannot enforce across the two methods.
  - Ordinary downstream code cannot construct `ChildrenVisitor`/`ChildrenVisitorMut`, install a raw
    child callback, or obtain a `Children` borrow from `ContainerState`.
  - A compile-time supertrait check proves every `Container` is a `Widget`; `Container` declares no
    second measure/update/paint methods.
  - A downstream container that returns `Captured` may override
    `retains_pointer_capture` using only its private local state and
    `on_pointer_capture_lost` to clear that state when the tree ends capture. The defaults keep
    ordinary capture valid and make loss notification a no-op, and neither hook can acquire,
    transfer, inspect, or clear the tree-owned capture ID.
  - `ContainerInputCtx::has_pointer_capture` reports only whether the current container owns tree
    capture, allowing an established captured gesture to distinguish direct delivery from ordinary
    hit routing; it exposes no ID and cannot mutate capture.
  - Public API/source checks find no `ContainerOption::RETAIN_POINTER_CAPTURE`, parent capture
    override, concrete-container downcast, or state-to-tree/Context capture callback.

  **Frozen contract evidence (2026-07-29)**

  The normative target signatures and ownership diagrams above now define one complete separation
  boundary for both leaves and containers: concrete runtime objects implement `WidgetStateOwner`
  plus `Widget` (and, for containers, `Container`), application data implements marker
  `WidgetState`/`ContainerState`, and each concrete runtime retains its persistent strong state cell.
  Application capabilities are weak typed handles returned explicitly by concrete APIs; no state
  trait becomes a phase-dispatch adapter.

  Repository inspection confirms that the current implementation still returns `ResourceState`
  from `Widget::update`, stores application/runtime state together, clones strong `WidgetHandle`
  values into `WidgetStateHandleDyn`, and couples crate-private `Container` to `NodeBehavior` with
  raw child-slice access. These are recorded migration gaps rather than preserved behavior. At the
  original freeze, P0.1 intentionally changed no production API: its compile-time and runtime
  acceptance criteria are
  protected specifications that become executable and green in their named P1.0/P1.1/P1.3/P2.3
  owner batches, with the public container surface landing atomically rather than partially.

  **P2.4 capture-lifecycle amendment (2026-07-31)**

  The plan owner explicitly selected the defaulted public
  `Container::retains_pointer_capture` and `Container::on_pointer_capture_lost` hooks as the missing
  lifecycle half of `ContainerInputResult::Captured`, plus the current-owner-only
  `ContainerInputCtx::has_pointer_capture` query then needed by route-before-update batching. P2.4 added
  that narrow surface after the initial public container contract: `WidgetTree` keeps sole ownership
  of the capture ID, the captured container reports and clears only its private local interaction,
  and ancestors retain only their existing `children_visible` subtree-gating authority. A dynamic
  `ContainerOption`, parent-controlled retention, routing-as-validity-probe, synthetic input event,
  and concrete downcast are rejected by the normative decision above. This amendment changes no
  common `Widget` phase and adds no second container phase.

  **P2.5 input-transaction amendment (2026-07-31)**

  The plan owner replaced per-render routed batching with one complete update/layout transaction per
  queued input. `Widget::update` therefore takes `Option<&UiInputEvent>`: exactly the routed
  recipient gets the one localized event and all other eligible nodes get `None`. P2.5 removes the
  batch helper and batching-specific capture deferral/coalescing machinery. The scoped capture bool
  remains useful only to distinguish an already-captured direct delivery; it is no longer a bridge
  across multiple events before update.

- [x] **P0.2 — Freeze the runtime-owned weak-state contract**

  **Problem**

  Strong public handles outlive topology. Every runtime must own its state strongly, while public
  state access needs only a typed weak capability with liveness and checked borrowing. The ownership
  boundary must not require a second wrapper, generic exposure policy, or Context metadata.

  **Decision needed: No — simplified by explicit plan-owner decision after P1.0 usage audit**

  **Implementation owner: P1.0 and P1.3**

  **Settled decision and rationale**

  Each concrete `WidgetStateOwner` runtime privately retains the strong `Rc<RefCell<T>>` and returns
  a `WidgetStateHandle<T>` for that same allocation. Builders return their associated concrete
  runtime, not an optional handle/opaque-owner pair. A meaningful convenience constructor may return
  `(WidgetStateHandle<T>, Runtime)` or `(WidgetStateHandle<T>, Node)`; a stateless/internal
  constructor may return only its runtime or completed node and discard the trivial/internal weak
  handle. Present handles are weak, so node lifetime remains authoritative. Neither the strong owner
  nor the weak handle is a Context capability or a lock.

  A generic mandatory-or-optional exposure switch was rejected because the concrete constructor's
  return type already states what its callers receive. Strong application handles were rejected
  because they keep removed widget state alive and allow one state allocation to outlive or back
  multiple nodes. Raw `Weak` exposure was rejected because callers could upgrade it and let a strong
  clone escape; the typed handle deliberately provides only closure-scoped upgrades.

  Public `Dropped`/`Borrowed` error variants were also rejected after the P1.0 implementation audit:
  no production or later-plan consumer branches on the cause, `is_alive` already supplies the only
  separately useful liveness fact, and ownership-moving mutation needs the original input rather
  than an error wrapper. The smaller API returns `None` for unavailable ordinary access and
  `Err(input)` for unavailable ownership-moving access. Internal runtime traversal diagnoses an
  incompatible application borrow precisely at its invariant boundary; dropped ownership is
  impossible while the concrete runtime method is running.

  **Wanted behavior and contract**

  Implement `WidgetStateHandle<T>`, input-preserving `try_update_with`, `WidgetStateOwner`, and the
  associated concrete-runtime builders. Each runtime creates and privately stores one
  `Rc<RefCell<State>>`; `WidgetStateHandle::new(&runtime.state)` downgrades a borrowed owner reference
  without exposing the raw `Weak` or returning a strong pointer. `is_alive` reports whether that weak
  cell can still be upgraded without borrowing its contents; an already-active access operation's
  temporary upgrade therefore keeps it true after the concrete runtime is dropped and until that
  operation returns. Handles contain only `Weak<RefCell<T>>`; `try_read` and `try_update`
  use checked borrows and return `Some(closure_result)` on success or `None` without invoking the
  closure when ownership or borrowing makes state unavailable. `try_update_with` returns
  `Ok(closure_result)` on success or the exact uncommitted `Err(input)` on either unavailable path.
  `is_alive` separately reports the allocation-liveness fact; remove `replace` and expose no public
  access-error or failure-wrapper type. Document that access closures may not invoke retained
  update/layout/paint traversal, and use the shared internal runtime-borrow diagnostic for built-ins instead
  of adding a frame/state-access gate. The application top-level-render prohibition explicitly
  exempts framework-created child measurement/layout/visitor recursion.

  **Acceptance tests**

  - `Checkbox::create` returns a typed handle and concrete `Checkbox`; cloning the handle does not
    increase strong count.
  - A compile-time test clones `WidgetStateHandle<NonCloneState>` and proves the handle's `Clone`
    implementation has no `T: Clone` bound.
  - Dropping the concrete Checkbox runtime makes `is_alive` false and ordinary idle access
    return `None`.
  - `is_alive` does not borrow state: it remains true during an existing read or mutable access,
    remains true after that closure drops the concrete runtime because the active operation holds a
    temporary upgrade, and becomes false when the final owner/active upgrade is gone.
  - `try_read` and `try_update` return `Some(closure_result)` on success; both an expired cell and an
    unavailable live cell return `None` without invoking the closure. `is_alive` distinguishes the
    liveness fact only when a caller actually needs it.
  - `try_update_with` returns the exact uncommitted input directly as `Err(input)` without cloning,
    substitution, or a public failure wrapper.
  - A stateless/internal proof runtime retains its unit state with one strong `Rc`; discarding a
    freshly produced weak handle neither changes strong count nor disables runtime dispatch.
  - Constructor signature tests pin the complete built-in return shapes: application-meaningful
    state constructors return a typed handle with their runtime/finished node, while `Custom` and
    internal fixed-composition constructors return only their runtime/node. The old header/tree
    `Node` is retired rather than assigned a state return shape.
  - No public Parameters type contains a generic exposure flag, and no `EXPOSE_STATE`, optional
    factory result, exposure-selector type, or post-construction exposure operation exists.
  - Same-cell reentrancy returns `None`; cross-cell access succeeds.
  - `try_update_with(node, ...)` returns that exact unmounted node as `Err(node)` for either expired
    ownership or a live borrow conflict without invoking the closure; successful access moves it
    once, and an invalid `insert` returns it from the inner operation.
  - Checked borrow outcomes are identical before, during, and after a `ContextFrame` when borrow
    state is identical. A layout-affecting mutation after the last UI commit requires dropping an
    unsubmitted frame and calling `update_ui` again before paint.
  - Updating, laying out, or rendering any retained root of the same Context from inside
    `try_read`/`try_update` is documented as unsupported. When that traversal reaches the actively
    borrowed associated state and requests an incompatible borrow, a built-in runtime reports a
    precise invariant panic rather than skipping the widget or producing stale output. Retained
    traversal never crosses Context boundaries.
  - A downstream container performs authorized nested `Children::measure_child`,
    `ContainerLayoutCtx::layout_child`, and visitor traversal while its parent state borrow is active
    without triggering the top-level-reentrancy diagnostic.
  - No frame/state-access flag or gate is added to enforce the reentrancy precondition.
  - Downstream conformance tests construct a private state `Rc`, implement `WidgetStateOwner` with
    `WidgetStateHandle::new(&self.state)`, and prove the handle observes the same allocation used by
    runtime phases. Compile-fail checks prove callers cannot construct a handle from a raw `Weak` or
    extract its `Rc`/`Weak`; no Context token, frame flag, mount metadata, strong `Rc`, or raw `Weak`
    is returned as the application state capability or consulted during access.
  - Public API/source searches find no `StateAccessError`, `StateAccessFailure`, or equivalent
    public classification of unavailable handle access.

  **Frozen contract evidence (2026-07-29)**

  The normative ownership and access sections above now define the complete direct-runtime
  contract. Every concrete runtime allocates and retains one strong state cell; concrete constructor
  return types state whether callers also receive a weak typed capability.
  Liveness is allocation-based, borrow conflicts are per cell, ordinary unavailable access is one
  `None` outcome, failed ownership-moving updates return their exact input directly, and neither
  frame existence nor Context identity participates in state access. The fixed built-in table is
  the exhaustive constructor-return compatibility boundary. The post-P1.0 audit found no consumer that needs
  the unavailable cause as public data, so the earlier error enum/wrapper design was removed rather
  than carried into later items.

  P1.0 now implements this final handle and direct-runtime construction contract for the widget
  path: ordinary access methods return `Option<R>`, ownership-moving access returns `Result<R, I>`,
  the public error enum/failure wrapper and owner wrapper are absent, and focused ownership,
  lifetime, borrow-conflict, liveness, and input-preservation tests are green. The legacy
  `WidgetHandle<T>` remains only for unsplit widgets during migration and is not part of the final
  contract. P1.3 applies the same settled behavior to container construction and supplies the
  remaining container-specific acceptance evidence.

- [x] **P0.3 — Freeze the state-owned `Children` contract**

  **Problem**

  A Context-owned editor duplicates the state-handle access path and exists primarily to enforce a
  frame boundary that is not required for safe single-threaded borrowing.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.3, P2.0-P2.4, and P3.2**

  **Settled decision and rationale**

  Every concrete container state owns one opaque `Children`, and its concrete
  `Container + WidgetStateOwner` runtime owns the sole persistent strong `Rc<RefCell<State>>`. A
  public dynamic container convenience constructor returns its typed handle plus a completed `Node`;
  a fixed/internal constructor returns only its completed `Node` after discarding any trivial or
  internal weak handle. Application code changes membership through the typed state handle, with no
  Context argument, mounted identity, `ContainerHandle`, or second editor API.

  A Context-owned `ContainerEditor` was rejected because it requires public or handle-carried mount
  identity and duplicates checked state mutation. Rebuilding/replacing roots was rejected because it
  preserves allocation, generated-identity, and state-transfer work for a local child-list change.
  The accepted consequence is traversal-order observation: same-container mutation while that state
  is borrowed returns `None`, while mutation of another available container follows documented
  deterministic traversal order. Focus/capture cleanup occurs at the next safe tree boundary.

  **Wanted behavior and contract**

  Add public opaque `Children` with `new`, `Default`, and `FromIterator<Node>` construction plus
  `len`, `is_empty`, indexed `measure_child`, `push`, `insert`, `remove_drop`, `clear`, and `replace`.
  Add public marker trait `ContainerState: WidgetState` and concrete built-in state types whose
  state-local membership methods mirror the applicable `Children` operations. No built-in state
  exposes `Children`, `&Children`, or `&mut Children`.

  `Children::new`/`Default` are empty, and `FromIterator` preserves iterator order. `measure_child`
  returns `None` for an out-of-range index without exposing the node. `push` appends. `insert`
  accepts every index in `0..=len`; a larger index leaves the collection unchanged and returns the
  exact still-unmounted input `Node`. `remove_drop` returns `true` and drops the indexed owner when
  the index exists, or returns `false` without mutation otherwise. `clear` drops every current
  owner, and `replace` collects the input in order and drops the previous owners without returning
  them. `Children` is not `Clone` and exposes no public iterator, raw node access, detachment, or
  removal-and-return operation.

  Use `WidgetStateHandle<C>` for container state; do not add `ContainerHandle` or `ContainerEditor`.
  Every public dynamic built-in container constructor returns
  `(WidgetStateHandle<C>, Node)` and performs its concrete-runtime construction plus
  `Node::container` wrapping internally. An internal fixed container constructor may return only its
  completed `Node`. Callers do not select the return shape through Parameters or a separate API.
  The atomic P1.3/P2.1 compile-safe batch publishes `Container`, constructible `Children`, the opaque
  visitors/contexts, owning `Node`, `ContainerBuilder`, generic `Node::container`, the Column vertical
  slice, and Disclosure as the old public `Node` replacement. Grid's state-owned slice lands as an
  early P2.0 correction; Row/Stack finish P2.0 and ScrollArea lands in P2.2 using that
  already-complete foundation. Downstream compile tests exercise
  the Column/Disclosure/custom-container paths in the atomic batch and expand to every built-in as
  each later item lands.

  The framework-provided APIs enforce unique ownership and no-reparent behavior for built-ins and
  ordinary callers. Safe Rust cannot prevent a downstream custom state type from publishing its own
  raw `Children` access or swapping collections; preserving the same no-detach/no-reparent boundary
  is therefore an explicit safe custom-container conformance obligation, not an `unsafe` trait
  requirement.

  Successful direct removal or replacement drops the affected `Node` owner immediately. Its weak
  widget/container state handles expire after any already-active access upgrades finish. Private
  focus, hover, capture, and the current routed recipient are checked against the retained tree before use
  and sanitized at the next safe boundary; a stale target is cleared, never redirected, and a
  replacement at the same index does not inherit it. Regardless of which cross-container mutation
  a current traversal has already observed, its post-event layout and the next explicit UI commit
  are fully stable.

  The following target code is illustrative of the concrete construction and mutation contract:

  ```rust
  impl ColumnState {
      pub fn push(&mut self, node: Node) {
          self.children.push(node);
      }

      pub fn remove_drop(&mut self, index: usize) -> bool {
          self.children.remove_drop(index)
      }

      pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>) {
          self.children.replace(nodes);
      }
  }

  // Build the persistent file-list container once. The concrete Column runtime owns the
  // strong Rc<RefCell<ColumnState>>; the application receives only this weak handle.
  let initial_rows = directory_entries().map(file_row_node);
  let (files_state, files_node) =
      Column::create(ColumnParameters::new(initial_rows));

  // A directory refresh replaces only this container's children. It does not rebuild the
  // dialog root and does not require Context or a container/node ID.
  let new_rows = directory_entries().map(file_row_node).collect::<Vec<_>>();
  files_state.try_update_with(new_rows, |column, new_rows| {
      column.replace(new_rows);
  })?;

  // Successful removal drops the child in place; no attached Node is returned for reuse.
  files_state
      .try_update(|column| {
          assert!(column.remove_drop(3));
      })
      .expect("file-list state unavailable");
  ```

  A crate-private fixed-composition container implementation uses the same strong state ownership
  but defines a constructor that returns only the completed node:

  ```rust
  let toolbar_node = FixedGroup::create(
      FixedGroupParameters::new(fixed_toolbar_children),
  );

  // The concrete FixedGroup runtime inside Node still owns its
  // Rc<RefCell<FixedGroupState>> and Children.
  ```

  `FixedGroup` is the internal/test proof of the hidden-container path, not a mode of `Column`,
  `Row`, or another public constructor. A custom downstream container makes the same fixed choice in
  its own constructor.

  Checked borrowing defines conflicting and cross-container mutation without a frame gate:

  ```rust
  files_state
      .try_read(|_files| {
          // The same state cell is already borrowed.
          assert_eq!(files_state.try_update(|_files| {}), None);

          // A different available container remains independently mutable.
          sidebar_state
              .try_update_with(notification_node, |sidebar, notification_node| {
                  sidebar.push(notification_node)
              })
              .unwrap();
      })
      .expect("file-list state unavailable");
  ```

  **Acceptance tests**

  - Framework construction and mutation consume each unique `Node` into exactly one `Children`
    owner; no framework-provided safe operation clones, detaches, shares, moves, or reparents an
    attached node.
  - Empty construction, ordered `FromIterator`, append, insertion at zero/`len`, out-of-range
    insertion, valid/invalid `remove_drop`, empty/non-empty `clear`, ordered replacement, and
    out-of-range `measure_child` follow the exact boundary behavior above.
  - A failed `insert` returns the exact input node with its weak descendant handles still live; a
    successful removal/clear/replacement returns no node and makes removed-state handles become
    non-live and return `None` after any active access upgrade ends.
  - Same-container mutation during its traversal returns `None` without panic.
  - Mutation of another available container follows traversal order: current phases process the
    state they observe without rollback, and the transaction's layout/next explicit UI commit is
    fully stable.
  - Marker `ContainerState` has no methods, and public `Children` has no direct node iterator or API
    yielding an attached `Node`, `&Node`, or `&mut Node`.
  - Compile-fail tests prove `Children` is not `Clone`; built-in state handles cannot obtain
    `Children`/`&Children`/`&mut Children`; and ordinary callers cannot use `mem::swap`,
    `mem::replace`, or `mem::take` to move an attached built-in collection.
  - Ordinary downstream callers cannot construct the opaque child visitors. A second `visit` call
    panics immediately and zero calls panic after the container method returns, with diagnostics
    naming the immutable/mutable method and the exactly-one rule.
  - Built-ins and the downstream conformance example submit the same authoritative `Children` from
    both visitor methods; documentation states that safe custom implementations must do the same and
    must not expose attached collection/node extraction or reparenting, and that the framework
    cannot type-enforce those obligations across downstream safe APIs and the two object-safe calls.
  - No topology method accepts Context or stores Context identity.
  - Removing or replacing a focused, hovered, captured, or current-event target drops its owner,
    clears the stale private target at the next safe boundary, never redirects it, and does not
    transfer interaction state to a replacement at the same index.
  - The atomic-batch downstream compile test constructs Column and Disclosure from their completed
    `Node` returns without a wrapping step; P2.0/P2.2 extend the same test to each later built-in.
  - Construction tests grow with the rollout and ultimately prove every public dynamic built-in
    container returns a typed handle plus `Node`, while the fixed internal proof container returns
    only `Node`; both paths retain identical concrete-runtime strong-owner shape.
  - A downstream custom container implements public `Container`/`WidgetStateOwner`/
    `ContainerBuilder`, uses `Children::new` or `collect::<Children>()`, returns the weak state handle
    from its concrete runtime, and is accepted by generic `Node::container` without private APIs.
  - File-dialog refresh replaces only the file/folder list children and does not call
    `Context::set_root_nodes`.
  - Public documentation includes the construction, replacement, removal, hidden-container, and
    borrow-conflict examples above.

  **Frozen contract evidence (2026-07-29)**

  The normative container-ownership and runtime-lifecycle sections above now define one mutation
  path: each concrete state owns one opaque ordered `Children`, its concrete container runtime
  retains the strong state cell, and explicitly returned weak typed handles provide checked
  state-local membership changes.
  The operation boundaries preserve unique unmounted inputs on failure, commit ownership exactly
  once on success, never return an attached node, and separate immediate owner destruction from
  deferred scalar-target sanitization. The custom-container exception is documented as a safe API
  conformance obligation because Rust cannot enforce it across arbitrary downstream inherent
  methods.

  Repository inspection confirms that the current built-in containers instead own raw
  `Vec<UiNode>` fields behind crate-private `Container: NodeBehavior`; `children_mut` lends the
  complete vector, `remove_child` returns an attached node, builder assembly replaces whole child
  vectors, and tests can swap collections between attached containers. `Context::set_root_nodes`
  replaces complete root projections, and the file dialog rebuilds and installs its full tree for
  local directory-list changes. Those facts are migration cost, not preserved behavior. P0.3
  intentionally changes no production API: its ownership, boundary, compile-fail, lifecycle, and
  file-dialog criteria become executable and green in P1.3/P2.0-P2.4/P3.2.

- [x] **P0.4 — Freeze the typed-state interaction contract**

  **Problem**

  Handle-based Context result/focus APIs require node-to-state identity linkage. The application
  should only need its typed state capability.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.1, P2.5, and P3.0-P3.2**

  **Settled decision and rationale**

  Widget-specific persistent values, consumable events, and value mutation operations live in the
  concrete `WidgetState`. Application code observes or changes them only through
  `WidgetStateHandle<State>::try_read`/`try_update`; it does not pass the handle back to Context or
  retain a parallel widget `NodeId`.

  Generic handle-based Context results/focus were rejected because they require state handles to
  carry mounted node/Context identity or require Context to maintain a state-to-node registry.
  Public generated `NodeId` lookup was rejected because it makes applications retain state and
  placement identity together. The accepted migration cost is that each built-in defines the
  value and event operations appropriate to its own state type. Focus is excluded because the
  router, not widget state, owns the current focus target.

  **Wanted behavior and contract**

  Define state-specific observation exactly as specified in the fixed event table: saturating
  pending counts consumed one occurrence at a time through `take_changed`/`take_submitted`. Change
  `Widget::update` to return `()` and remove
  all generic result production and storage. Remove `ResourceState`, `FrameResults`,
  `FrameResultGeneration`, public `RetainedId`, public widget `NodeId`, every `state_of*` operation,
  and targeted Context focus. Root chrome uses `RootState` and the same pending typed-event contract;
  use the fixed built-in constructor/mutation table above rather than reclassifying in P1.1.

  Every pending event counter starts at zero. Recording uses `saturating_add(1)`, so `u32::MAX`
  remains `u32::MAX` rather than wrapping. A `take_*` call at zero returns `false` without changing
  state; otherwise it subtracts exactly one and returns `true`. Change and submission counters are
  independent: one input-driven `Widget::update` may record one occurrence of each kind, but never
  more than one occurrence of the same semantic kind. Exactly one raw input drives each update
  traversal, so separate input events are never collapsed and may record separate occurrences.
  Pending occurrences remain in the strongly owned state across render calls, hidden roots, and
  gated descendants until consumed or the owner is destroyed.

  Built-ins absent from the fixed event table expose no generic interaction event. In particular,
  ordinary leaf `ACTIVE` remains private runtime/paint state rather than a pending typed event.
  `RootState::is_active` is the intentional root-only persistent-state exception, while root change
  and submission occurrences use the same independent saturating counters; P0.7 owns their exact
  chrome recording points and programmatic-silence rules.

  Persistent values are read and changed directly through typed state:

  ```rust
  let checked = checkbox_state
      .try_read(CheckboxState::checked)
      .expect("checkbox state unavailable");

  checkbox_state
      .try_update(|checkbox| {
          checkbox.set_checked(!checked);
      })
      .expect("checkbox state unavailable");
  ```

  Button-like interactions use a consumable count rather than a one-frame generic result flag, so
  an unconsumed submission persists and submissions from separate update invocations are not
  collapsed:

  ```rust
  pub struct ButtonState {
      pending_submissions: u32,
  }

  impl WidgetState for ButtonState {}

  impl ButtonState {
      pub fn take_submitted(&mut self) -> bool {
          if self.pending_submissions == 0 {
              return false;
          }

          self.pending_submissions -= 1;
          true
      }

      pub(crate) fn record_submission(&mut self) {
          self.pending_submissions = self.pending_submissions.saturating_add(1);
      }
  }

  // ButtonWidget::update records the typed event at the click decision point and returns ().
  if save_button_state
      .try_update(ButtonState::take_submitted)
      .expect("button state unavailable")
  {
      save_document();
  }
  ```

  Focus is not a cooperative widget-state command. The private dispatcher selects the front
  visible root, and that root's router delivers keyboard/text events only to its focused node.
  `WidgetUpdateCtx::focused` exposes the router-produced snapshot for editing and paint decisions,
  but the context has no `set_focus` or `clear_focus` capability. In particular, a textbox records
  Return as a submission without clearing focus, so an application can clear or replace its text
  through `TextboxState` and the next keyboard/text event still reaches the same router-owned
  target.

  A unit/internal-state widget or container uses the same `Widget::update -> ()` signature. Its
  convenience constructor may simply omit a meaningless state handle from its return type:

  ```rust
  let decoration_runtime = Decoration::create(parameters);
  let decoration = Node::widget(decoration_runtime);
  // DecorationWidget::update performs its runtime work and returns no generic leaf result.
  ```

  **Acceptance tests**

  - Every change/submission listed in the fixed event table is observed through its typed state.
    Zero-count reads are stable, each successful `take_*` consumes exactly one occurrence, separate
    event kinds remain independent, and a test-only maximum counter proves saturation without wrap.
  - Each real input produces a separate full update traversal. One such invocation records at most
    one occurrence of each semantic kind, while occurrences from separate queued inputs accumulate
    and require separate `take_*` calls. An invocation that produces both change and submission
    records one independently consumable occurrence of each kind.
  - Unconsumed occurrences survive update/render calls, root hiding/showing, and descendant gating;
    destruction drops them with their owning state rather than publishing a final generic result.
  - Programmatic setters are silent; tests cover every setter plus Combo's documented
    `update_items` clamp exception and silent direct `select`.
  - Combo records consumable `CHANGE`/`SUBMIT` equivalents at the same clamp/header-click decision
    points as the current implementation.
  - Keyboard/text events are routed only through the front visible root and then only to that
    runtime's current focused node. Textbox submission leaves that focus intact, subsequent text
    continues to reach the same textbox, and neither `TextboxState` nor `WidgetUpdateCtx` exposes a
    focus-mutation command.
  - Checkbox, slider, combo, textbox, and custom state require no Context argument.
  - A custom `Widget::update` compiles only with the unit return and stores no generic leaf result;
    its own typed state may define custom events independently.
  - Ordinary leaf active/pressed state has no application pending-event API. Root active state and
    root change/submission counters follow the shared typed-state contract, with P0.7 tests pinning
    their exact chrome transitions.
  - Public compile-fail checks and internal source/API audits prove `ResourceState`, `FrameResults`,
    `FrameResultGeneration`, `RetainedId`, public widget `NodeId`, `Context::committed_results`,
    `state_of_retained`/every other `state_of*` lookup, and `Context::set_root_focus_node` no longer
    exist; no replacement generic result store, event summary, or targeted focus API is introduced.
  - Applications retain no parallel node IDs for widget interaction.
  - Calculator, demo, and file-dialog application code consume typed state instead of any generic
    frame-result lookup.
  - Public examples and rustdoc include persistent-value, consumable-event, router-authority, and
    hidden-runtime cases matching the code above.

  **Frozen contract evidence (2026-07-29)**

  The normative typed-state table and mutation sections above now define the sole application
  interaction boundary. Persistent values and independent saturating event counts live with their
  concrete state; `Widget::update` mutates that state and returns `()`; focus, capture, and routing
  retain dedicated runtime mechanisms without becoming application identity APIs or cooperative
  widget-state commands. Event lifetime follows state lifetime rather than a frame generation, and
  programmatic setters remain silent except for the explicitly preserved Combo normalization
  behavior.

  Repository inspection confirms that the current implementation instead returns the
  `ResourceState::{CHANGE, SUBMIT, ACTIVE}` bitflags from every `Widget::update`, double-buffers them
  in `FrameResults` maps keyed by public/scoped `RetainedId`, exposes the committed generation through
  `Context::committed_results`, and uses public builder `NodeId` values for result lookup and
  `Context::set_root_focus_node`. The calculator, full demo, file dialog, tests, and custom-widget examples still
  produce or consume parts of that surface. Those facts are migration cost, not preserved behavior.
  P0.4 intentionally changes no production API: its typed-event, API-removal,
  compile-fail, application-migration, and documentation criteria become executable and green in
  P1.1/P2.5/P3.0-P3.2.

  **P2.5 input-transaction amendment (2026-07-31)**

  The plan owner removed routed batching. The “at most one occurrence per update” rule now applies
  to one real input transaction, and distinct queued inputs always run distinct updates and may
  accumulate distinct pending occurrences. This changes no counter API, saturation rule,
  application observation surface, or programmatic-setter silence.

- [x] **P0.5 — Freeze the process-unique private runtime identity contract**

  **Problem**

  Focus, capture, and routed events require stable identity, but neither application state nor
  Context validation requires that identity.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.3 and P2.4**

  **Settled decision and rationale**

  Assign every `Node` one private, process-unique, monotonically increasing `RuntimeNodeId` at node
  construction. The scalar belongs only to retained runtime nodes and internal routing state. It is
  never exposed, never reconstructed by the application, never stored in `WidgetStateHandle`, and
  never reused during the process lifetime.

  Per-Context IDs were rejected because they move uniqueness into Context identity, mount-time ID
  assignment, and foreign-Context validation. Allocation addresses were rejected because freed
  addresses can be reused, allowing a stale focus/capture target to alias a later node unless a
  generation counter or eager global cleanup is added. Both alternatives relocate more complexity
  than the single process-wide counter requires.

  **Wanted behavior and contract**

  Use a private nonzero `u64` and one relaxed atomic allocation operation:

  ```rust
  use std::num::NonZeroU64;
  use std::sync::atomic::{AtomicU64, Ordering};

  static NEXT_RUNTIME_NODE_ID: AtomicU64 = AtomicU64::new(1);

  #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
  struct RuntimeNodeId(NonZeroU64);

  impl RuntimeNodeId {
      fn allocate() -> Self {
          let raw = NEXT_RUNTIME_NODE_ID
              .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                  current.checked_add(1)
              })
              .expect("RuntimeNodeId space exhausted");

          Self(NonZeroU64::new(raw).expect("allocator returned zero"))
      }
  }
  ```

  `Ordering::Relaxed` is sufficient because the operation establishes uniqueness only; it does not
  publish node memory or synchronize frame work. Exhaustion is an explicit invariant panic before a
  new node is returned. Do not make ordinary node construction fallible for an unreachable `u64`
  process-lifetime limit. Sequential allocations begin at one, are nonzero, and increase
  monotonically. A private/local arithmetic helper may make the exhaustion boundary testable, but
  tests never mutate, replace, or reset the process-global allocator.

  Public `Node::widget`, `Node::custom_render`, and `Node::container` all allocate the ID immediately
  without a Context or mount step:

  ```rust
  pub struct Node {
      id: RuntimeNodeId,
      runtime: NodeRuntime,
      kind: NodeKind,
  }

  impl Node {
      fn from_kind(kind: NodeKind) -> Self {
          Self {
              id: RuntimeNodeId::allocate(),
              runtime: NodeRuntime::default(),
              kind,
          }
      }
  }
  ```

  `Node::from_kind` is the only allocation point. `Node::widget`, `Node::custom_render`, and
  `Node::container` reach it exactly once; built-in container constructors do not allocate an
  additional identity around their returned node. Moving an unmounted node, configuring it through
  consuming `with_policy`, wrapping it in an unmounted `GridItem`, returning it from a failed
  `Children`/`GridState` insertion, and mounting it in any Context preserve the original scalar.
  Dropping even a never-mounted node does not return its ID to the allocator.

  Actual focus, hover, capture, and routed input are owned per retained tree, below Context and
  inside its `WindowEntry`. Widget state cannot request or clear focus; the authoritative focused
  node remains the private tree target:

  ```rust
  struct WindowEntry {
      id: RootId,
      root_state: WidgetStateHandle<RootState>, // private weak clone
      tree: WidgetTree,
      // Root kind, z-order, and backend viewport policy.
  }

  struct WidgetTree {
      root: Node,
      focus: Option<RuntimeNodeId>,
      hover: Option<RuntimeNodeId>,
      capture: Option<RuntimeNodeId>,
      current_event: Option<(RuntimeNodeId, UiInputEvent)>,
  }
  ```

  Before routing or using a stored target, validate it against the owning retained tree. If its node
  was explicitly removed, clear the target. A subsequently inserted node necessarily has a different
  ID and cannot inherit the stale target:

  ```rust
  if tree.focus.is_some_and(|id| !tree.contains(id)) {
      tree.focus = None;
  }

  if tree.capture.is_some_and(|id| !tree.contains(id)) {
      tree.capture = None;
  }
  ```

  The same liveness validation applies to `hover` and the optional `current_event` recipient. A
  missing hover target is cleared, a missing capture/focus target is cleared before it can direct
  another event, and a current event for a missing node is discarded rather than delivered or
  transferred. Tree
  membership is checked by retained-tree traversal; do not add a node registry merely to validate
  these private scalar targets.

  This protects explicit dynamic child removal/insertion; it does not imply projection or root
  rebuilding. Persistent nodes retain their original ID for their entire lifetime. Remove Context
  node counters (but not the separate public `RootId` counter), mount metadata, foreign-handle
  validation, pointer-derived IDs, and public widget ID composition. `RootId` keeps its independent
  public lifecycle role and counter; there is no conversion, equality bridge, scoping composition,
  or shared allocator between `RootId` and `RuntimeNodeId`. The prohibition on pointer casts applies
  to runtime node identity allocation and validation rather than unrelated backend implementation
  details.

  **Acceptance tests**

  - Sequential unit tests observe nonzero monotonically increasing IDs, and IDs remain unique across
    multiple Contexts and independent node construction streams.
  - `Node::widget`, `Node::custom_render`, `Node::container`, and every built-in container constructor
    allocate exactly one ID per returned node; private root-chrome construction allocates exactly
    one ID for its chrome node.
  - Moving/configuring an unmounted node, a failed insertion that returns it, and mounting it in any
    Context preserve its original ID. Dropping an unmounted node permanently consumes its ID.
  - Explicitly removing a child and later inserting another never reuses the removed ID; unaffected
    persistent nodes retain their IDs without rebuilding.
  - Counter exhaustion remains an invariant panic before returning a node. It is not a release-gate
    scenario and tests must not mutate or reset the process-global allocator; a local allocator
    helper may cover the arithmetic boundary if useful.
  - State handles contain no ID and can mutate state regardless of owning Context.
  - Each `WindowEntry`'s `WidgetTree` owns its focus, hover, capture, and ephemeral current routed
    recipient; no
    application state or top-level Context field becomes their authoritative owner.
  - Missing focus, hover, and capture targets are cleared, and a missing current transaction
    recipient is discarded; none is redirected or inherited by a later node.
  - Downstream compile-fail checks prove `RuntimeNodeId` cannot be imported, named, constructed,
    compared, formatted, or extracted through `Node`/`WidgetStateHandle`; `Node` exposes no public ID
    accessor.
  - `RootId` remains public and independently allocated, with no public/private conversion or
    composed runtime-node identity.
  - No Context token, mount state, node registry, or pointer-derived runtime identity exists.
  - Rustdoc states that `RuntimeNodeId` is runtime-private and unrelated to public `RootId` and
    `WidgetStateHandle`.

  **Frozen contract evidence (2026-07-29)**

  The normative owning-node and lifecycle sections above now define identity as one private scalar
  allocated exactly once with each unique `Node`. It survives ordinary Rust moves and pre-insertion
  configuration, never enters application state, and is never recycled. Each retained tree remains
  the authoritative owner of its own focus, hover, capture, and ephemeral current routed recipient;
  process-wide
  uniqueness prevents a stale target in any tree from aliasing a replacement or a node in another
  Context without Context tokens, mount metadata, or a registry.

  Repository inspection confirms that the current `Id` is a public `usize` wrapper constructible
  from pointers, caller integers, and strings. `UiNodeBuilder` derives public `NodeId` values from
  scope seeds, node tags, sibling order, and optional keys, then validates duplicate hashes;
  `WidgetHandle::id` separately casts its strong `Rc` allocation address; `RetainedId::root_node`
  composes root and builder identities; and scroll-area synthetic descendants hash IDs from their
  parent and semantic part. The useful current per-root `UiRuntime` ownership of focus, hover,
  capture, and routed targeting is retained conceptually, while those public, pointer-derived,
  scoped, and synthetic ID sources are migration cost rather than compatibility behavior. P0.5
  intentionally changes no production API: allocator, privacy, preservation, sanitization, and
  documentation criteria become executable and green in P1.3/P2.4.

- [x] **P0.6 — Freeze the owning-`Node` name and visibility boundaries**

  **Problem**

  The crate already exports a header/tree widget named `Node`, while the persistent-tree contract
  requires that name for its unique owner. The proposed `NodeRuntime` ownership table also treats an
  unused current `visible` field as if generic visibility were a supported feature.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.3, P2.1, and P2.4**

  **Settled decision and rationale**

  Reserve `Node` for the unique owning tree value and retire the old header/tree widget name
  completely. Keeping both concepts through a deprecated alias was rejected because it would leave
  two incompatible ownership meanings in the public API during an already-breaking migration.
  Renaming the owner to `RetainedNode` or `UiNode` was rejected because the persistent value is the
  fundamental tree node, while the old type is one disclosure presentation whose behavior already
  belongs in a stateful container.

  Do not turn the current `UiNodeState::visible` field into a feature. It has no public mutation path
  and does not gate any current traversal phase, so preserving it would invent behavior rather than
  retain supported behavior. Root visibility and container-owned descendant gating have different
  owners, lifecycle effects, and sanitization rules; keeping those two explicit mechanisms avoids a
  third generic hide/show command and the identity or handle machinery it would require.

  **Wanted behavior and contract**

  Reserve crate-root, `retained`, and prelude `Node` for the same unique, opaque, non-cloneable owning
  tree type. The `widgets` module does not export another `Node`. Remove the old public
  `widgets::Node`, `NodeStateValue`, `Node::header`, and `Node::tree` surface without a deprecated
  alias, wrapper, or second public construction path. P1.3 and P2.1 land the removal and owning-node
  export atomically; any compile-safe legacy bridge between those owner items remains crate-private
  and is deleted in that same batch.

  P1.1's public-symbol classification and P2.1's implementation preserve the old disclosure
  capability through this exact mapping:

  | Old header/tree surface | Final classification or replacement |
  |---|---|
  | `Node::header` plus builder-owned children | `DisclosureParameters::header(label, expanded, children)` |
  | `Node::tree` plus builder-owned children | `DisclosureParameters::tree(label, expanded, children)` |
  | `NodeStateValue::{Expanded, Closed}` and direct `state` mutation | private `bool` state changed through `DisclosureState::{is_expanded, is_collapsed, expand, collapse, toggle}` |
  | public `label`, `config`, and `with_options` | initialization-only `DisclosureParameters` data and private runtime configuration; not mounted state |
  | `is_header` / `is_tree` | no mounted query; the constructor selects a private visual variant |
  | click toggling, label/icon paint, header framing, tree hover treatment, and tree indentation | one `Disclosure` container runtime with the P2.1 phase and routing contract |

  Header construction retains the framed presentation, tree construction retains the unframed and
  indented presentation, and an explicit parameter option overrides the respective default as it
  does today. Expansion is the only retained mounted state from `NodeStateValue`; the replacement
  exposes no public compatibility enum.

  `NodeRuntime` stores `Policy`, derived layout, and transient interaction flags, but no generic
  visibility bit or parent-specific metadata. Add the consuming pre-insertion method
  `Node::with_policy(Policy)`, defaulting through `Policy::auto()` and preserving the node's
  already-allocated private runtime identity. Policy participates once in parent slot allocation.

  Grid placement is instead an explicit `GridItem { node, span }` insertion value. `GridSpan::new`
  clamps each zero component to one, and `GridState` owns the index-matched span after insertion.
  `GridState::set_span` changes that parent-child edge without replacing the attached node. Non-grid
  parents neither carry nor ignore Grid metadata because it never enters their retained
  representation.

  Do not add `Node::show`, `hide`, `set_visible`, `is_visible`, or an equivalent generic handle/state
  command. Root hide/show remains a Context-coordinated `RootState` mutation with the P0.7 contract.
  Descendant gating remains the narrower `Container::children_visible` mechanism: a false gate keeps
  the container itself active and its descendant nodes/state handles owned and live, but descendants
  contribute no measure/layout and receive no input, update, paint, or custom-render callback.
  Sanitization clears their focus, hover, capture, and current transaction recipient without restoring those
  targets when the gate later reopens. These two mechanisms are independent and neither writes a
  generic node visibility field.

  **Acceptance tests**

  - Crate-root, `retained`, and prelude exports resolve `Node` to the same owning type; `widgets`
    exports no type or alias named `Node`, and no second public `Node` definition exists.
  - Compile-fail/API-surface tests prove `NodeStateValue`, legacy `Node::header`/`Node::tree`, and a
    clone operation on the owning `Node` no longer exist.
  - Disclosure header/tree tests pin initial expanded/collapsed state, predicates, explicit
    expand/collapse/toggle, label/icon paint, framed versus unframed defaults, option override, tree
    indentation, hover treatment, and click toggling without a public visual-variant enum/query.
  - `Node::with_policy` preserves runtime identity and defaults through `Policy::auto()`.
    `GridItem::spanned` preserves the same node identity, and zero columns/rows clamp independently
    through `GridSpan::new`.
  - Row/Column/Grid tests prove policy is applied once through `ContainerLayoutCtx`; Grid consumes
    only its state-owned span. Compile-fail/API checks prove generic `Node` and
    `ContainerLayoutCtx` expose no Grid placement field or accessor.
  - Production searches find no generic node `visible` field and no node-level `show`, `hide`,
    `set_visible`, `is_visible`, or equivalent generic state/handle command.
  - Root hide/show tests prove the complete retained root is gated while state and weak handles stay
    live. Disclosure collapse tests prove the container remains active, descendants stay owned/live,
    every descendant phase is skipped, transient targets are cleared, and expansion restores none.
    The two mechanisms do not alter one another.
  - Rustdoc and migration notes reserve `Node` for ownership, map every retired header/tree symbol to
    Disclosure or intentional removal, and distinguish root visibility from descendant gating.

  **Frozen contract evidence (2026-07-29)**

  The normative owning-node, Disclosure, and visibility sections now define one public meaning for
  `Node`, one pre-insertion placement surface, and exactly two visibility boundaries with explicit
  owners. The legacy header/tree presentation and expansion behavior survive through Disclosure;
  the unused generic visibility bit does not become compatibility behavior.

  Repository inspection confirms that `widgets::Node` is currently a cloneable combined
  state/runtime widget with public `label`, `state`, and `config`, public header/tree constructors and
  predicates, header `FRAME` and tree `NONE` defaults, and click-driven `NodeStateValue` toggling.
  It is exported from `widgets`, crate root, and the prelude, while the current `retained` module has
  no owning `Node` export. The current Disclosure container holds a strong `WidgetHandle<Node>`,
  recreates an erased widget adapter across phases, and gates its nested Column by reading that
  expansion state. Characterization and downstream tests exercise those facts as migration evidence.

  `UiNodeState::visible` currently initializes to `true` and is copied during projection-state
  transfer, but production traversal never reads it; the P0 characterization explicitly sets it to
  `false` and still observes descendant traversal. Actual current root visibility instead lives on
  `WindowEntry`, while Disclosure already performs container-local expansion gating. Existing
  `NodeOptions` establishes the intended `Policy::auto()`/`GridSpan::ONE` defaults and
  `GridSpan::new` zero clamping. During the projection bridge, policy moves to the unique node while
  span stays on private `BuilderChild` edge metadata and is converted to `GridItem` only by a Grid
  parent. P3.0 deletes this temporary builder span transport.

  P0.6 intentionally changes no production API: owning-name/export, legacy Disclosure replacement,
  placement, visibility removal, traversal gating, target sanitization, compile-fail, and
  documentation criteria become executable and green in the atomic P1.3/P2.1 batch and P2.4.

- [x] **P0.7 — Freeze unified root chrome, popup, and destruction behavior**

  **Problem**

  Root destruction is required for deterministic tree lifetime, but no operation currently removes
  a `WindowEntry`. Chrome is currently special-cased by the window manager and has no typed retained
  state, while keeping a root-only result mechanism would duplicate widget-state observation.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.4, P2.4, P2.5, P3.0, and P4.2**

  **Settled decision and rationale**

  Model every root as retained UI with the same typed weak-state capability and concrete-runtime
  strong ownership as ordinary widgets and containers. A parallel `WindowEntry` chrome-state/result API was rejected
  because it would duplicate geometry, visibility, interaction state, event lifetime, and checked
  borrowing outside the retained model. Synthetic title/close/resize child nodes were rejected
  because those chrome regions have no independent application identity or topology and would add
  routing/lifetime machinery without adding capability.

  Keep hiding and destruction distinct. Hiding is a reversible policy/state transition that retains
  the root tree, application state, and pending events; destruction is the irreversible lifecycle
  operation that removes the `WindowEntry` and releases retained ownership. Replacing the one
  application child while preserving `RootId` was rejected because it would reintroduce root
  projection replacement and ambiguous weak-handle lifetime. Dynamic content belongs in a
  persistent application container; changing the literal root type destroys and recreates the root.

  **Wanted behavior and contract**

  Add public `RootHandle`/`RootState`, private `RootChromeContainer`, and
  `Context::destroy_root(RootId) -> bool` with the exact ownership, weak-liveness, hidden-versus-
  destroyed, query, mutation, and pending-event semantics defined above. Change all root constructors
  to consume one application `Node` and return `RootHandle`; the private container owns that node as
  its one immutable child slot. Remove the complete generic result family rather than retaining a
  root exception.

  `RootHandle` contains the public lifecycle `RootId` and one weak
  `WidgetStateHandle<RootState>`. Cloning or dropping a handle never changes root lifetime while its
  Context remains live. The private `RootChromeContainer` runtime is the sole persistent strong
  `RootState` owner; `WindowEntry` retains only a framework-internal weak clone. `RootState`
  owns the immutable name, current options/rectangle/visibility, mutually exclusive moving/resizing
  mode, independent saturating change/submission counts, and exactly one application child. It
  exposes only the query/event methods specified above and no child getter or topology mutation.

  Root lifecycle transitions are fixed as follows:

  | Operation | Retained ownership and state | Events and transient targets |
  |---|---|---|
  | `create_window(name, rect, node)` | create a visible `FRAME` root at `rect`, raise it, and return a live `RootHandle` before any frame | no pending event; no interaction targets |
  | `create_dialog(name, rect, node)` | create a hidden `FRAME` root at `rect` with a live handle | no pending event; no interaction targets |
  | `create_popup(name, node)` | create a hidden root at `Recti::default()` with `FRAME | AUTO_SIZE | NO_RESIZE | NO_TITLE` and a live handle | no pending event or frame-based input suppression |
  | `set_root_visible(id, false)` | retain the complete tree/state and current pending events; remove a dialog from `modal_stack`, exposing the previous entry when the active dialog is removed | silently clear chrome mode plus focus/hover/capture/routed targets |
  | title close / eligible popup outside press | retain the complete tree/state but make the root hidden; remove a closed dialog from `modal_stack` | clear chrome/transient targets and record one typed submission |
  | show a window | retain its rectangle, make it visible, and raise it below any active dialog | preserve pending events; restore no transient target |
  | show a dialog | retain its rectangle, make it visible, move it to the end of `modal_stack`, and raise it above every other root | preserve its pending events and clear every other tree's focus/hover/capture/routed targets |
  | show a hidden popup | atomically hide any visible popup, position the new popup at the current committed pointer with a `1 x 1` seed, and raise it; the next `update_ui` synchronizes auto-size before draining input; an active dialog is immediately re-raised above it | old popup records nothing; both popup trees are sanitized; new pending events survive, but the popup is input-blocked while the dialog remains active |
  | show an already-visible popup | raise it below any active dialog without repositioning it | record nothing and preserve current state; it remains input-blocked while the dialog is active |
  | `destroy_root(id)` | immediately remove the entry and unmount/release its complete tree without returning any owned value; remove a dialog from `modal_stack`, exposing the previous entry when the active dialog is removed | record nothing; all targets disappear and weak handles expire after active access upgrades |

  Root IDs remain independent from private runtime-node IDs and are never reused after creation or
  destruction. Counter exhaustion is an invariant panic before a new root is registered. Dropping
  Context releases all remaining roots; while Context remains live, only `destroy_root` ends one
  registered root's lifetime. Unknown and already-destroyed IDs remain observationally identical.

  All public root reads occur through `RootState`. `set_root_rect`, `set_root_size`,
  `set_root_options`, and `set_root_visible` return `RootMutationError::UnknownRoot` when no entry
  exists and `RootMutationError::Borrowed` when the required live state cell is unavailable, without
  partial mutation. An extant entry whose weak state cannot upgrade is an invariant panic.
  `bring_root_to_front` and `destroy_root` need no state borrow and return `false` only when the ID is
  unknown; successful fronting changes z-order only, does not inspect `RootState`, and cannot move
  any inactive root above the active dialog. Showing a dialog is the operation that activates it.

  Programmatic rectangle and size setters write the requested geometry silently and preserve an
  active move/resize baseline. The next `update_ui` synchronization layout silently enforces the
  outer minimum before input drains; `AUTO_SIZE` instead replaces width/height with measured
  intrinsic size while preserving origin. A hidden root retains its requested rectangle until it is
  shown. Option changes are silent and synchronously clear incompatible chrome mode: `NO_TITLE`
  clears moving, while `NO_RESIZE` or `AUTO_SIZE` clears resizing, with matching tree capture
  released before another pointer event. Repeating `set_root_visible(false)` is a silent retained
  no-op after sanitization; repeating `true` still raises a window/dialog, while the already-visible
  popup behavior is fixed by the table above.

  Move chrome measure/layout/routing/update and its paint underlay to `RootChromeContainer` and one
  shared pure geometry helper. After tree paint, let the window compositor append only the chrome
  overlay from that helper so title/close/resize remain above descendants. Use ordinary `WidgetTree`
  capture for moving/resizing. `RootChromeContainer::retains_pointer_capture` reports whether its
  private `RootInteraction` is still moving/resizing, and `on_pointer_capture_lost` clears that mode
  when the tree ends capture, without either hook owning or receiving the captured ID.
  Keep only cross-root/modal selection, z-order, backend viewport
  coordination, popup outside-hit detection, and this post-tree compositing boundary in the window
  manager; outside dismissal calls the framework-private `RootState` operation before routing the
  press elsewhere. Title close and popup dismissal hide and record a typed submission; they do not
  destroy. Programmatic root operations are silent. Add no application-child replacement operation;
  document persistent-container content and root destroy/recreate behavior.

  The one pure `root_chrome_geometry` helper implements the exact normative formula above and is the
  only source of frame/title/close/body/resize rectangles, intrinsic and minimum outer sizes, and
  client/outer conversion. Close outranks resize, resize outranks title, and all three outrank body.
  The retained container paints the frame/window underlay before its application child; after tree
  paint the compositor reads `RootState` once and adds only the title/close/resize overlay. The
  window manager owns no second chrome interaction state, hit-test formula, capture flag, or update
  path. P4.2 finishes the explicit-constraint and backend-viewport verification against this same
  helper rather than introducing another formula.

  Only a left press can close, start moving, or start resizing. Other pointer-button presses over
  chrome are consumed without starting an action; body input falls through to the application child.
  The private root runtime ID participates in ordinary `WidgetTree` capture, so matching drag and
  release continue outside root bounds. Starting move/resize without a delta changes current mode
  but records no change. One update records at most one change when its final rectangle differs from
  its entry rectangle; separate updates accumulate, and release/sanitization/incompatible options/
  hiding clear current mode. Programmatic mutations and constraint normalization never record an
  event.

  Popup outside dismissal is the sole cross-root input boundary. There is no `just_opened`
  suppression: the input transaction that produced a typed event is already drained before
  application code observes that event and shows the popup. Any subsequently dequeued pointer-button
  press outside the sole visible popup hides and sanitizes it, records exactly one submission through
  `RootState`, and then routes that same press exactly once to the highest eligible remaining root.
  Showing popup B while popup A is visible first obtains both checked state borrows;
  `RootMutationError::Borrowed` leaves both roots, z-order, and targets unchanged. Nested popup
  relationships remain out of scope.

  **Acceptance tests**

  - Each window/dialog/popup constructor consumes exactly one non-cloneable application `Node` and
    returns a `RootHandle` whose ID identifies the Context entry and whose weak `RootState` handle
    observes the same geometry/visibility/options used by chrome. `RootState` is live before the
    first UI update or render.
  - Creation tests pin visible/hidden status, initial rectangle, immutable name, exact default
    options, initial inactive mode, zero pending counters, z-order behavior, and exactly one private
    chrome node plus one immutable application child for all three root kinds.
  - Cloning/dropping `RootHandle` changes neither strong owner count nor root lifetime. Dropping
    Context releases every remaining root; while it remains live, only `destroy_root` removes one.
  - Destroying an existing root returns `true`, immediately unmounts/releases the chrome container
    plus application subtree, and makes root/descendant handles become non-live and return `None`
    after active access closures finish; a root-state closure is the only temporary reason physical
    child drop may lag.
    A destroy invoked while that root state is actively borrowed still succeeds and the removed tree
    can never traverse again.
  - Destroying an unknown or already-destroyed root returns `false`; later creation never reuses the
    ID, and exhaustion panics before registration rather than wrapping or aliasing.
  - Every Context root setter distinguishes `RootMutationError::UnknownRoot` from
    `RootMutationError::Borrowed` and is atomic on failure;
    public root reads occur only through the checked `RootState` handle, and no parallel Context
    query remains. An internal weak-upgrade failure for an extant entry is an invariant panic.
  - Rectangle/size setters, minimum normalization, auto-size, options, visibility, fronting,
    creation, and destruction are event-silent. Tests pin immediate requested geometry, next-
    `update_ui` minimum/auto normalization, preserved drag baseline, and origin preservation under
    `AUTO_SIZE`.
  - Title close and popup outside-click increment `RootState`'s pending submission count while
    retaining a hidden tree; a subsequent explicit destroy removes it. Zero-count takes are stable,
    separate occurrences accumulate with saturation, and each successful take consumes one.
  - Only left press activates close/move/resize. An input transaction completes before application
    code can observe its typed event and show a popup, so that event is never delivered twice. The
    next dequeued pointer-button press outside dismisses before routing exactly once to the root
    behind it. Other buttons over ordinary chrome are consumed without activating it.
  - At most one popup is visible per Context. Showing another silently hides the previous popup,
    clears its transient targets without recording a submission, and gives the new popup no
    frame-based suppression. The switch is atomic and returns `RootMutationError::Borrowed`
    without changing either popup if one of the required state cells is unavailable. Nested popups
    remain a separate feature.
  - Re-showing an already-visible popup raises it without changing its rectangle or input policy;
    reopening a hidden popup applies current-pointer placement exactly once.
  - Drag/resize exposes current active/moving/resizing state and increments `take_changed` only when
    user interaction actually changes the rectangle. A press without movement changes mode but not
    the counter; each input-driven update records at most one change, and separate movement events
    accumulate because they run separate updates. Movement uses saturating coordinates, and resize
    uses the shared minimum clamp.
  - Matching release, hiding, lost/sanitized capture, and incompatible options clear active state;
    moving and resizing never overlap, and `is_active()` is exactly their union.
  - Root chrome returns `true` from `retains_pointer_capture` exactly while moving/resizing is
    locally active. Clearing that mode causes `WidgetTree` to release its captured root ID before
    another pointer event. When the tree ends capture first, `on_pointer_capture_lost` clears the
    mode at the specified immediate/deferred lifecycle boundary; the container never receives or
    mutates that ID.
  - Multiple root change/submission events persist across frames and hide/show cycles under the same
    independent saturating-count rules as widget events, and showing consumes none.
  - Root chrome is exactly one internal semantic container node, with no title/close/resize child
    nodes and no window-manager-owned parallel chrome capture/update state; the compositor retains
    only the documented post-tree overlay step.
  - The shared geometry helper yields identical rectangles for layout, routing, paint, min-size,
    outer/body conversion, and backend viewport integration. Tests cover every WindowOption
    combination, tiny/negative extents, title-text minimums, frame insets, close/resize precedence,
    non-negative outputs, and checked-overflow diagnostics.
  - Phase/order tests prove body input reaches the application child, close suppresses child
    update in the same input transaction and later paint, captured drag/release works outside
    bounds, underlay precedes descendants, and the rendering-only overlay follows them.
  - Hiding silently clears transient targets and active chrome state but retains typed state and
    pending events; showing does not restore cleared targets. Destruction releases all retained
    ownership subject only to already-active state upgrades.
  - Programmatic options clear incompatible move/resize capture, popup reopen applies current-pointer
    positioning without frame suppression, and auto-size normalization emits no change event.
  - Compile-fail/API-surface tests prove no operation replaces a root `Node` while retaining its
    `RootId`, no `RootState` child getter exists, and `RootChromeContainer`, `RootInteraction`, and
    framework transition helpers remain private. A persistent exposed container root supports
    dynamic content without replacement; literal root-type change requires destroy/recreate.
  - Production/API searches find no `ResourceState`, `FrameResults`, `FrameResultGeneration`,
    `RetainedId`, `state_of_root`, `state_of_retained`, `Context::root_rect`,
    `Context::root_visible`, `set_root_nodes`, parallel chrome-capture/update state, or second chrome
    geometry formula.
  - Rustdoc and migration notes document `RootHandle` weak ownership, state access before first
    frame, setter failure modes, hide versus destroy, typed event multiplicity, popup ordering,
    immutable root content, and persistent-container/destroy-recreate alternatives.

  **Frozen contract evidence (2026-07-29)**

  The normative root-chrome section now defines one retained ownership chain, one authoritative
  typed state, one chrome geometry function, and one ordered input path. Root lifecycle, policy,
  events, and external backend coordination each have one owner; hiding, typed observation, popup
  dismissal, and destruction no longer require a root-only result or replacement mechanism.

  Repository inspection confirms that `WindowEntry` currently stores name, rectangle, options,
  visibility, `just_opened`, `active_chrome`, z-order, a replaceable `Vec<UiNode>`, and a separate
  `UiRuntime` directly. Window/dialog/popup constructors consume `UiNodeSet` and return only
  `RootId`; `Context::root_rect`/`root_visible` duplicate reads, setters silently ignore unknown IDs,
  `bring_root_to_front` returns nothing, `set_root_nodes` replaces the projection and transfers
  runtime state, and no destruction operation exists. The per-Context root counter begins at one,
  advances with checked arithmetic, and is not currently recycled.

  Chrome geometry and execution are also split today. `WindowChrome::new`, `root_titlebar_height`,
  `root_min_size`, frame painting, overlay painting, `UiRuntime::measure_auto_size`, and retained
  body layout each own part of the formula. `WindowEntry::active_chrome` and
  `update_window_manager_chrome` independently hit-test raw pointer state, move/resize the rectangle,
  and hide on close outside retained routing/capture. Close and outside-popup dismissal record no
  typed root event. `RetainedId::Root` is constructible, but production traversal does not populate a
  root result entry.

  Current popup code repositions a hidden popup and arms `just_opened`, then later hides it directly
  when any pressed button is outside. It neither enforces one visible popup nor performs an atomic
  checked-state switch, and dismissal is interleaved with per-root iteration instead of being the
  first stage of one dispatcher before routing the same press onward. Existing characterization and
  chrome/popup tests pin useful creation defaults, geometry, z-order, drag, resize, close, auto-size,
  and outside-dismissal outcomes; projection replacement, split state, missing destruction, and
  missing typed events remain migration evidence rather than compatibility behavior.

  P0.7 intentionally changes no production API. Persistent root ownership/chrome/state/destruction
  land in P1.4; hidden/destroyed target cleanup in P2.4; ordered popup-boundary input in P2.5;
  projection/result/query removal and application migration in P3.0; and final explicit-constraint,
  shared-geometry, and backend-boundary verification in P4.2.

  **P2.5 popup-order amendment (2026-07-31)**

  The explicit input/update boundary makes `just_opened` both unnecessary and incorrect: the input
  that produced an application-visible opening event has completed before application code can show
  the popup. P2.5 removes the flag and its frame suppression. A press still queued when a popup is
  shown is subsequent input by contract and follows ordinary outside-dismissal/routing.

### P1 — Final state ownership and persistent topology

P1 implements the final concrete-runtime ownership model directly. Each item inherits the behavioral
acceptance criteria of its named P0 owner; the bullets below add migration sequencing, structural
removal, and focused implementation evidence rather than redefining that behavior.

- [x] **P1.0 — Land the public construction/state primitives with Checkbox as the vertical slice**

  **Problem**

  The new ownership contract must work end to end before all built-ins are split.

  **Decision needed: No — implements P0.1 and P0.2**

  **Target contract or migration**

  Add the public widget state/construction roles, weak handle primitives including input-preserving
  `try_update_with`, `WidgetStateOwner`, and the final
  `WidgetBuilder::{Parameters, W, create_widget}` contract. Split `Checkbox` into builder,
  parameters, state, and concrete runtime implementation; that runtime owns its strong state cell
  directly. Let a temporary crate-private projection adapter accept the concrete runtime generically
  and erase it to `Box<dyn Widget>` only at insertion; delete the legacy payload branch in P1.2. No
  public raw-box insertion API is exposed or retained past this item. Container/visitor contracts may be implemented
  crate-privately as migration scaffolding, but do not export public `Container`, `ContainerState`,
  `Children`, visitors, container contexts/results, or owning `Node` until the atomic P1.3/P2.1
  compile-safe batch supplies a usable complete surface and resolves the old public `Node` collision.

  **Acceptance tests**

  - `Checkbox::create` returns `WidgetStateHandle<CheckboxState>` plus concrete `Checkbox`.
  - Concrete `Checkbox` owns the only persistent strong `Rc<RefCell<CheckboxState>>`; its
    `state_handle` method downgrades that exact allocation.
  - A stateless proof runtime owns the same strong unit-state shape; discarding its weak handle does
    not alter ownership or phase behavior.
  - External code can implement the complete four-role custom widget path.
  - Checked read/update and destruction behavior match P0.2.
  - Current checkbox geometry, click toggle, paint, and options remain correct.
  - Public rustdoc explains which data belongs in Parameters versus State.
  - Public exports at this item contain the widget roles/handle surface and no incomplete
    container/visitor/owning-Node API. Any temporary projection or `NodeBehavior` migration bridge is
    crate-private and names the P1.3/P2.1 batch or P2.3 as its deletion/export point.

  **Implementation evidence (2026-07-30)**

  `src/widget.rs` now defines the public `WidgetState`, `WidgetParameters`, `WidgetStateOwner`,
  `WidgetBuilder`, and `WidgetStateHandle<T>` surface. The handle deliberately has no public
  access-error taxonomy: ordinary unavailable access returns `None`, while `try_update_with` returns
  its input directly as `Err(input)`. Each concrete runtime owns exactly one strong
  `Rc<RefCell<State>>` and implements `state_handle` with `WidgetStateHandle::new(&self.state)`. The
  builder associates Parameters with concrete `W` and returns that runtime directly; there is no
  framework allocator, optional exposure result, `EXPOSE_STATE`, state keep-alive trait, or parallel
  owner wrapper. The handle implementation uses checked per-cell borrows, has a manual `Clone`
  without a `T: Clone` bound, preserves owned input through `try_update_with`, and exposes no raw
  `Rc`, `Weak`, Context identity, mount identity, or frame gate. Shared internal runtime helpers turn
  an incompatible associated-state borrow into a precise invariant diagnostic.

  `Checkbox` is the first complete split built-in. `CheckboxParameters` owns its one-shot label,
  font, initial value, and options; `CheckboxState` owns the checked value and its silent
  programmatic operations; public `CheckboxBuilder` implements `WidgetBuilder`; and public concrete
  `Checkbox` implements the current common `Widget` phases plus `WidgetStateOwner` while owning the
  strong state cell. `Checkbox::create` returns its typed weak handle plus that concrete runtime.
  Measurement, framing/options, text and icon paint, click toggling, and the temporary
  legacy `CHANGE` result remain characterized. The typed pending-event migration and unit-returning
  `Widget::update` remain with P1.1/P3.0 as assigned by P0.4 rather than being partially applied to
  unsplit widgets here.

  The current projection contains one crate-private `WidgetPayload::{Legacy, Direct}` bridge. Its
  direct branch erases a generic concrete `W: WidgetStateOwner` to `Box<dyn Widget>` at the node
  boundary, performs no erased state-handle redispatch or owner clone, and records legacy frame
  results without inventing a widget-handle identity. The doc-hidden staging insertion is generic
  over `WidgetStateOwner` and never accepts `Box<dyn Widget>`; P1.2 removes the payload bridge when
  every leaf uses direct boxed runtime dispatch, and P1.3 replaces the staging insertion with public
  owning `Node`. Other widgets continue through the old strong-handle branch until their assigned
  migration items.

  Focused tests prove the sole persistent strong owner, weak cloning for non-`Clone` state,
  unavailable-access behavior, separate liveness observation, closure-result propagation,
  same-cell conflict, cross-cell access, active read and write lifetime, exact input recovery,
  discarded-unit-handle ownership, and the top-level retained-traversal precondition. A downstream
  integration implements all four roles for meaningful and unit-state widgets, inserts both
  concrete runtimes through the generic staging bridge, exercises them
  before/during/after a live `ContextFrame`, and observes handle expiry when Context releases the
  tree. Checkbox characterization proves direct typed mutation, retained geometry/text paint,
  options, click toggling, compatibility result recording, zero erased adapters on its branch, and
  the shared reentrant-borrow diagnostic. The full demo now moves its three concrete Checkbox runtimes
  once instead of cloning strong handles.

  Validation passes `cargo fmt --all -- --check`, `cargo test --all-targets` (180 unit tests and two
  downstream integration tests passed; three existing manual baselines ignored), `cargo test
  --doc` (17 passed), `cargo check --no-default-features`, `cargo doc --no-deps`, and separate
  all-example checks for `example-glow`, `example-vulkan`, and `example-wgpu`. `cargo clippy
  --all-targets -- -W clippy::all` completes with the repository's pre-existing warning baseline;
  a path-filtered rerun reports no warning in a P1.0-touched file.

- [x] **P1.1 — Split every built-in leaf using the fixed constructor/mutation mapping**

  **Problem**

  Current built-in structs mix all three roles, and examples mutate fields directly.

  **Decision needed: No — implements P0.1 and P0.4**

  **Target contract or migration**

  Implement the authoritative table in “Fixed built-in constructor and compatibility mapping”:
  every application-state leaf constructor returns its typed handle with the concrete runtime;
  `Custom` returns only its concrete runtime with `State = ()`, and
  the old header/tree `widgets::Node`/`NodeStateValue` APIs retire into exposed `DisclosureState` in
  P2.1 without an alias. Preserve exactly the mounted values/events/commands listed in that table.
  Move initialization-only visual/base configuration to Parameters and runtime-derived/cached data
  to private runtime fields. This is a deliberate breaking boundary; arbitrary old public-field
  mutation not listed in the table does not become a mounted State API.

  **Complete P1.1 old-public-symbol classification**

  This table is exhaustive for the public fields and inherent methods on the pre-split built-in
  leaf structs. `Parameters` entries are construction-only; “runtime-private” entries have no
  mounted mutation API. The new `create(Parameters)` convenience constructor is the fixed
  handle-plus-runtime result except for `Custom`, which returns only its runtime.

  | Old built-in | Old public fields and methods | P1.1 classification or exact replacement |
  |---|---|---|
  | `ButtonContent` / `Button` | `ButtonContent::{Text { label, icon }, Image { label, image }, ScaledImage { label, image }}`; fields `content`, `config`, `fill`; `new`, `with_opt`, `with_icon`, `with_image`, `with_scaled_image` | `ButtonContent` remains construction data in `ButtonParameters::content`; `config.{font,opt}` and `fill` become `ButtonParameters::{font,opt,fill}` plus runtime-private configuration; the old constructors move to `ButtonParameters`; mounted state exposes only `ButtonState::take_submitted`. |
  | `Checkbox` family (already split in P1.0) | `CheckboxParameters` fields `label`, `checked`, `font`, `opt` and methods `new`, `with_opt`, `font`; `CheckboxState::{check, uncheck, set_checked, checked}`; `Checkbox::create` | Parameter and value methods remain in their existing roles; P1.1 adds only `CheckboxState::take_changed` and its saturating pending count. Label/font/options remain initialization-only. |
  | `ListItem` | fields `label`, `icon`, `config`; `new`, `with_opt`, `with_icon`, `with_icon_opt` | Initial label/icon/font/options and the four old constructors move to `ListItemParameters`; mounted label becomes private `ListItemState` data exposed by `label`/`set_label`; icon/font/options are runtime-private after construction; submission is `take_submitted`. |
  | `ListBox` | fields `label`, `image`, `config`; `new`, `with_opt` | All old fields and both constructors move to `ListBoxParameters` and runtime-private paint/measure configuration; there is no mounted label/image/config mutation; `ListBoxState` exposes only `take_submitted`. |
  | `Combo` | field `config`; `new`, `with_opt`, `anchor`, `selected`, `is_open`, `open_popup`, `close_popup`, `update_items`, `select` | Font/options and constructors move to `ComboParameters`; all listed observation/selection methods move to `ComboState` (with `label` added for the current cached label); derived anchor is state-readable; change/submit observations are `take_changed`/`take_submitted`. |
  | `TextBlock` | fields `text`, `wrap`, `config`; `new`, `with_wrap` | Initial text/wrap/font/options and constructors move to `TextBlockParameters`; mounted text becomes private `TextBlockState` data exposed by `text`/`set_text`/`clear`; wrap/font/options are runtime-private after construction. |
  | `ColorSwatch` | fields `fill`, `label`, `config`; `new` | Initial fill/label/font/options and construction move to `ColorSwatchParameters`; mounted fill and label become private `ColorSwatchState` data exposed by getters/setters; font/options are runtime-private. |
  | `Slider` | fields `low`, `high`, `step`, `precision`, `config`; `new`, `with_opt`, `value`, `set_value`, `is_editing` | Bounds/step/precision/font/options and constructors move to `SliderParameters` plus private constraint/runtime data; value/editing methods move to `SliderState`; user changes are consumed with `take_changed`. Bounds remain initialization-only even though private retained constraint data clamps `set_value`. |
  | `Number` | fields `step`, `precision`, `config`; `new`, `with_opt`, `value`, `set_value`, `is_editing` | Step/precision/font/options and constructors move to `NumberParameters` and runtime-private configuration; value/editing methods move to `NumberState`; user changes are consumed with `take_changed`. |
  | `Textbox` | field `config`; `new`, `with_opt`, `text`, `set_text`, `clear`, `cursor`, `set_cursor`, `move_cursor_to_end` | Initial text/font/options and constructors move to `TextboxParameters`; every listed text/cursor operation moves to `TextboxState`; `take_changed` and `take_submitted` are added there. Focus remains router-owned and Return submission does not clear it. The contract is cursor-only: no selection state or selection API is introduced. |
  | `TextArea` | fields `wrap`, `config`; `new`, `with_opt`, `text`, `set_text`, `clear`, `cursor`, `set_cursor`, `move_cursor_to_end`, `scroll`, `set_scroll` | Initial text/wrap/font/options and constructors move to `TextAreaParameters`; every listed text/cursor/scroll operation moves to `TextAreaState`; `take_changed` and `take_submitted` are added there. Wrap/font/options and preferred-column/drag details are runtime-private. The contract is cursor-only. |
  | `Custom` | fields `name`, `config`; `new`, `with_opt` | Name/font/options and constructors move to `CustomParameters`; the runtime privately owns `State = ()`; `Custom::create` returns the runtime only and exposes no mounted application handle. |
  | `WidgetConfig` as used by leaves | fields `font`, `opt`; `new`, `font` | Each leaf absorbs these into its Parameters and private runtime fields; `WidgetConfig` remains temporarily only for the old header/tree `Node` until P2.1 and is not a leaf mounted-state API. |
  | old `widgets::Node` / `NodeStateValue` | `NodeStateValue::{Expanded, Closed, is_expanded, is_closed}`; `Node` fields `label`, `state`, `config`; `header`, `tree`, `with_options`, `is_expanded`, `is_closed`, `is_tree`, `is_header`; click-toggle and visual behavior | Retired in P2.1 without an alias. `header`/`tree` become `DisclosureParameters::{header,tree}`; label/options/visual variant are initialization-only; expansion becomes `DisclosureState::{is_expanded,is_collapsed,expand,collapse,toggle}`; `is_header`/`is_tree` retire; one private-variant `Disclosure` runtime preserves click toggle, label/icon paint, framing/hover treatment, and tree indentation. |

  **Acceptance tests**

  - A mapping test/document lists every old public field/method and classifies it as Parameters,
    exposed State, runtime-private/derived, or retired; it does not claim every field remains
    mutable after mounting.
  - Slider value, textbox/text-area content, combo selection, swatch color, and mutable display text
    remain typed and fallible through handles.
  - Checkbox/slider/number/text change and button/list/text/combo submission use the exact typed
    event APIs, recording points, saturating multiplicity, and programmatic-setter rules specified
    above; Combo clamp/select edge cases are pinned separately.
  - `Custom` has a signature test proving its convenience constructor returns no weak application
    handle and a lifetime/ownership test proving its concrete runtime retains the unit state cell;
    no constructor has a parameter-dependent return shape.
  - Built-in Widget implementations preserve current phase behavior and focus policies.
  - No built-in state type implements `Widget` merely to obtain dispatch.
  - The migration mapping names `Node::header`, `Node::tree`, `NodeStateValue::{Expanded, Closed}`,
    their predicates, label/options, and click-toggle behavior and points each one to its exact
    `DisclosureParameters`/`DisclosureState` replacement.

  **Implementation evidence (2026-07-30)**

  Every built-in leaf now has explicit Parameters, State, concrete runtime, and Builder roles. Each
  stateful constructor returns its weak typed state handle with the non-`Clone` runtime, while
  `Custom::create` returns only its unit-state runtime. Typed saturating event counters replace
  application-side generic result lookup for change/submission, programmatic setters remain silent,
  and Combo item-clamp changes are pinned separately. Textbox and TextArea deliberately expose
  cursor-only editing state; no selection or focus-command API was added. Textbox Return records a
  submission without clearing router-owned focus. The old header/tree `Node` surface remains only
  as the explicitly classified P2.1 migration input.

  Focused tests cover typed event multiplicity and saturation, silent setters, Combo clamp/select
  behavior, Custom's return shape and unit-state lifetime, text change/submission independence,
  cursor-only setters, focus retention after submission, and single-target keyboard/text routing.
  README examples, all shipped examples, the file dialog, window-manager construction, and
  characterization tests now use the split constructors and typed leaf-state capabilities.

  Validation passes `cargo fmt --all -- --check`, `cargo test --all-targets` (192 unit tests and two
  downstream integration tests passed; three existing manual baselines ignored), `cargo test
  --doc` (17 passed), `cargo check --no-default-features`, `cargo doc --no-deps`, and separate
  all-example checks for `example-glow`, `example-vulkan`, and `example-wgpu`. `cargo clippy --lib
  -- -W clippy::all` completes with the repository's pre-existing warning baseline and no P1.1
  warning remains in the newly split leaf implementations.

- [x] **P1.2 — Dispatch persistent leaf payloads directly through boxed runtimes**

  **Problem**

  Current `WidgetNode` erases and clones a strong state handle, then redispatches `Widget` methods.

  **Decision needed: No**

  **Target contract or migration**

  Change the internal leaf payload to own `Box<dyn Widget>`. Keep only the generic geometry/input
  adapter required to create `WidgetUpdateCtx`/`WidgetPaintCtx`; it delegates `Widget` directly and
  owns optional private `CustomRenderKey`. The final generic
  `Node::widget<W: WidgetStateOwner>(W)`/`Node::custom_render<W: WidgetStateOwner, B>(W,
  CustomRenderHandle<B>)` surface lands with the owning `Node` in P1.3; no raw box overload exists.
  Delete `WidgetStateHandleDyn`, `clone_box`,
  `erased_widget_state`, widget allocation IDs, and duplicate-state dispatch tracking. Keep existing
  renderer-registry preflight for removed, foreign, or backend-incompatible erased keys.

  Strict completion temporarily disables the file dialog at its compilation and integration edges
  rather than retaining a file-dialog-only strong-handle adapter. Comment out `mod file_dialog` and
  both public `FileDialogState` re-exports in `src/lib.rs`, and comment out the corresponding
  `demo-full` fields, initialization, evaluation, and visible controls. Mark every such region
  `P1.2 TEMPORARY: restore in P3.2`. Preserve `src/file_dialog.rs` and its tests in place without
  rewriting them to a transient reconstruction model. Do not add a replacement stub, a known-broken
  Cargo feature, or a second adapter. P1.2 is an internal-only integration state and cannot be
  merged, tagged, or released while this public capability is unavailable.

  **Acceptance tests**

  - Each requested leaf measure/update/paint invocation uses one direct Widget dispatch path; tests
    do not incorrectly require only one measurement request per update/layout call.
  - Moving a concrete runtime into a node does not change an already-returned weak handle or create a
    new one.
  - Built-in concrete runtimes are non-`Clone`; downstream documentation makes unique
    state-allocation ownership a safe `WidgetStateOwner` conformance obligation because arbitrary
    custom inherent APIs cannot be type-policed by the framework.
  - The P1.3 downstream example constructs a custom-render leaf from the concrete runtime returned by
    `WidgetBuilder::create_widget` and a `CustomRenderHandle<B>` without accessing `CustomRenderKey`.
  - `Node::widget` records no custom draw; `Node::custom_render` runs its callback after widget paint
    with the derived content rectangle and clip.
  - Removed/foreign custom-render handles fail existing preflight before backend acquisition.
  - `rg` finds no erased state-handle adapter after the temporary bridge is removed.
  - `src/file_dialog.rs` still contains the complete pre-migration implementation and tests at the
    same path. Git/source review finds no deletion, truncation, relocation, empty replacement, or
    mechanical state-recreation rewrite.
  - `src/lib.rs` and `demo-full` contain searchable `P1.2 TEMPORARY: restore in P3.2` comments at
    every disabled module, export, field, initialization, evaluation, and visible-control edge;
    those edges do not compile or appear in rustdoc during this internal interval.
  - The standard non-file-dialog library, tests, docs, and examples remain green. The validation
    report explicitly lists file-dialog tests and demo behavior as temporarily excluded rather than
    treating their absence as passing evidence.
  - Release checks fail or remain administratively blocked while any P1.2 restoration marker exists
    or the final polling-session types are absent from the crate-root/prelude exports.

  **Completion evidence (2026-07-30)**

  `WidgetNode` now accepts one concrete `WidgetStateOwner` runtime at generic insertion and owns it
  as `Box<dyn Widget>` with an optional private `CustomRenderKey`. Measure, update, paint, focus,
  options, result recording, and custom rendering use that one direct payload; the legacy/direct
  payload branch, erased state-handle trait and adapter, widget allocation identities, and duplicate
  widget-state dispatch tracker are deleted. Built-in, downstream, test, README, and shipped-example
  leaf call sites move non-cloneable concrete runtimes once while retaining only typed weak state
  handles. Legacy `WidgetHandle<Node>` remains solely for header/tree Disclosure until P1.3/P2.1.

  The file-dialog module and its two exports, plus every full-demo field, initialization,
  evaluation, and visible-control edge, are commented with 13 exact
  `P1.2 TEMPORARY: restore in P3.2` markers. `src/file_dialog.rs` and its tests remain byte-for-byte
  untouched but are intentionally uncompiled, so this evidence does not claim file-dialog test or
  demo coverage and the repository remains administratively release-blocked until P3.2 restores the
  capability and removes every marker.

  The reduced non-dialog surface passes `cargo fmt --all -- --check`, `cargo test --all-targets`
  (186 active library tests and two downstream integration tests passed; two existing manual
  baselines ignored), `cargo test --doc` (17 passed), `cargo check --no-default-features`, `cargo doc
  --no-deps`, and separate all-example checks for `example-glow`, `example-vulkan`, and
  `example-wgpu`. `cargo clippy --lib -- -W clippy::all` completes with the repository's existing 49
  warnings and no warning in the new direct leaf path. Source audits find none of
  `WidgetStateHandleDyn`, `erased_widget_state`, `WidgetPayload`, widget handle IDs, duplicate widget
  dispatch recording, legacy leaf `state_widget`, borrowed leaf insertion, or leaf-runtime
  `WidgetHandle` storage outside the preserved file-dialog source.

- [x] **P1.3 — Introduce unique `Node` ownership and state-owned container children**

  **Problem**

  Current nodes are rebuilt projections, while direct container state needs stable owned nodes.

  **Decision needed: No — implements P0.3, P0.5, and P0.6**

  **Target contract or migration**

  Add `ContainerBuilder`, the one public opaque non-cloneable owning `Node`, private `NodeKind`, private
  `RuntimeNodeId`, `NodeRuntime` without generic visibility, opaque constructible `Children`,
  public `Container: Widget`, marker `ContainerState`, opaque visitors, scoped layout/input
  contexts/results, the Column state/runtime vertical slice, and P2.1 Disclosure as the atomic
  P1.3/P2.1 public API batch. Add the consuming `Node::with_policy` placement method; successful
  child insertion consumes the node. Grid-only placement is excluded from the generic foundation
  and enters with Grid-owned `GridItem`/`GridState` in P2.0. Use the same typed weak
  `WidgetStateHandle<C>` model for both leaf and container state.
  Column and Disclosure constructors return `(WidgetStateHandle<C>, Node)` in this batch after using
  public generic `Node::container` internally. Grid adds that final shape in the early P2.0
  correction, Row/Stack finish P2.0, and ScrollArea does so in P2.2. Downstream custom constructors use
  `ContainerBuilder::create_container` and pass the returned concrete runtime to the same node
  constructor explicitly; no raw container box can enter the tree.

  The old header/tree `widgets::Node` exports must be removed before the owning `Node` export lands.
  P1.3 and P2.1 therefore land in one compile-safe integration batch, or a crate-private
  `LegacyDisclosureNode` adapter bridges only those two items and is deleted by P2.1. No temporary
  public alias is permitted.

  This item supplies the owning `Node`/`Children` prerequisite for the preserved file-dialog source,
  but deliberately does not re-enable a reduced Column-only or projection-rebuilding dialog.
  File-dialog restoration remains owned by P3.2 after P2.0 adds Row/Stack and P2.2 adds ScrollArea.

  **Acceptance tests**

  - Node creation assigns unique private identity before mounting.
  - Children growth/reordering does not change node IDs or state cell addresses.
  - Removing/clearing/replacing drops exactly the removed nodes, concrete runtimes, and state cells.
  - Failed out-of-range insertion returns the uncommitted input node.
  - `try_update_with` also returns the exact unmounted node as `Err(node)` whenever target-state
    access is unavailable before insertion begins; examples never move unique nodes into ordinary
    `try_update` closures when access failure must preserve them.
  - `Node::with_policy` works before insertion; no mounted policy or generic visibility mutator
    exists, and generic `Node`/`ContainerLayoutCtx` expose no Grid-only placement API.
  - No `ContainerHandle`, `ContainerEditor`, mounted-state metadata, raw mutable child callback, or
    framework-provided reparent path exists.
  - Column and Disclosure construction has one explicit typed-handle-plus-node return shape;
    P2.0/P2.2 extend that shape to the later public dynamic constructors.
  - No incomplete public container/visitor surface exists before this compile-safe batch, and the
    old public header/tree `Node` is absent before the owning `Node` export becomes reachable.
  - Atomic-batch downstream tests cover ergonomic Column/Disclosure `(handle, Node)` construction
    and an external `ContainerBuilder`/`Container` implementation created through
    `ContainerBuilder::create_container` and wrapped with generic `Node::container`; later rollout tests extend
    the constructor assertion to every built-in.
  - Every `P1.2 TEMPORARY: restore in P3.2` marker and the preserved `src/file_dialog.rs` source/tests
    remain intact; P1.3 neither silently restores an incomplete dialog nor deletes its migration
    inventory.

  **Completion evidence (2026-07-30)**

  The public retained API now exports one non-`Clone` owning `Node`, opaque constructible `Children`,
  marker-only `ContainerState`, associated-type `ContainerBuilder`, object-safe `Container: Widget`,
  exactly-once opaque child visitors, and the scoped layout/input contexts and result type. Generic
  `Node::widget`, `Node::custom_render`, and `Node::container` accept only concrete
  `WidgetStateOwner` runtimes; consuming `with_policy` configures an unmounted owner without
  exposing mounted mutation or identity. `RuntimeNodeId`, `NodeRuntime`, and
  `NodeKind` remain private to retained runtime implementation, and generic node visibility is gone.

  `Children` and the built-in Column/Disclosure states expose only the specified safe indexed
  membership family. Focused tests prove monotonically increasing construction identity, identity
  preservation across moves/configuration/failed insertion, ordered collection construction,
  exact-node recovery from invalid insertion and unavailable `try_update_with`, and immediate state
  expiry when remove/clear/replace drops the corresponding runtime owner. Visitor conformance tests
  pin the exact zero/multiple-submission diagnostics. A downstream integration constructs and runs a
  custom state-owned `Container` through `ContainerBuilder::create_container` and generic
  `Node::container`, using public `Children` measurement, visitors, and indexed layout only; it also
  covers direct Column/Disclosure `(WidgetStateHandle<C>, Node)` construction and owning custom
  rendering.

  The projection builder now creates owning nodes directly and no longer reconstructs runtime
  identity from seeds, sibling ordinals, or keys. Row/Stack and synthetic ScrollArea use an
  explicitly documented crate-private compatibility bridge until P2.0/P2.2. Grid has already moved
  to its final state-owned runtime; only its projection-time span remains on private
  `BuilderChild` edge metadata until P3.0. The public surface has
  no raw container box, `ContainerHandle`, `ContainerEditor`, node ID accessor, child iterator,
  detachment/reparent operation, or generic visibility mutator. The old salted UI-node namespaces
  are deleted. All 13 `P1.2 TEMPORARY: restore in P3.2` markers remain, and `src/file_dialog.rs` is
  unchanged.

  Validation passes `cargo fmt --all -- --check`, `cargo test --all-targets` (202 active library
  tests and four downstream integration tests passed; two manual baselines ignored), `cargo test
  --doc` (17 passed), `cargo check --no-default-features`, `cargo doc --no-deps`, and separate
  all-example checks for `example-glow`, `example-vulkan`, and `example-wgpu`. `cargo clippy
  --all-targets -- -W clippy::all` completes with the repository's existing warning baseline.

- [x] **P1.4 — Give each root one persistent `WidgetTree`**

  **Problem**

  `WindowEntry` currently accepts replaceable root projections and transfers runtime state.

  **Decision needed: No — implements P0.7**

  **Target contract or migration**

  Change window/dialog/popup creation to consume one application `Node`, construct one private
  `RootChromeContainer` around it, and return `RootHandle`. Give `WindowEntry` only a private weak
  clone of the `RootState` handle; the concrete private container runtime is the sole persistent
  strong state owner. A caller needing multiple application children constructs a column/row/stack
  as the single content node.

  Move title/frame/body/close/resize measurement, layout, hit-testing, update, and background underlay
  behind the internal container and shared geometry helper. Use the helper once more at the window
  compositing boundary to append title/close/resize overlays after descendants. Use ordinary tree
  capture for move/resize, leaving the window manager responsible only for cross-root/modal policy,
  z-order, outside-popup detection, backend viewport coordination, and that post-tree overlay.
  Hiding preserves the tree/state/pending events but clears transient targets; `destroy_root`
  unmounts/releases the tree and never returns it, with physical destruction delayed only by active
  state upgrades. Delete all generic and root-only result storage. Remove public root
  replacement after P3 migration and add no replacement of the one application child.

  Context owns one private `modal_stack: Vec<RootId>` as cross-root policy, not an application
  `RootHandle`. Its last entry is the active modal root, and the stack permits restoration without
  borrowing `RootState` from `bring_root_to_front` or `destroy_root`. Any shown dialog moves to the
  end of that stack and remains frontmost. Only its tree may
  receive pointer, keyboard, text, focus, capture, or input-driven update traversal; outside
  pointer input is swallowed. Activating it clears every other tree's transient targets. Hiding,
  closing, or destroying it exposes the previous dialog in the stack, if any. Other roots continue to
  lay out and paint, and popups have no implicit exception to the modal gate.

  **Acceptance tests**

  - Window, dialog, and popup behavior works from one persistent private chrome-container root and
    one application content child.
  - Creation returns `RootHandle`; hide/show preserves its weak state capability and pending events,
    while destruction expires root/descendant handles after active accesses finish.
  - Unknown/already-destroyed IDs return `false` from destruction/fronting and `UnknownRoot` from
    setters; destroyed handles become non-live and return `None`, same-state setter conflicts report
    `RootMutationError::Borrowed`, and IDs are never reused.
  - `RootState` is authoritative for rect/visibility/current chrome interaction/pending events;
    WindowEntry is authoritative for lifecycle/cross-root/z-order/backend concerns only.
  - Title close/popup dismissal hide and record typed submission; actual user drag/resize changes
    record typed change/current active state; programmatic changes are silent.
  - At most one popup is visible per Context; showing another silently hides and sanitizes the
    previous popup without recording a submission. Nested popup behavior is out of scope.
  - The last visible dialog in `modal_stack` is the sole input/update root and remains visually
    frontmost; underlying windows and popups cannot route pointer, keyboard, focus, or capture
    input. Activating a dialog clears their transient targets, and hiding/destroying the active
    dialog restores the previous visible dialog or ordinary routing.
  - Phase-count and hit-region tests prove chrome dispatches through the tree once, body input falls
    through to the application child, the post-tree overlay paints above descendants, and no
    parallel window-manager chrome capture/update exists.
  - Root creation/destruction needs no state-to-node registry, result generation, or token.
  - API-surface tests prove a root `Node` cannot be replaced while preserving `RootId`; a dynamic
    container-root test replaces descendants while preserving the root/window and persistent state.

  **Completion evidence (2026-07-31)**

  Window, dialog, and popup creation now consume one application `Node`, install one persistent
  private `RootChromeContainer` tree, and return a cloneable weak `RootHandle`. `WindowEntry` retains
  only lifecycle/cross-root data plus a weak root-state capability; `RootState` owns chrome geometry,
  visibility, interaction, and typed pending events. Hide/show, destruction, never-reused IDs,
  same-cell mutation conflicts, popup switching/dismissal, drag/close events, body fallthrough,
  phase counts, dynamic descendant replacement, and post-descendant chrome overlay ordering are
  covered by focused root tests. Generic/root-only result storage is gone.

  **Modal correction evidence (2026-08-01)**

  Context now stores one ID-only `modal_stack`; its last entry is the active modal root. Dialog
  lifecycle updates that stack without adding root-state borrows to destruction/fronting.
  Focused tests prove exclusive pointer and keyboard routing, immediate revocation of underlying
  focus/chrome capture, dialog-front z-order enforcement, restoration across hide/destruction, and
  file-dialog isolation from an underlying window. Layout and paint remain cross-root.

### P2 — Container mechanics and runtime traversal

P2 makes the P0 container, visibility, identity, and event behavior executable on P1 ownership.
Its acceptance bullets are implementation-specific evidence and edge coverage; any material need to
change a protected P0 behavior follows the explicit change-control rule.

- [x] **P2.0 — Convert row, grid, and stack and finish the shared layout-container mechanics**

  **Problem**

  Existing Row/Stack containers own child vectors but are reconstructed by the builder and lack
  typed state handles for local membership changes. Grid's state-owned portion landed early with
  the Grid-owned placement correction below.

  **Decision needed: No — implements P0.3**

  **Target contract or migration**

  Build on the `ColumnState`/Column vertical slice already landed in the atomic P1.3/P2.1 batch and
  the state-owned Grid slice landed with this plan correction. Introduce `RowState` and
  `StackState`; retain the completed `GridState`. Each owns its children and the exact mounted
  configuration defined above: Row widths/item height, Grid children/spans/column tracks/row tracks,
  and Stack item width/item height/direction; Column adds none. Their private runtimes borrow state for
  measure/layout and supply children for recursion through opaque visitors with explicit scopes.
  Every public dynamic state exposes the same safe
  `len`/`is_empty`/`push`/`insert`/`remove_drop`/`clear`/`replace` family and no whole-collection
  getter. Grid accepts `GridItem` for insertion failure recovery and `Into<GridItem>` where no
  failure value is returned; plain nodes receive `GridSpan::ONE`. Extend
  constructor-return/ownership conformance from Column/Disclosure/Grid to Row/Stack.

  **Acceptance tests**

  - Empty/populated/dynamic containers match current layout for every sizing policy.
  - State setters change Row/Grid/Stack layout during the next explicit `update_ui`
    synchronization or current input transaction's mandatory post-event layout; Stack direction
    migration replaces `demo-full` root rebuilding.
  - Missing/excess Row widths, empty/extra Grid tracks, grid reflow, grid span, Style spacing, and
    nested transforms follow the exact target rules.
  - `layout_child` applies `Policy` once after slot/span resolution; a non-`Auto` policy changes only
    that child allocation and never rewrites a shared track. No mounted policy setter exists;
    `GridState::set_span` reflows placement without replacing the child.
  - Same-container visitor mutation returns `None`; another container mutation follows pinned
    traversal order.
  - Dynamic membership performs no root reconstruction or state transfer.

  **Completion evidence (2026-07-31)**

  Row, Grid, and Stack now expose parameter/state/container/builder roles whose constructors return
  a typed weak state handle plus one completed `Node`. Their concrete states are the sole owners of
  children and parent-specific configuration; Grid owns spans in `GridItem`, while Row and Stack
  expose mounted track/direction setters without rebuilding nodes. Focused tests cover topology
  mutation, failed-access ownership, track changes, span reflow, direction changes, sizing-policy
  application, and shared weighted/remainder/fractional placement. No projection-only span transport
  remains.

- [x] **P2.1 — Make disclosure one stateful container**

  **Problem**

  Current disclosure stores a strong `WidgetHandle<Node>`, recreates erased adapters, and delegates
  children through a nested `Column`.

  **Decision needed: No**

  **Target contract or migration**

  Land this public replacement in the same compile-safe batch as P1.3's owning `Node`, opaque
  `Children`, public container roles, and visitors. Do not publish either half of that surface
  independently.

  Replace the old header/tree widget and nested disclosure adapter with one public container family:

  ```rust
  pub struct DisclosureParameters { /* private fields */ }

  impl DisclosureParameters {
      pub fn header(
          label: impl Into<String>,
          expanded: bool,
          children: impl IntoIterator<Item = Node>,
      ) -> Self;

      pub fn tree(
          label: impl Into<String>,
          expanded: bool,
          children: impl IntoIterator<Item = Node>,
      ) -> Self;

      pub fn with_options(self, opt: WidgetOption) -> Self;
  }

  pub struct DisclosureState {
      children: Children,
      expanded: bool,
  }

  impl DisclosureState {
      pub fn is_expanded(&self) -> bool;
      pub fn is_collapsed(&self) -> bool;
      pub fn expand(&mut self);
      pub fn collapse(&mut self);
      pub fn toggle(&mut self);
      // The same safe len/is_empty/push/insert/remove_drop/clear/replace family.
  }
  ```

  A private visual variant distinguishes the current header and tree presentation; there is no
  public replacement for `NodeStateValue`. The runtime implements the header behavior through the
  final Widget-style phase logic and routes only its header via
  `ContainerInputCtx::route_widget_in_rect`. `children_visible` mirrors `expanded`. Collapse retains
  descendants and handles but excludes them from measure/layout/input/update/paint/custom render,
  clears descendant transient targets at sanitization, and does not restore focus/capture on
  expansion. Explicit state mutation changes membership through the safe inherent operations.

  **Acceptance tests**

  - Expand/collapse preserves descendant boxes and weak handle liveness.
  - Header update/paint executes once without erased adapter reconstruction.
  - Hidden descendants contribute no measure/layout and receive no input/update/paint/custom render.
  - Collapsing clears descendant hover/focus/capture/routed events; expanding does not restore them.
  - Removing disclosure children drops them and stale runtime targets are sanitized.
  - Header/tree constructor styles, initial expanded/closed state, predicates, options, label paint,
    and click-toggle behavior cover the old `widgets::Node` capability mapping.
  - Production exports and searches contain no old `widgets::Node`, `NodeStateValue`, or temporary
    `LegacyDisclosureNode`.

  **Completion evidence (2026-07-30)**

  The old header/tree widget module and its `NodeStateValue` export are deleted. One public
  `Disclosure` family now owns label/options/private visual variant in its concrete runtime and
  expansion plus opaque children in `DisclosureState`; both header and tree constructors return the
  standard typed weak handle plus completed owning `Node`. The full demo and retained builder now
  construct disclosures through label/initial-state parameters and retain a typed handle only when
  later state mutation is required.

  The container measures, lays out, updates, and paints its header once through the shared Widget
  phases, routes only the header sub-rectangle, and gates descendant traversal from
  `children_visible`. Collapsed descendants retain their boxes and weak-handle liveness but are
  excluded from measure/layout overflow, input, update, paint, and custom rendering. Runtime layout
  boundaries sanitize hidden/removed hover, focus, capture, and the current routed recipient; expansion does
  not restore them. Tests cover header/tree defaults and option overrides, initial predicates and
  explicit expand/collapse/toggle, label painting and click toggling, hidden phase exclusion,
  descendant liveness, target sanitization, and immediate expiry after child removal. Production
  searches find no `widgets::Node`, `NodeStateValue`, or `LegacyDisclosureNode`.

- [x] **P2.2 — Make scroll area one container state with direct children**

  **Problem**

  Current scroll area creates viewport, track, scrollbar, and corner semantic nodes sharing one
  `Rc<RefCell<ScrollAreaState>>`.

  **Decision needed: No**

  **Target contract or migration**

  Add one `ScrollAreaState` with direct `Children`, public offset/scrolling-enabled state, private
  drag/derived geometry, and immutable parameter-owned framing/base options. Disabling scrolling
  synchronously clears private drag state and resets offset to zero; tree target sanitization clears
  capture before another event is routed to the disabled area, and derived layout hides bars. P2.4
  adds `ScrollAreaContainer::retains_pointer_capture`, which reports
  `scrolling_enabled && drag_axis.is_some()`, and `on_pointer_capture_lost`, which clears
  `drag_axis`, without giving state tree authority.
  Requested offsets clamp as specified above. The runtime owns clipping, translation,
  panel/bar/thumb/corner paint, wheel fallback, drag capture, and range clamping. Routing reads the
  current offset/geometry to decide whole-event consumption or capture and assigns the localized
  event to the current transaction; the immediately following inherited `Widget::update` is the only operation that changes offset or drag
  state. It uses
  `ContainerInputCtx::route_widget_in_rect` for viewport and scrollbar hit regions and
  `ContainerLayoutCtx::set_children_viewport` for the clipped/translated content surface. Remove
  all synthetic semantic nodes.
  `ScrollAreaState` exposes the same safe child-operation family and never returns or lends its
  complete collection. Its constructor extends the final `(handle, Node)` ownership/return-shape
  conformance already established for the other public dynamic containers.

  **Acceptance tests**

  - One scroll area plus one content widget has two semantic nodes.
  - Both axes, nested scrolling, boundary bubbling, minimum thumb, drag, corner, and clipping pass.
  - The nearest nested scroll owner consumes the whole wheel event only when at least one requested
    axis actually changes after clamping; otherwise the whole event bubbles. A diagonal event is not
    split into per-axis residuals.
  - Scrollbar capture starts only from a matching pointer-down; paint/hit/drag share inverse thumb
    geometry, and a track click centers the thumb on the pointer without a repeat timer.
  - Disabling scrolling clears drag/offset synchronously; before any later routed event, tree
    sanitization releases capture owned by that area. Re-enabling restores none of them, and framing
    remains construction-only.
  - Disable followed by re-enable before sanitation still reports no local capture retention because
    disabling cleared `drag_axis`; only a later matching pointer-down can acquire new tree capture.
  - Collapsing an ancestor while scrollbar capture is active invokes the captured area's loss hook,
    clears `drag_axis`, and prevents expansion or an unpaired drag from continuing the old gesture.
  - Routing a wheel/drag event does not mutate `ScrollAreaState`; it assigns exactly one localized
    event, and the same transaction's `Widget::update` applies the state change once before layout.
  - Sub-rectangle routing intersects the active clip, preserves container-local pointer
    coordinates, and never lets a scrollbar/body hit leak into the other region.
  - Replacing children preserves scroll state and clamps offset.
  - Removing the area expires all descendant handles and cannot redirect capture to a replacement.

  **Completion evidence (2026-07-31)**

  ScrollArea is now one public state-owned container with direct `Children`; viewport, tracks,
  thumbs, and corner are geometry painted and routed by that container rather than synthetic nodes.
  The state owns offset, scrolling enablement, drag state, derived geometry, and the safe indexed
  topology family. Layout uses one descendant viewport clip/translation boundary, while container
  paint/input remain in the parent clip. Focused tests pin direct semantic topology, mounted state
  mutation, offset/disable behavior, clipping, and child ownership.

- [x] **P2.3 — Traverse retained boxes and checked container borrows directly**

  **Problem**

  Current traversal matches projection payloads and performs targeted searches inherited from
  generated IDs. It also routes widgets and containers through the catch-all private `NodeBehavior`
  trait. Replacing that with parallel `Widget` and container measure/update/paint paths would retain
  the same duplication under new names even though `Container: Widget`.

  **Decision needed: No**

  **Target contract or migration**

  Traverse `NodeKind` directly. For framing, interaction policy, measure, update, and paint, obtain
  one `&dyn Widget`/`&mut dyn Widget` from either variant; the container arm uses stable trait-object
  upcasting from `dyn Container` to its `dyn Widget` supertrait:

  ```rust
  fn widget_mut(kind: &mut NodeKind) -> &mut dyn Widget {
      match kind {
          NodeKind::Widget(node) => &mut *node.widget,
          NodeKind::Container(container) => &mut **container,
      }
  }
  ```

  Use that shared path exactly once per requested common-phase invocation. Branch on
  `NodeKind::Container` only to call
  `Container::layout`, `Container::route_input`, check `children_visible`, service
  `has_pointer_capture` for the current routing target, invoke the P2.4 local capture lifecycle only
  on the currently captured target, and hold the scoped
  framework-created `ChildrenVisitor`/`ChildrenVisitorMut` borrow during recursion. The runtime uses crate-private
  double-ended `Children` iteration: forward for update/layout/paint and reverse for deepest-first
  input routing. Leaf layout remains the generic
  measure-to-content calculation. Move the recursion currently in `UiRuntime::measure_node_ref`
  behind private `Node` measurement, and let public `Children::measure_child` delegate to it; this
  lets inherited `Widget::measure` remain unchanged and requires no container measurement trait.
  The private node measurement returns enough internal detail for layout to reuse that one
  `Widget::measure` result when deriving leaf content size; do not retain separate
  `Node::measure_without_runtime` and `UiRuntime::measure_node_ref` algorithms or remeasure a leaf
  through a layout adapter. Delete the transitional `UiNode`, `UiNodeData`, `UiNodeState`, and
  `UiNodeId` aliases and use `Node`, `NodeKind`, `NodeRuntime`, and `RuntimeNodeId` directly.
  Carry transforms/clips/root state on the stack. Once all variants use direct dispatch, delete
  `NodeBehavior`, every implementation and bound of it, and any temporary adapter introduced during
  P1. Also delete the private `MeasureCtx`, `LayoutCtx`, `UpdateCtx`, `PaintCtx`, and `InputCtx`
  adapters that exist only to feed `NodeBehavior`. Do not replace them with another private
  catch-all runtime trait, phase-context layer, or parallel container measure/update/paint adapter.

  Query `children_visible` immediately before each descendant recursion. In particular, update the
  container itself first, then query the gate before updating its children; this gives Disclosure
  collapse and root close same-input-transaction suppression. Measure/layout/paint/input query the gate before
  entering children for that phase. The private root chrome gate does not create generic node
  visibility; normally the window manager skips the complete tree when `RootState` is hidden.

  **Acceptance tests**

  - Every live node whose ancestor container gates permit traversal updates/paints once in the
    established order; there is no generic node visibility check.
  - Phase-count tests prove each explicit measurement request uses one inherited `Widget::measure`
    dispatch and each eligible node receives one update per dequeued input and one paint per render,
    with no parallel container-phase path. The test permits the initial synchronization layout,
    one layout per input, and bounded scroll convergence rather than asserting one measure call per
    application loop.
  - Runtime layout reuses the authoritative private node-measurement result for leaf content size;
    production source contains no second node-measurement algorithm or layout-time leaf
    remeasurement adapter.
  - Ordinary containers use the generic `route_widget` default; special container routing only
    assigns or declines the current event, and the current transaction's inherited
    `Widget::update` call performs state changes.
  - Nested scroll boundary and pointer-capture tests prove the pre-update routing result is available
    without a second Widget update or a container-specific update phase.
  - A downstream custom container measures children through `Children::measure_child`, reads generic
    placement through `child_policy`, and lays them out through `layout_child` without private APIs
    or a second measure method. Parent-specific metadata remains in that container's own state.
  - Layout tests cover invalid indices returning `None`, the `Policy::auto` default, viewport
    intersection, child translation, content size, and overflow propagation. Grid-specific tests
    separately cover `GridSpan::ONE`, zero-span normalization, and state-owned span mutation.
  - Disclosure and scroll tests exercise `route_widget_in_rect`; ordinary containers exercise
    `route_widget` over the full content rectangle.
  - Tree walking performs no weak upgrade for identity, topology, lookup, dispatch, or concrete
    runtime state access and performs no hash lookup, registry access, or downcast. A concrete
    runtime borrows its directly owned state cell at most once per runtime method invocation that
    needs state.
  - P2.4's capture sanitation calls `retains_pointer_capture` and, on loss, the centralized
    `on_pointer_capture_lost` transition only on the current captured container, without a full-tree
    concrete-state query, downcast, registry, or parent callback.
  - Nested geometry remains correct at nonzero origins.
  - Update/layout/paint visit siblings forward; pointer routing visits siblings in reverse z-order.
  - Downstream compile-fail tests prove ordinary callers cannot construct the opaque visitors,
    obtain `&mut Children`/`&mut Node`, call direct node iteration, or swap/replace/take an attached
    collection through framework-provided APIs.
  - Same-cell and cross-cell visitor tests produce access outcomes/observation exactly as
    documented.
  - Zero/multiple visitor submissions panic with the specified diagnostics, while built-ins and the
    downstream conformance example submit the same authoritative collection through both methods.
  - Runtime module docs state phase, traversal, and borrow order.
  - A production-source search finds no `NodeBehavior` trait, implementation, bound, boxed object,
    import, or equivalent all-node behavior adapter; no transitional `UiNode`, `UiNodeData`,
    `UiNodeState`, or `UiNodeId` alias remains; and the obsolete private `MeasureCtx`, `LayoutCtx`,
    `UpdateCtx`, `PaintCtx`, and `InputCtx` adapter types are absent.

  **Completion evidence (2026-07-31)**

  Runtime traversal now matches `NodeKind` directly, dispatches framing, interaction, measure,
  update, and paint through the variant's one inherited `Widget`, and, as completed in P2.3,
  branches to `Container` only for layout, special input, descendant visibility, and scoped child
  visitation. P2.4 adds only the subsequently approved current-capture lifecycle hooks and scoped
  current-owner query; it does not reintroduce a common-phase adapter. Private `Node::measure` is
  the sole node-measurement algorithm and returns preferred `Dimensioni` directly for leaf content
  sizing. The transitional aliases, `NodeBehavior`, all implementations/bounds, and its
  five private phase adapters are deleted. Focused tests pin one-dispatch leaf measurement,
  parent-first/forward update and paint, reverse-z input, same-transaction descendant suppression, and a
  downstream container's public child measurement/policy/layout path including invalid indices.

  The cross-cutting matrix passes formatting, all targets (130 unit and three downstream tests;
  one existing manual performance test ignored), 17 doctests including seven compile-fail cases,
  Clippy with the repository's existing warning baseline, no-default-features, generated docs, and
  separate Glow, Vulkan, and WGPU example checks. Production-source checks find only the intended
  leaf `Box<dyn Widget>` erasure and the two public scoped container contexts. The file dialog
  remains deliberately excluded until P3.2 with all 13 exact restoration markers preserved.

- [x] **P2.4 — Sanitize runtime targets around direct topology changes**

  **Problem**

  Direct child mutation cannot synchronously call Context to clear focus/capture/routed events.

  **Decision needed: No — implements P0.1, P0.3, P0.5, P0.6, and P0.7 using the approved captured-container lifecycle contract**

  **Target contract or migration**

  Add the defaulted public `Container::retains_pointer_capture(&self) -> bool` and
  `Container::on_pointer_capture_lost(&mut self)` methods plus
  `ContainerInputCtx::has_pointer_capture(&self) -> bool` exactly as settled in the normative
  container contract. Do not add `ContainerOption`, overload `WidgetOption::NO_INTERACT`, use
  `FocusPolicy` as a capture lease, ask a parent to inspect a child, synthesize a fake release event,
  downcast `dyn Container`, or give state a `WidgetTree`/Context callback.

  Validate private targets before routing and sanitize missing or ineligible IDs at the next safe
  tree boundary. Rebuild the current live-target view from direct retained traversal rather than
  retaining node entries or a registry across frames. Never resolve an ID through a pointer or reuse
  an ID. A target is structurally eligible only when it still belongs to that tree and every
  ancestor container on its path currently returns `children_visible == true`; the container node
  owning a closed gate remains eligible while its descendants do not.

  Focus, hover, and the current routed recipient require structural eligibility. Capture requires the same
  structural eligibility and, when the captured node is a container, one
  `retains_pointer_capture` call on that target. The query's `true` default means only “this
  container does not locally revoke its existing capture”; it cannot acquire capture, see its ID,
  transfer it, or override a closed ancestor gate. A `false` result makes `WidgetTree` clear its own
  capture before another pointer event is routed. A replacement node at the same child index has a
  different never-reused ID and receives none of the removed node's targets or current event.

  Centralize every `Some(old_capture) -> None` or different-owner transition in `WidgetTree`. Locate
  that exact target without treating a closed ancestor gate as removal. Outside an active input
  transaction, call `on_pointer_capture_lost` immediately if the captured runtime still exists and
  is a container. For a routed release, clear the capture ID immediately, retain at most one private
  `capture_loss_after_update` ID, deliver that release to the old target in the transaction's full
  update, and then invoke the hook. Because the transaction completes before another raw event is
  popped, no pending-loss list, multi-event coalescing, or release/reacquire-in-one-batch logic is
  required. A removed runtime receives no notification because dropping it is definitive cleanup.
  The hook receives no cause or identity and cannot affect parent/tree policy. This closes the local
  lifecycle when capture ends because of pointer release, local rejection, ancestor gating, root
  hiding, transient-target clearing, or capture replacement.

  If sanitation invalidates capture before that pointer stream's drag/release is routed, retain one
  private tree-runtime discard marker after clearing the captured ID. A subsequent drag or release
  is reported as handled without routing it to any target, so ordinary hit routing cannot redirect
  stale input to a parent, sibling, or replacement; the release clears the marker. A fresh
  pointer-down also clears the marker and proceeds through ordinary routing as a new interaction.
  This is stream sanitation owned wholly by `WidgetTree`, not another container option, capture
  owner, identity-bearing token, or parent policy hook.

  `ScrollAreaContainer::retains_pointer_capture` reads its directly owned state once and returns
  `scrolling_enabled && drag_axis.is_some()`. `set_scrolling_enabled(false)` has already cleared
  `drag_axis`, so even a disable/re-enable sequence before sanitation returns false and cannot revive
  capture; only a later matching routed pointer-down can acquire it again.
  `ScrollAreaContainer::on_pointer_capture_lost` clears `drag_axis` without changing offset.
  `RootChromeContainer::retains_pointer_capture` returns whether `RootInteraction` is `Moving` or
  `Resizing`; its loss hook clears that interaction. Existing incompatible-option, hide, dismissal,
  and destruction transitions also clear the local mode; the tree remains the only captured-ID
  owner.

  Run sanitation after the initial `update_ui` synchronization layout and before target-directed
  routing, again after each event's full update/direct topology changes, and defensively at
  focused/captured direct-delivery entry points. Do not query a capture newly acquired by the
  current event before the same transaction's `Widget::update` applies its pointer-down. The
  mandatory sanitation/layout commit completes before the next event is popped;
  `ContainerInputCtx::has_pointer_capture` only identifies established direct capture delivery
  without exposing the ID. Hidden roots clear focus/hover/capture and the current recipient while
  retaining their tree; root destruction releases complete retained runtime ownership. There are no
  result generations to sanitize. Debug builds assert after sanitation that every remaining
  persistent or current target is structurally eligible and that a captured container still reports
  local retention.

  **Acceptance tests**

  - Removing a focused/captured target before `update_ui` clears it before new input routing.
  - Removing a hovered target or the current transaction recipient clears the corresponding target
    at the same safe boundary.
  - Cross-subtree removal during update cannot route later input to the removed or replacement node;
    unaffected persistent targets remain unchanged.
  - Pointer release after target removal is ignored safely and is not redirected to a parent,
    sibling, or replacement.
  - Hidden roots preserve widget/application state but clear focus/hover/capture/current recipient;
    showing does not restore those transient targets. Destroyed roots unmount all tree state and
    release it subject only to already-active state upgrades.
  - Collapsing Disclosure clears descendant focus/hover/capture/current recipient, and expanding does
    not restore them automatically. The Disclosure controls only structural subtree eligibility; it
    never answers retention on behalf of the captured descendant. If that descendant still exists,
    `WidgetTree` invokes its loss hook directly so private drag state cannot survive re-expansion.
  - Disabling a captured ScrollArea through its state handle releases capture before the next routed
    event without giving the state setter a `WidgetTree` or Context capability.
  - Disabling and re-enabling that ScrollArea before sanitation still releases the old capture
    because its private drag axis was cleared; a new matching pointer-down can acquire a fresh
    capture normally.
  - Ancestor collapse, root hiding, and explicit transient clearing call
    `on_pointer_capture_lost` immediately on a still-mounted captured container. A routing-time
    release clears the tree ID immediately, delivers that one release during the transaction's full
    update, and invokes the hook after the old target's update; removal drops the runtime without
    fabricating a callback or input event.
  - A pointer-down that newly acquires capture establishes local mode during its own full update and
    passes retention sanitation before the next queued drag/release is routed.
  - Drag, release, and a later matching pointer-down are three distinct transactions. The drag is
    applied and laid out before release; release cleanup completes before the new press can acquire
    fresh capture. Production code contains no pending capture-loss list or same-batch coalescing.
  - `RootChromeContainer` retains capture exactly while its private interaction is moving/resizing;
    incompatible options, hiding, matching release, and dismissal clear the local mode and the tree
    capture without a second window-manager capture owner.
  - A downstream custom container can rely on the `true` default for ordinary release-bounded
    capture and the no-op loss default when it has no local capture mode. It can override both hooks
    using only private local state when its interaction is externally revocable. The query,
    notification, and scoped current-owner check receive no ID, parent, Context, or tree capability.
  - Destroyed roots expire `RootState` and descendant handles; hidden roots keep those handles live.
  - Debug integrity checks find no remaining target absent from its tree, behind an ancestor gate,
    or locally rejected by the captured container after sanitization.
  - Public API/source checks find no `ContainerOption::RETAIN_POINTER_CAPTURE`, dynamic container
    option/status bag, parent capture-retention override, concrete-container downcast, capture ID in
    state, synthetic capture-loss input, or state-to-tree/Context callback.

  **Completion evidence (2026-07-31)**

  `Container` now exposes only the two defaulted, identity-free local lifecycle methods, and
  `ContainerInputCtx` exposes the scoped current-owner boolean needed by route-before-update
  batching. `UiRuntime` rebuilds structural eligibility by direct retained traversal before direct
  delivery and after layout/update topology boundaries. It owns newly-acquired capture deferral,
  ordered pending-loss delivery, same-owner reacquisition coalescing, and the private stale-stream
  discard marker; removed runtimes are dropped without callback, while still-mounted targets behind
  a closed ancestor gate receive direct local cleanup. No registry, parent retention decision,
  option bag, downcast, synthetic input, or state-to-tree callback was introduced.

  `ScrollAreaContainer` retains capture exactly while scrolling is enabled and a private drag axis
  exists, clears only that axis on loss, and uses the scoped owner query for same-batch drag/release.
  `RootChromeContainer` does the equivalent for move/resize interaction. Focused tests cover local
  rejection, disable/re-enable, newly acquired same-batch routing, ordered drag/release cleanup,
  release/reacquisition coalescing, ancestor gating, same-index replacement without target transfer,
  stale drag/release suppression, cross-subtree removal during update, root hide/show, and downstream
  custom-container use of the public defaults and overrides.

  Formatting and `git diff --check` pass. All targets pass with 140 unit tests and three downstream
  API tests; the existing manual render-performance test remains ignored. All 17 doctests pass,
  including seven compile-fail cases. The no-default-features build, generated docs, and separate
  Glow, Vulkan, and WGPU example configurations pass. Clippy reports only the repository's existing
  warning baseline. Source checks find none of the forbidden capture-policy mechanisms. The file
  dialog remains deliberately excluded until P3.2, with its preserved implementation/tests and all
  13 exact restoration markers unchanged.

  **P2.5 supersession note (2026-07-31)**

  The completion evidence above records the batching implementation that landed in P2.4; it is
  historical evidence, not the final runtime contract. P2.5 must delete
  `capture_awaiting_update`, the pending capture-loss collection, same-batch reacquisition
  coalescing, and batching-only tests. Keep the stale pointer-stream discard marker and the public
  retention/loss hooks. Replace multi-event deferral with the single current-transaction ordering
  specified above, and replace the focused tests with distinct down/drag/release transaction tests.

- [x] **P2.5 — Drain ordered input through full updates/layouts and make rendering paint-only**

  **Problem**

  Current input is an aggregate coupled to `ContextFrame::render_ui`: routing reconstructs a fixed
  event order from pressed/released/held state, update reinterprets the same raw `Input`, layout runs
  around that render-owned update, and only then does paint occur. Multiple OS events cannot observe
  each other's resulting layout, so a resize, scroll, disclosure, or topology change may leave the
  next event hit-testing stale geometry.

  **Decision needed: No — explicit plan-owner decision on 2026-07-31**

  **Target contract or migration**

  Implement the normative “Drain ordered input...” architecture above as one atomic API/runtime
  change:

  - replace aggregate transition fields with a private `VecDeque<RawInputEvent>` plus the committed
    pointer/button/key/modifier snapshot. Every public input-forwarding call appends exactly one
    event; popping applies it to the snapshot in FIFO order and normalizes it once. Do not coalesce
    pointer motion, wheel, key, or text calls and do not reconstruct an order from end-state flags;
    update `UiInputEvent` rustdoc from “this/previous frame” to “this transaction/previous queued
    pointer event” terminology;
  - add public `Context::update_ui(dimensions: Dimensioni)`. It performs an initial full layout
    synchronization, sanitizes targets, and then drains the queue. For each popped event, perform
    cross-root selection/outside-popup policy, route/localize once, traverse every eligible node
    through `Widget::update` once, finalize/sanitize focus and capture, and measure/layout every
    visible root before popping the next event;
  - validate positive dimensions before synchronization or queue mutation and panic with
    `update_ui dimensions must be positive` on invalid input; keep the queue intact on that panic;
  - change `Widget::update` to `Option<&UiInputEvent>` and delete `WidgetInputEvents` plus the
    per-node routed-event map/batch. The one recipient receives `Some(localized_event)`; every other
    eligible node receives `None`. `WidgetUpdateCtx` supplies only the event-time interaction
    snapshot plus `mouse_buttons`, `key_modes`, and `key_codes`. Remove synthetic
    `UiInputEvent::KeyState`/`KeyCodeState`, raw `Input`, `UiRuntime::interaction_for`, context/update/paint
    `scroll_delta`, and every duplicate interaction derivation;
  - keep routing non-mutating with respect to widget state. It decides target, propagation,
    localization, focus, and capture from committed geometry; the immediately following full update
    applies the event. The mandatory full layout then commits resize, scroll translation,
    disclosure visibility, intrinsic size, options, and topology before the next input;
  - split `render_window_manager` into private update/commit and paint/record entry points. Replace
    render-scoped `UiRuntime::begin_frame` with an update-call metrics reset plus
    `begin_input_event(pointer_input_enabled)` for each popped event. The latter clears the current
    recipient, one-event `clicked`, and per-event focus-update marker, then establishes that event's
    root eligibility. Pointer events recompute hover from committed geometry; non-pointer events
    preserve hover. Focus, capture, and held/active interaction persist until their ordered
    transition changes them. Paint performs no transient reset;
  - simplify P2.4 capture handling to the current one-event transaction. Keep tree ownership,
    stale-stream discard, retention/loss hooks, and `has_pointer_capture` for established direct
    delivery. Delete `capture_awaiting_update`, the pending-loss collection, batch coalescing, and
    same-batch paths. A routed release may retain one `capture_loss_after_update` ID until that
    target's current update finishes;
  - remove `WindowEntry.just_opened`. A UI input is fully drained before application code consumes
    its typed event and shows a popup, so the opening input cannot be delivered twice. Every queued
    event remaining after a programmatic show is subsequent input and follows normal outside-popup
    dismissal/routing;
  - split update from rendering. `ContextFrame::render_ui` no longer calls
    `update_and_record_ui`, `UiRuntime::update_paint_frame`, input prelude/epilogue, dispatch,
    update, auto-size, or layout; delete those combined orchestration methods rather than leaving
    unused alternate entry points. Rendering paints
    the last committed tree once, records rendering-only chrome/custom operations, and submits once;
  - track whether Context-owned operations have invalidated the UI commit and the dimensions of the
    last commit. Add `RenderError::UiUpdateRequired` and return it when rendering has no commit, has
    pending input, or uses different dimensions. Weak state handles intentionally cannot set this
    bit, so rustdoc requires a layout-only `update_ui` after external state/topology mutation;
  - an empty input queue performs the initial synchronization layout and zero `Widget::update`
    calls. Add no timer, tick, elapsed-time argument, idle callback, synthetic input, or render-time
    update. Any input-independent invariant belongs in its state setter/topology operation or pure
    measure/layout logic.

  Primary production surfaces are `src/input.rs`, `src/window_manager/input_api.rs`,
  `src/window_manager/mod.rs`, `src/window_manager/window_manager.rs`,
  `src/window_manager/root_chrome.rs`, `src/ui_node/runtime.rs`,
  `src/ui_node/containers/mod.rs`, `src/widget.rs`, `src/widget_ctx.rs`, and every built-in/custom
  `Widget::update` implementation. Update `src/lib.rs`/prelude exports, examples, downstream API
  tests, runtime phase metrics, and Context/root/input rustdoc in the same item; do not leave an
  internal compatibility batch adapter. Put ordered transaction/phase tests in the inline
  `src/ui_node/runtime.rs` tests and `src/window_manager/root_tests.rs`; adapt public signature and
  event-loop usage coverage in `tests/retained_api.rs`. Replace the current test-only zero-argument
  `Context::update_ui()` helper, whose name collides with the new public API; test helpers must call
  the public dimensioned commit and an explicitly separate render helper when paint is required.

  **Acceptance tests**

  - Enqueue interleaved move/down/move/up, wheel, key, and text calls and prove FIFO dispatch with no
    coalescing or fixed-kind reordering. Event-time button/key/modifier snapshots match each event,
    not the final aggregate state.
  - Invalid `update_ui` dimensions panic with the exact diagnostic before layout or dequeue; a later
    valid call drains the still-intact queue.
  - With N queued events, every eligible node receives exactly N `Widget::update` calls, each routed
    recipient sees exactly one `Some` event, all other calls see `None`, and instrumentation records
    one initial synchronization layout plus N post-event layouts. With N = 0 it records one layout,
    zero widget updates, and zero paint until rendering is requested.
  - A root resize event changes committed root/body geometry before the next queued pointer event;
    that event lands using the resized geometry. Equivalent two-event tests cover scroll offset,
    Disclosure collapse/expand, intrinsic-size mutation, and direct topology change.
  - Scroll routing itself leaves state unchanged; the immediately following full update changes
    offset/thumb state and the mandatory layout commits descendant translation before the next
    queued event.
  - Down, drag, release, and a later down execute as separate transactions. Capture/local mode from
    each is finalized before the next; final drag applies before release cleanup, stale streams are
    not redirected, and production code has no pending-loss batch/coalescing machinery.
  - Popup opening from a consumed typed event needs no suppression and the opening event is not
    delivered twice. The next outside press dismisses and then routes once behind it. Showing a
    popup while older input remains queued deliberately exposes it to those subsequent events.
  - `Context::update_ui` never paints or submits. `ContextFrame::render_ui` performs one paint and
    one backend submission with zero input pops, widget updates, measurements, or layouts.
    Rendering without a matching commit, after enqueueing input, or with changed dimensions returns
    `RenderError::UiUpdateRequired` before paint/display-list/backend work and does not update
    implicitly.
  - Programmatic state/topology mutation followed by a layout-only `update_ui` affects paint without
    a widget update. Rustdoc/example code shows the required second commit after application event
    consumption and before rendering; no timer/tick API or idle update path exists.
  - Production searches find no raw `Input` in retained node update contexts, no
    `UiRuntime::interaction_for`, `WidgetInputEvents`, `UiInputEvent::KeyState`/`KeyCodeState`,
    context-level `scroll_delta`, routed-event
    batch/map, `capture_awaiting_update`, pending capture-loss collection, `just_opened`, or
    `update_and_record_ui`/`UiRuntime::update_paint_frame`/other update-layout call reachable from
    `ContextFrame::render_ui`.
  - Textbox, slider, number, text-area, disclosure, nested scroll, capture, key/text, and front-root
    pointer gating tests pass with routed events as their only source; popup dismissal is covered as
    the single documented cross-root exception.
  - Keyboard/text routing selects only the front visible root and its focused node; widgets cannot
    cooperatively assign or clear that focus, and textbox submission leaves it intact.

  **Completion evidence (2026-07-31)**

  Input forwarding now appends one private raw event per public call to a FIFO `VecDeque`; popping
  each event advances the committed pointer/button/key snapshot and produces one normalized
  `UiInputEvent`. `Context::update_ui` performs the initial synchronization layout and then one full
  eligible-tree update plus one committed layout per event. `Widget::update` receives
  `Option<&UiInputEvent>`, while event-time held button/key/modifier state comes only from
  `WidgetUpdateCtx`. The routed batch/map, synthetic held-state events, duplicate scroll channel,
  P2.4 capture-loss batching/coalescing, and popup-opening suppression are removed.

  Rendering now requires a matching input-free UI commit and returns
  `RenderError::UiUpdateRequired` before paint or backend access otherwise. On a valid commit it
  only paints/records/submits; it performs no input drain, widget update, measurement, or layout.
  Examples and retained API documentation show the explicit event-pump, `update_ui`, optional
  application-state observation, second layout-only `update_ui`, and paint-only render sequence.

  Focused tests cover FIFO/no-coalescing and event-time held snapshots, exact N-update/N+1-layout
  phase counts, empty-queue layout synchronization, invalid-dimension queue preservation, geometry
  changes affecting later queued input, Disclosure expansion, popup dismissal with one behind-root
  delivery, per-event capture handling, and render preflight before backend acquisition. Formatting
  and `git diff --check` pass. All targets pass with 149 unit tests, one existing ignored manual
  test, and three downstream API tests; all 17 doctests pass. The no-default-features build,
  generated docs, and separate Glow, Vulkan, and WGPU example configurations pass. Clippy completes
  with only the repository's existing warning baseline. Forbidden-source searches find none of the
  removed aggregate/batch/update-from-render mechanisms. The file dialog remains deliberately
  excluded until P3.2, with its implementation/tests and all 13 restoration markers preserved.

### P3 — Public roots and application migration

- [x] **P3.0 — Delete projection builders and root replacement**

  **Problem**

  Projection construction remains the only current public tree-authoring path.

  **Decision needed: No**

  **Target contract or migration**

  Migrate root creation and all built-in constructors to their fixed typed-handle return shapes and
  concrete runtimes boxed inside unique Nodes. Delete `UiNodeSet`, `UiNodeBuilder`, `NodeBuilder`,
  `NodeOptions` (including its Grid span), private `BuilderChild` edge metadata, builder keys,
  `set_root_nodes`, and `transfer_runtime_state_from`. Delete the P1 temporary adapter
  in this item. Direct retained Grid construction uses `GridItem`; no replacement metadata
  transport is introduced. Delete `ResourceState`, the complete frame-result store/query API, and all public
  generated result identity. Migrate root owners from stored `RootId` alone to `RootHandle`, using
  `handle.id()` for Context lifecycle/policy calls and `handle.state()` for typed chrome observation.
  Use explicit `destroy_root` where lifetime, rather than visibility, should end.

  **Acceptance tests**

  - Windows/dialogs/popups are fully usable without a projection builder.
  - No public operation replaces an installed root while retaining identity.
  - `RootHandle`, `destroy_root`, typed root events, unknown-root behavior, weak-handle expiration,
    and never-reused `RootId` semantics match P0.7.
  - Changing a descendant widget type means dropping/removing that child and constructing a new
    node. A literal root type change requires destroy/recreate and a new `RootId`; examples needing
    stable root identity use a persistent container root.
  - Public imports contain one authoring path.
  - Production searches find no `NodeOptions::grid_span`, `BuilderChild::grid_span`, or equivalent
    projection-only Grid placement transport.

  **Completion evidence (2026-07-31)**

  The projection/builder/root-replacement surface and its compatibility implementation are deleted:
  `UiNodeSet`, `UiNodeBuilder`, `NodeBuilder`, `NodeOptions`, `BuilderChild`, builder keys,
  `set_root_nodes`, and runtime-state transfer no longer exist. Generic resource/frame result stores,
  retained/generated result IDs, and the public generated `Id` module are also gone. Root owners use
  `RootHandle` for lifecycle and typed observation; dynamic roots mutate container descendants, and
  literal root replacement requires destroy/recreate. Downstream API tests construct every built-in
  and a custom container through the sole owning-`Node` authoring path.

- [x] **P3.1 — Migrate examples and external custom widgets**

  **Problem**

  Examples cache strong widget handles, generated IDs, and projections and currently demonstrate
  direct `Widget` implementations on combined state/runtime structs.

  **Decision needed: No**

  **Target contract or migration**

  Migrate `simple`, calculator, `demo-full`, backend cube, texture smoke, and retained custom drawing
  to parameter/state/runtime/builder roles. Construct roots once, retain only the typed handles
  returned by stateful constructors, consume widget events from state, and mutate dynamic membership through
  exposed container state. Replace every old header/tree `Node` use with the corresponding
  `DisclosureParameters::header`/`tree` construction. Migrate the demonstrated stack-direction
  rebuild to `StackState::set_direction`; move initialization-only visual/font/wrap configuration
  into Parameters. Do not manufacture or retain meaningless unit/internal-state handles. Preserve
  the commented `demo-full` file-dialog integration and its
  `P1.2 TEMPORARY: restore in P3.2` markers while migrating the rest of that example; P3.2 owns the
  dependency-complete restoration and refactor.

  **Acceptance tests**

  - No example stores public Node IDs, `UiNodeSet`, or parallel state/result identity.
  - No example imports `NodeStateValue` or uses the old `Node::header`/`tree` constructors.
  - Custom widgets keep the current Widget method signatures.
  - Calculator/demo behavior remains equivalent under deterministic checks.
  - Glow, Vulkan, and WGPU examples compile separately.
  - The non-dialog portions of `demo-full` are migrated and green without deleting, moving, or
    prematurely uncommenting the preserved file-dialog integration regions.

  **Completion evidence (2026-07-31)**

  All examples now construct owning nodes directly, create each root once, retain only useful typed
  weak handles, and mutate dynamic membership/configuration through concrete container states.
  External custom examples use explicit parameter/builder/runtime roles, and demo-full changes stack
  direction through `StackState` instead of replacing a root. Separate Glow, Vulkan, and WGPU
  all-example checks pass. All 13 exact P3.2 restoration markers remain, and `src/file_dialog.rs` is
  unchanged.

- [x] **P3.2 — Re-enable the file dialog with polling sessions and prove local mutation**

  **Problem**

  The preserved `FileDialogState::eval` rebuilds its complete UI on ordinary application-driven
  evaluation, so P1.2 could not remove the last strong-handle leaf adapter while preserving control
  state. Requiring the application to call that evaluator also leaks the dialog's internal state
  machine across the Context boundary. The module/export/demo edges have therefore been commented
  out since P1.2, while `src/file_dialog.rs` and its tests remain in place as the authoritative
  migration input. The capability and its executable evidence must now be restored behind a
  Context-owned lifecycle before release.

  **Decision needed: No — explicit plan-owner decision on 2026-07-31**

  **Options considered**

  1. **Selected: polling session.** `Context::open_file_dialog(request)` returns a
     `FileDialogSession`; Context owns and advances the retained UI/controller, and application code
     peeks at `session.status()` after `Context::update_ui`. This keeps the state machine and widget
     topology inside the window manager without callbacks or application evaluation.
  2. **Rejected: completion callback.** A one-shot callback avoids application polling but adds
     callback storage/removal, reentrancy, and application mutation during Context processing.
  3. **Deferred: blocking/nested event loop.** A synchronous result-returning API needs a host event
     pump and nested-loop policy. Revisit it separately after the retained refactor; P3.2 does not
     emulate blocking with a second public state machine.

  **Target contract or migration**

  Start from the preserved `src/file_dialog.rs` implementation and every searchable
  `P1.2 TEMPORARY: restore in P3.2` marker. Refactor the module in place; do not replace it with a
  newly authored parallel file or discard its history. Restore the `src/lib.rs` module declaration
  and export `FileDialogRequest`, `FileDialogResult`, `FileDialogSession`, and `FileDialogStatus`
  from the crate root and prelude. Restore the complete `demo-full` open/result flow represented by
  the markers, replacing its old dialog-owned field and explicit `eval` call with an optional
  session and post-`update_ui` polling.

  The public lifecycle is:

  ```rust
  let dialog = ctx.open_file_dialog(request);

  // After Context::update_ui:
  match dialog.status() {
      FileDialogStatus::Pending => {}
      FileDialogStatus::Accepted(result) => open_file(result.file_path),
      FileDialogStatus::Cancelled => {}
  }
  ```

  `status()` returns a repeatable owned snapshot. Accepted and cancelled states remain observable
  after the root is gone. `Context::cancel_file_dialog(&session)` explicitly cancels a pending
  dialog and destroys its root. Accept, the Cancel button, and title-bar close do the same. Dropping
  the last session handle abandons a pending dialog; Context observes that weak-session expiry and
  removes the root during the next `update_ui`. Dropping Context marks any still-observed pending
  session cancelled. OK without a valid non-empty filename remains pending. Sessions expose no
  callback, evaluator, widget handle, root handle, or topology access.

  Construct the shell once. Inputs/buttons and folder/file list containers return state handles
  directly, so retain those typed handles at initialization. On refresh, construct row
  state/widget pairs and replace only list `Children`. Consume actions through button/list state.
  Preserve the existing scroll offset across child replacement, then clamp it to the new content
  range during the post-replacement layout. Do not reset it merely because the directory rows were
  refreshed. Remove each temporary marker only after its corresponding restored path compiles and
  has executable coverage; no commented-out dialog code, disabled test, compatibility stub, or
  alternate legacy implementation remains when this item completes.

  **Acceptance tests**

  - Idle pending processing allocates no nodes/state and changes no topology.
  - Refresh changes only row nodes and explicitly updated state.
  - Refresh uses `try_update_with(new_rows, ...)`; unavailable access returns the complete unmounted
    replacement vector as `Err(new_rows)` rather than dropping it through an uninvoked closure.
  - Refresh with a still-valid scroll offset preserves it exactly; shorter or empty replacement
    content clamps it to the nearest valid offset, including zero when no scrolling remains.
  - Removed row handles expire; persistent controls and scroll handles remain live.
  - No root replacement, generated ID, Context editor, or Context token remains.
  - The four polling API types are exported from the crate root and prelude, appear in rustdoc, and
    are usable by downstream code without an opt-in migration feature. `FileDialogState`, public
    `eval`, and completion callbacks are absent.
  - `demo-full` again exposes the dialog flow, including open, navigation, selection, accept, and
    cancel behavior. It polls only after `Context::update_ui` and retains no duplicate dialog state
    machine.
  - A terminal UI outcome is visible on the session in the same completed update, automatically
    destroys the retained root, and is stable across repeated `status()` calls. Explicit Context
    cancellation and last-session abandonment also remove the root deterministically.
  - Every preserved file-dialog test is re-enabled and adapted rather than deleted; focused tests
    cover construction geometry, click-without-hover, navigation, selection, empty accept, button
    and title-bar cancellation, explicit cancellation, abandonment, idle allocation, refresh
    topology, scroll clamping, and weak-handle lifetime under the final API.
  - Repository searches find no `P1.2 TEMPORARY: restore in P3.2` marker, commented-out
    file-dialog compilation/integration edge, legacy strong handle, or dormant duplicate source.
  - Release validation treats restored file-dialog API/docs/tests/demo behavior as mandatory rather
    than accepting the temporarily reduced P1.2 surface.

  **Completion evidence (2026-07-31)**

  `src/file_dialog.rs` now constructs one persistent retained shell behind a private
  `FileDialogController`. Context processes controller actions after each complete retained input
  update and before its matching layout, replaces only the two dynamic Stack child collections,
  and destroys roots on terminal outcomes. The public surface is the four polling types plus
  `Context::open_file_dialog`/`cancel_file_dialog`; no evaluator, callback, dialog widget/root
  handle, generated ID, or legacy strong handle is exported. `demo-full` opens a session and polls
  it only after the example runner's first `update_ui`, and the runner performs the required second
  synchronization update before paint.

  Focused tests exercise UI selection/accept, batched click routing, folder navigation, empty OK,
  Cancel, title close, foreign/terminal cancellation rejection, session abandonment, Context drop,
  row-owner expiry, persistent control/scroll handles, scroll preservation/clamping, exact
  `try_update_with` failure recovery, geometry, and downstream public use. The ignored allocation
  baseline passes when run serially and records zero allocations for idle controller processing.
  `cargo test --all-targets` passes with 161 unit tests and 4 downstream integration tests; the two
  intentionally manual baselines remain ignored. Formatting, doctests, no-default-features,
  rustdoc, and separate Glow/Vulkan/WGPU example checks pass. `cargo clippy --all-targets -- -W
  clippy::all` exits successfully with the repository's pre-existing warnings and no file-dialog
  warning. Source audits find zero restoration markers or legacy file-dialog integration APIs in
  `src`, `examples`, and `tests`.

- [x] **P3.3 — Align the final public API, modules, README, rustdoc, and migration notes**

  **Problem**

  Public documentation currently mixes strong retained state, generated identity, and builder
  projection terminology.

  **Decision needed: No — explicit plan-owner approval of the final documentation/API audit**

  **Target contract or migration**

  Document `Widget`, `WidgetState`, `WidgetParameters`, `WidgetBuilder`, public
  `Container: Widget`, marker `ContainerState`, opaque child visitors, the exact container-only
  layout/input methods, the defaulted `retains_pointer_capture`/`on_pointer_capture_lost` contract
  and scoped `has_pointer_capture` query, the final concrete-runtime ownership rule, typed weak
  handles, built-in
  state/parameter types, owning `Node`/opaque `Children`, `Disclosure` as the old header/tree
  replacement, the absence of generic node visibility, unified
  `RootHandle`/`RootState`/`RootMutationError` chrome and explicit root destruction, state-local
  events/commands, and traversal-order mutation. Include final custom-container construction through
  `ContainerBuilder::create_container` and generic `Node::container`; state the fixed ordinary return
  shape of every concrete built-in constructor and that discarding a weak handle never changes
  runtime ownership. Examples must not imply that callers select handle exposure through Parameters.
  Document the fixed leaf/container constructor table, the intentional removal of
  arbitrary mounted public-field mutation, exact mounted Row/Grid/Stack/Scroll configuration,
  input-preserving `try_update_with`, generic policy/Grid-owned span precedence, no root replacement, the one ordered
  input queue and popup-boundary exception, the explicit `update_ui(dimensions)` drain/commit
  boundary, one full update/layout per input, layout-only empty-queue synchronization, paint-only
  `render_ui`, the no-timer rule, the update-before-render error/precondition, and the complete removal of
  `ResourceState`/frame results, and the
  framework-recursion exemption from the application reentrancy prohibition.
  Custom-widget migration notes show `Widget::update(ctx, Option<&UiInputEvent>)`, direct matching
  of that one event, and `WidgetUpdateCtx::{mouse_buttons,key_modes,key_codes}` in place of
  `WidgetInputEvents` batch helpers.

  The final documentation audit also removes three misleading compatibility remnants rather than
  documenting them as supported architecture:

  - remove the unused `FocusPolicy` argument from
    `ContainerInputCtx::{route_widget, route_widget_in_rect}`. Focus policy remains authoritative
    through the current runtime's inherited `Widget::focus_policy` query; containers that had
    supplied `DragCapture` through the ignored argument move that intent to their `Widget`
    implementation;
  - make the ordered `Input` queue crate-private and remove its crate-root/prelude exports.
    Applications enqueue only through `Context::{mousemove,mousedown,mouseup,scroll,keydown,keyup,
    keydown_code,keyup_code,text}`; there is no second public queue owner;
  - delete the temporary public `WidgetConfig` helper and its exports. Built-in Parameters already
    own initialization-only font/options, while a downstream custom runtime keeps any immutable
    `FontChoice`/`WidgetOption` fields directly.

  Keep `WidgetStateHandle::new(&Rc<RefCell<T>>)` as the explicitly selected downstream
  `WidgetStateOwner` conformance boundary. It accepts a borrowed owner but returns no `Rc`, raw
  `Weak`, or escaping borrow. Hiding that input representation would require a fifth public state
  cell abstraction and a second ownership migration, so P3.3 documents the advanced implementor
  contract rather than introducing one.

  **Acceptance tests**

  - Crate docs/README examples compile where practical.
  - `cargo doc` exposes `Container: Widget` without parallel container measure/update/paint methods
    and exposes no private IDs, cells, legacy container adapter traits, or obsolete builders;
    production-source checks confirm that `NodeBehavior` itself was deleted in P2.3.
  - Container docs distinguish tree ownership of the captured ID, captured-container ownership of
    local retention/loss state, and ancestor ownership of descendant eligibility. They document the
    default/override obligation and per-event route-before-update ordering without introducing
    `ContainerOption` or parent-controlled capture.
  - Docs explicitly state that ContextFrame does not lock state and that no Context token exists,
    while layout-affecting mutation after the last commit still requires another `update_ui` before
    paint.
  - Docs distinguish initialization Parameters from mutable State with concrete examples.
  - Root docs distinguish hide from destroy, document `RootHandle` weak ownership, and define
    `RootState` current chrome queries plus pending `take_changed`/`take_submitted` semantics. They
    define visible dialogs as modal, including exclusive routing, outside-click swallowing,
    frontmost ordering, and restoration when the active dialog closes.
  - Crate-root/prelude exports include `RootHandle`, `RootState`, and `RootMutationError`, but not
    `RootChromeContainer`, `RootInteraction`, or framework-private state-transition helpers.
  - Visibility docs distinguish root visibility from Disclosure descendant gating and expose no
    generic node visibility API.
  - `ContainerInputCtx` exposes no ignored or duplicate focus-policy argument; custom-container
    examples obtain focus behavior only from their inherited `Widget::focus_policy`.
  - Crate-root/prelude exports contain no public `Input` queue or `WidgetConfig`; Context input
    forwarding and concrete Parameters/runtime fields are the only documented paths.

  **Completion evidence (2026-07-31)**

  The public audit removed the ignored focus-policy parameter from both `ContainerInputCtx`
  routing helpers and moved non-default capture intent into the authoritative `Widget` overrides
  on `ScrollAreaContainer`, root chrome, and the downstream custom-container fixture. The
  transitional `InputResult` alias was deleted. A focused runtime regression proves that a
  `Widget::focus_policy` override retains focus without a routing-helper policy channel.

  The ordered `Input` queue and all of its mutation methods are crate-private, with crate-root and
  prelude exports removed. Applications enqueue only through Context forwarding. The temporary
  `WidgetConfig` type and its exports are deleted; custom-render examples now keep immutable
  `WidgetOption` fields directly. `WidgetStateHandle::new(&Rc<RefCell<T>>)` remains the selected
  downstream conformance boundary and now documents the same-allocation, sole-persistent-owner,
  reentrancy, and commit-order requirements.

  Crate rustdoc, public container/node/root/state docs, and README now describe one Parameters /
  State / runtime ownership model, fixed constructor shapes, opaque topology, capture ownership,
  root hide-versus-destroy behavior, one-event update and FIFO commit ordering, the paint-only
  frame boundary, popup routing, visibility separation, and state access without a Context token.
  `MIGRATION.md` records the complete final mapping, including custom widget/container examples,
  exact mutable container configuration, owned-input recovery, and removed compatibility names.
  Two new compiling doctests cover the canonical update/render loop and downstream
  `ContainerBuilder` finalization.

  A 2026-08-01 modal-correction audit added the visible-dialog routing contract to Context and root
  rustdoc, README, migration notes, and the normative architecture section. The private Context
  modal stack contains only `RootId`s; no public modal handle, dialog option, or second ownership
  channel was introduced.

  `cargo test --all-targets` passes with 162 unit tests, 2 intentionally ignored manual
  baselines, and 4 downstream integration tests. `cargo test --doc` passes 12 positive and 7
  compile-fail doctests. Formatting, no-default-features, warning-free rustdoc, and separate
  Glow/Vulkan/WGPU example checks pass. `cargo clippy --all-targets -- -W clippy::all` exits
  successfully with the repository's pre-existing warnings and no P3.3-specific warning. Source,
  export, and generated-rustdoc audits find no production `WidgetConfig`, public `Input`, ignored
  focus-policy routing parameter, `NodeBehavior`, result-store family, private runtime node ID, or
  private root-chrome type. `git diff --check` passes.

### P4 — Correctness after simplification

- [x] **P4.0 — Pin dynamic-mutation, update-commit, and paint-only semantics**

  **Problem**

  Direct state/topology mutation is intentionally traversal-ordered. The runtime must define when
  layout and paint observe successful mutations.

  **Decision needed: No**

  **Target contract or migration**

  Use the settled P2.5 boundary: `Context::update_ui` creates a committed state/layout, and
  `ContextFrame::render_ui` only paints/submits that commit. During each input-driven full update,
  later nodes see earlier successful mutations; already-updated nodes do not rerun, but the mandatory
  post-event layout observes the complete resulting tree before the next event. There is no rollback
  or snapshot promise across nodes.

  A successful programmatic mutation before `update_ui` is observed by its initial synchronization
  layout. A layout-affecting mutation after a commit requires another layout-only `update_ui` before
  paint; a newly queued event also invalidates the commit. Paint and custom-render callbacks are
  observational with respect to application state, topology, interaction, and layout. They may
  update rendering-only caches, but using an independently held state handle to mutate retained UI
  during paint is a downstream contract violation, not a deferred-next-frame feature.

  **Acceptance tests**

  - An event-driven intrinsic-size change affects that transaction's post-event layout, the next
    queued event's hit testing, and the later paint-only render.
  - Mutation of a later/earlier sibling produces the documented distinct outcome.
  - Same-container topology mutation returns `None`; a not-currently-borrowed subtree mutation is
    safe and deterministic.
  - A programmatic layout/topology mutation followed by an empty-queue `update_ui` changes committed
    geometry with zero widget updates. Painting without that explicit commit is outside the typed-
    state contract; Context-detectable invalidation returns `RenderError::UiUpdateRequired`.
  - Paint and custom-render tests use observational callbacks only. Documentation explicitly rejects
    application-state/topology/layout mutation during either callback while allowing private
    rendering-cache mutation.
  - A state-access closure that completes before `Context::update_ui` remains valid; invoking any
    retained update/layout/paint traversal for the same Context from inside that closure is an
    explicitly unsupported reentrant call. If traversal reaches the actively borrowed associated
    state and requests an incompatible borrow, it receives the documented diagnostic without adding
    a global state-handle gate.
  - Framework-authorized child measurement, layout, and visitor recursion remains valid while a
    parent runtime's state borrow is active and is not diagnosed as application reentrancy.

  **Completion evidence (2026-08-01)**

  Focused window-manager characterization now proves that an input-driven intrinsic-size mutation
  is included in that event's post-update layout, changes the next queued pointer event's hit target,
  and supplies the geometry later observed by paint without render-time update/layout work. Separate
  probes pin parent-first forward-sibling semantics: a later sibling observes an earlier successful
  cross-cell mutation, while mutation of an already-updated sibling changes its final state without
  rerunning it.

  Topology characterization proves that the active container visitor borrow makes same-container
  mutation return `None`, while mutation of a not-yet-borrowed sibling subtree succeeds and its new
  descendant participates when traversal reaches that subtree in the same transaction. A
  programmatic child insertion followed by an empty-queue `update_ui` records one tree layout, zero
  widget updates, and committed nonzero geometry for the inserted node. Render preflight coverage
  now also proves that a Context-owned root mutation returns `UiUpdateRequired` before backend work.

  Reentrancy tests hold both mutable and shared application access borrows while deliberately
  entering retained traversal. Incompatible built-in `TextBlock::measure`, `TextBlock::paint`, and
  `Button::update` borrows report the shared phase-specific invariant diagnostic; after the access
  closure ends, ordinary commit and paint succeed. Existing downstream custom-container coverage
  continues to perform framework-authorized nested child measurement, layout, and mutable visitor
  recursion without that diagnostic or a global state-handle gate.

  README, migration guidance, crate/Widget/Context/custom-render rustdoc, renderer documentation,
  and traversal/visitor comments now explain the exact compiler-visible example, local `RefCell`
  borrow guard, no-snapshot mutation ordering, explicit empty-queue synchronization, and
  observational paint/custom-render rule. P4.0 changes no public API, Context/state ownership, phase
  signature, or traversal implementation.

  `cargo fmt --all -- --check`, `cargo test --all-targets`, `cargo test --doc`,
  `cargo check --no-default-features`, `cargo doc --no-deps`, and separate Glow/Vulkan/WGPU example
  checks pass. The suite has 175 passing unit tests, two existing ignored manual baselines, four
  passing downstream integration tests, and 19 passing doctest/compile-fail cases. Clippy completes
  with the repository's pre-existing warning baseline and no P4.0-specific warning;
  `git diff --check` passes.

- [x] **P4.1 — Correct scroll/disclosure edge cases on the single-owner representation**

  **Problem**

  Synthetic composition currently obscures scroll consumption, range calculation, and collapsed
  descendant lifecycle.

  **Decision needed: No**

  **Target contract or migration**

  Centralize pure scroll geometry and input rules plus disclosure visibility after direct ownership
  lands. Use one definition for surface, body, padding, content view, child content extent, range,
  track, thumb, and offset. Consume the whole wheel event only when its clamped offset changes on at
  least one requested axis; otherwise bubble it intact. Use one inverse thumb/drag range and require
  initiating pointer-down ownership; center the thumb on track click. Converge the four possible bar
  presence states with a strict bound rather than open-ended relayout. Keep generic node visibility
  absent; both containers use the exact sub-rectangle routing and child-viewport APIs established in
  P0.1/P2.3.

  **Acceptance tests**

  - Nested boundary scroll bubbles correctly.
  - A diagonal wheel event is consumed as a whole if either axis moves and otherwise bubbles as a
    whole; no residual-axis protocol exists.
  - Content that fits including padding creates no bars; mutually inducing bars converge in at most
    four presence states, and offset-only change does not rearrange children.
  - Thumb paint and drag inversion use identical range math.
  - Drag/up without a matching track/thumb down never captures; track clicks use the centered-jump
    rule.
  - Resize/content replacement clamps offsets without rebuilding state.
  - Collapse retains descendant ownership/handles but clears transient targets and skips every
    descendant phase; removal drops descendants and expires their handles.

  **Completion evidence (2026-08-01)**

  One private `ScrollbarGeometry` now owns the proportional/minimum thumb, thumb travel, maximum
  offset, exact drag inverse, and centered track-click mapping used by ScrollArea paint and input;
  TextArea uses the same paint/drag mapping. ScrollArea commits one documented geometry value for
  its surface, body, padded content view, child extent, range, offset, tracks, thumbs, and corner.
  Its monotonic no-bars-to-required-bars layout loop has a strict four-state bound, lays children in
  stable virtual content coordinates, and applies the content origin exactly once through the child
  viewport transform. Resize and dynamic content replacement clamp the existing offset in place.

  Wheel routing now consumes an entire diagonal event only if its clamped two-axis offset changes;
  otherwise the unchanged event bubbles from either content or track, including at nested-scroll
  boundaries. The generic public-widget router no longer queues a scroll event that it reports as
  ignored. Scrollbar drag/release requires a matching left-button track/thumb press, uses the same
  range as the painted thumb, and track presses outside the thumb center it on the pointer.

  Disclosure remains the sole owner of its descendants and its existing expansion bit remains the
  sole traversal gate. Integration coverage proves collapse retains weak-handle liveness while
  skipping measure, update, paint, custom render, and stale focus delivery; clearing the owned
  children expires their handles. Scroll integration covers nested boundary bubbling, diagonal
  atomicity, padding fit, mutually induced bars, exact thumb inversion, press ownership, stable
  offset-only allocation, resize clamping, and the existing file-dialog replacement/clamp path.

  `cargo fmt --all -- --check`, `cargo test --all-targets`, `cargo test --doc`,
  `cargo check --no-default-features`, `cargo doc --no-deps`, and separate Glow/Vulkan/WGPU example
  checks pass. The suite has 183 passing unit tests, two existing ignored manual baselines, four
  passing downstream integration tests, and 19 passing doctest/compile-fail cases. Clippy completes
  with the repository's pre-existing warning baseline and no P4.1-specific warning;
  `git diff --check` passes. P4.1 changes no public API.

- [x] **P4.2 — Replace pseudo-unbounded measurement and unify axis allocation**

  **Problem**

  `UiRuntime::measure_auto_size` currently passes height `10_000`, and flexible row/grid policies can
  manufacture intrinsic size from that probe. Column, Row, Grid, and Stack also use separate
  measurement/allocation logic, so preferred size, track allocation, spans, and overflow can
  disagree. Window chrome calculations currently leak into `ui_node` measurement.

  **Decision needed: No — use the existing public preferred-size contract directly**

  **Target contract or migration**

  Keep measurement limited to preferred content size: positive `Dimensioni` components may bound
  wrapping, while non-positive components request unconstrained preferred size. Do not introduce a
  second constraint type or apply `Node` placement policy inside node measurement. Expose the same
  indexed child-policy query to measurement and layout so downstream containers have the same
  capability as built-ins.

  Use one private scalar axis cursor with no per-child storage. With no bound, flexible track
  policies use child preferred size and `Fixed` remains exact. Grid rebuilds row-major placements
  only when its topology or column count changes and reads them immutably during measurement; fixed
  spans expose overflow rather than silently growing.

  The private `RootChromeContainer` measures retained application content and owns title/frame/body
  padding, outer minimum size, and conversion between intrinsic client size and outer root geometry
  through `root_chrome_geometry`. `WindowEntry` calls that same pure helper only to coordinate the
  backend viewport; it contains no second chrome formula or measure path. Traversal carries
  transforms/clips directly; no recursive parent-chain reconstruction or cross-window policy inside
  retained-node measurement remains.

  **Acceptance tests**

  - Production searches find no `10_000` or equivalent pseudo-unbounded measurement sentinel.
  - Auto-sized roots containing fixed/auto/fraction/weight/remainder Row/Grid/Stack policies use
    content intrinsic minima instead of the probe size.
  - Row measured width and height agree with the same policies used during allocation.
  - Grid intrinsic size reflects children, explicit/empty tracks, spacing, row/column spans, and the
    fixed-track overflow rule; one Grid-owned placement list feeds both axes until Grid mutation
    rebuilds it.
  - Column/Row/Grid/Stack share one allocation rule without a measurement solver or sizing cache.
  - Generic `src/ui_node` traversal contains no title/close/resize/window-option formula; the one
    private root-chrome module and `root_chrome_geometry` helper determine title height, all hit and
    paint rectangles, minimum size, and client/outer conversion for window/dialog/popup variants.
  - Deep nonzero-origin transform/clip tests pass without `parent_of` or recursive parent-transform
    reconstruction.

  **Completion evidence (2026-08-02)**

  `Widget::measure` now has one purpose: return preferred content, optionally using positive bounds
  for wrapping. It does not apply `Node` placement policy. `Children::measure_child` preserves that
  meaning, and `Children::child_policy` gives every container the separate indexed placement query
  needed for slot planning. No private constraint model, measurement plan object, or numeric probe
  remains. `AUTO_WIDTH` and `AUTO_HEIGHT` independently request unconstrained preferred size on one
  axis, and their `AUTO_SIZE` composite requests it on both with `Dimensioni::default()`.

  Built-in measurement remains an immutable query. A small private scalar `Axis` cursor resolves
  sibling policy, advance, and offered-slot arithmetic without a `Vec`, cache, `RefCell`, or renderer
  dependency. Grid owns row-major placements as derived Grid metadata and uses ordinary mutable
  vectors only inside its mutable layout phase. Warmed measurement/layout across every built-in
  container performs zero allocations. ScrollArea advances by the child's actual laid-out size, so
  fixed child policy no longer relies on policy-resolved measurement.

  `RootChromeContainer` owns the outer/body conversion through `root_chrome_geometry`.
  Focus/capture routing and debug geometry use one transform-carrying root-to-target traversal, so
  parent lookup and recursive transform reconstruction are gone. Focused tests cover preferred-size
  and placement-policy separation, track policies, Grid spans and overflow, Row sizing, and an
  auto-sized Row/Grid/Stack root seeded with a `2000 x 3000` rectangle.

  `cargo fmt --all -- --check`, `cargo test --all-targets`, `cargo test --doc`,
  `cargo check --no-default-features`, `cargo doc --no-deps`, and separate Glow/Vulkan/WGPU example
  checks pass. The suite has 189 passing unit tests, one existing ignored manual baseline, four
  passing downstream integration tests, and 19 passing doctest/compile-fail cases. Clippy completes
  with the repository's existing warning baseline and no P4.2-specific warning; `git diff --check`
  passes. P4.2 adds only the indexed `Children::child_policy` query.

### P5 — Cleanup and measured optimization

- [x] **P5.0 — Remove obsolete ownership, identity, and mutation machinery**

  **Problem**

  Leaving old types would preserve contradictory authoring and lifetime models.

  **Decision needed: No**

  **Target contract or migration**

  Delete strong `WidgetHandle`, `widget_handle`, `WidgetStateHandleDyn`, erased adapter cloning,
  duplicate state dispatch tracking, `NodeBehavior` or any equivalent catch-all node runtime trait,
  projection builders, generated/scoped public IDs, root replacement/state transfer, synthetic
  scroll nodes, Context container editors, mounted cell metadata, Context identity/token code,
  frame state locks, old header/tree `widgets::Node`, `NodeStateValue`, generic node `visible`, raw
  public child callbacks, method-bearing `ContainerState`, `ResourceState`, `FrameResults`,
  `FrameResultGeneration`, `RetainedId`, every generic/root result store and query, and
  obsolete compatibility aliases. Also remove raw `Input`/`interaction_for` from retained update,
  `WidgetUpdateCtx::scroll_delta`/`WidgetPaintCtx::scroll_delta` and their stored duplicate channel,
  aggregate input transition bitsets/text buffers, `WidgetInputEvents`, synthetic
  `UiInputEvent::KeyState`/`KeyCodeState`, routed-event batches/maps,
  `capture_awaiting_update`, pending capture-loss/coalescing machinery, `WindowEntry.just_opened`,
  render-owned update/layout glue, the unconditional middle layout, pseudo-unbounded numeric probes,
  independent container axis solvers, all generic result construction, and every result sink.

  **Acceptance tests**

  - Repository searches find none of the named obsolete production symbols or patterns.
  - No public API keeps removed state persistently alive or requires Context for state access.
  - Examples/tests/docs use the single parameter/state/builder model and contain no old disclosure,
    generic-visibility, frame-result, or mutable-child-borrow surface.
  - Searches find no `UiRuntime::interaction_for`, raw-input retained update context,
    `WidgetInputEvents`, synthetic held-state events, routed batch/map, batching-only capture
    deferral, `just_opened`, update/layout
    reachable from `render_ui`, context-level scroll-delta accessor/storage, `10_000` measurement
    probe, or root-node replacement entry point.
  - Structural type/line counts demonstrate net removal rather than another compatibility layer.

  **Completion evidence (2026-08-02)**

  Repository audits across `src`, `examples`, and `tests` find none of the named obsolete symbols,
  entry points, numeric probes, or duplicate scroll channels. `Context` now owns `Style` and the
  ordered `Input` queue directly; neither has an unnecessary `Rc`/`RefCell` ownership layer, and
  the private input queue is no longer cloneable or externally field-mutable.

  The retained runtime no longer carries the broad `dead_code` allowance, unused identity/layout
  helpers, duplicated display-list snapshots, or the hard-coded erased-adapter debug count. Internal
  child traversal remains behind `Children` instead of converting the authoritative collection to
  raw slices. The superseded 644-line Grid state-placement proposal is deleted. Excluding that
  proposal, this phase removes 201 lines and adds 76 across implementation and tests (net -125),
  with no new type, trait, alias, compatibility adapter, or state store.

  `cargo fmt --all -- --check`, `cargo test --all-targets`, `cargo test --doc`,
  `cargo check --no-default-features`, `cargo doc --no-deps`, and separate Glow/Vulkan/WGPU example
  checks pass. The suite has 189 passing unit tests, one existing ignored manual baseline, four
  passing downstream integration tests, and 19 passing doctest/compile-fail cases. Clippy completes
  with the repository's existing warning baseline and no P5.0-specific warning; `git diff --check`
  passes.

- [x] **P5.1 — Repeat allocation, phase, and code-structure baselines**

  **Problem**

  The design deliberately keeps one state allocation per widget/container and checked dynamic
  borrows. Their cost must be compared with removed rebuilding and adapters.

  **Decision needed: No — plan-owner clarification on 2026-08-02**

  **Target contract or migration**

  Repeat the P0 scenarios against the completed retained runtime. Verify the claimed structural
  removals, exact semantic-node topology, idle allocation behavior, and update/layout/paint phase
  separation. Confirm that concrete runtimes directly borrow their strongly owned state and that
  consumers, including `WindowEntry`, retain only typed weak capabilities. Record release-mode
  timing as an informational full-traversal baseline; do not introduce optimizations or a hard
  timing threshold without a separately approved performance budget. Record code/type counts as
  descriptive evidence rather than treating the deliberately explicit Parameters/State/Builder
  roles as a line-count reduction target.

  A weak state handle is an ownership boundary, not a performance target. Each checked handle
  access performs its ordinary scoped upgrade. A complete window-manager update, layout, or paint
  operation may use multiple non-overlapping checked accesses around retained traversal; do not
  expose a raw `Rc`, raw `Weak`, owner lease, duplicated root snapshot, or strong `RootHandle` to
  reduce that count. Concrete runtime methods instead borrow their directly owned strong state cell
  and perform no weak upgrade.

  **Acceptance tests**

  - Idle file-dialog controller processing allocates nothing and changes no topology.
  - A one-child scroll area contains exactly two application semantic nodes. Each Context root adds
    exactly one private chrome container around the application tree and no synthetic
    title/close/resize nodes.
  - Root reconstruction, reconciliation, erased dispatch, synthetic scroll nodes, and Context state
    validation are structurally absent.
  - Phase baselines separate `update_ui` from rendering. A call draining N events records N full
    eligible-tree update traversals and N + 1 complete layout commits; an empty-queue call records
    one layout and zero updates. A subsequent render records one paint traversal, zero updates, and
    zero layouts.
  - Checked state-handle reads and updates allocate nothing.
  - Concrete runtime state access directly borrows its strongly owned associated state and performs
    no weak upgrade for identity, topology, dispatch, or state access.
  - `WindowEntry` owns no strong `RootState` pointer and accesses root state only through its typed
    weak `WidgetStateHandle<RootState>` capability. The private `RootChromeContainer` remains the
    sole persistent strong owner.
  - Allocation and timing tables are recorded. Allocation/topology/phase invariants are hard
    assertions; timing is informational because P0 defines no performance budget. A stable
    concerning slowdown is reported with its observed cause and requires a separate decision before
    optimization, but raw elapsed time is never a flaky test assertion.

  **Completion evidence (2026-08-02)**

  `src/window_manager/p5_baseline.rs` now repeats the one-widget, 100-application-node, and
  20-content-widget ScrollArea scenarios in a serial ignored release test. Construction is measured
  once; warmed empty-queue synchronization and paint-only rendering are each averaged over 100
  calls. Application semantic nodes are reported separately from total retained nodes so the one
  private root-chrome node is visible rather than silently changing the P0 node-count definition.
  The same test records the three-event transaction split and hard-asserts all topology and phase
  invariants; elapsed time is printed but never asserted.

  A focused non-ignored topology test separately constructs a ScrollArea with one child and observes
  exactly three retained nodes under a Context root: the two application semantic nodes plus the
  root's single private chrome node.

  The release baseline used Rust 1.97.1 (`x86_64-unknown-linux-gnu`, LLVM 22.1.6):

  | Scenario | Application nodes | Total nodes | Chrome nodes | Build allocs | Build bytes | Sync allocs/call | Sync bytes/call | Sync ns/call | Render allocs/call | Render bytes/call | Render ns/call |
  |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
  | One widget | 1 | 2 | 1 | 8 | 1,669 | 3 | 288 | 4,709 | 3 | 109 | 4,570 |
  | 100-node tree | 100 | 101 | 1 | 305 | 21,651 | 891 | 85,536 | 368,749 | 129 | 9,710 | 51,468 |
  | Scroll area with 20 content widgets | 21 | 22 | 1 | 68 | 5,835 | 120 | 11,520 | 37,143 | 41 | 2,065 | 19,163 |

  For allocation comparison with P0, combining the now-separate synchronization and render rows
  reduces steady calls/bytes from `19 / 1,397` to `6 / 397` for one widget, from
  `1,957 / 172,364` to `1,020 / 95,246` for the 100-node tree, and from `497 / 42,244` to
  `161 / 13,585` for ScrollArea. Construction likewise falls from `14 / 2,821` to `8 / 1,669`,
  from `717 / 88,691` to `305 / 21,651`, and from `258 / 32,749` to `68 / 5,835`, respectively.
  The measured current sync-plus-render times were approximately 9.3 us, 420 us, and 56 us versus
  P0's informational single samples of 9.5 us, 1.29 ms, and 294 us. These timing figures include
  test allocation instrumentation and are not a compatibility threshold.

  File-dialog evidence constructs the retained shell once, isolates controller-only idle work, and
  then records complete layout-plus-render idle and refresh rows:

  | Scenario | Total nodes | Allocs | Bytes | Root rebuilds | Tree layouts | Measures | Layouts | Updates | Paints | Informational elapsed range |
  |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
  | File dialog idle | 34 | 23 | 220 | 0 | 1 | 34 | 34 | 0 | 34 | 0.86-1.13 ms |
  | File dialog refresh | 35 | 142 | 7,487 | 0 | 1 | 35 | 35 | 0 | 35 | 0.67-1.15 ms |

  The isolated idle controller window records exactly zero allocations and an unchanged 34-node
  topology. Refresh retains the same `RootId` and persistent control handles, expires replaced row
  handles, and adds only the one newly discovered row. Compared with P0, complete idle allocation
  falls from `612 / 57,782` calls/bytes to `23 / 220`, and refresh falls from `718 / 63,511` to
  `142 / 7,487`. Repeated elapsed samples overlap the P0 single samples and vary substantially with
  filesystem/test scheduling, confirming the decision to keep timing informational.

  Hard phase evidence records `tree_layouts = 4`, `updates = 6`, and `paints = 0` when three queued
  events update a two-node root, followed by the same layout/update counts and exactly two paints
  after `render_ui`. Empty-queue rows record one layout, zero updates, and zero paint until rendering.
  Focused tests additionally prove that 1,000 checked `try_read`/`try_update` pairs allocate nothing,
  a one-child ScrollArea contributes exactly two application nodes, all window/dialog/popup roots
  add one chrome node, and cloning every root consumer capability leaves `RootChromeContainer` as
  the sole persistent strong `RootState` owner.

  Structural source audits find none of `WidgetStateHandleDyn`, `NodeBehavior`, `UiNodeBuilder`,
  `UiNodeSet`, `WidgetHandle`, root replacement/state transfer, `ResourceState`, or the frame-result
  family in production/tests/examples. The only production `Rc<RefCell<RootState>>` is the field on
  `RootChromeContainer`; `WindowEntry.root_state` remains exactly
  `WidgetStateHandle<RootState>`. Built-in runtime phases have 74 direct
  `runtime_read_state(&self.state, ...)`/`runtime_update_state(&self.state, ...)` call sites and no
  phase-time `state_handle()` call, weak identity lookup, registry, or dispatch upgrade.

  A deliberately mechanical whole-`src` count moves from 64 Rust files, 21,282 lines, and 202
  textual type definitions at P0 to 60 files, 23,871 lines, and 359 definitions at P5.1. The type
  increase is expected evidence of the explicit Parameters/State/runtime/Builder split plus inline
  conformance tests, not a new compatibility layer; the meaningful structural gate is the complete
  absence of the old projection, identity, result, adapter, and reconciliation types. P5.0 already
  records the focused cleanup delta separately.

  Reproduce the measurements with:

  ```text
  cargo test --release ui_node_p5_baseline -- --ignored --nocapture --test-threads=1
  ```

  `cargo fmt --all -- --check`, `cargo test --all-targets`, `cargo test --doc`,
  `cargo clippy --all-targets -- -W clippy::all`, `cargo check --no-default-features`,
  `cargo doc --no-deps`, and separate Glow/Vulkan/WGPU example checks pass. The ordinary suite has
  195 passing unit tests, three intentionally ignored manual baselines, four passing downstream
  integration tests, and 19 passing doctest/compile-fail cases. Clippy reports only the repository's
  existing warning baseline and no P5.1-touched-file warning; `git diff --check` passes. P5.1 adds no
  public API, production ownership/runtime path, optimization, cache, index, dirty bit, or timing
  threshold. Full direct traversal remains the measured completion baseline for P5.2.

- [x] **P5.2 — Keep full traversal unless a separate measured optimization plan is approved**

  **Problem**

  Dirty flags, indexes, and retained paint would reintroduce lifecycle complexity if added without
  evidence.

  **Decision needed: No — full direct traversal is the migration baseline**

  **Target contract or migration**

  Complete this migration with full direct traversal. P5.1 has no approved numeric performance
  budget and its measurements remain informational. If a future measurement against a separately
  approved budget demonstrates a concrete miss, do not extend this plan or reopen its correctness
  definition: create and approve a separate plan covering state mutation, topology,
  style/font/atlas changes, resize, scroll, custom rendering, and removal.

  **Acceptance tests**

  - P5.1 records the full-traversal baseline; without an approved budget, timing remains
    informational rather than a release threshold.
  - No cache/index/dirty bit lands without lifecycle and invalidation tests.
  - Completion does not claim incremental rendering when full traversal remains.

  **Completion evidence (2026-08-02)**

  P5.1 records no material regression and no approved budget miss. Its hard allocation, topology,
  and phase assertions remain the evidence for this item; elapsed time remains informational. The
  recorded sync-plus-render allocation counts and elapsed samples improve on all three P0 retained
  runtime scenarios, while the file-dialog controller performs zero idle allocation and no root
  reconstruction.

  The retained execution paths remain unchanged. `Context::update_ui` performs one complete
  synchronization layout and one full eligible-tree update/layout transaction per queued event;
  `UiRuntime` recursively updates and paints every eligible child through direct owned traversal;
  and `ContextFrame::render_ui` paints/submits the complete committed tree without update or layout.
  Root visibility, modal eligibility, and `Container::children_visible` remain semantic gates, not
  dirty-state or incremental-work machinery.

  Production-source audits find no UI traversal dirty bit, retained-paint fragment cache, runtime
  node index, incremental layout/update/paint path, invalidation graph, generation, or revision
  mechanism. Existing committed node geometry, transient input targets, display-list capacity
  reuse, atlas/backend resources, and rendering-only widget caches retain their established owners;
  none skips retained traversal or claims incremental rendering.

  P5.2 adds no production code, public API, test-only runtime hook, cache, index, dirty flag,
  invalidation protocol, timing threshold, or optimization. Full direct traversal is the completed
  P0-P5 migration baseline. Any later incremental traversal or retained-paint work requires a
  separate repository-grounded plan with explicit lifecycle, invalidation, correctness, and
  performance-budget tests.

  The serial ignored release baselines pass and reproduce the P5.1 hard allocation, topology, and
  phase counts: the three-event scenario records four layouts, six node updates, and zero paints
  before render, then exactly two paints with no additional update or layout. File-dialog idle and
  refresh retain 34/35 nodes, perform zero root rebuilds, and preserve the recorded allocation and
  phase counts. Timing samples remain within the previously recorded informational ranges and are
  not asserted.

  `cargo fmt --all -- --check`, `cargo test --all-targets`, `cargo test --doc`,
  `cargo clippy --all-targets -- -W clippy::all`, `cargo check --no-default-features`,
  `cargo doc --no-deps`, and separate Glow/Vulkan/WGPU example checks pass. The ordinary suite has
  195 passing unit tests, three intentionally ignored manual baselines, four passing downstream
  integration tests, and 19 passing doctest/compile-fail cases. Clippy reports only the
  repository's existing warning baseline; `git diff --check` passes.

## Final release validation gate

P1.0 and P1.3 establish the concrete-runtime ownership boundary before the bulk migration. This gate does not
schedule a second construction rewrite; it audits that the P1 boundary survived P2-P5, that no raw
insertion bypass appeared, and that the complete migration is safe to expose. P0-P5 remain internal
until this validation passes.

### R0.0 — Validate the associated state owner at every retained insertion

**Problem**

The final release must prove that no later migration item bypassed P1's concrete
`WidgetStateOwner` boundary or reintroduced raw-box insertion. Rust cannot force an arbitrary safe
custom implementation to use a particular private field, return a handle for that same field, or
avoid publishing a cloning API; those are documented conformance obligations. The framework must
keep every built-in runtime non-cloneable, directly state-owning, and generically validated before
trait-object erasure.

**Decision needed: No — validation of P1.0/P1.3 before the first externally visible release**

**Validation contract**

Audit the normative “Final builder, owner, and stable state-handle contracts,” “Containers own
children in their state,” “Owning node and internal identity,” and root-chrome ownership sections.
P1.0/P1.3 must still be their only implementation owners: builders return associated concrete
runtimes, those runtimes own their state allocation, and generic `Node` constructors accept only
`WidgetStateOwner` runtimes before boxing them. The audit must not invent a second construction
contract or overstate what Rust can prove about arbitrary custom safe APIs.

**Acceptance tests**

- Compile-fail tests prove `Node::widget(Box::new(...))`,
  `Node::custom_render(Box::new(...), renderer)`, and `Node::container(Box::new(...))` cannot insert
  a raw runtime.
- Public API/source checks prove no `OwnedWidget`, `OwnedContainer`, free construction factory,
  `EXPOSE_STATE`, or state keep-alive trait exists.
- `WidgetBuilder::Parameters` and `ContainerBuilder::Parameters` both implement the same public
  `WidgetParameters` marker; no unbounded container-only parameter role contradicts the four-role
  contract.
- `WidgetBuilder::create_widget` and `ContainerBuilder::create_container` each consume Parameters
  exactly once and return their associated concrete runtime. A compile/runtime test moves
  non-cloneable child Nodes into Column and `RootState` without cloning, loss, or a fifth public
  construction role.
- Constructor signature tests prove application-meaningful built-ins return a typed handle with
  their runtime/completed node, while stateless/internal constructors return only the runtime/node;
  no Parameters value or call-site option changes the return shape.
- Every built-in runtime privately owns one strong state `Rc`, returns a weak handle for that exact
  allocation, and consults the same allocation during its runtime phases. Custom-builder docs state
  this same safe conformance contract.
- Each built-in runtime method borrows its directly owned associated state at most once for that
  invocation; traversal performs no identity/topology/dispatch weak upgrade.
- Each window-manager boundary operation that needs root data upgrades its private `RootState` weak
  clone at most once and scopes the borrow before invoking retained tree traversal.
- `WidgetStateHandle::new` accepts only a borrowed strong owner and exposes no raw `Weak` or strong
  pointer; cloning a `WidgetStateHandle<NonCloneState>` remains supported.
- `try_update_with` remains part of the final handle API and returns owned input unchanged on failed
  upgrade/borrow.
- Dropping an uninserted or removed concrete runtime makes all weak handles become
  non-live and return `None` after active access closures release temporary upgrades.
- Equivalent compile-time and lifetime coverage exists for external custom containers through
  public `ContainerBuilder`/`WidgetStateOwner`, while built-in convenience constructors still return
  a completed `Node`.
- Root creation uses a directly state-owning `RootChromeContainer`; dropping the
  `WindowEntry` expires both the `RootHandle` state capability and every descendant capability.
- The public `Widget` trait has exactly its P0-frozen runtime methods, including
  `Widget::update -> ()`, and remains distinct from `WidgetState`.
- Public `Container` still has `Widget` as its supertrait; its concrete runtime dispatches inherited
  `Widget` calls once and opaque child visitors reach the same directly owned state. Its defaulted
  capture lifecycle hooks expose no ID/tree/parent capability, read/clear only the captured
  container's local state when overridden, and the scoped current-owner query returns only a bool.
  No raw child callback reappears.
- Final documentation/examples explain that the concrete runtime is the retained widget/state owner,
  that convenience-constructor return shape controls whether a handle is immediately returned, and
  that custom builders must preserve the `WidgetStateOwner` conformance contract. They contain no
  raw-box insertion signature or staging migration path as a supported alternative.
- Crate-root, `retained`, and prelude exports expose owning `Node`, `WidgetStateOwner`, the builder
  traits, marker `ContainerState`, opaque visitors, and no old header/tree `Node`.
- Production-source/API searches find no raw-box overload on `Node`, no owner wrapper, no optional
  generic factory result, and no second supported construction path.
- Allocation and phase measurements remain within the P5.1 baseline; any regression gets a separate
  evidence-backed decision rather than weakening ownership.

## Known defect matrix and ownership

| Defect | Current cause | Simplification first | Fix/verification item |
|---|---|---|---|
| Removed widget state remains alive | Strong application handle | Runtime strong owner/application weak handle split | P0.2/P1.0 |
| Same state can be projected twice | Strong handle cloning | Non-cloneable built-in runtime + unique Node; documented custom conformance | P1.2/P1.3 |
| Typed mutation and runtime behavior are conflated | Built-in struct implements both roles | Parameters/State/Builder split | P0.1/P1.1 |
| Widget/container common phases have parallel dispatch | Private `NodeBehavior` plus forwarding `WidgetNode` | Public `Container: Widget` and one supertrait dispatch path | P0.1/P2.3 |
| Reentrant state access can panic | Infallible `RefCell` borrow | Checked per-cell handles | P0.2/P1.0 |
| Application stores handle plus node ID | Generic result lookup | State-local events/commands | P0.4/P1.1 |
| Typed change/submit delivery is underspecified after leaf results disappear | Generic per-frame flags hid per-widget API and persistence rules | Exact state-local counters, recording points, and silent-setter contract | P0.4/P1.1 |
| File dialog blocks strict P1.2 adapter removal | Its full-projection rebuild reuses strong leaf handles before state-owned Row/Stack/ScrollArea exist | Preserve and comment out compilation/integration edges during the internal interval; restore through final state-owned containers | P1.2/P1.3/P2.0/P2.2/P3.2 |
| File dialog rebuilds every evaluation | Projection is only topology API | State-owned Children | P0.3/P3.2 |
| List interaction follows position | Builder ordinal identity | Persistent unique nodes/state | P1.3/P3.2 |
| Scroll offset resets/recreates | Root replacement/synthetic state | Persistent scroll state | P2.2/P3.2 |
| Scroll area has synthetic semantic nodes | Decoration modeled as nodes | One container runtime | P2.2/P4.1 |
| A container can acquire capture but cannot report later local invalidation | `ContainerInputResult::Captured` has no matching state-derived retention query | Defaulted `Container::retains_pointer_capture`; captured container declares local continuation while `WidgetTree` owns/clears the ID | P0.1/P2.4 |
| Tree capture can end while a retained container keeps stale local drag state | Ancestor gating/release clears the runtime ID without notifying the captured abstraction | Defaulted `Container::on_pointer_capture_lost`; `WidgetTree` notifies only the old captured target if it still exists | P0.1/P2.4 |
| Scroll disable cannot synchronously clear tree-owned capture | State setter correctly has no tree capability | State resets local drag/offset; `ScrollAreaContainer::retains_pointer_capture` reports false; target sanitization releases capture before routing | P2.2/P2.4 |
| Scroll routing is described as both non-mutating and offset-mutating | Routing and update responsibilities were conflated | Routing selects/assigns the current event; unit-returning Widget update mutates state | P2.2/P2.5 |
| Public header/tree `Node` collides with owning `Node` | Old widget was not classified | Reserve `Node`; absorb behavior into Disclosure | P0.6/P1.1/P2.1 |
| Disclosure recreates adapters | Strong handle adapted per phase | One state-owning runtime | P2.1 |
| Whole attached child collections can be swapped | Raw `children_mut`/callback authority | Marker state plus safe inherent ops and opaque visitors | P0.3/P2.3 |
| Stale focus/capture after direct removal | Context no longer receives edit callback | Live-target sanitization | P2.4 |
| Generic node visibility is specified but behaviorally absent | Unused `UiNodeState::visible` field | Remove it; separate root visibility and container descendant gating | P0.6/P2.1/P2.4 |
| Root lifetime has no destruction operation | WindowEntry can only be hidden | Explicit `destroy_root` with immediate unmount/ownership release and active-access-safe final drop | P0.7/P1.4 |
| Root/chrome observation would require a parallel result mechanism | Chrome is special-cased outside retained typed state | One private `RootChromeContainer` with public `RootState` and `RootHandle` | P0.7/P1.4/P3.0 |
| Built-in container callers should not repeat the runtime-to-node wrapping step | Factory returns raw `Box<dyn Container>` | Built-in convenience constructor returns `Node`; custom insertion accepts concrete `Container + WidgetStateOwner` | P0.3/P1.3 |
| Container rollout previously assigned every built-in to the atomic foundation batch | Foundation and concrete migrations were conflated | Atomic Node/visitor/Column/Disclosure slice; Grid follows as an early state-owned correction, Row/Stack and ScrollArea extend it later | P1.3/P2.0/P2.1/P2.2 |
| Runtime phases cannot return a handle-unavailable outcome during traversal reentrancy | Phase signatures have no state-access outcome channel | Forbid top-level update/layout/paint inside state-access closures; local diagnostic only | P0.2/P4.0 |
| A failed state access can drop a moved, unmounted node before insertion | Plain closure capture gives the handle no way to return ownership | `try_update_with` validates access first and returns the exact input on failure | P0.2/P1.3 |
| `Widget::update` generic results have no runtime consumer after result removal | `ResourceState` historically fed `FrameResults` | Change update to return `()` and remove the complete generic result family | P0.1/P0.4/P5.0 |
| Raw input and routed events can disagree | Aggregate input is reconstructed for routing and independently interpreted by `interaction_for` | FIFO raw-event queue; normalize once and deliver at most one localized event in each full update transaction | P2.5/P5.0 |
| Context scroll accessors duplicate routed input | Scroll delta is copied into phase context state | Remove update/paint context accessors; inspect the one localized current event only | P2.5/P5.0 |
| Input, update/layout, and paint are coupled to one render call | Render owns aggregate dispatch plus pre-route/pre-update/post-update layout | Explicit `update_ui`: one sync layout and one full update/layout per event; `render_ui` paints/submits only | P2.5/P4.0 |
| Auto-size uses a `10_000` pseudo-unbounded probe | Public dimensions already define non-positive axes as unconstrained preferred-size requests | Pass zero only for each auto-sized axis; do not add a parallel constraint model | P4.2/P5.0 |
| Row/Grid measurement can disagree with allocation | Measurement applied placement policy and containers used independent track rules | Keep measurement content-only; use one bounded track allocator and one Grid placement list per call | P2.0/P4.2 |
| Mounted container configuration is underspecified | Old public fields and builder reconstruction blur initialization and state | Exact Row/Grid/Stack/Scroll state setters; immutable node policy and mutable Grid-owned child span | P1.1/P2.0/P2.2 |
| Grid span leaks through every generic node and container context | Parent-child edge data was modeled as intrinsic node data | `GridItem` construction plus one `GridState` authority for children, spans, and tracks; private builder-edge bridge only until P3.0 | P1.3/P2.0/P3.0 |
| A custom container can visit different child collections by phase | Safe Rust cannot relate two opaque visitor calls across methods | Document one-authoritative-`Children` conformance obligation and test examples | P0.1/P2.3 |
| A visitor can omit or repeat its one child submission | The visitor API has no `Result` channel | Framework invariant panic with container/type/phase diagnostic | P0.1/P2.3 |
| A mounted root cannot change widget type in place | Stable `RootId` and root replacement have conflicting lifetime semantics | Persistent container root for dynamic content; otherwise destroy/recreate with a new ID | P0.7/P1.4/P3.0 |
| Keyboard focus, pointer activity, and cross-root focus ownership are conflated | One per-tree `focus` target, front-root keyboard dispatch, and `FocusPolicy`/`HOLD_FOCUS` serve overlapping roles | Complete P0-P5 and remove the transitional tree/payload/result/routing shapes before choosing replacement types | Deferred post-refactor focus redesign section; separate follow-on plan |
| Optional `CustomRenderKey` has no public retained-node construction path | Builder removal drops key injection | Backend-typed `Node::custom_render` constructor | P1.2 |
| Raw boxed runtime does not prove an associated state is retained | A raw insertion boundary can bypass the builder allocation | Opaque framework-created retained owner | P1.0/P1.3/R0.0 |
| Late runtime-owner hardening would cause a second downstream API migration | Final ownership introduced after bulk migration | Establish `WidgetStateOwner` and generic insertion before bulk conversion | P1.0/P1.3 |

## Cross-cutting validation

Run after every completed production item:

```bash
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

Example backend features are mutually exclusive for concrete example execution and are checked
separately. Where supported, add targeted Miri coverage for same-cell/cross-cell access, weak
lifetime, node/container drop, direct topology mutation, and target sanitization. No Miri test should
be needed for raw-pointer dereference because the target contains none.

From P1.2 until P3.2, these commands intentionally compile the reduced internal surface because the
file-dialog module/export/demo edges are commented out. Each validation report in that interval must
state that `src/file_dialog.rs` and its tests remain preserved but uncompiled, list the exact
restoration marker count, and must not claim file-dialog coverage. P3.2 restores those edges and
their tests before rerunning the same matrix. Any release-oriented validation additionally requires
zero `P1.2 TEMPORARY: restore in P3.2` markers and the final polling-session exports to be present.

Prefer deterministic assertions for state, consumed events, geometry, event order, focus/capture,
operation counts, weak liveness, and allocation counts. Screenshots may supplement but not replace
them.

Update together:

- crate-level retained UI documentation and prelude;
- rustdoc for all four widget roles, `WidgetStateOwner`, public `Container: Widget`, marker
  `ContainerState`, opaque child visitors, the exact container-only scoped context methods and
  `retains_pointer_capture`/`on_pointer_capture_lost` lifecycle hooks, runtime/handle access, owning
  `Node`/opaque `Children`, Disclosure, visibility boundaries,
  exact mounted container configuration, input-preserving access, and unified
  `RootHandle`/`RootState`/`RootMutationError` chrome and lifecycle;
- README construction, state mutation, event consumption, dynamic list, custom-render construction,
  explicit update/drain before paint, empty-queue layout synchronization, paint-only/custom-render
  preconditions, render-reentrancy, scrolling, and destruction examples;
- simple, calculator, demo, custom drawing, texture, and backend examples;
- file-dialog implementation/tests;
- migration notes explaining the combined-widget split, old header/tree `Node` to Disclosure,
  bound-free weak-handle cloning, fallible access, direct topology, no-reparent enforcement,
  failure-preserving owned input, traversal order, root hide versus destroy, unsupported root
  replacement, typed root chrome and removal of all frame results, unit-returning widget update,
  absence of generic node
  visibility, immutable node placement, tree-owned capture versus captured-container local
  retention/loss versus parent descendant gating, the FIFO input queue/popup-boundary exception,
  one full update/layout transaction per input, layout-only empty-queue synchronization,
  paint-only rendering and no timer/idle update, explicit
  intrinsic constraints/shared axis allocation, removal of public widget IDs/results, and the one
  externally visible generic state-owning-runtime insertion boundary.

## Suggested implementation sequence

1. Land characterization and establish the final `Widget` signatures, including
   `Widget::update(Option<&UiInputEvent>) -> ()`.
2. Add parameters/state/builder/runtime-owner/weak-handle primitives and migrate one Checkbox plus
   one unit-state widget end to end.
3. Apply the fixed constructor/mutation table while splitting the remaining built-ins, defining only
   the listed state-local values, events, and commands.
4. Comment out the tracked file-dialog compilation/export/demo edges with the exact P3.2 restoration
   marker while preserving `src/file_dialog.rs` and its tests in place; then store `Box<dyn Widget>`
   directly, add the final generic `Node::widget`/`Node::custom_render` paths, and delete erased
   handle dispatch without exposing a raw insertion boundary.
5. Land the uniquely named owning Node, private runtime IDs, placement methods, marker
   `ContainerState`, constructible opaque Children, opaque traversal visitors, Disclosure, and the
   first state-owned column container as one compile-safe public batch.
6. Convert the other containers with the exact mutable configuration APIs, then establish one
   private `RootChromeContainer` per WindowEntry with `RootHandle`/`RootState`, explicit destruction,
   no application-child replacement, and no frame-result channel.
7. Switch traversal, target sanitization, Disclosure, and scroll area to direct ownership; complete
   container capture acquisition with the defaulted local-retention/loss hooks and scoped
   current-owner query while keeping the ID in `WidgetTree`, remove generic node visibility, replace
   aggregate input with a FIFO queue, drain it through one full update/layout transaction per event,
   and split explicit UI commit from paint-only rendering.
8. Remove public projection/root replacement and migrate one small example completely.
9. Replace pseudo-unbounded probes with the existing unconstrained preferred-size convention, use
   one bounded track allocator, and centralize chrome conversion in the retained root-chrome helper
   shared with the window boundary.
10. After Row/Stack/ScrollArea prerequisites exist, use the preserved source and markers to re-enable
    and migrate the file dialog as the dynamic-topology proof; restore its exports/demo/tests and
    remove every temporary marker before remaining examples/docs and release validation.
11. Delete all obsolete ownership/identity/mutation machinery, repeat baselines, and optimize only
    from evidence.
12. Run R0.0 and the full validation matrix against the P1 concrete-runtime ownership boundary, then and only
    then merge/tag/release the migration for downstream consumption.

P1 ownership changes may need one integration branch: weak handles are not valid until the concrete
runtime retains the strong state, and direct child mutation is not valid until runtime target
use tolerates removal. Keep commits mechanically reviewable, but expose no intermediate raw-box
contract; the externally visible branch must include the final ownership model and documentation.

## Final release completion definition

This is an audit checklist over the Target architecture and protected P0 baseline. If an item
conflicts with its defining section, the defining section controls until the explicit behavior
change process updates both.

The migration is complete when:

- the public `Widget` trait remains the only common phase contract for leaves and containers, and
  its final `update` method accepts `Option<&UiInputEvent>` and returns `()`;
- public `Container: Widget` is implementable by downstream custom containers, adds only
  opaque child visitation/layout/descendant-visibility/special-input behavior plus the defaulted
  local pointer-capture-retention/loss hooks and scoped current-owner query, and redeclares none of
  the common `Widget` phases;
- `NodeBehavior`, its implementations/bounds, and any equivalent catch-all runtime adapter are
  absent; private traversal dispatches common phases once through `Widget` and branches to
  `Container` only for container-specific work;
- each requested runtime invocation has one common `Widget` dispatch path; the initial
  synchronization layout and each post-input layout may invoke measurement and do not imply only
  one measure call per application loop;
- widget/container/chrome update constructs and returns no generic result; typed state events,
  `WidgetUpdateCtx`, and routed-input results are the respective application, focus, and
  capture/consumption mechanisms; `ResourceState` and frame-result APIs are absent;
- `WidgetState`, `WidgetParameters`, and `WidgetBuilder` have distinct data/construction roles, and
  marker `ContainerState` has no child-access methods; both `WidgetBuilder::Parameters` and
  `ContainerBuilder::Parameters` implement `WidgetParameters`;
- `WidgetStateOwner: Widget` associates every retained concrete runtime with its state type and
  returns a weak checked handle for the runtime's directly owned state cell;
- `WidgetBuilder::create_widget` and `ContainerBuilder::create_container` return their associated
  concrete runtimes directly; there is no owner wrapper or generic optional factory result;
- each public application-state widget constructor returns `WidgetStateHandle<T>` plus its concrete
  runtime, and each public dynamic built-in container constructor returns
  `WidgetStateHandle<C>` plus a completed `Node`;
- each concrete widget/container constructor has one fixed, documented ordinary return shape; no
  public exposure selector exists;
- `Checkbox`, `Button`, `ListItem`, `ListBox`, `Combo`, `TextBlock`, `ColorSwatch`, `Slider`,
  `Number`, `Textbox`, `TextArea`, and every public dynamic built-in container return a typed handle;
  `Custom` and explicitly fixed/internal containers return only their runtime/completed node, and the
  old widget `Node` is retired into exposed `DisclosureState`;
- every concrete `WidgetStateOwner` owns the only persistent strong allocation for its associated
  `State`, and every built-in `state_handle` observes the exact allocation used by runtime phases;
- discarding or not returning a weak handle does not change concrete-runtime strong state ownership;
- application state handles contain only a typed weak state capability, clone without requiring
  `T: Clone`, and use checked closures; `WidgetStateHandle::new` accepts a borrowed owner and exposes
  no raw `Weak` or strong pointer;
- `try_update_with` checks upgrade/borrow before committing its owned input and returns that exact
  input as `Err(input)` whenever access is unavailable; ordinary closure capture is documented as
  non-recovering;
- state/topology access never checks Context identity, mount state, frame state, or a global lock;
- same-cell conflicts between checked handle operations return `None`, while unrelated available
  cells can be accessed regardless of `ContextFrame` lifetime;
- state-access closures finish before retained update/layout/paint traversal; reentrant traversal
  from inside a closure is documented as unsupported and is not implemented with a frame/write
  gate. A layout-affecting mutation after a UI commit requires another explicit commit before paint;
- framework-owned nested `Children::measure_child`, `ContainerLayoutCtx::layout_child`, and opaque
  visitor traversal are authorized recursion, not application rendering reentrancy;
- `WidgetStateHandleDyn`, erased handle cloning, and duplicate state dispatch are absent;
- every built-in leaf has explicit Parameters, concrete State, fixed constructor return shape, runtime
  Widget, and Builder responsibilities;
- any application-observed widget values, events, and commands require an exposed typed state;
  internal runtime state never leaks through node/result identity;
- every fixed built-in change/submit event uses the specified private saturating count and
  one-occurrence `take_changed`/`take_submitted` API; ordinary programmatic setters are silent and
  Combo alone retains its documented clamp exception;
- keyboard/text events are delivered only through the front visible root and then to that
  runtime's focused node; textbox submission keeps that target, and widget state/update contexts
  expose no cooperative focus mutation;
- crate-root/prelude `Node` is the only public type with that name, is unique and non-cloneable, and
  owns one boxed concrete widget or container runtime; old `widgets::Node`, `NodeStateValue`, and
  compatibility aliases are absent;
- public generic `Node::widget<W: WidgetStateOwner>(W)` and backend-typed
  `Node::custom_render<W: WidgetStateOwner, B>(W, CustomRenderHandle<B>)` are the complete leaf
  insertion paths;
  `CustomRenderKey` remains private and registry preflight rejects invalid erased keys;
- `Node::with_policy` is the complete generic pre-insertion placement surface, and `NodeRuntime`
  has no generic visibility field, Grid span, or mutation API; `GridItem` supplies Grid-only
  placement and `GridState::set_span` mutates that edge without replacing the child;
- private process-unique runtime IDs support focus/capture/routing and are never exposed or stored in
  state handles;
- container runtime state owns a private opaque `Children`; application-dynamic container
  constructors return a weak checked state handle with only safe inherent operations, while
  fixed/internal constructors may return only a completed node;
- mounted Row widths/item height, Grid child spans/tracks, Stack width/height/direction, and Scroll
  offset/enablement have the exact state getters/setters and edge semantics specified above; Column
  adds no local layout configuration, Disclosure exposes expansion, and framing/base options remain
  construction-only;
- disabling ScrollArea synchronously resets state-owned drag/offset, while tree sanitization releases
  tree-owned capture before another event is routed by observing
  `ScrollAreaContainer::retains_pointer_capture == false`; disable/re-enable cannot resurrect the
  cleared drag/capture, `on_pointer_capture_lost` clears drag after externally caused loss, and
  routing assigns the current event but does not apply scroll state changes;
- `Children::new`/`Default`/`FromIterator<Node>` support downstream construction, but built-in
  states expose no whole collection and no framework-provided API returns an attached node;
- only framework-created opaque visitors reach container child collections; ordinary callers cannot
  construct an extraction callback, swap collections, or reach direct forward/reverse node
  iteration or any `&mut Node`;
- each container visitor method submits exactly one collection or triggers the specified invariant
  panic diagnostic; downstream containers are documented and tested to submit the same
  authoritative `Children` from immutable and mutable visitor methods;
- `ContainerLayoutCtx` and `ContainerInputCtx` expose exactly the generic policy/layout/geometry,
  current-container capture bool, and full/sub-rectangle routing methods specified above, with
  deterministic invalid-index, clipping, and coordinate behavior;
- successful removal drops nodes rather than returning/detaching them;
- stale runtime targets are validated and sanitized without a registry, pointer, Context token, or
  eager editor callback;
- `WidgetTree` is the sole owner of the captured runtime ID; only the current captured container may
  report whether its own local interaction remains active through `retains_pointer_capture` and
  clear that mode through `on_pointer_capture_lost`; ancestors affect descendants only through
  `children_visible` and cannot inspect, mutate, or override a child's local capture state;
- no `ContainerOption::RETAIN_POINTER_CAPTURE`, dynamic container status/option bag, concrete
  container downcast, parent-controlled retention hook, or state-to-tree/Context capture callback
  exists;
- each WindowEntry owns one persistent tree rooted by exactly one private `RootChromeContainer`;
  that container strongly owns public `RootState` and exactly one immutable application-child slot,
  while WindowEntry retains only a framework-private weak state clone;
- window/dialog/popup creation returns cloneable non-owning `RootHandle`; `handle.id()` addresses
  Context lifecycle/policy operations and `handle.state()` exposes the same checked weak typed-state
  access as widgets/containers;
- `RootHandle`, `RootState`, and `RootMutationError` are exported at crate root/prelude, while the
  chrome container, interaction enum, and framework mutation helpers remain private;
- root reads exist only on `RootState`; Context root setters return
  `RootMutationError::{UnknownRoot, Borrowed}` explicitly and keep
  z-order/backend/transient-target side effects coordinated with the state mutation, while
  front/destroy return `false` for an unknown ID;
- preserving a `RootId` while replacing the application child is unsupported, so dynamic root
  content uses a persistent application container or destroys/recreates the root with a new ID;
- `Context::destroy_root` removes and releases the root, returns `false` for unknown IDs, never
  reuses `RootId`, and expires root/descendant handles after active accesses; hide/show retains the tree,
  state handles, and pending typed events while clearing transient interaction targets;
- title close and popup outside dismissal hide and increment `RootState::take_submitted`; actual
  user move/resize geometry changes increment `take_changed` and expose current active mode, while
  programmatic root operations are silent;
- each Context has at most one visible popup; showing another silently hides and sanitizes the
  previous popup without recording a submission, while nested popup behavior remains out of scope;
- root measure/layout/routing/update and paint underlay use the ordinary retained-container path,
  generic tree capture owns the moving/resizing target,
  `RootChromeContainer::retains_pointer_capture` reports only its local interaction mode and
  `on_pointer_capture_lost` clears it without receiving an ID, and one pure chrome geometry helper
  supplies all phase, compositor, and backend rectangles; only popup outside-hit detection records
  across the tree boundary, while the documented post-tree chrome overlay is rendering-only and
  reads the same `RootState`;
- `ResourceState`, `FrameResults`, `FrameResultGeneration`, `RetainedId`, and all generic/root result
  lookup are absent;
- Disclosure and scroll area each have one direct container runtime and one associated state owner;
  Disclosure owns the old header/tree presentations and collapsed descendants skip every phase
  while remaining owned;
- root visibility and container descendant gating are the only visibility mechanisms; collapsing
  clears descendant transient targets without restoring them on expansion;
- the file dialog changes only row children on directory refresh, preserves then clamps its scroll
  offset to the new content range, and performs no idle reconstruction or widget update;
  `src/file_dialog.rs` was refactored in place, the polling request/result/session/status API is
  exported at crate root/prelude and used in `demo-full`, Context owns the dialog lifecycle, all
  preserved tests are active, and no P1.2 restoration marker or commented-out integration edge
  remains;
- focus, capture, input, layout, paint, clipping, scrolling, and custom rendering retain the
  supported behavior required by this migration under deterministic tests, subject to the
  explicitly unresolved post-refactor focus-model defect above;
- every public input call enters one FIFO queue without coalescing; each event is normalized/routed
  once and popup outside dismissal is the sole cross-root boundary exception. Public
  `Context::update_ui(dimensions)` performs one synchronization layout plus one full eligible-tree
  `Widget::update(Option<&UiInputEvent>)` traversal and one complete layout per dequeued event; the
  next event uses that committed geometry. An empty queue performs zero widget updates. Public
  `ContextFrame::render_ui` paints/submits only and returns `RenderError::UiUpdateRequired` for
  detectable missing/stale commits. There is no timer/idle update, raw-input `interaction_for`,
  `WidgetInputEvents`, synthetic held-state events, routed batch/map, duplicate scroll channel,
  batching-only capture deferral,
  `just_opened`, or update/layout reachable from rendering;
- retained measurement contains no large pseudo-unbounded sentinel, uses explicit private
  bounded/unbounded constraints and shared axis allocation, and keeps chrome/client conversion in
  the shared private root-chrome helper while carrying transforms/clips directly;
- strong widget handles, public widget Node IDs/all frame results, projection builders, root replacement/state
  transfer, synthetic scroll nodes, Context editors, mount metadata, and all frame/Context state
  locks are gone;
- raw-box insertion, old header/tree Node APIs, generic node visibility, method-bearing
  `ContainerState`, and raw mutable child callbacks are gone;
- docs/examples/tests describe one state-first, concrete-runtime-owned construction model using
  `WidgetStateOwner`; no raw-box or parallel owner-wrapper surface is documented as supported;
- measurements show zero root reconstruction, zero erased state redispatch, two semantic nodes for a
  one-child scroll area, exactly one internal chrome-container node per root, zero idle file-dialog
  tree allocation, N full updates and N + 1 layout commits for N queued inputs, one synchronization
  layout and zero updates for an empty queue, and one paint with zero update/layout work per render;
  there are zero associated-state weak upgrades during concrete runtime methods and at most one
  direct associated-state borrow per such method, no identity/topology/dispatch weak upgrades, and
  no material regression;
- no incremental traversal or retained-paint cache is added without a separate evidence-backed
  contract.

Meeting this definition, including R0.0 and the final validation pass, completes the migration and
permits its first externally visible release. P0-P5 alone are only an internal integration
milestone and must not be published as a compatibility boundary.
