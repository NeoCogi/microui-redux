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
//! Shared container handles used by windows, panels, and retained container nodes.

use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};

use crate::canvas::Canvas;
use crate::container::Container;
use crate::render::Renderer;
use crate::{Clip, Color, ContainerOption, ControlColor, Dimensioni, FontId, IconId, Recti, SlotId, TextWrap, Vec2i, ScrollBehavior, WidgetId, WidgetOption};

pub(crate) type ContainerId = *const ();

pub(crate) fn container_id_of(handle: &ContainerHandle) -> ContainerId {
    Rc::as_ptr(&handle.0) as *const ()
}

#[derive(Clone)]
/// Shared handle to a container that can be embedded inside windows or panels.
pub struct ContainerHandle(pub(crate) Rc<RefCell<Container>>);

/// Read-only view into a container borrowed from a handle.
pub struct ContainerView<'a> {
    inner: &'a Container,
}

impl<'a> ContainerView<'a> {
    fn new(inner: &'a Container) -> Self {
        Self { inner }
    }

    /// Returns the container outer rectangle.
    pub fn rect(&self) -> Recti {
        self.inner.rect()
    }

    /// Returns the current body rectangle.
    pub fn body(&self) -> Recti {
        self.inner.body()
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.inner.scroll()
    }

    /// Returns the measured content size.
    pub fn content_size(&self) -> Dimensioni {
        self.inner.content_size()
    }
}

/// Mutable view into a container borrowed from a handle.
pub struct ContainerViewMut<'a> {
    inner: &'a mut Container,
}

impl<'a> ContainerViewMut<'a> {
    fn new(inner: &'a mut Container) -> Self {
        Self { inner }
    }
    /// Returns the container outer rectangle.
    pub fn rect(&self) -> Recti {
        self.inner.rect()
    }

    /// Updates the container outer rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.inner.set_rect(rect);
    }

    /// Returns the current body rectangle.
    pub fn body(&self) -> Recti {
        self.inner.body()
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.inner.scroll()
    }

    /// Updates the current scroll offset.
    pub fn set_scroll(&mut self, scroll: Vec2i) {
        self.inner.set_scroll(scroll);
    }

    /// Returns the measured content size.
    pub fn content_size(&self) -> Dimensioni {
        self.inner.content_size()
    }

    /// Manually updates which widget owns focus.
    pub fn set_focus(&mut self, widget_id: Option<WidgetId>) {
        self.inner.set_focus(widget_id);
    }

    /// Pushes a new clip rectangle combined with the previous clip.
    pub fn push_clip_rect(&mut self, rect: Recti) {
        self.inner.push_clip_rect(rect);
    }

    /// Restores the previous clip rectangle from the stack.
    pub fn pop_clip_rect(&mut self) {
        self.inner.pop_clip_rect();
    }

    /// Returns the active clip rectangle.
    pub fn get_clip_rect(&mut self) -> Recti {
        self.inner.get_clip_rect()
    }

    /// Determines whether `rect` is visible under the current clip.
    pub fn check_clip(&mut self, rect: Recti) -> Clip {
        self.inner.check_clip(rect)
    }

    /// Adjusts the current clip rectangle.
    pub fn set_clip(&mut self, rect: Recti) {
        self.inner.set_clip(rect);
    }

    /// Records a filled rectangle draw command.
    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        self.inner.draw_rect(rect, color);
    }

    /// Records a rectangle outline.
    pub fn draw_box(&mut self, rect: Recti, color: Color) {
        self.inner.draw_box(rect, color);
    }

    /// Records a text draw command.
    pub fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        self.inner.draw_text(font, text, pos, color);
    }

    /// Records an icon draw command.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        self.inner.draw_icon(id, rect, color);
    }

    /// Records a slot draw command.
    pub fn draw_slot(&mut self, id: SlotId, rect: Recti, color: Color) {
        self.inner.draw_slot(id, rect, color);
    }

    /// Draws multi-line text without wrapping.
    pub fn text(&mut self, text: &str) {
        self.inner.text(text);
    }

    /// Draws multi-line text without wrapping using an explicit font.
    pub fn text_with_font(&mut self, font: FontId, text: &str) {
        self.inner.text_with_font(font, text);
    }

    /// Draws multi-line text using the provided wrapping mode.
    pub fn text_with_wrap(&mut self, text: &str, wrap: TextWrap) {
        self.inner.text_with_wrap(text, wrap);
    }

    /// Draws multi-line text using the provided wrapping mode and font.
    pub fn text_with_font_wrap(&mut self, font: FontId, text: &str, wrap: TextWrap) {
        self.inner.text_with_font_wrap(font, text, wrap);
    }

    /// Records a standard UI frame.
    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        self.inner.draw_frame(rect, colorid);
    }

    /// Records a standard widget frame.
    pub fn draw_widget_frame(&mut self, widget_id: WidgetId, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.inner.draw_widget_frame(widget_id, rect, colorid, opt);
    }

    /// Records a standard container frame.
    pub fn draw_container_frame(&mut self, widget_id: WidgetId, rect: Recti, colorid: ControlColor, opt: ContainerOption) {
        self.inner.draw_container_frame(widget_id, rect, colorid, opt);
    }

    /// Records control text using the style's body font.
    pub fn draw_control_text(&mut self, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.inner.draw_control_text(text, rect, colorid, opt);
    }

    /// Records control text using an explicit font.
    pub fn draw_control_text_with_font(&mut self, font: FontId, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.inner.draw_control_text_with_font(font, text, rect, colorid, opt);
    }

    /// Configures the body rectangle for standalone panel-style drawing.
    pub fn push_container_body(&mut self, body: Recti, opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        self.inner.push_container_body(body, opt, scroll_behavior);
    }
}

impl ContainerHandle {
    pub(crate) fn new(container: Container) -> Self {
        Self(Rc::new(RefCell::new(container)))
    }

    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        self.0.borrow_mut().render(canvas)
    }

    pub(crate) fn finish(&mut self) {
        self.0.borrow_mut().finish()
    }

    /// Returns an immutable borrow of the underlying container.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn inner<'a>(&'a self) -> Ref<'a, Container> {
        self.0.borrow()
    }

    /// Returns a mutable borrow of the underlying container.
    pub(crate) fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Container> {
        self.0.borrow_mut()
    }

    /// Executes `f` with a read-only view into the container.
    pub fn with<R>(&self, f: impl FnOnce(&ContainerView<'_>) -> R) -> R {
        let container = self.0.borrow();
        let view = ContainerView::new(&container);
        f(&view)
    }

    /// Executes `f` with a mutable view into the container.
    pub fn with_mut<R>(&mut self, f: impl FnOnce(&mut ContainerViewMut<'_>) -> R) -> R {
        let mut container = self.0.borrow_mut();
        let mut view = ContainerViewMut::new(&mut container);
        f(&mut view)
    }

    pub(crate) fn with_inner_mut<R>(&mut self, f: impl FnOnce(&mut Container) -> R) -> R {
        let mut container = self.0.borrow_mut();
        f(&mut container)
    }
}
