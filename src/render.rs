//! Renderer-facing traits and handles.

use std::sync::{Arc, RwLock};

use crate::atlas::AtlasHandle;
use crate::canvas::Vertex;
use crate::style::{Color, TextureId};

/// Trait implemented by render backends used by the UI context.
pub trait Renderer {
    /// Returns the atlas backing the renderer.
    fn get_atlas(&self) -> AtlasHandle;
    /// Begins a new frame with the viewport size and clear color.
    fn begin(&mut self, width: i32, height: i32, clr: Color);
    /// Pushes four vertices representing a quad to the backend.
    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex);
    /// Flushes any buffered geometry to the GPU.
    fn flush(&mut self);
    /// Ends the frame, finalizing any outstanding GPU work.
    fn end(&mut self);
    /// Creates a texture owned by the renderer.
    fn create_texture(&mut self, id: TextureId, width: i32, height: i32, pixels: &[u8]);
    /// Destroys a previously created texture.
    fn destroy_texture(&mut self, id: TextureId);
    /// Draws the provided textured quad.
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]);
}

/// Thread-safe handle that shares ownership of a [`Renderer`].
pub struct RendererHandle<R: Renderer> {
    handle: Arc<RwLock<R>>,
}

// `derive(Clone)` does not infer the bound correctly here, but `Arc` already provides
// the behavior we need.
impl<R: Renderer> Clone for RendererHandle<R> {
    fn clone(&self) -> Self {
        Self { handle: self.handle.clone() }
    }
}

impl<R: Renderer> RendererHandle<R> {
    /// Wraps a renderer inside an [`Arc<RwLock<...>>`] so it can be shared.
    pub fn new(renderer: R) -> Self {
        Self { handle: Arc::new(RwLock::new(renderer)) }
    }

    /// Executes the provided closure with a shared reference to the renderer.
    pub fn scope<Res, F: Fn(&R) -> Res>(&self, f: F) -> Res {
        match self.handle.read() {
            Ok(guard) => f(&*guard),
            Err(poisoned) => {
                // Reads can continue safely from the poisoned value.
                f(&*poisoned.into_inner())
            }
        }
    }

    /// Executes the provided closure with a mutable reference to the renderer.
    pub fn scope_mut<Res, F: FnMut(&mut R) -> Res>(&mut self, mut f: F) -> Res {
        match self.handle.write() {
            Ok(mut guard) => f(&mut *guard),
            Err(poisoned) => {
                // Preserve the current renderer state instead of aborting on poison.
                f(&mut *poisoned.into_inner())
            }
        }
    }
}
