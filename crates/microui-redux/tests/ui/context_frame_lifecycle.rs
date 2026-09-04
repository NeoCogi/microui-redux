//! Context mutation surrounds, but never overlaps, one exclusively borrowed logical frame.

use microui_redux::Context;
use microui_redux::render::{FrameInfo, RendererBackend};

#[allow(dead_code)]
fn cancel_then_mutate<B: RendererBackend>(context: &mut Context<B>, info: FrameInfo) {
    context.mousemove(10, 20);
    let frame = context.frame(info);
    drop(frame);
    context.mousemove(20, 30);
}

#[allow(dead_code)]
fn submit_once<B: RendererBackend>(context: &mut Context<B>, info: FrameInfo) {
    // Submission consumes the frame even when rendering returns an application-visible error.
    let _result = context.frame(info).render_ui();
}

fn main() {}
