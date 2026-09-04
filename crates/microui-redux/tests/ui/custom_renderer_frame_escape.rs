//! A callback must not retain the exclusively borrowed backend frame after returning.

#![allow(unused)]

use microui_redux::Context;
use microui_redux::render::{CustomRenderArgs, RendererBackend};

fn retain_frame<B: RendererBackend>(context: &mut Context<B>) {
    let mut retained = None;
    context
        .register_custom_renderer(move |frame: &mut B::Frame<'_>, _args: CustomRenderArgs| {
            retained = Some(frame);
        })
        .unwrap();
}

fn main() {}
