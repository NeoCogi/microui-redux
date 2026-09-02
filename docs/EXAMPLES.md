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
  menus, a standalone Test Popup menu, dialogs, custom drawing, external textures, and custom
  backend rendering. Its menu shows
  grouped and disabled commands, shortcut hints, a live checked item, radio choices in a cascading
  View > Log Spacing submenu, a View > Theme radio submenu for Default Skin, Windows 3.11,
  Windows 95, and Mac OS 9, and typed
  item submission events into the file dialog, log, and skin state. A titleless fullscreen family
  root at layer 0 renders a perspective X-Y grid beneath its content-clipped floating child windows.
  The grid root owns the Grid/Help menu, which records and handles above the complete child family;
  its private menu-popup nodes stack in the transient band derived from that layer-0 root. The child
  windows retain screen-space geometry and sibling activation order while inheriting that fixed
  layer. Left-drag an exposed part of the grid to rotate its arcball camera, use the mouse
  wheel there to zoom, or choose Grid > Reset View to restore the initial composition. Grid > Minor
  Grid Lines controls its unit-spaced divisions. The Demo Window exposes minimize and
  maximize/restore caption controls; Grid > Show Demo Window restores it after minimize or close.
  Grid segments are clipped in homogeneous space so
  rotation cannot project behind-camera endpoints into stray lines across the UI. Each `Window`
  transfers its menu bar into direct manager-owned `MenuSurface` values. Menu and ordinary
  composed-control popups are concrete forest nodes whose sole parent edges encode lifetime,
  modal eligibility, and stacking; exact non-menu anchors remain in screen space. Live item
  presentation is borrowed through `Ui::menu_item` or `Ui::menu_item_mut`.

## Keyboard controls

Every example uses the shared SDL adapter, which forwards logical key identity, pressed/released
state, modifier snapshots, and repeat state through `Context::key`; SDL text composition continues
separately through `Context::text`. The retained examples therefore share these controls:

- `Tab` and `Shift+Tab` move persistent focus forward and backward through eligible controls in the
  active window, wrapping at either end.
- `Ctrl+F6` and `Ctrl+Shift+F6` cycle forward and backward through visible application windows.
  Each window restores its remembered control focus when selected; an active menu or modal dialog
  retains keyboard ownership.
- Enter and Space invoke buttons and list choices; Space toggles checkboxes; Left/Right adjust
  sliders; Up/Down adjust number controls; and arrow/activation keys operate disclosures and combos.
- An initial non-repeated Escape press dismisses an active application popup. Repeats and release
  remain ordinary raw input for the restored parent, while a custom focused widget receives every
  Escape transition when no application popup accepts the initial press.
- `F10` or a tap of Alt enters a window menu. Arrow keys, Home, End, Enter, Space, and Escape navigate
  the active menu branch using the bindings described in the [menu guide](MENUS.md).

Custom drawing in `demo-full` declares its keyboard role explicitly. Pointer-only grid and graph
surfaces stay out of Tab order, while the Suzanne viewport remains a Tab stop because it implements
arrow-key orbiting and text-input W/S zoom. This keeps example-specific interaction consistent with
the same focus contract as built-in widgets.

The full demo's live Skin Editor exposes `focus` and `window focus` independently. The former
updates focused control fills, menu selection, and widget outlines; the latter updates the active
window title and frame. Every other example inherits their Windows-blue defaults without requiring
widget-specific focus painting.

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

`demo-full` loads `examples/FACEPALM.png`, `assets/suzanne.obj`, and both directories under
`themes/` from disk at runtime. Paths are anchored to the Cargo manifest directory, but the files
must remain present in a source checkout or package.

For a smaller release executable with runtime-loaded assets, build without default features and
enable exactly one backend plus `builder`:

```bash
cargo build \
  --release \
  --example demo-full \
  --no-default-features \
  --features "example-glow builder theme-json"
```

This keeps demo assets outside the executable: fonts/icons are read from `assets/`, the external
demo image is read from `examples/FACEPALM.png`, the Suzanne mesh is read from
`assets/suzanne.obj`, and theme JSON/PNGs are read from `themes/`. To inspect real binary section
size rather than asset size, use
`size -A target/release/examples/demo-full`.

For the smallest Linux executable, use the `build-min-size` Cargo alias with nightly. It builds for
a dedicated `x86_64-unknown-linux-min-size` platform target, rebuilds `std`, uses immediate-abort
panics, omits Rust unwind tables and panic formatting details, and strips symbols and the linker
build ID. Normal builds remain on their selected toolchain and platform:

```bash
cargo +nightly build-min-size \
  --example demo-full \
  --no-default-features \
  --features "example-glow builder theme-json"
```
The executable is written to
`target/x86_64-unknown-linux-min-size/min-size/examples/demo-full`. The alias accepts ordinary Cargo
feature and package-selection arguments; replace `example-glow` with `example-vulkan` or
`example-wgpu` when needed. It requires the nightly `rust-src` component (`rustup component add
rust-src --toolchain nightly`).

![microui-redux demo with retained windows and controls](../res/microui.png)
