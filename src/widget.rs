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
/// `handle` is invoked with a `WidgetCtx` and precomputed `ControlState`.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the behaviour options for this state.
    fn behaviour_opt(&self) -> &WidgetBehaviourOption;
    /// Returns the preferred widget size for automatic layout resolution.
    ///
    /// Called before [`Widget::handle`] each frame so the layout manager can allocate a cell.
    /// `avail` reports the current container body size visible to the widget.
    /// Values less than or equal to zero are treated as "use layout defaults" for that axis.
    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    /// Handles widget interaction and rendering for the current frame using the provided context.
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
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
#[derive(Default)]
pub struct FrameResults {
    states: HashMap<WidgetId, ResourceState>,
}

impl FrameResults {
    /// Clears all recorded widget states for a new frame.
    ///
    /// This preserves internal capacity to avoid repeated reallocations.
    pub fn begin_frame(&mut self) {
        self.states.clear();
    }

    /// Records a widget state under `widget_id`.
    pub fn record(&mut self, widget_id: WidgetId, state: ResourceState) {
        let prev = self.states.insert(widget_id, state);
        debug_assert!(prev.is_none(), "Widget {:?} was dispatched more than once in the same frame", widget_id);
    }

    /// Returns the recorded state for `widget_id` in the current frame.
    ///
    /// Returns [`ResourceState::NONE`] when no state is recorded.
    pub fn state(&self, widget_id: WidgetId) -> ResourceState {
        self.states.get(&widget_id).copied().unwrap_or(ResourceState::NONE)
    }

    /// Returns the recorded state for `widget` in the current frame.
    ///
    /// Returns [`ResourceState::NONE`] when no state is recorded.
    pub fn state_of<W: Widget + ?Sized>(&self, widget: &W) -> ResourceState {
        self.state(widget_id_of(widget))
    }

    /// Returns the recorded state for the widget stored in `handle`.
    pub fn state_of_handle<W: Widget>(&self, handle: &WidgetHandle<W>) -> ResourceState {
        self.state(widget_id_of_handle(handle))
    }
}

impl Widget for (WidgetOption, WidgetBehaviourOption) {
    fn widget_opt(&self) -> &WidgetOption {
        &self.0
    }

    fn behaviour_opt(&self) -> &WidgetBehaviourOption {
        &self.1
    }

    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let vertical_pad = max(1, padding / 2);
        let font_height = atlas.get_font_height(style.font) as i32;
        let icon_height = atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let content = max(font_height, icon_height);
        let height = (content + vertical_pad * 2).max(0);
        let width = (padding * 2 + content).max(0);
        Dimensioni::new(width, height)
    }

    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }
}
