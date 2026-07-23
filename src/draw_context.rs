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
//! Draw command recording helpers used by containers and widget graphics.
//!
//! This file owns clip-stack mutation and conversion from high-level widget drawing requests into
//! retained [`Command`] values. It deliberately does not talk to a renderer; that happens later
//! when draw commands are replayed through [`crate::Canvas`].
use crate::render_command::Command;
use crate::render::Vertex;
use crate::*;

/// Returns the intersection of `rect` with `limit`, defaulting to an empty rect when disjoint.
pub(crate) fn intersect_clip_rect(limit: Recti, rect: Recti) -> Recti {
    rect.intersect(&limit).unwrap_or_default()
}

/// Returns whether `bounds` is fully visible, partially visible, or fully outside `clip`.
pub(crate) fn clip_relation(bounds: Recti, clip: Recti) -> Clip {
    // Empty geometry is treated as fully clipped so callers can skip command emission.
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

/// Internal command emitter shared by container chrome, built-in widgets, and custom graphics.
pub(crate) struct CommandEmitter<'a> {
    /// Retained command list receiving high-level draw commands.
    commands: &'a mut Vec<Command>,
    /// Shared vertex arena for retained triangle geometry.
    triangle_vertices: &'a mut Vec<Vertex>,
    /// Active screen-space clip stack.
    clip_stack: &'a mut Vec<Recti>,
    /// Style used to resolve widget colors and text metrics.
    style: &'a Style,
    /// Atlas used for glyph/icon/slot metrics.
    atlas: &'a AtlasHandle,
}

/// Alias used by widgets and containers for command recording.
pub(crate) type DrawCtx<'a> = CommandEmitter<'a>;

impl<'a> CommandEmitter<'a> {
    /// Creates a recorder around the container-owned command and vertex buffers.
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

    /// Returns the style currently used to resolve widget colors and spacing.
    pub(crate) fn style(&self) -> &Style {
        self.style
    }

    /// Returns the atlas used for text/icon metrics and atlas slot lookups.
    pub(crate) fn atlas(&self) -> &AtlasHandle {
        self.atlas
    }

    /// Returns the active clip rectangle, or an unclipped sentinel when the stack is empty.
    pub(crate) fn current_clip_rect(&self) -> Recti {
        self.clip_stack.last().copied().unwrap_or(UNCLIPPED_RECT)
    }

    /// Returns the current clip stack depth.
    ///
    /// `Graphics` forwards widget-local clips onto the shared draw-context clip stack, so it needs
    /// to know how much of the stack existed before it started and to restore that depth on drop.
    pub(crate) fn clip_depth(&self) -> usize {
        self.clip_stack.len()
    }

    /// Pushes a nested clip after intersecting it with the existing top-of-stack clip.
    pub(crate) fn push_clip_rect(&mut self, rect: Recti) {
        let last = self.current_clip_rect();
        self.clip_stack.push(intersect_clip_rect(last, rect));
    }

    /// Pops the most recent clip scope.
    pub(crate) fn pop_clip_rect(&mut self) {
        self.clip_stack.pop();
    }

    /// Replaces the current top-of-stack clip with an already-intersected rect.
    ///
    /// `Graphics` computes the monotonic intersection in widget-local terms and then overwrites the
    /// shared screen-space clip without growing the stack.
    pub(crate) fn replace_current_clip_rect(&mut self, rect: Recti) {
        if let Some(top) = self.clip_stack.last_mut() {
            *top = rect;
        } else {
            self.clip_stack.push(rect);
        }
    }

    /// Restores the clip stack to a previously recorded depth.
    ///
    /// This keeps temporary graphics builders from leaking their local clip scopes back into the
    /// outer container traversal.
    pub(crate) fn pop_clip_rect_to(&mut self, depth: usize) {
        while self.clip_stack.len() > depth {
            self.clip_stack.pop();
        }
    }

    /// Appends a draw command to the container's retained command stream.
    pub(crate) fn push_command(&mut self, cmd: Command) {
        self.commands.push(cmd);
    }

    /// Returns the number of vertices currently stored in the retained triangle arena.
    ///
    /// Retained widget graphics append all triangle vertices into one container-owned arena, and
    /// individual commands store ranges into that arena instead of owning separate `Vec<Vertex>`
    /// allocations.
    pub(crate) fn triangle_vertex_count(&self) -> usize {
        self.triangle_vertices.len()
    }

    /// Returns the shared triangle arena for direct geometry output.
    pub(crate) fn triangle_vertices_mut(&mut self) -> &mut Vec<Vertex> {
        self.triangle_vertices
    }

    /// Emits a replay command that pushes a clip during render playback.
    fn push_replay_clip(&mut self, rect: Recti) {
        self.push_command(Command::PushClip { rect });
    }

    /// Emits a replay command that pops a clip during render playback.
    fn pop_replay_clip(&mut self) {
        self.push_command(Command::PopClip);
    }

    /// Emits a command under the minimum replay clip required by `bounds`.
    ///
    /// This reuses the same clip-state wrapper for text, icons, images, and slot redraws so both
    /// the legacy draw-context path and the graphics builder can emit those commands consistently.
    pub(crate) fn emit_clipped<F>(&mut self, bounds: Recti, clip: Recti, emit: F)
    where
        F: FnOnce(&mut Self),
    {
        let clipped = clip_relation(bounds, clip);
        if clipped == Clip::All {
            return;
        }
        // Partially visible commands replay under a temporary clip; fully visible commands avoid
        // the extra push/pop pair to keep the command stream compact.
        if clipped == Clip::Part {
            self.push_replay_clip(clip);
        }
        emit(self);
        if clipped != Clip::None {
            self.pop_replay_clip();
        }
    }

    /// Records a solid rectangle after applying the active clip immediately.
    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) {
        let rect = rect.intersect(&self.current_clip_rect()).unwrap_or_default();
        if rect.width > 0 && rect.height > 0 {
            self.push_command(Command::Recti { rect, color });
        }
    }

    /// Records the four edges of a one-pixel border rectangle.
    pub(crate) fn draw_box(&mut self, r: Recti, color: Color) {
        self.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
        self.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
        self.draw_rect(rect(r.x, r.y, 1, r.height), color);
        self.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
    }

    /// Draws a filled control background and optional one-pixel border for the color role.
    pub(crate) fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let color = self.style.colors[colorid as usize];
        self.draw_rect(rect, color);
        if let Some(border_color) = self.style.frame_border_color(colorid) {
            self.draw_box(expand_rect(rect, 1), border_color);
        }
    }
}
