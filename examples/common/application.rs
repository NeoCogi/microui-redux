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
use microui_redux::*;

use sdl2::{Sdl, VideoSubsystem};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::{GLContext, GLProfile, Window};

use super::*;

type MicroUI = microui_redux::Context<(), GLRenderer>;
pub struct Application<S> {
    state: S,
    sdl_ctx: Sdl,
    _sdl_vid: VideoSubsystem,
    gl_ctx: GLContext,
    window: Window,
    ctx: MicroUI,
}

impl<S> Application<S> {
    pub fn new<F: FnOnce(&mut MicroUI) -> S>(atlas: AtlasHandle, init_state: F) -> Result<Self, String> {
        let sdl_ctx = sdl2::init().unwrap();
        let video = sdl_ctx.video().unwrap();

        let gl_attr = video.gl_attr();
        gl_attr.set_context_profile(GLProfile::GLES);
        gl_attr.set_context_version(3, 0);

        let window = video.window("Window", 800, 600).opengl().build().unwrap();

        // Unlike the other example above, nobody created a context for your window, so you need to create one.

        // save the gl context from SDL as well, otherwise, it will be dropped and the gl context is lost
        let gl_ctx = window.gl_create_context().unwrap();
        let gl = unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _) };

        debug_assert_eq!(gl_attr.context_profile(), GLProfile::GLES);
        debug_assert_eq!(gl_attr.context_version(), (3, 0));

        let (width, height) = window.size();

        let rd = GLRenderer::new(gl, atlas, width, height);

        let mut ctx = microui_redux::Context::new(rd, Dimensioni::new(width as _, height as _));
        Ok(Self {
            state: init_state(&mut ctx),
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

            self.ctx.clear(width as i32, height as i32, color(0x7F, 0x7F, 0x7F, 255));

            fn map_mouse_button(sdl_mb: sdl2::mouse::MouseButton) -> microui_redux::MouseButton {
                match sdl_mb {
                    sdl2::mouse::MouseButton::Left => microui_redux::MouseButton::LEFT,
                    sdl2::mouse::MouseButton::Right => microui_redux::MouseButton::RIGHT,
                    sdl2::mouse::MouseButton::Middle => microui_redux::MouseButton::MIDDLE,
                    _ => microui_redux::MouseButton::NONE,
                }
            }

            fn map_keymode(sdl_km: sdl2::keyboard::Mod, sdl_kc: Option<sdl2::keyboard::Keycode>) -> microui_redux::KeyMode {
                match (sdl_km, sdl_kc) {
                    (sdl2::keyboard::Mod::LALTMOD, _) | (sdl2::keyboard::Mod::RALTMOD, _) => microui_redux::KeyMode::ALT,
                    (sdl2::keyboard::Mod::LCTRLMOD, _) | (sdl2::keyboard::Mod::RCTRLMOD, _) => microui_redux::KeyMode::CTRL,
                    (sdl2::keyboard::Mod::LSHIFTMOD, _) | (sdl2::keyboard::Mod::RSHIFTMOD, _) => microui_redux::KeyMode::SHIFT,
                    (_, Some(sdl2::keyboard::Keycode::Backspace)) => microui_redux::KeyMode::BACKSPACE,
                    (_, Some(sdl2::keyboard::Keycode::Return)) => microui_redux::KeyMode::RETURN,
                    _ => microui_redux::KeyMode::NONE,
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
                    Event::KeyDown { keymod, keycode, .. } => {
                        let km = map_keymode(keymod, keycode);
                        self.ctx.input.borrow_mut().keydown(km);
                    }
                    Event::KeyUp { keymod, keycode, .. } => {
                        let km = map_keymode(keymod, keycode);
                        self.ctx.input.borrow_mut().keyup(km);
                    }
                    Event::TextInput { text, .. } => {
                        self.ctx.input.borrow_mut().text(text.as_str());
                    }

                    _ => {}
                }
            }

            f(&mut self.ctx, &mut self.state);
            self.ctx.flush();
            self.window.gl_swap_window();

            ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
        }
    }
}
