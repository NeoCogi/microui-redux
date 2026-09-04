//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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
////////////////////////////////////////////////////////////////////////////////
//
// The MIT License (MIT)
//
// Copyright (c) 2014 Coeuvre Wong
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Packs small rectangles into a larger texture atlas.

use crate::{Rect, Recti};

/// Private rectangle-packing state used by the parent atlas builder.
#[derive(Clone)]
pub(super) struct Packer {
    /// Inner skyline packer operating on the padding-adjusted region.
    packer: DensePacker,
}

impl Packer {
    /// Fixed spacing retained between the atlas border and packed rectangles.
    const BORDER_PADDING: i32 = 1;
    /// Fixed spacing retained between independently packed rectangles.
    const RECTANGLE_PADDING: i32 = 1;

    /// Creates the atlas builder's empty packer with its fixed one-pixel separations.
    pub(super) fn new(width: i32, height: i32) -> Self {
        // The builder has one packing policy. Keeping it here prevents the parent module from
        // constructing or mutating the packer's internal representation. Calculate the usable
        // extent in i64 so extreme dimensions cannot overflow before the coordinate-domain clamp.
        let usable =
            |extent: i32| (i64::from(extent) + i64::from(Self::RECTANGLE_PADDING) - i64::from(Self::BORDER_PADDING) * 2).clamp(0, i64::from(i32::MAX)) as i32;
        let usable_width = usable(width);
        let usable_height = usable(height);

        Self {
            packer: DensePacker::new(usable_width, usable_height),
        }
    }

    /// Pack new rectangle. Returns position of the newly added rectangle. If there is not enough space returns `None`.
    /// If it returns `None` you can still try to add smaller rectangles.
    ///
    /// `allow_rotation` - allow 90° rotation of the input rectangle. You can detect whether rectangle was rotated by comparing
    /// returned `width` and `height` with the supplied ones.
    pub(super) fn pack(&mut self, width: i32, height: i32, allow_rotation: bool) -> Option<Recti> {
        if width <= 0 || height <= 0 {
            return None;
        }

        // Padding can push an otherwise representable rectangle past i32::MAX. Such a rectangle
        // cannot fit this coordinate-based packer, so report ordinary packing failure.
        let padded_width = width.checked_add(Self::RECTANGLE_PADDING)?;
        let padded_height = height.checked_add(Self::RECTANGLE_PADDING)?;
        if let Some(mut rect) = self.packer.pack(padded_width, padded_height, allow_rotation) {
            rect.width = rect.width.checked_sub(Self::RECTANGLE_PADDING)?;
            rect.height = rect.height.checked_sub(Self::RECTANGLE_PADDING)?;
            rect.x = rect.x.checked_add(Self::BORDER_PADDING)?;
            rect.y = rect.y.checked_add(Self::BORDER_PADDING)?;

            Some(rect)
        } else {
            None
        }
    }
}

#[derive(Clone)]
/// One horizontal skyline segment in the dense packer.
struct Skyline {
    /// Left x coordinate of the skyline segment.
    left: i32,
    /// Current y height at this skyline segment.
    y: i32,
    /// Width of this skyline segment.
    width: i32,
}

impl Skyline {
    #[inline(always)]
    /// Returns the exclusive right edge of the segment.
    fn right(&self) -> i32 {
        self.left + self.width
    }
}

/// Similar to `Packer` but does not add any padding between rectangles.
#[derive(Clone)]
struct DensePacker {
    /// Packer width in pixels.
    width: i32,
    /// Packer height in pixels.
    height: i32,

    /// Skyline segments sorted by their x position.
    skylines: Vec<Skyline>,
}

impl DensePacker {
    /// Create new empty `DensePacker` with the provided parameters.
    fn new(width: i32, height: i32) -> DensePacker {
        let width = std::cmp::max(0, width);
        let height = std::cmp::max(0, height);

        let skylines = vec![Skyline { left: 0, y: 0, width }];

        DensePacker { width, height, skylines }
    }

    /// Pack new rectangle. Returns position of the newly added rectangle. If there is not enough space returns `None`.
    /// If it returns `None` you can still try to add smaller rectangles.
    ///
    /// `allow_rotation` - allow 90° rotation of the input rectangle. You can detect whether rectangle was rotated by comparing
    /// returned `width` and `height` with the supplied ones.
    fn pack(&mut self, width: i32, height: i32, allow_rotation: bool) -> Option<Recti> {
        if width <= 0 || height <= 0 {
            return None;
        }

        if let Some((i, rect)) = self.find_skyline(width, height, allow_rotation) {
            self.split(i, &rect);
            self.merge();

            Some(rect)
        } else {
            None
        }
    }

    /// Returns a placement if the rectangle can fit starting at skyline `i`.
    fn can_put(&self, mut i: usize, w: i32, h: i32) -> Option<Recti> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut rect = Rect::new(self.skylines[i].left, 0, w, h);
        let mut width_left = rect.width;
        loop {
            rect.y = std::cmp::max(rect.y, self.skylines[i].y);
            // Test exclusive edges in i64 before later skyline code forms them in i32. Once this
            // placement passes, all later right/bottom additions are bounded by the packer extent.
            if i64::from(rect.x) + i64::from(rect.width) > i64::from(self.width) || i64::from(rect.y) + i64::from(rect.height) > i64::from(self.height) {
                return None;
            }
            if self.skylines[i].width >= width_left {
                return Some(rect);
            }
            width_left -= self.skylines[i].width;
            i += 1;
            if i >= self.skylines.len() {
                // A malformed or exhausted skyline is a failed placement, not a reason to panic
                // while processing builder-controlled rectangle dimensions.
                return None;
            }
        }
    }

    /// Finds the lowest suitable skyline placement for the requested rectangle.
    fn find_skyline(&self, w: i32, h: i32, allow_rotation: bool) -> Option<(usize, Recti)> {
        let mut bottom = i32::MAX;
        let mut width = i32::MAX;
        let mut index = None;
        let mut rect = Rect::new(0, 0, 0, 0);

        // keep the `bottom` and `width` as small as possible
        for i in 0..self.skylines.len() {
            if let Some(r) = self.can_put(i, w, h)
                && (r.y + r.height < bottom || (r.y + r.height == bottom && self.skylines[i].width < width))
            {
                bottom = r.y + r.height;
                width = self.skylines[i].width;
                index = Some(i);
                rect = r;
            }

            if allow_rotation
                && let Some(r) = self.can_put(i, h, w)
                && (r.y + r.height < bottom || (r.y + r.height == bottom && self.skylines[i].width < width))
            {
                bottom = r.y + r.height;
                width = self.skylines[i].width;
                index = Some(i);
                rect = r;
            }
        }

        index.map(|index| (index, rect))
    }

    /// Splits skyline segments after placing `rect` at segment `i`.
    fn split(&mut self, i: usize, rect: &Recti) {
        let skyline = Skyline {
            left: rect.x,
            y: rect.y + rect.height,
            width: rect.width,
        };

        assert!(skyline.right() <= self.width);
        assert!(skyline.y <= self.height);

        self.skylines.insert(i, skyline);

        while i + 1 < self.skylines.len() {
            assert!(self.skylines[i].left <= self.skylines[i + 1].left);

            if self.skylines[i + 1].left >= self.skylines[i].right() {
                break;
            }

            let shrink = self.skylines[i].right() - self.skylines[i + 1].left;
            if self.skylines[i + 1].width <= shrink {
                self.skylines.remove(i + 1);
            } else {
                self.skylines[i + 1].left += shrink;
                self.skylines[i + 1].width -= shrink;
                break;
            }
        }
    }

    /// Merges adjacent skyline segments with the same height.
    fn merge(&mut self) {
        let mut i = 1;
        while i < self.skylines.len() {
            if self.skylines[i - 1].y == self.skylines[i].y {
                self.skylines[i - 1].width += self.skylines[i].width;
                self.skylines.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies padding arithmetic remains total at the coordinate limit and ordinary packing is
    /// unchanged by the wider intermediate calculations.
    #[test]
    fn padded_packer_rejects_overflowing_extents_and_places_normal_rectangles() {
        let mut extreme = Packer::new(i32::MAX, i32::MAX);
        assert!(extreme.pack(i32::MAX, 1, false).is_none());

        let mut ordinary = Packer::new(8, 8);
        let placed = ordinary.pack(2, 2, false).expect("ordinary padded rectangle must fit");
        assert_eq!((placed.x, placed.y, placed.width, placed.height), (1, 1, 2, 2));
    }
}
