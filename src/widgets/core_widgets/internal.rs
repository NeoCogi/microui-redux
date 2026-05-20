use super::*;

#[derive(Clone)]
/// Persistent state for internal window/container controls.
pub struct Internal {
    /// Stable tag describing the internal control.
    pub tag: &'static str,
    /// Widget options applied to the internal control.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the internal control.
    pub scroll_behavior: ScrollBehavior,
}

impl Internal {
    /// Creates an internal control state with a stable tag.
    pub fn new(tag: &'static str) -> Self {
        Self {
            tag,
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

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

    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState {
        ResourceState::NONE
    }

    fn paint_widget(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) {}
}

implement_widget!(Internal, update_widget, paint_widget, preferred_size_widget);
