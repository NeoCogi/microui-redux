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
//! widget-event dispatcher. WindowManager chooses the eligible root, then the [`UiRuntime`]-owned
//! input router chooses a node; neither invokes application widget-event subscribers.
use bitflags::bitflags;

use crate::input::Input;
use crate::render::DisplayList;
use crate::{Dimensioni, Recti, Style, UiRuntime};
use roots::WindowEntry;
mod root_chrome;
mod roots;

pub use root_chrome::{RootChanged, RootHandle, RootSubmitted};
// PopupHandle and RootMutationError belong to registry capability and policy rather than the
// private chrome widget. Re-export both from the window-manager boundary as stable public types.
pub use roots::{PopupHandle, RootMutationError};

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
        /// Removes the Style-owned inset around root content.
        ///
        /// This is useful for edge-to-edge application surfaces whose content, such as a menu bar,
        /// already owns its internal spacing. It is independent of [`Self::FRAME`]: removing the
        /// frame does not otherwise remove the ordinary window-content inset.
        const NO_PADDING = 8;
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

/// Lowest application-selectable root layer.
pub const MIN_LAYER: u8 = 0;
/// Highest application-selectable root layer.
pub const MAX_LAYER: u8 = 15;
/// Layer assigned to newly created independent windows.
pub const DEFAULT_LAYER: u8 = MAX_LAYER;

/// Describes how one retained root obtains its stacking layer.
///
/// Top-level windows have a [`Fixed`](Self::Fixed) application layer. Every owned non-modal root
/// inherits through its stable parent, while dialogs occupy the dedicated modal layer above all
/// sixteen application layers. The window manager derives this value from the owned-root tree;
/// application code changes only top-level fixed window layers through
/// [`crate::Context::set_root_layer`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LayerBinding {
    /// A caller-selected application layer in the inclusive range [`MIN_LAYER`]..=[`MAX_LAYER`].
    Fixed(u8),
    /// An owned root inheriting the effective layer of its direct parent.
    Inherited(RootId),
    /// A dialog in the dedicated layer above all application-selectable layers.
    Modal,
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
    /// Last ordinary root explicitly activated by a pointer press.
    ///
    /// Activation is deliberately independent of stacking. A user can therefore focus a control
    /// in a low layer without raising that root over windows in a higher layer.
    active_root: Option<RootId>,
    /// Next root id counter.
    next_root_id: usize,
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
            active_root: None,
            next_root_id: 1,
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
mod retained_runtime_baseline;

#[cfg(test)]
mod root_tests;
