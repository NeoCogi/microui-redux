# Version history and roadmap

## Roadmap to Version 0.9
- [ ] Async/Multi-Threading?
- [ ] Theming/Skinning
    - [ ] Win311 Theme

## Version 0.8

Version `0.8` is a deliberate retained-API redesign relative to `0.7`. It unifies application
authoring, input, events, windows, menus, and rendering around uniquely owned retained values; the
superseded alpha APIs and their compatibility layers are not retained.

- [x] Rebuilt retained authoring around unique `Node` ownership and typed weak handles.
    - [x] Built-in leaf and container constructors return their `TypedWidgetHandle<W>` with the sole mountable `Node`; moving or mounting transfers ownership without invalidating handles.
    - [x] Concrete widgets own their semantic state, typed event ports, measurement, update, and paint behavior. `LeafWidget` and `ContainerWidget` define the two retained layout roles.
    - [x] `Linear`, `Grid`, `Disclosure`, `ScrollArea`, and the built-in controls expose direct retained mutation without public runtime-node identities or generic interaction lookup.
- [x] Made `Context` the retained transaction and typed-event boundary.
    - [x] Context owns the ordered input FIFO, surface forest, renderer, and application event dispatcher; `update_ui` and `update_ui_state` commit layout after every queued event.
    - [x] `ContextFrame::render_ui` is paint-only and rejects missing, stale, or dimension-mismatched commits before backend acquisition.
    - [x] `WidgetEventPortHandle<E>` endpoints connect through `subscribe`, `subscribe_with`, `subscribe_context`, and `subscribe_context_with`, dispatching only after retained borrows end.
    - [x] Borrowed `Ui<'_>` access unifies ordinary and event-time window, popup, menu-item, and service mutation without a standalone session object.
    - [x] `FileDialog` is an application-owned retained component with reusable controls, independent instances, and subscriber-driven completion instead of frame polling.
- [x] Unified all retained surfaces in one concrete ownership forest.
    - [x] `SurfaceForest` owns windows, structural child windows, dialogs, application popups, and menu popups exactly once; sole parent edges encode ownership and one deepest-popup key derives the visible transient branch.
    - [x] Forest storage is the chronological window order. One reusable visible traversal applies fixed layers, the modal band, child families, and the active popup path consistently to layout, input, paint, and diagnostics.
    - [x] Authenticated `WindowHandle` and `PopupHandle` capabilities use non-reused process identities; checked mutations reject stale, destroyed, or foreign handles through `SurfaceMutationError`.
    - [x] `WindowEvent` reports geometry and close requests, while `PopupEvent::Dismissed` remains a popup-specific lifecycle stream.
- [x] Added structural child-window composition without a general overlay graph.
    - [x] `Ui::create_child_window` accepts independent or child parents, keeps geometry in screen coordinates, inherits the family fixed layer, and preserves sibling-local raising.
    - [x] `Window::child_window_clip(ChildWindowClip::Content)` clips complete child surfaces to the parent application body, with nested descendants accumulating every ancestor boundary.
    - [x] Parent content paints below child families while the parent menu and chrome paint and receive input above them. Visibility, destruction, dialogs, and popups follow the complete owned family.
- [x] Replaced coordinated menu machinery with declarative, manager-owned menu surfaces.
    - [x] `Window::menu_bar` consumes `MenuBar`, `Menu`, item, separator, and recursive submenu declarations directly into compact `MenuSurface` and `MenuSlot` values.
    - [x] Stable `MenuItemHandle` capabilities expose typed submission and live enabled, check, radio, label, and shortcut presentation through checked `Ui` access.
    - [x] Parent edges and trigger slots anchor headings and submenus, reuse layout/path storage after warm-up, and require no menu widget runtimes, erased payloads, temporary compiled tree, or downcasting.
- [x] Added one backend-neutral Windows-style keyboard contract.
    - [x] `KeyEvent` carries logical key identity, pressed/released state, modifiers, and repeat information; composed UTF-8 remains a separate ordered text event.
    - [x] Persistent widget focus is one stable runtime node identity independent from pointer capture. `Tab` and `Shift+Tab` wrap through eligible `TAB_STOP` surfaces, while built-in controls share activation, adjustment, hierarchy, and popup actions.
    - [x] Application-popup Escape dismissal consumes only an initial non-repeated press; repeats and releases remain ordinary input for the subsequently active focused surface.
    - [x] F10 or an unchorded Alt tap enters intrinsic menu navigation with Windows-style heading, popup, submenu, activation, cancellation, and disabled-row skipping behavior.
    - [x] Ctrl+F6 and Ctrl+Shift+F6 cycle visible independent and child windows while preserving each runtime's focused widget and respecting popup, menu, and modal scope.
    - [x] `Style::focus_color` identifies the selected control/menu and its shared outline; `Style::window_focus_color` identifies the active title and framed window.
- [x] Completed reusable retained scrolling and style composition.
    - [x] Public `Scrollbar` exposes a typed range, value, and event contract; `ScrollArea` is a one-child viewport with retained bars and `scroll_to_end` support.
    - [x] `TextArea` composes editable content with `ScrollArea`, including nested wheel delegation and stable viewport layout.
    - [x] Cascading per-widget style overrides coexist with Context-owned theme defaults.
- [x] Unified rendering and expanded atlas support.
    - [x] `Painter` records backend-neutral work into the framework-owned display list, and `RendererBackend::Frame<'a>` gives each backend one exclusive submission frame.
    - [x] Typed custom-render callbacks receive the selected backend frame directly; Glow, Vulkan, and WGPU examples share the same retained application lifecycle and logical input adapter.
    - [x] Atlas configuration supports named font variants and semantic font roles across runtime construction, generated Rust embedding, and external PNG metadata.
- [x] Updated `demo-full`, crate/API guides, architecture, layout, menu, event, rendering, example, and downstream API coverage for the unified 0.8 contract.

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
