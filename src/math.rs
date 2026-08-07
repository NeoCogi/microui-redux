//! Lightweight integer-geometry conveniences shared by UI APIs.

use rs_math3d::{Recti, Vec2i};

/// Convenience constructor for [`Vec2i`].
pub fn vec2(x: i32, y: i32) -> Vec2i {
    Vec2i { x, y }
}

/// Convenience constructor for [`Recti`].
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> Recti {
    Recti { x, y, width: w, height: h }
}

/// Expands (or shrinks) a rectangle uniformly on all sides.
pub fn expand_rect(rectangle: Recti, amount: i32) -> Recti {
    rect(
        rectangle.x - amount,
        rectangle.y - amount,
        rectangle.width + amount * 2,
        rectangle.height + amount * 2,
    )
}
