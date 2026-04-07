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
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
use crate::*;
use std::cmp::{max, min};
use std::rc::Rc;

/// Returns the intersection of `rect` with `limit`, defaulting to an empty rect when disjoint.
pub(crate) fn intersect_clip_rect(limit: Recti, rect: Recti) -> Recti {
    rect.intersect(&limit).unwrap_or_default()
}

/// Returns whether `bounds` is fully visible, partially visible, or fully outside `clip`.
pub(crate) fn clip_relation(bounds: Recti, clip: Recti) -> Clip {
    if bounds.width <= 0 || bounds.height <= 0 || clip.width <= 0 || clip.height <= 0 {
        return Clip::All;
    }

    if bounds.x > clip.x + clip.width || bounds.x + bounds.width < clip.x || bounds.y > clip.y + clip.height || bounds.y + bounds.height < clip.y {
        return Clip::All;
    }

    if bounds.x >= clip.x && bounds.x + bounds.width <= clip.x + clip.width && bounds.y >= clip.y && bounds.y + bounds.height <= clip.y + clip.height {
        return Clip::None;
    }

    Clip::Part
}

pub(crate) struct DrawCtx<'a> {
    commands: &'a mut Vec<Command>,
    triangle_vertices: &'a mut Vec<Vertex>,
    clip_stack: &'a mut Vec<Recti>,
    style: &'a Style,
    atlas: &'a AtlasHandle,
}

/// Returns the top-aligned y coordinate that best preserves the font baseline inside `rect`.
///
/// Widgets and the graphics builder both use this to keep single-line labels visually centered
/// without letting a short control crop the ascent or descent unevenly.
pub(crate) fn baseline_aligned_top(rect: Recti, line_height: i32, baseline: i32) -> i32 {
    if rect.height >= line_height {
        return rect.y + (rect.height - line_height) / 2;
    }

    let baseline_center = rect.y + rect.height / 2;
    let min_top = rect.y + rect.height - line_height;
    let max_top = rect.y;
    clamp_i32(baseline_center - baseline, min_top, max_top)
}

#[inline]
fn clamp_i32(x: i32, a: i32, b: i32) -> i32 {
    min(b, max(a, x))
}

/// Computes the text origin for one control label inside `rect`.
///
/// The returned point is in the same coordinate space as `rect`, which lets both `DrawCtx` and
/// `Graphics` reuse the exact same centering and padding rules.
pub(crate) fn control_text_position_with_font(style: &Style, atlas: &AtlasHandle, font: FontId, text: &str, rect: Recti, opt: WidgetOption) -> Vec2i {
    let tsize = atlas.get_text_size(font, text);
    let padding = style.padding;
    let line_height = atlas.get_font_height(font) as i32;
    let baseline = atlas.get_font_baseline(font);
    let y = baseline_aligned_top(rect, line_height, baseline);
    let x = if opt.is_aligned_center() {
        rect.x + (rect.width - tsize.width) / 2
    } else if opt.is_aligned_right() {
        rect.x + rect.width - tsize.width - padding
    } else {
        rect.x + padding
    };
    vec2(x, y)
}

impl<'a> DrawCtx<'a> {
    pub(crate) fn new(
        commands: &'a mut Vec<Command>,
        triangle_vertices: &'a mut Vec<Vertex>,
        clip_stack: &'a mut Vec<Recti>,
        style: &'a Style,
        atlas: &'a AtlasHandle,
    ) -> Self {
        Self {
            commands,
            triangle_vertices,
            clip_stack,
            style,
            atlas,
        }
    }

    pub(crate) fn style(&self) -> &Style {
        self.style
    }

    pub(crate) fn atlas(&self) -> &AtlasHandle {
        self.atlas
    }

    pub(crate) fn current_clip_rect(&self) -> Recti {
        self.clip_stack.last().copied().unwrap_or(UNCLIPPED_RECT)
    }

    // `Graphics` forwards widget-local clips onto the shared draw-context clip stack, so it needs
    // to know how much of the stack existed before it started and to restore that depth on drop.
    pub(crate) fn clip_depth(&self) -> usize {
        self.clip_stack.len()
    }

    pub(crate) fn push_clip_rect(&mut self, rect: Recti) {
        let last = self.current_clip_rect();
        self.clip_stack.push(intersect_clip_rect(last, rect));
    }

    pub(crate) fn pop_clip_rect(&mut self) {
        self.clip_stack.pop();
    }

    // Replaces the current top-of-stack clip with an already-intersected rect. `Graphics`
    // computes the monotonic intersection in widget-local terms and then overwrites the shared
    // screen-space clip without growing the stack.
    pub(crate) fn replace_current_clip_rect(&mut self, rect: Recti) {
        if let Some(top) = self.clip_stack.last_mut() {
            *top = rect;
        } else {
            self.clip_stack.push(rect);
        }
    }

    // Restores the clip stack to a previously recorded depth. This keeps temporary graphics
    // builders from leaking their local clip scopes back into the outer container traversal.
    pub(crate) fn pop_clip_rect_to(&mut self, depth: usize) {
        while self.clip_stack.len() > depth {
            self.clip_stack.pop();
        }
    }

    pub(crate) fn push_command(&mut self, cmd: Command) {
        self.commands.push(cmd);
    }

    // Retained widget graphics append all triangle vertices into one container-owned arena, and
    // individual commands store ranges into that arena instead of owning separate `Vec<Vertex>`
    // allocations.
    pub(crate) fn triangle_vertex_count(&self) -> usize {
        self.triangle_vertices.len()
    }

    pub(crate) fn push_triangle_vertices(&mut self, v0: Vertex, v1: Vertex, v2: Vertex) {
        self.triangle_vertices.push(v0);
        self.triangle_vertices.push(v1);
        self.triangle_vertices.push(v2);
    }

    pub(crate) fn set_clip(&mut self, rect: Recti) {
        self.push_command(Command::Clip { rect });
    }

    // Reuses the same clip-state wrapper for text, icons, images, and slot redraws so both the
    // legacy draw-context path and the graphics builder can emit those commands consistently.
    pub(crate) fn emit_clipped<F>(&mut self, bounds: Recti, clip: Recti, emit: F)
    where
        F: FnOnce(&mut Self),
    {
        let clipped = clip_relation(bounds, clip);
        if clipped == Clip::All {
            return;
        }
        if clipped == Clip::Part {
            self.set_clip(clip);
        }
        emit(self);
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub(crate) fn check_clip(&self, r: Recti) -> Clip {
        clip_relation(r, self.current_clip_rect())
    }

    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) {
        let rect = rect.intersect(&self.current_clip_rect()).unwrap_or_default();
        if rect.width > 0 && rect.height > 0 {
            self.push_command(Command::Recti { rect, color });
        }
    }

    pub(crate) fn draw_box(&mut self, r: Recti, color: Color) {
        self.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
        self.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
        self.draw_rect(rect(r.x, r.y, 1, r.height), color);
        self.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
    }

    pub(crate) fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        let size = self.atlas.get_text_size(font, text);
        let bounds = rect(pos.x, pos.y, size.width, size.height);
        let clip = self.current_clip_rect();
        self.emit_clipped(bounds, clip, |draw| {
            draw.push_command(Command::Text {
                text: String::from(text),
                pos,
                color,
                font,
            });
        });
    }

    pub(crate) fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let clip = self.current_clip_rect();
        self.emit_clipped(rect, clip, |draw| {
            draw.push_command(Command::Icon { id, rect, color });
        });
    }

    pub(crate) fn push_image(&mut self, image: Image, rect: Recti, color: Color) {
        let clip = self.current_clip_rect();
        self.emit_clipped(rect, clip, |draw| {
            draw.push_command(Command::Image { image, rect, color });
        });
    }

    pub(crate) fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let clip = self.current_clip_rect();
        self.emit_clipped(rect, clip, |draw| {
            draw.push_command(Command::SlotRedraw { id, rect, color, payload: f });
        });
    }

    pub(crate) fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let color = self.style.colors[colorid as usize];
        self.draw_rect(rect, color);
        if colorid == ControlColor::ScrollBase || colorid == ControlColor::ScrollThumb || colorid == ControlColor::TitleBG {
            return;
        }
        let border_color = self.style.colors[ControlColor::Border as usize];
        if border_color.a != 0 {
            self.draw_box(expand_rect(rect, 1), border_color);
        }
    }

    pub(crate) fn draw_widget_frame(&mut self, focused: bool, hovered: bool, rect: Recti, mut colorid: ControlColor, opt: WidgetOption) {
        if opt.has_no_frame() {
            return;
        }
        if focused {
            colorid.focus()
        } else if hovered {
            colorid.hover()
        }
        self.draw_frame(rect, colorid);
    }

    pub(crate) fn draw_control_text_with_font(&mut self, font: FontId, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let color = self.style.colors[colorid as usize];
        let pos = control_text_position_with_font(self.style, self.atlas, font, text, rect, opt);

        self.push_clip_rect(rect);
        self.draw_text(font, text, pos, color);
        self.pop_clip_rect();
    }
}
