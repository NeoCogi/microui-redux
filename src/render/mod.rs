//! Rendering backend contracts and high-level frame execution.
//!
//! This module is the public integration boundary for render backends. It owns the final vertex
//! payload, backend trait and handle, custom-render callback API, owned display lists, and the
//! [`Renderer`] that manages frame execution and texture resources.

mod backend;
pub(crate) mod display_list;
pub(crate) mod geometry;
mod painter;
mod renderer;

pub use backend::{BackendHandle, CustomRenderArgs, CustomRenderCommand, RendererBackend, Vertex};
pub use display_list::DisplayList;
pub use painter::Painter;
pub use renderer::Renderer;
