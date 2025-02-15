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
use super::*;
use std::cell::RefCell;

pub struct CustomRenderArgs {
    pub content_area: Rect<i32>,
    pub view: Rect<i32>, // clipped area
    pub mouse_event: MouseEvent,
}

pub enum Command {
    Clip {
        rect: Recti,
    },
    Recti {
        rect: Recti,
        color: Color,
    },
    Text {
        font: FontId,
        pos: Vec2i,
        color: Color,
        text: String,
    },
    Icon {
        rect: Recti,
        id: IconId,
        color: Color,
    },
    Slot {
        rect: Recti,
        id: SlotId,
        color: Color,
    },
    SlotRedraw {
        rect: Recti,
        id: SlotId,
        color: Color,
        payload: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
    CustomRender(CustomRenderArgs, Box<dyn FnMut(Dimensioni, &CustomRenderArgs)>),
    None,
}

impl Default for Command {
    fn default() -> Self {
        Command::None
    }
}

pub struct Container {
    pub(crate) atlas: AtlasHandle,
    pub style: Style,
    pub name: String,
    pub rect: Recti,
    pub body: Recti,
    pub content_size: Vec2i,
    pub scroll: Vec2i,
    pub zindex: i32,
    pub command_list: Vec<Command>,
    pub clip_stack: Vec<Recti>,
    pub(crate) layout: LayoutManager,
    pub hover: Option<Id>,
    pub focus: Option<Id>,
    pub updated_focus: bool,
    pub idmngr: IdManager,
    pub input: Rc<RefCell<Input>>,
    pub in_hover_root: bool,
    pub number_edit_buf: String,
    pub number_edit: Option<Id>,

    panels: Vec<ContainerHandle>,
}

impl Container {
    pub(crate) fn new(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>) -> Self {
        Self {
            name: name.to_string(),
            style: style.clone(),
            atlas: atlas,
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Vec2i::default(),
            scroll: Vec2i::default(),
            zindex: 0,
            command_list: Vec::default(),
            clip_stack: Vec::default(),
            hover: None,
            focus: None,
            updated_focus: false,
            layout: LayoutManager::default(),
            idmngr: IdManager::new(),
            number_edit_buf: String::default(),
            number_edit: None,
            in_hover_root: false,
            input: input,

            panels: Default::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.hover = None;
        self.focus = None;
        self.updated_focus = false;
        self.in_hover_root = false;
    }

    pub(crate) fn prepare(&mut self) {
        self.command_list.clear();
        assert!(self.clip_stack.len() == 0);
        self.panels.clear();
    }

    #[inline(never)]
    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        for command in self.command_list.drain(0..) {
            match command {
                Command::Text { text, pos, color, font } => {
                    canvas.draw_chars(font, &text, pos, color);
                }
                Command::Recti { rect, color } => {
                    canvas.draw_rect(rect, color);
                }
                Command::Icon { id, rect, color } => {
                    canvas.draw_icon(id, rect, color);
                }
                Command::Clip { rect } => {
                    canvas.set_clip_rect(rect);
                }
                Command::Slot { rect, id, color } => {
                    canvas.draw_slot(id, rect, color);
                }
                Command::SlotRedraw { rect, id, color, payload } => {
                    canvas.draw_slot_with_function(id, rect, color, payload.clone());
                }
                Command::CustomRender(cra, mut f) => {
                    canvas.end();
                    (*f)(canvas.current_dimension(), &cra);
                }
                Command::None => (),
            }
        }

        for ap in &mut self.panels {
            ap.render(canvas)
        }
    }

    pub fn push_clip_rect(&mut self, rect: Recti) {
        let last = self.get_clip_rect();
        self.clip_stack.push(rect.intersect(&last).unwrap_or_default());
    }

    pub fn pop_clip_rect(&mut self) {
        self.clip_stack.pop();
    }

    pub fn get_clip_rect(&mut self) -> Recti {
        match self.clip_stack.last() {
            Some(r) => *r,
            None => UNCLIPPED_RECT,
        }
    }

    pub fn check_clip(&mut self, r: Recti) -> Clip {
        let cr = self.get_clip_rect();
        if r.x > cr.x + cr.width || r.x + r.width < cr.x || r.y > cr.y + cr.height || r.y + r.height < cr.y {
            return Clip::All;
        }
        if r.x >= cr.x && r.x + r.width <= cr.x + cr.width && r.y >= cr.y && r.y + r.height <= cr.y + cr.height {
            return Clip::None;
        }
        return Clip::Part;
    }

    pub fn push_command(&mut self, cmd: Command) {
        self.command_list.push(cmd);
    }

    pub fn set_clip(&mut self, rect: Recti) {
        self.push_command(Command::Clip { rect });
    }

    pub fn set_focus(&mut self, id: Option<Id>) {
        self.focus = id;
        self.updated_focus = true;
    }

    pub fn draw_rect(&mut self, mut rect: Recti, color: Color) {
        rect = rect.intersect(&self.get_clip_rect()).unwrap_or_default();
        if rect.width > 0 && rect.height > 0 {
            self.push_command(Command::Recti { rect, color });
        }
    }

    pub fn draw_box(&mut self, r: Recti, color: Color) {
        self.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
        self.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
        self.draw_rect(rect(r.x, r.y, 1, r.height), color);
        self.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
    }

    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        let tsize = self.atlas.get_text_size(font, str);
        let rect: Recti = rect(pos.x, pos.y, tsize.width, tsize.height);
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }

        self.push_command(Command::Text {
            text: String::from(str),
            pos,
            color,
            font,
        });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::Icon { id, rect, color });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub fn draw_slot(&mut self, id: SlotId, rect: Recti, color: Color) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::Slot { id, rect, color });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::SlotRedraw { id, rect, color, payload: f });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    #[inline(never)]
    pub fn text(&mut self, text: &str) {
        let font = self.style.font;
        let color = self.style.colors[ControlColor::Text as usize];
        let h = self.atlas.get_font_height(font) as i32;
        self.layout.begin_column();
        self.layout.row(&[-1], h);

        // lines() doesn't count line terminator
        for line in text.lines() {
            let mut r = self.layout.next();
            let mut rx = r.x;
            let words = line.split_inclusive(' ');
            for w in words {
                // TODO: split w when its width > w into many lines
                let tw = self.atlas.get_text_size(font, w).width;
                if tw + rx < r.x + r.width {
                    self.draw_text(font, w, vec2(rx, r.y), color);
                    rx += tw;
                } else {
                    r = self.layout.next();
                    rx = r.x;
                }
            }
        }
        self.layout.end_column();
    }

    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let color = self.style.colors[colorid as usize];
        self.draw_rect(rect, color);
        if colorid == ControlColor::ScrollBase || colorid == ControlColor::ScrollThumb || colorid == ControlColor::TitleBG {
            return;
        }
        let border_color = self.style.colors[ControlColor::Border as usize];
        if border_color.a != 0 {
            self.draw_box(expand_rect(rect, 1), border_color);
        }
    }

    pub fn draw_widget_frame(&mut self, id: Id, rect: Recti, mut colorid: ControlColor, _opt: WidgetOption) {
        if self.focus == Some(id) {
            colorid.focus()
        } else if self.hover == Some(id) {
            colorid.hover()
        }
        self.draw_frame(rect, colorid);
    }

    pub fn draw_container_frame(&mut self, id: Id, rect: Recti, mut colorid: ControlColor, opt: ContainerOption) {
        if opt.has_no_frame() {
            return;
        }

        if self.focus == Some(id) {
            colorid.focus()
        } else if self.hover == Some(id) {
            colorid.hover()
        }
        self.draw_frame(rect, colorid);
    }

    #[inline(never)]
    pub fn draw_control_text(&mut self, str: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let mut pos: Vec2i = Vec2i { x: 0, y: 0 };
        let font = self.style.font;
        let tsize = self.atlas.get_text_size(font, str);
        let padding = self.style.padding;
        let color = self.style.colors[colorid as usize];

        self.push_clip_rect(rect);
        pos.y = rect.y + (rect.height - tsize.height) / 2;
        if opt.is_aligned_center() {
            pos.x = rect.x + (rect.width - tsize.width) / 2;
        } else if opt.is_aligned_right() {
            pos.x = rect.x + rect.width - tsize.width - padding;
        } else {
            pos.x = rect.x + padding;
        }
        self.draw_text(font, str, pos, color);
        self.pop_clip_rect();
    }

    pub fn mouse_over(&mut self, rect: Recti, in_hover_root: bool) -> bool {
        let clip_rect = self.get_clip_rect();
        rect.contains(&self.input.borrow().mouse_pos) && clip_rect.contains(&self.input.borrow().mouse_pos) && in_hover_root
    }

    #[inline(never)]
    pub fn update_control(&mut self, id: Id, rect: Recti, opt: WidgetOption) {
        let in_hover_root = self.in_hover_root;
        let mouseover = self.mouse_over(rect, in_hover_root);
        if self.focus == Some(id) {
            // is this the same ID of the focused widget? by default set it to true unless otherwise
            self.updated_focus = true;
        }
        if opt.is_not_interactive() {
            return;
        }
        if mouseover && self.input.borrow().mouse_down.is_none() {
            self.hover = Some(id);
        }
        if self.focus == Some(id) {
            if !self.input.borrow().mouse_pressed.is_none() && !mouseover {
                self.set_focus(None);
            }
            if self.input.borrow().mouse_down.is_none() && !opt.is_holding_focus() {
                self.set_focus(None);
            }
        }
        if self.hover == Some(id) {
            if !self.input.borrow().mouse_pressed.is_none() {
                self.set_focus(Some(id));
            } else if !mouseover {
                self.hover = None;
            }
        }
    }

    pub fn finish(&mut self) {
        if !self.updated_focus {
            self.focus = None;
        }
        self.updated_focus = false;
    }

    #[inline(never)]
    fn node(&mut self, label: &str, is_treenode: bool, state: NodeState) -> NodeState {
        let id: Id = self.idmngr.get_id_from_str(label);
        self.layout.row(&[-1], 0);
        let mut r = self.layout.next();
        self.update_control(id, r, WidgetOption::NONE);

        let expanded = state.is_expanded();
        let active = expanded ^ (self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id));

        if is_treenode {
            if self.hover == Some(id) {
                self.draw_frame(r, ControlColor::ButtonHover);
            }
        } else {
            self.draw_widget_frame(id, r, ControlColor::Button, WidgetOption::NONE);
        }
        let color = self.style.colors[ControlColor::Text as usize];
        self.draw_icon(if expanded { COLLAPSE_ICON } else { EXPAND_ICON }, rect(r.x, r.y, r.height, r.height), color);
        r.x += r.height - self.style.padding;
        r.width -= r.height - self.style.padding;
        self.draw_control_text(label, r, ControlColor::Text, WidgetOption::NONE);
        return if active { NodeState::Expanded } else { NodeState::Closed };
    }

    #[must_use]
    pub fn header<F: FnOnce(&mut Self)>(&mut self, label: &str, state: NodeState, f: F) -> NodeState {
        let new_state = self.node(label, false, state);
        if new_state.is_expanded() {
            f(self);
        }
        new_state
    }

    #[must_use]
    pub fn treenode<F: FnOnce(&mut Self)>(&mut self, label: &str, state: NodeState, f: F) -> NodeState {
        let res = self.node(label, true, state);
        if res.is_expanded() && self.idmngr.last_id().is_some() {
            let indent = self.style.indent;
            self.layout.top_mut().indent += indent;
            self.idmngr.push_id(self.idmngr.last_id().unwrap());
        }

        if res.is_expanded() {
            f(self);
            let indent = self.style.indent;
            self.layout.top_mut().indent -= indent;
            self.idmngr.pop_id();
        }

        res
    }

    fn clamp(x: i32, a: i32, b: i32) -> i32 {
        min(b, max(a, x))
    }

    #[inline(never)]
    fn scrollbars(&mut self, body: &mut Recti) {
        let sz = self.style.scrollbar_size;
        let mut cs: Vec2i = self.content_size;
        cs.x += self.style.padding * 2;
        cs.y += self.style.padding * 2;
        self.push_clip_rect(body.clone());
        if cs.y > self.body.height {
            body.width -= sz;
        }
        if cs.x > self.body.width {
            body.height -= sz;
        }
        let body = *body;
        let maxscroll = cs.y - body.height;
        if maxscroll > 0 && body.height > 0 {
            let id: Id = self.idmngr.get_id_from_str("!scrollbary");
            let mut base = body;
            base.x = body.x + body.width;
            base.width = self.style.scrollbar_size;
            self.update_control(id, base, WidgetOption::NONE);
            if self.focus == Some(id) && self.input.borrow().mouse_down.is_left() {
                self.scroll.y += self.input.borrow().mouse_delta.y * cs.y / base.height;
            }

            self.draw_frame(base, ControlColor::ScrollBase);
            let mut thumb = base;
            thumb.height = if self.style.thumb_size > base.height * body.height / cs.y {
                self.style.thumb_size
            } else {
                base.height * body.height / cs.y
            };
            thumb.y += self.scroll.y * (base.height - thumb.height) / maxscroll;
            self.draw_frame(thumb, ControlColor::ScrollThumb);
            let in_hover_root = self.in_hover_root;
            if self.mouse_over(body, in_hover_root) {
                // TODO: doesn't solve the issue where we have a panel inside a panel
                self.scroll.y += self.input.borrow().scroll_delta.y;
            }
            self.scroll.y = Self::clamp(self.scroll.y, 0, maxscroll);
        } else {
            self.scroll.y = 0;
        }
        let maxscroll_0 = cs.x - body.width;
        if maxscroll_0 > 0 && body.width > 0 {
            let id_0: Id = self.idmngr.get_id_from_str("!scrollbarx");
            let mut base_0 = body;
            base_0.y = body.y + body.height;
            base_0.height = self.style.scrollbar_size;
            self.update_control(id_0, base_0, WidgetOption::NONE);
            if self.focus == Some(id_0) && self.input.borrow().mouse_down.is_left() {
                self.scroll.x += self.input.borrow().mouse_delta.x * cs.x / base_0.width;
            }

            self.draw_frame(base_0, ControlColor::ScrollBase);
            let mut thumb_0 = base_0;
            thumb_0.width = if self.style.thumb_size > base_0.width * body.width / cs.x {
                self.style.thumb_size
            } else {
                base_0.width * body.width / cs.x
            };
            thumb_0.x += self.scroll.x * (base_0.width - thumb_0.width) / maxscroll_0;
            self.draw_frame(thumb_0, ControlColor::ScrollThumb);
            let in_hover_root = self.in_hover_root;
            if self.mouse_over(body, in_hover_root) {
                self.scroll.x += self.input.borrow().scroll_delta.x;
            }
            self.scroll.x = Self::clamp(self.scroll.x, 0, maxscroll_0);
        } else {
            self.scroll.x = 0;
        }
        self.pop_clip_rect();
    }

    pub fn push_container_body(&mut self, body: Recti, opt: ContainerOption) {
        let mut body = body;
        if !opt.has_no_scroll() {
            self.scrollbars(&mut body);
        }
        let style = self.style;
        let padding = -style.padding;
        let scroll = self.scroll;
        self.layout.push_layout(expand_rect(body, padding), scroll);
        self.layout.style = self.style.clone();
        self.body = body;
    }

    fn pop_panel(&mut self, panel: &mut ContainerHandle) {
        let layout = *panel.inner().layout.top();
        let container = &mut panel.inner_mut();

        match layout.max {
            None => (),
            Some(lm) => container.content_size = Vec2i::new(lm.x - layout.body.x, lm.y - layout.body.y),
        }

        container.layout.stack.pop();
    }

    #[inline(never)]
    fn begin_panel(&mut self, panel: &mut ContainerHandle, opt: ContainerOption) {
        let rect = self.layout.next();
        let clip_rect = panel.inner().body;
        let container = &mut panel.inner_mut();
        container.prepare();

        container.rect = rect;
        if !opt.has_no_frame() {
            self.draw_frame(rect, ControlColor::PanelBG);
        }

        container.in_hover_root = self.in_hover_root;
        container.push_container_body(rect, opt);
        container.push_clip_rect(clip_rect);
    }

    fn end_panel(&mut self, panel: &mut ContainerHandle) {
        panel.inner_mut().pop_clip_rect();
        self.pop_panel(panel);
        self.panels.push(panel.clone())
    }

    pub fn panel<F: FnOnce(&mut ContainerHandle)>(&mut self, panel: &mut ContainerHandle, opt: ContainerOption, f: F) {
        self.begin_panel(panel, opt);

        // call the panel function
        f(panel);

        self.end_panel(panel);
    }

    pub fn set_row_widths_height(&mut self, widths: &[i32], height: i32) {
        self.layout.row(widths, height);
    }

    pub fn column<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.layout.begin_column();
        f(self);
        self.layout.end_column();
    }

    pub fn next_cell(&mut self) -> Recti {
        self.layout.next()
    }

    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    pub fn get_style(&self) -> Style {
        self.style.clone()
    }

    pub fn label(&mut self, text: &str) {
        let layout = self.layout.next();
        self.draw_control_text(text, layout, ControlColor::Text, WidgetOption::NONE);
    }

    #[inline(never)]
    pub fn button_ex(&mut self, label: &str, icon: Option<IconId>, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = if label.len() > 0 {
            self.idmngr.get_id_from_str(label)
        } else {
            self.idmngr.get_id_u32(icon.unwrap().into())
        };
        let r: Recti = self.layout.next();
        self.update_control(id, r, opt);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }
        self.draw_widget_frame(id, r, ControlColor::Button, opt);
        if label.len() > 0 {
            self.draw_control_text(label, r, ControlColor::Text, opt);
        }
        match icon {
            Some(icon) => {
                let color = self.style.colors[ControlColor::Text as usize];
                self.draw_icon(icon, r, color);
            }
            _ => (),
        }
        return res;
    }

    #[inline(never)]
    pub fn button_ex2(&mut self, label: &str, slot: Option<SlotId>, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = if label.len() > 0 {
            self.idmngr.get_id_from_str(label)
        } else {
            self.idmngr.get_id_u32(slot.unwrap().into())
        };
        let r: Recti = self.layout.next();
        self.update_control(id, r, opt);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }
        self.draw_widget_frame(id, r, ControlColor::Button, opt);
        if label.len() > 0 {
            self.draw_control_text(label, r, ControlColor::Text, opt);
        }
        match slot {
            Some(slot) => {
                let color = self.style.colors[ControlColor::Text as usize];
                self.draw_slot(slot, r, color);
            }
            _ => (),
        }
        return res;
    }

    #[inline(never)]
    pub fn button_ex3(&mut self, label: &str, slot: Option<SlotId>, opt: WidgetOption, f: Rc<dyn Fn(usize, usize) -> Color4b>) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = if label.len() > 0 {
            self.idmngr.get_id_from_str(label)
        } else {
            self.idmngr.get_id_u32(slot.unwrap().into())
        };
        let r: Recti = self.layout.next();
        self.update_control(id, r, opt);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }
        self.draw_widget_frame(id, r, ControlColor::Button, opt);
        if label.len() > 0 {
            self.draw_control_text(label, r, ControlColor::Text, opt);
        }
        match slot {
            Some(slot) => {
                let color = self.style.colors[ControlColor::Text as usize];
                self.draw_slot_with_function(slot, r, color, f);
            }
            _ => (),
        }
        return res;
    }

    #[inline(never)]
    pub fn checkbox(&mut self, label: &str, state: &mut bool) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = self.idmngr.get_id_from_ptr(state);
        let mut r: Recti = self.layout.next();
        let box_0: Recti = rect(r.x, r.y, r.height, r.height);
        self.update_control(id, r, WidgetOption::NONE);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::CHANGE;
            *state = *state == false;
        }
        self.draw_widget_frame(id, box_0, ControlColor::Base, WidgetOption::NONE);
        if *state {
            let color = self.style.colors[ControlColor::Text as usize];
            self.draw_icon(CHECK_ICON, box_0, color);
        }
        r = rect(r.x + box_0.width, r.y, r.width - box_0.width, r.height);
        self.draw_control_text(label, r, ControlColor::Text, WidgetOption::NONE);
        return res;
    }

    #[inline(never)]
    fn input_to_mouse_event(&self, id: Id, rect: &Recti) -> MouseEvent {
        let input = self.input.borrow();
        let orig = Vec2i::new(rect.x, rect.y);

        let prev_pos = input.last_mouse_pos - orig;
        let curr_pos = input.mouse_pos - orig;
        if self.focus == Some(id) && input.mouse_down.is_left() {
            return MouseEvent::Drag { prev_pos, curr_pos };
        }

        if self.hover == Some(id) {
            return MouseEvent::Move(curr_pos);
        }
        MouseEvent::None
    }

    #[inline(never)]
    pub fn custom_render_widget<F: FnMut(Dimensioni, &CustomRenderArgs) + 'static>(&mut self, name: &str, opt: WidgetOption, f: F) {
        let id: Id = self.idmngr.get_id_from_str(name);
        let rect: Recti = self.layout.next();
        self.update_control(id, rect, opt);

        let mouse_event = self.input_to_mouse_event(id, &rect);

        let cra = CustomRenderArgs {
            content_area: rect,
            view: self.get_clip_rect(),
            mouse_event,
        };
        self.command_list.push(Command::CustomRender(cra, Box::new(f)));
    }

    #[inline(never)]
    pub fn textbox_raw(&mut self, buf: &mut String, id: Id, r: Recti, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        self.update_control(id, r, opt | WidgetOption::HOLD_FOCUS);
        if self.focus == Some(id) {
            let mut len = buf.len();

            if self.input.borrow().input_text.len() > 0 {
                buf.push_str(self.input.borrow().input_text.as_str());
                len += self.input.borrow().input_text.len() as usize;
                res |= ResourceState::CHANGE
            }

            if self.input.borrow().key_pressed.is_backspace() && len > 0 {
                // skip utf-8 continuation bytes
                buf.pop();
                res |= ResourceState::CHANGE
            }
            if self.input.borrow().key_pressed.is_return() {
                self.set_focus(None);
                res |= ResourceState::SUBMIT;
            }
        }
        self.draw_widget_frame(id, r, ControlColor::Base, opt);
        if self.focus == Some(id) {
            let color = self.style.colors[ControlColor::Text as usize];
            let font = self.style.font;
            let tsize = self.atlas.get_text_size(font, buf.as_str());
            let ofx = r.width - self.style.padding - tsize.width - 1;
            let textx = r.x + (if ofx < self.style.padding { ofx } else { self.style.padding });
            let texty = r.y + (r.height - tsize.height) / 2;

            self.push_clip_rect(r);
            self.draw_text(font, buf.as_str(), vec2(textx, texty), color);
            self.draw_rect(rect(textx + tsize.width, texty, 1, tsize.height), color);
            self.pop_clip_rect();
        } else {
            self.draw_control_text(buf.as_str(), r, ControlColor::Text, opt);
        }
        return res;
    }

    #[inline(never)]
    fn number_textbox(&mut self, precision: usize, value: &mut Real, r: Recti, id: Id) -> ResourceState {
        if self.input.borrow().mouse_pressed.is_left() && self.input.borrow().key_down.is_shift() && self.hover == Some(id) {
            self.number_edit = Some(id);
            self.number_edit_buf.clear();
            self.number_edit_buf.push_str(format!("{:.*}", precision, value).as_str());
        }

        if self.number_edit == Some(id) {
            let mut temp = self.number_edit_buf.clone();
            let res: ResourceState = self.textbox_raw(&mut temp, id, r, WidgetOption::NONE);
            self.number_edit_buf = temp;
            if res.is_submitted() || self.focus != Some(id) {
                match self.number_edit_buf.parse::<f32>() {
                    Ok(v) => {
                        *value = v as Real;
                        self.number_edit = None;
                    }
                    _ => (),
                }
                self.number_edit = None;
            } else {
                return ResourceState::ACTIVE;
            }
        }
        return ResourceState::NONE;
    }

    pub fn textbox_ex(&mut self, buf: &mut String, opt: WidgetOption) -> ResourceState {
        let id: Id = self.idmngr.get_id_from_ptr(buf);
        let r: Recti = self.layout.next();
        return self.textbox_raw(buf, id, r, opt);
    }

    #[inline(never)]
    pub fn slider_ex(&mut self, value: &mut Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let last = *value;
        let mut v = last;
        let id = self.idmngr.get_id_from_ptr(value);
        let base = self.layout.next();
        if !self.number_textbox(precision, &mut v, base, id).is_none() {
            return res;
        }
        self.update_control(id, base, opt);
        if self.focus == Some(id) && (!self.input.borrow().mouse_down.is_none() | self.input.borrow().mouse_pressed.is_left()) {
            v = low + (self.input.borrow().mouse_pos.x - base.x) as Real * (high - low) / base.width as Real;
            if step != 0. {
                v = (v + step / 2 as Real) / step * step;
            }
        }
        v = if high < (if low > v { low } else { v }) {
            high
        } else if low > v {
            low
        } else {
            v
        };
        *value = v;
        if last != v {
            res |= ResourceState::CHANGE;
        }
        self.draw_widget_frame(id, base, ControlColor::Base, opt);
        let w = self.style.thumb_size;
        let x = ((v - low) * (base.width - w) as Real / (high - low)) as i32;
        let thumb = rect(base.x + x, base.y, w, base.height);
        self.draw_widget_frame(id, thumb, ControlColor::Button, opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", precision, value).as_str());
        self.draw_control_text(buff.as_str(), base, ControlColor::Text, opt);
        return res;
    }

    #[inline(never)]
    pub fn number_ex(&mut self, value: &mut Real, step: Real, precision: usize, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = self.idmngr.get_id_from_ptr(value);
        let base: Recti = self.layout.next();
        let last: Real = *value;
        if !self.number_textbox(precision, value, base, id).is_none() {
            return res;
        }
        self.update_control(id, base, opt);
        if self.focus == Some(id) && self.input.borrow().mouse_down.is_left() {
            *value += self.input.borrow().mouse_delta.x as Real * step;
        }
        if *value != last {
            res |= ResourceState::CHANGE;
        }
        self.draw_widget_frame(id, base, ControlColor::Base, opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", precision, value).as_str());
        self.draw_control_text(buff.as_str(), base, ControlColor::Text, opt);
        return res;
    }
}
