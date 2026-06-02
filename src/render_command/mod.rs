//! Backend-neutral draw command payloads and replay.

use crate::{
    Canvas, Color, Color4b, Dimensioni, FontId, IconId, Image, KeyCode, KeyMode, MouseEvent, Rect, Recti, Renderer, ScrollBehavior, SlotId, Vec2i, Vertex,
    WidgetOption,
};
use std::rc::Rc;

mod command;
pub use command::{CustomRenderArgs, CustomRenderCommand, TextWrap};
pub(crate) use command::Command;

mod replay;
pub(crate) use replay::render_command_stream;
