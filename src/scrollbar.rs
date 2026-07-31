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
//! Geometry helpers for retained container scrollbars.
//!
//! Containers own scrollbar state and input routing; this file keeps the pure calculations for
//! base tracks, thumb rectangles, and drag-to-scroll conversion independent of that state.
use crate::{Recti, Vec2i};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Axis selector used by shared scrollbar geometry helpers.
pub(crate) enum ScrollAxis {
    /// Vertical scrollbar operating on y offsets.
    Vertical,
    /// Horizontal scrollbar operating on x offsets.
    Horizontal,
}

/// Returns the scrollbar track rectangle just outside the container body on the selected axis.
pub(crate) fn scrollbar_base(axis: ScrollAxis, body: Recti, scrollbar_size: i32) -> Recti {
    let mut base = body;
    match axis {
        ScrollAxis::Vertical => {
            base.x = body.x + body.width;
            base.width = scrollbar_size;
        }
        ScrollAxis::Horizontal => {
            base.y = body.y + body.height;
            base.height = scrollbar_size;
        }
    }
    base
}

/// Returns the viewport body left after scrollbar track occupancy is reserved.
pub(crate) fn scrollbar_viewport_body(rect: Recti, content_size: crate::Dimensioni, padding: i32, scrollbar_size: i32) -> Recti {
    if scrollbar_size <= 0 {
        return rect;
    }
    let padding = padding.max(0);
    let content = crate::Dimensioni::new(
        content_size.width.saturating_add(padding.saturating_mul(2)),
        content_size.height.saturating_add(padding.saturating_mul(2)),
    );
    let mut body = rect;
    for _ in 0..3 {
        let needs_vertical = content.height > body.height && body.height > 0;
        let needs_horizontal = content.width > body.width && body.width > 0;
        let mut next = rect;
        if needs_vertical {
            next.width = next.width.saturating_sub(scrollbar_size);
        }
        if needs_horizontal {
            next.height = next.height.saturating_sub(scrollbar_size);
        }
        if next.x == body.x && next.y == body.y && next.width == body.width && next.height == body.height {
            break;
        }
        body = next;
    }
    body
}

/// Returns the largest scroll offset needed to reveal all content.
pub(crate) fn scrollbar_max_scroll(content_len: i32, view_len: i32) -> i32 {
    (content_len - view_len).max(0)
}

/// Converts pointer drag distance on the scrollbar track into content scroll distance.
pub(crate) fn scrollbar_drag_delta(axis: ScrollAxis, delta: Vec2i, content_len: i32, base: Recti) -> i32 {
    let base_len = match axis {
        ScrollAxis::Vertical => base.height,
        ScrollAxis::Horizontal => base.width,
    };
    if base_len <= 0 {
        return 0;
    }
    let axis_delta = match axis {
        ScrollAxis::Vertical => delta.y,
        ScrollAxis::Horizontal => delta.x,
    };
    axis_delta.saturating_mul(content_len) / base_len
}

/// Computes the thumb rectangle for the given view/content ratio and current scroll offset.
pub(crate) fn scrollbar_thumb(axis: ScrollAxis, base: Recti, view_len: i32, content_len: i32, scroll: i32, thumb_size: i32) -> Recti {
    let mut thumb = base;
    let base_len = match axis {
        ScrollAxis::Vertical => base.height,
        ScrollAxis::Horizontal => base.width,
    };
    if base_len <= 0 || content_len <= 0 || view_len <= 0 {
        return thumb;
    }

    // Thumb length represents the visible fraction but is clamped so it remains usable.
    let mut thumb_len = base_len.saturating_mul(view_len) / content_len;
    if thumb_len < thumb_size {
        thumb_len = thumb_size;
    }
    if thumb_len > base_len {
        thumb_len = base_len;
    }

    match axis {
        ScrollAxis::Vertical => thumb.height = thumb_len,
        ScrollAxis::Horizontal => thumb.width = thumb_len,
    }

    let max_scroll = scrollbar_max_scroll(content_len, view_len);
    if max_scroll > 0 {
        let track_len = base_len - thumb_len;
        if track_len > 0 {
            // Map scroll position into the remaining track length after reserving the thumb.
            let offset = scroll.clamp(0, max_scroll) * track_len / max_scroll;
            match axis {
                ScrollAxis::Vertical => thumb.y += offset,
                ScrollAxis::Horizontal => thumb.x += offset,
            }
        }
    }

    thumb
}
