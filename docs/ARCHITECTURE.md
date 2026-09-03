# Architecture

## Key concepts

- **Context**: owns the crate-private render executor, the only ordered input queue, the concrete
  retained surface forest, and typed application subscriptions. `Context<B>` applications call
  `update_ui(dimensions)`, while `Context<B, State>` applications call
  `update_ui_state(dimensions, state)` to dispatch typed events. Both commit layout before
  `frame(FrameInfo).render_ui()?` paints and submits once; neither rebuilds windows or polls
  transient UI commands per frame.
- **Container**: the generic retained branch owner. It stores one erased concrete `ContainerWidget` and one authoritative opaque `Children` collection. The concrete widget owns semantic state, configuration, event ports, and layout policy; only the generic container owns children strongly.
- **Layout engine + flows**: parent container widgets measure and assign child rectangles through scoped child-aware APIs and `ContainerLayoutCtx`. Linear, Grid, and Disclosure expose their layout configuration and topology through `TypedWidgetHandle<W>`; ScrollArea accepts one arbitrary content node and owns only viewport state.
- **Widget**: the common update/paint contract. A leaf additionally implements `LeafWidget` for intrinsic measurement; a branch implements `ContainerWidget` for child-aware measurement and placement. Concrete widgets combine semantic values, interaction state, native event ports, and runtime phases; `*Parameters` are only one-shot initialization.
- **Node**: the non-cloneable owner of one concrete leaf or container runtime. Leaf storage is erased to `Rc<RefCell<dyn LeafWidget>>`; container storage erases to `Rc<RefCell<dyn ContainerWidget>>`. Applications and coordinating widgets may retain a weak `TypedWidgetHandle<W>` without affecting node lifetime. A `Node` receives private process-unique identity when constructed and transfers exactly once into a window, an application popup, or an opaque `Children` collection; attached nodes cannot be detached or reparented.
- **Rendering**: widgets obtain a local `Painter` from `WidgetPaintCtx`; retained traversal owns the internal display list, and Context's private executor submits it through one exclusively borrowed `RendererBackend::Frame`. The portable target supports drawables up to 8192x8192 and geometry up to four maximum drawable spans beyond the viewport; see the [render subsystem guide](RENDER.md#supported-coordinate-domain) for the complete coordinate contract and integration API.
- **Typography**: atlases can bake multiple named fonts and sizes. `Skin` resolves semantic roles
  (`body`, `small`, `title`, `heading`, `mono`) through stable `FontRef` values, while text-bearing
  `*Parameters` select a per-widget font with `.font(...)`.
- **Skin**: `Context` owns one complete value shared by manager-owned window chrome and every
  retained node. The same value drives measurement, placement, input localization, update, and
  paint; nodes and containers add no override or inheritance layer. See the [skin
  architecture](SKINNING.md) for its concrete tables and ownership boundaries.
- **Application components**: application state may coordinate multiple retained windows and
  widgets behind a typed semantic API. `FileDialog` owns dialog behavior; a `Window` construction
  value transfers its body and optional declarative `MenuBar` together. Each `MenuItemHandle`
  carries private stable identity and projects its concrete item's submission port to the same
  typed application dispatcher as every other widget event.

The public API is intentionally centered on `microui_redux::prelude` for applications and `microui_redux::retained` for retained concepts such as `Node`, `Children`, `Container`, `Linear`, `Disclosure`, typed widget handles, and `Context`. Low-level rendering lives under `microui_redux::render`, and atlas construction lives under `microui_redux::atlas::builder`.

## Rendering

Widgets record backend-neutral drawing through a framework-created `Painter`; Context's private executor consumes the crate-owned display list and submits final geometry through `RendererBackend`. The [render subsystem guide](RENDER.md) covers architecture, clipping, textures, custom callbacks, backend implementation, and compiling code examples.

## Retained authoring model

The supported authoring path is retained widget trees registered as context-owned windows and
application popups. Applications pass `Window::new(name, rect, body)` to
`Context::ui().create_window(...)`; the optional
`.menu_bar(MenuBar::new(...))` builder installs the bar as part of that window. An ordinary window
may own screen-space structural children created with `Ui::create_child_window(&parent, window)`;
its `Window::child_window_clip(ChildWindowClip::Content)` construction policy optionally confines
their complete descendant surfaces to its application body. An independent or child window may
directly own dialogs created with `Ui::create_dialog(&parent, window)`. A window or dialog may retain
generic widget popups created with `Ui::create_popup(&parent, name, content)` or compact popup menus
created from the ordinary recursive `Menu` declaration with `Ui::create_menu_popup`. Applications
mutate leaves and containers through weak
`TypedWidgetHandle<W>` values, commit contexts without application callbacks through
`Context::update_ui(...)` or subscriber-driven contexts through `Context::update_ui_state(...)`,
and paint with `Context::frame(FrameInfo).render_ui()?`.

The application constructs one complete skin and installs it on the context:

```rust
let mut skin = ctx.skin().clone().with_metrics(|metrics| {
    metrics.spacing = 8;
    metrics.padding = 6;
});
let button_state = ControlState::Enabled(PointerState::Normal);
let mut button = skin.control(ControlRole::Button, button_state);
button.content_color = color(55, 90, 160, 255);
skin.set_control(ControlRole::Button, button_state, button);
ctx.set_skin(skin);
```

The context value styles manager-owned window chrome and every retained widget. Nodes, containers,
windows, and typed widget handles do not own local overrides. Call `Context::update_ui` after a
layout-affecting replacement and before painting. Custom widgets inspect the same value through
`MeasureCtx::skin`, `ContainerLayoutCtx::skin`, `WidgetUpdateCtx::skin`, and `WidgetPaintCtx::skin`.

Window and dialog creation consume one complete `Window` and return a non-owning `WindowHandle`;
popup creation consumes one persistent application `Node` and returns the distinct non-owning
`PopupHandle`. Creation validates the parent before transferring those unique values. A rejected
child window or dialog therefore returns `SurfaceCreationError<Window>`, while a rejected popup
returns `SurfaceCreationError<Node>`; `reason()` identifies the policy failure and `into_input()`
returns the unchanged owner for retry. There is no public numeric window or popup identifier. Every
checked `Ui` operation accepts the complete handle, whose private process-unique ID must belong to
the receiving forest. Event endpoints remain separate weak subscription capabilities and are never
used as object keys. Borrowed operations report a stale or foreign window as
`SurfaceMutationError::UnknownWindow`; popup-only mutations analogously return
`SurfaceMutationError::UnknownPopup`. The same concrete reason type reports ownership and policy
failures as `InvalidDialogOwner`, `InvalidChildWindowParent`, `InvalidPopupParent`, `InvalidLayer`,
or `ManagedLayer`.

Windows cannot be replaced while retaining their identity: mutate descendants through a container
state's weak topology capability, or destroy and recreate the window.

### Concrete surface forest and parent edges

`Context` owns one private `SurfaceForest`. Its single `Vec<SurfaceNode>` retains independent and
child windows, dialogs, application popups, and menu popups; no parallel `WindowEntry` registry,
child list, or popup-owner adapter exists. Each node combines common screen-space geometry with a
concrete role:

- a window or dialog owns one application `Node` and its `UiRuntime`, window policy, the unified
  `WindowEvent` port, and an optional concrete menu-bar `MenuSurface`;
- an application-addressable popup owns its anchor and `PopupEvent` port, plus either one
  application `Node` with its `UiRuntime` or one compact `MenuSurface`;
- a private relational menu popup owns its compact `MenuSurface` and parent trigger-slot index.

The surface layer uses concrete enums for those bodies and roles. It does not store `Any`, erased
surface payloads, or a manager-side menu controller. Widget implementations remain erased at the
existing leaf/container boundary inside `Node`; that is independent of surface ownership.

Every forest node has at most one parent edge. An independent window has none; a structural child
or dialog points to its ordinary owner; a top-level popup points to a window or dialog; and a
submenu menu surface points to its direct popup parent. Following edges derives both the top-level
family and popup owner without another relationship table. The restricted constructors admit only
ordinary child-window nesting and directly owned modal dialogs: there is no reparenting, nested
dialog ownership, or public subpopup-registration API. Destroying a window recursively destroys
every child window, dialog, and popup below it. Hiding a parent removes its complete family from
effective visibility while retaining each child's local visibility intent; showing the parent
restores those locally visible children, but does not resurrect modal dialogs closed while hiding.

A popup has no independent layer, visibility flag, or destruction operation. `SurfaceForest`
stores only the deepest active popup key and derives the visible parent-to-child chain by following
parent edges. Opening another top-level popup replaces the branch, opening a submenu extends or
replaces its suffix, and outside presses or owner changes truncate it. Each removed application
popup emits exactly one `PopupEvent::Dismissed`; private menu popups need no public lifecycle port.
No `PopupPath` object or parallel per-popup visibility state can disagree with that active key.

### Chronological order and visible traversal

The relative order of window and dialog nodes in the forest vector is the authoritative activation
chronology. Creating a visible window appends it. Showing or explicitly fronting a window or dialog
moves that complete `SurfaceNode` to the vector tail; its stable handle, event allocation, parent
edges, and owned widget allocations remain valid. For independent windows this changes order among
fixed-layer peers. For a child it changes order only among siblings with the same direct parent.
Changing an independent window's fixed layer mutates only that node's mode, so moving a complete
family between layers does not manufacture a new activation.

The forest rebuilds one allocation-reusing `visible_order` after an ordering, layer, visibility, or
popup-path change. It scans independent roots once for each fixed layer from `MIN_LAYER` to
`MAX_LAYER`, recursively inserts each locally visible child family in parent-first order, and then
scans modal nodes. The active popup chain is inserted at its owning fixed or modal transient tier.
Layout, retained updates, and diagnostics consume that materialized traversal directly.

Painting and hit testing use the same parent edges rather than a second z-order model. One family
paints each parent background and application body, then its child families in sibling chronology,
then the parent's intrinsic menu bar and manager chrome. Hit testing performs the inverse: parent
menu/chrome, child families in reverse sibling chronology, then the parent body. This gives the
required sandwich without a general overlay graph or render-command reordering. The popup branch
keeps its existing transient tier above the ordinary families in its inherited fixed band. Stable
sorting, z-index counters, child arrays, and separate fixed/modal order arrays remain unnecessary.

### Window bands, transients, and activation

Independent windows occupy one of sixteen fixed application layers. Layer `0` is the bottom, layer
`15` is the top and the default, and `Ui::set_window_layer(&window, layer)` changes an independent
window's `LayerBinding::Fixed(u8)`. Structural children have no separately mutable layer: they
inherit the fixed band of their top-level family, report that effective `LayerBinding::Fixed(u8)`
through `Ui::window_layer`, and reject `set_window_layer` with `ManagedLayer`. Numeric validation
uses `MIN_LAYER`, `MAX_LAYER`, and `DEFAULT_LAYER`. Dialogs report `LayerBinding::Modal`; popups
expose no layer binding and use their owner's fixed or modal band. No extra inherited binding variant
or cached per-child layer is needed.

Within a fixed layer, independent windows retain chronological order. Children retain a separate
chronological order at each direct parent. `Ui::bring_window_to_front(&window)` raises an
independent family among fixed-layer peers or a child family among its siblings; it cannot escape
that structural scope or cross a higher fixed layer. Pointer hit testing is the inverse of the
recursive family paint order, so overlap selects the surface whose pixels are visually in front.

A composed control creates a generic popup once in its stable window owner, then calls
`show_popup(&popup)` at the current pointer or `show_popup_at(&popup, anchor)` at an exact
screen-space rectangle. Generic popups keep that screen anchor until explicitly shown elsewhere;
they do not follow later owner movement. `PopupHandle` parameters keep popup operations separate
from window operations at compile time.

An intrinsic `MenuBar` is consumed with its `Window` into one compact concrete `MenuSurface` for the
bar and one directly owned `MenuSurface` body for each private popup definition. These are not
retained widget leaves and do not own `UiRuntime` values. Logical entries remain values inside the
menu surfaces rather than becoming retained row widgets. Top-level menus retain a
below-heading-slot relation; submenus retain a right-of-row-slot relation to their direct parent
popup. Each surface reuses storage for the local rectangles produced by its authoritative
measurement, and the manager translates those slots after layout, so open menus follow window
movement and ancestor menu geometry. `MenuItemHandle` capabilities remain directly
addressable through `Ui` and project a separate `submitted()` event endpoint; no menu coordinator,
public submenu handle, per-row anchor node,
generic surface payload, temporary recursive menu tree, or second visibility model is involved.

Dialogs occupy a dedicated modal band above all sixteen numeric layers. The frontmost visible
dialog is the only input-eligible window, and its active popup path uses the transient tier above
that dialog. Other windows remain visible, laid out, and painted but cannot interact until no
dialog remains visible. Bringing a dialog forward closes transients from the previous modal owner;
hiding it reveals the next visible dialog in z-order. Raising an ordinary owner does not reorder
its dialogs because they occupy the separate modal band.

Visual order is deliberately separate from keyboard activation. A pointer press records its exact
root or popup `SurfaceKey` without moving the owning family to a different fixed layer, and
keyboard/text routing returns to that concrete surface. Each widget runtime stores one persistent
focused `RuntimeNodeId` independently from pointer capture. Validation and key routing walk the
retained tree to that identity while enforcing every ancestor's participation and clip gates.
`Tab` and `Shift+Tab` replace the ID with the next eligible `KeyboardBehavior::TAB_STOP` surface in
retained sibling order, wrap at the ends, and skip hidden, clipped, disabled, or
pointer-focus-only surfaces.

Showing an application-addressable popup selects its concrete surface immediately. A widget popup
focuses its first Tab stop after geometry is committed and then wraps Tab traversal inside its own
tree. A popup menu instead uses the same direct-row keyboard navigation as an intrinsic menu.
Escape or an outside press dismisses either body and restores its direct parent surface, whose
independently retained focused widget ID was never discarded. An Escape dismissal retains no
key-tail state: only the initial non-repeated press is the dismissal command, while later repeats
and release transitions route normally against the restored parent surface.

`Ctrl+F6` selects the next visible independent or child window in activation chronology and
`Ctrl+Shift+F6` selects the previous one, wrapping at both ends. Selection reuses ordinary
within-scope raising and changes only the manager's active surface, so every window runtime preserves
its focused widget. An application popup closes before the switch; hidden roots are skipped, an
intrinsic menu retains its narrower keyboard scope, and an active modal dialog cannot be escaped.

`WindowOption::DISABLED` is independent from activation and visibility. A disabled root remains in
layout, update, paint, and hit-test order so retained state stays current and its pixels still
occlude lower windows, but it is excluded from pointer, keyboard, popup, menu, and chrome routing.
Painting passes one explicit enabled fact through base window chrome, the intrinsic menu, and
the root widget runtime; it does not synthesize disabled state merely because a different window is
active. Structural children inherit their parent's disabled policy. Modal dialogs own their policy
independently, allowing an enabled dialog to remain usable above a disabled owner.

The frontmost dialog replaces the ordinary active window as the keyboard scope while modal. An
intrinsic or standalone menu uses the same active `SurfaceKey` as windows and widget popups. The
root key denotes an intrinsic bar; each popup key denotes one concrete popup container. A
`MenuSurface` retains only its selected direct-child slot, so the active surface plus that slot is
the menu's complete focus route and no parallel `keyboard_menu_root` exists. Menu navigation
consumes key/text delivery but preserves the widget runtime's path so closing the menu resumes it
exactly. Pointer drags remain with their captured surface, while wheel input has no capture lifecycle
and goes to the topmost eligible surface under the pointer. Hiding or destroying an active surface
or structural ancestor repairs that same identity from the retained forest.

The manager projects that same routing decision into paint. Only the current keyboard surface
receives a visible focused state; nonselected runtimes preserve their target without drawing duplicate
carets or fills. Each widget paints the focused state of its semantic role during the ordinary tree
pass, while the active owner window selects the active state of its title and frame roles. During menu navigation
the menu selection replaces the suspended widget cue while the owning window remains visibly active.

A fullscreen application surface remains an ordinary independent window, not a special surface
kind. Give its `Window` a content clip policy, put the root in the desired fixed layer, remove its
title/resize/padding chrome, keep its rectangle synchronized with the drawable viewport, and create
the floating surfaces as its children:

```rust,ignore
let surface = context.ui().create_window(
    Window::new("desktop", rect(0, 0, 1, 1), desktop_content)
        .menu_bar(desktop_menu)
        .child_window_clip(ChildWindowClip::Content),
);
context.ui().set_window_layer(&surface, MIN_LAYER)?;
context.ui().set_window_options(
    &surface,
    WindowOption::NO_TITLE
        | WindowOption::NO_CLOSE
        | WindowOption::NO_RESIZE
        | WindowOption::NO_PADDING,
)?;
context
    .ui()
    .set_window_rect(&surface, rect(0, 0, dimensions.width, dimensions.height))?;
let tool = context.ui().create_child_window(
    &surface,
    Window::new("tool", rect(40, 40, 300, 450), tool_content),
)?;
```

`ChildWindowClip::Content` intersects each direct child's inherited clip with the committed
application-body rectangle. Root chrome removes the frame, title, and intrinsic menu bar in that
order, then applies the window-owned content inset only to the remaining application body.
Descendants accumulate clipping ancestors, but their authoritative rectangles stay in screen
coordinates. `ChildWindowClip::None`, the default, preserves the inherited viewport clip while
keeping the same content/children/overlay order. `NO_PADDING` removes only the window-owned content
inset; descendant widgets still use the complete `Skin`, including ordinary control and container
padding.

`demo-full` applies this exact recipe to a menu-bearing perspective X-Y grid family root with
left-drag arcball rotation, wheel zoom, and homogeneous line clipping. The original Demo Window and
the other floating windows are content-clipped children: they render above the grid body, remain
below the root's Grid/Help bar, and inherit the root's layer without a special desktop subsystem.

```rust
#[derive(Default)]
struct Model {
    submitted_names: Vec<String>,
}

impl Model {
    fn name_submitted(&mut self, event: &TextboxSubmitted) {
        self.submitted_names.push(event.text.clone());
    }
}

let (name, name_node) = Textbox::create(TextboxParameters::new(""));
let name_submitted = name.submitted();
let (_, label_node) = TextBlock::create(TextBlockParameters::new("Name"));
let (_, tree) = Linear::create(LinearParameters::horizontal(
    [
        LinearItem::fixed(label_node, 120),
        LinearItem::flex(name_node, 1.0),
    ],
));

let _window = ctx.ui().create_window(Window::new("main", rect(20, 20, 240, 120), tree));
let dimensions = Dimensioni::new(800, 600);
let info = FrameInfo::try_new(dimensions, color(20, 22, 26, 255))?;
ctx.subscribe(name_submitted, Model::name_submitted)?;
let mut model = Model::default();
ctx.update_ui_state(dimensions, &mut model);
ctx.frame(info).render_ui()?;
```

An event-driven context is constructed as `Context::<Backend, Model>::new(backend)`. It owns the
sole application dispatcher for its complete retained window set. Each subscribed source queues
its own typed payloads, and the context drains those queues into `Model` after retained widget
borrows have ended. Library components can participate without entering the window manager:
applications own `FileDialog` values in `Model`, while the component binds its controls through an
accessor into that same model, creates a hidden modal dialog window, stores its `WindowHandle`, and
matches `WindowEvent::CloseRequested` from the unified event port. A port still accepts exactly one
state method; compose additional effects inside that method.

Retained trees are the supported public authoring path. Each non-cloneable `Node` owns one concrete
leaf or one generic `Container`. A container owns its opaque children and one concrete branch
widget; the branch widget directly owns its semantic state, configuration, event ports, and weak
`ChildrenHandle` capability. Successful insertion transfers a node, while removal, clearing, or
replacement drops the removed runtime owner. Grid placement belongs to the concrete `Grid` widget,
not to generic nodes: plain nodes occupy one cell, while `GridItem::spanned(node, columns, rows)`
supplies an explicit parent-child span that can later be changed through
`TypedWidgetHandle<Grid>::try_update` without replacing the child. Every concrete widget is
accessed through `TypedWidgetHandle<W>`. Widgets implement `TypedWidget<E>` for native event
payloads, and the typed handle projects the corresponding weak `WidgetEventPortHandle<E>` without
exposing or owning the erased node. Disclosure headers remain real addressable leaf children, while
the concrete `Disclosure` widget owns expansion state and the weak body-topology capability.

## Retained identity and event endpoints

Each owning `Node` receives a private, process-unique runtime identity before mounting. Moving a
node, wrapping it in an unmounted `LinearItem` or `GridItem`, and inserting it into a container
preserve that identity; applications cannot read or construct it.
There is no public node ID or result lookup path. Weak typed widget handles expose event endpoints
after node erasure.

Windows and dialogs, every application or private menu popup, and menu items use a separate
retained-object identity source. Every object receives a private non-zero `u64` from one
process-wide monotonic allocator. Values are not reissued after destruction; checked advancement
stops before wraparound. The IDs are process-local implementation keys, not persistent application
identifiers. Concrete `RootId`, `PopupId`, and `MenuItemId` wrappers keep operations type-safe,
while their shared namespace prevents collisions between Contexts.

`WindowHandle`, `PopupHandle`, and `MenuItemHandle` aggregate one of those private IDs with the
weak endpoint associated with the same object. Aggregation is only an application convenience:
forest and menu lookup compares the ID, while `events()` or `submitted()` clones only the endpoint.
No code casts, stores, or compares an event-port pointer as object identity. A retained object
strongly owns its ports for the appropriate lifetime, so endpoint liveness normally follows object
liveness without defining it. If an object later exposes several ports, each remains a separately
named endpoint beside the same stable object ID.

Window chrome is manager-owned rather than represented by a retained widget. `WindowHandle::events`
projects concrete variants for geometry changes, close requests, minimization, maximization, and
restoration; the manager applies the new geometry or visibility before queuing an observation.
`PopupHandle::events` projects `PopupEvent::Dismissed` whenever policy removes that application
popup from the active branch. Private menu popups instead publish the selected item's
`MenuItemHandle::submitted` event.

The stable handles select mutations through the short-lived `Ui<'_>` façade returned by
`Context::ui()` and passed to context-aware event handlers. This keeps ordinary application code
and event-time code on one API while preventing a handle from another Context from addressing a
surface. For example:

```rust,ignore
fn window_event(&mut self, ui: &mut Ui<'_>, event: &WindowEvent) {
    match event {
        WindowEvent::GeometryChanged { rect } => self.last_rect = *rect,
        WindowEvent::CloseRequested => {
            ui.destroy_window(&self.window).expect("window must remain registered");
        }
        WindowEvent::Minimized { rect }
        | WindowEvent::Maximized { rect }
        | WindowEvent::Restored { rect } => {
            self.last_rect = *rect;
        }
    }
}

fn popup_event(&mut self, event: &PopupEvent) {
    if matches!(event, PopupEvent::Dismissed) {
        self.popup_open = false;
    }
}

context.subscribe_context(window.events(), Model::window_event).unwrap();
context.subscribe(popup.events(), Model::popup_event).unwrap();
context.ui().set_window_visible(&window, true).unwrap();
context.ui().show_popup_at(&popup, anchor).unwrap();
```

Registered windows can be configured with `Ui::set_window_options(&window, ...)` and
`WindowOption` to control chrome. Window overflow does not scroll implicitly; construct a
`ScrollArea` with `ScrollAreaOption::ENABLE_SCROLL` around one content node and use its
`TypedWidgetHandle<ScrollArea>` for offset changes. The content may be any leaf or container. It
fills at least the viewport width and keeps its desired height; explicit descendant relationships
remain in the content container.
Every retained node caches preferred measurements and placement across layout passes. A geometry
change clears that node and its weakly linked ancestor caches, while topology operations do this
automatically. Ordinary traversal rejects nodes whose retained rectangles do not intersect the
inherited viewport, regardless of which container owns them.
