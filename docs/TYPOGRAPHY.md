# Fonts and typography

- Atlas building supports multiple baked fonts and sizes through `atlas::builder::FontAsset`, and the same config can drive both runtime atlas construction and offline/prebuilt atlas export.
- `Context::new(...)` constructs its default `SkinBundle` from the conventional atlas font keys
  `body`, `small`, `title`, `heading`, and `mono`, plus the built-in semantic icon keys. To customize
  it, clone `context.skin()`, edit its concrete fields, and pass the complete value to
  `Context::set_skin(...)`.
- Text-bearing widget parameters expose `.font(FontRef)`. Select a semantic role with
  `FontRole::Heading.into()`, or create a checked allocation-independent named reference with
  `context.resource_catalog().font_ref("caption")`.
- Font sizes are selected by choosing another baked font variant, not by scaling one bitmap font at runtime.
- `examples/demo-full` uses this directly: `NORMAL.ttf` for control/body text, `BOLD.ttf` for window titles, and `CONSOLE.ttf` for the log window’s input/output text.

## Text encoding and glyph coverage

All public text enters the library as Rust `str` or `String` values and is therefore valid UTF-8.
Textbox removes CR and LF so its stored value always remains single-line. TextArea and TextBlock
canonicalize CRLF and lone CR to LF when text is constructed, replaced, or pasted. Textbox and text
area keep their byte cursor on Unicode scalar-value boundaries. Left/right movement and deletion
operate on one scalar value at a time, not on a user-perceived grapheme cluster. Combining
sequences and multi-scalar emoji can therefore require more than one cursor or deletion operation.

File-dialog paths also cross the public API as UTF-8 `String` values. On platforms that permit
non-UTF-8 paths, the default current directory and enumerated directory entries are converted
lossily. Accepting a typed name is lexical: the resulting path is not required to exist or identify
a regular file.

Rendering coverage is a separate atlas concern. Text measurement and drawing iterate Unicode
scalar values and use the same lookup rules:

- a character present in the selected atlas font uses its own glyph metrics and rectangle;
- a missing character uses the selected font's mandatory underscore (`_`) entry.

The built-in atlas builder bakes only printable ASCII (`U+0020` through `U+007E`), which includes
underscore. `AtlasSource` can describe arbitrary Unicode scalar values, so applications needing
broader coverage must provide their own glyph table and must include `_` in every font. Atlas
construction rejects a font without that explicit fallback. There is not yet a configurable
glyph-range option in `builder::Config`.

The text pipeline does not perform grapheme segmentation, script shaping, bidirectional
reordering, kerning, or fallback-font selection. `TextWrap::Word` wraps only at ASCII space
boundaries; an individual word is not split when it exceeds the available width.

An atlas used with `Context` must have a `body` font and the standard semantic icon names
`close`, `expand`, `collapse`, `check`, `expand_down`, `open_folder`, `closed_folder`, and `file`.
Every atlas, including a fontless atlas prepared before Context construction, has a validated opaque
white tile named `white` for solid geometry; no resource depends on a numeric table position.
`AtlasHandle::try_from` validates dimensions, decoded pixels, unique names and glyphs, font
metrics, the mandatory underscore entries, every glyph/icon rectangle, and the opaque white tile
before returning a handle. `Skin::from_atlas` and `SkinBundle::new` enforce the standard UI's
semantic font/icon naming policy and validate that image visuals belong to the paired atlas.

`FontId` and `IconId` are opaque capabilities scoped to one runtime atlas allocation. Cloning an
`AtlasHandle` preserves their owner, while loading identical source metadata again creates a
different owner. A renderer rejects foreign font and icon IDs during display-list preflight.
Retained widget state stores `FontRef` and `IconRef` instead; they resolve through the active bundle
when measurement or paint needs the short-lived capability.

```rust
use microui_redux::{atlas::builder, prelude::*};

let icons = vec![
    builder::IconAsset { name: "close".into(), path: "assets/CLOSE.png".into() },
    builder::IconAsset { name: "expand".into(), path: "assets/PLUS.png".into() },
    builder::IconAsset { name: "collapse".into(), path: "assets/MINUS.png".into() },
    builder::IconAsset { name: "check".into(), path: "assets/CHECK.png".into() },
    builder::IconAsset { name: "expand_down".into(), path: "assets/EXPAND_DOWN.png".into() },
    builder::IconAsset { name: "open_folder".into(), path: "assets/OPEN_FOLDER_16.png".into() },
    builder::IconAsset { name: "closed_folder".into(), path: "assets/CLOSED_FOLDER_16.png".into() },
    builder::IconAsset { name: "file".into(), path: "assets/FILE_16.png".into() },
];

let fonts = vec![
    builder::FontAsset {
        name: "body".into(),
        path: "assets/NORMAL.ttf".into(),
        size: 12,
    },
    builder::FontAsset {
        name: "small".into(),
        path: "assets/NORMAL.ttf".into(),
        size: 10,
    },
    builder::FontAsset {
        name: "title".into(),
        path: "assets/BOLD.ttf".into(),
        size: 12,
    },
    builder::FontAsset {
        name: "heading".into(),
        path: "assets/NORMAL.ttf".into(),
        size: 18,
    },
    builder::FontAsset {
        name: "mono".into(),
        path: "assets/CONSOLE.ttf".into(),
        size: 14,
    },
    builder::FontAsset {
        name: "calculator-display".into(),
        path: "assets/CONSOLE.ttf".into(),
        size: 28,
    },
];

let config = builder::Config {
    texture_width: 512,
    texture_height: 256,
    white_icon: "assets/WHITE.png".into(),
    icons,
    fonts,
};

let (_title, title_node) = TextBlock::create(
    TextBlockParameters::new("Inspector").font(FontRole::Heading.into()),
);
```

`builder::Config::fonts` may be empty when an application needs to prepare atlas data separately.
A `Context` atlas must name one entry `body`; optional roles that are absent fall back to that same
atlas-owned body font.
