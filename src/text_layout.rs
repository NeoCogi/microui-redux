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
use crate::{AtlasHandle, FontId, TextWrap};

#[derive(Clone, Copy)]
pub(crate) struct TextLine {
    pub start: usize,
    pub end: usize,
    pub width: i32,
}

fn push_wrapped_line(
    lines: &mut Vec<TextLine>,
    buf: &str,
    line_start: usize,
    line_end: usize,
    wrap: TextWrap,
    max_width: i32,
    font: FontId,
    atlas: &AtlasHandle,
) {
    let line = &buf[line_start..line_end];
    if line.is_empty() {
        lines.push(TextLine {
            start: line_start,
            end: line_start,
            width: 0,
        });
        return;
    }

    if wrap != TextWrap::Word || max_width <= 0 {
        let width = atlas.get_text_size(font, line).width;
        lines.push(TextLine {
            start: line_start,
            end: line_end,
            width,
        });
        return;
    }

    let mut offset = 0;
    let mut seg_start = 0;
    let mut seg_width = 0;
    for word in line.split_inclusive(' ') {
        let word_len = word.len();
        let word_width = atlas.get_text_size(font, word).width;
        if seg_width > 0 && seg_width + word_width > max_width {
            let seg_end = offset;
            lines.push(TextLine {
                start: line_start + seg_start,
                end: line_start + seg_end,
                width: seg_width,
            });
            seg_start = offset;
            seg_width = 0;
        }
        seg_width += word_width;
        offset += word_len;
    }

    lines.push(TextLine {
        start: line_start + seg_start,
        end: line_start + line.len(),
        width: seg_width,
    });
}

pub(crate) fn build_text_lines(buf: &str, wrap: TextWrap, max_width: i32, font: FontId, atlas: &AtlasHandle) -> Vec<TextLine> {
    let mut lines = Vec::new();
    if buf.is_empty() {
        lines.push(TextLine {
            start: 0,
            end: 0,
            width: 0,
        });
        return lines;
    }

    let mut line_start = 0;
    for (idx, ch) in buf.char_indices() {
        if ch == '\n' {
            push_wrapped_line(&mut lines, buf, line_start, idx, wrap, max_width, font, atlas);
            line_start = idx + ch.len_utf8();
        }
    }

    if line_start <= buf.len() {
        push_wrapped_line(&mut lines, buf, line_start, buf.len(), wrap, max_width, font, atlas);
    }

    if lines.is_empty() {
        lines.push(TextLine {
            start: 0,
            end: 0,
            width: 0,
        });
    }
    lines
}
