# microui-redux

[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

`microui-redux` is a retained, backend-agnostic Rust GUI toolkit inspired by
[rxi/microui](https://github.com/rxi/microui). It keeps microui's compact rendering model while
using unique owning `Node` trees, typed weak widget handles, context-owned roots, and typed backend
frames.

> **Development status:** this source tree is being prepared for the 0.8 alpha. Its package metadata
> still identifies it as `0.8.0-pre-alpha`; the alpha version will be finalized as a separate
> release step. The 0.8 line is a breaking retained-API redesign and is not API-compatible with
> 0.7.

Compared with [microui-rs](https://github.com/neocogi/microui-rs), this crate embraces standard
library types, reusable retained trees, and richer widgets such as custom rendering callbacks,
dialogs, and a file dialog.

## Documentation

- [Rendering and backend integration](src/render/RENDER.md)
- [Typed event architecture](#context-owned-typed-events)
- [Version history](#version-080-pre-alpha)
- [`simple` example](examples/simple.rs) and
  [`retained-custom-drawing` example](examples/retained-custom-drawing.rs)

## Dependency and backend

During this prerelease documentation pass, the package version remains:

```toml
[dependencies]
microui-redux = "0.8.0-pre-alpha"
```

`microui-redux` does not create a native window or graphics device. Applications provide a
`RendererBackend`; the repository examples contain SDL-based Glow, Vulkan, and WGPU integrations.
The `example-*` features enable those repository examples and are not a runtime backend-selection
API for downstream applications.

## Demo

Clone the repository and run the demo with one backend feature:

```bash
cargo run --example demo-full --features example-vulkan
cargo run --example demo-full --features example-glow
cargo run --example demo-full --features example-wgpu
```

`example-backend` is only a shared gate for example code paths; it is not runnable by itself.
Running with only `--features example-backend` will fail intentionally at compile time.
Backend features are additive for Cargo tooling. If several are enabled together, examples select
Glow first, then Vulkan, then WGPU; enable only the backend you want for normal interactive runs.

`demo-full` loads `examples/FACEPALM.png` and `assets/suzanne.obj` from disk at runtime. Run it
from the repository root so those relative paths resolve.

For a smaller release executable with runtime-loaded assets, build without default features and
enable exactly one backend plus `builder`:

```bash
cargo build \
  --release \
  --example demo-full \
  --no-default-features \
  --features "example-glow builder"
```

This keeps demo assets outside the executable: fonts/icons are read from `assets/`, the external
demo image is read from `examples/FACEPALM.png`, and the Suzanne mesh is read from
`assets/suzanne.obj`. To inspect real binary section size rather than asset size, use
`size -A target/release/examples/demo-full`.

For the smallest Linux executable, use the `build-min-size` Cargo alias with nightly. It builds for
a dedicated `x86_64-unknown-linux-min-size` platform target, rebuilds `std`, uses immediate-abort
panics, omits Rust unwind tables and panic formatting details, and strips symbols and the linker
build ID. Normal builds remain on their selected toolchain and platform:

```bash
cargo +nightly build-min-size \
  --example demo-full \
  --no-default-features \
  --features "example-glow builder"
```
The executable is written to
`target/x86_64-unknown-linux-min-size/min-size/examples/demo-full`. The alias accepts ordinary Cargo
feature and package-selection arguments; replace `example-glow` with `example-vulkan` or
`example-wgpu` when needed. It requires the nightly `rust-src` component (`rustup component add
rust-src --toolchain nightly`).

![microui-redux demo with retained windows and controls](res/microui.png)

## Key Concepts

- **Context**: owns the high-level `Renderer`, the only ordered input queue, and retained root windows. Applications enqueue through Context methods, call `update_ui(dimensions)` to drain input and commit layout, use typed widget handles between traversals, synchronize again if a mutation can affect layout, then call `frame(FrameInfo).render_ui()?` to paint and submit once.
- **Container**: the generic retained branch owner. It stores one erased concrete `ContainerWidget` and one authoritative opaque `Children` collection. The concrete widget owns semantic state, configuration, event ports, and layout policy; only the generic container owns children strongly.
- **Layout engine + flows**: parent container widgets measure and assign child rectangles through scoped child-aware APIs and `ContainerLayoutCtx`. Linear, Grid, and Disclosure expose their layout configuration and topology through `TypedWidgetHandle<W>`; ScrollArea accepts one arbitrary content node and owns only viewport state.
- **Widget**: the common update/paint contract. A leaf additionally implements `LeafWidget` for intrinsic measurement; a branch implements `ContainerWidget` for child-aware measurement and placement. Concrete widgets combine semantic values, interaction state, native event ports, and runtime phases; `*Parameters` are only one-shot initialization.
- **Node**: the non-cloneable owner of one concrete leaf or container runtime. Leaf storage is erased to `Rc<RefCell<dyn LeafWidget>>`; container storage erases to `Rc<RefCell<dyn ContainerWidget>>`. Applications and coordinating widgets may retain a weak `TypedWidgetHandle<W>` without affecting node lifetime. A `Node` receives private process-unique identity when constructed and transfers exactly once into a root or opaque `Children` collection; attached nodes cannot be detached or reparented.
- **Rendering**: widgets obtain a local `Painter` from `WidgetPaintCtx`; retained traversal owns the internal display list, and `Renderer` executes it through one exclusively borrowed `RendererBackend::Frame`. The portable target supports drawables up to 8192x8192 and geometry up to four maximum drawable spans beyond the viewport; see the [render subsystem guide](src/render/RENDER.md#supported-coordinate-domain) for the complete coordinate contract and integration API.
- **Typography**: atlases can bake multiple named fonts and sizes. `Style` resolves semantic roles (`body`, `small`, `title`, `heading`, `mono`) through `FontRole`, while text-bearing `*Parameters` select a per-widget font with `.font(...)`.

The public API is intentionally centered on `microui_redux::prelude` for applications and `microui_redux::retained` for retained concepts such as `Node`, `Children`, `Container`, `Linear`, `Disclosure`, typed widget handles, and `Context`. Low-level rendering lives under `microui_redux::render`, and atlas construction lives under `microui_redux::atlas::builder`.

### Rendering

Widgets record backend-neutral drawing through a framework-created `Painter`; `Renderer` executes the crate-owned display list and submits final geometry through `RendererBackend`. The [render subsystem guide](src/render/RENDER.md) covers architecture, clipping, textures, custom callbacks, backend implementation, and compiling code examples.

### How `SelectedBackend::Frame<'a>` works

`SelectedBackend` is not a special type supplied by microui-redux. The examples define it as an
ordinary compile-time alias for one concrete renderer. The feature guards use Glow, Vulkan, then
WGPU precedence so the aliases remain well-defined when additive Cargo tooling enables several
backend features:

```rust
#[cfg(feature = "example-glow")]
use common::glow_renderer::GLRenderer as SelectedBackend;
#[cfg(all(not(feature = "example-glow"), feature = "example-vulkan"))]
use common::vulkan_renderer::VulkanRenderer as SelectedBackend;
#[cfg(all(
    not(feature = "example-glow"),
    not(feature = "example-vulkan"),
    feature = "example-wgpu"
))]
use common::wgpu_renderer::WgpuRenderer as SelectedBackend;

type SelectedFrame<'a> =
    <SelectedBackend as RendererBackend>::Frame<'a>;
```

`RendererBackend::Frame<'a>` is a generic associated type (GAT): each backend chooses its own
active-frame type, and that type may borrow the backend for `'a`. For example, the GL backend
selects `GlFrame<'a>`, WGPU selects `WgpuFrame<'a>`, and Vulkan selects `VulkanFrame<'a>`. The
fully qualified alias above is simply an unambiguous way to spell “the frame type associated with
the backend selected by this build.”

The relevant part of the backend contract is:

```rust
trait RendererBackend {
    type Frame<'a>: RendererFrame
    where
        Self: 'a;

    fn frame(&mut self, info: FrameInfo) -> Result<Self::Frame<'_>, FrameError>;
    // atlas and persistent-texture methods omitted
}
```

`Self: 'a` permits the concrete frame to contain a borrow of its backend. Because Cargo selects a
concrete `SelectedBackend`, the compiler monomorphizes the callback and its frame methods; this is
not a `dyn RendererBackend` or runtime backend switch. If a selected frame does not implement an
inherent extension method used by a callback, that backend selection fails at compile time.

There are two frame values and one shared frame trait in the public lifecycle:

| Type | What it represents | What ending it does |
| --- | --- | --- |
| `ContextFrame<'ctx, B>` | The application-level paint/submission frame. It exclusively borrows `Context<B>` while committed retained UI is painted and recorded. | `render_ui(self)` paints and submits once. Dropping without submission cancels. |
| `B::Frame<'backend>` | The backend-level RAII frame. It exclusively borrows the concrete backend only while the recorded display list is executing. | Its `Drop` implementation performs backend-specific, best-effort finalization. WGPU/Vulkan submit and present there; GL flushes before the outer window runner swaps buffers. |
| `RendererFrame` | The common trait implemented by every `B::Frame<'_>`. | Defines the standard UI operations: atlas quads/triangles, flush boundaries, and external textures. |

The lifetime is created by the borrow of `&mut B` in `RendererBackend::frame`; applications do not
choose it and should not try to make it `'static`. While the returned frame exists, it owns the
backend's exclusive mutable borrow. Safe Rust therefore prevents acquiring a second frame or
retaining the frame after rendering, and the API exposes no parallel mutable backend handle. This
is the ownership guarantee that replaces a stateful `begin`/`end` protocol.

`Context`, the selected backend, its frames, and registered render callbacks stay on their owning
thread. The rendering traits intentionally have no `Send` or `Sync` requirement. Cross-thread work
should deliver owned application data before `Context::frame`; custom callbacks execute
synchronously during display-list execution.

The complete sequence is:

```text
application/resource updates + ordered input calls
        |
Context::update_ui[_state](...)       drain FIFO; full update + layout after each event
        |
Context::frame(FrameInfo)             logical ContextFrame
        |
ContextFrame::render_ui(self)         paint-only internal display-list recording
        |
Renderer preflight                    validate texture/custom-render keys
        |
RendererBackend::frame(&mut backend)  acquire SelectedBackend::Frame<'_>
        |
Internal display-list execution       RendererFrame calls + typed custom callbacks
        |
drop backend frame                    finalize/submit/present as applicable; release borrow
        |
drop logical frame                    release the exclusive Context borrow
```

The renderer acquires the backend frame only after resource preflight succeeds. Normal UI
operations use the backend-neutral `RendererFrame` methods. Immediately before a custom-render
operation, the renderer closes the current UI batch and invokes the registered callback with
`&mut SelectedFrame<'_>`. Later UI commands continue after that callback, preserving painter
order. Acquisition failures are returned as `FrameError` before callbacks execute. Once a frame
exists, its `Drop` path must be non-panicking and best-effort because Rust destructors cannot
return a presentation error.

This concrete type is useful because a backend frame may expose additional inherent methods that
are deliberately absent from `RendererFrame`: a render-pass encoder, a mesh submission method, or
another backend-specific command recorder. The example backends all provide
`enqueue_colored_vertices`, so a selected-backend callback can call it directly:

```rust
let cube_renderer = ctx.register_custom_renderer({
    let angle = angle.clone();
    move |frame: &mut SelectedFrame<'_>, args: CustomRenderArgs| {
        let area = CustomRenderArea {
            rect: args.content_area,
            clip: args.view,
        };
        let vertices = build_cube_vertices(args.content_area, white_uv, angle.get());

        // This is an inherent SelectedFrame method, not part of RendererFrame.
        frame.enqueue_colored_vertices(area, vertices);
    }
})?;

let cube = CubeBuilder::create_widget(CubeParameters);
let tree = Node::custom_render(cube, cube_renderer);
let _root = ctx.create_window("Cube", rect(40, 40, 360, 360), tree);
```

`register_custom_renderer` accepts a callback valid for every frame borrow lifetime. In expanded
form its important bound is `for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs)`. That
higher-ranked lifetime means the callback can use the active frame but cannot save it in captured
state. The returned `CustomRenderHandle<B>` is tagged with `B`, so registration and removal through
a Context using another backend type fail at compile time. `Node::custom_render` erases the handle
to its backend-neutral registry key after checking the backend type; a key originating from any
other Context is rejected by its foreign registry namespace during renderer preflight before a
backend frame is acquired.

`CustomRenderArgs` carries the geometry needed at execution time:

- `content_area` is the full screen-space rectangle allocated to the custom widget;
- `view` is the authoritative final visible rectangle after operation, content-area, viewport,
  window, and scroll clipping;
- `dimensions` is the validated drawable size of the active frame.

Renderer does not invoke the callback when that intersection is empty. Do not intersect
`content_area` and `view` again inside the callback.

Custom callbacks receive no input and should not acquire another frame, mutate the atlas, or
finalize/present the backend frame. Update application/widget state before `Context::frame`; inside
the callback, read that state and record work on the supplied frame. Use `Painter` instead when the
drawing can be expressed with portable UI primitives.

The complete, documented [backend-frame cube example](examples/backend-frame-cube.rs) implements a
small retained `CubeWidget`. It uses `rs_math3d`'s `Vec3f`, `Quatf`, `lookat`, `perspective`, and
`project3` primitives to transform the cube, depth-sorts its faces, and submits the resulting
triangles through `SelectedFrame::enqueue_colored_vertices`. Run it with exactly one backend:

```bash
cargo run --example backend-frame-cube --features example-glow
cargo run --example backend-frame-cube --features example-vulkan
cargo run --example backend-frame-cube --features example-wgpu
```

### Current retained authoring model

The supported authoring path is retained widget trees registered as context-owned roots. Applications call `Context::create_window(...)`, `Context::create_dialog(...)`, or `Context::create_popup(...)` once, mutate leaves and containers through weak `TypedWidgetHandle<W>` values, commit polling contexts with `Context::update_ui(...)` or event-driven contexts with `Context::update_ui_state(...)`, and paint with `Context::frame(FrameInfo).render_ui()?`.

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

An event-driven context is constructed as `Context::<Backend, Model>::new(backend)`. It owns
the sole event dispatcher for its complete root forest. Each subscribed widget port queues its own
native payloads, and the context drains those queues into `Model` after retained widget borrows
have ended. A port accepts one state method; compose additional effects inside that method.

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

### Context-owned typed events

The retained UI is one transaction domain. A `Context<B, State>` owns the hardware-input FIFO, all
window/dialog/popup roots, and one typed event dispatcher for `State`. Widgets remain independent
of the application state type: each concrete widget owns only its native
`WidgetEventPort<Event>`.

The example above registers native widget endpoints with `Context::subscribe`. Bound application
values can be attached without changing native widget payloads:

```rust
context.subscribe_with(slider.changed(), index, Model::slider_changed)?;
```

There is no public standalone `Session`. Polling-only contexts use `Context<B>` and
`Context::update_ui`; event-driven contexts use `Context<B, State>` and
`Context::update_ui_state`.

```text
Context input FIFO
    -> route one raw event through the eligible root tree
    -> widget mutates local state and appends E to WidgetEventPort<E>
    -> complete cross-root update releases retained widget borrows
    -> context dispatcher drains subscribed ports into &mut State
    -> layout commits before the next raw event is routed
```

The initial synchronization pass also drains events queued by programmatic widget changes, even
when no raw input is waiting. Dispatch repeats until every subscribed port is empty, so finite
events emitted by state methods complete in the same transaction. A cascade limit detects
accidental feedback loops. The limit is checked after each drained subscription batch, so one
large batch may cross the threshold before the dispatcher panics.

Event ownership and ordering follow these rules:

- A widget is the sole strong owner of its typed event ports.
- `WidgetEventHandle<Event>` and context subscription records hold weak port references.
- Each port accepts one context subscription and discards events while unsubscribed.
- Removing a widget drops its pending events; dead context bindings are pruned during dispatch.
- Dropping the context dispatcher disconnects its live ports.
- FIFO is preserved within each port. When several ports have pending events at one boundary,
  subscription order determines their dispatch order.
- One state method owns the effects for one port; application-level fan-out is ordinary method
  composition rather than multicast event infrastructure.

There is intentionally no total chronology across independent ports. An event emitted into a
subscription later in the current sweep can run during that sweep; one emitted into the current
or an earlier subscription runs in the next sweep. Application logic that requires a total order
should express it inside one state method or one event type.

The context dispatcher is the one dynamic boundary. It erases the concrete event type of each
subscription so one `Context<B, State>` can subscribe to heterogeneous native widget events.

### Retained node identity

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

### Preferred sizing, tracks, and retained layout

Layout traverses the retained node tree in three stages:

```text
constraints flow down -> desired sizes flow up -> exact rectangles flow down
```

A parent is the immediate container holding a child. During measurement, the parent sends each
child `Constraints`: a width and height that are independently bounded or unbounded. These are
questions about available space, not assigned sizes. Each child answers with its desired size, and
containers combine those answers into desired sizes reported toward the root. Once the root has an
actual rectangle, every parent walks downward again and assigns each child an exact
`Recti { x, y, width, height }`.

A **track** is the one-dimensional unit a parent sizes while turning those measurements into
rectangles. It starts as a rule—`Content`, `Fixed`, or `Flex`—and resolves to one non-negative pixel
extent:

```text
horizontal Linear inside 600 px

| Content: 80 | gap: 10 | Flex(1): 400 | gap: 10 | Fixed: 100 |
|   child 0   |         |    child 1   |         |   child 2  |
```

For a horizontal `Linear`, one track supplies each child's width; for a vertical `Linear`, one
track supplies each child's height. The container combines that main-axis extent with its
cross-axis rule to produce the child's rectangle.

“Slot” is close to “track” for Linear, but it would obscure why Linear and Grid use the same sizing
vocabulary and scalar resolver. They remain separate container algorithms: each measures its own
children and places its own rectangles, and Grid does not construct, contain, or delegate to a
Linear widget. Grid independently passes its row and column track data to the common resolver in
`layout.rs`. Several children can use the same row or column track, and one child can span several
tracks. A child's slot is the resulting two-dimensional area; a track is only one row, column,
width, or height used to form that area. Tracks are parent-owned because the container stores and
interprets these rules—the child reports desired content without knowing whether its parent will
place it in a content, fixed, or flexible track.

- Every built-in leaf reports its own intrinsic preferred size from content metrics (text/icon/thumb/line layout), while every container measures against its authoritative child collection.
- A consumed or captured event conservatively dirties its recipient's retained measurement; the runtime propagates that invalidation through dependent ancestors at the next layout boundary. Typed-handle mutations use the same propagation path, so widget implementations do not manage layout caches.
- `LeafWidget::measure` and `ContainerWidget::measure` report desired content, not an allocation. `Constraints` represents each axis as `AvailableSpace::Bounded(i32)` or `AvailableSpace::Unbounded`; bounded zero is not an unconstrained request.
- Auto-sized roots measure unbounded axes intrinsically. `TrackSize::Content` uses desired extent, `TrackSize::Fixed` stays exact, and `TrackSize::Flex` shares bounded space left after content, fixed tracks, and spacing while falling back to desired content when unbounded.
- `Context::update_ui` first synchronizes layout, then drains input in API-call order. Every event runs one complete eligible-tree `Widget::update` traversal and one follow-up layout, so geometry changed by one event is authoritative for routing the next.
- `ContextFrame::render_ui` performs no input, update, or layout work. It paints the committed tree with `Widget::paint` and submits one display list; missing, stale, pending-input, or dimension-mismatched commits return `RenderError::UiUpdateRequired` before backend acquisition.
- Leaves and containers share the public `Widget` update/paint contract. `LeafWidget` adds intrinsic measurement; `ContainerWidget` adds child-aware measurement, placement, and optional surface event filtering.
- Parent containers assign each node one exact retained parent-local allocation. Sizing relationships belong to parent-child edges such as `LinearItem`; `Node` has no global placement policy. Child offsets and clips remain node-local and are resolved through a composed transform during traversal.
- Resolved outer rectangles and clips remain runtime stack locals. Node behavior works against its local content surface, while outer frame painting, standard hit routing, and conversion from screen input remain runtime-owned.
- A public widget's Painter geometry and routed pointer positions share the derived content-local origin.
- Built-in leaf and container constructors return a weak `TypedWidgetHandle<W>` plus one completed owning `Node`. Concrete container constructors consume child nodes, and `Node::custom_render` plus `Node::typed_custom_render` cover backend-typed custom-render leaves.
- Every direction is a configuration of one `Linear` widget. Linear and Grid independently invoke the common scalar track resolver, so both apply identical content/fixed/flex, spacing, rounding, and overflow arithmetic without either container being implemented through the other.
- `LinearCrossSize` gives every direction the same shared-line choices: desired content, stretching across exact allocation, or an exact fixed cross extent. `LinearDirection` combines axis and leading edge.
- Negative desired extents are normalized to zero at the node boundary. A desired zero remains zero; generic containers do not substitute Style-owned fallback cells.

Built-in leaves and containers are mutated through their typed widget handles between commits. After programmatic state/topology changes, call `update_ui` even when no input is pending so layout is synchronized before paint. Feed raw input through methods such as `mousemove`, `mousedown`, `scroll`, `keydown_code`, and `text`; calls are queued without coalescing. A widget receives the current event as `Option<&UiInputEvent>`, while `WidgetUpdateCtx::{mouse_buttons,key_modes,key_codes}` exposes held state after that event was applied.

`ContextFrame` holds the Context borrow needed to serialize paint/submission, but it does not lock independent typed widget or root handles and there is no Context access token. Do not keep a typed-access closure active while retained update/layout/paint can reach that same widget. Framework recursion through a container's scoped child visitor is the intentional exception. If layout-affecting state changes after the last commit, drop any unsubmitted frame and call `update_ui` again before paint.

The application owns `Context` and its weak typed handles as independent Rust values, so the
compiler permits explicitly capturing the Context inside a handle-access closure. Do not initiate
retained traversal that way:

```rust
textbox.try_update(|widget| {
    widget.set_text("hello");
    context.update_ui(dimensions); // unsupported: the mutable widget borrow is still active
});
```

`try_update` holds the concrete widget's checked `RefCell` borrow until its closure returns. If the nested
layout, update, or paint traversal reaches that widget, the runtime's checked borrow is
incompatible and panics with a diagnostic naming the runtime phase. This is the reentrancy guard;
there is no separate Context lock. Finish typed access before committing instead:

```rust
textbox.set_text("hello").expect("textbox unavailable");
context.update_ui(dimensions);
```

Update and paint visit a node before its eligible children and visit siblings in forward order. A
successful mutation of a later, currently available state cell is visible when traversal reaches
it; mutating an already-updated sibling does not rerun that sibling. A container's active child
visitor borrow makes mutation of that same container return `None`, while another available subtree
may change. There is no transaction snapshot or rollback, but every input transaction ends with a
complete layout before the next event is routed.

`Widget::paint` is observational with respect to application-authored semantic state, topology,
interaction, and committed layout. A built-in widget may publish framework-owned, paint-derived
read-only geometry for later application use—`TypedWidgetHandle<Combo>::anchor`, for example—or update a private
rendering cache, but neither can alter the current commit. Registered custom-render callbacks may
update only callback-private rendering caches. Mutating retained UI through an independently
captured typed widget handle during either callback violates the contract; it is not a deferred-next-frame
update. Commit semantic changes before creating the frame.

## Fonts and typography

- Atlas building supports multiple baked fonts and sizes through `atlas::builder::FontAsset`, and the same config can drive both runtime atlas construction and offline/prebuilt atlas export.
- `Context::new(...)` binds the conventional atlas font keys `body`, `small`, `title`, `heading`, and `mono`, plus the built-in semantic icon keys, onto the default `Style`. `Context::set_style(...)` also rebinds font and icon fields that are still left at their default values, so tweaking colors or spacing on top of `Style::default()` preserves the atlas's semantic bindings.
- Text-bearing widget Parameters expose `.font(FontChoice)`, so you can either select a semantic role (`FontRole::Heading.into()`) or a concrete baked font ID (`atlas.font_id("caption").unwrap().into()`).
- Font sizes are selected by choosing another baked font variant, not by scaling one bitmap font at runtime.
- `examples/demo-full` uses this directly: `NORMAL.ttf` for control/body text, `BOLD.ttf` for window titles, and `CONSOLE.ttf` for the log window’s input/output text.

### Text encoding and glyph coverage

All public text enters the library as Rust `str` or `String` values and is therefore valid UTF-8.
Textboxes and text areas retain arbitrary UTF-8 and keep their byte cursor on Unicode scalar-value
boundaries. Left/right movement and deletion operate on one scalar value at a time, not on a
user-perceived grapheme cluster. Combining sequences and multi-scalar emoji can therefore require
more than one cursor or deletion operation.

Rendering coverage is a separate atlas concern. Text measurement and drawing iterate Unicode
scalar values and use the same lookup rules:

- a character present in the selected atlas font uses its own glyph metrics and rectangle;
- a missing character uses the selected font's underscore (`_`) entry;
- if underscore is also absent, the runtime uses a synthetic 8-by-8 fallback rectangle at the
  atlas origin.

The built-in atlas builder bakes only printable ASCII (`U+0020` through `U+007E`), which includes
underscore. `AtlasSource` can describe arbitrary Unicode scalar values, so applications needing
broader coverage must provide their own glyph table and should always include `_`. There is not
yet a configurable glyph-range option in `builder::Config`.

The text pipeline does not perform grapheme segmentation, script shaping, bidirectional
reordering, kerning, or fallback-font selection. `TextWrap::Word` wraps only at ASCII space
boundaries; an individual word is not split when it exceeds the available width.

An application-provided atlas must have at least one font and must reserve icon index zero for an
opaque white tile used by solid geometry. The standard style also expects the semantic icon names
`close`, `expand`, `collapse`, `check`, `expand_down`, `open_folder`, `closed_folder`, and `file`.
The current loader does not validate the complete contract, so treat atlas metadata as trusted
input and keep every glyph/icon rectangle within the declared texture dimensions.

```rust
use microui_redux::{atlas::builder, prelude::*};

const ICONS: &[builder::IconAsset<'static>] = &[
    builder::IconAsset { name: "close", path: "assets/CLOSE.png" },
    builder::IconAsset { name: "expand", path: "assets/PLUS.png" },
    builder::IconAsset { name: "collapse", path: "assets/MINUS.png" },
    builder::IconAsset { name: "check", path: "assets/CHECK.png" },
    builder::IconAsset { name: "expand_down", path: "assets/EXPAND_DOWN.png" },
    builder::IconAsset { name: "open_folder", path: "assets/OPEN_FOLDER_16.png" },
    builder::IconAsset { name: "closed_folder", path: "assets/CLOSED_FOLDER_16.png" },
    builder::IconAsset { name: "file", path: "assets/FILE_16.png" },
];

const FONTS: &[builder::FontAsset<'static>] = &[
    builder::FontAsset {
        name: "body",
        path: "assets/NORMAL.ttf",
        size: 12,
    },
    builder::FontAsset {
        name: "small",
        path: "assets/NORMAL.ttf",
        size: 10,
    },
    builder::FontAsset {
        name: "title",
        path: "assets/BOLD.ttf",
        size: 12,
    },
    builder::FontAsset {
        name: "heading",
        path: "assets/NORMAL.ttf",
        size: 18,
    },
    builder::FontAsset {
        name: "mono",
        path: "assets/CONSOLE.ttf",
        size: 14,
    },
    builder::FontAsset {
        name: "calculator-display",
        path: "assets/CONSOLE.ttf",
        size: 28,
    },
];

let config = builder::Config {
    texture_width: 512,
    texture_height: 256,
    white_icon: "assets/WHITE.png".into(),
    icons: ICONS,
    default_font: "assets/NORMAL.ttf".into(),
    default_font_size: 12,
    fonts: FONTS,
};

let (_title, title_node) = TextBlock::create(
    TextBlockParameters::new("Inspector").font(FontRole::Heading.into()),
);
```

If `fonts` is empty, `builder::Config` falls back to `default_font` + `default_font_size` for the old single-font atlas layout.

## Cargo features

- `builder` *(default)* – enables the runtime atlas builder and PNG decoding helpers used by the examples.
- `png_source` – accepts PNG-compressed serialized atlases and `ImageSource::Png { .. }`; pixels are decoded to RGBA when loaded.
- `save-to-rust` – enables `AtlasHandle::to_rust_files` to emit the current atlas as Rust code for embedding.
- `prebuilt-atlas` – opt-in example atlas embedding; without it, examples build their atlas at runtime.
- `external-atlas` – example-only loader for a repository-root `atlas.png` paired with the checked-in `examples/common/external_atlas_metadata.rs` metadata.
- `example-backend` – shared internal gate used by examples; pair it with at least one concrete backend.
- `example-glow` / `example-vulkan` / `example-wgpu` – concrete example backends. Features are additive; examples select Glow, then Vulkan, then WGPU when several are enabled. Enable only the desired backend for normal interactive runs.

Disabling default features leaves only the raw RGBA upload path (`ImageSource::Raw { .. }`):
`cargo build --no-default-features`

The demos build their atlas at runtime unless you opt into `prebuilt-atlas`, so `--no-default-features` example builds should include `builder`:
`cargo run --example demo-full --no-default-features --features "example-vulkan builder"`

Equivalent command using the shared gate explicitly:
`cargo run --example demo-full --no-default-features --features "example-backend example-vulkan builder"`

To embed the generated atlas instead, add `prebuilt-atlas` explicitly:
`cargo run --example demo-full --no-default-features --features "example-vulkan prebuilt-atlas"`

`external-atlas` is a repository-development path, not a self-contained package feature. It expects
an existing `atlas.png` whose pixels match the checked-in metadata exactly; `atlas.png` is ignored
by Git and excluded from the crate package. The repository does not currently provide a command
that regenerates this pair. Prefer `builder` or `prebuilt-atlas` unless you maintain both files
together. If both atlas-loading features are enabled, `prebuilt-atlas` takes precedence over
`external-atlas`.

To export an atlas as Rust, enable `save-to-rust` (and `png_source` when serializing PNG-backed atlas data) and call `AtlasHandle::to_rust_files`. The helper binary requires `builder`, `save-to-rust`, and `png_source`:
`cargo run --bin atlas_export --features "builder save-to-rust png_source" -- --output path/to/atlas.rs`

### Version 0.8.0-pre-alpha

`0.8.0-pre-alpha` is the current alpha candidate. It is a breaking retained-API redesign relative
to `0.7.0`. The manifest and this heading must be updated together when the final alpha identifier
is selected.

- [x] Replaced retained tree building with unique owning `Node` values.
    - [x] Built-in leaf and container constructors return `(TypedWidgetHandle<W>, Node)`.
    - [x] Moving or mounting a node transfers its single owner; typed widget handles remain weak.
    - [x] Public runtime node identity and generic interaction-result lookup were removed.
- [x] Merged semantic state and runtime behavior into concrete widgets.
    - [x] Each widget owns its parameters-derived state, native event ports, measurement, update, and paint behavior.
    - [x] `LeafWidget` defines intrinsic measurement and `ContainerWidget` defines child-aware layout.
    - [x] `Linear`, `Grid`, `Disclosure`, and `ScrollArea` expose retained mutation through typed handles.
- [x] Made `Context` the retained transaction boundary.
    - [x] Context owns the ordered input FIFO, complete root forest, renderer, and application event dispatcher.
    - [x] `update_ui` and `update_ui_state` commit layout after every queued input event.
    - [x] `ContextFrame::render_ui` is paint-only and rejects missing, stale, or dimension-mismatched commits before backend acquisition.
- [x] Added context-owned typed application events.
    - [x] Widgets expose weak `WidgetEventHandle<E>` endpoints for their native event types.
    - [x] `Context<B, State>::subscribe` and `subscribe_with` dispatch into application state after retained widget borrows end.
    - [x] Removed the public standalone event `Session`; polling-only applications continue to use `Context<B>`.
- [x] Extracted backend-independent retained root management.
    - [x] Windows, dialogs, and popups remain context-owned until explicit destruction.
    - [x] `RootHandle` exposes typed chrome state and events without extending root lifetime.
    - [x] Modal routing, popup dismissal, focus, capture, root movement, and resizing share one retained window manager.
- [x] Unified rendering behind recorded painter operations and typed backend frames.
    - [x] `Painter` records backend-neutral work into the framework-owned display list.
    - [x] `RendererBackend::Frame<'a>` gives each backend one exclusive submission frame.
    - [x] Typed custom-render callbacks receive the concrete selected backend frame without a shared backend handle.
    - [x] Glow, Vulkan, and WGPU repository examples use the same retained application lifecycle.
- [x] Expanded atlas and typography support.
    - [x] Atlas configuration supports multiple named font variants and semantic font roles.
    - [x] Default styles bind conventional font and icon names from the backend atlas.
    - [x] Runtime construction, generated Rust embedding, and external PNG loading share serialized atlas metadata.
- [x] Documented the alpha API and known limitations.
    - [x] Documented the context-owned typed-event architecture.
    - [x] Documented UTF-8 editing, atlas glyph coverage, scalar-value fallback, and text-layout limits.
    - [x] Documented the trusted atlas-metadata contract, external-atlas workflow, and UTF-8 file-dialog path boundary.

### Version 0.7.0

Version `0.7.0` is the context-owned retained-root release. Compared to `0.6.1`, it completes the retained migration by moving root lifetime, interaction identity, and frame traversal into the context instead of requiring applications to resubmit each root every frame.

- [x] Moved retained root lifetime into `Context`.
    - [x] Applications register windows, dialogs, and popups with `create_window`, `create_dialog`, and `create_popup`.
    - [x] Registered roots are traversed by `ContextFrame::render_ui`; visibility and options are controlled with `set_root_visible` and `set_root_options`, while destruction is explicit.
    - [x] The old callback-based per-frame root submission path was removed from the supported API.
- [x] Replaced public interaction lookup with typed retained state.
    - [x] The former builder-generated public identity path was removed in favor of private runtime identity and typed widget events.
    - [x] `RootHandle` exposes checked root state while widget/container constructors return typed weak widget handles.
    - [x] Root windows, scroll areas, and window chrome persist without tree reconstruction.
- [x] Split retained widget execution into explicit `measure`, `update`, and `paint` phases.
    - [x] Layout records geometry first; update records control state and typed events; paint records commands from updated widget state.
    - [x] Custom-render nodes receive content and clip geometry through `CustomRenderArgs`, while widget input remains in the update phase.
    - [x] Built-in widgets and examples capture events directly from concrete typed widget runtimes.
- [x] Reworked retained layout, scroll areas, and root chrome.
    - [x] `SizePolicy::Weight` now uses sibling share ratios, and `SizePolicy::Fraction` covers explicit proportional sizing.
    - [x] `ScrollArea` is a retained viewport around one arbitrary content node; its scrollbars are real structural leaf widgets.
    - [x] Root auto-size, popup placement/close behavior, dialog z-order, scrollbars, and bottom-right resize handling were aligned with retained traversal.
- [x] Tightened drawing, texture, atlas, and backend behavior.
    - [x] Renderer display-list execution batches ordinary draw operations while preserving custom render and retained scroll-area boundaries.
    - [x] External texture uploads validate dimensions and byte counts, and texture clipping has a dedicated smoke example.
    - [x] Atlas code is split into builder, runtime, image, source, and codegen modules; `atlas_export` now requires `png_source` when exporting PNG-backed atlas data.
    - [x] Glow, Vulkan, and WGPU examples share retained root handling, and `examples/retained-custom-drawing` documents the custom painting path.
- [x] Unified rendering behind `Painter`, `DisplayList`, `Renderer`, and `RendererBackend`.
    - [x] Removed the old immediate drawing and mutable clipping facades in favor of scoped recording and single-pass execution.
    - [x] Removed the shared backend handle; Renderer now uniquely owns its backend and lends one typed frame to synchronous execution.
    - [x] Documented the clean rendering break in the subsystem guide and compiling examples.
- [x] Reduced migration surface and documented internals.
    - [x] Public imports are grouped around `prelude`, `retained`, and the `render` subsystem.
    - [x] Direct container drawing is no longer part of the application authoring path.
    - [x] Runtime modules, private structs, enums, and functions now have rustdoc or implementation comments, and the retained behavior is covered by focused tests.

### Version 0.6.x

Version `0.6.0` introduced retained `WidgetTree` authoring on top of the older per-frame root submission loop. Compared to `0.5.0`, `0.6.x` replaced immediate/closure widget authoring with reusable retained trees, widget handles, committed interaction results, custom graphics primitives, and multi-font atlas support.

- [x] `Context::window`, `Context::dialog`, and `Context::popup` accepted retained trees instead of UI-building closures.
- [x] `WidgetTreeBuilder` introduced reusable widget/layout hierarchies with widgets, panels, headers/tree nodes, row/grid/column/stack groups, and custom-render leaves.
- [x] Widgets reported intrinsic sizes through `measure` and updated persistent state through the retained traversal.
    - [x] Interaction observation later moved from generic frame results to typed widget-owned events.
- [x] The widget paint context gained widget-local custom painting for rectangles, text/icons/images, line strokes, polygon fills, and scoped clips.
- [x] Runtime atlas building and offline/prebuilt atlas export gained shared multi-font configuration.
- [x] Version `0.6.1` switched demos to runtime atlas construction by default and made prebuilt atlas embedding opt-in.

### Version 0.5

- [x] Widget identity moved fully to pointer-based IDs.
    - [x] Removed `with_id`; focus/hover now use widget trait-object/state pointers.
- [x] Layout refactor: introduced `LayoutEngine` + specialized flows (`RowFlow`, `StackFlow`) instead of a one-size-fits-all manager.
    - [x] Preferred sizing pipeline: widget helpers now call `Widget::measure`, allocate rectangles, then run widgets directly against the current frame input.
    - [x] Directional stack support: `StackDirection::{TopToBottom, BottomToTop}` plus `stack_direction` and `stack_with_width_direction`.
- [x] Context/container API cleanup: `Context` module split, input forwarding helpers, container state encapsulation, and handle views.
- [x] Widget internals cleanup: helper macroization/simplification, node/widget scaffolding unification, and text widget module split.
- [x] Text and input fixes: shared text layout/edit paths, textbox delete/end fixes, centralized widget input fallback.
- [x] Scrollbar behavior cleanup: unified sizing, layout, and drag handling.
- [x] File dialog and atlas fixes, including file dialog layout redesign and footer/button spacing corrections.
- [x] Added WGPU example backend and migrated demo-full to new layout flow APIs.
- [x] Added directional stack demo window and expanded documentation/comments for layout and WGPU renderer.

### Version 0.4

- [x] Stateful widgets
    - [x] Stateful widgets for core controls (button, list item, checkbox, textbox, slider, number, custom).
    - [x] Pointer-based widget IDs; InputSnapshot threaded through widgets and cached per frame.
    - [x] IdManager removed; widget IDs now derive from state pointers.
    - [x] Widget API redesign requires stateful widget instances; trait/type renames applied.
    - [x] Legacy `button_ex*` shims removed.
    - [x] Drawing state was extracted into the shared widget execution context.
    - [x] Widget state/context pipeline with ControlState returned from `update_control`.
- [x] File dialog UX fixes (close on OK/cancel, path-aware browsing).
- [x] Expanded unit tests for scrollbars, sliders, and PNG decoding paths.
- [x] Style shared via `Rc<Style>` across containers/panels; window chrome state moved into `Window`.
- [x] `Container::style` now uses `Rc<Style>`.

### Version 0.3

- [x] Use `std` (`Vec`, `parse`, ...)
- [x] Containers contain clip stack and command list
- [x] Move `begin_*`, `end_*` functions to closures
- [x] Move to `AtlasRenderer` trait
- [x] Remove/refactor `Pool`
- [x] Change layout code
- [x] Add tree nodes
- [x] Manage window lifetime and ownership outside Context through root windows
- [x] Manage container lifetime and ownership outside containers
- [x] Add software-based textured rectangle clipping
- [x] Add atlas support
    - [x] Runtime atlas builder
        - [x] Icons
        - [x] Font hash tables
    - [x] Separate atlas construction from runtime lookup
    - [x] Add the `builder` feature
    - [x] Save an atlas as Rust source
    - [x] Load an atlas from constant Rust data
- [x] Add the image widget
- [x] Add PNG atlas sources
- [x] Add pass-through rendering commands for 3D viewports
- [x] Add custom rendering widgets
    - [x] Mouse input events
    - [x] Keyboard events
    - [x] Text events
    - [x] Dragging outside the region
    - [x] Rendering
- [x] Add dialog support
- [x] Add the file dialog
- [x] Iterate on APIs and examples
    - [x] Simple example
    - [x] Full API example with 3D rendering and dialogs
- [x] Add documentation

## Bundled asset attribution and licenses

The repository's BSD 3-Clause license covers the project code and the facepalm
demo image described below. The bundled third-party fonts and icons retain their
original names, authorship, and license terms:

- **Open Sans Regular** (`OpenSans-Regular`, stored as `assets/NORMAL.ttf`) and
  **Open Sans Bold** (`OpenSans-Bold`, stored as `assets/BOLD.ttf`) — copyright
  2020 The Open Sans Project Authors; licensed under the bundled
  [SIL Open Font License 1.1](LICENSES/OFL-1.1.txt).
- **Fixedsys Excelsior 3.01 Regular** (`FixedsysExcelsiorIIIb`, stored as
  `assets/CONSOLE.ttf`) — version 3.010 (2007), by Darien Valentine; released
  into the public domain, with the
  bundled [CC0 dedication](LICENSES/CC0-1.0.txt) applying
  where a public-domain release is not permitted. See the project's
  [distribution terms](https://github.com/kika/fixedsys#distribution-terms).
  The bundled TTF itself does not contain a formal license field.
- **Material Design Icons by Google** — the icon PNGs in `assets/` (all PNGs
  there except `WHITE.png`, which is a plain atlas texel) are derived from
  Google's Material Design Icons and are licensed under the
  bundled [Apache License 2.0](LICENSES/Apache-2.0.txt).
- **Facepalm demo image** (`examples/FACEPALM.png`) — copyright Raja Lehtihet &
  Wael El Oraiby; licensed under this repository's
  [BSD 3-Clause license](LICENSE).

The font names above come from the TTFs' embedded name records; `NORMAL.ttf`,
`BOLD.ttf`, and `CONSOLE.ttf` are only the filenames used by this repository.
