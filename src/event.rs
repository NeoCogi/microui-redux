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
//! widget owns native [`WidgetEventPort`] values in its state and exposes typed [`WidgetEvent`]
//! capabilities through its [`crate::WidgetStateHandle`]. [`Session::connect`] maps those native
//! events into one application `Message` type as they are emitted. Subscriber callbacks run only
//! after a complete retained update releases its state borrows.

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
    static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_CONNECTION_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("widget event connection id space exhausted")
}

type WidgetEventTarget<E> = Box<dyn FnMut(E)>;

/// One widget-owned, native event output.
///
/// This type is framework-facing. Applications obtain a weak [`WidgetEvent`] capability from a
/// widget's state handle instead of accessing the port itself.
pub(crate) struct WidgetEventPort<E: 'static> {
    target: Option<(u64, WidgetEventTarget<E>)>,
}

impl<E: 'static> WidgetEventPort<E> {
    pub(crate) fn new() -> Self {
        Self { target: None }
    }

    /// Sends one native widget event into its connected session, when present.
    pub(crate) fn emit(&mut self, event: E) {
        if let Some((_, target)) = &mut self.target {
            target(event);
        }
    }

    fn connect(&mut self, id: u64, target: WidgetEventTarget<E>) -> bool {
        if self.target.is_some() {
            return false;
        }
        self.target = Some((id, target));
        true
    }

    fn disconnect(&mut self, id: u64) {
        if self.target.as_ref().is_some_and(|(target_id, _)| *target_id == id) {
            self.target = None;
        }
    }
}

impl<E: 'static> Default for WidgetEventPort<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Weak, typed capability identifying one native event produced by a retained widget.
///
/// Widget-specific extension methods construct these values from a [`WidgetStateHandle`]. Holding
/// one does not keep the widget or its state alive.
pub struct WidgetEvent<S: WidgetState, E: 'static> {
    state: WidgetStateHandle<S>,
    port: for<'a> fn(&'a mut S) -> &'a mut WidgetEventPort<E>,
}

impl<S: WidgetState, E: 'static> WidgetEvent<S, E> {
    pub(crate) fn new(state: WidgetStateHandle<S>, port: for<'a> fn(&'a mut S) -> &'a mut WidgetEventPort<E>) -> Self {
        Self { state, port }
    }
}

impl<S: WidgetState, E: 'static> Clone for WidgetEvent<S, E> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            port: self.port,
        }
    }
}

impl<S: WidgetState, E: 'static> fmt::Debug for WidgetEvent<S, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WidgetEvent").field("alive", &self.state.is_alive()).finish_non_exhaustive()
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
        let Some(inbox) = self.inbox.upgrade() else {
            return false;
        };
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
        Self { entries: Vec::new(), next_id: 1 }
    }

    /// Registers a subscriber notified for every application message in registration order.
    pub fn subscribe(&mut self, callback: impl FnMut(&mut State, &Message, &mut Emit<Message>) + 'static) -> SubscriptionId {
        let id = SubscriptionId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect("message subscription id space exhausted");
        self.entries.push(Subscriber { id, callback: Box::new(callback) });
        id
    }

    /// Removes one prior subscription.
    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        let previous_len = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != previous_len
    }
}

impl<State, Message> Default for Subscribers<State, Message> {
    fn default() -> Self {
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
        Self {
            inbox: Rc::new(RefCell::new(Inbox::default())),
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
    pub fn connect<S, E>(&mut self, event: WidgetEvent<S, E>, map: impl Fn(E) -> Message + 'static) -> Result<(), ConnectError>
    where
        S: WidgetState,
        E: 'static,
    {
        let id = next_connection_id();
        let weak_inbox = Rc::downgrade(&self.inbox);
        let target = Box::new(move |native_event: E| {
            let Some(inbox) = weak_inbox.upgrade() else {
                return;
            };
            let message = map(native_event);
            inbox.borrow_mut().pending.push_back(message);
        });

        let connected = event.state.try_update(|state| (event.port)(state).connect(id, target));
        let Some(connected) = connected else {
            return Err(if event.state.is_alive() {
                ConnectError::WidgetBorrowed
            } else {
                ConnectError::WidgetExpired
            });
        };
        if !connected {
            return Err(ConnectError::AlreadyConnected);
        }

        let state = event.state.clone();
        let owner = event.state.clone();
        let port = event.port;
        self.connections.push(Connection {
            is_alive: Box::new(move || owner.is_alive()),
            disconnect: Some(Box::new(move || {
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
        self.connections.retain(Connection::is_alive);
    }

    /// Enqueues an application-authored message for the next dispatch boundary.
    pub fn emit(&mut self, message: Message) {
        self.inbox.borrow_mut().pending.push_back(message);
    }

    /// Dispatches queued messages and subscriber-emitted cascades in FIFO order.
    pub(crate) fn dispatch<State>(&mut self, state: &mut State, subscribers: &mut Subscribers<State, Message>) -> bool {
        let mut dispatched = 0usize;
        loop {
            let message = self.inbox.borrow_mut().pending.pop_front();
            let Some(message) = message else { break };
            dispatched += 1;
            assert!(
                dispatched <= MAX_CASCADE_MESSAGES,
                "application message cascade exceeded {MAX_CASCADE_MESSAGES} messages in one input transaction"
            );
            let mut emit = Emit { inbox: Rc::downgrade(&self.inbox) };
            for subscriber in &mut subscribers.entries {
                (subscriber.callback)(state, &message, &mut emit);
            }
        }
        dispatched != 0
    }
}

impl<Message: 'static> Default for Session<Message> {
    fn default() -> Self {
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
        (self.is_alive)()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(disconnect) = self.disconnect.take() {
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
        &mut state.changed
    }

    #[test]
    fn native_widget_events_map_to_typed_messages_and_cascade_fifo() {
        let owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let handle = WidgetStateHandle::new(&owner);
        let event = WidgetEvent::new(handle, changed);
        let mut session = Session::new();
        let mapping_calls = Rc::new(std::cell::Cell::new(0));
        let mapping_calls_in_connection = Rc::clone(&mapping_calls);
        session
            .connect(event, move |value| {
                mapping_calls_in_connection.set(mapping_calls_in_connection.get() + 1);
                Message::Changed(value)
            })
            .unwrap();

        let mut subscribers = Subscribers::new();
        subscribers.subscribe(|state: &mut State, message: &Message, emit| match message {
            Message::Changed(value) => {
                state.values.push(*value);
                emit.emit(Message::Cascaded(value + 10));
            }
            Message::Cascaded(value) => state.values.push(*value),
        });

        owner.borrow_mut().changed.emit(3);
        owner.borrow_mut().changed.emit(4);
        assert_eq!(mapping_calls.get(), 2, "event adapters must construct messages synchronously");
        let mut state = State::default();
        assert!(state.values.is_empty(), "subscribers must remain deferred until dispatch");
        session.dispatch(&mut state, &mut subscribers);

        assert_eq!(state.values, [3, 4, 13, 14]);
    }

    #[test]
    fn dropping_session_disconnects_widget_event() {
        let owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let handle = WidgetStateHandle::new(&owner);
        let event = WidgetEvent::new(handle, changed);
        let mut session = Session::new();
        session.connect(event.clone(), Message::Changed).unwrap();
        drop(session);

        let mut replacement = Session::new();
        assert_eq!(replacement.connect(event, Message::Changed), Ok(()));
    }

    #[test]
    fn expired_connections_are_pruned_at_an_explicit_topology_boundary() {
        let expired_owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let expired_event = WidgetEvent::new(WidgetStateHandle::new(&expired_owner), changed);
        let live_owner = Rc::new(RefCell::new(TestWidgetState { changed: WidgetEventPort::new() }));
        let live_event = WidgetEvent::new(WidgetStateHandle::new(&live_owner), changed);
        let mut session = Session::new();

        session.connect(expired_event, Message::Changed).unwrap();
        drop(expired_owner);
        session.connect(live_event, Message::Changed).unwrap();

        assert_eq!(session.connections.len(), 2, "connect must not rescan existing connections");
        session.prune_expired_connections();
        assert_eq!(session.connections.len(), 1);

        live_owner.borrow_mut().changed.emit(7);
        let mut state = State::default();
        let mut subscribers = Subscribers::new();
        subscribers.subscribe(|state: &mut State, message, _| {
            if let Message::Changed(value) = message {
                state.values.push(*value);
            }
        });
        assert!(session.dispatch(&mut state, &mut subscribers));
        assert_eq!(state.values, [7]);
    }
}
