# Typed events and application-owned components

Strongly typed semantic dispatch for retained widgets, application state, and reusable components.

This module connects events produced by retained widgets and application-owned components to
methods on one application state value. Its central rule remains deliberately narrow:

> One widget-event dispatcher owns subscriptions for exactly one concrete target type, while each
> retained producer owns and queues the payloads for its own event ports.

There is no public event bus, application-wide message enum, global queue, multicast list,
payload downcast, or public standalone dispatcher lifetime. The public surface consists of event
payload types, weak [`WidgetEventPortHandle`] values, state-only [`crate::Context::subscribe`] methods,
and opt-in context-aware [`crate::Context::subscribe_context`] methods. Context owns the
application widget-event dispatcher.

## Input routing versus widget-event dispatch

The retained input router and [`WidgetEventDispatcher`] occupy consecutive but separate stages:

```text
raw pointer / keyboard / text input
        │
        v
WindowManager chooses the eligible root
        │
        v
UiRuntime lends its tree and committed transform to InputRouter
        │
        v
InputRouter routes input to one retained node
        │
        v
Widget::update mutates widget state and may emit WidgetEvent
        │
        v
WidgetEventDispatcher invokes the subscribed application-state method
```

Each retained `UiRuntime` owns one `InputRouter`, but routing state is no longer mixed into the
runtime's measurement, layout, update, paint, or metrics state. `UiRuntime` explicitly lends the
router its authoritative tree and committed root transform. The router owns hit testing, clipping,
focus, hover, capture, bubbling, coordinate localization, and the sole raw event staged for the
subsequent widget update.

Widget-event dispatch knows none of those concepts: it drains typed semantic payloads such as
`ButtonSubmitted` only after retained traversal has released every widget borrow. Keeping the two
owners and names distinct makes that borrow-safe boundary explicit.

## Ownership

Context owns the retained root forest and the widget-event dispatcher containing its state handlers.
Application state may own reusable components and their semantic event sources. Neither a handle
nor a subscription keeps a removed producer alive.

```text
Context<B, State>
│
├── owns retained root forest
│      ├── owns concrete application Widget
│      │      └── owns Rc<RefCell<WidgetEventPort<E>>>
│
└── owns WidgetEventDispatcher<State>
       └── owns Vec<Box<dyn WidgetEventDispatch<State>>>
                  └── owns Subscription<State, E, Handler>
                             ├── owns Handler
                             └── owns Weak port reference ─────┐
                                                               │
WidgetEventPortHandle<E> ──────────────── Weak ────────────────┘

dispatch boundary ── lends &mut EventContext<'_> ──> opted-in Handler

Application State
├── owns FileDialog
│      ├── holds weak handles into its ordinary Context-owned dialog root
│      ├── owns shared dynamic-row event ports
│      └── owns WidgetEventPort<FileDialogCompleted>
└── owns WindowMenu<Command>
       ├── holds weak handles into its Context-owned window and popup roots
       ├── owns the authoritative menu specification
       └── owns WidgetEventPort<MenuInvoked<Command>>
```

The only strong event-port owner is its retained producer. Consequently:

- cloning a handle does not extend producer lifetime;
- registering a handler does not extend producer lifetime;
- removing a widget immediately destroys its port and queued payloads;
- an application-owned component source remains alive for the component lifetime; and
- the next dispatch prunes the now-dead subscription.

`Rc<RefCell<_>>` makes the port shareable inside the retained UI thread while preserving
runtime-checked, short mutable accesses. It also intentionally makes this mechanism neither
`Send` nor `Sync`; widget update and application dispatch belong to the context's owning thread.

## The port state machine

[`WidgetEventPort`] uses an explicit state enum for both subscription state and storage. It has
exactly two stable states:

```text
                       subscribe / connect
     ┌─────────────────────────────────────────────────┐
     │                                                 v
┌──────────────┐                               ┌────────────────────┐
│ Disconnected │                               │ Connected          │
│              │                               │ pending: FIFO      │
└──────────────┘                               └────────────────────┘
     │      ^                                      │          │
     │      │ subscription / dispatcher drop       │ emit(E)  │ drain
     │      └──────────────────────────────────────┘          │
     │                                                        │
     └── emit(E): discard                         FIFO <- E   └── FIFO -> handler
```

Connecting installs an empty queue. A second connection fails with
[`SubscribeError::AlreadySubscribed`]. Emission while connected appends the owned payload to
that queue; emission while disconnected is intentionally discarded, so subscribing never
replays historical retained activity. Draining moves the complete queue out and leaves the port
connected with a new empty queue. Dropping the exclusive subscription disconnects the port and
clears anything still pending.

### Exclusive-subscription policy

The one-subscription rule is an architectural policy, not a Rust ownership restriction. A
multicast design could retain one queue reader and invoke several handlers with the same borrowed
`&E`, as C# events invoke a delegate list. This module instead binds each port to one
application-state method. When one event has several consequences, that method composes those
effects explicitly.

This policy provides:

- exactly one subscription drains the queue, with no competing-consumer interpretation;
- one explicit place in application state that defines the consequences of a port's event;
- simple connection lifetime: dropping the subscription disconnects the entire port, with no
  per-handler unsubscribe or handler-list mutation during dispatch;
- direct movement of owned payloads from one producer queue to one handler, without multicast
  storage, payload cloning, or shared payload wrappers; and
- deterministic FIFO delivery without an additional same-port handler-ordering policy.

Cloning a [`WidgetEventPortHandle`] therefore clones only the weak capability identifying the port;
it does not create another subscriber slot. Supporting multicast later would require grouping an
ordered handler list inside the port's single subscription. Merely allowing several subscriptions
to connect would be incorrect because the first one to drain the queue would consume the events
before the others observed them.

## Subscription

A normal application subscription follows this path:

```text
typed retained source
     │ changed() / submitted() / completed() / ...
     v
WidgetEventPortHandle<E>
     │ Context::subscribe(port, State::method)
     v
WidgetEventDispatcher<State>::add
     │
     ├── upgrade the handle's Weak port reference
     ├── connect and retain the Weak port reference
     ├── retain the concrete method or bound-method handler
     └── erase Subscription<State, E, Handler>
                        as Box<dyn WidgetEventDispatch<State>>
```

[`crate::Context::subscribe_with`] follows the same path but captures one application value in
the concrete handler. That value belongs to the subscription and lives until the context is
dropped or the dead subscription is pruned. [`crate::Context::subscribe_context`] and
[`crate::Context::subscribe_context_with`] use parallel typed adapters whose methods also receive
a short-lived [`crate::EventContext`]. The dispatcher stores no context borrow; it lends the
capability only while invoking the method at the safe boundary below.

Application code therefore needs no separate dispatcher value:

```no_run
use microui_redux::prelude::*;

struct Model {
    saves: usize,
}

impl Model {
    fn save(&mut self, _: &ButtonSubmitted) {
        self.saves += 1;
    }
}

fn build<B: RendererBackend>(context: &mut Context<B, Model>) -> Node {
    let (button, node) = Button::create(ButtonParameters::new("Save"));
    context.subscribe(button.submitted(), Model::save).unwrap();
    node
}

fn update<B: RendererBackend>(
    context: &mut Context<B, Model>,
    model: &mut Model,
    dimensions: Dimensioni,
) {
    context.update_ui_state(dimensions, model);
}
```

A handler that must mutate a retained root opts into context access explicitly. The ordinary
state-only signature above remains unchanged:

```no_run
use microui_redux::prelude::*;

struct Model {
    popup: RootHandle,
}

impl Model {
    fn show_popup(&mut self, context: &mut EventContext<'_>, _: &ButtonSubmitted) {
        // The root remains Context-owned. EventContext is only an exclusive transaction borrow.
        context.set_root_visible(self.popup.id(), true).unwrap();
    }
}

fn subscribe<B: RendererBackend>(
    context: &mut Context<B, Model>,
    submitted: WidgetEventPortHandle<ButtonSubmitted>,
) {
    context.subscribe_context(submitted, Model::show_popup).unwrap();
}
```

The type erasure applies only to the subscriptions stored in the heterogeneous dispatcher vector:

```text
Subscription<State, ButtonSubmitted, fn(...)> ───────────────┐
Subscription<State, SliderChanged, BoundEventHandler<...>> ──┼──> dyn WidgetEventDispatch<State>
Subscription<State, TextboxChanged, fn(...)> ────────────────┘

                             E remains concrete ─────────> EventHandler::handle
```

Payloads are never converted to `Any`, cloned for a subscriber, or wrapped in `Rc`. A port has one
subscription, so its owned `E` values move directly from the widget's queue into one concrete
handler.

## Dispatch boundary

Widgets emit while the retained tree is updating. Calling application code synchronously at
that point would let it re-enter a tree whose widget cells are still borrowed. Instead,
[`crate::Context::update_ui_state`] dispatches only after the complete eligible-tree update has
returned and released those borrows:

```text
Context input FIFO
     │
     ├── initial synchronization layout
     │       └── dispatch already-pending widget events
     │               └── layout again if state handlers ran
     │
     └── for each raw input event
             ├── normalize and route input
             ├── update every eligible retained root
             │       └── widgets append native payloads to their own ports
             ├── dispatch application handlers with &mut State
             │       └── opted-in handlers also receive &mut EventContext<'_>
             └── commit layout before routing the next raw input event
```

This boundary gives handlers exclusive `&mut State` without coupling widgets to `State`. Because
the complete tree traversal has ended, the context may also lend its independent `WindowManager`
field through [`crate::EventContext`] without aliasing a widget borrow or the event dispatcher.
State-driven widget, root, or topology changes are therefore reflected by layout before the next
input event is hit-tested.

## Ordering and cascades

Ordering is exact within one port and deliberately local across ports:

- one port preserves emission order with `Vec<E>`;
- the dispatcher visits ports in subscription order;
- each visit drains that port's complete currently-pending batch; and
- the dispatcher repeats full subscription-order sweeps until no handler receives an event.

For subscriptions `[A, B]`, consider this initial state and the events emitted by handlers:

```text
before sweep 1: A = [a1, a2]       B = [b1]

sweep 1, drain A: handle a1, a2
                   ├── a1 emits a3 to A   (A was already drained)
                   └── a2 emits b2 to B   (B has not been drained yet)

sweep 1, drain B: handle b1, b2
sweep 2, drain A: handle a3
sweep 2, drain B: empty
sweep 3: both empty, stop

observed order: a1, a2, b1, b2, a3
```

Thus an event emitted into a later subscription can run in the current sweep, while one emitted
into the current or an earlier subscription runs in the next sweep. There is intentionally no
global chronology across independent ports; providing one would require moving ordering state
back into a shared queue. Application logic that requires a total order should express that
order inside one state method or one event type.

Repeated sweeps also make finite event cascades complete within the same context update. The
dispatch transaction panics after [`MAX_EVENT_DISPATCHES`] deliveries to terminate an accidental
feedback loop instead of hanging the UI thread.

## Lifetime and failure behavior

```text
widget removed
     └── port dropped
            ├── queued events dropped
            ├── handles become expired
            └── subscription's Weak expires -> subscription pruned on dispatch

context/dispatcher dropped
     └── subscriptions dropped
            └── live ports disconnected and their queues cleared
```

Subscribing through an expired handle returns [`SubscribeError::WidgetExpired`]. A live port
permits only one subscription and returns [`SubscribeError::AlreadySubscribed`] for a second. There
is no public unsubscribe operation: a context subscription normally lasts until either the
widget or context is dropped.

A subscription takes its whole pending batch out of the port before invoking its handler. This
is what releases the port's `RefCell` borrow and permits a handler to emit another event safely.
It also means that if a handler panics, the unprocessed remainder of that detached batch is
dropped during unwinding; newly emitted events still in live ports remain queued.

## Application-owned component binding

A reusable component can bind its private control sources to methods on the ordinary application
target by storing an accessor function with each subscription. `FileDialog::new` uses this pattern:
the application supplies `fn(&mut State) -> &mut FileDialog`, and the component registers its button,
textbox, list-item, and root sources through `Context::subscribe_with` or
`Context::subscribe_context_with`. Dispatch borrows the application state, follows the accessor to
the component, and invokes its behavior at the same safe boundary as every other application
handler.

The window manager does not dispatch, construct, settle, or otherwise recognize a file dialog. It
only owns the ordinary retained modal root created by the component:

```text
retained FileDialog controls
        │
        v
WidgetEventDispatcher<ApplicationState>
        │
        v
application-owned FileDialog
        │
        v
FileDialogCompleted source ──> WidgetEventDispatcher<ApplicationState>
```

## Cost model and non-goals

A disconnected port has no `Vec` allocation. Connecting creates an empty batch; for
non-zero-sized payloads, backing storage is allocated on demand by the first emission. `emit` is
amortized O(1), moves the payload once, and performs no dynamic dispatch. Draining moves the
queue buffer into the dispatch batch; when that batch is dropped, its capacity is released
rather than retained by the port.

Each subscription adds one vector entry and one boxed concrete [`Subscription`]. Constructing an
application-owned `FileDialog` adds one ordinary hidden root and one subscription for each stable
internal source; contexts that do not construct one pay none of those costs. Dynamic folder and file
rows share stable ports, so refreshing or reopening does not rebuild subscriptions. Context-aware
handlers add no queue or retained owner; their adapter contains only the supplied function pointer
and optional bound value. The handler is statically dispatched inside that subscription; only
[`WidgetEventDispatch`] is dynamically dispatched. The dispatcher first scans its `S` subscriptions to
prune dead widgets, then
visits every subscription once per cascade sweep and invokes handlers once per delivered event.
With `D` delivered events and `R` sweeps, the work is O(`D + S * R`). Ordinary non-cascading
delivery uses one productive sweep followed by one empty sweep.

Queues are not bounded, coalesced, deduplicated, prioritized, persisted, or synchronized across
threads. The cascade limit guards handler feedback during dispatch but is not backpressure for a
widget that produces a very large batch before dispatch begins. These are intentional non-goals
of a synchronous, context-local retained UI event mechanism.

## Event-time root and component coordination

Handlers that only mutate application or widget state continue to use `Context::subscribe` and
`Context::subscribe_with`. A handler that must create, show, hide, move, resize, raise, or destroy a
Context-owned root uses `subscribe_context` or `subscribe_context_with` and receives a short-lived
`EventContext<'_>`:

```rust,ignore
impl Model {
    fn show_popup(
        &mut self,
        event_context: &mut EventContext<'_>,
        _: &ButtonSubmitted,
    ) {
        event_context
            .set_root_visible(self.popup.id(), true)
            .expect("popup root must remain registered");
    }
}

ctx.subscribe_context(open_button.submitted(), Model::show_popup)?;
```

`EventContext` owns nothing. It is an exclusive borrow of the Context-owned `WindowManager`, lent
only after the complete retained-tree update has released its widget borrows and returned before
the following layout. Rust therefore prevents a handler from retaining it, and the same generic
root operations work for windows, dialogs, and popups without a `PopupController`, overlay
registry, per-control command enum, or second root lifetime model.

The full demo composes `Combo` and its popup root entirely through typed events. `ComboSubmitted`
carries the screen-space anchor from the update that routed the header click, so its context-aware
handler calls `show_popup_at` to update popup visibility and placement atomically in the triggering
input transaction. The demo state
already owns both retained handles: `RootSubmitted::PopupDismissed` closes the combo's shared
semantic state after an outside press or replacement by another popup. Starting a source-window
move or resize is such an outside press, so the popup is closed before any `RootChanged` movement
and requires no geometry-following mechanism. This coordination stays with the composed control
owner instead of leaking popup policy into the base widget or window-manager abstractions. Paint
does no coordination, and application state performs no per-frame popup polling. The `RootChanged`
handler updates the demo window's position and size diagnostics and enforces its minimum size; only
the FPS label remains frame-produced data.

`FileDialog` remains a reusable crate component while its instance and behavior live application-side.
The application constructs it with an accessor into its model, stores the returned value there, and
subscribes to that instance's completion source. Opening can occur directly inside a context-aware
application handler: it resets request-specific state and shows the existing ordinary dialog root.
Acceptance or cancellation hides the root again while preserving the component, ports, and static
widgets. Multiple component instances are independent and participate in the generic modal stack.

`WindowMenu<Command>` uses the same application-owned component binding for a different root
composition. It mounts a persistent bar in one ordinary window, reuses a second popup root for the
active menu panel, consumes generic popup-dismissal events internally, and publishes only the typed
`MenuInvoked<Command>` result to application state. See the [menu guide](MENUS.md) for construction,
state mutation, and current keyboard-navigation scope.

```rust,ignore
impl Model {
    fn file_dialog_mut(state: &mut Self) -> &mut FileDialog {
        &mut state.file_dialog
    }

    fn show_file_dialog(
        &mut self,
        event_context: &mut EventContext<'_>,
        _: &ButtonSubmitted,
    ) {
        if !self.file_dialog.is_open() {
            self.file_dialog
                .open_from_event(event_context, FileDialogRequest::default());
        }
    }

    fn file_dialog_completed(&mut self, event: &FileDialogCompleted) {
        match event.status() {
            FileDialogStatus::Accepted(result) => self.open_file(&result.file_path),
            FileDialogStatus::Cancelled => self.note_cancellation(),
        }
    }
}

let file_dialog = FileDialog::new(&mut ctx, Model::file_dialog_mut);
let completed = file_dialog.completed();
ctx.subscribe(completed, Model::file_dialog_completed)?;
let mut model = Model { file_dialog };
```

Accepted, in-dialog cancelled, title-closed, and explicitly cancelled activations each publish one
terminal `FileDialogCompleted` event at a safe dispatch boundary. `FileDialog::is_open` reports the
component's current activation without any frame-time polling requirement.
