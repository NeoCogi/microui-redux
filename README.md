# microui-redux

[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

`microui-redux` is a retained, backend-agnostic Rust GUI toolkit inspired by
[rxi/microui](https://github.com/rxi/microui). It keeps microui's compact rendering model while
using unique owning `Node` trees, typed weak widget handles, context-owned roots, and typed backend
frames.

> **Alpha status:** `0.8.0-alpha.5` is the current alpha of the breaking retained-API
> redesign. The 0.8 line is not API-compatible with 0.7 and may continue to evolve before the
> stable 0.8.0 release.

Compared with [microui-rs](https://github.com/neocogi/microui-rs), this crate embraces standard
library types, reusable retained trees, and richer widgets such as custom rendering callbacks,
dialogs, and a file dialog.

![microui-redux demo with retained windows and controls](res/microui.png)

## Getting started

Use the explicit alpha version while the retained API is being evaluated:

```toml
[dependencies]
microui-redux = "0.8.0-alpha.5"
```

`microui-redux` does not create a native window or graphics device. Applications provide a
`RendererBackend`; the repository examples contain SDL-based Glow, Vulkan, and WGPU integrations.

Clone the repository and run the full demo with one backend:

```bash
cargo run --example demo-full --features example-glow
```

See [Examples](docs/EXAMPLES.md) for the other backends, asset requirements, and size-focused
builds.

## Documentation

- [Documentation index](docs/README.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Typed events and retained services](docs/EVENTS.md)
- [Retained layout](docs/LAYOUT.md)
- [Per-window application menus](docs/MENUS.md)
- [Rendering and backend integration](docs/RENDER.md)
- [Backend frames and custom rendering](docs/BACKENDS.md)
- [Fonts and typography](docs/TYPOGRAPHY.md)
- [Examples and demos](docs/EXAMPLES.md)
- [Cargo features](docs/FEATURES.md)
- [Version history and roadmap](docs/CHANGELOG.md)
- [Bundled asset attribution and licenses](docs/ASSETS.md)

The application-facing API is centered on `microui_redux::prelude` and
`microui_redux::retained`. Low-level rendering lives under `microui_redux::render`, and atlas
construction lives under `microui_redux::atlas::builder`. The generated API reference is
available on [docs.rs](https://docs.rs/microui-redux).

## License

The project code is licensed under the [BSD 3-Clause License](LICENSE). Bundled fonts, icons, and
demo assets retain their original terms; see [asset attribution and licenses](docs/ASSETS.md).
