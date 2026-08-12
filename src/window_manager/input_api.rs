//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

//! Public input forwarding APIs for [`Context`].

use super::*;

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Queues one mouse-pointer position transition without coalescing.
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.window_manager.input.mousemove(x, y);
        self.invalidate_ui_commit();
    }

    /// Queues one mouse-button press at the supplied pointer position.
    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.window_manager.input.mousedown(x, y, btn);
        self.invalidate_ui_commit();
    }

    /// Queues one mouse-button release at the supplied pointer position.
    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.window_manager.input.mouseup(x, y, btn);
        self.invalidate_ui_commit();
    }

    /// Queues one scroll-wheel or trackpad delta without accumulating adjacent calls.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.window_manager.input.scroll(x, y);
        self.invalidate_ui_commit();
    }

    /// Queues one modifier/control-key press.
    pub fn keydown(&mut self, key: KeyMode) {
        self.window_manager.input.keydown(key);
        self.invalidate_ui_commit();
    }

    /// Queues one modifier/control-key release.
    pub fn keyup(&mut self, key: KeyMode) {
        self.window_manager.input.keyup(key);
        self.invalidate_ui_commit();
    }

    /// Queues one navigation-key press.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.window_manager.input.keydown_code(code);
        self.invalidate_ui_commit();
    }

    /// Queues one navigation-key release.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.window_manager.input.keyup_code(code);
        self.invalidate_ui_commit();
    }

    /// Queues one UTF-8 text transition, including an empty string.
    pub fn text(&mut self, text: &str) {
        self.window_manager.input.text(text);
        self.invalidate_ui_commit();
    }
}
