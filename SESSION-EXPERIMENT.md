# Typed session experiment

This branch replaces the first `Any`-backed signal experiment with one application-typed message
session. The retained tree, `UiRuntime`, `Node`, `Widget`, and `WidgetUpdateCtx` do not know the
application message type.

## Application API

Widgets expose native event endpoints through their existing state handles. The application maps
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
Their frame callbacks synchronize presentation but never consume pending widget counters.

## Boundary between the retained tree and the application

```text
raw input
    -> UiRuntime routes one native event
    -> Widget::update mutates widget-local state
    -> WidgetEventPort<E> queues a deferred E -> Message mapping
    -> complete retained update releases all state borrows
    -> Session<Message> evaluates mappings in FIFO order
    -> Subscribers<State, Message> observe each message
    -> subscribers may enqueue more Message values
    -> layout commits before routing the next raw event
```

The types at each layer are intentionally different:

```text
UiRuntime / Node / Widget        no application type parameter
WidgetStateHandle<State>         concrete widget state only
WidgetEvent<State, Event>        concrete native widget event
Session<Message>                 application message vocabulary
Subscribers<State, Message>      application state and message
```

## Event endpoints on state handles

The retained controls expose:

- `WidgetStateHandle<ButtonState>::submitted()`
- `WidgetStateHandle<CheckboxState>::changed()`
- `WidgetStateHandle<TextboxState>::changed()`
- `WidgetStateHandle<TextboxState>::submitted()`
- `WidgetStateHandle<TextAreaState>::changed()`
- `WidgetStateHandle<TextAreaState>::submitted()`
- `WidgetStateHandle<SliderState>::changed()`
- `WidgetStateHandle<NumberState>::changed()`
- `WidgetStateHandle<ComboState>::changed()`
- `WidgetStateHandle<ComboState>::submitted()`
- `WidgetStateHandle<ListBoxState>::submitted()`
- `WidgetStateHandle<ListItemState>::submitted()`
- `WidgetStateHandle<RootState>::changed()`
- `WidgetStateHandle<RootState>::submitted()`

The endpoint is weak: it does not retain a removed widget. `Session::connect` installs one mapping
for that endpoint and the session disconnects it when dropped. One widget event connects to one
session; multicast happens after conversion to `Message`, where all subscribers share the same
statically known type.

## No erased payloads

There is no `Any`, downcast, opaque signal identity, or heterogeneous payload map. A widget event
connection stores typed behavior `FnMut(Event)`, and the session's deferred mapping is
`FnOnce() -> Message`. Dynamic dispatch is used for callbacks, but every value crossing the
boundary remains compiler-checked.

Mappings are deliberately deferred. Invoking `event -> Message` inside `Widget::update` would let
an application closure run while a retained state cell is mutably borrowed. Instead the widget
only appends a typed deferred mapping; both that mapping and every subscriber run at the safe
post-traversal boundary.

## Current experiment scope

- Every retained semantic event producer has a typed endpoint. The file dialog and both interactive
  examples consume typed sessions rather than polling widget state.
- Legacy pending counters remain temporarily as a source-compatibility layer. They are no longer
  consumed by framework or example code and can be deleted independently.
- Long-lived sessions prune weak connections whose widgets were removed, which lets dynamic
  subtrees such as file-dialog directory rows reconnect without retaining stale state.
- A deferred widget event currently allocates one boxed `FnOnce() -> Message`. This preserves the
  non-reentrant boundary without erasing event data. A later implementation could use a
  session-owned slab if measurements justify removing that allocation.
- The application message enum can be divided into component enums and lifted with ordinary Rust
  variant constructors such as `Message::Calculator`.
