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
//! Retained custom drawing example.
//!
//! This example demonstrates implementing a custom widget that emits widget-local triangle
//! geometry through `WidgetCtx::graphics`.

use microui_redux::{backend::Vertex, prelude::*, AtlasSource};

const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

struct NoopRenderer {
    atlas: AtlasHandle,
}

impl Renderer for NoopRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn begin(&mut self, _width: i32, _height: i32, _clr: Color) {}

    fn push_quad_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex, _v3: &Vertex) {}

    fn push_triangle_vertices(&mut self, _v0: &Vertex, _v1: &Vertex, _v2: &Vertex) {}

    fn flush(&mut self) {}

    fn end(&mut self) {}

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}

    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

#[derive(Clone)]
struct RetainedPaint {
    opt: WidgetOption,
}

impl RetainedPaint {
    fn new() -> Self {
        Self { opt: WidgetOption::NONE }
    }
}

impl Widget for RetainedPaint {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        ScrollBehavior::NONE
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        Dimensioni::new(96, 48)
    }

    fn update(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        ctx.graphics(|graphics| {
            let bounds = graphics.local_rect();
            let fill = if control.hovered { color(54, 116, 155, 255) } else { color(42, 70, 92, 255) };
            graphics.draw_rect(bounds, fill);
            graphics.draw_box(bounds, color(230, 236, 240, 255));
            graphics.with_clip(rect(4, 4, bounds.width - 8, bounds.height - 8), |graphics| {
                graphics.stroke_line(
                    Vec2f::new(8.0, bounds.height as f32 - 10.0),
                    Vec2f::new(bounds.width as f32 - 8.0, 10.0),
                    3.0,
                    color(255, 202, 72, 255),
                );
            });
        });
    }
}

fn make_atlas() -> AtlasHandle {
    let pixels = [0xFF, 0xFF, 0xFF, 0xFF];
    let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, rect(0, 0, 1, 1))).collect();
    let entries = vec![
        (
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: rect(0, 0, 1, 1),
            },
        ),
        (
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: rect(0, 0, 1, 1),
            },
        ),
    ];
    let fonts = vec![(
        "default",
        FontEntry {
            line_size: 10,
            baseline: 8,
            font_size: 10,
            entries: &entries,
        },
    )];
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
        slots: &[],
    };
    AtlasHandle::from(&source)
}

fn main() {
    let renderer = RendererHandle::new(NoopRenderer { atlas: make_atlas() });
    let mut ctx = Context::new(renderer, Dimensioni::new(160, 100));
    let paint = widget_handle(RetainedPaint::new());
    let tree = WidgetTreeBuilder::build(move |tree| {
        tree.widget(&paint);
    });
    ctx.create_window("retained custom drawing", rect(12, 12, 132, 84), tree);

    ctx.begin_render_frame(160, 100, color(18, 20, 22, 255));
    ctx.update_ui();
    ctx.end_render_frame();
}
