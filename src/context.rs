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
    rect, Canvas, Color, Container, ContainerHandle, ContainerOption, Dimensioni, FrameResultGeneration, FrameResults, ImageSource, Input, KeyCode, KeyMode,
    MouseButton, Recti, Renderer, RendererHandle, ScrollBehavior, Style, TextureId, WidgetTree, WindowHandle,
};
#[cfg(test)]
use crate::window::WindowChromeIds;

#[cfg(test)]
use crate::{UNCLIPPED_RECT, Vec2i};

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Runtime behavior class for a registered root.
enum RootKind {
    Window,
    Dialog,
    Popup,
}

/// Registered retained root plus the tree/options needed to render it every frame.
struct RootEntry {
    /// Stable application-facing identifier for this root.
    id: RootId,
    /// Window/dialog/popup runtime handle containing container state.
    handle: WindowHandle,
    /// Retained tree rendered inside the root body.
    tree: WidgetTree,
    /// Container chrome/sizing options.
    opt: ContainerOption,
    /// Scroll behavior applied to the root body.
    scroll_behavior: ScrollBehavior,
    /// Whether this root should be considered during frame traversal.
    visible: bool,
    /// Behavior class for opening, closing, and hover routing.
    kind: RootKind,
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

    /// Updates the current mouse pointer position.
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.input.borrow_mut().mousemove(x, y);
    }

    /// Records that the specified mouse button was pressed.
    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input.borrow_mut().mousedown(x, y, btn);
    }

    /// Records that the specified mouse button was released.
    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input.borrow_mut().mouseup(x, y, btn);
    }

    /// Accumulates scroll wheel movement.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.input.borrow_mut().scroll(x, y);
    }

    /// Records that a modifier key was pressed.
    pub fn keydown(&mut self, key: KeyMode) {
        self.input.borrow_mut().keydown(key);
    }

    /// Records that a modifier key was released.
    pub fn keyup(&mut self, key: KeyMode) {
        self.input.borrow_mut().keyup(key);
    }

    /// Records that a navigation key was pressed.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.input.borrow_mut().keydown_code(code);
    }

    /// Records that a navigation key was released.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.input.borrow_mut().keyup_code(code);
    }

    /// Appends UTF-8 text to the input buffer.
    pub fn text(&mut self, text: &str) {
        self.input.borrow_mut().text(text);
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
        match (mouse_pressed.is_none(), &self.next_hover_root) {
            (false, Some(next_hover_root)) if next_hover_root.zindex() < self.last_zindex && next_hover_root.zindex() >= 0 => {
                // Clicking a window brings it forward after all roots have had a chance to report hover.
                self.bring_to_front(&mut next_hover_root.clone());
            }
            _ => (),
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

    /// Creates an open top-level window handle with a new root id.
    fn new_window(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let root_id = self.next_root_id();
        let mut window = WindowHandle::window(root_id, name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect);
        self.bring_to_front(&mut window);
        window
    }

    /// Creates a hidden dialog handle with a new root id.
    fn new_dialog(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let root_id = self.next_root_id();
        WindowHandle::dialog(root_id, name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect)
    }

    /// Creates a hidden popup handle with a new root id.
    fn new_popup(&mut self, name: &str) -> WindowHandle {
        let root_id = self.next_root_id();
        WindowHandle::popup(root_id, name, self.canvas.get_atlas(), self.style.clone(), self.input.clone())
    }

    /// Creates a retained panel handle for use with [`crate::WidgetTreeBuilder::container`].
    ///
    /// The handle owns panel-local focus, hover, scroll, layout cache, and draw commands across
    /// frames; application code supplies its children through the retained tree.
    pub fn new_panel(&mut self, name: &str) -> ContainerHandle {
        ContainerHandle::new(Container::new(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone()))
    }

    /// Allocates the next stable root id.
    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained root id counter overflowed");
        id
    }

    /// Stores a retained root entry and returns the handle's stable root id.
    fn register_root(
        &mut self,
        kind: RootKind,
        handle: WindowHandle,
        tree: WidgetTree,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        visible: bool,
    ) -> RootId {
        let id = handle.root_id();
        // The handle owns mutable runtime state; the tree remains replaceable by app code.
        self.retained_roots.push(RootEntry {
            id,
            handle,
            tree,
            opt,
            scroll_behavior,
            visible,
            kind,
        });
        id
    }

    /// Finds a mutable registered root by stable id.
    fn root_entry_mut(&mut self, root: RootId) -> Option<&mut RootEntry> {
        self.retained_roots.iter_mut().find(|entry| entry.id == root)
    }

    /// Registers an open retained window and returns its stable root identifier.
    ///
    /// The window is rendered by subsequent calls to [`Context::update_ui`] without the
    /// application re-submitting its tree.
    pub fn create_window(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        let window = self.new_window(name, rect);
        self.register_root(RootKind::Window, window, tree, ContainerOption::NONE, ScrollBehavior::NONE, true)
    }

    /// Registers a retained dialog root.
    ///
    /// Dialogs start hidden; call [`Context::set_root_visible`] with `true` to open the dialog and
    /// bring it to the front.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        let dialog = self.new_dialog(name, rect);
        self.register_root(RootKind::Dialog, dialog, tree, ContainerOption::NONE, ScrollBehavior::NONE, false)
    }

    /// Registers a retained popup root.
    ///
    /// Popups start hidden; calling [`Context::set_root_visible`] with `true` opens the popup at the
    /// current mouse position.
    pub fn create_popup(&mut self, name: &str, tree: WidgetTree) -> RootId {
        let popup = self.new_popup(name);
        self.register_root(RootKind::Popup, popup, tree, Self::default_popup_options(), ScrollBehavior::NONE, false)
    }

    /// Replaces the retained widget tree for a registered root.
    ///
    /// Invalid root identifiers are ignored.
    pub fn set_root_tree(&mut self, root: RootId, tree: WidgetTree) {
        if let Some(entry) = self.root_entry_mut(root) {
            entry.tree = tree;
        }
    }

    /// Replaces the container options and scroll behavior for a registered root.
    ///
    /// Popups are created with the default retained popup options. This method can override those
    /// defaults for retained roots that need custom chrome or sizing.
    pub fn set_root_options(&mut self, root: RootId, opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        if let Some(entry) = self.root_entry_mut(root) {
            entry.opt = opt;
            entry.scroll_behavior = scroll_behavior;
        }
    }

    /// Shows or hides a registered retained root.
    ///
    /// Windows reopen in their existing z-order. Dialogs and newly opened popups are brought to the
    /// front.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) {
        let Some(index) = self.retained_roots.iter().position(|entry| entry.id == root) else {
            return;
        };
        self.set_root_visible_at(index, visible);
    }

    /// Returns the window handle owned by a registered retained root.
    pub fn root_handle(&self, root: RootId) -> Option<WindowHandle> {
        self.retained_roots.iter().find(|entry| entry.id == root).map(|entry| entry.handle.clone())
    }

    /// Bumps the window's Z order so it renders above others.
    pub fn bring_to_front(&mut self, window: &mut WindowHandle) {
        self.last_zindex += 1;
        window.set_zindex(self.last_zindex);
    }

    /// Applies visibility changes by index so callers can avoid a second root lookup.
    fn set_root_visible_at(&mut self, index: usize, visible: bool) {
        let mouse_pos = self.input.borrow().mouse_pos;
        let mut bring_to_front = None;
        let mut hover_root = None;

        {
            let entry = &mut self.retained_roots[index];
            if !visible {
                entry.visible = false;
                entry.handle.close();
                return;
            }

            let was_open = entry.handle.is_open();
            entry.visible = true;
            match entry.kind {
                RootKind::Window => {
                    // Windows preserve their z-order when reopened.
                    entry.handle.open();
                }
                RootKind::Dialog => {
                    entry.handle.open();
                    if !was_open {
                        // Newly opened dialogs should float above normal windows.
                        bring_to_front = Some(entry.handle.clone());
                    }
                }
                RootKind::Popup => {
                    if !was_open {
                        // Popups anchor at the current pointer and become the hover root for the
                        // opening frame so their first click does not immediately close them.
                        entry.handle.set_rect(rect(mouse_pos.x, mouse_pos.y, 1, 1));
                        entry.handle.open();
                        entry.handle.set_root_hover_active(true);
                        entry.handle.mark_popup_just_opened();
                        bring_to_front = Some(entry.handle.clone());
                        hover_root = Some(entry.handle.clone());
                    }
                }
            }
        }

        if let Some(mut window) = bring_to_front {
            self.bring_to_front(&mut window);
        }
        if let Some(window) = hover_root {
            self.next_hover_root = Some(window.clone());
            self.hover_root = Some(window);
        }
    }

    /// Brings a window forward only when it is not already top-most.
    fn bring_to_front_if_behind(&mut self, window: &mut WindowHandle) {
        if window.zindex() < self.last_zindex {
            self.bring_to_front(window);
        }
    }

    #[inline(never)]
    /// Starts command recording and hover/scroll routing for a root container.
    fn begin_root_container(&mut self, window: &mut WindowHandle) {
        window.prepare_for_frame(self.frame);
        self.root_list.push(window.clone());

        // Highest z-index root under the pointer becomes next frame's hover root.
        if window.root_contains_point(self.input.borrow().mouse_pos)
            && (self.next_hover_root.is_none() || window.zindex() > self.next_hover_root.as_ref().unwrap().zindex())
        {
            self.next_hover_root = Some(window.clone());
        }
        let scroll_delta = self.input.borrow().scroll_delta;
        let pending_scroll = if window.root_in_hover_root() && (scroll_delta.x != 0 || scroll_delta.y != 0) {
            Some(scroll_delta)
        } else {
            None
        };
        window.begin_root_command_scope(pending_scroll);
    }

    #[inline(never)]
    /// Ends command recording for a root container.
    fn end_root_container(&mut self, window: &mut WindowHandle) {
        window.finish_root_command_scope();
    }

    #[inline(never)]
    #[must_use]
    /// Opens a root for retained traversal and returns whether its body should be rendered.
    fn begin_window(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior) -> bool {
        if !window.is_open() {
            return false;
        }

        if !self.update_popup_root_state(window) {
            return false;
        }

        self.begin_root_container(window);
        window.begin_window(&mut self.frame_results, opt, scroll_behavior);

        true
    }

    /// Completes retained traversal for a root and applies resize results.
    fn end_window(&mut self, window: &mut WindowHandle, opt: ContainerOption) {
        window.end_window();
        self.end_root_container(window);
        window.finish_resize(&mut self.frame_results, opt);
    }

    /// Handles popup auto-close behavior before a popup root is traversed.
    fn update_popup_root_state(&mut self, window: &mut WindowHandle) -> bool {
        if !window.root_is_popup() {
            return true;
        }

        if window.root_popup_just_opened() {
            window.clear_root_popup_just_opened();
            return true;
        }

        let click_outside_popup = {
            let input = self.input.borrow();
            // A popup closes only on a press outside both its hover root and rectangle.
            !input.mouse_pressed.is_none() && !window.root_in_hover_root() && !window.root_contains_point(input.mouse_pos)
        };
        if click_outside_popup {
            window.close();
            return false;
        }

        true
    }

    /// Measures, begins, traverses, and ends one window-like retained tree.
    fn render_window_tree(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        if window.is_open() {
            window.set_root_style(self.style.clone());
            if opt.is_auto_sizing() {
                // Auto-size is measured against committed previous-frame results before the live traversal.
                window.measure_auto_size(&self.frame_results, opt, scroll_behavior, tree);
            }
        }

        if self.begin_window(window, opt, scroll_behavior) {
            {
                let mut inner = window.inner_mut();
                // Widget traversal records layout, interaction, and draw commands into the root container.
                inner.main.widget_tree(&mut self.frame_results, tree);
            }
            self.end_window(window, opt);

            if !window.is_open() {
                window.reset_after_close();
            }
        }
    }

    /// Renders an open dialog and forces it to remain the active hover/root focus layer.
    fn render_dialog_tree(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        if window.is_open() {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            window.set_root_hover_active(true);
            self.bring_to_front_if_behind(window);

            self.render_window_tree(window, opt, scroll_behavior, tree);
        }
    }

    /// Renders one registered retained root if it is visible and still open.
    fn render_retained_root(&mut self, entry: &mut RootEntry) {
        if !entry.visible {
            return;
        }
        if !entry.handle.is_open() {
            entry.visible = false;
            return;
        }

        match entry.kind {
            RootKind::Window => self.render_window_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Dialog => self.render_dialog_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Popup => self.render_window_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
        }

        if !entry.handle.is_open() {
            entry.visible = false;
        }
    }

    /// Traverses all retained roots without borrowing `self.retained_roots` during rendering.
    fn render_registered_roots(&mut self) {
        // Rendering needs `&mut self` for z-order, hover, and frame results, so take the root list
        // out temporarily to avoid aliasing the vector while entries are rendered.
        let mut roots = std::mem::take(&mut self.retained_roots);
        for entry in &mut roots {
            self.render_retained_root(entry);
        }
        self.retained_roots = roots;
    }

    /// Returns the chrome options used by retained popups unless the app overrides them.
    const fn default_popup_options() -> ContainerOption {
        ContainerOption::AUTO_SIZE.union(ContainerOption::NO_RESIZE).union(ContainerOption::NO_TITLE)
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
