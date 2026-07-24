//! Transitional conversion from legacy commands into the unified DisplayList executor.

use super::*;
use crate::render::DisplayList;

/// Converts a legacy command list once, then executes it through [`Canvas::render`].
///
/// This adapter exists only until all producers record directly into DisplayList. It performs no
/// renderer work and never scans ahead for custom operations; each command is consumed exactly
/// once and custom barriers are handled later by Canvas.
pub(crate) fn render_command_stream<R: Renderer>(canvas: &mut Canvas<R>, list: &mut DisplayList, commands: &mut Vec<Command>, triangle_vertices: &[Vertex]) {
    let pending = std::mem::take(commands);
    list.clear();
    record_legacy_commands(list, pending, triangle_vertices);
    canvas.render(list);
}

/// Translates legacy payloads whose effective clips were captured during recording.
fn record_legacy_commands(list: &mut DisplayList, commands: Vec<Command>, triangle_vertices: &[Vertex]) {
    for Command { clip, kind } in commands {
        match kind {
            CommandKind::Text { text, pos, color, font } => {
                list.push_text(clip, font, pos, color, text);
            }
            CommandKind::Recti { rect, color } => {
                list.push_fill_rect(clip, rect, color);
            }
            CommandKind::Icon { id, rect, color } => {
                list.push_icon(clip, id, rect, color);
            }
            CommandKind::Image { rect, image, color } => {
                list.push_image(clip, image, rect, color);
            }
            CommandKind::SlotRedraw { rect, id, color, payload } => {
                list.push_redraw_slot(clip, id, rect, color, payload);
            }
            CommandKind::Triangle { vertex_start, vertex_count } => {
                let Some(vertex_end) = vertex_start.checked_add(vertex_count) else {
                    continue;
                };
                let Some(vertices) = triangle_vertices.get(vertex_start..vertex_end) else {
                    continue;
                };
                list.push_backend_triangles(clip, vertices);
            }
            CommandKind::BackendCustomRender(args, command) => {
                list.push_custom(clip, args, command);
            }
            CommandKind::None => {}
        }
    }
}
