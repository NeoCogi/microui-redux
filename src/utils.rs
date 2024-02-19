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

pub fn clip_rect(dst_r: Recti, src_r: Recti, clip_r: Recti) -> Option<(Recti, Recti)> {
    match dst_r.intersect(&clip_r) {
        Some(rect) if rect.width == dst_r.width && rect.height == dst_r.height => Some((dst_r, src_r)),
        Some(rect) if rect.width != 0 && rect.height != 0 => {
            let dx = dst_r.x as f32;
            let dy = dst_r.y as f32;
            let dw = dst_r.width as f32;
            let dh = dst_r.height as f32;

            let rx = rect.x as f32;
            let ry = rect.y as f32;
            let rw = rect.width as f32;
            let rh = rect.height as f32;

            let tx = (rx - dx) / dw;
            let ty = (ry - dy) / dh;
            let tw = (rx + rw - dx) / dw;
            let th = (ry + rh - dy) / dh;

            let sx = src_r.x as f32;
            let sy = src_r.y as f32;
            let sw = src_r.width as f32;
            let sh = src_r.height as f32;

            let st_x = sx + tx * sw;
            let st_y = sy + ty * sh;
            let st_w = sx + tw * sw - st_x;
            let st_h = sy + th * sh - st_y;

            Some((rect, Recti::new(st_x as _, st_y as _, st_w as _, st_h as _)))
        }
        _ => None,
    }
}
