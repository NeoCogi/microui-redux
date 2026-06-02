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
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR ITS CONTRIBUTORS BE
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
//! Retained scroll-area viewport state used by node-runtime scroll containers.

use crate::context::NodeLayout;
use crate::{Dimensioni, Recti, Vec2i};

/// Retained state for one scrollable child subtree.
pub(crate) struct ScrollAreaState {
    rect: Recti,
    body: Recti,
    content_size: Dimensioni,
    scroll: Vec2i,
}

impl ScrollAreaState {
    /// Creates retained scroll-area state.
    pub(crate) fn new(_name: &str) -> Self {
        Self {
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Dimensioni::default(),
            scroll: Vec2i::default(),
        }
    }

    /// Returns the scroll area outer rectangle.
    pub fn rect(&self) -> Recti {
        self.rect
    }

    /// Updates the scroll area outer rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.rect = rect;
    }

    /// Returns the visible body rectangle.
    pub fn body(&self) -> Recti {
        self.body
    }

    /// Returns the measured content size.
    pub fn content_size(&self) -> Dimensioni {
        self.content_size
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.scroll
    }

    /// Updates the current scroll offset.
    pub fn set_scroll(&mut self, scroll: Vec2i) {
        self.scroll = scroll;
    }

    /// Applies the latest node-runtime viewport layout.
    pub(crate) fn apply_viewport_layout(&mut self, layout: NodeLayout) {
        self.rect = layout.rect;
        self.body = layout.body;
        self.content_size = layout.content_size;
    }
}
