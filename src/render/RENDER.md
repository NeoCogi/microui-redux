# Rendering

The `render` module is the boundary between retained UI painting and a concrete
GPU or software backend. Widgets record backend-neutral operations; the
renderer expands and clips those operations; the backend receives final
vertices and texture commands.

Most application code only needs `WidgetCtx::painter()` and the rendering types
re-exported by `microui_redux::prelude`. Low-level integrations and backend
implementations import the remaining types explicitly:

```rust
use microui_redux::{
    prelude::{BackendHandle, RendererBackend},
    render::{DisplayList, Painter, Renderer, Vertex},
};
```

## Architecture

```text
Widget::paint
     |
     v
  Painter  --->  DisplayList  --->  Renderer  --->  BackendHandle  --->  RendererBackend
  records       owns ordered     expands and     synchronizes       batches and
  primitives    draw operations  clips work      backend access     submits work
```

Each layer has one responsibility:

| Layer | Owns | Does not own |
| --- | --- | --- |
| `Painter` | Local-to-screen translation, operation recording, scoped clip intersection, solid-shape tessellation | Backend state, frame lifecycle, atlas lookup, input, style policy |
| `DisplayList` | Ordered operations, operation clips, owned text/callback payloads, solid triangles, reusable recording storage | Execution, backend locks, textures |
| `Renderer` | Frame dimensions, atlas expansion, final clipping, texture lifetime, display-list execution, reusable execution scratch | Widget input, widget layout, mutable drawing state |
| `BackendHandle` | Shared synchronized access to one backend | Rendering policy or command interpretation |
| `RendererBackend` | GPU/software resources, batching, texture binding, final submission | UI input, widget state, clipping decisions |

The source layout follows those boundaries:

```text
src/render/
├── backend.rs       public backend contract, handle, vertices, custom callbacks
├── display_list.rs  owned operations and recording storage
├── geometry.rs      internal tessellation and final clipping geometry
├── painter.rs       public widget-local recorder
├── renderer.rs      public frame/resource owner and operation executor
└── RENDER.md        architecture and integration guide
```

`display_list` and `geometry` are private implementation modules even though
`DisplayList` itself is public. Their internals can change without growing the
public API.

## Frame execution

The normal `Context` path is:

```text
Context::begin_render_frame
    -> Renderer::begin

Context::update_ui
    -> measure widgets
    -> route input and update widget state
    -> Widget::paint records one ordered DisplayList

Context::end_render_frame
    -> Renderer::render drains the DisplayList once
    -> Renderer::end
```

Input belongs to widget update and never enters the rendering subsystem. By the
time painting starts, widgets record only visual state.

A low-level integration can own the list and renderer directly:

```rust
use microui_redux::{
    prelude::{color, Recti, Vec2i},
    render::{DisplayList, Painter, Renderer, RendererBackend},
};

fn render_frame<B: RendererBackend>(
    renderer: &mut Renderer<B>,
    display_list: &mut DisplayList,
) {
    let dimensions = renderer.dimensions();
    let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);

    renderer.begin(
        dimensions.width,
        dimensions.height,
        color(18, 20, 24, 255),
    );
    {
        let mut painter =
            Painter::new(display_list, Vec2i::default(), viewport, viewport);
        painter.fill_rect(
            Recti::new(8, 8, 80, 24),
            color(70, 110, 180, 255),
        );
    }
    renderer.render(display_list);
    renderer.end();
}
```

`Renderer::render` consumes every operation in painter order and leaves the
list empty for reuse. Ordinary adjacent operations execute while one backend
lock is held. A custom-render operation is a barrier: normal work is flushed,
the lock is released for the callback, the callback runs, and normal execution
then resumes.

## Painting custom widgets

`WidgetCtx::painter()` creates a recorder in widget-local coordinates. A
custom widget can draw without knowing its screen position or the concrete
backend:

```rust
use microui_redux::prelude::*;

#[derive(Clone)]
struct PaintedSwatch {
    options: WidgetOption,
}

impl Widget for PaintedSwatch {
    fn widget_opt(&self) -> &WidgetOption {
        &self.options
    }

    fn measure(
        &self,
        _style: &Style,
        _atlas: &AtlasHandle,
        _available: Dimensioni,
    ) -> Dimensioni {
        Dimensioni::new(96, 48)
    }

    fn update(
        &mut self,
        _ctx: &mut WidgetCtx<'_>,
        _events: Vec<UiInputEvent>,
    ) -> ResourceState {
        ResourceState::NONE
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>) {
        let hovered = ctx.hovered();
        let mut painter = ctx.painter();
        let bounds = painter.local_rect();
        let fill = if hovered {
            color(54, 116, 155, 255)
        } else {
            color(42, 70, 92, 255)
        };

        painter.fill_rect(bounds, fill);
        painter.stroke_rect(bounds, 1, color(230, 236, 240, 255));
        painter.with_clip(
            Recti::new(4, 4, bounds.width - 8, bounds.height - 8),
            |painter| {
                painter.stroke_line(
                    Vec2f::new(8.0, bounds.height as f32 - 10.0),
                    Vec2f::new(bounds.width as f32 - 8.0, 10.0),
                    3.0,
                    color(255, 202, 72, 255),
                );
            },
        );
    }
}
```

The main primitives are:

| Method | Recorded work |
| --- | --- |
| `fill_rect` | Semantic atlas-backed solid rectangle |
| `stroke_rect` | Inside-aligned rectangle border recorded as fills |
| `text` | UTF-8 text run expanded through the atlas during execution |
| `icon` | Atlas icon |
| `image` | Atlas slot or external texture |
| `redraw_slot` | Dynamic atlas-slot update followed by its draw |
| `stroke_line` | Tessellated solid line |
| `fill_polygon` | Tessellated solid polygon |
| `with_clip` | Child painter with an intersected local clip |

Style-aware decisions stay in widgets. `Painter` records colors, fonts, and
geometry that the caller has already selected; it does not inspect hover,
focus, input, or the active `Style`.

### Text

`Painter::text` records a UTF-8 run and its selected `FontId`; glyph lookup and
final clipping happen during renderer execution. Retained text widgets center
the font baseline inside their cells, and each line receives a small vertical
pad so glyphs do not touch widget borders. `TextBlock` supports wrapped
multi-line content while preserving outer padding without inserting additional
spacing between lines.

## Coordinates and clipping

Three coordinate concepts are deliberately separate:

1. Widget methods and `Painter` primitives use local coordinates.
2. `Painter` translates recorded geometry into screen coordinates using its
   fixed origin.
3. `Renderer` consumes screen-space operations and clips them to the current
   viewport immediately before backend submission.

Every display-list operation owns its effective screen-space clip. A clip is
not mutable renderer state and there is no public clip stack.

```rust
use microui_redux::{
    prelude::{color, Recti},
    render::Painter,
};

fn paint_inside(painter: &mut Painter<'_>) {
    let bounds = painter.local_rect();
    painter.with_clip(
        Recti::new(4, 4, bounds.width - 8, bounds.height - 8),
        |painter| {
            painter.fill_rect(bounds, color(80, 120, 180, 255));
        },
    );
}
```

`with_clip` intersects the requested rectangle with the parent clip, so nested
scopes can only narrow visibility. Returning from the closure restores the
parent painter naturally.

Final clipping has one authority:

- semantic atlas rectangles are clipped and their source rectangles adjusted;
- external texture quads are clipped while preserving UV projection;
- solid triangles are clipped against the operation clip and viewport;
- disjoint or empty geometry is discarded.

Backends therefore never receive UI clip rectangles. They consume final
screen-space vertices.

## Display-list ownership and reuse

`DisplayList` owns all data required after recording, including strings,
custom-render callbacks, and solid geometry. No operation borrows widget or
container memory.

The list is designed to be reused:

- recording appends into retained operation and geometry allocations;
- `Renderer::render` detaches and drains the current frame once;
- drained storage is recycled into the list;
- renderer-side glyph and clipping scratch buffers are retained between
  frames.

Solid line and polygon tessellation appends into `SolidGeometry` workspace
rather than returning temporary vectors in the rendering path. Final clipped
vertices likewise append into renderer-owned scratch storage.

## Images and textures

`Image::Slot` references an atlas slot and participates in normal atlas
batching. `Image::Texture` references an external texture owned through
`Renderer`.

Image-bearing widgets accept either variant. `WidgetFillOption` controls which
interaction states draw the widget's filled background; use
`WidgetFillOption::ALL` to retain the normal, hover, and click fills around an
image.

Normal applications should manage texture lifetime through `Context`:

```rust
use microui_redux::{
    prelude::{Context, Image, RendererBackend},
};

fn upload_image<B: RendererBackend>(
    context: &mut Context<B>,
    width: i32,
    height: i32,
    rgba: &[u8],
) -> Result<Image, String> {
    let texture = context.try_load_image_rgba(width, height, rgba)?;
    Ok(Image::Texture(texture))
}
```

Use `Context::free_image` when the texture is no longer needed.
`Context::load_image_from` accepts `ImageSource`, while
`Context::load_image_rgba` is the panicking convenience form for already
validated RGBA data.

Low-level integrations use the equivalent renderer methods:

```rust
use microui_redux::{
    prelude::TextureId,
    render::{Renderer, RendererBackend},
};

fn upload_checkerboard<B: RendererBackend>(
    renderer: &mut Renderer<B>,
) -> Result<TextureId, String> {
    let rgba = [
        255, 255, 255, 255, 0, 0, 0, 255,
        0, 0, 0, 255, 255, 255, 255, 255,
    ];
    renderer.try_load_texture_rgba(2, 2, &rgba)
}
```

Texture dimensions and RGBA byte length are validated before a texture ID is
consumed. Failed backend creation does not leave a tracked texture behind.
Dropping `Renderer` destroys every external texture it still owns.

## Custom-render callbacks

Portable UI drawing belongs in `Painter`. A `CustomRenderCommand` is the
escape hatch for backend-specific work such as a 3D viewport.

```rust
use microui_redux::{
    prelude::{Dimensioni, Recti},
    render::{CustomRenderArgs, CustomRenderCommand},
};

fn custom_command() -> Box<dyn CustomRenderCommand> {
    Box::new(|dimensions: Dimensioni, args: &CustomRenderArgs| {
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        let content_area = args.content_area;
        let visible_area = args.view;
        let _ = (viewport, content_area, visible_area);
    })
}
```

The callback receives:

- `content_area`: the custom node's full content rectangle;
- `view`: the final visible rectangle after retained clipping;
- frame dimensions through the callback's first argument.

It deliberately receives no input. Interaction remains in widget update.
Applications that need backend access can capture a cloned `BackendHandle` in
the callback.

## Implementing a backend

A backend implements `RendererBackend` and consumes `render::Vertex`:

```rust
use microui_redux::{
    prelude::{AtlasHandle, Color, TextureId},
    render::{RendererBackend, Vertex},
};

struct Backend {
    atlas: AtlasHandle,
}

impl RendererBackend for Backend {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, _width: i32, _height: i32, _clear: Color) {}

    fn push_quad_vertices(
        &mut self,
        _v0: &Vertex,
        _v1: &Vertex,
        _v2: &Vertex,
        _v3: &Vertex,
    ) {
    }

    fn push_triangle_vertices(
        &mut self,
        _v0: &Vertex,
        _v1: &Vertex,
        _v2: &Vertex,
    ) {
    }

    fn flush(&mut self) {}

    fn end(&mut self) {}

    fn create_texture(
        &mut self,
        _id: TextureId,
        _width: i32,
        _height: i32,
        _pixels: &[u8],
    ) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}

    fn draw_texture(
        &mut self,
        _id: TextureId,
        _vertices: [Vertex; 4],
    ) {
    }
}
```

Backend rules:

- `push_quad_vertices` and `push_triangle_vertices` receive final atlas-backed
  geometry and may batch it.
- `flush` must submit outstanding batched work without ending the frame.
- `draw_texture` receives a pre-clipped quad. Backends that batch atlas work
  must preserve painter order when switching textures.
- `create_texture` must return an error without retaining the ID when creation
  fails.
- `destroy_texture` releases the matching backend resource.
- the backend does not calculate UI clipping or inspect input.

`BackendHandle` wraps the implementation in shared synchronized ownership.
`scope` provides read access and `scope_mut` provides exclusive mutable access.
The renderer holds the lock across runs of ordinary operations instead of
locking once per primitive.

## Working examples

- [`examples/retained-custom-drawing.rs`](../../examples/retained-custom-drawing.rs)
  implements a retained custom widget using `WidgetCtx::painter`.
- [`examples/texture-clipping-smoke.rs`](../../examples/texture-clipping-smoke.rs)
  exercises low-level display-list execution, texture upload, clipping, and
  final vertices.
- `examples/common/glow_renderer.rs`,
  `examples/common/vulkan_renderer.rs`, and
  `examples/common/wgpu_renderer.rs` are complete backend implementations.
- `examples/demo-full.rs` demonstrates retained UI, custom painting, external
  textures, and custom backend rendering together.

The rendering API is a clean break from the former layout. See
[`MIGRATION.md`](../../MIGRATION.md) for the complete path and method mapping.
