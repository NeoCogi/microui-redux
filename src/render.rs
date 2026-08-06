#![doc = include_str!("render/RENDER.md")]

mod backend;
mod color;
pub(crate) mod display_list;
pub(crate) mod geometry;
mod painter;
#[cfg(test)]
mod performance;
mod renderer;
mod texture;

pub use backend::{
    CustomRender, CustomRenderArgs, CustomRenderHandle, CustomRenderRegistryError, FrameError, FrameInfo, FrameInfoError, RendererBackend, RendererFrame,
    Vertex,
};
pub(crate) use backend::CustomRenderKey;
pub use color::{Color, color};
pub(crate) use display_list::DisplayList;
pub use painter::Painter;
pub use renderer::{RenderError, Renderer};
pub use texture::TextureId;
