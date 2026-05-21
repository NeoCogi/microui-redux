//! Tests for layout policy resolution and scoped flow restoration.

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
fn row_weight_uses_space_left_after_fixed_tracks() {
    let mut layout = LayoutManager::default();
    layout.style = Style::default();
    let body = rect(0, 0, 240, 80);
    layout.reset(body, vec2(0, 0));
    layout.set_default_cell_height(10);
    layout.row(
        &[
            SizePolicy::Fixed(80),
            SizePolicy::Weight(1.0),
            SizePolicy::Weight(1.0),
            SizePolicy::Weight(1.0),
            SizePolicy::Weight(1.0),
            SizePolicy::Weight(1.0),
        ],
        SizePolicy::Fixed(10),
    );

    let label = layout.next();
    let a = layout.next();
    let b = layout.next();
    let c = layout.next();
    let d = layout.next();
    let swatch = layout.next();
    let spacing = layout.style.spacing;
    let expected = (body.width - label.width - spacing * 5) / 5;

    assert_eq!(label.width, 80);
    assert_eq!(a.width, expected);
    assert_eq!(b.width, expected);
    assert_eq!(c.width, expected);
    assert_eq!(d.width, expected);
    assert_eq!(swatch.width, expected);
    assert_eq!(swatch.x + swatch.width, body.x + body.width);
}

#[test]
fn stack_fraction_uses_explicit_proportion() {
    let mut layout = LayoutManager::default();
    layout.style = Style::default();
    let body = rect(0, 0, 120, 60);
    layout.reset(body, vec2(0, 0));
    layout.set_default_cell_height(10);
    layout.stack(SizePolicy::Fraction(0.5), SizePolicy::Fraction(0.25));

    let first = layout.next();
    let second = layout.next();

    assert_eq!(first.width, 60);
    assert_eq!(first.height, 15);
    assert_eq!(second.width, 60);
    assert_eq!(second.height, 15);
}

#[test]
fn stack_weight_without_siblings_acts_as_one_share() {
    let mut layout = LayoutManager::default();
    layout.style = Style::default();
    let body = rect(0, 0, 120, 60);
    layout.reset(body, vec2(0, 0));
    layout.set_default_cell_height(10);
    layout.stack(SizePolicy::Weight(1.0), SizePolicy::Weight(1.0));

    let first = layout.next();

    assert_eq!(first.width, body.width);
    assert_eq!(first.height, body.height);
}

#[test]
fn row_remainder_margin_preserves_footer_rows() {
    let mut layout = LayoutManager::default();
    layout.style = Style::default();
    let body = rect(0, 0, 160, 120);
    layout.reset(body, vec2(0, 0));
    layout.set_default_cell_height(10);

    let row = [SizePolicy::Remainder(0)];
    let spacing = layout.style.spacing;

    layout.row(&row, SizePolicy::Fixed(10));
    let toolbar = layout.next();

    layout.row(&row, SizePolicy::Remainder(10 * 2 + spacing * 2));
    let pane = layout.next();

    layout.row(&row, SizePolicy::Fixed(10));
    let filename = layout.next();

    layout.row(&row, SizePolicy::Fixed(10));
    let actions = layout.next();

    assert_eq!(toolbar.y, body.y);
    assert_eq!(filename.y, pane.y + pane.height + spacing);
    assert_eq!(actions.y, filename.y + filename.height + spacing);
    assert!(actions.y + actions.height <= body.y + body.height);
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
