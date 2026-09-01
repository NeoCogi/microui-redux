//! Compile-time contract tests for public APIs whose safety depends on type or lifetime rejection.

/// Verifies custom-render callbacks remain bound to one concrete backend and one frame borrow.
#[test]
fn custom_renderer_callback_contracts() {
    let cases = trybuild::TestCases::new();
    // Keep one successful generic registration beside the negative fixtures. This distinguishes a
    // broken public import or signature from the backend/lifetime errors those fixtures target.
    cases.pass("tests/ui/custom_renderer_valid.rs");
    cases.compile_fail("tests/ui/custom_renderer_wrong_backend.rs");
    cases.compile_fail("tests/ui/custom_renderer_frame_escape.rs");
}

/// Keeps the FileDialog guide's setup and event-handler pattern on the actual public API.
#[test]
fn documented_file_dialog_api_compiles() {
    trybuild::TestCases::new().pass("tests/ui/file_dialog_public_api.rs");
}
