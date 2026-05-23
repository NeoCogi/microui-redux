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

use rs_math3d::{Dimensioni, Recti};

use crate::input::{ControlState, ResourceState};
use super::NodeId;

/// Geometry resolved for a retained node in one frame.
///
/// This cache is intentionally layout-only. Parent nodes such as headers,
/// tree nodes, and scroll areas need the previous frame's rectangles to
/// react to structural input before the current frame's layout runs.
#[derive(Copy, Clone, Debug, Default)]
pub struct NodeLayout {
    /// Outer rectangle assigned to the node.
    pub rect: Recti,
    /// Inner body rectangle, when the node exposes one.
    pub body: Recti,
    /// Content size produced while traversing the node's children.
    pub content_size: Dimensioni,
}

impl NodeLayout {
    /// Creates a layout snapshot for one node.
    pub const fn new(rect: Recti, body: Recti, content_size: Dimensioni) -> Self {
        Self { rect, body, content_size }
    }
}

/// Interaction data sampled for a retained node in one frame.
#[allow(dead_code)]
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

/// Previous/current frame cache for widget tree nodes.
///
/// Layout and interaction are stored in separate generations so retained traversal can read
/// previous-frame geometry while writing the next frame's layout and update outputs independently.
#[derive(Default)]
pub struct WidgetTreeCache {
    layout: FrameCache<NodeLayout>,
    interaction: FrameCache<NodeInteraction>,
}

#[derive(Default)]
struct FrameCache<T> {
    previous: HashMap<NodeId, T>,
    current: HashMap<NodeId, T>,
}

impl<T> FrameCache<T> {
    fn begin_frame(&mut self) {
        self.current.clear();
    }

    fn finish_frame(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.current);
        self.current.clear();
    }

    fn clear(&mut self) {
        self.previous.clear();
        self.current.clear();
    }

    fn previous(&self, node_id: NodeId) -> Option<&T> {
        self.previous.get(&node_id)
    }

    fn current(&self, node_id: NodeId) -> Option<&T> {
        self.current.get(&node_id)
    }

    fn record(&mut self, node_id: NodeId, value: T) -> Option<T> {
        self.current.insert(node_id, value)
    }
}

impl WidgetTreeCache {
    /// Clears the in-progress frame cache while preserving the committed frame.
    pub fn begin_frame(&mut self) {
        self.layout.begin_frame();
        self.interaction.begin_frame();
    }

    /// Publishes the current frame cache as the previous frame for the next run.
    pub fn finish_frame(&mut self) {
        self.layout.finish_frame();
        self.interaction.finish_frame();
    }

    /// Drops both previous and current cached node data.
    pub fn clear(&mut self) {
        self.layout.clear();
        self.interaction.clear();
    }

    /// Returns the previous frame layout for `node_id`.
    pub fn prev_layout(&self, node_id: NodeId) -> Option<&NodeLayout> {
        self.layout.previous(node_id)
    }

    /// Returns the current frame layout for `node_id`.
    pub fn current_layout(&self, node_id: NodeId) -> Option<&NodeLayout> {
        self.layout.current(node_id)
    }

    /// Returns the previous frame interaction for `node_id`.
    #[allow(dead_code)]
    pub fn prev_interaction(&self, node_id: NodeId) -> Option<&NodeInteraction> {
        self.interaction.previous(node_id)
    }

    /// Returns the current frame interaction for `node_id`.
    #[allow(dead_code)]
    pub fn current_interaction(&self, node_id: NodeId) -> Option<&NodeInteraction> {
        self.interaction.current(node_id)
    }

    /// Records the current frame layout for `node_id`.
    pub fn record_layout(&mut self, node_id: NodeId, layout: NodeLayout) {
        let prev = self.layout.record(node_id, layout);
        debug_assert!(prev.is_none(), "Node {:?} layout was recorded more than once in the same frame", node_id);
    }

    /// Records the current frame interaction for `node_id`.
    pub fn record_interaction(&mut self, node_id: NodeId, interaction: NodeInteraction) {
        let prev = self.interaction.record(node_id, interaction);
        debug_assert!(prev.is_none(), "Node {:?} interaction was recorded more than once in the same frame", node_id);
    }
}

#[cfg(test)]
mod tests;
