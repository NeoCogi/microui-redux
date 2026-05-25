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

use crate::input::ControlState;
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

/// Previous/current frame cache for widget tree nodes.
///
/// Layout and control state are stored separately so retained traversal can read previous-frame
/// geometry while painting with the control state produced by the current update pass.
#[derive(Default)]
pub struct WidgetTreeCache {
    /// Previous/current layout snapshots keyed by node id.
    layout: FrameCache<NodeLayout>,
    /// Previous/current control snapshots keyed by node id.
    control: FrameCache<ControlState>,
}

#[derive(Default)]
/// Previous/current map pair for one retained node data type.
struct FrameCache<T> {
    /// Published data from the previous completed frame.
    previous: HashMap<NodeId, T>,
    /// Data recorded during the current frame.
    current: HashMap<NodeId, T>,
}

impl<T> FrameCache<T> {
    /// Clears current-frame data before traversal records a new generation.
    fn begin_frame(&mut self) {
        self.current.clear();
    }

    /// Promotes current-frame data to previous-frame data.
    fn finish_frame(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.current);
        self.current.clear();
    }

    /// Clears both previous and current generations.
    fn clear(&mut self) {
        self.previous.clear();
        self.current.clear();
    }

    #[cfg(test)]
    fn previous(&self, node_id: NodeId) -> Option<&T> {
        self.previous.get(&node_id)
    }

    /// Returns current-frame data for `node_id`.
    fn current(&self, node_id: NodeId) -> Option<&T> {
        self.current.get(&node_id)
    }

    /// Records current-frame data, returning a previous value if one existed this frame.
    fn record(&mut self, node_id: NodeId, value: T) -> Option<T> {
        self.current.insert(node_id, value)
    }
}

impl WidgetTreeCache {
    /// Clears the in-progress frame cache while preserving the committed frame.
    pub fn begin_frame(&mut self) {
        self.layout.begin_frame();
        self.control.begin_frame();
    }

    /// Publishes the current frame cache as the previous frame for the next run.
    pub fn finish_frame(&mut self) {
        self.layout.finish_frame();
        self.control.finish_frame();
    }

    /// Drops both previous and current cached node data.
    pub fn clear(&mut self) {
        self.layout.clear();
        self.control.clear();
    }

    /// Returns the previous frame layout for `node_id`.
    #[cfg(test)]
    pub fn prev_layout(&self, node_id: NodeId) -> Option<&NodeLayout> {
        self.layout.previous(node_id)
    }

    /// Returns the current frame layout for `node_id`.
    pub fn current_layout(&self, node_id: NodeId) -> Option<&NodeLayout> {
        self.layout.current(node_id)
    }

    /// Returns the current frame control state for `node_id`.
    pub fn current_control(&self, node_id: NodeId) -> Option<&ControlState> {
        self.control.current(node_id)
    }

    /// Records the current frame layout for `node_id`.
    pub fn record_layout(&mut self, node_id: NodeId, layout: NodeLayout) {
        let prev = self.layout.record(node_id, layout);
        if prev.is_some() {
            panic!("Node {:?} layout was recorded more than once in the same frame", node_id);
        }
    }

    /// Records the current frame control state for `node_id`.
    pub fn record_control(&mut self, node_id: NodeId, control: ControlState) {
        let prev = self.control.record(node_id, control);
        if prev.is_some() {
            panic!("Node {:?} control state was recorded more than once in the same frame", node_id);
        }
    }
}

#[cfg(test)]
mod tests;
