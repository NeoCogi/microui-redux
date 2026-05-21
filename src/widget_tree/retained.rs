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
    id::Id,
    input::{ControlState, ResourceState, ScrollBehavior, WidgetOption},
    style::Style,
    widget::{FocusPolicy, Widget},
    widget_ctx::WidgetCtx,
    CustomRenderCommand,
};

/// Shared ownership handle for retained widget state.
///
/// Cloning a handle shares one widget state object. Handles may be cloned freely for ownership
/// convenience, but the same handle must not be rendered in multiple tree positions during one
/// frame; duplicate dispatch will panic.
pub type WidgetHandle<T> = Rc<RefCell<T>>;

/// Wraps widget state into a retained handle.
///
/// The returned handle may be cloned to share ownership, but each frame may dispatch that handle
/// at most once.
pub fn widget_handle<T>(value: T) -> WidgetHandle<T> {
    Rc::new(RefCell::new(value))
}

/// Uses the shared handle allocation address as the stable widget-state id.
pub(crate) fn widget_handle_id<W>(handle: &WidgetHandle<W>) -> Id {
    Id::new(Rc::as_ptr(handle) as *const () as usize as u64)
}

/// Shared custom-render command object stored by retained custom nodes.
pub(crate) type TreeCustomRender = Rc<RefCell<Box<dyn CustomRenderCommand + 'static>>>;

/// Type-erased adapter for retained widget state handles.
pub(crate) trait WidgetStateHandleDyn {
    /// Returns the stable id of the wrapped widget handle.
    fn widget_handle_id(&self) -> Id;
    /// Returns the widget options after applying widget-specific effective-state overrides.
    fn effective_widget_opt(&self) -> WidgetOption;
    /// Returns the widget scroll behavior after applying widget-specific overrides.
    fn effective_scroll_behavior(&self) -> ScrollBehavior;
    /// Returns how the widget wants focus to be retained or released.
    fn focus_policy(&self) -> FocusPolicy;
    /// Measures the widget without mutating it.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    /// Returns whether update/paint need a captured input snapshot.
    fn needs_input_snapshot(&self) -> bool;
    /// Updates the widget through interior mutability.
    fn update(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
    /// Paints the widget through interior mutability.
    fn paint(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState);
}

/// Concrete erased adapter around a strongly typed widget handle.
struct WidgetStateHandle<W: Widget + 'static> {
    handle: WidgetHandle<W>,
}

impl<W: Widget + 'static> WidgetStateHandleDyn for WidgetStateHandle<W> {
    fn widget_handle_id(&self) -> Id {
        widget_handle_id(&self.handle)
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        let widget = self.handle.borrow();
        widget.effective_widget_opt()
    }

    fn effective_scroll_behavior(&self) -> ScrollBehavior {
        let widget = self.handle.borrow();
        widget.effective_scroll_behavior()
    }

    fn focus_policy(&self) -> FocusPolicy {
        let widget = self.handle.borrow();
        widget.focus_policy()
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let widget = self.handle.borrow();
        widget.measure(style, atlas, avail)
    }

    fn needs_input_snapshot(&self) -> bool {
        let widget = self.handle.borrow();
        widget.needs_input_snapshot()
    }

    fn update(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        // Borrow only for the duration of dispatch so later result recording cannot hold state.
        let mut widget = self.handle.borrow_mut();
        widget.update(ctx, control)
    }

    fn paint(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        // Paint may mutate retained widget state for caches such as text layout.
        let mut widget = self.handle.borrow_mut();
        widget.paint(ctx, control);
    }
}

/// Boxes a typed widget handle behind the retained traversal's erased dispatch trait.
pub(crate) fn erased_widget_state<W: Widget + 'static>(handle: WidgetHandle<W>) -> Box<dyn WidgetStateHandleDyn> {
    Box::new(WidgetStateHandle { handle })
}
