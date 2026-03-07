#[path = "./common/mod.rs"]
mod common;

use application::Application;
use common::{atlas_assets, *};
use microui_redux::*;

struct State {
    window: WindowHandle,
    tree: WidgetTree,
}

fn main() {
    let slots = atlas_assets::default_slots();
    let atlas = atlas_assets::load_atlas(&slots);
    let mut fw = Application::new(atlas.clone(), move |_gl, ctx| {
        let hello_button = widget_handle(Button::with_opt("Hello World!", WidgetOption::ALIGN_CENTER));
        let tree = WidgetTreeBuilder::build({
            let hello_button = hello_button.clone();
            move |tree| {
                tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                    tree.widget(hello_button.clone());
                });
            }
        });
        State {
            window: ctx.new_window("Hello Window", rect(40, 40, 300, 450)),
            tree,
        }
    })
    .unwrap();

    fw.event_loop(|ctx, state| {
        ctx.frame(|ctx| {
            ctx.window(
                &mut state.window.clone(),
                ContainerOption::NONE,
                WidgetBehaviourOption::NONE,
                |container, results| {
                    container.widget_tree(results, &state.tree);
                    WindowState::Open
                },
            );
        });
    });
}
