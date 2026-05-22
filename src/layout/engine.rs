//! Layout engine orchestration and public layout methods.

use std::cmp::max;

use crate::{vec2, Dimensioni, Recti, Style, Vec2i};

use super::{
    flow::{FlowState, ResolveCtx, RowFlow, StackFlow},
    frame::LayoutFrame,
    FlowSnapshot, SizePolicy, StackDirection,
};

#[derive(Clone, Default)]
pub(crate) struct LayoutEngine {
    // Style snapshot used by resolution rules (spacing/default widths/padding fallbacks).
    pub style: Style,
    // Last emitted absolute rectangle.
    pub last_rect: Recti,
    // Default control height seeded by container setup.
    default_cell_height: i32,
    // Nested scope stack (window body, columns, etc.).
    stack: Vec<LayoutFrame>,
}

impl LayoutEngine {
    // Pushes a scope with an explicit flow (used by reset/column).
    fn push_scope_with_flow(&mut self, body: Recti, scroll: Vec2i, flow: FlowState) {
        let mut frame = LayoutFrame::new(body, scroll);
        frame.flow = flow;
        self.stack.push(frame);
    }

    fn top(&self) -> &LayoutFrame {
        self.stack.last().expect("Layout stack should never be empty when accessed")
    }

    fn top_mut(&mut self) -> &mut LayoutFrame {
        self.stack.last_mut().expect("Layout stack should never be empty when accessed")
    }

    /// Chooses effective fallback dimensions for a widget that reported no preferred size.
    fn fallback_dimensions(&self, preferred: Dimensioni) -> (i32, i32) {
        let padding = self.style.padding;
        // Width fallback mirrors legacy behavior: default width + horizontal padding.
        let fallback_width = self.style.default_cell_width + padding * 2;
        // Height fallback prefers container-provided default cell height, then padding-only fallback.
        let base_height = if self.default_cell_height > 0 { self.default_cell_height } else { 0 };
        let fallback_height = if base_height > 0 { base_height } else { padding * 2 };

        let default_width = if preferred.width > 0 { preferred.width } else { fallback_width };
        let default_height = if preferred.height > 0 { preferred.height } else { fallback_height };
        (default_width, default_height)
    }

    /// Clears all scopes and starts a fresh root body using the current scroll offset.
    pub fn reset(&mut self, body: Recti, scroll: Vec2i) {
        self.stack.clear();
        self.last_rect = Recti::default();
        // Root scope starts with default row flow.
        self.push_scope_with_flow(body, scroll, FlowState::default());
    }

    /// Stores the default control height used when widgets report zero preferred height.
    pub fn set_default_cell_height(&mut self, height: i32) {
        self.default_cell_height = height.max(0);
    }

    /// Returns the absolute body rectangle for the active layout scope.
    pub fn current_body(&self) -> Recti {
        self.top().scope.body
    }

    /// Returns the largest absolute content extent seen in the active scope.
    pub fn current_max(&self) -> Option<Vec2i> {
        self.top().scope.max
    }

    /// Pops the active layout scope.
    pub fn pop_scope(&mut self) {
        self.stack.pop();
    }

    /// Adjusts horizontal indentation in the active scope.
    pub fn adjust_indent(&mut self, delta: i32) {
        self.top_mut().scope.indent += delta;
    }

    /// Allocates a child cell and starts a nested column scope inside it.
    pub fn begin_column(&mut self) {
        // A column is allocated from the parent as one cell, then becomes a nested scope.
        let layout_rect = self.next();
        self.push_scope_with_flow(layout_rect, vec2(0, 0), FlowState::Row(RowFlow::new(&[SizePolicy::Auto], SizePolicy::Auto)));
    }

    /// Ends the active column scope and merges its extents into the parent scope.
    pub fn end_column(&mut self) {
        self.end_nested_scope("cannot end column without an active child layout");
    }

    /// Allocates a scoped retained-tree node using explicit layout policies.
    pub(crate) fn begin_node_scope_with_policies(&mut self, preferred: Dimensioni, width: SizePolicy, height: SizePolicy) -> Recti {
        let layout_rect = self.next_with_policies(preferred, width, height);
        self.push_scope_with_flow(layout_rect, vec2(0, 0), FlowState::default());
        layout_rect
    }

    /// Ends a retained-tree node scope and returns its measured content size.
    pub(crate) fn end_node_scope(&mut self) -> Dimensioni {
        self.end_nested_scope("cannot end node scope without an active child layout")
    }

    /// Pops a nested scope, returns its content size, and merges cursor/max state upward.
    fn end_nested_scope(&mut self, panic_message: &'static str) -> Dimensioni {
        let finished = self.stack.pop().expect(panic_message);
        let content_size = finished
            .scope
            .max
            .map(|max_rect| Dimensioni::new((max_rect.x - finished.scope.body.x).max(0), (max_rect.y - finished.scope.body.y).max(0)))
            .unwrap_or_default();
        let parent = self.top_mut();

        // Merge child cursor/row extents back into parent-local space.
        let child_position_x = finished.scope.cursor.x + finished.scope.body.x - parent.scope.body.x;
        let child_next_row = finished.scope.next_row + finished.scope.body.y - parent.scope.body.y;

        parent.scope.cursor.x = max(parent.scope.cursor.x, child_position_x);
        parent.scope.next_row = max(parent.scope.next_row, child_next_row);

        // Merge absolute max extents for content-size/scroll calculations.
        match (&mut parent.scope.max, finished.scope.max) {
            (None, None) => (),
            (Some(_), None) => (),
            (None, Some(m)) => parent.scope.max = Some(m),
            (Some(am), Some(bm)) => {
                parent.scope.max = Some(Vec2i::new(max(am.x, bm.x), max(am.y, bm.y)));
            }
        }

        content_size
    }

    /// Switches the active scope to a repeating row template.
    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy) {
        let frame = self.top_mut();
        frame.flow = FlowState::Row(RowFlow::new(widths, height));
        // Applying a new flow resets placement to the current line start.
        frame.scope.reset_cursor_for_next_row();
    }

    /// Switches the active scope to a grid template.
    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy]) {
        let frame = self.top_mut();
        frame.flow = FlowState::Row(RowFlow::new_grid(widths, heights));
        // Applying a new flow resets placement to the current line start.
        frame.scope.reset_cursor_for_next_row();
    }

    /// Switches the active scope to a top-to-bottom stack.
    pub fn stack(&mut self, width: SizePolicy, height: SizePolicy) {
        self.stack_with_direction(width, height, StackDirection::TopToBottom);
    }

    /// Switches the active scope to a stack with explicit direction.
    pub fn stack_with_direction(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection) {
        let frame = self.top_mut();
        frame.flow = FlowState::Stack(StackFlow::new(width, height, direction));
        // Applying a new flow resets placement to the current line start.
        frame.scope.reset_cursor_for_next_row();
    }

    /// Captures the active flow so temporary node-level layout overrides can be restored.
    pub(crate) fn snapshot_flow_state(&self) -> FlowSnapshot {
        FlowSnapshot::from_layout(self.top())
    }

    /// Restores a previously captured flow snapshot.
    pub(crate) fn restore_flow_state(&mut self, snapshot: FlowSnapshot) {
        snapshot.apply(self.top_mut());
    }

    /// Allocates the next cell using automatic preferred dimensions.
    pub fn next(&mut self) -> Recti {
        self.next_with_preferred(Dimensioni::new(0, 0))
    }

    /// Allocates the next cell using a widget-provided preferred size.
    pub fn next_with_preferred(&mut self, preferred: Dimensioni) -> Recti {
        self.next_with_policies(preferred, SizePolicy::Auto, SizePolicy::Auto)
    }

    /// Allocates the next cell using preferred size plus explicit node policy overrides.
    pub(crate) fn next_with_policies(&mut self, preferred: Dimensioni, width: SizePolicy, height: SizePolicy) -> Recti {
        let spacing = self.style.spacing;
        let (default_width, default_height) = self.fallback_dimensions(preferred);
        let width_override = match width {
            SizePolicy::Auto => None,
            policy => Some(policy),
        };
        let height_override = match height {
            SizePolicy::Auto => None,
            policy => Some(policy),
        };
        let mut local = {
            let frame = self.top_mut();
            let ctx = ResolveCtx {
                spacing,
                default_width,
                default_height,
                width_override,
                height_override,
            };
            frame.flow.next_local(&mut frame.scope, ctx)
        };

        // Convert local cell coordinates into absolute container coordinates.
        let origin = {
            let frame = self.top();
            vec2(frame.scope.body.x, frame.scope.body.y)
        };

        local.x += origin.x;
        local.y += origin.y;

        {
            let frame = self.top_mut();
            // Track absolute max extent reached by emitted content.
            match frame.scope.max {
                None => frame.scope.max = Some(Vec2i::new(local.x + local.width, local.y + local.height)),
                Some(am) => {
                    frame.scope.max = Some(Vec2i::new(max(am.x, local.x + local.width), max(am.y, local.y + local.height)));
                }
            }
        }

        self.last_rect = local;
        self.last_rect
    }
}

pub(crate) type LayoutManager = LayoutEngine;
