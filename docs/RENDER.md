# Rendering

The `render` module is the boundary between retained UI painting and a concrete
GPU or software backend. Widgets record backend-neutral operations; Context's
private executor expands and clips those operations; the backend receives final
vertices and texture commands.

Most application code only needs `WidgetPaintCtx::painter()` and the rendering types
re-exported by `microui_redux::prelude`. Backend implementations import their frame contracts
explicitly:

```rust
use microui_redux::{
    prelude::RendererBackend,
    render::{FrameInfo, RendererFrame, Vertex},
};
```

## Architecture

```text
Widget::paint
     |
     v
  Painter  --->  DisplayList  --->  Context executor  --->  RendererBackend::Frame
  records       owns ordered     expands and clips      batches, submits,
  primitives    draw operations  crate-owned work       and presents work
```

Each layer has one responsibility:

| Layer | Owns | Does not own |
| --- | --- | --- |
| `Painter` | Local-to-screen translation, operation recording, scoped clip intersection, solid-shape tessellation | Backend state, frame lifecycle, atlas lookup, input, style policy |
| Internal display list | Ordered operations, operation clips, owned text, custom-render keys, solid triangles, reusable recording storage | Execution, backend access, textures |
| Context render executor | Unique backend ownership, atlas expansion, final clipping, texture lifetime, display-list execution, reusable execution scratch | Public standalone submission, widget input, widget layout, mutable drawing state |
| `RendererBackend` | Persistent GPU/software and texture resources | UI input, widget state, clipping decisions |
| `RendererBackend::Frame` | One acquired frame, batching, texture binding, final submission/presentation | Persistent application ownership |

The source layout follows those boundaries:

```text
src/render/
├── backend.rs       public backend/frame contracts, vertices, typed custom callbacks
├── display_list.rs  crate-owned operations and recording storage
├── geometry.rs      internal tessellation and final clipping geometry
├── painter.rs       public widget-local recorder
├── performance.rs   test-only timing, allocation, and submission benchmark
└── renderer.rs      crate-private frame/resource owner and operation executor

docs/
└── RENDER.md        architecture and integration guide
```

`display_list` and `geometry` are private implementation modules. Applications cannot construct a
display list or submit one directly; retained traversal owns recording and submission so every
public `Painter` comes from a traversal-derived `WidgetPaintCtx`.

```compile_fail
use microui_redux::render::DisplayList;
```

## Frame execution

Applications deliver input and mutate retained state before committing an update. Rendering then
paints only that commit.
The normal `Context` path is:

```text
Context::update_ui(positive dimensions)
    -> run one synchronization layout
    -> if input is empty: update every eligible widget once, commit layout
    -> otherwise drain raw input in FIFO order
       -> for each event: route once, update every eligible widget, commit layout

Context::frame(validated FrameInfo)
    -> returns an exclusively borrowed ContextFrame

ContextFrame::render_ui(self)
    -> require a matching commit, no pending input, and no visible measurement mutation
    -> Widget::paint records one ordered DisplayList
    -> preflight resource keys
    -> RendererBackend::frame acquires native frame resources
    -> Context's private executor drains and submits the DisplayList once
    -> backend frame Drop flushes, submits, and presents
```

`render_ui` never drains input, updates widgets, or computes layout. A missing or stale commit,
pending input, a measurement-affecting typed mutation in a visible widget tree, or different frame
dimensions returns `RenderError::UiUpdateRequired` before paint, display-list execution, or backend
acquisition. Input and measurement invalidation belong to `update_ui`; by the time painting starts,
widgets record only committed visual state.

Widget paint is observational with respect to application-authored semantic state, topology,
interaction, and committed layout. Widgets may maintain private rendering caches, but cannot alter
the current commit or publish application-coordination events. Backend custom-render callbacks may
maintain callback-private rendering caches only. A callback that captures a `TypedWidgetHandle` and
mutates retained UI during rendering
violates the contract; the mutation is not scheduled as deferred work, and weak handles cannot
invalidate the already selected commit. Perform semantic mutations before `update_ui` and create
the frame only after that commit is complete.

The crate-owned submission path consumes every operation in painter order and leaves its internal
list empty for reuse, including validation or frame-acquisition failures.
Ordinary adjacent operations execute through one exclusively borrowed backend
frame. External textures and custom-render operations are ordering barriers:
normal atlas work is flushed immediately before each barrier.

## Painting custom widgets

`WidgetPaintCtx::painter()` creates a recorder in widget-local coordinates. A
custom widget can draw without knowing its screen position or the concrete
backend. `WidgetUpdateCtx` deliberately has no painter or display-list access,
so visual ordering cannot depend on work recorded during update. A `Widget::paint` implementation
may update private rendering caches; it must not change application-authored semantic state, publish
coordination events, or alter the current committed layout:

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

    fn update(
        &mut self,
        _ctx: &mut WidgetUpdateCtx<'_>,
        _event: Option<&UiInputEvent>,
    ) {}

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
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

impl LeafWidget for PaintedSwatch {
    fn measure(
        &self,
        _style: &Style,
        _atlas: &AtlasHandle,
        _constraints: Constraints,
    ) -> Dimensioni {
        Dimensioni::new(96, 48)
    }
}
```

The main primitives are:

| Method | Recorded work |
| --- | --- |
| `fill_rect` | Semantic atlas-backed solid rectangle |
| `stroke_rect` | Inside-aligned rectangle border recorded as fills |
| `text` | UTF-8 string expanded through the selected atlas font during execution |
| `icon` | Atlas icon |
| `image` | External texture identified by `TextureId` |
| `stroke_line` | Tessellated solid line |
| `fill_polygon` | Tessellated solid polygon |
| `with_clip` | Child painter with an intersected local clip |

Style-aware decisions stay in widgets. `Painter` records colors, fonts, and
geometry that the caller has already selected; it does not inspect hover,
focus, input, or the active `Style`.

### Text

`Painter::text` records a UTF-8 string and its selected `FontId`; glyph lookup and final clipping
happen during renderer execution. Text storage is UTF-8-safe, while glyph coverage is
atlas-dependent. Rendering and measurement iterate Unicode scalar values. A missing character
uses the selected font's underscore entry. Every serialized font must therefore contain `_`;
strict atlas construction rejects a font without that explicit fallback rather than sampling a
synthetic rectangle at the atlas origin.

The built-in builder bakes printable ASCII (`U+0020` through `U+007E`) only. Serialized atlas
sources can describe arbitrary Unicode scalar values, but the renderer does not perform grapheme
segmentation, script shaping, bidirectional reordering, kerning, or fallback-font selection.
Textbox and text-area cursor operations also work on scalar-value boundaries rather than
user-perceived grapheme clusters. `TextWrap::Word` uses ASCII spaces as wrap opportunities and does
not split an overlong individual word. Textbox removes CR and LF at construction, replacement, and
input boundaries. TextArea and TextBlock instead canonicalize CRLF and lone CR to LF. Low-level
atlas measurement and drawing likewise treat CRLF as one line ending.

`FontId` is an opaque capability containing one runtime atlas owner and one local font slot.
Cloned handles to the same atlas mint equal IDs; separately loading identical metadata does not.
Context preflight rejects a foreign font before acquiring a backend frame.

Retained text widgets center the font baseline inside their cells, and each line receives a small
vertical pad so glyphs do not touch widget borders. `TextBlock` supports wrapped multi-line content
while preserving outer padding without inserting additional spacing between lines.

## Coordinates and clipping

Three coordinate concepts are deliberately separate:

1. Widget methods and `Painter` primitives use local coordinates.
2. `Painter` translates recorded geometry into screen coordinates using its
   fixed origin.
3. Context's private executor consumes screen-space operations and clips them to the current
   viewport immediately before backend submission.

Every display-list operation owns its effective screen-space clip. A clip is
not mutable renderer state and there is no public clip stack.

Surface layout supplies the initial clip for each retained root before widget-local painting
begins. Independent windows receive the drawable viewport. A structural child inherits its direct
parent's committed surface clip and, when that parent was constructed with
`ChildWindowClip::Content`, intersects it with the parent's application-body rectangle. Nested
families therefore accumulate clipping ancestors through the same ordinary per-operation clips;
the renderer and backend require no child-window concept. Manager-owned backgrounds, menus, chrome,
and pointer hit testing consume the same committed surface boundary.

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

### Supported coordinate domain

The portable rendering target is a positive drawable no larger than
8192x8192 pixels. Screen-space geometry may extend up to four maximum
drawable spans beyond each viewport edge, giving a supported logical edge
range of `-32768..=40960`. Widget layout, scrolling, and custom painting must keep translated
rectangle edges and accumulated
positions within that range.

This bound leaves substantial `i32` headroom while covering ordinary
off-screen layout and clipping. Values outside the range are unsupported
rendering input even when their individual components fit in `Recti`.
`FrameInfo::try_new` validates positive dimensions but does not enforce the
portable 8192-pixel limit; a larger surface may work on a particular backend,
but it is not a cross-backend guarantee.

The renderer deliberately retains `Recti` and `Vec2i` rather than maintaining
a second large-coordinate geometry model. Empty and negative-extent
rectangles continue to produce no geometry.

## Internal display-list ownership and reuse

The crate-owned display list contains all data required after recording, including strings, typed
custom-render registry keys, and solid geometry. No operation borrows widget or container memory.

The list is designed to be reused:

- recording appends into retained operation and geometry allocations;
- Context submission preflights resource keys, then drains operations directly
  from the list through one frame-owned executor;
- the outer submission boundary clears operations and solid geometry after
  success or a returned error while retaining their allocations;
- glyph quads are submitted as the atlas visits them, without renderer-side
  glyph scratch;
- final triangle-clipping scratch is retained between frames.

The frame-owned executor handles ordinary drawing, external-texture barriers,
and custom-render barriers in one painter-order loop. Its backend frame is
finalized when the executor leaves scope.

Solid line and polygon tessellation appends into `SolidGeometry` workspace
rather than returning temporary vectors in the rendering path. Final clipped
vertices likewise append into renderer-owned scratch storage.

## Images and textures

The atlas is immutable after construction and contains fonts plus named bitmap
icons addressed by atlas-owned `IconId` capabilities. `ThemeIcons::from_atlas`
resolves the semantic icons used by built-in components, while applications may
look up other named icons. Like fonts, foreign icon IDs are rejected during
renderer preflight before backend acquisition.
General images are external textures owned through `Context` and addressed
directly by `TextureId`; `ImageSource` describes upload input, but there is no
persistent image-resource wrapper or atlas-slot path.

Image-bearing widgets accept `TextureId`. `WidgetFillOption` controls which
interaction states draw the widget's filled background; use
`WidgetFillOption::ALL` to retain the normal, hover, and click fills around an
image.

Normal applications should manage texture lifetime through `Context`:

```rust
use microui_redux::{
    prelude::{Context, RendererBackend, TextureId},
};

fn upload_image<B: RendererBackend>(
    context: &mut Context<B>,
    width: i32,
    height: i32,
    rgba: &[u8],
) -> Result<TextureId, String> {
    context.try_load_image_rgba(width, height, rgba)
}
```

Use `Context::free_image` when the texture is no longer needed.
`Context::load_image_from` accepts `ImageSource`, while
`Context::load_image_rgba` is the panicking convenience form: it validates the
RGBA data and panics on invalid dimensions, invalid byte length, or backend
upload failure.

Texture dimensions and RGBA byte length are validated before a texture ID is
consumed. Failed backend creation does not leave a tracked texture behind.
`TextureId` carries its renderer identity, renderer-local allocation slot, and
immutable dimensions. Equality and hashing cover all three, so matching local
slots from separate contexts remain distinct capabilities. The private executor
tracks complete live handles without storing a second copy of their dimensions.
`RendererBackend::create_texture` receives only the ID and pixel bytes; it uses
`TextureId::size` rather than accepting contradictory dimension arguments.
Repeated `free_image` calls for the same handle are debug-asserted
as lifecycle mistakes and become idempotent no-ops in release builds; they
notify the backend only once. Dropping `Context` destroys every external
texture it still owns. Applications inspect atlas metadata through
`Context::atlas`; they never need access to the crate-owned executor.

## Custom-render callbacks

Portable UI drawing belongs in `Painter`. A registered typed callback is the
escape hatch for backend-specific work such as a 3D viewport.

```rust
use microui_redux::{
    prelude::{Context, CustomRenderArgs, CustomRenderHandle},
    render::{CustomRenderRegistryError, RendererBackend},
};

fn register_custom<B: RendererBackend>(
    context: &mut Context<B>,
) -> Result<CustomRenderHandle<B>, CustomRenderRegistryError> {
    context.register_custom_renderer(
        |_frame: &mut B::Frame<'_>, args: CustomRenderArgs| {
            let dimensions = args.dimensions;
            let content_area = args.content_area;
            let visible_area = args.view;
            let _ = (dimensions, content_area, visible_area);
        },
    )
}
```

The callback receives:

- `content_area`: the custom node's full content rectangle;
- `view`: the authoritative final visible rectangle after operation, content-area, viewport, and
  retained clipping;
- `dimensions`: the validated active-frame dimensions.

The executor skips the callback when that intersection is empty; callbacks must not intersect
`content_area` and `view` again. The callback deliberately receives no input. Interaction remains
in widget update.
The first callback argument is `&mut B::Frame<'_>`, so backend-specific methods
can be called without raw backend exposure or a second frame acquisition.
`Node::custom_render` checks the backend-typed `CustomRenderHandle<B>` and stores
only its private backend-neutral registry key. Context preflight validates that
key before acquiring a backend frame.

## Implementing a backend

A backend implements `RendererBackend` and consumes `render::Vertex`:

```rust
use microui_redux::{
    prelude::{AtlasHandle, Context, TextureId},
    render::{FrameError, FrameInfo, RendererBackend, RendererFrame, Vertex},
};

struct Backend {
    atlas: AtlasHandle,
}

#[must_use = "dropping the frame finalizes it"]
struct BackendFrame<'a>(&'a mut Backend);

impl RendererFrame for BackendFrame<'_> {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {}
    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}
    fn flush(&mut self) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

impl RendererBackend for Backend {
    type Frame<'a> = BackendFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(BackendFrame(self))
    }

    fn create_texture(
        &mut self,
        _id: TextureId,
        _pixels: &[u8],
    ) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

fn install_backend(backend: Backend) -> Context<Backend> {
    // Context takes unique ownership and supplies all display-list execution and resource tracking.
    Context::new(backend)
}
```

Backend rules:

- Every constructible `AtlasHandle` already contains a validated opaque white
  rendering tile named `white`; the renderer resolves its atlas-owned `IconId`
  by name. A backend used by `Context` additionally returns an atlas with the
  `body` font and all lowercase semantic names consumed by
  `ThemeIcons::from_atlas`.
- `frame` acquires all fallible native frame resources and returns a value
  that exclusively borrows the backend.
- `push_quad` and `push_triangle` receive final atlas-backed
  geometry and may batch it.
- `flush` must submit outstanding batched work without ending the frame.
- `draw_texture` receives a pre-clipped quad. Backends that batch atlas work
  must preserve painter order when switching textures.
- concrete frame `Drop` performs best-effort, non-panicking final flush,
  submission, and presentation.
- `create_texture` obtains immutable dimensions from `TextureId::size` and must
  return an error without retaining the ID when creation fails.
- `destroy_texture` releases the matching backend resource.
- the backend does not calculate UI clipping or inspect input.

`Context::new` uniquely owns the backend behind its private executor. Safe Rust
therefore prevents persistent resource mutation or another frame acquisition
while a backend frame exists. Applications implement `RendererBackend` and
`RendererFrame`; they do not construct or replace the executor itself.

Context, backend frames, and custom-render callbacks remain on their owning
thread. `RendererBackend` and `CustomRender` intentionally have
no `Send` or `Sync` bounds, and the retained tree and immutable atlas use
single-threaded shared ownership. Cross-thread application work should produce
owned results and deliver them to Context before `Context::frame`; callbacks
then execute synchronously while Context's executor interprets that frame's
display list.

Tests that need to inspect backend work keep a separate `Rc<RefCell<_>>`
recording log. They do not clone, lock, or expose the backend itself.

## Performance validation

The render microbenchmark measures recording and execution together after one
warm-up frame has populated reusable storage:

```bash
cargo test --release render_performance_baseline \
  -- --ignored --nocapture --test-threads=1
```

It uses a counting global allocator and a counter-only backend. The reported
frame count covers private executor submission, excluding setup and texture upload.
Allocation counts include both Painter
recording and executor work. Timings include the allocation counter's
atomic instrumentation and are intended as a reproducible regression baseline,
not as GPU frame timings.

The following baseline was recorded on 2026-07-24 with Rust 1.97.1 in the
crate's release profile on an Intel Core Ultra 7 155H:

| Scenario | Operations | Solid triangles | Backend frames | Allocations | Bytes | Submitted vertices | Time/frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 4,096 rectangles | 4,096 | 0 | 1 | 0 | 0 | 16,384 | 332.2 µs |
| 4,096 glyphs | 1 | 0 | 1 | 1 | 4,096 | 16,384 | 188.8 µs |
| 2,048 rectangles + concave polygons | 4,096 | 6,144 | 1 | 0 | 0 | 26,624 | 1,751.5 µs |
| 32 nested clips + 4,096 attempted rectangles | 3,844 | 0 | 1 | 0 | 0 | 15,376 | 262.7 µs |
| 2,048 atlas/external texture pairs | 4,096 | 0 | 1 | 0 | 0 | 16,384 | 348.5 µs |
| 4,096 rectangles + 8 custom barriers | 4,104 | 0 | 1 | 0 | 0 | 16,384 | 277.9 µs |

The nested-clip scenario records 3,844 operations because Painter rejects the
rectangles fully outside the final effective clip. Text intentionally allocates
one owned `String` snapshot for its single display-list operation; allocation
count does not scale with its 4,096 glyphs. The benchmark asserts the exact
aggregate allocation and backend-frame counts across 200 frames, so integer averaging
cannot conceal an intermittent growth allocation.

### Historical comparison

No wall-clock or allocator benchmark was checked in before the rendering
migration, so a trustworthy historical timing number cannot be reconstructed.
The pre-migration implementation at commit `8ecdb95` can still be compared
structurally:

| Property | Pre-migration implementation | Current implementation |
| --- | --- | --- |
| Backend access | One renderer scope per normal replay segment | One exclusively borrowed frame for the complete submission |
| Custom barriers | One normal scope per segment plus two flush scopes per barrier | Typed callback on the active frame after one executor-owned flush |
| Replay clipping | Allocated a `Vec` clip stack for every normal replay segment | No replay clip stack; each operation owns its effective clip |
| Concave polygon recording | Allocated simplified-point and index vectors per polygon | Reuses `SolidGeometry` polygon and triangle storage; zero warmed allocations |
| Glyph expansion | Reused Canvas-owned rectangle scratch | Reuses executor-owned rectangle scratch |
| Custom-widget rectangles | Batched as two solid triangles, submitting six vertices per rectangle | Remain semantic atlas quads, submitting four vertices per rectangle |
| Recording storage | Reused command and flat vertex vectors | Reuses operation and strongly typed triangle vectors |

The new per-operation clip adds fixed display-list metadata, and semantic
custom-widget rectangles can produce more operation records than the old
triangle-batch command. In return, those rectangles submit one quad instead of
two triangles, clipping has one authority, and the measured warmed path has no
per-rectangle or per-triangle allocation. The representative results show no
meaningful regression that requires a follow-up.

The performance tests also enforce the architectural constraints: normal
operations and typed custom callbacks remain inside one backend frame, final
clipping stays in the private executor, and no state commands or
recording-time software triangle clipping are introduced for benchmark gains.

## Working examples

- `examples/retained-custom-drawing.rs` implements a retained custom widget using
  `WidgetPaintCtx::painter`.
- `examples/texture-clipping-smoke.rs` exercises low-level display-list execution, texture upload,
  clipping, and final vertices.
- `examples/common/glow_renderer.rs`,
  `examples/common/vulkan_renderer.rs`, and
  `examples/common/wgpu_renderer.rs` are complete backend implementations.
- `examples/demo-full.rs` demonstrates retained UI, custom painting, external
  textures, and custom backend rendering together.

The rendering API is a clean break from the former layout. This guide and the
compiling examples above define the supported integration paths.
