# Retained transient-root coordination plan

## Status

- [x] Record the goals, ownership design, non-goals, API sketch, and implementation order.
- [x] Add a safe context-mutation capability to context-owned event dispatch.
- [x] Prove context-aware handlers can mutate every retained root kind before the next layout commit.
- [x] Move the demo popup and file-dialog opening out of frame-polled boolean flags.
- [x] Reconcile the demo combo popup entirely through typed events and authoritative retained state.
- [x] Publish combo anchor geometry during update instead of mutating state during paint.
- [x] Add regression tests for popup dismissal, same-transaction placement, and file-dialog opening.
- [ ] Update public lifecycle documentation and examples.
- [ ] Run formatting, focused tests, the complete test suite, and Clippy.

The boxes above are the delivery checklist. A box is checked only in the commit that completes and
verifies that step.

## Goals

1. Let an application event handler safely perform `Context`-owned UI mutations after retained
   widget borrows have ended and before the next layout commit.
2. Use one general mechanism for windows, dialogs, and popups. The mechanism must not know what a
   combo box, menu, file dialog, or other composite control is.
3. Remove frame-polled command flags from the full demo. Opening a popup or file dialog should be a
   direct consequence of the typed event that requested it.
4. Keep widget-specific behavior in the widget or application code that composes it. In particular,
   combo selection and open state remain `Combo` concerns; root visibility, modal policy, placement,
   and z-order remain `WindowManager` concerns.
5. Preserve the crate's ownership rules: `Context` owns roots, `Node` remains uniquely owned, and
   public handles remain non-owning capabilities.

## Non-goals

- Do not add a `PopupController`, an overlay registry, a type-erased controller hierarchy, or a
  command enum with variants for individual controls.
- Do not move application state into `WindowManager` or make it generic over the renderer or state
  type.
- Do not let widget code re-enter `Context` while its retained cell is borrowed.
- Do not replace the file-dialog session result with a generalized asynchronous runtime. Polling a
  one-shot completion value remains supported; this work removes frame-polled UI *commands*.
- Do not add a second root lifetime model. Synchronous `Context` methods remain authoritative, and
  the event-time capability delegates to those same `WindowManager` operations.

## Design

### Ownership

`Context` already owns the two objects needed for the transaction:

```text
Context<B, State>
├── WindowManager                 owns roots, input, modal policy, and file dialogs
└── EventDispatcher<State>        owns typed application subscriptions
```

The two fields can be mutably borrowed independently. `WindowManager` invokes the dispatch closure
only after a complete retained-tree update releases every widget borrow. At that boundary it lends
itself through a short-lived public façade:

```rust
pub struct EventContext<'a> {
    window_manager: &'a mut WindowManager,
}
```

`EventContext` owns nothing and cannot escape the handler call. It exposes the same root and
file-dialog operations that are already safe on `Context`, delegating directly to the borrowed
`WindowManager`:

```rust
impl EventContext<'_> {
    pub fn set_root_visible(
        &mut self,
        root: RootId,
        visible: bool,
    ) -> Result<(), RootMutationError> {
        self.window_manager.set_root_visible(root, visible)
    }

    pub fn open_file_dialog(
        &mut self,
        request: FileDialogRequest,
    ) -> FileDialogSession {
        self.window_manager.open_file_dialog(request)
    }
}
```

This introduces no `Rc`, `Weak`, self-reference, or additional root owner. `RootHandle` remains weak,
and `EventContext` remains an ordinary exclusive borrow whose lifetime is bounded by dispatch.

### Opt-in context-aware subscriptions

Existing state-only subscriptions remain unchanged:

```rust
fn submitted(&mut self, event: &ButtonSubmitted) {
    self.count += 1;
}

context.subscribe(button.submitted(), State::submitted)?;
```

Handlers that need to mutate retained roots opt into a separate method and receive the narrow
capability:

```rust
fn open_popup(
    &mut self,
    event_context: &mut EventContext<'_>,
    _: &ButtonSubmitted,
) {
    event_context.set_root_visible(self.popup.id(), true).unwrap();
}

context.subscribe_context(button.submitted(), State::open_popup)?;
```

Bound subscriptions receive the bound value before the event context and payload, matching the
existing `subscribe_with` ordering:

```rust
fn open_indexed_popup(
    &mut self,
    index: &usize,
    event_context: &mut EventContext<'_>,
    _: &ButtonSubmitted,
) {
    let root = self.popups[*index].id();
    event_context.set_root_visible(root, true).unwrap();
}

context.subscribe_context_with(
    button.submitted(),
    index,
    State::open_indexed_popup,
)?;
```

The existing handlers do not pay for or depend on context access. Both forms remain function
pointers stored behind the existing single event-dispatch trait-object boundary.

### Transaction order

The retained update order becomes:

```text
initial layout
→ dispatch already-pending handlers with EventContext
→ layout again when handlers ran

for each input event:
    route input
    → update every eligible retained tree
    → process framework-owned file-dialog actions
    → dispatch application handlers with EventContext
    → layout before routing the next input event
```

A handler therefore mutates root visibility, placement, topology, or file-dialog lifecycle while no
widget is borrowed. The existing layout immediately below the dispatch boundary commits that change.
No deferred application flag or second ownership system is required.

### Combo and popup responsibilities

The general layer remains unaware of combos. The full demo composes the existing `Combo` header and
popup root as follows:

```rust
fn combo_submitted(
    &mut self,
    event_context: &mut EventContext<'_>,
    event: &ComboSubmitted,
) {
    event_context
        .set_root_visible(self.combo_popup_root.id(), event.open)
        .unwrap();
    if event.open {
        event_context
            .set_root_rect(self.combo_popup_root.id(), self.combo.anchor().unwrap())
            .unwrap();
    }
}
```

Outside-popup dismissal continues to be generic `WindowManager` policy. Its typed
`RootSubmitted::PopupDismissed` event closes the combo's semantic state, preventing root visibility
and `Combo::is_open()` from diverging. The application performs this composition because it owns the
choice that this particular popup belongs to this particular combo.

The combo publishes its anchor from `WidgetUpdateCtx::screen_content_rect`, not from paint. Paint
then remains observational and opening placement uses the geometry that routed the triggering event.

### File dialog

The file dialog remains a specialized retained widget tree because directory navigation and
selection are intrinsically file-dialog behavior. No file-dialog behavior is added to the general
event system. A context-aware button handler simply invokes the existing operation at the safe
transaction boundary:

```rust
fn open_dialog(
    &mut self,
    event_context: &mut EventContext<'_>,
    _: &ButtonSubmitted,
) {
    self.dialog_session = Some(
        event_context.open_file_dialog(FileDialogRequest::default()),
    );
}
```

The returned session remains the one-shot completion capability. Its status is result observation,
not a command that reconstructs or resubmits UI.

## Implementation steps

1. Add `EventContext` and context-aware handler adapters to the existing event dispatcher.
2. Change the internal dispatch closure to receive `&mut WindowManager` at its established safe
   boundary, without moving the update loop or root ownership into `Context`.
3. Mirror only existing root lifecycle and file-dialog operations on `EventContext`.
4. Add unit tests proving event-time mutations commit before the next queued event and work for
   windows, dialogs, and popups through the same API.
5. Move combo anchor publication from paint to update and add a paint-purity regression test.
6. Convert the full demo's popup, combo, and file-dialog opening handlers to context-aware
   subscriptions; remove `open_popup`, `open_dialog`, and `combo_open`.
7. Subscribe to combo-popup dismissal and reconcile the combo's retained semantic state.
8. Run `cargo fmt --check`, focused library/integration tests, `cargo test`, and Clippy.
