# Typed subscriber-session experiment

This branch uses C#-style typed event subscriptions. The retained tree, `UiRuntime`, `Node`,
`Widget`, and `WidgetUpdateCtx` do not know the application state type, and the application no
longer needs a shared message enum.

## Application API

Widgets expose native event endpoints through their typed widget handles. The application connects
each endpoint directly to a method on its state:

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

let (name, name_widget) = Textbox::create(TextboxParameters::new(""));
let (save, save_widget) = Button::create(ButtonParameters::new("Save"));

let mut session = Session::<Model>::new();
session.subscribe(name.changed(), Model::name_changed)?;
session.subscribe(save.submitted(), Model::save)?;

context.update_ui_session(dimensions, &mut session, &mut model);
```

When an application method needs context that is not part of the widget's native payload, it binds
that context once at subscription time:

```rust
impl Model {
    fn slider_changed(&mut self, index: &usize, event: &SliderChanged) {
        self.values[*index] = event.value;
    }
}

session.subscribe_with(slider.changed(), index, Model::slider_changed)?;
```

`examples/calculator.rs` binds each button's calculator action directly to `State::apply_action`.
`examples/demo-full.rs` binds slider indices and button labels while simple events use ordinary
`State` methods.

## Safe dispatch boundary

```text
raw input
    -> UiRuntime routes one native event
    -> Widget::update mutates widget-local state
    -> WidgetEventPort<E> queues typed State method invocations
    -> complete retained update releases all widget borrows
    -> Session<State> invokes subscribers in FIFO order
    -> layout commits before routing the next raw event
```

The types at each layer remain concrete:

```text
UiRuntime / Node / Widget        no application type parameter
TypedWidgetHandle<Widget>        weak access to one concrete widget
WidgetEventHandle<Event>         weak access to one native event port
Session<State>                   heterogeneous FIFO of typed State invocations
State::method(&Event)            application event handler
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

An endpoint is weak and does not retain a removed widget. `Session::subscribe` adds one multicast
subscriber to the endpoint, and dropping the session or calling `Session::unsubscribe` detaches
that subscriber. Multiple methods and multiple sessions may subscribe to the same widget event.

## Minimal type erasure

There is no application message type, mapping adapter, closure-erased callback, `Any`, payload
downcast, or heterogeneous payload map. Both necessary dynamic boundaries are named traits with
visible implementations:

- `EventSubscriber<Event>` lets a multicast port hold subscribers targeting different application
  state types. `MethodEventSubscriber` visibly stores its weak queue and state method;
  `BoundMethodEventSubscriber` additionally stores its shared context.
- `SubscriberInvoker<State>` lets one session FIFO contain invocations carrying different concrete
  event types. Each invocation visibly stores its event and statically typed state method.

One emitted event is placed in `Rc<Event>` so every multicast subscriber observes the same value
without requiring `Event: Clone`. Each subscriber appends one boxed invocation to its session FIFO.
Subscriber methods run only after retained widget borrows have ended.

## Current experiment scope

- Every retained semantic event producer has a typed multicast endpoint.
- The file dialog and interactive examples register state methods and contain no routing message
  enums.
- Long-lived sessions prune weak subscriptions whose widgets were removed, allowing dynamic
  subtrees such as file-dialog rows to be replaced without retaining stale state.
- `subscribe_with` stores bound context once in `Rc<Context>` and reuses it for every invocation.
