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
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
use crate::*;

#[derive(Clone, Copy)]
enum NodeKind {
    Header,
    Tree,
}

#[derive(Clone, Copy)]
/// Expansion state used by tree nodes, headers, and similar widgets.
pub enum NodeStateValue {
    /// Child content is visible.
    Expanded,
    /// Child content is hidden.
    Closed,
}

impl NodeStateValue {
    /// Returns `true` when the node is expanded.
    pub fn is_expanded(&self) -> bool {
        match self {
            Self::Expanded => true,
            _ => false,
        }
    }

    /// Returns `true` when the node is closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Closed => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
/// Persistent state for headers and tree nodes.
pub struct Node {
    /// Label displayed for the node.
    pub label: String,
    /// Current expansion state.
    pub state: NodeStateValue,
    /// Widget options applied to the node.
    pub opt: WidgetOption,
    /// Behaviour options applied to the node.
    pub bopt: WidgetBehaviourOption,
    kind: NodeKind,
}

impl Node {
    /// Creates a header node state with the default widget options.
    pub fn new(label: impl Into<String>, state: NodeStateValue) -> Self { Self::header(label, state) }

    /// Creates a header node state with the default widget options.
    pub fn header(label: impl Into<String>, state: NodeStateValue) -> Self {
        Self {
            label: label.into(),
            state,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
            kind: NodeKind::Header,
        }
    }

    /// Creates a tree node state with the default widget options.
    pub fn tree(label: impl Into<String>, state: NodeStateValue) -> Self {
        Self {
            label: label.into(),
            state,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
            kind: NodeKind::Tree,
        }
    }

    /// Creates a header node state with explicit widget options.
    pub fn with_opt(label: impl Into<String>, state: NodeStateValue, opt: WidgetOption) -> Self { Self::with_opt_header(label, state, opt) }

    /// Creates a header node state with explicit widget options.
    pub fn with_opt_header(label: impl Into<String>, state: NodeStateValue, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            state,
            opt,
            bopt: WidgetBehaviourOption::NONE,
            kind: NodeKind::Header,
        }
    }

    /// Creates a tree node state with explicit widget options.
    pub fn with_opt_tree(label: impl Into<String>, state: NodeStateValue, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            state,
            opt,
            bopt: WidgetBehaviourOption::NONE,
            kind: NodeKind::Tree,
        }
    }

    /// Returns `true` when the node is expanded.
    pub fn is_expanded(&self) -> bool { self.state.is_expanded() }

    /// Returns `true` when the node is closed.
    pub fn is_closed(&self) -> bool { self.state.is_closed() }

    /// Returns `true` when this node is configured as a tree node.
    pub fn is_tree(&self) -> bool { matches!(self.kind, NodeKind::Tree) }

    /// Returns `true` when this node is configured as a header node.
    pub fn is_header(&self) -> bool { matches!(self.kind, NodeKind::Header) }

    fn handle_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let expanded = self.state.is_expanded();
        let style = ctx.style();
        let padding = style.padding;
        let text_color = style.colors[ControlColor::Text as usize];
        let mut r = ctx.rect();

        match self.kind {
            NodeKind::Tree => {
                if control.hovered {
                    ctx.draw_frame(r, ControlColor::ButtonHover);
                }
            }
            NodeKind::Header => {
                ctx.draw_widget_frame(control, r, ControlColor::Button, self.opt);
            }
        }

        ctx.draw_icon(
            if expanded { COLLAPSE_ICON } else { EXPAND_ICON },
            rect(r.x, r.y, r.height, r.height),
            text_color,
        );
        r.x += r.height - padding;
        r.width -= r.height - padding;
        ctx.draw_control_text(self.label.as_str(), r, ControlColor::Text, self.opt);

        if control.clicked {
            self.state = if expanded { NodeStateValue::Closed } else { NodeStateValue::Expanded };
            res |= ResourceState::CHANGE;
        }
        res
    }
}

implement_widget!(Node, handle_widget);
