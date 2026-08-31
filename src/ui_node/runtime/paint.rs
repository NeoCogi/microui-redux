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

//! Parent-first paint traversal and custom-render dispatch.

use super::*;

/// Deferred focus-outline geometry captured while painting one retained widget tree.
///
/// The outline is emitted only after the complete tree, including custom rendering, so later
/// descendants and siblings cannot cover the current keyboard target inside the same surface.
#[derive(Copy, Clone)]
struct FocusIndicator {
    /// Focused widget's complete outer allocation in screen coordinates.
    rect: Recti,
    /// Traversal-derived screen clip that prevents the outline escaping scroll or parent clips.
    clip: Recti,
    /// Resolved per-node Style accent, including a widget-local style override when present.
    color: crate::Color,
    /// Inside-aligned stroke width shared with ordinary Style-owned frames.
    width: i32,
}

impl FocusIndicator {
    /// Records the final focus outline after every ordinary and custom operation in the tree.
    fn record(self, display_list: &mut DisplayList) {
        // Transparent accents intentionally disable the visual without changing focus routing.
        if self.color.a == 0 || self.rect.width <= 0 || self.rect.height <= 0 {
            return;
        }
        let mut painter = crate::render::Painter::screen_space(display_list, self.clip);
        painter.stroke_rect(self.rect, self.width.max(1), self.color);
    }
}

impl UiRuntime {
    /// Paints one persistent root and records at most one scope-visible focus outline last.
    pub(crate) fn paint_tree_root(&mut self, root: &mut Node, display_list: &mut DisplayList, style: &Style, atlas: crate::AtlasHandle, focus_visible: bool) {
        // Every runtime remembers focus independently, but only the manager-selected keyboard
        // surface may present it. This prevents inactive windows and menu-suspended widgets from
        // showing simultaneous carets, fills, or outlines.
        if let Some(indicator) = self.paint_node_ref(root, self.root_transform, display_list, style, atlas, focus_visible) {
            indicator.record(display_list);
        }
    }

    /// Paints one already-borrowed node and descendants.
    fn paint_node_ref(
        &mut self,
        node: &mut Node,
        parent_transform: Transform,
        display_list: &mut DisplayList,
        style: &Style,
        atlas: crate::AtlasHandle,
        focus_visible: bool,
    ) -> Option<FocusIndicator> {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.paints += 1);
        let style = node.resolve_style(style);
        let style = &style;
        // Resolve the same outer/content geometry used for input and update before recording paint.
        let framed = node_is_framed(node);
        let screen_rect = parent_transform.resolve(node.state.layout.allocation);
        let screen_origin = Vec2i::new(screen_rect.x, screen_rect.y);
        let local_rect = Recti::new(0, 0, screen_rect.width, screen_rect.height);
        let frame_geometry = crate::ui_node::frame::frame_geometry(local_rect, framed, style);
        let content_rect = frame_geometry.content_or_empty();
        let screen_clip = parent_transform.clip.positive_intersection(screen_rect).unwrap_or_default();
        if framed {
            // Generic framing belongs beneath the widget's own paint and descendant paint.
            let mut painter = crate::render::Painter::screen_space(display_list, screen_clip);
            crate::ui_node::frame::paint_internal_frame(&mut painter, screen_rect, None, style.frame_border());
        }
        let child_transform = parent_transform.push(node.state.layout);
        let local_clip = screen_clip.relative_to(screen_origin);
        let content_clip = local_clip
            .positive_intersection(content_rect)
            .unwrap_or_else(|| Recti::new(content_rect.x, content_rect.y, 0, 0));
        let screen_content_rect = content_rect.translated(screen_origin);
        let screen_content_clip = content_clip.translated(screen_origin);
        let focused = focus_visible && node.state.focused;
        let mut focus_indicator = focused.then_some(FocusIndicator {
            rect: screen_rect,
            clip: screen_clip,
            color: style.focus_color,
            width: style.frame_border_width.max(1),
        });
        {
            // Limit the mutable display-list borrow to this widget call before custom/child output.
            let mut widget_ctx = crate::WidgetPaintCtx::new_with_content_geometry(
                screen_content_rect,
                display_list,
                screen_content_clip,
                style,
                &atlas,
                node.state.hovered,
                focused,
                node.state.clicked,
                node.state.active,
            );
            node.data.with_widget_mut(|widget| widget.paint(&mut widget_ctx));
        }

        if let NodeKind::Widget(widget) = &node.data
            && let Some(renderer) = widget.custom_render()
        {
            // Custom backend work is an ordered barrier immediately after ordinary widget paint.
            display_list.push_custom(screen_content_clip, renderer, screen_content_rect);
        }

        let traverse_children = node.is_container();
        if traverse_children {
            // Children paint after their ordinary parent surface, matching reverse-order hit tests.
            node.with_children_mut(|children| {
                for child in children
                    .iter_mut()
                    .filter(|child| node_is_visible(child) && child.intersects_clip(child_transform))
                {
                    if let Some(child_focus) = self.paint_node_ref(child, child_transform, display_list, style, atlas.clone(), focus_visible) {
                        // Focus identity is singular by invariant. Prefer a descendant defensively
                        // if externally mutated state ever exposes both an ancestor and child.
                        focus_indicator = Some(child_focus);
                    }
                }
            });
        }
        focus_indicator
    }
}
