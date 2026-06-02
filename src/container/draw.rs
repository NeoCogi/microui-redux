//! Draw-command replay.

use super::*;

/// Replays a command list into the renderer canvas.
pub(crate) fn render_command_stream<R: Renderer>(canvas: &mut Canvas<R>, commands: &mut Vec<Command>, triangle_vertices: &[Vertex]) {
    let mut pending = std::mem::take(commands);
    while !pending.is_empty() {
        let special_index = pending.iter().position(|command| matches!(command, Command::BackendCustomRender(_, _)));
        let batch_len = special_index.unwrap_or(pending.len());
        if batch_len > 0 {
            render_command_batch(canvas, triangle_vertices, pending.drain(..batch_len));
        }

        if special_index.is_none() {
            break;
        }

        if let Some(Command::BackendCustomRender(mut cra, mut f)) = pending.drain(..1).next() {
            canvas.flush();
            let prev_clip = canvas.current_clip_rect();
            let merged_clip = match prev_clip.intersect(&cra.view) {
                Some(rect) => rect,
                None => Recti::new(cra.content_area.x, cra.content_area.y, 0, 0),
            };
            canvas.set_clip_rect(merged_clip);
            cra.view = merged_clip;
            f.render(canvas.current_dimension(), &cra);
            canvas.flush();
            canvas.set_clip_rect(prev_clip);
        }
    }
    *commands = pending;
}

fn render_command_batch<R, I>(canvas: &mut Canvas<R>, triangle_vertices: &[Vertex], commands: I)
where
    R: Renderer,
    I: IntoIterator<Item = Command>,
{
    let base_clip = canvas.current_clip_rect();
    canvas.render_scope(|canvas| {
        let mut clip_stack = vec![base_clip];
        canvas.set_clip_rect(base_clip);
        for command in commands {
            match command {
                Command::Text { text, pos, color, font } => {
                    canvas.draw_chars(font, &text, pos, color);
                }
                Command::Recti { rect, color } => {
                    canvas.draw_rect(rect, color);
                }
                Command::Icon { id, rect, color } => {
                    canvas.draw_icon(id, rect, color);
                }
                Command::PushClip { rect } => {
                    let current = clip_stack.last().copied().unwrap_or(base_clip);
                    let next = current.intersect(&rect).unwrap_or_default();
                    clip_stack.push(next);
                    canvas.set_clip_rect(next);
                }
                Command::PopClip => {
                    if clip_stack.len() > 1 {
                        clip_stack.pop();
                    }
                    let current = clip_stack.last().copied().unwrap_or(base_clip);
                    canvas.set_clip_rect(current);
                }
                Command::Image { rect, image, color } => {
                    canvas.draw_image(image, rect, color);
                }
                Command::SlotRedraw { rect, id, color, payload } => {
                    canvas.draw_slot_with_function(id, rect, color, payload);
                }
                Command::Triangle { vertex_start, vertex_count } => {
                    let end = vertex_start + vertex_count;
                    canvas.draw_triangles(&triangle_vertices[vertex_start..end]);
                }
                Command::BackendCustomRender(_, _) | Command::None => (),
            }
        }
        canvas.set_clip_rect(base_clip);
    });
}
