//! Characterization tests for ordered command replay into the renderer boundary.

use super::*;
use crate::render::{Renderer, RendererHandle, Vertex};
use crate::test_support::{recording_renderer, RenderEvent};
use crate::{color, color4b, AtlasHandle, AtlasSource, Canvas, CharEntry, FontEntry, Image, Recti, SourceFormat, TextureId, Vec2f, Vec2i, CLOSE_ICON};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn make_replay_atlas() -> AtlasHandle {
    let pixels = [0xFF; 8 * 8 * 4];
    let icons = [("white", Recti::new(0, 0, 1, 1)), ("close", Recti::new(4, 0, 4, 4))];
    let entries = [
        (
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(4, 0),
                rect: Recti::new(0, 4, 4, 4),
            },
        ),
        (
            'a',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(4, 0),
                rect: Recti::new(0, 4, 4, 4),
            },
        ),
    ];
    let fonts = [(
        "default",
        FontEntry {
            line_size: 4,
            baseline: 4,
            font_size: 4,
            entries: &entries,
        },
    )];
    let slots = [Recti::new(4, 4, 4, 4)];
    AtlasHandle::from(&AtlasSource {
        width: 8,
        height: 8,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
        slots: &slots,
    })
}

fn custom_args(content_area: Recti, view: Recti) -> CustomRenderArgs {
    CustomRenderArgs { content_area, view }
}

fn load_test_texture<R: Renderer>(canvas: &mut Canvas<R>) -> TextureId {
    canvas.try_load_texture_rgba(1, 1, &[0xFF; 4]).unwrap()
}

#[test]
fn mixed_commands_reach_the_renderer_in_stream_order() {
    let (renderer, log) = recording_renderer(make_replay_atlas());
    let mut canvas = Canvas::from(renderer, crate::Dimensioni::new(32, 32));
    let texture = load_test_texture(&mut canvas);
    log.clear();

    let mut commands = vec![
        Command::Recti {
            rect: Recti::new(0, 0, 4, 4),
            color: color(255, 0, 0, 255),
        },
        Command::Text {
            font: crate::FontId::default(),
            pos: Vec2i::new(4, 0),
            color: color(0, 255, 0, 255),
            text: String::from("a"),
        },
        Command::Triangle { vertex_start: 0, vertex_count: 3 },
        Command::Image {
            rect: Recti::new(12, 0, 4, 4),
            image: Image::Texture(texture),
            color: color(255, 255, 0, 255),
        },
    ];
    let triangle_vertices = [
        Vertex::new(Vec2f::new(8.0, 0.0), Vec2f::default(), color4b(0, 0, 255, 255)),
        Vertex::new(Vec2f::new(12.0, 0.0), Vec2f::default(), color4b(0, 0, 255, 255)),
        Vertex::new(Vec2f::new(8.0, 4.0), Vec2f::default(), color4b(0, 0, 255, 255)),
    ];

    render_command_stream(&mut canvas, &mut commands, &triangle_vertices);

    assert!(commands.is_empty());
    let events = log.snapshot();
    assert_eq!(events.len(), 4);
    let RenderEvent::AtlasQuad(rect) = &events[0] else {
        panic!("rectangle must be the first renderer event");
    };
    assert_eq!(rect[0].color, [255, 0, 0, 255]);
    let RenderEvent::AtlasQuad(text) = &events[1] else {
        panic!("text must be the second renderer event");
    };
    assert_eq!(text[0].color, [0, 255, 0, 255]);
    let RenderEvent::Triangle(triangle) = &events[2] else {
        panic!("triangle must be the third renderer event");
    };
    assert_eq!(triangle[0].color, [0, 0, 255, 255]);
    let RenderEvent::ExternalTexture { id, vertices } = &events[3] else {
        panic!("external image must be the fourth renderer event");
    };
    assert_eq!(*id, texture);
    assert_eq!(vertices[0].color, [255, 255, 0, 255]);
}

#[test]
fn custom_render_is_clipped_flushed_and_can_reenter_the_renderer_handle() {
    let (renderer, log) = recording_renderer(make_replay_atlas());
    let mut canvas = Canvas::from(renderer.clone(), crate::Dimensioni::new(20, 20));
    canvas.set_clip_rect(Recti::new(0, 0, 20, 20));
    let observed = Rc::new(RefCell::new(Vec::new()));
    let callback_observed = observed.clone();
    let mut callback_renderer: RendererHandle<_> = renderer.clone();

    let callback = move |dim: crate::Dimensioni, args: &CustomRenderArgs| {
        callback_observed
            .borrow_mut()
            .push(((dim.width, dim.height), (args.view.x, args.view.y, args.view.width, args.view.height)));
        callback_renderer.scope_mut(|renderer| renderer.record_marker("custom"));
    };
    let mut commands = vec![
        Command::Recti {
            rect: Recti::new(0, 0, 4, 4),
            color: color(255, 0, 0, 255),
        },
        Command::BackendCustomRender(custom_args(Recti::new(0, 0, 40, 40), Recti::new(10, 10, 20, 20)), Box::new(callback)),
        Command::Recti {
            rect: Recti::new(4, 0, 4, 4),
            color: color(0, 0, 255, 255),
        },
    ];

    render_command_stream(&mut canvas, &mut commands, &[]);

    assert_eq!(*observed.borrow(), vec![((20, 20), (10, 10, 10, 10))]);
    let restored_clip = canvas.current_clip_rect();
    assert_eq!((restored_clip.x, restored_clip.y, restored_clip.width, restored_clip.height), (0, 0, 20, 20));
    let events = log.snapshot();
    assert_eq!(events.len(), 5);
    assert!(matches!(events[0], RenderEvent::AtlasQuad(_)));
    assert_eq!(events[1], RenderEvent::Flush);
    assert_eq!(events[2], RenderEvent::Marker(String::from("custom")));
    assert_eq!(events[3], RenderEvent::Flush);
    assert!(matches!(events[4], RenderEvent::AtlasQuad(_)));
}

#[test]
fn dynamic_slot_payload_runs_before_its_atlas_quad() {
    let atlas = make_replay_atlas();
    let update_before = atlas.get_last_update_id();
    let (renderer, log) = recording_renderer(atlas.clone());
    let mut canvas = Canvas::from(renderer, crate::Dimensioni::new(20, 20));
    let payload_log = log.clone();
    let payload_called = Rc::new(Cell::new(false));
    let payload_called_in_callback = payload_called.clone();
    let payload = Rc::new(move |_, _| {
        if !payload_called_in_callback.replace(true) {
            payload_log.record_marker("slot-payload");
        }
        color4b(7, 8, 9, 255)
    });
    let mut commands = vec![Command::SlotRedraw {
        rect: Recti::new(0, 0, 4, 4),
        id: crate::SlotId::default(),
        color: color(255, 255, 255, 255),
        payload,
    }];

    render_command_stream(&mut canvas, &mut commands, &[]);

    assert_eq!(atlas.get_last_update_id(), update_before.wrapping_add(1));
    assert!(payload_called.get());
    let events = log.snapshot();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], RenderEvent::Marker(String::from("slot-payload")));
    assert!(matches!(events[1], RenderEvent::AtlasQuad(_)));
    assert_eq!(crate::test_support::recorded_color(atlas.pixels_clone()[4 + 4 * 8]), [7, 8, 9, 255]);
}

#[test]
fn icon_command_obeys_nested_replay_clips() {
    let (renderer, log) = recording_renderer(make_replay_atlas());
    let mut canvas = Canvas::from(renderer, crate::Dimensioni::new(20, 20));
    let mut commands = vec![
        Command::PushClip { rect: Recti::new(2, 0, 10, 10) },
        Command::PushClip { rect: Recti::new(3, 1, 2, 2) },
        Command::Icon {
            rect: Recti::new(1, 0, 4, 4),
            id: CLOSE_ICON,
            color: color(255, 255, 255, 255),
        },
        Command::PopClip,
        Command::PopClip,
    ];

    render_command_stream(&mut canvas, &mut commands, &[]);

    let events = log.snapshot();
    let [RenderEvent::AtlasQuad(vertices)] = events.as_slice() else {
        panic!("nested clips should produce one clipped icon quad");
    };
    assert_eq!(vertices[0].position, [3.0, 1.0]);
    assert_eq!(vertices[2].position, [5.0, 3.0]);
    assert_eq!(vertices[0].tex_coord, [0.75, 0.125]);
    assert_eq!(vertices[2].tex_coord, [1.0, 0.375]);
}
