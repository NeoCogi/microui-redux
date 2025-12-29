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
use crate::text_layout::TextLine;
use crate::{AtlasHandle, FontId, InputSnapshot};

pub(crate) enum ReturnBehavior {
    Submit,
    Newline { submit_on_ctrl: bool },
}

pub(crate) struct TextEditOutcome {
    pub cursor: usize,
    pub changed: bool,
    pub moved: bool,
    pub submit: bool,
}

fn insert_text(buf: &mut String, cursor: &mut usize, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let insert_at = (*cursor).min(buf.len());
    buf.insert_str(insert_at, text);
    *cursor = insert_at + text.len();
    true
}

fn delete_prev(buf: &mut String, cursor: &mut usize, allow_leading_newline: bool) -> bool {
    if buf.is_empty() {
        return false;
    }
    if *cursor == 0 {
        if allow_leading_newline && buf.as_bytes().first() == Some(&b'\n') {
            buf.replace_range(0..1, "");
            return true;
        }
        return false;
    }
    let mut start = (*cursor).min(buf.len());
    start -= 1;
    while start > 0 && !buf.is_char_boundary(start) {
        start -= 1;
    }
    buf.replace_range(start..*cursor, "");
    *cursor = start;
    true
}

fn delete_next(buf: &mut String, cursor: usize) -> bool {
    if buf.is_empty() || cursor >= buf.len() {
        return false;
    }
    let mut end = cursor + 1;
    while end < buf.len() && !buf.is_char_boundary(end) {
        end += 1;
    }
    buf.replace_range(cursor..end, "");
    true
}

fn move_left(buf: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut new_cursor = cursor - 1;
    while new_cursor > 0 && !buf.is_char_boundary(new_cursor) {
        new_cursor -= 1;
    }
    new_cursor
}

fn move_right(buf: &str, cursor: usize) -> usize {
    if cursor >= buf.len() {
        return buf.len();
    }
    let mut new_cursor = cursor + 1;
    while new_cursor < buf.len() && !buf.is_char_boundary(new_cursor) {
        new_cursor += 1;
    }
    new_cursor
}

pub(crate) fn apply_text_input(
    buf: &mut String,
    cursor: usize,
    input: &InputSnapshot,
    allow_leading_newline: bool,
    return_behavior: ReturnBehavior,
) -> TextEditOutcome {
    let mut cursor_pos = cursor.min(buf.len());
    let mut changed = false;
    let mut moved = false;
    let mut submit = false;

    if insert_text(buf, &mut cursor_pos, input.text_input.as_str()) {
        changed = true;
    }

    if input.key_pressed.is_backspace() && delete_prev(buf, &mut cursor_pos, allow_leading_newline) {
        changed = true;
    }

    let delete_pressed = input.key_pressed.is_delete() || input.key_code_pressed.is_delete() || input.key_codes.is_delete();
    if delete_pressed && delete_next(buf, cursor_pos) {
        changed = true;
    }

    if input.key_code_pressed.is_left() && cursor_pos > 0 {
        cursor_pos = move_left(buf.as_str(), cursor_pos);
        moved = true;
    }

    if input.key_code_pressed.is_right() && cursor_pos < buf.len() {
        cursor_pos = move_right(buf.as_str(), cursor_pos);
        moved = true;
    }

    if input.key_pressed.is_return() {
        match return_behavior {
            ReturnBehavior::Submit => {
                submit = true;
            }
            ReturnBehavior::Newline { submit_on_ctrl } => {
                if submit_on_ctrl && input.key_mods.is_ctrl() {
                    submit = true;
                } else if insert_text(buf, &mut cursor_pos, "\n") {
                    changed = true;
                }
            }
        }
    }

    TextEditOutcome {
        cursor: cursor_pos,
        changed,
        moved,
        submit,
    }
}

pub(crate) fn line_index_for_cursor(lines: &[TextLine], cursor: usize) -> usize {
    for (idx, line) in lines.iter().enumerate() {
        if cursor <= line.end {
            return idx;
        }
    }
    lines.len().saturating_sub(1)
}

pub(crate) fn cursor_x_in_line(line: &TextLine, buf: &str, cursor: usize, font: FontId, atlas: &AtlasHandle) -> i32 {
    let end = cursor.min(line.end).max(line.start);
    if end <= line.start {
        0
    } else {
        atlas.get_text_size(font, &buf[line.start..end]).width
    }
}

pub(crate) fn cursor_from_x(line: &TextLine, buf: &str, target_x: i32, font: FontId, atlas: &AtlasHandle) -> usize {
    if target_x <= 0 {
        return line.start;
    }
    let slice = &buf[line.start..line.end];
    let mut last_width = 0;
    for (idx, ch) in slice.char_indices() {
        let next = idx + ch.len_utf8();
        let width = atlas.get_text_size(font, &slice[..next]).width;
        if target_x < width {
            if target_x < (last_width + width) / 2 {
                return line.start + idx;
            }
            return line.start + next;
        }
        last_width = width;
    }
    line.end
}

pub(crate) fn clamp_scroll(value: i32, max_value: i32) -> i32 {
    if max_value <= 0 {
        0
    } else {
        value.clamp(0, max_value)
    }
}
