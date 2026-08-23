# Application menus

Application menus are concrete retained widgets and Context-owned roots. There is no command enum,
generic menu model, menu-specific type erasure, or copied menu specification. Each actionable item
owns a typed event port and the application registers that exact item with its handler.

The ownership flow is:

```text
MenuItem node -> MenuGroup -> Menu -> MenuPanel -> WindowMenu::create -> Context roots
```

`Node` is the unique owner while the menu is being assembled. After `WindowMenu::create`, the
`Context` owns the window tree and one popup tree per top-level menu. `WindowMenu` is an
application-owned coordinator containing weak widget and root handles; it does not own those roots.

## Concrete types

| Type | Role |
| --- | --- |
| `MenuItem` | Pointer-operated leaf widget with a label, enabled state, visual mark, shortcut hint, and typed submission port. |
| `MenuGroup` | Ordered item nodes; empty groups are skipped during assembly. |
| `Menu` | One top-level heading and its groups. |
| `MenuPanel` | Ordered top-level menus consumed during window construction. |
| `WindowMenu` | Coordinates the persistent bar, popup roots, active index, and owning window. |

None of these types has a type parameter. `MenuGroup`, `Menu`, and `MenuPanel` are one-shot
composition values. Keep a `TypedWidgetHandle<MenuItem>` only when application code needs to read or
change that retained item later.

## Construction and registration

Create and register every actionable item before moving its unique node into the menu hierarchy:

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

fn build_model<B: RendererBackend>(
    context: &mut Context<B, Model>,
) -> Result<Model, SubscribeError> {
    let (open, open_node) =
        MenuItem::create(MenuItemParameters::new("Open...").shortcut_hint("Ctrl+O"));
    WindowMenu::register_item(
        context,
        Model::menu_mut,
        &open,
        Model::open,
    )?;

    // This disabled item has no handler, so it does not need a subscription.
    let (save, save_node) = MenuItem::create(
        MenuItemParameters::new("Save")
            .shortcut_hint("Ctrl+S")
            .disabled(),
    );

    let panel = MenuPanel::new([Menu::new(
        "File",
        [MenuGroup::new([open_node, save_node])],
    )]);
    let body = TextBlock::create(TextBlockParameters::new("document")).1;
    let menu = WindowMenu::create(
        context,
        Model::menu_mut,
        "Document",
        rect(20, 20, 640, 480),
        panel,
        body,
    );

    Ok(Model { menu, save })
}
```

`WindowMenu::register_item` subscribes to that item's `MenuItemSubmitted` port. When the item
submits, the adapter closes the active popup first and then invokes the supplied application method
with `EventContext` access. The item event carries no command value or copied menu data.

Store the returned `WindowMenu` where the accessor will find it before the first
`Context::update_ui_state` call, and keep that accessor resolving to the same live coordinator while
its subscriptions can dispatch. An item port accepts one context subscription, so do not also
register the same port through another `Context::subscribe*` call.

Disabled items never emit `MenuItemSubmitted` and may be left unregistered when they have no
behavior. An enabled but unregistered item also has no coordinated action: its event is discarded,
and selecting it does not close the popup. Register every enabled item that should act like a menu
command.

## Groups and separators

A `MenuGroup` contains item nodes in display order. During menu assembly, empty groups are skipped
and a retained separator is inserted between each successive non-empty group:

```rust
let file = Menu::new("File", [
    MenuGroup::new([new_node, open_node, save_node]),
    MenuGroup::new([]), // skipped
    MenuGroup::new([clear_node]),
    MenuGroup::new([exit_node]),
]);
```

Separators are private, non-interactive widgets. Applications do not create sentinel entries or
encode separator positions in a command list.

## Live item state

Enabled and mark state have convenience methods on the typed handle. Each operation returns `None`
if the retained item has expired or is currently borrowed:

```rust
save.set_enabled(true).expect("Save item unavailable");
auto_scroll
    .set_mark(MenuItemMark::Checked(enabled))
    .expect("Auto-scroll item unavailable");
comfortable
    .set_mark(MenuItemMark::Radio(selected))
    .expect("Comfortable item unavailable");
compact
    .set_mark(MenuItemMark::Radio(!selected))
    .expect("Compact item unavailable");
```

Marks are presentation only. Selecting a checked item does not toggle it, and radio items do not
enforce exclusivity; the application handler must update those values. State changes are silent and
do not synthesize a submission.

The label and shortcut hint can be changed through the general typed mutation API:

```rust
save.try_update(|item| {
    item.set_label("Save As...");
    item.set_shortcut_hint(Some("Ctrl+Shift+S".into()));
});
```

Typed mutations invalidate retained measurement, so changed labels and hints are measured again at
the next layout commit.

## Opening, switching, and dismissal

`WindowMenu::create` creates one ordinary window containing the menu bar and supplied body, plus one
initially hidden, auto-sized popup root per top-level menu. Popup indices match heading order.

Interaction is pointer-driven:

- left-pressing a closed heading opens its existing popup below that heading;
- left-pressing the active heading toggles it closed;
- left-pressing another heading switches to that popup;
- selecting a registered, enabled item closes the active popup before its handler runs;
- an outside pointer press or replacement by another popup dismisses the active popup and clears the
  heading highlight.

Moving across headings changes hover presentation but does not switch or open menus. Opening a menu
changes root visibility and position; it does not clone a specification or rebuild item nodes.

`WindowMenu::active_menu` and `WindowMenu::is_open` report the coordinator's synchronized state.
Use `WindowMenu::close` for programmatic closure. There is no public component-aware operation for
programmatically opening a heading in this alpha.

## Root handles and lifetime

`WindowMenu::window` returns the ordinary `RootHandle`. `WindowMenu::popups` and
`WindowMenu::popup` return typed `PopupHandle` values in heading order. These are weak handles to
roots retained by `Context`.

Treat those handles as inspection and whole-component lifetime capabilities. Calling
`Context::set_root_visible` or `Context::show_popup_at` directly on a menu popup bypasses the
coordinator and can make root visibility disagree with `active_menu` and the bar highlight. Use the
heading interaction or `WindowMenu::close` for normal menu state changes.

`WindowMenu` assumes its window, bar, and every popup remain registered. Destroying one of those
roots and then calling component operations can panic. There is no component teardown helper in this
alpha; to remove a menu window permanently, destroy the window and every popup root and then discard
the `WindowMenu` without using it again.

## Current limitations

- Menu operation is pointer-only; there is no keyboard navigation or mnemonic handling.
- `shortcut_hint` draws text only. It does not register or dispatch an accelerator.
- Check and radio marks are application-managed presentation, not automatic selection behavior.
- Cascading submenus are not represented.
- Hovering another heading while a menu is open does not switch menus; press the heading instead.
- The bar and items preserve the application's existing keyboard focus while pointer menus operate.
