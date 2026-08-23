# Application menus

Application menus use the same concrete retained widgets, typed handles, event ports, nodes, layout
containers, and Context-owned roots as the rest of microui-redux. There is no command type and no
generic menu data model.

The ownership flow is:

```text
MenuItem node -> MenuGroup -> Menu -> MenuPanel -> WindowMenu
```

Create and register each actionable item first. Then move its unique Node through the concrete
hierarchy. Keep a TypedWidgetHandle<MenuItem> only when application state must later change that
item.

## Concrete types

| Type | Role |
| --- | --- |
| `MenuItem` | Retained leaf widget with label, enabled state, marker, hint, and its own event port. |
| `MenuGroup` | Ordered item nodes. Adjacent non-empty groups receive a separator. |
| `Menu` | One top-level heading and its concrete groups. |
| `MenuPanel` | Ordered top-level menus installed into a window. |
| `WindowMenu` | Coordinates the persistent bar, popup roots, and owning window. |

None of these types has a type parameter. No `Any` or runtime downcast is used.

## Construction and registration

```rust
use microui_redux::prelude::*;

struct Model {
    menu: WindowMenu,
    save: TypedWidgetHandle<MenuItem>,
}

impl Model {
    fn menu_mut(state: &mut Self) -> &mut WindowMenu {
        &mut state.menu
    }

    fn open(
        &mut self,
        _context: &mut EventContext<'_>,
        _event: &MenuItemSubmitted,
    ) {
        // Open the document.
    }
}

let (open, open_node) =
    MenuItem::create(MenuItemParameters::new("Open...").shortcut_hint("Ctrl+O"));
WindowMenu::register_item(
    &mut context,
    Model::menu_mut,
    &open,
    Model::open,
)?;

let (save, save_node) =
    MenuItem::create(MenuItemParameters::new("Save").shortcut_hint("Ctrl+S").disabled());

let panel = MenuPanel::new([
    Menu::new("File", [
        MenuGroup::new([open_node, save_node]),
    ]),
]);

let body = TextBlock::create(TextBlockParameters::new("document")).1;
let menu = WindowMenu::create(
    &mut context,
    Model::menu_mut,
    "Document",
    rect(20, 20, 640, 480),
    panel,
    body,
);
# Ok::<(), SubscribeError>(())
```

`WindowMenu::register_item` is generic only over the renderer and application state needed by
`Context`. It connects that exact item's concrete `MenuItemSubmitted` port, closes the active
menu, and calls the supplied application method. The menu types and event payload remain concrete.

As with other application-owned components, store the returned `WindowMenu` at the accessor's
location before the first context update. An item port accepts one subscription, so do not also
subscribe the same item through `Context::subscribe_context`.

Disabled items may be left unregistered when they have no behavior.

## Groups and separators

A `MenuGroup` contains item nodes in display order. `Menu` inserts a retained separator widget
between adjacent non-empty groups:

```rust
let file = Menu::new("File", [
    MenuGroup::new([new_node, open_node, save_node]),
    MenuGroup::new([clear_node]),
    MenuGroup::new([exit_node]),
]);
```

This keeps separators structural. Applications do not create sentinel entries or encode separator
positions in a command list.

## Live item state

State lives on the concrete item and changes through its typed handle:

```rust
save.set_enabled(true);
auto_scroll.set_mark(MenuItemMark::Checked(enabled));
comfortable.set_mark(MenuItemMark::Radio(selected));
compact.set_mark(MenuItemMark::Radio(!selected));
```

These mutations are silent. They invalidate retained measurement when needed and do not synthesize a
user submission. Dropping the menu roots expires the weak handles.

`MenuItemParameters::shortcut_hint` is presentation-only. Logical shortcut dispatch remains a
separate input concern.

## Roots and dismissal

`WindowMenu::create` creates:

- one ordinary window containing the bar and supplied body;
- one hidden, auto-sized popup root per top-level menu.

Opening a heading shows its already-retained popup at the heading anchor. It does not clone a
specification or rebuild item nodes. Selecting a registered item closes the active popup before the
application handler runs. Outside presses and popup replacement use ordinary
`RootSubmitted::PopupDismissed` policy and reconcile the bar automatically.

Use `WindowMenu::window` to inspect the ordinary `RootHandle`, or `WindowMenu::popups` and
`WindowMenu::popup` to inspect the typed `PopupHandle` values.
Use `WindowMenu::close` for programmatic closure so root and bar state remain synchronized.
