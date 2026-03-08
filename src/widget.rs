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

/// Trait implemented by persistent widget state structures.
///
/// Widgets participate in two retained phases:
/// 1. `measure`, which reports intrinsic size for the current frame's layout pass.
/// 2. `run`, which records draw commands, samples interaction, mutates widget-local state,
///    and produces the current frame result.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the behaviour options for this state.
    fn behaviour_opt(&self) -> &WidgetBehaviourOption;
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
    /// Returns the effective behavior options used by generic dispatch.
    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption {
        *self.behaviour_opt()
    }
    /// Returns whether this widget needs per-frame input snapshots.
    fn needs_input_snapshot(&self) -> bool {
        false
    }
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
/// Duplicate dispatches with the same ID panic in all builds.
///
/// The storage is split into two generations:
/// - the committed result set published at the end of the previous frame,
/// - and the current in-progress result set being written by this frame.
#[derive(Default)]
pub(crate) struct FrameResults {
    committed: HashMap<WidgetId, ResourceState>,
    current: HashMap<WidgetId, ResourceState>,
    current_dispatch_sites: HashMap<WidgetId, String>,
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
    pub(crate) fn begin_frame(&mut self) {
        self.current.clear();
        self.current_dispatch_sites.clear();
    }

    /// Publishes the current frame as the next committed result generation.
    pub(crate) fn finish_frame(&mut self) {
        std::mem::swap(&mut self.committed, &mut self.current);
        self.current.clear();
        self.current_dispatch_sites.clear();
    }

    /// Records the current frame state under `widget_id`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn record(&mut self, widget_id: WidgetId, state: ResourceState) {
        self.record_with_context(widget_id, state, "unknown widget dispatch site");
    }

    /// Records the current frame state under `widget_id` with a human-readable dispatch site.
    pub(crate) fn record_with_context(&mut self, widget_id: WidgetId, state: ResourceState, dispatch_site: impl Into<String>) {
        let dispatch_site = dispatch_site.into();
        if let Some(first_site) = self.current_dispatch_sites.get(&widget_id) {
            panic!(
                "duplicate widget dispatch detected for widget {:p}; a WidgetHandle may only be rendered once per frame. first dispatch: {}. duplicate dispatch: {}.",
                widget_id, first_site, dispatch_site
            );
        }

        let prev_state = self.current.insert(widget_id, state);
        let prev_site = self.current_dispatch_sites.insert(widget_id, dispatch_site);
        debug_assert_eq!(
            prev_state.is_some(),
            prev_site.is_some(),
            "widget result and dispatch-site tracking diverged for widget {:p}",
            widget_id
        );
    }

    /// Returns the committed result generation published by the previous frame.
    pub(crate) fn committed(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.committed)
    }

    /// Returns the in-progress result generation for the current frame.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.current)
    }
}

#[cfg(test)]
mod tests {
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

    fn run(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }
}
