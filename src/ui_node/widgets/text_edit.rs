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
//! UTF-8-safe text editing primitives shared by textbox and text-area widgets.
//!
//! The helpers in this file keep cursor indices on valid byte boundaries, apply keyboard/text
//! input, and translate pointer positions into cursor locations. Cursor movement and deletion use
//! Unicode scalar-value boundaries, not grapheme-cluster boundaries.
use crate::ui_node::text_layout::TextLine;
use crate::{rect, AtlasHandle, FontId, KeyCode, KeyMode, Recti};

/// Determines what pressing return means for the active editor.
pub(crate) enum ReturnBehavior {
    /// Return submits the edit.
    Submit,
    /// Return inserts a newline, optionally submitting when Ctrl is held.
    Newline {
        /// Whether Ctrl+Return should submit instead of inserting a newline.
        submit_on_ctrl: bool,
    },
}

/// Result of applying one input snapshot to a text buffer.
pub(crate) struct TextEditOutcome {
    /// New cursor byte position.
    pub cursor: usize,
    /// Whether the text buffer changed.
    pub changed: bool,
    /// Whether the cursor moved.
    pub moved: bool,
    /// Whether the edit requested submission.
    pub submit: bool,
}

#[derive(Copy, Clone)]
/// Metrics needed to align text and draw a caret consistently.
pub(crate) struct FontLineMetrics {
    /// Distance between baselines in pixels.
    pub line_height: i32,
    /// Baseline offset from the top of the line.
    pub baseline: i32,
    /// Descent below the baseline.
    pub descent: i32,
}

/// Reads line-height, baseline, and descent from the atlas.
pub(crate) fn font_line_metrics(font: FontId, atlas: &AtlasHandle) -> FontLineMetrics {
    let line_height = atlas.get_font_height(font) as i32;
    let baseline = atlas.get_font_baseline(font);
    let descent = (line_height - baseline).max(0);
    FontLineMetrics { line_height, baseline, descent }
}

/// Centers a single text line inside bounds while keeping it fully clipped to the bounds.
pub(crate) fn centered_line_top(bounds: Recti, line_height: i32) -> i32 {
    let mut text_y = bounds.y + bounds.height / 2 - line_height / 2;
    if text_y < bounds.y {
        text_y = bounds.y;
    }
    let max_text_y = (bounds.y + bounds.height - line_height).max(bounds.y);
    if text_y > max_text_y {
        text_y = max_text_y;
    }
    text_y
}

/// Builds a one-pixel caret rectangle clipped to the visible text area.
pub(crate) fn caret_rect(x: i32, baseline_y: i32, metrics: FontLineMetrics, clip: Recti) -> Recti {
    let caret_top = (baseline_y - metrics.baseline + 2).max(clip.y).min(clip.y + clip.height);
    let caret_bottom = (baseline_y + metrics.descent - 2).max(clip.y).min(clip.y + clip.height);
    rect(x, caret_top, 1, (caret_bottom - caret_top).max(1))
}

/// Clamps a byte cursor to the nearest previous Unicode scalar-value boundary.
pub(crate) fn clamp_cursor_boundary(buf: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(buf.len());
    while cursor > 0 && !buf.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

/// Inserts text at a valid cursor boundary and advances the cursor past the inserted text.
fn insert_text(buf: &mut String, cursor: &mut usize, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let insert_at = clamp_cursor_boundary(buf, *cursor);
    buf.insert_str(insert_at, text);
    *cursor = insert_at + text.len();
    true
}

/// Deletes the previous UTF-8 scalar value, with optional leading-newline cleanup for text areas.
fn delete_prev(buf: &mut String, cursor: &mut usize, allow_leading_newline: bool) -> bool {
    if buf.is_empty() {
        return false;
    }
    *cursor = clamp_cursor_boundary(buf, *cursor);
    if *cursor == 0 {
        if allow_leading_newline && buf.as_bytes().first() == Some(&b'\n') {
            buf.replace_range(0..1, "");
            return true;
        }
        return false;
    }
    let mut start = *cursor;
    start -= 1;
    // Walk back to the start byte of the previous UTF-8 scalar.
    while start > 0 && !buf.is_char_boundary(start) {
        start -= 1;
    }
    buf.replace_range(start..*cursor, "");
    *cursor = start;
    true
}

/// Deletes the next UTF-8 scalar value after `cursor`.
fn delete_next(buf: &mut String, cursor: usize) -> bool {
    let cursor = clamp_cursor_boundary(buf, cursor);
    if buf.is_empty() || cursor >= buf.len() {
        return false;
    }
    let mut end = cursor + 1;
    // Walk forward to the first boundary after the deleted scalar.
    while end < buf.len() && !buf.is_char_boundary(end) {
        end += 1;
    }
    buf.replace_range(cursor..end, "");
    true
}

/// Moves the cursor one UTF-8 scalar value to the left.
fn move_left(buf: &str, cursor: usize) -> usize {
    let cursor = clamp_cursor_boundary(buf, cursor);
    if cursor == 0 {
        return 0;
    }
    let mut new_cursor = cursor - 1;
    while new_cursor > 0 && !buf.is_char_boundary(new_cursor) {
        new_cursor -= 1;
    }
    new_cursor
}

/// Moves the cursor one UTF-8 scalar value to the right.
fn move_right(buf: &str, cursor: usize) -> usize {
    let cursor = clamp_cursor_boundary(buf, cursor);
    if cursor >= buf.len() {
        return buf.len();
    }
    let mut new_cursor = cursor + 1;
    while new_cursor < buf.len() && !buf.is_char_boundary(new_cursor) {
        new_cursor += 1;
    }
    new_cursor
}

/// Applies text, editing keys, cursor keys, and return behavior to a UTF-8 buffer.
pub(crate) fn apply_text_input(
    buf: &mut String,
    cursor: usize,
    text_input: &str,
    key_mods: KeyMode,
    key_pressed: KeyMode,
    key_code_pressed: KeyCode,
    allow_leading_newline: bool,
    return_behavior: ReturnBehavior,
) -> TextEditOutcome {
    let mut cursor_pos = clamp_cursor_boundary(buf, cursor);
    let mut changed = false;
    let mut moved = false;
    let mut submit = false;

    if insert_text(buf, &mut cursor_pos, text_input) {
        changed = true;
    }

    if key_pressed.intersects(KeyMode::BACKSPACE) && delete_prev(buf, &mut cursor_pos, allow_leading_newline) {
        changed = true;
    }

    let delete_pressed = key_pressed.intersects(KeyMode::DELETE) || key_code_pressed.intersects(KeyCode::DELETE);
    if delete_pressed && delete_next(buf, cursor_pos) {
        changed = true;
    }

    if key_code_pressed.intersects(KeyCode::LEFT) && cursor_pos > 0 {
        cursor_pos = move_left(buf.as_str(), cursor_pos);
        moved = true;
    }

    if key_code_pressed.intersects(KeyCode::RIGHT) && cursor_pos < buf.len() {
        cursor_pos = move_right(buf.as_str(), cursor_pos);
        moved = true;
    }

    if key_pressed.intersects(KeyMode::RETURN) {
        match return_behavior {
            ReturnBehavior::Submit => {
                submit = true;
            }
            ReturnBehavior::Newline { submit_on_ctrl } => {
                // Text areas can use Ctrl+Enter for submit while plain Enter inserts a newline.
                if submit_on_ctrl && key_mods.intersects(KeyMode::CTRL) {
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

/// Finds the display line containing `cursor`.
pub(crate) fn line_index_for_cursor(lines: &[TextLine], cursor: usize) -> usize {
    for (idx, line) in lines.iter().enumerate() {
        if cursor <= line.end {
            return idx;
        }
    }
    // last_line_index = line_count - 1, remaining at zero for an empty list.
    lines.len().saturating_sub(1)
}

/// Returns the x offset of `cursor` measured from the start of `line`.
pub(crate) fn cursor_x_in_line(line: &TextLine, buf: &str, cursor: usize, font: FontId, atlas: &AtlasHandle) -> i32 {
    let end = cursor.min(line.end).max(line.start);
    if end <= line.start {
        0
    } else {
        atlas.get_text_size(font, &buf[line.start..end]).width
    }
}

/// Converts a target x coordinate inside one line into the nearest UTF-8 cursor position.
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
            // Snap to whichever side of the glyph midpoint the target falls on.
            if target_x < (last_width + width) / 2 {
                return line.start + idx;
            }
            return line.start + next;
        }
        last_width = width;
    }
    line.end
}

/// Converts a target x coordinate in single-line text into the nearest UTF-8 cursor position.
pub(crate) fn cursor_from_text_x(buf: &str, target_x: i32, font: FontId, atlas: &AtlasHandle) -> usize {
    if target_x <= 0 {
        return 0;
    }

    let mut last_width = 0;
    for (idx, ch) in buf.char_indices() {
        let next = idx + ch.len_utf8();
        let width = atlas.get_text_size(font, &buf[..next]).width;
        if target_x < width {
            if target_x < (last_width + width) / 2 {
                return idx;
            }
            return next;
        }
        last_width = width;
    }
    buf.len()
}

#[cfg(test)]
mod tests {
    //! Tests for UTF-8 safe text editing primitives.

    use super::*;
    use crate::{KeyCode, KeyMode};

    #[test]
    fn text_input_clamps_external_cursor_to_utf8_boundary() {
        let mut buf = String::from("éa");
        let outcome = apply_text_input(&mut buf, 1, "x", KeyMode::NONE, KeyMode::NONE, KeyCode::NONE, false, ReturnBehavior::Submit);

        assert_eq!(buf, "xéa");
        assert_eq!(outcome.cursor, 1);
        assert!(outcome.changed);
    }

    #[test]
    fn delete_next_clamps_external_cursor_to_utf8_boundary() {
        let mut buf = String::from("éa");
        let outcome = apply_text_input(&mut buf, 1, "", KeyMode::NONE, KeyMode::NONE, KeyCode::DELETE, false, ReturnBehavior::Submit);

        assert_eq!(buf, "a");
        assert_eq!(outcome.cursor, 0);
        assert!(outcome.changed);
    }
}
