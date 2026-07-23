//! Rendering backend contracts and renderer-facing frame resources.
//!
//! This module is the public integration boundary for render backends. It owns the final vertex
//! payload, renderer trait and handle, custom-render callback API, and the Canvas that manages
//! frame execution and texture resources.

mod backend;

pub use backend::{CustomRenderArgs, CustomRenderCommand, Renderer, RendererHandle, Vertex};
pub use crate::canvas::Canvas;
