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
//! This example demonstrates implementing a custom widget that records widget-local geometry
//! through `WidgetPaintCtx::painter`.

use microui_redux::{prelude::*, render::Vertex, AtlasSource, Constraints};
const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

struct NoopRenderer {
    atlas: AtlasHandle,
}

#[must_use]
struct NoopFrame;

impl RendererFrame for NoopFrame {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {}
    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}
    fn flush(&mut self) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

impl RendererBackend for NoopRenderer {
    type Frame<'a> = NoopFrame;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(NoopFrame)
    }

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

struct RetainedPaint {
    opt: WidgetOption,
}

struct RetainedPaintParameters;

impl WidgetParameters for RetainedPaintParameters {}

struct RetainedPaintBuilder;

impl WidgetBuilder for RetainedPaintBuilder {
    type Parameters = RetainedPaintParameters;
    type W = RetainedPaint;

    fn create_widget(_parameters: Self::Parameters) -> Self::W {
        RetainedPaint { opt: WidgetOption::NONE }
    }
}

impl Widget for RetainedPaint {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let hovered = ctx.hovered();
        let mut painter = ctx.painter();
        let bounds = painter.local_rect();
        let fill = if hovered { color(54, 116, 155, 255) } else { color(42, 70, 92, 255) };
        painter.fill_rect(bounds, fill);
        painter.stroke_rect(bounds, 1, color(230, 236, 240, 255));
        painter.with_clip(rect(4, 4, bounds.width - 8, bounds.height - 8), |painter| {
            painter.stroke_line(
                Vec2f::new(8.0, bounds.height as f32 - 10.0),
                Vec2f::new(bounds.width as f32 - 8.0, 10.0),
                3.0,
                color(255, 202, 72, 255),
            );
        });
    }
}

impl LeafWidget for RetainedPaint {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(96, 48)
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
    };
    AtlasHandle::from(&source)
}

fn main() -> Result<(), String> {
    let backend = NoopRenderer { atlas: make_atlas() };
    let mut ctx = Context::<_>::new(backend);
    let paint = RetainedPaintBuilder::create_widget(RetainedPaintParameters);
    let tree = Node::widget(paint);
    ctx.create_window(Window::new("retained custom drawing", rect(12, 12, 132, 84), tree));

    let dimensions = Dimensioni::new(160, 100);
    let info = FrameInfo::try_new(dimensions, color(18, 20, 22, 255)).map_err(|error| error.to_string())?;
    ctx.update_ui(dimensions);
    ctx.frame(info).render_ui().map_err(|error| error.to_string())?;
    Ok(())
}
