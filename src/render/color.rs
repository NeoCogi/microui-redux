//! Backend-neutral color primitives.

#[derive(Default, Copy, Clone)]
#[repr(C)]
/// Simple RGBA color stored with 8-bit components.
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

/// Convenience constructor for [`Color`].
pub fn color(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}
