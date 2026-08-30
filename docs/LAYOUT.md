# Retained layout

## Preferred sizing, tracks, and retained layout

Layout traverses the retained node tree in three stages:

```text
constraints flow down -> desired sizes flow up -> exact rectangles flow down
```

A parent is the immediate container holding a child. During measurement, the parent sends each
child `Constraints`: a width and height that are independently bounded or unbounded. These are
questions about available space, not assigned sizes. Each child answers with its desired size, and
containers combine those answers into desired sizes reported toward the root. Once the root has an
actual rectangle, every parent walks downward again and assigns each child an exact
`Recti { x, y, width, height }`.

A **track** is the one-dimensional unit a parent sizes while turning those measurements into
rectangles. It starts as a rule—`Content`, `Fixed`, or `Flex`—and resolves to one non-negative pixel
extent:

```text
horizontal Linear inside 600 px

| Content: 80 | gap: 10 | Flex(1): 400 | gap: 10 | Fixed: 100 |
|   child 0   |         |    child 1   |         |   child 2  |
```

For a horizontal `Linear`, one track supplies each child's width; for a vertical `Linear`, one
track supplies each child's height. The container combines that main-axis extent with its
cross-axis rule to produce the child's rectangle.

“Slot” is close to “track” for Linear, but it would obscure why Linear and Grid use the same sizing
vocabulary and scalar resolver. They remain separate container algorithms: each measures its own
children and places its own rectangles, and Grid does not construct, contain, or delegate to a
Linear widget. Grid independently passes its row and column track data to the common resolver in
`layout.rs`. Several children can use the same row or column track, and one child can span several
tracks. A child's slot is the resulting two-dimensional area; a track is only one row, column,
width, or height used to form that area. Tracks are parent-owned because the container stores and
interprets these rules—the child reports desired content without knowing whether its parent will
place it in a content, fixed, or flexible track.

- Every built-in leaf reports its own intrinsic preferred size from content metrics (text/icon/thumb/line layout), while every container measures against its authoritative child collection.
- A consumed or captured event conservatively dirties its recipient's retained measurement; the runtime propagates that invalidation through dependent ancestors at the next layout boundary. Typed-handle mutations use the same propagation path, so widget implementations do not manage layout caches.
- `LeafWidget::measure` and `ContainerWidget::measure` report desired content, not an allocation. `Constraints` represents each axis as `AvailableSpace::Bounded(i32)` or `AvailableSpace::Unbounded`; bounded zero is not an unconstrained request.
- Auto-sized roots measure unbounded axes intrinsically. `TrackSize::Content` uses desired extent, `TrackSize::Fixed` stays exact, and `TrackSize::Flex` shares bounded space left after content, fixed tracks, and spacing while falling back to desired content when unbounded.
- `Context::update_ui` first synchronizes layout, then drains input in API-call order. Every event runs one complete eligible-tree `Widget::update` traversal and one follow-up layout, so geometry changed by one event is authoritative for routing the next.
- `ContextFrame::render_ui` performs no input, update, or layout work. It paints the committed tree with `Widget::paint` and submits one display list; missing, stale, pending-input, or dimension-mismatched commits return `RenderError::UiUpdateRequired` before backend acquisition.
- Leaves and containers share the public `Widget` update/paint contract. `LeafWidget` adds intrinsic measurement; `ContainerWidget` adds child-aware measurement, placement, and optional surface event filtering.
- Parent containers assign each node one exact retained parent-local allocation. Sizing relationships belong to parent-child edges such as `LinearItem`; `Node` has no global placement policy. Child offsets and clips remain node-local and are resolved through a composed transform during traversal.
- Resolved outer rectangles and clips remain runtime stack locals. Node behavior works against its local content surface, while outer frame painting, standard hit routing, and conversion from screen input remain runtime-owned.
- A public widget's Painter geometry and routed pointer positions share the derived content-local origin.
- Handle-bearing built-in leaf and container constructors return a weak `TypedWidgetHandle<W>` plus one completed owning `Node`. The stateless `Custom::create` exception returns a `Custom` runtime for mounting through `Node::widget`, `Node::custom_render`, or `Node::typed_custom_render`. Concrete container constructors consume child nodes.
- Every direction is a configuration of one `Linear` widget. Linear and Grid independently invoke the common scalar track resolver, so both apply identical content/fixed/flex, spacing, rounding, and overflow arithmetic without either container being implemented through the other.
- `LinearCrossSize` gives every direction the same shared-line choices: desired content, stretching across exact allocation, or an exact fixed cross extent. `LinearDirection` combines axis and leading edge.
- Negative desired extents are normalized to zero at the node boundary. A desired zero remains zero; generic containers do not substitute Style-owned fallback cells.

Built-in leaves and containers are mutated through their typed widget handles between commits. After programmatic state/topology changes, call `update_ui` even when no input is pending so layout is synchronized before paint. Feed raw input through methods such as `mousemove`, `mousedown`, `scroll`, `key`, and `text`; calls are queued without coalescing. `Context::key` accepts one backend-normalized `KeyEvent` containing logical identity, pressed/released state, the complete modifier snapshot, and repeat state. Printable key transitions remain distinct from `text`, which is the authoritative channel for composed UTF-8 input. A widget receives the current event as `Option<&UiInputEvent>`, while `WidgetUpdateCtx::{mouse_buttons,modifiers}` exposes held state after that event was applied.

Keyboard focus persists after a pointer release and is independent of pointer capture. Within the
active eligible window, `Tab` and `Shift+Tab` traverse enabled, visible `TAB_STOP` surfaces in
retained sibling order and wrap at the ends. Hidden, clipped, disabled, and pointer-focus-only
surfaces are skipped. A modal dialog suspends ordinary-window traversal, and an active menu scope
suspends widget key/text delivery without discarding the widget that will regain focus when the
menu closes. Custom widgets opt into focus, traversal, and the shared Windows-style action mapping
through `Widget::keyboard_behavior`; their default behavior is keyboard-inert.

Paint exposes focus only for the manager-selected keyboard surface even though every window runtime
retains its own target. `Style::focus_color` fills selected controls such as disclosure rows and
menus and records one clipped, inside-aligned outline around the focused widget after its complete
ordinary, child, and custom-render output. The outline reuses `max(frame_border_width, 1)` and does
not affect measurement or hit geometry. `Style::window_focus_color` fills the active title and
outlines an active framed window; inactive windows retain their ordinary title and border colors.

`ContextFrame` holds the Context borrow needed to serialize paint/submission, but it does not lock independent typed widget or root handles and there is no Context access token. Do not keep a typed-access closure active while retained update/layout/paint can reach that same widget. Framework recursion through a container's scoped child visitor is the intentional exception. If layout-affecting state changes after the last commit, drop any unsubmitted frame and call `update_ui` again before paint.

The application owns `Context` and its weak typed handles as independent Rust values, so the
compiler permits explicitly capturing the Context inside a handle-access closure. Do not initiate
retained traversal that way:

```rust
textbox.try_update(|widget| {
    widget.set_text("hello");
    context.update_ui(dimensions); // unsupported: the mutable widget borrow is still active
});
```

`try_update` holds the concrete widget's checked `RefCell` borrow until its closure returns. If the nested
layout, update, or paint traversal reaches that widget, the runtime's checked borrow is
incompatible and panics with a diagnostic naming the runtime phase. This is the reentrancy guard;
there is no separate Context lock. Finish typed access before committing instead:

```rust
textbox.set_text("hello").expect("textbox unavailable");
context.update_ui(dimensions);
```

Update and paint visit a node before its eligible children and visit siblings in forward order. A
successful mutation of a later, currently available state cell is visible when traversal reaches
it; mutating an already-updated sibling does not rerun that sibling. A container's active child
visitor borrow makes mutation of that same container return `None`, while another available subtree
may change. There is no transaction snapshot or rollback, but every input transaction ends with a
complete layout before the next event is routed.

`Widget::paint` is observational with respect to application-authored semantic state, topology,
interaction, and committed layout; it may update only private rendering caches. Registered
custom-render callbacks likewise update only callback-private rendering caches. Mutating retained
UI through an independently captured typed widget handle during either callback violates the
contract; it is not a deferred-next-frame update. Commit semantic changes before creating the
frame.
