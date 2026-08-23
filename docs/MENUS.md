# Per-window application menus

`WindowMenu<Command>` adds an application menu to one retained window. The application owns the
component and its semantic `MenuBarSpec<Command>`; `Context` continues to own the window, popup,
and widget trees. Selecting an enabled item emits `MenuInvoked<Command>` through the ordinary typed
event dispatcher.

This is deliberately a per-window design. A menu bar is mounted in the window's retained layout,
so multiple windows may expose different commands and state without introducing active-application
or active-window policy into `Context`. Platform integrations may translate the same typed menu
model into a native global menu later, but that is not part of the portable retained contract.

## Model and component boundary

The public types have distinct responsibilities:

| Type | Responsibility |
| --- | --- |
| `MenuBarSpec<Command>` | Ordered collection of top-level menus. |
| `MenuSpec<Command>` | One heading and its flat list of entries. |
| `MenuEntry<Command>` | Either a command item or a separator group boundary. |
| `MenuItemSpec<Command>` | Label, command, enabled state, marker, and shortcut hint. |
| `WindowMenu<Command>` | Coordinates the retained bar, popup panel, and typed events. |
| `MenuInvoked<Command>` | Application-facing selection payload. |

`Command` is application-defined and must implement `Clone + 'static`. It should normally be a
small enum. Add `PartialEq` when the application needs the live enabled/check/radio mutation APIs.

## Creating a menu window

Store the component in application state, provide an accessor to that stable location, and
subscribe to its command port once:

```rust
use microui_redux::prelude::*;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Command {
    Open,
    Save,
    WordWrap,
    About,
}

struct Model {
    menu: WindowMenu<Command>,
    word_wrap: bool,
}

impl Model {
    fn menu_mut(model: &mut Self) -> &mut WindowMenu<Command> {
        &mut model.menu
    }

    fn menu_invoked(&mut self, _context: &mut EventContext<'_>, event: &MenuInvoked<Command>) {
        match event.command {
            Command::Open => {
                // Open an application-owned document or dialog.
            }
            Command::Save => {
                // Save the active document.
            }
            Command::WordWrap => {
                self.word_wrap = !self.word_wrap;
                self.menu.set_checked(&Command::WordWrap, self.word_wrap);
            }
            Command::About => {
                // Show application information.
            }
        }
    }
}

let specification = MenuBarSpec::new([
    MenuSpec::new(
        "File",
        [
            MenuEntry::item("Open...", Command::Open).shortcut_hint("Ctrl+O"),
            MenuEntry::item("Save", Command::Save).shortcut_hint("Ctrl+S").disabled(),
        ],
    ),
    MenuSpec::new(
        "View",
        [
            MenuEntry::item("Word Wrap", Command::WordWrap).checked(true),
            MenuEntry::separator(),
            MenuEntry::item("About", Command::About),
        ],
    ),
]);

let body = TextBlock::create(TextBlockParameters::new("Document content")).1;
let menu = WindowMenu::create(
    &mut context,
    Model::menu_mut,
    "Document",
    rect(30, 30, 480, 320),
    specification,
    body,
);
let invoked = menu.invoked();
context.subscribe_context(invoked, Model::menu_invoked)?;

let mut model = Model {
    menu,
    word_wrap: true,
};
```

The component must be installed at the supplied accessor before the first
`Context::update_ui_state` call. This matches `FileDialog`: construction records typed internal
subscriptions, while later dispatch resolves the component through application state.

## Entries and groups

`MenuEntry::separator()` creates a non-interactive group boundary. Command entries may be disabled
or decorated with check/radio state:

```rust
let view = MenuSpec::new(
    "View",
    [
        MenuEntry::item("Show Grid", Command::WordWrap).checked(false),
        MenuEntry::separator(),
        MenuEntry::item("Comfortable", Command::Open).radio(true),
        MenuEntry::item("Compact", Command::Save).radio(false),
    ],
);
```

Radio behavior is intentionally application-owned. When one choice is invoked, update every item
in that logical group. This keeps command semantics explicit and avoids hiding a selection model in
presentation widgets:

```rust
menu.set_radio(&Command::Open, false);
menu.set_radio(&Command::Save, true);
menu.set_enabled(&Command::Save, document_is_dirty);
```

Each setter updates every matching command and returns the number of changed entries. If the
affected top-level menu is open, its retained popup snapshot is refreshed immediately. The initial
topology is fixed; create a new component and roots when an application needs a structurally
different set or order of menus.

## Retained roots and popup placement

`WindowMenu::create` creates two roots:

- `window()` is the ordinary visible root. Its one application node is a vertical shell containing
  the menu bar above the supplied content.
- `popup()` is a hidden auto-sized popup root reused for each top-level menu.

The panel is a separate root so it is not clipped to the owning window and participates in generic
popup z-order, outside dismissal, modal blocking, and replacement policy. A bar submission carries
the current screen-space heading anchor. `EventContext::show_popup_at` installs that anchor and
shows the popup atomically before the next layout commit.

Only one generic popup is visible in a `Context` at a time. Opening another menu, combo, or popup
dismisses the current one and publishes `RootSubmitted::PopupDismissed`; `WindowMenu` consumes that
lifecycle event to reconcile its bar and semantic open state. Use `WindowMenu::close` for an
application-requested close so all three authorities remain synchronized.

## Styling and current scope

Menus use the current semantic `Style` rather than a separate skin:

- `PanelBG` paints the bar and panel backgrounds.
- `ButtonHover` and `ButtonFocus` paint hovered and open headings/items.
- `Text` paints enabled labels and shortcut hints; disabled text uses reduced alpha.
- The theme's check icon paints checked items; radio choices use a compact filled marker.

The first menu contract is pointer-driven and flat. Shortcut hints are presentation strings, not
active accelerators. Logical-key navigation, mnemonics, and cascading submenus remain future work;
they require typed key chords, focus traversal, and popup-family ownership rather than control-local
special cases. The flat API does not simulate these features with text input.

See `demo-full` for File, View, and Help menus with live checked/radio state, disabled entries,
separator groups, shortcut hints, and a file-dialog command.
