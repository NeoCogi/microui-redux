# Version history and roadmap

## Roadmap to Version 0.9
- [ ] Key navigation
- [ ] Async/Multi-Threading?
- [ ] Theming/Skinning
    - [ ] Win311 Theme

## Version 0.8.0-alpha.5

`0.8.0-alpha.5` adds explicit retained-root layering to the breaking 0.8 API and demonstrates the
model with a fullscreen menu-bearing X-Y grid surface beneath independent floating windows.

- [x] Added sixteen fixed application layers for ordinary windows.
    - [x] Layers are numbered `0` through `15`, ordered bottom to top, and new windows retain the compatibility-preserving default layer `15`.
    - [x] `Context::set_root_layer` and `EventContext::set_root_layer` assign `LayerBinding::Fixed(u8)`; `root_layer_binding` exposes fixed, inherited, unbound, and modal policy.
    - [x] Layout, painting, hit testing, and debug inspection share one complete stacking key, so `bring_root_to_front` and pointer activation reorder only within an effective layer.
- [x] Bound transient roots to the layer of the root that initiated them.
    - [x] `show_popup` and `show_popup_at` require an initiating `RootId`; the anchored API therefore gains an additional argument in this alpha.
    - [x] A shown popup uses `LayerBinding::Inherited(source)` and a transient tier above ordinary roots in that source layer, but remains below every higher fixed layer.
    - [x] Popup-initiated chains normalize to their non-popup source, source layer changes propagate to retained popups, and source hiding or destruction dismisses visible transients.
    - [x] Generic `set_root_visible(popup, true)` now returns `PopupInitiatorRequired`; generic visibility remains valid for hiding a popup.
    - [x] `WindowMenu` supplies its owning window as the initiator, keeping every menu panel in the same effective layer as its persistent bar and body.
- [x] Kept modal policy structurally above the numeric application range.
    - [x] Dialogs report `LayerBinding::Modal` and reject direct fixed-layer assignment.
    - [x] A popup initiated by the active dialog occupies the modal transient tier and joins that dialog's exclusive input group; blocked application roots cannot open popups during a modal transaction.
- [x] Separated ordinary keyboard activation from visual stacking with `active_root`.
    - [x] Pressing a lower-layer window focuses it without raising it across a higher layer, while overlap hit testing continues to follow visual priority.
    - [x] Pointer capture, popup-to-source activation, modal routing, root hiding, and destruction reconcile the active root without adding parent-window ownership.
    - [x] Wheel input follows the topmost eligible root under the pointer independently of `active_root`, while an in-progress pointer drag remains confined to its captured root.
- [x] Added edge-to-edge application-surface support.
    - [x] `WindowOption::NO_PADDING` removes only the root-owned content inset and preserves normal descendant style padding.
    - [x] `demo-full` now resizes a dedicated chromeless `WindowMenu` root to the drawable viewport at layer `0`, renders an interactive perspective X-Y grid in its body, and gives that surface its own Grid/Help menu.
    - [x] The exposed grid supports left-drag arcball rotation, bounded wheel zoom, and menu-driven view reset; homogeneous six-plane segment clipping prevents behind-camera projections from emitting stray geometry.
    - [x] The original movable, resizable Demo Window and all other independent windows remain at the default layer above the grid; modal dialogs remain topmost.
- [x] Added retained and downstream tests for layer validation, bounded raising, popup inheritance and lifetime, modal popups, active-root keyboard routing, and no-padding chrome geometry.

## Version 0.8.0-alpha.4

`0.8.0-alpha.4` continues the breaking retained-API redesign from the published
`0.8.0-alpha.3`. It is intended for integration testing and API feedback before the stable
`0.8.0` release.

- [x] Simplified application-owned event coordination.
    - [x] Folded the separate event listener into Context subscriptions and renamed weak widget event endpoints to `WidgetEventPortHandle<E>`.
    - [x] Made `FileDialog` an application-owned retained component with reusable controls, subscriber-driven completion, and independent instances.
    - [x] Extracted an `InputRouter` per retained runtime for node targeting, focus, hover, and capture state.
- [x] Completed reusable retained scrolling composition.
    - [x] Promoted `Scrollbar` to a public retained widget with a typed range/value/event contract.
    - [x] Made `ScrollArea` a complete one-child viewport with retained scrollbars and `scroll_to_end` support.
    - [x] Rebuilt `TextArea` from editable content and `ScrollArea`, including nested wheel delegation and stable viewport layout.
- [x] Added cascading per-widget style overrides without replacing Context-owned theme defaults.
- [x] Added application-owned per-window menus.
    - [x] Concrete `MenuItem`, `MenuGroup`, `Menu`, `MenuPanel`, and `WindowMenu` types compose registered retained items without a command type, generic menu model, `Any`, or copied specifications.
    - [x] Each item owns its typed `MenuItemSubmitted` source and live enabled/check/radio presentation; top-level menus retain independent auto-sized popup trees.
    - [x] `PopupHandle` gives `show_popup_at` compile-time root-kind safety for anchored placement and popup exclusivity.
    - [x] `demo-full` includes File, View, and Help menus that invoke application behavior and update live item state.
    - [x] Shortcut hints are presentation-only; keyboard navigation, mnemonics, automatic check/radio behavior, and cascading submenus are not implemented in this alpha.

## Version 0.8.0-alpha.3

`0.8.0-alpha.3` builds on the first public alpha of the breaking retained-API redesign relative to
`0.7.0`. It is intended for integration testing and API feedback before the stable `0.8.0`
release. This alpha completes event-time coordination for transient roots and file-dialog results,
removing the remaining application-level frame polling from `demo-full`.

- [x] Replaced retained tree building with unique owning `Node` values.
    - [x] Handle-bearing built-in leaf and container constructors return `(TypedWidgetHandle<W>, Node)`; stateless `Custom::create` returns a runtime for explicit `Node` mounting.
    - [x] Moving or mounting a node transfers its single owner; typed widget handles remain weak.
    - [x] Public runtime node identity and generic interaction-result lookup were removed.
- [x] Merged semantic state and runtime behavior into concrete widgets.
    - [x] Each widget owns its parameters-derived state, native event ports, measurement, update, and paint behavior.
    - [x] `LeafWidget` defines intrinsic measurement and `ContainerWidget` defines child-aware layout.
    - [x] `Linear`, `Grid`, `Disclosure`, and `ScrollArea` expose retained mutation through typed handles.
- [x] Made `Context` the retained transaction boundary.
    - [x] Context owns the ordered input FIFO, complete root forest, renderer, and application event dispatcher.
    - [x] `update_ui` and `update_ui_state` commit layout after every queued input event.
    - [x] `ContextFrame::render_ui` is paint-only and rejects missing, stale, or dimension-mismatched commits before backend acquisition.
    - [x] `EventContext<'_>` lends safe Context-owned mutation access only after retained widget borrows end and before the next layout commit.
- [x] Added context-owned typed application events.
    - [x] Widgets expose weak `WidgetEventHandle<E>` endpoints for their native event types.
    - [x] `Context<B, State>::subscribe` and `subscribe_with` dispatch into application state after retained widget borrows end.
    - [x] `subscribe_context` and `subscribe_context_with` opt handlers into the same typed dispatch with short-lived root and service mutation access.
    - [x] Context-owned services publish typed lifecycle events through the same generic dispatcher without control-specific dispatcher branches.
    - [x] Removed the public standalone event `Session`; applications without model callbacks continue to use `Context<B>` without rebuilding retained roots.
- [x] Extracted backend-independent retained root management.
    - [x] Windows, dialogs, and popups remain context-owned until explicit destruction.
    - [x] `RootHandle` exposes typed chrome state and events without extending root lifetime.
    - [x] Modal routing, popup dismissal, focus, capture, root movement, and resizing share one retained window manager.
- [x] Removed frame-polled transient-root and file-dialog coordination from the full demo.
    - [x] Popup and file-dialog opening mutate Context-owned state directly from the typed event that requested them; application command flags were removed.
    - [x] `ComboSubmitted` carries same-transaction opening geometry; the demo's existing shared state reconciles `RootSubmitted::PopupDismissed` back into `Combo`, so source-root interaction or replacement by another popup closes its semantic state without geometry APIs or frame polling.
    - [x] Demo window position, size, and minimum-size reconciliation consume `RootChanged` rather than polling `RootChrome` from frame processing.
    - [x] File-dialog acceptance and cancellation publish exactly one `FileDialogCompleted` event through a Context-lifetime source; application frame code no longer polls session status.
    - [x] Abandoned file-dialog sessions are settled after application dispatch and before layout or the next queued input; `FileDialogSession` warns when its ownership capability is ignored.
    - [x] `FileDialogStatus` represents terminal outcomes only; `FileDialogSession::status()` uses `None` for pending while completion handlers exhaustively match accepted or cancelled outcomes.
    - [x] The general layer remains unaware of combos and file-dialog behavior: `EventContext` exposes existing root/service operations, while specialized payloads stay with their owners.
- [x] Unified rendering behind recorded painter operations and typed backend frames.
    - [x] `Painter` records backend-neutral work into the framework-owned display list.
    - [x] `RendererBackend::Frame<'a>` gives each backend one exclusive submission frame.
    - [x] Typed custom-render callbacks receive the concrete selected backend frame without a shared backend handle.
    - [x] Glow, Vulkan, and WGPU repository examples use the same retained application lifecycle.
- [x] Expanded atlas and typography support.
    - [x] Atlas configuration supports multiple named font variants and semantic font roles.
    - [x] Default styles bind conventional font and icon names from the backend atlas.
    - [x] Runtime construction, generated Rust embedding, and external PNG loading share serialized atlas metadata.
- [x] Documented the alpha API and known limitations.
    - [x] Documented the context-owned typed-event architecture.
    - [x] Documented event-time `EventContext` ownership, generic transient-root coordination, and subscriber-driven file-dialog completion.
    - [x] Documented UTF-8 editing, atlas glyph coverage, scalar-value fallback, and text-layout limits.
    - [x] Documented the trusted atlas-metadata contract, external-atlas workflow, and UTF-8 file-dialog path boundary.

## Version 0.7.0

Version `0.7.0` is the context-owned retained-root release. Compared to `0.6.1`, it completes the retained migration by moving root lifetime, interaction identity, and frame traversal into the context instead of requiring applications to resubmit each root every frame.

- [x] Moved retained root lifetime into `Context`.
    - [x] Applications register windows, dialogs, and popups with `create_window`, `create_dialog`, and `create_popup`.
    - [x] Registered roots are traversed by `ContextFrame::render_ui`; visibility and options are controlled with `set_root_visible` and `set_root_options`, while destruction is explicit.
    - [x] The old callback-based per-frame root submission path was removed from the supported API.
- [x] Replaced public interaction lookup with typed retained state.
    - [x] The former builder-generated public identity path was removed in favor of private runtime identity and typed widget events.
    - [x] `RootHandle` exposes checked root state while widget/container constructors return typed weak widget handles.
    - [x] Root windows, scroll areas, and window chrome persist without tree reconstruction.
- [x] Split retained widget execution into explicit `measure`, `update`, and `paint` phases.
    - [x] Layout records geometry first; update records control state and typed events; paint records commands from updated widget state.
    - [x] Custom-render nodes receive content and clip geometry through `CustomRenderArgs`, while widget input remains in the update phase.
    - [x] Built-in widgets and examples capture events directly from concrete typed widget runtimes.
- [x] Reworked retained layout, scroll areas, and root chrome.
    - [x] `SizePolicy::Weight` now uses sibling share ratios, and `SizePolicy::Fraction` covers explicit proportional sizing.
    - [x] `ScrollArea` is a retained viewport around one arbitrary content node; its scrollbars are real structural leaf widgets.
    - [x] Root auto-size, popup placement/close behavior, dialog z-order, scrollbars, and bottom-right resize handling were aligned with retained traversal.
- [x] Tightened drawing, texture, atlas, and backend behavior.
    - [x] Renderer display-list execution batches ordinary draw operations while preserving custom render and retained scroll-area boundaries.
    - [x] External texture uploads validate dimensions and byte counts, and texture clipping has a dedicated smoke example.
    - [x] Atlas code is split into builder, runtime, image, source, and codegen modules; `atlas_export` now requires `png_source` when exporting PNG-backed atlas data.
    - [x] Glow, Vulkan, and WGPU examples share retained root handling, and `examples/retained-custom-drawing` documents the custom painting path.
- [x] Unified rendering behind `Painter`, `DisplayList`, `Renderer`, and `RendererBackend`.
    - [x] Removed the old immediate drawing and mutable clipping facades in favor of scoped recording and single-pass execution.
    - [x] Removed the shared backend handle; Renderer now uniquely owns its backend and lends one typed frame to synchronous execution.
    - [x] Documented the clean rendering break in the subsystem guide and compiling examples.
- [x] Reduced migration surface and documented internals.
    - [x] Public imports are grouped around `prelude`, `retained`, and the `render` subsystem.
    - [x] Direct container drawing is no longer part of the application authoring path.
    - [x] Runtime modules, private structs, enums, and functions now have rustdoc or implementation comments, and the retained behavior is covered by focused tests.

## Version 0.6.x

Version `0.6.0` introduced retained `WidgetTree` authoring on top of the older per-frame root submission loop. Compared to `0.5.0`, `0.6.x` replaced immediate/closure widget authoring with reusable retained trees, widget handles, committed interaction results, custom graphics primitives, and multi-font atlas support.

- [x] `Context::window`, `Context::dialog`, and `Context::popup` accepted retained trees instead of UI-building closures.
- [x] `WidgetTreeBuilder` introduced reusable widget/layout hierarchies with widgets, panels, headers/tree nodes, row/grid/column/stack groups, and custom-render leaves.
- [x] Widgets reported intrinsic sizes through `measure` and updated persistent state through the retained traversal.
    - [x] Interaction observation later moved from generic frame results to typed widget-owned events.
- [x] The widget paint context gained widget-local custom painting for rectangles, text/icons/images, line strokes, polygon fills, and scoped clips.
- [x] Runtime atlas building and offline/prebuilt atlas export gained shared multi-font configuration.
- [x] Version `0.6.1` switched demos to runtime atlas construction by default and made prebuilt atlas embedding opt-in.

## Version 0.5

- [x] Widget identity moved fully to pointer-based IDs.
    - [x] Removed `with_id`; focus/hover now use widget trait-object/state pointers.
- [x] Layout refactor: introduced `LayoutEngine` + specialized flows (`RowFlow`, `StackFlow`) instead of a one-size-fits-all manager.
    - [x] Preferred sizing pipeline: widget helpers now call `Widget::measure`, allocate rectangles, then run widgets directly against the current frame input.
    - [x] Directional stack support: `StackDirection::{TopToBottom, BottomToTop}` plus `stack_direction` and `stack_with_width_direction`.
- [x] Context/container API cleanup: `Context` module split, input forwarding helpers, container state encapsulation, and handle views.
- [x] Widget internals cleanup: helper macroization/simplification, node/widget scaffolding unification, and text widget module split.
- [x] Text and input fixes: shared text layout/edit paths, textbox delete/end fixes, centralized widget input fallback.
- [x] Scrollbar behavior cleanup: unified sizing, layout, and drag handling.
- [x] File dialog and atlas fixes, including file dialog layout redesign and footer/button spacing corrections.
- [x] Added WGPU example backend and migrated demo-full to new layout flow APIs.
- [x] Added directional stack demo window and expanded documentation/comments for layout and WGPU renderer.

## Version 0.4

- [x] Stateful widgets
    - [x] Stateful widgets for core controls (button, list item, checkbox, textbox, slider, number, custom).
    - [x] Pointer-based widget IDs; InputSnapshot threaded through widgets and cached per frame.
    - [x] IdManager removed; widget IDs now derive from state pointers.
    - [x] Widget API redesign requires stateful widget instances; trait/type renames applied.
    - [x] Legacy `button_ex*` shims removed.
    - [x] Drawing state was extracted into the shared widget execution context.
    - [x] Widget state/context pipeline with ControlState returned from `update_control`.
- [x] File dialog UX fixes (close on OK/cancel, path-aware browsing).
- [x] Expanded unit tests for scrollbars, sliders, and PNG decoding paths.
- [x] Style shared via `Rc<Style>` across containers/panels; window chrome state moved into `Window`.
- [x] `Container::style` now uses `Rc<Style>`.

## Version 0.3

- [x] Use `std` (`Vec`, `parse`, ...)
- [x] Containers contain clip stack and command list
- [x] Move `begin_*`, `end_*` functions to closures
- [x] Move to `AtlasRenderer` trait
- [x] Remove/refactor `Pool`
- [x] Change layout code
- [x] Add tree nodes
- [x] Manage window lifetime and ownership outside Context through root windows
- [x] Manage container lifetime and ownership outside containers
- [x] Add software-based textured rectangle clipping
- [x] Add atlas support
    - [x] Runtime atlas builder
        - [x] Icons
        - [x] Font hash tables
    - [x] Separate atlas construction from runtime lookup
    - [x] Add the `builder` feature
    - [x] Save an atlas as Rust source
    - [x] Load an atlas from constant Rust data
- [x] Add the image widget
- [x] Add PNG atlas sources
- [x] Add pass-through rendering commands for 3D viewports
- [x] Add custom rendering widgets
    - [x] Mouse input events
    - [x] Keyboard events
    - [x] Text events
    - [x] Dragging outside the region
    - [x] Rendering
- [x] Add dialog support
- [x] Add the file dialog
- [x] Iterate on APIs and examples
    - [x] Simple example
    - [x] Full API example with 3D rendering and dialogs
- [x] Add documentation
