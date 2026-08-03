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
