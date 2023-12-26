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
extern crate std;
use std::{f32, collections::HashMap, hash::Hash};

mod atlas;
pub use atlas::*;

use bitflags::*;

pub trait AtlasRenderer {
    fn draw_rect(&mut self, rect: Rect, color: Color);
    fn draw_chars(&mut self, text: &[char], pos: Vec2i, color: Color);
    fn draw_icon(&mut self, id: Icon, r: Rect, color: Color);
    fn set_clip_rect(&mut self, width: i32, height: i32, rect: Rect);
    fn clear(&mut self, width: i32, height: i32, clr: Color);
    fn flush(&mut self);
    fn get_char_width(&self, font: FontId, c: char) -> usize;
    fn get_font_height(&self, font: FontId) -> usize;
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

#[repr(C)]
pub struct Context {
    atlas_renderer: Box<dyn AtlasRenderer>,
    pub style: Style,
    pub hover: Option<Id>,
    pub focus: Option<Id>,
    pub last_id: Option<Id>,
    pub last_zindex: i32,
    pub updated_focus: bool,
    pub frame: usize,
    pub hover_root: Option<usize>,
    pub next_hover_root: Option<usize>,
    pub scroll_target: Option<usize>,
    pub number_edit_buf: String,
    pub number_edit: Option<Id>,
    pub root_list: Vec<usize>,
    pub container_stack: Vec<usize>,
    pub id_stack: Vec<Id>,
    pub text_stack: Vec<char>,
    pub containers: Vec<Container>,
    pub treenode_pool: Pool<Id, ()>,
    pub mouse_pos: Vec2i,
    pub last_mouse_pos: Vec2i,
    pub mouse_delta: Vec2i,
    pub scroll_delta: Vec2i,
    pub mouse_down: MouseButton,
    pub mouse_pressed: MouseButton,
    pub key_down: KeyMode,
    pub key_pressed: KeyMode,
    pub input_text: String,
}

impl Context {
    pub fn new(atlas_renderer: Box<dyn AtlasRenderer>) -> Self {
        Self {
            atlas_renderer,
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
            text_stack: Vec::default(),
            containers: Vec::default(),
            treenode_pool: Pool::default(),
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

#[derive(Default, Copy, Clone)]
pub struct Vec2i {
    pub x: i32,
    pub y: i32,
}

#[derive(Default, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Id(u32);

#[derive(Default, Clone)]
pub struct Container {
    id: Id,
    pub style: Style,
    pub name: String,
    pub rect: Rect,
    pub body: Rect,
    pub content_size: Vec2i,
    pub scroll: Vec2i,
    pub zindex: i32,
    pub open: bool,
    pub command_list: Vec<Command>,
    pub clip_stack: Vec<Rect>,
    pub children: Vec<usize>,

    pub last_rect: Rect,
    pub layout_stack: Vec<Layout>,
}

impl Container {}
#[derive(Default, Copy, Clone)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Default, Copy, Clone)]
pub struct Layout {
    pub body: Rect,
    pub next: Rect,
    pub position: Vec2i,
    pub size: Vec2i,
    pub max: Vec2i,
    pub widths: [i32; 16],
    pub items: usize,
    pub item_index: usize,
    pub next_row: i32,
    pub next_type: LayoutPosition,
    pub indent: i32,
}

#[derive(Clone)]
pub enum Command {
    Clip {
        rect: Rect,
    },
    Rect {
        rect: Rect,
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
        rect: Rect,
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
    pub size: Vec2i,
    pub padding: i32,
    pub spacing: i32,
    pub indent: i32,
    pub title_height: i32,
    pub scrollbar_size: i32,
    pub thumb_size: i32,
    pub colors: [Color; 14],
}

pub type Real = f32;

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
pub enum LayoutPosition {
    Absolute = 2,
    Relative = 1,
    None = 0,
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::None
    }
}

static UNCLIPPED_RECT: Rect = Rect { x: 0, y: 0, w: i32::MAX, h: i32::MAX };

impl Default for Style {
    fn default() -> Self {
        Self {
            font: FontId(0),
            size: Vec2i { x: 68, y: 10 },
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

pub fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

pub fn color(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

pub fn expand_rect(r: Rect, n: i32) -> Rect {
    rect(r.x - n, r.y - n, r.w + n * 2, r.h + n * 2)
}

pub fn intersect_rects(r1: Rect, r2: Rect) -> Rect {
    let x1 = i32::max(r1.x, r2.x);
    let y1 = i32::max(r1.y, r2.y);
    let mut x2 = i32::min(r1.x + r1.w, r2.x + r2.w);
    let mut y2 = i32::min(r1.y + r1.h, r2.y + r2.h);
    if x2 < x1 {
        x2 = x1;
    }
    if y2 < y1 {
        y2 = y1;
    }
    return rect(x1, y1, x2 - x1, y2 - y1);
}

pub fn rect_overlaps_vec2(r: Rect, p: Vec2i) -> bool {
    p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
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
    fn push_layout(&mut self, body: Rect, scroll: Vec2i) {
        let mut layout: Layout = Layout {
            body: Rect { x: 0, y: 0, w: 0, h: 0 },
            next: Rect { x: 0, y: 0, w: 0, h: 0 },
            position: Vec2i { x: 0, y: 0 },
            size: Vec2i { x: 0, y: 0 },
            max: Vec2i { x: 0, y: 0 },
            widths: [0; 16],
            items: 0,
            item_index: 0,
            next_row: 0,
            next_type: LayoutPosition::None,
            indent: 0,
        };
        layout.body = rect(body.x - scroll.x, body.y - scroll.y, body.w, body.h);
        layout.max = vec2(-i32::MAX, -i32::MAX);
        self.layout_stack.push(layout);
        self.layout_row(&[0], 0);
    }

    fn get_layout(&self) -> &Layout {
        return self.layout_stack.last().unwrap();
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        return self.layout_stack.last_mut().unwrap();
    }

    pub fn layout_begin_column(&mut self) {
        let layout = self.layout_next();
        self.push_layout(layout, vec2(0, 0));
    }

    pub fn layout_end_column(&mut self) {
        let b = self.get_layout().clone();
        self.layout_stack.pop();

        let a = self.get_layout_mut();
        a.position.x = if a.position.x > b.position.x + b.body.x - a.body.x {
            a.position.x
        } else {
            b.position.x + b.body.x - a.body.x
        };
        a.next_row = if a.next_row > b.next_row + b.body.y - a.body.y {
            a.next_row
        } else {
            b.next_row + b.body.y - a.body.y
        };
        a.max.x = i32::max(a.max.x, b.max.x);
        a.max.y = i32::max(a.max.y, b.max.y);
    }

    pub fn layout_row_for_layout(layout: &mut Layout, widths: &[i32], height: i32) {
        layout.items = widths.len();
        assert!(widths.len() <= 16);
        for i in 0..widths.len() {
            layout.widths[i] = widths[i];
        }
        layout.position = vec2(layout.indent, layout.next_row);
        layout.size.y = height;
        layout.item_index = 0;
    }

    pub fn layout_row(&mut self, widths: &[i32], height: i32) {
        let layout = self.get_layout_mut();
        Self::layout_row_for_layout(layout, widths, height);
    }

    pub fn layout_width(&mut self, width: i32) {
        self.get_layout_mut().size.x = width;
    }

    pub fn layout_height(&mut self, height: i32) {
        self.get_layout_mut().size.y = height;
    }

    pub fn layout_set_next(&mut self, r: Rect, position: LayoutPosition) {
        let layout = self.get_layout_mut();
        layout.next = r;
        layout.next_type = position;
    }

    pub fn layout_next(&mut self) -> Rect {
        let style_size = self.style.size;
        let style_padding = self.style.padding;
        let style_spacing = self.style.spacing;

        let layout = self.get_layout_mut();
        let mut res: Rect = Rect { x: 0, y: 0, w: 0, h: 0 };
        if layout.next_type != LayoutPosition::None {
            let type_0 = layout.next_type;
            layout.next_type = LayoutPosition::None;
            res = layout.next;
            if type_0 == LayoutPosition::Absolute {
                self.last_rect = res;
                return self.last_rect;
            }
        } else {
            let litems = layout.items;
            let lsize_y = layout.size.y;
            let mut undefined_widths = [0; 16];
            undefined_widths[0..litems as usize].copy_from_slice(&layout.widths[0..litems as usize]);
            if layout.item_index == layout.items {
                Self::layout_row_for_layout(layout, &undefined_widths[0..litems as usize], lsize_y);
            }
            res.x = layout.position.x;
            res.y = layout.position.y;
            res.w = if layout.items > 0 {
                layout.widths[layout.item_index as usize]
            } else {
                layout.size.x
            };
            res.h = layout.size.y;

            if res.w == 0 {
                res.w = style_size.x + style_padding * 2;
            }
            if res.h == 0 {
                res.h = style_size.y + style_padding * 2;
            }
            if res.w < 0 {
                res.w += layout.body.w - res.x + 1;
            }
            if res.h < 0 {
                res.h += layout.body.h - res.y + 1;
            }
            layout.item_index += 1;
        }
        layout.position.x += res.w + style_spacing;
        layout.next_row = if layout.next_row > res.y + res.h + style_spacing {
            layout.next_row
        } else {
            res.y + res.h + style_spacing
        };
        res.x += layout.body.x;
        res.y += layout.body.y;
        layout.max.x = if layout.max.x > res.x + res.w { layout.max.x } else { res.x + res.w };
        layout.max.y = if layout.max.y > res.y + res.h { layout.max.y } else { res.y + res.h };
        self.last_rect = res;
        return self.last_rect;
    }
}

impl Context {
    pub fn clear(&mut self, width: i32, height: i32, clr: Color) {
        self.atlas_renderer.clear(width, height, clr);
    }

    fn render_container(&mut self, container_idx: usize) {
        let container = &self.containers[container_idx];
        for command in &container.command_list {
            match command {
                Command::Text { str_start, str_len, pos, color, .. } => {
                    let str = &self.text_stack[*str_start..*str_start + *str_len];
                    self.atlas_renderer.draw_chars(str, *pos, *color);
                }
                Command::Rect { rect, color } => {
                    self.atlas_renderer.draw_rect(*rect, *color);
                }
                Command::Icon { id, rect, color } => {
                    self.atlas_renderer.draw_icon(*id, *rect, *color);
                }
                Command::Clip { rect } => {
                    self.atlas_renderer.set_clip_rect(800, 600, *rect);
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
        self.atlas_renderer.flush()
    }

    fn draw_frame(&mut self, rect: Rect, colorid: ControlColor) {
        self.draw_rect(rect, self.style.colors[colorid as usize]);
        if colorid == ControlColor::ScrollBase || colorid == ControlColor::ScrollThumb || colorid == ControlColor::TitleBG {
            return;
        }
        if self.style.colors[ControlColor::Border as usize].a != 0 {
            self.draw_box(expand_rect(rect, 1), self.style.colors[ControlColor::Border as usize]);
        }
    }

    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.root_list.clear();
        self.text_stack.clear();
        self.scroll_target = None;
        self.hover_root = self.next_hover_root;
        self.next_hover_root = None;
        self.mouse_delta.x = self.mouse_pos.x - self.last_mouse_pos.x;
        self.mouse_delta.y = self.mouse_pos.y - self.last_mouse_pos.y;
        for c in &mut self.containers {
            c.command_list.clear();
            c.children.clear();
            assert!(c.clip_stack.len() == 0);
        }
        self.frame += 1;

        // execute the frame function
        f(self);

        assert_eq!(self.container_stack.len(), 0);
        assert_eq!(self.id_stack.len(), 0);
        // assert_eq!(self.layout_stack.len(), 0);
        if !self.scroll_target.is_none() {
            self.containers[self.scroll_target.unwrap()].scroll.x += self.scroll_delta.x;
            self.containers[self.scroll_target.unwrap()].scroll.y += self.scroll_delta.y;
        }
        if !self.updated_focus {
            self.focus = None;
        }
        self.updated_focus = false;
        if !self.mouse_pressed.is_none()
            && !self.next_hover_root.is_none()
            && self.containers[self.next_hover_root.unwrap()].zindex < self.last_zindex
            && self.containers[self.next_hover_root.unwrap()].zindex >= 0
        {
            self.bring_to_front(self.next_hover_root.unwrap());
        }
        self.key_pressed = KeyMode::NONE;
        self.input_text.clear();
        self.mouse_pressed = MouseButton::NONE;
        self.scroll_delta = vec2(0, 0);
        self.last_mouse_pos = self.mouse_pos;
        self.root_list.sort_by(|a, b| self.containers[*a].zindex.cmp(&self.containers[*b].zindex));

        self.treenode_pool.gc(self.frame);
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

    pub fn push_clip_rect(&mut self, rect: Rect) {
        let last = self.get_clip_rect();
        let container = &mut self.containers[*self.container_stack.last().unwrap()];
        container.clip_stack.push(intersect_rects(rect, last));
    }

    pub fn pop_clip_rect(&mut self) {
        let container = &mut self.containers[*self.container_stack.last().unwrap()];
        container.clip_stack.pop();
    }

    pub fn get_clip_rect(&mut self) -> Rect {
        match self.container_stack.last() {
            Some(idx) => {
                let container = &mut self.containers[*idx];
                match container.clip_stack.last() {
                    Some(r) => *r,
                    None => UNCLIPPED_RECT,
                }
            }
            None => UNCLIPPED_RECT,
        }
    }

    pub fn check_clip(&mut self, r: Rect) -> Clip {
        let cr = self.get_clip_rect();
        if r.x > cr.x + cr.w || r.x + r.w < cr.x || r.y > cr.y + cr.h || r.y + r.h < cr.y {
            return Clip::All;
        }
        if r.x >= cr.x && r.x + r.w <= cr.x + cr.w && r.y >= cr.y && r.y + r.h <= cr.y + cr.h {
            return Clip::None;
        }
        return Clip::Part;
    }

    fn pop_container(&mut self) {
        let layout = *self.top_container().get_layout();
        self.top_container_mut().content_size.x = layout.max.x - layout.body.x;
        self.top_container_mut().content_size.y = layout.max.y - layout.body.y;
        self.top_container_mut().layout_stack.pop();

        self.container_stack.pop();
        self.pop_id();
    }

    pub fn top_container(&self) -> &Container {
        &self.containers[*self.container_stack.last().unwrap()]
    }

    pub fn top_container_mut(&mut self) -> &mut Container {
        &mut self.containers[*self.container_stack.last().unwrap()]
    }

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
            ..Default::default()
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

    pub fn input_mousemove(&mut self, x: i32, y: i32) {
        self.mouse_pos = vec2(x, y);
    }

    pub fn input_mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input_mousemove(x, y);
        self.mouse_down |= btn;
        self.mouse_pressed |= btn;
    }

    pub fn input_mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input_mousemove(x, y);
        self.mouse_down &= !btn;
    }

    pub fn input_scroll(&mut self, x: i32, y: i32) {
        self.scroll_delta.x += x;
        self.scroll_delta.y += y;
    }

    pub fn input_keydown(&mut self, key: KeyMode) {
        self.key_pressed |= key;
        self.key_down |= key;
    }

    pub fn input_keyup(&mut self, key: KeyMode) {
        self.key_down &= !key;
    }

    pub fn input_text(&mut self, text: &str) {
        self.input_text += text;
    }

    pub fn push_command(&mut self, cmd: Command) {
        let container = &mut self.containers[*self.container_stack.last().unwrap()];
        container.command_list.push(cmd);
    }

    pub fn push_text(&mut self, str: &str) -> usize {
        let str_start = self.text_stack.len();
        for c in str.chars() {
            self.text_stack.push(c);
        }
        return str_start;
    }

    pub fn set_clip(&mut self, rect: Rect) {
        self.push_command(Command::Clip { rect });
    }

    pub fn draw_rect(&mut self, mut rect: Rect, color: Color) {
        rect = intersect_rects(rect, self.get_clip_rect());
        if rect.w > 0 && rect.h > 0 {
            self.push_command(Command::Rect { rect, color });
        }
    }

    pub fn draw_box(&mut self, r: Rect, color: Color) {
        self.draw_rect(rect(r.x + 1, r.y, r.w - 2, 1), color);
        self.draw_rect(rect(r.x + 1, r.y + r.h - 1, r.w - 2, 1), color);
        self.draw_rect(rect(r.x, r.y, 1, r.h), color);
        self.draw_rect(rect(r.x + r.w - 1, r.y, 1, r.h), color);
    }

    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        let rect: Rect = rect(pos.x, pos.y, self.get_text_width(font, str), self.get_text_height(font, str));
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

    pub fn draw_icon(&mut self, id: Icon, rect: Rect, color: Color) {
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

    fn in_hover_root(&mut self) -> bool {
        match self.hover_root {
            Some(hover_root) => {
                let len = self.container_stack.len();
                for i in 0..len {
                    if self.container_stack[len - i - 1] == hover_root {
                        return true;
                    }
                    if self.containers[self.container_stack[len - i - 1]].command_list.len() != 0 {
                        break;
                    }
                }
                false
            }
            None => false,
        }
    }

    pub fn draw_control_frame(&mut self, id: Id, rect: Rect, mut colorid: ControlColor, opt: WidgetOption) {
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

    pub fn draw_control_text(&mut self, str: &str, rect: Rect, colorid: ControlColor, opt: WidgetOption) {
        let mut pos: Vec2i = Vec2i { x: 0, y: 0 };
        let font = self.style.font;
        let tw = self.get_text_width(font, str);
        self.push_clip_rect(rect);
        pos.y = rect.y + (rect.h - self.get_text_height(font, str)) / 2;
        if opt.is_aligned_center() {
            pos.x = rect.x + (rect.w - tw) / 2;
        } else if opt.is_aligned_right() {
            pos.x = rect.x + rect.w - tw - self.style.padding;
        } else {
            pos.x = rect.x + self.style.padding;
        }
        self.draw_text(font, str, pos, self.style.colors[colorid as usize]);
        self.pop_clip_rect();
    }

    pub fn mouse_over(&mut self, rect: Rect) -> bool {
        rect_overlaps_vec2(rect, self.mouse_pos) && rect_overlaps_vec2(self.get_clip_rect(), self.mouse_pos) && self.in_hover_root()
    }

    pub fn update_control(&mut self, id: Id, rect: Rect, opt: WidgetOption) {
        let mouseover = self.mouse_over(rect);
        if self.focus == Some(id) {
            self.updated_focus = true;
        }
        if opt.is_not_interactive() {
            return;
        }
        if mouseover && self.mouse_down.is_none() {
            self.hover = Some(id);
        }
        if self.focus == Some(id) {
            if !self.mouse_pressed.is_none() && !mouseover {
                self.set_focus(None);
            }
            if self.mouse_down.is_none() && !opt.is_holding_focus() {
                self.set_focus(None);
            }
        }
        if self.hover == Some(id) {
            if !self.mouse_pressed.is_none() {
                self.set_focus(Some(id));
            } else if !mouseover {
                self.hover = None;
            }
        }
    }

    pub fn get_text_width(&self, font: FontId, text: &str) -> i32 {
        let mut res = 0;
        let mut acc = 0;
        for c in text.chars() {
            if c == '\n' {
                res = usize::max(res, acc);
                acc = 0;
            }
            acc += self.atlas_renderer.get_char_width(font, c);
        }
        res = usize::max(res, acc);
        res as i32
    }

    pub fn get_text_height(&self, font: FontId, text: &str) -> i32 {
        let font_height = self.atlas_renderer.get_font_height(font);
        let lc = text.lines().count();
        (lc * font_height) as i32
    }

    pub fn text(&mut self, text: &str) {
        let font = self.style.font;
        let color = self.style.colors[ControlColor::Text as usize];
        let h = self.atlas_renderer.get_font_height(font) as i32;
        self.top_container_mut().layout_begin_column();
        self.top_container_mut().layout_row(&[-1], h);
        for line in text.lines() {
            let mut r = self.top_container_mut().layout_next();
            let mut rx = r.x;
            let words = line.split_inclusive(' ');
            for w in words {
                // TODO: split w when its width > w into many lines
                let tw = self.get_text_width(font, w);
                if tw + rx < r.x + r.w {
                    self.draw_text(font, w, vec2(rx, r.y), color);
                    rx += tw;
                } else {
                    r = self.top_container_mut().layout_next();
                    rx = r.x;
                }
            }
        }
        self.top_container_mut().layout_end_column();
    }

    pub fn label(&mut self, text: &str) {
        let layout = self.top_container_mut().layout_next();
        self.draw_control_text(text, layout, ControlColor::Text, WidgetOption::NONE);
    }

    pub fn button_ex(&mut self, label: &str, icon: Icon, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = if label.len() > 0 {
            self.get_id_from_str(label)
        } else {
            self.get_id_u32(icon as u32)
        };
        let r: Rect = self.top_container_mut().layout_next();
        self.update_control(id, r, opt);
        if self.mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }
        self.draw_control_frame(id, r, ControlColor::Button, opt);
        if label.len() > 0 {
            self.draw_control_text(label, r, ControlColor::Text, opt);
        }
        if icon != Icon::None {
            self.draw_icon(icon, r, self.style.colors[ControlColor::Text as usize]);
        }
        return res;
    }

    pub fn checkbox(&mut self, label: &str, state: &mut bool) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = self.get_id_from_ptr(state);
        let mut r: Rect = self.top_container_mut().layout_next();
        let box_0: Rect = rect(r.x, r.y, r.h, r.h);
        self.update_control(id, r, WidgetOption::NONE);
        if self.mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::CHANGE;
            *state = *state == false;
        }
        self.draw_control_frame(id, box_0, ControlColor::Base, WidgetOption::NONE);
        if *state {
            self.draw_icon(Icon::Check, box_0, self.style.colors[ControlColor::Text as usize]);
        }
        r = rect(r.x + box_0.w, r.y, r.w - box_0.w, r.h);
        self.draw_control_text(label, r, ControlColor::Text, WidgetOption::NONE);
        return res;
    }

    pub fn textbox_raw(&mut self, buf: &mut String, id: Id, r: Rect, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        self.update_control(id, r, opt | WidgetOption::HOLD_FOCUS);
        if self.focus == Some(id) {
            let mut len = buf.len();

            if self.input_text.len() > 0 {
                buf.push_str(self.input_text.as_str());
                len += self.input_text.len() as usize;
                res |= ResourceState::CHANGE
            }

            if self.key_pressed.is_backspace() && len > 0 {
                // skip utf-8 continuation bytes
                buf.pop();
                res |= ResourceState::CHANGE
            }
            if self.key_pressed.is_return() {
                self.set_focus(None);
                res |= ResourceState::SUBMIT;
            }
        }
        self.draw_control_frame(id, r, ControlColor::Base, opt);
        if self.focus == Some(id) {
            let color = self.style.colors[ControlColor::Text as usize];
            let font = self.style.font;
            let textw = self.get_text_width(font, buf.as_str());
            let texth = self.get_text_height(font, buf.as_str());
            let ofx = r.w - self.style.padding - textw - 1;
            let textx = r.x + (if ofx < self.style.padding { ofx } else { self.style.padding });
            let texty = r.y + (r.h - texth) / 2;
            self.push_clip_rect(r);
            self.draw_text(font, buf.as_str(), vec2(textx, texty), color);
            self.draw_rect(rect(textx + textw, texty, 1, texth), color);
            self.pop_clip_rect();
        } else {
            self.draw_control_text(buf.as_str(), r, ControlColor::Text, opt);
        }
        return res;
    }

    fn number_textbox(&mut self, precision: usize, value: &mut Real, r: Rect, id: Id) -> ResourceState {
        if self.mouse_pressed.is_left() && self.key_down.is_shift() && self.hover == Some(id) {
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
        let r: Rect = self.top_container_mut().layout_next();
        return self.textbox_raw(buf, id, r, opt);
    }

    pub fn slider_ex(&mut self, value: &mut Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let last = *value;
        let mut v = last;
        let id = self.get_id_from_ptr(value);
        let base = self.top_container_mut().layout_next();
        if !self.number_textbox(precision, &mut v, base, id).is_none() {
            return res;
        }
        self.update_control(id, base, opt);
        if self.focus == Some(id) && (!self.mouse_down.is_none() | self.mouse_pressed.is_left()) {
            v = low + (self.mouse_pos.x - base.x) as Real * (high - low) / base.w as Real;
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
        let x = ((v - low) * (base.w - w) as Real / (high - low)) as i32;
        let thumb = rect(base.x + x, base.y, w, base.h);
        self.draw_control_frame(id, thumb, ControlColor::Button, opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", precision, value).as_str());
        self.draw_control_text(buff.as_str(), base, ControlColor::Text, opt);
        return res;
    }

    pub fn number_ex(&mut self, value: &mut Real, step: Real, precision: usize, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = self.get_id_from_ptr(value);
        let base: Rect = self.top_container_mut().layout_next();
        let last: Real = *value;
        if !self.number_textbox(precision, value, base, id).is_none() {
            return res;
        }
        self.update_control(id, base, opt);
        if self.focus == Some(id) && self.mouse_down.is_left() {
            *value += self.mouse_delta.x as Real * step;
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

    fn node(&mut self, label: &str, is_treenode: bool, opt: WidgetOption) -> ResourceState {
        let id: Id = self.get_id_from_str(label);
        self.top_container_mut().layout_row(&[-1], 0);
        let mut r = self.top_container_mut().layout_next();
        self.update_control(id, r, WidgetOption::NONE);
        let state = self.treenode_pool.get(id);
        let mut active = state.is_some();
        // clever substitution for if opt.is_expanded() { !active } else { active };
        let expanded = opt.is_expanded() ^ active;
        active ^= self.mouse_pressed.is_left() && self.focus == Some(id);
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
                self.draw_frame(r, ControlColor::ButtonHover);
            }
        } else {
            self.draw_control_frame(id, r, ControlColor::Button, WidgetOption::NONE);
        }
        self.draw_icon(
            if expanded { Icon::Expanded } else { Icon::Collapsed },
            rect(r.x, r.y, r.h, r.h),
            self.style.colors[ControlColor::Text as usize],
        );
        r.x += r.h - self.style.padding;
        r.w -= r.h - self.style.padding;
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
            self.top_container_mut().get_layout_mut().indent += self.style.indent;
            self.id_stack.push(self.last_id.unwrap());
        }

        if !res.is_none() {
            f(self);
            self.top_container_mut().get_layout_mut().indent -= self.style.indent;
            self.pop_id();
        }
    }

    fn clamp(x: i32, a: i32, b: i32) -> i32 {
        i32::min(b, i32::max(a, x))
    }

    fn scrollbars(&mut self, cnt_id: usize, body: &mut Rect) {
        let sz = self.style.scrollbar_size;
        let mut cs: Vec2i = self.containers[cnt_id].content_size;
        cs.x += self.style.padding * 2;
        cs.y += self.style.padding * 2;
        self.push_clip_rect(body.clone());
        if cs.y > self.containers[cnt_id].body.h {
            body.w -= sz;
        }
        if cs.x > self.containers[cnt_id].body.w {
            body.h -= sz;
        }
        let body = *body;
        let maxscroll = cs.y - body.h;
        if maxscroll > 0 && body.h > 0 {
            let id: Id = self.get_id_from_str("!scrollbary");
            let mut base = body;
            base.x = body.x + body.w;
            base.w = self.style.scrollbar_size;
            self.update_control(id, base, WidgetOption::NONE);
            if self.focus == Some(id) && self.mouse_down.is_left() {
                self.containers[cnt_id].scroll.y += self.mouse_delta.y * cs.y / base.h;
            }
            self.containers[cnt_id].scroll.y = Self::clamp(self.containers[cnt_id].scroll.y, 0, maxscroll);

            self.draw_frame(base, ControlColor::ScrollBase);
            let mut thumb = base;
            thumb.h = if self.style.thumb_size > base.h * body.h / cs.y {
                self.style.thumb_size
            } else {
                base.h * body.h / cs.y
            };
            thumb.y += self.containers[cnt_id].scroll.y * (base.h - thumb.h) / maxscroll;
            self.draw_frame(thumb, ControlColor::ScrollThumb);
            if self.mouse_over(body) {
                self.scroll_target = Some(cnt_id);
            }
        } else {
            self.containers[cnt_id].scroll.y = 0;
        }
        let maxscroll_0 = cs.x - body.w;
        if maxscroll_0 > 0 && body.w > 0 {
            let id_0: Id = self.get_id_from_str("!scrollbarx");
            let mut base_0 = body;
            base_0.y = body.y + body.h;
            base_0.h = self.style.scrollbar_size;
            self.update_control(id_0, base_0, WidgetOption::NONE);
            if self.focus == Some(id_0) && self.mouse_down.is_left() {
                self.containers[cnt_id].scroll.x += self.mouse_delta.x * cs.x / base_0.w;
            }
            self.containers[cnt_id].scroll.x = Self::clamp(self.containers[cnt_id].scroll.x, 0, maxscroll_0);

            self.draw_frame(base_0, ControlColor::ScrollBase);
            let mut thumb_0 = base_0;
            thumb_0.w = if self.style.thumb_size > base_0.w * body.w / cs.x {
                self.style.thumb_size
            } else {
                base_0.w * body.w / cs.x
            };
            thumb_0.x += self.containers[cnt_id].scroll.x * (base_0.w - thumb_0.w) / maxscroll_0;
            self.draw_frame(thumb_0, ControlColor::ScrollThumb);
            if self.mouse_over(body) {
                self.scroll_target = Some(cnt_id);
            }
        } else {
            self.containers[cnt_id].scroll.x = 0;
        }
        self.pop_clip_rect();
    }

    fn push_container_body(&mut self, cnt_idx: usize, body: Rect, opt: WidgetOption) {
        let mut body = body;
        if !opt.has_no_scroll() {
            self.scrollbars(cnt_idx, &mut body);
        }
        let padding = -self.style.padding;
        let scroll = self.containers[cnt_idx].scroll;
        self.top_container_mut().push_layout(expand_rect(body, padding), scroll);
        self.containers[cnt_idx].body = body;
    }

    fn begin_root_container(&mut self, cnt: usize) {
        self.container_stack.push(cnt);

        self.root_list.push(cnt);

        if rect_overlaps_vec2(self.containers[cnt].rect, self.mouse_pos)
            && (self.next_hover_root.is_none() || self.containers[cnt].zindex > self.containers[self.next_hover_root.unwrap()].zindex)
        {
            self.next_hover_root = Some(cnt);
        }
        let container = &mut self.containers[*self.container_stack.last().unwrap()];
        container.clip_stack.push(UNCLIPPED_RECT);
    }

    fn end_root_container(&mut self) {
        self.pop_clip_rect();
        self.pop_container();
    }

    pub fn window<F: FnOnce(&mut Self)>(&mut self, title: &str, mut r: Rect, opt: WidgetOption, f: F) {
        let id = self.get_id_from_str(title);
        let cnt_id = self.get_container_index_intern(id, title, opt);
        if cnt_id.is_none() || !self.containers[cnt_id.unwrap()].open {
            return;
        }
        self.id_stack.push(id);

        if self.containers[cnt_id.unwrap()].rect.w == 0 {
            self.containers[cnt_id.unwrap()].rect = r;
        }
        self.begin_root_container(cnt_id.unwrap());
        let mut body = self.containers[cnt_id.unwrap()].rect;
        r = body;
        if !opt.has_no_frame() {
            self.draw_frame(r, ControlColor::WindowBG);
        }
        if !opt.has_no_title() {
            let mut tr: Rect = r;
            tr.h = self.style.title_height;
            self.draw_frame(tr, ControlColor::TitleBG);

            // TODO: Is this necessary?
            if !opt.has_no_title() {
                let id = self.get_id_from_str("!title");
                self.update_control(id, tr, opt);
                self.draw_control_text(title, tr, ControlColor::TitleText, opt);
                if Some(id) == self.focus && self.mouse_down.is_left() {
                    self.containers[cnt_id.unwrap()].rect.x += self.mouse_delta.x;
                    self.containers[cnt_id.unwrap()].rect.y += self.mouse_delta.y;
                }
                body.y += tr.h;
                body.h -= tr.h;
            }
            if !opt.has_no_close() {
                let id = self.get_id_from_str("!close");
                let r: Rect = rect(tr.x + tr.w - tr.h, tr.y, tr.h, tr.h);
                tr.w -= r.w;
                self.draw_icon(Icon::Close, r, self.style.colors[ControlColor::TitleText as usize]);
                self.update_control(id, r, opt);
                if self.mouse_pressed.is_left() && Some(id) == self.focus {
                    self.containers[cnt_id.unwrap()].open = false;
                }
            }
        }
        self.push_container_body(cnt_id.unwrap(), body, opt);
        if !opt.is_auto_sizing() {
            let sz = self.style.title_height;
            let id_2 = self.get_id_from_str("!resize");
            let r_0 = rect(r.x + r.w - sz, r.y + r.h - sz, sz, sz);
            self.update_control(id_2, r_0, opt);
            if Some(id_2) == self.focus && self.mouse_down.is_left() {
                self.containers[cnt_id.unwrap()].rect.w = if 96 > self.containers[cnt_id.unwrap()].rect.w + self.mouse_delta.x {
                    96
                } else {
                    self.containers[cnt_id.unwrap()].rect.w + self.mouse_delta.x
                };
                self.containers[cnt_id.unwrap()].rect.h = if 64 > self.containers[cnt_id.unwrap()].rect.h + self.mouse_delta.y {
                    64
                } else {
                    self.containers[cnt_id.unwrap()].rect.h + self.mouse_delta.y
                };
            }
        }
        if opt.is_auto_sizing() {
            let r_1 = self.top_container_mut().get_layout().body;
            self.containers[cnt_id.unwrap()].rect.w = self.containers[cnt_id.unwrap()].content_size.x + (self.containers[cnt_id.unwrap()].rect.w - r_1.w);
            self.containers[cnt_id.unwrap()].rect.h = self.containers[cnt_id.unwrap()].content_size.y + (self.containers[cnt_id.unwrap()].rect.h - r_1.h);
        }

        if opt.is_popup() && !self.mouse_pressed.is_none() && self.hover_root != cnt_id {
            self.containers[cnt_id.unwrap()].open = false;
        }
        self.push_clip_rect(self.containers[cnt_id.unwrap()].body);

        // call the window function
        f(self);

        self.pop_clip_rect();
        self.end_root_container();
    }

    pub fn open_popup(&mut self, name: &str) {
        let cnt = self.get_container_index(name);
        self.next_hover_root = cnt;
        self.hover_root = self.next_hover_root;
        self.containers[cnt.unwrap()].rect = rect(self.mouse_pos.x, self.mouse_pos.y, 1, 1);
        self.containers[cnt.unwrap()].open = true;
        self.bring_to_front(cnt.unwrap());
    }

    pub fn popup<F: FnOnce(&mut Self)>(&mut self, name: &str, f: F) {
        let opt =
            WidgetOption::POPUP | WidgetOption::AUTO_SIZE | WidgetOption::NO_RESIZE | WidgetOption::NO_SCROLL | WidgetOption::NO_TITLE | WidgetOption::CLOSED;
        self.window(name, rect(0, 0, 0, 0), opt, f);
    }

    pub fn panel<F: FnOnce(&mut Self)>(&mut self, name: &str, opt: WidgetOption, f: F) {
        self.push_id_from_str(name);

        // A panel can only exist inside a root container
        assert!(self.root_list.len() != 0);

        let cnt_id = self.get_container_index_intern(self.last_id.unwrap(), name, opt);
        self.containers[*self.root_list.last().unwrap()].children.push(cnt_id.unwrap());
        let rect = self.top_container_mut().layout_next();
        let clip_rect = self.containers[cnt_id.unwrap()].body;
        self.containers[cnt_id.unwrap()].rect = rect;
        if !opt.has_no_frame() {
            self.draw_frame(rect, ControlColor::PanelBG);
        }

        self.container_stack.push(cnt_id.unwrap());
        self.push_container_body(cnt_id.unwrap(), rect, opt);

        self.push_clip_rect(clip_rect);

        // call the panel function
        f(self);

        self.pop_clip_rect();
        self.pop_container();
    }
}
