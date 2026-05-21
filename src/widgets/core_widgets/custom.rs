//! Custom widget state for user-provided retained drawing.
//!
//! Custom widgets reserve a normal layout cell and provide interaction payloads to callback-based
//! rendering commands.

use super::*;

#[derive(Clone)]
/// Persistent state for custom render widgets.
pub struct Custom {
    /// Label used for debugging or inspection.
    pub name: String,
    /// Widget options applied to the custom widget.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the custom widget.
    pub scroll_behavior: ScrollBehavior,
}

impl Custom {
    /// Creates a custom widget state with default options.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

    /// Creates a custom widget state with explicit options.
    pub fn with_opt(name: impl Into<String>, opt: WidgetOption, scroll_behavior: ScrollBehavior) -> Self {
        Self { name: name.into(), opt, scroll_behavior }
    }

    /// Measures the custom widget's debug label as its default preferred size.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let text_w = if self.name.is_empty() {
            0
        } else {
            text_size(style, atlas, FontChoice::default(), self.name.as_str()).width
        };
        let width = padding * 2 + text_w;
        let height = content_height(style, atlas, FontChoice::default(), 0);
        Dimensioni::new(width.max(0), height)
    }

    /// Custom render state itself does not update; the retained command callback owns drawing.
    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }

    /// Custom widgets do not paint through the normal widget state path.
    fn paint_widget(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) {}
}

implement_widget!(Custom, update_widget, paint_widget, preferred_size_widget);
