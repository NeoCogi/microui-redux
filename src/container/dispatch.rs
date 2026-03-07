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
//! Generic widget dispatch used by both ad-hoc widget calls and retained trees.

use super::*;

impl Container {
    pub(crate) fn reconcile_widget<W: Widget + ?Sized>(&mut self, results: &FrameResults, state: &mut W) -> WidgetId {
        let widget_id = widget_id_of(state);
        state.reconcile(CommittedWidgetState::new(results.committed_state(widget_id)));
        widget_id
    }

    pub(crate) fn render_widget<W: Widget + ?Sized>(
        &mut self,
        results: &mut FrameResults,
        state: &mut W,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> (ControlState, ResourceState) {
        let widget_id = widget_id_of(state);
        let control = self.update_control_with_opts(widget_id, rect, opt, bopt);
        let mut ctx = self.widget_ctx(widget_id, rect, input);
        let res = state.render(&mut ctx, &control);
        results.record(widget_id, res);
        (control, res)
    }

    pub(crate) fn measure_widget_rect<W: Widget + ?Sized>(&mut self, state: &W) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = state.measure(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_preferred(preferred)
    }

    pub(crate) fn handle_widget<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, state: &mut W, input: Option<Rc<InputSnapshot>>) -> ResourceState {
        self.reconcile_widget(results, state);
        let rect = self.measure_widget_rect(state);
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let (_, res) = self.render_widget(results, state, rect, input, opt, bopt);
        res
    }

    pub(crate) fn handle_widget_in_rect<W: Widget + ?Sized>(
        &mut self,
        results: &mut FrameResults,
        state: &mut W,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> ResourceState {
        self.reconcile_widget(results, state);
        let (_, res) = self.render_widget(results, state, rect, input, opt, bopt);
        res
    }

    pub(crate) fn handle_widget_raw<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, state: &mut W) -> ResourceState {
        self.reconcile_widget(results, state);
        let rect = self.measure_widget_rect(state);
        let opt = state.effective_widget_opt();
        let bopt = state.effective_behaviour_opt();
        let input = if state.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let (_, res) = self.render_widget(results, state, rect, input, opt, bopt);
        res
    }

    pub(crate) fn reconcile_widget_dyn(&mut self, results: &FrameResults, widget: &dyn WidgetStateHandleDyn) -> WidgetId {
        let widget_id = widget.widget_id();
        widget.reconcile(CommittedWidgetState::new(results.committed_state(widget_id)));
        widget_id
    }

    pub(crate) fn measure_widget_rect_dyn(&mut self, widget: &dyn WidgetStateHandleDyn) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = widget.measure(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_preferred(preferred)
    }

    pub(crate) fn render_widget_dyn(
        &mut self,
        results: &mut FrameResults,
        widget: &dyn WidgetStateHandleDyn,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> (ControlState, ResourceState) {
        let widget_id = widget.widget_id();
        let control = self.update_control_with_opts(widget_id, rect, opt, bopt);
        let mut ctx = self.widget_ctx(widget_id, rect, input);
        let res = widget.render(&mut ctx, &control);
        results.record(widget_id, res);
        (control, res)
    }

    pub(crate) fn reconcile_widget_handle<W: Widget>(&mut self, results: &FrameResults, handle: &WidgetHandle<W>) -> WidgetId {
        let widget_id = {
            let state = handle.borrow();
            widget_id_of(&*state)
        };
        {
            let mut state = handle.borrow_mut();
            state.reconcile(CommittedWidgetState::new(results.committed_state(widget_id)));
        }
        widget_id
    }

    pub(crate) fn measure_widget_rect_handle<W: Widget>(&mut self, handle: &WidgetHandle<W>) -> Recti {
        let state = handle.borrow();
        self.measure_widget_rect(&*state)
    }

    pub(crate) fn render_widget_handle<W: Widget>(
        &mut self,
        results: &mut FrameResults,
        handle: &WidgetHandle<W>,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> (ControlState, ResourceState) {
        let widget_id = {
            let state = handle.borrow();
            widget_id_of(&*state)
        };
        let control = self.update_control_with_opts(widget_id, rect, opt, bopt);
        let mut ctx = self.widget_ctx(widget_id, rect, input);
        let res = {
            let mut state = handle.borrow_mut();
            state.render(&mut ctx, &control)
        };
        results.record(widget_id, res);
        (control, res)
    }

    /// Runs a widget in an explicit rectangle.
    pub fn widget_in_rect<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, widget: &mut W, rect: Recti) -> ResourceState {
        let opt = widget.effective_widget_opt();
        let bopt = widget.effective_behaviour_opt();
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        self.handle_widget_in_rect(results, widget, rect, input, opt, bopt)
    }

    /// Evaluates each widget state using the current flow.
    pub fn widgets(&mut self, results: &mut FrameResults, runs: &mut [WidgetRef<'_>]) {
        for widget in runs.iter_mut() {
            let _ = self.handle_widget_raw(results, &mut **widget);
        }
    }

    /// Emits a row flow and evaluates each widget run in order.
    pub fn row_widgets(&mut self, results: &mut FrameResults, widths: &[SizePolicy], height: SizePolicy, runs: &mut [WidgetRef<'_>]) {
        self.with_row(widths, height, |container| {
            container.widgets(results, runs);
        });
    }

    /// Emits a grid flow and evaluates each widget run in row-major order.
    pub fn grid_widgets(&mut self, results: &mut FrameResults, widths: &[SizePolicy], heights: &[SizePolicy], runs: &mut [WidgetRef<'_>]) {
        self.with_grid(widths, heights, |container| {
            container.widgets(results, runs);
        });
    }

    /// Emits a nested column scope and evaluates each widget run in order.
    pub fn column_widgets(&mut self, results: &mut FrameResults, runs: &mut [WidgetRef<'_>]) {
        self.column(|container| {
            container.widgets(results, runs);
        });
    }

    /// Emits a stack flow and evaluates each widget run in order.
    pub fn stack_widgets(&mut self, results: &mut FrameResults, width: SizePolicy, height: SizePolicy, direction: StackDirection, runs: &mut [WidgetRef<'_>]) {
        self.stack_with_width_direction(width, height, direction, |container| {
            container.widgets(results, runs);
        });
    }
}
