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
//! Geometry-first cache used to pre-handle retained tree nodes across frames.

use std::collections::HashMap;

use rs_math3d::{Recti, Vec2i};

use crate::input::{ControlState, ResourceState};

use super::NodeId;

/// Cached per-frame data for a tree node keyed by [`NodeId`].
///
/// The cache is intentionally geometry-first. Parent nodes such as headers,
/// tree nodes, and embedded containers need the previous frame's rectangles to
/// react to structural input before the current frame's layout runs.
#[derive(Copy, Clone, Debug)]
pub struct NodeCacheEntry {
    /// Outer rectangle assigned to the node.
    pub rect: Recti,
    /// Inner body rectangle, when the node exposes one.
    pub body: Recti,
    /// Content size produced while traversing the node's children.
    pub content_size: Vec2i,
    /// Control state observed while handling the node this frame.
    pub control: ControlState,
    /// Resource state returned by the node this frame.
    pub result: ResourceState,
}

impl Default for NodeCacheEntry {
    fn default() -> Self {
        Self {
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Vec2i::default(),
            control: ControlState::default(),
            result: ResourceState::NONE,
        }
    }
}

/// Previous/current frame cache for widget tree nodes.
///
/// `curr` is cleared at the start of each frame, populated while the runtime
/// tree runs, then swapped into `prev` at frame end.
#[derive(Default)]
pub struct WidgetTreeCache {
    prev: HashMap<NodeId, NodeCacheEntry>,
    curr: HashMap<NodeId, NodeCacheEntry>,
}

impl WidgetTreeCache {
    /// Clears the in-progress frame cache while preserving the previous frame.
    pub fn begin_frame(&mut self) {
        self.curr.clear();
    }

    /// Publishes the current frame cache as the previous frame for the next run.
    pub fn finish_frame(&mut self) {
        std::mem::swap(&mut self.prev, &mut self.curr);
        self.curr.clear();
    }

    /// Drops both previous and current cached node state.
    pub fn clear(&mut self) {
        self.prev.clear();
        self.curr.clear();
    }

    /// Returns the previous frame state for `node_id`.
    pub fn prev(&self, node_id: NodeId) -> Option<&NodeCacheEntry> {
        self.prev.get(&node_id)
    }

    /// Returns the current frame state for `node_id`.
    pub fn current(&self, node_id: NodeId) -> Option<&NodeCacheEntry> {
        self.curr.get(&node_id)
    }

    /// Records the current frame state for `node_id`.
    pub fn record(&mut self, node_id: NodeId, state: NodeCacheEntry) {
        let prev = self.curr.insert(node_id, state);
        debug_assert!(prev.is_none(), "Node {:?} was recorded more than once in the same frame", node_id);
    }
}
