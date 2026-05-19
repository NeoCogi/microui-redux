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
        let mut commands = std::mem::take(&mut self.draw.commands);
        while !commands.is_empty() {
            let special_index = commands
                .iter()
                .position(|command| matches!(command, Command::BackendCustomRender(_, _) | Command::RetainedPanel { .. }));
            let batch_len = special_index.unwrap_or(commands.len());
            if batch_len > 0 {
                Self::render_batch(canvas, &self.draw.triangle_vertices, commands.drain(..batch_len));
            }

            if special_index.is_none() {
                break;
            }

            match commands.drain(..1).next() {
                Some(Command::BackendCustomRender(mut cra, mut f)) => {
                    // Backend extension callbacks may use RendererHandle directly, so keep them
                    // outside the batched renderer lock.
                    canvas.flush();
                    let prev_clip = canvas.current_clip_rect();
                    let merged_clip = match prev_clip.intersect(&cra.view) {
                        Some(rect) => rect,
                        None => Recti::new(cra.content_area.x, cra.content_area.y, 0, 0),
                    };
                    canvas.set_clip_rect(merged_clip);
                    cra.view = merged_clip;
                    f.render(canvas.current_dimension(), &cra);
                    canvas.flush();
                    canvas.set_clip_rect(prev_clip);
                }
                Some(Command::RetainedPanel { mut handle }) => {
                    canvas.flush();
                    handle.render(canvas);
                    canvas.flush();
                }
                _ => (),
            }
        }
        self.draw.commands = commands;

        self.draw.triangle_vertices.clear();
    }

    fn render_batch<R, I>(canvas: &mut Canvas<R>, triangle_vertices: &[Vertex], commands: I)
    where
        R: Renderer,
        I: IntoIterator<Item = Command>,
    {
        let base_clip = canvas.current_clip_rect();
        canvas.render_scope(|canvas| {
            let mut clip_stack = vec![base_clip];
            canvas.set_clip_rect(base_clip);
            for command in commands {
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
                    Command::PushClip { rect } => {
                        let current = clip_stack.last().copied().unwrap_or(base_clip);
                        let next = current.intersect(&rect).unwrap_or_default();
                        clip_stack.push(next);
                        canvas.set_clip_rect(next);
                    }
                    Command::PopClip => {
                        if clip_stack.len() > 1 {
                            clip_stack.pop();
                        }
                        let current = clip_stack.last().copied().unwrap_or(base_clip);
                        canvas.set_clip_rect(current);
                    }
                    Command::Image { rect, image, color } => {
                        canvas.draw_image(image, rect, color);
                    }
                    Command::SlotRedraw { rect, id, color, payload } => {
                        canvas.draw_slot_with_function(id, rect, color, payload);
                    }
                    Command::Triangle { vertex_start, vertex_count } => {
                        let end = vertex_start + vertex_count;
                        canvas.draw_triangles(&triangle_vertices[vertex_start..end]);
                    }
                    Command::RetainedPanel { .. } | Command::BackendCustomRender(_, _) | Command::None => (),
                }
            }
            canvas.set_clip_rect(base_clip);
        });
    }

    fn draw_ctx(&mut self) -> DrawCtx<'_> {
        DrawCtx::new(
            &mut self.draw.commands,
            &mut self.draw.triangle_vertices,
            &mut self.draw.clip_stack,
            self.style.as_ref(),
            &self.atlas,
        )
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
    #[cfg(test)]
    pub fn check_clip(&mut self, r: Recti) -> Clip {
        self.draw_ctx().check_clip(r)
    }

    /// Adjusts the current clip rectangle.
    #[cfg(test)]
    pub fn set_clip(&mut self, rect: Recti) {
        let mut draw = self.draw_ctx();
        draw.set_current_clip_rect(rect);
    }

    /// Records a filled rectangle draw command.
    #[cfg(test)]
    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_rect(rect, color);
    }

    /// Records a text draw command.
    #[cfg(test)]
    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_text(font, str, pos, color);
    }

    /// Records an icon draw command.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_icon(id, rect, color);
    }

    /// Draws a frame and optional border using the specified color.
    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let mut draw = self.draw_ctx();
        draw.draw_frame(rect, colorid);
    }

    #[inline(never)]
    /// Draws widget text with the appropriate alignment flags using an explicit font.
    pub fn draw_control_text_with_font(&mut self, font: FontId, str: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let mut draw = self.draw_ctx();
        draw.draw_control_text_with_font(font, str, rect, colorid, opt);
    }
}
