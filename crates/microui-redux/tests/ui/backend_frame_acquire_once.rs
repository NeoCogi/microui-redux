//! One backend frame may be acquired, used, and dropped before another borrow begins.

use microui_redux::render::{FrameInfo, RendererBackend, RendererFrame};

#[allow(dead_code)]
fn acquire_once<B: RendererBackend>(backend: &mut B, info: FrameInfo) {
    let mut frame = backend.frame(info).expect("the fixture characterizes ownership after successful acquisition");
    // A frame operation proves the associated type exposes the intended public frame contract.
    frame.flush();
    drop(frame);
}

fn main() {}
