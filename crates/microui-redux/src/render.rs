//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

#![doc = include_str!("../docs/RENDER.md")]

mod backend;
mod color;
pub(crate) mod display_list;
pub(crate) mod geometry;
mod nine_patch;
mod painter;
#[cfg(test)]
mod performance;
mod renderer;
mod texture;

pub use backend::{
    AtlasUploadError, CustomRenderArgs, CustomRenderHandle, CustomRenderRegistryError, FrameError, FrameInfo, FrameInfoError, RendererBackend, RendererFrame,
    Vertex,
};
pub(crate) use backend::CustomRenderKey;
pub use color::{Color, color};
pub(crate) use display_list::DisplayList;
pub use painter::Painter;
pub use renderer::RenderError;
pub(crate) use renderer::Renderer;
pub use nine_patch::{NinePatch, NinePatchCell, NinePatchCells, NinePatchContent, NinePatchImage, SliceInsets};
pub use texture::{TextureError, TextureId};
