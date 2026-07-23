//! Backend-neutral draw command payloads and replay.

use crate::render::{CustomRenderArgs, CustomRenderCommand, Renderer, Vertex};
use crate::{Canvas, Color, Color4b, FontId, IconId, Image, Recti, SlotId, Vec2i};
use std::rc::Rc;

mod command;
pub use command::TextWrap;
pub(crate) use command::Command;

mod replay;
pub(crate) use replay::render_command_stream;

#[cfg(test)]
mod tests;
