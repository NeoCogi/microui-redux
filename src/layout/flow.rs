//! Layout flow state and cell allocation policies.

use std::cmp::max;

use crate::{rect, vec2, Recti, Vec2i};

use super::{SizePolicy, StackDirection};

#[derive(Clone, Default)]
/// Cursor and content-extents state for one layout scope.
pub(super) struct ScopeState {
    /// Scope rectangle expressed in local space after scroll offset is folded in.
    pub(super) body: Recti,
    /// Current cursor in local coordinates.
    pub(super) cursor: Vec2i,
    /// Max absolute extent reached by generated cells for scroll/content sizing.
    pub(super) max: Option<Vec2i>,
    /// Y coordinate where the next logical line should start.
    pub(super) next_row: i32,
    /// Horizontal indentation applied to the scope.
    pub(super) indent: i32,
}

impl ScopeState {
    /// Resets the cursor to the start of the next line while preserving indentation.
    pub(super) fn reset_cursor_for_next_row(&mut self) {
        self.cursor = vec2(self.indent, self.next_row);
    }
}

#[derive(Copy, Clone)]
/// Dimension-resolution inputs shared by all flow implementations.
pub(super) struct ResolveCtx {
    /// Global inter-cell spacing from style.
    pub(super) spacing: i32,
    /// Width fallback after preferred-size resolution.
    pub(super) default_width: i32,
    /// Height fallback after preferred-size resolution.
    pub(super) default_height: i32,
    /// Optional node-local width policy override.
    pub(super) width_override: Option<SizePolicy>,
    /// Optional node-local height policy override.
    pub(super) height_override: Option<SizePolicy>,
}

impl ResolveCtx {
    /// Returns the effective width policy after applying any node-local override.
    fn width_policy(&self, fallback: SizePolicy) -> SizePolicy {
        self.width_override.unwrap_or(fallback)
    }

    /// Returns the effective height policy after applying any node-local override.
    fn height_policy(&self, fallback: SizePolicy) -> SizePolicy {
        self.height_override.unwrap_or(fallback)
    }
}

/// Placement strategy for allocating the next local cell.
trait LayoutFlow {
    /// Produces the next local cell and advances scope-local cursors/state.
    fn next_local(&mut self, scope: &mut ScopeState, ctx: ResolveCtx) -> Recti;
}

#[derive(Clone)]
/// Height policy mode for row/grid flows.
pub(super) enum RowHeights {
    /// One height policy shared by every emitted row.
    Uniform(SizePolicy),
    /// Repeating list of row-height tracks.
    Tracks(Vec<SizePolicy>),
}

impl Default for RowHeights {
    fn default() -> Self {
        Self::Uniform(SizePolicy::Auto)
    }
}

#[derive(Clone, Default)]
/// Repeating row/grid flow that allocates cells left-to-right.
pub(super) struct RowFlow {
    /// Width policy for each slot in the active row pattern.
    widths: Vec<SizePolicy>,
    /// Height policy shared by rows or repeated by grid track.
    heights: RowHeights,
    /// Current slot index in `widths`.
    item_index: usize,
    /// Current row index in the active pattern.
    row_index: usize,
}

impl RowFlow {
    /// Creates a repeating row flow with one height policy for every emitted row.
    pub(super) fn new(widths: &[SizePolicy], height: SizePolicy) -> Self {
        Self::from_parts(widths.to_vec(), RowHeights::Uniform(height))
    }

    /// Creates a grid flow with independent width and row-height tracks.
    pub(super) fn new_grid(widths: &[SizePolicy], heights: &[SizePolicy]) -> Self {
        Self::from_parts(widths.to_vec(), RowHeights::Tracks(heights.to_vec()))
    }

    /// Creates a row flow from already-owned policy vectors.
    fn from_parts(widths: Vec<SizePolicy>, heights: RowHeights) -> Self {
        Self {
            widths,
            heights,
            item_index: 0,
            row_index: 0,
        }
    }

    /// Replaces the active row/grid template and restarts iteration at the first slot.
    fn apply_template(&mut self, widths: Vec<SizePolicy>, heights: RowHeights) {
        self.widths = widths;
        self.heights = heights;
        self.item_index = 0;
        self.row_index = 0;
    }

    /// Returns the row height policy and optional row-count hint for grid distribution.
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

    /// Computes the space left for weighted tracks after fixed/auto/fraction tracks reserve room.
    fn weight_reference_space(policies: &[SizePolicy], default_size: i32, total_space: i32) -> i32 {
        if policies.iter().any(|policy| matches!(policy, SizePolicy::Remainder(_))) {
            return total_space.max(0);
        }

        let reserved = policies
            .iter()
            .map(|policy| match *policy {
                SizePolicy::Auto => default_size.max(0),
                SizePolicy::Fixed(value) => value.max(0),
                SizePolicy::Weight(_) => 0,
                SizePolicy::Fraction(fraction) => SizePolicy::resolve_fraction(fraction, total_space),
                SizePolicy::Remainder(_) => 0,
            })
            .sum::<i32>();

        total_space.saturating_sub(reserved).max(0)
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
        let width_policy = ctx.width_policy(width_policy);
        let (height_policy, row_count_hint) = self.current_height_policy();
        let height_policy = ctx.height_policy(height_policy);

        let x = scope.cursor.x;
        let y = scope.cursor.y;
        let slot_count = slot_count as i32;
        let row_spacing = ctx.spacing.saturating_mul(slot_count.saturating_sub(1));
        let row_reference_width = scope.body.width.saturating_sub(scope.indent).saturating_sub(row_spacing);
        let row_width_weight = SizePolicy::total_weight(&self.widths);
        let row_weight_reference_width = if row_width_weight.is_some() {
            Self::weight_reference_space(&self.widths, ctx.default_width, row_reference_width)
        } else {
            row_reference_width
        };
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
        let width = width_policy.resolve_with_reference(ctx.default_width, available_width, row_weight_reference_width, row_width_weight);
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
/// Vertical stack flow that allocates one item per line.
pub(super) struct StackFlow {
    /// Width policy used for every stacked item.
    width: SizePolicy,
    /// Height policy used for every stacked item.
    height: SizePolicy,
    /// Vertical direction for cell emission.
    direction: StackDirection,
    /// Offset consumed from the stack anchor for bottom-up stacks.
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
    /// Creates a vertical stack flow anchored in the requested direction.
    pub(super) fn new(width: SizePolicy, height: SizePolicy, direction: StackDirection) -> Self {
        Self { width, height, direction, offset: 0 }
    }

    /// Replaces the active stack template and resets consumed offset.
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
        let width_policy = ctx.width_policy(self.width);
        let width = width_policy.resolve_with_reference(ctx.default_width, available_width, available_width, None);

        match self.direction {
            StackDirection::TopToBottom => {
                // Top-down stacks continue from the scope's row cursor.
                let y = scope.next_row;
                let available_height = scope.body.height.saturating_sub(y);
                let height_policy = ctx.height_policy(self.height);
                let height = height_policy.resolve_with_reference(ctx.default_height, available_height, scope.body.height, None);

                // Move directly to the next stacked row.
                let next = y.saturating_add(height).saturating_add(ctx.spacing);
                scope.next_row = next;
                scope.cursor = vec2(scope.indent, next);

                rect(x, y, width, height)
            }
            StackDirection::BottomToTop => {
                // Bottom-up stacks are anchored to the scope bottom and use local offset.
                let available_height = scope.body.height.saturating_sub(self.offset);
                let height_policy = ctx.height_policy(self.height);
                let height = height_policy.resolve_with_reference(ctx.default_height, available_height, scope.body.height, None);
                let y = scope.body.height.saturating_sub(self.offset).saturating_sub(height);
                self.offset = self.offset.saturating_add(height).saturating_add(ctx.spacing);
                rect(x, y, width, height)
            }
        }
    }
}

#[derive(Clone)]
/// Active flow implementation for a layout frame.
pub(super) enum FlowState {
    /// Repeating row/grid pattern.
    Row(RowFlow),
    /// One-cell-per-line vertical stack.
    Stack(StackFlow),
}

impl Default for FlowState {
    fn default() -> Self {
        FlowState::Row(RowFlow::new(&[SizePolicy::Auto], SizePolicy::Auto))
    }
}

impl FlowState {
    /// Stores the active flow as a lightweight template for later restoration.
    pub(super) fn as_template(&self) -> FlowTemplate {
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

    /// Applies a saved flow template, reusing the active flow allocation where possible.
    pub(super) fn apply_template(&mut self, template: FlowTemplate) {
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

    /// Delegates cell generation to the active flow implementation.
    pub(super) fn next_local(&mut self, scope: &mut ScopeState, ctx: ResolveCtx) -> Recti {
        match self {
            FlowState::Row(flow) => flow.next_local(scope, ctx),
            FlowState::Stack(flow) => flow.next_local(scope, ctx),
        }
    }
}

#[derive(Clone)]
/// Saved flow configuration used by temporary scoped layout overrides.
pub(super) enum FlowTemplate {
    /// Snapshot for row/grid flow configuration.
    Row {
        /// Saved row width tracks.
        widths: Vec<SizePolicy>,
        /// Saved row height policy.
        heights: RowHeights,
    },
    /// Snapshot for stack flow configuration.
    Stack {
        /// Saved stack width policy.
        width: SizePolicy,
        /// Saved stack height policy.
        height: SizePolicy,
        /// Saved stack direction.
        direction: StackDirection,
    },
}
