# Documentation

The root [README](../README.md) gives a short project overview and quick start. The guides here
cover the retained UI model, integration details, and repository workflows.

## Core concepts

- [Architecture](ARCHITECTURE.md) — retained trees, concrete surfaces, ownership, and stable identity.
- [Events](EVENTS.md) — typed widget events, dispatch ordering, roots, and retained services.
- [Built-in widgets](WIDGETS.md) — control and container selection, native events, ownership, and
  retained mutation.
- [Layout](LAYOUT.md) — measurement, tracks, placement, invalidation, and update/paint boundaries.
- [Application menus](MENUS.md) — concrete registered items, retained composition, popup
  coordination, and Windows-style keyboard/submenu navigation.
- [Typography](TYPOGRAPHY.md) — semantic font roles, UTF-8 behavior, glyph coverage, and atlas setup.
- [Skin architecture](SKINNING.md) — concrete roles and states, stable resources, atomic bundles,
  one context-wide skin, and cache identity.
- [JSON themes](THEMES.md) — typed appearance roles and states, PNG nine-patches, flat fallbacks,
  bundled classic themes, and window chrome recipes.

## Rendering and integration

- [Rendering](RENDER.md) — painter and display-list architecture, clipping, textures, backend
  implementation, and performance validation.
- [Backend frames and custom rendering](BACKENDS.md) — selected backend frame types, frame
  lifetimes, custom callbacks, and the backend-frame cube example.
- [Examples](EXAMPLES.md) — running the demos, backend selection, asset loading, and size-focused
  builds.
- [Cargo features](FEATURES.md) — crate features and atlas-loading combinations.

## Project information

- [Version history and roadmap](CHANGELOG.md)
- [Bundled asset attribution and licenses](ASSETS.md)
