#[path = "./common/mod.rs"]
mod common;

use application::Application;
use common::{atlas_assets, *};
use microui_redux::*;

struct State {
    window: WindowHandle,
    hello_button: Button,
}

fn main() {
    let slots = atlas_assets::default_slots();
    let atlas = atlas_assets::load_atlas(&slots);
    let mut fw = Application::new(atlas.clone(), move |_gl, ctx| State {
        window: ctx.new_window("Hello Window", rect(40, 40, 300, 450)),
        hello_button: Button::with_opt("Hello World!", WidgetOption::ALIGN_CENTER),
    })
    .unwrap();

    fw.event_loop(|ctx, state| {
        ctx.frame(|ctx| {
            ctx.window(&mut state.window.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |container| {
                container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                    container.button(&mut state.hello_button);
                });
                WindowState::Open
            });
        });
    });
}
