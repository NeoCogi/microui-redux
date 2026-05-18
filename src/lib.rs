//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
#![deny(missing_docs)]
// `clippy::pedantic` is useful as an occasional review tool for this crate, but these categories
// are intentionally outside the local lint profile. The UI/rendering path performs many bounded
// pixel/UV casts, internal modules use crate preludes heavily, and the immediate-mode public API
// should not grow `#[must_use]` or pedantic doc-section noise mechanically.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::struct_field_names,
    clippy::too_many_arguments,
    clippy::wildcard_imports
)]
//! `microui-redux` provides a GUI toolkit inspired by [rxi/microui](https://github.com/rxi/microui).
//! The crate uses retained [`WidgetTree`] values as the public UI authoring model while keeping Microui's
//! compact frame-driven execution and renderer integration.
//! It exposes the core context, retained widget tree builders, widget state types, renderer traits,
//! styles, and image APIs needed to embed a UI inside custom render backends while remaining
//! allocator- and platform-agnostic.
//! Built-in widget placement is driven by each widget's `measure` result, so auto-sized rows can use
//! per-widget intrinsic text/icon metrics instead of a single shared control size.
//! Layout internals are flow-based: row tracks and vertical stack flows both run through the same
//! engine so scope/scroll/content bookkeeping stays consistent.
//! Per-frame interaction results are collected internally and published as a committed
//! generation through [`Context::committed_results`].
//! Retained application/business logic reacts through
//! [`Context::committed_results`], which exposes the previous frame's published
//! interaction generation as the crate's public retained contract.

mod atlas;
mod canvas;
mod container;
mod container_handle;
mod context;
mod draw_context;
mod file_dialog;
mod graphics;
mod id;
mod input;
mod layout;
mod rect_packer;
mod render;
mod scrollbar;
mod style;
mod text_layout;
mod widget;
mod widget_ctx;
mod widget_tree;
mod widgets;
mod window;

/// Low-level renderer integration types.
///
/// Most applications should use [`Context`] plus retained [`WidgetTree`] values. Backend authors
/// and renderer smoke tests can use these types when they need direct access to the command canvas
/// or the exact vertex payload delivered to [`Renderer`].
pub mod backend {
    pub use crate::canvas::{Canvas, Vertex};
}

pub use atlas::*;
pub use canvas::Vertex;
pub use container::{CustomRenderArgs, CustomRenderCommand, TextWrap};
pub use container_handle::*;
pub use context::Context;
pub use file_dialog::*;
pub use graphics::*;
pub use id::Id;
pub use input::*;
pub use layout::{SizePolicy, StackDirection};
pub use render::*;
pub use rs_math3d::*;
pub use style::*;
pub use widget::*;
pub use widget_tree::*;
pub use widgets::*;
pub use window::*;

pub(crate) use canvas::Canvas;
pub(crate) use container::Container;
pub(crate) use layout::LayoutManager;
pub(crate) use std::{
    cell::RefCell,
    cmp::{max, min},
    hash::Hash,
    rc::Rc,
};
pub(crate) use container_handle::{container_id_of, ContainerId};
pub(crate) use style::UNCLIPPED_RECT;
