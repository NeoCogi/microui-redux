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
//! Draw-command recording helpers that operate on container-local state.

use super::*;

impl Container {
    #[inline(never)]
    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        for command in self.command_list.drain(0..) {
            match command {
                Command::Text { text, pos, color, font } => {
                    canvas.draw_chars(font, &text, pos, color);
                }
                Command::Recti { rect, color } => {
                    canvas.draw_rect(rect, color);
                }
                Command::Icon { id, rect, color } => {
                    canvas.draw_icon(id, rect, color);
                }
                Command::Clip { rect } => {
                    canvas.set_clip_rect(rect);
                }
                Command::Image { rect, image, color } => {
                    canvas.draw_image(image, rect, color);
                }
                Command::SlotRedraw { rect, id, color, payload } => {
                    canvas.draw_slot_with_function(id, rect, color, payload.clone());
                }
                Command::Triangle { vertices } => {
                    canvas.draw_triangles(vertices.as_slice());
                }
                Command::CustomRender(mut cra, mut f) => {
                    canvas.flush();
                    let prev_clip = canvas.current_clip_rect();
                    let merged_clip = match prev_clip.intersect(&cra.view) {
                        Some(rect) => rect,
                        None => Recti::new(cra.content_area.x, cra.content_area.y, 0, 0),
                    };
                    canvas.set_clip_rect(merged_clip);
                    cra.view = merged_clip;
                    (*f)(canvas.current_dimension(), &cra);
                    canvas.flush();
                    canvas.set_clip_rect(prev_clip);
                }
                Command::None => (),
            }
        }

        for panel in &mut self.panels {
            panel.render(canvas)
        }
    }

    fn draw_ctx(&mut self) -> DrawCtx<'_> {
        DrawCtx::new(&mut self.command_list, &mut self.clip_stack, self.style.as_ref(), &self.atlas)
    }

    /// Pushes a new clip rectangle combined with the previous clip.
    pub fn push_clip_rect(&mut self, rect: Recti) {
        let mut draw = self.draw_ctx();
        draw.push_clip_rect(rect);
    }

    /// Restores the previous clip rectangle from the stack.
    pub fn pop_clip_rect(&mut self) {
        let mut draw = self.draw_ctx();
        draw.pop_clip_rect();
    }

    /// Returns the active clip rectangle, or an unclipped rect when the stack is empty.
    pub fn get_clip_rect(&mut self) -> Recti {
        self.draw_ctx().current_clip_rect()
    }

    /// Determines whether `r` is fully visible, partially visible, or completely clipped.
    pub fn check_clip(&mut self, r: Recti) -> Clip {
        self.draw_ctx().check_clip(r)
    }

    /// Adjusts the current clip rectangle.
    pub fn set_clip(&mut self, rect: Recti) {
        let mut draw = self.draw_ctx();
        draw.set_clip(rect);
    }

    /// Records a filled rectangle draw command.
    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_rect(rect, color);
    }

    /// Records a rectangle outline.
    pub fn draw_box(&mut self, r: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_box(r, color);
    }

    /// Records a text draw command.
    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_text(font, str, pos, color);
    }

    /// Records an icon draw command.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_icon(id, rect, color);
    }

    /// Records a slot draw command.
    pub fn draw_slot(&mut self, id: SlotId, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.push_image(Image::Slot(id), rect, color);
    }

    /// Records a slot redraw that uses a callback to fill pixels.
    pub fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let mut draw = self.draw_ctx();
        draw.draw_slot_with_function(id, rect, color, f);
    }

    #[inline(never)]
    /// Draws multi-line text within the container without wrapping.
    pub fn text(&mut self, text: &str) {
        self.text_with_wrap(text, TextWrap::None);
    }

    #[inline(never)]
    /// Draws multi-line text within the container using the provided wrapping mode.
    /// The block is rendered inside an internal column with zero spacing so consecutive
    /// lines sit back-to-back while the outer widget spacing/padding remains intact.
    pub fn text_with_wrap(&mut self, text: &str, wrap: TextWrap) {
        if text.is_empty() {
            return;
        }
        let style = self.style.as_ref();
        let font = style.font;
        let color = style.colors[ControlColor::Text as usize];
        let line_height = self.atlas.get_font_height(font) as i32;
        let baseline = self.atlas.get_font_baseline(font);
        let saved_spacing = self.layout.style.spacing;
        self.layout.style.spacing = 0;
        self.column(|ui| {
            ui.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Fixed(line_height));
            let first_rect = ui.layout.next();
            let max_width = first_rect.width;
            let mut lines = build_text_lines(text, wrap, max_width, font, &ui.atlas);
            if text.ends_with('\n') {
                if let Some(last) = lines.last() {
                    if last.start == text.len() && last.end == text.len() {
                        lines.pop();
                    }
                }
            }
            for (idx, line) in lines.iter().enumerate() {
                let r = if idx == 0 { first_rect } else { ui.layout.next() };
                let line_top = Self::baseline_aligned_top(r, line_height, baseline);
                let slice = &text[line.start..line.end];
                if !slice.is_empty() {
                    ui.draw_text(font, slice, vec2(r.x, line_top), color);
                }
            }
        });
        self.layout.style.spacing = saved_spacing;
    }

    /// Draws a frame and optional border using the specified color.
    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let mut draw = self.draw_ctx();
        draw.draw_frame(rect, colorid);
    }

    /// Draws a widget background, applying hover/focus accents when needed.
    pub fn draw_widget_frame(&mut self, widget_id: WidgetId, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let focused = self.focus == Some(widget_id);
        let hovered = self.hover == Some(widget_id);
        let mut draw = self.draw_ctx();
        draw.draw_widget_frame(focused, hovered, rect, colorid, opt);
    }

    /// Draws a container frame, skipping rendering when the option disables it.
    pub fn draw_container_frame(&mut self, widget_id: WidgetId, rect: Recti, mut colorid: ControlColor, opt: ContainerOption) {
        if opt.has_no_frame() {
            return;
        }

        if self.focus == Some(widget_id) {
            colorid.focus()
        } else if self.hover == Some(widget_id) {
            colorid.hover()
        }
        let mut draw = self.draw_ctx();
        draw.draw_frame(rect, colorid);
    }

    #[inline(never)]
    /// Draws widget text with the appropriate alignment flags.
    pub fn draw_control_text(&mut self, str: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let mut draw = self.draw_ctx();
        draw.draw_control_text(str, rect, colorid, opt);
    }
}
