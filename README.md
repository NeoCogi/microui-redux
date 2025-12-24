# Rxi's Microui Port to Idiomatic Rust
[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

This was originally a port of Rxi's MicroUI to Rust language. We used C2Rust to create the initial code and iterated > 60 times to make microui-rs. This (microui-redux) builds on top of that by much more like custom rendering widget for 3D, dialogs, file dialog!

While we tried to keep the usage pattern as close to the original as possible, we wanted also to make it as idiomatic to Rust as possible (closures, safety, ...). In contrast with ![microui-rs](https://github.com/neocogi/microui-rs), this version uses the standard library to give us more flexibity and switch to closures for all container related operations (Window, Panel, Columns, ...).

## Demo
Clone and build the demo (enable exactly one backend feature):
```
$ cargo run --example demo-full --features example-vulkan   # Vulkan backend
# or
$ cargo run --example demo-full --features example-glow     # Glow backend
```

![random](https://github.com/NeoCogi/microui-redux/raw/master/res/microui.png)

## Key Concepts
- **Context**: owns the atlas, renderer handle, user input and root windows. Each frame starts by feeding input into the context, then calling `context.window(...)` for every visible window or popup.
- **Container**: describes one layout surface. Every window, panel, popup or custom widget receives a mutable `Container` that exposes high-level widgets (buttons, sliders, etc.) and lower-level drawing helpers.
- **Layout manager**: controls how cells are sized. `Container::with_row` lets you scope a set of widgets to a row of `SizePolicy`s, while nested columns can be created with `container.column(|ui| { ... })`.
- **Widget**: stateful UI element implementing the `Widget` trait (for example `Button`, `Textbox`, `Slider`). These structs hold interaction state and supply stable IDs.
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

Window, dialog, and popup builders now accept a `WidgetBehaviourOption` to control scroll behavior. Use `WidgetBehaviourOption::NO_SCROLL`
for popups that should not scroll, `WidgetBehaviourOption::GRAB_SCROLL` for widgets that want to consume scroll, and
`WidgetBehaviourOption::NONE` for default behavior. Custom widgets receive consumed scroll in `CustomRenderArgs::scroll_delta`.

## Images and textures
Widgets take an `Image` enum, which can reference either a slot **or** an uploaded texture at runtime:

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
- `Image::Texture` targets renderer-owned textures. The command list flushes before drawing the texture so the backend can bind the correct resource.
- `WidgetFillOption` controls which interaction states draw a filled background; use `WidgetFillOption::ALL` to keep the default normal/hover/click fills.
- Use `Context::load_image_rgba`/`load_image_from` and `Context::free_image` to manage the lifetime of external textures.

## Cargo features
- `builder` *(default)* – enables the runtime atlas builder and PNG decoding helpers used by the examples.
- `png_source` – allows serialized atlases and `ImageSource::Png { .. }` uploads to stay compressed.
- `save-to-rust` – emits the current atlas as Rust code so it can be embedded in your binary.

Disabling default features leaves only the raw RGBA upload path (`ImageSource::Raw { .. }`). The demos require `builder`, so run them with `--features builder` if you build with `--no-default-features`.

## Text rendering and layout
- Container text widgets automatically center the font’s **baseline** inside each cell, and every line gets a small vertical pad so glyphs never touch the widget borders.
- `Container::text_with_wrap` supports explicit wrapping modes (`TextWrap::None` or `TextWrap::Word`) and renders wrapped lines back-to-back inside an internal column, so the block keeps the outer padding without adding extra spacing between lines.
- Custom drawing code can call `Container::draw_text` directly when precise placement is required, or use `draw_control_text` to get automatic alignment/clip handling.

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
