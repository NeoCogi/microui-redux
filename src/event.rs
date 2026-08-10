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

//! Typed widget events dispatched directly to application-state methods.
//!
//! A concrete widget owns one [`WidgetEventPort`] for each native event type it publishes. An
//! application subscribes a method such as `State::slider_changed` through [`Session::subscribe`].
//! When the widget emits, its port appends one typed method invocation per subscriber to the
//! session FIFO. [`Session::dispatch`] invokes those methods after retained widget borrows have
//! ended.
//!
//! ```text
//! widget.emit(E)
//!      │
//!      ├─ queue Subscriber<State, E> for State::method
//!      └─ queue Subscriber<State, E> for State::other_method
//!                         │
//!                         ▼
//!                  Session<State> FIFO
//!                         │ dispatch
//!                         ▼
//!                       &mut State
//! ```
//!
//! There is no application-wide message enum, mapping adapter, downcast, or `Any` payload. The
//! queue erases only the invocation behavior required to hold different native event types in one
//! FIFO. Multiple subscribers receive the same `Rc<E>`, matching C# multicast-event semantics
//! without requiring `E: Clone`.
//!
//! Event handles and session subscription records hold weak widget-port references, so neither
//! keeps a removed widget alive. A port subscriber holds only a weak session-queue reference, so a
//! live widget cannot keep a dropped session alive. Dropping or explicitly removing a subscription
//! detaches it from a live port.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Widget;

/// Maximum number of subscriber methods invoked from one dispatch transaction.
const MAX_SUBSCRIBER_INVOCATIONS: usize = 1_000_000;

fn next_subscription_id() -> SubscriptionId {
    // IDs are process-wide because subscriptions from different sessions may share one multicast
    // port. A stale disconnection must never remove another session's newer subscriber.
    static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_SUBSCRIPTION_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("widget event subscription id space exhausted");
    SubscriptionId(id)
}

/// Marker implemented by every native semantic event payload emitted by a widget.
pub trait WidgetEvent: 'static {}

/// A concrete retained widget that owns one typed native event source.
///
/// A widget may implement this trait more than once with different `E` types.
pub trait TypedWidget<E: WidgetEvent>: Widget {
    /// Returns a weak capability for this widget's `E` event source.
    fn event(&self) -> WidgetEventHandle<E>;
}

/// Opaque identity returned for one widget-event subscription.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionId(u64);

struct PortSubscriber<E> {
    id: SubscriptionId,
    enqueue: Box<dyn Fn(Rc<E>)>,
}

/// One widget-owned, typed multicast event output.
///
/// Applications access this port through a weak [`WidgetEventHandle`].
pub(crate) struct WidgetEventPort<E: WidgetEvent> {
    subscribers: RefCell<Vec<PortSubscriber<E>>>,
}

impl<E: WidgetEvent> WidgetEventPort<E> {
    pub(crate) fn new() -> Self {
        Self { subscribers: RefCell::new(Vec::new()) }
    }

    /// Queues one invocation for every subscriber in registration order.
    pub(crate) fn emit(&self, event: E) {
        let subscribers = self.subscribers.borrow();
        if subscribers.is_empty() {
            return;
        }

        // One shared allocation lets a non-Clone event reach every subscriber. Subscriber
        // callbacks only append to their session queues and cannot synchronously call State.
        let event = Rc::new(event);
        for subscriber in subscribers.iter() {
            (subscriber.enqueue)(Rc::clone(&event));
        }
    }

    fn subscribe(&self, id: SubscriptionId, enqueue: impl Fn(Rc<E>) + 'static) {
        self.subscribers.borrow_mut().push(PortSubscriber { id, enqueue: Box::new(enqueue) });
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        self.subscribers.borrow_mut().retain(|subscriber| subscriber.id != id);
    }
}

impl<E: WidgetEvent> Default for WidgetEventPort<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Weak, typed capability identifying one native event source owned by a retained widget.
///
/// Holding or cloning this value does not keep the widget or its event port alive.
pub struct WidgetEventHandle<E: WidgetEvent> {
    port: Weak<WidgetEventPort<E>>,
}

impl<E: WidgetEvent> WidgetEventHandle<E> {
    pub(crate) fn new(port: &Rc<WidgetEventPort<E>>) -> Self {
        Self { port: Rc::downgrade(port) }
    }

    pub(crate) fn expired() -> Self {
        Self { port: Weak::new() }
    }

    /// Returns whether the concrete widget still owns this event source.
    pub fn is_alive(&self) -> bool {
        self.port.strong_count() != 0
    }
}

impl<W: Widget + 'static> crate::TypedWidgetHandle<W> {
    /// Projects a concrete widget-owned event port through this weak typed widget handle.
    pub(crate) fn widget_event<E: WidgetEvent>(&self) -> WidgetEventHandle<E>
    where
        W: TypedWidget<E>,
    {
        self.try_read(TypedWidget::event).unwrap_or_else(WidgetEventHandle::expired)
    }
}

impl<E: WidgetEvent> Clone for WidgetEventHandle<E> {
    fn clone(&self) -> Self {
        Self { port: self.port.clone() }
    }
}

impl<E: WidgetEvent> fmt::Debug for WidgetEventHandle<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WidgetEventHandle")
            .field("alive", &(self.port.strong_count() != 0))
            .finish_non_exhaustive()
    }
}

/// Failure to subscribe an application-state method to a widget event.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SubscribeError {
    /// The widget that owns the event port has already been removed.
    WidgetExpired,
}

impl fmt::Display for SubscribeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("widget event owner has expired")
    }
}

impl std::error::Error for SubscribeError {}

/// Type-erased invocation stored in one `Session<State>` FIFO.
///
/// `State` is the erased queue's common receiver; each implementation retains its concrete event
/// type and calls a statically typed method.
trait SubscriberInvoker<State> {
    fn invoke(self: Box<Self>, state: &mut State);
}

struct Subscriber<State, E> {
    event: Rc<E>,
    method: fn(&mut State, &E),
}

impl<State, E> SubscriberInvoker<State> for Subscriber<State, E> {
    fn invoke(self: Box<Self>, state: &mut State) {
        (self.method)(state, &self.event);
    }
}

struct BoundSubscriber<State, Context, E> {
    context: Rc<Context>,
    event: Rc<E>,
    method: fn(&mut State, &Context, &E),
}

impl<State, Context, E> SubscriberInvoker<State> for BoundSubscriber<State, Context, E> {
    fn invoke(self: Box<Self>, state: &mut State) {
        (self.method)(state, &self.context, &self.event);
    }
}

type SubscriberQueue<State> = Rc<RefCell<VecDeque<Box<dyn SubscriberInvoker<State>>>>>;

/// Cloneable access to a session queue for state objects that own their own `Session` field.
///
/// Cloning this lightweight dispatcher ends the borrow of that field before application methods
/// receive `&mut State`.
pub(crate) struct SessionDispatcher<State> {
    queue: SubscriberQueue<State>,
}

impl<State> SessionDispatcher<State> {
    pub(crate) fn dispatch(&self, state: &mut State) -> bool {
        dispatch_queue(&self.queue, state)
    }
}

/// A FIFO of typed widget-event invocations targeting one application state type.
///
/// The session owns its widget subscriptions and dispatches native events directly to registered
/// `State` methods. Neither the renderer nor retained tree becomes generic over `State`.
pub struct Session<State> {
    queue: SubscriberQueue<State>,
    subscriptions: Vec<Box<dyn ErasedSubscription>>,
}

impl<State: 'static> Session<State> {
    /// Creates an empty application event session.
    pub fn new() -> Self {
        Self {
            queue: Rc::new(RefCell::new(VecDeque::new())),
            subscriptions: Vec::new(),
        }
    }

    /// Subscribes a `State` method to one native widget event.
    ///
    /// Each event occurrence is queued while the widget is updating. The method runs later at the
    /// session dispatch boundary and receives the original concrete event type.
    pub fn subscribe<E: WidgetEvent>(&mut self, event: WidgetEventHandle<E>, method: fn(&mut State, &E)) -> Result<SubscriptionId, SubscribeError> {
        let Some(port) = event.port.upgrade() else {
            return Err(SubscribeError::WidgetExpired);
        };

        let id = next_subscription_id();
        let queue = Rc::downgrade(&self.queue);
        port.subscribe(id, move |event| {
            let Some(queue) = queue.upgrade() else {
                return;
            };
            queue.borrow_mut().push_back(Box::new(Subscriber { event, method }));
        });
        self.subscriptions.push(Box::new(PortSubscription { id, port: Rc::downgrade(&port) }));
        Ok(id)
    }

    /// Subscribes a `State` method together with application-owned context.
    ///
    /// This is the direct-method equivalent of a captured C# delegate. The context is allocated
    /// once when subscribed and shared by queued invocations. It is useful for values such as a
    /// row index or domain identifier that do not belong in the widget's native event payload.
    pub fn subscribe_with<E: WidgetEvent, Context: 'static>(
        &mut self,
        event: WidgetEventHandle<E>,
        context: Context,
        method: fn(&mut State, &Context, &E),
    ) -> Result<SubscriptionId, SubscribeError> {
        let Some(port) = event.port.upgrade() else {
            return Err(SubscribeError::WidgetExpired);
        };

        let id = next_subscription_id();
        let queue = Rc::downgrade(&self.queue);
        let context = Rc::new(context);
        port.subscribe(id, move |event| {
            let Some(queue) = queue.upgrade() else {
                return;
            };
            queue.borrow_mut().push_back(Box::new(BoundSubscriber {
                context: Rc::clone(&context),
                event,
                method,
            }));
        });
        self.subscriptions.push(Box::new(PortSubscription { id, port: Rc::downgrade(&port) }));
        Ok(id)
    }

    /// Removes one subscription from its widget event port.
    ///
    /// Invocations already queued before removal remain in FIFO order, matching the invocation-list
    /// snapshot taken by a synchronous multicast event.
    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        let Some(index) = self.subscriptions.iter().position(|subscription| subscription.id() == id) else {
            return false;
        };
        drop(self.subscriptions.remove(index));
        true
    }

    /// Removes subscription records whose event-owning widgets have left the retained tree.
    pub fn prune_expired_subscriptions(&mut self) {
        self.subscriptions.retain(|subscription| subscription.is_alive());
    }

    pub(crate) fn dispatcher(&self) -> SessionDispatcher<State> {
        SessionDispatcher { queue: Rc::clone(&self.queue) }
    }

    /// Invokes queued subscriber methods in event and subscription registration order.
    pub(crate) fn dispatch(&mut self, state: &mut State) -> bool {
        dispatch_queue(&self.queue, state)
    }
}

fn dispatch_queue<State>(queue: &SubscriberQueue<State>, state: &mut State) -> bool {
    let mut invoked = 0usize;
    loop {
        // End the queue borrow before invoking application code. A method may cause another widget
        // event to append more work to this same transaction.
        let subscriber = queue.borrow_mut().pop_front();
        let Some(subscriber) = subscriber else { break };

        invoked += 1;
        assert!(
            invoked <= MAX_SUBSCRIBER_INVOCATIONS,
            "widget event cascade exceeded {MAX_SUBSCRIBER_INVOCATIONS} subscriber invocations in one input transaction"
        );
        subscriber.invoke(state);
    }
    invoked != 0
}

impl<State: 'static> Default for Session<State> {
    fn default() -> Self {
        Self::new()
    }
}

trait ErasedSubscription {
    fn id(&self) -> SubscriptionId;
    fn is_alive(&self) -> bool;
}

struct PortSubscription<E: WidgetEvent> {
    id: SubscriptionId,
    port: Weak<WidgetEventPort<E>>,
}

impl<E: WidgetEvent> ErasedSubscription for PortSubscription<E> {
    fn id(&self) -> SubscriptionId {
        self.id
    }

    fn is_alive(&self) -> bool {
        self.port.strong_count() != 0
    }
}

impl<E: WidgetEvent> Drop for PortSubscription<E> {
    fn drop(&mut self) {
        if let Some(port) = self.port.upgrade() {
            port.unsubscribe(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl WidgetEvent for i32 {}

    #[derive(Default)]
    struct State {
        values: Vec<i32>,
        source: Option<Rc<WidgetEventPort<i32>>>,
    }

    impl State {
        fn record(&mut self, event: &i32) {
            self.values.push(*event);
        }

        fn cascade(&mut self, event: &i32) {
            if *event < 10 {
                self.source.as_ref().unwrap().emit(event + 10);
            }
        }

        fn record_with_offset(&mut self, offset: &i32, event: &i32) {
            self.values.push(offset + event);
        }
    }

    #[test]
    fn native_widget_events_queue_typed_state_methods_and_multicast_fifo() {
        let owner = Rc::new(WidgetEventPort::new());
        let event = WidgetEventHandle::new(&owner);
        let mut session = Session::new();
        session.subscribe(event.clone(), State::record).unwrap();
        session.subscribe(event, State::cascade).unwrap();

        owner.emit(3);
        owner.emit(4);
        let mut state = State {
            source: Some(Rc::clone(&owner)),
            ..State::default()
        };
        assert!(state.values.is_empty(), "state methods must remain deferred until dispatch");
        assert!(session.dispatch(&mut state));

        // Each event snapshots both subscribers. Cascades append after already queued invocations.
        assert_eq!(state.values, [3, 4, 13, 14]);
    }

    #[test]
    fn bound_subscriber_receives_registration_context() {
        let owner = Rc::new(WidgetEventPort::new());
        let mut session = Session::new();
        session.subscribe_with(WidgetEventHandle::new(&owner), 40, State::record_with_offset).unwrap();

        owner.emit(2);
        let mut state = State::default();
        assert!(session.dispatch(&mut state));
        assert_eq!(state.values, [42]);
    }

    #[test]
    fn dropping_session_disconnects_live_widget_subscriptions() {
        let owner = Rc::new(WidgetEventPort::new());
        let event = WidgetEventHandle::new(&owner);
        let mut session = Session::<State>::new();
        session.subscribe(event, State::record).unwrap();
        assert_eq!(owner.subscribers.borrow().len(), 1);

        drop(session);
        assert!(owner.subscribers.borrow().is_empty());
    }

    #[test]
    fn unsubscribe_detaches_future_events_but_preserves_queued_invocations() {
        let owner = Rc::new(WidgetEventPort::new());
        let mut session = Session::new();
        let subscription = session.subscribe(WidgetEventHandle::new(&owner), State::record).unwrap();

        owner.emit(1);
        assert!(session.unsubscribe(subscription));
        assert!(!session.unsubscribe(subscription));
        owner.emit(2);

        let mut state = State::default();
        assert!(session.dispatch(&mut state));
        assert_eq!(state.values, [1]);
    }

    #[test]
    fn expired_subscriptions_are_pruned_at_an_explicit_topology_boundary() {
        let expired_owner = Rc::new(WidgetEventPort::new());
        let expired_event = WidgetEventHandle::new(&expired_owner);
        let live_owner = Rc::new(WidgetEventPort::new());
        let live_event = WidgetEventHandle::new(&live_owner);
        let mut session = Session::new();

        session.subscribe(expired_event, State::record).unwrap();
        drop(expired_owner);
        session.subscribe(live_event, State::record).unwrap();

        assert_eq!(session.subscriptions.len(), 2, "subscribe must not rescan prior records");
        session.prune_expired_subscriptions();
        assert_eq!(session.subscriptions.len(), 1);

        live_owner.emit(7);
        let mut state = State::default();
        assert!(session.dispatch(&mut state));
        assert_eq!(state.values, [7]);
    }
}
