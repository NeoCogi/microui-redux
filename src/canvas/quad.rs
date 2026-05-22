//! Shared quad vertex construction for atlas and external texture draws.

use super::*;

pub(super) fn textured_quad_vertices(dst: Recti, src: Recti, texture_dim: Dimensioni, color: Color) -> [Vertex; 4] {
    let tex_width = texture_dim.width as f32;
    let tex_height = texture_dim.height as f32;
    let x0 = src.x as f32 / tex_width;
    let y0 = src.y as f32 / tex_height;
    let x1 = (src.x + src.width) as f32 / tex_width;
    let y1 = (src.y + src.height) as f32 / tex_height;

    let px0 = dst.x as f32;
    let py0 = dst.y as f32;
    let px1 = (dst.x + dst.width) as f32;
    let py1 = (dst.y + dst.height) as f32;

    let color = color4b(color.r, color.g, color.b, color.a);
    [
        Vertex::new(Vec2f::new(px0, py0), Vec2f::new(x0, y0), color),
        Vertex::new(Vec2f::new(px1, py0), Vec2f::new(x1, y0), color),
        Vertex::new(Vec2f::new(px1, py1), Vec2f::new(x1, y1), color),
        Vertex::new(Vec2f::new(px0, py1), Vec2f::new(x0, y1), color),
    ]
}
