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
//! The retained runtime remains independent of an application's message vocabulary. A concrete
//! widget defines native payloads implementing [`WidgetEvent`], owns matching [`WidgetEventPort`]
//! values in its state, and exposes weak [`WidgetEventHandle`] capabilities through its
//! [`crate::WidgetStateHandle`]. [`Session::connect`] maps those native events into one application
//! `Message` type as they are emitted. Subscriber callbacks run only after a complete retained
//! update releases its state borrows.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{WidgetState, WidgetStateHandle};

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

/// Type-erased delivery behavior installed in one native event port.
///
/// The wrapper makes the [`WidgetEvent`] constraint local and explicit. Only the callback's concrete
/// implementation type is erased; its input remains the statically known native event `E`.
struct WidgetEventTarget<E: WidgetEvent> {
    callback: Box<dyn FnMut(E)>,
}

impl<E: WidgetEvent> WidgetEventTarget<E> {
    fn new(callback: impl FnMut(E) + 'static) -> Self {
        // Box the connection-specific adapter once so the widget state does not become generic over
        // the application's closure type.
        Self { callback: Box::new(callback) }
    }

    fn emit(&mut self, event: E) {
        // Forward the concrete native event without erasing its value or performing a downcast.
        (self.callback)(event);
    }
}

/// One widget-owned, native event output.
///
/// This type is framework-facing. Applications obtain a weak [`WidgetEventHandle`] capability from
/// a widget's state handle instead of accessing the port itself.
pub(crate) struct WidgetEventPort<E: WidgetEvent> {
    target: Option<(u64, WidgetEventTarget<E>)>,
}

impl<E: WidgetEvent> WidgetEventPort<E> {
    pub(crate) fn new() -> Self {
        // A widget starts detached; Session::connect installs the only permitted target later.
        Self { target: None }
    }

    /// Sends one native widget event into its connected session, when present.
    pub(crate) fn emit(&mut self, event: E) {
        // The target owns the E -> Message adapter. With no target, the event is intentionally
        // discarded instead of being buffered in the widget or exposed through polling state.
        if let Some((_, target)) = &mut self.target {
            target.emit(event);
        }
    }

    fn connect(&mut self, id: u64, target: WidgetEventTarget<E>) -> bool {
        // A native port is point-to-point. Application-level multicast happens after conversion to
        // Message, so installing a second session target would make ownership and ordering unclear.
        if self.target.is_some() {
            return false;
        }
        // Store the identity beside the callback so only its owning Connection can remove it.
        self.target = Some((id, target));
        true
    }

    fn disconnect(&mut self, id: u64) {
        // Ignore stale disconnect requests. In particular, an older Connection must not clear a
        // target that was installed later with a different process-wide identity.
        if self.target.as_ref().is_some_and(|(target_id, _)| *target_id == id) {
            self.target = None;
        }
    }
}

impl<E: WidgetEvent> Default for WidgetEventPort<E> {
    fn default() -> Self {
        // Keep Default and the explicit framework constructor on the same detached-state path.
        Self::new()
    }
}

/// Weak, typed capability identifying one native event produced by a retained widget.
///
/// Widget-specific extension methods construct these values from a [`WidgetStateHandle`]. Holding
/// one does not keep the widget or its state alive.
pub struct WidgetEventHandle<S: WidgetState, E: WidgetEvent> {
    state: WidgetStateHandle<S>,
    port: for<'a> fn(&'a mut S) -> &'a mut WidgetEventPort<E>,
}

impl<S: WidgetState, E: WidgetEvent> WidgetEventHandle<S, E> {
    pub(crate) fn new(state: WidgetStateHandle<S>, port: for<'a> fn(&'a mut S) -> &'a mut WidgetEventPort<E>) -> Self {
        // The weak state handle locates the widget without extending its retained lifetime. The
        // function pointer selects this event's concrete port after checked state access succeeds.
        Self { state, port }
    }
}

impl<S: WidgetState, E: WidgetEvent> Clone for WidgetEventHandle<S, E> {
    fn clone(&self) -> Self {
        // Cloning duplicates only a weak capability and a function pointer; it never clones S or
        // keeps the owning widget mounted.
        Self {
            state: self.state.clone(),
            port: self.port,
        }
    }
}

impl<S: WidgetState, E: WidgetEvent> fmt::Debug for WidgetEventHandle<S, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Do not borrow or reveal widget state while formatting. Liveness is the only useful,
        // non-invasive diagnostic available through this capability.
        f.debug_struct("WidgetEventHandle")
            .field("alive", &self.state.is_alive())
            .finish_non_exhaustive()
    }
}

/// Failure to connect a widget event to an application session.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ConnectError {
    /// The widget has already been removed from the retained tree.
    WidgetExpired,
    /// The widget state is currently borrowed by application or retained runtime code.
    WidgetBorrowed,
    /// This native event already feeds a session. Multicast occurs after conversion to `Message`.
    AlreadyConnected,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep user-facing text centralized and stable while Debug continues to expose variants.
        f.write_str(match self {
            Self::WidgetExpired => "widget event owner has expired",
            Self::WidgetBorrowed => "widget event owner is currently borrowed",
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
    pub fn connect<S, E>(&mut self, event: WidgetEventHandle<S, E>, map: impl Fn(E) -> Message + 'static) -> Result<(), ConnectError>
    where
        S: WidgetState,
        E: WidgetEvent,
    {
        // Allocate the identity before installing the target because both the port and its later
        // disconnection closure must agree on the same value.
        let id = next_connection_id();
        // A weak queue reference prevents the widget -> target -> inbox path from keeping Session
        // alive. If a best-effort disconnect cannot reach the widget, the leftover target cannot
        // deliver messages and disappears when the widget itself is dropped.
        let weak_inbox = Rc::downgrade(&self.inbox);
        let target = WidgetEventTarget::new(move |native_event: E| {
            // Session may already be gone if disconnection was blocked by an active widget borrow.
            // In that case the native event has no application destination and is discarded.
            let Some(inbox) = weak_inbox.upgrade() else {
                return;
            };
            // Mapping is deliberately synchronous and construction-only. Compute before borrowing
            // the queue so the adapter never runs under an Inbox RefCell borrow.
            let message = map(native_event);
            // Messages from every widget share this FIFO and are delivered later by dispatch.
            inbox.borrow_mut().pending.push_back(message);
        });

        // Checked access distinguishes an unavailable widget from an occupied native event port.
        // The target moves into the port only when this closure runs successfully.
        let connected = event.state.try_update(|state| (event.port)(state).connect(id, target));
        let Some(connected) = connected else {
            // try_update intentionally combines expiration and borrow conflict. A separate liveness
            // check refines the public error without attempting another state borrow.
            return Err(if event.state.is_alive() {
                ConnectError::WidgetBorrowed
            } else {
                ConnectError::WidgetExpired
            });
        };
        if !connected {
            // The state was accessible, but the point-to-point port already owns another target.
            return Err(ConnectError::AlreadyConnected);
        }

        // Each erased closure needs its own weak state capability. One supports liveness pruning;
        // the other reaches the typed port when this Connection is eventually dropped.
        let state = event.state.clone();
        let owner = event.state.clone();
        let port = event.port;
        // Record cleanup only after installation succeeds, keeping the Vec and widget port in sync.
        self.connections.push(Connection {
            is_alive: Box::new(move || owner.is_alive()),
            disconnect: Some(Box::new(move || {
                // Disconnection is best-effort because Drop cannot report a temporary borrow
                // conflict. A surviving target holds only a dead Weak inbox and emits nothing.
                let _ = state.try_update(|state| port(state).disconnect(id));
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
        // implementation; the disconnect attempt then becomes a no-op because the state is gone.
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
        // The closure recovers the concrete WidgetStateHandle type hidden by Connection.
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

    struct TestWidgetState {
        changed: WidgetEventPort<i32>,
    }

    impl WidgetState for TestWidgetState {}
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

    fn changed(state: &mut TestWidgetState) -> &mut WidgetEventPort<i32> {
        // Tests pass this ordinary function pointer through WidgetEventHandle to select the native
        // port.
        &mut state.changed
    }

    #[test]
    fn native_widget_events_map_to_typed_messages_and_cascade_fifo() {
        // Arrange one retained-like strong state owner and expose only its weak event capability.
        let owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let handle = WidgetStateHandle::new(&owner);
        let event = WidgetEventHandle::new(handle, changed);
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
        owner.borrow_mut().changed.emit(3);
        owner.borrow_mut().changed.emit(4);
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
        let owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let handle = WidgetStateHandle::new(&owner);
        let event = WidgetEventHandle::new(handle, changed);
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
        let expired_owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let expired_event = WidgetEventHandle::new(WidgetStateHandle::new(&expired_owner), changed);
        let live_owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let live_event = WidgetEventHandle::new(WidgetStateHandle::new(&live_owner), changed);
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
        live_owner.borrow_mut().changed.emit(7);
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
