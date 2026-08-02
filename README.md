# Rxi's Microui Port to Idiomatic Rust
[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

This project started as a C2Rust conversion of Rxi's MicroUI and has since grown into a Rust-first UI toolkit. It keeps Microui's compact rendering model while moving UI authoring onto unique owning `Node` trees, typed weak state handles, context-owned roots, and backend-agnostic rendering hooks. Runtime node identity is private.

Compared to [microui-rs](https://github.com/neocogi/microui-rs), this crate embraces std types, reusable retained trees, and richer widgets such as custom rendering callbacks, dialogs, and a file dialog.

The current API model and upgrade mapping are summarized below; the detailed breaking-change guide is in [MIGRATION.md](MIGRATION.md).

## Demo
Clone and build the demo (enable exactly one backend feature):
```
$ cargo run --example demo-full --features example-vulkan   # Vulkan backend
# or
$ cargo run --example demo-full --features example-glow     # Glow backend
# or
$ cargo run --example demo-full --features example-wgpu     # WGPU backend
```

`example-backend` is only a shared gate for example code paths; it is **not** runnable by itself.
Running with only `--features example-backend` will fail intentionally at compile time.

`demo-full` now loads `examples/FACEPALM.png` and `assets/suzanne.obj` from disk at runtime (no `include_bytes!` for those files).

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

If `atlas.png` has already been generated, the demo can skip runtime font/icon atlas construction
and load the atlas image from disk instead:
```bash
cargo build \
  --release \
  --example demo-full \
  --no-default-features \
  --features "example-glow external-atlas"
```

For an even smaller executable, use nightly + rebuilt `std`:
```bash
CARGO_PROFILE_RELEASE_PANIC=immediate-abort \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C strip=symbols -C link-arg=-s -Zlocation-detail=none -Zfmt-debug=none" \
cargo +nightly build \
  --release \
  -Z build-std=std,panic_abort \
  -Z build-std-features=optimize_for_size \
  -Z panic-immediate-abort \
  --example demo-full \
  --no-default-features \
  --features "example-wgpu builder"
```
Replace `example-wgpu` with `example-glow` or `example-vulkan` if needed.

![random](res/microui-0.6.png)

## Key Concepts
- **Context**: owns the high-level `Renderer`, the only ordered input queue, and retained root windows. Applications enqueue through Context methods, call `update_ui(dimensions)` to drain input and commit layout, observe or mutate typed state, synchronize again if that mutation can affect layout, then call `frame(FrameInfo).render_ui()?` to paint and submit once.
- **Container**: a public `Widget` subtrait for runtimes that own one authoritative opaque `Children` collection. Application-facing container state uses typed weak `WidgetStateHandle` values and safe indexed membership operations.
- **Layout engine + flows**: parent containers assign child rectangles through scoped `ContainerLayoutCtx` services. Row, Grid, Column, Stack, Disclosure, and ScrollArea all own persistent children behind typed container state.
- **Widget**: a runtime UI element implementing `Widget` (for example `Button`, `Textbox`, or `Slider`) and uniquely owning its associated state allocation. `*Parameters` are one-shot initialization; `*State` holds mounted mutable values and events.
- **Node**: the non-cloneable owner of one concrete widget or container runtime. A `Node` receives private process-unique identity when constructed and transfers exactly once into a root or opaque `Children` collection; attached nodes cannot be detached or reparented.
- **Rendering**: widgets obtain a local `Painter` from `WidgetPaintCtx`; retained traversal owns the internal display list, and `Renderer` executes it through one exclusively borrowed `RendererBackend::Frame`. The portable target supports drawables up to 8192x8192 and geometry up to four maximum drawable spans beyond the viewport; see the [render subsystem guide](src/render/RENDER.md#supported-coordinate-domain) for the complete coordinate contract and integration API.
- **Typography**: atlases can bake multiple named fonts and sizes. `Style` resolves semantic roles (`body`, `small`, `title`, `heading`, `mono`) through `FontRole`, while text-bearing `*Parameters` select a per-widget font with `.font(...)`.

The public API is intentionally centered on `microui_redux::prelude` for applications and `microui_redux::retained` for retained concepts such as `Node`, `Children`, `Container`, `Column`, `Disclosure`, typed state handles, and `Context`. Low-level rendering lives under `microui_redux::render`, and atlas construction lives under `microui_redux::atlas::builder`.

### Rendering

Widgets record backend-neutral drawing through a framework-created `Painter`; `Renderer` executes the crate-owned display list and submits final geometry through `RendererBackend`. The [render subsystem guide](src/render/RENDER.md) covers architecture, clipping, textures, custom callbacks, backend implementation, and compiling code examples.

### How `SelectedBackend::Frame<'a>` works

`SelectedBackend` is not a special type supplied by microui-redux. The examples define it as an
ordinary compile-time alias for exactly one concrete renderer:

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
Context::update_ui(dimensions)        drain FIFO; full update + layout after each event
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
let tree = Node::custom_render(cube, cube_renderer).with_policy(Policy::fill());
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

The current supported authoring path is retained widget trees registered as context-owned roots. Applications can call `Context::create_window(...)`, `Context::create_dialog(...)`, or `Context::create_popup(...)` once, mutate built-in state through typed `WidgetStateHandle` values, commit updates with `Context::update_ui(...)`, and paint with `Context::frame(FrameInfo).render_ui()?`.

Root creation consumes one persistent application `Node` and returns a non-owning `RootHandle`.
Roots cannot be replaced while retaining their identity: mutate descendants through a state-owned
container, or destroy and recreate the root. Visibility is controlled with `set_root_visible`.
A visible dialog is modal: it stays above every window and popup, receives all eligible pointer,
keyboard, text, focus, and capture routing, and blocks interaction with other roots until hidden or
destroyed. Other roots remain visible and continue to be laid out and painted.

```rust
let (name_state, name_runtime) = Textbox::create(TextboxParameters::new(""));
let (_, label_runtime) = TextBlock::create(TextBlockParameters::new("Name"));
let (_, tree) = Row::create(RowParameters::new(
    [SizePolicy::Fixed(120), SizePolicy::Remainder(0)],
    SizePolicy::Auto,
    [Node::widget(label_runtime), Node::widget(name_runtime)],
));

let _root = ctx.create_window("main", rect(20, 20, 240, 120), tree);
let dimensions = Dimensioni::new(800, 600);
let info = FrameInfo::try_new(dimensions, color(20, 22, 26, 255))?;
ctx.update_ui(dimensions);

if name_state.try_update(TextboxState::take_submitted).unwrap_or(false) {
    // react to the textbox submission here
}

// If the reaction changed layout-affecting widget/container state, synchronize once more.
// ctx.update_ui(dimensions);
ctx.frame(info).render_ui()?;
```

Retained trees are the supported public authoring path. Each non-cloneable `Node` owns one concrete
widget or container runtime. Dynamic `ColumnState`, `DisclosureState`, and `GridState` values own
opaque children; successful insertion transfers a node, while removal, clearing, or replacement
drops the removed runtime owners. Grid placement belongs to `GridState`, not to generic nodes:
plain nodes occupy one cell, while `GridItem::spanned(node, columns, rows)` supplies an explicit
parent-child span that can later be changed with `GridState::set_span` without replacing the child.
Built-in values, events, and commands are accessed through typed weak state handles. Disclosure
headers and tree rows use `DisclosureParameters::{header, tree}` and no longer have a separate
widget `Node` or `NodeStateValue` API.

### Retained node identity

Each owning `Node` receives a private, process-unique runtime identity before mounting. Moving a
node, applying consuming `with_policy`, wrapping it in an unmounted `GridItem`, and inserting it
into `Children` or `GridState` preserve that identity; applications cannot read or construct it.
There is no public node ID or result lookup path. Widgets record consumable events in their typed
state, and root chrome exposes its rectangle, visibility, active mode, and pending events through
`RootHandle::state()`.

Registered roots can be configured with `Context::set_root_options(...)` and `WindowOption` to
control window chrome. Root overflow does not scroll implicitly; construct a `ScrollArea` with
`ScrollAreaOption::ENABLE_SCROLL` and use its typed state handle for offset or membership changes.

### Preferred sizing and retained layout
- Every built-in widget reports its own intrinsic preferred size from content metrics (text/icon/thumb/line layout).
- `Context::update_ui` first synchronizes layout, then drains input in API-call order. Every event runs one complete eligible-tree `Widget::update` traversal and one follow-up layout, so geometry changed by one event is authoritative for routing the next.
- `ContextFrame::render_ui` performs no input, update, or layout work. It paints the committed tree with `Widget::paint` and submits one display list; missing, stale, pending-input, or dimension-mismatched commits return `RenderError::UiUpdateRequired` before backend acquisition.
- Widgets and containers share the public `Widget` phase contract. Containers additionally expose only opaque child visitors, indexed layout services, descendant visibility, and scoped input routing.
- Parent containers assign each node a retained parent-local allocation; child offsets and clips remain node-local and are resolved through a stack-only transform during traversal.
- Resolved outer rectangles and clips remain runtime stack locals. Node behavior works against its local content surface, while outer frame painting, standard hit routing, and conversion from screen input remain runtime-owned.
- A public widget's Painter geometry and routed pointer positions share the derived content-local origin.
- Concrete container constructors consume child `Node` values and return a typed state handle plus one completed owning `Node`; `Node::custom_render` covers backend-typed custom-render leaves.
- `SizePolicy::Weight(value)` distributes available track space by sibling share ratio (spacing accounted for). Use `SizePolicy::Fraction(value)` for explicit `0.0..=1.0` proportional sizing in single-track flows.
- Returning `<= 0` for either axis from `Widget::measure` still means "use layout fallback/defaults" for that axis.

Built-in state is mutated through typed handles between commits. After programmatic state/topology changes, call `update_ui` even when no input is pending so layout is synchronized before paint. Feed raw input through methods such as `mousemove`, `mousedown`, `scroll`, `keydown_code`, and `text`; calls are queued without coalescing. A widget receives the current event as `Option<&UiInputEvent>`, while `WidgetUpdateCtx::{mouse_buttons,key_modes,key_codes}` exposes held state after that event was applied.

`ContextFrame` holds the Context borrow needed to serialize paint/submission, but it does not lock independent widget or root state handles and there is no Context access token. Do not keep a state-access closure active while retained update/layout/paint can reach that same state. Framework recursion through a container's scoped child visitor is the intentional exception. If layout-affecting state changes after the last commit, drop any unsubmitted frame and call `update_ui` again before paint.

The application owns `Context` and its weak state handles as independent Rust values, so the
compiler permits explicitly capturing the Context inside a state-access closure. Do not initiate
retained traversal that way:

```rust
textbox_state.try_update(|state| {
    state.set_text("hello");
    context.update_ui(dimensions); // unsupported: the mutable state borrow is still active
});
```

`try_update` holds the state's checked `RefCell` borrow until its closure returns. If the nested
layout, update, or paint traversal reaches that state, the built-in runtime's checked borrow is
incompatible and panics with a diagnostic naming the runtime phase. This is the reentrancy guard;
there is no separate Context lock. Finish the state access before committing instead:

```rust
textbox_state
    .try_update(|state| state.set_text("hello"))
    .expect("textbox state unavailable");
context.update_ui(dimensions);
```

Update and paint visit a node before its eligible children and visit siblings in forward order. A
successful mutation of a later, currently available state cell is visible when traversal reaches
it; mutating an already-updated sibling does not rerun that sibling. A container's active child
visitor borrow makes mutation of that same container return `None`, while another available subtree
may change. There is no transaction snapshot or rollback, but every input transaction ends with a
complete layout before the next event is routed.

`Widget::paint` and registered custom-render callbacks are observational with respect to
application state, topology, interaction, and layout. They may update private rendering-only
caches, but mutating retained UI through an independently captured state handle during either
callback violates the contract; it is not a deferred-next-frame update. Commit those changes before
creating the frame.

## Fonts and typography
- Atlas building supports multiple baked fonts and sizes through `atlas::builder::FontAsset`, and the same config can drive both runtime atlas construction and offline/prebuilt atlas export.
- `Context::new(...)` binds the conventional atlas keys `body`, `small`, `title`, `heading`, and `mono` onto the default `Style`. `Context::set_style(...)` also rebinds any font fields that are still left at their default/unset values, so tweaking colors or spacing on top of `Style::default()` keeps the intended body/title sizes.
- Text-bearing widget Parameters expose `.font(FontChoice)`, so you can either select a semantic role (`FontRole::Heading.into()`) or a concrete baked font ID (`atlas.font_id("caption").unwrap().into()`).
- Font sizes are selected by choosing another baked font variant, not by scaling one bitmap font at runtime.
- `examples/demo-full` uses this directly: `NORMAL.ttf` for control/body text, `BOLD.ttf` for window titles, and `CONSOLE.ttf` for the log window’s input/output text.

```rust
use microui_redux::{atlas::builder, prelude::*};

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
        size: 16,
    },
    builder::FontAsset {
        name: "heading",
        path: "assets/NORMAL.ttf",
        size: 18,
    },
    builder::FontAsset {
        name: "mono",
        path: "assets/CONSOLE.ttf",
        size: 12,
    },
];

let config = builder::Config {
    texture_width: 512,
    texture_height: 256,
    white_icon: "assets/WHITE.png".into(),
    close_icon: "assets/CLOSE.png".into(),
    expand_icon: "assets/PLUS.png".into(),
    collapse_icon: "assets/MINUS.png".into(),
    check_icon: "assets/CHECK.png".into(),
    expand_down_icon: "assets/EXPAND_DOWN.png".into(),
    open_folder_16_icon: "assets/OPEN_FOLDER_16.png".into(),
    closed_folder_16_icon: "assets/CLOSED_FOLDER_16.png".into(),
    file_16_icon: "assets/FILE_16.png".into(),
    default_font: "assets/NORMAL.ttf".into(),
    default_font_size: 12,
    fonts: FONTS,
};

let (_title_state, title_runtime) = TextBlock::create(
    TextBlockParameters::new("Inspector").font(FontRole::Heading.into()),
);
```

If `fonts` is empty, `builder::Config` falls back to `default_font` + `default_font_size` for the old single-font atlas layout.

## Cargo features
- `builder` *(default)* – enables the runtime atlas builder and PNG decoding helpers used by the examples.
- `png_source` – allows serialized atlases and `ImageSource::Png { .. }` uploads to stay compressed.
- `save-to-rust` – enables `AtlasHandle::to_rust_files` to emit the current atlas as Rust code for embedding.
- `prebuilt-atlas` – opt-in example atlas embedding; without it, examples build their atlas at runtime.
- `example-backend` – shared internal gate used by examples; pair it with exactly one concrete backend.
- `example-glow` / `example-vulkan` / `example-wgpu` – concrete example backends; choose exactly one when running examples.

Disabling default features leaves only the raw RGBA upload path (`ImageSource::Raw { .. }`):
`cargo build --no-default-features`

The demos build their atlas at runtime unless you opt into `prebuilt-atlas`, so `--no-default-features` example builds should include `builder`:
`cargo run --example demo-full --no-default-features --features "example-vulkan builder"`

Equivalent command using the shared gate explicitly:
`cargo run --example demo-full --no-default-features --features "example-backend example-vulkan builder"`

To embed the generated atlas instead, add `prebuilt-atlas` explicitly:
`cargo run --example demo-full --no-default-features --features "example-vulkan prebuilt-atlas"`

To export an atlas as Rust, enable `save-to-rust` (and `png_source` when serializing PNG-backed atlas data) and call `AtlasHandle::to_rust_files`. The helper binary requires `builder`, `save-to-rust`, and `png_source`:
`cargo run --bin atlas_export --features "builder save-to-rust png_source" -- --output path/to/atlas.rs`

### Version 0.8.0-pre-alpha

`0.8.0-pre-alpha` is the current in-development UI-node/runtime refactor. It establishes the final
direction for unique owning nodes, concrete runtime-owned state, typed weak application handles,
public custom containers, one-event update commits, and paint-only rendering. The public API,
README, rustdoc, and [migration guide](MIGRATION.md) are being aligned as each remaining
correctness and cleanup phase in [UI-NODE-REFACTOR-PLAN.md](UI-NODE-REFACTOR-PLAN.md) lands.

This is intentionally a pre-alpha version: downstream users should expect further breaking changes
before `0.8.0` and should consult the plan's completed-item evidence when evaluating a snapshot.

### Version 0.7.0
Version `0.7.0` is the context-owned retained-root release. Compared to `0.6.1`, it completes the retained migration by moving root lifetime, interaction identity, and frame traversal into the context instead of requiring applications to resubmit each root every frame.

- [x] Moved retained root lifetime into `Context`.
    - [x] Applications register windows, dialogs, and popups with `create_window`, `create_dialog`, and `create_popup`.
    - [x] Registered roots are traversed by `ContextFrame::render_ui`; visibility and options are controlled with `set_root_visible` and `set_root_options`, while destruction is explicit.
    - [x] The old callback-based per-frame root submission path was removed from the supported API.
- [x] Replaced public interaction lookup with typed retained state.
    - [x] The former builder-generated public identity path was removed in favor of private runtime identity and typed state events.
    - [x] `RootHandle` exposes checked root state while widget/container constructors return typed weak state handles.
    - [x] Root windows, scroll areas, and window chrome persist without tree reconstruction.
- [x] Split retained widget execution into explicit `measure`, `update`, and `paint` phases.
    - [x] Layout records geometry first; update records control state and typed events; paint records commands from updated widget state.
    - [x] Custom-render nodes receive content and clip geometry through `CustomRenderArgs`, while widget input remains in the update phase.
    - [x] Built-in widgets and examples consume events directly from their typed state.
- [x] Reworked retained layout, scroll areas, and root chrome.
    - [x] `SizePolicy::Weight` now uses sibling share ratios, and `SizePolicy::Fraction` covers explicit proportional sizing.
    - [x] `ScrollAreaState` is the retained nested-scroll and membership API; old synthetic scrollbar nodes were removed.
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
    - [x] Interaction observation later moved from generic frame results to typed widget-state events.
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
- [x] Move to AtlasRenderer Trait
- [x] Remove/Refactor `Pool`
- [x] Change layout code
- [x] Treenode as tree
- [x] Manage windows lifetime & ownership outside of context (use root windows)
- [x] Manage containers lifetime & ownership outside of contaienrs
- [x] Software based textured rectangle clipping
- [x] Add Atlasser to the code
    - [x] Runtime atlasser
        - [x] Icon
        - [x] Font (Hash Table)
    - [x] Separate Atlas Builder from the Atlas
    - [x] Builder feature
    - [x] Save Atlas to rust
    - [x] Atlas loader from const rust
- [x] Image widget
- [x] Png Atlas source
- [x] Pass-Through rendering command (for 3D viewports)
- [x] Custom Rendering widget
    - [x] Mouse input event
    - [x] Keyboard event
    - [x] Text event
    - [x] Drag outside of the region
    - [x] Rendering
- [x] Dialog support
- [x] File dialog
- [x] API/Examples loop/iterations
    - [x] Simple example
    - [x] Full api use example (3d/dialog/..)
- [x] Documentation
