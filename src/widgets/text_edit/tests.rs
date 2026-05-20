use super::*;
use crate::KeyCode;

#[test]
fn text_input_clamps_external_cursor_to_utf8_boundary() {
    let mut buf = String::from("éa");
    let input = InputSnapshot {
        text_input: String::from("x"),
        ..Default::default()
    };

    let outcome = apply_text_input(&mut buf, 1, &input, false, ReturnBehavior::Submit);

    assert_eq!(buf, "xéa");
    assert_eq!(outcome.cursor, 1);
    assert!(outcome.changed);
}

#[test]
fn delete_next_clamps_external_cursor_to_utf8_boundary() {
    let mut buf = String::from("éa");
    let input = InputSnapshot {
        key_code_pressed: KeyCode::DELETE,
        ..Default::default()
    };

    let outcome = apply_text_input(&mut buf, 1, &input, false, ReturnBehavior::Submit);

    assert_eq!(buf, "a");
    assert_eq!(outcome.cursor, 0);
    assert!(outcome.changed);
}
