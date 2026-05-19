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
use crate::{Dimensioni, NodeId, Recti, RetainedId, Vec2i};

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

/// Mutable view into retained container state borrowed from a handle.
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

    /// Sets focus to a retained node in this container.
    pub fn set_focus_node(&mut self, node_id: NodeId) {
        self.inner.set_focus_node(node_id);
    }

    /// Clears focus in this container.
    pub fn clear_focus(&mut self) {
        self.inner.clear_focus();
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

    /// Returns the retained interaction identity for a node inside this container.
    pub fn retained_id_for_node(&self, node_id: NodeId) -> RetainedId {
        self.0.borrow().retained_id_for_node(node_id)
    }
}
