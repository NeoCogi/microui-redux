//! Update and paint contexts expose the distinct capabilities appropriate to their phases.

use microui_redux::prelude::{WidgetPaintCtx, WidgetUpdateCtx};

#[allow(dead_code)]
fn inspect_during_update(context: &WidgetUpdateCtx<'_>) {
    // Geometry inspection is available without granting any recording capability.
    let _bounds = context.local_rect();
}

#[allow(dead_code)]
fn record_during_paint(context: &mut WidgetPaintCtx<'_>) {
    // Painter acquisition belongs exclusively to the paint phase.
    let _painter = context.painter();
}

fn main() {}
