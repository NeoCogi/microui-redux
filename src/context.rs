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
//! `Context` owns renderer-facing canvas state, global input, root windows/dialogs/popups, and the
//! published frame results that application code reads after each retained update.
use std::{cell::RefCell, rc::Rc};

#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;

#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{ColorType, Decoder};

use crate::{
    rect, Canvas, Color, ContainerOption, Dimensioni, FrameResultGeneration, FrameResults, ImageSource, Input, KeyCode, KeyMode, MouseButton, Recti, Renderer,
    RendererHandle, ScrollArea, ScrollAreaHandle, ScrollBehavior, Style, TextureId, WidgetTree, WindowHandle,
};
use roots::RootEntry;
#[cfg(test)]
use crate::window::WindowChromeIds;

#[cfg(test)]
use crate::{UNCLIPPED_RECT, Vec2i};

mod input_api;
mod roots;

/// Opaque identifier for a root window, dialog, or popup registered with [`Context`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RootId(usize);

impl RootId {
    /// Wraps a raw counter value as a root identifier.
    pub(crate) const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    /// Returns the raw counter value for stable internal hashing.
    pub(crate) fn raw(self) -> usize {
        self.0
    }
}

/// Primary entry point used to drive the UI over a renderer implementation.
pub struct Context<R: Renderer> {
    canvas: Canvas<R>,
    style: Rc<Style>,

    last_zindex: i32,
    frame: usize,
    hover_root: Option<WindowHandle>,
    next_hover_root: Option<WindowHandle>,

    root_list: Vec<WindowHandle>,
    retained_roots: Vec<RootEntry>,
    next_root_id: usize,
    frame_results: FrameResults,

    input: Rc<RefCell<Input>>,
}

impl<R: Renderer> Context<R> {
    /// Creates a new UI context around the provided renderer and dimensions.
    pub fn new(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        // The renderer supplies the atlas; the default style then binds semantic font roles from it.
        let canvas = Canvas::from(renderer, dim);
        let style = Style::default().with_named_fonts(&canvas.get_atlas());
        Self {
            canvas,
            style: Rc::new(style),
            last_zindex: 0,
            frame: 0,
            hover_root: None,
            next_hover_root: None,

            root_list: Vec::default(),
            retained_roots: Vec::default(),
            next_root_id: 1,
            frame_results: FrameResults::default(),

            input: Rc::new(RefCell::new(Input::default())),
        }
    }
}

#[cfg(test)]
mod tests;

impl<R: Renderer> Context<R> {
    /// Begins a renderer draw pass for the current viewport.
    ///
    /// Call this once after determining the viewport size and before presenting UI commands for
    /// the frame. Input events may be collected before or after this call, as long as
    /// [`Context::update_ui`] runs after the input state has been updated.
    pub fn begin_render_frame(&mut self, width: i32, height: i32, clr: Color) {
        self.canvas.begin(width, height, clr);
    }

    /// Flushes recorded root commands to the renderer and ends the draw pass.
    pub fn end_render_frame(&mut self) {
        for r in &mut self.root_list {
            r.render(&mut self.canvas);
        }
        self.canvas.end()
    }

    /// Returns a handle to the underlying renderer.
    pub fn renderer_handle(&self) -> RendererHandle<R> {
        self.canvas.renderer_handle()
    }

    #[inline(never)]
    /// Starts a logical UI frame and clears transient root/render state.
    fn frame_begin(&mut self) {
        self.frame_results.begin_frame();
        self.input.borrow_mut().prelude();
        self.frame += 1;
        self.root_list.clear();
    }

    #[inline(never)]
    /// Finishes root traversal, publishes results, and prepares hover/z-order for the next frame.
    fn frame_end(&mut self) {
        for r in &mut self.root_list {
            r.finish();
        }
        self.frame_results.finish_frame();

        let mouse_pressed = self.input.borrow().mouse_pressed;
        let next_front = match (mouse_pressed.is_none(), &self.next_hover_root) {
            (false, Some(next_hover_root)) if next_hover_root.zindex() < self.last_zindex && next_hover_root.zindex() >= 0 => Some(next_hover_root.clone()),
            _ => None,
        };
        if let Some(next_hover_root) = next_front {
            // Clicking a window brings it forward after all roots have had a chance to report hover.
            self.bring_to_front(&next_hover_root);
        }

        self.input.borrow_mut().epilogue();

        // Promote the next hover root after input epilogue so current-frame routing stays stable.
        self.hover_root = self.next_hover_root.clone();
        self.next_hover_root = None;
        for r in &mut self.root_list {
            r.set_root_hover_active(false);
        }
        match &mut self.hover_root {
            Some(window) => window.set_root_hover_active(true),
            _ => (),
        }

        // Sort all windows by z-index so render order matches interaction order.
        self.root_list.sort_by(|a, b| a.zindex().cmp(&b.zindex()));
    }

    /// Runs one UI frame using only roots previously registered with this context.
    ///
    /// Applications create roots once with [`Context::create_window`],
    /// [`Context::create_dialog`], or [`Context::create_popup`], mutate widget handle state over
    /// time, and call this method each frame without re-submitting root trees.
    pub fn update_ui(&mut self) {
        self.frame_begin();
        self.render_registered_roots();
        self.frame_end();
    }

    /// Returns the previous frame's published widget results.
    ///
    /// This is the public business-logic view of retained interaction state.
    /// App code should react to this generation after rendering, accepting the
    /// one-frame delay as part of the retained pipeline contract.
    pub fn committed_results(&self) -> FrameResultGeneration<'_> {
        self.frame_results.committed()
    }

    /// Returns the in-progress result generation being written by the current frame.
    ///
    /// This is mainly useful for framework internals or advanced debugging.
    /// Normal application/business logic should prefer [`Context::committed_results`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current_results(&self) -> FrameResultGeneration<'_> {
        self.frame_results.current()
    }

    /// Replaces the current UI style.
    ///
    /// Unset/default font fields are rebound automatically from the current atlas when it exposes
    /// the conventional `body` / `small` / `title` / `heading` / `mono` font names. Use
    /// [`Style::with_named_fonts`] or [`Style::bind_named_fonts`] when you want to force all
    /// semantic roles to those atlas bindings explicitly.
    pub fn set_style(&mut self, style: &Style) {
        let mut resolved = style.clone();
        resolved.bind_default_named_fonts(&self.canvas.get_atlas());
        self.style = Rc::new(resolved)
    }

    /// Returns the underlying canvas used for advanced backend inspection.
    ///
    /// Application code should prefer the higher-level context image APIs and retained widget
    /// rendering. Backend tests can name this type as [`crate::backend::Canvas`].
    pub fn canvas(&self) -> &crate::backend::Canvas<R> {
        &self.canvas
    }

    /// Attempts to upload an RGBA image to the renderer and returns its [`TextureId`].
    ///
    /// Dimensions and byte length are validated before an id is allocated. Backend upload errors
    /// are returned without recording texture state in the canvas.
    pub fn try_load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, String> {
        self.canvas.try_load_texture_rgba(width, height, pixels)
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
    pub fn free_image(&mut self, id: TextureId) {
        self.canvas.free_texture(id);
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
