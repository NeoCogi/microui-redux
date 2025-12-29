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
use std::{cell::RefCell, rc::Rc};

#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;

#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{ColorType, Decoder};

use crate::{
    rect, Canvas, Color, Container, ContainerHandle, ContainerOption, Dimensioni, ImageSource, Input, Recti, Renderer,
    RendererHandle, Style, TextureId, UNCLIPPED_RECT, Vec2i, WidgetBehaviourOption, WindowHandle, WindowState,
};

/// Primary entry point used to drive the UI over a renderer implementation.
pub struct Context<R: Renderer> {
    canvas: Canvas<R>,
    style: Rc<Style>,

    last_zindex: i32,
    frame: usize,
    hover_root: Option<WindowHandle>,
    next_hover_root: Option<WindowHandle>,
    scroll_target: Option<WindowHandle>,

    root_list: Vec<WindowHandle>,

    /// Shared pointer to the input state driving this context.
    pub input: Rc<RefCell<Input>>,
}

impl<R: Renderer> Context<R> {
    /// Creates a new UI context around the provided renderer and dimensions.
    pub fn new(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        Self {
            canvas: Canvas::from(renderer, dim),
            style: Rc::new(Style::default()),
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
    /// Begins a new draw pass on the underlying canvas.
    pub fn begin(&mut self, width: i32, height: i32, clr: Color) { self.canvas.begin(width, height, clr); }

    /// Flushes recorded draw commands to the renderer and ends the draw pass.
    pub fn end(&mut self) {
        for r in &mut self.root_list {
            r.render(&mut self.canvas);
        }
        self.canvas.end()
    }

    /// Returns a handle to the underlying renderer.
    pub fn renderer_handle(&self) -> RendererHandle<R> { self.canvas.renderer_handle() }

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

    /// Runs the UI for a single frame by wrapping input/layout bookkeeping.
    /// Rendering still requires calling [`Context::begin`] and [`Context::end`].
    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.frame_begin();

        // execute the frame function
        f(self);

        self.frame_end();
    }

    /// Creates a new movable window rooted at the provided rectangle.
    pub fn new_window(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let mut window = WindowHandle::window(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect);
        self.bring_to_front(&mut window);
        window
    }

    /// Creates a modal dialog window.
    pub fn new_dialog(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        WindowHandle::dialog(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect)
    }

    /// Creates a popup window that appears under the mouse cursor.
    pub fn new_popup(&mut self, name: &str) -> WindowHandle { WindowHandle::popup(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone()) }

    /// Creates a standalone panel that can be embedded inside other windows.
    pub fn new_panel(&mut self, name: &str) -> ContainerHandle {
        ContainerHandle::new(Container::new(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone()))
    }

    /// Bumps the window's Z order so it renders above others.
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
        let scroll_delta = self.input.borrow().scroll_delta;
        let pending_scroll = if container.in_hover_root && (scroll_delta.x != 0 || scroll_delta.y != 0) {
            Some(scroll_delta)
        } else {
            None
        };
        container.seed_pending_scroll(pending_scroll);
        container.clip_stack.push(UNCLIPPED_RECT);
    }

    #[inline(never)]
    fn end_root_container(&mut self, window: &mut WindowHandle) {
        let container = &mut window.inner_mut().main;
        container.pop_clip_rect();

        let layout_body = container.layout.current_body();
        match container.layout.current_max() {
            None => (),
            Some(lm) => container.content_size = Vec2i::new(lm.x - layout_body.x, lm.y - layout_body.y),
        }
        container.consume_pending_scroll();
        container.layout.pop_scope();
    }

    #[inline(never)]
    #[must_use]
    fn begin_window(&mut self, window: &mut WindowHandle, opt: ContainerOption, bopt: WidgetBehaviourOption) -> bool {
        if !window.is_open() {
            return false;
        }

        self.begin_root_container(window);
        window.begin_window(opt, bopt);

        true
    }

    fn end_window(&mut self, window: &mut WindowHandle) {
        window.end_window();
        self.end_root_container(window);
    }

    /// Opens a window, executes the provided UI builder, and closes the window.
    pub fn window<F: FnOnce(&mut Container) -> WindowState>(
        &mut self,
        window: &mut WindowHandle,
        opt: ContainerOption,
        bopt: WidgetBehaviourOption,
        f: F,
    ) {
        // call the window function if the window is open
        if self.begin_window(window, opt, bopt) {
            window.inner_mut().main.style = self.style.clone();
            let state = f(&mut window.inner_mut().main);
            self.end_window(window);
            if window.is_open() {
                window.inner_mut().win_state = state;
            }

            // in case the window needs to be reopened, reset all states
            if !window.is_open() {
                window.inner_mut().main.reset();
            }
        }
    }

    /// Marks a dialog window as open for the next frame.
    pub fn open_dialog(&mut self, window: &mut WindowHandle) { window.inner_mut().win_state = WindowState::Open; }

    /// Renders a dialog window if it is currently open.
    pub fn dialog<F: FnOnce(&mut Container) -> WindowState>(
        &mut self,
        window: &mut WindowHandle,
        opt: ContainerOption,
        bopt: WidgetBehaviourOption,
        f: F,
    ) {
        if window.is_open() {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            window.inner_mut().main.in_hover_root = true;
            self.bring_to_front(window);

            self.window(window, opt, bopt, f);
        }
    }

    /// Shows a popup at the mouse cursor position.
    pub fn open_popup(&mut self, window: &mut WindowHandle) {
        let was_open = window.is_open();
        let mouse_pos = self.input.borrow().mouse_pos;
        {
            let mut inner = window.inner_mut();
            if was_open {
                let mut rect = inner.main.rect;
                rect.x = mouse_pos.x;
                rect.y = mouse_pos.y;
                inner.main.rect = rect;
            } else {
                inner.main.rect = rect(mouse_pos.x, mouse_pos.y, 1, 1);
                inner.win_state = WindowState::Open;
                inner.main.in_hover_root = true;
                inner.main.popup_just_opened = true;
            }
        }
        if !was_open {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            self.bring_to_front(window);
        }
    }

    /// Shows a popup anchored at the provided rectangle instead of the mouse cursor.
    pub fn open_popup_at(&mut self, window: &mut WindowHandle, anchor: Recti) {
        let was_open = window.is_open();
        {
            let mut inner = window.inner_mut();
            if was_open {
                let mut rect = inner.main.rect;
                rect.x = anchor.x;
                rect.y = anchor.y;
                rect.width = anchor.width;
                inner.main.rect = rect;
            } else {
                inner.main.rect = anchor;
                inner.win_state = WindowState::Open;
                inner.main.in_hover_root = true;
                inner.main.popup_just_opened = true;
            }
        }
        if !was_open {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            self.bring_to_front(window);
        }
    }

    /// Opens a popup window with default options.
    pub fn popup<F: FnOnce(&mut Container) -> WindowState>(
        &mut self,
        window: &mut WindowHandle,
        bopt: WidgetBehaviourOption,
        f: F,
    ) {
        let opt = ContainerOption::AUTO_SIZE | ContainerOption::NO_RESIZE | ContainerOption::NO_TITLE;
        self.window(window, opt, bopt, f);
    }

    /// Replaces the current UI style.
    pub fn set_style(&mut self, style: &Style) { self.style = Rc::new(style.clone()) }

    /// Returns the underlying canvas used for rendering.
    pub fn canvas(&self) -> &Canvas<R> { &self.canvas }

    /// Uploads an RGBA image to the renderer and returns its [`TextureId`].
    pub fn load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId { self.canvas.load_texture_rgba(width, height, pixels) }

    /// Deletes a previously uploaded texture.
    pub fn free_image(&mut self, id: TextureId) { self.canvas.free_texture(id); }

    /// Uploads texture data described by `source`. PNG decoding is only available when the
    /// `png_source` (or `builder`) feature is enabled.
    pub fn load_image_from(&mut self, source: ImageSource) -> Result<TextureId, String> {
        match source {
            ImageSource::Raw { width, height, pixels } => {
                Self::assert_rgba_len(width, height, pixels.len())?;
                Ok(self.load_image_rgba(width, height, pixels))
            }
            #[cfg(any(feature = "builder", feature = "png_source"))]
            ImageSource::Png { bytes } => {
                let (width, height, rgba) = Self::decode_png(bytes)?;
                Ok(self.load_image_rgba(width, height, rgba.as_slice()))
            }
        }
    }

    fn assert_rgba_len(width: i32, height: i32, len: usize) -> Result<(), String> {
        if width <= 0 || height <= 0 {
            return Err(String::from("Image dimensions must be positive"));
        }
        let expected = width as usize * height as usize * 4;
        if len != expected {
            return Err(format!("Expected {} RGBA bytes, received {}", expected, len));
        }
        Ok(())
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    fn decode_png(bytes: &[u8]) -> Result<(i32, i32, Vec<u8>), String> {
        let cursor = Cursor::new(bytes);
        let decoder = Decoder::new(cursor);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let buf_size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG decoder did not report output size".to_string())?;
        let mut buf = vec![0; buf_size];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let raw = &buf[..info.buffer_size()];
        let mut rgba = Vec::with_capacity((info.width as usize) * (info.height as usize) * 4);
        match info.color_type {
            ColorType::Rgba => rgba.extend_from_slice(raw),
            ColorType::Rgb => {
                for chunk in raw.chunks(3) {
                    rgba.extend_from_slice(chunk);
                    rgba.push(0xFF);
                }
            }
            ColorType::Grayscale => {
                for &v in raw {
                    rgba.extend_from_slice(&[v, v, v, 0xFF]);
                }
            }
            ColorType::GrayscaleAlpha => {
                for chunk in raw.chunks(2) {
                    let v = chunk[0];
                    let a = chunk[1];
                    rgba.extend_from_slice(&[v, v, v, a]);
                }
            }
            _ => {
                return Err("Unsupported PNG color type".into());
            }
        }
        Ok((info.width as i32, info.height as i32, rgba))
    }
}
