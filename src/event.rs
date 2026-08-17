//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

//! Context-owned dispatch for typed widget events.
//!
//! This module connects native events produced by retained widgets to methods on one application
//! state value. Its central rule is deliberately narrow:
//!
//! > One [`crate::Context`] owns one [`EventDispatcher`] for one application state type, while each
//! > widget owns and queues the payloads for its own event ports.
//!
//! There is no public event bus, application-wide message enum, global queue, multicast list,
//! payload downcast, or independent dispatcher lifetime. The public surface consists of event payload
//! types, weak [`WidgetEventHandle`] values, state-only [`crate::Context::subscribe`] methods, and
//! opt-in context-aware [`crate::Context::subscribe_context`] methods. Everything that actually
//! dispatches events or mutates retained roots is owned by the context.
//!
//! # Ownership
//!
//! The context owns both sides of an update transaction: its retained root forest contains the
//! widgets that produce events, and its dispatcher contains the handlers that consume them. Neither a
//! handle nor a subscription keeps a removed widget alive.
//!
//! ```text
//! Context<B, State>
//! │
//! ├── owns retained root forest
//! │      └── owns concrete Widget
//! │             └── owns Rc<RefCell<WidgetEventPort<E>>>
//! │                         └── owns Option<Vec<E>>
//! │
//! └── owns EventDispatcher<State>
//!        └── owns Vec<Box<dyn EventDispatch<State>>>
//!                   └── owns Subscription<State, E, Handler>
//!                              ├── owns Handler
//!                              └── owns WidgetEventListener<E>
//!                                         └── Weak ──────────────┐
//!                                                                │
//! WidgetEventHandle<E> ───────────────────── Weak ───────────────┘
//!
//! dispatch boundary ── lends &mut EventContext<'_> ──> opted-in Handler
//! ```
//!
//! The only strong event-port owner is the widget. Consequently:
//!
//! - cloning a handle does not extend widget lifetime;
//! - registering a handler does not extend widget lifetime;
//! - removing the widget immediately destroys its port and queued payloads; and
//! - the next dispatch prunes the now-dead subscription.
//!
//! `Rc<RefCell<_>>` makes the port shareable inside the retained UI thread while preserving
//! runtime-checked, short mutable accesses. It also intentionally makes this mechanism neither
//! `Send` nor `Sync`; widget update and application dispatch belong to the context's owning thread.
//!
//! # The port state machine
//!
//! [`WidgetEventPort`] uses `Option<Vec<E>>` for both subscription state and storage. It has
//! exactly two stable states:
//!
//! ```text
//!                         listen / connect
//!      ┌─────────────────────────────────────────────────┐
//!      │                                                 v
//! ┌──────────────┐                               ┌────────────────────┐
//! │ disconnected │                               │ connected          │
//! │ pending: None│                               │ pending: Some(FIFO)│
//! └──────────────┘                               └────────────────────┘
//!      │      ^                                      │          │
//!      │      │ listener drop / dispatcher drop      │ emit(E)  │ drain
//!      │      └──────────────────────────────────────┘          │
//!      │                                                        │
//!      └── emit(E): discard                         FIFO <- E   └── FIFO -> handler
//! ```
//!
//! Connecting installs an empty queue. A second connection fails with
//! [`SubscribeError::AlreadySubscribed`]. Emission while connected appends the owned payload to
//! that queue; emission while disconnected is intentionally discarded, so subscribing never
//! replays historical widget activity. Draining moves the complete queue out and leaves the port
//! connected with a new empty queue. Dropping the exclusive listener disconnects the port and
//! clears anything still pending.
//!
//! # Subscription
//!
//! A normal application subscription follows this path:
//!
//! ```text
//! typed widget handle
//!      │ changed() / submitted() / ...
//!      v
//! WidgetEventHandle<E>
//!      │ Context::subscribe(handle, State::method)
//!      v
//! EventDispatcher<State>::add
//!      │
//!      ├── upgrade the handle's Weak port reference
//!      ├── connect the port and create its exclusive listener
//!      ├── retain the concrete method or bound-method handler
//!      └── erase Subscription<State, E, Handler>
//!                         as Box<dyn EventDispatch<State>>
//! ```
//!
//! [`crate::Context::subscribe_with`] follows the same path but captures one application value in
//! the concrete handler. That value belongs to the subscription and lives until the context is
//! dropped or the dead subscription is pruned. [`crate::Context::subscribe_context`] and
//! [`crate::Context::subscribe_context_with`] use parallel typed adapters whose methods also receive
//! a short-lived [`crate::EventContext`]. The dispatcher stores no context borrow; it lends the
//! capability only while invoking the method at the safe boundary below.
//!
//! Application code therefore needs no separate dispatcher value:
//!
//! ```no_run
//! use microui_redux::prelude::*;
//!
//! struct Model {
//!     saves: usize,
//! }
//!
//! impl Model {
//!     fn save(&mut self, _: &ButtonSubmitted) {
//!         self.saves += 1;
//!     }
//! }
//!
//! fn build<B: RendererBackend>(context: &mut Context<B, Model>) -> Node {
//!     let (button, node) = Button::create(ButtonParameters::new("Save"));
//!     context.subscribe(button.submitted(), Model::save).unwrap();
//!     node
//! }
//!
//! fn update<B: RendererBackend>(
//!     context: &mut Context<B, Model>,
//!     model: &mut Model,
//!     dimensions: Dimensioni,
//! ) {
//!     context.update_ui_state(dimensions, model);
//! }
//! ```
//!
//! A handler that must mutate a retained root opts into context access explicitly. The ordinary
//! state-only signature above remains unchanged:
//!
//! ```no_run
//! use microui_redux::prelude::*;
//!
//! struct Model {
//!     popup: RootHandle,
//! }
//!
//! impl Model {
//!     fn show_popup(&mut self, context: &mut EventContext<'_>, _: &ButtonSubmitted) {
//!         // The root remains Context-owned. EventContext is only an exclusive transaction borrow.
//!         context.set_root_visible(self.popup.id(), true).unwrap();
//!     }
//! }
//!
//! fn subscribe<B: RendererBackend>(
//!     context: &mut Context<B, Model>,
//!     submitted: WidgetEventHandle<ButtonSubmitted>,
//! ) {
//!     context.subscribe_context(submitted, Model::show_popup).unwrap();
//! }
//! ```
//!
//! The type erasure applies only to the subscriptions stored in the heterogeneous dispatcher vector:
//!
//! ```text
//! Subscription<State, ButtonSubmitted, fn(...)> ───────────────┐
//! Subscription<State, SliderChanged, BoundEventHandler<...>> ──┼──> dyn EventDispatch<State>
//! Subscription<State, TextboxChanged, fn(...)> ────────────────┘
//!
//!                              E remains concrete ─────────> EventHandler::handle
//! ```
//!
//! Payloads are never converted to `Any`, cloned for a subscriber, or wrapped in `Rc`. A port has
//! one listener, so its owned `E` values move directly from the widget's queue into one concrete
//! handler.
//!
//! # Dispatch boundary
//!
//! Widgets emit while the retained tree is updating. Calling application code synchronously at
//! that point would let it re-enter a tree whose widget cells are still borrowed. Instead,
//! [`crate::Context::update_ui_state`] dispatches only after the complete eligible-tree update has
//! returned and released those borrows:
//!
//! ```text
//! Context input FIFO
//!      │
//!      ├── initial synchronization layout
//!      │       └── dispatch already-pending widget events
//!      │               └── layout again if state handlers ran
//!      │
//!      └── for each raw input event
//!              ├── normalize and route input
//!              ├── update every eligible retained root
//!              │       └── widgets append native payloads to their own ports
//!              ├── finish framework-controller work
//!              ├── dispatch application handlers with &mut State
//!              │       └── opted-in handlers also receive &mut EventContext<'_>
//!              └── commit layout before routing the next raw input event
//! ```
//!
//! This boundary gives handlers exclusive `&mut State` without coupling widgets to `State`. Because
//! the complete tree traversal has ended, the context may also lend its independent `WindowManager`
//! field through [`crate::EventContext`] without aliasing a widget borrow or the event dispatcher.
//! State-driven widget, root, or topology changes are therefore reflected by layout before the next
//! input event is hit-tested.
//!
//! # Ordering and cascades
//!
//! Ordering is exact within one port and deliberately local across ports:
//!
//! - one port preserves emission order with `Vec<E>`;
//! - the dispatcher visits ports in subscription order;
//! - each visit drains that port's complete currently-pending batch; and
//! - the dispatcher repeats full subscription-order sweeps until no handler receives an event.
//!
//! For subscriptions `[A, B]`, consider this initial state and the events emitted by handlers:
//!
//! ```text
//! before sweep 1: A = [a1, a2]       B = [b1]
//!
//! sweep 1, drain A: handle a1, a2
//!                    ├── a1 emits a3 to A   (A was already drained)
//!                    └── a2 emits b2 to B   (B has not been drained yet)
//!
//! sweep 1, drain B: handle b1, b2
//! sweep 2, drain A: handle a3
//! sweep 2, drain B: empty
//! sweep 3: both empty, stop
//!
//! observed order: a1, a2, b1, b2, a3
//! ```
//!
//! Thus an event emitted into a later subscription can run in the current sweep, while one emitted
//! into the current or an earlier subscription runs in the next sweep. There is intentionally no
//! global chronology across independent ports; providing one would require moving ordering state
//! back into a shared queue. Application logic that requires a total order should express that
//! order inside one state method or one event type.
//!
//! Repeated sweeps also make finite event cascades complete within the same context update. The
//! dispatch transaction panics after [`MAX_EVENT_DISPATCHES`] deliveries to terminate an accidental
//! feedback loop instead of hanging the UI thread.
//!
//! # Lifetime and failure behavior
//!
//! ```text
//! widget removed
//!      └── port dropped
//!             ├── queued events dropped
//!             ├── handles become expired
//!             └── listener becomes dead -> subscription pruned on dispatch
//!
//! context/dispatcher dropped
//!      └── subscriptions dropped
//!             └── listeners disconnect live ports and clear their queues
//! ```
//!
//! Subscribing through an expired handle returns [`SubscribeError::WidgetExpired`]. A live port
//! permits only one listener and returns [`SubscribeError::AlreadySubscribed`] for a second. There
//! is no public unsubscribe operation: a context subscription normally lasts until either the
//! widget or context is dropped.
//!
//! A subscription takes its whole pending batch out of the port before invoking its handler. This
//! is what releases the port's `RefCell` borrow and permits a handler to emit another event safely.
//! It also means that if a handler panics, the unprocessed remainder of that detached batch is
//! dropped during unwinding; newly emitted events still in live ports remain queued.
//!
//! # Framework listeners
//!
//! [`WidgetEventListener`] is also used by context-owned framework controllers such as the file
//! dialog. Such a controller drains its native controls directly because it already lives inside
//! the context transaction and does not dispatch into application `State`. The same one-listener,
//! weak-lifetime, and queue-clearing rules apply.
//!
//! # Cost model and non-goals
//!
//! A disconnected port has no `Vec` allocation. Connecting creates an empty batch; for
//! non-zero-sized payloads, backing storage is allocated on demand by the first emission. `emit` is
//! amortized O(1), moves the payload once, and performs no dynamic dispatch. Draining moves the
//! queue buffer into the dispatch batch; when that batch is dropped, its capacity is released
//! rather than retained by the port.
//!
//! Each subscription adds one vector entry and one boxed concrete [`Subscription`]. Context-aware
//! handlers add no queue, controller, or retained owner; their adapter contains only the supplied
//! function pointer and optional bound value. The handler is statically dispatched inside that
//! subscription; only [`EventDispatch`] is dynamically dispatched. Dispatcher dispatch first scans
//! the `S` subscriptions to prune dead widgets, then
//! visits every subscription once per cascade sweep and invokes handlers once per delivered event.
//! With `D` delivered events and `R` sweeps, the work is O(`D + S * R`). Ordinary non-cascading
//! delivery uses one productive sweep followed by one empty sweep.
//!
//! Queues are not bounded, coalesced, deduplicated, prioritized, persisted, or synchronized across
//! threads. The cascade limit guards handler feedback during dispatch but is not backpressure for a
//! widget that produces a very large batch before dispatch begins. These are intentional non-goals
//! of a synchronous, context-local retained UI event mechanism.

use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use crate::Widget;

/// Maximum number of native events handled in one dispatch transaction.
///
/// The limit applies across all subscriptions and all cascade sweeps initiated by one call to
/// [`EventDispatcher::dispatch_with_context`]. It is a correctness guard against mutually emitting handlers, not a
/// normal flow-control mechanism.
const MAX_EVENT_DISPATCHES: usize = 1_000_000;

/// Marker implemented by every native semantic event payload emitted by a widget.
///
/// An event is an owned snapshot of the semantic fact the widget is reporting. For example, a
/// textbox change event contains the text as it existed when the change occurred rather than a
/// reference back into the mutable widget. This lets the port retain the event until the safe
/// context dispatch boundary.
///
/// The `'static` bound is required because subscriptions of different concrete event types coexist
/// behind the dispatcher's `EventDispatch` boundary. It does not require event values to be `Clone`,
/// `Send`, or `Sync`.
pub trait WidgetEvent: 'static {}

/// A concrete retained widget that owns one typed native event source.
///
/// A widget may implement this trait more than once with different `E` types, such as separate
/// `Changed` and `Submitted` ports. The implementation returns a weak capability rather than the
/// port itself: callers can subscribe and inspect liveness, but cannot emit or drain widget events.
pub trait TypedWidget<E: WidgetEvent>: Widget {
    /// Returns a weak capability for this widget's `E` event source.
    ///
    /// Repeated calls identify the same widget-owned port. Each returned handle is weak and does
    /// not affect the lifetime of either the widget or port.
    fn event(&self) -> WidgetEventHandle<E>;
}

/// One widget-owned typed event queue and its connection state.
///
/// `None` means that no listener exists and emissions are discarded. `Some(queue)` means exactly
/// one listener is connected. Keeping those states in one field makes it impossible for the
/// connection flag and queue lifetime to disagree.
pub(crate) struct WidgetEventPort<E: WidgetEvent> {
    pending: Option<Vec<E>>,
}

impl<E: WidgetEvent> WidgetEventPort<E> {
    /// Creates a disconnected port.
    ///
    /// Widgets construct their ports before they are mounted or subscribed, so the initial state
    /// deliberately has no queue allocation and records no events.
    pub(crate) fn new() -> Self {
        Self { pending: None }
    }

    /// Appends an event when a listener is connected, otherwise discards it.
    ///
    /// The widget calls this only after it has committed the semantic state described by `event`.
    /// Delivery is deferred; this method never invokes application code.
    pub(crate) fn emit(&mut self, event: E) {
        if let Some(pending) = &mut self.pending {
            pending.push(event);
        }
    }

    /// Transitions a disconnected port to a connected port with an empty queue.
    ///
    /// The exclusive connection enforces the design's one-port/one-state-method rule.
    fn connect(&mut self) -> Result<(), SubscribeError> {
        if self.pending.is_some() {
            return Err(SubscribeError::AlreadySubscribed);
        }
        self.pending = Some(Vec::new());
        Ok(())
    }

    /// Transitions a connected port to the disconnected state and drops queued payloads.
    ///
    /// Calling this for an already disconnected port is harmless. In normal operation it is called
    /// by the exclusive listener's [`Drop`] implementation.
    fn disconnect(&mut self) {
        self.pending = None;
    }

    /// Moves the complete pending batch out while preserving the connected state.
    ///
    /// Returning an owned queue ensures the port's `RefCell` borrow ends before a handler runs. An
    /// event emitted recursively by that handler therefore enters the new empty queue and is
    /// observed by a subsequent dispatcher sweep.
    fn drain(&mut self) -> Vec<E> {
        self.pending.as_mut().map(std::mem::take).unwrap_or_default()
    }
}

/// The non-owning reference used by both public handles and exclusive listeners.
type WeakWidgetEventPort<E> = Weak<RefCell<WidgetEventPort<E>>>;

/// Weak, typed capability identifying one native event source owned by a retained widget.
///
/// Holding or cloning this value does not keep the widget or its pending events alive.
/// The event type parameter prevents connecting a handler for one payload type to another port at
/// compile time.
pub struct WidgetEventHandle<E: WidgetEvent> {
    port: WeakWidgetEventPort<E>,
}

impl<E: WidgetEvent> WidgetEventHandle<E> {
    /// Creates a weak handle to a live widget-owned port.
    pub(crate) fn new(port: &Rc<RefCell<WidgetEventPort<E>>>) -> Self {
        Self { port: Rc::downgrade(port) }
    }

    /// Creates the same observable state as a handle whose widget has already been removed.
    ///
    /// Projection through an unavailable typed widget handle uses this value so event access stays
    /// weak and fallible without manufacturing an owner.
    pub(crate) fn expired() -> Self {
        Self { port: Weak::new() }
    }

    /// Returns whether the concrete widget still owns this event source.
    ///
    /// This is a liveness observation, not an ownership claim. The handle remains weak, and a later
    /// attempt to subscribe can still report [`SubscribeError::WidgetExpired`] if the widget is
    /// removed between operations.
    pub fn is_alive(&self) -> bool {
        self.port.strong_count() != 0
    }

    /// Consumes this handle and connects the port's exclusive queue listener.
    ///
    /// Consuming one cloned handle does not invalidate other clones; they continue to report port
    /// liveness but cannot establish a second listener while this one exists.
    pub(crate) fn listen(self) -> Result<WidgetEventListener<E>, SubscribeError> {
        let Some(port) = self.port.upgrade() else {
            return Err(SubscribeError::WidgetExpired);
        };
        port.borrow_mut().connect()?;
        Ok(WidgetEventListener { port: Rc::downgrade(&port) })
    }
}

impl<W: Widget + 'static> crate::TypedWidgetHandle<W> {
    /// Projects a concrete widget-owned event port through this weak typed widget handle.
    ///
    /// Access is intentionally brief: [`TypedWidget::event`] clones only the weak port capability,
    /// and the concrete widget borrow is released before the caller can subscribe. If the widget
    /// cell is currently unavailable or already dead, the result behaves as an expired event
    /// handle.
    pub(crate) fn widget_event<E: WidgetEvent>(&self) -> WidgetEventHandle<E>
    where
        W: TypedWidget<E>,
    {
        self.try_read(TypedWidget::event).unwrap_or_else(WidgetEventHandle::expired)
    }
}

impl<E: WidgetEvent> Clone for WidgetEventHandle<E> {
    /// Clones only the weak capability; this neither clones an event nor retains a widget.
    fn clone(&self) -> Self {
        Self { port: self.port.clone() }
    }
}

impl<E: WidgetEvent> fmt::Debug for WidgetEventHandle<E> {
    /// Reports current liveness without exposing port identity or queued payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WidgetEventHandle")
            .field("alive", &(self.port.strong_count() != 0))
            .finish_non_exhaustive()
    }
}

/// Failure to subscribe an application-state method to a widget event.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SubscribeError {
    /// The widget that owns the event port has already been removed or was unavailable when its
    /// typed handle projected the event capability.
    WidgetExpired,
    /// The event port already has its exclusive listener.
    ///
    /// In normal application code this means the port is already subscribed through its owning
    /// context. Framework controllers use the same exclusivity rule for directly drained ports.
    AlreadySubscribed,
}

impl fmt::Display for SubscribeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WidgetExpired => "widget event owner has expired",
            Self::AlreadySubscribed => "widget event already has a subscriber",
        })
    }
}

impl std::error::Error for SubscribeError {}

/// Exclusive weak queue reader retained by either a state subscription or framework controller.
///
/// Creation and destruction of this value are the connection lifetime of a port. It remains weak
/// so the subscription side cannot keep a removed widget alive. There is intentionally no `Clone`
/// implementation: duplicating a listener would violate the single-consumer queue contract.
pub(crate) struct WidgetEventListener<E: WidgetEvent> {
    port: WeakWidgetEventPort<E>,
}

impl<E: WidgetEvent> WidgetEventListener<E> {
    /// Takes all events currently pending on a live port in FIFO order.
    ///
    /// A dead or disconnected port produces an empty queue. The listener itself remains usable as
    /// a liveness record until its owner prunes or drops it.
    pub(crate) fn drain(&self) -> Vec<E> {
        self.port.upgrade().map(|port| port.borrow_mut().drain()).unwrap_or_default()
    }

    /// Returns whether the widget still strongly owns the port.
    fn is_alive(&self) -> bool {
        self.port.strong_count() != 0
    }
}

impl<E: WidgetEvent> Drop for WidgetEventListener<E> {
    /// Disconnects a still-live port and clears its queue.
    ///
    /// No action is necessary if widget removal already destroyed the port.
    fn drop(&mut self) {
        if let Some(port) = self.port.upgrade() {
            port.borrow_mut().disconnect();
        }
    }
}

/// Typed invocation contract between one event payload and the dispatch target.
///
/// Implementations may carry immutable registration context, but invocation mutates only the
/// supplied application `Target`. The trait therefore uses `&self`, making stateless handler
/// behavior part of the event-system contract instead of accepting an arbitrary stateful closure.
trait EventHandler<Target, E: WidgetEvent> {
    /// Applies one concrete event payload to the dispatch target and optional UI mutation façade.
    fn handle(&self, target: &mut Target, context: &mut crate::EventContext<'_>, event: &E);
}

/// A plain `fn(&mut Target, &E)` is already a complete typed event handler.
impl<Target, E: WidgetEvent> EventHandler<Target, E> for fn(&mut Target, &E) {
    fn handle(&self, target: &mut Target, _context: &mut crate::EventContext<'_>, event: &E) {
        // State-only handlers deliberately ignore the context capability, preserving their existing
        // signature and keeping context access opt-in at registration.
        self(target, event);
    }
}

/// Explicit adapter for a target method registered with one immutable bound value.
///
/// This is the concrete representation created by `subscribe_with`; no callable closure is stored.
struct BoundEventHandler<Target, BoundContext, E: WidgetEvent> {
    context: BoundContext,
    method: fn(&mut Target, &BoundContext, &E),
}

impl<Target, BoundContext, E: WidgetEvent> EventHandler<Target, E> for BoundEventHandler<Target, BoundContext, E> {
    fn handle(&self, target: &mut Target, _event_context: &mut crate::EventContext<'_>, event: &E) {
        // Bound state-only handlers retain the same invocation order and do not receive UI access.
        (self.method)(target, &self.context, event);
    }
}

/// Explicit adapter for a state method that opts into context-owned UI mutation access.
///
/// A higher-ranked function pointer accepts whichever short lifetime belongs to the current
/// dispatch boundary. The handler cannot store that borrow in `'static` application state.
struct ContextEventHandler<Target, E: WidgetEvent> {
    /// Concrete application method invoked for each owned event payload.
    method: for<'a> fn(&mut Target, &mut crate::EventContext<'a>, &E),
}

impl<Target, E: WidgetEvent> EventHandler<Target, E> for ContextEventHandler<Target, E> {
    /// Invokes the method with the exclusive capability lent by the current dispatch transaction.
    fn handle(&self, target: &mut Target, context: &mut crate::EventContext<'_>, event: &E) {
        // No Context or WindowManager reference is stored in the subscription; the borrow enters and
        // leaves entirely within this call.
        (self.method)(target, context, event);
    }
}

/// Explicit adapter for a context-aware target method with one subscription-owned bound value.
struct BoundContextEventHandler<Target, BoundContext, E: WidgetEvent> {
    /// Immutable application value retained for the subscription lifetime.
    context: BoundContext,
    /// Concrete method receiving state, the bound value, safe UI access, and the event payload.
    method: for<'a> fn(&mut Target, &BoundContext, &mut crate::EventContext<'a>, &E),
}

impl<Target, BoundContext, E: WidgetEvent> EventHandler<Target, E> for BoundContextEventHandler<Target, BoundContext, E> {
    /// Invokes the context-aware method without erasing its event or bound-value types.
    fn handle(&self, target: &mut Target, event_context: &mut crate::EventContext<'_>, event: &E) {
        // Keep argument order consistent with Context::subscribe_context_with.
        (self.method)(target, &self.context, event_context, event);
    }
}

/// Object-safe boundary allowing one dispatcher to store heterogeneous event subscriptions.
///
/// `Target` remains common to the whole vector; the implementation retains each concrete event and
/// handler type. This is the module's only dynamic dispatch boundary.
trait EventDispatch<Target> {
    /// Reports whether the concrete widget still owns the subscribed port.
    fn is_alive(&self) -> bool;
    /// Drains one port batch into `target` and returns the number of delivered payloads.
    fn dispatch(&self, target: &mut Target, context: &mut crate::EventContext<'_>) -> usize;
}

/// Concrete binding between one typed port listener and one typed target handler.
///
/// `Handler` is either the method pointer supplied to `subscribe` or the explicit
/// [`BoundEventHandler`] created by `subscribe_with`. `PhantomData` records the handler's `Target`
/// relationship even though the target value is borrowed only when dispatch runs.
struct Subscription<Target, E: WidgetEvent, Handler> {
    listener: WidgetEventListener<E>,
    handler: Handler,
    target: PhantomData<fn(&mut Target)>,
}

impl<Target, E, Handler> EventDispatch<Target> for Subscription<Target, E, Handler>
where
    E: WidgetEvent,
    Handler: EventHandler<Target, E>,
{
    /// Delegates widget lifetime observation to the weak listener.
    fn is_alive(&self) -> bool {
        self.listener.is_alive()
    }

    /// Detaches one complete batch, then invokes the concrete handler in port FIFO order.
    ///
    /// Detaching before the first invocation is essential: handler code may cause widgets to emit
    /// without colliding with a live mutable borrow of this port.
    fn dispatch(&self, target: &mut Target, context: &mut crate::EventContext<'_>) -> usize {
        let events = self.listener.drain();
        let count = events.len();
        for event in events {
            // The port borrow ended when drain returned, so a context-aware handler may safely
            // mutate or destroy retained roots before the next payload is delivered.
            self.handler.handle(target, context, &event);
        }
        count
    }
}

/// The sole typed event dispatcher owned by one UI context.
///
/// Vector position is subscription order and therefore the deterministic cross-port sweep order.
/// Each boxed element retains concrete payload and handler types behind [`EventDispatch`]. This
/// type is crate-private because its lifetime must not diverge from the context that owns the
/// corresponding retained widget forest.
pub(crate) struct EventDispatcher<Target> {
    subscriptions: Vec<Box<dyn EventDispatch<Target>>>,
}

impl<Target: 'static> EventDispatcher<Target> {
    /// Creates the empty dispatcher embedded in a new context.
    pub(crate) fn new() -> Self {
        Self { subscriptions: Vec::new() }
    }

    /// Registers a target method as the sole consumer of one event port.
    ///
    /// The function pointer implements [`EventHandler`] directly, requiring no adapter or closure
    /// allocation beyond the subscription's existing trait-object allocation.
    pub(crate) fn subscribe<E: WidgetEvent>(&mut self, event: WidgetEventHandle<E>, method: fn(&mut Target, &E)) -> Result<(), SubscribeError> {
        self.add(event, method)
    }

    /// Registers a target method together with one subscription-owned application value.
    ///
    /// [`BoundEventHandler`] stores both the value and the typed method explicitly. `BoundContext`
    /// means the bound value's type and is unrelated to [`crate::Context`].
    pub(crate) fn subscribe_with<E: WidgetEvent, BoundContext: 'static>(
        &mut self,
        event: WidgetEventHandle<E>,
        context: BoundContext,
        method: fn(&mut Target, &BoundContext, &E),
    ) -> Result<(), SubscribeError> {
        self.add(event, BoundEventHandler { context, method })
    }

    /// Registers a target method that receives the dispatch transaction's UI mutation capability.
    pub(crate) fn subscribe_context<E: WidgetEvent>(
        &mut self,
        event: WidgetEventHandle<E>,
        method: for<'a> fn(&mut Target, &mut crate::EventContext<'a>, &E),
    ) -> Result<(), SubscribeError> {
        // Wrap the higher-ranked function pointer explicitly so ordinary state-only handlers retain
        // their original adapter and public signature.
        self.add(event, ContextEventHandler { method })
    }

    /// Registers a context-aware target method with one subscription-owned immutable value.
    pub(crate) fn subscribe_context_with<E: WidgetEvent, BoundContext: 'static>(
        &mut self,
        event: WidgetEventHandle<E>,
        context: BoundContext,
        method: for<'a> fn(&mut Target, &BoundContext, &mut crate::EventContext<'a>, &E),
    ) -> Result<(), SubscribeError> {
        // Store the typed bound value beside the typed function pointer; no closure or payload
        // downcast is introduced.
        self.add(event, BoundContextEventHandler { context, method })
    }

    /// Connects the port and appends its concrete subscription in sweep order.
    ///
    /// The fallible connection is completed before `Vec::push`; a failed subscription therefore
    /// leaves the dispatcher unchanged.
    fn add<E: WidgetEvent, Handler: EventHandler<Target, E> + 'static>(&mut self, event: WidgetEventHandle<E>, handler: Handler) -> Result<(), SubscribeError> {
        self.subscriptions.push(Box::new(Subscription {
            listener: event.listen()?,
            handler,
            target: PhantomData,
        }));
        Ok(())
    }

    /// Delivers all pending events and finite cascades into the dispatch target.
    ///
    /// Dead widget subscriptions are removed first. The remaining subscriptions are swept in
    /// vector order until one complete sweep delivers nothing. The return value is `true` when at
    /// least one event was delivered; the context uses that result at its initial synchronization
    /// boundary to decide whether target-handler effects require another layout commit.
    ///
    /// The cumulative count is checked after every drained port batch. Arithmetic overflow and a
    /// transaction exceeding [`MAX_EVENT_DISPATCHES`] both panic because either indicates a broken
    /// event feedback loop rather than recoverable input.
    pub(crate) fn dispatch_with_context(&mut self, target: &mut Target, context: &mut crate::EventContext<'_>) -> bool {
        // Prune subscriptions whose weak event ports expired before lending UI mutation access to
        // any application method.
        self.subscriptions.retain(|subscription| subscription.is_alive());

        let mut dispatched = 0usize;
        loop {
            let before = dispatched;
            for subscription in &self.subscriptions {
                dispatched = dispatched
                    .checked_add(subscription.dispatch(target, context))
                    .expect("widget event dispatch count overflowed");
                assert!(
                    dispatched <= MAX_EVENT_DISPATCHES,
                    "widget event cascade exceeded {MAX_EVENT_DISPATCHES} native events in one input transaction"
                );
            }
            if dispatched == before {
                break;
            }
        }
        dispatched != 0
    }

    /// Delivers pending events with an otherwise unused context capability in focused unit tests.
    ///
    /// Production dispatch always uses [`Self::dispatch_with_context`] and the live context-owned
    /// window manager. Tests for isolated widget ports do not exercise root mutation, so constructing
    /// an empty manager here keeps their scope narrow without weakening the production signature.
    #[cfg(test)]
    pub(crate) fn dispatch(&mut self, target: &mut Target) -> bool {
        // The temporary manager is owned for exactly this dispatch and cannot affect a real context.
        let mut window_manager = crate::window_manager::WindowManager::new(crate::Style::default());
        let mut context = crate::EventContext::new(&mut window_manager);
        self.dispatch_with_context(target, &mut context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl WidgetEvent for i32 {}

    #[derive(Default)]
    struct State {
        values: Vec<i32>,
        source: Option<Rc<RefCell<WidgetEventPort<i32>>>>,
    }

    impl State {
        fn record_and_cascade(&mut self, event: &i32) {
            self.values.push(*event);
            if *event < 10 {
                self.source.as_ref().unwrap().borrow_mut().emit(event + 10);
            }
        }

        fn record_with_offset(&mut self, offset: &i32, event: &i32) {
            self.values.push(offset + event);
        }
    }

    #[test]
    fn port_queues_native_events_until_the_dispatcher_dispatches() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let mut dispatcher = EventDispatcher::new();
        dispatcher.subscribe(WidgetEventHandle::new(&owner), State::record_and_cascade).unwrap();

        owner.borrow_mut().emit(3);
        owner.borrow_mut().emit(4);
        let mut state = State {
            source: Some(Rc::clone(&owner)),
            ..State::default()
        };
        assert!(state.values.is_empty());
        assert!(dispatcher.dispatch(&mut state));
        assert_eq!(state.values, [3, 4, 13, 14]);
    }

    #[test]
    fn bound_subscriber_receives_registration_context() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let mut dispatcher = EventDispatcher::new();
        dispatcher
            .subscribe_with(WidgetEventHandle::new(&owner), 40, State::record_with_offset)
            .unwrap();

        owner.borrow_mut().emit(2);
        let mut state = State::default();
        assert!(dispatcher.dispatch(&mut state));
        assert_eq!(state.values, [42]);
    }

    #[test]
    fn one_port_accepts_only_one_context_subscription() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let event = WidgetEventHandle::new(&owner);
        let mut dispatcher = EventDispatcher::new();
        dispatcher.subscribe(event.clone(), State::record_and_cascade).unwrap();

        assert_eq!(dispatcher.subscribe(event, State::record_and_cascade), Err(SubscribeError::AlreadySubscribed));
    }

    #[test]
    fn dropping_the_context_dispatcher_disconnects_and_clears_its_ports() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let event = WidgetEventHandle::new(&owner);
        let mut dispatcher = EventDispatcher::new();
        dispatcher.subscribe(event.clone(), State::record_and_cascade).unwrap();

        owner.borrow_mut().emit(1);
        drop(dispatcher);
        owner.borrow_mut().emit(2);
        assert!(owner.borrow().pending.is_none());

        let mut replacement = EventDispatcher::new();
        replacement.subscribe(event, State::record_and_cascade).unwrap();
    }

    #[test]
    fn expired_subscriptions_are_pruned_during_dispatch() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let mut dispatcher = EventDispatcher::new();
        dispatcher.subscribe(WidgetEventHandle::new(&owner), State::record_and_cascade).unwrap();
        drop(owner);

        assert!(!dispatcher.dispatch(&mut State::default()));
        assert!(dispatcher.subscriptions.is_empty());
    }

    #[test]
    fn ports_discard_events_without_a_listener() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        owner.borrow_mut().emit(1);

        let mut dispatcher = EventDispatcher::new();
        dispatcher.subscribe(WidgetEventHandle::new(&owner), State::record_and_cascade).unwrap();
        assert!(!dispatcher.dispatch(&mut State::default()));
    }
}
