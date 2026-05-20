use super::*;

#[test]
fn layout_and_interaction_generations_are_independent() {
    let node_id = NodeId::new(7);
    let layout = NodeLayout::new(Recti::new(1, 2, 3, 4), Recti::new(5, 6, 7, 8), Dimensioni::new(9, 10));
    let interaction = NodeInteraction::new(ControlState::default(), ResourceState::SUBMIT);

    let mut cache = WidgetTreeCache::default();
    cache.record_layout(node_id, layout);

    let current_layout = cache.current_layout(node_id).copied().unwrap();
    assert_eq!(current_layout.rect.x, layout.rect.x);
    assert_eq!(current_layout.rect.y, layout.rect.y);
    assert_eq!(current_layout.rect.width, layout.rect.width);
    assert_eq!(current_layout.rect.height, layout.rect.height);
    assert!(cache.current_interaction(node_id).is_none());

    cache.record_interaction(node_id, interaction);
    cache.finish_frame();

    let committed_layout = cache.prev_layout(node_id).copied().unwrap();
    assert_eq!(committed_layout.content_size.width, layout.content_size.width);
    assert_eq!(committed_layout.content_size.height, layout.content_size.height);
    assert!(cache.prev_interaction(node_id).copied().unwrap().result.is_submitted());
}
