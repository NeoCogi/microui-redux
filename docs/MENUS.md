# Application menus

Menus are an optional, intrinsic part of a [`Window`](../src/window_manager.rs). A window owns its
body, compact [`MenuBar`](../src/menu.rs) data, and the private surfaces that present that data.
Applications describe the hierarchy once and keep only the `MenuItemHandle`s needed for later state
changes.

The public composition types are deliberately small:

| Type | Role |
| --- | --- |
| `MenuBar` | Ordered top-level menus installed on one window. |
| `Menu` | One heading or submenu, built with `item`, `separator`, and `submenu`. |
| `MenuItem` | Uniquely owned actionable value moved into exactly one menu position. |
| `MenuItemHandle` | Cloneable non-owning handle containing private stable identity and a separately projected submission endpoint. Mounted presentation is borrowed through `Ui`. |

There is no public menu-popup handle, coordinator, command enum, row widget, or parallel menu model.

## Construction and events

`MenuItem::create` returns a handle and a uniquely owned item value. Subscribe through
`handle.submitted()`, move the value into the declaration, and install the completed bar on its
owning window:

```rust
use microui_redux::prelude::*;

struct Model {
    window: WindowHandle,
    /// Retained because Save becomes enabled after a document has changed.
    save: MenuItemHandle,
}

impl Model {
    /// Handles the event source belonging only to the concrete Open item.
    fn open(&mut self, _context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        // Open the selected document.
    }
}

fn build_model<B: RendererBackend>(
    context: &mut Context<B, Model>,
) -> Result<Model, SubscribeError> {
    // The handle projects the event port; the value moves into one menu below.
    let (open, open_item) =
        MenuItem::create(MenuItemParameters::new("Open...").shortcut_hint("Ctrl+O"));
    context.subscribe_context(open.submitted(), Model::open)?;

    // This command has the same handler but remains a distinct typed event source.
    let (recent, recent_item) = MenuItem::create(MenuItemParameters::new("Recent Document"));
    context.subscribe_context(recent.submitted(), Model::open)?;

    // Keep this handle because application state will mutate the item after construction.
    let (save, save_item) = MenuItem::create(
        MenuItemParameters::new("Save")
            .shortcut_hint("Ctrl+S")
            .disabled(),
    );
    let body = TextBlock::create(TextBlockParameters::new("document")).1;
    let menu_bar = MenuBar::new([
        Menu::new("File")
            .item(open_item)
            .item(save_item)
            .separator()
            .submenu(Menu::new("Recent").item(recent_item)),
    ]);
    let window = context.ui().create_window(
        Window::new("Document", rect(20, 20, 640, 480), body).menu_bar(menu_bar),
    );

    Ok(Model { window, save })
}
```

Consuming `MenuItem` in `Menu::item` makes the relationship unforgeable: a value can occupy only
one declaration position, and a caller cannot accidentally pair one item's visible state with a
different item's submission handle.

The handle's private stable ID identifies its concrete item; the subscribed port only delivers
`MenuItemSubmitted` and carries no copied command value. The manager closes an active menu before
dispatch reaches the application handler. An enabled item without a subscriber still closes the
menu when selected; its unobserved event is simply discarded.

Use a `MenuItemHandle` as the stable identity for short-lived presentation borrows from `Ui`:

```rust
let save = ui.menu_item_mut(&save)?;
save.enabled = true;
save.label = "Save document".into();
save.shortcut_hint = Some("Ctrl+Shift+S".into());

ui.menu_item_mut(&auto_scroll)?.mark = MenuItemMark::Checked(enabled);
```

`Ui::menu_item` provides immutable inspection. Both accessors return
`MenuItemAccessError::UnknownItem` when the item is unmounted, its owning window has been
destroyed, or the handle originated in another context. A mutable borrow conservatively
invalidates layout because every public field can affect geometry or presentation.

Check and radio marks are presentation only. Application handlers own toggling and radio-group
exclusivity. Shortcut hints also draw text only; they do not register accelerators.

## Compact retained architecture

Menu entries are data, not retained widget nodes. The manager owns one concrete `MenuSurface` for
the persistent bar and one `SurfaceBody::Menu` forest node for each popup. A surface measures,
hit-tests, and paints all of its headings or rows directly. Consequently, adding an item does not
add a retained item, icon, label, shortcut, separator, or submenu-row node, and there is no
`UiRuntime`, `MenuContainer`, controller, action bridge, or runtime node identity for menus.

One geometry calculation supplies the rectangles used by measurement, hit testing, painting, and
submenu placement. Each popup derives its leading mark column from its direct items. The column
collapses completely when none of those items has a check or radio mark; otherwise every direct row
uses the shared content offset. Nested submenu popups calculate their columns independently.

Each `MenuSlot` directly owns its `MenuItemId`, `MenuItemParameters`, and strong submission port.
`MenuItemHandle` carries the same private ID and a weak projection of that port; it does not mirror
presentation state. IDs come from the process-wide non-reused retained-object namespace, so a
destroyed item's endpoint allocation can be recycled after its weak endpoints are released without
redirecting stale lookup. Mutation
through `Ui::menu_item_mut` invalidates the owning layout transaction, ensuring that role and
text-width changes resize and reanchor open popups correctly. Warm layout reuses the surface's
slot-geometry vector and the forest's popup-path workspace.

## Placement and interaction

Menu topology and placement remain manager-private. Every menu popup is a concrete forest node with
one parent edge and one trigger-slot index. A top-level popup is anchored below its heading slot in
the bar surface; a submenu popup is anchored at the right edge of its row slot in its parent popup.
The manager resolves these relationships from current surface geometry, so open menus follow their
window and parent rows without application-supplied screen coordinates or retained anchor nodes.

The manager stores only the deepest active popup. Following sole parent edges derives the one
visible heading-to-descendant chain:

- pressing a closed heading opens its popup;
- pressing the active heading closes the path;
- pressing another heading replaces the path;
- pressing a submenu row appends that child while retaining its ancestors;
- opening a sibling submenu replaces only the older descendant branch;
- selecting an enabled item or pressing outside the chain closes the complete path.

A disabled item cannot be invoked: it emits no `MenuItemSubmitted` event and does not close the
menu. The popup remains one retained input surface, so disabled rows and separators still occlude
content behind it while resolving to no menu action. Moving across headings changes hover
presentation but does not open or switch menus.

## Style and current scope

`Style::menu_foreground` colors menu labels, item text, marks, arrows, and separators.
`Style::menu_background` fills the persistent bar and popup surfaces. Menu surfaces receive the
resolved owning window style and paint directly; because they are not widget nodes, they do not run
a separate menu-node style cascade.

Menu operation is currently pointer-driven. Keyboard navigation, mnemonics, and shortcut dispatch
remain outside the menu component. The bar preserves application keyboard focus while pointer menus
operate.
