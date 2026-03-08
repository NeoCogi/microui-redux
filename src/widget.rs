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
use crate::input::{ControlState, ResourceState, WidgetBehaviourOption, WidgetOption};
use crate::style::Style;
use crate::widget_ctx::WidgetCtx;
use crate::widget_tree::WidgetHandle;

/// Committed retained-state view passed into the widget reconcile phase.
///
/// This is the previous frame's published interaction result for the widget.
#[derive(Copy, Clone, Debug)]
pub struct CommittedWidgetState {
    /// Interaction result published at the end of the previous frame.
    pub previous_result: ResourceState,
}

impl CommittedWidgetState {
    /// Creates a committed retained-state view from the previous frame result.
    pub const fn new(previous_result: ResourceState) -> Self {
        Self { previous_result }
    }

    /// Returns `true` when the previous frame produced transient widget state
    /// that should be committed during reconcile.
    pub fn should_commit_pending(self) -> bool {
        !self.previous_result.is_none()
    }
}

impl Default for CommittedWidgetState {
    fn default() -> Self {
        Self::new(ResourceState::NONE)
    }
}

/// Trait implemented by persistent widget state structures.
///
/// Widgets participate in three retained phases:
/// 1. `reconcile`, which consumes previously committed frame state.
/// 2. `measure`, which reports intrinsic size for the current frame's layout pass.
/// 3. `render`, which records draw commands and produces the next frame result.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the behaviour options for this state.
    fn behaviour_opt(&self) -> &WidgetBehaviourOption;
    /// Applies previously committed frame state to the persistent widget state.
    ///
    /// This runs before measurement so retained state changes can influence layout.
    fn reconcile(&mut self, _committed: CommittedWidgetState) {}
    /// Returns the intrinsic widget size for the current frame's layout pass.
    ///
    /// Called after [`Widget::reconcile`] each frame so the layout manager can allocate a cell.
    /// `avail` reports the current container body size visible to the widget.
    /// Values less than or equal to zero are treated as "use layout defaults" for that axis.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    /// Renders the widget for the current frame and returns the next committed result.
    fn render(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
    /// Returns the effective widget options used by generic dispatch.
    ///
    /// Widgets can override this to apply dynamic option adjustments.
    fn effective_widget_opt(&self) -> WidgetOption {
        *self.widget_opt()
    }
    /// Returns the effective behavior options used by generic dispatch.
    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption {
        *self.behaviour_opt()
    }
    /// Returns whether this widget needs per-frame input snapshots.
    fn needs_input_snapshot(&self) -> bool {
        false
    }
}

/// Raw widget dispatch reference used by batch APIs.
pub type WidgetRef<'a> = &'a mut dyn Widget;

/// Creates a [`WidgetRef`] from a widget state reference.
pub fn widget_ref<'a, W: Widget>(widget: &'a mut W) -> WidgetRef<'a> {
    widget as &mut dyn Widget
}

/// Raw pointer identity used for widget hover/focus tracking.
pub type WidgetId = *const ();

/// Returns the pointer identity for a widget state object.
/// Use this when calling APIs such as `Container::set_focus`.
pub fn widget_id_of<W: Widget + ?Sized>(widget: &W) -> WidgetId {
    widget as *const W as *const ()
}

/// Returns the pointer identity for the widget state stored in `handle`.
pub fn widget_id_of_handle<W: Widget>(handle: &WidgetHandle<W>) -> WidgetId {
    let widget = handle.borrow();
    widget_id_of(&*widget)
}

/// Per-frame widget interaction results keyed by [`WidgetId`].
///
/// A single widget state is expected to be dispatched once per frame.
/// Duplicate dispatches with the same ID trigger a debug assertion.
///
/// The storage is split into two generations:
/// - the committed result set published at the end of the previous frame,
/// - and the current in-progress result set being written by this frame.
#[derive(Default)]
pub struct FrameResults {
    committed: HashMap<WidgetId, ResourceState>,
    current: HashMap<WidgetId, ResourceState>,
}

/// Read-only view over one frame-result generation.
#[derive(Copy, Clone)]
pub struct FrameResultGeneration<'a> {
    entries: &'a HashMap<WidgetId, ResourceState>,
}

impl<'a> FrameResultGeneration<'a> {
    fn new(entries: &'a HashMap<WidgetId, ResourceState>) -> Self {
        Self { entries }
    }

    /// Returns the state for `widget_id` in this generation.
    pub fn state(&self, widget_id: WidgetId) -> ResourceState {
        self.entries.get(&widget_id).copied().unwrap_or(ResourceState::NONE)
    }

    /// Returns the state for `widget` in this generation.
    pub fn state_of<W: Widget + ?Sized>(&self, widget: &W) -> ResourceState {
        self.state(widget_id_of(widget))
    }

    /// Returns the state for the widget stored in `handle` in this generation.
    pub fn state_of_handle<W: Widget>(&self, handle: &WidgetHandle<W>) -> ResourceState {
        self.state(widget_id_of_handle(handle))
    }
}

impl FrameResults {
    /// Clears the in-progress frame results for a new frame.
    ///
    /// Previously committed results remain available through [`FrameResults::committed`].
    pub fn begin_frame(&mut self) {
        self.current.clear();
    }

    /// Publishes the current frame as the next committed result generation.
    pub fn finish_frame(&mut self) {
        std::mem::swap(&mut self.committed, &mut self.current);
        self.current.clear();
    }

    /// Records the current frame state under `widget_id`.
    pub fn record(&mut self, widget_id: WidgetId, state: ResourceState) {
        let prev = self.current.insert(widget_id, state);
        debug_assert!(prev.is_none(), "Widget {:?} was dispatched more than once in the same frame", widget_id);
    }

    /// Returns the committed result generation published by the previous frame.
    pub fn committed(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.committed)
    }

    /// Returns the in-progress result generation for the current frame.
    pub fn current(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.current)
    }

    /// Returns the committed result for `widget_id` from the previous frame.
    pub fn committed_state(&self, widget_id: WidgetId) -> ResourceState {
        self.committed().state(widget_id)
    }

    /// Returns the in-progress result for `widget_id` in the current frame.
    pub fn current_state(&self, widget_id: WidgetId) -> ResourceState {
        self.current().state(widget_id)
    }

    /// Returns the most relevant available state for `widget_id`.
    ///
    /// Once any widget has been rendered in the current frame, this reports only the
    /// current-frame result set. Before the first current-frame record, it falls back
    /// to the previously committed result generation.
    ///
    /// This is a compatibility lookup for code that still expects a single
    /// result map. New code should prefer [`FrameResults::committed`] or
    /// [`FrameResults::current`] explicitly.
    pub fn state(&self, widget_id: WidgetId) -> ResourceState {
        if self.current.is_empty() {
            self.committed_state(widget_id)
        } else {
            self.current_state(widget_id)
        }
    }

    /// Returns the most relevant available state for `widget`.
    ///
    /// See [`FrameResults::state`] for the lookup behavior.
    pub fn state_of<W: Widget + ?Sized>(&self, widget: &W) -> ResourceState {
        self.state(widget_id_of(widget))
    }

    /// Returns the committed result for `widget` from the previous frame.
    pub fn committed_state_of<W: Widget + ?Sized>(&self, widget: &W) -> ResourceState {
        self.committed_state(widget_id_of(widget))
    }

    /// Returns the committed result for the widget stored in `handle`.
    pub fn committed_state_of_handle<W: Widget>(&self, handle: &WidgetHandle<W>) -> ResourceState {
        self.committed_state(widget_id_of_handle(handle))
    }

    /// Returns the most relevant available state for the widget stored in `handle`.
    pub fn state_of_handle<W: Widget>(&self, handle: &WidgetHandle<W>) -> ResourceState {
        self.state(widget_id_of_handle(handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_does_not_fall_back_to_committed_once_current_frame_is_active() {
        let committed_widget = 1_u8;
        let current_widget = 2_u8;
        let committed_id = (&committed_widget as *const u8).cast::<()>();
        let current_id = (&current_widget as *const u8).cast::<()>();

        let mut results = FrameResults::default();
        results.record(committed_id, ResourceState::SUBMIT);
        results.finish_frame();

        assert!(results.state(committed_id).is_submitted());

        results.begin_frame();
        results.record(current_id, ResourceState::CHANGE);

        assert!(results.state(committed_id).is_none());
        assert!(results.state(current_id).is_changed());
        assert!(results.committed_state(committed_id).is_submitted());
    }

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

impl Widget for (WidgetOption, WidgetBehaviourOption) {
    fn widget_opt(&self) -> &WidgetOption {
        &self.0
    }

    fn behaviour_opt(&self) -> &WidgetBehaviourOption {
        &self.1
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

    fn render(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }
}
