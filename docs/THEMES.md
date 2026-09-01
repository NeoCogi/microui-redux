# JSON themes

The `theme-json` feature is enabled by default. It adds `Context::load_theme_file`, which reads one
strict, versioned JSON definition and uploads each referenced PNG through that Context's renderer.
The returned `LoadedTheme` contains a rebuilt font atlas and its complete matching `Style`; install
both with `context.set_theme(&theme)`. Loading does not change the active renderer, so several
themes can be prepared before the user selects one. Selection uploads the replacement atlas first
and publishes the Style only after that backend transaction succeeds.

All PNG paths are relative to the JSON file. Theme textures remain owned by the loading Context, so
a loaded style cannot be installed in another Context. A failed load destroys every image uploaded
earlier in that same call.
Repeated state entries that resolve to the same PNG path share one decoded and uploaded texture;
their source insets, destination insets, and tints remain independent typed patch metadata.

## Bundled example themes

The repository includes [`themes/windows-3.11/theme.json`](../themes/windows-3.11/theme.json),
[`themes/windows-95/theme.json`](../themes/windows-95/theme.json), and
[`themes/mac-os-9/theme.json`](../themes/mac-os-9/theme.json). All use original BSD-licensed pixel
artwork authored for this project; their directory READMEs identify the GTK, platform-guideline,
and gallery references used for visual research and explicitly document that no third-party theme
files were copied.

`demo-full` loads the Default Style and every bundled file once at startup. Choose them from
`View > Theme`; each selection installs a pristine editable copy, so the existing Style Editor can
modify it without changing the stored base theme. The example requires the `theme-json` feature and
the theme directories must remain available beside the repository sources at runtime.

## Minimal schema

```json
{
  "schema_version": 1,
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
  "style": {
    "padding": 4,
    "spacing": 4,
    "title_height": 20,
    "frame_insets": { "left": 1, "top": 1, "right": 1, "bottom": 1 },
    "colors": {
      "text": [0, 0, 0, 255],
      "border": [0, 0, 0, 255],
      "button": [192, 192, 192, 255],
      "button_hover": [208, 208, 208, 255],
      "focus": [0, 0, 128, 255]
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
      "disabled": { "png": "button-disabled.png", "tint": [255, 255, 255, 160] },
      "inactive": { "png": "button-inactive.png" }
    }
  }
}
```

The optional `fonts` object is all-or-nothing. When present, it declares atlas texture dimensions
and exact file/size recipes for the five semantic roles: `body`, `small`, `title`, `heading`, and
`mono`. Paths are relative to the JSON file. Loading copies the current atlas's named icons into a
fresh atlas of the requested size, rasterizes only these declared fonts, and binds the resulting
font IDs into the theme Style. The bundled classic themes all use this path.

Atlas-scoped IDs are intentionally concrete capabilities. A widget configured with
`FontChoice::Id` from the preceding atlas cannot survive a theme switch; use a semantic `FontRole`
for theme-controlled text. Custom rendering code that caches raw atlas UV coordinates must refresh
those coordinates when installing a different atlas.

An appearance or state may be omitted. Every omitted state keeps its own flat-color fallback; it
does not borrow another state's PNG. A state may set `foreground` without a PNG to recolor its text
and semantic glyphs over that fallback. Conversely, a PNG state may omit `foreground` and retain
the fallback color. This makes partial themes predictable and lets a theme use images only where
they add value.

`insets` controls destination layout and stretching. `source_insets` divides the PNG and defaults to
the role's destination insets. Source insets must be non-negative and opposing values must fit
inside the PNG. Destination insets may be larger than a runtime control; the renderer reduces
opposing sides proportionally for tiny destinations.

## Style fields

The optional `style` object accepts these integer metrics:

- `default_cell_width`
- `padding`
- `spacing`
- `indent`
- `title_height`
- `window_border` (`left`, `top`, `right`, and `bottom` structural edge thicknesses)
- `scrollbar_size`
- `thumb_size`
- `frame_insets`

The optional `colors` object accepts RGBA byte arrays under these keys:

- `text`, `border`, `window_background`, `title_background`, `title_text`
- `inactive_text`, `inactive_background`, `inactive_title_text`
- `panel_background`, `button`, `button_hover`, `input`, `input_hover`
- `scrollbar_track`, `scrollbar_thumb`, `focus`, `window_focus`
- `menu_foreground`, `menu_background`

These colors construct the complete flat appearance and foreground fallback catalogs before any
per-state PNG or `foreground` override is installed. `Style::foreground(role, state)` and the
public `ForegroundCatalog` provide the same concrete enum-indexed lookup and mutation model as
background appearances; no erased or string-keyed payload participates at paint time.

## Loading and selecting

```rust,no_run
# use microui_redux::{Context, LoadedTheme};
# use microui_redux::render::RendererBackend;
fn select<B: RendererBackend, State: 'static>(context: &mut Context<B, State>, theme: &LoadedTheme) {
    context.set_theme(theme).expect("backend must upload the selected theme atlas");
}
```

`LoadedTheme::from_style` can capture the initial flat atlas/style pair so a selector can return to
the default appearance after choosing a file theme. Ordinary Style Editor changes continue to use
`Context::set_style`; they are valid while they retain IDs from the currently installed atlas.

## Appearance roles

The `appearances` object accepts the following exact keys:

- `generic_frame`, `panel`, `button`, `checkbox`, `checkbox_checked`, `text_input`
- `list_item`, `list_item_selected`, `combo`, `slider_track`, `slider_thumb`
- `scrollbar_track`, `scrollbar_thumb`, `disclosure_header`
- `menu_bar`, `menu_title`, `menu_title_open`, `menu_popup`, `menu_item`,
  `menu_item_selected`
- `window_frame`, `window_frame_active`, `window_title`, `window_title_active`
- `window_close_button`, `window_minimize_button`, `window_maximize_button`,
  `window_restore_button`, `window_resize_grip`
- `window_close_glyph`, `window_minimize_glyph`, `window_maximize_glyph`,
  `window_restore_glyph`

Unknown fields and role names are errors. This prevents a misspelled state or control name from
silently falling back to a flat appearance.

`panel` and container-owned generic frames are passive structure: their body and border resolve the
normal appearance while the pointer moves across them. When a top-level window deactivates, its
chrome and complete retained widget hierarchy resolve the separate `inactive` state instead;
deactivation does not erase remembered focus or masquerade as hover, ordinary focus, or disabled.
The inactive foreground colors apply to built-in text and semantic icons, and the inactive
background supplies flat fallbacks for roles without an `inactive` PNG. Likewise, `window_frame` and
`window_frame_active` use normal center artwork for the application body even when a resize edge is
hovered or captured. Interactive descendants, resize borders, caption controls, and the title
remain free to resolve their own hover and pressed states.

## Window borders and caption controls

`style.window_border` is the window's structural border thickness. Its four values drive client
layout and the right/bottom one-axis resize hit regions. `window_frame.insets` instead controls the
fixed visual corner span of the three-by-three artwork; active frame art is normalized to that
visual authority. Keeping the values separate permits a four-pixel Windows 3.11 edge to carry a
23-pixel L-shaped corner without reserving 23 pixels around the client. The bottom-right two-axis
region remains larger for easy input, but themes may leave `window_resize_grip` transparent when
the frame corner itself is the complete visible affordance.

Window caption controls are enabled explicitly through `WindowOption::MINIMIZE_BUTTON` and
`WindowOption::MAXIMIZE_BUTTON`. The close button remains enabled unless `WindowOption::NO_CLOSE`
is present. Caption and resize roles receive `hovered` and `pressed` states from manager-owned
pointer capture just like widgets. A press dragged away from its originating caption button is no
longer painted pressed and does not activate on release.

The optional caption-glyph roles paint centered image artwork inside their corresponding button
face. If a glyph role is transparent or omitted, the renderer uses its deterministic procedural
fallback. This lets classic themes provide period-specific triangle controls and pressed offsets
without forcing every flat application style to ship additional images.

Minimize hides the retained window and emits `WindowEvent::Minimized`; the same `WindowHandle` can
be shown again. Maximize saves the exact normal outer rectangle, tracks the complete inherited
viewport, and emits `WindowEvent::Maximized`. Activating the same caption position while maximized
uses `window_restore_button`, restores the saved rectangle, and emits `WindowEvent::Restored`.
