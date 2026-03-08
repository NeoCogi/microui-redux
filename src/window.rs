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
use crate::{widget::FrameResults, widget_tree::WidgetTree};
use std::cell::{Ref, RefMut};

#[derive(Clone, Copy, Debug)]
/// Indicates whether a window should be rendered this frame.
pub enum WindowState {
    /// Window is visible and will receive input.
    Open,
    /// Window is hidden.
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
    last_root_frame: Option<usize>,
    pub(crate) main: Container,
    /// Internal state for the window title bar.
    pub(crate) title_state: Internal,
    /// Internal state for the window close button.
    pub(crate) close_state: Internal,
    /// Internal state for the window resize handle.
    pub(crate) resize_state: Internal,
}

impl Window {
    fn titlebar_height(container: &Container) -> i32 {
        let style = container.style.as_ref();
        let font_height = container.atlas.get_font_height(style.font) as i32;
        let padding = style.padding.max(0);
        let min_title_h = font_height + (padding / 2).max(1) * 2;
        style.title_height.max(min_title_h)
    }

    fn body_rect_for(container: &Container, opt: ContainerOption) -> Recti {
        let mut body = container.rect;
        if !opt.has_no_title() {
            let title_h = Self::titlebar_height(container);
            body.y += title_h;
            body.height -= title_h;
        }
        body
    }

    fn apply_auto_size(container: &mut Container, opt: ContainerOption) {
        if !opt.is_auto_sizing() || (container.content_size.x <= 0 && container.content_size.y <= 0) {
            return;
        }

        let padding = container.style.as_ref().padding.max(0) * 2;
        let target_body_width = container.content_size.x.saturating_add(padding);
        let target_body_height = container.content_size.y.saturating_add(padding);
        let body = Self::body_rect_for(container, opt);
        let chrome_width = container.rect.width - body.width;
        let chrome_height = container.rect.height - body.height;

        container.rect.width = (target_body_width + chrome_width).max(0);
        container.rect.height = (target_body_height + chrome_height).max(0);
    }

    /// Creates a dialog window that starts closed.
    pub fn dialog(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.rect = initial_rect;

        Self {
            ty: Type::Dialog,
            win_state: WindowState::Closed,
            last_root_frame: None,
            main,
            title_state: Internal::new("!title"),
            close_state: Internal::new("!close"),
            resize_state: Internal::new("!resize"),
        }
    }

    /// Creates a standard window that starts open.
    pub fn window(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.rect = initial_rect;

        Self {
            ty: Type::Window,
            win_state: WindowState::Open,
            last_root_frame: None,
            main,
            title_state: Internal::new("!title"),
            close_state: Internal::new("!close"),
            resize_state: Internal::new("!resize"),
        }
    }

    /// Creates a popup window that starts closed.
    pub fn popup(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.rect = initial_rect;

        Self {
            ty: Type::Popup,
            win_state: WindowState::Closed,
            last_root_frame: None,
            main,
            title_state: Internal::new("!title"),
            close_state: Internal::new("!close"),
            resize_state: Internal::new("!resize"),
        }
    }

    /// Returns `true` if this handle manages a popup window.
    pub fn is_popup(&self) -> bool {
        match self.ty {
            Type::Popup => true,
            _ => false,
        }
    }

    #[inline(never)]
    fn begin_window(&mut self, opt: ContainerOption, bopt: WidgetBehaviourOption) {
        let is_popup = self.is_popup();
        let Window {
            win_state,
            main: container,
            title_state,
            close_state,
            resize_state: _,
            ..
        } = self;
        Self::apply_auto_size(container, opt);

        let r = container.rect;
        if !opt.has_no_frame() {
            container.draw_frame(r, ControlColor::WindowBG);
        }
        if !opt.has_no_title() {
            let mut tr: Recti = r;
            let title_text_color = container.style.as_ref().colors[ControlColor::TitleText as usize];
            tr.height = Self::titlebar_height(container);
            container.draw_frame(tr, ControlColor::TitleBG);

            let title_id = widget_id_of(title_state);
            let control_state = (title_state.opt, title_state.bopt);
            let control = container.update_control(title_id, tr, &control_state);
            {
                let mut ctx = container.widget_ctx(title_id, tr, None);
                let _ = title_state.run(&mut ctx, &control);
            }
            let name = container.name.clone(); // Necessary due to borrow checker limitations
            container.draw_control_text(&name, tr, ControlColor::TitleText, WidgetOption::NONE);
            if control.active {
                container.rect.x += container.input.borrow().mouse_delta.x;
                container.rect.y += container.input.borrow().mouse_delta.y;
            }
            if !opt.has_no_close() {
                let close_id = widget_id_of(close_state);
                let r: Recti = rect(tr.x + tr.width - tr.height, tr.y, tr.height, tr.height);
                let color = title_text_color;
                container.draw_icon(CLOSE_ICON, r, color);
                let control_state = (close_state.opt, close_state.bopt);
                let control = container.update_control(close_id, r, &control_state);
                {
                    let mut ctx = container.widget_ctx(close_id, r, None);
                    let _ = close_state.run(&mut ctx, &control);
                }
                if control.clicked {
                    *win_state = WindowState::Closed;
                }
            }
        }
        let body = Self::body_rect_for(container, opt);
        container.configure_container_body(body, bopt);

        if is_popup && container.popup_just_opened {
            // Skip the auto-close check on the same frame the popup is opened.
            container.popup_just_opened = false;
        } else if is_popup && !container.input.borrow().mouse_pressed.is_none() && !container.in_hover_root {
            *win_state = WindowState::Closed;
        }
        let body = container.body;
        container.push_clip_rect(body);
    }

    fn end_window(&mut self) {
        let container = &mut self.main;
        container.pop_clip_rect();
    }

    fn prepare_for_root_frame(&mut self, frame: usize) {
        if self.last_root_frame == Some(frame) {
            panic!("window {:?} was rendered more than once in frame {}", self.main.name, frame);
        }

        let contiguous = self.last_root_frame.and_then(|last| last.checked_add(1)) == Some(frame);
        if !contiguous {
            self.main.clear_root_frame_state();
        }
        self.main.prepare();
        self.last_root_frame = Some(frame);
    }

    fn reset_after_close(&mut self) {
        self.last_root_frame = None;
        self.main.reset();
    }

    fn measure_auto_size(&mut self, results: &FrameResults, opt: ContainerOption, bopt: WidgetBehaviourOption, tree: &WidgetTree) {
        let body = Self::body_rect_for(&self.main, opt);
        let saved_scroll = self.main.scroll;
        // Auto-size should measure desired content against the raw body rect rather than inheriting
        // last frame's scrollbar decision or scroll offset.
        self.main.scroll = Vec2i::default();
        self.main.content_size = Vec2i::default();
        self.main.configure_container_body(body, bopt);
        let content_size = self.main.measure_widget_tree_content(results, tree);
        self.main.scroll = saved_scroll;
        self.main.content_size = content_size;
    }

    fn finish_resize(&mut self, opt: ContainerOption) {
        if opt.is_auto_sizing() || opt.is_fixed() {
            return;
        }

        let container = &mut self.main;
        let sz = container.style.as_ref().title_height;
        let resize_id = widget_id_of(&self.resize_state);
        let rect = rect(
            container.rect.x + container.rect.width - sz,
            container.rect.y + container.rect.height - sz,
            sz,
            sz,
        );
        let control_state = (self.resize_state.opt, self.resize_state.bopt);
        let control = container.update_control(resize_id, rect, &control_state);
        {
            let mut ctx = container.widget_ctx(resize_id, rect, None);
            let _ = self.resize_state.run(&mut ctx, &control);
        }
        if control.active {
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
}

#[derive(Clone)]
/// Reference-counted handle to the internal window object.
pub struct WindowHandle(Rc<RefCell<Window>>);

impl WindowHandle {
    pub(crate) fn window(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        Self(Rc::new(RefCell::new(Window::window(name, atlas, style, input, initial_rect))))
    }

    pub(crate) fn dialog(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        Self(Rc::new(RefCell::new(Window::dialog(name, atlas, style, input, initial_rect))))
    }

    pub(crate) fn popup(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>) -> Self {
        Self(Rc::new(RefCell::new(Window::popup(name, atlas, style, input, Recti::new(0, 0, 0, 0)))))
    }

    /// Returns `true` if the window's state is `Open`.
    pub fn is_open(&self) -> bool {
        match self.0.borrow().win_state {
            WindowState::Open => true,
            _ => false,
        }
    }

    /// Returns the current visibility state for the window.
    pub fn state(&self) -> WindowState {
        self.0.borrow().win_state
    }

    /// Updates the window visibility state.
    ///
    /// Closing a window resets its container state so the next open starts cleanly.
    pub fn set_state(&mut self, state: WindowState) {
        let mut inner = self.inner_mut();
        inner.win_state = state;
        if matches!(state, WindowState::Closed) {
            inner.reset_after_close();
        }
    }

    /// Marks the window as open.
    pub fn open(&mut self) {
        self.set_state(WindowState::Open);
    }

    /// Marks the window as closed and resets its container state.
    pub fn close(&mut self) {
        self.set_state(WindowState::Closed);
    }

    /// Returns the current window rectangle.
    pub fn rect(&self) -> Recti {
        self.inner().main.rect
    }

    /// Replaces the current window rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.inner_mut().main.rect = rect;
    }

    /// Sets the focused widget inside the window's root container.
    pub fn set_focus(&mut self, widget_id: Option<WidgetId>) {
        self.inner_mut().main.set_focus(widget_id);
    }

    pub(crate) fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Window> {
        self.0.borrow_mut()
    }

    pub(crate) fn inner<'a>(&'a self) -> Ref<'a, Window> {
        self.0.borrow()
    }

    pub(crate) fn prepare_for_frame(&mut self, frame: usize) {
        self.inner_mut().prepare_for_root_frame(frame)
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

    pub(crate) fn begin_window(&mut self, opt: ContainerOption, bopt: WidgetBehaviourOption) {
        self.0.borrow_mut().begin_window(opt, bopt)
    }

    pub(crate) fn measure_auto_size(&mut self, results: &FrameResults, opt: ContainerOption, bopt: WidgetBehaviourOption, tree: &WidgetTree) {
        self.inner_mut().measure_auto_size(results, opt, bopt, tree)
    }

    pub(crate) fn end_window(&mut self) {
        self.inner_mut().end_window()
    }

    pub(crate) fn finish_resize(&mut self, opt: ContainerOption) {
        self.inner_mut().finish_resize(opt)
    }

    pub(crate) fn reset_after_close(&mut self) {
        self.inner_mut().reset_after_close()
    }

    /// Resizes the underlying window rectangle.
    pub fn set_size(&mut self, size: &Dimensioni) {
        self.inner_mut().main.rect.width = size.width;
        self.inner_mut().main.rect.height = size.height;
    }
}
