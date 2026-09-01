//! Widget updates must not record drawing through a phase-inappropriate Painter.

use microui_redux::prelude::WidgetUpdateCtx;

fn painting_is_not_an_update_capability(context: &mut WidgetUpdateCtx<'_>) {
    let _painter = context.painter();
}

fn main() {}
