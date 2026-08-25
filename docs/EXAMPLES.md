# Examples

## Example catalog

- [`simple`](../examples/simple.rs) shows the smallest complete retained application.
- [`calculator`](../examples/calculator.rs) builds a focused retained calculator UI.
- [`retained-custom-drawing`](../examples/retained-custom-drawing.rs) implements a custom widget
  with the backend-neutral `Painter` API.
- [`backend-frame-cube`](../examples/backend-frame-cube.rs) records backend-specific 3D work from a
  typed custom-render callback.
- [`texture-clipping-smoke`](../examples/texture-clipping-smoke.rs) exercises low-level texture
  upload and clipping behavior.
- [`demo-full`](../examples/demo-full.rs) combines retained widgets, per-window File/View/Help
  menus, dialogs, custom drawing, external textures, and custom backend rendering. Its menu shows
  grouped and disabled commands, shortcut hints, a live checked item, radio choices in a cascading
  View > Log Spacing submenu, and typed
  command dispatch into the file dialog, log, and style state. A separate titleless fullscreen root
  at layer 0 renders a perspective X-Y grid beneath the original Demo Window and every other
  default-layer floating window. The grid surface owns an independent Grid/Help menu whose popups
  inherit layer 0. Left-drag an exposed part of the grid to rotate its arcball camera, use the mouse
  wheel there to zoom, or choose Grid > Reset View to restore the initial composition. Grid > Minor
  Grid Lines controls its unit-spaced divisions. Grid segments are clipped in homogeneous space so
  rotation cannot project behind-camera endpoints into stray lines across the UI. Each menu window
  owns its complete popup/submenu root subtree, and the Demo Window owns its reusable file dialog
  and composed-control popups; these relationships affect lifetime and stacking without changing
  any root's screen-space geometry.

## Full demo

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

![microui-redux demo with retained windows and controls](../res/microui.png)
