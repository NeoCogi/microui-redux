//! Borrowed runtime view derived from retained widget tree nodes each frame.

use crate::{
    input::{ContainerOption, WidgetBehaviourOption},
    layout::{SizePolicy, StackDirection},
    ContainerHandle, Custom, Node,
};

use super::{NodeId, TreeCustomRender, TreeRun, WidgetHandle, WidgetStateHandleDyn, WidgetTree, WidgetTreeNode, WidgetTreeNodeKind};

pub(crate) struct RuntimeTreeNode<'a> {
    id: NodeId,
    kind: RuntimeTreeNodeKind<'a>,
    children: Vec<RuntimeTreeNode<'a>>,
}

impl<'a> RuntimeTreeNode<'a> {
    pub(super) fn from_retained(node: &'a WidgetTreeNode) -> Self {
        let (_, kind, children) = node.parts();
        let kind = match kind {
            WidgetTreeNodeKind::Widget { widget } => RuntimeTreeNodeKind::Widget { widget: &**widget },
            WidgetTreeNodeKind::CustomRender { state, render } => RuntimeTreeNodeKind::CustomRender { state, render },
            WidgetTreeNodeKind::Run { run } => RuntimeTreeNodeKind::Run { run },
            WidgetTreeNodeKind::Container { handle, opt, behaviour } => RuntimeTreeNodeKind::Container {
                handle: handle.clone(),
                opt: *opt,
                behaviour: *behaviour,
            },
            WidgetTreeNodeKind::Header { state } => RuntimeTreeNodeKind::Header { state },
            WidgetTreeNodeKind::Tree { state } => RuntimeTreeNodeKind::Tree { state },
            WidgetTreeNodeKind::Row { widths, height } => RuntimeTreeNodeKind::Row { widths, height: *height },
            WidgetTreeNodeKind::Grid { widths, heights } => RuntimeTreeNodeKind::Grid { widths, heights },
            WidgetTreeNodeKind::Column => RuntimeTreeNodeKind::Column,
            WidgetTreeNodeKind::Stack { width, height, direction } => RuntimeTreeNodeKind::Stack {
                width: *width,
                height: *height,
                direction: *direction,
            },
        };
        Self {
            id: node.id(),
            kind,
            children: children.iter().map(Self::from_retained).collect(),
        }
    }

    pub(crate) fn id(&self) -> NodeId {
        self.id
    }

    pub(crate) fn kind(&self) -> &RuntimeTreeNodeKind<'a> {
        &self.kind
    }

    pub(crate) fn children(&self) -> &[RuntimeTreeNode<'a>] {
        &self.children
    }
}

pub(crate) enum RuntimeTreeNodeKind<'a> {
    Widget {
        widget: &'a dyn WidgetStateHandleDyn,
    },
    CustomRender {
        state: &'a WidgetHandle<Custom>,
        render: &'a TreeCustomRender,
    },
    Run {
        run: &'a TreeRun,
    },
    Container {
        handle: ContainerHandle,
        opt: ContainerOption,
        behaviour: WidgetBehaviourOption,
    },
    Header {
        state: &'a WidgetHandle<Node>,
    },
    Tree {
        state: &'a WidgetHandle<Node>,
    },
    Row {
        widths: &'a [SizePolicy],
        height: SizePolicy,
    },
    Grid {
        widths: &'a [SizePolicy],
        heights: &'a [SizePolicy],
    },
    Column,
    Stack {
        width: SizePolicy,
        height: SizePolicy,
        direction: StackDirection,
    },
}

impl WidgetTree {
    pub(crate) fn runtime_roots(&self) -> Vec<RuntimeTreeNode<'_>> {
        self.roots.iter().map(RuntimeTreeNode::from_retained).collect()
    }
}
