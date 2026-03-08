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
//! Retained widget handles and erased dispatch adapters.

use std::{cell::RefCell, rc::Rc};

use rs_math3d::Dimensioni;

use crate::{
    atlas::AtlasHandle,
    input::{ControlState, ResourceState, WidgetBehaviourOption, WidgetOption},
    style::Style,
    widget::{widget_id_of, Widget, WidgetId},
    widget_ctx::WidgetCtx,
    CustomRenderArgs,
};

/// Shared ownership handle for retained widget state.
pub type WidgetHandle<T> = Rc<RefCell<T>>;

/// Wraps widget state into a retained handle.
pub fn widget_handle<T>(value: T) -> WidgetHandle<T> {
    Rc::new(RefCell::new(value))
}

pub(crate) type TreeCustomRender = Rc<RefCell<Box<dyn FnMut(Dimensioni, &CustomRenderArgs) + 'static>>>;

pub(crate) trait WidgetStateHandleDyn {
    fn widget_id(&self) -> WidgetId;
    fn effective_widget_opt(&self) -> WidgetOption;
    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption;
    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    fn needs_input_snapshot(&self) -> bool;
    fn run(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
}

struct WidgetStateHandle<W: Widget + 'static> {
    handle: WidgetHandle<W>,
}

impl<W: Widget + 'static> WidgetStateHandleDyn for WidgetStateHandle<W> {
    fn widget_id(&self) -> WidgetId {
        let widget = self.handle.borrow();
        widget_id_of(&*widget)
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        let widget = self.handle.borrow();
        widget.effective_widget_opt()
    }

    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption {
        let widget = self.handle.borrow();
        widget.effective_behaviour_opt()
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let widget = self.handle.borrow();
        widget.measure(style, atlas, avail)
    }

    fn needs_input_snapshot(&self) -> bool {
        let widget = self.handle.borrow();
        widget.needs_input_snapshot()
    }

    fn run(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut widget = self.handle.borrow_mut();
        widget.run(ctx, control)
    }
}

pub(crate) fn erased_widget_state<W: Widget + 'static>(handle: WidgetHandle<W>) -> Box<dyn WidgetStateHandleDyn> {
    Box::new(WidgetStateHandle { handle })
}
