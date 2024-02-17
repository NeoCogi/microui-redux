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
use std::cell::{Ref, RefMut};
use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Activity {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Type {
    Window,
    Popup,
}

#[derive(Clone)]
pub(crate) struct Window {
    activity: Activity,
    pub(crate) main: Container,
}

impl Window {
    pub fn window(id: Id, name: &str, atlas: Rc<dyn Atlas>, style: &Style, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(id, name, atlas, style, input);
        main.rect = initial_rect;

        Self { activity: Activity::Open, main }
    }
}

#[derive(Clone)]
pub struct WindowHandle(Rc<RefCell<Window>>);

impl WindowHandle {
    pub(crate) fn new(id: Id, name: &str, atlas: Rc<dyn Atlas>, style: &Style, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        Self(Rc::new(RefCell::new(Window::window(id, name, atlas, style, input, initial_rect))))
    }

    pub fn is_open(&self) -> bool {
        match self.0.borrow().activity {
            Activity::Open => true,
            _ => false,
        }
    }

    pub(crate) fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Window> {
        self.0.borrow_mut()
    }

    pub(crate) fn inner<'a>(&'a mut self) -> Ref<'a, Window> {
        self.0.borrow()
    }

    pub(crate) fn prepare(&mut self) {
        self.inner_mut().main.prepare()
    }

    pub(crate) fn render(&self, canvas: &mut dyn Canvas) {
        self.0.borrow().main.render(canvas)
    }

    pub(crate) fn finish(&mut self) {
        self.inner_mut().main.finish()
    }

    pub(crate) fn zindex(&self) -> i32 {
        self.0.borrow().main.zindex
    }

    pub(crate) fn begin_window(&mut self, opt: WidgetOption) {
        self.0.borrow_mut().main.begin_window(opt)
    }

    pub(crate) fn end_window(&mut self) {
        self.inner_mut().main.end_window()
    }
}
