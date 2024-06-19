#[path = "./common/mod.rs"]
mod common;

use common::{atlas_config, start};
use microui_redux::*;

struct State {
    window: WindowHandle,
}

fn main() {
    let slots = vec![Dimensioni::new(64, 64), Dimensioni::new(24, 32), Dimensioni::new(64, 24)];
    let atlas = builder::Builder::from_config(&atlas_config(&slots)).unwrap().to_atlas();
    start(
        atlas,
        |ctx| State {
            window: ctx.new_window("Hello Window", rect(40, 40, 300, 450)),
        },
        |ctx, state| {
            ctx.frame(|ctx| {
                ctx.window(&mut state.window.clone(), WidgetOption::NONE, |container| {
                    container.set_row_widths_height(&[-1], 0);
                    container.button_ex("Hello World!", None, WidgetOption::ALIGN_CENTER);
                });
            });
        },
    );
}
