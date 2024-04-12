extern crate sdl2;
#[path = "./common/mod.rs"]
mod common;

use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::GLProfile;
use common::{atlas_config, GLRenderer};
use microui_redux::*;

use crate::common::Application;

struct State {
    window: WindowHandle<()>,
}
fn main() {
    let slots = vec![Dimensioni::new(64, 64), Dimensioni::new(24, 32), Dimensioni::new(64, 24)];
    let atlas = builder::Builder::from_config(&atlas_config(&slots)).unwrap().to_atlas();
    let mut fw = Application::new(atlas, |ctx| State {
        window: ctx.new_window("Hello Window", rect(40, 40, 300, 450)),
    })
    .unwrap();

    fw.event_loop(|ctx, state| {
        ctx.frame(|ctx| {
            ctx.window(&mut state.window.clone(), WidgetOption::NONE, |container| {
                container.set_row_widths_height(&[-1], 0);
                container.button_ex("Hello World!", None, WidgetOption::ALIGN_CENTER);
            });
        });
    });
}
