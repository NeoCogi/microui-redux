//! Internal retained node model and traversal runtime.
//!
//! One node representation owns either a widget or a container; no parallel legacy tree or
//! generated public identity path remains.
//!
//! Topology is assembled from unique owning nodes and concrete state-owned containers. Update and
//! paint visit a node before its eligible children in forward sibling order; pointer input visits
//! eligible children first in reverse sibling order so the deepest, topmost node wins. Measurement
//! and layout recurse only through a container's scoped child APIs.
//!
//! Each node retains an allocation in its parent's child coordinates plus a node-local child
//! offset and clip. Recursive passes carry one stack-only [`Transform`]. Resolved outer rectangles
//! and outer clips remain runtime stack locals; phase contexts expose node-local content geometry.
//! Common widget phases dispatch once through the [`crate::Widget`] owned by each private
//! `NodeKind` variant. Traversal branches to [`Container`] only for layout, routed input,
//! descendant visibility, and scoped child visitation. Container update and paint run before the
//! visibility gate is checked. Each concrete container borrows its directly owned state for the
//! current runtime method, and each child borrow remains scoped to one opaque visitor call before
//! recursion continues.
#![allow(dead_code)]

use crate::render::DisplayList;
use crate::{Dimensioni, Recti, Style, UNCLIPPED_RECT};
use crate::{WidgetOption, WindowOption};
use crate::sizing::SizePolicy;
use crate::widget::FocusPolicy;

mod node;
pub use node::{Children, Node};
pub(crate) use node::{NodeKind, NodeLayout, NodeMeasurement, NodeRuntime, RuntimeNodeId, Transform};
mod runtime;
pub(crate) use runtime::UiRuntime;
#[cfg(test)]
pub(crate) use runtime::RuntimeMetrics;
mod containers;
pub(crate) use containers::WidgetNode;
pub use containers::{
    ChildrenVisitor, ChildrenVisitorMut, Column, ColumnBuilder, ColumnContainer, ColumnParameters, ColumnState, Container, ContainerBuilder, ContainerInputCtx,
    ContainerInputResult, ContainerLayoutCtx, ContainerState, Disclosure, DisclosureBuilder, DisclosureContainer, DisclosureParameters, DisclosureState, Grid,
    GridBuilder, GridContainer, GridItem, GridParameters, GridSpan, GridState, Row, RowBuilder, RowContainer, RowParameters, RowState, Stack, StackBuilder,
    ScrollArea, ScrollAreaBuilder, ScrollAreaContainer, ScrollAreaParameters, ScrollAreaState, StackContainer, StackParameters, StackState,
};
pub use containers::UiInputEvent;
pub use containers::ScrollAreaOption;

/// Computes titlebar height from style minimums and current title font metrics.
fn root_titlebar_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.title_font) as i32;
    let padding = style.padding.max(0);
    let min_title_h = font_height + (padding / 2).max(1) * 2;
    style.title_height.max(min_title_h)
}

/// Returns the union of two rectangles.
fn union_rect(a: Recti, b: Recti) -> Recti {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);
    Recti::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Compares rectangle components directly.
fn same_rect(a: Recti, b: Recti) -> bool {
    (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height)
}

/// Adds symmetric style padding to content size for scrollbar range checks.
fn add_padding(size: Dimensioni, padding: i32) -> Dimensioni {
    Dimensioni::new(
        size.width.saturating_add(padding.saturating_mul(2)),
        size.height.saturating_add(padding.saturating_mul(2)),
    )
}

/// Returns the child indentation for a disclosure container.
fn disclosure_child_indent(indent_children: bool, style: &Style) -> i32 {
    if indent_children { style.indent.max(0) } else { 0 }
}

/// Returns the fallback control width for auto-sized cells.
fn default_cell_width(style: &Style) -> i32 {
    style.default_cell_width.saturating_add(style.padding.max(0) * 2).max(0)
}

/// Returns the fallback control height for auto-sized cells.
fn default_cell_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.font) as i32;
    font_height.saturating_add(style.padding.max(0) * 2).max(style.padding.max(0) * 2)
}

/// Expands a possibly shorter track-policy slice to a requested count.
fn track_policies(policies: &[SizePolicy], count: usize) -> Vec<SizePolicy> {
    (0..count).map(|index| policies.get(index).copied().unwrap_or(SizePolicy::Auto)).collect()
}

/// Returns the first free cell in a row-major occupancy grid, extending rows as needed.
fn first_free_grid_cell(occupied: &mut Vec<Vec<bool>>, cols: usize, mut row: usize, mut col: usize) -> (usize, usize) {
    loop {
        while occupied.len() <= row {
            occupied.push(vec![false; cols]);
        }
        while col < cols {
            if !occupied[row][col] {
                return (row, col);
            }
            col += 1;
        }
        row += 1;
        col = 0;
    }
}

/// Marks a rectangular cell range as occupied, extending rows as needed.
fn mark_grid_occupied(occupied: &mut Vec<Vec<bool>>, cols: usize, row: usize, col: usize, row_span: usize, col_span: usize) {
    for y in row..row.saturating_add(row_span.max(1)) {
        while occupied.len() <= y {
            occupied.push(vec![false; cols]);
        }
        for x in col..col.saturating_add(col_span.max(1)).min(cols) {
            occupied[y][x] = true;
        }
    }
}

/// Sums a track span, including the spacing between spanned tracks.
fn span_size(tracks: &[i32], start: usize, span: usize, spacing: i32) -> i32 {
    let span = span.max(1);
    let size = tracks.iter().skip(start).take(span).copied().sum::<i32>();
    size.saturating_add(spacing.saturating_mul(span.saturating_sub(1) as i32))
}

/// Resolves one size policy against a preferred size, available space, and optional weight context.
fn resolve_size(policy: SizePolicy, preferred: i32, available: i32, reference: i32, total_weight: Option<f32>) -> i32 {
    let resolved = match policy {
        SizePolicy::Auto => preferred,
        SizePolicy::Fixed(value) => value,
        SizePolicy::Fraction(value) => {
            let fraction = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
            ((reference.max(0) as f32) * fraction).floor() as i32
        }
        SizePolicy::Weight(value) => {
            let weight = if value.is_finite() { value.max(0.0) } else { 0.0 };
            if weight <= 0.0 {
                0
            } else {
                let denom = total_weight.filter(|total| total.is_finite() && *total > 0.0).unwrap_or(weight);
                ((reference.max(0) as f32) * (weight / denom)).floor() as i32
            }
        }
        SizePolicy::Remainder(margin) => available.saturating_sub(margin),
    };
    resolved.max(0)
}

/// Resolves the available size passed into a child measurement from explicit placement policy.
fn measure_axis_available(policy: SizePolicy, available: i32) -> i32 {
    match policy {
        SizePolicy::Fixed(value) => value.max(0),
        SizePolicy::Fraction(value) => resolve_size(SizePolicy::Fraction(value), 0, available, available, None),
        SizePolicy::Remainder(margin) => available.saturating_sub(margin.max(0)).max(0),
        SizePolicy::Auto | SizePolicy::Weight(_) => available.max(0),
    }
}

/// Resolves a node inside an already allocated parent slot.
fn resolve_allocated_size(policy: SizePolicy, preferred: i32, allocated: i32, reference: i32, total_weight: Option<f32>) -> i32 {
    match policy {
        SizePolicy::Auto => allocated.max(0),
        _ => resolve_size(policy, preferred, allocated, reference, total_weight),
    }
}

/// Resolves sibling tracks in one axis using fixed, auto, fraction, weight, and remainder policies.
fn resolve_axis_tracks(policies: &[SizePolicy], preferred: &[i32], available: i32) -> Vec<i32> {
    if policies.is_empty() {
        return Vec::new();
    }

    let available = available.max(0);
    let mut sizes = vec![0; policies.len()];
    let has_remainder = policies.iter().any(|policy| matches!(policy, SizePolicy::Remainder(_)));
    let total_weight = policies
        .iter()
        .filter_map(|policy| match *policy {
            SizePolicy::Weight(value) if value.is_finite() => Some(value.max(0.0)),
            _ => None,
        })
        .sum::<f32>();
    let reserved_for_weight = if has_remainder {
        0
    } else {
        policies
            .iter()
            .copied()
            .enumerate()
            .map(|(index, policy)| match policy {
                SizePolicy::Auto => preferred.get(index).copied().unwrap_or_default().max(0),
                SizePolicy::Fixed(value) => value.max(0),
                SizePolicy::Fraction(value) => resolve_size(SizePolicy::Fraction(value), 0, available, available, None),
                SizePolicy::Weight(_) | SizePolicy::Remainder(_) => 0,
            })
            .sum::<i32>()
    };
    let weight_reference = if total_weight > 0.0 {
        if has_remainder {
            available
        } else {
            available.saturating_sub(reserved_for_weight)
        }
    } else {
        0
    };

    let mut used: i32 = 0;
    for (index, policy) in policies.iter().copied().enumerate() {
        let remaining = available.saturating_sub(used);
        match policy {
            SizePolicy::Auto => {
                sizes[index] = preferred.get(index).copied().unwrap_or_default().max(0);
            }
            SizePolicy::Fixed(value) => {
                sizes[index] = value.max(0);
            }
            SizePolicy::Fraction(value) => {
                sizes[index] = resolve_size(SizePolicy::Fraction(value), 0, available, available, None);
            }
            SizePolicy::Weight(value) => {
                sizes[index] = resolve_size(SizePolicy::Weight(value), 0, remaining, weight_reference, Some(total_weight));
            }
            SizePolicy::Remainder(margin) => {
                sizes[index] = remaining.saturating_sub(margin.max(0)).max(0);
            }
        }
        used = used.saturating_add(sizes[index]);
    }
    sizes
}

/// One linear-axis placement planned by a container.
///
/// `offered` is the unresolved extent passed to child layout. `advance` is the amount by which the
/// container advances its sibling cursor after that child. Keeping these values separate matters
/// for non-idempotent policies: for example, a `Fraction(0.5)` child advances by half of the
/// available axis but must still be offered the complete reference axis so `layout_child` applies
/// the fraction exactly once.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct AxisPlacement {
    offered: i32,
    advance: i32,
}

/// Plans sibling cursor movement without feeding policy-resolved extents back into child layout.
///
/// The existing track resolver computes the eventual sibling advances, including shared `Weight`
/// distribution. Most policies can use that extent as their offered slot. `Fraction` needs the
/// complete reference axis, while `Remainder` needs its pre-margin extent; child layout then remains
/// the sole operation that applies the node's policy to its offered rectangle.
fn resolve_axis_placements(policies: &[SizePolicy], preferred: &[i32], available: i32) -> Vec<AxisPlacement> {
    let available = available.max(0);
    resolve_axis_tracks(policies, preferred, available)
        .into_iter()
        .zip(policies.iter().copied())
        .map(|(advance, policy)| {
            let offered = match policy {
                SizePolicy::Fraction(_) => available,
                SizePolicy::Remainder(margin) => advance.saturating_add(margin).max(0),
                SizePolicy::Auto | SizePolicy::Fixed(_) | SizePolicy::Weight(_) => advance,
            };
            AxisPlacement { offered, advance }
        })
        .collect()
}

/// Returns the screen-space rectangle occupied by a child and any overflow content it measured.
fn child_content_rect(node: &Node) -> Recti {
    let allocation = node.state.layout.allocation;
    let content_size = node.state.layout.content_size;
    Recti::new(
        allocation.x,
        allocation.y,
        allocation.width.max(content_size.width),
        allocation.height.max(content_size.height),
    )
}

#[cfg(test)]
mod sizing_tests {
    use super::*;

    #[test]
    fn weighted_tracks_share_the_available_axis_by_ratio() {
        assert_eq!(
            resolve_axis_tracks(&[SizePolicy::Weight(1.0), SizePolicy::Weight(2.0), SizePolicy::Weight(3.0)], &[0, 0, 0], 120,),
            [20, 40, 60]
        );
    }

    #[test]
    fn fractional_placement_offers_the_full_reference_for_one_policy_application() {
        assert_eq!(
            resolve_axis_placements(&[SizePolicy::Fraction(0.5)], &[80], 200),
            [AxisPlacement { offered: 200, advance: 100 }]
        );
    }

    #[test]
    fn remainder_tracks_consume_only_the_axis_left_by_prior_siblings() {
        assert_eq!(
            resolve_axis_tracks(&[SizePolicy::Fixed(30), SizePolicy::Auto, SizePolicy::Remainder(5)], &[0, 20, 0], 100,),
            [30, 20, 45]
        );
    }
}
