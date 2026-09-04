# Support and compatibility

`microui-redux` is pre-1.0 and currently intended for experimentation and early integration. This
page records what downstream users can rely on without implying guarantees the project does not yet
make.

## Rust toolchain

The crate uses Rust 2024 edition but does not publish a minimum supported Rust version (MSRV).
Development and CI target the current stable Rust toolchain. Dependency updates or a pre-1.0 minor
release may therefore require a newer stable compiler.

## Platforms and renderers

The library owns retained UI behavior but does not create an operating-system window, graphics
device, or event loop. Those responsibilities belong to the application through
`RendererBackend` and the input-forwarding methods on `Context`.

The repository includes SDL-based Glow, Vulkan, and WGPU integrations as native executable
examples, plus a `web-sys` WebGL 2 integration for an HTML canvas. The automated project
workflow runs native checks on Linux and builds the browser demo for GitHub Pages. Windows and
macOS portability are intended, but they are not currently a CI-backed platform guarantee. Treat
the example integrations as reference code to adapt to an application's own renderer and
lifecycle.

## API compatibility

Pre-1.0 minor releases may make breaking API and data-format changes. The project favors a smaller,
consistent API over compatibility layers, so downstream applications should review the
[changelog](CHANGELOG.md) before upgrading. Patch releases should remain focused on compatible fixes,
but no formal long-term-support branch exists.

## Reporting a problem

Use the [GitHub issue tracker](https://github.com/NeoCogi/microui-redux/issues). Include the crate
version or commit, enabled Cargo features, Rust version, operating system, rendering backend, and a
minimal reproduction when possible.
