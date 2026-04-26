//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
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
    /// Pushes one triangle into the backend's current UI batch.
    fn push_triangle_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex);
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
    pub fn scope<Res, F: FnOnce(&R) -> Res>(&self, f: F) -> Res {
        match self.handle.read() {
            Ok(guard) => f(&*guard),
            Err(poisoned) => {
                // Reads can continue safely from the poisoned value.
                f(&*poisoned.into_inner())
            }
        }
    }

    /// Executes the provided closure with a mutable reference to the renderer.
    pub fn scope_mut<Res, F: FnOnce(&mut R) -> Res>(&mut self, f: F) -> Res {
        match self.handle.write() {
            Ok(mut guard) => f(&mut *guard),
            Err(poisoned) => {
                // Preserve the current renderer state instead of aborting on poison.
                f(&mut *poisoned.into_inner())
            }
        }
    }
}
