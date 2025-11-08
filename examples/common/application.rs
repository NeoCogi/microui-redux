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
use crate::*;
use common::*;
use microui_redux as microui;

use std::sync::Arc;

use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{Sdl, VideoSubsystem};
type MicroUI = microui_redux::Context<glow_renderer::GLRenderer>;

pub struct Application<S> {
    state: S,
    sdl_ctx: Sdl,
    _sdl_vid: VideoSubsystem,
    gl_ctx: GLContext,
    window: Window,
    ctx: MicroUI,
}

impl<S> Application<S> {
    pub fn new<F: FnMut(Arc<glow::Context>, &mut MicroUI) -> S>(atlas: AtlasHandle, mut init_state: F) -> Result<Self, String> {
        let sdl_ctx = sdl2::init().unwrap();
        let video = sdl_ctx.video().unwrap();

        let gl_attr = video.gl_attr();
        gl_attr.set_context_profile(GLProfile::GLES);
        gl_attr.set_context_version(3, 0);
        gl_attr.set_depth_size(24);

        let window = video.window("Window", 800, 600).resizable().opengl().build().unwrap();

        // Unlike the other example above, nobody created a context for your window, so you need to create one.

        // save the gl context from SDL as well, otherwise, it will be dropped and the gl context is lost
        let gl_ctx = window.gl_create_context().unwrap();
        let gl = unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _) };

        debug_assert_eq!(gl_attr.context_profile(), GLProfile::GLES);
        debug_assert_eq!(gl_attr.context_version(), (3, 0));

        let (width, height) = window.size();

        window.gl_make_current(&gl_ctx).unwrap();
        let gl = Arc::new(gl);
        let rd = RendererHandle::new(glow_renderer::GLRenderer::new(gl.clone(), atlas, width, height));

        let mut ctx = microui::Context::new(rd, Dimensioni::new(width as _, height as _));
        Ok(Self {
            state: init_state(gl, &mut ctx),
            sdl_ctx,
            _sdl_vid: video,
            gl_ctx,
            window,
            ctx,
        })
    }

    pub fn event_loop<F: Fn(&mut MicroUI, &mut S)>(&mut self, f: F) {
        self.window.gl_make_current(&self.gl_ctx).unwrap();

        let mut event_pump = self.sdl_ctx.event_pump().unwrap();
        'running: loop {
            let (width, height) = self.window.size();

            self.ctx.begin(width as i32, height as i32, color(0x7F, 0x7F, 0x7F, 255));

            fn map_mouse_button(sdl_mb: sdl2::mouse::MouseButton) -> microui::MouseButton {
                match sdl_mb {
                    sdl2::mouse::MouseButton::Left => microui::MouseButton::LEFT,
                    sdl2::mouse::MouseButton::Right => microui::MouseButton::RIGHT,
                    sdl2::mouse::MouseButton::Middle => microui::MouseButton::MIDDLE,
                    _ => microui::MouseButton::NONE,
                }
            }

            fn map_keymode(sdl_kc: Option<sdl2::keyboard::Keycode>) -> microui::KeyMode {
                match sdl_kc {
                    Some(sdl2::keyboard::Keycode::Backspace) => microui::KeyMode::BACKSPACE,
                    Some(sdl2::keyboard::Keycode::Return) => microui::KeyMode::RETURN,
                    Some(sdl2::keyboard::Keycode::LAlt) | Some(sdl2::keyboard::Keycode::RAlt) => microui::KeyMode::ALT,
                    Some(sdl2::keyboard::Keycode::LCtrl) | Some(sdl2::keyboard::Keycode::RCtrl) => microui::KeyMode::CTRL,
                    Some(sdl2::keyboard::Keycode::LShift) | Some(sdl2::keyboard::Keycode::RShift) => microui::KeyMode::SHIFT,
                    _ => microui::KeyMode::NONE,
                }
            }

            fn map_keycode(sdl_kc: Option<sdl2::keyboard::Keycode>) -> microui::KeyCode {
                match sdl_kc {
                    Some(sdl2::keyboard::Keycode::Up) => microui::KeyCode::UP,
                    Some(sdl2::keyboard::Keycode::Down) => microui::KeyCode::DOWN,
                    Some(sdl2::keyboard::Keycode::Left) => microui::KeyCode::LEFT,
                    Some(sdl2::keyboard::Keycode::Right) => microui::KeyCode::RIGHT,
                    _ => microui::KeyCode::NONE,
                }
            }
            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,
                    Event::Window { win_event: WindowEvent::Close, .. } => break 'running,
                    Event::MouseMotion { x, y, .. } => self.ctx.input.borrow_mut().mousemove(x, y),
                    Event::MouseWheel { y, .. } => self.ctx.input.borrow_mut().scroll(0, y * -30),
                    Event::MouseButtonDown { x, y, mouse_btn, .. } => {
                        let mb = map_mouse_button(mouse_btn);
                        self.ctx.input.borrow_mut().mousedown(x, y, mb);
                    }
                    Event::MouseButtonUp { x, y, mouse_btn, .. } => {
                        let mb = map_mouse_button(mouse_btn);
                        self.ctx.input.borrow_mut().mouseup(x, y, mb);
                    }
                    Event::KeyDown { keycode, .. } => {
                        let km = map_keymode(keycode);
                        if !km.is_none() {
                            self.ctx.input.borrow_mut().keydown(km);
                        }
                        let kc = map_keycode(keycode);
                        if !kc.is_none() {
                            self.ctx.input.borrow_mut().keydown_code(kc);
                        }
                    }
                    Event::KeyUp { keycode, .. } => {
                        let km = map_keymode(keycode);
                        if !km.is_none() {
                            self.ctx.input.borrow_mut().keyup(km);
                        }
                        let kc = map_keycode(keycode);
                        if !kc.is_none() {
                            self.ctx.input.borrow_mut().keyup_code(kc);
                        }
                    }
                    Event::TextInput { text, .. } => {
                        self.ctx.input.borrow_mut().text(text.as_str());
                    }

                    _ => {}
                }
            }

            f(&mut self.ctx, &mut self.state);
            self.ctx.end();
            self.window.gl_swap_window();

            ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
        }
    }
}

pub fn atlas_config(slots: &Vec<Dimensioni>) -> builder::Config {
    builder::Config {
        texture_height: 256,
        texture_width: 256,
        white_icon: String::from("assets/WHITE.png"),
        close_icon: String::from("assets/CLOSE.png"),
        expand_icon: String::from("assets/PLUS.png"),
        collapse_icon: String::from("assets/MINUS.png"),
        check_icon: String::from("assets/CHECK.png"),
        default_font: String::from("assets/NORMAL.ttf"),
        default_font_size: 12,
        slots,
    }
}
