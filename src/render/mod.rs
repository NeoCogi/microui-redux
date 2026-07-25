#![doc = include_str!("RENDER.md")]

mod backend;
pub(crate) mod display_list;
pub(crate) mod geometry;
mod painter;
#[cfg(test)]
mod performance;
mod renderer;

pub use backend::{
    CustomRender, CustomRenderArgs, CustomRenderHandle, CustomRenderRegistryError, FrameError, FrameInfo, FrameInfoError, RendererBackend, RendererFrame,
    Vertex,
};
pub(crate) use backend::CustomRenderKey;
pub use display_list::DisplayList;
pub use painter::Painter;
pub use renderer::{RenderError, Renderer};
