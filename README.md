# Rxi's Microui Port to Idiomatic Rust
[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

This project started as a C2Rust conversion of Rxi's MicroUI and has since grown into a Rust-first UI toolkit. It preserves the immediate-mode feel while using stateful widget structs with pointer-derived widget identity, a row/column layout helper API, retained `WidgetTree` values for reusable UI structure, and backend-agnostic rendering hooks.

Compared to [microui-rs](https://github.com/neocogi/microui-rs), this crate embraces std types, closure-based window/panel/column scopes, and richer widgets such as custom rendering callbacks, dialogs, and a file dialog.

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

`demo-full` now loads `examples/FACEPALM.png` and `assets/suzane.obj` from disk at runtime (no `include_bytes!` for those files).

For a smaller release executable, use nightly + rebuilt `std`:
```bash
RUSTFLAGS="-C strip=symbols -C link-arg=-s -Zlocation-detail=none -Zfmt-debug=none" \
cargo +nightly build \
  --release \
  -Z build-std=std,panic_abort \
  -Z build-std-features=optimize_for_size \
  --example demo-full \
  --no-default-features \
  --features "example-wgpu png_source"
```
Replace `example-wgpu` with `example-glow` or `example-vulkan` if needed.

![random](https://github.com/NeoCogi/microui-redux/raw/master/res/microui.png)

## Key Concepts
- **Context**: owns the renderer handle, user input, and root windows. The atlas is provided by the renderer and accessed through the context. Each frame starts by feeding input into the context, then calling `context.window(...)` for every visible window or popup.
- **Container**: describes one layout surface. Every window, panel, popup or custom widget receives a mutable `Container` that exposes high-level widgets (buttons, sliders, etc.), retained-tree execution helpers, and lower-level drawing helpers.
- **Layout engine + flows**: the engine tracks scope stack, scroll-adjusted coordinates, and content extents, while flows control placement behavior. Use `Container::with_row` for row tracks, `Container::stack`/`Container::stack_direction` for one-item-per-line sections, and `container.column(|ui| { ... })` for nested scopes. Widget helpers (`button`, `textbox`, `slider`, etc.) query each widget's `preferred_size` before allocating the cell, so `SizePolicy::Auto` can follow per-widget intrinsic sizing.
- **Widget**: stateful UI element implementing the `Widget` trait (for example `Button`, `Textbox`, `Slider`). These structs hold interaction state and use pointer-derived IDs from their current address.
- **WidgetTree**: retained widget/layout hierarchy built once with `WidgetTreeBuilder` and replayed each frame through `Container::widget_tree(results, &tree)`. Tree nodes cover widgets, panels, headers/tree nodes, row/grid/column/stack layout groups, and custom rendering. `tree.run(...)` remains available as a runtime escape hatch for sections that still need live geometry.
- **Renderer**: any backend that implements the `Renderer` trait can be used. The included SDL2 + glow example demonstrates how to batch the commands produced by a container and upload them to the GPU.

```rust
let name = widget_handle(Textbox::new(""));
let tree = WidgetTreeBuilder::build({
    let name = name.clone();
    move |tree| {
        tree.row(&[SizePolicy::Fixed(120), SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
            tree.label("Name");
            tree.widget(name.clone());
        });
    }
});

ctx.window(&mut main_window, ContainerOption::NONE, WidgetBehaviourOption::NONE, |ui, results| {
    ui.widget_tree(results, &tree);
    WindowState::Open
});
```

For direct one-off UI, the immediate-style `Container` helpers still exist. The retained tree path is the preferred way to build reusable window bodies, panels, dialogs, and list/grid scaffolds.

```rust
let mut temp_name = Textbox::new("");

ctx.window(&mut main_window, ContainerOption::NONE, WidgetBehaviourOption::NONE, |ui, results| {
    let widths = [SizePolicy::Fixed(120), SizePolicy::Remainder(0)];
    ui.with_row(&widths, SizePolicy::Auto, |ui| {
        ui.label("Name");
        ui.textbox(results, &mut temp_name);
    });
    WindowState::Open
});
```

### Widget IDs
Widget IDs default to the address of the widget state. This is stable as long as the state stays at a fixed address, but it can change if the state lives inside a `Vec` that grows/shrinks (reallocation moves items). If that happens, focus/hover continuity follows the new addresses.

When setting focus manually, pass a widget pointer ID from `widget_id_of` or `widget_id_of_handle`:

```rust
ui.set_focus(Some(widget_id_of_handle(&my_textbox_handle)));
```

Window, dialog, and popup builders now accept a `WidgetBehaviourOption` to control scroll behavior. Use `WidgetBehaviourOption::NO_SCROLL`
for popups that should not scroll, `WidgetBehaviourOption::GRAB_SCROLL` for widgets that want to consume scroll, and
`WidgetBehaviourOption::NONE` for default behavior. Custom widgets receive consumed scroll in `CustomRenderArgs::scroll_delta`.

### Preferred sizing
- Every built-in widget now reports its own intrinsic preferred size from content metrics (text/icon/thumb/line layout).
- `Container` widget helpers reconcile from the previous committed frame result, call `Widget::measure`, allocate the widget rectangle, then call `Widget::render`.
- Returning `<= 0` for either axis still means "use layout fallback/defaults" for that axis.
- `next_cell()` is the raw layout helper that does not run widget preferred sizing.

### Flow helpers
- `with_row(widths, height, ...)` configures an explicit multi-slot row track.
- `with_grid(widths, heights, ...)` configures an explicit row/column track matrix and emits cells row-major.
- `SizePolicy::Weight(value)` distributes available track space by sibling weight ratio (spacing accounted for). In single-track flows, it uses a `0..=100` scale.
- `stack(height, ...)` configures a vertical one-slot flow with width `SizePolicy::Remainder(0)`.
- `stack_direction(height, direction, ...)` is the same as `stack`, but allows `StackDirection::BottomToTop`.
- `stack_with_width(width, height, ...)` is the same as `stack`, but with explicit width policy.
- `stack_with_width_direction(width, height, direction, ...)` combines explicit width policy with directional stacking.
- `column(...)` starts a nested scope; inside it you can choose row or stack flow independently.

## Images and textures
Some widgets can render an `Image`, which can reference either a slot **or** an uploaded texture at runtime:

```rust
let texture = ctx.load_image_from(ImageSource::Png { bytes: include_bytes!("assets/IMAGE.png") })?;
let mut image_button = Button::with_image(
    "External Image",
    Some(Image::Texture(texture)),
    WidgetOption::NONE,
    WidgetFillOption::ALL,
);
ui.button(&mut image_button);
```

- `Image::Slot` renders an entry from the atlas and benefits from batching.
- `Image::Texture` targets renderer-owned textures (the backend handles binding when drawing).
- `WidgetFillOption` controls which interaction states draw a filled background; use `WidgetFillOption::ALL` to keep the default normal/hover/click fills.
- Use `Context::load_image_rgba`/`load_image_from` and `Context::free_image` to manage the lifetime of external textures.

## Cargo features
- `builder` *(default)* – enables the runtime atlas builder and PNG decoding helpers used by the examples.
- `png_source` – allows serialized atlases and `ImageSource::Png { .. }` uploads to stay compressed.
- `save-to-rust` – enables `AtlasHandle::to_rust_files` to emit the current atlas as Rust code for embedding.
- `example-backend` – shared internal gate used by examples; pair it with exactly one concrete backend.
- `example-glow` / `example-vulkan` / `example-wgpu` – concrete example backends; choose exactly one when running examples.

Disabling default features leaves only the raw RGBA upload path (`ImageSource::Raw { .. }`):
`cargo build --no-default-features`

The demos require `builder`, so run them with `--no-default-features` plus `builder`:
`cargo run --example demo-full --no-default-features --features "example-vulkan builder"`

Equivalent command using the shared gate explicitly:
`cargo run --example demo-full --no-default-features --features "example-backend example-vulkan builder"`

To export an atlas as Rust, enable `save-to-rust` (optionally `png_source` for PNG bytes) and call `AtlasHandle::to_rust_files`, or use the helper binary:
`cargo run --bin atlas_export --features "builder save-to-rust" -- --output path/to/atlas.rs`

## Text rendering and layout
- Container text widgets automatically center the font’s **baseline** inside each cell, and every line gets a small vertical pad so glyphs never touch the widget borders.
- `Container::text_with_wrap` supports explicit wrapping modes (`TextWrap::None` or `TextWrap::Word`) and renders wrapped lines back-to-back inside an internal column, so the block keeps the outer padding without adding extra spacing between lines.
- Custom drawing code can call `Container::draw_text` directly when precise placement is required, or use `draw_control_text` to get automatic alignment/clip handling.

### Version 0.6
- [x] Added retained widget trees.
    - [x] New `WidgetTree` / `WidgetTreeBuilder` API with stable `NodeId`s and reusable widget/layout hierarchy.
    - [x] `Container::widget_tree`, `build_tree`, and `build_tree_with_key` execute retained trees through the normal container/widget paths.
    - [x] Tree nodes now cover widgets, panels, headers/tree nodes, row/grid/column/stack groups, custom render widgets, and `run(...)` escape hatches.
- [x] Migrated shipped UI to the retained model.
    - [x] `examples/simple`, `examples/calculator`, `examples/demo-full`, and the file dialog now build trees once and replay them every frame.
    - [x] Complex sections that depend on live container geometry stay on `tree.run(...)` for now.
- [x] Reworked dispatch around `FrameResults`.
    - [x] Replaced per-call output slots with a per-frame result registry keyed by widget ID.
    - [x] `window` / `dialog` / `popup` closures now receive `(&mut Container, &mut FrameResults)`.
    - [x] Added handle-oriented helpers such as `FrameResults::state_of_handle` and `widget_id_of_handle`.
- [x] Simplified widget batching and handle-backed dispatch.
    - [x] Migrated generic dispatch to `widget_ref(...)` batches and unified container batch helpers.
    - [x] Stabilized demo/file-dialog labels by reusing persistent `ListItem` state instead of rebuilding labels every frame.
- [x] Improved interaction routing and widget input behavior.
    - [x] Mouse coordinates delivered to interactive widgets/custom render callbacks are now relative to the widget rectangle.
    - [x] Fixed stale hover refocus behavior and refined root/panel hover routing for nested retained containers.
- [x] Expanded layout/rendering coverage for the new API surface.
    - [x] Added weight-based layout sizing and updated the calculator/demo examples accordingly.
    - [x] Hardened Vulkan frame resource lifetime handling for dynamic UI/custom rendering workloads.

### Version 0.5
- [x] Widget identity moved fully to pointer-based IDs.
    - [x] Removed `with_id`; focus/hover now use widget trait-object/state pointers.
- [x] Layout refactor: introduced `LayoutEngine` + specialized flows (`RowFlow`, `StackFlow`) instead of a one-size-fits-all manager.
    - [x] Preferred sizing pipeline: widget helpers now reconcile retained state, call `Widget::measure`, then allocate rectangles before `Widget::render`.
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
    - [x] DrawCtx extracted into its own module and shared via WidgetCtx.
    - [x] WidgetState/WidgetCtx pipeline with ControlState returned from `update_control`.
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
