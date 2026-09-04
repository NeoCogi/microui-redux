# Skin architecture

Skinning has one resolved runtime model. Built-in widgets, manager-owned window chrome, JSON
themes, and programmatic edits all use the same context-owned concrete `Skin` value. There is no
property bag, `Any` payload, string-keyed runtime lookup, local override cascade, or parallel
patch and semantic-content-color model.

## Runtime ownership

```text
ResourceCatalog ── builds/loads ──> Skin + exact AtlasHandle
                                         │
                                         v
                                    SkinBundle
                                         │ atomic install
                                         v
                                      Context
                                         │ one borrowed value
                                         v
                            WindowManager ──> retained Node tree
```

`Context::new` captures the backend's initial atlas in a stable `ResourceCatalog` and creates a
default `SkinBundle`. A bundle owns one complete `Skin` and the exact immutable `AtlasHandle` that
minted every image capability in its visual catalog. `Context::set_skin_bundle` uploads the atlas
first and publishes the pair only after the backend accepts it. A failed upload leaves the old pair
active.

`Context::set_skin` is the scalar/programmatic edit path. It pairs the replacement skin with the
currently active atlas and rejects image-backed visuals from another allocation. JSON loading
always derives from `Context::resource_catalog()`, never from the previously selected theme, so
switching themes cannot gradually accumulate fonts, icons, or artwork.

## Concrete types

| Type | Responsibility |
| --- | --- |
| `Skin` | One resolved value containing metrics, complete visuals, and chrome policy. |
| `SkinMetrics` | Layout, spacing, window inset, border, title, scrollbar, and thumb geometry. |
| `Skin::surface` / `control` / `menu` / `chrome` | Resolves one complete `Visual { patch, content_color }` in a concrete family. |
| `SurfaceRole` / `ControlRole` | Concrete structural and interactive widget-facing roles. |
| `MenuRole` / `ChromeRole` | Concrete menu and manager-owned window-decoration roles. |
| `SurfaceState` | Structural availability: normal or disabled. |
| `ControlState` / `PointerState` | Disabled, or enabled/focused with a nested normal, hovered, or pressed pointer state. |
| `MenuState` | Menu-local normal, hovered, pressed, focused, open, and disabled states. |
| `ChromeState` | Window-local base, active, and disabled states. |
| `WindowChromeSkin` | Data recipe for title alignment, caption placement, sizing, and backdrop. |
| `FontRef` / `IconRef` | Stable semantic or named references resolved against the active bundle. |
| `SkinBundle` | Validated atomic ownership of one skin and its exact atlas. |

Each role and state enum exposes `ALL` and `COUNT`. The private catalog stores four concrete arrays,
so lookup is total inside each family and code cannot pass menu-open state to a widget or pointer
state to a window frame. Paint resolves one `Visual`, keeping a state's artwork and content color
adjacent without erased values or a universal role/state cross-product.

## Programmatic construction

Start from a skin whose atlas ownership is known, mutate its concrete fields, and install the
completed value:

```rust
use microui_redux::{Context, color, ControlRole, ControlState, PointerState, Visual};
use microui_redux::render::RendererBackend;

# fn customize<B: RendererBackend, State: 'static>(context: &mut Context<B, State>) {
let mut skin = context.skin().clone();
skin.metrics.padding = 8;
let state = ControlState::Focused(PointerState::Normal);
let focused_button = skin.control(ControlRole::Button, state);
skin.set_control(
    ControlRole::Button,
    state,
    Visual::new(focused_button.patch, color(255, 255, 255, 255)),
);
context.set_skin(skin);
# }
```

The runtime retains only complete `Skin` values. `FlatPalette` is construction input for flat skins,
and `Skin::apply_flat_palette` deliberately regenerates the complete flat visual catalog. It does
not reinterpret or partially recolor an image-backed theme. JSON themes likewise
compile their flat fallbacks first and then replace explicitly authored states with PNG visuals.

## Stable resources

Raw `FontId` and `IconId` values belong to one atlas allocation. Retained widget parameters instead
store `FontRef` and `IconRef`:

- a semantic reference contains a closed `FontRole` or `IconRole`;
- a named reference contains a checked application resource name created through
  `Context::resource_catalog()`.

Measurement and paint resolve those references against the current bundle. Named application fonts
and icons are copied into every rebuilt JSON-theme atlas, while a theme may replace the complete
five-role semantic font catalog and/or complete eight-role semantic icon catalog. Replacement
preserves each resource's stable ID, so already-retained exact references remain valid in the
derived atlas. Custom renderer code that caches atlas rectangles or UVs must still refresh those
allocation-bound values after a bundle switch.

## Global skin and caches

`Context` owns the sole active skin. Window chrome, every retained node, and every runtime phase
borrow that same complete value; containers and widgets neither store nor resolve local skins.
Replacing it through `Context::set_skin` or `Context::set_skin_bundle` therefore has one explicit
scope: the entire UI managed by that context.

Each validated skin receives one private, non-reused revision. Retained measurement caches key
their results by constraints and that complete revision, so a global replacement cannot reuse
measurements produced with an earlier skin whose scalar fields happened to look similar.

## JSON compilation

The optional `theme-json` feature adds a strict authoring format documented in
[JSON themes](THEMES.md). Loading has three boundaries:

1. Decode one strict, self-contained document and resolve its asset paths relative to that file.
2. Deserialize family-local appearance keys directly into concrete role enums and reject unknown
   keys or invalid schema data before atlas work.
3. Build one immutable atlas, compile flat fallbacks and authored state images into a complete
   `Skin`, then construct one validated `SkinBundle`.

The runtime never retains the source document, palette, or string appearance keys. Those are
compiler inputs only.
