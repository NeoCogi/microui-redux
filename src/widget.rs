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
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
//! Widget runtime contracts and per-frame result tracking.

use std::cmp::max;
use std::collections::HashMap;

use rs_math3d::Dimensioni;

use crate::atlas::{AtlasHandle, EXPAND_DOWN_ICON};
use crate::context::RootId;
use crate::id::Id;
use crate::input::{ControlState, ResourceState, ScrollBehavior, WidgetOption};
use crate::style::Style;
pub use crate::widget_ctx::WidgetCtx;

/// High-level focus behavior requested by a widget.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FocusPolicy {
    /// Focus is only needed for the click interaction and clears when the button is released.
    Momentary,
    /// Focus remains after release until the widget explicitly clears it or another click moves it.
    HoldUntilBlur,
    /// Focus captures a pointer drag and clears when the drag button is released.
    DragCapture,
}

impl FocusPolicy {
    /// Derives a policy from legacy widget options.
    pub fn from_widget_options(opt: WidgetOption) -> Self {
        if opt.is_holding_focus() { Self::HoldUntilBlur } else { Self::Momentary }
    }

    /// Returns whether focus should clear when the pointer button is released.
    pub(crate) fn releases_on_mouse_up(self) -> bool {
        matches!(self, Self::Momentary | Self::DragCapture)
    }
}

/// Trait implemented by persistent widget state structures.
///
/// Widgets participate in three retained execution phases:
/// 1. `measure`, which reports intrinsic size for the current frame's layout pass.
/// 2. `update`, which samples interaction, mutates widget-local state, and produces the current
///    frame result.
/// 3. `paint`, which records paint commands for the updated widget state.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the scroll behavior for this state.
    fn scroll_behavior(&self) -> ScrollBehavior {
        ScrollBehavior::NONE
    }
    /// Returns the intrinsic widget size for the current frame's layout pass.
    ///
    /// `avail` reports the current container body size visible to the widget.
    /// Values less than or equal to zero are treated as "use layout defaults" for that axis.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    /// Updates retained widget state for the current frame and returns its interaction result.
    fn update(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
    /// Records paint commands for the current frame.
    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState);
    /// Returns the effective widget options used by generic dispatch.
    ///
    /// Widgets can override this to apply dynamic option adjustments.
    fn effective_widget_opt(&self) -> WidgetOption {
        *self.widget_opt()
    }
    /// Returns the effective scroll behavior used by generic dispatch.
    fn effective_scroll_behavior(&self) -> ScrollBehavior {
        self.scroll_behavior()
    }
    /// Returns the focus behavior used by generic dispatch.
    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::from_widget_options(self.effective_widget_opt())
    }
    /// Returns whether this widget needs per-frame input snapshots.
    fn needs_input_snapshot(&self) -> bool {
        false
    }
}

/// Retained interaction identity used by focus, hover, and frame results.
///
/// Normal retained traversal uses `Node` identities. `Root` is available for root-level results
/// and future framework controls that do not naturally belong to a widget-tree node.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RetainedId {
    /// Stable root-window, dialog, or popup identity.
    Root(RootId),
    /// Stable retained-tree node identity.
    Node(Id),
    /// Stable retained-tree node identity scoped to the owning root or scroll area.
    ScopedNode {
        /// Stable owner/root/scroll-area scope.
        scope: Id,
        /// Stable node ID within that scope.
        node: Id,
    },
}

impl RetainedId {
    /// Creates a retained root interaction ID.
    pub const fn root(root_id: RootId) -> Self {
        Self::Root(root_id)
    }

    /// Creates a retained node interaction ID.
    pub const fn node(node_id: Id) -> Self {
        Self::Node(node_id)
    }

    /// Creates a scoped retained node interaction ID.
    ///
    /// Root containers use a scope derived from their `RootId`; retained scroll areas use their
    /// node ID as the child-container scope.
    pub const fn scoped_node(scope: Id, node_id: Id) -> Self {
        Self::ScopedNode { scope, node: node_id }
    }

    /// Creates a retained node ID scoped to a registered root.
    pub fn root_node(root_id: RootId, node_id: Id) -> Self {
        Self::scoped_node(Id::new(root_id.raw() as u64), node_id)
    }
}

/// Per-frame widget interaction results keyed by retained identity.
///
/// Retained nodes are the primary storage. Widget handle identities are kept only internally to
/// catch duplicate `WidgetHandle` dispatch in a single frame.
///
/// The storage is split into two generations:
/// - the committed result set published at the end of the previous frame,
/// - and the current in-progress result set being written by this frame.
#[derive(Default)]
pub(crate) struct FrameResults {
    committed: FrameResultStore,
    current: FrameResultStore,
    current_dispatch: FrameDispatchTracker,
}

#[derive(Default)]
struct FrameResultStore {
    /// Primary public result storage keyed by fully scoped retained identity.
    entries: HashMap<RetainedId, ResourceState>,
    /// Compatibility index for node-id lookup APIs that do not include a root/scroll-area scope.
    node_index: HashMap<Id, RetainedId>,
}

impl FrameResultStore {
    fn clear(&mut self) {
        self.entries.clear();
        self.node_index.clear();
    }

    fn record_node_index(&mut self, node_id: Id, retained_id: RetainedId) {
        self.node_index.entry(node_id).or_insert(retained_id);
    }

    fn record_retained(&mut self, retained_id: RetainedId, state: ResourceState) {
        let prev_state = self.entries.insert(retained_id, state);
        debug_assert!(prev_state.is_none(), "retained result for {:?} was recorded more than once", retained_id);
    }

    fn generation(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.entries, &self.node_index)
    }
}

#[derive(Default)]
struct FrameDispatchTracker {
    /// Dispatch site for each retained ID seen in the current frame.
    retained_sites: HashMap<RetainedId, String>,
    /// Dispatch site for each widget handle seen in the current frame.
    widget_sites: HashMap<Id, String>,
}

impl FrameDispatchTracker {
    fn clear(&mut self) {
        self.retained_sites.clear();
        self.widget_sites.clear();
    }

    fn record_widget(&mut self, widget_handle_id: Id, dispatch_site: &str) {
        if let Some(first_site) = self.widget_sites.get(&widget_handle_id) {
            panic!(
                "duplicate widget dispatch detected for handle {:?}; a WidgetHandle may only be rendered once per frame. first dispatch: {}. duplicate dispatch: {}.",
                widget_handle_id, first_site, dispatch_site
            );
        }

        self.widget_sites.insert(widget_handle_id, dispatch_site.to_string());
    }

    fn record_retained(&mut self, retained_id: RetainedId, dispatch_site: String) {
        if let Some(first_site) = self.retained_sites.get(&retained_id) {
            panic!(
                "duplicate retained dispatch detected for {:?}. first dispatch: {}. duplicate dispatch: {}.",
                retained_id, first_site, dispatch_site
            );
        }

        self.retained_sites.insert(retained_id, dispatch_site);
    }
}

/// Read-only view over one frame-result generation.
#[derive(Copy, Clone)]
pub struct FrameResultGeneration<'a> {
    entries: &'a HashMap<RetainedId, ResourceState>,
    node_ids: &'a HashMap<Id, RetainedId>,
}

impl<'a> FrameResultGeneration<'a> {
    /// Creates a read-only view over a specific result generation.
    fn new(entries: &'a HashMap<RetainedId, ResourceState>, node_ids: &'a HashMap<Id, RetainedId>) -> Self {
        Self { entries, node_ids }
    }

    /// Returns the state for a retained interaction ID in this generation.
    pub fn state_of_retained(&self, retained_id: RetainedId) -> ResourceState {
        self.entries.get(&retained_id).copied().unwrap_or(ResourceState::NONE)
    }

    /// Returns the state for a retained tree node in this generation.
    pub fn state_of_node(&self, node_id: Id) -> ResourceState {
        self.node_ids
            .get(&node_id)
            .copied()
            .map(|retained_id| self.state_of_retained(retained_id))
            .unwrap_or_else(|| self.state_of_retained(RetainedId::node(node_id)))
    }
}

impl FrameResults {
    /// Clears the in-progress frame results for a new frame.
    ///
    /// Previously committed results remain available through [`FrameResults::committed`].
    pub(crate) fn begin_frame(&mut self) {
        self.current.clear();
        self.current_dispatch.clear();
    }

    /// Publishes the current frame as the next committed result generation.
    pub(crate) fn finish_frame(&mut self) {
        std::mem::swap(&mut self.committed, &mut self.current);
        self.current.clear();
        self.current_dispatch.clear();
    }

    /// Records a retained node result and checks that its widget handle is not dispatched twice.
    pub(crate) fn record_retained_with_context(
        &mut self,
        retained_id: RetainedId,
        node_id: Id,
        widget_handle_id: Id,
        state: ResourceState,
        dispatch_site: impl Into<String>,
    ) {
        let dispatch_site = dispatch_site.into();
        self.current_dispatch.record_widget(widget_handle_id, &dispatch_site);
        self.current.record_node_index(node_id, retained_id);
        self.record_retained_id_with_context(retained_id, state, dispatch_site);
    }

    /// Records an internal retained node result without a legacy widget identity.
    pub(crate) fn record_node_with_context(&mut self, retained_id: RetainedId, node_id: Id, state: ResourceState, dispatch_site: impl Into<String>) {
        self.current.record_node_index(node_id, retained_id);
        self.record_retained_id_with_context(retained_id, state, dispatch_site);
    }

    /// Records a retained id after duplicate-dispatch validation.
    fn record_retained_id_with_context(&mut self, retained_id: RetainedId, state: ResourceState, dispatch_site: impl Into<String>) {
        let dispatch_site = dispatch_site.into();
        self.current_dispatch.record_retained(retained_id, dispatch_site);
        self.current.record_retained(retained_id, state);
    }

    /// Returns the committed result generation published by the previous frame.
    pub(crate) fn committed(&self) -> FrameResultGeneration<'_> {
        self.committed.generation()
    }

    /// Returns the in-progress result generation for the current frame.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current(&self) -> FrameResultGeneration<'_> {
        self.current.generation()
    }
}

#[cfg(test)]
mod tests;

impl Widget for (WidgetOption, ScrollBehavior) {
    fn widget_opt(&self) -> &WidgetOption {
        &self.0
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.1
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        // Internal placeholder widgets reserve enough room for text or an expand icon.
        let padding = style.padding.max(0);
        let vertical_pad = max(1, padding / 2);
        let font_height = atlas.get_font_height(style.font) as i32;
        let icon_height = atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let content = max(font_height, icon_height);
        let height = (content + vertical_pad * 2).max(0);
        let width = (padding * 2 + content).max(0);
        Dimensioni::new(width, height)
    }

    fn update(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) {}
}
