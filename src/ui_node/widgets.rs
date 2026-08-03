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
//! Built-in retained widgets.
//!
//! Each leaf widget separates one-shot construction parameters, application-facing persistent
//! state, and its concrete retained runtime. Applications keep weak [`crate::WidgetStateHandle`]
//! capabilities while the runtime remains the sole strong owner of state.

use crate::*;

mod button;
mod checkbox;
mod color_swatch;
mod combo;
mod control;
mod custom;
mod list_box;
mod list_item;
mod number;
mod numeric_edit;
mod pending_event;
mod slider;
mod text_area;
mod text_block;
mod text_edit;
mod textbox;

use control::{content_height, inline_content_size, layout_inline_content, layout_scaled_visual_content, scaled_visual_content_size, text_size, widget_fill_color};
pub(crate) use pending_event::{record_pending_event, take_pending_event};

pub use button::{Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonState};
pub use checkbox::{Checkbox, CheckboxBuilder, CheckboxParameters, CheckboxState};
pub use color_swatch::{ColorSwatch, ColorSwatchBuilder, ColorSwatchParameters, ColorSwatchState};
pub use combo::{Combo, ComboBuilder, ComboParameters, ComboState};
pub use custom::{Custom, CustomBuilder, CustomParameters};
pub use list_box::{ListBox, ListBoxBuilder, ListBoxParameters, ListBoxState};
pub use list_item::{ListItem, ListItemBuilder, ListItemParameters, ListItemState};
pub use number::{Number, NumberBuilder, NumberParameters, NumberState};
pub use slider::{Slider, SliderBuilder, SliderParameters, SliderState};
pub use text_area::{TextArea, TextAreaBuilder, TextAreaParameters, TextAreaState};
pub use text_block::{TextBlock, TextBlockBuilder, TextBlockParameters, TextBlockState};
pub use textbox::{Textbox, TextboxBuilder, TextboxParameters, TextboxState};

#[cfg(test)]
mod tests;
