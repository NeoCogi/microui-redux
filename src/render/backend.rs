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
//! Public renderer integration types.

use super::texture::{RendererId, TextureError};
use crate::atlas::AtlasHandle;
use crate::render::{Color, TextureId};
use rs_math3d::{Color4b, Dimensioni, Rect, Vec2f, color4b};
use std::{collections::HashMap, error::Error, fmt, marker::PhantomData};

#[derive(Default, Copy, Clone)]
#[repr(C)]
/// Vertex submitted by the UI to a renderer backend.
pub struct Vertex {
    /// Screen-space position in pixels.
    pos: Vec2f,
    /// Normalized texture coordinate.
    tex: Vec2f,
    /// Vertex color multiplied with the sampled texture.
    color: Color4b,
}

impl Vertex {
    /// Creates a vertex with the provided position, texture coordinate, and color.
    pub fn new(pos: Vec2f, tex: Vec2f, color: Color4b) -> Self {
        Self { pos, tex, color }
    }

    /// Returns the position of the vertex in screen space.
    pub fn position(&self) -> Vec2f {
        self.pos
    }

    /// Returns the texture coordinates associated with the vertex.
    pub fn tex_coord(&self) -> Vec2f {
        self.tex
    }

    /// Returns the vertex color.
    pub fn color(&self) -> Color4b {
        self.color
    }

    /// Linearly interpolates every render attribute between two vertices.
    ///
    /// Positions and texture coordinates use vector arithmetic, while packed color channels are
    /// rounded back to the nearest byte. Clamping the amount prevents callers from extrapolating
    /// attributes beyond the supplied segment.
    pub(crate) fn lerp(left: Self, right: Self, amount: f32) -> Self {
        // amount = clamp(requested_amount, 0, 1).
        let amount = amount.clamp(0.0, 1.0);

        Self {
            pos: left.pos + (right.pos - left.pos) * amount,
            tex: left.tex + (right.tex - left.tex) * amount,
            color: color4b(
                Self::lerp_channel(left.color.x, right.color.x, amount),
                Self::lerp_channel(left.color.y, right.color.y, amount),
                Self::lerp_channel(left.color.z, right.color.z, amount),
                Self::lerp_channel(left.color.w, right.color.w, amount),
            ),
        }
    }

    /// Interpolates one packed color channel and rounds it back into byte storage.
    fn lerp_channel(left: u8, right: u8, amount: f32) -> u8 {
        // channel = left + (right - left) * amount, clamped to one byte.
        ((left as f32) + (right as f32 - left as f32) * amount).round().clamp(0.0, 255.0) as u8
    }
}

/// Geometry forwarded to a custom backend rendering callback.
#[derive(Copy, Clone, Debug)]
pub struct CustomRenderArgs {
    /// Dimensions of the active backend frame.
    pub dimensions: Dimensioni,
    /// Rectangle describing the widget's content area.
    pub content_area: Rect<i32>,
    /// Final visible region after operation, content-area, and viewport clipping.
    ///
    /// This value is authoritative; callbacks do not need to intersect it with `content_area`
    /// again.
    pub view: Rect<i32>,
}

/// Error returned when constructing frame metadata.
#[derive(Copy, Clone, Debug)]
pub enum FrameInfoError {
    /// A drawable frame must have positive width and height.
    NonPositiveDimensions(Dimensioni),
}

impl PartialEq for FrameInfoError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NonPositiveDimensions(left), Self::NonPositiveDimensions(right)) => (left.width, left.height) == (right.width, right.height),
        }
    }
}

impl Eq for FrameInfoError {}

impl fmt::Display for FrameInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositiveDimensions(dimensions) => {
                write!(f, "frame dimensions must be positive, got {}x{}", dimensions.width, dimensions.height)
            }
        }
    }
}

impl Error for FrameInfoError {}

/// Validated immutable metadata for one logical/backend frame.
#[derive(Copy, Clone)]
pub struct FrameInfo {
    dimensions: Dimensioni,
    clear: Color,
}

impl fmt::Debug for FrameInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameInfo")
            .field("width", &self.dimensions.width)
            .field("height", &self.dimensions.height)
            .field("clear", &[self.clear.r, self.clear.g, self.clear.b, self.clear.a])
            .finish()
    }
}

impl FrameInfo {
    /// Creates frame metadata after validating a positive drawable size.
    pub fn try_new(dimensions: Dimensioni, clear: Color) -> Result<Self, FrameInfoError> {
        if dimensions.width <= 0 || dimensions.height <= 0 {
            return Err(FrameInfoError::NonPositiveDimensions(dimensions));
        }
        Ok(Self { dimensions, clear })
    }

    /// Returns the drawable frame dimensions.
    pub fn dimensions(&self) -> Dimensioni {
        self.dimensions
    }

    /// Returns the frame clear color.
    pub fn clear(&self) -> Color {
        self.clear
    }
}

/// Backend-frame acquisition failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameError {
    message: String,
}

impl FrameError {
    /// Creates a backend-frame error from a displayable message.
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl Error for FrameError {}

impl From<String> for FrameError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for FrameError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

/// Failure to upload and publish a replacement UI atlas in one renderer backend.
///
/// Theme switching uses a separate error from ordinary Context textures because the atlas is a
/// renderer-wide resource: a failed replacement must leave the previous atlas fully usable. The
/// backend-specific diagnostic is captured as owned text at this concrete public boundary, so
/// callers never need type erasure or backend-dependent generic error plumbing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtlasUploadError {
    /// Stable display representation of the backend allocation, upload, or binding failure.
    message: String,
}

impl AtlasUploadError {
    /// Creates an atlas-upload error from one displayable backend diagnostic.
    pub fn new(message: impl Into<String>) -> Self {
        // The originating driver value is often temporary, so retain the complete diagnostic as
        // an owned string while preserving a dedicated, matchable atlas error type.
        Self { message: message.into() }
    }
}

impl fmt::Display for AtlasUploadError {
    /// Writes the backend diagnostic without adding an unrelated texture classification.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The type itself identifies the operation; the retained message preserves backend detail.
        self.message.fmt(formatter)
    }
}

impl Error for AtlasUploadError {}

impl From<String> for AtlasUploadError {
    /// Preserves an owned backend diagnostic without another formatting allocation.
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for AtlasUploadError {
    /// Copies a static or borrowed diagnostic into the independently owned public error.
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

/// Active-frame geometry submission interface implemented by every backend frame.
pub trait RendererFrame {
    /// Appends one atlas-backed quad.
    fn push_quad(&mut self, vertices: [Vertex; 4]);
    /// Appends one atlas-backed triangle.
    fn push_triangle(&mut self, vertices: [Vertex; 3]);
    /// Closes the current atlas batch at an ordering boundary.
    fn flush(&mut self);
    /// Draws one pre-clipped external-texture quad.
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]);
}

/// Trait implemented by render backends used by the UI context.
///
/// Backends and their frames execute on the owning Context thread. This trait deliberately has no
/// `Send` or `Sync` bound.
///
/// Backends consume final [`Vertex`] values from Context's crate-private display-list executor:
///
/// ```
/// use microui_redux::{
///     prelude::{AtlasHandle, Color, TextureId},
///     render::{FrameError, FrameInfo, RendererBackend, RendererFrame, TextureError, Vertex},
/// };
///
/// struct Backend {
///     atlas: AtlasHandle,
/// }
///
/// #[must_use]
/// struct BackendFrame<'a>(&'a mut Backend);
///
/// impl RendererFrame for BackendFrame<'_> {
///     fn push_quad(&mut self, _vertices: [Vertex; 4]) {}
///     fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}
///     fn flush(&mut self) {}
///     fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
/// }
///
/// impl RendererBackend for Backend {
///     type Frame<'a> = BackendFrame<'a>;
///
///     fn get_atlas(&self) -> AtlasHandle {
///         self.atlas.clone()
///     }
///
///     fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), microui_redux::render::AtlasUploadError> {
///         self.atlas = atlas;
///         Ok(())
///     }
///
///     fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
///         Ok(BackendFrame(self))
///     }
///
///     fn create_texture(
///         &mut self,
///         _id: TextureId,
///         _pixels: &[u8],
///     ) -> Result<(), TextureError> {
///         Ok(())
///     }
///
///     fn destroy_texture(&mut self, _id: TextureId) {}
/// }
/// ```
///
/// An active frame owns the backend's exclusive mutable borrow, so safe Rust cannot acquire a
/// second frame until the first is dropped. The diagnostic-matched
/// `tests/ui/backend_frame_acquire_twice.rs` contract test verifies that rejection beside a passing
/// single-acquisition fixture.
pub trait RendererBackend: 'static {
    /// Exclusively borrowed active frame produced by this backend.
    type Frame<'a>: RendererFrame
    where
        Self: 'a;

    /// Returns the atlas backing the UI renderer.
    ///
    /// Every constructible [`AtlasHandle`] already contains a validated opaque white rendering tile
    /// named `white`, which the renderer samples for solid geometry. Backends used with
    /// [`crate::Context`] must additionally return a handle containing the `body` font and every
    /// lowercase semantic icon required by [`crate::ThemeIcons::from_atlas`].
    fn get_atlas(&self) -> AtlasHandle;
    /// Uploads and publishes one replacement UI atlas transactionally.
    ///
    /// Implementations must create and populate every replacement GPU resource before discarding
    /// the currently active atlas. Returning an error promises that [`Self::get_atlas`] and later
    /// frames still use the previous atlas. This operation is called only between frames.
    fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), AtlasUploadError>;
    /// Acquires and initializes one backend frame.
    fn frame(&mut self, info: FrameInfo) -> Result<Self::Frame<'_>, FrameError>;
    /// Creates a texture owned by the backend.
    ///
    /// The caller validates [`TextureId::size`] and RGBA byte length before calling this method.
    /// Backends should return [`TextureError::backend`] without retaining `id` when GPU creation
    /// or upload fails. Image validation and identifier allocation are owned by the Context, so a
    /// backend does not manufacture those higher-level error classifications.
    /// Dimensions are carried only by `id`, preventing a backend from observing contradictory
    /// handle and argument sizes.
    fn create_texture(&mut self, id: TextureId, pixels: &[u8]) -> Result<(), TextureError>;
    /// Destroys a previously created texture.
    fn destroy_texture(&mut self, id: TextureId);
}

/// Private heterogeneous storage shape for one backend-specialized callback closure.
///
/// Context registration accepts this same higher-ranked `FnMut` contract directly. Keeping only
/// the closure shape avoids exposing a named trait that applications could implement but no public
/// registration API could consume. Callbacks remain on the owning Context thread and therefore
/// deliberately have no `Send` or `Sync` bound.
type CustomRenderCallback<B> = dyn for<'frame> FnMut(&mut <B as RendererBackend>::Frame<'frame>, CustomRenderArgs) + 'static;

/// Backend-neutral key retained by UI nodes and display-list operations.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct CustomRenderKey {
    /// Identity of the renderer that owns this callback registry.
    renderer: RendererId,
    /// Monotonically allocated callback slot within `renderer`.
    slot: u64,
}

/// Typed public handle for a callback registered on a particular backend type.
pub struct CustomRenderHandle<B: RendererBackend> {
    pub(crate) key: CustomRenderKey,
    _backend: PhantomData<fn(&mut B)>,
}

impl<B: RendererBackend> Copy for CustomRenderHandle<B> {}

impl<B: RendererBackend> Clone for CustomRenderHandle<B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B: RendererBackend> fmt::Debug for CustomRenderHandle<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomRenderHandle").field("key", &self.key).finish()
    }
}

/// Custom-render registry mutation failure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CustomRenderRegistryError {
    /// This registry's monotonically increasing callback slot counter was exhausted.
    SlotExhausted,
    /// The supplied handle is foreign to this registry or has already been removed.
    UnknownRenderer,
}

impl fmt::Display for CustomRenderRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlotExhausted => f.write_str("custom-render registry slot counter exhausted"),
            Self::UnknownRenderer => f.write_str("unknown custom renderer"),
        }
    }
}

impl Error for CustomRenderRegistryError {}

/// Renderer-owned callbacks specialized for one concrete backend.
pub(crate) struct CustomRenderRegistry<B: RendererBackend> {
    /// Identity shared with the Renderer and every other renderer-owned capability.
    renderer: RendererId,
    /// Last callback slot allocated by this registry.
    next_slot: u64,
    /// Live callbacks keyed by renderer provenance and local slot.
    callbacks: HashMap<CustomRenderKey, Box<CustomRenderCallback<B>>>,
}

impl<B: RendererBackend> CustomRenderRegistry<B> {
    /// Creates the sole custom-render registry owned by `renderer`.
    pub(crate) fn new(renderer: RendererId) -> Self {
        // Renderer constructs this registry exactly once and supplies the same identity copied into
        // texture handles. No second namespace allocator or lazy initialization state is required.
        Self {
            renderer,
            next_slot: 0,
            callbacks: HashMap::new(),
        }
    }

    /// Registers one concrete backend-specialized callback under a fresh local slot.
    pub(crate) fn register<F>(&mut self, callback: F) -> Result<CustomRenderHandle<B>, CustomRenderRegistryError>
    where
        F: for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs) + 'static,
    {
        // next_slot = current_slot + 1.
        let slot = self.next_slot.checked_add(1).ok_or(CustomRenderRegistryError::SlotExhausted)?;
        self.next_slot = slot;
        let key = CustomRenderKey { renderer: self.renderer, slot };
        let previous = self.callbacks.insert(key, Box::new(callback));
        debug_assert!(previous.is_none(), "fresh custom-render key was already occupied");
        Ok(CustomRenderHandle { key, _backend: PhantomData })
    }

    /// Removes a live callback only when its handle belongs to this renderer.
    pub(crate) fn remove(&mut self, handle: CustomRenderHandle<B>) -> Result<(), CustomRenderRegistryError> {
        // Check provenance before touching the map so a same-slot foreign handle can never remove
        // this registry's callback.
        if handle.key.renderer != self.renderer || self.callbacks.remove(&handle.key).is_none() {
            return Err(CustomRenderRegistryError::UnknownRenderer);
        }
        Ok(())
    }

    /// Reports whether `key` identifies a live callback owned by this renderer.
    pub(crate) fn contains(&self, key: CustomRenderKey) -> bool {
        key.renderer == self.renderer && self.callbacks.contains_key(&key)
    }

    /// Borrows a live callback only when its key belongs to this renderer.
    pub(crate) fn get_mut(&mut self, key: CustomRenderKey) -> Option<&mut Box<CustomRenderCallback<B>>> {
        if key.renderer != self.renderer {
            return None;
        }
        self.callbacks.get_mut(&key)
    }
}
