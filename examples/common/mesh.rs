#![allow(dead_code)]

use std::sync::Arc;

use microui_redux::*;

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
