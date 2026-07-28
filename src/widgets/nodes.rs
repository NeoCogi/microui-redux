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
//! Collapsible header and tree-node widget state.
//!
//! These widgets own expansion state while container traversal decides whether child nodes are
//! included in each retained pass.
use crate::*;
use super::WidgetConfig;

#[derive(Clone, Copy)]
/// Built-in expandable node visual mode.
enum NodeKind {
    /// Header node without tree indentation affordance.
    Header,
    /// Tree node with nested-child affordance.
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
    /// Shared widget configuration.
    pub config: WidgetConfig,
    /// Visual/behavior mode for this expandable node.
    kind: NodeKind,
}

impl Node {
    /// Creates a header node state with the default widget options.
    pub fn header(label: impl Into<String>, state: NodeStateValue) -> Self {
        Self {
            label: label.into(),
            state,
            config: WidgetConfig::new(WidgetOption::FRAME),
            kind: NodeKind::Header,
        }
    }

    /// Creates a tree node state with the default widget options.
    pub fn tree(label: impl Into<String>, state: NodeStateValue) -> Self {
        Self {
            label: label.into(),
            state,
            config: WidgetConfig::new(WidgetOption::NONE),
            kind: NodeKind::Tree,
        }
    }

    /// Applies widget options to this node state.
    pub fn with_options(mut self, opt: WidgetOption) -> Self {
        self.config.opt = opt;
        self
    }

    /// Returns `true` when the node is expanded.
    pub fn is_expanded(&self) -> bool {
        self.state.is_expanded()
    }

    /// Returns `true` when the node is closed.
    pub fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    /// Returns `true` when this node is configured as a tree node.
    pub fn is_tree(&self) -> bool {
        matches!(self.kind, NodeKind::Tree)
    }

    /// Returns `true` when this node is configured as a header node.
    pub fn is_header(&self) -> bool {
        matches!(self.kind, NodeKind::Header)
    }

    /// Measures disclosure icon plus label text.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        let font = style.resolve_font_choice(self.config.font);
        let font_height = atlas.get_font_height(font) as i32;
        let icon = atlas.get_icon_size(EXPAND_ICON);
        let text_w = if self.label.is_empty() {
            0
        } else {
            atlas.get_text_size(font, self.label.as_str()).width
        };
        let height = (font_height.max(icon.height) + vertical_pad * 2).max(0);
        let consumed = (height - padding).max(icon.width);
        let width = (padding * 2 + consumed + text_w).max(0);
        Dimensioni::new(width, height)
    }

    /// Toggles expanded/closed state on click.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: &[UiInputEvent]) -> ResourceState {
        let mut res = ResourceState::NONE;
        if ctx.clicked() {
            // The node owns expansion state; container traversal reads it to decide child passes.
            self.state = if self.state.is_expanded() {
                NodeStateValue::Closed
            } else {
                NodeStateValue::Expanded
            };
            res |= ResourceState::CHANGE;
        }
        res
    }

    /// Paints header/tree frame, disclosure icon, and label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let expanded = self.state.is_expanded();
        let style = ctx.style();
        let padding = style.padding;
        let text_color = style.colors[ControlColor::Text as usize];
        let mut r = ctx.local_rect();

        match self.kind {
            NodeKind::Tree => {
                if ctx.hovered() {
                    ctx.draw_rect(r, ctx.style().colors[ControlColor::ButtonHover as usize]);
                }
            }
            NodeKind::Header => {
                ctx.draw_widget_fill(r, ControlColor::Button);
            }
        }

        // Reserve a square disclosure region at the left of the row.
        ctx.draw_icon(
            if expanded { COLLAPSE_ICON } else { EXPAND_ICON },
            rect(r.x, r.y, r.height, r.height),
            text_color,
        );
        r.x += r.height - padding;
        r.width -= r.height - padding;
        let font = ctx.style().resolve_font_choice(self.config.font);
        ctx.draw_control_text_with_font(font, self.label.as_str(), r, ControlColor::Text, self.config.opt);
    }
}

impl Widget for Node {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Vec<UiInputEvent>) -> ResourceState {
        self.update_widget(ctx, &input)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}
