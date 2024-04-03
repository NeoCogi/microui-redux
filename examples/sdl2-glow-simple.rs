extern crate sdl2;
#[path = "./common/mod.rs"]
mod common;

use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::GLProfile;
use common::{atlas_config, GLRenderer};
use microui_redux::*;

struct State {
    window: WindowHandle<()>,
}
fn main() {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let gl_attr = video_subsystem.gl_attr();
    gl_attr.set_context_profile(GLProfile::GLES);
    gl_attr.set_context_version(3, 0);

    let window = video_subsystem.window("Window", 800, 600).opengl().build().unwrap();

    // Unlike the other example above, nobody created a context for your window, so you need to create one.

    // TODO: the rust compiler optimizes this out
    let _x_ = window.gl_create_context().unwrap();
    let gl = unsafe { glow::Context::from_loader_function(|s| video_subsystem.gl_get_proc_address(s) as *const _) };

    debug_assert_eq!(gl_attr.context_profile(), GLProfile::GLES);
    debug_assert_eq!(gl_attr.context_version(), (3, 0));

    let mut event_pump = sdl_context.event_pump().unwrap();
    let (width, height) = window.size();
    let slots = vec![Dimensioni::new(64, 64), Dimensioni::new(24, 32), Dimensioni::new(64, 24)];
    let atlas = builder::Builder::from_config(&atlas_config(&slots)).unwrap().to_atlas();
    let rd = GLRenderer::new(gl, atlas, width, height);

    let mut ctx = microui_redux::Context::new(rd, Dimensioni::new(width as _, height as _));
    let state = State {
        window: ctx.new_window("Hello Window", rect(40, 40, 300, 450)),
    };

    'running: loop {
        let (width, height) = window.size();

        ctx.clear(width as i32, height as i32, color(0x7F, 0x7F, 0x7F, 255));

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
                Event::MouseMotion { x, y, .. } => ctx.input.borrow_mut().mousemove(x, y),
                Event::MouseWheel { y, .. } => ctx.input.borrow_mut().scroll(0, y * -30),
                Event::MouseButtonDown { x, y, mouse_btn, .. } => {
                    let mb = map_mouse_button(mouse_btn);
                    ctx.input.borrow_mut().mousedown(x, y, mb);
                }
                Event::MouseButtonUp { x, y, mouse_btn, .. } => {
                    let mb = map_mouse_button(mouse_btn);
                    ctx.input.borrow_mut().mouseup(x, y, mb);
                }
                Event::KeyDown { keymod, keycode, .. } => {
                    let km = map_keymode(keymod, keycode);
                    ctx.input.borrow_mut().keydown(km);
                }
                Event::KeyUp { keymod, keycode, .. } => {
                    let km = map_keymode(keymod, keycode);
                    ctx.input.borrow_mut().keyup(km);
                }
                Event::TextInput { text, .. } => {
                    ctx.input.borrow_mut().text(text.as_str());
                }

                _ => {}
            }
        }

        ctx.frame(|ctx| {
            ctx.window(&mut state.window.clone(), WidgetOption::NONE, |container| {
                container.set_row_widths_height(&[-1], 0);
                container.button_ex("Hello World!", None, WidgetOption::ALIGN_CENTER);
            });
        });

        ctx.flush();
        window.gl_swap_window();

        ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
    }
}
