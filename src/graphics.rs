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
//! - clips tessellated triangles against the current widget-local clip rect in software,
//! - emits already-clipped retained triangle commands that do not depend on backend scissor state.
//!
//! Because clipping happens before commands are emitted, clip-stack changes no longer need to
//! fragment the retained command stream. A single graphics closure can therefore accumulate one
//! larger triangle batch even if it uses nested local clip scopes internally. The actual triangle
//! vertices live in the container-owned arena held by `DrawCtx`, so individual widgets do not
//! allocate their own per-batch vertex vectors.

use crate::container::Command;
use crate::draw_context::{intersect_clip_rect, DrawCtx};
use crate::text_layout::control_text_position_with_font;
use crate::*;
use std::rc::Rc;

mod clip;
pub(crate) use clip::clip_triangle_vertices_to_rect;

const GEOM_EPS: f32 = 1.0e-5;
const GEOM_EPS_SQ: f32 = GEOM_EPS * GEOM_EPS;

// Applies an integer translation without changing extents. Geometry code uses this to hop
// between widget-local and screen-space rectangles while keeping clip math reusable.
fn translate_rect(rect: Recti, offset: Vec2i) -> Recti {
    Recti::new(rect.x + offset.x, rect.y + offset.y, rect.width, rect.height)
}

// Applies the widget's screen-space origin to one retained triangle vertex without disturbing its
// UV or color payload.
fn translate_vertex(vertex: Vertex, offset: Vec2f) -> Vertex {
    Vertex::new(vertex.position() + offset, vertex.tex_coord(), vertex.color())
}

// Computes a conservative integer bounding box for a set of floating-point positions. The helper
// is only used by tests to verify geometric translation behavior.
#[cfg(test)]
fn rect_from_points(points: &[Vec2f]) -> Recti {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for point in points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }

    let x0 = min_x.floor() as i32;
    let y0 = min_y.floor() as i32;
    let x1 = max_x.ceil() as i32;
    let y1 = max_y.ceil() as i32;
    Recti::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
}

// Returns the signed 2D cross product. This is the core orientation predicate reused by the
// polygon cleanup, convexity tests, and point-in-triangle checks.
fn cross2(a: Vec2f, b: Vec2f) -> f32 {
    a.x * b.y - a.y * b.x
}

// Uses squared distance so degenerate/duplicate vertices can be rejected without paying for a
// square root in hot polygon preprocessing code.
fn distance_sq(a: Vec2f, b: Vec2f) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

// Computes polygon winding and area in one pass. The sign decides whether the input needs to be
// reversed before triangulation, while near-zero area indicates a degenerate polygon.
fn signed_area(points: &[Vec2f]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }

    let mut area = 0.0;
    for idx in 0..points.len() {
        let curr = points[idx];
        let next = points[(idx + 1) % points.len()];
        area += curr.x * next.y - next.x * curr.y;
    }
    area * 0.5
}

// Removes duplicate closing points, repeated neighbors, and strictly collinear vertices before
// triangulation. The fill paths rely on a compact boundary because both the convex fast path and
// ear clipping become simpler and faster once obvious redundancy is stripped out.
fn dedupe_and_simplify_polygon(points: &[Vec2f]) -> Vec<Vec2f> {
    let mut deduped = Vec::with_capacity(points.len());
    for point in points {
        if deduped.last().map(|prev| distance_sq(*prev, *point) > GEOM_EPS_SQ).unwrap_or(true) {
            deduped.push(*point);
        }
    }

    if deduped.len() > 1 && distance_sq(deduped[0], *deduped.last().unwrap()) <= GEOM_EPS_SQ {
        deduped.pop();
    }

    if deduped.len() < 3 {
        return Vec::new();
    }

    let mut simplified = Vec::with_capacity(deduped.len());
    for idx in 0..deduped.len() {
        let prev = deduped[(idx + deduped.len() - 1) % deduped.len()];
        let curr = deduped[idx];
        let next = deduped[(idx + 1) % deduped.len()];

        if distance_sq(prev, curr) <= GEOM_EPS_SQ || distance_sq(curr, next) <= GEOM_EPS_SQ {
            continue;
        }

        if cross2(curr - prev, next - curr).abs() <= GEOM_EPS {
            continue;
        }

        simplified.push(curr);
    }

    simplified
}

// Ear clipping assumes counter-clockwise winding, so "convex" means a positive turn here.
fn is_convex_ccw(prev: Vec2f, curr: Vec2f, next: Vec2f) -> bool {
    cross2(curr - prev, next - curr) > GEOM_EPS
}

// Detects the cheap convex case up front so common polygons can skip the heavier ear-clipping
// loop and fall straight into a triangle fan.
fn is_convex_polygon_ccw(points: &[Vec2f]) -> bool {
    if points.len() < 3 {
        return false;
    }

    for idx in 0..points.len() {
        let prev = points[(idx + points.len() - 1) % points.len()];
        let curr = points[idx];
        let next = points[(idx + 1) % points.len()];
        if !is_convex_ccw(prev, curr, next) {
            return false;
        }
    }
    true
}

// Uses inclusive edge tests so boundary points still count as inside. That makes the ear test
// robust against vertices that land directly on a candidate triangle edge after simplification.
fn point_in_triangle_ccw(point: Vec2f, a: Vec2f, b: Vec2f, c: Vec2f) -> bool {
    let ab = cross2(b - a, point - a);
    let bc = cross2(c - b, point - b);
    let ca = cross2(a - c, point - c);
    ab >= -GEOM_EPS && bc >= -GEOM_EPS && ca >= -GEOM_EPS
}

/// Widget-local 2D geometry builder.
///
/// Coordinates passed to this builder are local to the widget rectangle that created it:
/// `(0, 0)` is the widget's top-left corner, while `(width, height)` is the bottom-right corner.
/// Nested clips are also widget-local and are pushed onto the shared draw-context clip stack after
/// being translated into screen space, so a widget-local clip can only reduce visibility and can
/// never expand beyond the area the widget already owns.
///
/// The builder tessellates higher-level shapes into triangles immediately and software-clips every
/// triangle against the current widget-local clip rectangle before storing it. Because the emitted
/// triangles are already clipped, nested local clip scopes do not need to flush the batch or emit
/// retained clip commands.
pub struct Graphics<'a, 'b> {
    draw: &'a mut DrawCtx<'b>,
    widget_rect: Recti,
    widget_origin: Vec2f,
    white_uv: Vec2f,
    clip_base_depth: usize,
    triangle_batch_start: usize,
    triangle_batch_count: usize,
}

impl<'a, 'b> Graphics<'a, 'b> {
    pub(crate) fn new(draw: &'a mut DrawCtx<'b>, widget_rect: Recti) -> Self {
        Self::new_with_clip_root(draw, widget_rect, widget_rect)
    }

    // Public widget-local graphics keep their clip root inside the widget bounds. Internal widget
    // paint adapters can supply a wider clip root to preserve legacy frame overflow behavior while
    // still reusing the same local-coordinate drawing code.
    pub(crate) fn new_with_clip_root(draw: &'a mut DrawCtx<'b>, widget_rect: Recti, clip_root: Recti) -> Self {
        // The builder records how deep the shared clip stack was before it started, then pushes one
        // root clip in screen space. All later widget-local clip changes are translated onto that
        // same stack, and drop restores the previous depth so the outer traversal state is intact.
        let clip_base_depth = draw.clip_depth();
        draw.push_clip_rect(clip_root);
        let white_rect = draw.atlas().get_icon_rect(WHITE_ICON);
        let atlas_dim = draw.atlas().get_texture_dimension();
        let white_uv = Vec2f::new(
            (white_rect.x as f32 + white_rect.width as f32 * 0.5) / atlas_dim.width as f32,
            (white_rect.y as f32 + white_rect.height as f32 * 0.5) / atlas_dim.height as f32,
        );
        let triangle_batch_start = draw.triangle_vertex_count();

        Self {
            draw,
            widget_rect,
            widget_origin: Vec2f::new(widget_rect.x as f32, widget_rect.y as f32),
            white_uv,
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
        self.draw.push_clip_rect(self.local_to_screen_rect(rect));
    }

    /// Replaces the current clip with an intersection against `rect`.
    ///
    /// Unlike `push_clip_rect`, this keeps the current stack depth. The replacement is still
    /// monotonic: it intersects with the existing top clip instead of replacing it wholesale.
    pub fn set_clip_rect(&mut self, rect: Recti) {
        let clip = intersect_clip_rect(self.draw.current_clip_rect(), self.local_to_screen_rect(rect));
        self.draw.replace_current_clip_rect(clip);
    }

    /// Restores the previous widget-local clip rectangle.
    pub fn pop_clip_rect(&mut self) {
        if self.draw.clip_depth() > self.clip_base_depth + 1 {
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
        self.push_quad_local(Vec2f::new(x0, y0), Vec2f::new(x1, y0), Vec2f::new(x1, y1), Vec2f::new(x0, y1), color);
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
        self.emit_clipped_command(bounds, |draw| {
            draw.push_command(Command::Text { text, pos: screen_pos, color, font });
        });
    }

    /// Draws one icon rectangle using widget-local coordinates.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let screen_rect = self.local_to_screen_rect(rect);
        self.emit_clipped_command(rect, |draw| {
            draw.push_command(Command::Icon { id, rect: screen_rect, color });
        });
    }

    /// Draws one image rectangle using widget-local coordinates.
    pub fn draw_image(&mut self, image: Image, rect: Recti, color: Color) {
        let screen_rect = self.local_to_screen_rect(rect);
        self.emit_clipped_command(rect, |draw| {
            draw.push_command(Command::Image { image, rect: screen_rect, color });
        });
    }

    /// Re-renders a slot and then draws it using widget-local coordinates.
    pub fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, payload: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let screen_rect = self.local_to_screen_rect(rect);
        self.emit_clipped_command(rect, |draw| {
            draw.push_command(Command::SlotRedraw { id, rect: screen_rect, color, payload });
        });
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
        if opt.has_no_frame() {
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

        let delta = b - a;
        let len_sq = delta.x * delta.x + delta.y * delta.y;
        if len_sq <= GEOM_EPS_SQ {
            let half = width * 0.5;
            self.push_quad_local(
                Vec2f::new(a.x - half, a.y - half),
                Vec2f::new(a.x + half, a.y - half),
                Vec2f::new(a.x + half, a.y + half),
                Vec2f::new(a.x - half, a.y + half),
                color,
            );
            return;
        }

        let inv_len = len_sq.sqrt().recip();
        let normal = Vec2f::new(-delta.y * inv_len, delta.x * inv_len) * (width * 0.5);

        let p0 = a + normal;
        let p1 = b + normal;
        let p2 = b - normal;
        let p3 = a - normal;
        self.push_quad_local(p0, p1, p2, p3, color);
    }

    /// Fills a simple polygon described in widget-local coordinates.
    ///
    /// Convex polygons take the fast triangle-fan path. Concave simple polygons fall back to a
    /// compact ear-clipping triangulator implemented here to avoid pulling a heavier dependency
    /// into the core crate. Self-intersecting polygons are intentionally unsupported.
    pub fn fill_polygon(&mut self, points: &[Vec2f], color: Color) {
        if points.len() < 3 || color.a == 0 {
            return;
        }

        let mut points = dedupe_and_simplify_polygon(points);
        if points.len() < 3 {
            return;
        }

        let area = signed_area(points.as_slice());
        if area.abs() <= GEOM_EPS {
            return;
        }
        if area < 0.0 {
            points.reverse();
        }

        let rgba = color4b(color.r, color.g, color.b, color.a);

        if is_convex_polygon_ccw(points.as_slice()) {
            self.push_triangle_fan(points.as_slice(), rgba);
            return;
        }

        let mut indices: Vec<usize> = (0..points.len()).collect();
        while indices.len() > 3 {
            let mut ear_found = false;

            for idx in 0..indices.len() {
                let prev = indices[(idx + indices.len() - 1) % indices.len()];
                let curr = indices[idx];
                let next = indices[(idx + 1) % indices.len()];
                let a = points[prev];
                let b = points[curr];
                let c = points[next];

                if !is_convex_ccw(a, b, c) {
                    continue;
                }

                let mut contains_other = false;
                for probe in &indices {
                    if *probe == prev || *probe == curr || *probe == next {
                        continue;
                    }
                    if point_in_triangle_ccw(points[*probe], a, b, c) {
                        contains_other = true;
                        break;
                    }
                }
                if contains_other {
                    continue;
                }

                self.push_triangle_local(a, b, c, rgba);
                indices.remove(idx);
                ear_found = true;
                break;
            }

            if !ear_found {
                return;
            }
        }

        if indices.len() == 3 {
            self.push_triangle_local(points[indices[0]], points[indices[1]], points[indices[2]], rgba);
        }
    }

    // Converts the current widget-local clip into the screen-space clip consumed by retained text,
    // icon, image, and slot commands.
    fn current_screen_clip_rect(&self) -> Recti {
        self.draw.current_clip_rect()
    }

    // Converts widget-local integer positions into the screen-space coordinates used by the rest
    // of the retained command stream.
    fn local_to_screen_pos(&self, pos: Vec2i) -> Vec2i {
        pos + Vec2i::new(self.widget_rect.x, self.widget_rect.y)
    }

    // Converts widget-local integer rectangles into screen-space rectangles while preserving
    // extents.
    fn local_to_screen_rect(&self, rect: Recti) -> Recti {
        translate_rect(rect, Vec2i::new(self.widget_rect.x, self.widget_rect.y))
    }

    // Converts the shared screen-space clip back into widget-local coordinates so the tessellator
    // can software-clip generated triangles before they ever reach the retained command stream.
    fn screen_to_local_rect(&self, rect: Recti) -> Recti {
        translate_rect(rect, Vec2i::new(-self.widget_rect.x, -self.widget_rect.y))
    }

    // Flushes any pending triangle batch, then emits a retained non-triangle command clipped
    // against the current local clip rect. This keeps ordering correct when widgets mix text,
    // images, and solid geometry inside one graphics builder.
    fn emit_clipped_command<F>(&mut self, bounds_local: Recti, emit: F)
    where
        F: FnOnce(&mut DrawCtx<'b>),
    {
        self.flush_batch();
        let clip = self.current_screen_clip_rect();
        let bounds = self.local_to_screen_rect(bounds_local);
        self.draw.emit_clipped(bounds, clip, emit);
    }

    // Emits a convex polygon as a triangle fan rooted at the first point. This is the fastest path
    // for fills and avoids any temporary index bookkeeping.
    fn push_triangle_fan(&mut self, points: &[Vec2f], color: Color4b) {
        for idx in 1..points.len() - 1 {
            self.push_triangle_local(points[0], points[idx], points[idx + 1], color);
        }
    }

    // Reuses the triangle path for thick-line quads and other four-corner shapes. Keeping quads as
    // two triangles avoids a separate code path in the retained command stream and the backends.
    fn push_quad_local(&mut self, p0: Vec2f, p1: Vec2f, p2: Vec2f, p3: Vec2f, color: Color) {
        let rgba = color4b(color.r, color.g, color.b, color.a);
        self.push_triangle_local(p0, p1, p2, rgba);
        self.push_triangle_local(p0, p2, p3, rgba);
    }

    // Clips one local triangle against the current local clip and appends the surviving triangles
    // into the shared container-owned arena. Clipping here means later clip-stack changes no
    // longer need to fragment the retained command stream.
    fn push_triangle_local(&mut self, a: Vec2f, b: Vec2f, c: Vec2f, color: Color4b) {
        let clip = self.current_clip_rect();
        let widget_origin = self.widget_origin;
        clip_triangle_vertices_to_rect(
            Vertex::new(a, self.white_uv, color),
            Vertex::new(b, self.white_uv, color),
            Vertex::new(c, self.white_uv, color),
            clip,
            |va, vb, vc| {
                self.draw.push_triangle_vertices(
                    translate_vertex(va, widget_origin),
                    translate_vertex(vb, widget_origin),
                    translate_vertex(vc, widget_origin),
                );
                self.triangle_batch_count += 3;
            },
        );
    }

    // Finalizes the current triangle batch. At this point every triangle has already been clipped
    // in software, so replay only needs the range into the shared arena and no extra clip-state
    // changes.
    fn flush_batch(&mut self) {
        if self.triangle_batch_count == 0 {
            return;
        }

        self.draw.push_command(Command::Triangle {
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
