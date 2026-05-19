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
//! Generic widget dispatch used by retained traversal and widget internals.

use super::*;

impl Container {
    pub(crate) fn measure_widget_rect_with_policy<W: Widget + ?Sized>(&mut self, state: &W, policy: Policy) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = state.measure(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_policies(preferred, policy.width, policy.height)
    }

    pub(crate) fn measure_widget_rect_dyn_with_policy(&mut self, widget: &dyn WidgetStateHandleDyn, policy: Policy) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = widget.measure(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_policies(preferred, policy.width, policy.height)
    }

    pub(crate) fn execute_node_dyn(
        &mut self,
        results: &mut FrameResults,
        node_id: NodeId,
        widget: &dyn WidgetStateHandleDyn,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        scroll_behavior: ScrollBehavior,
        focus_policy: FocusPolicy,
        dispatch_site: String,
    ) -> (ControlState, ResourceState) {
        let widget_id = widget.widget_id();
        self.record_widget_node(widget_id, node_id);
        let retained_id = self.retained_id_for_node(node_id);
        let control = self.update_control_for(retained_id, rect, opt, scroll_behavior, focus_policy);
        let mut ctx = self.widget_ctx_for(widget_id, retained_id, rect, input);
        let res = widget.update(&mut ctx, &control);
        widget.paint(&mut ctx, &control);
        results.record_retained_with_context(retained_id, node_id, widget_id, res, dispatch_site);
        (control, res)
    }

    pub(crate) fn execute_internal_node<W: Widget + ?Sized>(&mut self, node_id: NodeId, widget: &mut W, rect: Recti) -> (ControlState, ResourceState) {
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let retained_id = self.retained_id_for_node(node_id);
        let control = self.update_control_for(retained_id, rect, opt, scroll_behavior, focus_policy);
        let widget_id = node_id.raw() as WidgetId;
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let mut ctx = self.widget_ctx_for(widget_id, retained_id, rect, input);
        let res = widget.update(&mut ctx, &control);
        widget.paint(&mut ctx, &control);
        (control, res)
    }

    pub(crate) fn measure_widget_rect_handle_with_policy<W: Widget>(&mut self, handle: &WidgetHandle<W>, policy: Policy) -> Recti {
        let state = handle.borrow();
        self.measure_widget_rect_with_policy(&*state, policy)
    }

    pub(crate) fn execute_node_handle<W: Widget>(
        &mut self,
        results: &mut FrameResults,
        node_id: NodeId,
        handle: &WidgetHandle<W>,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        scroll_behavior: ScrollBehavior,
        focus_policy: FocusPolicy,
        dispatch_site: String,
    ) -> (ControlState, ResourceState) {
        let widget_id = {
            let state = handle.borrow();
            widget_id_of(&*state)
        };
        self.record_widget_node(widget_id, node_id);
        let retained_id = self.retained_id_for_node(node_id);
        let control = self.update_control_for(retained_id, rect, opt, scroll_behavior, focus_policy);
        let mut ctx = self.widget_ctx_for(widget_id, retained_id, rect, input);
        let res = {
            let mut state = handle.borrow_mut();
            let res = state.update(&mut ctx, &control);
            state.paint(&mut ctx, &control);
            res
        };
        results.record_retained_with_context(retained_id, node_id, widget_id, res, dispatch_site);
        (control, res)
    }
}
