# Skin architecture

Skinning has one resolved runtime model. Built-in widgets, manager-owned window chrome, JSON
themes, and programmatic edits all use the same context-owned concrete `Skin` value. There is no
property bag, `Any` payload, string-keyed runtime lookup, local override cascade, or parallel
background and foreground model.

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
| `Skin` | One resolved value containing `metrics`, `visuals`, `effects`, and `chrome`. |
| `SkinMetrics` | Layout, spacing, window inset, border, title, scrollbar, and thumb geometry. |
| `Skin::visual` | Resolves one complete `Visual { patch, foreground }` for a role and state. |
| `AppearanceRole` | Closed semantic UI-part domain such as `Button`, `TextInput`, or `WindowFrame`. |
| `VisualState` | Closed interaction domain: normal, hover, press, focus combinations, and disabled. |
| `StateTable<T>` | Exhaustive state-indexed generic storage with one concrete `T`. |
| `SkinEffects` | Focus outline and active-window accents that are not role/state backgrounds. |
| `WindowChromeSkin` | Data recipe for title alignment, caption placement, sizing, and backdrop. |
| `FontRef` / `IconRef` | Stable semantic or named references resolved against the active bundle. |
| `SkinBundle` | Validated atomic ownership of one skin and its exact atlas. |

The generic tables provide code sharing without erasing types. Both enum domains expose `ALL` and
`COUNT`; lookup is total, and a new role or state must participate in the complete table contract.
Paint resolves one `Visual`, keeping a state's artwork and foreground adjacent.

## Programmatic construction

Start from a skin whose atlas ownership is known, mutate its concrete fields, and install the
completed value:

```rust,ignore
use microui_redux::{color, AppearanceRole, Visual, VisualState};

let mut skin = context.skin().clone();
skin.metrics.padding = 8;
let focused_button = skin.visual(AppearanceRole::Button, VisualState::Focused);
skin.set_visual(
    AppearanceRole::Button,
    VisualState::Focused,
    Visual::new(focused_button.patch, color(255, 255, 255, 255)),
);
context.set_skin(skin);
```

The runtime retains only complete `Skin` values. `FlatPalette` is construction input for flat skins,
and `Skin::apply_flat_palette` deliberately regenerates the complete flat visual catalog and related
effects. It does not reinterpret or partially recolor an image-backed theme. JSON themes likewise
compile their flat fallbacks first and then replace explicitly authored states with PNG visuals.

## Stable resources

Raw `FontId` and `IconId` values belong to one atlas allocation. Retained widget parameters instead
store `FontRef` and `IconRef`:

- a semantic reference contains a closed `FontRole` or `IconRole`;
- a named reference contains a checked application resource name created through
  `Context::resource_catalog()`.

Measurement and paint resolve those references against the current bundle. Named application fonts
and icons are copied into every rebuilt JSON-theme atlas, while a theme may replace the five
semantic font roles. Custom renderer code that caches atlas rectangles or UVs must still refresh
those allocation-bound values after a bundle switch.

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

1. Decode each file into a strict syntax document and resolve `extends` paths and asset paths
   relative to the file that declares them.
2. Merge parents into one typed definition, converting appearance names to `AppearanceRole` and
   rejecting unknown keys, cycles, excessive depth, and invalid schema data before atlas work.
3. Build one immutable atlas, compile flat fallbacks and authored state images into a complete
   `Skin`, then construct one validated `SkinBundle`.

The runtime never retains the source documents, palette, inheritance graph, or string appearance
keys. Those are compiler inputs only.
