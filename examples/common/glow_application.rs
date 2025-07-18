use crate::*;
use common::*;
use microui_redux as microui;

use std::sync::Arc;

use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{Sdl, VideoSubsystem};
type MicroUI = microui_redux::Context<glow_renderer::GLRenderer>;

pub struct GlowApplication<S> {
    state: S,
    sdl_ctx: Sdl,
    _sdl_vid: VideoSubsystem,
    gl_ctx: GLContext,
    window: Window,
    ctx: MicroUI,
}

impl<S> GlowApplication<S> {
    pub fn new<F: FnMut(Arc<glow::Context>, &mut MicroUI) -> S>(atlas: AtlasHandle, mut init_state: F) -> Result<Self, String> {
        let sdl_ctx = sdl2::init().unwrap();
        let video = sdl_ctx.video().unwrap();

        let gl_attr = video.gl_attr();
        gl_attr.set_context_profile(GLProfile::GLES);
        gl_attr.set_context_version(3, 0);
        gl_attr.set_depth_size(24);

        let window = video.window("Window", 800, 600).resizable().opengl().build().unwrap();

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

            fn map_keymode(sdl_km: sdl2::keyboard::Mod, sdl_kc: Option<sdl2::keyboard::Keycode>) -> microui::KeyMode {
                match (sdl_km, sdl_kc) {
                    (sdl2::keyboard::Mod::LALTMOD, _) | (sdl2::keyboard::Mod::RALTMOD, _) => microui::KeyMode::ALT,
                    (sdl2::keyboard::Mod::LCTRLMOD, _) | (sdl2::keyboard::Mod::RCTRLMOD, _) => microui::KeyMode::CTRL,
                    (sdl2::keyboard::Mod::LSHIFTMOD, _) | (sdl2::keyboard::Mod::RSHIFTMOD, _) => microui::KeyMode::SHIFT,
                    (_, Some(sdl2::keyboard::Keycode::Backspace)) => microui::KeyMode::BACKSPACE,
                    (_, Some(sdl2::keyboard::Keycode::Return)) => microui::KeyMode::RETURN,
                    _ => microui::KeyMode::NONE,
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
            self.ctx.end();
            self.window.gl_swap_window();

            ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
        }
    }
}
