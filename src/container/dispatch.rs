//! Generic widget dispatch used by both ad-hoc widget calls and retained trees.

use super::*;

impl Container {
    pub(crate) fn run_widget<W: Widget + ?Sized>(
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
        let res = state.handle(&mut ctx, &control);
        results.record(widget_id, res);
        (control, res)
    }

    pub(crate) fn next_widget_rect<W: Widget + ?Sized>(&mut self, state: &W) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = state.preferred_size(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_preferred(preferred)
    }

    pub(crate) fn handle_widget<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, state: &mut W, input: Option<Rc<InputSnapshot>>) -> ResourceState {
        let rect = self.next_widget_rect(state);
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let (_, res) = self.run_widget(results, state, rect, input, opt, bopt);
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
        let (_, res) = self.run_widget(results, state, rect, input, opt, bopt);
        res
    }

    pub(crate) fn handle_widget_raw<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, state: &mut W) -> ResourceState {
        let rect = self.next_widget_rect(state);
        let opt = state.effective_widget_opt();
        let bopt = state.effective_behaviour_opt();
        let input = if state.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let (_, res) = self.run_widget(results, state, rect, input, opt, bopt);
        res
    }

    pub(crate) fn next_widget_rect_dyn(&mut self, widget: &dyn WidgetStateHandleDyn) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = widget.preferred_size(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_preferred(preferred)
    }

    pub(crate) fn run_widget_dyn(
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
        let res = widget.handle(&mut ctx, &control);
        results.record(widget_id, res);
        (control, res)
    }

    pub(crate) fn next_widget_rect_handle<W: Widget>(&mut self, handle: &WidgetHandle<W>) -> Recti {
        let state = handle.borrow();
        self.next_widget_rect(&*state)
    }

    pub(crate) fn run_widget_handle<W: Widget>(
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
            state.handle(&mut ctx, &control)
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
