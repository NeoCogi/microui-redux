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
// The retained widget tree owns the long-lived UI structure. Composite nodes
// such as headers, tree nodes, and embedded containers store their child lists
// here and keep stable NodeIds across frames. Each frame the container derives
// a short-lived runtime tree from these retained nodes, uses the previous-frame
// cache for structural pre-handle, then traverses the runtime tree through the
// normal layout and widget paths.

mod builder;
mod cache;
mod node;
mod retained;
mod runtime;

pub use builder::WidgetTreeBuilder;
pub use cache::{NodeCacheEntry, WidgetTreeCache};
pub use node::{NodeId, Policy, WidgetTree, WidgetTreeNode};
pub use retained::{widget_handle, WidgetHandle};

pub(crate) use node::WidgetTreeNodeKind;
pub(crate) use retained::{erased_widget_state, TreeCustomRender, WidgetStateHandleDyn};
pub(crate) use runtime::{RuntimeTreeNode, RuntimeTreeNodeKind};

#[cfg(test)]
mod tests {
    use crate::{AtlasHandle, AtlasSource, Button, CharEntry, Container, ContainerHandle, FontEntry, Input, Recti, SizePolicy, SourceFormat, Style, Vec2i};
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

    fn make_test_atlas() -> AtlasHandle {
        let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
        let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
        let entries = vec![
            (
                '_',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'a',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'b',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
        ];
        let fonts = vec![(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &entries,
            },
        )];
        let source = AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &[],
        };
        AtlasHandle::from(&source)
    }

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
            builder.keyed_widget("a", button_a.clone());
            builder.keyed_widget("b", button_b.clone());
        });
        let ids_a: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();
        let tree_b = WidgetTreeBuilder::build(|builder| {
            builder.keyed_widget("b", button_b.clone());
            builder.keyed_widget("a", button_a.clone());
        });
        let ids_b: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

        assert_eq!(ids_a[0], ids_b[1]);
        assert_eq!(ids_a[1], ids_b[0]);
    }

    #[test]
    fn row_nodes_capture_children_and_track_policy() {
        let button_a = widget_handle(Button::new("A"));
        let button_b = widget_handle(Button::new("B"));

        let tree = WidgetTreeBuilder::build(|builder| {
            builder.row_with_policy(
                Policy::fill(),
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
        let atlas = make_test_atlas();
        let input = Rc::new(RefCell::new(Input::default()));
        let handle = ContainerHandle::new(Container::new("panel", atlas, Rc::new(Style::default()), input));
        let leaf = widget_handle((crate::WidgetOption::NONE, crate::WidgetBehaviourOption::NONE));

        let tree = WidgetTreeBuilder::build(|builder| {
            builder.container_with_policy(
                Policy::fill(),
                handle.clone(),
                crate::ContainerOption::NONE,
                crate::WidgetBehaviourOption::NONE,
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
}
