# Backend frames and custom rendering

## How `SelectedBackend::Frame<'a>` works

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
| `ContextFrame<'ctx, B, State = ()>` | The application-level paint/submission frame. It exclusively borrows `Context<B, State>` while committed retained UI is painted and recorded. | `render_ui(self)` paints and submits once. Dropping without submission cancels. |
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
Context executor preflight           validate texture/custom-render keys
        |
RendererBackend::frame(&mut backend)  acquire SelectedBackend::Frame<'_>
        |
Internal display-list execution       RendererFrame calls + typed custom callbacks
        |
drop backend frame                    finalize/submit/present as applicable; release borrow
        |
drop logical frame                    release the exclusive Context borrow
```

Context's executor acquires the backend frame only after resource preflight succeeds. Normal UI
operations use the backend-neutral `RendererFrame` methods. Immediately before a custom-render
operation, the executor closes the current UI batch and invokes the registered callback with
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
let _root = ctx.ui().create_window(Window::new("Cube", rect(40, 40, 360, 360), tree));
```

`register_custom_renderer` accepts a callback valid for every frame borrow lifetime. In expanded
form its important bound is `for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs)`. That
higher-ranked lifetime means the callback can use the active frame but cannot save it in captured
state. The returned `CustomRenderHandle<B>` is tagged with `B`, so registration and removal through
a Context using another backend type fail at compile time. `Node::custom_render` erases the handle
to its backend-neutral registry key after checking the backend type; a key originating from any
other Context retains a different concrete owner identity and is rejected during Context
preflight before a backend frame is acquired.

`CustomRenderArgs` carries the geometry needed at execution time:

- `content_area` is the full screen-space rectangle allocated to the custom widget;
- `view` is the authoritative final visible rectangle after operation, content-area, viewport,
  window, and scroll clipping;
- `dimensions` is the validated drawable size of the active frame.

The executor does not invoke the callback when that intersection is empty. Do not intersect
`content_area` and `view` again inside the callback.

Custom callbacks receive no input and should not acquire another frame, mutate the atlas, or
finalize/present the backend frame. Update application/widget state before `Context::frame`; inside
the callback, read that state and record work on the supplied frame. Use `Painter` instead when the
drawing can be expressed with portable UI primitives.

The complete, documented [backend-frame cube example](../examples/backend-frame-cube.rs)
implements a small retained `CubeWidget`. It uses `rs_math3d`'s `Vec3f`, `Quatf`, `lookat`,
`perspective`, and `project3` primitives to transform the cube, depth-sorts its faces, and submits
the resulting triangles through `SelectedFrame::enqueue_colored_vertices`. Run it with exactly one
backend:

```bash
cargo run --example backend-frame-cube --features example-glow
cargo run --example backend-frame-cube --features example-vulkan
cargo run --example backend-frame-cube --features example-wgpu
```
