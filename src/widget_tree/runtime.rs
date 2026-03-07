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
