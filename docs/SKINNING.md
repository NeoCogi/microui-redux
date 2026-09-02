# Skin architecture

Skinning has one resolved runtime model. Built-in widgets, manager-owned window chrome, JSON
themes, programmatic edits, and retained overrides all converge on the same concrete `Skin` value.
There is no property bag, `Any` payload, string-keyed runtime lookup, or parallel background and
foreground model.

## Runtime ownership

```text
ResourceCatalog ── builds/loads ──> Skin + exact AtlasHandle
                                         │
                                         v
                                    SkinBundle
                                         │ atomic install
                                         v
                                      Context
                                         │ complete value inheritance
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
| `VisualCatalog` | One complete `Visual { patch, foreground }` for every role and state. |
| `AppearanceRole` | Closed semantic UI-part domain such as `Button`, `TextInput`, or `WindowFrame`. |
| `VisualState` | Closed interaction domain: normal, hover, press, focus combinations, and disabled. |
| `RoleTable<T>` / `StateTable<T>` | Exhaustive enum-indexed generic storage with one concrete `T`. |
| `SkinEffects` | Focus outline and active-window accents that are not role/state backgrounds. |
| `WindowChromeSkin` | Data recipe for title alignment, caption placement, sizing, and backdrop. |
| `FontRef` / `IconRef` | Stable semantic or named references resolved against the active bundle. |
| `SkinPatch` | Sparse typed authoring edit with deterministic later-present-value precedence. |
| `SkinBundle` | Validated atomic ownership of one skin and its exact atlas. |

The generic tables provide code sharing without erasing types. Both enum domains expose `ALL` and
`COUNT`; lookup is total, and a new role or state must participate in the complete table contract.
Paint resolves one `Visual`, keeping a state's artwork and foreground adjacent.

## Programmatic construction and layering

Start from a skin whose atlas ownership is known, then mutate grouped concrete fields or apply a
typed patch:

```rust,ignore
use microui_redux::{
    color, AppearanceRole, SkinPatch, VisualPatch, VisualState,
};

let mut patch = SkinPatch::default();
patch.metrics.padding = Some(8);
patch.visuals.set(
    AppearanceRole::Button,
    VisualState::Focused,
    VisualPatch::foreground(color(255, 255, 255, 255)),
);

let mut skin = context.skin().clone();
skin.apply_patch(&patch);
context.set_skin(skin);
```

Every patch leaf is an `Option<ConcreteType>`. `None` preserves the destination; `Some(value)`
replaces it. `SkinPatch::merge_later` uses that rule at every leaf, so merging a layer stack and
applying layers in order have identical results. Window chrome is replaced as a complete recipe
because its alignment and caption-bank geometry are interdependent.

`FlatPalette` is compiler/editor input, not retained shadow state. `Skin::apply_flat_palette`
immediately regenerates the resolved visual catalog and related effects when constructing a flat
skin. A live editor working over an image theme instead uses
`SkinPatch::from_flat_palette_transition`: it changes only affected flat cells, foregrounds, and
effects while retaining image-backed cells. Merge those event patches over a pristine selected base
skin so later edits compose without turning theme artwork back into flat fallbacks. JSON palette
fields use full flat construction first, before authored PNG states replace individual visuals.

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

## Retained overrides and caches

A node override is a complete `Skin`, not a partial mutation bag. `Node::with_skin_override`,
`Node::set_skin_override`, and the matching `TypedWidgetHandle::try_set_skin_override` path replace
the inherited value for that node. A container passes the value to descendants until another node
provides its own complete replacement. Clearing the override restores inheritance.

Each validated bundle receives one private, non-reused skin revision. Retained measurement caches
key their results by constraints and that complete revision. Installing a bundle or changing a
local override therefore cannot accidentally reuse measurements from a skin that happens to share
only a few scalar fields.

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
