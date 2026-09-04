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
use std::borrow::Cow;

use crate::math::clamp_i64_to_i32;
use crate::ui_node::text_layout::TextLine;
use crate::{rect, AtlasHandle, FontId, Key, KeyEvent, Modifiers, Recti};

/// Determines what pressing return means for the active editor.
#[derive(Copy, Clone)]
pub(crate) enum ReturnBehavior {
    /// Return submits the edit.
    Submit,
    /// Return inserts a newline, optionally submitting when Ctrl is held.
    Newline {
        /// Whether Ctrl+Return should submit instead of inserting a newline.
        submit_on_ctrl: bool,
    },
}

/// Selects one visual side of a byte position shared by two wrapped lines.
///
/// Word wrapping does not insert a byte between adjacent visual lines: the previous line's `end`
/// is exactly the continuation line's `start`. Retaining this concrete affinity lets vertical and
/// pointer navigation keep the caret on the selected visual line without changing the public byte
/// cursor representation.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum CaretAffinity {
    /// Prefer the visual line ending at the shared byte position.
    #[default]
    Upstream,
    /// Prefer the continuation line starting at the shared byte position.
    Downstream,
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

/// Converts multiline text to the editor's sole stored newline representation.
///
/// CRLF becomes one LF and a lone CR becomes one LF. The original allocation is returned unchanged
/// when it already contains only LF line endings, keeping the common path allocation-free beyond
/// the caller-owned `String` itself.
pub(crate) fn normalize_multiline_text(text: impl Into<String>) -> String {
    let text = text.into();
    if !text.contains('\r') {
        return text;
    }

    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            // Consume the LF half of CRLF so one logical line ending occupies one stored byte.
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }
    normalized
}

/// Removes line endings from text entering a single-line editor.
///
/// Both halves of CRLF are discarded rather than replaced with presentation-only whitespace. The
/// resulting stored value therefore always satisfies the textbox's one-line measurement contract.
pub(crate) fn normalize_single_line_text(text: impl Into<String>) -> String {
    let mut text = text.into();
    text.retain(|ch| ch != '\r' && ch != '\n');
    text
}

/// Borrows ordinary text input directly and allocates only when its line endings need rewriting.
fn normalize_input(text: &str, behavior: ReturnBehavior) -> Cow<'_, str> {
    match behavior {
        ReturnBehavior::Submit if text.contains('\r') || text.contains('\n') => Cow::Owned(normalize_single_line_text(text)),
        ReturnBehavior::Newline { .. } if text.contains('\r') => Cow::Owned(normalize_multiline_text(text)),
        ReturnBehavior::Submit | ReturnBehavior::Newline { .. } => Cow::Borrowed(text),
    }
}

/// Reads line-height, baseline, and descent from the atlas.
pub(crate) fn font_line_metrics(font: FontId, atlas: &AtlasHandle) -> FontLineMetrics {
    let line_height = atlas.get_font_height(font) as i32;
    let baseline = atlas.get_font_baseline(font);
    let descent = line_height.saturating_sub(baseline).max(0);
    FontLineMetrics { line_height, baseline, descent }
}

/// Centers a single text line inside bounds while keeping it fully clipped to the bounds.
pub(crate) fn centered_line_top(bounds: Recti, line_height: i32) -> i32 {
    let bounds_y = i64::from(bounds.y);
    let candidate = bounds_y + i64::from(bounds.height) / 2 - i64::from(line_height) / 2;
    let max_text_y = (bounds_y + i64::from(bounds.height) - i64::from(line_height)).max(bounds_y);
    // Clamp the complete alignment expression in i64 so a large font and extreme rectangle origin
    // cannot overflow before the final retained coordinate is produced.
    clamp_i64_to_i32(candidate.clamp(bounds_y, max_text_y))
}

/// Builds a one-pixel caret rectangle clipped to the visible text area.
pub(crate) fn caret_rect(x: i32, baseline_y: i32, metrics: FontLineMetrics, clip: Recti) -> Recti {
    let clip_top = i64::from(clip.y);
    let clip_bottom = clip_top + i64::from(clip.height.max(0));
    let caret_top = (i64::from(baseline_y) - i64::from(metrics.baseline) + 2).clamp(clip_top, clip_bottom);
    let caret_bottom = (i64::from(baseline_y) + i64::from(metrics.descent) - 2).clamp(clip_top, clip_bottom);
    let height = (caret_bottom - caret_top).clamp(1, i64::from(i32::MAX)) as i32;
    rect(x, clamp_i64_to_i32(caret_top), 1, height)
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

/// Deletes the UTF-8 scalar value immediately before the cursor.
fn delete_prev(buf: &mut String, cursor: &mut usize) -> bool {
    if buf.is_empty() {
        return false;
    }
    *cursor = clamp_cursor_boundary(buf, *cursor);
    if *cursor == 0 {
        // Backspace never deletes text at or after the cursor. Delete owns the forward direction.
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
    key_event: Option<KeyEvent>,
    return_behavior: ReturnBehavior,
) -> TextEditOutcome {
    let mut cursor_pos = clamp_cursor_boundary(buf, cursor);
    let mut changed = false;
    let mut moved = false;
    let mut submit = false;

    // Normalize pasted/platform text at the same boundary as typed input. Constructors and public
    // setters apply the corresponding storage policy, so no mutation path can bypass it.
    let text_input = normalize_input(text_input, return_behavior);
    if insert_text(buf, &mut cursor_pos, text_input.as_ref()) {
        changed = true;
    }

    // Releases never mutate editor state. Key repeat arrives as another pressed transition and
    // therefore naturally repeats editing without a retained set of held navigation keys.
    let pressed = key_event.filter(|event| event.is_pressed());

    if pressed.is_some_and(|event| event.key == Key::Backspace) && delete_prev(buf, &mut cursor_pos) {
        changed = true;
    }

    if pressed.is_some_and(|event| event.key == Key::Delete) && delete_next(buf, cursor_pos) {
        changed = true;
    }

    if pressed.is_some_and(|event| event.key == Key::ArrowLeft) && cursor_pos > 0 {
        cursor_pos = move_left(buf.as_str(), cursor_pos);
        moved = true;
    }

    if pressed.is_some_and(|event| event.key == Key::ArrowRight) && cursor_pos < buf.len() {
        cursor_pos = move_right(buf.as_str(), cursor_pos);
        moved = true;
    }

    if let Some(event) = pressed.filter(|event| event.key == Key::Enter) {
        match return_behavior {
            ReturnBehavior::Submit => {
                submit = true;
            }
            ReturnBehavior::Newline { submit_on_ctrl } => {
                // Text areas can use Ctrl+Enter for submit while plain Enter inserts a newline.
                if submit_on_ctrl && event.modifiers.intersects(Modifiers::CTRL) {
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

/// Finds the display line containing `cursor`, preserving a wrapped-boundary preference.
pub(crate) fn line_index_for_cursor(lines: &[TextLine], cursor: usize, affinity: CaretAffinity) -> usize {
    for (idx, line) in lines.iter().enumerate() {
        if cursor < line.end {
            return idx;
        }
        if cursor == line.end {
            // Only wrapped neighbors share a boundary. Logical newline-separated lines leave the
            // delimiter byte between their ranges and therefore remain unambiguous.
            if affinity == CaretAffinity::Downstream && lines.get(idx + 1).is_some_and(|next| next.start == cursor) {
                return idx + 1;
            }
            return idx;
        }
    }
    // last_line_index = line_count - 1, remaining at zero for an empty list.
    lines.len().saturating_sub(1)
}

/// Chooses the affinity that resolves `cursor` back to `selected_line`.
pub(crate) fn affinity_for_line(lines: &[TextLine], selected_line: usize, cursor: usize) -> CaretAffinity {
    if selected_line > 0
        && lines.get(selected_line).is_some_and(|line| line.start == cursor)
        && lines.get(selected_line - 1).is_some_and(|line| line.end == cursor)
    {
        CaretAffinity::Downstream
    } else {
        CaretAffinity::Upstream
    }
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
            let midpoint = (i64::from(last_width) + i64::from(width)) / 2;
            if i64::from(target_x) < midpoint {
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
            let midpoint = (i64::from(last_width) + i64::from(width)) / 2;
            if i64::from(target_x) < midpoint {
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
    use crate::{Key, KeyEvent, Modifiers};

    #[test]
    fn text_input_clamps_external_cursor_to_utf8_boundary() {
        let mut buf = String::from("éa");
        let outcome = apply_text_input(&mut buf, 1, "x", None, ReturnBehavior::Submit);

        assert_eq!(buf, "xéa");
        assert_eq!(outcome.cursor, 1);
        assert!(outcome.changed);
    }

    #[test]
    fn delete_next_clamps_external_cursor_to_utf8_boundary() {
        let mut buf = String::from("éa");
        let outcome = apply_text_input(&mut buf, 1, "", Some(KeyEvent::pressed(Key::Delete, Modifiers::NONE)), ReturnBehavior::Submit);

        assert_eq!(buf, "a");
        assert_eq!(outcome.cursor, 0);
        assert!(outcome.changed);
    }

    /// Verifies platform and legacy line endings collapse to the one multiline representation.
    #[test]
    fn multiline_normalization_collapses_crlf_and_lone_cr() {
        assert_eq!(normalize_multiline_text("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    /// Verifies single-line storage cannot retain either newline scalar.
    #[test]
    fn single_line_normalization_removes_all_line_endings() {
        assert_eq!(normalize_single_line_text("a\r\nb\rc\n"), "abc");
    }

    /// Verifies pasted text follows the same newline policy as constructors and setters.
    #[test]
    fn text_input_normalizes_for_each_editor_kind() {
        let mut single = String::new();
        let single_outcome = apply_text_input(&mut single, 0, "a\r\nb\rc\n", None, ReturnBehavior::Submit);
        assert_eq!(single, "abc");
        assert_eq!(single_outcome.cursor, 3);

        let mut multiline = String::new();
        let multiline_outcome = apply_text_input(&mut multiline, 0, "a\r\nb\rc\n", None, ReturnBehavior::Newline { submit_on_ctrl: true });
        assert_eq!(multiline, "a\nb\nc\n");
        assert_eq!(multiline_outcome.cursor, multiline.len());
    }

    /// Verifies backspace at the beginning never removes content after the cursor.
    #[test]
    fn backspace_at_zero_does_not_delete_a_leading_newline() {
        let mut buf = String::from("\ntext");
        let outcome = apply_text_input(
            &mut buf,
            0,
            "",
            Some(KeyEvent::pressed(Key::Backspace, Modifiers::NONE)),
            ReturnBehavior::Newline { submit_on_ctrl: true },
        );

        assert_eq!(buf, "\ntext");
        assert_eq!(outcome.cursor, 0);
        assert!(!outcome.changed);
    }

    /// Verifies one shared byte position can resolve to either wrapped visual line.
    #[test]
    fn wrapped_boundary_resolution_respects_caret_affinity() {
        let lines = [TextLine { start: 0, end: 2, width: 16 }, TextLine { start: 2, end: 3, width: 8 }];

        assert_eq!(line_index_for_cursor(&lines, 2, CaretAffinity::Upstream), 0);
        assert_eq!(line_index_for_cursor(&lines, 2, CaretAffinity::Downstream), 1);
        assert_eq!(affinity_for_line(&lines, 0, 2), CaretAffinity::Upstream);
        assert_eq!(affinity_for_line(&lines, 1, 2), CaretAffinity::Downstream);
    }
}
