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
use std::{f32, collections::HashMap, hash::Hash, rc::Rc};

use rs_math3d::*;

mod layout;
pub use layout::*;

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

#[derive(Clone)]
struct PoolItem<PO: Clone> {
    object: PO,
    frame: usize,
}

#[derive(Clone)]
pub struct Pool<ID, PO: Clone> {
    items: HashMap<ID, PoolItem<PO>>,
    gc_ids: Vec<ID>,
}

impl<ID: PartialEq + Eq + Hash + Clone, PO: Clone> Pool<ID, PO> {
    pub fn insert(&mut self, id: ID, object: PO, frame: usize) -> ID {
        match self.items.get_mut(&id) {
            Some(v) => v.frame = frame,
            None => {
                self.items.insert(id.clone(), PoolItem { object, frame });
            }
        }
        id
    }

    pub fn get(&self, id: ID) -> Option<&PO> {
        self.items.get(&id).map(|pi| &pi.object)
    }

    pub fn get_mut(&mut self, id: ID) -> Option<&mut PO> {
        self.items.get_mut(&id).map(|po| &mut po.object)
    }

    pub fn update(&mut self, id: ID, frame: usize) {
        self.items.get_mut(&id).unwrap().frame = frame
    }

    pub fn remove(&mut self, id: ID) {
        self.items.remove(&id);
    }

    pub fn gc(&mut self, current_frame: usize) {
        self.gc_ids.clear();
        for kv in &self.items {
            if kv.1.frame < current_frame - 2 {
                self.gc_ids.push(kv.0.clone());
            }
        }

        for gid in &self.gc_ids {
            self.items.remove(gid);
        }
    }
}

impl<ID, PO: Clone> Default for Pool<ID, PO> {
    fn default() -> Self {
        Self {
            items: HashMap::default(),
            gc_ids: Vec::default(),
        }
    }
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
        const EXPANDED = 4096;
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

impl WidgetOption {
    pub fn is_expanded(&self) -> bool {
        self.intersects(WidgetOption::EXPANDED)
    }
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
    hover: Option<Id>,
    focus: Option<Id>,
    pub last_id: Option<Id>,
    last_zindex: i32,
    updated_focus: bool,
    frame: usize,
    hover_root: Option<usize>,
    next_hover_root: Option<usize>,
    scroll_target: Option<usize>,
    number_edit_buf: String,
    number_edit: Option<Id>,
    root_list: Vec<usize>,
    container_stack: Vec<usize>,
    id_stack: Vec<Id>,
    containers: Vec<Container>,
    treenode_pool: Pool<Id, ()>,
    pub input: Input,
}

impl Context {
    pub fn new(atlas: Rc<dyn Atlas>, canvas: Box<dyn Canvas>) -> Self {
        Self {
            atlas,
            canvas,
            style: Style::default(),
            hover: None,
            focus: None,
            last_id: None,
            last_zindex: 0,
            updated_focus: false,
            frame: 0,
            hover_root: None,
            next_hover_root: None,
            scroll_target: None,
            number_edit_buf: String::default(),
            number_edit: None,
            root_list: Vec::default(),
            container_stack: Vec::default(),
            id_stack: Vec::default(),
            containers: Vec::default(),
            treenode_pool: Pool::default(),
            input: Input::default(),
        }
    }
}

#[derive(Default, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Id(u32);

#[derive(Clone)]
pub struct Container {
    id: Id,
    atlas: Rc<dyn Atlas>,
    pub style: Style,
    pub name: String,
    pub rect: Recti,
    pub body: Recti,
    pub content_size: Vec2i,
    pub scroll: Vec2i,
    pub zindex: i32,
    pub open: bool,
    pub command_list: Vec<Command>,
    pub clip_stack: Vec<Recti>,
    pub children: Vec<usize>,
    pub is_root: bool,
    pub text_stack: Vec<char>,
    layout: LayoutManager,
}

#[derive(Clone)]
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
        str_start: usize,
        str_len: usize,
    },
    Icon {
        rect: Recti,
        id: Icon,
        color: Color,
    },
    None,
}

impl Default for Command {
    fn default() -> Self {
        Command::None
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

fn hash_step(h: u32, n: u32) -> u32 {
    (h ^ n).wrapping_mul(16777619 as u32)
}

fn hash_u32(hash_0: &mut Id, orig_id: u32) {
    let bytes = orig_id.to_be_bytes();
    for b in bytes {
        *hash_0 = Id(hash_step(hash_0.0, b as u32));
    }
}

fn hash_str(hash_0: &mut Id, s: &str) {
    for c in s.chars() {
        *hash_0 = Id(hash_step(hash_0.0, c as u32));
    }
}

fn hash_bytes(hash_0: &mut Id, s: &[u8]) {
    for c in s {
        *hash_0 = Id(hash_step(hash_0.0, *c as u32));
    }
}

impl Container {
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

    pub fn push_text(&mut self, str: &str) -> usize {
        let str_start = self.text_stack.len();
        for c in str.chars() {
            self.text_stack.push(c);
        }
        return str_start;
    }

    pub fn set_clip(&mut self, rect: Recti) {
        self.push_command(Command::Clip { rect });
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

        let str_start = self.push_text(str);
        self.push_command(Command::Text {
            str_start,
            str_len: str.len(),
            pos,
            color,
            font,
        });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    pub fn draw_icon(&mut self, id: Icon, rect: Recti, color: Color) {
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

    fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
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
}

impl Context {
    pub fn clear(&mut self, width: i32, height: i32, clr: Color) {
        self.canvas.clear(width, height, clr);
    }

    fn render_container(&mut self, container_idx: usize) {
        let container = &self.containers[container_idx];
        for command in &container.command_list {
            match command {
                Command::Text { str_start, str_len, pos, color, .. } => {
                    let str = &container.text_stack[*str_start..*str_start + *str_len];
                    self.canvas.draw_chars(str, *pos, *color);
                }
                Command::Recti { rect, color } => {
                    self.canvas.draw_rect(*rect, *color);
                }
                Command::Icon { id, rect, color } => {
                    self.canvas.draw_icon(*id, *rect, *color);
                }
                Command::Clip { rect } => {
                    self.canvas.set_clip_rect(800, 600, *rect);
                }
                _ => {}
            }
        }
        for child in container.children.clone() {
            self.render_container(child);
        }
    }

    pub fn flush(&mut self) {
        for rc in self.root_list.clone() {
            self.render_container(rc);
        }
        self.canvas.flush()
    }

    #[inline(never)]
    fn frame_begin(&mut self) {
        self.root_list.clear();
        self.scroll_target = None;
        self.hover_root = self.next_hover_root;
        self.next_hover_root = None;
        self.input.prelude();
        for c in &mut self.containers {
            c.command_list.clear();
            c.children.clear();
            assert!(c.clip_stack.len() == 0);
            c.text_stack.clear();
            c.style = self.style.clone();
        }
        self.frame += 1;
    }

    #[inline(never)]
    fn frame_end(&mut self) {
        assert_eq!(self.container_stack.len(), 0);
        assert_eq!(self.id_stack.len(), 0);
        // assert_eq!(self.layout_stack.len(), 0);
        if !self.scroll_target.is_none() {
            self.containers[self.scroll_target.unwrap()].scroll.x += self.input.scroll_delta.x;
            self.containers[self.scroll_target.unwrap()].scroll.y += self.input.scroll_delta.y;
        }
        if !self.updated_focus {
            self.focus = None;
        }
        self.updated_focus = false;
        if !self.input.mouse_pressed.is_none()
            && !self.next_hover_root.is_none()
            && self.containers[self.next_hover_root.unwrap()].zindex < self.last_zindex
            && self.containers[self.next_hover_root.unwrap()].zindex >= 0
        {
            self.bring_to_front(self.next_hover_root.unwrap());
        }

        self.input.epilogue();
        self.root_list.sort_by(|a, b| self.containers[*a].zindex.cmp(&self.containers[*b].zindex));

        self.treenode_pool.gc(self.frame);
    }

    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.frame_begin();

        // execute the frame function
        f(self);

        self.frame_end();
    }

    pub fn set_focus(&mut self, id: Option<Id>) {
        self.focus = id;
        self.updated_focus = true;
    }

    pub fn get_id_u32(&mut self, orig_id: u32) -> Id {
        let mut res: Id = match self.id_stack.last() {
            Some(id) => *id,
            None => Id(2166136261),
        };
        hash_u32(&mut res, orig_id);
        self.last_id = Some(res);
        return res;
    }

    pub fn get_id_from_ptr<T: ?Sized>(&mut self, orig_id: &T) -> Id {
        let mut res: Id = match self.id_stack.last() {
            Some(id) => *id,
            None => Id(2166136261),
        };
        let ptr = orig_id as *const T as *const u8 as usize;
        let bytes = ptr.to_le_bytes();
        hash_bytes(&mut res, &bytes);
        self.last_id = Some(res);
        return res;
    }

    pub fn get_id_from_str(&mut self, s: &str) -> Id {
        let mut res: Id = match self.id_stack.last() {
            Some(id) => *id,
            None => Id(2166136261),
        };
        hash_str(&mut res, s);
        self.last_id = Some(res);
        return res;
    }

    pub fn push_id_from_ptr<T>(&mut self, orig_id: &T) {
        let id = self.get_id_from_ptr(orig_id);
        self.id_stack.push(id);
    }

    pub fn push_id_from_str(&mut self, s: &str) {
        let id = self.get_id_from_str(s);
        self.id_stack.push(id);
    }

    pub fn pop_id(&mut self) {
        self.id_stack.pop();
    }

    fn pop_container(&mut self) {
        let layout = *self.top_container().layout.top();
        self.top_container_mut().content_size.x = layout.max.x - layout.body.x;
        self.top_container_mut().content_size.y = layout.max.y - layout.body.y;
        self.top_container_mut().layout.stack.pop();

        self.container_stack.pop();
        self.pop_id();
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
            children: Vec::default(),
            is_root: false,
            text_stack: Vec::default(),

            layout: LayoutManager::default(),
        });
        self.bring_to_front(idx);
        Some(idx)
    }

    fn get_container_index(&mut self, name: &str) -> Option<usize> {
        let id = self.get_id_from_str(name);
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

    pub fn draw_control_frame(&mut self, id: Id, rect: Recti, mut colorid: ControlColor, opt: WidgetOption) {
        if opt.has_no_frame() {
            return;
        }

        if self.focus == Some(id) {
            colorid.focus()
        } else if self.hover == Some(id) {
            colorid.hover()
        }
        self.top_container_mut().draw_frame(rect, colorid);
    }

    #[inline(never)]
    pub fn draw_control_text(&mut self, str: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let mut pos: Vec2i = Vec2i { x: 0, y: 0 };
        let font = self.style.font;
        let tsize = self.atlas.get_text_size(font, str);
        self.top_container_mut().push_clip_rect(rect);
        pos.y = rect.y + (rect.height - tsize.height) / 2;
        if opt.is_aligned_center() {
            pos.x = rect.x + (rect.width - tsize.width) / 2;
        } else if opt.is_aligned_right() {
            pos.x = rect.x + rect.width - tsize.width - self.style.padding;
        } else {
            pos.x = rect.x + self.style.padding;
        }
        let color = self.style.colors[colorid as usize];
        self.top_container_mut().draw_text(font, str, pos, color);
        self.top_container_mut().pop_clip_rect();
    }

    pub fn mouse_over(&mut self, rect: Recti) -> bool {
        let clip_rect = self.top_container_mut().get_clip_rect();
        rect.contains(&self.input.mouse_pos) && clip_rect.contains(&self.input.mouse_pos) && self.in_hover_root()
    }

    #[inline(never)]
    pub fn update_control(&mut self, id: Id, rect: Recti, opt: WidgetOption) {
        let mouseover = self.mouse_over(rect);
        if self.focus == Some(id) {
            self.updated_focus = true;
        }
        if opt.is_not_interactive() {
            return;
        }
        if mouseover && self.input.mouse_down.is_none() {
            self.hover = Some(id);
        }
        if self.focus == Some(id) {
            if !self.input.mouse_pressed.is_none() && !mouseover {
                self.set_focus(None);
            }
            if self.input.mouse_down.is_none() && !opt.is_holding_focus() {
                self.set_focus(None);
            }
        }
        if self.hover == Some(id) {
            if !self.input.mouse_pressed.is_none() {
                self.set_focus(Some(id));
            } else if !mouseover {
                self.hover = None;
            }
        }
    }

    pub fn label(&mut self, text: &str) {
        let layout = self.top_container_mut().layout.next();
        self.draw_control_text(text, layout, ControlColor::Text, WidgetOption::NONE);
    }

    #[inline(never)]
    pub fn button_ex(&mut self, label: &str, icon: Icon, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = if label.len() > 0 {
            self.get_id_from_str(label)
        } else {
            self.get_id_u32(icon as u32)
        };
        let r: Recti = self.top_container_mut().layout.next();
        self.update_control(id, r, opt);
        if self.input.mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }
        self.draw_control_frame(id, r, ControlColor::Button, opt);
        if label.len() > 0 {
            self.draw_control_text(label, r, ControlColor::Text, opt);
        }
        if icon != Icon::None {
            let color = self.style.colors[ControlColor::Text as usize];
            self.top_container_mut().draw_icon(icon, r, color);
        }
        return res;
    }

    #[inline(never)]
    pub fn checkbox(&mut self, label: &str, state: &mut bool) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = self.get_id_from_ptr(state);
        let mut r: Recti = self.top_container_mut().layout.next();
        let box_0: Recti = rect(r.x, r.y, r.height, r.height);
        self.update_control(id, r, WidgetOption::NONE);
        if self.input.mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::CHANGE;
            *state = *state == false;
        }
        self.draw_control_frame(id, box_0, ControlColor::Base, WidgetOption::NONE);
        if *state {
            let color = self.style.colors[ControlColor::Text as usize];
            self.top_container_mut().draw_icon(Icon::Check, box_0, color);
        }
        r = rect(r.x + box_0.width, r.y, r.width - box_0.width, r.height);
        self.draw_control_text(label, r, ControlColor::Text, WidgetOption::NONE);
        return res;
    }

    pub fn textbox_raw(&mut self, buf: &mut String, id: Id, r: Recti, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        self.update_control(id, r, opt | WidgetOption::HOLD_FOCUS);
        if self.focus == Some(id) {
            let mut len = buf.len();

            if self.input.input_text.len() > 0 {
                buf.push_str(self.input.input_text.as_str());
                len += self.input.input_text.len() as usize;
                res |= ResourceState::CHANGE
            }

            if self.input.key_pressed.is_backspace() && len > 0 {
                // skip utf-8 continuation bytes
                buf.pop();
                res |= ResourceState::CHANGE
            }
            if self.input.key_pressed.is_return() {
                self.set_focus(None);
                res |= ResourceState::SUBMIT;
            }
        }
        self.draw_control_frame(id, r, ControlColor::Base, opt);
        if self.focus == Some(id) {
            let color = self.style.colors[ControlColor::Text as usize];
            let font = self.style.font;
            let tsize = self.atlas.get_text_size(font, buf.as_str());
            let ofx = r.width - self.style.padding - tsize.width - 1;
            let textx = r.x + (if ofx < self.style.padding { ofx } else { self.style.padding });
            let texty = r.y + (r.height - tsize.height) / 2;
            self.top_container_mut().push_clip_rect(r);
            self.top_container_mut().draw_text(font, buf.as_str(), vec2(textx, texty), color);
            self.top_container_mut().draw_rect(rect(textx + tsize.width, texty, 1, tsize.height), color);
            self.top_container_mut().pop_clip_rect();
        } else {
            self.draw_control_text(buf.as_str(), r, ControlColor::Text, opt);
        }
        return res;
    }

    #[inline(never)]
    fn number_textbox(&mut self, precision: usize, value: &mut Real, r: Recti, id: Id) -> ResourceState {
        if self.input.mouse_pressed.is_left() && self.input.key_down.is_shift() && self.hover == Some(id) {
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
        let id: Id = self.get_id_from_ptr(buf);
        let r: Recti = self.top_container_mut().layout.next();
        return self.textbox_raw(buf, id, r, opt);
    }

    #[inline(never)]
    pub fn slider_ex(&mut self, value: &mut Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let last = *value;
        let mut v = last;
        let id = self.get_id_from_ptr(value);
        let base = self.top_container_mut().layout.next();
        if !self.number_textbox(precision, &mut v, base, id).is_none() {
            return res;
        }
        self.update_control(id, base, opt);
        if self.focus == Some(id) && (!self.input.mouse_down.is_none() | self.input.mouse_pressed.is_left()) {
            v = low + (self.input.mouse_pos.x - base.x) as Real * (high - low) / base.width as Real;
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
        self.draw_control_frame(id, base, ControlColor::Base, opt);
        let w = self.style.thumb_size;
        let x = ((v - low) * (base.width - w) as Real / (high - low)) as i32;
        let thumb = rect(base.x + x, base.y, w, base.height);
        self.draw_control_frame(id, thumb, ControlColor::Button, opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", precision, value).as_str());
        self.draw_control_text(buff.as_str(), base, ControlColor::Text, opt);
        return res;
    }

    pub fn number_ex(&mut self, value: &mut Real, step: Real, precision: usize, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = self.get_id_from_ptr(value);
        let base: Recti = self.top_container_mut().layout.next();
        let last: Real = *value;
        if !self.number_textbox(precision, value, base, id).is_none() {
            return res;
        }
        self.update_control(id, base, opt);
        if self.focus == Some(id) && self.input.mouse_down.is_left() {
            *value += self.input.mouse_delta.x as Real * step;
        }
        if *value != last {
            res |= ResourceState::CHANGE;
        }
        self.draw_control_frame(id, base, ControlColor::Base, opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", precision, value).as_str());
        self.draw_control_text(buff.as_str(), base, ControlColor::Text, opt);
        return res;
    }

    #[inline(never)]
    fn node(&mut self, label: &str, is_treenode: bool, opt: WidgetOption) -> ResourceState {
        let id: Id = self.get_id_from_str(label);
        self.top_container_mut().layout.row(&[-1], 0);
        let mut r = self.top_container_mut().layout.next();
        self.update_control(id, r, WidgetOption::NONE);
        let state = self.treenode_pool.get(id);
        let mut active = state.is_some();
        // clever substitution for if opt.is_expanded() { !active } else { active };
        let expanded = opt.is_expanded() ^ active;
        active ^= self.input.mouse_pressed.is_left() && self.focus == Some(id);
        if state.is_some() {
            if active {
                self.treenode_pool.update(id, self.frame);
            } else {
                self.treenode_pool.remove(id);
            }
        } else if active {
            self.treenode_pool.insert(id, (), self.frame);
        }

        if is_treenode {
            if self.hover == Some(id) {
                self.top_container_mut().draw_frame(r, ControlColor::ButtonHover);
            }
        } else {
            self.draw_control_frame(id, r, ControlColor::Button, WidgetOption::NONE);
        }
        let color = self.style.colors[ControlColor::Text as usize];
        self.top_container_mut().draw_icon(
            if expanded { Icon::Expanded } else { Icon::Collapsed },
            rect(r.x, r.y, r.height, r.height),
            color,
        );
        r.x += r.height - self.style.padding;
        r.width -= r.height - self.style.padding;
        self.draw_control_text(label, r, ControlColor::Text, WidgetOption::NONE);
        return if expanded { ResourceState::ACTIVE } else { ResourceState::NONE };
    }

    pub fn header<F: FnOnce(&mut Self)>(&mut self, label: &str, opt: WidgetOption, f: F) {
        if !self.node(label, false, opt).is_none() {
            f(self);
        }
    }

    pub fn treenode<F: FnOnce(&mut Self)>(&mut self, label: &str, opt: WidgetOption, f: F) {
        let res = self.node(label, true, opt);
        if res.is_active() && self.last_id.is_some() {
            let indent = self.style.indent;
            self.top_container_mut().layout.top_mut().indent += indent;
            self.id_stack.push(self.last_id.unwrap());
        }

        if !res.is_none() {
            f(self);
            let indent = self.style.indent;
            self.top_container_mut().layout.top_mut().indent -= indent;
            self.pop_id();
        }
    }

    fn clamp(x: i32, a: i32, b: i32) -> i32 {
        min(b, max(a, x))
    }

    #[inline(never)]
    fn scrollbars(&mut self, cnt_id: usize, body: &mut Recti) {
        let sz = self.style.scrollbar_size;
        let mut cs: Vec2i = self.containers[cnt_id].content_size;
        cs.x += self.style.padding * 2;
        cs.y += self.style.padding * 2;
        self.top_container_mut().push_clip_rect(body.clone());
        if cs.y > self.containers[cnt_id].body.height {
            body.width -= sz;
        }
        if cs.x > self.containers[cnt_id].body.width {
            body.height -= sz;
        }
        let body = *body;
        let maxscroll = cs.y - body.height;
        if maxscroll > 0 && body.height > 0 {
            let id: Id = self.get_id_from_str("!scrollbary");
            let mut base = body;
            base.x = body.x + body.width;
            base.width = self.style.scrollbar_size;
            self.update_control(id, base, WidgetOption::NONE);
            if self.focus == Some(id) && self.input.mouse_down.is_left() {
                self.containers[cnt_id].scroll.y += self.input.mouse_delta.y * cs.y / base.height;
            }
            self.containers[cnt_id].scroll.y = Self::clamp(self.containers[cnt_id].scroll.y, 0, maxscroll);

            self.top_container_mut().draw_frame(base, ControlColor::ScrollBase);
            let mut thumb = base;
            thumb.height = if self.style.thumb_size > base.height * body.height / cs.y {
                self.style.thumb_size
            } else {
                base.height * body.height / cs.y
            };
            thumb.y += self.containers[cnt_id].scroll.y * (base.height - thumb.height) / maxscroll;
            self.top_container_mut().draw_frame(thumb, ControlColor::ScrollThumb);
            if self.mouse_over(body) {
                self.scroll_target = Some(cnt_id);
            }
        } else {
            self.containers[cnt_id].scroll.y = 0;
        }
        let maxscroll_0 = cs.x - body.width;
        if maxscroll_0 > 0 && body.width > 0 {
            let id_0: Id = self.get_id_from_str("!scrollbarx");
            let mut base_0 = body;
            base_0.y = body.y + body.height;
            base_0.height = self.style.scrollbar_size;
            self.update_control(id_0, base_0, WidgetOption::NONE);
            if self.focus == Some(id_0) && self.input.mouse_down.is_left() {
                self.containers[cnt_id].scroll.x += self.input.mouse_delta.x * cs.x / base_0.width;
            }
            self.containers[cnt_id].scroll.x = Self::clamp(self.containers[cnt_id].scroll.x, 0, maxscroll_0);

            self.top_container_mut().draw_frame(base_0, ControlColor::ScrollBase);
            let mut thumb_0 = base_0;
            thumb_0.width = if self.style.thumb_size > base_0.width * body.width / cs.x {
                self.style.thumb_size
            } else {
                base_0.width * body.width / cs.x
            };
            thumb_0.x += self.containers[cnt_id].scroll.x * (base_0.width - thumb_0.width) / maxscroll_0;
            self.top_container_mut().draw_frame(thumb_0, ControlColor::ScrollThumb);
            if self.mouse_over(body) {
                self.scroll_target = Some(cnt_id);
            }
        } else {
            self.containers[cnt_id].scroll.x = 0;
        }
        self.top_container_mut().pop_clip_rect();
    }

    fn push_container_body(&mut self, cnt_idx: usize, body: Recti, opt: WidgetOption) {
        let mut body = body;
        if !opt.has_no_scroll() {
            self.scrollbars(cnt_idx, &mut body);
        }
        let style = self.style;
        let padding = -style.padding;
        let scroll = self.containers[cnt_idx].scroll;
        self.containers[cnt_idx].layout.push_layout(expand_rect(body, padding), scroll);
        self.containers[cnt_idx].layout.style = self.style.clone();
        self.containers[cnt_idx].body = body;
    }

    fn begin_root_container(&mut self, cnt: usize) {
        self.container_stack.push(cnt);
        self.root_list.push(cnt);
        self.containers[cnt].is_root = true;

        if self.containers[cnt].rect.contains(&self.input.mouse_pos)
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
    fn begin_window(&mut self, title: &str, mut r: Recti, opt: WidgetOption) {
        let id = self.get_id_from_str(title);
        let cnt_id = self.get_container_index_intern(id, title, opt);
        if cnt_id.is_none() || !self.containers[cnt_id.unwrap()].open {
            return;
        }
        self.id_stack.push(id);

        if self.containers[cnt_id.unwrap()].rect.width == 0 {
            self.containers[cnt_id.unwrap()].rect = r;
        }
        self.begin_root_container(cnt_id.unwrap());
        let mut body = self.containers[cnt_id.unwrap()].rect;
        r = body;
        if !opt.has_no_frame() {
            self.top_container_mut().draw_frame(r, ControlColor::WindowBG);
        }
        if !opt.has_no_title() {
            let mut tr: Recti = r;
            tr.height = self.style.title_height;
            self.top_container_mut().draw_frame(tr, ControlColor::TitleBG);

            // TODO: Is this necessary?
            if !opt.has_no_title() {
                let id = self.get_id_from_str("!title");
                self.update_control(id, tr, opt);
                self.draw_control_text(title, tr, ControlColor::TitleText, opt);
                if Some(id) == self.focus && self.input.mouse_down.is_left() {
                    self.containers[cnt_id.unwrap()].rect.x += self.input.mouse_delta.x;
                    self.containers[cnt_id.unwrap()].rect.y += self.input.mouse_delta.y;
                }
                body.y += tr.height;
                body.height -= tr.height;
            }
            if !opt.has_no_close() {
                let id = self.get_id_from_str("!close");
                let r: Recti = rect(tr.x + tr.width - tr.height, tr.y, tr.height, tr.height);
                tr.width -= r.width;
                let color = self.style.colors[ControlColor::TitleText as usize];
                self.top_container_mut().draw_icon(Icon::Close, r, color);
                self.update_control(id, r, opt);
                if self.input.mouse_pressed.is_left() && Some(id) == self.focus {
                    self.containers[cnt_id.unwrap()].open = false;
                }
            }
        }
        self.push_container_body(cnt_id.unwrap(), body, opt);
        if !opt.is_auto_sizing() {
            let sz = self.style.title_height;
            let id_2 = self.get_id_from_str("!resize");
            let r_0 = rect(r.x + r.width - sz, r.y + r.height - sz, sz, sz);
            self.update_control(id_2, r_0, opt);
            if Some(id_2) == self.focus && self.input.mouse_down.is_left() {
                self.containers[cnt_id.unwrap()].rect.width = if 96 > self.containers[cnt_id.unwrap()].rect.width + self.input.mouse_delta.x {
                    96
                } else {
                    self.containers[cnt_id.unwrap()].rect.width + self.input.mouse_delta.x
                };
                self.containers[cnt_id.unwrap()].rect.height = if 64 > self.containers[cnt_id.unwrap()].rect.height + self.input.mouse_delta.y {
                    64
                } else {
                    self.containers[cnt_id.unwrap()].rect.height + self.input.mouse_delta.y
                };
            }
        }
        if opt.is_auto_sizing() {
            let r_1 = self.top_container_mut().layout.top().body;
            self.containers[cnt_id.unwrap()].rect.width =
                self.containers[cnt_id.unwrap()].content_size.x + (self.containers[cnt_id.unwrap()].rect.width - r_1.width);
            self.containers[cnt_id.unwrap()].rect.height =
                self.containers[cnt_id.unwrap()].content_size.y + (self.containers[cnt_id.unwrap()].rect.height - r_1.height);
        }

        if opt.is_popup() && !self.input.mouse_pressed.is_none() && self.hover_root != cnt_id {
            self.containers[cnt_id.unwrap()].open = false;
        }
        let body = self.top_container().body;
        self.top_container_mut().push_clip_rect(body);
    }

    fn end_window(&mut self) {
        self.top_container_mut().pop_clip_rect();
        self.end_root_container();
    }

    pub fn window<F: FnOnce(&mut Self)>(&mut self, title: &str, r: Recti, opt: WidgetOption, f: F) {
        self.begin_window(title, r, opt);
        // call the window function
        f(self);
        self.end_window();
    }

    pub fn open_popup(&mut self, name: &str) {
        let cnt = self.get_container_index(name);
        self.next_hover_root = cnt;
        self.hover_root = self.next_hover_root;
        self.containers[cnt.unwrap()].rect = rect(self.input.mouse_pos.x, self.input.mouse_pos.y, 1, 1);
        self.containers[cnt.unwrap()].open = true;
        self.bring_to_front(cnt.unwrap());
    }

    pub fn popup<F: FnOnce(&mut Self)>(&mut self, name: &str, f: F) {
        let opt =
            WidgetOption::POPUP | WidgetOption::AUTO_SIZE | WidgetOption::NO_RESIZE | WidgetOption::NO_SCROLL | WidgetOption::NO_TITLE | WidgetOption::CLOSED;
        self.window(name, rect(0, 0, 0, 0), opt, f);
    }

    #[inline(never)]
    fn begin_panel(&mut self, name: &str, opt: WidgetOption) {
        self.push_id_from_str(name);

        // A panel can only exist inside a root container
        assert!(self.root_list.len() != 0);

        let cnt_id = self.get_container_index_intern(self.last_id.unwrap(), name, opt);
        self.containers[*self.root_list.last().unwrap()].children.push(cnt_id.unwrap());
        let rect = self.top_container_mut().layout.next();
        let clip_rect = self.containers[cnt_id.unwrap()].body;
        self.containers[cnt_id.unwrap()].rect = rect;
        if !opt.has_no_frame() {
            self.top_container_mut().draw_frame(rect, ControlColor::PanelBG);
        }

        self.container_stack.push(cnt_id.unwrap());
        self.push_container_body(cnt_id.unwrap(), rect, opt);

        self.top_container_mut().push_clip_rect(clip_rect);
    }

    fn end_panel(&mut self) {
        self.top_container_mut().pop_clip_rect();
        self.pop_container();
    }
    pub fn panel<F: FnOnce(&mut Self)>(&mut self, name: &str, opt: WidgetOption, f: F) {
        self.begin_panel(name, opt);

        // call the panel function
        f(self);

        self.end_panel();
    }

    pub fn column<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.top_container_mut().layout.begin_column();
        f(self);
        self.top_container_mut().layout.end_column();
    }

    pub fn set_row_widths_height(&mut self, widths: &[i32], height: i32) {
        self.top_container_mut().layout.row(widths, height);
    }

    pub fn next_cell(&mut self) -> Recti {
        self.top_container_mut().layout.next()
    }

    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    pub fn get_style(&self) -> Style {
        self.style.clone()
    }
}
