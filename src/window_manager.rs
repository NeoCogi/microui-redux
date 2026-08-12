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
//! Backend-independent retained root-window coordination.
//!
//! [`WindowManager`] owns retained roots, ordered input, modal policy, layout state, and display-list
//! recording. The generic [`crate::Context`] façade owns it alongside the renderer and application
//! event dispatcher.
use bitflags::bitflags;

use crate::input::Input;
use crate::render::DisplayList;
use crate::{Dimensioni, Recti, Style, UiRuntime};
use roots::WindowEntry;
mod root_chrome;
mod roots;

pub use root_chrome::{RootChanged, RootHandle, RootMutationError, RootChrome, RootSubmitted};

bitflags! {
    #[derive(Copy, Clone)]
    /// Options that control a root window, dialog, or popup.
    pub struct WindowOption : u32 {
        /// Gives the root a Style-owned outer border and inset content area.
        const FRAME = 1024;
        /// Adapts the root width to its content while retaining its programmed height.
        const AUTO_WIDTH = 256;
        /// Adapts the root height to its content while retaining its programmed width.
        const AUTO_HEIGHT = 512;
        /// Adapts both root axes to intrinsic content size.
        const AUTO_SIZE = Self::AUTO_WIDTH.bits() | Self::AUTO_HEIGHT.bits();
        /// Hides the title bar.
        const NO_TITLE = 128;
        /// Hides the close button.
        const NO_CLOSE = 64;
        /// Prevents the user from resizing the root.
        const NO_RESIZE = 16;
        /// No special options.
        const NONE = 0;
    }
}

/// Opaque identifier for a root window, dialog, or popup registered with [`crate::Context`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RootId(usize);

impl RootId {
    /// Wraps a raw counter value as a root identifier.
    pub(crate) const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }
}

/// Backend- and application-state-independent retained window manager.
pub(crate) struct WindowManager {
    /// Reusable operation storage for window-manager frame and chrome drawing.
    display_list: DisplayList,
    /// Window-manager-owned style used by all roots and scroll areas.
    style: Style,

    /// Highest z-index allocated to an open window-manager root.
    last_zindex: i32,
    /// Registered window-manager roots replayed by [`crate::ContextFrame::render_ui`].
    roots: Vec<WindowEntry>,
    /// Visible dialogs in nesting order; the last entry is the sole input root.
    modal_stack: Vec<RootId>,
    /// Next root id counter.
    next_root_id: usize,
    /// Context-owned file-dialog controllers advanced after retained input updates.
    pub(crate) file_dialogs: Vec<crate::file_dialog::FileDialogController>,
    /// Next file-dialog session id counter.
    pub(crate) next_file_dialog_id: usize,
    /// Ordered input state owned and consumed directly by this window manager.
    input: Input,
    /// Dimensions of the most recent complete update/layout commit.
    ui_commit: Option<Dimensioni>,
}

impl WindowManager {
    pub(crate) fn new(style: Style) -> Self {
        Self {
            display_list: DisplayList::new(),
            style,
            last_zindex: 0,
            roots: Vec::default(),
            modal_stack: Vec::new(),
            next_root_id: 1,
            file_dialogs: Vec::new(),
            next_file_dialog_id: 1,
            input: Input::default(),
            ui_commit: None,
        }
    }

    pub(crate) fn invalidate_ui_commit(&mut self) {
        self.ui_commit = None;
    }

    pub(crate) fn style(&self) -> &Style {
        &self.style
    }

    pub(crate) fn set_style(&mut self, style: Style) {
        self.style = style;
        self.invalidate_ui_commit();
    }

    pub(crate) fn mousemove(&mut self, x: i32, y: i32) {
        self.input.mousemove(x, y);
        self.invalidate_ui_commit();
    }

    pub(crate) fn mousedown(&mut self, x: i32, y: i32, button: crate::MouseButton) {
        self.input.mousedown(x, y, button);
        self.invalidate_ui_commit();
    }

    pub(crate) fn mouseup(&mut self, x: i32, y: i32, button: crate::MouseButton) {
        self.input.mouseup(x, y, button);
        self.invalidate_ui_commit();
    }

    pub(crate) fn scroll(&mut self, x: i32, y: i32) {
        self.input.scroll(x, y);
        self.invalidate_ui_commit();
    }

    pub(crate) fn keydown(&mut self, key: crate::KeyMode) {
        self.input.keydown(key);
        self.invalidate_ui_commit();
    }

    pub(crate) fn keyup(&mut self, key: crate::KeyMode) {
        self.input.keyup(key);
        self.invalidate_ui_commit();
    }

    pub(crate) fn keydown_code(&mut self, code: crate::KeyCode) {
        self.input.keydown_code(code);
        self.invalidate_ui_commit();
    }

    pub(crate) fn keyup_code(&mut self, code: crate::KeyCode) {
        self.input.keyup_code(code);
        self.invalidate_ui_commit();
    }

    pub(crate) fn text(&mut self, text: &str) {
        self.input.text(text);
        self.invalidate_ui_commit();
    }

    pub(crate) fn can_render(&self, dimensions: Dimensioni) -> bool {
        self.ui_commit
            .is_some_and(|committed| (committed.width, committed.height) == (dimensions.width, dimensions.height))
            && !self.input.has_pending()
    }

    pub(crate) fn display_list_mut(&mut self) -> &mut DisplayList {
        &mut self.display_list
    }

    pub(crate) fn cancel_frame(&mut self) {
        self.display_list.clear();
    }
}

#[cfg(test)]
mod p5_baseline;

#[cfg(test)]
mod root_tests;
