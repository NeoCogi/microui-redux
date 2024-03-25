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
use std::{
    cell::{Ref, RefCell, RefMut},
    f32,
    hash::Hash,
    rc::Rc,
};

mod atlas;
mod canvas;
mod container;
mod idmngr;
mod layout;
mod rect_packer;
mod window;

pub use atlas::*;
pub use idmngr::*;
pub use layout::*;
pub use container::*;
pub use window::*;
pub use canvas::*;
pub use rect_packer::*;
pub use rs_math3d::*;

use bitflags::*;
use std::cmp::{min, max};

pub trait Renderer {
    fn get_atlas(&self) -> AtlasHandle;
    fn clear(&mut self, width: i32, height: i32, clr: Color);
    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex);
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
    #[derive(Copy, Clone, Debug)]
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
    #[derive(Copy, Clone, Debug)]
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

#[derive(Clone, Debug)]
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
        for c in text.chars() {
            self.input_text.push(c);
        }
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
            font: FontId::default(),
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

#[derive(Clone)]
pub struct ContainerHandle(Rc<RefCell<Container>>);

impl ContainerHandle {
    pub(crate) fn new(container: Container) -> Self {
        Self(Rc::new(RefCell::new(container)))
    }

    pub(crate) fn render<R: Renderer>(&self, canvas: &mut Canvas<R>) {
        self.0.borrow().render(canvas)
    }

    pub fn inner<'a>(&'a self) -> Ref<'a, Container> {
        self.0.borrow()
    }

    pub fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Container> {
        self.0.borrow_mut()
    }
}

pub struct Context<R: Renderer> {
    canvas: Canvas<R>,
    style: Style,

    last_zindex: i32,
    frame: usize,
    hover_root: Option<WindowHandle>,
    next_hover_root: Option<WindowHandle>,
    scroll_target: Option<WindowHandle>,

    root_list: Vec<WindowHandle>,

    pub input: Rc<RefCell<Input>>,
}

impl<R: Renderer> Context<R> {
    pub fn new(renderer: R, dim: Dimensioni) -> Self {
        Self {
            canvas: Canvas::from(renderer, dim),
            style: Style::default(),
            last_zindex: 0,
            frame: 0,
            hover_root: None,
            next_hover_root: None,
            scroll_target: None,

            root_list: Vec::default(),

            input: Rc::new(RefCell::new(Input::default())),
        }
    }
}

impl<R: Renderer> Context<R> {
    pub fn clear(&mut self, width: i32, height: i32, clr: Color) {
        self.canvas.clear(width, height, clr);
    }

    pub fn flush(&mut self) {
        for r in &self.root_list {
            r.render(&mut self.canvas);
        }
        self.canvas.flush()
    }

    #[inline(never)]
    fn frame_begin(&mut self) {
        self.scroll_target = None;
        self.input.borrow_mut().prelude();
        for r in &mut self.root_list {
            r.prepare();
        }
        self.frame += 1;
        self.root_list.clear();
    }

    #[inline(never)]
    fn frame_end(&mut self) {
        for r in &mut self.root_list {
            r.finish();
        }

        let mouse_pressed = self.input.borrow().mouse_pressed;
        match (mouse_pressed.is_none(), &self.next_hover_root) {
            (false, Some(next_hover_root)) if next_hover_root.zindex() < self.last_zindex && next_hover_root.zindex() >= 0 => {
                self.bring_to_front(&mut next_hover_root.clone());
            }
            _ => (),
        }

        self.input.borrow_mut().epilogue();

        // prepare the next frame
        self.hover_root = self.next_hover_root.clone();
        self.next_hover_root = None;
        for r in &mut self.root_list {
            r.inner_mut().main.in_hover_root = false;
        }
        match &mut self.hover_root {
            Some(window) => window.inner_mut().main.in_hover_root = true,
            _ => (),
        }

        // sort all windows
        self.root_list.sort_by(|a, b| a.zindex().cmp(&b.zindex()));
    }

    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.frame_begin();

        // execute the frame function
        f(self);

        self.frame_end();
    }

    pub fn new_window(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let mut window = WindowHandle::window(name, self.canvas.get_atlas(), &self.style, self.input.clone(), initial_rect);
        self.bring_to_front(&mut window);
        window
    }

    pub fn new_popup(&mut self, name: &str) -> WindowHandle {
        WindowHandle::popup(name, self.canvas.get_atlas(), &self.style, self.input.clone())
    }

    pub fn new_panel(&mut self, name: &str) -> ContainerHandle {
        ContainerHandle::new(Container::new(name, self.canvas.get_atlas(), &self.style, self.input.clone()))
    }

    pub fn bring_to_front(&mut self, window: &mut WindowHandle) {
        self.last_zindex += 1;
        window.inner_mut().main.zindex = self.last_zindex;
    }

    #[inline(never)]
    fn begin_root_container(&mut self, window: &mut WindowHandle) {
        self.root_list.push(window.clone());

        if window.inner().main.rect.contains(&self.input.borrow().mouse_pos)
            && (self.next_hover_root.is_none() || window.zindex() > self.next_hover_root.as_ref().unwrap().zindex())
        {
            self.next_hover_root = Some(window.clone());
        }
        let container = &mut window.inner_mut().main;
        container.clip_stack.push(UNCLIPPED_RECT);
    }

    #[inline(never)]
    fn end_root_container(&mut self, window: &mut WindowHandle) {
        let container = &mut window.inner_mut().main;
        container.pop_clip_rect();

        let layout = *container.layout.top();
        container.content_size.x = layout.max.x - layout.body.x;
        container.content_size.y = layout.max.y - layout.body.y;
        container.layout.stack.pop();
    }

    #[inline(never)]
    #[must_use]
    fn begin_window(&mut self, window: &mut WindowHandle, opt: WidgetOption) -> bool {
        if !window.is_open() {
            return false;
        }

        self.begin_root_container(window);
        window.begin_window(opt);

        true
    }

    fn end_window(&mut self, window: &mut WindowHandle) {
        window.end_window();
        self.end_root_container(window);
    }

    pub fn window<F: FnOnce(&mut Container)>(&mut self, window: &mut WindowHandle, opt: WidgetOption, f: F) {
        // call the window function if the window is open
        if self.begin_window(window, opt) {
            window.inner_mut().main.style = self.style.clone();
            f(&mut window.inner_mut().main);
            self.end_window(window);
        }
    }

    pub fn open_popup(&mut self, window: &mut WindowHandle) {
        self.next_hover_root = Some(window.clone());
        self.hover_root = self.next_hover_root.clone();
        window.inner_mut().main.rect = rect(self.input.borrow().mouse_pos.x, self.input.borrow().mouse_pos.y, 1, 1);
        window.inner_mut().activity = Activity::Open;
        window.inner_mut().main.in_hover_root = true;
        self.bring_to_front(window);
    }

    pub fn popup<F: FnOnce(&mut Container)>(&mut self, window: &mut WindowHandle, f: F) {
        let opt = WidgetOption::AUTO_SIZE | WidgetOption::NO_RESIZE | WidgetOption::NO_SCROLL | WidgetOption::NO_TITLE;
        self.window(window, opt, f);
    }

    pub fn set_style(&mut self, style: &Style) {
        self.style = style.clone()
    }
}
