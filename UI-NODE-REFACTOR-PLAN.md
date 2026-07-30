# Widget/runtime separation and persistent node plan

## Status and scope

This is the sole authoritative UI-node migration plan. It supersedes the obsolete, now-removed
`UI-NODE-PLAN.md`. The following corrections and decisions are authoritative:

1. `crate::Widget` remains the sole common widget execution contract, but this breaking migration
   simplifies `Widget::update` to return `()`; typed widget state is the only leaf-event surface;
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
8. the opaque `OwnedWidget`/`OwnedContainer` ownership boundary lands in P1 before the bulk widget
   and container migrations. Any raw-box step used to keep a private integration commit compiling
   is local scaffolding, is never a public checklist contract, and must disappear within its P1
   owner item;
9. `ResourceState`, `FrameResults`, `FrameResultGeneration`, `RetainedId`, and generic frame-result
   lookup are removed. Typed `WidgetState`/`ContainerState` handles are the sole public observation
   and mutation mechanism, including for root/window chrome; focus remains a `WidgetUpdateCtx`
   operation, and routing owns input consumption/capture;
10. built-in exposure and mounted mutation are fixed by the mapping below. The migration preserves
    demonstrated/intended application state, events, and commands, not arbitrary post-mount
    mutation of every field that was public on a combined widget struct;
11. row/grid/stack layout configuration and scroll enablement are mutable through their exposed
    container states. `Node` policy and grid span are pre-insertion-only and have no mounted setter;
12. moving a unique unmounted value through a state access uses the input-preserving
    `WidgetStateHandle::try_update_with` operation, so `Dropped`/`Borrowed` returns the uncommitted
    input instead of dropping an uninvoked closure capture;
13. replacing a mounted root's application child while retaining its `RootId` is intentionally
    unsupported. Dynamic root content uses a persistent application container; destroying and
    recreating the root yields a new `RootHandle` with a fresh, never-reused `RootId`;
14. this document is the only UI-node migration plan. It owns every still-applicable correctness
    defect from the removed `UI-NODE-PLAN.md`, including explicit constraints, one authoritative
    input-dispatch stream, a two-layout frame pipeline, shared axis allocation, scrolling,
    disclosure, and
    window/transform boundary work.

All common runtime phases belong to `Widget`, including for containers. Public `Container: Widget`
adds only opaque child visitation, layout, descendant-visibility, and container-specific
routed-input hooks; application state belongs to concrete `WidgetState`/marker `ContainerState`
types. No adapter or alternate trait may transfer phase methods onto state. There is also no
frame-wide state lock, Context token, or in-frame/out-of-frame state distinction.

The target keeps the useful part of the prior direction: one persistent, uniquely owned node tree
and optional typed weak application handles. Framework-created
`OwnedWidget`/`OwnedContainer` values retain the sole persistent strong `Rc<RefCell<T>>` for their
`T: WidgetState`; the concrete runtime consumes the same cell through a weak typed handle.
`Some(handle)` exposes an additional clone of that weak capability; `None` withholds it without
changing ownership. P1 establishes this type-enforced boundary before the bulk migration so later
items implement one ownership model.

Breaking public API changes are expected. The migration does not preserve `widget_handle`, strong
`WidgetHandle<T>`, generated builder identity, `UiNodeBuilder`, `UiNodeSet`, public widget `NodeId`,
`ResourceState`, any frame-result lookup API, the `Widget::update -> ResourceState` return, or
context-level `scroll_delta` accessors. Root reads move from Context to `RootState`, and root setters
become fallible through `RootMutationError`.

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

## Goal

Replace projection rebuilding with a persistent retained tree whose application-facing capability is
typed state:

- `Widget` remains the object-safe runtime phase trait for both leaves and containers, with
  `update` simplified to return `()`;
- public `Container: Widget` adds only the object-safe opaque-child-visitation, layout,
  descendant-visibility, and routed-input contract needed by container nodes;
- `WidgetState` marks concrete application state and contains widget-specific operations;
- `WidgetParameters` represents construction input;
- `WidgetBuilder` associates parameters with one concrete state type, chooses optional handle
  exposure, and constructs the concrete boxed runtime through the framework-owned factory;
- final widget construction returns `Option<WidgetStateHandle<T>>` and `OwnedWidget`;
- final custom-container construction returns `Option<WidgetStateHandle<T>>` and
  `OwnedContainer`;
- the opaque owned wrapper always owns the persistent strong `Rc<RefCell<T>>` for its state;
- `Some(handle)` exposes a weak typed application capability; `None` means only that no application
  state handle is exposed;
- `WidgetStateHandle<T>` is cloneable without requiring `T: Clone`; every clone remains weak;
- each public built-in container construction returns its optional typed state handle and a completed
  `Node`, while downstream custom containers may use public
  `Node::container(OwnedContainer)`;
- built-in container state owns its child `Node` values directly;
- application-mutable container membership is changed through an exposed state handle, while fixed
  containers may hide their state; neither path uses a Context editor or identity token;
- public container-state APIs never return a child or lend the complete mutable child collection;
- same-cell conflicts between checked state-handle access operations return `Borrowed` rather than
  panicking;
- unrelated state cells may be read or mutated at any time, including while a `ContextFrame` exists;
- node identity, focus, capture, routing, layout, and painting remain internal runtime concerns;
- removing a node drops its owned runtime/state wrapper and makes any exposed weak state handle
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
- same-frame update-to-layout-to-paint behavior;
- focus, hover, pointer capture, and routed input;
- two-axis and nested scrolling;
- correct intrinsic auto-size without a numeric pseudo-unbounded probe;
- one authoritative input-dispatch stream—ordinary interaction through routed events plus the
  explicit cross-root popup-dismissal boundary—and exactly the required pre-input/post-update tree
  layouts;
- deterministic destruction and stale weak-handle behavior.

## Non-goals

This migration does not initially attempt to:

- make nodes, widgets, containers, state handles, or Context `Send` or `Sync`;
- support concurrent traversal from multiple threads;
- impose a global state-mutation boundary around a frame;
- provide order-independent or snapshot semantics for cross-widget mutation;
- detach, return, move, clone, or reparent an attached node;
- keep state alive after its retained `OwnedWidget`/`OwnedContainer` and all temporary access
  upgrades are gone;
- return a strong `Rc`, raw `Weak`, raw pointer, internal runtime ID, or Context identity as an
  application state capability; the framework factory keeps the sole persistent strong
  `Rc<RefCell<T>>` behind the opaque owner;
- preserve any generic public frame-result channel when the same observation can live in typed
  widget, container, or root state;
- preserve source compatibility with projection builders or strong handles;
- preserve arbitrary post-mount mutation of every field that was public on a combined widget
  struct; the built-in mapping below is the compatibility boundary;
- preserve the old header/tree `widgets::Node` or `NodeStateValue` names;
- replace a mounted root `Node` while retaining its `RootId`;
- mutate an attached node's `Policy` or `GridSpan`;
- add generic per-node hide/show state or public node-visibility mutation;
- add dirty propagation, retained paint fragments, a node registry, or an arena without measurement.

Single-threaded, traversal-ordered execution is a contract. A state or topology mutation succeeds
whenever the target cell is live and its checked borrow is available. If the same cell is currently
borrowed by its widget, container traversal, or another handle closure, access returns `Borrowed`.
Mutating a different available cell is valid; later work observes the mutation and already-completed
work is not retroactively repeated.

The `Borrowed` result belongs to the public state-handle API. Widget phase signatures do not
propagate `StateAccessError`, so application closures passed to
`try_read`/`try_update`/`try_update_with` must not invoke `ContextFrame::render_ui` or another
top-level retained traversal entry point. This is an explicit application-reentrancy precondition,
not a frame lock: state access while a `ContextFrame` merely exists remains valid when the closure
returns before rendering begins. Framework-authorized recursion through
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
  unconditional pre-update repeat and then a post-update layout. The target requires only the
  pre-input and post-update tree layouts.
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
    +-- Some(WidgetStateHandle<CheckboxState>) --- Weak<RefCell<CheckboxState>>
    |
    +-- OwnedWidget
            +-- runtime: Box<dyn Widget>
            |      -> CheckboxWidget
            |           -> WidgetStateHandle<CheckboxState>   weak runtime access
            +-- state keep-alive: Rc<RefCell<CheckboxState>>   sole persistent strong owner

Node::widget(OwnedWidget)
    -> private RuntimeNodeId + NodeRuntime + owned Widget

Decoration::create(DecorationParameters)           hidden state
    |
    +-- None: Option<WidgetStateHandle<DecorationState>>
    |
    +-- OwnedWidget
            +-- runtime: Box<dyn Widget> -> DecorationWidget -> weak runtime access
            +-- state keep-alive: Rc<RefCell<DecorationState>>
                no application Weak returned

Column::create(ColumnParameters { children })      exposed container state
    |
    +-- Some(WidgetStateHandle<ColumnState>) ------ Weak<RefCell<ColumnState>>
    |
    +-- Node
         -> private NodeKind
              -> Container(OwnedContainer)       public Container: Widget, private enum variant
                   +-- runtime: Box<dyn Container> -> ColumnContainer -> weak runtime access
                   +-- state keep-alive: Rc<RefCell<ColumnState>>
                            -> Children(Vec<Node>)

Context
    -> WindowEntry
         +-- RootId / kind / z-order / just-opened / backend policy
         +-- WidgetStateHandle<RootState>       framework-only weak clone
         +-- WidgetTree
              -> Node root
                   -> Container(OwnedContainer: private RootChromeContainer)
                        +-- runtime weak RootState access
                        +-- state keep-alive: Rc<RefCell<RootState>>
                              -> Children(exactly one application Node)
```

### Ownership boundary and compile-safe migration

The architecture above is the only supported result. P1 establishes opaque
`OwnedWidget`/`OwnedContainer` insertion before the bulk built-in and container migrations. A
short-lived raw-box adapter may exist only inside one private, compile-safe P1 implementation step;
it is not a public API, plan milestone, example surface, or compatibility boundary and is removed by
the same item's completion. All subsequent checklist items and documentation use only the opaque
owned insertion model.

Application state access is Context-free:

```text
upgrade weak state cell
    -> failure: Dropped
    -> try_borrow / try_borrow_mut
    -> conflict: Borrowed
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
        input: Vec<UiInputEvent>,
    );
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>);

    fn effective_widget_opt(&self) -> WidgetOption;
    fn focus_policy(&self) -> FocusPolicy;
}
```

Use the actual current default implementations for `effective_widget_opt` and `focus_policy`; the
sketch omits their bodies only for brevity. `Widget::update` records any application-observable
event directly in its typed associated state and returns no generic summary. Do not rename this
trait, add state/mount identity methods to it, or create a second internal trait with the same
responsibility.

`WidgetNode` becomes a thin runtime owner of `OwnedWidget` plus optional custom-render metadata. It
invokes the owned runtime's delegated `Widget` methods directly. The current
`WidgetStateHandleDyn` redispatch layer and erased handle cloning disappear. Public
`Node::widget` constructs the `None` metadata path; public backend-typed
`Node::custom_render(widget, CustomRenderHandle<B>)` supplies the `Some` path while keeping
`CustomRenderKey` private.

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

    fn route_input(
        &mut self,
        ctx: &mut ContainerInputCtx<'_>,
        event: &UiInputEvent,
    ) -> ContainerInputResult {
        ctx.route_widget(
            event,
            self.effective_widget_opt(),
            self.focus_policy(),
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

    pub fn child_grid_span(
        &self,
        children: &Children,
        index: usize,
    ) -> Option<GridSpan>;

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
    pub fn route_widget(
        &mut self,
        event: &UiInputEvent,
        opt: WidgetOption,
        focus: FocusPolicy,
    ) -> ContainerInputResult;

    pub fn route_widget_in_rect(
        &mut self,
        event: &UiInputEvent,
        rect: Recti,
        opt: WidgetOption,
        focus: FocusPolicy,
    ) -> ContainerInputResult;
}
```

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
`Widget::measure`. `ContainerLayoutCtx::child_policy` and `child_grid_span` read the unique node's
private placement metadata without exposing the node. `layout_child` assigns one indexed child
rectangle in current-container content coordinates and returns the child's resulting content size,
or `None` for an invalid index. `set_children_viewport` installs a node-local visible viewport and
translation for descendants; the runtime intersects the viewport with the current content clip.
`set_content_size` and `set_child_overflow_propagation` update only the current node's derived layout
state. None of these methods changes topology or returns a child.

`ContainerInputCtx` exists because routing precedes the one per-frame `Widget::update` call and must
immediately decide deepest-first propagation and pointer capture. Its default `route_widget` path
performs the same generic geometry/options/focus routing as
a leaf over the current node's complete content rectangle and queues the accepted event for the
inherited `Widget::update` batch. `route_widget_in_rect` applies the same rules to a supplied
container-local sub-rectangle, intersected with the active clip; pointer coordinates remain in the
container's local content coordinate space. Disclosure uses it for the header, and scroll area uses
it for viewport/scrollbar hit regions. A special container may instead return `Ignored` at a scroll
boundary. Routing never applies the widget/container state change itself. The later
`Container::<Widget>::update` consumes the queued event and performs the mutation.

Removing this hook would leave nowhere to return `Ignored`/`Consumed`/`Captured` before update:
`Widget::update` is deliberately one-way and routing has already selected the recipient. Doing so
would require reordering updates, invoking them more than once, or adding routing outcomes to the
common Widget contract. None is part of this migration.
`ContainerInputCtx` and both child-visitor fields/constructors remain private and expose no IDs, raw
node storage, Context identity, raw child callback, or unrestricted tree mutation.

Export `Container`, marker `ContainerState`, `ChildrenVisitor`, `ChildrenVisitorMut`,
`ContainerLayoutCtx`, `ContainerInputCtx`, and `ContainerInputResult` from the public retained API
and its prelude. Do not add parallel container measure/update/paint contexts:
containers implement the final `Widget::measure`, `Widget::update`, `Widget::paint`,
`Widget::effective_widget_opt`, and `Widget::focus_policy` methods. `NodeBehavior` is deleted rather
than exported.

Private retained-tree traversal obtains `&dyn Widget`/`&mut dyn Widget` from either `NodeKind`
variant through `OwnedWidget`/`OwnedContainer` delegation and uses exactly one common Widget
dispatch path per requested measurement, update, or paint invocation. This is not a promise of one
`Widget::measure` call per frame: the required pre-input and post-update layouts, plus an explicitly
bounded scroll-constraint convergence, may issue multiple legitimate measurement requests. The
invariant forbids parallel leaf/container phase paths and duplicate remeasurement inside one
request. Traversal branches to `Container` only for layout, scoped child recursion,
container-owned descendant visibility, and special input routing. Generic `NodeRuntime` has no
visibility bit or hide/show API. There is no shared `NodeBehavior` trait or parallel container phase
adapter.

Built-in factories return the completed `Node` for ergonomics, while downstream code may construct
and insert its own implementation explicitly:

```rust
let (custom_state, custom_container) =
    CustomContainer::create(custom_parameters);
let custom_node = Node::container(custom_container);
```

### Final builder, owner, and stable state-handle contracts

`WidgetState`, `WidgetParameters`, `WidgetStateHandle`, `StateAccessError`, and the opaque owned
wrappers are the only construction and state-lifetime model implemented by P1 and used thereafter.
`WidgetBuilder` uses associated types; a bare `WidgetParameters` argument would be a trait object and
would lose the concrete parameter type. The associated `State` is the concrete state retained by
the opaque owner even when construction returns `None`. A genuinely stateless runtime may use `()`;
`None` is not represented by a special state type.

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StateAccessError {
    Dropped,
    Borrowed,
}

pub struct StateAccessFailure<I> {
    error: StateAccessError,
    input: I,
}

impl<I> StateAccessFailure<I> {
    pub fn error(&self) -> StateAccessError;
    pub fn into_input(self) -> I;
    pub fn into_parts(self) -> (StateAccessError, I);
}

impl<T: WidgetState> WidgetStateHandle<T> {
    pub(crate) fn from_owner(owner: &Rc<RefCell<T>>) -> Self;

    pub fn is_alive(&self) -> bool;

    pub fn try_read<R>(
        &self,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, StateAccessError>;

    pub fn try_update<R>(
        &self,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, StateAccessError>;

    pub fn try_update_with<I, R>(
        &self,
        input: I,
        f: impl FnOnce(&mut T, I) -> R,
    ) -> Result<R, StateAccessFailure<I>>;
}

// Private, method-free lifetime erasure only.
trait StateKeepAlive {}

impl<T: WidgetState> StateKeepAlive for RefCell<T> {}

pub struct OwnedWidget {
    runtime: Box<dyn Widget>,
    _state: Rc<dyn StateKeepAlive>,
}

pub struct OwnedContainer {
    runtime: Box<dyn Container>,
    _state: Rc<dyn StateKeepAlive>,
}

pub trait WidgetBuilder: Sized + 'static {
    type Parameters: WidgetParameters;
    type State: WidgetState;

    fn initialize(parameters: Self::Parameters) -> (Self::State, Self);
    fn build(self, state: WidgetStateHandle<Self::State>) -> Box<dyn Widget>;
    const EXPOSE_STATE: bool;
}

pub fn create_widget<B: WidgetBuilder>(
    parameters: B::Parameters,
) -> (Option<WidgetStateHandle<B::State>>, OwnedWidget);

pub trait ContainerBuilder: Sized + 'static {
    type Parameters: WidgetParameters;
    type State: ContainerState;

    fn initialize(parameters: Self::Parameters) -> (Self::State, Self);
    fn build(self, state: WidgetStateHandle<Self::State>) -> Box<dyn Container>;
    const EXPOSE_STATE: bool;
}

pub fn create_container<B: ContainerBuilder>(
    parameters: B::Parameters,
) -> (Option<WidgetStateHandle<B::State>>, OwnedContainer);
```

The non-overridable factories consume Parameters through `initialize`, allocate exactly one
`Rc<RefCell<State>>`, supply the runtime with a weak typed handle, and erase the sole persistent
strong owner into the returned opaque wrapper. `EXPOSE_STATE` controls only whether an additional
weak clone is returned to the application. `StateKeepAlive` has no methods, downcast, phase,
Context, or policy role. An incorrect custom builder can ignore its supplied weak handle, which
remains a documented safe-builder conformance error, but no raw boxed runtime can bypass the
framework-owned state allocation at insertion.

`WidgetStateHandle::from_owner` merely downgrades a reference to the owner; it does not clone or
return a strong pointer, and it is crate-private after P1 establishes the factory. Its manual
`Clone` implementation clones only `Weak` and deliberately imposes no `T: Clone` bound. Built-ins
provide inherent constructors that forward to `create_widget` or `create_container` so users do not
need fully qualified factory syntax.

`try_update_with` upgrades and successfully borrows the cell before moving `input` into `f`. A
`Dropped` or `Borrowed` failure therefore returns the untouched input in `StateAccessFailure` and
never invokes the closure. Ordinary `try_update` retains normal Rust closure semantics and cannot
recover a value moved into an uninvoked closure; examples must use `try_update_with` whenever a
unique `Node`, `OwnedWidget`, `OwnedContainer`, or another non-cloneable input must survive access
failure. Once `f` starts, it owns the input normally. A container `insert` that rejects an index
returns its uncommitted node inside the successful outer access result.

State exposure is chosen by the concrete widget or container implementation, not by its caller.
Each specific constructor has one documented outcome: it always returns `Some(handle)` when that
implementation exposes application-facing state, or always returns `None` when it keeps state
internal. Parameters do not contain a generic exposure flag, and discarding a returned handle does
not alter the constructor's policy.

This is identical for widgets and containers. `Checkbox::create` and public dynamic layout
containers return `Some`; a decoration widget or an internal fixed container returns `None`. Both
outcomes use the same strong state representation in the opaque owner. A builder that never exposes
state still names its actual state type; if it has no state data at all, it uses `()` and the wrapper
owns `Rc<RefCell<()>>` under the strict uniform-ownership contract.

### Final Checkbox construction example

The split is data-oriented rather than a rename of today's `Checkbox` struct:

```rust
pub struct Checkbox {
    label: String,
    opt: WidgetOption,
}

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

struct CheckboxWidget {
    builder: Checkbox,
    state: WidgetStateHandle<CheckboxState>,
}

impl Widget for CheckboxWidget {
    // Current Widget methods. measure/paint read state and update mutates it.
}

impl WidgetBuilder for Checkbox {
    type Parameters = CheckboxParameters;
    type State = CheckboxState;

    fn initialize(parameters: Self::Parameters) -> (Self::State, Self) {
        (
            CheckboxState {
                checked: parameters.checked,
            },
            Self {
                label: parameters.label,
                opt: parameters.opt,
            },
        )
    }

    fn build(self, state: WidgetStateHandle<Self::State>) -> Box<dyn Widget> {
        Box::new(CheckboxWidget {
            builder: self,
            state,
        })
    }

    const EXPOSE_STATE: bool = true;
}

impl Checkbox {
    pub fn create(
        parameters: CheckboxParameters,
    ) -> (Option<WidgetStateHandle<CheckboxState>>, OwnedWidget) {
        create_widget::<Self>(parameters)
    }
}
```

Construction and insertion are explicit:

```rust
let (checkbox_state, checkbox) = Checkbox::create(CheckboxParameters::new("Enabled", false));
let checkbox_state = checkbox_state.expect("Checkbox exposes CheckboxState");
let checkbox = Node::widget(checkbox);

column_state.try_update_with(checkbox, |column, checkbox| {
    column.push(checkbox);
})?;
checkbox_state.try_update(CheckboxState::check)?;
```

A widget with no application-visible state uses the same construction surface and withholds the
weak application handle. Its opaque wrapper still owns the state strongly. This genuinely stateless
example uses `()`; a hidden stateful widget instead uses its actual concrete state type:

```rust
struct DecorationWidget {
    state: WidgetStateHandle<()>,
    // Other runtime-only configuration and caches may remain ordinary fields.
}

impl WidgetBuilder for Decoration {
    type Parameters = DecorationParameters;
    type State = ();

    fn initialize(parameters: Self::Parameters) -> (Self::State, Self) {
        ((), Self::new(parameters))
    }

    fn build(self, state: WidgetStateHandle<Self::State>) -> Box<dyn Widget> {
        Box::new(DecorationWidget::new(self, state))
    }

    const EXPOSE_STATE: bool = false;
}
```

An initialization parameter may seed state, configure the runtime object, or both. Anything the
application must mutate later belongs in the state type. Stable runtime configuration needed by
`Widget::widget_opt`, such as the base `WidgetOption`, remains directly owned by the runtime widget;
`effective_widget_opt` may derive a copied dynamic override from state when required.

### State ownership and access

Every `OwnedWidget`/`OwnedContainer` owns one persistent strong `Rc<RefCell<T>>` for its
`T: WidgetState`; the runtime consumes a factory-supplied weak handle. Any application
`WidgetStateHandle<T>` clones are weak and non-owning. Exposure changes only whether construction
returns an application weak handle; it never changes whether the retained value keeps the state
cell alive.

An access operation temporarily upgrades the weak pointer. Consequently:

- dropping an uninserted owned wrapper invalidates any exposed state handle;
- removing a node drops its retained owner and invalidates handles once any already-running access closure
  releases its temporary strong upgrade;
- handles do not know or care which Context or container owns the widget;
- moving the unique `Node` before insertion does not affect any exposed state handle;
- no frame state is checked;
- same-cell reentrancy through another checked handle access returns `Borrowed`;
- cross-cell access succeeds when the other cell is available.

The temporary-upgrade qualification is intentional. Preventing an active access closure from
briefly keeping the cell allocation alive would require another global or per-node lifecycle lock.
The observable contract is instead that no new successful access begins after both the node owner
and all already-active access operations are gone.

### Application state-access closures are not top-level rendering callbacks

Retained traversal is Context-local and never crosses Context boundaries. Entering a top-level
retained render for the same Context that owns a borrowed state cell from inside application
`WidgetStateHandle::try_read`/`try_update`/`try_update_with` is explicitly unsupported. The runtime
cannot turn a state borrow conflict into `StateAccessError::Borrowed` because the existing
`Widget::measure`, `update`, and `paint` return types do not carry that error. Skipping a borrowed
widget or painting stale data is also not an acceptable fallback.

This sequence is supported because the state borrow ends before traversal starts:

```rust
let frame = ctx.frame(frame_info);

checkbox_state.try_update(CheckboxState::check)?;

frame.render_ui()?;
```

This sequence violates the API precondition because `render_ui` runs while the mutable state borrow
is still held by the closure:

```rust
checkbox_state.try_update(|checkbox| {
    checkbox.check();
    ctx.frame(frame_info).render_ui() // unsupported reentrant rendering
})?;
```

Do not add a FrameGate, Context token, state-access depth counter, or global “currently borrowed”
flag to detect this condition. Built-in runtimes should use a small internal state-borrow helper that
panics with a precise diagnostic if an unsupported reentrant render reaches a borrowed state cell;
external custom widgets and containers are bound by the same documented precondition. Ordinary
handle-to-handle borrow conflicts continue to return `StateAccessError::Borrowed`.

This prohibition does not apply to framework-authorized recursive traversal. A container runtime
may retain its current checked state borrow while calling `Children::measure_child`,
`ContainerLayoutCtx::layout_child`, or supplying that same state's collection through an active
opaque visitor. Those capabilities are constructed only by the framework, recurse into distinct
owned child state cells, and are required by the public custom-container contract. Reentering
`ContextFrame::render_ui` or another root traversal from those methods remains unsupported.

```rust
fn runtime_read<T>(cell: &RefCell<T>) -> Ref<'_, T> {
    cell.try_borrow().expect(
        "widget state is already mutably borrowed; rendering from a state-access closure is unsupported",
    )
}

fn runtime_update<T>(cell: &RefCell<T>) -> RefMut<'_, T> {
    cell.try_borrow_mut().expect(
        "widget state is already borrowed; rendering from a state-access closure is unsupported",
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
measurement trait; it delegates to the child's private `Node` measurement path and exposes no node
ID or storage. Direct node iteration is crate-private and double-ended, so update/layout/paint can
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
phase/traversal order determines whether its old or new children participate in the current frame.
There is no snapshot or rollback: each phase renders or processes the state it observes when it
reaches that node. Work already completed in the frame is not repeated except for the scheduled
post-update layout. Regardless of same-frame observation order, the next ordinary frame must be
fully stable against the successful mutation.

Public factories for row, column, grid, stack, disclosure, and scroll area return
`(Option<WidgetStateHandle<SpecificContainerState>>, Node)`. Each built-in factory constructs its
owned container runtime and immediately wraps it with `Node::container`, so downstream callers
receive the ready node instead of performing a redundant wrapping step. Exposure is a fixed,
documented
property of each concrete constructor, just as it is for leaf widgets; callers do not select it in
Parameters. The public dynamic layout primitives listed above return `Some`, while an internal
fixed-composition container may define a constructor that always returns `None`. Either way, the
opaque wrapper strongly owns the container state cell containing its children. External custom
leaf widgets remain supported through `Widget`; external custom container runtimes are supported
through public `ContainerBuilder`, `create_container`, and `Node::container(OwnedContainer)`.

```rust
impl ContainerBuilder for Column {
    type Parameters = ColumnParameters;
    type State = ColumnState;

    fn initialize(parameters: Self::Parameters) -> (Self::State, Self) {
        (
            ColumnState {
                children: parameters.children,
            },
            Self,
        )
    }

    fn build(self, state: WidgetStateHandle<Self::State>) -> Box<dyn Container> {
        Box::new(ColumnContainer::new(state))
    }

    const EXPOSE_STATE: bool = true;
}

impl Column {
    pub fn create(
        parameters: ColumnParameters,
    ) -> (Option<WidgetStateHandle<ColumnState>>, Node) {
        let (state, container) = create_container::<Self>(parameters);
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
owning `WidgetTree`, not `ScrollAreaState`: target sanitization releases capture for a disabled area
before another event can be routed to it. Derived layout hides both bars. `set_offset` clamps
negative components immediately and clamps the upper bound during the next layout, when current
content and viewport extents are known.

Row width entries correspond to children by index; a missing entry is `Auto` and excess entries are
ignored. An empty grid column list means one `Auto` column. Explicit extra grid column/row tracks
remain part of grid geometry even when currently empty. Changing grid columns reflows row-major
placement without changing child IDs or state cells.

Layout precedence is single and directional:

1. the container resolves a slot or shared grid tracks from its state configuration and child
   intrinsic measurements;
2. a grid applies the child's pre-insertion `GridSpan` to form the offered slot;
3. `ContainerLayoutCtx::layout_child` applies the child's pre-insertion `Policy` exactly once to
   that slot;
4. `Auto` fills the offered slot during allocation, while a non-`Auto` policy has final precedence
   for that child's allocation without rewriting shared row/grid track definitions;
5. a smaller child allocation leaves trailing slot space, while a larger allocation participates in
   the explicit overflow contract.

Containers may inspect `child_policy` for measurement and slot planning, but must not resolve it and
then let generic traversal apply it a second time. There is no mounted child-policy/span setter;
changing either requires constructing and inserting a replacement node. State mutations made before
pre-input layout affect that frame; mutations during update affect post-update layout; mutations
after paint become fully visible on the next frame.

### Owning node and internal identity

The public name `Node` is reserved for the unique, opaque owner placed in roots and `Children`.
The existing header/tree widget with that name and its `NodeStateValue` enum are not aliased or
renamed as public compatibility types; their supported behavior is absorbed by `Disclosure` below.

The final payload stores only opaque owners:

```rust
pub struct Node {
    id: RuntimeNodeId,
    runtime: NodeRuntime,
    kind: NodeKind,
}

enum NodeKind {
    Widget(WidgetNode),
    Container(OwnedContainer),
}

struct WidgetNode {
    widget: OwnedWidget,
    custom_render: Option<CustomRenderKey>,
}

struct NodeRuntime {
    // Derived layout and transient hover/focus/capture-related flags, but no visibility bit.
    policy: Policy,
    grid_span: GridSpan,
    /* private derived/transient fields */
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct RuntimeNodeId(NonZeroU64);
```

Leaf construction has two paths. Both keep `CustomRenderKey` private:

```rust
impl Node {
    pub fn widget(widget: OwnedWidget) -> Self {
        Self::widget_with_custom_render(widget, None)
    }

    pub fn custom_render<B: RendererBackend>(
        widget: OwnedWidget,
        renderer: CustomRenderHandle<B>,
    ) -> Self {
        Self::widget_with_custom_render(widget, Some(renderer.key))
    }

    fn widget_with_custom_render(
        widget: OwnedWidget,
        custom_render: Option<CustomRenderKey>,
    ) -> Self {
        Self::from_kind(NodeKind::Widget(WidgetNode {
            widget,
            custom_render,
        }))
    }

    pub fn container(container: OwnedContainer) -> Self {
        Self::from_kind(NodeKind::Container(container))
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.runtime.policy = policy;
        self
    }

    pub fn with_grid_span(mut self, columns: usize, rows: usize) -> Self {
        self.runtime.grid_span = GridSpan::new(columns, rows);
        self
    }
}
```

`Node::from_kind` initializes `Policy::auto()` and `GridSpan::ONE`. `with_policy` and
`with_grid_span` consume and return the still-unmounted unique node; zero spans clamp through the
existing `GridSpan::new` rule. There are no mounted-node placement setters. Every built-in
container follows the single-application policy precedence above; grid additionally consults
`child_grid_span`, while non-grid parents ignore the span.

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
and `Node::custom_render` create leaf variants from `OwnedWidget`. Public `Node::container` accepts
downstream `OwnedContainer`; no raw-box overload exists. Built-in container factories call the
applicable constructor internally and return the finished `Node` for convenience. Every path
assigns a never-reused runtime
ID from one process-wide monotonic allocator. Relaxed atomic allocation is sufficient because the
value is only uniqueness metadata, not synchronization.

IDs are created with nodes, not assigned by Context and not stored in state handles. They support
focus, capture, routed input, liveness validation, and cleanup only. The application never
receives or reconstructs them. Because IDs are globally unique and never reused, a stale internal
target cannot alias a node in another Context; no Context token or mount metadata is required.

`Node` is not `Clone`. Placement policy and grid span are configured with the consuming builder
methods above before insertion. A successful insertion consumes it. Generic node visibility is
absent: there is no `visible` field, `set_visible`, `show`, or `hide` operation on `Node` or
`NodeRuntime`.

### Fixed built-in state exposure and compatibility mapping

Exposure does not remain a P1 implementation choice. Every built-in has this fixed constructor
outcome and mounted application surface:

| Built-in | Exposure | Mounted application state/events |
|---|---|---|
| `Checkbox` | `Some` | checked value and consumable change observation |
| `Button` | `Some` | consumable submissions |
| `ListItem` | `Some` | mutable label and consumable submissions |
| `ListBox` | `Some` | consumable submissions |
| `Combo` | `Some` | selected/open state, current label/anchor, selection operations, and consumable change/submit events |
| `TextBlock` | `Some` | mutable text |
| `ColorSwatch` | `Some` | mutable fill and label |
| `Slider` | `Some` | value/editing state and consumable changes |
| `Number` | `Some` | value/editing state and consumable changes |
| `Textbox` | `Some` | text, cursor/selection, change/submit events, and queued focus command |
| `TextArea` | `Some` | text, cursor/selection, scroll, and change/submit events |
| `Custom` | `None` | no mounted application state; it uses `State = ()` |
| old `widgets::Node`/`NodeStateValue` | retired | replaced by exposed `DisclosureState` |

Every public dynamic built-in container (`Column`, `Row`, `Grid`, `Stack`, `Disclosure`, and
`ScrollArea`) returns `Some`; only explicitly fixed/internal container implementations return
`None`. The outcome never depends on parameters or whether a caller plans to retain the handle.

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
values and commands live in the widget-specific state:

- `CheckboxState::checked` exposes the persistent value;
- `SliderState::value` and `set_value` expose numeric state;
- `TextboxState` owns text, cursor, selection, and a focus-request command;
- `ComboState` owns open/selected state;
- widget states expose the exact consumable change/submit operations below;
- custom widgets define their own state and observation methods.

Each consumable event kind is a private saturating `u32` pending count. Public
`take_changed() -> bool` or `take_submitted() -> bool` consumes exactly one occurrence and returns
`false` only when none is pending. Events therefore persist across frames and hidden periods until
consumed. A built-in records at most one occurrence of each semantic event kind per `Widget::update`
invocation: multiple low-level edits in one routed batch are one `changed` occurrence, while
occurrences from separate updates accumulate instead of collapsing. The fixed event API and
recording points are:

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

Programmatic value setters (`set_checked`, `set_value`, `set_text`, cursor/selection setters, and
open/close/select operations) do not record interaction events. This matches current behavior and
prevents feedback loops. `ComboState::update_items` clamping remains the one explicit compatibility
exception because current Combo reports `CHANGE` for that normalization; direct `select` remains
silent because the caller performs it after consuming the submitted popup-item event. `ACTIVE` is
runtime interaction/paint state for ordinary leaf widgets, not an application event, and is not
copied into their typed state. Root chrome separately exposes its persistent moving/resizing mode
through `RootState::is_active` because that mode is itself application-observable window state; it
is still not a consumable event.

`Widget::update` returns `()` and records typed events at the interaction decision point. Focus is
changed through `WidgetUpdateCtx`; input consumption and capture come from
`ContainerInputResult`/private routing state. Root chrome records its values and events in
`RootState` by exactly the same mechanism. Do not add a replacement result store, generic event
summary, or root-only side channel.

Programmatic widget commands also use typed state. For example, `TextboxState::request_focus`
records a request that persists while the textbox is unable to take focus, including while its root
is hidden, an ancestor gates traversal, its effective options make it non-interactive, or current
cross-root/modal policy makes it ineligible. On an eligible update, `WidgetUpdateCtx::set_focus`
reports whether focus was actually assigned; only a successful assignment clears the queued
request. This removes public targeted node operations and their Context-validation problem.

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
  -> WindowEntry {
       id: RootId,
       kind / z-order / just-opened / backend viewport data,
       root_state: WidgetStateHandle<RootState>, // framework-only weak clone
       tree: WidgetTree,
     }
       -> Node::Container(private RootChromeContainer runtime)
            -> OwnedContainer                 // sole persistent strong RootState owner
                 -> RootState.children        // exactly one application Node
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
    type State = RootState;

    fn initialize(parameters: RootChromeParameters) -> (RootState, Self) {
        let children = core::iter::once(parameters.content).collect();
        (
            RootState::new(
                parameters.name,
                parameters.options,
                parameters.rect,
                parameters.visible,
                children,
            ),
            Self,
        )
    }

    fn build(self, state: WidgetStateHandle<RootState>) -> Box<dyn Container> {
        Box::new(RootChromeContainer::new(state))
    }

    const EXPOSE_STATE: bool = true;
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
handle reports `StateAccessError::Dropped`. Root mutation remains on `Context` because it also
coordinates z-order, front-root selection, backend viewport state, and transient input cleanup:

- `set_root_rect(root, rect)` and `set_root_size(root, size)` mutate `RootState` through
  `WindowEntry.root_state` and emit no typed event. They preserve an in-progress move/resize, with
  the next captured delta applied from the new programmatic rectangle. If `AUTO_SIZE` is enabled,
  the next pre-input measure replaces width/height but preserves the programmatic origin;
- `set_root_options(root, options)` mutates the same authoritative state silently. Enabling
  `NO_TITLE` clears a current move; enabling `NO_RESIZE` or `AUTO_SIZE` clears a current resize; the
  matching tree capture is released before another pointer event routes;
- `set_root_visible(root, false)` keeps the tree alive, silently clears moving/resizing state plus
  invalid focus/hover/capture/routed targets, and emits no submission;
- `set_root_visible(root, true)` raises the root. A window/dialog keeps its rectangle. Before a
  hidden popup is shown, any other visible popup in that Context is silently hidden and sanitized
  without recording a submission. The new popup is repositioned at the current pointer with a
  `1 x 1` seed rectangle, is marked `just_opened`, and receives its content-derived auto-size before
  input. Showing keeps application state and pending events but does not restore cleared transient
  targets. Switching popups is atomic: Context resolves both entries and obtains the required
  checked state borrows before changing either; if either state is borrowed, the operation returns
  `RootMutationError::Borrowed` and leaves both visibility states unchanged;
- `bring_root_to_front(root)` changes only z-order and emits no typed event;
- mutation by an unknown/destroyed ID returns `RootMutationError::UnknownRoot`. Trying a Context
  mutation while a state-handle closure currently borrows that same `RootState` returns
  `RootMutationError::Borrowed`; callers finish the closure and retry. A weak-upgrade failure for an
  extant `WindowEntry` is an internal ownership invariant panic. `bring_root_to_front` and
  `destroy_root` need no state borrow and return `false` for an unknown/destroyed ID.

Chrome events use the same private saturating `u32` counters and one-occurrence `take_*` contract as
built-in widget events:

- `take_submitted` records a left-button press in the title-close rectangle or the first pointer-
  button press outside an eligible popup after its `just_opened` suppression. Either action first
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
  clamp. One update increments `pending_changes` only if its final rectangle differs from its entry
  rectangle, regardless of how many low-level move events were batched;
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
| `Container::route_input` | on left press, give close/title/resize regions precedence, queue the chrome event and return `Consumed` for close or `Captured` for title/resize; consume other pointer presses over chrome without starting an action; return `Ignored` for body events so ordinary child routing descends into the application node |
| `Widget::update` | consume the routed chrome events, hide/submit on close, enter or leave moving/resizing state, mutate the rectangle during captured drag, and record a change only for an actual user-driven rectangle change |
| `Widget::paint` | paint the window/frame background underlay before descendants from the shared geometry; ordinary traversal paints the application child in the body clip |
| `Container::children_visible` | return the current `RootState::is_visible` value, allowing a close handled by the parent update to suppress application-child traversal in that same frame |

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

Before pre-input layout, a visible `AUTO_SIZE` root is measured through the internal chrome node with explicit
constraints and its width/height are silently normalized in `RootState`; this is not a user change
event. The frame then lays out with that authoritative rectangle, routes and updates chrome through
the tree, and uses post-update layout for any user-mutated rectangle. If the container's own update
hides the root on close, traversal rechecks root visibility before descending, so application-child
update/paint and unnecessary post-layout are skipped for that root.

Generic `WidgetTree` capture owns title drag and resizing just as it owns container capture. The
captured private root runtime ID receives matching left-button drag/release events outside its
current bounds without a second hit test; release or capture sanitization clears both tree capture
and `RootState.interaction`. Sanitization also clears either half if the other can no longer be valid.
The window manager must not keep a second chrome-capture flag or independently reinterpret raw
pointer input. Chrome hit regions outrank the body only within the front eligible root, after
cross-root/modal gating by the window manager.

Popup outside-click is necessarily detected at that cross-root boundary rather than by ordinary
inside-tree hit routing. The pointer press that opens/shows a popup is ignored for dismissal through
`WindowEntry.just_opened`; that flag clears after the popup's first eligible frame. On a later press
outside it, before routing that press elsewhere, the window manager upgrades its private weak
`RootState` handle and invokes a framework-private dismissal operation that clears active chrome
state/capture, hides the popup, increments the same pending-submission counter, and sanitizes that
tree's transient targets. This is the sole special boundary hook; it does not create a second result
or event mechanism. Failure to upgrade means the entry is internally inconsistent and is an
invariant panic, not a silently missing application event.

At most one popup is visible in a Context. Showing popup B while popup A is visible silently hides A,
clears A's transient targets and capture, preserves A's state and pending events, and records no
submission because the transition is programmatic. B then receives the ordinary pointer placement,
z-order, and `just_opened` behavior. Nested popup ownership and dismissal form a separate future
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
the root and descendant weak state handles reports `Dropped` after all such active access closures
release their temporary upgrades. Destruction never returns the `Node`, tree, pending events, or
state. Hiding is distinct: title close, popup dismissal, and
`set_root_visible(false)` retain both the tree and weak-handle liveness, and application code may
later destroy the root for permanent removal.

### Visibility boundaries

There is no generic node-visibility feature. Root visibility is the `RootState` value coordinated
through Context/`WindowEntry` policy.
`Container::children_visible` is a narrower traversal gate used by containers such as Disclosure:
when it returns `false`, descendants remain owned and their weak handles remain live, but they make
no measurement/layout contribution and receive no input, update, paint, or custom-render call.
Focus, hover, capture, and queued routed events targeting the hidden descendant subtree are cleared
at the next safe sanitization point and are not automatically restored when traversal resumes. The
container itself still measures, updates, and paints. Its own `Widget::measure`/`layout`
implementation is responsible for omitting hidden descendants consistently with the traversal gate.

### Runtime lifecycle after direct topology mutation

Focus, hover, pointer capture, and routed events remain private `RuntimeNodeId` values in the owning
`WidgetTree`. Direct state mutation means removal does not call Context cleanup synchronously.
Instead, every target use is liveness-checked against the retained tree, and the normal frame
boundary sanitizes targets that no longer exist before routing new input.

Required rules:

- a missing focus/capture/routed target is cleared and never redirected;
- IDs are never reused, so stale state cannot target a replacement node;
- there are no public per-node or per-root result entries to preserve or transfer; all application
  observation lives in typed state;
- removal drops the node immediately even if runtime target cleanup occurs at the next safe tree
  boundary;
- mutation of the child collection currently borrowed by traversal returns `Borrowed`;
- cross-subtree topology mutation is observed according to deterministic traversal order.

This is safe without a registry or Context callback because runtime targets are scalar IDs, not
pointers. Focused tests must pin removal-before-frame, removal-during-update in another subtree,
capture removal, and replacement behavior.

### Authoritative ownership

| Datum | Authoritative owner |
|---|---|
| Common measure/update/paint/options/focus behavior | concrete runtime implementing `Widget`, including the runtime inside each `OwnedContainer` through `Container: Widget` |
| Container-only child/layout/special-input behavior | concrete runtime inside `OwnedContainer` |
| Widget/container state lifetime | `OwnedWidget`/`OwnedContainer` strong `Rc<RefCell<T>>` keep-alive |
| Hidden runtime-only caches/configuration | ordinary concrete boxed widget/container fields |
| Optional application state capability | `Some(WidgetStateHandle<T>)`; `None` means unexposed |
| Construction input and exposure policy | specific `Parameters`; each concrete widget/container constructor has one fixed documented exposure outcome |
| Container children | `Children` inside the concrete container state cell |
| Node placement, derived layout, and transient interaction flags | `NodeRuntime`; no generic visibility field |
| Descendant traversal visibility | concrete `Container::children_visible`, principally `Disclosure` |
| Focus/hover/capture/routed input | owning `WidgetTree`, keyed by private `RuntimeNodeId` |
| Widget action/value observation | widget-specific `WidgetState` |
| Root application content | private `RootState.children`, containing exactly one application `Node` and exposing no public topology mutation |
| Root geometry/visibility/chrome interaction/events | `RootState`, strongly owned by the private `RootChromeContainer` and exposed weakly through `RootHandle` |
| Root lifecycle identity | never-reused `RootId` inside `RootHandle`; `Context`/`WindowEntry` changes lifetime only by creation and `destroy_root` |
| Root cross-window policy/z-order/backend viewport | `WindowEntry`/window manager, using its framework-internal weak `RootState` handle |
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
17. Treating opaque owner insertion as late release hardening would force a second construction
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
   `update` method returns `()` rather than a generic leaf result.
2. Public `Container: Widget` adds only opaque child visitation, layout, descendant visibility, and
   special routed-input behavior and is implementable downstream; it does not redeclare
   measure/update/paint or lend raw child collections to ordinary callers.
3. `WidgetState` is data only and `ContainerState: WidgetState` is marker-only; implementing either
   does not implement runtime phases or grant generic child access.
4. `WidgetBuilder` associates one concrete parameter type and one concrete state type, chooses
   whether to expose a weak handle, and constructs one boxed runtime widget without exposing its
   concrete implementation type.
5. Every `OwnedWidget`/`OwnedContainer` owns the only persistent strong `Rc<RefCell<T>>` for its
   `T: WidgetState`; no raw runtime insertion or direct concrete-runtime owner is a checklist
   contract. `Some` and `None` differ only in weak application-handle exposure.
6. When present, application state handles are typed, weak, cloneable without `T: Clone`, and contain
   no Context or node identity.
7. State access uses non-escaping closures and checked per-cell borrows; `try_update_with` preserves
   and returns owned input if upgrade/borrow fails before closure invocation.
8. `ContextFrame` existence has no effect on state or container-state access, but a state-access
   closure must finish before retained rendering/traversal begins in the same Context; retained
   traversal never crosses Context boundaries.
9. A same-cell conflict between checked handle accesses returns `Borrowed`; runtime phase access
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
    APIs; attached `Policy` and `GridSpan` never change.
25. Raw input is normalized once. Ordinary retained interaction derives only from routed events;
    cross-root popup outside dismissal is the sole boundary exception and consumes that same ordered
    pointer stream before routing it onward. Each normal frame performs pre-input and post-update
    tree layouts only, and intrinsic layout uses explicit bounded/unbounded constraints plus shared
    axis allocation.
26. Root chrome uses the same routed-input, update, capture, typed-state, and pending-event machinery
    as other containers. Cross-root popup dismissal is the only private boundary injection and
    records into that same `RootState`.
27. P1 establishes opaque `OwnedWidget`/`OwnedContainer` insertion before bulk migration; raw-box
    insertion is never a public checklist or released compatibility boundary.

## Priority and completion rules

- **P0 — Baseline and wanted-behavior freeze:** characterize supported current behavior, freeze the
  currently wanted state/builder/container/event/root contracts, and route any later preservation
  trade-off through explicit change control.
- **P1 — Ownership:** establish the final weak-handle/opaque-owner factories and direct
  `OwnedWidget`/`OwnedContainer` nodes before bulk migration or projection deletion.
- **P2 — Runtime mechanics:** migrate layouts, disclosure, scroll, routing, and cleanup onto direct
  state-owned topology.
- **P3 — Application boundary:** migrate roots, examples, custom widgets, and the file dialog to
  constructor-returned optional state handles and owned nodes.
- **P4 — Correctness:** pin dynamic-mutation semantics and repair scroll, intrinsic measurement,
  axis allocation, window-boundary sizing, and transform edge cases against the simplified
  representation.
- **P5 — Cleanup and measurement:** remove all obsolete adapters/identity/reconciliation and optimize
  only measured hot paths.
- **Release validation:** verify the P1 opaque ownership boundary, rerun the complete validation
  matrix, and only then permit external release.

An item is complete only when production code, focused tests, affected examples, public docs, and
named obsolete-code removal land together. Temporary adapters must be crate-private and have a named
deletion point inside the same P1 owner item. P0-P5 remain internal integration milestones until the
final validation pass; do not merge/tag/release an incomplete migration or leave two externally
supported widget construction or ownership models.

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
  for one widget, a 100-node tree, a scroll area, and idle/refresh file-dialog evaluation.

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

  **Implementation owner: P1.0, P1.1, P1.3, and P2.3**

  **Wanted behavior and contract**

  Keep `Widget` as the sole common runtime phase trait but change `Widget::update` to return `()`.
  Add marker `WidgetState` and `WidgetParameters`,
  associated-type `WidgetBuilder`, the non-overridable `create_widget` factory, and the
  exposed/hidden construction shapes shown above. Construction returns
  `Option<WidgetStateHandle<Self::State>>` plus opaque `OwnedWidget` without exposing the concrete
  runtime type. Every returned owner retains the sole persistent strong
  `Rc<RefCell<Self::State>>`, including on the `None` path, while the runtime receives the matching
  weak typed handle. `WidgetState` has no
  measure/update/paint behavior and is never used as a substitute dispatch trait. Do not add
  identity, mount, child, or Context methods to `Widget`.

  Apply the same construction boundary to containers through associated-type `ContainerBuilder`,
  the non-overridable `create_container` factory, and opaque `OwnedContainer`. Construction returns
  `Option<WidgetStateHandle<Self::State>>` plus `OwnedContainer`; the opaque owner retains the sole
  persistent strong state cell while the concrete runtime receives only the matching weak typed
  handle. `ContainerState: WidgetState` remains marker-only and does not acquire child access or
  runtime behavior.

  Specify the replacement of the current crate-private `Container: NodeBehavior` coupling with the
  final public object-safe
  `Container: Widget`. Keep `ContainerState` as the separate marker-only data trait. Add only the
  container-specific opaque child visitors, exact indexed layout services, descendant visibility,
  and full/sub-rectangle routed-input hooks shown above; do not
  duplicate `Widget` measurement, update, paint, option, or focus methods. Public `Container` must
  be implementable by downstream crates without exposing private tree machinery; P2.3 deletes
  `NodeBehavior` after direct `NodeKind`/`Widget` dispatch lands. This P0 item freezes those
  signatures but does not publish an unusable partial container surface: public `Container`,
  `ContainerState`, `Children`, visitors, scoped contexts/results, owning `Node`, and downstream
  container compile tests land atomically in the P1.3/P2.1 compile-safe batch.

  **Acceptance tests**

  - Compile-time signature tests pin the public `Widget` methods/defaults and prove `update` returns
    `()` with no generic result summary.
  - `CheckboxState` can be mutated without exposing measure/update/paint.
  - `CheckboxWidget` dispatches through `OwnedWidget` with no erased state-handle trait.
  - A hidden-state widget returns `None` while its opaque owner still retains the strong state `Rc`;
    no weak application capability is returned.
  - An external custom leaf implements `Widget`, `WidgetState`, `WidgetParameters`, and
    `WidgetBuilder` without private APIs.
  - The P1.3/P2.1 batch's external custom-container test implements `Widget`, `Container`, and marker
    `ContainerState`; proves the returned `OwnedContainer` retains the sole persistent strong state
    cell while the concrete runtime receives its weak handle; constructs `Children` through
    `new`/`FromIterator`; supplies the same authoritative collection exactly once through both
    opaque visitors; measures/layouts it through the exact public scoped operations; constructs it
    through `create_container`; and enters the tree through public
    `Node::container(OwnedContainer)` without private APIs.
  - Container conformance tests panic with the specified diagnostics when either opaque visitor
    receives zero or multiple `Children` submissions; downstream documentation states the
    same-authoritative-collection obligation that safe Rust cannot enforce across the two methods.
  - Ordinary downstream code cannot construct `ChildrenVisitor`/`ChildrenVisitorMut`, install a raw
    child callback, or obtain a `Children` borrow from `ContainerState`.
  - A compile-time supertrait check proves every `Container` is a `Widget`; `Container` declares no
    second measure/update/paint methods.

  **Frozen contract evidence (2026-07-29)**

  The normative target signatures and ownership diagrams above now define one complete separation
  boundary for both leaves and containers: concrete runtime objects implement `Widget` (and, for
  containers, `Container`), application data implements marker `WidgetState`/`ContainerState`, and
  only opaque owners retain persistent strong state cells. Optional application capabilities and
  concrete runtimes receive weak typed handles; no state trait becomes a phase-dispatch adapter.

  Repository inspection confirms that the current implementation still returns `ResourceState`
  from `Widget::update`, stores application/runtime state together, clones strong `WidgetHandle`
  values into `WidgetStateHandleDyn`, and couples crate-private `Container` to `NodeBehavior` with
  raw child-slice access. These are recorded migration gaps rather than preserved behavior. P0.1
  intentionally changes no production API: its compile-time and runtime acceptance criteria are
  protected specifications that become executable and green in their named P1.0/P1.1/P1.3/P2.3
  owner batches, with the public container surface landing atomically rather than partially.

- [x] **P0.2 — Freeze the optional weak-exposure contract**

  **Problem**

  Strong public handles outlive topology. Every runtime must own its state strongly, but that does
  not mean every widget/container should expose a state capability to the application. The previous
  replacement also added Context metadata even though an exposed handle needs only liveness and
  borrow checking.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.0 and P1.3**

  **Settled decision and rationale**

  `create_widget`/`create_container` returns `Some(WidgetStateHandle<T>)` when the concrete builder's
  fixed `EXPOSE_STATE` policy permits application access and `None` when it keeps that capability
  private. The opaque owned wrapper retains the strong `Rc<RefCell<T>>` in both exposure cases.
  Present handles are weak, so node lifetime remains authoritative. Neither the strong owner nor
  the weak handle is a Context capability or a lock.

  Mandatory exposure was rejected because it leaks implementation-only state or creates a
  meaningless public capability. Strong application handles were rejected because they keep
  removed widget state alive and allow one state allocation to outlive or back multiple nodes.

  **Wanted behavior and contract**

  Implement `WidgetStateHandle<T>`, `StateAccessError`, input-preserving
  `StateAccessFailure<I>`/`try_update_with`, the associated builders, and the required
  optional-returning framework factories. Each factory creates one `Rc<RefCell<B::State>>` for its
  concrete builder `B`, supplies the runtime a weak handle to it, and moves the strong owner into
  `OwnedWidget` or `OwnedContainer`. `WidgetStateHandle::from_owner` is crate-private; the
  application receives a weak clone only for the `Some` path. `is_alive` reports whether that weak
  cell can still be upgraded without borrowing its contents; an already-active access operation's
  temporary upgrade therefore keeps it true after the opaque owner is dropped and until that
  operation returns. Each concrete widget or container constructor makes one fixed, documented
  exposure choice; Parameters do not contain a generic exposure selector and exposure cannot
  change after construction. Handles contain only `Weak<RefCell<T>>`; `try_read` and `try_update`
  use checked borrows, return the closure result, and expose only `Dropped`/`Borrowed`; remove
  `replace`. Document that access closures may not invoke retained traversal/rendering, and use the
  shared internal runtime-borrow diagnostic for built-ins instead of adding a frame/state-access
  gate. The application top-level-render prohibition explicitly exempts framework-created child
  measurement/layout/visitor recursion.

  **Acceptance tests**

  - Checkbox returns `Some`; cloning that handle does not increase strong count.
  - A compile-time test clones `WidgetStateHandle<NonCloneState>` and proves the handle's `Clone`
    implementation has no `T: Clone` bound.
  - Dropping Checkbox's returned `OwnedWidget` makes an idle handle report `Dropped`.
  - `is_alive` does not borrow state: it remains true during an existing read or mutable access,
    remains true after that closure drops the opaque owner because the active operation holds a
    temporary upgrade, and becomes false when the final owner/active upgrade is gone.
  - `try_read` and `try_update` return their closure results; an expired cell returns `Dropped`, and
    an unavailable live cell returns `Borrowed` without invoking the closure.
  - `StateAccessFailure::error`, `into_input`, and `into_parts` report the original error and return
    the exact uncommitted input without cloning or substitution.
  - `Custom` and a crate-private fixed-container proof implementation return `None`, but each
    returned opaque wrapper owns its strong state `Rc`.
  - Classification tests pin the complete fixed built-in outcome: `Checkbox`, `Button`, `ListItem`,
    `ListBox`, `Combo`, `TextBlock`, `ColorSwatch`, `Slider`, `Number`, `Textbox`, and `TextArea`
    return `Some`; `Custom` returns `None`; and every public dynamic container (`Column`, `Row`,
    `Grid`, `Stack`, `Disclosure`, and `ScrollArea`) returns `Some`. The old header/tree `Node` is
    retired rather than assigned an exposure outcome.
  - Repeated calls to the same constructor have the same exposure outcome regardless of parameter
    values or how the caller uses the result.
  - No public Parameters type contains a generic exposure flag, and no exposure-selector type or
    post-construction exposure operation exists.
  - Same-cell reentrancy returns `Borrowed`; cross-cell access succeeds.
  - `try_update_with(node, ...)` returns that exact unmounted node on both `Dropped` and `Borrowed`
    without invoking the closure; successful access moves it once, and an invalid `insert` returns
    it from the inner operation.
  - State access behaves identically before, during, and after a `ContextFrame` when borrow state is
    identical and the access closure completes before `render_ui` begins.
  - Rendering any retained root of the same Context from inside `try_read`/`try_update` is documented
    as unsupported, and a built-in runtime reports a precise invariant panic rather than skipping
    the widget or producing stale output. Retained traversal never crosses Context boundaries.
  - A downstream container performs authorized nested `Children::measure_child`,
    `ContainerLayoutCtx::layout_child`, and visitor traversal while its parent state borrow is active
    without triggering the top-level-reentrancy diagnostic.
  - No frame/state-access flag or gate is added to enforce the reentrancy precondition.
  - Compile-fail checks prove ordinary downstream code cannot construct a state handle from a raw
    `Weak`, extract its `Rc`/`Weak`, or access `WidgetStateHandle::from_owner`; no Context token,
    frame flag, mount metadata, strong `Rc`, or raw `Weak` is returned as the application state
    capability or consulted during access.

  **Frozen contract evidence (2026-07-29)**

  The normative ownership and access sections above now define the complete optional-exposure
  contract. Factories always allocate and retain one strong state cell inside the opaque owner;
  `EXPOSE_STATE` controls only whether the application receives another weak typed capability.
  Liveness is allocation-based, borrow conflicts are per cell, failed ownership-moving updates
  return their exact input, and neither frame existence nor Context identity participates in state
  access. The fixed built-in table is the exhaustive exposure compatibility boundary.

  Repository inspection confirms that the current `WidgetHandle<T>` instead stores and clones a
  strong `Rc<RefCell<T>>`, exposes allocation-derived identity, uses panicking `borrow`/`borrow_mut`,
  offers whole-value `replace`, and is cloned again by `WidgetStateHandleDyn` for runtime dispatch.
  Those properties explain the migration but are not preserved behavior. P0.2 intentionally changes
  no production API: its compile-time, lifetime, borrow, and input-preservation criteria become
  executable and green in P1.0/P1.3, including the hidden-state proof implementations needed to
  inspect otherwise unexposed ownership.

- [x] **P0.3 — Freeze the state-owned `Children` contract**

  **Problem**

  A Context-owned editor duplicates the state-handle access path and exists primarily to enforce a
  frame boundary that is not required for safe single-threaded borrowing.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.3, P2.0-P2.4, and P3.2**

  **Settled decision and rationale**

  Every concrete container state owns one opaque `Children`, and `OwnedContainer` owns the sole
  persistent strong `Rc<RefCell<State>>`. A container that permits application topology changes
  returns `Some(WidgetStateHandle<State>)`; a fixed/internal container returns `None` but retains
  the same strong ownership shape. Application code changes membership through the typed state
  handle, with no Context argument, mounted identity, `ContainerHandle`, or second editor API.

  A Context-owned `ContainerEditor` was rejected because it requires public or handle-carried mount
  identity and duplicates checked state mutation. Rebuilding/replacing roots was rejected because it
  preserves allocation, generated-identity, and state-transfer work for a local child-list change.
  The accepted consequence is traversal-order observation: same-container mutation while that state
  is borrowed returns `Borrowed`, while mutation of another available container follows documented
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
  Every public built-in container constructor returns
  `(Option<WidgetStateHandle<C>>, Node)` and performs its `create_container`/`OwnedContainer`
  wrapping internally.
  Each concrete constructor owns its fixed exposure policy: the public dynamic layout constructors
  return `Some`, while an internal fixed container constructor may always return `None`. Callers do
  not select exposure through Parameters or a separate API.
  The atomic P1.3/P2.1 compile-safe batch publishes `Container`, constructible `Children`, the opaque
  visitors/contexts, owning `Node`, `ContainerBuilder`, `create_container`,
  `Node::container(OwnedContainer)`, the Column vertical
  slice, and Disclosure as the old public `Node` replacement. Row/Grid/Stack land in P2.0 and
  ScrollArea lands in P2.2 using that already-complete foundation. Downstream compile tests exercise
  the Column/Disclosure/custom-container paths in the atomic batch and expand to every built-in as
  each later item lands.

  The framework-provided APIs enforce unique ownership and no-reparent behavior for built-ins and
  ordinary callers. Safe Rust cannot prevent a downstream custom state type from publishing its own
  raw `Children` access or swapping collections; preserving the same no-detach/no-reparent boundary
  is therefore an explicit safe custom-container conformance obligation, not an `unsafe` trait
  requirement.

  Successful direct removal or replacement drops the affected `Node` owner immediately. Its weak
  widget/container state handles expire after any already-active access upgrades finish. Private
  focus, hover, capture, and queued routed targets are checked against the retained tree before use
  and sanitized at the next safe boundary; a stale target is cleared, never redirected, and a
  replacement at the same index does not inherit it. Regardless of which cross-container mutation
  a current traversal has already observed, the next ordinary frame is fully stable.

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

  // Build the persistent file-list container once. Column's OwnedContainer owns the
  // strong Rc<RefCell<ColumnState>>; the application receives only this weak handle.
  let initial_rows = directory_entries().map(file_row_node);
  let (files_state, files_node) =
      Column::create(ColumnParameters::new(initial_rows));
  let files_state = files_state.expect("the dynamic file list exposes ColumnState");

  // A directory refresh replaces only this container's children. It does not rebuild the
  // dialog root and does not require Context or a container/node ID.
  let new_rows = directory_entries().map(file_row_node).collect::<Vec<_>>();
  files_state.try_update_with(new_rows, |column, new_rows| {
      column.replace(new_rows);
  })?;

  // Successful removal drops the child in place; no attached Node is returned for reuse.
  files_state.try_update(|column| {
      assert!(column.remove_drop(3));
  })?;
  ```

  A crate-private fixed-composition container implementation uses the same strong state ownership
  but defines a constructor that always withholds the application capability:

  ```rust
  let (toolbar_state, toolbar_node) =
      FixedGroup::create(FixedGroupParameters::new(fixed_toolbar_children));

  assert!(toolbar_state.is_none());
  // The Node's private FixedGroup OwnedContainer still owns
  // Rc<RefCell<FixedGroupState>> and its Children.
  ```

  `FixedGroup` is the internal/test proof of the hidden-container path, not a mode of `Column`,
  `Row`, or another public constructor. A custom downstream container makes the same fixed choice in
  its own constructor.

  Checked borrowing defines conflicting and cross-container mutation without a frame gate:

  ```rust
  files_state.try_read(|_files| {
      // The same state cell is already borrowed.
      assert_eq!(
          files_state.try_update(|_files| {}),
          Err(StateAccessError::Borrowed),
      );

      // A different available container remains independently mutable.
      sidebar_state
          .try_update_with(notification_node, |sidebar, notification_node| {
              sidebar.push(notification_node)
          })
          .unwrap();
  })?;
  ```

  **Acceptance tests**

  - Framework construction and mutation consume each unique `Node` into exactly one `Children`
    owner; no framework-provided safe operation clones, detaches, shares, moves, or reparents an
    attached node.
  - Empty construction, ordered `FromIterator`, append, insertion at zero/`len`, out-of-range
    insertion, valid/invalid `remove_drop`, empty/non-empty `clear`, ordered replacement, and
    out-of-range `measure_child` follow the exact boundary behavior above.
  - A failed `insert` returns the exact input node with its weak descendant handles still live; a
    successful removal/clear/replacement returns no node and makes removed-state handles report
    `Dropped` after any active access upgrade ends.
  - Same-container mutation during its traversal returns `Borrowed` without panic.
  - Mutation of another available container follows traversal order: current phases process the
    state they observe without rollback, and the next ordinary frame is fully stable.
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
  - Removing or replacing a focused, hovered, captured, or queued-event target drops its owner,
    clears the stale private target at the next safe boundary, never redirects it, and does not
    transfer interaction state to a replacement at the same index.
  - The atomic-batch downstream compile test constructs Column and Disclosure from their completed
    `Node` returns without a wrapping step; P2.0/P2.2 extend the same test to each later built-in.
  - Construction tests grow with the rollout and ultimately prove every public dynamic built-in
    container returns `Some`, while the fixed internal proof container returns `None`; both paths
    retain identical strong-owner shape.
  - A downstream custom container implements public `Container`/`ContainerBuilder`, uses
    `Children::new` or `collect::<Children>()`, receives the factory-supplied weak state handle, and
    is accepted by `Node::container(OwnedContainer)` without private APIs.
  - File-dialog refresh replaces only the file/folder list children and does not call
    `Context::set_root_nodes`.
  - Public documentation includes the construction, replacement, removal, hidden-container, and
    borrow-conflict examples above.

  **Frozen contract evidence (2026-07-29)**

  The normative container-ownership and runtime-lifecycle sections above now define one mutation
  path: each concrete state owns one opaque ordered `Children`, its `OwnedContainer` retains the
  strong state cell, and optional weak typed handles provide checked state-local membership changes.
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

  Widget-specific persistent values, consumable events, and queued commands live in the concrete
  `WidgetState`. Application code observes or changes them only through
  `WidgetStateHandle<State>::try_read`/`try_update`; it does not pass the handle back to Context or
  retain a parallel widget `NodeId`.

  Generic handle-based Context results/focus were rejected because they require state handles to
  carry mounted node/Context identity or require Context to maintain a state-to-node registry.
  Public generated `NodeId` lookup was rejected because it makes applications retain state and
  placement identity together. The accepted migration cost is that each built-in defines the
  value, event, and command operations appropriate to its own state type.

  **Wanted behavior and contract**

  Define state-specific observation exactly as specified in the fixed event table: saturating
  pending counts consumed one occurrence at a time through `take_changed`/`take_submitted`, plus
  queued commands such as textbox focus request. Change `Widget::update` to return `()` and remove
  all generic result production and storage. Remove `ResourceState`, `FrameResults`,
  `FrameResultGeneration`, public `RetainedId`, public widget `NodeId`, every `state_of*` operation,
  and targeted Context focus. Root chrome uses `RootState` and the same pending typed-event contract;
  use the fixed built-in exposure/mutation table above rather than reclassifying in P1.1.

  Every pending event counter starts at zero. Recording uses `saturating_add(1)`, so `u32::MAX`
  remains `u32::MAX` rather than wrapping. A `take_*` call at zero returns `false` without changing
  state; otherwise it subtracts exactly one and returns `true`. Change and submission counters are
  independent: one `Widget::update` may record one occurrence of each kind, but never more than one
  occurrence of the same semantic kind regardless of how many low-level inputs contributed to that
  update. Pending occurrences remain in the strongly owned state across frames, hidden roots, and
  gated descendants until consumed or the owner is destroyed.

  Built-ins absent from the fixed event table expose no generic interaction event. In particular,
  ordinary leaf `ACTIVE` remains private runtime/paint state rather than a pending typed event.
  `RootState::is_active` is the intentional root-only persistent-state exception, while root change
  and submission occurrences use the same independent saturating counters; P0.7 owns their exact
  chrome recording points and programmatic-silence rules.

  Persistent values are read and changed directly through typed state:

  ```rust
  let checked = checkbox_state.try_read(CheckboxState::checked)?;

  checkbox_state.try_update(|checkbox| {
      checkbox.set_checked(!checked);
  })?;
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
  if save_button_state.try_update(ButtonState::take_submitted)? {
      save_document();
  }
  ```

  Programmatic commands are queued in state and consumed by that same widget at its next eligible
  update. For example, focus does not require the application to know the textbox's runtime node ID:

  ```rust
  pub struct TextboxState {
      text: String,
      focus_requested: bool,
  }

  impl WidgetState for TextboxState {}

  impl TextboxState {
      pub fn request_focus(&mut self) {
          self.focus_requested = true;
      }

      pub(crate) fn focus_requested(&self) -> bool {
          self.focus_requested
      }

      pub(crate) fn clear_focus_request(&mut self) {
          self.focus_requested = false;
      }
  }

  // Application code queues the command through the weak typed state handle.
  textbox_state.try_update(TextboxState::request_focus)?;

  // Inside TextboxWidget::update, after obtaining &mut TextboxState as `state`,
  // the runtime consumes the command using its existing widget-local context.
  if state.focus_requested() && ctx.set_focus() {
      state.clear_focus_request();
  }
  ```

  `WidgetUpdateCtx::set_focus(&mut self) -> bool` returns `true`, establishes or retains focus, and
  marks focus as refreshed when the current widget is eligible, including when it already owns
  focus. It returns `false` without changing the focus slot or its update marker when effective
  options or cross-root/modal policy makes the widget ineligible. Existing custom widgets may
  ignore the returned value; queued commands use it to avoid consuming a request that was not
  fulfilled. A hidden or gated descendant receives no update, so its queued request remains
  untouched.

  A hidden widget/container uses the same `Widget::update -> ()` signature. Returning `None` from
  construction means only that none of its values, events, or commands form an application API:

  ```rust
  let (decoration_state, decoration_runtime) = Decoration::create(parameters);
  assert!(decoration_state.is_none());

  let decoration = Node::widget(decoration_runtime);
  // DecorationWidget::update performs its runtime work and returns no generic leaf result.
  ```

  **Acceptance tests**

  - Every change/submission listed in the fixed event table is observed through its typed state.
    Zero-count reads are stable, each successful `take_*` consumes exactly one occurrence, separate
    event kinds remain independent, and a test-only maximum counter proves saturation without wrap.
  - Multiple low-level inputs contributing to one update produce one semantic occurrence, while
    occurrences from separate updates accumulate and require separate `take_*` calls. An update
    that produces both change and submission records one independently consumable occurrence of
    each kind.
  - Unconsumed occurrences survive ordinary frames, root hiding/showing, and descendant gating;
    destruction drops them with their owning state rather than publishing a final generic result.
  - Programmatic setters are silent; tests cover every setter plus Combo's documented
    `update_items` clamp exception and silent direct `select`.
  - Combo records consumable `CHANGE`/`SUBMIT` equivalents at the same clamp/header-click decision
    points as the current implementation.
  - Textbox focus requests survive hidden, gated, non-interactive, or cross-root/modal-ineligible
    updates and clear only after an eligible update successfully assigns focus. `set_focus` returns
    `true` for an eligible already-focused widget and `false` without changing focus/update state
    for an ineligible widget.
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
  - Public examples and rustdoc include persistent-value, consumable-event, queued-command, and
    hidden-runtime cases matching the code above.

  **Frozen contract evidence (2026-07-29)**

  The normative typed-state table and command sections above now define the sole application
  interaction boundary. Persistent values, independent saturating event counts, and queued commands
  live with their concrete state; `Widget::update` mutates that state and returns `()`; focus,
  capture, and routing retain dedicated runtime mechanisms without becoming application identity
  APIs. Event lifetime follows state lifetime rather than a frame generation, and programmatic
  setters remain silent except for the explicitly preserved Combo normalization behavior.

  Repository inspection confirms that the current implementation instead returns the
  `ResourceState::{CHANGE, SUBMIT, ACTIVE}` bitflags from every `Widget::update`, double-buffers them
  in `FrameResults` maps keyed by public/scoped `RetainedId`, exposes the committed generation through
  `Context::committed_results`, and uses public builder `NodeId` values for result lookup and
  `Context::set_root_focus_node`. `WidgetUpdateCtx::set_focus` currently assigns unconditionally and
  returns `()`. The calculator, full demo, file dialog, tests, and custom-widget examples still
  produce or consume parts of that surface. Those facts are migration cost, not preserved behavior.
  P0.4 intentionally changes no production API: its typed-event, focus-command, API-removal,
  compile-fail, application-migration, and documentation criteria become executable and green in
  P1.1/P2.5/P3.0-P3.2.

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
  `Node::container` reach it exactly once; built-in container factories do not allocate an
  additional identity around their returned node. Moving an unmounted node, configuring it through
  consuming `with_policy`/`with_grid_span`, returning it from a failed `Children::insert`, and
  mounting it in any Context preserve the original scalar. Dropping even a never-mounted node does
  not return its ID to the allocator.

  Actual focus, hover, capture, and routed input are owned per retained tree, below Context and
  inside its `WindowEntry`. A state type may contain a command such as `focus_requested`, but the
  authoritative focused node remains the private tree target:

  ```rust
  struct WindowEntry {
      id: RootId,
      root_state: WidgetStateHandle<RootState>, // private weak clone
      tree: WidgetTree,
      // Root kind, z-order, just-opened, and backend viewport policy.
  }

  struct WidgetTree {
      root: Node,
      focus: Option<RuntimeNodeId>,
      hover: Option<RuntimeNodeId>,
      capture: Option<RuntimeNodeId>,
      routed_events: HashMap<RuntimeNodeId, Vec<UiInputEvent>>,
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

  The same liveness validation applies to `hover` and every key in `routed_events`. A missing hover
  target is cleared, a missing capture/focus target is cleared before it can direct another event,
  and queued batches for missing nodes are discarded rather than delivered or transferred. Tree
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
  - `Node::widget`, `Node::custom_render`, `Node::container`, and every built-in container factory
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
  - Each `WindowEntry`'s `WidgetTree` owns its focus, hover, capture, and routed-event targets; no
    application state or top-level Context field becomes their authoritative owner.
  - Missing focus, hover, and capture targets are cleared, and every queued routed-event entry for a
    missing node is discarded; none is redirected or inherited by a later node.
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
  the authoritative owner of its own focus, hover, capture, and routed-event targets; process-wide
  uniqueness prevents a stale target in any tree from aliasing a replacement or a node in another
  Context without Context tokens, mount metadata, or a registry.

  Repository inspection confirms that the current `Id` is a public `usize` wrapper constructible
  from pointers, caller integers, and strings. `UiNodeBuilder` derives public `NodeId` values from
  scope seeds, node tags, sibling order, and optional keys, then validates duplicate hashes;
  `WidgetHandle::id` separately casts its strong `Rc` allocation address; `RetainedId::root_node`
  composes root and builder identities; and scroll-area synthetic descendants hash IDs from their
  parent and semantic part. The useful current per-root `UiRuntime` ownership of focus, hover,
  capture, and routed-event maps is retained conceptually, while those public, pointer-derived,
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

  `NodeRuntime` stores `Policy`, `GridSpan`, derived layout, and transient interaction flags, but no
  generic visibility bit. Add the exact consuming pre-insertion methods
  `Node::with_policy(Policy)` and `Node::with_grid_span(columns, rows)`, defaulting through
  `Policy::auto()` and `GridSpan::ONE`. Each method changes only its own placement field, so either
  chaining order preserves the other setting. `with_grid_span` delegates to `GridSpan::new` and
  clamps each zero component to one. Both methods preserve the node's already-allocated private
  runtime identity. Policy participates once in parent slot allocation; Grid consumes span while
  non-grid parents ignore it. There is no mounted policy/span setter or public placement getter that
  lends runtime state.

  Do not add `Node::show`, `hide`, `set_visible`, `is_visible`, or an equivalent generic handle/state
  command. Root hide/show remains a Context-coordinated `RootState` mutation with the P0.7 contract.
  Descendant gating remains the narrower `Container::children_visible` mechanism: a false gate keeps
  the container itself active and its descendant nodes/state handles owned and live, but descendants
  contribute no measure/layout and receive no input, update, paint, or custom-render callback.
  Sanitization clears their focus, hover, capture, and queued routed events without restoring those
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
  - `Node::with_policy` and `with_grid_span` preserve runtime identity and one another in either
    chaining order. Defaults are `Policy::auto()`/`GridSpan::ONE`; zero columns/rows clamp
    independently through `GridSpan::new`.
  - Row/Column/Grid tests prove policy is applied once through `ContainerLayoutCtx`, Grid consumes
    the configured span, and non-grid parents ignore span. Compile-fail/API checks prove there is no
    mounted placement setter or runtime-state getter.
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
  `NodeOptions` also establishes the intended `Policy::auto()`/`GridSpan::ONE` defaults and
  `GridSpan::new` zero clamping, which move to the unique node rather than changing semantics.

  P0.6 intentionally changes no production API: owning-name/export, legacy Disclosure replacement,
  placement, visibility removal, traversal gating, target sanitization, compile-fail, and
  documentation criteria become executable and green in the atomic P1.3/P2.1 batch and P2.4.

- [ ] **P0.7 — Freeze unified root chrome, popup, and destruction behavior**

  **Problem**

  Root destruction is required for deterministic tree lifetime, but no operation currently removes
  a `WindowEntry`. Chrome is currently special-cased by the window manager and has no typed retained
  state, while keeping a root-only result mechanism would duplicate widget-state observation.

  **Decision needed: No — protected wanted-behavior baseline**

  **Implementation owner: P1.4, P2.4, and P3.0**

  **Wanted behavior and contract**

  Add public `RootHandle`/`RootState`, private `RootChromeContainer`, and
  `Context::destroy_root(RootId) -> bool` with the exact ownership, weak-liveness, hidden-versus-
  destroyed, query, mutation, and pending-event semantics defined above. Change all root constructors
  to consume one application `Node` and return `RootHandle`; the private container owns that node as
  its one immutable child slot. Remove the complete generic result family rather than retaining a
  root exception.

  Move chrome measure/layout/routing/update and its paint underlay to `RootChromeContainer` and one
  shared pure geometry helper. After tree paint, let the window compositor append only the chrome
  overlay from that helper so title/close/resize remain above descendants. Use ordinary `WidgetTree`
  capture for moving/resizing. Keep only cross-root/modal selection, z-order, backend viewport
  coordination, popup outside-hit detection, and this post-tree compositing boundary in the window
  manager; outside dismissal calls the framework-private `RootState` operation before routing the
  press elsewhere. Title close and popup dismissal hide and record a typed submission; they do not
  destroy. Programmatic root operations are silent. Add no application-child replacement operation;
  document persistent-container content and root destroy/recreate behavior.

  **Acceptance tests**

  - Each window/dialog/popup constructor returns a `RootHandle` whose ID identifies the Context
    entry and whose weak `RootState` handle observes the same geometry/visibility used by chrome.
  - Creation tests pin visible/hidden status, initial rectangle, immutable name, and default options
    for all three root kinds; state handles are usable before any frame.
  - Cloning/dropping `RootHandle` never changes root lifetime; only `destroy_root` does.
  - Destroying an existing root returns `true`, immediately unmounts/releases the chrome container
    plus application subtree, and makes root/descendant handles report `Dropped` after active access
    closures finish; a root-state closure is the only temporary reason physical child drop may lag.
  - Destroying an unknown or already-destroyed root returns `false`; the ID is never reused.
  - Every Context root setter distinguishes `UnknownRoot` from `Borrowed`; public root reads occur
    only through the checked `RootState` handle, and no parallel Context query remains.
  - Title close and popup outside-click increment `RootState`'s pending submission count while
    retaining a hidden tree; a subsequent explicit destroy removes it.
  - Only left press activates close/move/resize. A popup ignores its opening/showing press for
    outside dismissal, then any later pointer-button press outside dismisses before that press routes
    to the root behind it.
  - At most one popup is visible per Context. Showing another silently hides the previous popup,
    clears its transient targets without recording a submission, and gives the new popup ordinary
    `just_opened` suppression. The switch is atomic and returns `Borrowed` without changing either
    popup if one of the required state cells is unavailable. Nested popups remain a separate
    feature.
  - Drag/resize exposes current active/moving/resizing state and increments `take_changed` only when
    user interaction actually changes the rectangle; programmatic operations increment no event.
  - Multiple root events persist/accumulate under the same saturating-count rules as widget events.
  - Root chrome is exactly one internal semantic container node, with no title/close/resize child
    nodes and no window-manager-owned parallel chrome capture/update state; the compositor retains
    only the documented post-tree overlay step.
  - The shared geometry helper yields identical rectangles for layout, routing, paint, min-size,
    outer/body conversion, and backend viewport integration.
  - Hiding silently clears transient targets and active chrome state but retains typed state and
    pending events; showing does not restore cleared targets.
  - Programmatic options clear incompatible move/resize capture, popup reopen applies pointer
    positioning/`just_opened`, and auto-size normalization emits no change event.
  - Compile-fail/API-surface tests prove no operation replaces a root `Node` while retaining its
    `RootId`; a persistent exposed container root supports dynamic content without replacement.
  - Production/API searches find no `ResourceState`, `FrameResults`, `FrameResultGeneration`,
    `RetainedId`, `state_of_root`, or `state_of_retained`.

### P1 — Final state ownership and persistent topology

P1 implements the final opaque ownership model directly. Each item inherits the behavioral
acceptance criteria of its named P0 owner; the bullets below add migration sequencing, structural
removal, and focused implementation evidence rather than redefining that behavior.

- [ ] **P1.0 — Land the public construction/state primitives with Checkbox as the vertical slice**

  **Problem**

  The new ownership contract must work end to end before all built-ins are split.

  **Decision needed: No — implements P0.1 and P0.2**

  **Target contract or migration**

  Add the four public widget roles, weak handle/error primitives including `try_update_with`, private
  method-free `StateKeepAlive`, opaque `OwnedWidget`, non-overridable `create_widget`, and the final
  `WidgetBuilder::{initialize, build, EXPOSE_STATE}` contract. Split `Checkbox` into builder,
  parameters, state, and runtime implementation. Let a temporary crate-private projection adapter
  own and delegate the new `OwnedWidget`; delete it in P1.2. No raw-box insertion API is exposed or
  retained past this item. Container/visitor contracts may be implemented
  crate-privately as migration scaffolding, but do not export public `Container`, `ContainerState`,
  `Children`, visitors, container contexts/results, or owning `Node` until the atomic P1.3/P2.1
  compile-safe batch supplies a usable complete surface and resolves the old public `Node` collision.

  **Acceptance tests**

  - Checkbox construction returns `Some(WidgetStateHandle<CheckboxState>)` plus `OwnedWidget`.
  - Checkbox's `OwnedWidget` owns the only persistent strong `Rc<RefCell<CheckboxState>>`; its
    runtime uses the exact factory-supplied weak handle.
  - A hidden-state proof widget returns `None` plus a fully functional `OwnedWidget` that owns the
    same strong unit-state shape.
  - External code can implement the complete four-role custom widget path.
  - Checked read/update and destruction behavior match P0.2.
  - Current checkbox geometry, click toggle, paint, and options remain correct.
  - Public rustdoc explains which data belongs in Parameters versus State.
  - Public exports at this item contain the widget roles/handle/error surface and no incomplete
    container/visitor/owning-Node API. Any temporary projection or `NodeBehavior` migration bridge is
    crate-private and names the P1.3/P2.1 batch or P2.3 as its deletion/export point.

- [ ] **P1.1 — Split every built-in leaf using the fixed exposure/mutation mapping**

  **Problem**

  Current built-in structs mix all three roles, and examples mutate fields directly.

  **Decision needed: No — implements P0.1 and P0.4**

  **Target contract or migration**

  Implement the authoritative table in “Fixed built-in state exposure and compatibility mapping”:
  every listed leaf except `Custom` returns `Some`, `Custom` returns `None` with `State = ()`, and
  the old header/tree `widgets::Node`/`NodeStateValue` APIs retire into exposed `DisclosureState` in
  P2.1 without an alias. Preserve exactly the mounted values/events/commands listed in that table.
  Move initialization-only visual/base configuration to Parameters and runtime-derived/cached data
  to private runtime fields. This is a deliberate breaking boundary; arbitrary old public-field
  mutation not listed in the table does not become a mounted State API.

  **Acceptance tests**

  - A mapping test/document lists every old public field/method and classifies it as Parameters,
    exposed State, runtime-private/derived, or retired; it does not claim every field remains
    mutable after mounting.
  - Slider value, textbox/text-area content, combo selection, swatch color, and mutable display text
    remain typed and fallible through handles.
  - Checkbox/slider/number/text change and button/list/text/combo submission use the exact typed
    event APIs, recording points, saturating multiplicity, and programmatic-setter rules specified
    above; Combo clamp/select edge cases are pinned separately.
  - `Custom` has a negative test proving no weak application handle is returned and a
    lifetime/ownership test proving its `OwnedWidget` retains the unit state cell; no other listed
    leaf is conditionally hidden.
  - Built-in Widget implementations preserve current phase behavior and focus policies.
  - No built-in state type implements `Widget` merely to obtain dispatch.
  - The migration mapping names `Node::header`, `Node::tree`, `NodeStateValue::{Expanded, Closed}`,
    their predicates, label/options, and click-toggle behavior and points each one to its exact
    `DisclosureParameters`/`DisclosureState` replacement.

- [ ] **P1.2 — Dispatch persistent leaf payloads directly through `OwnedWidget`**

  **Problem**

  Current `WidgetNode` erases and clones a strong state handle, then redispatches `Widget` methods.

  **Decision needed: No**

  **Target contract or migration**

  Change the internal leaf payload to own `OwnedWidget`. Keep only the generic geometry/input
  adapter required to create `WidgetUpdateCtx`/`WidgetPaintCtx`; it delegates `Widget` directly and
  owns optional private `CustomRenderKey`. The final
  `Node::widget(OwnedWidget)`/`Node::custom_render(OwnedWidget, CustomRenderHandle<B>)` surface lands
  with the owning `Node` in P1.3; no raw box overload exists. Delete `WidgetStateHandleDyn`, `clone_box`,
  `erased_widget_state`, widget allocation IDs, and duplicate-state dispatch tracking. Keep existing
  renderer-registry preflight for removed, foreign, or backend-incompatible erased keys.

  **Acceptance tests**

  - Each requested leaf measure/update/paint invocation uses one direct Widget dispatch path; tests
    do not incorrectly require only one measurement request per frame.
  - Moving the `OwnedWidget` into a node does not change an exposed handle or create one for a hidden
    widget.
  - An `OwnedWidget` cannot be cloned into two nodes through safe APIs.
  - The P1.3 downstream example constructs a custom-render leaf from the owner returned by
    `create_widget` and a `CustomRenderHandle<B>` without accessing `CustomRenderKey`.
  - `Node::widget` records no custom draw; `Node::custom_render` runs its callback after widget paint
    with the derived content rectangle and clip.
  - Removed/foreign custom-render handles fail existing preflight before backend acquisition.
  - `rg` finds no erased state-handle adapter after the temporary bridge is removed.

- [ ] **P1.3 — Introduce unique `Node` ownership and state-owned container children**

  **Problem**

  Current nodes are rebuilt projections, while direct container state needs stable owned nodes.

  **Decision needed: No — implements P0.3, P0.5, and P0.6**

  **Target contract or migration**

  Add `ContainerBuilder`, non-overridable `create_container`, opaque `OwnedContainer`, the one public
  opaque non-cloneable owning `Node`, private `NodeKind`, private
  `RuntimeNodeId`, `NodeRuntime` without generic visibility, opaque constructible `Children`,
  public `Container: Widget`, marker `ContainerState`, opaque visitors, scoped layout/input
  contexts/results, the Column state/owner vertical slice, and P2.1 Disclosure as the atomic
  P1.3/P2.1 public API batch. Add exact consuming
  `Node::with_policy`/`with_grid_span` placement methods; successful child insertion consumes the
  node. Use the same optional
  `WidgetStateHandle<C>` model for both leaf and container state. Column and Disclosure constructors
  return `(Option<WidgetStateHandle<C>>, Node)` in this batch after using public `Node::container`
  internally. Row/Grid/Stack add the same final shape in P2.0, and ScrollArea does so in P2.2; each
  returns its fixed documented exposure outcome when it lands. Downstream custom constructors use
  `create_container` and pass the returned `OwnedContainer` to the same node constructor explicitly;
  no raw container box can enter the tree.

  The old header/tree `widgets::Node` exports must be removed before the owning `Node` export lands.
  P1.3 and P2.1 therefore land in one compile-safe integration batch, or a crate-private
  `LegacyDisclosureNode` adapter bridges only those two items and is deleted by P2.1. No temporary
  public alias is permitted.

  **Acceptance tests**

  - Node creation assigns unique private identity before mounting.
  - Children growth/reordering does not change node IDs or state cell addresses.
  - Removing/clearing/replacing drops exactly the removed nodes and opaque owners.
  - Failed out-of-range insertion returns the uncommitted input node.
  - `try_update_with` also returns the exact unmounted node if the target state is `Dropped` or
    `Borrowed` before insertion begins; examples never move unique nodes into ordinary `try_update`
    closures when access failure must preserve them.
  - `Node::with_policy` and `with_grid_span` work before insertion; no mounted placement or generic
    visibility mutator exists.
  - No `ContainerHandle`, `ContainerEditor`, mounted-state metadata, raw mutable child callback, or
    framework-provided reparent path exists.
  - Column and Disclosure construction has no heuristic, parameter-selected, or caller-usage-based
    exposure decision; P2.0/P2.2 extend that fixed policy to the later constructors.
  - No incomplete public container/visitor surface exists before this compile-safe batch, and the
    old public header/tree `Node` is absent before the owning `Node` export becomes reachable.
  - Atomic-batch downstream tests cover ergonomic Column/Disclosure `(handle, Node)` construction
    and an external `ContainerBuilder`/`Container` implementation created through
    `create_container` and wrapped with `Node::container(OwnedContainer)`; later rollout tests extend
    the constructor assertion to every built-in.

- [ ] **P1.4 — Give each root one persistent `WidgetTree`**

  **Problem**

  `WindowEntry` currently accepts replaceable root projections and transfers runtime state.

  **Decision needed: No — implements P0.7**

  **Target contract or migration**

  Change window/dialog/popup creation to consume one application `Node`, construct one private
  `RootChromeContainer` around it, and return `RootHandle`. Give `WindowEntry` only a private weak
  clone of the `RootState` handle; the internal container's owned wrapper is the sole persistent
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

  **Acceptance tests**

  - Window, dialog, and popup behavior works from one persistent private chrome-container root and
    one application content child.
  - Creation returns `RootHandle`; hide/show preserves its weak state capability and pending events,
    while destruction expires root/descendant handles after active accesses finish.
  - Unknown/already-destroyed IDs return `false` from destruction/fronting and `UnknownRoot` from
    setters; handles report `Dropped`, same-state setter conflicts report `Borrowed`, and IDs are
    never reused.
  - `RootState` is authoritative for rect/visibility/current chrome interaction/pending events;
    WindowEntry is authoritative for lifecycle/cross-root/z-order/backend concerns only.
  - Title close/popup dismissal hide and record typed submission; actual user drag/resize changes
    record typed change/current active state; programmatic changes are silent.
  - At most one popup is visible per Context; showing another silently hides and sanitizes the
    previous popup without recording a submission. Nested popup behavior is out of scope.
  - Phase-count and hit-region tests prove chrome dispatches through the tree once, body input falls
    through to the application child, the post-tree overlay paints above descendants, and no
    parallel window-manager chrome capture/update exists.
  - Root creation/destruction needs no state-to-node registry, result generation, or token.
  - API-surface tests prove a root `Node` cannot be replaced while preserving `RootId`; a dynamic
    container-root test replaces descendants while preserving the root/window and persistent state.

### P2 — Container mechanics and runtime traversal

P2 makes the P0 container, visibility, identity, and event behavior executable on P1 ownership.
Its acceptance bullets are implementation-specific evidence and edge coverage; any material need to
change a protected P0 behavior follows the explicit change-control rule.

- [ ] **P2.0 — Convert row, grid, and stack and finish the shared layout-container mechanics**

  **Problem**

  Existing containers own child vectors but are reconstructed by the builder and lack typed state
  handles for local membership changes.

  **Decision needed: No — implements P0.3**

  **Target contract or migration**

  Build on the `ColumnState`/Column vertical slice already landed in the atomic P1.3/P2.1 batch.
  Introduce `RowState`, `GridState`, and `StackState`, each owning `Children` and the exact mounted
  configuration defined above: Row widths/item height, Grid column/row tracks, and Stack item
  width/item height/direction; Column adds none. Their private runtimes borrow state for
  measure/layout and supply children for recursion through opaque visitors with explicit scopes.
  Every public dynamic state exposes the same safe
  `len`/`is_empty`/`push`/`insert`/`remove_drop`/`clear`/`replace` family and no whole-collection
  getter. Extend constructor/exposure/ownership conformance from Column/Disclosure to Row/Grid/Stack.

  **Acceptance tests**

  - Empty/populated/dynamic containers match current layout for every sizing policy.
  - State setters change Row/Grid/Stack layout on the scheduled post-update or next-frame pass as
    defined; Stack direction migration replaces `demo-full` root rebuilding.
  - Missing/excess Row widths, empty/extra Grid tracks, grid reflow, grid span, Style spacing, and
    nested transforms follow the exact target rules.
  - `layout_child` applies `Policy` once after slot/span resolution; a non-`Auto` policy changes only
    that child allocation and never rewrites a shared track. No mounted policy/span setter exists.
  - Same-container visitor mutation returns `Borrowed`; another container mutation follows pinned
    traversal order.
  - Dynamic membership performs no root reconstruction or state transfer.

- [ ] **P2.1 — Make disclosure one stateful container**

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

- [ ] **P2.2 — Make scroll area one container state with direct children**

  **Problem**

  Current scroll area creates viewport, track, scrollbar, and corner semantic nodes sharing one
  `Rc<RefCell<ScrollAreaState>>`.

  **Decision needed: No**

  **Target contract or migration**

  Add one `ScrollAreaState` with direct `Children`, public offset/scrolling-enabled state, private
  drag/derived geometry, and immutable parameter-owned framing/base options. Disabling scrolling
  synchronously clears private drag state and resets offset to zero; tree target sanitization clears
  capture before another event is routed to the disabled area, and derived layout hides bars.
  Requested offsets clamp as specified above. The runtime owns clipping, translation,
  panel/bar/thumb/corner paint, wheel fallback, drag capture, and range clamping. Routing reads the
  current offset/geometry to decide whole-event consumption or capture and queues the localized
  event; the later inherited `Widget::update` is the only operation that changes offset or drag
  state. It uses
  `ContainerInputCtx::route_widget_in_rect` for viewport and scrollbar hit regions and
  `ContainerLayoutCtx::set_children_viewport` for the clipped/translated content surface. Remove
  all synthetic semantic nodes.
  `ScrollAreaState` exposes the same safe child-operation family and never returns or lends its
  complete collection. Its constructor extends the final `(Some(handle), Node)` ownership/exposure
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
  - Routing a wheel/drag event does not mutate `ScrollAreaState`; it queues exactly one localized
    event, and the subsequent `Widget::update` applies the state change once.
  - Sub-rectangle routing intersects the active clip, preserves container-local pointer
    coordinates, and never lets a scrollbar/body hit leak into the other region.
  - Replacing children preserves scroll state and clamps offset.
  - Removing the area expires all descendant handles and cannot redirect capture to a replacement.

- [ ] **P2.3 — Traverse retained boxes and checked container borrows directly**

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
  `Container::layout`, `Container::route_input`, check `children_visible`, and hold the scoped
  framework-created `ChildrenVisitor`/`ChildrenVisitorMut` borrow during recursion. The runtime uses crate-private
  double-ended `Children` iteration: forward for update/layout/paint and reverse for deepest-first
  input routing. Leaf layout remains the generic
  measure-to-content calculation. Move the recursion currently in `UiRuntime::measure_node_ref`
  behind private `Node` measurement, and let public `Children::measure_child` delegate to it; this
  lets inherited `Widget::measure` remain unchanged and requires no container measurement trait.
  Carry transforms/clips/root state on the stack. Once all variants use direct dispatch, delete
  `NodeBehavior`, every implementation and bound of it, and any temporary adapter introduced during
  P1. Do not replace it with another private catch-all runtime trait or parallel container
  measure/update/paint adapter.

  Query `children_visible` immediately before each descendant recursion. In particular, update the
  container itself first, then query the gate before updating its children; this gives Disclosure
  collapse and root close same-frame suppression. Measure/layout/paint/input query the gate before
  entering children for that phase. The private root chrome gate does not create generic node
  visibility; normally the window manager skips the complete tree when `RootState` is hidden.

  **Acceptance tests**

  - Every live node whose ancestor container gates permit traversal updates/paints once in the
    established order; there is no generic node visibility check.
  - Phase-count tests prove each explicit measurement request uses one inherited `Widget::measure`
    dispatch and each eligible node receives one update/paint dispatch, with no parallel
    container-phase path. The test permits the specified two layout phases and bounded scroll
    convergence rather than asserting one measure call per frame.
  - Ordinary containers use the generic `route_widget` default; special container routing only
    queues or declines events, and the single inherited `Widget::update` call performs state changes.
  - Nested scroll boundary and pointer-capture tests prove the pre-update routing result is available
    without a second Widget update or a container-specific update phase.
  - A downstream custom container measures children through `Children::measure_child`, reads
    placement through `child_policy`/`child_grid_span`, and lays them out through `layout_child`
    without private APIs or a second measure method.
  - Layout tests cover invalid indices returning `None`, `Policy::auto`/`GridSpan::ONE` defaults,
    zero-span clamping, viewport intersection, child translation, content size, and overflow
    propagation.
  - Disclosure and scroll tests exercise `route_widget_in_rect`; ordinary containers exercise
    `route_widget` over the full content rectangle.
  - Tree walking performs no weak upgrade for identity, topology, lookup, or dispatch and performs
    no hash lookup, registry access, or downcast. A concrete runtime may upgrade only its own
    factory-supplied associated-state handle, at most once per runtime method invocation that needs
    state; those upgrades are counted separately in baselines.
  - Nested geometry remains correct at nonzero origins.
  - Update/layout/paint visit siblings forward; pointer routing visits siblings in reverse z-order.
  - Downstream compile-fail tests prove ordinary callers cannot construct the opaque visitors,
    obtain `&mut Children`/`&mut Node`, call direct node iteration, or swap/replace/take an attached
    collection through framework-provided APIs.
  - Same-cell and cross-cell visitor tests produce errors/observation exactly as documented.
  - Zero/multiple visitor submissions panic with the specified diagnostics, while built-ins and the
    downstream conformance example submit the same authoritative collection through both methods.
  - Runtime module docs state phase, traversal, and borrow order.
  - A production-source search finds no `NodeBehavior` trait, implementation, bound, boxed object,
    import, or equivalent all-node behavior adapter.

- [ ] **P2.4 — Sanitize runtime targets around direct topology changes**

  **Problem**

  Direct child mutation cannot synchronously call Context to clear focus/capture/routed events.

  **Decision needed: No — implements P0.3, P0.5, P0.6, and P0.7**

  **Target contract or migration**

  Validate private targets before routing and sanitize missing IDs at the next safe tree boundary.
  Rebuild per-frame live-target data rather than retaining removed widget entries. Never resolve an
  ID through a pointer or reuse an ID. A `children_visible == false` subtree is ineligible for
  focus/hover/capture/routed targets even though its IDs remain live. A ScrollArea whose exposed
  state has disabled scrolling is ineligible to retain pointer capture; sanitization reads that
  state and releases its tree-owned capture before routing. A hidden root similarly clears
  focus/hover/capture/routed targets while retaining the tree; root destruction releases complete
  retained runtime ownership. There are no result generations to sanitize.

  **Acceptance tests**

  - Removing a focused/captured target before a frame clears it before new input routing.
  - Cross-subtree removal during update cannot route later input to the removed or replacement node.
  - Pointer release after target removal is ignored safely.
  - Hidden roots preserve widget/application state but clear focus/hover/capture/routed targets;
    showing does not restore those transient targets. Destroyed roots unmount all tree state and
    release it subject only to already-active state upgrades.
  - Collapsing Disclosure clears descendant focus/hover/capture/routed events, and expanding does
    not restore them automatically.
  - Disabling a captured ScrollArea through its state handle releases capture before the next routed
    event without giving the state setter a `WidgetTree` or Context capability.
  - Destroyed roots expire `RootState` and descendant handles; hidden roots keep those handles live.
  - Debug integrity checks find no live target absent from its tree after sanitization.

- [ ] **P2.5 — Make routed events authoritative and reduce the frame to two tree layouts**

  **Problem**

  Current input is interpreted twice: routing queues localized `UiInputEvent` values, then update
  receives raw `Input` and calls `UiRuntime::interaction_for` to derive hover, click, active,
  focus, and scroll again. The window manager also performs pre-input layout before
  `UiRuntime::update_paint_frame` repeats layout both before and after update, producing three tree
  layouts.

  **Decision needed: No — carries forward the approved correctness contract**

  **Target contract or migration**

  Convert raw `Input` to ordered routed events once at the window-manager boundary. Routing owns
  hit testing, focus/capture transitions, the per-node interaction snapshot, event localization, and
  wheel propagation/consumption. `Widget::update` receives only the queued localized events plus the
  snapshot in `WidgetUpdateCtx`; remove raw `Input` from update traversal,
  `UiRuntime::interaction_for`, and the duplicate scroll-delta channel. Delete public
  `WidgetUpdateCtx::scroll_delta()` and `WidgetPaintCtx::scroll_delta()` plus their common stored
  field; widgets read scroll only through `WidgetInputEvents::scroll_delta()` on the localized
  `UiInputEvent` batch. `WidgetUpdateCtx::set_focus` returns whether the current routed snapshot
  permits focus; it refuses non-interactive widgets so queued typed-state focus commands can remain
  pending. Existing custom widget calls may ignore the returned value. Document these intentional
  custom-widget API changes.

  Cross-root/modal selection is the first stage of that same dispatcher. It applies the one
  `just_opened`/outside-popup dismissal rule through the popup's private `RootState` operation, then
  routes the same press to the newly eligible front root. It must not derive another click/drag/
  wheel snapshot or let retained widget update inspect raw `Input`.

  Use this ordinary frame pipeline:

  ```text
  begin frame
      -> pre-input tree layout
      -> cross-root gating / optional popup dismissal / route input once
      -> update each eligible widget/container once
      -> post-update tree layout
      -> paint
  ```

  The pre-input layout supplies hit geometry. Pointer events retain routing-time local coordinates,
  so a later layout cannot relocalize them. Scroll routing only chooses consumption/capture and
  queues the event; the recipient's single `Widget::update` changes offset/derived translation,
  which post-update layout/paint observes without an unconditional middle layout. The post-update
  pass remains required because updates can change intrinsic size, disclosure state, dynamic
  options, or topology. Skipping it requires the separate measured optimization decision in P5.2.

  **Acceptance tests**

  - Every raw transition enters one ordered dispatcher and has one authoritative interaction
    outcome; popup outside dismissal is decided there before the same press is routed onward.
  - Widget update receives exactly the localized events selected by routing; hover/click/active,
    focus/capture, and wheel state cannot disagree with those events.
  - Production searches find no raw `Input` in retained node update contexts, no
    `UiRuntime::interaction_for`, no context-level `scroll_delta` accessor/storage, and no duplicate
    scroll derivation; `WidgetInputEvents::scroll_delta()` reads only the routed batch.
  - Instrumentation reports exactly one pre-input and one post-update tree-layout phase in an
    ordinary frame; there is no unconditional pre-update repeat inside update/paint.
  - Scroll routing itself leaves widget state unchanged; the one update applies translation/thumb
    state before paint without an intermediate full layout. Size-changing update remains visible to
    same-frame post-update layout and paint.
  - Textbox, slider, number, text-area, disclosure, nested scroll, capture, key/text, and front-root
    pointer gating tests pass with routed events as their only source; popup dismissal is covered as
    the single documented cross-root exception.
  - A textbox focus command remains queued when its update snapshot is non-interactive and clears
    only after `set_focus` returns `true`; hidden or gated widgets do not update and therefore also
    retain the request.

### P3 — Public roots and application migration

- [ ] **P3.0 — Delete projection builders and root replacement**

  **Problem**

  Projection construction remains the only current public tree-authoring path.

  **Decision needed: No**

  **Target contract or migration**

  Migrate root creation and all built-in factories to constructor-returned optional state handles
  and boxed runtimes wrapped in unique Nodes. Delete `UiNodeSet`, `UiNodeBuilder`, `NodeBuilder`,
  builder keys, `set_root_nodes`, and `transfer_runtime_state_from`. Delete the P1 temporary adapter
  in this item. Delete `ResourceState`, the complete frame-result store/query API, and all public
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

- [ ] **P3.1 — Migrate examples and external custom widgets**

  **Problem**

  Examples cache strong widget handles, generated IDs, and projections and currently demonstrate
  direct `Widget` implementations on combined state/runtime structs.

  **Decision needed: No**

  **Target contract or migration**

  Migrate `simple`, calculator, `demo-full`, backend cube, texture smoke, and retained custom drawing
  to parameter/state/runtime/builder roles. Construct roots once, retain only the typed handles
  returned as `Some`, consume exposed widget events from state, and mutate dynamic membership through
  exposed container state. Replace every old header/tree `Node` use with the corresponding
  `DisclosureParameters::header`/`tree` construction. Migrate the demonstrated stack-direction
  rebuild to `StackState::set_direction`; move initialization-only visual/font/wrap configuration
  into Parameters. Do not manufacture handles for hidden widgets.

  **Acceptance tests**

  - No example stores public Node IDs, `UiNodeSet`, or parallel state/result identity.
  - No example imports `NodeStateValue` or uses the old `Node::header`/`tree` constructors.
  - Custom widgets keep the current Widget method signatures.
  - Calculator/demo behavior remains equivalent under deterministic checks.
  - Glow, Vulkan, and WGPU examples compile separately.

- [ ] **P3.2 — Prove local mutation with the file dialog**

  **Problem**

  `FileDialogState::eval` rebuilds its complete UI on ordinary evaluation.

  **Decision needed: No**

  **Target contract or migration**

  Construct the shell once. Inputs/buttons and folder/file list containers deliberately expose state,
  so require `Some` at initialization and retain those typed handles. On refresh, construct row
  state/widget pairs and replace only list `Children`. Consume actions through button/list state.
  Preserve the existing scroll offset across child replacement, then clamp it to the new content
  range during the post-replacement layout. Do not reset it merely because the directory rows were
  refreshed.

  **Acceptance tests**

  - Idle visible/hidden evaluation allocates no nodes/state and changes no topology.
  - Refresh changes only row nodes and explicitly updated state.
  - Refresh uses `try_update_with(new_rows, ...)`; a `Dropped`/`Borrowed` access failure returns the
    complete unmounted replacement vector rather than dropping it through an uninvoked closure.
  - Refresh with a still-valid scroll offset preserves it exactly; shorter or empty replacement
    content clamps it to the nearest valid offset, including zero when no scrolling remains.
  - Removed row handles expire; persistent controls and scroll handles remain live.
  - No root replacement, generated ID, Context editor, or Context token remains.

- [ ] **P3.3 — Align public modules, README, rustdoc, and migration notes**

  **Problem**

  Public documentation currently mixes strong retained state, generated identity, and builder
  projection terminology.

  **Decision needed: No**

  **Target contract or migration**

  Document `Widget`, `WidgetState`, `WidgetParameters`, `WidgetBuilder`, public
  `Container: Widget`, marker `ContainerState`, opaque child visitors, the exact container-only
  layout/input methods, the final opaque ownership rule, optional weak handles, built-in
  state/parameter types, owning `Node`/opaque `Children`, `Disclosure` as the old header/tree
  replacement, the absence of generic node visibility, unified
  `RootHandle`/`RootState`/`RootMutationError` chrome and explicit root destruction, state-local
  events/commands, and traversal-order mutation. Include final custom-container construction through
  `create_container`/`Node::container(OwnedContainer)`, state the fixed `Some`/`None` policy of every
  concrete built-in constructor, and state clearly that `None` changes exposure, not ownership.
  Examples must not imply that callers select exposure through Parameters. Document the fixed
  leaf/container exposure table, the intentional removal of
  arbitrary mounted public-field mutation, exact mounted Row/Grid/Stack/Scroll configuration,
  input-preserving `try_update_with`, policy/span precedence, no root replacement, the one ordered
  input dispatcher and popup-boundary exception, the two-layout frame, the complete removal of
  `ResourceState`/frame results, and the
  framework-recursion exemption from the application reentrancy prohibition.

  **Acceptance tests**

  - Crate docs/README examples compile where practical.
  - `cargo doc` exposes `Container: Widget` without parallel container measure/update/paint methods
    and exposes no private IDs, cells, legacy container adapter traits, or obsolete builders;
    production-source checks confirm that `NodeBehavior` itself was deleted in P2.3.
  - Docs explicitly state that ContextFrame does not lock state and that no Context token exists.
  - Docs distinguish initialization Parameters from mutable State with concrete examples.
  - Root docs distinguish hide from destroy, document `RootHandle` weak ownership, and define
    `RootState` current chrome queries plus pending `take_changed`/`take_submitted` semantics.
  - Crate-root/prelude exports include `RootHandle`, `RootState`, and `RootMutationError`, but not
    `RootChromeContainer`, `RootInteraction`, or framework-private state-transition helpers.
  - Visibility docs distinguish root visibility from Disclosure descendant gating and expose no
    generic node visibility API.

### P4 — Correctness after simplification

- [ ] **P4.0 — Pin dynamic-mutation and frame-pass semantics**

  **Problem**

  Direct state/topology mutation is intentionally traversal-ordered. The runtime must define when
  layout and paint observe successful mutations.

  **Decision needed: No**

  **Target contract or migration**

  Use the settled P2.5 frame sequence: pre-input layout, routed input, update, post-update layout,
  and paint. Document: later nodes in a phase see earlier successful mutations; completed phases do
  not rerun except the scheduled post-update layout; paint-time state changes may become fully
  visible next frame. The current frame presents exactly what its ordered phases observed, with no
  rollback or snapshot promise, and the next ordinary frame must be fully stable.

  **Acceptance tests**

  - Update-time intrinsic-size changes affect same-frame post-update layout and paint.
  - Mutation of a later/earlier sibling produces the documented distinct outcome.
  - Same-container topology mutation is `Borrowed`; a not-currently-borrowed subtree mutation is
    safe and deterministic.
  - Custom-render callback mutation follows the same contract without panic.
  - A state-access closure that completes before `ContextFrame::render_ui` remains valid; invoking
    any retained root traversal for the same Context from inside that closure is an explicitly
    unsupported reentrant call and receives the documented diagnostic without adding a frame gate.
  - Framework-authorized child measurement, layout, and visitor recursion remains valid while a
    parent runtime's state borrow is active and is not diagnosed as application reentrancy.

- [ ] **P4.1 — Correct scroll/disclosure edge cases on the single-owner representation**

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

- [ ] **P4.2 — Replace pseudo-unbounded measurement and unify axis allocation**

  **Problem**

  `UiRuntime::measure_auto_size` currently passes height `10_000`, and flexible row/grid policies can
  manufacture intrinsic size from that probe. Column, Row, Grid, and Stack also use separate
  measurement/allocation logic, so preferred size, track allocation, spans, and overflow can
  disagree. Window chrome calculations currently leak into `ui_node` measurement.

  **Decision needed: No — the approved correctness scope selects explicit constraints and shared
  axis primitives**

  **Target contract or migration**

  Introduce private `AxisConstraint::{Bounded(i32), Unbounded}` and `MeasureConstraints`. Under an
  unbounded axis, `Auto`, `Fraction`, `Weight`, and `Remainder` contribute their content-derived
  intrinsic minimum; only `Fixed` forces its fixed extent. At the unchanged public
  `Widget::measure(Dimensioni)` boundary, adapt `Unbounded` to the already documented non-positive
  “use intrinsic/defaults” input (`0`) rather than a large numeric sentinel. Built-in containers and
  `Children::measure_child` preserve the explicit internal constraint mode while recursing.

  Implement shared internal `intrinsic_tracks` and `allocate_tracks` primitives for linear and grid
  axes. Row/Grid measurement and layout use the same track list. Grid computes row-major placements
  once, derives per-track intrinsic minima including spans and spacing, keeps fixed tracks fixed,
  and reports overflow when a fixed span cannot satisfy a child rather than silently growing it.
  Apply the mounted configuration and child-policy precedence defined above exactly once.

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
    fixed-track overflow rule; measurement and layout share one placement list.
  - Column/Row/Grid/Stack use the common axis primitives without a general constraint-solver layer.
  - Generic `src/ui_node` traversal contains no title/close/resize/window-option formula; the one
    private root-chrome module and `root_chrome_geometry` helper determine title height, all hit and
    paint rectangles, minimum size, and client/outer conversion for window/dialog/popup variants.
  - Deep nonzero-origin transform/clip tests pass without `parent_of` or recursive parent-transform
    reconstruction.

### P5 — Cleanup and measured optimization

- [ ] **P5.0 — Remove obsolete ownership, identity, and mutation machinery**

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
  the unconditional middle layout, pseudo-unbounded numeric probes, independent container axis
  solvers, all generic result construction, and every result sink.

  **Acceptance tests**

  - Repository searches find none of the named obsolete production symbols or patterns.
  - No public API keeps removed state persistently alive or requires Context for state access.
  - Examples/tests/docs use the single parameter/state/builder model and contain no old disclosure,
    generic-visibility, frame-result, or mutable-child-borrow surface.
  - Searches find no `UiRuntime::interaction_for`, raw-input retained update context, third tree
    layout, context-level scroll-delta accessor/storage, `10_000` measurement probe, or root-node
    replacement entry point.
  - Structural type/line counts demonstrate net removal rather than another compatibility layer.

- [ ] **P5.1 — Repeat allocation, phase, and code-structure baselines**

  **Problem**

  The design deliberately keeps one state allocation per widget/container and checked dynamic
  borrows. Their cost must be compared with removed rebuilding and adapters.

  **Decision needed: No**

  **Target contract or migration**

  Repeat P0 measurements for allocations, semantic nodes, idle file dialog, phase counts, state
  access, associated-state weak upgrades, and code/type counts. Full traversal remains the
  completion baseline.

  **Acceptance tests**

  - Idle file dialog has zero tree/state allocation.
  - One-child scroll area has two semantic nodes; each Context root adds exactly one private chrome
    container around the application tree and no synthetic title/close/resize nodes.
  - Root reconstruction, reconciliation, erased-handle cloning, and Context state validation are
    structurally absent.
  - An ordinary frame records two tree-layout phases; each explicit runtime method uses at most one
    associated-state upgrade when it needs state and no identity/topology/dispatch weak upgrade.
    Window-manager boundary operations may likewise upgrade `WindowEntry.root_state` once per
    operation; measure/layout/runtime access then uses the root container's ordinary associated-state
    upgrade rather than a registry lookup.
  - No material frame-time regression is accepted without a recorded cause and follow-up decision.

- [ ] **P5.2 — Keep full traversal unless a separate measured optimization plan is approved**

  **Problem**

  Dirty flags, indexes, and retained paint would reintroduce lifecycle complexity if added without
  evidence.

  **Decision needed: No — full direct traversal is the migration baseline**

  **Target contract or migration**

  Complete this migration with full direct traversal. If P5.1 demonstrates a concrete budget miss,
  do not extend this plan or delay its correctness definition: create and approve a separate plan
  covering state mutation, topology, style/font/atlas changes, resize, scroll, custom rendering,
  and removal.

  **Acceptance tests**

  - P5.1 records the full-traversal baseline and any measured budget miss.
  - No cache/index/dirty bit lands without lifecycle and invalidation tests.
  - Completion does not claim incremental rendering when full traversal remains.

## Final release validation gate

P1.0 and P1.3 establish the opaque ownership boundary before the bulk migration. This gate does not
schedule a second construction rewrite; it audits that the P1 boundary survived P2-P5, that no raw
insertion bypass appeared, and that the complete migration is safe to expose. P0-P5 remain internal
until this validation passes.

### R0.0 — Validate the associated state owner at every retained insertion

**Problem**

The final release must prove that no later migration item bypassed P1's framework-created
`Rc<RefCell<State>>` owner or reintroduced raw-box insertion. Rust cannot force an arbitrary
implementation of a public runtime trait to consult a particular field, but the P1 insertion
boundary must continue to pair every retained runtime with the framework-created allocation for its
builder's associated `State`.

**Decision needed: No — validation of P1.0/P1.3 before the first externally visible release**

**Validation contract**

Audit the normative “Final builder, owner, and stable state-handle contracts,” “Containers own
children in their state,” “Owning node and internal identity,” and root-chrome ownership sections.
P1.0/P1.3 must still be their only implementation owners: the non-overridable factories allocate the
state, the opaque wrapper keeps it alive, the runtime receives the matching weak handle, and only
`OwnedWidget`/`OwnedContainer` enter `Node`. The audit must not invent a second construction contract
or overstate what Rust can prove about whether an arbitrary custom runtime actually consults its
factory-supplied weak handle.

**Acceptance tests**

- Compile-fail tests prove `Node::widget(Box::new(...))`,
  `Node::custom_render(Box::new(...), renderer)`, and `Node::container(Box::new(...))` cannot insert
  a raw runtime.
- A compile-fail test proves downstream code cannot directly construct `OwnedWidget`/
  `OwnedContainer` or insert a runtime without the factory-created associated-state keep-alive.
- `WidgetBuilder` cannot override the state-allocation/ownership factory.
- `WidgetBuilder::Parameters` and `ContainerBuilder::Parameters` both implement the same public
  `WidgetParameters` marker; no unbounded container-only parameter role contradicts the four-role
  contract.
- `initialize` consumes Parameters exactly once and returns `(State, Builder)`; a compile/runtime
  test moves non-cloneable child Nodes into Column and `RootState` without cloning, loss, or a fifth
  public construction role, then calls `build` exactly once with the factory-supplied weak handle.
- `EXPOSE_STATE` is fixed by each widget/container builder implementation; no Parameters value or
  call-site option can change it.
- The `Some` and `None` paths each leave exactly one persistent strong state owner in the opaque
  retained wrapper; only `Some` returns an application weak handle.
- The runtime receives a weak handle to the exact allocation retained by its wrapper; conformance
  tests prove every built-in runtime uses that supplied handle, and custom-builder docs state the
  same correctness contract.
- Each runtime method that needs associated state upgrades its supplied weak handle at most once for
  that invocation; traversal performs no identity/topology/dispatch weak upgrade.
- Each window-manager boundary operation that needs root data upgrades its private `RootState` weak
  clone at most once and scopes the borrow before invoking retained tree traversal.
- Downstream code cannot call `WidgetStateHandle::from_owner`; cloning a
  `WidgetStateHandle<NonCloneState>` remains supported.
- `try_update_with` remains part of the final handle API and returns owned input unchanged on failed
  upgrade/borrow.
- Dropping an uninserted or removed `OwnedWidget`/`OwnedContainer` makes all weak handles report
  `Dropped` after active access closures release temporary upgrades.
- Equivalent compile-time and lifetime coverage exists for external custom containers through
  public `ContainerBuilder`/`OwnedContainer`, while built-in factories still return a completed
  `Node` for convenience.
- Root creation uses `create_container`/`OwnedContainer` for `RootChromeContainer`; dropping the
  `WindowEntry` expires both the `RootHandle` state capability and every descendant capability.
- The public `Widget` trait has exactly its P0-frozen runtime methods, including
  `Widget::update -> ()`, and remains distinct from `WidgetState`.
- Public `Container` still has `Widget` as its supertrait; `OwnedContainer` delegates inherited
  `Widget` calls once and the opaque child visitors work when the runtime consumes state through the
  factory-supplied weak handle. No raw child callback reappears.
- `StateKeepAlive` remains private and method-free, with no downcast, phase, Context, frame-token, or
  write-lock behavior.
- Final documentation/examples explain that the opaque wrapper is the retained widget/state owner,
  that `None` affects exposure only, and that custom builders must use the supplied state handle.
  They contain no raw-box insertion signature or staging migration path as a supported alternative.
- Crate-root, `retained`, and prelude exports expose owning `Node`, `OwnedWidget`, `OwnedContainer`,
  the framework factories, marker `ContainerState`, opaque visitors, and no old header/tree `Node`.
- Production-source/API searches find no raw-box overload on `Node`, no overridable
  `WidgetBuilder::create`, and no second supported construction path.
- Allocation and phase measurements remain within the P5.1 baseline; any regression gets a separate
  evidence-backed decision rather than weakening ownership.

## Known defect matrix and ownership

| Defect | Current cause | Simplification first | Fix/verification item |
|---|---|---|---|
| Removed widget state remains alive | Strong application handle | Runtime strong owner/application weak handle split | P0.2/P1.0 |
| Same state can be projected twice | Strong handle cloning | Unique `OwnedWidget` + Node | P1.2/P1.3 |
| Typed mutation and runtime behavior are conflated | Built-in struct implements both roles | Parameters/State/Builder split | P0.1/P1.1 |
| Widget/container common phases have parallel dispatch | Private `NodeBehavior` plus forwarding `WidgetNode` | Public `Container: Widget` and one supertrait dispatch path | P0.1/P2.3 |
| Reentrant state access can panic | Infallible `RefCell` borrow | Checked per-cell handles | P0.2/P1.0 |
| Application stores handle plus node ID | Generic result lookup | State-local events/commands | P0.4/P1.1 |
| Typed change/submit delivery is underspecified after leaf results disappear | Generic per-frame flags hid per-widget API and persistence rules | Exact state-local counters, recording points, and silent-setter contract | P0.4/P1.1 |
| File dialog rebuilds every evaluation | Projection is only topology API | State-owned Children | P0.3/P3.2 |
| List interaction follows position | Builder ordinal identity | Persistent unique nodes/state | P1.3/P3.2 |
| Scroll offset resets/recreates | Root replacement/synthetic state | Persistent scroll state | P2.2/P3.2 |
| Scroll area has synthetic semantic nodes | Decoration modeled as nodes | One container runtime | P2.2/P4.1 |
| Scroll disable cannot synchronously clear tree-owned capture | State setter has no tree capability | State resets local drag/offset; target sanitization releases capture before routing | P2.2/P2.4 |
| Scroll routing is described as both non-mutating and offset-mutating | Routing and update responsibilities were conflated | Routing decides/queues; unit-returning Widget update mutates state | P2.2/P2.5 |
| Public header/tree `Node` collides with owning `Node` | Old widget was not classified | Reserve `Node`; absorb behavior into Disclosure | P0.6/P1.1/P2.1 |
| Disclosure recreates adapters | Strong handle adapted per phase | One state-owning runtime | P2.1 |
| Whole attached child collections can be swapped | Raw `children_mut`/callback authority | Marker state plus safe inherent ops and opaque visitors | P0.3/P2.3 |
| Stale focus/capture after direct removal | Context no longer receives edit callback | Live-target sanitization | P2.4 |
| Generic node visibility is specified but behaviorally absent | Unused `UiNodeState::visible` field | Remove it; separate root visibility and container descendant gating | P0.6/P2.1/P2.4 |
| Root lifetime has no destruction operation | WindowEntry can only be hidden | Explicit `destroy_root` with immediate unmount/ownership release and active-access-safe final drop | P0.7/P1.4 |
| Root/chrome observation would require a parallel result mechanism | Chrome is special-cased outside retained typed state | One private `RootChromeContainer` with public `RootState` and `RootHandle` | P0.7/P1.4/P3.0 |
| Built-in container callers should not repeat the runtime-to-node wrapping step | Factory returns raw `Box<dyn Container>` | Built-in factory returns `Node`; custom insertion accepts `OwnedContainer` | P0.3/P1.3 |
| Container rollout previously assigned every built-in to the atomic foundation batch | Foundation and concrete migrations were conflated | Atomic Node/visitor/Column/Disclosure slice; Row/Grid/Stack and ScrollArea extend it later | P1.3/P2.0/P2.1/P2.2 |
| Runtime phases cannot return `StateAccessError` during render reentrancy | Phase signatures have no state-access error channel | Forbid rendering inside state-access closures; local diagnostic only | P0.2/P4.0 |
| A failed state access can drop a moved, unmounted node before insertion | Plain closure capture gives the handle no way to return ownership | `try_update_with` validates access first and returns the exact input on failure | P0.2/P1.3 |
| `Widget::update` generic results have no runtime consumer after result removal | `ResourceState` historically fed `FrameResults` | Change update to return `()` and remove the complete generic result family | P0.1/P0.4/P5.0 |
| Raw input and routed events can disagree | Retained update derives interaction separately with `interaction_for` | One ordered dispatcher; routed events for ordinary interaction and one explicit popup boundary decision | P2.5/P5.0 |
| Context scroll accessors duplicate the routed event batch | Scroll delta is copied into phase context state | Remove update/paint context accessors; inspect localized events only | P2.5/P5.0 |
| A frame lays out the retained tree three times | Pre-route, pre-update, and post-update layout are separate | One pre-input and one post-update tree layout | P2.5/P4.0 |
| Auto-size uses a `10_000` pseudo-unbounded probe | Public dimensions encode both bounds and intrinsic requests | Private explicit constraints adapt unbounded axes to the documented public `0` convention | P4.2/P5.0 |
| Row/Grid measurement can disagree with allocation | Independent policy and track solvers | Shared intrinsic/allocation axis primitives and one Grid placement list | P2.0/P4.2 |
| Mounted container configuration is underspecified | Old public fields and builder reconstruction blur initialization and state | Exact Row/Grid/Stack/Scroll state setters; immutable node policy/span | P1.1/P2.0/P2.2 |
| A custom container can visit different child collections by phase | Safe Rust cannot relate two opaque visitor calls across methods | Document one-authoritative-`Children` conformance obligation and test examples | P0.1/P2.3 |
| A visitor can omit or repeat its one child submission | The visitor API has no `Result` channel | Framework invariant panic with container/type/phase diagnostic | P0.1/P2.3 |
| A mounted root cannot change widget type in place | Stable `RootId` and root replacement have conflicting lifetime semantics | Persistent container root for dynamic content; otherwise destroy/recreate with a new ID | P0.7/P1.4/P3.0 |
| Optional `CustomRenderKey` has no public retained-node construction path | Builder removal drops key injection | Backend-typed `Node::custom_render` constructor | P1.2 |
| Raw boxed runtime does not prove an associated state is retained | A raw insertion boundary can bypass the builder allocation | Opaque framework-created retained owner | P1.0/P1.3/R0.0 |
| Late opaque-owner hardening would cause a second downstream API migration | Final ownership introduced after bulk migration | Establish Owned types before bulk conversion | P1.0/P1.3 |

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

Prefer deterministic assertions for state, consumed events, geometry, event order, focus/capture,
operation counts, weak liveness, and allocation counts. Screenshots may supplement but not replace
them.

Update together:

- crate-level retained UI documentation and prelude;
- rustdoc for all four widget roles, public `Container: Widget`, marker `ContainerState`, opaque
  child visitors, the exact container-only scoped context methods, owner/handle access,
  `OwnedWidget`/`OwnedContainer`, owning `Node`/opaque `Children`, Disclosure, visibility boundaries,
  exact mounted container configuration, input-preserving access, and unified
  `RootHandle`/`RootState`/`RootMutationError` chrome and lifecycle;
- README construction, state mutation, event consumption, dynamic list, custom-render construction,
  render-reentrancy precondition, scrolling, and destruction examples;
- simple, calculator, demo, custom drawing, texture, and backend examples;
- file-dialog implementation/tests;
- migration notes explaining the combined-widget split, old header/tree `Node` to Disclosure,
  bound-free weak-handle cloning, fallible access, direct topology, no-reparent enforcement,
  failure-preserving owned input, traversal order, root hide versus destroy, unsupported root
  replacement, typed root chrome and removal of all frame results, unit-returning widget update,
  absence of generic node
  visibility, immutable node placement, the ordered input dispatcher/popup-boundary exception, the
  two-layout frame, explicit
  intrinsic constraints/shared axis allocation, removal of public widget IDs/results, and the one
  externally visible Owned insertion boundary.

## Suggested implementation sequence

1. Land characterization and establish the final `Widget` signatures, including
   `Widget::update -> ()`.
2. Add parameters/state/builder/owner/optional-handle primitives and migrate one exposed Checkbox
   plus one hidden-state widget end to end.
3. Apply the fixed exposure/mutation table while splitting the remaining built-ins, defining only
   the listed state-local values, events, and commands.
4. Store `OwnedWidget` directly, add the final `Node::widget`/`Node::custom_render` paths, and delete
   erased handle dispatch without exposing a raw insertion boundary.
5. Land the uniquely named owning Node, private runtime IDs, placement methods, marker
   `ContainerState`, constructible opaque Children, opaque traversal visitors, Disclosure, and the
   first state-owned column container as one compile-safe public batch.
6. Convert the other containers with the exact mutable configuration APIs, then establish one
   private `RootChromeContainer` per WindowEntry with `RootHandle`/`RootState`, explicit destruction,
   no application-child replacement, and no frame-result channel.
7. Switch traversal, target sanitization, Disclosure, and scroll area to direct ownership; remove
   generic node visibility, make routed events authoritative, and reduce the frame to two tree
   layouts.
8. Remove public projection/root replacement and migrate one small example completely.
9. Replace pseudo-unbounded probes and independent Row/Grid/container solvers with explicit private
   constraints and shared axis primitives; centralize chrome conversion in the retained root-chrome
   helper shared with the window boundary.
10. Migrate the file dialog early as the dynamic-topology proof, then remaining examples/docs.
11. Delete all obsolete ownership/identity/mutation machinery, repeat baselines, and optimize only
    from evidence.
12. Run R0.0 and the full validation matrix against the P1 opaque ownership boundary, then and only
    then merge/tag/release the migration for downstream consumption.

P1 ownership changes may need one integration branch: weak handles are not valid until the returned
opaque owner retains the strong state, and direct child mutation is not valid until runtime target
use tolerates removal. Keep commits mechanically reviewable, but expose no intermediate raw-box
contract; the externally visible branch must include the final ownership model and documentation.

## Final release completion definition

This is an audit checklist over the Target architecture and protected P0 baseline. If an item
conflicts with its defining section, the defining section controls until the explicit behavior
change process updates both.

The migration is complete when:

- the public `Widget` trait remains the only common phase contract for leaves and containers, and
  its final `update` method returns `()`;
- public `Container: Widget` is implementable by downstream custom containers, adds only
  opaque child visitation/layout/descendant-visibility/special-input behavior, and redeclares none
  of the common `Widget` phases;
- `NodeBehavior`, its implementations/bounds, and any equivalent catch-all runtime adapter are
  absent; private traversal dispatches common phases once through `Widget` and branches to
  `Container` only for container-specific work;
- each requested runtime invocation has one common `Widget` dispatch path; the two required layout
  phases may each invoke measurement and do not imply only one measure call per frame;
- widget/container/chrome update constructs and returns no generic result; typed state events,
  `WidgetUpdateCtx`, and routed-input results are the respective application, focus, and
  capture/consumption mechanisms; `ResourceState` and frame-result APIs are absent;
- `WidgetState`, `WidgetParameters`, and `WidgetBuilder` have distinct data/construction roles, and
  marker `ContainerState` has no child-access methods; both `WidgetBuilder::Parameters` and
  `ContainerBuilder::Parameters` implement `WidgetParameters`;
- each final widget constructor returns `Option<WidgetStateHandle<T>>` plus `OwnedWidget` through
  the framework factory;
- each public built-in container constructor returns `Option<WidgetStateHandle<C>>` plus a completed
  `Node`, while `ContainerBuilder`/`create_container` and public
  `Node::container(OwnedContainer)` support downstream custom containers;
- each concrete widget/container constructor has one fixed, documented `Some` or `None` exposure
  policy chosen by its implementation; no public exposure selector exists;
- `Checkbox`, `Button`, `ListItem`, `ListBox`, `Combo`, `TextBlock`, `ColorSwatch`, `Slider`,
  `Number`, `Textbox`, `TextArea`, and every public dynamic built-in container return `Some`;
  `Custom` and explicitly fixed/internal containers return `None`, and the old widget `Node` is
  retired into exposed `DisclosureState`;
- every `OwnedWidget`/`OwnedContainer` owns the only persistent strong allocation for its builder's
  associated `State`, including when construction returns `None`, and every built-in runtime uses
  the exact weak handle supplied by its factory;
- `Some` returns a weak application state capability while `None` withholds that capability without
  changing the retained wrapper's strong state ownership;
- present application state handles contain only a typed weak state capability, clone without
  requiring `T: Clone`, and use checked closures; `from_owner` is not public at the release boundary;
- `try_update_with` checks upgrade/borrow before committing its owned input and returns that exact
  input with `Dropped` or `Borrowed`; ordinary closure capture is documented as non-recovering;
- state/topology access never checks Context identity, mount state, frame state, or a global lock;
- same-cell conflicts between checked handle operations return `Borrowed`, while unrelated available
  cells can be accessed regardless of `ContextFrame` lifetime;
- state-access closures finish before retained traversal/rendering; reentrant rendering from inside
  a closure is documented as unsupported and is not implemented with a frame/write gate;
- framework-owned nested `Children::measure_child`, `ContainerLayoutCtx::layout_child`, and opaque
  visitor traversal are authorized recursion, not application rendering reentrancy;
- `WidgetStateHandleDyn`, erased handle cloning, and duplicate state dispatch are absent;
- every built-in leaf has explicit Parameters, concrete State, optional handle exposure, runtime
  Widget, and Builder responsibilities;
- any application-observed widget values, events, and commands require an exposed typed state;
  hidden runtime state never leaks through node/result identity;
- every fixed built-in change/submit event uses the specified private saturating count and
  one-occurrence `take_changed`/`take_submitted` API; ordinary programmatic setters are silent and
  Combo alone retains its documented clamp exception;
- `TextboxState::request_focus` remains queued while the textbox is hidden, gated, or
  non-interactive and clears only when `WidgetUpdateCtx::set_focus` reports successful assignment;
- crate-root/prelude `Node` is the only public type with that name, is unique and non-cloneable, and
  owns one `OwnedWidget` or `OwnedContainer`; old `widgets::Node`, `NodeStateValue`, and compatibility
  aliases are absent;
- public `Node::widget(OwnedWidget)` and backend-typed
  `Node::custom_render(OwnedWidget, CustomRenderHandle<B>)` are the complete leaf insertion paths;
  `CustomRenderKey` remains private and registry preflight rejects invalid erased keys;
- `Node::with_policy` and `with_grid_span` are the complete pre-insertion placement surface, and
  `NodeRuntime` has no generic visibility field or mutation API; mounted policy/span mutation is
  unsupported and layout applies the settled slot/span/policy precedence exactly once;
- private process-unique runtime IDs support focus/capture/routing and are never exposed or stored in
  state handles;
- container runtime state owns a private opaque `Children`; application-dynamic containers expose a
  weak checked state handle with only safe inherent operations, while fixed/internal containers may
  return `None`;
- mounted Row widths/item height, Grid tracks, Stack width/height/direction, and Scroll offset/
  enablement have the exact state getters/setters and edge semantics specified above; Column adds no
  local layout configuration, Disclosure exposes expansion, and framing/base options remain
  construction-only;
- disabling ScrollArea synchronously resets state-owned drag/offset, while tree sanitization releases
  tree-owned capture before another event is routed; routing queues but does not apply scroll state
  changes;
- `Children::new`/`Default`/`FromIterator<Node>` support downstream construction, but built-in
  states expose no whole collection and no framework-provided API returns an attached node;
- only framework-created opaque visitors reach container child collections; ordinary callers cannot
  construct an extraction callback, swap collections, or reach direct forward/reverse node
  iteration or any `&mut Node`;
- each container visitor method submits exactly one collection or triggers the specified invariant
  panic diagnostic; downstream containers are documented and tested to submit the same
  authoritative `Children` from immutable and mutable visitor methods;
- `ContainerLayoutCtx` and `ContainerInputCtx` expose exactly the policy/span/layout/geometry and
  full/sub-rectangle routing methods specified above, with deterministic invalid-index, clipping,
  and coordinate behavior;
- successful removal drops nodes rather than returning/detaching them;
- stale runtime targets are validated and sanitized without a registry, pointer, Context token, or
  eager editor callback;
- each WindowEntry owns one persistent tree rooted by exactly one private `RootChromeContainer`;
  that container strongly owns public `RootState` and exactly one immutable application-child slot,
  while WindowEntry retains only a framework-private weak state clone;
- window/dialog/popup creation returns cloneable non-owning `RootHandle`; `handle.id()` addresses
  Context lifecycle/policy operations and `handle.state()` exposes the same checked weak typed-state
  access as widgets/containers;
- `RootHandle`, `RootState`, and `RootMutationError` are exported at crate root/prelude, while the
  chrome container, interaction enum, and framework mutation helpers remain private;
- root reads exist only on `RootState`; Context root setters return `UnknownRoot` or `Borrowed`
  explicitly and keep z-order/backend/transient-target side effects coordinated with the state
  mutation, while front/destroy return `false` for an unknown ID;
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
  generic tree capture owns moving/resizing, and one pure chrome geometry helper supplies all phase,
  compositor, and backend rectangles; only popup outside-hit detection records across the tree
  boundary, while the documented post-tree chrome overlay is rendering-only and reads the same
  `RootState`;
- `ResourceState`, `FrameResults`, `FrameResultGeneration`, `RetainedId`, and all generic/root result
  lookup are absent;
- Disclosure and scroll area each have one direct container runtime and one associated state owner;
  Disclosure owns the old header/tree presentations and collapsed descendants skip every phase
  while remaining owned;
- root visibility and container descendant gating are the only visibility mechanisms; collapsing
  clears descendant transient targets without restoring them on expansion;
- the file dialog changes only row children on directory refresh, preserves then clamps its scroll
  offset to the new content range, and performs no idle tree work;
- focus, capture, input, layout, paint, clipping, scrolling, and custom rendering retain supported
  behavior under deterministic tests;
- raw input enters one ordered dispatcher; routed events are the only ordinary retained interaction
  source and popup outside dismissal is the sole cross-root boundary exception. A frame performs
  exactly one pre-input and one post-update retained-tree layout, with no raw-input
  `interaction_for` path or unconditional middle layout; context-level scroll-delta
  storage/accessors are absent and widgets inspect only their localized routed-event batch;
- retained measurement contains no large pseudo-unbounded sentinel, uses explicit private
  bounded/unbounded constraints and shared axis allocation, and keeps chrome/client conversion in
  the shared private root-chrome helper while carrying transforms/clips directly;
- strong widget handles, public widget Node IDs/all frame results, projection builders, root replacement/state
  transfer, synthetic scroll nodes, Context editors, mount metadata, and all frame/Context state
  locks are gone;
- raw-box insertion, old header/tree Node APIs, generic node visibility, method-bearing
  `ContainerState`, and raw mutable child callbacks are gone;
- docs/examples/tests describe one state-first, framework-owned construction model using
  `OwnedWidget`/`OwnedContainer`; no raw-box surface is documented as supported;
- measurements show zero root reconstruction, zero erased state redispatch, two semantic nodes for a
  one-child scroll area, exactly one internal chrome-container node per root, zero idle file-dialog
  tree allocation, two retained-tree layouts per
  ordinary frame, at most one associated-state weak upgrade per runtime method that needs state, no
  identity/topology/dispatch weak upgrades, and no material regression;
- no incremental traversal or retained-paint cache is added without a separate evidence-backed
  contract.

Meeting this definition, including R0.0 and the final validation pass, completes the migration and
permits its first externally visible release. P0-P5 alone are only an internal integration
milestone and must not be published as a compatibility boundary.
