# Typed session experiment

This branch replaces the first `Any`-backed signal experiment with one application-typed message
session. The retained tree, `UiRuntime`, `Node`, `Widget`, and `WidgetUpdateCtx` do not know the
application message type.

## Application API

Widgets expose native event endpoints through their typed widget handles. The application maps
those events into its own message enum once:

```rust
enum Message {
    Save,
    NameChanged(String),
}

let (name, name_widget) = Textbox::create(TextboxParameters::new(""));
let (save, save_widget) = Button::create(ButtonParameters::new("Save"));

let mut session = Session::<Message>::new();
session.connect(name.changed(), |event| Message::NameChanged(event.text))?;
session.connect(save.submitted(), |_| Message::Save)?;

let mut subscribers = Subscribers::<Model, Message>::new();
subscribers.subscribe(|model, message, _emit| match message {
    Message::NameChanged(name) => model.name = name.clone(),
    Message::Save => model.save(),
});

context.update_ui_session(
    dimensions,
    &mut session,
    &mut model,
    &mut subscribers,
);
```

`examples/calculator.rs` uses this API for all twenty buttons. `examples/demo-full.rs` maps its
buttons, list items, combo header, text submission, and sliders into one application message enum.
Their frame callbacks synchronize presentation but never poll widget state for events.

## Boundary between the retained tree and the application

```text
raw input
    -> UiRuntime routes one native event
    -> Widget::update mutates widget-local state
    -> WidgetEventPort<E> maps E and queues Message
    -> complete retained update releases all widget borrows
    -> Session<Message> drains messages in FIFO order
    -> Subscribers<State, Message> observe each message
    -> subscribers may enqueue more Message values
    -> layout commits before routing the next raw event
```

The types at each layer are intentionally different:

```text
UiRuntime / Node / Widget        no application type parameter
TypedWidgetHandle<Widget>        weak access to one concrete widget
WidgetEvent<State, Event>        concrete native widget event
Session<Message>                 application message vocabulary
Subscribers<State, Message>      application state and message
```

## Event endpoints on typed widget handles

The retained controls expose:

- `TypedWidgetHandle<Button>::submitted()`
- `TypedWidgetHandle<Checkbox>::changed()`
- `TypedWidgetHandle<Textbox>::changed()`
- `TypedWidgetHandle<Textbox>::submitted()`
- `TypedWidgetHandle<TextArea>::changed()`
- `TypedWidgetHandle<TextArea>::submitted()`
- `TypedWidgetHandle<Slider>::changed()`
- `TypedWidgetHandle<Number>::changed()`
- `TypedWidgetHandle<Combo>::changed()`
- `TypedWidgetHandle<Combo>::submitted()`
- `TypedWidgetHandle<ListBox>::submitted()`
- `TypedWidgetHandle<ListItem>::submitted()`
- `TypedWidgetHandle<RootChrome>::changed()`
- `TypedWidgetHandle<RootChrome>::submitted()`

The endpoint is weak: it does not retain a removed widget. `Session::connect` installs one mapping
for that endpoint and the session disconnects it when dropped. One widget event connects to one
session; multicast happens after conversion to `Message`, where all subscribers share the same
statically known type.

## No erased payloads

There is no `Any`, downcast, opaque signal identity, or heterogeneous payload map. A widget event
connection stores typed behavior `Fn(Event)`: it constructs a concrete `Message` and appends that
message directly to the session inbox. Dynamic dispatch is used for the adapter callback, but every
value crossing the boundary remains compiler-checked.

The adapter runs during `Widget::update`, so it must remain construction-only and must not perform
application effects or access the emitting widget. Subscriber callbacks run later at the safe
post-traversal boundary after retained widget borrows have ended.

## Current experiment scope

- Every retained semantic event producer has a typed endpoint. The file dialog and both interactive
  examples consume typed sessions rather than polling widget state.
- Long-lived sessions prune weak connections whose widgets were removed, which lets dynamic
  subtrees such as file-dialog directory rows reconnect without retaining stale state.
- Each connection allocates its boxed `Fn(Event) -> Message` adapter once. Emission queues only the
  resulting `Message`; it does not allocate or enqueue a per-event closure.
- The application message enum can be divided into component enums and lifted with ordinary Rust
  variant constructors such as `Message::Calculator`.
