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
use std::{cell::RefCell, f32, hash::Hash, rc::Rc};

use rs_math3d::*;

mod container;
mod idmngr;
mod layout;

pub use idmngr::*;
pub use layout::*;
pub use container::*;

use bitflags::*;
use std::cmp::{min, max};
pub trait Atlas {
    fn get_char_width(&self, font: FontId, c: char) -> usize;
    fn get_font_height(&self, font: FontId) -> usize;
    fn get_icon_size(&self, icon: Icon) -> Dimensioni;

    fn get_text_size(&self, font: FontId, text: &str) -> Dimensioni {
        let mut res = 0;
        let mut acc_x = 0;
        let mut acc_y = 0;
        let h = self.get_font_height(font);
        for c in text.chars() {
            if acc_y == 0 {
                acc_y = h
            }
            if c == '\n' {
                res = max(res, acc_x);
                acc_x = 0;
                acc_y += h;
            }
            acc_x += self.get_char_width(font, c);
        }
        res = max(res, acc_x);
        Dimension::new(res as i32, acc_y as i32)
    }
}

pub trait Canvas {
    fn draw_rect(&mut self, rect: Recti, color: Color);
    fn draw_chars(&mut self, text: &[char], pos: Vec2i, color: Color);
    fn draw_icon(&mut self, id: Icon, r: Recti, color: Color);
    fn set_clip_rect(&mut self, width: i32, height: i32, rect: Recti);
    fn clear(&mut self, width: i32, height: i32, clr: Color);
    fn flush(&mut self);
}

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
pub enum Clip {
    None = 0,
    Part = 1,
    All = 2,
}

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
pub enum ControlColor {
    Max = 14,
    ScrollThumb = 13,
    ScrollBase = 12,
    BaseFocus = 11,
    BaseHover = 10,
    Base = 9,
    ButtonFocus = 8,
    ButtonHover = 7,
    Button = 6,
    PanelBG = 5,
    TitleText = 4,
    TitleBG = 3,
    WindowBG = 2,
    Border = 1,
    Text = 0,
}

impl ControlColor {
    pub fn hover(&mut self) {
        *self = match self {
            Self::Base => Self::BaseHover,
            Self::Button => Self::ButtonHover,
            _ => *self,
        }
    }

    pub fn focus(&mut self) {
        *self = match self {
            Self::Base => Self::BaseFocus,
            Self::Button => Self::ButtonFocus,
            Self::BaseHover => Self::BaseFocus,
            Self::ButtonHover => Self::ButtonFocus,
            _ => *self,
        }
    }
}

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
pub enum Icon {
    Max = 5,
    Expanded = 4,
    Collapsed = 3,
    Check = 2,
    Close = 1,
    None = 0,
}

bitflags! {
    pub struct ResourceState : u32 {
        const CHANGE = 4;
        const SUBMIT = 2;
        const ACTIVE = 1;
        const NONE = 0;
    }
}

impl ResourceState {
    pub fn is_changed(&self) -> bool {
        self.intersects(Self::CHANGE)
    }
    pub fn is_submitted(&self) -> bool {
        self.intersects(Self::SUBMIT)
    }
    pub fn is_active(&self) -> bool {
        self.intersects(Self::ACTIVE)
    }
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }
}

bitflags! {
    #[derive(Copy, Clone)]
    pub struct WidgetOption : u32 {
        const CLOSED = 2048;
        const POPUP= 1024;
        const AUTO_SIZE = 512;
        const HOLD_FOCUS = 256;
        const NO_TITLE = 128;
        const NO_CLOSE = 64;
        const NO_SCROLL = 32;
        const NO_RESIZE = 16;
        const NO_FRAME = 8;
        const NO_INTERACT = 4;
        const ALIGN_RIGHT = 2;
        const ALIGN_CENTER = 1;
        const NONE = 0;
    }
}

#[derive(Clone, Copy)]
pub enum NodeState {
    Expanded,
    Closed,
}

impl NodeState {
    pub fn is_expanded(&self) -> bool {
        match self {
            Self::Expanded => true,
            _ => false,
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            Self::Closed => true,
            _ => false,
        }
    }
}

impl WidgetOption {
    pub fn is_closed(&self) -> bool {
        self.intersects(WidgetOption::CLOSED)
    }
    pub fn is_popup(&self) -> bool {
        self.intersects(WidgetOption::POPUP)
    }
    pub fn is_auto_sizing(&self) -> bool {
        self.intersects(WidgetOption::AUTO_SIZE)
    }
    pub fn is_holding_focus(&self) -> bool {
        self.intersects(WidgetOption::HOLD_FOCUS)
    }
    pub fn has_no_title(&self) -> bool {
        self.intersects(WidgetOption::NO_TITLE)
    }
    pub fn has_no_close(&self) -> bool {
        self.intersects(WidgetOption::NO_CLOSE)
    }
    pub fn has_no_scroll(&self) -> bool {
        self.intersects(WidgetOption::NO_SCROLL)
    }
    pub fn is_fixed(&self) -> bool {
        self.intersects(WidgetOption::NO_RESIZE)
    }
    pub fn has_no_frame(&self) -> bool {
        self.intersects(WidgetOption::NO_FRAME)
    }
    pub fn is_not_interactive(&self) -> bool {
        self.intersects(WidgetOption::NO_INTERACT)
    }
    pub fn is_aligned_right(&self) -> bool {
        self.intersects(WidgetOption::ALIGN_RIGHT)
    }
    pub fn is_aligned_center(&self) -> bool {
        self.intersects(WidgetOption::ALIGN_CENTER)
    }
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }
}

bitflags! {
    #[derive(Copy, Clone)]
    pub struct MouseButton : u32 {
        const MIDDLE = 4;
        const RIGHT = 2;
        const LEFT = 1;
        const NONE = 0;
    }
}

impl MouseButton {
    pub fn is_middle(&self) -> bool {
        self.intersects(Self::MIDDLE)
    }
    pub fn is_right(&self) -> bool {
        self.intersects(Self::RIGHT)
    }
    pub fn is_left(&self) -> bool {
        self.intersects(Self::LEFT)
    }
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }
}

bitflags! {
    #[derive(Copy, Clone)]
    pub struct KeyMode : u32 {
        const RETURN = 16;
        const BACKSPACE = 8;
        const ALT = 4;
        const CTRL = 2;
        const SHIFT = 1;
        const NONE = 0;
    }
}

impl KeyMode {
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }
    pub fn is_return(&self) -> bool {
        self.intersects(Self::RETURN)
    }
    pub fn is_backspace(&self) -> bool {
        self.intersects(Self::BACKSPACE)
    }
    pub fn is_alt(&self) -> bool {
        self.intersects(Self::ALT)
    }
    pub fn is_ctrl(&self) -> bool {
        self.intersects(Self::CTRL)
    }
    pub fn is_shift(&self) -> bool {
        self.intersects(Self::SHIFT)
    }
}

#[derive(Clone)]
pub struct Input {
    mouse_pos: Vec2i,
    last_mouse_pos: Vec2i,
    mouse_delta: Vec2i,
    scroll_delta: Vec2i,
    mouse_down: MouseButton,
    mouse_pressed: MouseButton,
    key_down: KeyMode,
    key_pressed: KeyMode,
    input_text: String,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            mouse_pos: Vec2i::default(),
            last_mouse_pos: Vec2i::default(),
            mouse_delta: Vec2i::default(),
            scroll_delta: Vec2i::default(),
            mouse_down: MouseButton::NONE,
            mouse_pressed: MouseButton::NONE,
            key_down: KeyMode::NONE,
            key_pressed: KeyMode::NONE,
            input_text: String::default(),
        }
    }
}

impl Input {
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.mouse_pos = vec2(x, y);
    }

    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.mousemove(x, y);
        self.mouse_down |= btn;
        self.mouse_pressed |= btn;
    }

    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.mousemove(x, y);
        self.mouse_down &= !btn;
    }

    pub fn scroll(&mut self, x: i32, y: i32) {
        self.scroll_delta.x += x;
        self.scroll_delta.y += y;
    }

    pub fn keydown(&mut self, key: KeyMode) {
        self.key_pressed |= key;
        self.key_down |= key;
    }

    pub fn keyup(&mut self, key: KeyMode) {
        self.key_down &= !key;
    }

    pub fn text(&mut self, text: &str) {
        self.input_text += text;
    }

    fn prelude(&mut self) {
        self.mouse_delta.x = self.mouse_pos.x - self.last_mouse_pos.x;
        self.mouse_delta.y = self.mouse_pos.y - self.last_mouse_pos.y;
    }

    fn epilogue(&mut self) {
        self.key_pressed = KeyMode::NONE;
        self.input_text.clear();
        self.mouse_pressed = MouseButton::NONE;
        self.scroll_delta = vec2(0, 0);
        self.last_mouse_pos = self.mouse_pos;
    }
}

pub struct Context {
    atlas: Rc<dyn Atlas>,
    canvas: Box<dyn Canvas>,
    style: Style,

    last_zindex: i32,
    frame: usize,
    hover_root: Option<usize>,
    next_hover_root: Option<usize>,
    scroll_target: Option<usize>,

    root_list: Vec<usize>,
    container_stack: Vec<usize>,
    containers: Vec<Container>,
    pub idmngr: IdManager,
    pub input: Rc<RefCell<Input>>,
}

impl Context {
    pub fn new(atlas: Rc<dyn Atlas>, canvas: Box<dyn Canvas>) -> Self {
        Self {
            atlas,
            canvas,
            style: Style::default(),
            last_zindex: 0,
            frame: 0,
            hover_root: None,
            next_hover_root: None,
            scroll_target: None,

            root_list: Vec::default(),
            container_stack: Vec::default(),
            idmngr: IdManager::new(),
            containers: Vec::default(),
            input: Rc::new(RefCell::new(Input::default())),
        }
    }
}

#[derive(Default, Copy, Clone)]
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub trait Font {
    fn name(&self) -> &str;
    fn get_size(&self) -> usize;
    fn get_char_size(&self, c: char) -> (usize, usize);
}

#[derive(Copy, Clone)]
pub struct FontId(pub usize);

#[derive(Copy, Clone)]
pub struct Style {
    pub font: FontId,
    pub default_cell_size: Dimensioni,
    pub padding: i32,
    pub spacing: i32,
    pub indent: i32,
    pub title_height: i32,
    pub scrollbar_size: i32,
    pub thumb_size: i32,
    pub colors: [Color; 14],
}

pub type Real = f32;

static UNCLIPPED_RECT: Recti = Recti {
    x: 0,
    y: 0,
    width: i32::MAX,
    height: i32::MAX,
};

impl Default for Style {
    fn default() -> Self {
        Self {
            font: FontId(0),
            default_cell_size: Dimension { width: 68, height: 10 },
            padding: 5,
            spacing: 4,
            indent: 24,
            title_height: 24,
            scrollbar_size: 12,
            thumb_size: 8,
            colors: [
                Color { r: 230, g: 230, b: 230, a: 255 },
                Color { r: 25, g: 25, b: 25, a: 255 },
                Color { r: 50, g: 50, b: 50, a: 255 },
                Color { r: 25, g: 25, b: 25, a: 255 },
                Color { r: 240, g: 240, b: 240, a: 255 },
                Color { r: 0, g: 0, b: 0, a: 0 },
                Color { r: 75, g: 75, b: 75, a: 255 },
                Color { r: 95, g: 95, b: 95, a: 255 },
                Color { r: 115, g: 115, b: 115, a: 255 },
                Color { r: 30, g: 30, b: 30, a: 255 },
                Color { r: 35, g: 35, b: 35, a: 255 },
                Color { r: 40, g: 40, b: 40, a: 255 },
                Color { r: 43, g: 43, b: 43, a: 255 },
                Color { r: 30, g: 30, b: 30, a: 255 },
            ],
        }
    }
}

pub fn vec2(x: i32, y: i32) -> Vec2i {
    Vec2i { x, y }
}

pub fn rect(x: i32, y: i32, w: i32, h: i32) -> Recti {
    Recti { x, y, width: w, height: h }
}

pub fn color(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

pub fn expand_rect(r: Recti, n: i32) -> Recti {
    rect(r.x - n, r.y - n, r.width + n * 2, r.height + n * 2)
}

impl Context {
    pub fn clear(&mut self, width: i32, height: i32, clr: Color) {
        self.canvas.clear(width, height, clr);
    }

    pub fn flush(&mut self) {
        for rc in self.root_list.clone() {
            self.containers[rc].render(self.canvas.as_mut());
        }
        self.canvas.flush()
    }

    #[inline(never)]
    fn frame_begin(&mut self) {
        self.root_list.clear();
        self.scroll_target = None;
        self.hover_root = self.next_hover_root;
        self.next_hover_root = None;
        match self.hover_root {
            Some(id) => self.containers[id].in_hover_root = true,
            _ => (),
        }

        self.input.borrow_mut().prelude();
        for c in &mut self.containers {
            c.prepare();
        }
        self.frame += 1;
    }

    #[inline(never)]
    fn frame_end(&mut self) {
        assert_eq!(self.container_stack.len(), 0);
        assert_eq!(self.idmngr.len(), 0);

        for i in 0..self.containers.len() {
            self.containers[i].finish();
        }

        if !self.input.borrow().mouse_pressed.is_none()
            && !self.next_hover_root.is_none()
            && self.containers[self.next_hover_root.unwrap()].zindex < self.last_zindex
            && self.containers[self.next_hover_root.unwrap()].zindex >= 0
        {
            self.bring_to_front(self.next_hover_root.unwrap());
        }

        self.input.borrow_mut().epilogue();
        self.root_list.sort_by(|a, b| self.containers[*a].zindex.cmp(&self.containers[*b].zindex));
    }

    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.frame_begin();

        // execute the frame function
        f(self);

        self.frame_end();
    }

    fn pop_container(&mut self) {
        let layout = *self.top_container().layout.top();
        let container = self.top_container_mut();
        container.content_size.x = layout.max.x - layout.body.x;
        container.content_size.y = layout.max.y - layout.body.y;
        container.layout.stack.pop();

        self.container_stack.pop();
        self.idmngr.pop_id();
    }

    pub fn top_container(&self) -> &Container {
        &self.containers[*self.container_stack.last().unwrap()]
    }

    pub fn top_container_mut(&mut self) -> &mut Container {
        &mut self.containers[*self.container_stack.last().unwrap()]
    }

    #[inline(never)]
    fn get_container_index_intern(&mut self, id: Id, name: &str, opt: WidgetOption) -> Option<usize> {
        for idx in 0..self.containers.len() {
            if self.containers[idx].id == id {
                return Some(idx);
            }
        }
        if opt.is_closed() {
            return None;
        }

        let idx = self.containers.len();
        self.containers.push(Container {
            id,
            name: name.to_string(),
            open: true,
            style: self.style.clone(),
            atlas: self.atlas.clone(),
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Vec2i::default(),
            scroll: Vec2i::default(),
            zindex: 0,
            command_list: Vec::default(),
            clip_stack: Vec::default(),
            is_root: false,
            text_stack: Vec::default(),
            hover: None,
            focus: None,
            updated_focus: false,
            layout: LayoutManager::default(),
            idmngr: IdManager::new(),
            number_edit_buf: String::default(),
            number_edit: None,
            in_hover_root: false,
            input: self.input.clone(),
            panel_map: Default::default(),
            panels: Default::default(),
            active_panels: Default::default(),
        });
        self.bring_to_front(idx);
        Some(idx)
    }

    fn get_container_index(&mut self, name: &str) -> Option<usize> {
        let id = self.idmngr.get_id_from_str(name);
        self.get_container_index_intern(id, name, WidgetOption::NONE)
    }

    pub fn bring_to_front(&mut self, cnt: usize) {
        self.last_zindex += 1;
        self.containers[cnt].zindex = self.last_zindex;
    }

    fn in_hover_root(&mut self) -> bool {
        match self.hover_root {
            Some(hover_root) => {
                let len = self.container_stack.len();
                for i in 0..len {
                    if self.container_stack[len - i - 1] == hover_root {
                        return true;
                    }
                    if self.containers[self.container_stack[len - i - 1]].is_root {
                        // panel cannot scroll
                        break;
                    }
                }
                false
            }
            None => false,
        }
    }

    fn begin_root_container(&mut self, cnt: usize) {
        self.container_stack.push(cnt);
        self.root_list.push(cnt);
        self.containers[cnt].is_root = true;

        if self.containers[cnt].rect.contains(&self.input.borrow().mouse_pos)
            && (self.next_hover_root.is_none() || self.containers[cnt].zindex > self.containers[self.next_hover_root.unwrap()].zindex)
        {
            self.next_hover_root = Some(cnt);
        }
        let container = &mut self.containers[*self.container_stack.last().unwrap()];
        container.clip_stack.push(UNCLIPPED_RECT);
    }

    fn end_root_container(&mut self) {
        self.top_container_mut().is_root = false;
        self.top_container_mut().pop_clip_rect();
        self.pop_container();
    }

    #[inline(never)]
    #[must_use]
    fn begin_window(&mut self, title: &str, r: Recti, opt: WidgetOption) -> bool {
        let id = self.idmngr.get_id_from_str(title);
        let cnt_id = self.get_container_index_intern(id, title, opt);
        if cnt_id.is_none() || !self.containers[cnt_id.unwrap()].open {
            return false;
        }
        self.idmngr.push_id(id);

        if self.containers[cnt_id.unwrap()].rect.width == 0 {
            self.containers[cnt_id.unwrap()].rect = r;
        }
        self.begin_root_container(cnt_id.unwrap());
        self.containers[cnt_id.unwrap()].begin_window(title, opt);

        true
    }

    fn end_window(&mut self) {
        self.top_container_mut().end_window();
        self.end_root_container();
    }

    pub fn window<F: FnOnce(&mut Container)>(&mut self, title: &str, r: Recti, opt: WidgetOption, f: F) {
        // call the window function if the window is open
        if self.begin_window(title, r, opt) {
            f(self.top_container_mut());
            self.end_window();
        }
    }

    pub fn open_popup(&mut self, name: &str) {
        let cnt = self.get_container_index(name);
        self.next_hover_root = cnt;
        self.hover_root = self.next_hover_root;
        self.containers[cnt.unwrap()].rect = rect(self.input.borrow().mouse_pos.x, self.input.borrow().mouse_pos.y, 1, 1);
        self.containers[cnt.unwrap()].open = true;
        self.bring_to_front(cnt.unwrap());
    }

    pub fn popup<F: FnOnce(&mut Container)>(&mut self, name: &str, f: F) {
        let opt =
            WidgetOption::POPUP | WidgetOption::AUTO_SIZE | WidgetOption::NO_RESIZE | WidgetOption::NO_SCROLL | WidgetOption::NO_TITLE | WidgetOption::CLOSED;
        let _ = self.window(name, rect(0, 0, 0, 0), opt, f);
    }

    pub fn propagate_style(&mut self, style: &Style) {
        for c in &mut self.containers {
            c.propagate_style(style)
        }
    }
}
