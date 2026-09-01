//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

//! Parent-first widget update traversal and interaction snapshots.

use super::*;

impl UiRuntime {
    /// Updates one already-borrowed node and descendants.
    pub(super) fn update_node_ref(&mut self, node: &mut Node, parent_transform: Transform, style: &Style, atlas: crate::AtlasHandle, input: InputSnapshot) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.updates += 1);
        let style = node.resolve_style(style);
        let style = &style;
        // Reconstruct exactly the frame/content coordinate spaces committed during layout. Widgets
        // see content-local geometry even though allocations and inherited clips use other spaces.
        let frame_role = node_frame_role(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let frame_geometry = crate::ui_node::frame::frame_geometry(local_rect, frame_role, style);
        let content_rect = frame_geometry.content_or_empty();
        let screen_clip = parent_transform.clip.positive_intersection(screen_rect).unwrap_or_default();
        let child_transform = parent_transform.push(node.state.layout);
        let local_clip = screen_clip.relative_to(screen_origin);
        let content_clip = local_clip
            .positive_intersection(content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));

        // Snapshot interaction before invoking user code so all reads during this update are stable.
        let (opt, _keyboard) = node_interaction_config(node);
        let id = node.id();
        let was_focused = node.state.focused;
        let (hovered, focused, clicked, active) = self.input_router.commit_interaction_snapshot(id, node.state.hovered, input, opt);
        node.state.hovered = hovered;
        node.state.focused = focused;
        node.state.clicked = clicked;
        node.state.active = active;

        // Only the preselected recipient takes the routed event; all other nodes still receive their
        // normal eventless update in parent-first order.
        let event = self
            .input_router
            .take_routed_event(id)
            .map(|event| super::widget_context::localize_event(Vec2i::new(content_rect.x, content_rect.y), event));
        let accepts_pointer_input = self.input_router.accepts_pointer_input();
        let screen_content_rect = content_rect.translated(screen_origin);
        let screen_content_clip = content_clip.translated(screen_origin);
        let mut widget_ctx = crate::WidgetUpdateCtx::new_with_content_geometry(
            screen_content_rect,
            screen_content_clip,
            style,
            &atlas,
            accepts_pointer_input,
            node.state.hovered,
            node.state.focused,
            node.state.clicked,
            node.state.active,
            input.mouse_buttons,
            input.modifiers,
        );
        node.data.with_widget_mut(|widget| widget.update(&mut widget_ctx, event.as_ref()));
        if event.is_some() || was_focused != node.state.focused {
            // A routed event is precisely a consumed/captured delivery. A focus transition is the
            // other generic update trigger: the old focus owner may commit intrinsic state while
            // receiving `None` (for example an inline numeric editor committing on blur). Both
            // write the same node-local marker as typed handles; layout performs the sole upward
            // propagation after the update traversal completes.
            node.mark_measurement_dirty();
        }
        let traverse_children = node.is_container();
        if traverse_children {
            node.with_children_mut(|children| {
                for child in children
                    .iter_mut()
                    .filter(|child| node_is_visible(child) && child.intersects_clip(child_transform))
                {
                    // Forward order is observable by deliberate cross-cell mutation: a later child
                    // sees successful earlier changes, while an already-updated child is not rerun.
                    // The mandatory post-event layout observes the final state/topology. Rendering
                    // enters the tree later through the distinct paint traversal below.
                    self.update_node_ref(child, child_transform, style, atlas.clone(), input);
                }
            });
        }
    }
}
