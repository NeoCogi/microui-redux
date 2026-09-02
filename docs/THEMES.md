# JSON themes

The `theme-json` feature is enabled by default. It adds `Context::load_theme_file`, which reads one
strict, versioned JSON definition and bakes each referenced PNG into the theme's immutable atlas.
The returned `LoadedTheme` owns a validated `SkinBundle`: one rebuilt resource atlas and its
complete matching `Skin`. Install both with `context.set_theme(&theme)`. Loading does not change
the active renderer, so several themes can be prepared before the user selects one. Selection
uploads the replacement atlas first and publishes the bundle only after that backend transaction
succeeds.

All PNG paths are relative to the JSON file. Repeated state entries that resolve to the same path
share one decoded atlas region; their source insets, destination insets, and tints remain
independent typed patch metadata. Theme nine-patches therefore join text, icons, and flat fills in
the ordinary atlas vertex batch. External textures remain reserved for application-owned images.
A failed load leaves the active renderer and its external textures untouched.

## Bundled example themes

The repository includes [`themes/windows-3.11/theme.json`](../themes/windows-3.11/theme.json),
[`themes/windows-95/theme.json`](../themes/windows-95/theme.json), and
[`themes/mac-os-9/theme.json`](../themes/mac-os-9/theme.json). All use original BSD-licensed pixel
artwork authored for this project; their directory READMEs identify the GTK, platform-guideline,
and gallery references used for visual research and explicitly document that no third-party theme
files were copied.

`demo-full` loads the Default Skin and every bundled file once at startup. Choose them from
`View > Theme`; each selection installs a pristine editable copy, so the existing Skin Editor can
modify it without changing the stored base theme. The example requires the `theme-json` feature and
the theme directories must remain available beside the repository sources at runtime.

## Minimal schema

```json
{
  "schema_version": 2,
  "name": "Example",
  "fonts": {
    "texture_width": 512,
    "texture_height": 256,
    "body": { "path": "body.ttf", "size": 12 },
    "small": { "path": "body.ttf", "size": 10 },
    "title": { "path": "title.ttf", "size": 12 },
    "heading": { "path": "body.ttf", "size": 18 },
    "mono": { "path": "mono.ttf", "size": 14 }
  },
  "skin": {
    "padding": 4,
    "window_content_insets": { "left": 0, "top": 0, "right": 0, "bottom": 0 },
    "spacing": 4,
    "title_height": 20,
    "window_chrome_layout": "trailing_buttons",
    "frame_insets": { "left": 1, "top": 1, "right": 1, "bottom": 1 },
    "colors": {
      "text": [0, 0, 0, 255],
      "border": [0, 0, 0, 255],
      "button": [192, 192, 192, 255],
      "button_hover": [208, 208, 208, 255],
      "control_focus": [0, 0, 0, 255],
      "selection_background": [0, 0, 128, 255],
      "selection_foreground": [255, 255, 255, 255],
      "window_active": [0, 0, 128, 255]
    }
  },
  "appearances": {
    "button": {
      "insets": { "left": 2, "top": 2, "right": 2, "bottom": 2 },
      "normal": {
        "png": "button-normal.png",
        "source_insets": { "left": 2, "top": 2, "right": 2, "bottom": 2 }
      },
      "hovered": { "png": "button-hovered.png" },
      "pressed": { "png": "button-pressed.png", "foreground": [255, 255, 255, 255] },
      "focused": { "png": "button-focused.png" },
      "hovered_focused": { "png": "button-hovered-focused.png" },
      "pressed_focused": { "png": "button-pressed-focused.png" },
      "disabled": { "png": "button-disabled.png", "tint": [255, 255, 255, 160] }
    }
  }
}
```

The optional `fonts` object is all-or-nothing. When present, it declares atlas texture dimensions
and exact file/size recipes for the five semantic roles: `body`, `small`, `title`, `heading`, and
`mono`. Paths are relative to the JSON file. Loading copies the application catalog's named icons into a
fresh atlas of the requested size, rasterizes these declared fonts, packs the unique state PNGs,
and binds the resulting capabilities into the theme bundle. Without a font recipe, an
artwork-bearing theme reuses the application resource catalog's atlas dimensions and repacks its
icons and baked glyphs before adding the PNG regions. A flat palette-only theme reuses that source
atlas allocation exactly. Every load starts from the immutable `ResourceCatalog` captured by
`Context::new`, not from the currently selected theme. The bundled classic themes all provide
explicit font recipes and dimensions.

Atlas-scoped IDs are intentionally concrete capabilities and are not retained by widgets.
Use `FontRef::role(FontRole::Heading)` for theme-controlled typography or create a checked named
reference with `context.resource_catalog().font_ref("caption")` for application typography copied
into every derived atlas. `IconRef` follows the same semantic/named model. Custom rendering code
that caches raw atlas UV coordinates must refresh those coordinates after installing another
bundle.

An appearance or state may be omitted. Every omitted state keeps its own flat-color fallback; it
does not borrow another state's PNG. A state may set `foreground` without a PNG to recolor its text
and semantic glyphs over that fallback. Conversely, a PNG state may omit `foreground` and retain
the fallback color. This makes partial themes predictable and lets a theme use images only where
they add value.

`insets` controls destination layout and stretching. `source_insets` divides the PNG and defaults to
the role's destination insets. Source insets must be non-negative and opposing values must fit
inside the PNG. Destination insets may be larger than a runtime control; the renderer reduces
opposing sides proportionally for tiny destinations.

## Skin fields

The optional `skin` object accepts these integer metrics:

- `default_cell_width`
- `padding`
- `window_content_insets` (`left`, `top`, `right`, and `bottom` application-body insets)
- `spacing`
- `indent`
- `title_height`
- `window_chrome_layout` (`trailing_buttons` or `classic_mac`)
- `window_border` (`left`, `top`, `right`, and `bottom` structural edge thicknesses)
- `scrollbar_size`
- `thumb_size`
- `frame_insets`

The optional `colors` object accepts RGBA byte arrays under these keys:

- `text`, `border`, `window_background`, `title_background`, `title_text`
- `disabled_text`, `disabled_background`, `disabled_title_text`
- `panel_background`, `button`, `button_hover`, `input`, `input_hover`
- `scrollbar_track`, `scrollbar_thumb`, `control_focus`
- `selection_background`, `selection_foreground`, `window_active`
- `menu_foreground`, `menu_background`

These colors construct Skin's complete visual table before any per-state PNG or `foreground`
override is installed. Each `AppearanceRole`/`VisualState` cell is one concrete
`Visual { patch, foreground }`; background and foreground cannot drift through parallel catalogs,
and no erased or string-keyed payload participates at runtime.

## Inheritance

A document may set `"extends": "../base/theme.json"`. Parent paths, font paths, and PNG paths are
resolved relative to the exact document that declares them. The child name becomes the loaded
theme name. A child-supplied `fonts` object replaces the complete inherited font recipe because the
five semantic entries and atlas dimensions form one unit.

The `skin` object merges field by field. Appearance roles merge independently; within a role,
`insets` and each visual state merge independently; within a state, `png`, `source_insets`, `tint`,
and `foreground` merge independently. The more-derived present value wins and an omitted value
preserves its parent. There is no JSON `null` removal operation in schema version 2.

The loader rejects unknown document fields, unknown skin/palette fields, unknown appearance names,
unknown state fields, cycles, chains deeper than 32 documents, unsupported schema versions, and
invalid names before constructing an atlas. The fully merged compiler input contains typed
`AppearanceRole` tables and fully resolved paths; the runtime bundle retains no inheritance graph
or string-keyed role map.

## Loading and selecting

```rust,no_run
# use microui_redux::{Context, LoadedTheme};
# use microui_redux::render::RendererBackend;
fn select<B: RendererBackend, State: 'static>(context: &mut Context<B, State>, theme: &LoadedTheme) {
    context.set_theme(theme).expect("backend must upload the selected theme atlas");
}
```

Capture `context.skin_bundle().clone()` in `LoadedTheme::new("Default Skin", bundle)` when a
selector needs to return to the initial flat appearance after choosing a file theme. Ordinary Skin
changes clone `context.skin()` and use `Context::set_skin`; image-backed visual edits must continue
to belong to the currently active atlas. A flat palette editor cannot reinterpret multicolor PNG
pixels as named RGBA fields, so `demo-full` enables its palette controls only for the programmatic
flat skin while continuing to permit concrete metric changes for image-backed themes.

## Appearance roles

The `appearances` object accepts the following exact keys:

- `generic_frame`, `panel`, `button`, `checkbox`, `text_input`, `item`
- `combo`, `slider_track`, `slider_thumb`, `scrollbar_track`, `scrollbar_thumb`
- `menu_bar`, `menu_title`, `menu_title_open`, `menu_popup`, `menu_item`
- `window_frame`, `window_frame_active`, `dialog_frame`, `dialog_frame_active`
- `window_title`, `window_title_active`
- `window_close_button`, `window_minimize_button`, `window_maximize_button`,
  `window_restore_button`, `window_resize_grip`
- `window_close_glyph`, `window_minimize_glyph`, `window_maximize_glyph`,
  `window_restore_glyph`

Unknown fields and role names are errors. This prevents a misspelled state or control name from
silently falling back to a flat appearance.

`panel` and container-owned generic frames are passive structure: their body and border resolve the
normal appearance while the pointer moves across them. Losing top-level activation selects the
passive `window_frame`, `dialog_frame`, and `window_title` roles but does not rewrite enabled child
widgets. A disabled widget or subtree resolves `disabled` independently of activation; its
foreground uses `disabled_text` (or `disabled_title_text` for chrome), and omitted PNG states use
`disabled_background` as their flat fallback. The ordinary frame pair and modal dialog pair also
use normal center artwork for the application body even when a resize edge is hovered or captured.
Interactive descendants, resize borders, caption controls, and the title remain free to resolve
their own hover and pressed states while enabled.

`WindowOption::DISABLED` is the explicit whole-window counterpart. It keeps the root visible and
keeps its retained update traversal running, but resolves the passive frame/title roles, intrinsic
menu bar, caption controls, and complete widget tree through `disabled`. It also rejects pointer,
keyboard, popup, move, resize, and caption input while retaining focus for later re-enabling. A
structural child window inherits a disabled structural parent; an owned modal dialog remains an
independent root so it can stay enabled above a deliberately disabled owner. This policy is
separate from ordinary activation: selecting another window never disables the former window's
children.

## Window borders and caption controls

`skin.window_border` is the window's structural border thickness. Its four values drive client
layout and the right/bottom one-axis resize hit regions. `window_frame.insets` and
`dialog_frame.insets` instead control the fixed visual corner span for their respective
three-by-three artwork; each active role is normalized to its corresponding passive role's visual
authority. Keeping the values separate permits a four-pixel Windows 3.11 edge to carry a 23-pixel
L-shaped ordinary-window corner while a modal dialog uses a uniform four-pixel outline, without
reserving 23 pixels around either client. The bottom-right two-axis region remains larger for easy
input, but themes may leave `window_resize_grip` transparent when the frame corner itself is the
complete visible affordance.

`skin.window_content_insets` is a separate four-edge inset around the application body. Root
geometry applies it after the frame, title, and menu bar have been allocated, so it never narrows
the menubar and never changes ordinary widget padding. The bundled Windows and Mac themes set all
four edges to zero; the default flat Skin retains a five-pixel body inset.
`WindowOption::NO_PADDING` overrides the metric with zero for an individual root.

Application-authored popup windows use the `menu_popup` appearance for both their structural
client inset and their outer frame paint. This keeps combo/list popups aligned with themed menu
panels instead of borrowing ordinary window L-corners or `skin.window_border`; compact menu
popups already paint the same role as their complete manager-owned panel.

Window caption controls are enabled explicitly through `WindowOption::MINIMIZE_BUTTON` and
`WindowOption::MAXIMIZE_BUTTON`. The close button remains enabled unless `WindowOption::NO_CLOSE`
is present. Caption and resize roles receive `hovered` and `pressed` states from manager-owned
pointer capture just like widgets. A press dragged away from its originating caption button is no
longer painted pressed and does not activate on release.

`skin.window_chrome_layout` selects concrete platform geometry without changing the semantic
caption roles. `trailing_buttons` preserves the ordinary left-aligned title and places every
caption control at the trailing edge. `classic_mac` centers the title, places a compact close box
at the leading edge, places compact zoom/windowshade controls at the trailing edge, and omits those
faces from passive titles. Classic Mac caption PNGs are complete faces, so this layout does not
overlay the generic procedural glyphs used by flat and Windows-oriented skins. The JSON enum is
compiled once into a concrete `WindowChromeSkin` data recipe; manager code does not branch on a
theme or platform mode.

The optional caption-glyph roles paint centered image artwork inside their corresponding button
face. If a glyph role is transparent or omitted, the renderer uses its deterministic procedural
fallback. This lets classic themes provide period-specific triangle controls and pressed offsets
without forcing every flat application skin to ship additional images.

Minimize hides the retained window and emits `WindowEvent::Minimized`; the same `WindowHandle` can
be shown again. Maximize saves the exact normal outer rectangle, tracks the complete inherited
viewport, and emits `WindowEvent::Maximized`. Activating the same caption position while maximized
uses `window_restore_button`, restores the saved rectangle, and emits `WindowEvent::Restored`.
