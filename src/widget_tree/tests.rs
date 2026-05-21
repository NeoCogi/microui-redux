//! Tests for retained widget-tree building and identity behavior.

use crate::{test_support::test_atlas, Button, Container, ContainerHandle, Input, SizePolicy, Style};
use std::{cell::RefCell, rc::Rc};

use super::*;

#[test]
fn unkeyed_widget_ids_are_stable_for_same_shape() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));

    let tree_a = WidgetTreeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.widget(button_b.clone());
    });
    let tree_a_ids: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();
    let tree_b = WidgetTreeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.widget(button_b.clone());
    });
    let tree_b_ids: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

    assert_eq!(tree_a_ids[0], tree_b_ids[0]);
    assert_eq!(tree_a_ids[1], tree_b_ids[1]);
}

#[test]
fn keyed_widgets_keep_ids_across_reorder() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));

    let tree_a = WidgetTreeBuilder::build(|builder| {
        builder.widget_with(NodeOptions::keyed("a"), button_a.clone());
        builder.widget_with(NodeOptions::keyed("b"), button_b.clone());
    });
    let ids_a: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();
    let tree_b = WidgetTreeBuilder::build(|builder| {
        builder.widget_with(NodeOptions::keyed("b"), button_b.clone());
        builder.widget_with(NodeOptions::keyed("a"), button_a.clone());
    });
    let ids_b: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

    assert_eq!(ids_a[0], ids_b[1]);
    assert_eq!(ids_a[1], ids_b[0]);
}

#[test]
fn inserting_keyed_widget_does_not_shift_later_unkeyed_ids() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));
    let keyed = widget_handle(Button::new("keyed"));

    let tree_a = WidgetTreeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.widget(button_b.clone());
    });
    let ids_a: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();

    let tree_b = WidgetTreeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.widget_with(NodeOptions::keyed("inserted"), keyed.clone());
        builder.widget(button_b.clone());
    });
    let ids_b: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

    assert_eq!(ids_a[0], ids_b[0]);
    assert_eq!(ids_a[1], ids_b[2]);
}

#[test]
fn row_nodes_capture_children_and_track_policy() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));

    let tree = WidgetTreeBuilder::build(|builder| {
        builder.row_with(
            NodeOptions::with_policy(Policy::fill()),
            &[SizePolicy::Fixed(40), SizePolicy::Remainder(0)],
            SizePolicy::Fixed(24),
            |builder| {
                builder.widget(button_a.clone());
                builder.widget(button_b.clone());
            },
        );
    });

    let row = &tree.roots()[0];
    assert_eq!(row.policy(), Policy::fill());
    assert_eq!(row.children().len(), 2);

    match row.kind() {
        WidgetTreeNodeKind::Row { widths, height } => {
            assert_eq!(widths, &[SizePolicy::Fixed(40), SizePolicy::Remainder(0)]);
            assert_eq!(*height, SizePolicy::Fixed(24));
        }
        _ => panic!("expected row node"),
    }
}

#[test]
fn container_nodes_store_handle_and_children() {
    let atlas = test_atlas();
    let input = Rc::new(RefCell::new(Input::default()));
    let handle = ContainerHandle::new(Container::new("panel", atlas, Rc::new(Style::default()), input));
    let leaf = widget_handle((crate::WidgetOption::NONE, crate::ScrollBehavior::NONE));

    let tree = WidgetTreeBuilder::build(|builder| {
        builder.container_with(
            NodeOptions::with_policy(Policy::fill()),
            handle.clone(),
            crate::ContainerOption::NONE,
            crate::ScrollBehavior::NONE,
            |builder| {
                builder.widget(leaf.clone());
            },
        );
    });

    let node = &tree.roots()[0];
    assert_eq!(node.children().len(), 1);
    match node.kind() {
        WidgetTreeNodeKind::Container { .. } => {}
        _ => panic!("expected container node"),
    }
}

#[test]
fn text_nodes_are_recorded_as_widgets() {
    let tree = WidgetTreeBuilder::build(|builder| {
        builder.text("hello");
    });

    assert_eq!(tree.roots().len(), 1);
    match tree.roots()[0].kind() {
        WidgetTreeNodeKind::Widget { .. } => {}
        _ => panic!("expected retained text widget node"),
    }
}
