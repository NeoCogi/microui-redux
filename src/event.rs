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
//! Each widget event port owns its pending native events. A context owns one [`EventSession`] for
//! its application state type, and that session owns the sole listener for every subscribed port.
//! Dispatch drains port queues only after retained widget borrows have ended.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use crate::Widget;

/// Maximum number of native events handled in one dispatch transaction.
const MAX_EVENT_DISPATCHES: usize = 1_000_000;

/// Marker implemented by every native semantic event payload emitted by a widget.
pub trait WidgetEvent: 'static {}

/// A concrete retained widget that owns one typed native event source.
///
/// A widget may implement this trait more than once with different `E` types.
pub trait TypedWidget<E: WidgetEvent>: Widget {
    /// Returns a weak capability for this widget's `E` event source.
    fn event(&self) -> WidgetEventHandle<E>;
}

/// One widget-owned typed event queue.
pub(crate) struct WidgetEventPort<E: WidgetEvent> {
    pending: Option<VecDeque<E>>,
}

impl<E: WidgetEvent> WidgetEventPort<E> {
    pub(crate) fn new() -> Self {
        Self { pending: None }
    }

    /// Queues an event when this port belongs to the context's event session.
    pub(crate) fn emit(&mut self, event: E) {
        if let Some(pending) = &mut self.pending {
            pending.push_back(event);
        }
    }

    fn connect(&mut self) -> Result<(), SubscribeError> {
        if self.pending.is_some() {
            return Err(SubscribeError::AlreadySubscribed);
        }
        self.pending = Some(VecDeque::new());
        Ok(())
    }

    fn disconnect(&mut self) {
        self.pending = None;
    }

    fn drain(&mut self) -> VecDeque<E> {
        self.pending.as_mut().map(std::mem::take).unwrap_or_default()
    }
}

type WeakWidgetEventPort<E> = Weak<RefCell<WidgetEventPort<E>>>;

/// Weak, typed capability identifying one native event source owned by a retained widget.
///
/// Holding or cloning this value does not keep the widget or its pending events alive.
pub struct WidgetEventHandle<E: WidgetEvent> {
    port: WeakWidgetEventPort<E>,
}

impl<E: WidgetEvent> WidgetEventHandle<E> {
    pub(crate) fn new(port: &Rc<RefCell<WidgetEventPort<E>>>) -> Self {
        Self { port: Rc::downgrade(port) }
    }

    pub(crate) fn expired() -> Self {
        Self { port: Weak::new() }
    }

    /// Returns whether the concrete widget still owns this event source.
    pub fn is_alive(&self) -> bool {
        self.port.strong_count() != 0
    }

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
    /// The event port already belongs to this context's sole event session.
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

/// Exclusive queue reader retained by either a state subscription or framework controller.
pub(crate) struct WidgetEventListener<E: WidgetEvent> {
    port: WeakWidgetEventPort<E>,
}

impl<E: WidgetEvent> WidgetEventListener<E> {
    pub(crate) fn drain(&self) -> VecDeque<E> {
        self.port.upgrade().map(|port| port.borrow_mut().drain()).unwrap_or_default()
    }

    fn is_alive(&self) -> bool {
        self.port.strong_count() != 0
    }
}

impl<E: WidgetEvent> Drop for WidgetEventListener<E> {
    fn drop(&mut self) {
        if let Some(port) = self.port.upgrade() {
            port.borrow_mut().disconnect();
        }
    }
}

trait EventDispatch<State> {
    fn is_alive(&self) -> bool;
    fn dispatch(&mut self, state: &mut State) -> usize;
}

struct Subscription<State, E: WidgetEvent, Handler> {
    listener: WidgetEventListener<E>,
    handler: Handler,
    state: PhantomData<fn(&mut State)>,
}

impl<State, E, Handler> EventDispatch<State> for Subscription<State, E, Handler>
where
    E: WidgetEvent,
    Handler: FnMut(&mut State, &E),
{
    fn is_alive(&self) -> bool {
        self.listener.is_alive()
    }

    fn dispatch(&mut self, state: &mut State) -> usize {
        let events = self.listener.drain();
        let count = events.len();
        for event in events {
            (self.handler)(state, &event);
        }
        count
    }
}

/// The sole typed event dispatcher owned by one UI context.
pub(crate) struct EventSession<State> {
    subscriptions: Vec<Box<dyn EventDispatch<State>>>,
}

impl<State: 'static> EventSession<State> {
    pub(crate) fn new() -> Self {
        Self { subscriptions: Vec::new() }
    }

    pub(crate) fn subscribe<E: WidgetEvent>(&mut self, event: WidgetEventHandle<E>, method: fn(&mut State, &E)) -> Result<(), SubscribeError> {
        self.add(event, method)
    }

    pub(crate) fn subscribe_with<E: WidgetEvent, Context: 'static>(
        &mut self,
        event: WidgetEventHandle<E>,
        context: Context,
        method: fn(&mut State, &Context, &E),
    ) -> Result<(), SubscribeError> {
        self.add(event, move |state, event| method(state, &context, event))
    }

    fn add<E: WidgetEvent, Handler: FnMut(&mut State, &E) + 'static>(&mut self, event: WidgetEventHandle<E>, handler: Handler) -> Result<(), SubscribeError> {
        self.subscriptions.push(Box::new(Subscription {
            listener: event.listen()?,
            handler,
            state: PhantomData,
        }));
        Ok(())
    }

    pub(crate) fn dispatch(&mut self, state: &mut State) -> bool {
        self.subscriptions.retain(|subscription| subscription.is_alive());

        let mut dispatched = 0usize;
        loop {
            let before = dispatched;
            for subscription in &mut self.subscriptions {
                dispatched = dispatched
                    .checked_add(subscription.dispatch(state))
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
    fn port_queues_native_events_until_the_session_dispatches() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let mut session = EventSession::new();
        session.subscribe(WidgetEventHandle::new(&owner), State::record_and_cascade).unwrap();

        owner.borrow_mut().emit(3);
        owner.borrow_mut().emit(4);
        let mut state = State {
            source: Some(Rc::clone(&owner)),
            ..State::default()
        };
        assert!(state.values.is_empty());
        assert!(session.dispatch(&mut state));
        assert_eq!(state.values, [3, 4, 13, 14]);
    }

    #[test]
    fn bound_subscriber_receives_registration_context() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let mut session = EventSession::new();
        session.subscribe_with(WidgetEventHandle::new(&owner), 40, State::record_with_offset).unwrap();

        owner.borrow_mut().emit(2);
        let mut state = State::default();
        assert!(session.dispatch(&mut state));
        assert_eq!(state.values, [42]);
    }

    #[test]
    fn one_port_accepts_only_one_context_subscription() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let event = WidgetEventHandle::new(&owner);
        let mut session = EventSession::new();
        session.subscribe(event.clone(), State::record_and_cascade).unwrap();

        assert_eq!(session.subscribe(event, State::record_and_cascade), Err(SubscribeError::AlreadySubscribed));
    }

    #[test]
    fn dropping_the_context_session_disconnects_and_clears_its_ports() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let event = WidgetEventHandle::new(&owner);
        let mut session = EventSession::new();
        session.subscribe(event.clone(), State::record_and_cascade).unwrap();

        owner.borrow_mut().emit(1);
        drop(session);
        owner.borrow_mut().emit(2);
        assert!(owner.borrow().pending.is_none());

        let mut replacement = EventSession::new();
        replacement.subscribe(event, State::record_and_cascade).unwrap();
    }

    #[test]
    fn expired_subscriptions_are_pruned_during_dispatch() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        let mut session = EventSession::new();
        session.subscribe(WidgetEventHandle::new(&owner), State::record_and_cascade).unwrap();
        drop(owner);

        assert!(!session.dispatch(&mut State::default()));
        assert!(session.subscriptions.is_empty());
    }

    #[test]
    fn ports_discard_events_without_a_listener() {
        let owner = Rc::new(RefCell::new(WidgetEventPort::new()));
        owner.borrow_mut().emit(1);

        let mut session = EventSession::new();
        session.subscribe(WidgetEventHandle::new(&owner), State::record_and_cascade).unwrap();
        assert!(!session.dispatch(&mut State::default()));
    }
}
