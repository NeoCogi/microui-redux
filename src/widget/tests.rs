//! Tests for retained frame-result generation views.

use super::*;

#[test]
fn committed_and_current_generation_views_are_explicit() {
    let committed_id = Id::new(1);
    let current_id = Id::new(2);

    let mut results = FrameResults::default();
    results.record_node_with_context(RetainedId::node(committed_id), committed_id, ResourceState::SUBMIT, "committed");
    results.finish_frame();
    results.begin_frame();
    results.record_node_with_context(RetainedId::node(current_id), current_id, ResourceState::CHANGE, "current");

    assert!(results.committed().state_of_node(committed_id).is_submitted());
    assert!(results.current().state_of_node(committed_id).is_none());
    assert!(results.current().state_of_node(current_id).is_changed());
}
