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
//! High-level layout helpers layered over the shared `LayoutManager`.

use super::*;

impl Container {
    /// Temporarily overrides the row definition and restores it after `f` executes.
    pub(crate) fn with_row<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], height: SizePolicy, f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.row(widths, height);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Temporarily overrides the layout with explicit column and row tracks and restores it after `f`.
    ///
    /// Widgets are emitted row-major within the provided track matrix.
    pub(crate) fn with_grid<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.grid(widths, heights);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Same as [`Container::stack_with_width`], but controls whether items are emitted top-down or bottom-up.
    pub(crate) fn stack_with_width_direction<F: FnOnce(&mut Self)>(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        if direction == StackDirection::TopToBottom {
            self.layout.stack(width, height);
        } else {
            self.layout.stack_with_direction(width, height, direction);
        }
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Creates a nested column scope where each call to `next_cell` yields a single column.
    pub(crate) fn column<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.layout.begin_column();
        f(self);
        self.layout.end_column();
    }
}
