# Changelog

## 0.8.0 (unreleased)

Version 0.8 is a deliberate retained-API redesign. It unifies application authoring, input,
events, surfaces, menus, and rendering around uniquely owned retained values. Superseded alpha APIs
and compatibility layers were removed.

### Retained authoring and events

- `Node` is the sole mountable owner of a widget or container. Built-in constructors return a typed
  weak handle beside that node for later state access.
- Concrete widgets now own their semantic state, typed event ports, measurement, update, and paint
  behavior. `LeafWidget` and `ContainerWidget` define the two retained layout roles.
- `Context` owns input ordering, surface lifetime, rendering, and the application event dispatcher.
  `update_ui` and `update_ui_state` are the synchronization boundary for input and programmatic
  mutations.
- `WidgetEventPortHandle<E>` connects through `subscribe`, `subscribe_with`, `subscribe_context`,
  and `subscribe_context_with`; application handlers run only after retained widget borrows end.
- `FileDialog` is now a reusable, application-owned component whose completion is delivered through
  the same typed event dispatcher.

### Surfaces, menus, and input

- One `SurfaceForest` owns windows, structural child windows, dialogs, application popups, and menu
  popups. Parent edges define lifetime and one active popup path defines transient visibility.
- Stable `WindowHandle`, `PopupHandle`, and `MenuItemHandle` capabilities reject destroyed, stale,
  or foreign targets. Failed owned-surface creation returns its unchanged input for retry.
- Structural child windows inherit their family layer and may clip their complete output to the
  parent's application content.
- Declarative menu bars and standalone menu popups compile directly into manager-owned menu
  surfaces. Checked item access supports live labels, enabled state, shortcut hints, and check/radio
  marks.
- Logical key events, persistent focus, wrapping Tab traversal, Ctrl+F6 window cycling, popup Escape
  handling, and F10/Alt menu navigation now share one backend-neutral keyboard contract.

### Rendering, themes, and widgets

- `Painter` records backend-neutral work into a display list. `RendererBackend::Frame<'a>` provides
  one exclusive submission frame, and typed custom-render callbacks receive that selected frame
  directly.
- `ContextFrame::render_ui` is paint-only and rejects missing, stale, or dimension-mismatched UI
  commits before acquiring a backend frame.
- `Skin` provides typed role/state visuals, stable font and icon references, configurable window
  chrome, and atomic skin/atlas replacement. Strict JSON themes compile before atlas construction.
- Original Windows 3.11, Windows 95, and Mac OS 9 themes are bundled and selectable in `demo-full`.
- Public retained scrolling now includes `Scrollbar`, `ScrollArea`, and a scrollable `TextArea`.
- Atlas configuration supports semantic font roles and named variants in runtime-built, generated,
  and external atlas forms.
- The examples and guides were updated for the 0.8 contracts. Standalone guides now participate in
  doctesting, with complete Rust examples compiled during documentation tests.

## 0.7.0 — 2026-05-25

Version 0.7 completed the first retained migration by moving root lifetime, interaction identity,
and frame traversal into `Context` instead of requiring applications to resubmit every root each
frame.

- Applications registered windows, dialogs, and popups in `Context`, then controlled visibility,
  options, and explicit destruction through retained handles.
- Private runtime identity and typed widget events replaced public interaction lookup.
- Widget execution was split into explicit measure, update, and paint phases.
- Retained layout gained weighted sibling sizing, fractional sizing, scroll areas, and corrected root
  chrome, popup, dialog, and resize behavior.
- Rendering was unified behind `Painter`, `DisplayList`, `Renderer`, and `RendererBackend`; external
  textures and atlas inputs gained stricter validation.
- Glow, Vulkan, and WGPU examples adopted the same retained-root lifecycle.

## 0.6.x

Version 0.6 introduced retained `WidgetTree` authoring on top of the earlier per-frame root loop.

- Retained trees replaced immediate closure-based widget authoring.
- Widgets gained persistent state, measurement, typed events, and custom painting.
- Custom-render leaves received geometry and input for application rendering.
- Atlas construction and generated/external data gained multi-font support.
- Version 0.6.1 made runtime atlas construction the demo default and prebuilt embedding opt-in.

## 0.5

- Widget identity moved to pointer-based IDs and explicit stateful widget instances.
- Layout was split into a shared engine and specialized row/stack flows, including directional
  stacks and weighted or fractional sizing.
- Text editing, scrollbar behavior, file-dialog layout, and PNG decoding were corrected.
- Container styling used shared `Rc<Style>` values and window chrome state moved into `Window`.
- WGPU example rendering and custom graphics demos were added.

## 0.4

- Stateful built-in controls replaced the legacy immediate widget calls.
- Widget measurement and input were routed through a shared execution context.
- Persistent widget identity tracked focus and hover.
- Dialogs, file-dialog interaction, atlas handling, and scrollbar tests were expanded.
- Styles became shared across containers and panels.

## 0.3

- Standard-library collections and parsing replaced the earlier custom equivalents.
- Container layout, clip stacks, and command recording were redesigned.
- Atlas building, generated Rust data, external PNG data, images, and multiple fonts were added.
- Custom drawing and 3D pass-through rendering gained mouse, keyboard, text, and drag input.
- Dialogs and the file dialog were introduced.
- The calculator, simple example, and full rendering demo established the example suite.
