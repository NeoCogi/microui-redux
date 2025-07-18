// VulkanApplication for microui-redux
use crate::*;
use common::*;
use microui_redux as microui;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::Window;
use sdl2::{Sdl, VideoSubsystem};
use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;
use std::sync::Arc;
use vulkan_renderer::VulkanRenderer;
use ash::vk;
use ash::vk::Handle;

type MicroUI = microui_redux::Context<VulkanRenderer>;

pub struct VulkanApplication<S> {
    state: S,
    sdl_ctx: Sdl,
    _sdl_vid: VideoSubsystem,
    window: Window,
    ctx: MicroUI,
    surface: vk::SurfaceKHR,
}

impl<S> VulkanApplication<S> {
    pub fn new<F: FnMut(&mut MicroUI) -> S>(atlas: AtlasHandle, mut init_state: F) -> Result<Self, String> {
        let sdl_ctx = sdl2::init().unwrap();
        let video = sdl_ctx.video().unwrap();
        let window = video.window("Window", 800, 600).resizable().vulkan().build().unwrap();
        let (width, height) = window.size();

        // Create Vulkan instance (to get surface)
        let entry = unsafe { ash::Entry::load().expect("Failed to load Vulkan entry") };
        // Query SDL2 for required Vulkan instance extensions
        let sdl_extensions = window.vulkan_instance_extensions().unwrap();
        let extension_ptrs: Vec<*const i8> = sdl_extensions.iter().map(|ext| ext.as_ptr() as *const i8).collect();

        let app_name = CString::new("microui-redux").unwrap();
        let engine_name = CString::new("microui-redux").unwrap();
        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name.as_c_str())
            .application_version(0)
            .engine_name(engine_name.as_c_str())
            .engine_version(0)
            .api_version(vk::API_VERSION_1_0);
        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_ptrs);
        let instance = unsafe { entry.create_instance(&create_info, None).unwrap() };

        // Create Vulkan surface from SDL2 window
        let raw_surface = window.vulkan_create_surface(instance.handle().as_raw() as usize).unwrap();
        let surface = vk::SurfaceKHR::from_raw(raw_surface);

        // Create VulkanRenderer with surface
        let rd = RendererHandle::new(VulkanRenderer::new_with_surface(atlas, entry, instance, surface));
        let mut ctx = microui::Context::new(rd, Dimensioni::new(width as _, height as _));
        Ok(Self {
            state: init_state(&mut ctx),
            sdl_ctx,
            _sdl_vid: video,
            window,
            ctx,
            surface,
        })
    }

    pub fn event_loop<F: Fn(&mut MicroUI, &mut S)>(&mut self, mut f: F) {
        let mut event_pump = self.sdl_ctx.event_pump().unwrap();
        'running: loop {
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
            // Present the frame (clear and present)
            self.ctx.renderer_scope_mut(|r| r.present());
        }
    }
}
