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
use crate::widget_tree::WidgetHandle;

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

    pub(crate) fn releases_on_mouse_up(self) -> bool {
        matches!(self, Self::Momentary | Self::DragCapture)
    }
}

/// Trait implemented by persistent widget state structures.
///
/// Widgets participate in two retained phases:
/// 1. `measure`, which reports intrinsic size for the current frame's layout pass.
/// 2. `run`, which records draw commands, samples interaction, mutates widget-local state,
///    and produces the current frame result.
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
    /// Runs the widget for the current frame and returns the current frame result.
    fn run(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
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

/// Raw pointer identity used for widget hover/focus tracking.
pub type WidgetId = *const ();

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
    /// Stable retained-tree node identity scoped to the owning root or panel.
    ScopedNode {
        /// Stable owner/root/panel scope.
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
    /// Root containers use a scope derived from their `RootId`; retained panels use their panel
    /// node ID as the child-container scope.
    pub const fn scoped_node(scope: Id, node_id: Id) -> Self {
        Self::ScopedNode { scope, node: node_id }
    }

    /// Creates a retained node ID scoped to a registered root.
    pub fn root_node(root_id: RootId, node_id: Id) -> Self {
        Self::scoped_node(Id::new(root_id.raw() as u64), node_id)
    }

    pub(crate) fn compat_widget(widget_id: WidgetId) -> Self {
        Self::Node(compat_widget_node_id(widget_id))
    }

}

fn compat_widget_node_id(widget_id: WidgetId) -> Id {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    fn write(mut hash: u64, value: u64) -> u64 {
        for byte in value.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    let hash = write(FNV_OFFSET_BASIS, 0x6d69_6372_6f75_695f_u64);
    let hash = write(hash, 0x6c65_6761_6379_5f77_u64);
    Id::new(write(hash, widget_id as usize as u64))
}

/// Returns the pointer identity for a widget state object.
/// Use this when calling focus APIs such as `WindowHandle::set_focus`.
pub fn widget_id_of<W: Widget + ?Sized>(widget: &W) -> WidgetId {
    widget as *const W as *const ()
}

/// Returns the pointer identity for the widget state stored in `handle`.
pub fn widget_id_of_handle<W: Widget>(handle: &WidgetHandle<W>) -> WidgetId {
    let widget = handle.borrow();
    widget_id_of(&*widget)
}

/// Per-frame widget interaction results keyed by retained identity.
///
/// Retained nodes are the primary storage. Legacy widget-pointer lookups remain as a compatibility
/// layer by recording which retained node dispatched each widget handle during traversal.
///
/// The storage is split into two generations:
/// - the committed result set published at the end of the previous frame,
/// - and the current in-progress result set being written by this frame.
#[derive(Default)]
pub(crate) struct FrameResults {
    committed: HashMap<RetainedId, ResourceState>,
    current: HashMap<RetainedId, ResourceState>,
    current_dispatch_sites: HashMap<RetainedId, String>,
    current_widget_dispatch_sites: HashMap<WidgetId, String>,
    committed_widget_nodes: HashMap<WidgetId, RetainedId>,
    current_widget_nodes: HashMap<WidgetId, RetainedId>,
    committed_node_ids: HashMap<Id, RetainedId>,
    current_node_ids: HashMap<Id, RetainedId>,
}

/// Read-only view over one frame-result generation.
#[derive(Copy, Clone)]
pub struct FrameResultGeneration<'a> {
    entries: &'a HashMap<RetainedId, ResourceState>,
    widget_nodes: &'a HashMap<WidgetId, RetainedId>,
    node_ids: &'a HashMap<Id, RetainedId>,
}

impl<'a> FrameResultGeneration<'a> {
    fn new(
        entries: &'a HashMap<RetainedId, ResourceState>,
        widget_nodes: &'a HashMap<WidgetId, RetainedId>,
        node_ids: &'a HashMap<Id, RetainedId>,
    ) -> Self {
        Self {
            entries,
            widget_nodes,
            node_ids,
        }
    }

    /// Returns the state for a retained interaction ID in this generation.
    pub fn state_of_retained(&self, retained_id: RetainedId) -> ResourceState {
        self.entries.get(&retained_id).copied().unwrap_or(ResourceState::NONE)
    }

    /// Returns the state for `widget_id` in this generation.
    ///
    /// Deprecated: prefer [`FrameResultGeneration::state_of_node`] or
    /// [`FrameResultGeneration::state_of_retained`]. This compatibility helper maps the widget
    /// pointer to the retained node that dispatched it in this generation when possible.
    #[deprecated(note = "use state_of_node or state_of_retained; widget pointer result lookup is a compatibility path")]
    pub fn state(&self, widget_id: WidgetId) -> ResourceState {
        self.widget_nodes
            .get(&widget_id)
            .copied()
            .map(|retained_id| self.state_of_retained(retained_id))
            .unwrap_or_else(|| self.state_of_retained(RetainedId::compat_widget(widget_id)))
    }

    /// Returns the state for `widget` in this generation.
    ///
    /// Deprecated: prefer [`FrameResultGeneration::state_of_node`] or
    /// [`FrameResultGeneration::state_of_retained`].
    #[deprecated(note = "use state_of_node or state_of_retained; widget pointer result lookup is a compatibility path")]
    pub fn state_of<W: Widget + ?Sized>(&self, widget: &W) -> ResourceState {
        #[allow(deprecated)]
        self.state(widget_id_of(widget))
    }

    /// Returns the state for the widget stored in `handle` in this generation.
    ///
    /// This compatibility helper maps the handle to the retained node that dispatched it in this
    /// generation. Prefer [`FrameResultGeneration::state_of_node`] when the caller already has a
    /// retained node ID.
    pub fn state_of_handle<W: Widget>(&self, handle: &WidgetHandle<W>) -> ResourceState {
        #[allow(deprecated)]
        self.state(widget_id_of_handle(handle))
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
        self.current_dispatch_sites.clear();
        self.current_widget_dispatch_sites.clear();
        self.current_widget_nodes.clear();
        self.current_node_ids.clear();
    }

    /// Publishes the current frame as the next committed result generation.
    pub(crate) fn finish_frame(&mut self) {
        std::mem::swap(&mut self.committed, &mut self.current);
        std::mem::swap(&mut self.committed_widget_nodes, &mut self.current_widget_nodes);
        std::mem::swap(&mut self.committed_node_ids, &mut self.current_node_ids);
        self.current.clear();
        self.current_dispatch_sites.clear();
        self.current_widget_dispatch_sites.clear();
        self.current_widget_nodes.clear();
        self.current_node_ids.clear();
    }

    /// Records the current frame state under `widget_id`.
    ///
    /// Deprecated: manual/debug compatibility path. Retained traversal records by retained ID.
    #[deprecated(note = "record retained results with record_retained_with_context or record_node_with_context")]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn record(&mut self, widget_id: WidgetId, state: ResourceState) {
        #[allow(deprecated)]
        self.record_with_context(widget_id, state, "unknown widget dispatch site");
    }

    /// Records the current frame state under `widget_id` with a human-readable dispatch site.
    ///
    /// Deprecated: manual/debug compatibility path. Retained traversal records by retained ID.
    #[deprecated(note = "record retained results with record_retained_with_context or record_node_with_context")]
    pub(crate) fn record_with_context(&mut self, widget_id: WidgetId, state: ResourceState, dispatch_site: impl Into<String>) {
        let dispatch_site = dispatch_site.into();
        if let Some(first_site) = self.current_widget_dispatch_sites.get(&widget_id) {
            panic!(
                "duplicate widget dispatch detected for widget {:p}; a WidgetHandle may only be rendered once per frame. first dispatch: {}. duplicate dispatch: {}.",
                widget_id, first_site, dispatch_site
            );
        }

        let retained_id = RetainedId::compat_widget(widget_id);
        self.current_widget_nodes.insert(widget_id, retained_id);
        let prev_site = self.current_widget_dispatch_sites.insert(widget_id, dispatch_site.clone());
        let prev_state = self.current.insert(retained_id, state);
        let prev_retained_site = self.current_dispatch_sites.insert(retained_id, dispatch_site);
        debug_assert_eq!(
            prev_state.is_some(),
            prev_retained_site.is_some(),
            "retained result and dispatch-site tracking diverged for {:?}",
            retained_id
        );
        debug_assert!(prev_site.is_none(), "widget dispatch-site tracking diverged for widget {:p}", widget_id);
    }

    /// Records a retained node result under both stable node identity and legacy widget identity.
    pub(crate) fn record_retained_with_context(
        &mut self,
        retained_id: RetainedId,
        node_id: Id,
        widget_id: WidgetId,
        state: ResourceState,
        dispatch_site: impl Into<String>,
    ) {
        let dispatch_site = dispatch_site.into();
        if let Some(first_site) = self.current_widget_dispatch_sites.get(&widget_id) {
            panic!(
                "duplicate widget dispatch detected for widget {:p}; a WidgetHandle may only be rendered once per frame. first dispatch: {}. duplicate dispatch: {}.",
                widget_id, first_site, dispatch_site
            );
        }

        self.current_widget_dispatch_sites.insert(widget_id, dispatch_site.clone());
        self.current_widget_nodes.insert(widget_id, retained_id);
        self.current_node_ids.entry(node_id).or_insert(retained_id);
        self.record_retained_id_with_context(retained_id, state, dispatch_site);
    }

    /// Records an internal retained node result without a legacy widget identity.
    pub(crate) fn record_node_with_context(&mut self, retained_id: RetainedId, node_id: Id, state: ResourceState, dispatch_site: impl Into<String>) {
        self.current_node_ids.entry(node_id).or_insert(retained_id);
        self.record_retained_id_with_context(retained_id, state, dispatch_site);
    }

    fn record_retained_id_with_context(&mut self, retained_id: RetainedId, state: ResourceState, dispatch_site: impl Into<String>) {
        let dispatch_site = dispatch_site.into();
        if let Some(first_site) = self.current_dispatch_sites.get(&retained_id) {
            panic!(
                "duplicate retained dispatch detected for {:?}. first dispatch: {}. duplicate dispatch: {}.",
                retained_id, first_site, dispatch_site
            );
        }

        let prev_state = self.current.insert(retained_id, state);
        let prev_site = self.current_dispatch_sites.insert(retained_id, dispatch_site);
        debug_assert_eq!(
            prev_state.is_some(),
            prev_site.is_some(),
            "retained result and dispatch-site tracking diverged for {:?}",
            retained_id
        );
    }

    /// Returns the committed result generation published by the previous frame.
    pub(crate) fn committed(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.committed, &self.committed_widget_nodes, &self.committed_node_ids)
    }

    /// Returns the in-progress result generation for the current frame.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.current, &self.current_widget_nodes, &self.current_node_ids)
    }
}

#[cfg(test)]
mod tests {
    #![allow(deprecated)]

    use super::*;

    #[test]
    fn committed_and_current_generation_views_are_explicit() {
        let committed_widget = 1_u8;
        let current_widget = 2_u8;
        let committed_id = (&committed_widget as *const u8).cast::<()>();
        let current_id = (&current_widget as *const u8).cast::<()>();

        let mut results = FrameResults::default();
        results.record(committed_id, ResourceState::SUBMIT);
        results.finish_frame();
        results.begin_frame();
        results.record(current_id, ResourceState::CHANGE);

        assert!(results.committed().state(committed_id).is_submitted());
        assert!(results.current().state(committed_id).is_none());
        assert!(results.current().state(current_id).is_changed());
    }
}

impl Widget for (WidgetOption, ScrollBehavior) {
    fn widget_opt(&self) -> &WidgetOption {
        &self.0
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.1
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let vertical_pad = max(1, padding / 2);
        let font_height = atlas.get_font_height(style.font) as i32;
        let icon_height = atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let content = max(font_height, icon_height);
        let height = (content + vertical_pad * 2).max(0);
        let width = (padding * 2 + content).max(0);
        Dimensioni::new(width, height)
    }

    fn run(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }
}
