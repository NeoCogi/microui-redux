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

use super::*;

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct LayoutFlags: u32 {
        const NONE = 0x00000000;
        const FILL = 0x00000001;
    }
}

#[derive(Copy, Clone)]
pub enum HorizontalAlignment {
    LeftToRight,
    RightToLeft,
}

#[derive(Copy, Clone)]
pub enum VerticalAlignment {
    TopDown,
    BottomUp,
}

#[derive(Copy, Clone)]
pub enum LayoutDirection {
    Horizontal(HorizontalAlignment),
    Vertical(VerticalAlignment),
}

#[derive(Clone)]
pub struct Layout {
    pub direction: LayoutDirection,
    pub free_space: Recti,
    pub max: Dimensioni,
}

impl Layout {
    pub fn next(&mut self, desired_rect: Dimensioni, flags: LayoutFlags) -> Option<Recti> {
        let remaining_height = self.free_space.height;
        let remaining_width = self.free_space.width;

        let (x, width, inc_x, dec_width, y, height, inc_y, dec_height) = match self.direction {
            LayoutDirection::Horizontal(dir) => {
                let desired_width = if flags.intersects(LayoutFlags::FILL) {
                    remaining_width
                } else {
                    desired_rect.width
                };
                let max_allowed_width = min(desired_width, remaining_width);
                self.max.width += desired_width;

                let (x, inc_x, dec_width) = match dir {
                    HorizontalAlignment::LeftToRight => (self.free_space.x, max_allowed_width, max_allowed_width),
                    HorizontalAlignment::RightToLeft => (self.free_space.x + (remaining_width - max_allowed_width), 0, max_allowed_width),
                };
                (x, max_allowed_width, inc_x, dec_width, self.free_space.y, self.free_space.height, 0, 0)
            }
            LayoutDirection::Vertical(dir) => {
                let desired_height = if flags.intersects(LayoutFlags::FILL) {
                    remaining_height
                } else {
                    desired_rect.height
                };
                let max_allowed_height = min(desired_height, remaining_height);
                self.max.height += desired_height;

                let (y, inc_y, dec_height) = match dir {
                    VerticalAlignment::TopDown => (self.free_space.y, max_allowed_height, max_allowed_height),
                    VerticalAlignment::BottomUp => (self.free_space.y + (remaining_height - max_allowed_height), 0, max_allowed_height),
                };

                (self.free_space.x, self.free_space.width, 0, 0, y, max_allowed_height, inc_y, dec_height)
            }
        };

        // bail if we don't have enough space
        if self.free_space.width <= 0 || self.free_space.height <= 0 {
            return None;
        }

        self.free_space.x += inc_x;
        self.free_space.width -= dec_width;
        self.free_space.y += inc_y;
        self.free_space.height -= dec_height;
        Some(Recti::new(x, y, width, height))
    }
}
