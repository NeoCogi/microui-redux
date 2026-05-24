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
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR ITS CONTRIBUTORS BE
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
//! Shared handles for retained scroll-area nodes.

use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

use crate::canvas::Canvas;
use crate::container::ScrollArea;
use crate::render::Renderer;
use crate::{Dimensioni, NodeId, Recti, RetainedId, Vec2i};

#[derive(Clone)]
/// Shared handle to a retained scroll area.
pub struct ScrollAreaHandle(pub(crate) Rc<RefCell<ScrollArea>>);

impl From<&ScrollAreaHandle> for ScrollAreaHandle {
    fn from(handle: &ScrollAreaHandle) -> Self {
        handle.clone()
    }
}

/// Read-only view into a retained scroll area borrowed from a handle.
pub struct ScrollAreaView<'a> {
    inner: &'a ScrollArea,
}

impl<'a> ScrollAreaView<'a> {
    fn new(inner: &'a ScrollArea) -> Self {
        Self { inner }
    }

    /// Returns the scroll area outer rectangle.
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

/// Mutable view into retained scroll-area state borrowed from a handle.
pub struct ScrollAreaViewMut<'a> {
    inner: &'a mut ScrollArea,
}

impl<'a> ScrollAreaViewMut<'a> {
    fn new(inner: &'a mut ScrollArea) -> Self {
        Self { inner }
    }

    /// Returns the scroll area outer rectangle.
    pub fn rect(&self) -> Recti {
        self.inner.rect()
    }

    /// Updates the scroll area outer rectangle.
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

    /// Sets focus to a retained node in this scroll area.
    pub fn set_focus_node(&mut self, node_id: NodeId) {
        self.inner.set_focus_node(node_id);
    }

    /// Clears focus in this scroll area.
    pub fn clear_focus(&mut self) {
        self.inner.clear_focus();
    }
}

impl ScrollAreaHandle {
    pub(crate) fn new(scroll_area: ScrollArea) -> Self {
        Self(Rc::new(RefCell::new(scroll_area)))
    }

    pub(crate) fn render<R: Renderer>(&self, canvas: &mut Canvas<R>) {
        self.0.borrow_mut().render(canvas)
    }

    /// Returns an immutable borrow of the underlying scroll area.
    pub(crate) fn inner<'a>(&'a self) -> Ref<'a, ScrollArea> {
        self.0.borrow()
    }

    /// Executes `f` with a read-only view into the scroll area.
    pub fn with<R>(&self, f: impl FnOnce(&ScrollAreaView<'_>) -> R) -> R {
        let scroll_area = self.0.borrow();
        let view = ScrollAreaView::new(&scroll_area);
        f(&view)
    }

    /// Executes `f` with a mutable view into the scroll area.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut ScrollAreaViewMut<'_>) -> R) -> R {
        let mut scroll_area = self.0.borrow_mut();
        let mut view = ScrollAreaViewMut::new(&mut scroll_area);
        f(&mut view)
    }

    pub(crate) fn with_inner_mut<R>(&self, f: impl FnOnce(&mut ScrollArea) -> R) -> R {
        let mut scroll_area = self.0.borrow_mut();
        f(&mut scroll_area)
    }

    /// Returns the retained interaction identity for a node inside this scroll area.
    pub fn retained_id_for_node(&self, node_id: NodeId) -> RetainedId {
        self.0.borrow().retained_id_for_node(node_id)
    }
}

/// Compatibility alias for code that still uses the old retained container name.
#[deprecated(since = "0.6.1", note = "use ScrollAreaHandle")]
pub type ContainerHandle = ScrollAreaHandle;
/// Compatibility alias for the old read-only retained container view.
#[deprecated(since = "0.6.1", note = "use ScrollAreaView")]
pub type ContainerView<'a> = ScrollAreaView<'a>;
/// Compatibility alias for the old mutable retained container view.
#[deprecated(since = "0.6.1", note = "use ScrollAreaViewMut")]
pub type ContainerViewMut<'a> = ScrollAreaViewMut<'a>;
