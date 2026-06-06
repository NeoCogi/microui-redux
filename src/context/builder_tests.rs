//! Tests for retained UI node building and identity behavior.

use crate::{Button, ScrollAreaHandle, ScrollAreaState, SizePolicy};
use crate::ui_node::UiNodeData;

use super::*;

#[test]
fn unkeyed_widget_ids_are_stable_for_same_shape() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));

    let tree_a = UiNodeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.widget(button_b.clone());
    });
    let tree_a_ids: Vec<NodeId> = tree_a.roots().to_vec();
    let tree_b = UiNodeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.widget(button_b.clone());
    });
    let tree_b_ids: Vec<NodeId> = tree_b.roots().to_vec();

    assert_eq!(tree_a_ids[0], tree_b_ids[0]);
    assert_eq!(tree_a_ids[1], tree_b_ids[1]);
}

#[test]
fn keyed_widgets_keep_ids_across_reorder() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));

    let tree_a = UiNodeBuilder::build(|builder| {
        builder.node(NodeOptions::keyed("a")).widget(button_a.clone());
        builder.node(NodeOptions::keyed("b")).widget(button_b.clone());
    });
    let ids_a: Vec<NodeId> = tree_a.roots().to_vec();
    let tree_b = UiNodeBuilder::build(|builder| {
        builder.node(NodeOptions::keyed("b")).widget(button_b.clone());
        builder.node(NodeOptions::keyed("a")).widget(button_a.clone());
    });
    let ids_b: Vec<NodeId> = tree_b.roots().to_vec();

    assert_eq!(ids_a[0], ids_b[1]);
    assert_eq!(ids_a[1], ids_b[0]);
}

#[test]
fn inserting_keyed_widget_does_not_shift_later_unkeyed_ids() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));
    let keyed = widget_handle(Button::new("keyed"));

    let tree_a = UiNodeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.widget(button_b.clone());
    });
    let ids_a: Vec<NodeId> = tree_a.roots().to_vec();

    let tree_b = UiNodeBuilder::build(|builder| {
        builder.widget(button_a.clone());
        builder.node(NodeOptions::keyed("inserted")).widget(keyed.clone());
        builder.widget(button_b.clone());
    });
    let ids_b: Vec<NodeId> = tree_b.roots().to_vec();

    assert_eq!(ids_a[0], ids_b[0]);
    assert_eq!(ids_a[1], ids_b[2]);
}

#[test]
#[should_panic(expected = "duplicate retained node id")]
fn duplicate_keyed_sibling_ids_are_rejected_at_build_time() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));

    UiNodeBuilder::build(|builder| {
        builder.node(NodeOptions::keyed("same")).widget(&button_a);
        builder.node(NodeOptions::keyed("same")).widget(&button_b);
    });
}

#[test]
fn matching_child_keys_in_different_scopes_remain_distinct() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));
    let mut first_child = NodeId::default();
    let mut second_child = NodeId::default();

    UiNodeBuilder::build(|builder| {
        builder.row(&[SizePolicy::Auto], SizePolicy::Auto, |builder| {
            first_child = builder.node(NodeOptions::keyed("same")).widget(&button_a);
        });
        builder.row(&[SizePolicy::Auto], SizePolicy::Auto, |builder| {
            second_child = builder.node(NodeOptions::keyed("same")).widget(&button_b);
        });
    });

    assert_ne!(first_child, second_child);
}

#[test]
fn row_nodes_capture_children_and_track_policy() {
    let button_a = widget_handle(Button::new("A"));
    let button_b = widget_handle(Button::new("B"));

    let tree = UiNodeBuilder::build(|builder| {
        builder
            .node(NodeOptions::with_policy(Policy::fill()))
            .row(&[SizePolicy::Fixed(40), SizePolicy::Remainder(0)], SizePolicy::Fixed(24), |builder| {
                builder.widget(button_a.clone());
                builder.widget(button_b.clone());
            });
    });

    let row_id = tree.roots()[0];
    let row = tree.node(row_id).expect("row node missing");
    assert_eq!(row.policy, Policy::fill());
    assert_eq!(row.children().len(), 2);
    assert!(matches!(row.data, UiNodeData::Branch { .. }));
}

#[test]
fn scroll_area_nodes_store_handle_and_children() {
    let handle = ScrollAreaHandle::new(ScrollAreaState::new("scroll area"));
    let leaf = widget_handle((crate::WidgetOption::NONE, crate::ScrollBehavior::NONE));

    let tree = UiNodeBuilder::build(|builder| {
        builder.node(NodeOptions::with_policy(Policy::fill())).scroll_area(
            handle.clone(),
            crate::ContainerOption::NONE,
            crate::ScrollBehavior::NONE,
            |builder| {
                builder.widget(leaf.clone());
            },
        );
    });

    let node = tree.node(tree.roots()[0]).expect("scroll area node missing");
    assert_eq!(node.children().len(), 1);
    assert!(matches!(node.data, UiNodeData::Branch { .. }));
}

#[test]
fn text_nodes_are_recorded_as_widgets() {
    let tree = UiNodeBuilder::build(|builder| {
        builder.text("hello");
    });

    assert_eq!(tree.roots().len(), 1);
    let node = tree.node(tree.roots()[0]).expect("text node missing");
    assert!(matches!(node.data, UiNodeData::Leaf { .. }));
}
