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

#![allow(dead_code)]
//! Shared mesh buffer and submission helpers for renderer examples.

use std::sync::Arc;

use microui_redux::prelude::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
/// CPU-side vertex used by mesh submissions in the demo path.
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone)]
/// Shared mesh storage so callers can cheaply clone submissions across frames.
pub struct MeshBuffers {
    vertices: Arc<[MeshVertex]>,
    indices: Arc<[u32]>,
}

impl MeshBuffers {
    pub fn from_vecs(vertices: Vec<MeshVertex>, indices: Vec<u32>) -> Self {
        Self {
            vertices: vertices.into(),
            indices: indices.into(),
        }
    }

    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.indices.is_empty()
    }
}

#[derive(Clone)]
/// A single mesh draw request transformed by caller-provided matrices.
pub struct MeshSubmission {
    pub mesh: MeshBuffers,
    pub pvm: Mat4f,
    pub view_model: Mat4f,
}

#[derive(Clone, Copy)]
/// Target rectangle and clip rectangle for custom non-UI draws.
pub struct CustomRenderArea {
    pub rect: Recti,
    pub clip: Recti,
}
