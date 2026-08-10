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
//! Shared SDL application runner for examples.
//!
//! This module owns window/event setup, backend selection, input forwarding, and the example main
//! loop used by retained-mode demos.
use crate::*;
use common::*;
use microui_redux::{
    self as microui,
    prelude::{AtlasHandle, Dimensioni, FrameInfo, Session},
};

#[cfg(feature = "example-glow")]
use std::sync::Arc;

#[cfg(feature = "example-glow")]
use crate::common::glow_renderer;
#[cfg(all(not(feature = "example-glow"), feature = "example-vulkan"))]
use crate::common::vulkan_renderer;
#[cfg(all(not(feature = "example-glow"), not(feature = "example-vulkan"), feature = "example-wgpu"))]
use crate::common::wgpu_renderer;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
#[cfg(feature = "example-glow")]
use sdl2::video::{GLContext, GLProfile, SwapInterval};
use sdl2::video::Window;
use sdl2::{Sdl, VideoSubsystem};

#[cfg(feature = "example-glow")]
type SelectedBackend = glow_renderer::GLRenderer;
#[cfg(all(not(feature = "example-glow"), feature = "example-vulkan"))]
type SelectedBackend = vulkan_renderer::VulkanRenderer;
#[cfg(all(not(feature = "example-glow"), not(feature = "example-vulkan"), feature = "example-wgpu"))]
type SelectedBackend = wgpu_renderer::WgpuRenderer;

// The example app keeps one concrete renderer backend behind the shared microui `Context`. The
// rest of the example code only talks to `MicroUI`, while backend initialization stays feature-
// gated in this file.
type MicroUI = microui::Context<SelectedBackend>;

#[cfg(feature = "example-glow")]
pub type BackendInitContext = Arc<glow::Context>;
#[cfg(any(
    all(not(feature = "example-glow"), feature = "example-vulkan"),
    all(not(feature = "example-glow"), not(feature = "example-vulkan"), feature = "example-wgpu"),
))]
pub struct BackendInitContext;

pub struct Application<S> {
    // Drop renderer-backed state before the SDL window/subsystem. Vulkan and wgpu surfaces
    // borrow native window resources, so the window must outlive `state`, `ctx`, and backend data.
    state: S,
    ctx: MicroUI,
    #[cfg(feature = "example-glow")]
    backend: BackendData,
    window: Window,
    _sdl_vid: VideoSubsystem,
    sdl_ctx: Sdl,
}

impl<S> Application<S> {
    /// Creates the example application by initializing SDL, the chosen backend, and user state.
    pub fn new<F: FnMut(BackendInitContext, &mut MicroUI) -> S>(atlas: AtlasHandle, mut init_state: F) -> Result<Self, String> {
        // SDL/video/window setup is backend-dependent, but state construction always receives a
        // ready-to-use microui `Context` plus any backend-specific initialization payload.
        let sdl_ctx = sdl2::init().map_err(|err| err.to_string())?;
        let video = sdl_ctx.video().map_err(|err| err.to_string())?;
        let (bundle, init_ctx) = init_backend(&video, atlas)?;
        #[cfg(feature = "example-glow")]
        let BackendBundle { window, backend, renderer } = bundle;
        #[cfg(any(
            all(not(feature = "example-glow"), feature = "example-vulkan"),
            all(not(feature = "example-glow"), not(feature = "example-vulkan"), feature = "example-wgpu"),
        ))]
        let BackendBundle { window, renderer } = bundle;

        let mut ctx = microui::Context::new(renderer);
        Ok(Self {
            state: init_state(init_ctx, &mut ctx),
            ctx,
            #[cfg(feature = "example-glow")]
            backend,
            window,
            _sdl_vid: video,
            sdl_ctx,
        })
    }

    /// Runs the SDL event loop, forwarding input into microui and invoking the user frame callback.
    #[allow(dead_code)] // Each example selects either polling or subscriber-driven updates.
    pub fn event_loop<F: Fn(&mut MicroUI, &mut S, Dimensioni)>(&mut self, f: F) {
        self.event_loop_with_update(|ctx, _state, dimensions| ctx.update_ui(dimensions), f);
    }

    /// Runs the SDL loop with one application-state event session.
    #[allow(dead_code)] // Each example selects either polling or subscriber-driven updates.
    pub fn event_loop_session<Setup, F>(&mut self, setup: Setup, f: F)
    where
        S: 'static,
        Setup: FnOnce(&S, &mut Session<S>),
        F: Fn(&mut MicroUI, &mut S, Dimensioni),
    {
        let mut session = Session::new();
        setup(&self.state, &mut session);
        self.event_loop_with_update(move |ctx, state, dimensions| ctx.update_ui_session(dimensions, &mut session, state), f);
    }

    /// Shared SDL driver parameterized by the first retained update performed each frame.
    fn event_loop_with_update<Update, F>(&mut self, mut update: Update, f: F)
    where
        Update: FnMut(&mut MicroUI, &mut S, Dimensioni),
        F: Fn(&mut MicroUI, &mut S, Dimensioni),
    {
        #[cfg(feature = "example-glow")]
        {
            self.window.gl_make_current(&self.backend.gl_ctx).unwrap();
            // The shared runner otherwise renders without a scheduling boundary on drivers whose
            // default swap interval is immediate. That can starve SDL event delivery in debug
            // examples and make an already-queued click appear delayed.
            let _ = self._sdl_vid.gl_set_swap_interval(SwapInterval::VSync);
        }

        let mut event_pump = self.sdl_ctx.event_pump().unwrap();
        'running: loop {
            let (width, height) = self.window.size();

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
                    Some(sdl2::keyboard::Keycode::Delete) => microui::KeyMode::DELETE,
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
                    Some(sdl2::keyboard::Keycode::Delete) => microui::KeyCode::DELETE,
                    Some(sdl2::keyboard::Keycode::End) => microui::KeyCode::END,
                    Some(sdl2::keyboard::Keycode::Up) => microui::KeyCode::UP,
                    Some(sdl2::keyboard::Keycode::Down) => microui::KeyCode::DOWN,
                    Some(sdl2::keyboard::Keycode::Left) => microui::KeyCode::LEFT,
                    Some(sdl2::keyboard::Keycode::Right) => microui::KeyCode::RIGHT,
                    _ => microui::KeyCode::NONE,
                }
            }
            // SDL events are translated into the narrower microui input vocabulary here. This
            // keeps the rest of the demo code backend-agnostic.
            let mut pending_mouse_move = None;
            for event in event_pump.poll_iter() {
                // SDL can produce many consecutive motion samples between rendered frames. The
                // retained Context intentionally commits every event independently, so forwarding
                // all of those samples creates a queue of full update/layout transactions ahead
                // of a later click. Preserve the last position and flush it before any non-motion
                // event, keeping observable ordering while bounding motion work per host batch.
                let event = match event {
                    Event::MouseMotion { x, y, .. } => {
                        pending_mouse_move = Some((x, y));
                        continue;
                    }
                    event => event,
                };
                if let Some((x, y)) = pending_mouse_move.take() {
                    self.ctx.mousemove(x, y);
                }
                match event {
                    Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,
                    Event::Window { win_event: WindowEvent::Close, .. } => break 'running,
                    Event::MouseWheel { x, y, .. } => self.ctx.scroll(x * -30, y * -30),
                    Event::MouseButtonDown { x, y, mouse_btn, .. } => {
                        let mb = map_mouse_button(mouse_btn);
                        self.ctx.mousedown(x, y, mb);
                    }
                    Event::MouseButtonUp { x, y, mouse_btn, .. } => {
                        let mb = map_mouse_button(mouse_btn);
                        self.ctx.mouseup(x, y, mb);
                    }
                    Event::KeyDown { keycode, .. } => {
                        let km = map_keymode(keycode);
                        if !km.is_empty() {
                            self.ctx.keydown(km);
                        }
                        let kc = map_keycode(keycode);
                        if !kc.is_empty() {
                            self.ctx.keydown_code(kc);
                        }
                    }
                    Event::KeyUp { keycode, .. } => {
                        let km = map_keymode(keycode);
                        if !km.is_empty() {
                            self.ctx.keyup(km);
                        }
                        let kc = map_keycode(keycode);
                        if !kc.is_empty() {
                            self.ctx.keyup_code(kc);
                        }
                    }
                    Event::TextInput { text, .. } => {
                        self.ctx.text(text.as_str());
                    }

                    _ => {}
                }
            }
            if let Some((x, y)) = pending_mouse_move {
                self.ctx.mousemove(x, y);
            }

            let dimensions = Dimensioni::new(width as i32, height as i32);
            if let Ok(info) = FrameInfo::try_new(dimensions, color(0x7F, 0x7F, 0x7F, 255)) {
                // First commit queued host input so application polling observes this frame's
                // completed widget and Context-owned dialog actions.
                update(&mut self.ctx, &mut self.state, dimensions);
                f(&mut self.ctx, &mut self.state, dimensions);
                // Application mutations can affect retained state and layout, so synchronize once
                // more before the paint-only frame is submitted.
                self.ctx.update_ui(dimensions);
                if let Err(error) = self.ctx.frame(info).render_ui() {
                    eprintln!("[microui-redux][example] frame failed: {error}");
                }
            }
            #[cfg(feature = "example-glow")]
            self.window.gl_swap_window();

            //::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
        }
    }
}

#[cfg(feature = "example-glow")]
/// Initializes the OpenGL example backend and returns the window, renderer, and GL init context.
fn init_backend(video: &VideoSubsystem, atlas: AtlasHandle) -> Result<(BackendBundle, BackendInitContext), String> {
    // The GL example owns an explicit SDL GL context in addition to the microui renderer.
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(GLProfile::GLES);
    gl_attr.set_context_version(3, 0);
    gl_attr.set_depth_size(24);

    let window = video.window("Window", 1024, 768).resizable().opengl().build().map_err(|err| err.to_string())?;
    let gl_ctx = window.gl_create_context().map_err(|err| err.to_string())?;
    window.gl_make_current(&gl_ctx).map_err(|err| err.to_string())?;

    let gl = unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _) };
    debug_assert_eq!(gl_attr.context_profile(), GLProfile::GLES);
    debug_assert_eq!(gl_attr.context_version(), (3, 0));

    let (width, height) = window.size();
    let gl = Arc::new(gl);
    let renderer = glow_renderer::GLRenderer::new(gl.clone(), atlas, width, height)?;

    Ok((
        BackendBundle {
            window,
            backend: BackendData { gl_ctx },
            renderer,
        },
        gl,
    ))
}

#[cfg(all(not(feature = "example-glow"), feature = "example-vulkan"))]
/// Initializes the Vulkan example backend and returns the window, renderer, and init marker.
fn init_backend(video: &VideoSubsystem, atlas: AtlasHandle) -> Result<(BackendBundle, BackendInitContext), String> {
    // Vulkan and wgpu derive their native surfaces from the SDL window itself, so the renderer is
    // created immediately from that window handle and then stored inside the microui `Context`.
    let window = video.window("Window", 1024, 768).resizable().vulkan().build().map_err(|err| err.to_string())?;
    let (width, height) = window.size();
    let renderer = vulkan_renderer::VulkanRenderer::new(&window, atlas, width, height)?;
    let init_ctx = BackendInitContext;

    Ok((BackendBundle { window, renderer }, init_ctx))
}

#[cfg(all(not(feature = "example-glow"), not(feature = "example-vulkan"), feature = "example-wgpu"))]
/// Initializes the wgpu example backend and returns the window, renderer, and init marker.
fn init_backend(video: &VideoSubsystem, atlas: AtlasHandle) -> Result<(BackendBundle, BackendInitContext), String> {
    let window = video.window("Window", 1024, 768).resizable().build().map_err(|err| err.to_string())?;
    let (width, height) = window.size();
    let renderer = wgpu_renderer::WgpuRenderer::new(&window, atlas, width, height)?;
    let init_ctx = BackendInitContext;

    Ok((BackendBundle { window, renderer }, init_ctx))
}

struct BackendBundle {
    window: Window,
    #[cfg(feature = "example-glow")]
    backend: BackendData,
    renderer: SelectedBackend,
}

#[cfg(feature = "example-glow")]
struct BackendData {
    gl_ctx: GLContext,
}
