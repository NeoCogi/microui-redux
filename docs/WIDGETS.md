# Built-in widgets and containers

Application code normally imports these types from `microui_redux::prelude`. This guide is a map
of the built-in choices; the generated API reference remains the authority for every parameter and
method.

## Construction and ownership

Most built-ins accept a one-shot `*Parameters` value and return
`(TypedWidgetHandle<ConcreteType>, Node)`. Move the unique `Node` into a window or container and keep
the weak typed handle only when later reads, mutations, or event subscriptions are needed. Removing
the node expires its handles instead of keeping an invisible widget alive.

```rust
# use microui_redux::prelude::*;
# struct Model { checked: bool }
# impl Model {
#     fn checkbox_changed(&mut self, event: &CheckboxChanged) {
#         self.checked = event.checked;
#     }
# }
# fn install<B: RendererBackend>(
#     context: &mut Context<B, Model>,
# ) -> Result<WindowHandle, SubscribeError> {
let (checkbox, checkbox_node) =
    Checkbox::create(CheckboxParameters::new("Enable preview", false));
context.subscribe(checkbox.changed(), Model::checkbox_changed)?;

let (_, content) = Linear::create(LinearParameters::vertical([
    LinearItem::content(checkbox_node),
]));
let window = context
    .ui()
    .create_window(Window::new("settings", rect(40, 40, 260, 120), content));
# Ok(window)
# }
```

`Custom` is the stateless exception: `Custom::create` returns its runtime directly. Mount it with
`Node::widget`, or pair it with a registered backend callback through `Node::custom_render`.

## Controls

| Type | Purpose | Native event ports |
| --- | --- | --- |
| `Button` | Labeled or visual action | `ButtonSubmitted` |
| `Checkbox` | Boolean choice | `CheckboxChanged` |
| `Combo` | Selected-item and popup-header state | `ComboChanged`, `ComboSubmitted` |
| `ListBox` | Activatable label with an optional image | `ListBoxSubmitted` |
| `ListItem` | Activatable row with an optional semantic icon | `ListItemSubmitted` |
| `Slider` | Bounded scalar input | `SliderChanged` |
| `Number` | Draggable and editable numeric input | `NumberChanged` |
| `Textbox` | Single-line UTF-8 editor | `TextboxChanged`, `TextboxSubmitted` |
| `TextArea` | Multiline UTF-8 editor with retained scrolling | `TextAreaChanged`, `TextAreaSubmitted` |
| `Scrollbar` | Standalone horizontal or vertical range control | `ScrollbarChanged` |
| `TextBlock` | Read-only, optionally wrapped text | None |
| `ColorSwatch` | Color preview with an optional label | None |
| `Custom` | Stateless input/measurement surface for custom painting | None |

Events contain owned or copied semantic snapshots, so handlers do not need to borrow the emitting
widget. The [events guide](EVENTS.md) covers subscription forms, ordering, and context-aware
handlers. Combo popup content remains application-owned: handle `ComboSubmitted`, present the
choices, then call `select` on the combo handle.

## Containers

| Type | Use it for |
| --- | --- |
| `Linear` | One row or column with content-sized, fixed, or weighted child tracks |
| `Grid` | Explicit rows, columns, and spanning cells |
| `Disclosure` | A header whose retained body participates only while expanded |
| `ScrollArea` | A framed or unframed one-child viewport with retained scrollbars |

Containers exclusively own their child nodes. Topology mutations consume new nodes and return an
unaccepted value unchanged when insertion cannot occur; removed nodes are dropped rather than
detached for reparenting.

## Retained mutation

Convenience methods on `TypedWidgetHandle<W>` return `None` after the mounted widget has been
removed. Use `try_read`, `try_update`, or `try_update_with` for behavior not exposed by a specialized
method, and finish the access closure before starting retained traversal. After every programmatic
mutation, call `Context::update_ui` or `Context::update_ui_state` before depending on its visual,
interaction, or layout result. See the [layout guide](LAYOUT.md) for the synchronization contract.
