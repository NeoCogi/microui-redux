# Architecture

## Key concepts

- **Context**: owns the high-level `Renderer`, the only ordered input queue, retained root windows, and typed application subscriptions. `Context<B>` applications call `update_ui(dimensions)`, while `Context<B, State>` applications call `update_ui_state(dimensions, state)` to dispatch typed events. Both commit layout before `frame(FrameInfo).render_ui()?` paints and submits once; neither rebuilds roots or polls transient UI commands per frame.
- **Container**: the generic retained branch owner. It stores one erased concrete `ContainerWidget` and one authoritative opaque `Children` collection. The concrete widget owns semantic state, configuration, event ports, and layout policy; only the generic container owns children strongly.
- **Layout engine + flows**: parent container widgets measure and assign child rectangles through scoped child-aware APIs and `ContainerLayoutCtx`. Linear, Grid, and Disclosure expose their layout configuration and topology through `TypedWidgetHandle<W>`; ScrollArea accepts one arbitrary content node and owns only viewport state.
- **Widget**: the common update/paint contract. A leaf additionally implements `LeafWidget` for intrinsic measurement; a branch implements `ContainerWidget` for child-aware measurement and placement. Concrete widgets combine semantic values, interaction state, native event ports, and runtime phases; `*Parameters` are only one-shot initialization.
- **Node**: the non-cloneable owner of one concrete leaf or container runtime. Leaf storage is erased to `Rc<RefCell<dyn LeafWidget>>`; container storage erases to `Rc<RefCell<dyn ContainerWidget>>`. Applications and coordinating widgets may retain a weak `TypedWidgetHandle<W>` without affecting node lifetime. A `Node` receives private process-unique identity when constructed and transfers exactly once into a root or opaque `Children` collection; attached nodes cannot be detached or reparented.
- **Rendering**: widgets obtain a local `Painter` from `WidgetPaintCtx`; retained traversal owns the internal display list, and `Renderer` executes it through one exclusively borrowed `RendererBackend::Frame`. The portable target supports drawables up to 8192x8192 and geometry up to four maximum drawable spans beyond the viewport; see the [render subsystem guide](RENDER.md#supported-coordinate-domain) for the complete coordinate contract and integration API.
- **Typography**: atlases can bake multiple named fonts and sizes. `Style` resolves semantic roles (`body`, `small`, `title`, `heading`, `mono`) through `FontRole`, while text-bearing `*Parameters` select a per-widget font with `.font(...)`.

The public API is intentionally centered on `microui_redux::prelude` for applications and `microui_redux::retained` for retained concepts such as `Node`, `Children`, `Container`, `Linear`, `Disclosure`, typed widget handles, and `Context`. Low-level rendering lives under `microui_redux::render`, and atlas construction lives under `microui_redux::atlas::builder`.

## Rendering

Widgets record backend-neutral drawing through a framework-created `Painter`; `Renderer` executes the crate-owned display list and submits final geometry through `RendererBackend`. The [render subsystem guide](RENDER.md) covers architecture, clipping, textures, custom callbacks, backend implementation, and compiling code examples.

## Retained authoring model

The supported authoring path is retained widget trees registered as context-owned roots.
Applications call `Context::create_window(...)`, `Context::create_dialog(...)`, or
`Context::create_popup(...)` once, mutate leaves and containers through weak
`TypedWidgetHandle<W>` values, commit contexts without application callbacks through
`Context::update_ui(...)` or subscriber-driven contexts through `Context::update_ui_state(...)`,
and paint with `Context::frame(FrameInfo).render_ui()?`.

Root creation consumes one persistent application `Node` and returns a non-owning `RootHandle`.
Roots cannot be replaced while retaining their identity: mutate descendants through a container
state's weak topology capability, or destroy and recreate the root. Visibility is controlled with
`set_root_visible`.
A visible dialog is modal: it stays above every window and popup, receives all eligible pointer,
keyboard, text, focus, and capture routing, and blocks interaction with other roots until hidden or
destroyed. Other roots remain visible and continue to be laid out and painted.

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

let _root = ctx.create_window("main", rect(20, 20, 240, 120), tree);
let dimensions = Dimensioni::new(800, 600);
let info = FrameInfo::try_new(dimensions, color(20, 22, 26, 255))?;
ctx.subscribe(name_submitted, Model::name_submitted)?;
let mut model = Model::default();
ctx.update_ui_state(dimensions, &mut model);
ctx.frame(info).render_ui()?;
```

An event-driven context is constructed as `Context::<Backend, Model>::new(backend)`. It owns the
sole application dispatcher for its complete root forest. Each application-subscribed source queues
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
payloads, and the typed handle projects the corresponding weak `WidgetEventHandle<E>` without
exposing or owning the erased node. Disclosure headers remain real addressable leaf children, while
the concrete `Disclosure` widget owns expansion state and the weak body-topology capability.

## Retained node identity

Each owning `Node` receives a private, process-unique runtime identity before mounting. Moving a
node, wrapping it in an unmounted `LinearItem` or `GridItem`, and inserting it into a container
preserve that identity; applications cannot read or construct it.
There is no public node ID or result lookup path. Weak typed widget handles expose event endpoints
after node erasure, while root chrome exposes its rectangle, visibility, and active mode through
`RootHandle::widget()` and its typed endpoints through `RootHandle::{changed, submitted}`.

Registered roots can be configured with `Context::set_root_options(...)` and `WindowOption` to
control window chrome. Root overflow does not scroll implicitly; construct a `ScrollArea` with
`ScrollAreaOption::ENABLE_SCROLL` around one content node and use its `TypedWidgetHandle<ScrollArea>`
for offset changes. The content may be any leaf or container. It fills at least the viewport width
and keeps its desired height; explicit descendant relationships remain in the content container.
Every retained node caches preferred measurements and placement across layout passes. A geometry
change clears that node and its weakly linked ancestor caches, while topology operations do this
automatically. Ordinary traversal rejects nodes whose retained rectangles do not intersect the
inherited viewport, regardless of which container owns them.
