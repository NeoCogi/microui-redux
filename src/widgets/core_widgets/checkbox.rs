//! Checkbox widget state and rendering.
//!
//! The checkbox toggles persistent boolean state on click and paints the atlas check icon when
//! selected.

use super::*;

#[derive(Clone)]
/// Persistent state for checkbox widgets.
pub struct Checkbox {
    /// Label displayed for the checkbox.
    pub label: String,
    /// Current value of the checkbox.
    pub value: bool,
    /// Shared widget configuration.
    pub config: WidgetConfig,
}

impl Checkbox {
    /// Creates a checkbox with default widget options.
    pub fn new(label: impl Into<String>, value: bool) -> Self {
        Self {
            label: label.into(),
            value,
            config: WidgetConfig::default(),
        }
    }

    /// Creates a checkbox with explicit widget options.
    pub fn with_opt(label: impl Into<String>, value: bool, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            value,
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
        }
    }

    /// Measures checkbox square plus optional label.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let check_icon = atlas.get_icon_size(CHECK_ICON);
        let height = content_height(style, atlas, self.config.font, check_icon.height);
        let mut width = padding * 2 + height;
        if !self.label.is_empty() {
            width += text_size(style, atlas, self.config.font, &self.label).width + padding;
        }
        Dimensioni::new(width.max(0), height)
    }

    /// Toggles the persistent value on click.
    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        if control.clicked {
            // The checkbox itself owns the boolean state, so change reporting is local.
            self.value = !self.value;
            res |= ResourceState::CHANGE;
        }
        res
    }

    /// Paints the checkbox square, check mark, and label.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let bounds = ctx.screen_rect();
        let box_rect = rect(bounds.x, bounds.y, bounds.height, bounds.height);
        ctx.draw_widget_frame(control, box_rect, ControlColor::Base, self.config.opt);
        if self.value {
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.draw_icon(CHECK_ICON, box_rect, color);
        }
        let text_rect = rect(bounds.x + box_rect.width, bounds.y, bounds.width - box_rect.width, bounds.height);
        if !self.label.is_empty() {
            let font = ctx.style().resolve_font_choice(self.config.font);
            ctx.draw_control_text_with_font(font, &self.label, text_rect, ControlColor::Text, self.config.opt);
        }
    }
}

impl Widget for Checkbox {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.config.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        self.update_widget(ctx, control)
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        self.paint_widget(ctx, control);
    }
}
