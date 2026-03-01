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
use super::*;

// Layout internals follow a two-layer model:
// 1) `LayoutEngine` owns scope stack, coordinate transforms, and content extents.
// 2) `LayoutFlow` implementations (`RowFlow`, `StackFlow`) decide how local cells are emitted.
//
// This keeps scroll/extent bookkeeping centralized while allowing specialized placement logic.

/// Describes how a layout dimension should be resolved.
#[derive(Copy, Clone, Debug, PartialEq)]
/// Size policy used by rows and columns when resolving cells.
pub enum SizePolicy {
    /// Uses the default cell size defined by the style.
    Auto,
    /// Reserves a fixed number of pixels.
    Fixed(i32),
    /// Uses weighted distribution of the current row/column size.
    ///
    /// When multiple sibling tracks use `Weight`, each track receives
    /// `weight / total_weight` of the available track space.
    /// When no sibling weight context exists (single-track flows),
    /// values are interpreted on a `0..=100` scale.
    Weight(f32),
    /// Consumes the remaining space with an optional margin.
    Remainder(i32),
}

impl SizePolicy {
    fn clamp_weight(value: f32) -> f32 {
        if value.is_finite() { value.max(0.0) } else { 0.0 }
    }

    fn resolve_weight(weight: f32, reference_space: i32, total_weight: Option<f32>) -> i32 {
        let w = Self::clamp_weight(weight);
        if w <= 0.0 {
            return 0;
        }

        let denom = match total_weight {
            Some(total) if total.is_finite() && total > 0.0 => total,
            _ => 100.0,
        };
        let reference = reference_space.max(0) as f32;
        (reference * (w / denom)).floor() as i32
    }

    fn total_weight(policies: &[SizePolicy]) -> Option<f32> {
        let total = policies
            .iter()
            .map(|policy| match policy {
                SizePolicy::Weight(value) => Self::clamp_weight(*value),
                _ => 0.0,
            })
            .sum::<f32>();
        if total > 0.0 { Some(total) } else { None }
    }

    fn resolve(self, default_size: i32, available_space: i32) -> i32 {
        let resolved = match self {
            SizePolicy::Auto => default_size,
            SizePolicy::Fixed(value) => value,
            SizePolicy::Weight(weight) => Self::resolve_weight(weight, available_space, None),
            SizePolicy::Remainder(margin) => available_space.saturating_sub(margin),
        };
        resolved.max(0)
    }

    fn resolve_with_reference(self, default_size: i32, available_space: i32, reference_space: i32, total_weight: Option<f32>) -> i32 {
        let resolved = match self {
            SizePolicy::Weight(weight) => Self::resolve_weight(weight, reference_space, total_weight),
            _ => self.resolve(default_size, available_space),
        };
        resolved.max(0)
    }
}

impl Default for SizePolicy {
    fn default() -> Self {
        SizePolicy::Auto
    }
}

/// Direction used by stack flows when emitting vertical cells.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StackDirection {
    /// Place cells from the current row start downward.
    TopToBottom,
    /// Place cells from the bottom of the current scope upward.
    BottomToTop,
}

impl Default for StackDirection {
    fn default() -> Self {
        Self::TopToBottom
    }
}

#[derive(Clone, Default)]
struct ScopeState {
    // Scope rectangle expressed in local space (already offset by scroll).
    body: Recti,
    // Current cursor in local coordinates.
    cursor: Vec2i,
    // Max absolute extent reached by generated cells (for scroll/content sizing).
    max: Option<Vec2i>,
    // Y coordinate where the next logical line should start.
    next_row: i32,
    // Horizontal indentation applied to the scope.
    indent: i32,
}

impl ScopeState {
    // Reset cursor to the start of the next line while preserving active indentation.
    fn reset_cursor_for_next_row(&mut self) {
        self.cursor = vec2(self.indent, self.next_row);
    }
}

#[derive(Copy, Clone)]
struct ResolveCtx {
    // Global inter-cell spacing from style.
    spacing: i32,
    // Width fallback after preferred-size resolution.
    default_width: i32,
    // Height fallback after preferred-size resolution.
    default_height: i32,
}

trait LayoutFlow {
    // Produces the next local cell and advances scope-local cursors/state.
    fn next_local(&mut self, scope: &mut ScopeState, ctx: ResolveCtx) -> Recti;
}

#[derive(Clone)]
enum RowHeights {
    Uniform(SizePolicy),
    Tracks(Vec<SizePolicy>),
}

impl Default for RowHeights {
    fn default() -> Self {
        Self::Uniform(SizePolicy::Auto)
    }
}

#[derive(Clone, Default)]
struct RowFlow {
    // Width policy for each slot in the active row pattern.
    widths: Vec<SizePolicy>,
    // Height policy shared by all cells in the row pattern, or one policy per row track.
    heights: RowHeights,
    // Current slot index in `widths`.
    item_index: usize,
    // Current row index in the active pattern.
    row_index: usize,
}

impl RowFlow {
    fn new(widths: &[SizePolicy], height: SizePolicy) -> Self {
        Self::from_parts(widths.to_vec(), RowHeights::Uniform(height))
    }

    fn new_grid(widths: &[SizePolicy], heights: &[SizePolicy]) -> Self {
        Self::from_parts(widths.to_vec(), RowHeights::Tracks(heights.to_vec()))
    }

    fn from_parts(widths: Vec<SizePolicy>, heights: RowHeights) -> Self {
        Self {
            widths,
            heights,
            item_index: 0,
            row_index: 0,
        }
    }

    fn apply_template(&mut self, widths: Vec<SizePolicy>, heights: RowHeights) {
        self.widths = widths;
        self.heights = heights;
        self.item_index = 0;
        self.row_index = 0;
    }

    fn current_height_policy(&self) -> (SizePolicy, Option<i32>) {
        match &self.heights {
            RowHeights::Uniform(policy) => (*policy, None),
            RowHeights::Tracks(policies) => {
                if policies.is_empty() {
                    (SizePolicy::Auto, Some(1))
                } else {
                    let idx = self.row_index % policies.len();
                    (policies[idx], Some(policies.len() as i32))
                }
            }
        }
    }
}

impl LayoutFlow for RowFlow {
    fn next_local(&mut self, scope: &mut ScopeState, ctx: ResolveCtx) -> Recti {
        // Once all row slots are consumed, wrap to the next line and restart the pattern.
        let slot_count = self.widths.len().max(1);
        if self.item_index >= slot_count {
            self.item_index = 0;
            self.row_index = self.row_index.saturating_add(1);
            scope.reset_cursor_for_next_row();
        }

        // Empty width patterns are treated as a single Auto slot.
        let width_policy = if self.widths.is_empty() {
            SizePolicy::Auto
        } else {
            self.widths.get(self.item_index).copied().unwrap_or(SizePolicy::Auto)
        };
        let (height_policy, row_count_hint) = self.current_height_policy();

        let x = scope.cursor.x;
        let y = scope.cursor.y;
        let slot_count = slot_count as i32;
        let row_spacing = ctx.spacing.saturating_mul(slot_count.saturating_sub(1));
        let row_reference_width = scope.body.width.saturating_sub(scope.indent).saturating_sub(row_spacing);
        let row_width_weight = SizePolicy::total_weight(&self.widths);
        let row_reference_height = match row_count_hint {
            Some(row_count) => scope.body.height.saturating_sub(ctx.spacing.saturating_mul(row_count.saturating_sub(1))),
            None => scope.body.height,
        };
        let row_height_weight = match &self.heights {
            RowHeights::Tracks(policies) => SizePolicy::total_weight(policies),
            RowHeights::Uniform(_) => None,
        };

        // Resolve dimensions from policy + remaining space inside scope bounds.
        let available_width = scope.body.width.saturating_sub(x);
        let available_height = scope.body.height.saturating_sub(y);
        let width = width_policy.resolve_with_reference(ctx.default_width, available_width, row_reference_width, row_width_weight);
        let height = height_policy.resolve_with_reference(ctx.default_height, available_height, row_reference_height, row_height_weight);

        self.item_index = self.item_index.saturating_add(1);

        // Advance cursor to the right and grow the next-line marker by the tallest seen cell.
        scope.cursor.x = scope.cursor.x.saturating_add(width).saturating_add(ctx.spacing);
        let line_end = y.saturating_add(height).saturating_add(ctx.spacing);
        scope.next_row = max(scope.next_row, line_end);

        rect(x, y, width, height)
    }
}

#[derive(Clone)]
struct StackFlow {
    // Width policy used for every stacked item.
    width: SizePolicy,
    // Height policy used for every stacked item.
    height: SizePolicy,
    // Vertical direction for cell emission.
    direction: StackDirection,
    // Offset consumed from the stack anchor (used by bottom-up stacks).
    offset: i32,
}

impl Default for StackFlow {
    fn default() -> Self {
        Self {
            width: SizePolicy::Remainder(0),
            height: SizePolicy::Auto,
            direction: StackDirection::TopToBottom,
            offset: 0,
        }
    }
}

impl StackFlow {
    fn new(width: SizePolicy, height: SizePolicy, direction: StackDirection) -> Self {
        Self { width, height, direction, offset: 0 }
    }

    fn apply_template(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection) {
        self.width = width;
        self.height = height;
        self.direction = direction;
        self.offset = 0;
    }
}

impl LayoutFlow for StackFlow {
    fn next_local(&mut self, scope: &mut ScopeState, ctx: ResolveCtx) -> Recti {
        let x = scope.indent;
        let available_width = scope.body.width.saturating_sub(x);
        let width = self.width.resolve_with_reference(ctx.default_width, available_width, available_width, None);

        match self.direction {
            StackDirection::TopToBottom => {
                // Top-down stacks continue from the scope's row cursor.
                let y = scope.next_row;
                let available_height = scope.body.height.saturating_sub(y);
                let height = self
                    .height
                    .resolve_with_reference(ctx.default_height, available_height, scope.body.height, None);

                // Move directly to the next stacked row.
                let next = y.saturating_add(height).saturating_add(ctx.spacing);
                scope.next_row = next;
                scope.cursor = vec2(scope.indent, next);

                rect(x, y, width, height)
            }
            StackDirection::BottomToTop => {
                // Bottom-up stacks are anchored to the scope bottom and use local offset.
                let available_height = scope.body.height.saturating_sub(self.offset);
                let height = self
                    .height
                    .resolve_with_reference(ctx.default_height, available_height, scope.body.height, None);
                let y = scope.body.height.saturating_sub(self.offset).saturating_sub(height);
                self.offset = self.offset.saturating_add(height).saturating_add(ctx.spacing);
                rect(x, y, width, height)
            }
        }
    }
}

#[derive(Clone)]
enum FlowState {
    // Repeating row pattern with N slots.
    Row(RowFlow),
    // One-cell-per-line vertical stack.
    Stack(StackFlow),
}

impl Default for FlowState {
    fn default() -> Self {
        FlowState::Row(RowFlow::new(&[SizePolicy::Auto], SizePolicy::Auto))
    }
}

impl FlowState {
    // Store a flow as a lightweight template so scoped overrides can be restored later.
    fn as_template(&self) -> FlowTemplate {
        match self {
            FlowState::Row(row) => FlowTemplate::Row {
                widths: row.widths.clone(),
                heights: row.heights.clone(),
            },
            FlowState::Stack(stack) => FlowTemplate::Stack {
                width: stack.width,
                height: stack.height,
                direction: stack.direction,
            },
        }
    }

    fn apply_template(&mut self, template: FlowTemplate) {
        match template {
            FlowTemplate::Row { widths, heights } => match self {
                FlowState::Row(row) => row.apply_template(widths, heights),
                _ => {
                    *self = FlowState::Row(RowFlow::from_parts(widths, heights));
                }
            },
            FlowTemplate::Stack { width, height, direction } => match self {
                FlowState::Stack(stack) => stack.apply_template(width, height, direction),
                _ => {
                    *self = FlowState::Stack(StackFlow::new(width, height, direction));
                }
            },
        }
    }

    // Delegate cell generation to the active flow implementation.
    fn next_local(&mut self, scope: &mut ScopeState, ctx: ResolveCtx) -> Recti {
        match self {
            FlowState::Row(flow) => flow.next_local(scope, ctx),
            FlowState::Stack(flow) => flow.next_local(scope, ctx),
        }
    }
}

#[derive(Clone)]
struct LayoutFrame {
    // Coordinates/cursors for one nested layout scope.
    scope: ScopeState,
    // Placement logic used for this scope.
    flow: FlowState,
}

impl LayoutFrame {
    fn new(body: Recti, scroll: Vec2i) -> Self {
        Self {
            scope: ScopeState {
                // Scope body is shifted by scroll so local coordinates remain stable while content moves.
                body: rect(body.x - scroll.x, body.y - scroll.y, body.width, body.height),
                cursor: vec2(0, 0),
                max: None,
                next_row: 0,
                indent: 0,
            },
            flow: FlowState::default(),
        }
    }
}

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

    pub fn reset(&mut self, body: Recti, scroll: Vec2i) {
        self.stack.clear();
        self.last_rect = Recti::default();
        // Root scope starts with default row flow.
        self.push_scope_with_flow(body, scroll, FlowState::default());
    }

    pub fn set_default_cell_height(&mut self, height: i32) {
        self.default_cell_height = height.max(0);
    }

    pub fn current_body(&self) -> Recti {
        self.top().scope.body
    }

    pub fn current_max(&self) -> Option<Vec2i> {
        self.top().scope.max
    }

    pub fn pop_scope(&mut self) {
        self.stack.pop();
    }

    pub fn adjust_indent(&mut self, delta: i32) {
        self.top_mut().scope.indent += delta;
    }

    pub fn begin_column(&mut self) {
        // A column is allocated from the parent as one cell, then becomes a nested scope.
        let layout_rect = self.next();
        self.push_scope_with_flow(layout_rect, vec2(0, 0), FlowState::Row(RowFlow::new(&[SizePolicy::Auto], SizePolicy::Auto)));
    }

    pub fn end_column(&mut self) {
        let finished = self.stack.pop().expect("cannot end column without an active child layout");
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
    }

    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy) {
        let frame = self.top_mut();
        frame.flow = FlowState::Row(RowFlow::new(widths, height));
        // Applying a new flow resets placement to the current line start.
        frame.scope.reset_cursor_for_next_row();
    }

    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy]) {
        let frame = self.top_mut();
        frame.flow = FlowState::Row(RowFlow::new_grid(widths, heights));
        // Applying a new flow resets placement to the current line start.
        frame.scope.reset_cursor_for_next_row();
    }

    pub fn stack(&mut self, width: SizePolicy, height: SizePolicy) {
        self.stack_with_direction(width, height, StackDirection::TopToBottom);
    }

    pub fn stack_with_direction(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection) {
        let frame = self.top_mut();
        frame.flow = FlowState::Stack(StackFlow::new(width, height, direction));
        // Applying a new flow resets placement to the current line start.
        frame.scope.reset_cursor_for_next_row();
    }

    pub(crate) fn snapshot_flow_state(&self) -> FlowSnapshot {
        FlowSnapshot::from_layout(self.top())
    }

    pub(crate) fn restore_flow_state(&mut self, snapshot: FlowSnapshot) {
        snapshot.apply(self.top_mut());
    }

    pub fn next(&mut self) -> Recti {
        self.next_with_preferred(Dimensioni::new(0, 0))
    }

    pub fn next_with_preferred(&mut self, preferred: Dimensioni) -> Recti {
        let spacing = self.style.spacing;
        let (default_width, default_height) = self.fallback_dimensions(preferred);
        let mut local = {
            let frame = self.top_mut();
            let ctx = ResolveCtx { spacing, default_width, default_height };
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

#[derive(Clone)]
enum FlowTemplate {
    // Snapshot for row flow configuration.
    Row {
        widths: Vec<SizePolicy>,
        heights: RowHeights,
    },
    // Snapshot for stack flow configuration.
    Stack {
        width: SizePolicy,
        height: SizePolicy,
        direction: StackDirection,
    },
}

pub(crate) struct FlowSnapshot {
    // Captures active flow configuration for scoped overrides.
    flow: FlowTemplate,
}

impl FlowSnapshot {
    fn from_layout(layout: &LayoutFrame) -> Self {
        Self { flow: layout.flow.as_template() }
    }

    fn apply(self, layout: &mut LayoutFrame) {
        layout.flow.apply_template(self.flow);
    }
}

pub(crate) type LayoutManager = LayoutEngine;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_next_advances_row() {
        let mut layout = LayoutManager::default();
        layout.style = Style::default();
        let body = rect(0, 0, 100, 100);
        layout.reset(body, vec2(0, 0));
        layout.set_default_cell_height(10);
        layout.row(&[SizePolicy::Auto], SizePolicy::Auto);

        let first = layout.next();
        let second = layout.next();

        let expected_width = layout.style.default_cell_width + layout.style.padding * 2;
        assert_eq!(first.x, body.x);
        assert_eq!(first.y, body.y);
        assert_eq!(first.width, expected_width);
        assert_eq!(first.height, 10);
        assert_eq!(second.x, body.x);
        assert_eq!(second.y, body.y + first.height + layout.style.spacing);
    }

    #[test]
    fn layout_remainder_consumes_available_width() {
        let mut layout = LayoutManager::default();
        layout.style = Style::default();
        let body = rect(0, 0, 120, 40);
        layout.reset(body, vec2(0, 0));
        layout.set_default_cell_height(10);
        layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Fixed(10));

        let cell = layout.next();
        assert_eq!(cell.width, body.width);
        assert_eq!(cell.height, 10);
    }

    #[test]
    fn stack_flow_uses_full_width_by_default() {
        let mut layout = LayoutManager::default();
        layout.style = Style::default();
        let body = rect(0, 0, 120, 60);
        layout.reset(body, vec2(0, 0));
        layout.set_default_cell_height(10);
        layout.stack(SizePolicy::Remainder(0), SizePolicy::Auto);

        let first = layout.next();
        let second = layout.next();

        assert_eq!(first.width, body.width);
        assert_eq!(second.y, first.y + first.height + layout.style.spacing);
    }

    #[test]
    fn stack_flow_bottom_to_top_anchors_to_scope_bottom() {
        let mut layout = LayoutManager::default();
        layout.style = Style::default();
        let body = rect(0, 0, 120, 60);
        layout.reset(body, vec2(0, 0));
        layout.set_default_cell_height(10);
        layout.stack_with_direction(SizePolicy::Remainder(0), SizePolicy::Fixed(10), StackDirection::BottomToTop);

        let first = layout.next();
        let second = layout.next();

        assert_eq!(first.width, body.width);
        assert_eq!(first.y, body.y + body.height - 10);
        assert_eq!(second.y, first.y - (10 + layout.style.spacing));
    }

    #[test]
    fn row_weight_divides_usable_row_space() {
        let mut layout = LayoutManager::default();
        layout.style = Style::default();
        let body = rect(0, 0, 200, 80);
        layout.reset(body, vec2(0, 0));
        layout.set_default_cell_height(10);
        layout.row(
            &[
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
            ],
            SizePolicy::Fixed(10),
        );

        let a = layout.next();
        let b = layout.next();
        let c = layout.next();
        let d = layout.next();
        let expected = (body.width - layout.style.spacing * 3) / 4;

        assert_eq!(a.width, expected);
        assert_eq!(b.width, expected);
        assert_eq!(c.width, expected);
        assert_eq!(d.width, expected);
        assert_eq!(d.x + d.width, body.x + body.width);
    }

    #[test]
    fn stack_weight_without_siblings_uses_100_scale() {
        let mut layout = LayoutManager::default();
        layout.style = Style::default();
        let body = rect(0, 0, 120, 60);
        layout.reset(body, vec2(0, 0));
        layout.set_default_cell_height(10);
        layout.stack(SizePolicy::Weight(50.0), SizePolicy::Weight(25.0));

        let first = layout.next();
        let second = layout.next();

        assert_eq!(first.width, 60);
        assert_eq!(first.height, 15);
        assert_eq!(second.width, 60);
        assert_eq!(second.height, 15);
    }

    #[test]
    fn grid_weight_is_symmetric_across_axes() {
        let mut layout = LayoutManager::default();
        layout.style = Style::default();
        let body = rect(0, 0, 200, 100);
        layout.reset(body, vec2(0, 0));
        layout.set_default_cell_height(10);
        layout.grid(
            &[SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)],
            &[SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)],
        );

        let a = layout.next();
        let b = layout.next();
        let c = layout.next();
        let d = layout.next();

        let expected_width = (body.width - layout.style.spacing) / 2;
        let expected_height = (body.height - layout.style.spacing) / 2;

        assert_eq!(a.width, expected_width);
        assert_eq!(b.width, expected_width);
        assert_eq!(c.width, expected_width);
        assert_eq!(d.width, expected_width);

        assert_eq!(a.height, expected_height);
        assert_eq!(b.height, expected_height);
        assert_eq!(c.height, expected_height);
        assert_eq!(d.height, expected_height);
    }
}
