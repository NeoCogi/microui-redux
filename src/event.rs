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

//! Application-typed sessions and widget-native event connections.
//!
//! # Architecture
//!
//! This module is the boundary between widget-defined events and an application's message model.
//! The retained tree and UI runtime do not know the application's `Message` type. A concrete widget
//! knows only its own native event payloads, while [`Session<Message>`](Session) performs the
//! application-specific conversion and dispatch.
//!
//! ```text
//! retained tree                                             application
//! ┌─────────────────────────────────┐          ┌────────────────────────────┐
//! │ concrete widget W               │          │ Session<Message>           │
//! │ ├─ semantic WidgetState S       │          │ ├─ FIFO Inbox<Message>     │
//! │ └─ Rc<WidgetEventPort<E>>       │          │ └─ Connection records      │
//! │      └─ WidgetEventTarget<E> ───┼─ Weak ──►│                            │
//! └──────────────┬──────────────────┘          └─────────────┬──────────────┘
//!                │ implements                               │ dispatch
//!                ▼                                          ▼
//!        TypedWidget<E>                         Subscribers<State, Message>
//!                │
//!                └─ WidgetEventHandle<E> (Weak port capability)
//! ```
//!
//! ## Widget side
//!
//! A widget author defines an owned native payload such as `SliderChanged` and explicitly
//! implements [`WidgetEvent`] for it. The concrete widget runtime owns a private
//! [`WidgetEventPort<E>`](WidgetEventPort) alongside, but separate from, its semantic state. User
//! input updates state and calls `port.emit(event)` only for user-originated semantic changes;
//! ordinary programmatic state setters remain silent.
//!
//! The runtime implements [`TypedWidget<E>`], possibly once for each distinct native event type.
//! Applications capture a [`WidgetEventHandle<E>`] from the concrete runtime before moving it into
//! a type-erased [`crate::Node`]. The handle contains only a weak pointer to the selected port. It
//! neither borrows semantic state nor keeps the widget mounted.
//!
//! ## Connecting a widget to an application
//!
//! [`Session::connect`] consumes an event handle and an application adapter `Fn(E) -> Message`.
//! Upgrading the handle installs one [`WidgetEventTarget<E>`](WidgetEventTarget) directly in the
//! selected port. A port accepts exactly one target; application fan-out happens later through
//! [`Subscribers`] after every native payload has become the session's one `Message` type.
//!
//! The target boxes the connection-specific closure once because its concrete closure type depends
//! on the application. It retains `E` statically, captures the adapter, and holds only a weak
//! reference to the session inbox. No event value is converted through `Any`, and no downcast is
//! used.
//!
//! The session separately stores an erased [`Connection`] record containing widget liveness and
//! disconnection behavior. This lets one `Session<Message>` own connections to heterogeneous
//! widget and event types without making the retained runtime generic over `Message`.
//!
//! ## Event and dispatch flow
//!
//! ```text
//! raw input
//!    │
//!    ▼
//! retained widget update mutates S and constructs E
//!    │
//!    ▼
//! WidgetEventPort<E>::emit(E)
//!    │
//!    ├─ adapter constructs Message synchronously
//!    └─ Message is appended to the session FIFO
//!          │
//!          │ complete retained update releases all state borrows
//!          ▼
//! Session::dispatch
//!    │
//!    └─ each subscriber receives (&mut State, &Message, &mut Emit<Message>)
//! ```
//!
//! The `Fn(E) -> Message` adapter runs while the emitting widget may still be mutably borrowed. It
//! is therefore a construction-only adapter: widget access and application effects belong in a
//! subscriber. [`crate::Context::update_ui_session`] invokes dispatch only after the complete
//! retained update has released widget borrows and before committing the layout used for the next
//! queued raw event.
//!
//! Widget events, [`Session::emit`], and subscriber cascades all append to the same
//! [`VecDeque<Message>`](VecDeque). Dispatch removes from the front, subscribers run in registration
//! order, and [`Emit::emit`] appends cascades at the back. This provides one FIFO order across every
//! message source. A cascade limit terminates accidental subscriber feedback loops.
//!
//! ## Ownership and cleanup
//!
//! The concrete widget is the sole strong owner of each port. Event handles and session connection
//! records hold weak port references, so neither can retain a removed widget. Session owns the
//! inbox and every [`Connection`]; widget targets and temporary [`Emit`] values hold weak inbox
//! references, so widgets and callbacks cannot keep a dropped session alive.
//! Dropping a session drops its connection records, which remove matching targets from live ports.
//! Dropping a widget destroys its port immediately but leaves an expired weak connection record in
//! the session. Dynamic-tree owners call [`Session::prune_expired_connections`] once per topology
//! replacement batch, avoiding the quadratic cost of scanning all prior connections inside every
//! `connect` call.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Widget;

/// Maximum number of application messages dispatched from one raw-input transaction, including
/// messages recursively emitted by subscribers.
const MAX_CASCADE_MESSAGES: usize = 1_000_000;

/// Allocates identities used to disconnect one session from one widget event port.
fn next_connection_id() -> u64 {
    // IDs are process-wide because a delayed/stale disconnection must never match a newer target
    // installed by another Session. Relaxed ordering is sufficient: the atomic only guarantees
    // uniqueness and does not publish memory between threads.
    static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_CONNECTION_ID
        // Refuse to wrap because reusing an ID could let an old Connection remove a new target.
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("widget event connection id space exhausted")
}

/// Marker implemented by every native semantic event payload emitted by a widget.
///
/// Widget authors define these payloads as part of the widget's typed public contract. The trait
/// has no behavior: it distinguishes native widget events from application `Message` types while
/// preserving each payload's concrete type through [`Session::connect`].
pub trait WidgetEvent: 'static {}

/// A concrete retained widget that owns one typed native event source.
///
/// A widget may implement this trait more than once with different `E` types. Applications obtain
/// each weak event capability before moving the concrete widget into a type-erased [`crate::Node`].
pub trait TypedWidget<E: WidgetEvent>: Widget {
    /// Returns a weak capability for this widget's `E` event source.
    fn event(&self) -> WidgetEventHandle<E>;
}

/// Type-erased delivery behavior installed in one native event port.
///
/// The wrapper makes the [`WidgetEvent`] constraint local and explicit. Only the callback's concrete
/// implementation type is erased; its input remains the statically known native event `E`.
struct WidgetEventTarget<E: WidgetEvent> {
    callback: Box<dyn Fn(E)>,
}

impl<E: WidgetEvent> WidgetEventTarget<E> {
    fn new(callback: impl Fn(E) + 'static) -> Self {
        // Box the connection-specific adapter once so the concrete widget does not become generic
        // over the application's closure type.
        Self { callback: Box::new(callback) }
    }

    fn emit(&self, event: E) {
        // Delivery needs no mutable callback state: queue mutation happens through Inbox's RefCell.
        // Forward the concrete native event without erasing its value or performing a downcast.
        (self.callback)(event);
    }
}

/// One widget-owned, native event output.
///
/// This type is framework-facing. Applications obtain a weak [`WidgetEventHandle`] capability from
/// the concrete widget's [`TypedWidget`] implementation instead of accessing the port itself.
pub(crate) struct WidgetEventPort<E: WidgetEvent> {
    target: RefCell<Option<(u64, WidgetEventTarget<E>)>>,
}

impl<E: WidgetEvent> WidgetEventPort<E> {
    pub(crate) fn new() -> Self {
        // A widget starts detached; Session::connect installs the only permitted target later.
        Self { target: RefCell::new(None) }
    }

    /// Sends one native widget event into its connected session, when present.
    pub(crate) fn emit(&self, event: E) {
        // The target owns the E -> Message adapter. With no target, the event is intentionally
        // discarded instead of being buffered in the widget or exposed through polling state.
        if let Some((_, target)) = &*self.target.borrow() {
            target.emit(event);
        }
    }

    fn connect(&self, id: u64, target: WidgetEventTarget<E>) -> bool {
        // A native port is point-to-point. Application-level multicast happens after conversion to
        // Message, so installing a second session target would make ownership and ordering unclear.
        let mut current = self.target.borrow_mut();
        if current.is_some() {
            return false;
        }
        // Store the identity beside the callback so only its owning Connection can remove it.
        *current = Some((id, target));
        true
    }

    fn disconnect(&self, id: u64) {
        // Ignore stale disconnect requests. In particular, an older Connection must not clear a
        // target that was installed later with a different process-wide identity.
        let mut current = self.target.borrow_mut();
        if current.as_ref().is_some_and(|(target_id, _)| *target_id == id) {
            *current = None;
        }
    }
}

impl<E: WidgetEvent> Default for WidgetEventPort<E> {
    fn default() -> Self {
        // Keep Default and the explicit framework constructor on the same detached-state path.
        Self::new()
    }
}

/// Weak framework capability used when a semantic state operation must publish through a port
/// whose strong ownership remains with the concrete widget runtime.
pub(crate) struct WidgetEventEmitter<E: WidgetEvent> {
    port: Weak<WidgetEventPort<E>>,
}

impl<E: WidgetEvent> WidgetEventEmitter<E> {
    pub(crate) fn new(port: &Rc<WidgetEventPort<E>>) -> Self {
        // State may borrow this capability, but only the concrete widget keeps the port alive.
        Self { port: Rc::downgrade(port) }
    }

    pub(crate) fn emit(&self, event: E) {
        // An expired widget has no observable event stream, so emission becomes a no-op.
        if let Some(port) = self.port.upgrade() {
            port.emit(event);
        }
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
        // The concrete widget remains the sole strong owner of its event source.
        Self { port: Rc::downgrade(port) }
    }

    /// Returns whether the concrete widget still owns this event source.
    pub fn is_alive(&self) -> bool {
        // strong_count does not upgrade or borrow the port and therefore cannot affect lifetime.
        self.port.strong_count() != 0
    }
}

impl<E: WidgetEvent> Clone for WidgetEventHandle<E> {
    fn clone(&self) -> Self {
        // Cloning duplicates only a Weak pointer and never keeps the owning widget mounted.
        Self { port: self.port.clone() }
    }
}

impl<E: WidgetEvent> fmt::Debug for WidgetEventHandle<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Liveness is the only useful diagnostic exposed by an otherwise opaque capability.
        f.debug_struct("WidgetEventHandle")
            .field("alive", &(self.port.strong_count() != 0))
            .finish_non_exhaustive()
    }
}

/// Failure to connect a widget event to an application session.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ConnectError {
    /// The widget has already been removed from the retained tree.
    WidgetExpired,
    /// This native event already feeds a session. Multicast occurs after conversion to `Message`.
    AlreadyConnected,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep user-facing text centralized and stable while Debug continues to expose variants.
        f.write_str(match self {
            Self::WidgetExpired => "widget event owner has expired",
            Self::AlreadyConnected => "widget event is already connected to a session",
        })
    }
}

impl std::error::Error for ConnectError {}

struct Inbox<Message> {
    pending: VecDeque<Message>,
}

impl<Message> Default for Inbox<Message> {
    fn default() -> Self {
        // VecDeque provides FIFO removal from the front and retains capacity across transactions.
        Self { pending: VecDeque::new() }
    }
}

/// Application-message emission capability supplied to session subscribers.
pub struct Emit<Message> {
    inbox: Weak<RefCell<Inbox<Message>>>,
}

impl<Message> Emit<Message> {
    /// Appends a message to the current session transaction.
    ///
    /// Returns `false` only when the owning session has already been dropped.
    pub fn emit(&mut self, message: Message) -> bool {
        // Emit is intentionally weak: a callback must not prolong the lifetime of its Session.
        let Some(inbox) = self.inbox.upgrade() else {
            return false;
        };
        // Append cascaded messages behind everything already queued. dispatch releases its queue
        // borrow before invoking subscribers, so this mutable borrow cannot overlap that one.
        inbox.borrow_mut().pending.push_back(message);
        true
    }
}

/// Opaque identity returned for one application-message subscription.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionId(u64);

type SubscriberCallback<State, Message> = Box<dyn FnMut(&mut State, &Message, &mut Emit<Message>)>;

struct Subscriber<State, Message> {
    id: SubscriptionId,
    callback: SubscriberCallback<State, Message>,
}

/// Ordered subscribers to one session-wide application message stream.
pub struct Subscribers<State, Message> {
    entries: Vec<Subscriber<State, Message>>,
    next_id: u64,
}

impl<State, Message> Subscribers<State, Message> {
    /// Creates an empty subscriber collection.
    pub fn new() -> Self {
        // IDs begin at one only to reserve a simple nonzero-looking public identity; registration
        // order is represented directly by the Vec.
        Self { entries: Vec::new(), next_id: 1 }
    }

    /// Registers a subscriber notified for every application message in registration order.
    pub fn subscribe(&mut self, callback: impl FnMut(&mut State, &Message, &mut Emit<Message>) + 'static) -> SubscriptionId {
        // Allocate a stable identity before moving the callback into erased storage. Overflow is a
        // hard error because reusing an ID could unsubscribe the wrong callback.
        let id = SubscriptionId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect("message subscription id space exhausted");
        // The box erases only callback implementation type. State and Message remain statically
        // known, and Vec insertion preserves notification order.
        self.entries.push(Subscriber { id, callback: Box::new(callback) });
        id
    }

    /// Removes one prior subscription.
    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        // Retain preserves the relative order of every remaining subscriber. Comparing lengths
        // reports whether the opaque ID actually matched an entry.
        let previous_len = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != previous_len
    }
}

impl<State, Message> Default for Subscribers<State, Message> {
    fn default() -> Self {
        // Route all construction through new so ID initialization cannot diverge.
        Self::new()
    }
}

/// One application-typed semantic message session.
///
/// A session owns the message queue, every widget-event connection installed through it, and the
/// safe-boundary dispatcher. Neither the renderer Context nor the retained tree is generic over
/// `Message`.
pub struct Session<Message> {
    inbox: Rc<RefCell<Inbox<Message>>>,
    connections: Vec<Connection>,
}

impl<Message: 'static> Session<Message> {
    /// Creates an empty application message session.
    pub fn new() -> Self {
        // Widget targets and subscriber Emit handles refer to one shared queue. Session keeps the
        // sole long-lived strong owner; both outward-facing paths receive Weak references.
        Self {
            inbox: Rc::new(RefCell::new(Inbox::default())),
            // Connection records are retained so dropping the Session can detach every live port.
            connections: Vec::new(),
        }
    }

    /// Connects one widget-native event to this session's application message stream.
    ///
    /// The `map` closure runs synchronously when the native event is emitted, while the event-owning
    /// widget state may still be mutably borrowed. It must only construct a `Message` from the event
    /// and captured values; widget access and application effects belong in a [`Subscribers`]
    /// callback, which runs after retained state borrows have ended.
    ///
    /// Exactly one session may be connected to a widget event; fan-out belongs in [`Subscribers`]
    /// after the event has become an application `Message`.
    pub fn connect<E: WidgetEvent>(&mut self, event: WidgetEventHandle<E>, map: impl Fn(E) -> Message + 'static) -> Result<(), ConnectError> {
        // Allocate the identity before installing the target because both the port and its later
        // disconnection closure must agree on the same value.
        let id = next_connection_id();
        // A weak queue reference prevents the widget -> target -> inbox path from keeping Session
        // alive.
        let weak_inbox = Rc::downgrade(&self.inbox);
        let target = WidgetEventTarget::new(move |native_event: E| {
            // Session may already be gone if a target is invoked during teardown. In that case the
            // native event has no application destination and is discarded.
            let Some(inbox) = weak_inbox.upgrade() else {
                return;
            };
            // Mapping is deliberately synchronous and construction-only. Compute before borrowing
            // the queue so the adapter never runs under an Inbox RefCell borrow.
            let message = map(native_event);
            // Messages from every widget share this FIFO and are delivered later by dispatch.
            inbox.borrow_mut().pending.push_back(message);
        });

        // Upgrade the widget-owned port without borrowing semantic widget state. Expiration now has
        // one unambiguous cause: the concrete widget has been removed.
        let Some(port) = event.port.upgrade() else {
            return Err(ConnectError::WidgetExpired);
        };
        if !port.connect(id, target) {
            // The port is live, but its point-to-point slot already owns another target.
            return Err(ConnectError::AlreadyConnected);
        }

        // Session records only weak port references; the concrete widget remains the sole owner.
        let owner = Rc::downgrade(&port);
        let disconnect = Rc::downgrade(&port);
        // Record cleanup only after installation succeeds, keeping the Vec and widget port in sync.
        self.connections.push(Connection {
            is_alive: Box::new(move || owner.strong_count() != 0),
            disconnect: Some(Box::new(move || {
                // A live widget can be disconnected without borrowing its semantic state.
                if let Some(port) = disconnect.upgrade() {
                    port.disconnect(id);
                }
            })),
        });
        Ok(())
    }

    /// Removes connections whose event-owning widgets have left the retained tree.
    ///
    /// Call this once after replacing a dynamic subtree and before connecting its replacement
    /// endpoints. Cleanup is an explicit topology boundary so connecting a batch of `n` live
    /// widgets remains O(n), rather than rescanning all preceding connections for every widget.
    /// Active connections and already queued messages are preserved.
    pub fn prune_expired_connections(&mut self) {
        // retain examines each existing weak owner once. Removing an expired record runs its Drop
        // implementation; the disconnect attempt then becomes a no-op because the port is gone.
        self.connections.retain(Connection::is_alive);
    }

    /// Enqueues an application-authored message for the next dispatch boundary.
    pub fn emit(&mut self, message: Message) {
        // Application-authored and widget-authored messages use the same queue, preserving one
        // total FIFO order regardless of their source.
        self.inbox.borrow_mut().pending.push_back(message);
    }

    /// Dispatches queued messages and subscriber-emitted cascades in FIFO order.
    pub(crate) fn dispatch<State>(&mut self, state: &mut State, subscribers: &mut Subscribers<State, Message>) -> bool {
        // Count original and recursively emitted messages together so a subscriber feedback loop
        // cannot monopolize a raw-input transaction indefinitely.
        let mut dispatched = 0usize;
        loop {
            // Keep this RefCell borrow in a single statement. It must end before callbacks run so
            // Emit can append cascaded messages to the same queue without a dynamic borrow panic.
            let message = self.inbox.borrow_mut().pending.pop_front();
            // Reaching an empty queue means all cascades produced so far have also been consumed.
            let Some(message) = message else { break };
            dispatched += 1;
            assert!(
                dispatched <= MAX_CASCADE_MESSAGES,
                "application message cascade exceeded {MAX_CASCADE_MESSAGES} messages in one input transaction"
            );
            // Give callbacks only a weak queue capability. They can enqueue follow-up messages but
            // cannot dispatch recursively or take ownership of Session.
            let mut emit = Emit { inbox: Rc::downgrade(&self.inbox) };
            // Vec order is subscription order. Every subscriber observes the same borrowed message
            // before dispatch advances to anything appended by a callback.
            for subscriber in &mut subscribers.entries {
                (subscriber.callback)(state, &message, &mut emit);
            }
        }
        // The caller uses this to distinguish an idle boundary from one that delivered messages.
        dispatched != 0
    }
}

impl<Message: 'static> Default for Session<Message> {
    fn default() -> Self {
        // Keep Default behavior identical to the explicit constructor.
        Self::new()
    }
}

/// Session-owned disconnection behavior. Only behavior is dynamically dispatched; widget event
/// and application message values remain statically typed.
struct Connection {
    is_alive: Box<dyn Fn() -> bool>,
    disconnect: Option<Box<dyn FnOnce()>>,
}

impl Connection {
    fn is_alive(&self) -> bool {
        // The closure recovers the concrete typed port hidden by this heterogeneous record.
        (self.is_alive)()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // take transfers ownership of the FnOnce and guarantees it cannot be invoked twice, even if
        // Connection cleanup is later refactored to call this path explicitly before field drops.
        if let Some(disconnect) = self.disconnect.take() {
            // The typed closure removes only the target whose ID belongs to this record.
            disconnect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl WidgetEvent for i32 {}

    #[derive(Debug, Eq, PartialEq)]
    enum Message {
        Changed(i32),
        Cascaded(i32),
    }

    #[derive(Default)]
    struct State {
        values: Vec<i32>,
    }

    #[test]
    fn native_widget_events_map_to_typed_messages_and_cascade_fifo() {
        // Arrange one runtime-owned native port and expose only its weak event capability.
        let owner = Rc::new(WidgetEventPort::new());
        let event = WidgetEventHandle::new(&owner);
        let mut session = Session::new();
        // Count adapter calls independently so the test can distinguish mapping time from subscriber
        // dispatch time.
        let mapping_calls = Rc::new(std::cell::Cell::new(0));
        let mapping_calls_in_connection = Rc::clone(&mapping_calls);
        session
            .connect(event, move |value| {
                // Mapping constructs a Message synchronously and performs no widget access.
                mapping_calls_in_connection.set(mapping_calls_in_connection.get() + 1);
                Message::Changed(value)
            })
            .unwrap();

        // Each Changed message records itself and appends one cascade to the same session FIFO.
        let mut subscribers = Subscribers::new();
        subscribers.subscribe(|state: &mut State, message: &Message, emit| match message {
            Message::Changed(value) => {
                state.values.push(*value);
                emit.emit(Message::Cascaded(value + 10));
            }
            Message::Cascaded(value) => state.values.push(*value),
        });

        // Act: native emission maps immediately, but no subscriber receives either message yet.
        owner.emit(3);
        owner.emit(4);
        assert_eq!(mapping_calls.get(), 2, "event adapters must construct messages synchronously");
        let mut state = State::default();
        assert!(state.values.is_empty(), "subscribers must remain deferred until dispatch");
        session.dispatch(&mut state, &mut subscribers);

        // The two original messages remain ahead of both cascades in the shared FIFO.
        assert_eq!(state.values, [3, 4, 13, 14]);
    }

    #[test]
    fn dropping_session_disconnects_widget_event() {
        // Arrange one port connected to an initial Session.
        let owner = Rc::new(WidgetEventPort::new());
        let event = WidgetEventHandle::new(&owner);
        let mut session = Session::new();
        session.connect(event.clone(), Message::Changed).unwrap();

        // Dropping Session drops its Connection, which removes the matching target from the port.
        drop(session);

        // A second Session can therefore claim the now-empty point-to-point port.
        let mut replacement = Session::new();
        assert_eq!(replacement.connect(event, Message::Changed), Ok(()));
    }

    #[test]
    fn expired_connections_are_pruned_at_an_explicit_topology_boundary() {
        // Arrange one widget that will leave the tree and one that will remain live.
        let expired_owner = Rc::new(WidgetEventPort::new());
        let expired_event = WidgetEventHandle::new(&expired_owner);
        let live_owner = Rc::new(WidgetEventPort::new());
        let live_event = WidgetEventHandle::new(&live_owner);
        let mut session = Session::new();

        // Connecting the replacement must be O(1), so the stale record intentionally remains until
        // the caller marks the end of its topology update.
        session.connect(expired_event, Message::Changed).unwrap();
        drop(expired_owner);
        session.connect(live_event, Message::Changed).unwrap();

        // Explicit pruning removes only the expired record and preserves the live connection.
        assert_eq!(session.connections.len(), 2, "connect must not rescan existing connections");
        session.prune_expired_connections();
        assert_eq!(session.connections.len(), 1);

        // Prove that retaining the live record also leaves its installed target operational.
        live_owner.emit(7);
        let mut state = State::default();
        let mut subscribers = Subscribers::new();
        subscribers.subscribe(|state: &mut State, message, _| {
            // This test is concerned only with native Changed delivery; no cascade is needed.
            if let Message::Changed(value) = message {
                state.values.push(*value);
            }
        });
        assert!(session.dispatch(&mut state, &mut subscribers));
        assert_eq!(state.values, [7]);
    }
}
