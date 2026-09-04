# Documentation

The root [README](../README.md) gives a short project overview and quick start. The guides here
cover the retained UI model, integration details, and repository workflows.

## Core concepts

- [Architecture](../crates/microui-redux/docs/ARCHITECTURE.md) — retained trees, concrete surfaces, ownership, and stable identity.
- [Events](../crates/microui-redux/docs/EVENTS.md) — typed widget events, dispatch ordering, roots, and retained services.
- [Built-in widgets](../crates/microui-redux/docs/WIDGETS.md) — control and container selection, native events, ownership, and
  retained mutation.
- [Layout](../crates/microui-redux/docs/LAYOUT.md) — measurement, tracks, placement, invalidation, and update/paint boundaries.
- [Application menus](../crates/microui-redux/docs/MENUS.md) — concrete registered items, retained composition, popup
  coordination, and Windows-style keyboard/submenu navigation.
- [Typography](../crates/microui-redux/docs/TYPOGRAPHY.md) — semantic font roles, UTF-8 behavior, glyph coverage, and atlas setup.
- [Skin architecture](../crates/microui-redux/docs/SKINNING.md) — concrete roles and states, stable resources, atomic bundles,
  one context-wide skin, and cache identity.
- [JSON themes](../crates/microui-redux/docs/THEMES.md) — typed appearance roles and states, PNG nine-patches, flat fallbacks,
  bundled classic themes, and window chrome recipes.

## Rendering and integration

- [Rendering](../crates/microui-redux/docs/RENDER.md) — painter and display-list architecture, clipping, textures, backend
  implementation, and performance validation.
- [Backend frames and custom rendering](../crates/microui-redux/docs/BACKENDS.md) — selected backend frame types, frame
  lifetimes, custom callbacks, and the backend-frame cube example.
- [Examples](EXAMPLES.md) — running the demos, backend selection, asset loading, and size-focused
  builds.
- [WebGL demo](WEB.md) — building, serving, and publishing the HTML canvas demo.
- [Cargo features](FEATURES.md) — crate features and atlas-loading combinations.

## Project information

- [Changelog](CHANGELOG.md)
- [Support and compatibility](SUPPORT.md) — Rust toolchain, platform, renderer, and API stability
  expectations.
- [Bundled asset attribution and licenses](ASSETS.md)
