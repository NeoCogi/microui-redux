//! Renderer-owned external texture handles.

use rs_math3d::Dimensioni;

/// Handle referencing an external texture managed by the renderer.
///
/// Equality and hashing include the renderer-issued numeric identifier and the immutable width and
/// height carried by the handle. Renderer validation therefore accepts only the exact handle whose
/// dimensions will be used for texture-coordinate projection.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureId {
    /// Backend-local texture identifier.
    raw: u32,
    /// Texture width in pixels.
    width: i32,
    /// Texture height in pixels.
    height: i32,
}

impl TextureId {
    /// Creates a texture id with known dimensions.
    pub(crate) fn new(raw: u32, width: i32, height: i32) -> Self {
        Self { raw, width, height }
    }

    /// Returns the raw numeric identifier stored inside the handle.
    pub fn raw(self) -> u32 {
        self.raw
    }

    /// Returns the texture width in pixels.
    pub fn width(self) -> i32 {
        self.width
    }

    /// Returns the texture height in pixels.
    pub fn height(self) -> i32 {
        self.height
    }

    /// Returns the texture dimensions in pixels.
    pub fn size(self) -> Dimensioni {
        Dimensioni::new(self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn identity_includes_immutable_dimensions() {
        let texture = TextureId::new(7, 32, 16);
        let same = TextureId::new(7, 32, 16);
        let different_width = TextureId::new(7, 64, 16);
        let different_height = TextureId::new(7, 32, 8);

        assert_eq!(texture, same);
        assert_ne!(texture, different_width);
        assert_ne!(texture, different_height);

        let textures = HashSet::from([texture]);
        assert!(textures.contains(&same));
        assert!(!textures.contains(&different_width));
        assert!(!textures.contains(&different_height));
    }
}
