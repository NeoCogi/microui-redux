//! Shared types and construction helpers for the repository's example renderers.

pub mod mesh;

#[doc(hidden)]
pub mod resource_guard;

pub use mesh::{CustomRenderArea, MeshBuffers, MeshSubmission, MeshVertex};
