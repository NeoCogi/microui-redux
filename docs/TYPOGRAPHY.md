# Fonts and typography

- Atlas building supports multiple baked fonts and sizes through `atlas::builder::FontAsset`, and the same config can drive both runtime atlas construction and offline/prebuilt atlas export.
- `Context::new(...)` constructs its `Style` directly from the conventional atlas font keys `body`, `small`, `title`, `heading`, and `mono`, plus the built-in semantic icon keys. To customize it, copy `*context.style()`, change scalar fields, and pass the complete value to `Context::set_style(...)`; no placeholder IDs or rebinding pass exists.
- Text-bearing widget Parameters expose `.font(FontChoice)`, so you can either select a semantic role (`FontRole::Heading.into()`) or a concrete baked font ID (`atlas.font_id("caption").unwrap().into()`).
- Font sizes are selected by choosing another baked font variant, not by scaling one bitmap font at runtime.
- `examples/demo-full` uses this directly: `NORMAL.ttf` for control/body text, `BOLD.ttf` for window titles, and `CONSOLE.ttf` for the log window’s input/output text.

## Text encoding and glyph coverage

All public text enters the library as Rust `str` or `String` values and is therefore valid UTF-8.
Textboxes and text areas retain arbitrary UTF-8 and keep their byte cursor on Unicode scalar-value
boundaries. Left/right movement and deletion operate on one scalar value at a time, not on a
user-perceived grapheme cluster. Combining sequences and multi-scalar emoji can therefore require
more than one cursor or deletion operation.

File-dialog paths also cross the public API as UTF-8 `String` values. On platforms that permit
non-UTF-8 paths, the default current directory and enumerated directory entries are converted
lossily. Accepting a typed name is lexical: the resulting path is not required to exist or identify
a regular file.

Rendering coverage is a separate atlas concern. Text measurement and drawing iterate Unicode
scalar values and use the same lookup rules:

- a character present in the selected atlas font uses its own glyph metrics and rectangle;
- a missing character uses the selected font's underscore (`_`) entry;
- if underscore is also absent, the runtime uses a synthetic 8-by-8 fallback rectangle at the
  atlas origin.

The built-in atlas builder bakes only printable ASCII (`U+0020` through `U+007E`), which includes
underscore. `AtlasSource` can describe arbitrary Unicode scalar values, so applications needing
broader coverage must provide their own glyph table and should always include `_`. There is not
yet a configurable glyph-range option in `builder::Config`.

The text pipeline does not perform grapheme segmentation, script shaping, bidirectional
reordering, kerning, or fallback-font selection. `TextWrap::Word` wraps only at ASCII space
boundaries; an individual word is not split when it exceeds the available width.

An application-provided atlas must have a `body` font and an opaque white tile named `white` for
solid geometry; neither resource depends on a numeric table position. The standard style also expects the semantic icon names
`close`, `expand`, `collapse`, `check`, `expand_down`, `open_folder`, `closed_folder`, and `file`.
The current loader does not validate the complete contract, so treat atlas metadata as trusted
input and keep every glyph/icon rectangle within the declared texture dimensions.

`FontId` and `IconId` are opaque capabilities scoped to one runtime atlas allocation. Cloning an
`AtlasHandle` preserves their owner, while loading identical source metadata again creates a
different owner. A renderer rejects foreign font and icon IDs during display-list preflight.

```rust
use microui_redux::{atlas::builder, prelude::*};

const ICONS: &[builder::IconAsset<'static>] = &[
    builder::IconAsset { name: "close", path: "assets/CLOSE.png" },
    builder::IconAsset { name: "expand", path: "assets/PLUS.png" },
    builder::IconAsset { name: "collapse", path: "assets/MINUS.png" },
    builder::IconAsset { name: "check", path: "assets/CHECK.png" },
    builder::IconAsset { name: "expand_down", path: "assets/EXPAND_DOWN.png" },
    builder::IconAsset { name: "open_folder", path: "assets/OPEN_FOLDER_16.png" },
    builder::IconAsset { name: "closed_folder", path: "assets/CLOSED_FOLDER_16.png" },
    builder::IconAsset { name: "file", path: "assets/FILE_16.png" },
];

const FONTS: &[builder::FontAsset<'static>] = &[
    builder::FontAsset {
        name: "body",
        path: "assets/NORMAL.ttf",
        size: 12,
    },
    builder::FontAsset {
        name: "small",
        path: "assets/NORMAL.ttf",
        size: 10,
    },
    builder::FontAsset {
        name: "title",
        path: "assets/BOLD.ttf",
        size: 12,
    },
    builder::FontAsset {
        name: "heading",
        path: "assets/NORMAL.ttf",
        size: 18,
    },
    builder::FontAsset {
        name: "mono",
        path: "assets/CONSOLE.ttf",
        size: 14,
    },
    builder::FontAsset {
        name: "calculator-display",
        path: "assets/CONSOLE.ttf",
        size: 28,
    },
];

let config = builder::Config {
    texture_width: 512,
    texture_height: 256,
    white_icon: "assets/WHITE.png".into(),
    icons: ICONS,
    fonts: FONTS,
};

let (_title, title_node) = TextBlock::create(
    TextBlockParameters::new("Inspector").font(FontRole::Heading.into()),
);
```

`builder::Config::fonts` must be non-empty. Standard Context atlases name one entry `body`; optional
roles that are absent fall back to that same atlas-owned body font.
