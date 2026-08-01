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

/// Records one pending semantic event without wrapping at the counter boundary.
pub(crate) fn record_pending_event(pending: &mut u32) {
    *pending = pending.saturating_add(1);
}

/// Consumes exactly one pending semantic event.
pub(crate) fn take_pending_event(pending: &mut u32) -> bool {
    if *pending == 0 {
        return false;
    }
    *pending -= 1;
    true
}

// Widget implementations stay grouped here, while the shared execution context now
// lives in `widget_ctx.rs` beside the widget runtime traits.

mod core_widgets;
mod display;
mod slider;
mod text_area;
mod text_edit;
mod textbox;

pub use core_widgets::{
    Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonState, Checkbox, CheckboxBuilder, CheckboxParameters, CheckboxState, Combo, ComboBuilder,
    ComboParameters, ComboState, Custom, CustomBuilder, CustomParameters, ListBox, ListBoxBuilder, ListBoxParameters, ListBoxState, ListItem, ListItemBuilder,
    ListItemParameters, ListItemState,
};
pub use display::{ColorSwatch, ColorSwatchBuilder, ColorSwatchParameters, ColorSwatchState, TextBlock, TextBlockBuilder, TextBlockParameters, TextBlockState};
pub use slider::{Number, NumberBuilder, NumberParameters, NumberState, Slider, SliderBuilder, SliderParameters, SliderState};
pub use text_area::{TextArea, TextAreaBuilder, TextAreaParameters, TextAreaState};
pub use textbox::{Textbox, TextboxBuilder, TextboxParameters, TextboxState};
