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
//! Transitional retained handles used by legacy disclosure containers.

use std::{cell::RefCell, rc::Rc};

/// Shared ownership handle for transitional legacy disclosure state.
///
/// Leaf nodes own concrete state-owning runtimes directly. This handle remains only for legacy
/// header/tree [`crate::Node`] state until those containers migrate to framework-owned state. Its
/// reference-counting and interior-mutability storage is intentionally private; callers should use
/// [`WidgetHandle::read`], [`WidgetHandle::update`], or [`WidgetHandle::replace`] instead of
/// depending on the handle representation.
pub struct WidgetHandle<T> {
    /// Shared retained widget state.
    inner: Rc<RefCell<T>>,
}

impl<T> Clone for WidgetHandle<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T> From<&WidgetHandle<T>> for WidgetHandle<T> {
    fn from(handle: &WidgetHandle<T>) -> Self {
        handle.clone()
    }
}

impl<T> WidgetHandle<T> {
    /// Creates a handle around persistent widget state.
    pub fn new(value: T) -> Self {
        Self { inner: Rc::new(RefCell::new(value)) }
    }

    /// Runs `f` with read-only access to the widget state.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let state = self.inner.borrow();
        f(&state)
    }

    /// Runs `f` with mutable access to the widget state.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut state = self.inner.borrow_mut();
        f(&mut state)
    }

    /// Replaces the widget state and returns the previous value.
    pub fn replace(&self, value: T) -> T {
        self.inner.replace(value)
    }
}

/// Wraps legacy disclosure state into a retained handle.
///
/// The returned handle may be cloned to share one header/tree state during the temporary legacy
/// disclosure migration.
pub fn widget_handle<T>(value: T) -> WidgetHandle<T> {
    WidgetHandle::new(value)
}
