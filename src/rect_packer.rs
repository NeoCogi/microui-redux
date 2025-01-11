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

//! Pack small rectangles into a larger one. This is useful for creating texture atlases for the efficient GPU rendering.

use crate::*;

/// Describes size and padding requirements of rectangle packing.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Config {
    /// Width of the encompassing rectangle.
    pub width: i32,
    /// Height of the encompassing rectangle.
    pub height: i32,

    /// Minimum spacing between border and rectangles.
    pub border_padding: i32,
    /// Minimum spacing between rectangles.
    pub rectangle_padding: i32,
}

pub trait RectTrait {
    fn top(&self) -> i32;
    fn bottom(&self) -> i32;
    fn left(&self) -> i32;
    fn right(&self) -> i32;

    fn area(&self) -> i32 {
        (self.bottom() - self.top()) * (self.right() - self.left())
    }

    /// Check if intersection of two rectangles is non empty.
    fn intersects(&self, other: &Self) -> bool {
        self.contains_point(other.left(), other.top())
            || self.contains_point(other.left(), other.bottom() - 1)
            || self.contains_point(other.right() - 1, other.bottom() - 1)
            || self.contains_point(other.right() - 1, other.top())
            || other.contains_point(self.left(), self.top())
            || other.contains_point(self.left(), self.bottom() - 1)
            || other.contains_point(self.right() - 1, self.bottom() - 1)
            || other.contains_point(self.right() - 1, self.top())
    }

    /// Check if `other` rectangle is completely inside `self`.
    fn contains_rect(&self, other: &Self) -> bool {
        self.left() <= other.left()
            && self.right() >= other.right()
            && self.top() <= other.top()
            && self.bottom() >= other.bottom()
    }

    /// Check if given pixel is inside this rectangle.
    fn contains_point(&self, x: i32, y: i32) -> bool {
        self.left() <= x && x < self.right() && self.top() <= y && y < self.bottom()
    }
}

impl RectTrait for Recti {
    #[inline(always)]
    fn top(&self) -> i32 {
        self.y
    }

    #[inline(always)]
    fn bottom(&self) -> i32 {
        self.y + self.height
    }

    #[inline(always)]
    fn left(&self) -> i32 {
        self.x
    }

    #[inline(always)]
    fn right(&self) -> i32 {
        self.x + self.width
    }
}

/// `Packer` is the main structure in this crate. It holds packing context.
#[derive(Clone)]
pub struct Packer {
    config: Config,
    packer: DensePacker,
}

impl Packer {
    /// Create new empty `Packer` with the provided parameters.
    pub fn new(config: Config) -> Packer {
        let width = std::cmp::max(
            0,
            config.width + config.rectangle_padding - 2 * config.border_padding,
        );
        let height = std::cmp::max(
            0,
            config.height + config.rectangle_padding - 2 * config.border_padding,
        );

        Packer {
            config: config,
            packer: DensePacker::new(width, height),
        }
    }

    /// Get config that this packer was created with.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Pack new rectangle. Returns position of the newly added rectangle. If there is not enough space returns `None`.
    /// If it returns `None` you can still try to add smaller rectangles.
    ///
    /// `allow_rotation` - allow 90° rotation of the input rectangle. You can detect whether rectangle was rotated by comparing
    /// returned `width` and `height` with the supplied ones.
    pub fn pack(&mut self, width: i32, height: i32, allow_rotation: bool) -> Option<Recti> {
        if width <= 0 || height <= 0 {
            return None;
        }

        if let Some(mut rect) = self.packer.pack(
            width + self.config.rectangle_padding,
            height + self.config.rectangle_padding,
            allow_rotation,
        ) {
            rect.width -= self.config.rectangle_padding;
            rect.height -= self.config.rectangle_padding;
            rect.x += self.config.border_padding;
            rect.y += self.config.border_padding;

            Some(rect)
        } else {
            None
        }
    }

    /// Check if rectangle with the specified size can be added.
    pub fn can_pack(&self, width: i32, height: i32, allow_rotation: bool) -> bool {
        self.packer.can_pack(
            width + self.config.rectangle_padding,
            height + self.config.rectangle_padding,
            allow_rotation,
        )
    }
}

#[derive(Clone)]
struct Skyline {
    pub left: i32,
    pub y: i32,
    pub width: i32,
}

impl Skyline {
    #[inline(always)]
    pub fn right(&self) -> i32 {
        self.left + self.width
    }
}

/// Similar to `Packer` but does not add any padding between rectangles.
#[derive(Clone)]
pub struct DensePacker {
    width: i32,
    height: i32,

    // the skylines are sorted by their `x` position
    skylines: Vec<Skyline>,
}

impl DensePacker {
    /// Create new empty `DensePacker` with the provided parameters.
    pub fn new(width: i32, height: i32) -> DensePacker {
        let width = std::cmp::max(0, width);
        let height = std::cmp::max(0, height);

        let skylines = vec![Skyline {
            left: 0,
            y: 0,
            width: width,
        }];

        DensePacker {
            width: width,
            height: height,
            skylines: skylines,
        }
    }

    /// Get size that this packer was created with.
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Set new size for this packer.
    ///
    /// New size should be not less than the current size.
    pub fn resize(&mut self, width: i32, height: i32) {
        assert!(width >= self.width && height >= self.height);

        self.width = width;
        self.height = height;

        // Add a new skyline to fill the gap
        // The new skyline starts where the furthest one ends
        let left = self.skylines.last().unwrap().right();
        self.skylines.push(Skyline {
            left: left,
            y: 0,
            width: width - left,
        });
    }

    /// Pack new rectangle. Returns position of the newly added rectangle. If there is not enough space returns `None`.
    /// If it returns `None` you can still try to add smaller rectangles.
    ///
    /// `allow_rotation` - allow 90° rotation of the input rectangle. You can detect whether rectangle was rotated by comparing
    /// returned `width` and `height` with the supplied ones.
    pub fn pack(&mut self, width: i32, height: i32, allow_rotation: bool) -> Option<Recti> {
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

    /// Check if rectangle with the specified size can be added.
    pub fn can_pack(&self, width: i32, height: i32, allow_rotation: bool) -> bool {
        self.find_skyline(width, height, allow_rotation).is_some()
    }

    // return `rect` if rectangle (w, h) can fit the skyline started at `i`
    fn can_put(&self, mut i: usize, w: i32, h: i32) -> Option<Recti> {
        let mut rect = Rect::new(self.skylines[i].left, 0, w, h);
        let mut width_left = rect.width;
        loop {
            rect.y = std::cmp::max(rect.y, self.skylines[i].y);
            // the source rect is too large
            if !Rect::new(0, 0, self.width, self.height).contains_rect(&rect) {
                return None;
            }
            if self.skylines[i].width >= width_left {
                return Some(rect);
            }
            width_left -= self.skylines[i].width;
            i += 1;
            assert!(i < self.skylines.len());
        }
    }

    fn find_skyline(&self, w: i32, h: i32, allow_rotation: bool) -> Option<(usize, Recti)> {
        let mut bottom = std::i32::MAX;
        let mut width = std::i32::MAX;
        let mut index = None;
        let mut rect = Rect::new(0, 0, 0, 0);

        // keep the `bottom` and `width` as small as possible
        for i in 0..self.skylines.len() {
            if let Some(r) = self.can_put(i, w, h) {
                if r.bottom() < bottom || (r.bottom() == bottom && self.skylines[i].width < width) {
                    bottom = r.bottom();
                    width = self.skylines[i].width;
                    index = Some(i);
                    rect = r;
                }
            }

            if allow_rotation {
                if let Some(r) = self.can_put(i, h, w) {
                    if r.bottom() < bottom
                        || (r.bottom() == bottom && self.skylines[i].width < width)
                    {
                        bottom = r.bottom();
                        width = self.skylines[i].width;
                        index = Some(i);
                        rect = r;
                    }
                }
            }
        }

        if let Some(index) = index {
            Some((index, rect))
        } else {
            None
        }
    }

    fn split(&mut self, i: usize, rect: &Recti) {
        let skyline = Skyline {
            left: rect.left(),
            y: rect.bottom(),
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
