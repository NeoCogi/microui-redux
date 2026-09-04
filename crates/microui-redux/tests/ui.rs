//! Compile-time contract tests for public APIs whose safety depends on type or lifetime rejection.

/// Verifies public type, capability, borrow, and ownership contracts against exact diagnostics.
#[test]
fn public_api_contracts() {
    let cases = trybuild::TestCases::new();

    // Keep one successful generic callback registration beside the two renderer rejections.
    cases.pass("tests/ui/custom_renderer_valid.rs");
    cases.compile_fail("tests/ui/custom_renderer_wrong_backend.rs");
    cases.compile_fail("tests/ui/custom_renderer_frame_escape.rs");

    // Prove Painter is the supported recording boundary before checking DisplayList privacy.
    cases.pass("tests/ui/painter_public_api.rs");
    cases.compile_fail("tests/ui/display_list_private.rs");

    // Resolve both concrete ID types through an atlas before rejecting ownerless fabrication.
    cases.pass("tests/ui/atlas_ids_from_owner.rs");
    cases.compile_fail("tests/ui/font_id_default.rs");
    cases.compile_fail("tests/ui/icon_id_default.rs");

    // Exercise one legal operation in each widget phase before rejecting update-time painting.
    cases.pass("tests/ui/widget_phase_capabilities.rs");
    cases.compile_fail("tests/ui/widget_update_cannot_paint.rs");

    // Pair the backend's exclusive-borrow diagnostic with its valid acquire/use/drop lifecycle.
    cases.pass("tests/ui/backend_frame_acquire_once.rs");
    cases.compile_fail("tests/ui/backend_frame_acquire_twice.rs");

    // Validate cancellation and single submission before checking both ContextFrame violations.
    cases.pass("tests/ui/context_frame_lifecycle.rs");
    cases.compile_fail("tests/ui/context_mutate_during_frame.rs");
    cases.compile_fail("tests/ui/context_frame_submit_twice.rs");
}

/// Keeps the FileDialog guide's setup and event-handler pattern on the actual public API.
#[test]
fn documented_file_dialog_api_compiles() {
    trybuild::TestCases::new().pass("tests/ui/file_dialog_public_api.rs");
}
