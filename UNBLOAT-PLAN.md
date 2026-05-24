# Unbloat Plan

Ordered by API impact, architectural payoff, and expected reduction in repetitive code.

- [x] **P0.1: Narrow the root public API surface**
  - Problem: The crate root re-exports broad low-level surfaces such as `ContainerHandle`, `Graphics`, `WindowHandle`, `rs_math3d`, backend-adjacent types, input bitflags, and direct drawing helpers. This makes compatibility and backend escape hatches look like the intended public model, even though retained `WidgetTree` authoring is the stated direction.
  - Solution: Keep the root exports focused on `Context`, retained tree building, widget state types, style/image/input event types, and renderer integration. Move direct container mutation, command inspection, and backend-only helpers under explicit `advanced` or `backend` modules. Update examples so normal usage does not depend on escape-hatch exports.
  - Status: Root exports no longer expose `ContainerHandle`, `Graphics`, `WindowHandle`, or `rs_math3d` directly. Low-level renderer types live under `backend`, and compatibility/inspection handles live under `advanced`.

- [x] **P0.2: Replace `WidgetHandle<T> = Rc<RefCell<T>>` with a real handle**
  - Problem: `WidgetHandle` leaks reference counting, runtime borrowing, pointer identity, and clone-heavy ownership patterns into every caller. It also makes examples noisy and leaves duplicate dispatch as a runtime failure.
  - Solution: Introduce a `WidgetHandle<T>` newtype that owns the `Rc<RefCell<T>>` privately. Provide focused methods such as `read`, `update`, `replace`, and stable identity access. Keep raw borrowing as an explicit advanced escape hatch only if it remains necessary.
  - Status: `WidgetHandle<T>` is now an opaque newtype with `read`, `update`, `replace`, and `id`. Examples and internal callers no longer use raw handle borrowing.

- [x] **P0.3: Make retained `NodeId` the primary interaction/result identity**
  - Problem: The runtime keeps both pointer `WidgetId` and retained `NodeId` paths alive through `InteractionId`, dual frame-result maps, and dual dispatch recording. This compatibility layer makes focus/result logic larger than the retained model requires.
  - Solution: Make `NodeId` the normal public result and focus identity for retained UI. Keep pointer-derived `WidgetId` only for immediate/manual compatibility APIs, ideally behind an advanced module or compatibility feature. Collapse duplicate result recording once callers have migrated.
  - Status: Frame results are retained-ID/node-ID based. Pointer identity remains only as an internal duplicate-dispatch guard.

- [x] **P0.4: Clean up `WindowHandle` mutability and clone semantics**
  - Problem: `WindowHandle` is already interior-mutable, but context APIs still require `&mut WindowHandle`. Examples then pass `&mut window.clone()`, which is misleading and spreads boilerplate through application code.
  - Solution: If handles remain `Rc<RefCell<_>>`, make `Context::window`, `dialog`, `popup`, `open_dialog`, and related helpers accept `&WindowHandle`. Remove the need for `&mut handle.clone()` from examples. Alternatively, remove interior mutability and use true mutable ownership, but that is a larger migration.
  - Status: Window and scroll-area handle mutation methods now take `&self` where the handle is already interior-mutable, removing `&mut handle.clone()` usage.

- [x] **P1.1: Shrink `ContainerHandle` and its view wrappers**
  - Problem: `ContainerView` and `ContainerViewMut` are mostly one-line pass-through methods into `Container`. The mutable view repeats rect, scroll, focus, clip, drawing, text, and frame APIs that already exist on other drawing layers.
  - Solution: Keep a small inspection/mutation surface for real embedded-panel needs, such as rect, scroll, content size, and maybe focus. Move direct drawing/clip methods to an advanced backend-facing API or remove them if retained widgets cover the use cases.
  - Status: The retained handle is now `ScrollAreaHandle`; the public views expose only rect/body/scroll/content-size and focus mutation. Old `Container*` names are deprecated compatibility aliases under `advanced`.

- [x] **P1.2: Consolidate the drawing facades**
  - Problem: `Container`, `ContainerHandle`, `WidgetCtx`, `DrawCtx`, and `Graphics` all expose overlapping operations for rectangles, boxes, text, icons, images, frames, control text, and clips. Some paths operate in screen/container coordinates while others operate in widget-local coordinates.
  - Solution: Introduce one internal `Painter` or `CommandEmitter` that owns clip state, command emission, triangle batching, and coordinate conversion. Keep `Graphics` as the public widget-local facade over that emitter, and route container chrome plus widget drawing through the same implementation.
  - Status: The internal recorder is now `CommandEmitter` (`DrawCtx` is only a crate-internal alias), with command emission, clip stack handling, and triangle buffering centralized there. `Graphics` remains the public widget-local facade, while `WidgetCtx` no longer exposes public direct draw/clip helpers outside `graphics` / `begin_graphics`. Widget geometry APIs now name coordinate space explicitly: `local_rect` / `rect` for widget-local bounds and `screen_rect` for absolute bounds.

- [x] **P1.3: Split `Container` into pass-specific runtime objects**
  - Problem: `Container` owns layout state, draw commands, input routing, retained cache, panel state, scroll state, style, atlas, z-order, scrollbar widgets, and a `measurement_mode` flag. Measurement creates a partial scratch clone of live state, which grows fragile as fields are added.
  - Solution: Extract smaller internal objects such as `LayoutPass`, `RenderPass`, `InteractionRouter`, `DrawList`, `PanelHost`, and `RetainedCache`. Replace `measurement_scratch` and `measurement_mode` with a dedicated measurement context that only contains the fields needed for measurement.
  - Status: Runtime state is split into viewport, draw, layout, interaction, retained cache, scroll-area, and tree traversal modules. Measurement no longer uses live-container scratch cloning or a `measurement_mode` flag; it runs through a dedicated `MeasurementContext` that only carries layout, style/atlas, scroll/body geometry, and retained cache data needed for sizing.

- [x] **P1.4: Remove duplicate erased-widget dispatch APIs**
  - Problem: `WidgetStateHandleDyn` repeats most of the `Widget` trait surface so retained tree nodes can call through boxed handles. `Container` then has parallel `measure_widget_rect_dyn_with_policy`, `measure_widget_rect_handle_with_policy`, `render_widget_dyn`, and `render_widget_handle` paths.
  - Solution: Make the retained node store a single dispatch abstraction, or make the handle newtype provide a common object-safe dispatch implementation. Keep one measure path and one render path, with typed convenience wrappers only at construction boundaries.
  - Status: The typed retained dispatch paths were removed. Tree traversal now routes retained widgets through the erased dispatch path, with typed handles only at builder construction boundaries.

- [x] **P1.5: Separate retained tree description from live widget state**
  - Problem: `WidgetTreeNodeKind` stores live widget handles, container handles, and render callbacks directly. That makes the tree both a declarative structure and a runtime state registry, which drives handle cloning and makes validation harder.
  - Solution: Move toward a two-layer model: tree nodes describe structure and stable IDs, while widget state lives in a registry keyed by typed handles or node IDs. Builders can still feel ergonomic, but the stored tree should not need to own every live handle directly.
  - Status: `WidgetTreeNodeKind` now stores resource IDs for widgets, scroll areas, custom render callbacks, headers, and tree nodes. Live handles/callbacks live in a `WidgetTreeResources` registry owned by `WidgetTree`, and traversal resolves resources during layout/update/paint.

- [x] **P2.1: Reduce builder method pair explosion**
  - Problem: Most builder APIs come in default and `_with` variants: `widget/widget_with`, `container/container_with`, `header/header_with`, `row/row_with`, `grid/grid_with`, `column/column_with`, and `stack/stack_with`. Many are thin wrappers that only supply `NodeOptions::new()`.
  - Solution: Keep the simple methods that materially improve readability, but consider a fluent options pattern or fewer generic entry points for uncommon variants. For example, keep `row` and `widget`, but move keyed/policy-heavy calls through a compact `node(options).row(...)` or similar builder adapter.
  - Status: Public `_with` option-pair methods were collapsed behind `node(options).<kind>(...)`; deprecated `container` builder aliases were removed.

- [x] **P2.2: Encapsulate widget state invariants**
  - Problem: Built-in widgets expose many fields directly, including transient cursor positions, scroll offsets, combo state, slider edit state, and numeric edit buffers. The runtime clamps during `run`, but invalid state can exist between frames and affect measurement or results.
  - Solution: Keep plain user data fields editable where useful, but add setters for invariant-bearing state. Text widgets should expose `set_text`, `text`, `set_cursor`, and `move_cursor_to_end`; numeric widgets should clamp in `set_value`; complex transient state should become private.
  - Status: Textbox/text-area text, cursor, and scroll state now go through methods. Slider/number values go through accessors/setters, numeric edit buffers are private, and combo popup/open/selection state is method-based.

- [x] **P2.3: Deduplicate common widget boilerplate**
  - Problem: Most widget structs repeat `font`, `opt`, `scroll_behavior`, default constructors, `with_opt` constructors, `preferred_size_widget`, and `handle_widget` patterns. The macro reduces trait impl repetition but does not reduce the repeated field shape.
  - Solution: Introduce a small shared `WidgetConfig` or `CommonWidgetState` for `font`, `opt`, and `scroll_behavior`, or use focused constructors/builders for common configuration. Keep individual widget structs focused on state unique to that widget.
  - Status: Built-in widgets now share `WidgetConfig` for font, widget options, and scroll behavior. The widget macro and examples/docs use `config.font`, `config.opt`, and `config.scroll_behavior` instead of repeating those fields on every widget type.

- [x] **P2.4: Prune tiny option and flag helper methods**
  - Problem: `ContainerOption`, `WidgetOption`, `WidgetFillOption`, `MouseButton`, `KeyMode`, `KeyCode`, and `ResourceState` define many one-line `is_*`, `has_*`, and `fill_*` wrappers around `intersects` or `bits() == 0`.
  - Solution: Keep helpers that encode domain language used widely or hide non-obvious semantics. Remove or make private helpers that only rename `intersects`, especially where direct bitflag calls are equally clear.
  - Status: One-line flag wrappers on `ContainerOption`, `WidgetOption`, `WidgetFillOption`, `MouseButton`, `KeyMode`, and `KeyCode` were removed. Internal call sites now use explicit `intersects(...)` or `is_empty()`. `ResourceState` keeps semantic helpers such as `is_submitted()` and `is_changed()`.

- [x] **P2.5: Simplify `Node` constructor aliases**
  - Problem: `Node::new`, `header`, `tree`, `with_opt`, `with_opt_header`, and `with_opt_tree` are small variants over the same fields. They add API surface without adding much behavior.
  - Solution: Keep one clear default constructor per semantic kind, likely `Node::header` and `Node::tree`. Replace `new` and generic `with_opt` aliases with explicit configuration setters or a compact builder-style method like `.with_options(opt)`.
  - Status: `Node::new`, `Node::with_opt`, `Node::with_opt_header`, and `Node::with_opt_tree` were removed. `Node::header`, `Node::tree`, and `.with_options(...)` remain.

- [x] **P2.6: Remove retained-cache interaction stubs if they are not used**
  - Problem: `NodeInteraction`, `prev_interaction`, and `current_interaction` are retained but marked dead-code-tolerant. They add maps and generation logic without being part of the public retained result API.
  - Solution: Either expose a real retained debugging/inspection API that uses these values, or remove the interaction generation from `WidgetTreeCache` and rely on `FrameResults` for interaction state.
  - Status: `NodeInteraction` and the unused interaction frame-cache maps were removed; retained cache now records layout and current control state only.

- [x] **P2.7: Tame example clone walls**
  - Problem: `examples/demo-full.rs` and smaller examples spend many lines cloning handles before closures and then cloning them again inside tree builders. This is mostly an API smell from handle and tree ownership, but it also makes examples much harder to read.
  - Solution: After handle/tree API cleanup, refactor examples to use direct references, small helper builders, or registry-backed node insertion. Normal examples should demonstrate the compact intended API, not every workaround needed by the current storage model.
  - Status: Builder insertion accepts `&WidgetHandle<T>`/`&ScrollAreaHandle`, examples and README snippets no longer call `tree.widget(handle.clone())`, `tree.header(handle.clone())`, or equivalent clone-at-insertion patterns. `FileDialogState` and the calculator example also build trees from borrowed handles instead of pre-cloned handle vectors.

- [x] **P3.1: Audit small pass-through accessors after larger cleanup**
  - Problem: Several one-line accessors and forwarding helpers exist only because the current layering is broad. Removing them too early may create churn, but leaving them afterward will preserve unnecessary API mass.
  - Solution: After P0-P2 work, run a focused dead-code and public-API audit. Remove pass-through methods that no longer protect invariants, no longer simplify call sites, or only expose internals under another name.
  - Status: The post-refactor audit moved current-frame result access, previous/current retained layout inspection, scrollbar test helpers, and body mutation helpers behind `cfg(test)` where they are only used by tests. An unused raw mutable scroll-area borrow was removed. The remaining `rect_packer` dead-code allowance is isolated to the vendored-style packing implementation, not UI pass-through API.
