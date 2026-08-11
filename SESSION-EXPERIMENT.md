# Context-owned typed events

The retained UI is one transaction domain. A `Context<B, State>` owns the hardware-input FIFO, all
window/dialog/popup roots, and one typed event session for `State`. Widgets remain independent of
the application state type: each concrete widget owns only its native `WidgetEventPort<Event>`.

## Application API

```rust
struct Model {
    name: String,
}

impl Model {
    fn name_changed(&mut self, event: &TextboxChanged) {
        self.name.clone_from(&event.text);
    }

    fn save(&mut self, _: &ButtonSubmitted) {
        // Persist the model.
    }
}

let mut context = Context::<Backend, Model>::new(backend);
let (name, name_node) = Textbox::create(TextboxParameters::new(""));
let (save, save_node) = Button::create(ButtonParameters::new("Save"));

context.subscribe(name.changed(), Model::name_changed)?;
context.subscribe(save.submitted(), Model::save)?;

// Mount name_node and save_node in the context's root forest.
context.update_ui_state(dimensions, &mut model);
```

Bound application values remain available without changing native widget payloads:

```rust
context.subscribe_with(slider.changed(), index, Model::slider_changed)?;
```

There is no public standalone `Session`. Polling-only contexts use `Context<B>` and
`Context::update_ui`; event-driven contexts use `Context<B, State>` and
`Context::update_ui_state`.

## Dispatch boundary

```text
Context input FIFO
    -> route one raw event through the eligible root tree
    -> widget mutates local state and appends E to WidgetEventPort<E>
    -> complete cross-root update releases retained widget borrows
    -> context session drains subscribed ports into &mut State
    -> layout commits before the next raw event is routed
```

The initial synchronization pass also drains events queued by programmatic widget changes, even
when no raw input is waiting. Dispatch repeats until every subscribed port is empty, so events
emitted by state methods complete in the same transaction. A cascade limit terminates accidental
feedback loops.

## Ownership and ordering

- A widget is the sole strong owner of its typed event ports.
- `WidgetEventHandle<Event>` and context subscription records hold weak port references.
- Each port accepts one context subscription and discards events while unsubscribed.
- Removing a widget drops its pending events; dead context bindings are pruned during dispatch.
- Dropping the context session disconnects its live ports.
- FIFO is preserved within each port. When several ports have pending events at one boundary,
  subscription order determines their dispatch order.
- One state method owns the effects for one port; application-level fan-out is ordinary method
  composition rather than multicast event infrastructure.

This leaves one dynamic boundary: the context session erases the concrete event type of each port
dispatcher so a single `Context<B, State>` can subscribe to heterogeneous native widget events.
