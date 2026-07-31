//! Tests for retained UI node building and identity behavior.

use crate::{ButtonBuilder, ButtonParameters, SizePolicy};
use crate::test_support::projected_widget;
use crate::ui_node::UiNodeData;

use super::*;

#[test]
fn independently_constructed_widgets_receive_distinct_ids() {
    let tree_a = UiNodeBuilder::build(|builder| {
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("A")));
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("B")));
    });
    let tree_a_ids: Vec<NodeId> = tree_a.roots().to_vec();
    let tree_b = UiNodeBuilder::build(|builder| {
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("A")));
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("B")));
    });
    let tree_b_ids: Vec<NodeId> = tree_b.roots().to_vec();

    assert_ne!(tree_a_ids[0], tree_b_ids[0]);
    assert_ne!(tree_a_ids[1], tree_b_ids[1]);
}

#[test]
fn transitional_builder_keys_do_not_reconstruct_runtime_ids() {
    let tree_a = UiNodeBuilder::build(|builder| {
        builder
            .node(NodeOptions::keyed("a"))
            .widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("A")));
        builder
            .node(NodeOptions::keyed("b"))
            .widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("B")));
    });
    let ids_a: Vec<NodeId> = tree_a.roots().to_vec();
    let tree_b = UiNodeBuilder::build(|builder| {
        builder
            .node(NodeOptions::keyed("b"))
            .widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("B")));
        builder
            .node(NodeOptions::keyed("a"))
            .widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("A")));
    });
    let ids_b: Vec<NodeId> = tree_b.roots().to_vec();

    assert!(ids_a.iter().all(|id| !ids_b.contains(id)));
}

#[test]
fn every_builder_insertion_allocates_fresh_identity() {
    let tree_a = UiNodeBuilder::build(|builder| {
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("A")));
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("B")));
    });
    let ids_a: Vec<NodeId> = tree_a.roots().to_vec();

    let tree_b = UiNodeBuilder::build(|builder| {
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("A")));
        builder
            .node(NodeOptions::keyed("inserted"))
            .widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("keyed")));
        builder.widget(projected_widget::<ButtonBuilder>(ButtonParameters::new("B")));
    });
    let ids_b: Vec<NodeId> = tree_b.roots().to_vec();

    assert!(ids_a.iter().all(|id| !ids_b.contains(id)));
}

#[test]
fn duplicate_transitional_keys_cannot_alias_sibling_runtime_ids() {
    let button_a = projected_widget::<ButtonBuilder>(ButtonParameters::new("A"));
    let button_b = projected_widget::<ButtonBuilder>(ButtonParameters::new("B"));

    let tree = UiNodeBuilder::build(|builder| {
        builder.node(NodeOptions::keyed("same")).widget(button_a);
        builder.node(NodeOptions::keyed("same")).widget(button_b);
    });
    assert_ne!(tree.roots()[0], tree.roots()[1]);
}

#[test]
fn matching_child_keys_in_different_scopes_remain_distinct() {
    let button_a = projected_widget::<ButtonBuilder>(ButtonParameters::new("A"));
    let button_b = projected_widget::<ButtonBuilder>(ButtonParameters::new("B"));
    let mut first_child = NodeId::default();
    let mut second_child = NodeId::default();

    UiNodeBuilder::build(|builder| {
        builder.row(&[SizePolicy::Auto], SizePolicy::Auto, |builder| {
            first_child = builder.node(NodeOptions::keyed("same")).widget(button_a);
        });
        builder.row(&[SizePolicy::Auto], SizePolicy::Auto, |builder| {
            second_child = builder.node(NodeOptions::keyed("same")).widget(button_b);
        });
    });

    assert_ne!(first_child, second_child);
}

#[test]
fn row_nodes_capture_children_and_track_policy() {
    let button_a = projected_widget::<ButtonBuilder>(ButtonParameters::new("A"));
    let button_b = projected_widget::<ButtonBuilder>(ButtonParameters::new("B"));

    let tree = UiNodeBuilder::build(|builder| {
        builder
            .node(NodeOptions::with_policy(Policy::fill()))
            .row(&[SizePolicy::Fixed(40), SizePolicy::Remainder(0)], SizePolicy::Fixed(24), |builder| {
                builder.widget(button_a);
                builder.widget(button_b);
            });
    });

    let row_id = tree.roots()[0];
    tree.with_node(row_id, |row| {
        assert_eq!(row.state.policy, Policy::fill());
        row.with_children(|children| assert_eq!(children.len(), 2));
        assert!(matches!(row.data, UiNodeData::LegacyContainer(_)));
    })
    .expect("row node missing");
}

#[test]
fn scroll_area_nodes_store_viewport_and_chrome_children() {
    let tree = UiNodeBuilder::build(|builder| {
        builder.node(NodeOptions::with_policy(Policy::fill())).scroll_area(
            crate::ScrollAreaOption::FRAME | crate::ScrollAreaOption::ENABLE_SCROLL,
            |builder| {
                builder.text("leaf");
            },
        );
    });

    tree.with_node(tree.roots()[0], |node| {
        node.with_children(|children| {
            assert_eq!(children.len(), 4);
            children[0].with_children(|viewport_children| assert_eq!(viewport_children.len(), 1));
        });
        assert!(matches!(node.data, UiNodeData::LegacyContainer(_)));
    })
    .expect("scroll area node missing");
}

#[test]
fn text_nodes_are_recorded_as_widgets() {
    let tree = UiNodeBuilder::build(|builder| {
        builder.text("hello");
    });

    assert_eq!(tree.roots().len(), 1);
    tree.with_node(tree.roots()[0], |node| assert!(matches!(node.data, UiNodeData::Widget(_))))
        .expect("text node missing");
}
