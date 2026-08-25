# Architecture

## Key concepts

- **Context**: owns the high-level `Renderer`, the only ordered input queue, retained root windows, and typed application subscriptions. `Context<B>` applications call `update_ui(dimensions)`, while `Context<B, State>` applications call `update_ui_state(dimensions, state)` to dispatch typed events. Both commit layout before `frame(FrameInfo).render_ui()?` paints and submits once; neither rebuilds roots or polls transient UI commands per frame.
- **Container**: the generic retained branch owner. It stores one erased concrete `ContainerWidget` and one authoritative opaque `Children` collection. The concrete widget owns semantic state, configuration, event ports, and layout policy; only the generic container owns children strongly.
- **Layout engine + flows**: parent container widgets measure and assign child rectangles through scoped child-aware APIs and `ContainerLayoutCtx`. Linear, Grid, and Disclosure expose their layout configuration and topology through `TypedWidgetHandle<W>`; ScrollArea accepts one arbitrary content node and owns only viewport state.
- **Widget**: the common update/paint contract. A leaf additionally implements `LeafWidget` for intrinsic measurement; a branch implements `ContainerWidget` for child-aware measurement and placement. Concrete widgets combine semantic values, interaction state, native event ports, and runtime phases; `*Parameters` are only one-shot initialization.
- **Node**: the non-cloneable owner of one concrete leaf or container runtime. Leaf storage is erased to `Rc<RefCell<dyn LeafWidget>>`; container storage erases to `Rc<RefCell<dyn ContainerWidget>>`. Applications and coordinating widgets may retain a weak `TypedWidgetHandle<W>` without affecting node lifetime. A `Node` receives private process-unique identity when constructed and transfers exactly once into a root or opaque `Children` collection; attached nodes cannot be detached or reparented.
- **Rendering**: widgets obtain a local `Painter` from `WidgetPaintCtx`; retained traversal owns the internal display list, and `Renderer` executes it through one exclusively borrowed `RendererBackend::Frame`. The portable target supports drawables up to 8192x8192 and geometry up to four maximum drawable spans beyond the viewport; see the [render subsystem guide](RENDER.md#supported-coordinate-domain) for the complete coordinate contract and integration API.
- **Typography**: atlases can bake multiple named fonts and sizes. `Style` resolves semantic roles (`body`, `small`, `title`, `heading`, `mono`) through `FontRole`, while text-bearing `*Parameters` select a per-widget font with `.font(...)`.
- **Style overrides**: every retained node can supply a `Style` in place of its inherited style. A container passes that style to its descendants until another node replaces it. The same effective value drives measurement, placement, input localization, update, and paint.
- **Application components**: application state may coordinate multiple retained windows and
  widgets behind a typed semantic API. `FileDialog` owns dialog behavior; a `Window` construction
  value transfers its body and optional declarative `MenuBar` together. Concrete `MenuItem` ports
  connect directly to the same typed application dispatcher as every other widget event.

The public API is intentionally centered on `microui_redux::prelude` for applications and `microui_redux::retained` for retained concepts such as `Node`, `Children`, `Container`, `Linear`, `Disclosure`, typed widget handles, and `Context`. Low-level rendering lives under `microui_redux::render`, and atlas construction lives under `microui_redux::atlas::builder`.

## Rendering

Widgets record backend-neutral drawing through a framework-created `Painter`; `Renderer` executes the crate-owned display list and submits final geometry through `RendererBackend`. The [render subsystem guide](RENDER.md) covers architecture, clipping, textures, custom callbacks, backend implementation, and compiling code examples.

## Retained authoring model

The supported authoring path is retained widget trees registered as context-owned roots.
Applications pass `Window::new(name, rect, body)` to `Context::create_window(...)`; the optional
`.menu_bar(MenuBar::new(...))` builder installs the bar as part of that window. An ordinary window
may directly own dialogs created with `Context::create_dialog(parent, window)`. A window or dialog
may retain generic popups created with `Context::create_popup(parent, name, content)`. Applications
mutate leaves and containers through weak
`TypedWidgetHandle<W>` values, commit contexts without application callbacks through
`Context::update_ui(...)` or subscriber-driven contexts through `Context::update_ui_state(...)`,
and paint with `Context::frame(FrameInfo).render_ui()?`.

Local styles can be installed while building a node or changed later through its typed widget
handle:

```rust
let mut section_style = *ctx.style();
section_style.spacing = 8;
section_style.padding = 6;

let (submit, submit_node) = Button::create(ButtonParameters::new("Submit"));
let (_, section) = Linear::create(LinearParameters::vertical([
    LinearItem::content(submit_node),
]));
let section = section.with_style_override(section_style);

// The button inherits section_style. After mounting, it can replace that style through its handle.
let mut submit_style = section_style;
submit_style.colors[ControlColor::Button as usize] = color(55, 90, 160, 255);
submit.try_set_style_override(submit_style);
submit.try_clear_style_override();
```

An override is a complete `Style`, so derive it from `*Context::style()` or from the intended
container style when only a few fields need to differ. As with other layout-affecting handle
mutations, call `Context::update_ui` before painting. Custom widgets can inspect the effective
value through `MeasureCtx::style`, `ContainerLayoutCtx::style`, `WidgetUpdateCtx::style`, and
`WidgetPaintCtx::style`.

Window and dialog creation consume one complete `Window` and return a non-owning `RootHandle`;
popup creation consumes one persistent application `Node` and returns the more specific non-owning
`PopupHandle`.
Roots cannot be replaced while retaining their identity: mutate descendants through a container
state's weak topology capability, or destroy and recreate the root.

### Flat windows and window-owned popups

Context stores one flat collection of `WindowEntry` values. An entry is either an independent
ordinary window or a modal dialog directly owned by an ordinary window. Dialog ownership controls
show eligibility and lifetime, but does not create a general root hierarchy: there are no child
windows, dialog nesting, reparenting, or public subpopup registration APIs.

Each entry owns one retained surface and all of its popup definitions. A popup therefore has no
independent root identity, layer, visibility flag, or destruction operation. Its typed
`PopupHandle` addresses the definition within its owner; hiding or destroying an ordinary window
also affects its directly owned dialogs, and destroying any window drops its popup definitions.

One `PopupPath` is the complete semantic visibility state for transients. It identifies one owner
window entry and an ordered parent-to-child sequence of popup definitions; every prefix is visible.
Opening another top-level popup replaces the path, opening a submenu extends or replaces its
suffix, and outside presses or owner changes truncate it while emitting dismissal events. No
parallel popup visibility state is mirrored on individual definitions.

### Window bands, transients, and activation

Independent windows occupy one of sixteen fixed application layers. Layer `0` is the bottom, layer
`15` is the top and the default, and `Context::set_root_layer(root, layer)` changes a top-level
window's `LayerBinding::Fixed(u8)`. Numeric validation uses `MIN_LAYER`, `MAX_LAYER`, and
`DEFAULT_LAYER`. Dialogs report `LayerBinding::Modal`; popups expose no layer binding and use their
owner's fixed or modal band. There is no inherited `LayerBinding` variant.

Within a fixed layer, ordinary windows retain their usual z-order and `bring_root_to_front` raises
a window only among peers in that layer. It cannot cross a higher fixed layer. Pointer hit testing
and painting consume the same complete stacking key, so overlap selects the surface visually in
front.

A composed control creates a generic popup once in its stable window owner, then calls
`show_popup(&popup)` at the current pointer or `show_popup_at(&popup, anchor)` at an exact
screen-space rectangle. Generic popups keep that screen anchor until explicitly shown elsewhere;
they do not follow later owner movement. `PopupHandle` parameters keep popup operations separate
from ordinary root operations at compile time.

An intrinsic `MenuBar` is compiled with its `Window` into one persistent bar-and-body tree plus
private popup definitions. Top-level menus retain a below-heading relation; submenus retain a
right-of-row relation to their direct parent popup. The manager resolves these node relationships
after each layout, so open menus follow window movement and ancestor menu geometry. `MenuItem`
submission ports remain directly subscribable application events; no menu coordinator, public
submenu handle, anchor cache, or second visibility model is involved.

Dialogs occupy a dedicated modal band above all sixteen numeric layers. The frontmost visible
dialog is the only input-eligible window, and its active popup path uses the transient tier above
that dialog. Other windows remain visible, laid out, and painted but cannot interact until no
dialog remains visible. Bringing a dialog forward closes transients from the previous modal owner;
hiding it reveals the next visible dialog in z-order. Raising an ordinary owner does not reorder
its dialogs because they occupy the separate modal band.

Visual order is deliberately separate from keyboard activation. A pointer press records the
ordinary `active_root` (or a popup's ordinary source) without moving it to a different fixed layer.
Keyboard and text input return to that root after the press, while pointer overlap still follows
the visual stack. Pointer drags remain with their captured root, but wheel input has no capture
lifecycle and goes to the topmost eligible root under the pointer. This lets exposed regions of a
layer-0 application surface scroll or zoom even while a layer-15 floating window remains active.
Modal policy and active pointer capture take precedence. Hiding or destroying the active root clears
the record.

A fullscreen application surface is therefore an independent window at layer `0`, not a special
root kind. Remove its chrome and outer inset, keep its rectangle synchronized with the
drawable viewport, and let independent windows use the default layer:

```rust,ignore
context.set_root_layer(surface.id(), MIN_LAYER)?;
context.set_root_options(
    surface.id(),
    WindowOption::NO_TITLE
        | WindowOption::NO_CLOSE
        | WindowOption::NO_RESIZE
        | WindowOption::NO_PADDING,
)?;
context.set_root_rect(surface.id(), rect(0, 0, dimensions.width, dimensions.height))?;
```

`NO_PADDING` removes only the root-owned content inset; descendant widgets still use the complete
`Style`, including ordinary control and container padding. `demo-full` applies this recipe to a
dedicated menu-bearing perspective X-Y grid surface with left-drag arcball rotation, wheel zoom,
and homogeneous line clipping. Its original Demo Window and the other floating windows remain
independent default-layer roots above that background.

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

let _root = ctx.create_window(Window::new("main", rect(20, 20, 240, 120), tree));
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
accessor into that same model and creates an ordinary hidden modal root. A port still accepts exactly
one state method; compose additional effects inside that method.

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

## Retained node identity

Each owning `Node` receives a private, process-unique runtime identity before mounting. Moving a
node, wrapping it in an unmounted `LinearItem` or `GridItem`, and inserting it into a container
preserve that identity; applications cannot read or construct it.
There is no public node ID or result lookup path. Weak typed widget handles expose event endpoints
after node erasure. Window chrome is manager-owned rather than represented by a retained widget;
`RootHandle` and `PopupHandle` expose only stable identity, liveness, and their typed `changed` and
`submitted` endpoints. Geometry and visibility changes pass through `Context` so cross-root policy
and layout invalidation remain centralized.

Registered roots can be configured with `Context::set_root_options(...)` and `WindowOption` to
control window chrome. Root overflow does not scroll implicitly; construct a `ScrollArea` with
`ScrollAreaOption::ENABLE_SCROLL` around one content node and use its `TypedWidgetHandle<ScrollArea>`
for offset changes. The content may be any leaf or container. It fills at least the viewport width
and keeps its desired height; explicit descendant relationships remain in the content container.
Every retained node caches preferred measurements and placement across layout passes. A geometry
change clears that node and its weakly linked ancestor caches, while topology operations do this
automatically. Ordinary traversal rejects nodes whose retained rectangles do not intersect the
inherited viewport, regardless of which container owns them.
