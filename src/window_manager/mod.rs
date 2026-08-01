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
//! Top-level retained UI context.
//!
//! `Context` owns the high-level renderer, global input, window-manager state, and the published
//! retained roots driven by each frame.
use std::{cell::RefCell, rc::Rc};

use bitflags::bitflags;
#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;

#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{ColorType, Decoder};

use crate::input::Input;
use crate::{rect, Dimensioni, ImageSource, KeyCode, KeyMode, MouseButton, Recti, Style, TextureId, UiRuntime};
use crate::render::{CustomRenderArgs, CustomRenderHandle, CustomRenderRegistryError, DisplayList, FrameInfo, RenderError, Renderer, RendererBackend};
use window_manager::WindowEntry;
mod input_api;
mod root_chrome;
mod window_manager;

pub use root_chrome::{RootHandle, RootMutationError, RootState};

bitflags! {
    #[derive(Copy, Clone)]
    /// Options that control a root window, dialog, or popup.
    pub struct WindowOption : u32 {
        /// Gives the root a Style-owned outer border and inset content area.
        const FRAME = 1024;
        /// Automatically adapts the root size to its content.
        const AUTO_SIZE = 512;
        /// Hides the title bar.
        const NO_TITLE = 128;
        /// Hides the close button.
        const NO_CLOSE = 64;
        /// Prevents the user from resizing the root.
        const NO_RESIZE = 16;
        /// No special options.
        const NONE = 0;
    }
}

/// Opaque identifier for a root window, dialog, or popup registered with [`Context`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RootId(usize);

impl RootId {
    /// Wraps a raw counter value as a root identifier.
    pub(crate) const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }
}

/// Primary entry point used to drive the UI over a rendering backend.
///
/// `Context` is the only public ordered input-queue boundary. Input forwarding calls append raw
/// transitions without coalescing; [`Context::update_ui`] drains them in call order. The exception
/// to ordinary root hit routing is the popup boundary: an outside pointer press dismisses the
/// active popup before the event may continue to the root underneath.
///
/// `Context`, its retained state, and its registered custom-render callbacks stay on the thread
/// that owns the context. The rendering contracts intentionally do not require `Send` or `Sync`;
/// applications should deliver any cross-thread results before starting a [`ContextFrame`].
///
/// A live [`ContextFrame`] exclusively owns the Context borrow, preventing input/resource
/// mutation or another logical frame until it is rendered or cancelled:
///
/// ```compile_fail
/// use microui_redux::Context;
/// use microui_redux::render::{FrameInfo, RendererBackend};
///
/// fn mutate_during_frame<B: RendererBackend>(context: &mut Context<B>, info: FrameInfo) {
///     let frame = context.frame(info);
///     context.mousemove(10, 20);
///     drop(frame);
/// }
/// ```
pub struct Context<B: RendererBackend> {
    /// High-level renderer that replays root display lists.
    renderer: Renderer<B>,
    /// Reusable operation storage for window-manager frame and chrome drawing.
    display_list: DisplayList,
    /// Shared style used by all roots and scroll areas.
    style: Rc<Style>,

    /// Highest z-index allocated to an open window-manager root.
    last_zindex: i32,
    /// Registered window-manager roots replayed by [`ContextFrame::render_ui`].
    roots: Vec<WindowEntry>,
    /// Next root id counter.
    next_root_id: usize,
    /// Context-owned file-dialog controllers advanced after retained input updates.
    pub(crate) file_dialogs: Vec<crate::file_dialog::FileDialogController>,
    /// Next file-dialog session id counter.
    pub(crate) next_file_dialog_id: usize,
    /// Shared input state mutated by public input APIs and consumed during traversal.
    input: Rc<RefCell<Input>>,
    /// Dimensions of the most recent complete update/layout commit.
    ui_commit: Option<Dimensioni>,
    /// Drawable size used by retained behavior tests that drive complete frames tersely.
    #[cfg(test)]
    test_dimensions: Dimensioni,
}

impl<B: RendererBackend> Context<B> {
    /// Creates a new UI context with unique ownership of the provided backend.
    pub fn new(backend: B) -> Self {
        // The backend supplies the atlas; the default style then binds semantic font roles from it.
        let renderer = Renderer::new(backend);
        let style = Style::default().with_named_fonts(&renderer.atlas());
        Self {
            renderer,
            display_list: DisplayList::new(),
            style: Rc::new(style),
            last_zindex: 0,
            roots: Vec::default(),
            next_root_id: 1,
            file_dialogs: Vec::new(),
            next_file_dialog_id: 1,
            input: Rc::new(RefCell::new(Input::default())),
            ui_commit: None,
            #[cfg(test)]
            test_dimensions: Dimensioni::new(1, 1),
        }
    }

    /// Creates a Context whose test-only frame helper uses `dimensions`.
    #[cfg(test)]
    pub(crate) fn new_test(backend: B, dimensions: Dimensioni) -> Self {
        let mut context = Self::new(backend);
        context.test_dimensions = dimensions;
        context
    }

    /// Updates and renders once for retained behavior tests.
    #[cfg(test)]
    pub(crate) fn update_and_render_ui(&mut self) {
        let info = FrameInfo::try_new(self.test_dimensions, crate::color(0, 0, 0, 0)).expect("test Context dimensions must be positive");
        self.update_ui(self.test_dimensions);
        self.frame(info).render_ui().expect("test backend frame should render");
    }
}

/// Exclusively owned logical UI frame.
///
/// This value borrows `Context` to serialize paint/submission, but it does not lock independent
/// [`crate::WidgetStateHandle`] or [`RootHandle::state`] access. Mutating layout-affecting state after the
/// last update commit makes that commit semantically stale; drop the unsubmitted frame and call
/// [`Context::update_ui`] again before painting. No separate Context token exists.
///
/// Submission consumes the frame, making a second submission unrepresentable:
///
/// ```compile_fail
/// use microui_redux::Context;
/// use microui_redux::render::{FrameInfo, RendererBackend};
///
/// fn submit_twice<B: RendererBackend>(context: &mut Context<B>, info: FrameInfo) {
///     let frame = context.frame(info);
///     frame.render_ui().unwrap();
///     frame.render_ui().unwrap();
/// }
/// ```
#[must_use = "call render_ui() to submit this UI frame; dropping it cancels"]
pub struct ContextFrame<'a, B: RendererBackend> {
    context: &'a mut Context<B>,
    info: FrameInfo,
    completed: bool,
}

#[cfg(test)]
mod root_tests;

impl<B: RendererBackend> Context<B> {
    /// Starts one paint/submission frame for state previously committed by [`Context::update_ui`].
    pub fn frame(&mut self, info: FrameInfo) -> ContextFrame<'_, B> {
        ContextFrame { context: self, info, completed: false }
    }

    /// Drains ordered input and commits retained state plus layout for `dimensions`.
    ///
    /// One synchronization layout always runs first. Each queued input event then causes exactly
    /// one route followed by one full eligible-tree update and another layout commit. Geometry
    /// produced for one event is therefore authoritative when routing the next. With an empty
    /// queue, the initial layout is the complete synchronization commit. This method performs no
    /// timer synthesis, painting, or backend submission.
    ///
    /// Context-owned input, style, and root mutations invalidate a prior commit automatically.
    /// Mutations made through weak widget/container state handles cannot notify Context; callers
    /// must invoke this method after those mutations, including when the input queue is empty.
    #[track_caller]
    pub fn update_ui(&mut self, dimensions: Dimensioni) {
        assert!(dimensions.width > 0 && dimensions.height > 0, "update_ui dimensions must be positive");
        self.ui_commit = None;
        self.update_window_manager(dimensions);
        self.ui_commit = Some(dimensions);
    }

    /// Invalidates any layout commit known to have been affected through a Context API.
    pub(super) fn invalidate_ui_commit(&mut self) {
        self.ui_commit = None;
    }

    /// Registers one backend-specific callback for retained custom-render nodes.
    ///
    /// A callback written for another backend frame type cannot be registered:
    ///
    /// ```compile_fail
    /// use microui_redux::{Context, CustomRenderArgs};
    /// use microui_redux::render::RendererBackend;
    ///
    /// fn register_for_wrong_backend<A, B, F>(context: &mut Context<A>, callback: F)
    /// where
    ///     A: RendererBackend,
    ///     B: RendererBackend,
    ///     F: for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs) + 'static,
    /// {
    ///     context.register_custom_renderer(callback).unwrap();
    /// }
    /// ```
    ///
    /// The active frame borrow cannot escape the callback invocation:
    ///
    /// ```compile_fail
    /// use microui_redux::{Context, CustomRenderArgs};
    /// use microui_redux::render::RendererBackend;
    ///
    /// fn retain_frame<B: RendererBackend>(context: &mut Context<B>) {
    ///     let mut retained = None;
    ///     context.register_custom_renderer(
    ///         move |frame: &mut B::Frame<'_>, _args: CustomRenderArgs| {
    ///             retained = Some(frame);
    ///         },
    ///     ).unwrap();
    /// }
    /// ```
    pub fn register_custom_renderer<F>(&mut self, callback: F) -> Result<CustomRenderHandle<B>, CustomRenderRegistryError>
    where
        F: for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs) + 'static,
    {
        self.renderer.register_custom_renderer(callback)
    }

    /// Removes a previously registered custom-render callback.
    pub fn unregister_custom_renderer(&mut self, handle: CustomRenderHandle<B>) -> Result<(), CustomRenderRegistryError> {
        self.renderer.unregister_custom_renderer(handle)
    }

    /// Replaces the current UI style.
    ///
    /// Unset/default font fields are rebound automatically from the current atlas when it exposes
    /// the conventional `body` / `small` / `title` / `heading` / `mono` font names. Use
    /// [`Style::with_named_fonts`] or [`Style::bind_named_fonts`] when you want to force all
    /// semantic roles to those atlas bindings explicitly.
    pub fn set_style(&mut self, style: &Style) {
        let mut resolved = style.clone();
        resolved.bind_default_named_fonts(&self.renderer.atlas());
        self.style = Rc::new(resolved);
        self.invalidate_ui_commit();
    }

    /// Returns the high-level renderer used for frame execution and resource management.
    ///
    /// Application code should prefer the higher-level context image APIs and retained widget
    /// rendering. Backend integrations can use this accessor for atlas metadata.
    pub fn renderer(&self) -> &Renderer<B> {
        &self.renderer
    }

    /// Attempts to upload an RGBA image to the renderer and returns its [`TextureId`].
    ///
    /// Dimensions and byte length are validated before an id is allocated. Backend upload errors
    /// are returned without recording texture state in the renderer.
    pub fn try_load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, String> {
        self.renderer.try_load_texture_rgba(width, height, pixels)
    }

    /// Uploads an RGBA image to the renderer and returns its [`TextureId`].
    ///
    /// Panics if the RGBA dimensions/byte length are invalid or the backend rejects the upload.
    /// Prefer [`Context::try_load_image_rgba`] when callers can handle upload failure.
    #[track_caller]
    pub fn load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId {
        self.try_load_image_rgba(width, height, pixels).expect("failed to upload RGBA image")
    }

    /// Deletes a previously uploaded texture.
    ///
    /// Deleting an unknown or already-freed handle triggers a debug assertion and is an idempotent
    /// no-op in release builds.
    pub fn free_image(&mut self, id: TextureId) {
        self.renderer.free_texture(id);
    }

    /// Uploads texture data described by `source`. PNG decoding is only available when the
    /// `png_source` (or `builder`) feature is enabled.
    pub fn load_image_from(&mut self, source: ImageSource) -> Result<TextureId, String> {
        match source {
            ImageSource::Raw { width, height, pixels } => self.try_load_image_rgba(width, height, pixels),
            #[cfg(any(feature = "builder", feature = "png_source"))]
            ImageSource::Png { bytes } => {
                let (width, height, rgba) = Self::decode_png(bytes)?;
                self.try_load_image_rgba(width, height, rgba.as_slice())
            }
        }
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    /// Decodes PNG bytes into RGBA pixels for renderer texture upload.
    fn decode_png(bytes: &[u8]) -> Result<(i32, i32, Vec<u8>), String> {
        let cursor = Cursor::new(bytes);
        let decoder = Decoder::new(cursor);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let buf_size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG decoder did not report output size".to_string())?;
        let mut buf = vec![0; buf_size];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let raw = &buf[..info.buffer_size()];
        let width = i32::try_from(info.width).map_err(|_| String::from("PNG width exceeds supported range"))?;
        let height = i32::try_from(info.height).map_err(|_| String::from("PNG height exceeds supported range"))?;
        let mut rgba = Vec::with_capacity(crate::atlas::checked_rgba_byte_len(width, height)?);
        match info.color_type {
            ColorType::Rgba => rgba.extend_from_slice(raw),
            ColorType::Rgb => {
                // Expand RGB to opaque RGBA so the renderer texture upload has one format.
                for chunk in raw.chunks(3) {
                    rgba.extend_from_slice(chunk);
                    rgba.push(0xFF);
                }
            }
            ColorType::Grayscale => {
                // Treat grayscale input as opaque luminance.
                for &v in raw {
                    rgba.extend_from_slice(&[v, v, v, 0xFF]);
                }
            }
            ColorType::GrayscaleAlpha => {
                // Preserve grayscale alpha while expanding luminance into RGB channels.
                for chunk in raw.chunks(2) {
                    let v = chunk[0];
                    let a = chunk[1];
                    rgba.extend_from_slice(&[v, v, v, a]);
                }
            }
            _ => {
                return Err("Unsupported PNG color type".into());
            }
        }
        Ok((width, height, rgba))
    }
}

impl<B: RendererBackend> ContextFrame<'_, B> {
    /// Consumes this logical frame, paints the last committed UI once, and submits it once.
    ///
    /// Returns [`RenderError::UiUpdateRequired`] before paint or backend acquisition when no commit
    /// exists for these dimensions or when raw input is pending. This operation is paint-only: it
    /// does not route input, update semantic state, run layout, synthesize timers, or produce a
    /// generic frame-result/resource-state object.
    pub fn render_ui(mut self) -> Result<(), RenderError> {
        let dimensions = self.info.dimensions();
        let commit_matches = self
            .context
            .ui_commit
            .is_some_and(|committed| (committed.width, committed.height) == (dimensions.width, dimensions.height));
        if !commit_matches || self.context.input.borrow().has_pending() {
            self.completed = true;
            return Err(RenderError::UiUpdateRequired);
        }
        self.context.paint_window_manager(self.info.dimensions());
        let Context { renderer, display_list, .. } = &mut *self.context;
        let result = renderer.render(self.info, display_list);
        self.completed = true;
        result
    }
}

impl<B: RendererBackend> Drop for ContextFrame<'_, B> {
    fn drop(&mut self) {
        if !self.completed {
            self.context.display_list.clear();
        }
    }
}
