//! Rendering backend contracts and renderer-facing frame resources.
//!
//! This module is the public integration boundary for render backends. It owns the final vertex
//! payload, renderer trait and handle, custom-render callback API, owned display lists, and the
//! Canvas that manages frame execution and texture resources.

mod backend;
mod canvas;
pub(crate) mod display_list;
pub(crate) mod geometry;
mod painter;
mod quad;

pub use backend::{CustomRenderArgs, CustomRenderCommand, Renderer, RendererHandle, Vertex};
pub use canvas::Canvas;
pub use display_list::DisplayList;
pub use painter::Painter;
