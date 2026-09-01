# JSON themes

The `theme-json` feature is enabled by default. It adds `Context::load_theme_file`, which reads one
strict, versioned JSON definition and uploads each referenced PNG through that Context's renderer.
The returned `LoadedTheme` contains a complete `Style`; install it with
`context.set_style(theme.style().clone())`.

All PNG paths are relative to the JSON file. Theme textures remain owned by the loading Context, so
a loaded style cannot be installed in another Context. A failed load destroys every image uploaded
earlier in that same call.

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
