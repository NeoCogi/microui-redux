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
use super::*;
use crate::{
    context::RootId,
    widget::FrameResults,
    widget_tree::{NodeInteraction, NodeLayout, WidgetTree},
};
use std::cell::{Ref, RefMut};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct WindowChromeIds {
    pub(crate) title: Id,
    pub(crate) close: Id,
    pub(crate) resize: Id,
}

impl WindowChromeIds {
    pub(crate) fn from_root_seed(seed: usize) -> Self {
        Self {
            title: chrome_node_id(seed, ChromePart::Title),
            close: chrome_node_id(seed, ChromePart::Close),
            resize: chrome_node_id(seed, ChromePart::Resize),
        }
    }

    fn from_root(root_id: RootId) -> Self {
        Self::from_root_seed(root_id.raw())
    }
}

#[derive(Copy, Clone)]
enum ChromePart {
    Title,
    Close,
    Resize,
}

fn chrome_node_id(seed: usize, part: ChromePart) -> Id {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    fn write(mut hash: u64, value: u64) -> u64 {
        for byte in value.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    let part = match part {
        ChromePart::Title => 1,
        ChromePart::Close => 2,
        ChromePart::Resize => 3,
    };

    let hash = write(FNV_OFFSET_BASIS, 0x6d69_6372_6f75_695f_u64);
    let hash = write(hash, 0x726f_6f74_u64);
    let hash = write(hash, seed as u64);
    Id::new(write(hash, part))
}

fn chrome_result(control: &ControlState, submit_on_click: bool) -> ResourceState {
    let mut result = ResourceState::NONE;
    if submit_on_click && control.clicked {
        result |= ResourceState::SUBMIT;
    }
    if control.active {
        result |= ResourceState::ACTIVE;
    }
    result
}

#[derive(Copy, Clone)]
enum WindowChromePart {
    Title,
    Close,
    Resize,
}

#[derive(Copy, Clone)]
struct WindowChromeNode {
    id: Id,
    part: WindowChromePart,
    rect: Recti,
}

impl WindowChromeNode {
    fn new(id: Id, part: WindowChromePart, rect: Recti) -> Self {
        Self { id, part, rect }
    }
}

struct WindowChromeTree {
    ids: WindowChromeIds,
    title_state: Internal,
    close_state: Internal,
    resize_state: Internal,
}

impl WindowChromeTree {
    fn new(ids: WindowChromeIds) -> Self {
        Self {
            ids,
            title_state: Internal::new("!title"),
            close_state: Internal::new("!close"),
            resize_state: Internal::new("!resize"),
        }
    }

    fn title_node(&self, container: &Container, opt: ContainerOption) -> Option<WindowChromeNode> {
        if opt.has_no_title() {
            return None;
        }

        let mut rect = container.rect();
        rect.height = Window::titlebar_height(container);
        Some(WindowChromeNode::new(self.ids.title, WindowChromePart::Title, rect))
    }

    fn close_node(&self, title_rect: Recti, opt: ContainerOption) -> Option<WindowChromeNode> {
        if opt.has_no_close() {
            return None;
        }

        let rect = rect(
            title_rect.x + title_rect.width - title_rect.height,
            title_rect.y,
            title_rect.height,
            title_rect.height,
        );
        Some(WindowChromeNode::new(self.ids.close, WindowChromePart::Close, rect))
    }

    fn resize_node(&self, container: &Container, opt: ContainerOption) -> Option<WindowChromeNode> {
        if opt.is_auto_sizing() || opt.is_fixed() {
            return None;
        }

        let size = container.style().title_height;
        let container_rect = container.rect();
        let rect = rect(
            container_rect.x + container_rect.width - size,
            container_rect.y + container_rect.height - size,
            size,
            size,
        );
        Some(WindowChromeNode::new(self.ids.resize, WindowChromePart::Resize, rect))
    }

    fn dispatch_node(
        container: &mut Container,
        results: &mut FrameResults,
        node: WindowChromeNode,
        state: &mut Internal,
        dispatch_site: &'static str,
    ) -> ControlState {
        container.record_tree_layout(
            node.id,
            NodeLayout::new(node.rect, node.rect, Dimensioni::new(node.rect.width, node.rect.height)),
        );
        let (control, widget_result) = container.execute_internal_node(node.id, state, node.rect);
        let submit_on_click = matches!(node.part, WindowChromePart::Close);
        let result = widget_result | chrome_result(&control, submit_on_click);
        container.record_tree_interaction(node.id, NodeInteraction::new(control, result));
        results.record_node_with_context(container.retained_id_for_node(node.id), node.id, result, dispatch_site);
        control
    }

    fn render_title_bar(&mut self, container: &mut Container, results: &mut FrameResults, win_state: &mut WindowState, opt: ContainerOption) {
        let Some(title_node) = self.title_node(container, opt) else {
            return;
        };

        let title_text_color = container.style().colors[ControlColor::TitleText as usize];
        container.draw_frame(title_node.rect, ControlColor::TitleBG);

        let title_control = Self::dispatch_node(container, results, title_node, &mut self.title_state, "window chrome title");
        let name = container.name().to_string();
        container.draw_control_text_with_font(
            container.style().title_font,
            &name,
            title_node.rect,
            ControlColor::TitleText,
            WidgetOption::NONE,
        );
        if title_control.active {
            let delta = container.input().borrow().mouse_delta;
            container.translate_rect(delta);
        }

        let Some(close_node) = self.close_node(title_node.rect, opt) else {
            return;
        };

        container.draw_icon(CLOSE_ICON, close_node.rect, title_text_color);
        let close_control = Self::dispatch_node(container, results, close_node, &mut self.close_state, "window chrome close");
        if close_control.clicked {
            *win_state = WindowState::Closed;
        }
    }

    fn render_resize_handle(&mut self, container: &mut Container, results: &mut FrameResults, opt: ContainerOption) {
        let Some(resize_node) = self.resize_node(container, opt) else {
            return;
        };

        let resize_control = Self::dispatch_node(container, results, resize_node, &mut self.resize_state, "window chrome resize");
        if resize_control.active {
            let delta = container.input().borrow().mouse_delta;
            container.resize_rect_by(delta, Dimensioni::new(96, 64));
        }
    }
}

#[derive(Clone, Copy, Debug)]
/// Indicates whether a window should be rendered this frame.
pub enum WindowState {
    /// Window is visible and will receive input.
    Open,
    /// Window is hidden.
    Closed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Type {
    Dialog,
    Window,
    Popup,
}

pub(crate) struct Window {
    pub(crate) ty: Type,
    pub(crate) win_state: WindowState,
    root_id: RootId,
    last_root_frame: Option<usize>,
    pub(crate) main: Container,
    chrome_tree: WindowChromeTree,
}

impl Window {
    fn titlebar_height(container: &Container) -> i32 {
        let style = container.style();
        let font_height = container.atlas().get_font_height(style.title_font) as i32;
        let padding = style.padding.max(0);
        let min_title_h = font_height + (padding / 2).max(1) * 2;
        style.title_height.max(min_title_h)
    }

    fn body_rect_for(container: &Container, opt: ContainerOption) -> Recti {
        let mut body = container.rect();
        if !opt.has_no_title() {
            let title_h = Self::titlebar_height(container);
            body.y += title_h;
            body.height -= title_h;
        }
        body
    }

    fn apply_auto_size(container: &mut Container, opt: ContainerOption) {
        let content_size = container.content_size();
        if !opt.is_auto_sizing() || (content_size.width <= 0 && content_size.height <= 0) {
            return;
        }

        let padding = container.style().padding.max(0) * 2;
        let target_body_width = content_size.width.saturating_add(padding);
        let target_body_height = content_size.height.saturating_add(padding);
        let body = Self::body_rect_for(container, opt);
        let mut window_rect = container.rect();
        let chrome_width = window_rect.width - body.width;
        let chrome_height = window_rect.height - body.height;

        window_rect.width = (target_body_width + chrome_width).max(0);
        window_rect.height = (target_body_height + chrome_height).max(0);
        container.set_rect(window_rect);
    }

    /// Creates a dialog window that starts closed.
    pub fn dialog(root_id: RootId, name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.set_internal_id_seed(Id::new(root_id.raw() as u64));
        main.set_rect(initial_rect);
        let chrome_ids = WindowChromeIds::from_root(root_id);

        Self {
            ty: Type::Dialog,
            win_state: WindowState::Closed,
            root_id,
            last_root_frame: None,
            main,
            chrome_tree: WindowChromeTree::new(chrome_ids),
        }
    }

    /// Creates a standard window that starts open.
    pub fn window(root_id: RootId, name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.set_internal_id_seed(Id::new(root_id.raw() as u64));
        main.set_rect(initial_rect);
        let chrome_ids = WindowChromeIds::from_root(root_id);

        Self {
            ty: Type::Window,
            win_state: WindowState::Open,
            root_id,
            last_root_frame: None,
            main,
            chrome_tree: WindowChromeTree::new(chrome_ids),
        }
    }

    /// Creates a popup window that starts closed.
    pub fn popup(root_id: RootId, name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        let mut main = Container::new(name, atlas, style, input);
        main.set_internal_id_seed(Id::new(root_id.raw() as u64));
        main.set_rect(initial_rect);
        let chrome_ids = WindowChromeIds::from_root(root_id);

        Self {
            ty: Type::Popup,
            win_state: WindowState::Closed,
            root_id,
            last_root_frame: None,
            main,
            chrome_tree: WindowChromeTree::new(chrome_ids),
        }
    }

    /// Returns `true` if this handle manages a popup window.
    pub fn is_popup(&self) -> bool {
        match self.ty {
            Type::Popup => true,
            _ => false,
        }
    }

    #[inline(never)]
    fn begin_window(&mut self, results: &mut FrameResults, opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        Self::apply_auto_size(&mut self.main, opt);

        let r = self.main.rect();
        if !opt.has_no_frame() {
            self.main.draw_frame(r, ControlColor::WindowBG);
        }

        self.chrome_tree.render_title_bar(&mut self.main, results, &mut self.win_state, opt);

        let body = Self::body_rect_for(&self.main, opt);
        self.main.configure_container_body(body, scroll_behavior);
        let body = self.main.body();
        self.main.push_clip_rect(body);
    }

    fn end_window(&mut self) {
        let container = &mut self.main;
        container.pop_clip_rect();
    }

    fn prepare_for_root_frame(&mut self, frame: usize) {
        if self.last_root_frame == Some(frame) {
            panic!("window {:?} was rendered more than once in frame {}", self.main.name(), frame);
        }

        let contiguous = self.last_root_frame.and_then(|last| last.checked_add(1)) == Some(frame);
        if !contiguous {
            self.main.clear_root_frame_state();
        }
        self.main.prepare();
        self.last_root_frame = Some(frame);
    }

    fn reset_after_close(&mut self) {
        self.last_root_frame = None;
        self.main.reset();
    }

    fn measure_auto_size(&mut self, results: &FrameResults, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        let body = Self::body_rect_for(&self.main, opt);
        // Auto-size should measure desired content against the raw body rect rather than inheriting
        // last frame's scrollbar decision or scroll offset.
        let mut scratch = self.main.measurement_scratch();
        scratch.clear_content_and_scroll();
        scratch.configure_container_body(body, scroll_behavior);
        self.main.set_content_size(scratch.measure_widget_tree_content(results, tree));
    }

    fn finish_resize(&mut self, results: &mut FrameResults, opt: ContainerOption) {
        self.chrome_tree.render_resize_handle(&mut self.main, results, opt);
    }

    #[cfg(test)]
    pub(crate) fn chrome_ids(&self) -> WindowChromeIds {
        self.chrome_tree.ids
    }
}

#[derive(Clone)]
/// Reference-counted handle to the internal window object.
pub struct WindowHandle(Rc<RefCell<Window>>);

impl WindowHandle {
    pub(crate) fn window(root_id: RootId, name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        Self(Rc::new(RefCell::new(Window::window(root_id, name, atlas, style, input, initial_rect))))
    }

    pub(crate) fn dialog(root_id: RootId, name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>, initial_rect: Recti) -> Self {
        Self(Rc::new(RefCell::new(Window::dialog(root_id, name, atlas, style, input, initial_rect))))
    }

    pub(crate) fn popup(root_id: RootId, name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>) -> Self {
        Self(Rc::new(RefCell::new(Window::popup(root_id, name, atlas, style, input, Recti::new(0, 0, 0, 0)))))
    }

    /// Returns `true` if the window's state is `Open`.
    pub fn is_open(&self) -> bool {
        match self.0.borrow().win_state {
            WindowState::Open => true,
            _ => false,
        }
    }

    /// Returns the current visibility state for the window.
    pub fn state(&self) -> WindowState {
        self.0.borrow().win_state
    }

    /// Updates the window visibility state.
    ///
    /// Closing a window resets its container state so the next open starts cleanly.
    pub fn set_state(&mut self, state: WindowState) {
        let mut inner = self.inner_mut();
        inner.win_state = state;
        if matches!(state, WindowState::Closed) {
            inner.reset_after_close();
        }
    }

    /// Marks the window as open.
    pub fn open(&mut self) {
        self.set_state(WindowState::Open);
    }

    /// Marks the window as closed and resets its container state.
    pub fn close(&mut self) {
        self.set_state(WindowState::Closed);
    }

    /// Returns the current window rectangle.
    pub fn rect(&self) -> Recti {
        self.inner().main.rect()
    }

    /// Replaces the current window rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.inner_mut().main.set_rect(rect);
    }

    /// Sets focus to a retained node inside the window's root container.
    pub fn set_focus_node(&mut self, node_id: NodeId) {
        self.inner_mut().main.set_focus_node(node_id);
    }

    /// Clears focus in the window's root container.
    pub fn clear_focus(&mut self) {
        self.inner_mut().main.clear_focus();
    }

    /// Sets focus to the retained node that most recently dispatched `handle`.
    pub fn set_focus_handle<W: Widget>(&mut self, handle: &WidgetHandle<W>) -> bool {
        self.inner_mut().main.set_focus_handle(handle)
    }

    pub(crate) fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Window> {
        self.0.borrow_mut()
    }

    pub(crate) fn inner<'a>(&'a self) -> Ref<'a, Window> {
        self.0.borrow()
    }

    pub(crate) fn root_id(&self) -> RootId {
        self.inner().root_id
    }

    pub(crate) fn prepare_for_frame(&mut self, frame: usize) {
        self.inner_mut().prepare_for_root_frame(frame)
    }

    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        self.0.borrow_mut().main.render(canvas)
    }

    pub(crate) fn finish(&mut self) {
        self.inner_mut().main.finish()
    }

    pub(crate) fn zindex(&self) -> i32 {
        self.0.borrow().main.zindex()
    }

    pub(crate) fn set_zindex(&mut self, zindex: i32) {
        self.inner_mut().main.set_zindex(zindex);
    }

    pub(crate) fn set_root_style(&mut self, style: Rc<Style>) {
        self.inner_mut().main.set_style_handle(style);
    }

    pub(crate) fn root_contains_point(&self, point: Vec2i) -> bool {
        self.inner().main.contains_point(point)
    }

    pub(crate) fn root_in_hover_root(&self) -> bool {
        self.inner().main.in_hover_root()
    }

    pub(crate) fn set_root_hover_active(&mut self, active: bool) {
        self.inner_mut().main.set_in_hover_root(active);
    }

    pub(crate) fn mark_popup_just_opened(&mut self) {
        self.inner_mut().main.mark_popup_just_opened();
    }

    pub(crate) fn begin_root_command_scope(&mut self, pending_scroll: Option<Vec2i>) {
        self.inner_mut().main.begin_root_command_scope(pending_scroll);
    }

    pub(crate) fn finish_root_command_scope(&mut self) {
        self.inner_mut().main.finish_root_command_scope();
    }

    pub(crate) fn begin_window(&mut self, results: &mut FrameResults, opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        self.0.borrow_mut().begin_window(results, opt, scroll_behavior)
    }

    pub(crate) fn measure_auto_size(&mut self, results: &FrameResults, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        self.inner_mut().measure_auto_size(results, opt, scroll_behavior, tree)
    }

    pub(crate) fn end_window(&mut self) {
        self.inner_mut().end_window()
    }

    pub(crate) fn finish_resize(&mut self, results: &mut FrameResults, opt: ContainerOption) {
        self.inner_mut().finish_resize(results, opt)
    }

    pub(crate) fn reset_after_close(&mut self) {
        self.inner_mut().reset_after_close()
    }

    pub(crate) fn root_is_popup(&self) -> bool {
        self.inner().is_popup()
    }

    pub(crate) fn root_popup_just_opened(&self) -> bool {
        self.inner().main.popup_just_opened()
    }

    pub(crate) fn clear_root_popup_just_opened(&mut self) {
        self.inner_mut().main.clear_popup_just_opened();
    }

    /// Resizes the underlying window rectangle.
    pub fn set_size(&mut self, size: &Dimensioni) {
        self.inner_mut().main.set_rect_size(*size);
    }
}
