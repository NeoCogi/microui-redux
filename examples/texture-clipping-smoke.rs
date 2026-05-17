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

use microui_redux::{
    color, AtlasHandle, AtlasSource, Canvas, Color, Dimensioni, Image, Recti, Renderer, RendererHandle, SourceFormat, TextureId, Vec2f, Vertex, WHITE_ICON,
};

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
    events: Vec<SmokeEvent>,
    textures: Vec<TextureId>,
}

impl SmokeRenderer {
    fn new(atlas: AtlasHandle) -> Self {
        Self {
            atlas,
            pending_quads: 0,
            events: Vec::new(),
            textures: Vec::new(),
        }
    }

    fn flush_pending_quads(&mut self) {
        if self.pending_quads > 0 {
            self.events.push(SmokeEvent::AtlasBatch { quads: self.pending_quads });
            self.pending_quads = 0;
        }
    }
}

impl Renderer for SmokeRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {
        self.pending_quads = 0;
        self.events.clear();
    }

    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {
        self.pending_quads += 1;
    }

    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {
        self.pending_quads += 1;
    }

    fn flush(&mut self) {
        self.flush_pending_quads();
    }

    fn end(&mut self) {
        self.flush();
    }

    fn create_texture(&mut self, id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        self.textures.push(id);
        Ok(())
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.textures.retain(|texture| *texture != id);
    }

    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]) {
        if !self.textures.contains(&id) {
            return;
        }

        self.flush_pending_quads();
        self.events.push(SmokeEvent::Texture { id, vertices });
    }
}

fn make_smoke_atlas() -> AtlasHandle {
    let pixels = [0xFF; 16];
    let icons = [("white", Recti::new(0, 0, 1, 1))];
    let source = AtlasSource {
        width: 2,
        height: 2,
        pixels: &pixels,
        icons: &icons,
        fonts: &[],
        format: SourceFormat::Raw,
        slots: &[],
    };
    AtlasHandle::from(&source)
}

fn assert_vec2f_eq(actual: Vec2f, expected: Vec2f) {
    assert!((actual.x - expected.x).abs() < 1.0e-6, "expected x {}, got {}", expected.x, actual.x);
    assert!((actual.y - expected.y).abs() < 1.0e-6, "expected y {}, got {}", expected.y, actual.y);
}

fn main() -> Result<(), String> {
    let renderer = RendererHandle::new(SmokeRenderer::new(make_smoke_atlas()));
    let mut canvas = Canvas::from(renderer.clone(), Dimensioni::new(64, 64));
    let texture = canvas.try_load_texture_rgba(16, 12, &[0xFF; 16 * 12 * 4])?;

    canvas.begin(64, 64, color(0, 0, 0, 255));
    canvas.draw_icon(WHITE_ICON, Recti::new(0, 0, 4, 4), color(255, 255, 255, 255));
    canvas.set_clip_rect(Recti::new(10, 12, 8, 6));
    canvas.draw_image(Image::Texture(texture), Recti::new(6, 9, 16, 12), color(255, 255, 255, 255));
    canvas.set_clip_rect(Recti::new(0, 0, 64, 64));
    canvas.draw_icon(WHITE_ICON, Recti::new(30, 0, 4, 4), color(255, 255, 255, 255));
    canvas.end();

    renderer.scope(|renderer| {
        assert_eq!(renderer.events.len(), 3);

        match &renderer.events[0] {
            SmokeEvent::AtlasBatch { quads } => assert_eq!(*quads, 1),
            event => panic!("expected first event to be an atlas batch, got {}", event.name()),
        }

        match &renderer.events[1] {
            SmokeEvent::Texture { id, vertices } => {
                assert_eq!(*id, texture);
                assert_vec2f_eq(vertices[0].position(), Vec2f::new(10.0, 12.0));
                assert_vec2f_eq(vertices[1].position(), Vec2f::new(18.0, 12.0));
                assert_vec2f_eq(vertices[2].position(), Vec2f::new(18.0, 18.0));
                assert_vec2f_eq(vertices[3].position(), Vec2f::new(10.0, 18.0));
                assert_vec2f_eq(vertices[0].tex_coord(), Vec2f::new(0.25, 0.25));
                assert_vec2f_eq(vertices[2].tex_coord(), Vec2f::new(0.75, 0.75));
            }
            event => panic!("expected second event to be a texture draw, got {}", event.name()),
        }

        match &renderer.events[2] {
            SmokeEvent::AtlasBatch { quads } => assert_eq!(*quads, 1),
            event => panic!("expected final event to be an atlas batch, got {}", event.name()),
        }
    });

    Ok(())
}
