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

impl TraversalHost {
    /// Measures an erased retained widget handle and advances layout using the supplied node policy.
    pub(crate) fn measure_widget_rect_dyn_with_policy(&mut self, widget: &dyn WidgetStateHandleDyn, policy: Policy) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = widget.measure(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_policies(preferred, policy.width, policy.height)
    }

    /// Updates an erased retained widget and records its public frame result.
    pub(crate) fn update_node_dyn(
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
        let widget_handle_id = widget.widget_handle_id();
        let retained_id = self.retained_id_for_node(node_id);
        let control = self.update_control_for(retained_id, rect, opt, scroll_behavior, focus_policy);
        let mut ctx = self.widget_ctx_for(retained_id, rect, input);
        let res = widget.update(&mut ctx, &control);
        // Results are keyed by both node id and widget-handle id so app code can choose either
        // structural or state-handle lookup patterns.
        results.record_retained_with_context(retained_id, node_id, widget_handle_id, res, dispatch_site);
        (control, res)
    }

    /// Paints an erased retained widget using the control state produced during update.
    pub(crate) fn paint_node_dyn(
        &mut self,
        node_id: NodeId,
        widget: &dyn WidgetStateHandleDyn,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        control: &ControlState,
    ) {
        let retained_id = self.retained_id_for_node(node_id);
        let mut ctx = self.widget_ctx_for(retained_id, rect, input);
        widget.paint(&mut ctx, control);
    }

    /// Updates a framework-owned internal widget such as a scrollbar.
    pub(crate) fn update_internal_node<W: Widget + ?Sized>(&mut self, node_id: NodeId, widget: &mut W, rect: Recti) -> (ControlState, ResourceState) {
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let retained_id = self.retained_id_for_node(node_id);
        let control = self.update_control_for(retained_id, rect, opt, scroll_behavior, focus_policy);
        // Snapshot input only for widgets that need text/key state to keep common controls cheap.
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let mut ctx = self.widget_ctx_for(retained_id, rect, input);
        let res = widget.update(&mut ctx, &control);
        (control, res)
    }

    /// Updates a framework-owned control that is layered above child hover roots.
    pub(crate) fn update_internal_node_unblocked<W: Widget + ?Sized>(&mut self, node_id: NodeId, widget: &mut W, rect: Recti) -> (ControlState, ResourceState) {
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let retained_id = self.retained_id_for_node(node_id);
        let control = self.update_control_for_unblocked(retained_id, rect, opt, scroll_behavior, focus_policy);
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let mut ctx = self.widget_ctx_for(retained_id, rect, input);
        let res = widget.update(&mut ctx, &control);
        (control, res)
    }

    /// Paints a framework-owned internal widget.
    pub(crate) fn paint_internal_node<W: Widget + ?Sized>(&mut self, node_id: NodeId, widget: &mut W, rect: Recti, control: &ControlState) {
        let retained_id = self.retained_id_for_node(node_id);
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let mut ctx = self.widget_ctx_for(retained_id, rect, input);
        widget.paint(&mut ctx, control);
    }
}
