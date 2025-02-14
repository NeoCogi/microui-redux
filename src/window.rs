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
use std::cell::{Ref, RefMut};

#[derive(Clone, Copy, Debug)]
pub enum WindowState {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Type {
    Dialog,
    Window,
    Popup,
}

pub(crate) struct Window {
    pub(crate) ty: Type,
    pub(crate) win_state: WindowState,
    pub(crate) main: Container,
}

impl Window {
    pub fn dialog(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.rect = initial_rect;

        Self {
            ty: Type::Dialog,
            win_state: WindowState::Closed,
            main,
        }
    }

    pub fn window(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.rect = initial_rect;

        Self {
            ty: Type::Window,
            win_state: WindowState::Open,
            main,
        }
    }

    pub fn popup(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.rect = initial_rect;

        Self {
            ty: Type::Popup,
            win_state: WindowState::Closed,
            main,
        }
    }

    pub fn is_popup(&self) -> bool {
        match self.ty {
            Type::Popup => true,
            _ => false,
        }
    }

    #[inline(never)]
    fn begin_window(&mut self, opt: ContainerOption) {
        let is_popup = self.is_popup();
        let container = &mut self.main;
        let mut body = container.rect;
        let r = body;
        if !opt.has_no_frame() {
            container.draw_frame(r, ControlColor::WindowBG);
        }
        if !opt.has_no_title() {
            let mut tr: Recti = r;
            tr.height = container.style.title_height;
            container.draw_frame(tr, ControlColor::TitleBG);

            // TODO: Is this necessary?
            if !opt.has_no_title() {
                let id = container.idmngr.get_id_from_str("!title");
                container.update_control(id, tr, WidgetOption::NONE);
                container.draw_control_text(
                    &container.name.clone(), /* TODO: cloning the string is expensive, go to a different approach */
                    tr,
                    ControlColor::TitleText,
                    WidgetOption::NONE,
                );
                if Some(id) == container.focus && container.input.borrow().mouse_down.is_left() {
                    container.rect.x += container.input.borrow().mouse_delta.x;
                    container.rect.y += container.input.borrow().mouse_delta.y;
                }
                body.y += tr.height;
                body.height -= tr.height;
            }
            if !opt.has_no_close() {
                let id = container.idmngr.get_id_from_str("!close");
                let r: Recti = rect(tr.x + tr.width - tr.height, tr.y, tr.height, tr.height);
                tr.width -= r.width;
                let color = container.style.colors[ControlColor::TitleText as usize];
                container.draw_icon(CLOSE_ICON, r, color);
                container.update_control(id, r, WidgetOption::NONE);
                if container.input.borrow().mouse_pressed.is_left() && Some(id) == container.focus {
                    self.win_state = WindowState::Closed;
                }
            }
        }
        container.push_container_body(body, opt);
        if !opt.is_auto_sizing() {
            let sz = container.style.title_height;
            let id_2 = container.idmngr.get_id_from_str("!resize");
            let r_0 = rect(r.x + r.width - sz, r.y + r.height - sz, sz, sz);
            container.update_control(id_2, r_0, WidgetOption::NONE);
            if Some(id_2) == container.focus && container.input.borrow().mouse_down.is_left() {
                container.rect.width = if 96 > container.rect.width + container.input.borrow().mouse_delta.x {
                    96
                } else {
                    container.rect.width + container.input.borrow().mouse_delta.x
                };
                container.rect.height = if 64 > container.rect.height + container.input.borrow().mouse_delta.y {
                    64
                } else {
                    container.rect.height + container.input.borrow().mouse_delta.y
                };
            }
        }
        if opt.is_auto_sizing() {
            let r_1 = container.layout.top().body;
            container.rect.width = container.content_size.x + (container.rect.width - r_1.width);
            container.rect.height = container.content_size.y + (container.rect.height - r_1.height);
        }

        if is_popup && !container.input.borrow().mouse_pressed.is_none() && !container.in_hover_root {
            self.win_state = WindowState::Closed;
        }
        let body = container.body;
        container.push_clip_rect(body);
    }

    fn end_window(&mut self) {
        let container = &mut self.main;
        container.pop_clip_rect();
    }
}

#[derive(Clone)]
pub struct WindowHandle(Rc<RefCell<Window>>);

impl WindowHandle {
    pub(crate) fn window(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        Self(Rc::new(RefCell::new(Window::window(name, atlas, style, input, initial_rect))))
    }

    pub(crate) fn dialog(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        Self(Rc::new(RefCell::new(Window::dialog(name, atlas, style, input, initial_rect))))
    }

    pub(crate) fn popup(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>) -> Self {
        Self(Rc::new(RefCell::new(Window::popup(name, atlas, style, input, Recti::new(0, 0, 0, 0)))))
    }

    pub fn is_open(&self) -> bool {
        match self.0.borrow().win_state {
            WindowState::Open => true,
            _ => false,
        }
    }

    pub(crate) fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Window> {
        self.0.borrow_mut()
    }

    pub(crate) fn inner<'a>(&'a self) -> Ref<'a, Window> {
        self.0.borrow()
    }

    pub(crate) fn prepare(&mut self) {
        self.inner_mut().main.prepare()
    }

    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        self.0.borrow_mut().main.render(canvas)
    }

    pub(crate) fn finish(&mut self) {
        self.inner_mut().main.finish()
    }

    pub(crate) fn zindex(&self) -> i32 {
        self.0.borrow().main.zindex
    }

    pub(crate) fn begin_window(&mut self, opt: ContainerOption) {
        self.0.borrow_mut().begin_window(opt)
    }

    pub(crate) fn end_window(&mut self) {
        self.inner_mut().end_window()
    }

    pub fn set_size(&mut self, size: &Dimensioni) {
        self.inner_mut().main.rect.width = size.width;
        self.inner_mut().main.rect.height = size.height;
    }
}
