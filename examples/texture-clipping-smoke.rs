//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
//! Texture clipping smoke test example.
//!
//! This standalone example validates that external texture drawing respects UI clipping.

use microui_redux::{prelude::*, render::Vertex};
use std::{cell::RefCell, rc::Rc};

const THEME_ICON_NAMES: [&str; 9] = [
    "white",
    "close",
    "expand",
    "collapse",
    "check",
    "expand_down",
    "open_folder",
    "closed_folder",
    "file",
];

enum SmokeEvent {
    AtlasBatch { quads: usize },
    Texture { id: TextureId, vertices: [Vertex; 4] },
}

impl SmokeEvent {
    fn name(&self) -> &'static str {
        match self {
            Self::AtlasBatch { .. } => "atlas batch",
            Self::Texture { .. } => "texture draw",
        }
    }
}

struct SmokeRenderer {
    atlas: AtlasHandle,
    pending_quads: usize,
    events: Rc<RefCell<Vec<SmokeEvent>>>,
    textures: Vec<TextureId>,
}

impl SmokeRenderer {
    fn new(atlas: AtlasHandle, events: Rc<RefCell<Vec<SmokeEvent>>>) -> Self {
        Self {
            atlas,
            pending_quads: 0,
            events,
            textures: Vec::new(),
        }
    }

    fn flush_pending_quads(&mut self) {
        if self.pending_quads > 0 {
            self.events.borrow_mut().push(SmokeEvent::AtlasBatch { quads: self.pending_quads });
            self.pending_quads = 0;
        }
    }
}

#[must_use]
struct SmokeFrame<'a> {
    backend: &'a mut SmokeRenderer,
}

impl RendererFrame for SmokeFrame<'_> {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {
        self.backend.pending_quads += 1;
    }

    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {
        self.backend.pending_quads += 1;
    }

    fn flush(&mut self) {
        self.backend.flush_pending_quads();
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        if !self.backend.textures.contains(&id) {
            return;
        }
        self.backend.flush_pending_quads();
        self.backend.events.borrow_mut().push(SmokeEvent::Texture { id, vertices });
    }
}

impl Drop for SmokeFrame<'_> {
    fn drop(&mut self) {
        self.backend.flush_pending_quads();
    }
}

impl RendererBackend for SmokeRenderer {
    type Frame<'a> = SmokeFrame<'a>;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), AtlasUploadError> {
        // This CPU smoke backend stores no GPU atlas object, so publishing the handle is atomic.
        self.atlas = atlas;
        Ok(())
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        self.pending_quads = 0;
        self.events.borrow_mut().clear();
        Ok(SmokeFrame { backend: self })
    }

    fn create_texture(&mut self, id: TextureId, _pixels: &[u8]) -> Result<(), TextureError> {
        self.textures.push(id);
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.textures.retain(|texture| *texture != id);
    }
}

fn make_smoke_atlas() -> AtlasHandle {
    let pixels = [0xFF; 16];
    let icons: Vec<_> = THEME_ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
    let entries = [(
        '_',
        CharEntry {
            offset: Vec2i::new(0, 0),
            advance: Vec2i::new(1, 0),
            rect: Recti::new(0, 0, 1, 1),
        },
    )];
    let fonts = [(
        "body",
        FontEntry {
            line_size: 1,
            baseline: 1,
            font_size: 1,
            entries: &entries,
        },
    )];
    let source = AtlasSource {
        width: 2,
        height: 2,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
    };
    // The smoke test keeps its compact embedded source while still accepting only a completely
    // validated runtime atlas.
    AtlasHandle::try_from(&source).expect("texture-clipping smoke atlas must satisfy the complete atlas contract")
}

fn assert_vec2f_eq(actual: Vec2f, expected: Vec2f) {
    assert!((actual.x - expected.x).abs() < 1.0e-6, "expected x {}, got {}", expected.x, actual.x);
    assert!((actual.y - expected.y).abs() < 1.0e-6, "expected y {}, got {}", expected.y, actual.y);
}

struct TextureClippingProbe {
    texture: TextureId,
    /// Atlas-owned white tile recorded on both sides of the external texture draw.
    white_icon: IconId,
    options: WidgetOption,
    screen_content: Rc<RefCell<Option<Recti>>>,
}

struct TextureClippingParameters {
    texture: TextureId,
    /// Concrete icon capability minted by the renderer's atlas before widget construction.
    white_icon: IconId,
    screen_content: Rc<RefCell<Option<Recti>>>,
}

impl WidgetParameters for TextureClippingParameters {}

struct TextureClippingBuilder;

impl WidgetBuilder for TextureClippingBuilder {
    type Parameters = TextureClippingParameters;
    type W = TextureClippingProbe;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        TextureClippingProbe {
            texture: parameters.texture,
            white_icon: parameters.white_icon,
            options: WidgetOption::NO_INTERACT,
            screen_content: parameters.screen_content,
        }
    }
}

impl Widget for TextureClippingProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.options
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _events: Option<&UiInputEvent>) {}

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        *self.screen_content.borrow_mut() = Some(ctx.screen_content_rect());
        let white = color(255, 255, 255, 255);
        let mut painter = ctx.painter();
        painter.icon(self.white_icon, rect(0, 0, 4, 4), white);
        painter.with_clip(rect(10, 12, 8, 6), |painter| {
            painter.image(self.texture, rect(6, 9, 16, 12), white);
        });
        painter.icon(self.white_icon, rect(30, 0, 4, 4), white);
    }
}

impl LeafWidget for TextureClippingProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(64, 64)
    }
}

fn main() -> Result<(), String> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let atlas = make_smoke_atlas();
    let white_icon = atlas.white_icon();
    let backend = SmokeRenderer::new(atlas, events.clone());
    let mut ctx = Context::<_>::new(backend);
    // This executable already normalizes its independent frame and render errors to text at the
    // process boundary; retain the typed texture failure until that same final boundary.
    let texture = ctx.try_load_image_rgba(16, 12, &[0xFF; 16 * 12 * 4]).map_err(|error| error.to_string())?;
    let screen_content = Rc::new(RefCell::new(None));
    let probe = TextureClippingBuilder::create_widget(TextureClippingParameters {
        texture,
        white_icon,
        screen_content: screen_content.clone(),
    });
    let tree = Node::widget(probe);
    let window = ctx.ui().create_window(Window::new("texture clipping smoke", rect(0, 0, 64, 64), tree));
    ctx.ui()
        .set_window_options(&window, WindowOption::NO_TITLE | WindowOption::NO_CLOSE | WindowOption::NO_RESIZE)
        .expect("window should remain registered");

    // Keep the window background out of the recording log so the assertions isolate the widget's
    // atlas/texture ordering while still exercising the retained public rendering path.
    let mut style = ctx.style().clone();
    style.colors[ControlColor::WindowBG as usize] = color(0, 0, 0, 0);
    ctx.set_style(style);

    let dimensions = Dimensioni::new(64, 64);
    let info = FrameInfo::try_new(dimensions, color(0, 0, 0, 255)).map_err(|error| error.to_string())?;
    ctx.update_ui(dimensions);
    ctx.frame(info).render_ui().map_err(|error| error.to_string())?;

    {
        let events = events.borrow();
        let content = screen_content.borrow().expect("the retained probe should be painted");
        assert_eq!(events.len(), 3);

        match &events[0] {
            SmokeEvent::AtlasBatch { quads } => assert_eq!(*quads, 1),
            event => panic!("expected first event to be an atlas batch, got {}", event.name()),
        }

        match &events[1] {
            SmokeEvent::Texture { id, vertices } => {
                assert_eq!(*id, texture);
                let x0 = content.x as f32 + 10.0;
                let y0 = content.y as f32 + 12.0;
                assert_vec2f_eq(vertices[0].position(), Vec2f::new(x0, y0));
                assert_vec2f_eq(vertices[1].position(), Vec2f::new(x0 + 8.0, y0));
                assert_vec2f_eq(vertices[2].position(), Vec2f::new(x0 + 8.0, y0 + 6.0));
                assert_vec2f_eq(vertices[3].position(), Vec2f::new(x0, y0 + 6.0));
                assert_vec2f_eq(vertices[0].tex_coord(), Vec2f::new(0.25, 0.25));
                assert_vec2f_eq(vertices[2].tex_coord(), Vec2f::new(0.75, 0.75));
            }
            event => panic!("expected second event to be a texture draw, got {}", event.name()),
        }

        match &events[2] {
            SmokeEvent::AtlasBatch { quads } => assert_eq!(*quads, 1),
            event => panic!("expected final event to be an atlas batch, got {}", event.name()),
        }
    }

    Ok(())
}
