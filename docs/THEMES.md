# JSON themes

The `theme-json` feature is enabled by default. It adds `Context::load_theme_file`, which reads one
strict, versioned JSON definition and uploads each referenced PNG through that Context's renderer.
The returned `LoadedTheme` contains a complete `Style`; install it with
`context.set_style(theme.style().clone())`.

All PNG paths are relative to the JSON file. Theme textures remain owned by the loading Context, so
a loaded style cannot be installed in another Context. A failed load destroys every image uploaded
earlier in that same call.
Repeated state entries that resolve to the same PNG path share one decoded and uploaded texture;
their source insets, destination insets, and tints remain independent typed patch metadata.

## Bundled example themes

The repository includes [`themes/windows-95/theme.json`](../themes/windows-95/theme.json) and
[`themes/mac-os-9/theme.json`](../themes/mac-os-9/theme.json). Both use original BSD-licensed pixel
artwork authored for this project; their directory READMEs identify the GTK and gallery references
used for visual research and explicitly document that no third-party theme files were copied.

`demo-full` loads the Default Style and both bundled files once at startup. Choose them from
`View > Theme`; each selection installs a pristine editable copy, so the existing Style Editor can
modify it without changing the stored base theme. The example requires the `theme-json` feature and
the theme directories must remain available beside the repository sources at runtime.

## Minimal schema

```json
{
  "schema_version": 1,
  "name": "Example",
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
      "pressed": { "png": "button-pressed.png" },
      "focused": { "png": "button-focused.png" },
      "hovered_focused": { "png": "button-hovered-focused.png" },
      "pressed_focused": { "png": "button-pressed-focused.png" },
      "disabled": { "png": "button-disabled.png", "tint": [255, 255, 255, 160] }
    }
  }
}
```

An appearance or state may be omitted. Every omitted state keeps its own flat-color fallback; it
does not borrow another state's PNG. This makes partial themes predictable and lets a theme use
images only where they add value.

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
- `scrollbar_size`
- `thumb_size`
- `frame_insets`

The optional `colors` object accepts RGBA byte arrays under these keys:

- `text`, `border`, `window_background`, `title_background`, `title_text`
- `panel_background`, `button`, `button_hover`, `input`, `input_hover`
- `scrollbar_track`, `scrollbar_thumb`, `focus`, `window_focus`
- `menu_foreground`, `menu_background`

These colors construct the complete flat fallback catalog before any PNG state is installed.

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

Unknown fields and role names are errors. This prevents a misspelled state or control name from
silently falling back to a flat appearance.

## Window borders and caption controls

`window_frame.insets` is the window's sole structural border thickness. Its left, top, right, and
bottom values drive client layout, inactive and active frame paint, and hit testing together;
`window_frame_active.insets` is normalized to that authority during painting. A resizable window
uses the configured right and bottom thicknesses as its width-only and height-only hit regions; the
existing bottom-right grip remains a two-axis resize region whose visible art comes from
`window_resize_grip`.

Window caption controls are enabled explicitly through `WindowOption::MINIMIZE_BUTTON` and
`WindowOption::MAXIMIZE_BUTTON`. The close button remains enabled unless `WindowOption::NO_CLOSE`
is present. Caption and resize roles receive `hovered` and `pressed` states from manager-owned
pointer capture just like widgets. A press dragged away from its originating caption button is no
longer painted pressed and does not activate on release.

Minimize hides the retained window and emits `WindowEvent::Minimized`; the same `WindowHandle` can
be shown again. Maximize saves the exact normal outer rectangle, tracks the complete inherited
viewport, and emits `WindowEvent::Maximized`. Activating the same caption position while maximized
uses `window_restore_button`, restores the saved rectangle, and emits `WindowEvent::Restored`.
