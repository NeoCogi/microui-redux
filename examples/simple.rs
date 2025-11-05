#[path = "./common/mod.rs"]
mod common;

use application::Application;
use common::*;
use microui_redux::*;

struct State {
    window: WindowHandle,
}

fn main() {
    let slots = vec![Dimensioni::new(64, 64), Dimensioni::new(24, 32), Dimensioni::new(64, 24)];
    let atlas = builder::Builder::from_config(&application::atlas_config(&slots)).unwrap().to_atlas();
    let mut fw = Application::new(atlas.clone(), move |gl, ctx| State {
        window: ctx.new_window("Hello Window", rect(40, 40, 300, 450)),
    })
    .unwrap();

    fw.event_loop(|ctx, state| {
        ctx.frame(|ctx| {
            ctx.window(&mut state.window.clone(), ContainerOption::NONE, |container| {
                container.set_row_widths_height(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
                container.button_ex("Hello World!", None, WidgetOption::ALIGN_CENTER);
                WindowState::Open
            });
        });
    });
}
