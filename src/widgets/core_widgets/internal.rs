//! Framework-owned internal control widget.
//!
//! Containers use `Internal` for chrome and scrollbar hit-testing without exposing those controls
//! as public application widgets.

use super::*;

#[derive(Clone)]
/// Persistent state for internal window/container controls.
pub struct Internal {
    /// Stable tag describing the internal control.
    pub tag: &'static str,
    /// Shared widget configuration.
    pub config: WidgetConfig,
}

impl Internal {
    /// Creates an internal control state with a stable tag.
    pub fn new(tag: &'static str) -> Self {
        Self { tag, config: WidgetConfig::default() }
    }

    /// Measures the internal tag for debug-visible chrome controls.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let text_w = if self.tag.is_empty() {
            0
        } else {
            text_size(style, atlas, FontChoice::default(), self.tag).width
        };
        let width = padding * 2 + text_w;
        let height = content_height(style, atlas, FontChoice::default(), 0);
        Dimensioni::new(width.max(0), height)
    }

    /// Internal controls do not submit by themselves; callers interpret their control state.
    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }

    /// Internal controls paint through their caller-specific chrome path.
    fn paint_widget(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) {}
}

implement_widget!(Internal, update_widget, paint_widget, preferred_size_widget);
