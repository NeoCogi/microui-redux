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

impl Container {
    pub(crate) fn consume_pending_scroll(&mut self) {
        if !self.scroll_enabled {
            return;
        }
        let delta = match self.pending_scroll {
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
            self.pending_scroll = None;
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
            self.render_scrollbars(self.body);
        }
    }

    pub(crate) fn render_scrollbars(&mut self, body: Recti) {
        let (scrollbar_size, padding, thumb_size) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding, style.thumb_size)
        };
        let mut cs = self.content_size;
        cs.width += padding * 2;
        cs.height += padding * 2;
        let maxscroll_y = scrollbar_max_scroll(cs.height, body.height);
        let maxscroll_x = scrollbar_max_scroll(cs.width, body.width);
        let mut clip_rect = body;
        if maxscroll_y > 0 && body.height > 0 {
            clip_rect.width += scrollbar_size;
        }
        if maxscroll_x > 0 && body.width > 0 {
            clip_rect.height += scrollbar_size;
        }
        self.push_clip_rect(clip_rect);
        if maxscroll_y > 0 && body.height > 0 {
            let scrollbar_y_id = widget_id_of(&self.scrollbar_y_state);
            let base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
            let control = self.update_control_with_opts(scrollbar_y_id, base, self.scrollbar_y_state.opt, self.scrollbar_y_state.bopt);
            {
                let mut ctx = WidgetCtx::new(
                    scrollbar_y_id,
                    base,
                    &mut self.command_list,
                    &mut self.triangle_vertices,
                    &mut self.clip_stack,
                    self.style.as_ref(),
                    &self.atlas,
                    &mut self.focus,
                    &mut self.updated_focus,
                    self.in_hover_root,
                    None,
                );
                let _ = self.scrollbar_y_state.run(&mut ctx, &control);
            }
            if control.active {
                let delta = scrollbar_drag_delta(ScrollAxis::Vertical, self.input.borrow().mouse_delta, cs.height, base);
                self.scroll.y += delta;
            }
            self.scroll.y = Self::clamp(self.scroll.y, 0, maxscroll_y);
            self.draw_frame(base, ControlColor::ScrollBase);
            let thumb = scrollbar_thumb(ScrollAxis::Vertical, base, body.height, cs.height, self.scroll.y, thumb_size);
            self.draw_frame(thumb, ControlColor::ScrollThumb);
        } else {
            self.scroll.y = 0;
        }

        if maxscroll_x > 0 && body.width > 0 {
            let scrollbar_x_id = widget_id_of(&self.scrollbar_x_state);
            let base = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
            let control = self.update_control_with_opts(scrollbar_x_id, base, self.scrollbar_x_state.opt, self.scrollbar_x_state.bopt);
            {
                let mut ctx = WidgetCtx::new(
                    scrollbar_x_id,
                    base,
                    &mut self.command_list,
                    &mut self.triangle_vertices,
                    &mut self.clip_stack,
                    self.style.as_ref(),
                    &self.atlas,
                    &mut self.focus,
                    &mut self.updated_focus,
                    self.in_hover_root,
                    None,
                );
                let _ = self.scrollbar_x_state.run(&mut ctx, &control);
            }
            if control.active {
                let delta = scrollbar_drag_delta(ScrollAxis::Horizontal, self.input.borrow().mouse_delta, cs.width, base);
                self.scroll.x += delta;
            }
            self.scroll.x = Self::clamp(self.scroll.x, 0, maxscroll_x);
            self.draw_frame(base, ControlColor::ScrollBase);
            let thumb = scrollbar_thumb(ScrollAxis::Horizontal, base, body.width, cs.width, self.scroll.x, thumb_size);
            self.draw_frame(thumb, ControlColor::ScrollThumb);
        } else {
            self.scroll.x = 0;
        }
        self.pop_clip_rect();
    }

    /// Configures layout state for the container's client area without drawing.
    pub(crate) fn configure_container_body(&mut self, body: Recti, bopt: WidgetBehaviourOption) {
        let mut body = body;
        self.scroll_enabled = !bopt.is_no_scroll();
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
        let vertical_pad = Self::vertical_text_padding(style_padding);
        let icon_height = self.atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let default_height = max(font_height + vertical_pad * 2, icon_height);
        self.layout.set_default_cell_height(default_height);
        self.body = body;
    }

    /// Configures layout state for the container's client area, handling scrollbars when necessary.
    pub fn push_container_body(&mut self, body: Recti, _opt: ContainerOption, bopt: WidgetBehaviourOption) {
        self.configure_container_body(body, bopt);
        self.render_active_scrollbars();
    }

    fn pop_panel(&mut self, panel: &mut ContainerHandle) {
        let layout_body = panel.inner().layout.current_body();
        let layout_max = panel.inner().layout.current_max();
        let container = &mut panel.inner_mut();

        if let Some(lm) = layout_max {
            container.content_size = Dimensioni::new(lm.x - layout_body.x, lm.y - layout_body.y);
        }

        container.layout.pop_scope();
    }

    pub(crate) fn begin_panel_layout(&mut self, panel: &mut ContainerHandle, _opt: ContainerOption, bopt: WidgetBehaviourOption) {
        let rect = self.layout.next();
        let container = &mut panel.inner_mut();
        container.prepare();
        container.style = self.style.clone();
        container.rect = rect;
        container.configure_container_body(rect, bopt);
    }

    pub(crate) fn end_panel_layout(&mut self, panel: &mut ContainerHandle) {
        self.pop_panel(panel);
    }

    pub(crate) fn begin_panel_render(&mut self, panel: &mut ContainerHandle, opt: ContainerOption, bopt: WidgetBehaviourOption, layout: NodeLayout) {
        let panel_id = container_id_of(panel);
        if self.hit_test_rect(layout.rect, self.in_hover_root) {
            self.next_hover_root_child = Some(panel_id);
            self.next_hover_root_child_rect = Some(layout.rect);
        }

        let container = &mut panel.inner_mut();
        container.style = self.style.clone();
        container.rect = layout.rect;
        container.body = layout.body;
        container.content_size = layout.content_size;
        container.scroll_enabled = !bopt.is_no_scroll();

        if !opt.has_no_frame() {
            self.draw_frame(layout.rect, ControlColor::PanelBG);
        }

        container.in_hover_root = self.in_hover_root && self.hover_root_child == Some(panel_id);
        if self.pending_scroll.is_some() && container.in_hover_root {
            container.pending_scroll = self.pending_scroll.take();
        }
        container.render_active_scrollbars();
        container.push_clip_rect(layout.body);
    }

    pub(crate) fn end_panel_render(&mut self, panel: &mut ContainerHandle) {
        panel.inner_mut().pop_clip_rect();
        {
            let mut inner = panel.inner_mut();
            inner.consume_pending_scroll();
            let pending = inner.pending_scroll.take();
            if self.pending_scroll.is_none() {
                self.pending_scroll = pending;
            }
        }
        self.panels.push(panel.clone())
    }
}
