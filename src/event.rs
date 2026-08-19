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

#![doc = include_str!("../docs/EVENTS.md")]

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

/// Marker implemented by every typed event payload emitted by retained UI.
///
/// An event is an owned snapshot of the semantic or lifecycle fact its producer is reporting. For
/// example, a textbox change contains the text as it existed when the change occurred rather than a
/// reference into the mutable widget. This lets the port hold the payload until a safe context
/// dispatch boundary. Context-owned retained services use the same contract for
/// lifecycle events such as [`crate::FileDialogCompleted`]; no service-specific behavior enters the
/// dispatcher.
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

/// One retained-producer-owned typed event queue and its connection state.
///
/// The queue exists only in `Connected`, so "disconnected" and "connected but empty" remain
/// distinct without a boolean whose value must agree with a separate collection.
pub(crate) enum WidgetEventPort<E: WidgetEvent> {
    /// No listener exists; emitted events are discarded.
    Disconnected,
    /// Exactly one listener exists and owns the right to drain this FIFO.
    Connected { pending: Vec<E> },
}

impl<E: WidgetEvent> WidgetEventPort<E> {
    /// Creates a disconnected port.
    ///
    /// Widgets construct their ports before they are mounted or subscribed, so the initial state
    /// deliberately has no queue allocation and records no events.
    pub(crate) fn new() -> Self {
        Self::Disconnected
    }

    /// Appends an event when a listener is connected, otherwise discards it.
    ///
    /// The widget calls this only after committing the semantic or lifecycle state described by
    /// `event`. Delivery is deferred; this method never invokes application code.
    pub(crate) fn emit(&mut self, event: E) {
        if let Self::Connected { pending } = self {
            pending.push(event);
        }
    }

    /// Transitions a disconnected port to a connected port with an empty queue.
    ///
    /// The exclusive connection enforces the design's one-port/one-state-method rule.
    fn connect(&mut self) -> Result<(), SubscribeError> {
        if matches!(self, Self::Connected { .. }) {
            return Err(SubscribeError::AlreadySubscribed);
        }
        *self = Self::Connected { pending: Vec::new() };
        Ok(())
    }

    /// Transitions a connected port to the disconnected state and drops queued payloads.
    ///
    /// Calling this for an already disconnected port is harmless. In normal operation it is called
    /// by the exclusive listener's [`Drop`] implementation.
    fn disconnect(&mut self) {
        *self = Self::Disconnected;
    }

    /// Moves the complete pending batch out while preserving the connected state.
    ///
    /// Returning an owned queue ensures the port's `RefCell` borrow ends before a handler runs. An
    /// event emitted recursively by that handler therefore enters the new empty queue and is
    /// observed by a subsequent dispatcher sweep.
    fn drain(&mut self) -> Vec<E> {
        match self {
            Self::Disconnected => Vec::new(),
            Self::Connected { pending } => std::mem::take(pending),
        }
    }
}

/// The non-owning reference used by both public handles and exclusive listeners.
type WeakWidgetEventPort<E> = Weak<RefCell<WidgetEventPort<E>>>;

/// Weak, typed capability identifying one event source owned by retained UI.
///
/// Holding or cloning this value does not keep the widget or its pending events alive.
/// The event type parameter prevents connecting a handler for one payload type to another port at
/// compile time.
pub struct WidgetEventHandle<E: WidgetEvent> {
    port: WeakWidgetEventPort<E>,
}

impl<E: WidgetEvent> WidgetEventHandle<E> {
    /// Creates a weak handle to a live retained-producer-owned port.
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

    /// Returns whether the concrete retained producer still owns this event source.
    ///
    /// This is a liveness observation, not an ownership claim. The handle remains weak, and a later
    /// attempt to subscribe can still report [`SubscribeError::WidgetExpired`] if the producer is
    /// removed between operations. The variant retains its established widget-oriented name for API
    /// compatibility.
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

/// Failure to subscribe an application-state method to a retained UI event.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SubscribeError {
    /// The retained producer that owns the event port has already been removed or was unavailable
    /// when its typed handle projected the event capability.
    WidgetExpired,
    /// The event port already has its exclusive listener.
    ///
    /// In normal application code this means the port is already subscribed through its owning
    /// context. Framework controllers and Context-owned services use the same exclusivity rule.
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
/// so the subscription side cannot keep a removed producer alive. There is intentionally no
/// `Clone` implementation: duplicating a listener would violate the single-consumer queue contract.
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
    /// Reports whether the concrete retained producer still owns the subscribed port.
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
/// corresponding retained widget forest and Context-owned service sources.
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
        assert!(matches!(&*owner.borrow(), WidgetEventPort::Disconnected));

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
