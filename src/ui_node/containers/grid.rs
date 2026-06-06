use crate::sizing::SizePolicy;
use crate::{Dimensioni, Recti};

use super::{NodeBehavior, LayoutCtx, MeasureCtx};
use crate::ui_node::UiNodeId;

/// Grid container.
#[derive(Clone)]
pub(crate) struct Grid {
    /// Width policies for columns.
    pub(crate) widths: Vec<SizePolicy>,
    /// Height policies for rows.
    pub(crate) heights: Vec<SizePolicy>,
}

impl NodeBehavior for Grid {
    fn measure(&self, ctx: &MeasureCtx<'_>, id: UiNodeId, available: Dimensioni) -> Dimensioni {
        let cols = self.widths.len().max(1);
        let rows = grid_placements(ctx, id, cols)
            .into_iter()
            .map(|placement| placement.row + placement.row_span)
            .max()
            .unwrap_or(0);
        let rows = rows.max(self.heights.len()).max(1);
        let default_height = super::super::default_cell_height(ctx.style, ctx.atlas);
        let width = available.width;
        let height = if self.heights.is_empty() {
            default_height
                .saturating_mul(rows as i32)
                .saturating_add(ctx.style.spacing.saturating_mul(rows.saturating_sub(1) as i32))
        } else {
            available.height
        };
        Dimensioni::new(width.max(0), height.max(0))
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, id: UiNodeId, rect: Recti, clip: Recti) {
        let cols = self.widths.len().max(1);
        let placements = grid_placements_layout(ctx, id, cols);
        let rows = placements
            .iter()
            .map(|placement| placement.row + placement.row_span)
            .max()
            .unwrap_or(0)
            .max(self.heights.len())
            .max(1);
        let available_width = rect.width.saturating_sub(ctx.style.spacing.saturating_mul(cols.saturating_sub(1) as i32));
        let available_height = rect.height.saturating_sub(ctx.style.spacing.saturating_mul(rows.saturating_sub(1) as i32));
        let preferred_widths = vec![super::super::default_cell_width(ctx.style); cols];
        let preferred_heights = vec![super::super::default_cell_height(ctx.style, ctx.atlas); rows];
        let col_widths = super::super::resolve_axis_tracks(&super::super::track_policies(&self.widths, cols), &preferred_widths, available_width);
        let row_heights = super::super::resolve_axis_tracks(&super::super::track_policies(&self.heights, rows), &preferred_heights, available_height);
        for placement in placements {
            let x = rect.x + col_widths.iter().take(placement.col).sum::<i32>() + ctx.style.spacing.saturating_mul(placement.col as i32);
            let y = rect.y + row_heights.iter().take(placement.row).sum::<i32>() + ctx.style.spacing.saturating_mul(placement.row as i32);
            let width = super::super::span_size(&col_widths, placement.col, placement.col_span, ctx.style.spacing);
            let height = super::super::span_size(&row_heights, placement.row, placement.row_span, ctx.style.spacing);
            ctx.layout_node(placement.child, Recti::new(x, y, width, height), clip);
        }
    }
}

/// Concrete row-major placement of one child inside a grid.
struct GridPlacement {
    child: UiNodeId,
    col: usize,
    row: usize,
    col_span: usize,
    row_span: usize,
}

fn grid_placements(ctx: &MeasureCtx<'_>, id: UiNodeId, cols: usize) -> Vec<GridPlacement> {
    let cols = cols.max(1);
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let mut placements = Vec::with_capacity(ctx.child_count(id));
    let mut search_row = 0;
    let mut search_col = 0;

    for index in 0..ctx.child_count(id) {
        let Some(child) = ctx.child_at(id, index) else { continue };
        let (row, col) = super::super::first_free_grid_cell(&mut occupied, cols, search_row, search_col);
        let span = ctx.runtime.nodes.get(&child).map(|node| node.grid_span).unwrap_or(crate::GridSpan::ONE);
        let col_span = span.columns.max(1).min(cols.saturating_sub(col).max(1));
        let row_span = span.rows.max(1);
        super::super::mark_grid_occupied(&mut occupied, cols, row, col, row_span, col_span);
        placements.push(GridPlacement { child, col, row, col_span, row_span });

        search_row = row;
        search_col = col.saturating_add(col_span);
        while search_col >= cols {
            search_col -= cols;
            search_row += 1;
        }
    }

    placements
}

fn grid_placements_layout(ctx: &LayoutCtx<'_>, id: UiNodeId, cols: usize) -> Vec<GridPlacement> {
    let cols = cols.max(1);
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let mut placements = Vec::with_capacity(ctx.child_count(id));
    let mut search_row = 0;
    let mut search_col = 0;

    for index in 0..ctx.child_count(id) {
        let Some(child) = ctx.child_at(id, index) else { continue };
        let (row, col) = super::super::first_free_grid_cell(&mut occupied, cols, search_row, search_col);
        let span = ctx.grid_span(child);
        let col_span = span.columns.max(1).min(cols.saturating_sub(col).max(1));
        let row_span = span.rows.max(1);
        super::super::mark_grid_occupied(&mut occupied, cols, row, col, row_span, col_span);
        placements.push(GridPlacement { child, col, row, col_span, row_span });

        search_row = row;
        search_col = col.saturating_add(col_span);
        while search_col >= cols {
            search_col -= cols;
            search_row += 1;
        }
    }

    placements
}
