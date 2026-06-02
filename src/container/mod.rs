//! Shared draw commands and retained scroll-area handles.
//!
//! The old cursor traversal engine lived here. The root/window and scroll-area traversal paths now
//! run through `UiRuntime`, so this module only keeps the neutral command payloads and handle state
//! still used by widgets, graphics, and node-runtime rendering.

use crate::{
    Canvas, Color, Color4b, Dimensioni, FontId, IconId, Image, KeyCode, KeyMode, MouseEvent, Rect, Recti, Renderer, ScrollBehavior, SlotId, Vec2i, Vertex,
    WidgetOption,
};
use std::rc::Rc;

mod command;
pub use command::{CustomRenderArgs, CustomRenderCommand, TextWrap};
pub(crate) use command::Command;

mod draw;
pub(crate) use draw::render_command_stream;

mod handle;
pub use handle::{ScrollAreaHandle, ScrollAreaView, ScrollAreaViewMut};

mod scroll_area;
pub use scroll_area::ScrollArea;
