# Retained UI migration guide

This guide describes the `0.8.0-pre-alpha` retained authoring model and the breaking migration from
the `0.7` and older per-frame tree, generated-identity, and batch-input APIs. The crate root,
`prelude`, and `retained` module expose the same core concepts; renderer integration remains under
`render`. Because this is a pre-alpha development release, the remaining work tracked by
`UI-NODE-REFACTOR-PLAN.md` may still refine this surface before `0.8.0`.

## The final ownership model

There is one retained tree representation:

- `Node` is the unique, non-cloneable owner of one concrete widget or container runtime.
- `Children` is an opaque ordered owner. Public code can insert new unmounted nodes or drop
  attached owners, but cannot detach, inspect, identify, or reparent attached nodes.
- `Context` consumes one root node in `create_window`, `create_dialog`, or `create_popup` and owns
  that complete tree until `destroy_root`.
- `WidgetStateHandle<T>` and `RootHandle` are cloneable weak capabilities. Cloning them never keeps
  mounted state alive; dropping them never removes mounted state.
- Runtime node identity is private. There is no generated public widget ID, public node ID, lookup
  result, or identity stored in a typed state handle.

An attached node cannot move to another parent. Mutate it through its typed state, mutate its
parent's topology in place, or construct a replacement. Root content has no replacement operation:
destroy and recreate the root when a different root owner is required.

## Parameters, state, runtime, and builder

These roles no longer overlap:

| Role | Lifetime and responsibility |
| --- | --- |
| `WidgetParameters` / concrete `*Parameters` | One-shot construction input: initial value, font, options, constraints, and initial children. |
| `WidgetState` / concrete `*State` | Mounted mutable values, topology, commands, and counted semantic events. |
| `Widget` | Concrete retained runtime phases: `measure`, one-event `update`, and observational `paint`. |
| `WidgetStateOwner` | Associates the runtime with the one private state allocation it uniquely owns. |
| `WidgetBuilder` | Maps leaf Parameters to a concrete runtime. |
| `Container: Widget` | Adds only scoped child visitation, layout, descendant gating, local input routing, and capture cleanup. |
| `ContainerBuilder` | Maps container Parameters to a concrete state-owning container runtime. |
| `ContainerState` | Marker for application-facing container state; it grants no generic child access or runtime phases. |

Parameters never select whether a state handle is exposed. Each concrete constructor has a fixed
ordinary return shape:

| Kind | Constructors | Return shape |
| --- | --- | --- |
| Stateful leaves | `Button`, `Checkbox`, `ColorSwatch`, `Combo`, `ListBox`, `ListItem`, `Number`, `Slider`, `TextArea`, `TextBlock`, `Textbox` | `(WidgetStateHandle<ConcreteState>, ConcreteRuntime)` |
| Stateless custom leaf | `Custom` | `Custom` runtime only; its state is `()` |
| Containers | `Column`, `Disclosure`, `Grid`, `Row`, `ScrollArea`, `Stack` | `(WidgetStateHandle<ConcreteState>, Node)` |

Finish an ordinary leaf with `Node::widget(runtime)`. Container convenience constructors already
finish the runtime with `Node::container`. A backend custom-render leaf uses
`Node::custom_render(runtime, callback_handle)`.

```rust
use microui_redux::prelude::*;

let (button_state, button) = Button::create(ButtonParameters::new("Save"));
let button = Node::widget(button).with_policy(Policy::fixed(100, 28));
let (column_state, column) = Column::create(ColumnParameters::new([button]));

// Both handles are weak. The column Node, and later Context, own the runtimes.
assert!(button_state.is_alive());
assert!(column_state.is_alive());
drop(column_state);
assert!(button_state.is_alive());
drop(column);
assert!(!button_state.is_alive());
```

Initialization-only fields are not arbitrary mounted public mutation points. For example, change a
textbox's mounted text through `TextboxState`, but choose its font/options through
`TextboxParameters` before construction. A downstream custom runtime stores any immutable
`FontChoice` or `WidgetOption` fields it needs directly; the temporary `WidgetConfig` façade is
removed.

## Container configuration and topology

Every built-in container state exposes only topology-safe operations. Removal and replacement drop
the removed runtime owners; no operation returns an attached `Node`.

| Container state | Mounted mutable configuration |
| --- | --- |
| `ColumnState` | Ordered membership only. |
| `DisclosureState` | Ordered membership and expanded/collapsed descendant gating. Label, header/tree presentation, and base options are initialization-only. |
| `RowState` | Ordered membership, index-matched width tracks, and shared item-height policy. |
| `GridState` | Ordered membership, Grid-owned child spans, column tracks, and row tracks. |
| `StackState` | Ordered membership, shared item width/height policies, and direction. |
| `ScrollAreaState` | Ordered membership, offset, and scrolling enabled state. Framing is initialization-only; disabling scrolling clears drag state and resets offset. |

Generic `Node::with_policy` controls parent sizing. A `GridItem` span is separate Grid-owned
parent-child metadata and determines cell occupancy; `GridState::set_span` changes that metadata
without replacing or re-identifying the child.

When ownership-bearing mutation can fail, use `try_update_with` so the input survives both an
expired owner and a same-state borrow conflict:

```rust
# use microui_redux::prelude::*;
# fn add(column: &WidgetStateHandle<ColumnState>, node: Node) -> Result<(), Node> {
column.try_update_with(node, |state, node| state.push(node))
# }
```

## Input, update, layout, and paint

`Context` is the only public raw-input owner. Call `mousemove`, `mousedown`, `mouseup`, `scroll`,
`keydown`, `keyup`, `keydown_code`, `keyup_code`, and `text` on it. Calls append without
coalescing, including repeated, zero-valued, and empty-text transitions.

The canonical loop is:

1. Deliver application/resource changes and raw input calls.
2. Call `Context::update_ui(dimensions)`.
3. Observe counted state-local events and perform application state changes.
4. If those changes can affect layout or traversal, call `update_ui(dimensions)` again with the
   now-empty queue to synchronize.
5. Call `Context::frame(info).render_ui()` to paint and submit the commit once.

`update_ui` always performs an initial layout synchronization. It then drains FIFO input; every
event is routed before exactly one full eligible-tree `Widget::update` traversal, and that traversal
is followed by layout before the next event is routed. Held input state for that event is exposed by
`WidgetUpdateCtx::{mouse_buttons, key_modes, key_codes}`. The runtime synthesizes no timer events.

`render_ui` is paint-only. It performs no input routing, semantic update, or layout. A missing,
Context-invalidated, dimension-mismatched, or pending-input commit yields
`RenderError::UiUpdateRequired` before paint and backend acquisition. The old `ResourceState`,
generic frame results, and implicit per-frame result lookup are removed.

A popup is the deliberate cross-root routing exception: an outside pointer press first dismisses
the visible popup and records its submission, then may continue to the root underneath. Ordinary
events target one eligible root.

## Custom widget migration

Replace batch helpers such as `WidgetInputEvents` with direct matching of the one event passed to
`Widget::update`:

```rust
use microui_redux::prelude::*;
use std::{cell::RefCell, rc::Rc};

struct DragState {
    total_x: i32,
}
impl WidgetState for DragState {}

struct DragParameters;
impl WidgetParameters for DragParameters {}

struct DragWidget {
    state: Rc<RefCell<DragState>>,
    opt: WidgetOption,
}

struct DragBuilder;
impl WidgetBuilder for DragBuilder {
    type Parameters = DragParameters;
    type W = DragWidget;

    fn create_widget(_: DragParameters) -> DragWidget {
        DragWidget {
            state: Rc::new(RefCell::new(DragState { total_x: 0 })),
            opt: WidgetOption::NONE,
        }
    }
}

impl WidgetStateOwner for DragWidget {
    type State = DragState;

    fn state_handle(&self) -> WidgetStateHandle<DragState> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Widget for DragWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, _: &Style, _: &AtlasHandle, _: Dimensioni) -> Dimensioni {
        Dimensioni::new(120, 40)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        if let Some(UiInputEvent::MouseDrag { delta, .. }) = input {
            self.state.borrow_mut().total_x += delta.x;
        }
        let _buttons_held_after_this_event = ctx.mouse_buttons();
        let _modifiers_held_after_this_event = ctx.key_modes();
        let _navigation_keys_held_after_this_event = ctx.key_codes();
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Paint observes semantic state; it must not make future behavior/layout depend on paint.
        let _ = (self.state.borrow().total_x, ctx.local_rect());
    }
}
```

`WidgetStateHandle::new(&Rc<RefCell<T>>)` is intentionally the advanced implementor boundary. Pass
the same private allocation used by all runtime phases, keep the runtime as its only persistent
strong owner, and do not expose another strong owner or a state-cloning path that lets two retained
nodes share the allocation.

State access is checked and non-panicking. `try_read`/`try_update` return `None` for either expiry or
a same-cell borrow conflict; `is_alive` can distinguish those cases when needed. Access closures
must finish before update/layout/paint reaches that same state. Independent cells may be nested.
Framework recursion through opaque child visitors is permitted because the framework scopes the
parent borrow and never exposes it to application traversal.

`ContextFrame` does not lock state handles and there is no Context token. If layout-affecting state
changes after the last commit, drop an unsubmitted frame and run `update_ui` again before paint.

## Custom container migration

A custom container implements the ordinary `Widget` phases once, then implements only these
container-specific operations:

- `visit_children` and `visit_children_mut`, each submitting the same authoritative `Children`
  collection exactly once through the opaque visitor;
- `layout`, using indexed `ContainerLayoutCtx` services;
- optional `children_visible` for ancestor-owned descendant gating;
- optional `route_input`, using `ContainerInputCtx` to route the container's own full content area
  or a local sub-rectangle;
- optional `retains_pointer_capture` and `on_pointer_capture_lost` for revocable local capture
  state.

Construct the concrete runtime through `ContainerBuilder::create_container`, obtain its weak handle
through `WidgetStateOwner::state_handle`, and pass it to `Node::container`. Do not add parallel
container measurement, update, paint, option, or focus fields. `Widget::focus_policy` is the single
authoritative focus query; `ContainerInputCtx` deliberately accepts no focus-policy argument.

Capture has three owners:

- the retained tree owns the private captured node identity;
- the captured container owns its local retention predicate and loss cleanup;
- each ancestor owns whether descendants remain eligible through `children_visible`.

The default capture methods are correct for containers without revocable local capture state. A
container with a local drag mode should override both methods consistently. Ancestors cannot claim,
transfer, or clean up descendant capture.

## Roots, visibility, and events

`RootHandle::id()` is the lifecycle key accepted by Context root operations. `RootHandle::state()`
is a weak checked capability for current name/options/rectangle/visibility/chrome interaction and
counted `take_changed`/`take_submitted` events.

`set_root_visible(id, false)` hides a root while preserving its tree and typed state.
`destroy_root(id)` permanently unregisters the root and drops that tree. `RootMutationError`
distinguishes an unknown root from an active root-state borrow.

Root visibility is not generic node visibility. `DisclosureState::{collapse, expand, toggle}` gates
only that Disclosure's descendants while retaining them. There is no public generic node visibility
bit or visibility mutation API.

## Removed compatibility surface

Do not preserve or wrap these concepts in new code:

- public `Input`: enqueue through `Context`;
- `WidgetConfig`: use concrete Parameters or direct immutable fields in a custom runtime;
- public/generated widget or node IDs and result lookup;
- `WidgetTreeBuilder`, legacy container adapters, `NodeBehavior`, and obsolete builders;
- separate header/tree nodes and `NodeStateValue`: use `DisclosureParameters::{header, tree}` and
  `DisclosureState`;
- `WidgetInputEvents` batch helpers: match one `Option<&UiInputEvent>` and read held state from
  `WidgetUpdateCtx`;
- generic frame results and `ResourceState`: consume events/commands from typed state;
- implicit root replacement: mutate typed descendants or destroy and recreate;
- a second focus-policy value passed to container routing: override `Widget::focus_policy`.
