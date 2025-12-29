# Rxi's Microui Port to Idiomatic Rust
[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

This project started as a C2Rust conversion of Rxi's MicroUI and has since grown into a Rust-first UI toolkit. It preserves the immediate-mode feel while using stateful widget structs with stable IDs, a row/column layout helper API, and backend-agnostic rendering hooks.

Compared to [microui-rs](https://github.com/neocogi/microui-rs), this crate embraces std types, closure-based window/panel/column scopes, and richer widgets such as custom rendering callbacks, dialogs, and a file dialog.

## Demo
Clone and build the demo (enable exactly one backend feature):
```
$ cargo run --example demo-full --features example-vulkan   # Vulkan backend
# or
$ cargo run --example demo-full --features example-glow     # Glow backend
```

![random](https://github.com/NeoCogi/microui-redux/raw/master/res/microui.png)

## Key Concepts
- **Context**: owns the renderer handle, user input, and root windows. The atlas is provided by the renderer and accessed through the context. Each frame starts by feeding input into the context, then calling `context.window(...)` for every visible window or popup.
- **Container**: describes one layout surface. Every window, panel, popup or custom widget receives a mutable `Container` that exposes high-level widgets (buttons, sliders, etc.) and lower-level drawing helpers.
- **Layout manager**: controls how cells are sized. `Container::with_row` lets you scope a set of widgets to a row of `SizePolicy`s, while nested columns can be created with `container.column(|ui| { ... })`.
- **Widget**: stateful UI element implementing the `Widget` trait (for example `Button`, `Textbox`, `Slider`). These structs hold interaction state and supply stable IDs derived from their address; use `with_id` when the state can move.
- **Renderer**: any backend that implements the `Renderer` trait can be used. The included SDL2 + glow example demonstrates how to batch the commands produced by a container and upload them to the GPU.

```rust
let mut name = Textbox::new("");
ctx.window(&mut main_window, ContainerOption::NONE, WidgetBehaviourOption::NONE, |ui| {
    let widths = [SizePolicy::Fixed(120), SizePolicy::Remainder(0)];
    ui.with_row(&widths, SizePolicy::Auto, |ui| {
        ui.label("Name");
        ui.textbox_ex(&mut name);
    });
});
```

### Widget IDs
Widget IDs default to the address of the widget state. This is stable as long as the state stays at a fixed address, but it can change if the state lives inside a `Vec` that grows/shrinks (reallocation moves items). In those cases, set explicit IDs on the widget state:

```rust
let mut row_button = Button::new("Row").with_id(Id::from_str("settings/row/0"));
ui.button(&mut row_button);
```

Window, dialog, and popup builders now accept a `WidgetBehaviourOption` to control scroll behavior. Use `WidgetBehaviourOption::NO_SCROLL`
for popups that should not scroll, `WidgetBehaviourOption::GRAB_SCROLL` for widgets that want to consume scroll, and
`WidgetBehaviourOption::NONE` for default behavior. Custom widgets receive consumed scroll in `CustomRenderArgs::scroll_delta`.

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

Disabling default features leaves only the raw RGBA upload path (`ImageSource::Raw { .. }`):
`cargo build --no-default-features`

The demos require `builder`, so run them with `--no-default-features` plus `builder`:
`cargo run --example demo-full --no-default-features --features "example-vulkan builder"`

To export an atlas as Rust, enable `save-to-rust` (optionally `png_source` for PNG bytes) and call `AtlasHandle::to_rust_files`, or use the helper binary:
`cargo run --bin atlas_export --features "builder save-to-rust" -- --output path/to/atlas.rs`

## Text rendering and layout
- Container text widgets automatically center the font’s **baseline** inside each cell, and every line gets a small vertical pad so glyphs never touch the widget borders.
- `Container::text_with_wrap` supports explicit wrapping modes (`TextWrap::None` or `TextWrap::Word`) and renders wrapped lines back-to-back inside an internal column, so the block keeps the outer padding without adding extra spacing between lines.
- Custom drawing code can call `Container::draw_text` directly when precise placement is required, or use `draw_control_text` to get automatic alignment/clip handling.

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
