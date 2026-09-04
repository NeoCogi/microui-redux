//! A callback generic over the Context's own backend satisfies the higher-ranked frame contract.

use microui_redux::Context;
use microui_redux::render::{CustomRenderArgs, RendererBackend};

#[allow(dead_code)]
fn register_for_backend<B, F>(context: &mut Context<B>, callback: F)
where
    B: RendererBackend,
    F: for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs) + 'static,
{
    let _handle = context.register_custom_renderer(callback).expect("a fresh generic callback should register");
}

fn main() {
    // Type-checking the generic function body is sufficient; a UI fixture needs no GPU backend.
}
