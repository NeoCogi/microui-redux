# Typed events and retained services

Context-owned dispatch for typed retained UI events.

This module connects native events produced by retained widgets and semantic events produced by
Context-owned services to methods on one application state value. Its central rule is
deliberately narrow:

> One [`crate::Context`] owns one [`EventDispatcher`] for one application state type, while each
> retained producer owns and queues the payloads for its own event ports.

There is no public event bus, application-wide message enum, global queue, multicast list,
payload downcast, or independent dispatcher lifetime. The public surface consists of event payload
types, weak [`WidgetEventHandle`] values, state-only [`crate::Context::subscribe`] methods, and
opt-in context-aware [`crate::Context::subscribe_context`] methods. Everything that actually
dispatches events or mutates retained roots is owned by the context.

## Ownership

The context owns both sides of an update transaction: its retained root forest and services
contain the event producers, and its dispatcher contains the handlers that consume them. Neither
a handle nor a subscription keeps a removed producer alive.

```text
Context<B, State>
│
├── owns retained root forest
│      └── owns concrete Widget
│             └── owns Rc<RefCell<WidgetEventPort<E>>>
│                         └── owns WidgetEventPortState<E>
├── owns retained services
│      └── owns Rc<RefCell<WidgetEventPort<E>>>
│                  └── owns WidgetEventPortState<E>
│
└── owns EventDispatcher<State>
       └── owns Vec<Box<dyn EventDispatch<State>>>
                  └── owns Subscription<State, E, Handler>
                             ├── owns Handler
                             └── owns WidgetEventListener<E>
                                        └── Weak ──────────────┐
                                                               │
WidgetEventHandle<E> ───────────────────── Weak ───────────────┘

dispatch boundary ── lends &mut EventContext<'_> ──> opted-in Handler
```

The only strong event-port owner is its retained producer. Consequently:

- cloning a handle does not extend producer lifetime;
- registering a handler does not extend producer lifetime;
- removing a widget immediately destroys its port and queued payloads;
- a Context-owned service source remains alive for the Context lifetime; and
- the next dispatch prunes the now-dead subscription.

`Rc<RefCell<_>>` makes the port shareable inside the retained UI thread while preserving
runtime-checked, short mutable accesses. It also intentionally makes this mechanism neither
`Send` nor `Sync`; widget update and application dispatch belong to the context's owning thread.

## The port state machine

[`WidgetEventPort`] uses an explicit state enum for both subscription state and storage. It has
exactly two stable states:

```text
                        listen / connect
     ┌─────────────────────────────────────────────────┐
     │                                                 v
┌──────────────┐                               ┌────────────────────┐
│ Disconnected │                               │ Connected          │
│              │                               │ pending: FIFO      │
└──────────────┘                               └────────────────────┘
     │      ^                                      │          │
     │      │ listener drop / dispatcher drop      │ emit(E)  │ drain
     │      └──────────────────────────────────────┘          │
     │                                                        │
     └── emit(E): discard                         FIFO <- E   └── FIFO -> handler
```

Connecting installs an empty queue. A second connection fails with
[`SubscribeError::AlreadySubscribed`]. Emission while connected appends the owned payload to
that queue; emission while disconnected is intentionally discarded, so subscribing never
replays historical retained activity. Draining moves the complete queue out and leaves the port
connected with a new empty queue. Dropping the exclusive listener disconnects the port and
clears anything still pending.

### Exclusive-listener policy

The one-listener rule is an architectural policy, not a Rust ownership restriction. A multicast
design could retain one queue reader and invoke several handlers with the same borrowed `&E`, as
C# events invoke a delegate list. This module instead binds each port to one application-state
method. When one event has several consequences, that method composes those effects explicitly.

This policy provides:

- exactly one queue drainer, with no competing-consumer interpretation;
- one explicit place in application state that defines the consequences of a port's event;
- simple connection lifetime: dropping the listener disconnects the entire port, with no
  per-handler unsubscribe or handler-list mutation during dispatch;
- direct movement of owned payloads from one producer queue to one handler, without multicast
  storage, payload cloning, or shared payload wrappers; and
- deterministic FIFO delivery without an additional same-port handler-ordering policy.

Cloning a [`WidgetEventHandle`] therefore clones only the weak capability identifying the port;
it does not create another subscriber slot. Supporting multicast later would require grouping an
ordered handler list behind the port's single queue reader. Merely allowing several listeners to
connect would be incorrect because the first listener to drain the queue would consume the events
before the others observed them.

## Subscription

A normal application subscription follows this path:

```text
typed retained source
     │ changed() / submitted() / completed() / ...
     v
WidgetEventHandle<E>
     │ Context::subscribe(handle, State::method)
     v
EventDispatcher<State>::add
     │
     ├── upgrade the handle's Weak port reference
     ├── connect the port and create its exclusive listener
     ├── retain the concrete method or bound-method handler
     └── erase Subscription<State, E, Handler>
                        as Box<dyn EventDispatch<State>>
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
    submitted: WidgetEventHandle<ButtonSubmitted>,
) {
    context.subscribe_context(submitted, Model::show_popup).unwrap();
}
```

The type erasure applies only to the subscriptions stored in the heterogeneous dispatcher vector:

```text
Subscription<State, ButtonSubmitted, fn(...)> ───────────────┐
Subscription<State, SliderChanged, BoundEventHandler<...>> ──┼──> dyn EventDispatch<State>
Subscription<State, TextboxChanged, fn(...)> ────────────────┘

                             E remains concrete ─────────> EventHandler::handle
```

Payloads are never converted to `Any`, cloned for a subscriber, or wrapped in `Rc`. A port has
one listener, so its owned `E` values move directly from the widget's queue into one concrete
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
             ├── finish framework-controller work
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
            └── listener becomes dead -> subscription pruned on dispatch

context/dispatcher dropped
     └── subscriptions dropped
            └── listeners disconnect live ports and clear their queues
```

Subscribing through an expired handle returns [`SubscribeError::WidgetExpired`]. A live port
permits only one listener and returns [`SubscribeError::AlreadySubscribed`] for a second. There
is no public unsubscribe operation: a context subscription normally lasts until either the
widget or context is dropped.

A subscription takes its whole pending batch out of the port before invoking its handler. This
is what releases the port's `RefCell` borrow and permits a handler to emit another event safely.
It also means that if a handler panics, the unprocessed remainder of that detached batch is
dropped during unwinding; newly emitted events still in live ports remain queued.

## Framework listeners

[`WidgetEventListener`] is also used by context-owned framework controllers such as the file
dialog. Such a controller drains its native controls directly because it already lives inside
the context transaction and does not dispatch into application `State`. The same one-listener,
weak-lifetime, and queue-clearing rules apply.

## Cost model and non-goals

A disconnected port has no `Vec` allocation. Connecting creates an empty batch; for
non-zero-sized payloads, backing storage is allocated on demand by the first emission. `emit` is
amortized O(1), moves the payload once, and performs no dynamic dispatch. Draining moves the
queue buffer into the dispatch batch; when that batch is dropped, its capacity is released
rather than retained by the port.

Each subscription adds one vector entry and one boxed concrete [`Subscription`]. Context-aware
handlers add no queue, controller, or retained owner; their adapter contains only the supplied
function pointer and optional bound value. The handler is statically dispatched inside that
subscription; only [`EventDispatch`] is dynamically dispatched. Dispatcher dispatch first scans
the `S` subscriptions to prune dead widgets, then
visits every subscription once per cascade sweep and invokes handlers once per delivered event.
With `D` delivered events and `R` sweeps, the work is O(`D + S * R`). Ordinary non-cascading
delivery uses one productive sweep followed by one empty sweep.

Queues are not bounded, coalesced, deduplicated, prioritized, persisted, or synchronized across
threads. The cascade limit guards handler feedback during dispatch but is not backpressure for a
widget that produces a very large batch before dispatch begins. These are intentional non-goals
of a synchronous, context-local retained UI event mechanism.

## Event-time root and service coordination

Handlers that only mutate application or widget state continue to use `Context::subscribe` and
`Context::subscribe_with`. A handler that must create, show, hide, move, resize, raise, or destroy a
Context-owned root—or open or cancel a retained file dialog—uses `subscribe_context` or
`subscribe_context_with` and receives a short-lived `EventContext<'_>`:

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
handler updates popup visibility and placement in the triggering input transaction. The demo state
already owns both retained handles: `RootSubmitted::PopupDismissed` closes the combo's shared
semantic state after an outside press or replacement by another popup. Starting a source-window
move or resize is such an outside press, so the popup is closed before any `RootChanged` movement
and requires no geometry-following mechanism. This coordination stays with the composed control
owner instead of leaking popup policy into the base widget or window-manager abstractions. Paint
does no coordination, and application state performs no per-frame popup polling. The `RootChanged`
handler updates the demo window's position and size diagnostics and enforces its minimum size; only
the FPS label remains frame-produced data.

The retained file-dialog service uses the same generic dispatch mechanism. Opening occurs directly
inside a context-aware handler. `WindowManager` owns one typed completion source for the Context
lifetime, so removing a completed dialog controller and root cannot discard the queued terminal
event. The returned `FileDialogSession` is a must-use ownership capability: dropping a pending
session abandons the operation, and a session dropped by an event handler is removed before layout
or the next queued input can route through its modal root. Applications subscribe once and match
completion to the exact live session:

```rust,ignore
impl Model {
    fn file_dialog_completed(&mut self, event: &FileDialogCompleted) {
        let Some(session) = self.dialog_session.as_ref() else {
            return;
        };
        if !event.is_for(session) {
            return;
        }

        match event.status() {
            FileDialogStatus::Accepted(result) => self.open_file(&result.file_path),
            FileDialogStatus::Cancelled => self.note_cancellation(),
        }
        self.dialog_session = None;
    }
}

let completed = ctx.file_dialog_completed();
ctx.subscribe(completed, Model::file_dialog_completed)?;
```

Accepted, in-dialog cancelled, title-closed, and explicitly cancelled sessions each publish one
terminal `FileDialogCompleted` event at a safe dispatch boundary. `FileDialogSession::status`
remains a synchronous compatibility snapshot: it returns `None` while pending and
`Some(FileDialogStatus)` after completion. Retained application flow and `demo-full` do not inspect
it from frame processing.
