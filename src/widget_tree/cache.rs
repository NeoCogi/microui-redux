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
//! Previous/current frame caches for retained node layout and interaction data.

use std::collections::HashMap;

use rs_math3d::{Recti, Vec2i};

use crate::input::{ControlState, ResourceState};

use super::NodeId;

/// Geometry resolved for a retained node in one frame.
///
/// This cache is intentionally layout-only. Parent nodes such as headers,
/// tree nodes, and embedded containers need the previous frame's rectangles to
/// react to structural input before the current frame's layout runs.
#[derive(Copy, Clone, Debug, Default)]
pub struct NodeLayout {
    /// Outer rectangle assigned to the node.
    pub rect: Recti,
    /// Inner body rectangle, when the node exposes one.
    pub body: Recti,
    /// Content size produced while traversing the node's children.
    pub content_size: Vec2i,
}

impl NodeLayout {
    /// Creates a layout snapshot for one node.
    pub const fn new(rect: Recti, body: Recti, content_size: Vec2i) -> Self {
        Self { rect, body, content_size }
    }
}

/// Interaction data sampled for a retained node in one frame.
#[derive(Copy, Clone, Debug)]
pub struct NodeInteraction {
    /// Control state observed while handling the node this frame.
    pub control: ControlState,
    /// Resource state returned by the node this frame.
    pub result: ResourceState,
}

impl NodeInteraction {
    /// Creates an interaction snapshot for one node.
    pub const fn new(control: ControlState, result: ResourceState) -> Self {
        Self { control, result }
    }
}

impl Default for NodeInteraction {
    fn default() -> Self {
        Self::new(ControlState::default(), ResourceState::NONE)
    }
}

/// Combined previous/current frame view for callers that need both layout and
/// interaction at the same time.
#[derive(Copy, Clone, Debug, Default)]
pub struct NodeFrameState {
    /// Layout resolved for the node in one frame.
    pub layout: NodeLayout,
    /// Interaction sampled for the node in one frame.
    pub interaction: NodeInteraction,
}

impl NodeFrameState {
    /// Creates a combined frame-state snapshot for one node.
    pub const fn new(layout: NodeLayout, interaction: NodeInteraction) -> Self {
        Self { layout, interaction }
    }
}

/// Previous/current frame cache for widget tree nodes.
///
/// Layout and interaction are stored in separate generations so pass 1 can read
/// only previous-frame geometry while pass 3 writes the next frame's geometry
/// and interaction outputs independently.
#[derive(Default)]
pub struct WidgetTreeCache {
    prev_layout: HashMap<NodeId, NodeLayout>,
    curr_layout: HashMap<NodeId, NodeLayout>,
    prev_interaction: HashMap<NodeId, NodeInteraction>,
    curr_interaction: HashMap<NodeId, NodeInteraction>,
}

impl WidgetTreeCache {
    /// Clears the in-progress frame cache while preserving the committed frame.
    pub fn begin_frame(&mut self) {
        self.curr_layout.clear();
        self.curr_interaction.clear();
    }

    /// Publishes the current frame cache as the previous frame for the next run.
    pub fn finish_frame(&mut self) {
        std::mem::swap(&mut self.prev_layout, &mut self.curr_layout);
        std::mem::swap(&mut self.prev_interaction, &mut self.curr_interaction);
        self.curr_layout.clear();
        self.curr_interaction.clear();
    }

    /// Drops both previous and current cached node data.
    pub fn clear(&mut self) {
        self.prev_layout.clear();
        self.curr_layout.clear();
        self.prev_interaction.clear();
        self.curr_interaction.clear();
    }

    /// Returns the previous frame layout for `node_id`.
    pub fn prev_layout(&self, node_id: NodeId) -> Option<&NodeLayout> {
        self.prev_layout.get(&node_id)
    }

    /// Returns the current frame layout for `node_id`.
    pub fn current_layout(&self, node_id: NodeId) -> Option<&NodeLayout> {
        self.curr_layout.get(&node_id)
    }

    /// Returns the previous frame interaction for `node_id`.
    pub fn prev_interaction(&self, node_id: NodeId) -> Option<&NodeInteraction> {
        self.prev_interaction.get(&node_id)
    }

    /// Returns the current frame interaction for `node_id`.
    pub fn current_interaction(&self, node_id: NodeId) -> Option<&NodeInteraction> {
        self.curr_interaction.get(&node_id)
    }

    /// Returns the previous combined frame state for `node_id`.
    pub fn prev_state(&self, node_id: NodeId) -> Option<NodeFrameState> {
        let layout = self.prev_layout(node_id).copied()?;
        let interaction = self.prev_interaction(node_id).copied().unwrap_or_default();
        Some(NodeFrameState::new(layout, interaction))
    }

    /// Returns the current combined frame state for `node_id`.
    pub fn current_state(&self, node_id: NodeId) -> Option<NodeFrameState> {
        let layout = self.current_layout(node_id).copied()?;
        let interaction = self.current_interaction(node_id).copied().unwrap_or_default();
        Some(NodeFrameState::new(layout, interaction))
    }

    /// Records the current frame layout for `node_id`.
    pub fn record_layout(&mut self, node_id: NodeId, layout: NodeLayout) {
        let prev = self.curr_layout.insert(node_id, layout);
        debug_assert!(prev.is_none(), "Node {:?} layout was recorded more than once in the same frame", node_id);
    }

    /// Records the current frame interaction for `node_id`.
    pub fn record_interaction(&mut self, node_id: NodeId, interaction: NodeInteraction) {
        let prev = self.curr_interaction.insert(node_id, interaction);
        debug_assert!(prev.is_none(), "Node {:?} interaction was recorded more than once in the same frame", node_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_interaction_generations_are_independent() {
        let node_id = NodeId::new(7);
        let layout = NodeLayout::new(Recti::new(1, 2, 3, 4), Recti::new(5, 6, 7, 8), Vec2i::new(9, 10));
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
        assert_eq!(committed_layout.content_size.x, layout.content_size.x);
        assert_eq!(committed_layout.content_size.y, layout.content_size.y);
        assert!(cache.prev_interaction(node_id).copied().unwrap().result.is_submitted());
    }
}
