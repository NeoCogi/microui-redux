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
//! Embedded panel lifecycle and scroll management.

use super::*;

#[derive(Copy, Clone)]
enum InternalControlPart {
    ScrollbarY,
    ScrollbarX,
}

#[derive(Copy, Clone)]
struct ScrollbarSpec {
    axis: ScrollAxis,
    part: InternalControlPart,
    node_id: NodeId,
    base: Recti,
    max_scroll: i32,
    view_len: i32,
    content_len: i32,
}

impl Container {
    pub(crate) fn panel_scope_id(&self, node_id: NodeId) -> Id {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        fn write(mut hash: u64, value: u64) -> u64 {
            for byte in value.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash
        }

        let hash = write(FNV_OFFSET_BASIS, 0x6d69_6372_6f75_695f_u64);
        let hash = write(hash, 0x7061_6e65_6c5f_7363_u64);
        let hash = write(hash, self.internal_id_seed.raw() as u64);
        Id::new(write(hash, node_id.raw() as u64))
    }

    fn internal_control_node_id(&self, part: InternalControlPart) -> NodeId {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        fn write(mut hash: u64, value: u64) -> u64 {
            for byte in value.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash
        }

        let part = match part {
            InternalControlPart::ScrollbarY => 1,
            InternalControlPart::ScrollbarX => 2,
        };
        let hash = write(FNV_OFFSET_BASIS, 0x6d69_6372_6f75_695f_u64);
        let hash = write(hash, 0x696e_7465_726e_616c_u64);
        let hash = write(hash, self.internal_id_seed.raw() as u64);
        Id::new(write(hash, part))
    }

    pub(crate) fn consume_pending_scroll(&mut self) {
        if !self.scroll_enabled {
            return;
        }
        let delta = match self.interaction.pending_scroll {
            Some(delta) if delta.x != 0 || delta.y != 0 => delta,
            _ => return,
        };

        let mut consumed = false;
        let mut scroll = self.scroll;
        let mut content_size = self.content_size;
        let padding = self.style.as_ref().padding * 2;
        content_size.width += padding;
        content_size.height += padding;
        let body = self.body;

        let maxscroll_y = content_size.height - body.height;
        if delta.y != 0 && maxscroll_y > 0 && body.height > 0 {
            let new_scroll = Self::clamp(scroll.y + delta.y, 0, maxscroll_y);
            if new_scroll != scroll.y {
                scroll.y = new_scroll;
                consumed = true;
            }
        }

        let maxscroll_x = content_size.width - body.width;
        if delta.x != 0 && maxscroll_x > 0 && body.width > 0 {
            let new_scroll = Self::clamp(scroll.x + delta.x, 0, maxscroll_x);
            if new_scroll != scroll.x {
                scroll.x = new_scroll;
                consumed = true;
            }
        }

        if consumed {
            self.scroll = scroll;
            self.interaction.clear_pending_scroll();
        }
    }

    fn resolve_scrollbars(&mut self, body: &mut Recti) {
        let (scrollbar_size, padding) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding)
        };
        let sz = scrollbar_size;
        let mut cs = self.content_size;
        cs.width += padding * 2;
        cs.height += padding * 2;
        let base_body = *body;
        if cs.height > base_body.height {
            body.width -= sz;
        }
        if cs.width > base_body.width {
            body.height -= sz;
        }
        let body = *body;
        let maxscroll_y = scrollbar_max_scroll(cs.height, body.height);
        self.scroll.y = if maxscroll_y > 0 && body.height > 0 {
            Self::clamp(self.scroll.y, 0, maxscroll_y)
        } else {
            0
        };

        let maxscroll_x = scrollbar_max_scroll(cs.width, body.width);
        self.scroll.x = if maxscroll_x > 0 && body.width > 0 {
            Self::clamp(self.scroll.x, 0, maxscroll_x)
        } else {
            0
        };
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[inline(never)]
    pub(crate) fn scrollbars(&mut self, body: &mut Recti) {
        self.resolve_scrollbars(body);
        self.render_scrollbars(*body);
    }

    pub(crate) fn render_active_scrollbars(&mut self) {
        if self.scroll_enabled {
            self.update_scrollbars(self.body);
            self.paint_scrollbars(self.body);
        }
    }

    pub(crate) fn update_active_scrollbars(&mut self) {
        if self.scroll_enabled {
            self.update_scrollbars(self.body);
        }
    }

    pub(crate) fn paint_active_scrollbars(&mut self) {
        if self.scroll_enabled {
            self.paint_scrollbars(self.body);
        }
    }

    pub(crate) fn render_scrollbars(&mut self, body: Recti) {
        self.update_scrollbars(body);
        self.paint_scrollbars(body);
    }

    fn scrollbar_content_size(&self, padding: i32) -> Dimensioni {
        let mut cs = self.content_size;
        cs.width += padding * 2;
        cs.height += padding * 2;
        cs
    }

    fn scrollbar_clip_rect(body: Recti, content_size: Dimensioni, scrollbar_size: i32) -> Recti {
        let mut clip_rect = body;
        if scrollbar_max_scroll(content_size.height, body.height) > 0 && body.height > 0 {
            clip_rect.width += scrollbar_size;
        }
        if scrollbar_max_scroll(content_size.width, body.width) > 0 && body.width > 0 {
            clip_rect.height += scrollbar_size;
        }
        clip_rect
    }

    fn scrollbar_spec(&self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32) -> Option<ScrollbarSpec> {
        let (part, view_len, content_len) = match axis {
            ScrollAxis::Vertical => (InternalControlPart::ScrollbarY, body.height, content_size.height),
            ScrollAxis::Horizontal => (InternalControlPart::ScrollbarX, body.width, content_size.width),
        };
        let max_scroll = scrollbar_max_scroll(content_len, view_len);
        if max_scroll <= 0 || view_len <= 0 {
            return None;
        }
        let node_id = self.internal_control_node_id(part);
        let base = scrollbar_base(axis, body, scrollbar_size);
        Some(ScrollbarSpec {
            axis,
            part,
            node_id,
            base,
            max_scroll,
            view_len,
            content_len,
        })
    }

    fn scroll_axis(&self, axis: ScrollAxis) -> i32 {
        match axis {
            ScrollAxis::Vertical => self.scroll.y,
            ScrollAxis::Horizontal => self.scroll.x,
        }
    }

    fn set_scroll_axis(&mut self, axis: ScrollAxis, value: i32) {
        match axis {
            ScrollAxis::Vertical => self.scroll.y = value,
            ScrollAxis::Horizontal => self.scroll.x = value,
        }
    }

    fn add_scroll_axis(&mut self, axis: ScrollAxis, delta: i32) {
        let value = self.scroll_axis(axis) + delta;
        self.set_scroll_axis(axis, value);
    }

    fn update_scrollbar_internal(&mut self, spec: ScrollbarSpec) -> (ControlState, ResourceState) {
        match spec.part {
            InternalControlPart::ScrollbarY => {
                let mut state = std::mem::replace(&mut self.scrollbar_y_state, Internal::new("!scrollbary"));
                let output = self.update_internal_node(spec.node_id, &mut state, spec.base);
                self.scrollbar_y_state = state;
                output
            }
            InternalControlPart::ScrollbarX => {
                let mut state = std::mem::replace(&mut self.scrollbar_x_state, Internal::new("!scrollbarx"));
                let output = self.update_internal_node(spec.node_id, &mut state, spec.base);
                self.scrollbar_x_state = state;
                output
            }
        }
    }

    fn paint_scrollbar_internal(&mut self, spec: ScrollbarSpec, control: &ControlState) {
        match spec.part {
            InternalControlPart::ScrollbarY => {
                let mut state = std::mem::replace(&mut self.scrollbar_y_state, Internal::new("!scrollbary"));
                self.paint_internal_node(spec.node_id, &mut state, spec.base, control);
                self.scrollbar_y_state = state;
            }
            InternalControlPart::ScrollbarX => {
                let mut state = std::mem::replace(&mut self.scrollbar_x_state, Internal::new("!scrollbarx"));
                self.paint_internal_node(spec.node_id, &mut state, spec.base, control);
                self.scrollbar_x_state = state;
            }
        }
    }

    fn update_scrollbar(&mut self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32) {
        let Some(spec) = self.scrollbar_spec(axis, body, content_size, scrollbar_size) else {
            self.set_scroll_axis(axis, 0);
            return;
        };

        self.record_tree_layout(
            spec.node_id,
            NodeLayout::new(spec.base, spec.base, Dimensioni::new(spec.base.width, spec.base.height)),
        );
        let (control, result) = self.update_scrollbar_internal(spec);
        self.record_tree_interaction(spec.node_id, NodeInteraction::new(control, result));
        if control.active {
            let delta = scrollbar_drag_delta(spec.axis, self.input.borrow().mouse_delta, spec.content_len, spec.base);
            self.add_scroll_axis(axis, delta);
        }
        let scroll = Self::clamp(self.scroll_axis(axis), 0, spec.max_scroll);
        self.set_scroll_axis(axis, scroll);
    }

    fn paint_scrollbar(&mut self, axis: ScrollAxis, body: Recti, content_size: Dimensioni, scrollbar_size: i32, thumb_size: i32) {
        let Some(spec) = self.scrollbar_spec(axis, body, content_size, scrollbar_size) else {
            return;
        };

        let control = self
            .tree
            .current_interaction(spec.node_id)
            .map(|interaction| interaction.control)
            .unwrap_or_default();
        self.paint_scrollbar_internal(spec, &control);
        self.draw_frame(spec.base, ControlColor::ScrollBase);
        let thumb = scrollbar_thumb(spec.axis, spec.base, spec.view_len, spec.content_len, self.scroll_axis(axis), thumb_size);
        self.draw_frame(thumb, ControlColor::ScrollThumb);
    }

    fn update_scrollbars(&mut self, body: Recti) {
        let (scrollbar_size, padding) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding)
        };
        let cs = self.scrollbar_content_size(padding);
        let maxscroll_y = scrollbar_max_scroll(cs.height, body.height);
        let maxscroll_x = scrollbar_max_scroll(cs.width, body.width);
        let clip_rect = Self::scrollbar_clip_rect(body, cs, scrollbar_size);
        self.push_clip_rect(clip_rect);
        if maxscroll_y > 0 {
            self.update_scrollbar(ScrollAxis::Vertical, body, cs, scrollbar_size);
        } else {
            self.scroll.y = 0;
        }
        if maxscroll_x > 0 {
            self.update_scrollbar(ScrollAxis::Horizontal, body, cs, scrollbar_size);
        } else {
            self.scroll.x = 0;
        }
        self.pop_clip_rect();
    }

    fn paint_scrollbars(&mut self, body: Recti) {
        let (scrollbar_size, padding, thumb_size) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding, style.thumb_size)
        };
        let cs = self.scrollbar_content_size(padding);
        let clip_rect = Self::scrollbar_clip_rect(body, cs, scrollbar_size);
        self.push_clip_rect(clip_rect);
        self.paint_scrollbar(ScrollAxis::Vertical, body, cs, scrollbar_size, thumb_size);
        self.paint_scrollbar(ScrollAxis::Horizontal, body, cs, scrollbar_size, thumb_size);
        self.pop_clip_rect();
    }

    /// Configures layout state for the container's client area without drawing.
    pub(crate) fn configure_container_body(&mut self, body: Recti, scroll_behavior: ScrollBehavior) {
        let mut body = body;
        self.scroll_enabled = !scroll_behavior.is_no_scroll();
        if self.scroll_enabled {
            self.resolve_scrollbars(&mut body);
        }
        let (layout_padding, style_padding, font, style_clone) = {
            let style = self.style.as_ref();
            (-style.padding, style.padding, style.font, *style)
        };
        let scroll = self.scroll;
        self.layout.reset(expand_rect(body, layout_padding), scroll);
        self.layout.style = style_clone;
        let font_height = self.atlas.get_font_height(font) as i32;
        let vertical_pad = crate::text_layout::vertical_text_padding(style_padding);
        let icon_height = self.atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let default_height = max(font_height + vertical_pad * 2, icon_height);
        self.layout.set_default_cell_height(default_height);
        self.body = body;
    }

    /// Configures layout state for the container's client area, handling scrollbars when necessary.
    #[cfg(test)]
    pub fn push_container_body(&mut self, body: Recti, _opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        self.configure_container_body(body, scroll_behavior);
        self.render_active_scrollbars();
    }

    fn pop_panel_container(container: &mut Container) {
        let layout_body = container.layout.current_body();
        let layout_max = container.layout.current_max();
        if let Some(lm) = layout_max {
            container.content_size = Dimensioni::new(lm.x - layout_body.x, lm.y - layout_body.y);
        }

        container.layout.pop_scope();
    }

    fn begin_panel_layout_container(&mut self, container: &mut Container, scroll_behavior: ScrollBehavior, policy: Policy) {
        let rect = self.layout.next_with_policies(Dimensioni::default(), policy.width, policy.height);
        container.prepare();
        container.rect = rect;
        container.configure_container_body(rect, scroll_behavior);
    }

    fn apply_panel_base_state(&self, container: &mut Container, panel_scope: Id) {
        container.set_internal_id_seed(panel_scope);
        container.style = self.style.clone();
    }

    fn apply_panel_layout_state(&self, container: &mut Container, panel_scope: Id, scroll_behavior: ScrollBehavior, layout: NodeLayout) {
        self.apply_panel_base_state(container, panel_scope);
        container.rect = layout.rect;
        container.body = layout.body;
        container.content_size = layout.content_size;
        container.scroll_enabled = !scroll_behavior.is_no_scroll();
    }

    pub(crate) fn begin_panel_layout(
        &mut self,
        panel: &mut ContainerHandle,
        node_id: NodeId,
        _opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        policy: Policy,
    ) {
        let panel_scope = self.panel_scope_id(node_id);
        let container = &mut panel.inner_mut();
        self.apply_panel_base_state(container, panel_scope);
        self.begin_panel_layout_container(container, scroll_behavior, policy);
    }

    pub(crate) fn end_panel_layout(&mut self, panel: &mut ContainerHandle) {
        let container = &mut panel.inner_mut();
        Self::pop_panel_container(container);
    }

    pub(crate) fn measure_panel_layout(
        &mut self,
        panel: &ContainerHandle,
        node_id: NodeId,
        scroll_behavior: ScrollBehavior,
        policy: Policy,
        results: &FrameResults,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        let mut scratch = panel.inner().measurement_scratch();
        scratch.measurement_mode = true;
        self.apply_panel_base_state(&mut scratch, self.panel_scope_id(node_id));
        self.begin_panel_layout_container(&mut scratch, scroll_behavior, policy);
        scratch.layout_tree_nodes(results, children);
        Self::pop_panel_container(&mut scratch);
        NodeLayout::new(scratch.rect(), scratch.body(), scratch.content_size())
    }

    pub(crate) fn begin_panel_update(
        &mut self,
        panel: &mut ContainerHandle,
        node_id: NodeId,
        _opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        layout: NodeLayout,
    ) {
        let panel_id = self.retained_id_for_node(node_id);
        let panel_scope = self.panel_scope_id(node_id);
        if self.hit_test_rect(layout.rect, self.interaction.in_hover_root) {
            self.interaction.set_next_hover_root_child(panel_id, layout.rect);
        }

        let container = &mut panel.inner_mut();
        self.apply_panel_layout_state(container, panel_scope, scroll_behavior, layout);

        container.interaction.in_hover_root = self.interaction.in_hover_root && self.interaction.hover_root_child == Some(panel_id);
        if self.interaction.pending_scroll.is_some() && container.interaction.in_hover_root {
            container.interaction.seed_pending_scroll(self.interaction.take_pending_scroll());
        }
        container.push_clip_rect(layout.body);
    }

    pub(crate) fn end_panel_update(&mut self, panel: &mut ContainerHandle) {
        panel.inner_mut().pop_clip_rect();
        {
            let mut inner = panel.inner_mut();
            inner.update_active_scrollbars();
            inner.consume_pending_scroll();
            let pending = inner.interaction.take_pending_scroll();
            if self.interaction.pending_scroll.is_none() {
                self.interaction.seed_pending_scroll(pending);
            }
        }
    }

    pub(crate) fn begin_panel_paint(
        &mut self,
        panel: &mut ContainerHandle,
        node_id: NodeId,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        layout: NodeLayout,
    ) {
        let panel_scope = self.panel_scope_id(node_id);
        let container = &mut panel.inner_mut();
        self.apply_panel_layout_state(container, panel_scope, scroll_behavior, layout);

        if !opt.has_no_frame() {
            self.draw_frame(layout.rect, ControlColor::PanelBG);
        }

        container.paint_active_scrollbars();
        container.push_clip_rect(layout.body);
    }

    pub(crate) fn end_panel_paint(&mut self, panel: &mut ContainerHandle) {
        panel.inner_mut().pop_clip_rect();
        self.draw.push_command(Command::RetainedPanel { handle: panel.clone() });
        self.panels.push(panel.clone())
    }
}
