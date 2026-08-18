# Typed events and retained services

## Context-owned typed events

The retained UI is one transaction domain. A `Context<B, State>` owns the hardware-input FIFO, all
window/dialog/popup roots, Context-owned services, and one typed event dispatcher for `State`.
Widgets and retained services remain independent of the application state type: each producer owns
only its typed `WidgetEventPort<Event>`, while the dispatcher stores the application method that
consumes it.

The example above registers native widget endpoints with `Context::subscribe`. Bound application
values can be attached without changing native widget payloads:

```rust
context.subscribe_with(slider.changed(), index, Model::slider_changed)?;
```

There is no public standalone event `Session`. Contexts without application callbacks use
`Context<B>` and `Context::update_ui`; subscriber-driven contexts use `Context<B, State>` and
`Context::update_ui_state`. Both retain their trees and roots between updates.

```text
Context input FIFO
    -> route one raw event through the eligible root tree
    -> widget mutates local state and appends E to WidgetEventPort<E>
    -> complete cross-root update releases retained widget borrows
    -> framework services publish any resulting lifecycle events
    -> context dispatcher drains subscribed ports into &mut State
    -> context-aware handlers may mutate retained roots and services
    -> layout commits before the next raw event is routed
```

The initial synchronization pass also drains events queued by programmatic widget changes, even
when no raw input is waiting. Dispatch repeats until every subscribed port is empty, so finite
events emitted by state methods complete in the same transaction. A cascade limit detects
accidental feedback loops. The limit is checked after each drained subscription batch, so one
large batch may cross the threshold before the dispatcher panics.

Event ownership and ordering follow these rules:

- A retained producer is the sole strong owner of its typed event ports. Usually that producer is a
  widget; a Context-owned service source remains alive for the Context lifetime.
- `WidgetEventHandle<Event>` and context subscription records hold weak port references.
- Each port accepts one context subscription and discards events while unsubscribed.
- Removing a widget drops its pending events; dead context bindings are pruned during dispatch.
- Dropping the context dispatcher disconnects its live ports.
- FIFO is preserved within each port. When several ports have pending events at one boundary,
  subscription order determines their dispatch order.
- One state method owns the effects for one port; application-level fan-out is ordinary method
  composition rather than multicast event infrastructure.

There is intentionally no total chronology across independent ports. An event emitted into a
subscription later in the current sweep can run during that sweep; one emitted into the current
or an earlier subscription runs in the next sweep. Application logic that requires a total order
should express it inside one state method or one event type.

The context dispatcher is the one dynamic boundary. It erases the concrete event type of each
subscription so one `Context<B, State>` can subscribe to heterogeneous widget and retained-service
events. It has no popup, combo, file-dialog, or other control-specific branch.

## Event-time root and service coordination

Handlers that only mutate application or widget state continue to use `Context::subscribe` and
`Context::subscribe_with`. A handler that must create, show, hide, move, resize, raise, or destroy a
Context-owned root—or open or cancel a retained file dialog—uses `subscribe_context` or
`subscribe_context_with` and receives a short-lived `EventContext<'_>`:

```rust
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

```rust
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
