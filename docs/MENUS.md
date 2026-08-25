# Application menus

Menus are an optional, intrinsic part of a [`Window`](../src/window_manager.rs). A window owns its
body, persistent [`MenuBar`](../src/menu.rs), and the private popup surfaces compiled from that bar.
Applications describe the hierarchy once and keep only the concrete `MenuItem` handles they need
to update later.

The public composition types are deliberately small:

| Type | Role |
| --- | --- |
| `MenuBar` | Ordered top-level menus installed on one window. |
| `Menu` | One heading or submenu, built with `item`, `separator`, and `submenu`. |
| `MenuItem` | Actionable retained widget with enabled state, an optional mark and shortcut hint, and a typed submission port. |

There is no public menu-popup handle, coordinator, command enum, or parallel menu model.

## Construction and events

Create each concrete item, subscribe directly to its `MenuItemSubmitted` port, and move its unique
node into the declaration. Then install the completed bar on the owning window:

```rust
use microui_redux::prelude::*;

struct Model {
    root: RootHandle,
    save: TypedWidgetHandle<MenuItem>,
}

impl Model {
    fn open(&mut self, _context: &mut EventContext<'_>, _event: &MenuItemSubmitted) {
        // Open the selected document.
    }
}

fn build_model<B: RendererBackend>(
    context: &mut Context<B, Model>,
) -> Result<Model, SubscribeError> {
    let (open, open_node) =
        MenuItem::create(MenuItemParameters::new("Open...").shortcut_hint("Ctrl+O"));
    context.subscribe_context(open.submitted(), Model::open)?;

    let (recent, recent_node) = MenuItem::create(MenuItemParameters::new("Recent Document"));
    context.subscribe_context(recent.submitted(), Model::open)?;

    let (save, save_node) = MenuItem::create(
        MenuItemParameters::new("Save")
            .shortcut_hint("Ctrl+S")
            .disabled(),
    );
    let body = TextBlock::create(TextBlockParameters::new("document")).1;
    let menu_bar = MenuBar::new([
        Menu::new("File")
            .item(open_node)
            .item(save_node)
            .separator()
            .submenu(Menu::new("Recent").item(recent_node)),
    ]);
    let root = context.create_window(
        Window::new("Document", rect(20, 20, 640, 480), body).menu_bar(menu_bar),
    );

    Ok(Model { root, save })
}
```

The subscribed port identifies its concrete item; `MenuItemSubmitted` carries no copied command
value. The manager closes an active menu before dispatch reaches the application handler. An
enabled item without a subscriber still closes the menu when selected; its unobserved event is
simply discarded.

Keep a `TypedWidgetHandle<MenuItem>` only for live presentation state:

```rust
save.set_enabled(true).expect("Save item unavailable");
auto_scroll
    .set_mark(MenuItemMark::Checked(enabled))
    .expect("Auto-scroll item unavailable");
```

Check and radio marks are presentation only. Application handlers own toggling and radio-group
exclusivity. Shortcut hints also draw text only; they do not register accelerators.

## Placement and interaction

Menu topology and placement are manager-private relationships. A top-level popup is anchored below
its retained heading. A submenu popup is anchored at the right edge of its retained submenu row.
The manager resolves those node rectangles during layout, so open menus follow their window and
their parent rows without application-supplied screen coordinates.

The manager holds one active popup path. For menus, that means one visible
heading-to-descendant chain:

- pressing a closed heading opens its popup;
- pressing the active heading closes the path;
- pressing another heading replaces the path;
- pressing a submenu row appends that child while retaining its ancestors;
- opening a sibling submenu replaces only the older descendant branch;
- selecting an enabled item or pressing outside the chain closes the complete path.

A disabled `MenuItem` does not participate in hit testing, does not emit `MenuItemSubmitted`, and
does not close the menu. Moving across headings changes hover presentation but does not open or
switch menus.

## Style and current scope

`Style::menu_foreground` colors menu labels, item text, marks, arrows, and separators.
`Style::menu_background` fills the persistent bar and popup surfaces. Ordinary cascading node style
resolution still applies.

Menu operation is currently pointer-driven. Keyboard navigation, mnemonics, and shortcut dispatch
remain outside the menu component. The bar preserves application keyboard focus while pointer menus
operate.
