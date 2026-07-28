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
//! Built-in retained widget state.
//!
//! The widget structs intentionally keep their user-facing state public so applications can update
//! labels, values, configuration, and local state between frames. Raw input remains owned by
//! [`crate::Context`], and each widget's `update` path clamps transient invariants such as text
//! cursors, scroll offsets, selected indices, and numeric ranges before `paint` records commands.

use crate::{FontChoice, FontRole, WidgetOption};

#[derive(Copy, Clone)]
/// Shared configuration carried by built-in retained widgets.
pub struct WidgetConfig {
    /// Font selection used by text-bearing widgets.
    pub font: FontChoice,
    /// Widget options applied during interaction and painting.
    pub opt: WidgetOption,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self::new(WidgetOption::NONE)
    }
}

impl WidgetConfig {
    /// Creates a config with the body font and explicit widget options.
    pub const fn new(opt: WidgetOption) -> Self {
        Self {
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Sets the font selection.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Implements the common [`Widget`] forwarding methods for built-in widget state types.
macro_rules! implement_widget {
    ($ty:ty, $update:ident, $paint:ident, $measure:ident) => {
        impl Widget for $ty {
            fn widget_opt(&self) -> &WidgetOption {
                &self.config.opt
            }
            fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
                self.$measure(style, atlas, avail)
            }
            fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Vec<UiInputEvent>) -> ResourceState {
                self.$update(ctx, &input)
            }
            fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
                self.$paint(ctx);
            }
        }
    };
}

// Widget implementations stay grouped here, while the shared execution context now
// lives in `widget_ctx.rs` beside the widget runtime traits.

mod core_widgets;
mod display;
mod nodes;
mod slider;
mod text_area;
mod text_edit;
mod textbox;

pub use core_widgets::{Button, ButtonContent, Checkbox, Combo, Custom, ListBox, ListItem};
pub use display::{ColorSwatch, TextBlock};
pub use nodes::{Node, NodeStateValue};
pub use slider::{Number, Slider};
pub use text_area::TextArea;
pub use textbox::Textbox;
