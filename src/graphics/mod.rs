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
// -----------------------------------------------------------------------------
//! Widget-local 2D geometry recording.
//!
//! The rest of the UI draw path is largely rectangle-oriented and mostly works in container
//! coordinates. This module adds a dedicated widget-local geometry builder that:
//! - accepts points relative to the current widget origin,
//! - forwards widget-local clips onto the shared draw-context clip stack,
//! - retains the effective clip beside each finalized triangle range,
//! - leaves final software clipping to Canvas execution.
//!
//! The actual triangle vertices live in the container-owned arena held by `DrawCtx`, so individual
//! widgets do not allocate their own per-batch vertex vectors. A clip change finalizes the current
//! range because every retained operation owns one effective clip.

use crate::draw_context::{clip_relation, intersect_clip_rect, DrawCtx};
use crate::render::Vertex;
use crate::render::geometry::{SolidGeometry, SolidTriangle, translate_rect};
#[cfg(test)]
use crate::render::geometry::GEOM_EPS;
use crate::render_command::CommandKind;
use crate::text_layout::control_text_position_with_font;
use crate::*;
use std::rc::Rc;

/// Widget-local 2D geometry builder.
///
/// Coordinates passed to this builder are local to the widget rectangle that created it:
/// `(0, 0)` is the widget's top-left corner, while `(width, height)` is the bottom-right corner.
/// Nested clips are also widget-local and are pushed onto the shared draw-context clip stack after
/// being translated into screen space, so a widget-local clip can only reduce visibility and can
/// never expand beyond the area the widget already owns.
///
/// The builder tessellates higher-level shapes immediately but does not clip their triangles.
/// Canvas applies the retained effective clip exactly once during final execution.
pub struct Graphics<'a, 'b> {
    /// Shared draw context receiving commands and triangle vertices.
    draw: &'a mut DrawCtx<'b>,
    /// Reusable typed geometry and private polygon workspace.
    solid_geometry: &'a mut SolidGeometry,
    /// Screen-space widget rectangle that anchors local coordinates.
    widget_rect: Recti,
    /// Floating-point widget origin used when translating triangle vertices.
    widget_origin: Vec2f,
    /// Clip-stack depth that existed before this builder was created.
    clip_base_depth: usize,
    /// First vertex of the current unflushed triangle batch.
    triangle_batch_start: usize,
    /// Number of vertices in the current unflushed triangle batch.
    triangle_batch_count: usize,
}

impl<'a, 'b> Graphics<'a, 'b> {
    /// Creates a graphics builder clipped to the widget rectangle.
    pub(crate) fn new(draw: &'a mut DrawCtx<'b>, solid_geometry: &'a mut SolidGeometry, widget_rect: Recti) -> Self {
        Self::new_with_clip_root(draw, solid_geometry, widget_rect, widget_rect)
    }

    /// Creates a graphics builder with an explicit screen-space clip root.
    ///
    /// Public widget-local graphics keep their clip root inside the widget bounds. Internal widget
    /// paint adapters can supply a wider clip root to preserve legacy frame overflow behavior while
    /// still reusing the same local-coordinate drawing code.
    pub(crate) fn new_with_clip_root(draw: &'a mut DrawCtx<'b>, solid_geometry: &'a mut SolidGeometry, widget_rect: Recti, clip_root: Recti) -> Self {
        // The builder records how deep the shared clip stack was before it started, then pushes one
        // root clip in screen space. All later widget-local clip changes are translated onto that
        // same stack, and drop restores the previous depth so the outer traversal state is intact.
        let clip_base_depth = draw.clip_depth();
        draw.push_clip_rect(clip_root);
        let triangle_batch_start = draw.triangle_vertex_count();

        Self {
            draw,
            solid_geometry,
            widget_rect,
            widget_origin: Vec2f::new(widget_rect.x as f32, widget_rect.y as f32),
            clip_base_depth,
            triangle_batch_start,
            triangle_batch_count: 0,
        }
    }

    /// Returns the widget-local rectangle available to this graphics builder.
    ///
    /// This is the widget's full layout rect expressed in local coordinates, regardless of parent
    /// clipping. Use [`Graphics::current_clip_rect`] when the visible area matters.
    pub fn local_rect(&self) -> Recti {
        Recti::new(0, 0, self.widget_rect.width, self.widget_rect.height)
    }

    /// Returns the current widget-local clip rectangle.
    ///
    /// The returned rect is derived from the shared draw-context clip stack. It is therefore
    /// already intersected with the widget root and all earlier local clip scopes.
    pub fn current_clip_rect(&self) -> Recti {
        self.screen_to_local_rect(self.draw.current_clip_rect())
    }

    /// Narrows the current clip by intersecting it with `rect`.
    ///
    /// The clip is expressed in widget-local coordinates, translated into screen space, and pushed
    /// onto the shared draw-context stack. Because `DrawCtx::push_clip_rect` intersects against
    /// the current top, this can never expand the visible area.
    pub fn push_clip_rect(&mut self, rect: Recti) {
        self.flush_batch();
        self.draw.push_clip_rect(self.local_to_screen_rect(rect));
    }

    /// Replaces the current clip with an intersection against `rect`.
    ///
    /// Unlike `push_clip_rect`, this keeps the current stack depth. The replacement is still
    /// monotonic: it intersects with the existing top clip instead of replacing it wholesale.
    pub fn set_clip_rect(&mut self, rect: Recti) {
        self.flush_batch();
        let clip = intersect_clip_rect(self.draw.current_clip_rect(), self.local_to_screen_rect(rect));
        self.draw.replace_current_clip_rect(clip);
    }

    /// Restores the previous widget-local clip rectangle.
    pub fn pop_clip_rect(&mut self) {
        if self.draw.clip_depth() > self.clip_base_depth + 1 {
            self.flush_batch();
            self.draw.pop_clip_rect();
        }
    }

    /// Executes `f` with an additional widget-local clip applied.
    pub fn with_clip<F: FnOnce(&mut Self)>(&mut self, rect: Recti, f: F) {
        self.push_clip_rect(rect);
        f(self);
        self.pop_clip_rect();
    }

    /// Fills a solid axis-aligned rectangle in widget-local coordinates.
    ///
    /// Rectangles are routed through the same triangle path as every other filled primitive so the
    /// widget paint stack only has one geometry implementation to maintain.
    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        if rect.width <= 0 || rect.height <= 0 || color.a == 0 {
            return;
        }

        let x0 = rect.x as f32;
        let y0 = rect.y as f32;
        let x1 = (rect.x + rect.width) as f32;
        let y1 = (rect.y + rect.height) as f32;
        let rgba = color4b(color.r, color.g, color.b, color.a);
        let p0 = Vec2f::new(x0, y0);
        let p1 = Vec2f::new(x1, y0);
        let p2 = Vec2f::new(x1, y1);
        let p3 = Vec2f::new(x0, y1);
        self.push_triangle_local(p0, p1, p2, rgba);
        self.push_triangle_local(p0, p2, p3, rgba);
    }

    /// Draws a 1-pixel outline around `rect`.
    ///
    /// The outline is decomposed into four filled edge rectangles so it stays on the same clipped
    /// triangle path as every other solid primitive.
    pub fn draw_box(&mut self, rect: Recti, color: Color) {
        self.draw_rect(Recti::new(rect.x + 1, rect.y, rect.width - 2, 1), color);
        self.draw_rect(Recti::new(rect.x + 1, rect.y + rect.height - 1, rect.width - 2, 1), color);
        self.draw_rect(Recti::new(rect.x, rect.y, 1, rect.height), color);
        self.draw_rect(Recti::new(rect.x + rect.width - 1, rect.y, 1, rect.height), color);
    }

    /// Draws text using widget-local coordinates for the glyph origin.
    ///
    /// Text itself still reuses the existing retained text command, but the graphics builder owns
    /// the local-to-screen translation and the clip-state wrapping so widgets no longer have to
    /// decide which paint API to use.
    pub fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        if text.is_empty() || color.a == 0 {
            return;
        }

        let size = self.draw.atlas().get_text_size(font, text);
        let bounds = Recti::new(pos.x, pos.y, size.width, size.height);
        let screen_pos = self.local_to_screen_pos(pos);
        let text = text.to_string();
        self.emit_clipped_command(bounds, CommandKind::Text { text, pos: screen_pos, color, font });
    }

    /// Draws one icon rectangle using widget-local coordinates.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let screen_rect = self.local_to_screen_rect(rect);
        self.emit_clipped_command(rect, CommandKind::Icon { id, rect: screen_rect, color });
    }

    /// Draws one image rectangle using widget-local coordinates.
    pub fn draw_image(&mut self, image: Image, rect: Recti, color: Color) {
        let screen_rect = self.local_to_screen_rect(rect);
        self.emit_clipped_command(rect, CommandKind::Image { image, rect: screen_rect, color });
    }

    /// Re-renders a slot and then draws it using widget-local coordinates.
    pub fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, payload: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let screen_rect = self.local_to_screen_rect(rect);
        self.emit_clipped_command(rect, CommandKind::SlotRedraw { id, rect: screen_rect, color, payload });
    }

    /// Draws one framed control using the current style colors.
    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let color = self.draw.style().colors[colorid as usize];
        self.draw_rect(rect, color);
        if let Some(border) = self.draw.style().frame_border_color(colorid) {
            self.draw_box(expand_rect(rect, 1), border);
        }
    }

    /// Draws one widget frame using the same focus/hover color promotion as the legacy widget
    /// helpers.
    pub fn draw_widget_frame(&mut self, focused: bool, hovered: bool, rect: Recti, mut colorid: ControlColor, opt: WidgetOption) {
        if opt.intersects(WidgetOption::NO_FRAME) {
            return;
        }
        if focused {
            colorid.focus();
        } else if hovered {
            colorid.hover();
        }
        self.draw_frame(rect, colorid);
    }

    /// Draws centered or aligned control text inside `rect`.
    ///
    /// This reuses the shared control-text positioning helper from `DrawCtx` so widget and
    /// container labels stay visually identical even though widgets now paint through `Graphics`.
    pub fn draw_control_text(&mut self, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.draw_control_text_with_font(self.draw.style().font, text, rect, colorid, opt);
    }

    /// Draws centered or aligned control text using an explicit font.
    pub fn draw_control_text_with_font(&mut self, font: FontId, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let (font, color, pos) = {
            let style = self.draw.style();
            let atlas = self.draw.atlas();
            (
                font,
                style.colors[colorid as usize],
                control_text_position_with_font(style, atlas, font, text, rect, opt),
            )
        };
        self.push_clip_rect(rect);
        self.draw_text(font, text, pos, color);
        self.pop_clip_rect();
    }

    /// Strokes one solid line segment with the provided width.
    ///
    /// The stroke is tessellated into two triangles instead of relying on platform line
    /// primitives. That keeps behavior predictable across backends and makes rectangular clipping
    /// behave the same way as filled polygon rendering.
    pub fn stroke_line(&mut self, a: Vec2f, b: Vec2f, width: f32, color: Color) {
        if width <= 0.0 || color.a == 0 {
            return;
        }
        let rgba = color4b(color.r, color.g, color.b, color.a);
        self.solid_geometry.clear();
        let _ = self.solid_geometry.append_line(a, b, width, rgba, Vec2f::new(0.0, 0.0));
        self.push_solid_geometry_local();
    }

    /// Fills a simple polygon described in widget-local coordinates.
    ///
    /// Convex polygons take the fast triangle-fan path. Concave simple polygons fall back to a
    /// compact ear-clipping path owned by `SolidGeometry`. Self-intersecting polygons are
    /// intentionally unsupported.
    pub fn fill_polygon(&mut self, points: &[Vec2f], color: Color) {
        if points.len() < 3 || color.a == 0 {
            return;
        }
        let rgba = color4b(color.r, color.g, color.b, color.a);
        self.solid_geometry.clear();
        let _ = self.solid_geometry.append_polygon(points, rgba, Vec2f::new(0.0, 0.0));
        self.push_solid_geometry_local();
    }

    /// Clips and records every triangle currently held by the reusable geometry object.
    fn push_solid_geometry_local(&mut self) {
        let triangle_count = self.solid_geometry.triangles().len();
        for index in 0..triangle_count {
            // Copy one triangle so the geometry borrow ends before recording mutably borrows the
            // rest of the graphics builder.
            let triangle = self.solid_geometry.triangles()[index];
            self.push_solid_triangle_local(triangle);
        }
    }

    /// Returns the current screen-space clip consumed by retained non-triangle commands.
    fn current_screen_clip_rect(&self) -> Recti {
        self.draw.current_clip_rect()
    }

    /// Converts a widget-local integer position into screen-space coordinates.
    fn local_to_screen_pos(&self, pos: Vec2i) -> Vec2i {
        pos + Vec2i::new(self.widget_rect.x, self.widget_rect.y)
    }

    /// Converts a widget-local integer rectangle into a screen-space rectangle.
    fn local_to_screen_rect(&self, rect: Recti) -> Recti {
        translate_rect(rect, Vec2i::new(self.widget_rect.x, self.widget_rect.y))
    }

    /// Converts a screen-space rectangle into widget-local coordinates.
    ///
    /// This expresses the shared screen clip in coordinates meaningful to widget code.
    fn screen_to_local_rect(&self, rect: Recti) -> Recti {
        translate_rect(rect, Vec2i::new(-self.widget_rect.x, -self.widget_rect.y))
    }

    /// Flushes pending triangles, then emits a clipped non-triangle command.
    ///
    /// This keeps ordering correct when widgets mix text, images, and solid geometry inside one
    /// graphics builder.
    fn emit_clipped_command(&mut self, bounds_local: Recti, kind: CommandKind) {
        self.flush_batch();
        let clip = self.current_screen_clip_rect();
        let bounds = self.local_to_screen_rect(bounds_local);
        if clip_relation(bounds, clip) != Clip::All {
            self.draw.push_command_with_clip(clip, kind);
        }
    }

    /// Appends one unclipped widget-local triangle into the shared vertex arena.
    fn push_triangle_local(&mut self, a: Vec2f, b: Vec2f, c: Vec2f, color: Color4b) {
        let clip = self.current_screen_clip_rect();
        if clip.width <= 0 || clip.height <= 0 {
            return;
        }
        let widget_origin = self.widget_origin;
        let vertices = self.draw.triangle_vertices_mut();
        vertices.extend([
            Vertex::new(a + widget_origin, Vec2f::default(), color),
            Vertex::new(b + widget_origin, Vec2f::default(), color),
            Vertex::new(c + widget_origin, Vec2f::default(), color),
        ]);
        self.triangle_batch_count += 3;
    }

    /// Appends one strongly typed solid triangle through the transitional recorder.
    fn push_solid_triangle_local(&mut self, triangle: SolidTriangle) {
        let [a, b, c] = *triangle.vertices();
        self.push_triangle_local(a.position, b.position, c.position, a.color);
    }

    /// Finalizes the current triangle batch as one retained command.
    ///
    /// The range captures the active effective clip for final Canvas execution.
    fn flush_batch(&mut self) {
        if self.triangle_batch_count == 0 {
            return;
        }

        self.draw.push_command(CommandKind::Triangle {
            vertex_start: self.triangle_batch_start,
            vertex_count: self.triangle_batch_count,
        });
        self.triangle_batch_start = self.draw.triangle_vertex_count();
        self.triangle_batch_count = 0;
    }
}

impl<'a, 'b> Drop for Graphics<'a, 'b> {
    fn drop(&mut self) {
        self.flush_batch();
        self.draw.pop_clip_rect_to(self.clip_base_depth);
    }
}

#[cfg(test)]
mod tests;
