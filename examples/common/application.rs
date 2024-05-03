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
use miniquad::{conf, window, EventHandler, RenderingBackend};

use super::*;

type MicroUI = microui_redux::Context<(), MQRenderer>;
struct Application<S> {
    state: Box<S>,
    ctx: MicroUI,
    update: Box<dyn FnMut(&mut MicroUI, &mut Box<S>)>,
}

fn map_mouse_button(mb: miniquad::MouseButton) -> microui_redux::MouseButton {
    match mb {
        miniquad::MouseButton::Left => microui_redux::MouseButton::LEFT,
        miniquad::MouseButton::Right => microui_redux::MouseButton::RIGHT,
        miniquad::MouseButton::Middle => microui_redux::MouseButton::MIDDLE,
        _ => microui_redux::MouseButton::NONE,
    }
}

fn map_keymode(km: miniquad::KeyMods, kc: miniquad::KeyCode) -> microui_redux::KeyMode {
    match (km, kc) {
        (_, miniquad::KeyCode::LeftAlt) | (_, miniquad::KeyCode::RightAlt) => microui_redux::KeyMode::ALT,
        (_, miniquad::KeyCode::LeftControl) | (_, miniquad::KeyCode::RightControl) => microui_redux::KeyMode::CTRL,
        (_, miniquad::KeyCode::LeftShift) | (_, miniquad::KeyCode::RightShift) => microui_redux::KeyMode::SHIFT,
        (_, miniquad::KeyCode::Backspace) => microui_redux::KeyMode::BACKSPACE,
        (_, miniquad::KeyCode::Enter) => microui_redux::KeyMode::RETURN,
        _ => microui_redux::KeyMode::NONE,
    }
}

impl<S> EventHandler for Application<S> {
    fn update(&mut self) {}

    fn draw(&mut self) {
        let (width, height) = (conf::Conf::default().window_width, conf::Conf::default().window_height);

        self.ctx.begin(width, height, color(0x7F, 0x7F, 0x7F, 0xFF));
        (self.update)(&mut self.ctx, &mut self.state);

        self.ctx.end();

        ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
    }

    fn key_down_event(&mut self, keycode: miniquad::KeyCode, keymod: miniquad::KeyMods, _repeat: bool) {
        match keycode {
            miniquad::KeyCode::Escape => miniquad::window::order_quit(),
            keycode => {
                let km = map_keymode(keymod, keycode);
                self.ctx.input.borrow_mut().keydown(km);
            }
        }
    }

    fn key_up_event(&mut self, keycode: miniquad::KeyCode, keymod: miniquad::KeyMods) {
        let km = map_keymode(keymod, keycode);
        self.ctx.input.borrow_mut().keyup(km);
    }

    fn char_event(&mut self, character: char, _keymods: miniquad::KeyMods, _repeat: bool) {
        self.ctx.input.borrow_mut().text(&character.to_string());
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.ctx.input.borrow_mut().mousemove(x as _, y as _);
    }

    fn mouse_wheel_event(&mut self, _x: f32, y: f32) {
        self.ctx.input.borrow_mut().scroll(0, (y * -30.0) as _);
    }

    fn mouse_button_down_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        let mb = map_mouse_button(button);
        self.ctx.input.borrow_mut().mousedown(x as _, y as _, mb);
    }

    fn mouse_button_up_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        let mb = map_mouse_button(button);
        self.ctx.input.borrow_mut().mouseup(x as _, y as _, mb);
    }
}
pub fn start<S: 'static, I: FnOnce(&mut MicroUI) -> S + 'static, U: FnMut(&mut MicroUI, &mut Box<S>) + 'static>(
    atlas: AtlasHandle,
    init_state: I,
    mut update_function: U,
) {
    let mut conf = conf::Conf::default();
    let metal = std::env::args().nth(1).as_deref() == Some("metal");
    conf.platform.apple_gfx_api = if metal { conf::AppleGfxApi::Metal } else { conf::AppleGfxApi::OpenGl };
    let width = conf.window_width;
    let height = conf.window_height;

    miniquad::start(conf, move || {
        let ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        let rd = MQRenderer::new(ctx, atlas, width as _, height as _);
        let mut mui = microui_redux::Context::new(rd, Dimensioni::new(width as _, height as _));
        let application = Application {
            update: Box::new(move |ctx, state| update_function(ctx, state)),
            state: Box::new(init_state(&mut mui)),
            ctx: mui,
        };

        Box::new(application)
    });
}
