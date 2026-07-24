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
//! Draw operation definitions recorded during node traversal.
//!
//! The layout and widget passes record what should be drawn or invoked; the final canvas/backend
//! consumes this list later when the command stream is replayed.

use super::*;

/// Controls how text should wrap when rendered inside a container.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextWrap {
    /// Render text on a single line without wrapping.
    None,
    /// Wrap text at word boundaries when it exceeds the cell width.
    Word,
}

/// One legacy draw payload and the effective screen-space clip captured with it.
pub(crate) struct Command {
    /// Final producer-side clip for this operation.
    pub(crate) clip: Recti,
    /// Backend-neutral draw payload.
    pub(crate) kind: CommandKind,
}

impl Command {
    /// Associates a draw payload with its effective clip.
    pub(crate) const fn new(clip: Recti, kind: CommandKind) -> Self {
        Self { clip, kind }
    }
}

/// Backend-neutral payload retained until the runtime records directly into DisplayList.
pub(crate) enum CommandKind {
    /// Draws a solid rectangle.
    Recti {
        /// Target rectangle.
        rect: Recti,
        /// Fill color.
        color: Color,
    },
    /// Draws text.
    Text {
        /// Font to use.
        font: FontId,
        /// Top-left text position.
        pos: Vec2i,
        /// Text color.
        color: Color,
        /// UTF-8 string to render.
        text: String,
    },
    /// Draws an icon from the atlas.
    Icon {
        /// Target rectangle.
        rect: Recti,
        /// Icon identifier.
        id: IconId,
        /// Tint color.
        color: Color,
    },
    /// Draws an arbitrary image (slot or texture).
    Image {
        /// Target rectangle.
        rect: Recti,
        /// Image identifier.
        image: Image,
        /// Tint color.
        color: Color,
    },
    /// Re-renders a slot before drawing it.
    SlotRedraw {
        /// Target rectangle.
        rect: Recti,
        /// Slot to update.
        id: SlotId,
        /// Tint color.
        color: Color,
        /// Callback generating pixels.
        payload: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
    /// Draws a triangle list using already transformed screen-space vertices.
    ///
    /// Every three consecutive vertices form one solid triangle. Geometry remains unclipped until
    /// Canvas execution; the effective clip is retained alongside the arena range.
    Triangle {
        /// Index of the first triangle vertex inside the container-owned arena.
        vertex_start: usize,
        /// Number of consecutive vertices in the triangle list.
        vertex_count: usize,
    },
    /// Invokes a backend extension callback for custom rendering.
    ///
    /// This keeps custom drawing inside the same traversal and clipping pipeline as the built-in
    /// widgets, while making the backend-specific escape hatch explicit at the command boundary.
    BackendCustomRender(CustomRenderArgs, Box<dyn CustomRenderCommand>),
    /// Sentinel used when no command is enqueued.
    ///
    /// This is mainly a convenience for `Default` and placeholder command-list storage.
    None,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            clip: Recti::default(),
            kind: CommandKind::None,
        }
    }
}
