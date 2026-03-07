//! High-level layout helpers layered over the shared `LayoutManager`.

use super::*;

impl Container {
    /// Temporarily overrides the row definition and restores it after `f` executes.
    pub fn with_row<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], height: SizePolicy, f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.row(widths, height);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Sets the active row flow without restoring the previous flow.
    ///
    /// This matches legacy header/tree behavior where flow changes persist for
    /// subsequent layout calls in the current scope.
    pub fn set_row_flow(&mut self, widths: &[SizePolicy], height: SizePolicy) {
        self.layout.row(widths, height);
    }

    /// Temporarily overrides the layout with explicit column and row tracks and restores it after `f`.
    ///
    /// Widgets are emitted row-major within the provided track matrix.
    pub fn with_grid<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.grid(widths, heights);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Temporarily uses a vertical stack flow and restores the previous flow after `f` executes.
    ///
    /// Each `next_cell`/widget call in the scope gets a dedicated row using `height`.
    /// Width defaults to `Remainder(0)` so cells fill available horizontal space.
    pub fn stack<F: FnOnce(&mut Self)>(&mut self, height: SizePolicy, f: F) {
        self.stack_with_width_direction(SizePolicy::Remainder(0), height, StackDirection::TopToBottom, f);
    }

    /// Same as [`Container::stack`], but controls whether items are emitted top-down or bottom-up.
    pub fn stack_direction<F: FnOnce(&mut Self)>(&mut self, height: SizePolicy, direction: StackDirection, f: F) {
        self.stack_with_width_direction(SizePolicy::Remainder(0), height, direction, f);
    }

    /// Same as [`Container::stack`], but allows overriding the stack cell width policy.
    pub fn stack_with_width<F: FnOnce(&mut Self)>(&mut self, width: SizePolicy, height: SizePolicy, f: F) {
        self.stack_with_width_direction(width, height, StackDirection::TopToBottom, f);
    }

    /// Same as [`Container::stack_with_width`], but controls whether items are emitted top-down or bottom-up.
    pub fn stack_with_width_direction<F: FnOnce(&mut Self)>(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        if direction == StackDirection::TopToBottom {
            self.layout.stack(width, height);
        } else {
            self.layout.stack_with_direction(width, height, direction);
        }
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Creates a nested column scope where each call to `next_cell` yields a single column.
    pub fn column<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.layout.begin_column();
        f(self);
        self.layout.end_column();
    }

    /// Temporarily applies horizontal indentation for nested scopes.
    pub fn with_indent<F: FnOnce(&mut Self)>(&mut self, delta: i32, f: F) {
        self.layout.adjust_indent(delta);
        f(self);
        self.layout.adjust_indent(-delta);
    }

    /// Returns the next raw layout cell rectangle.
    ///
    /// Unlike widget helper methods (`button`, `textbox`, etc.), this does not consult
    /// a widget's `preferred_size`; it uses only the current row/column policies.
    pub fn next_cell(&mut self) -> Recti {
        self.layout.next()
    }
}
