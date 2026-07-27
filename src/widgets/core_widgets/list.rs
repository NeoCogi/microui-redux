//! List item and list-box widgets.
//!
//! `ListItem` represents one selectable row and `ListBox` stores shared selection state for a
//! retained list.

use super::*;

#[derive(Clone)]
/// Persistent state for list items.
pub struct ListItem {
    /// Label displayed for the list item.
    pub label: String,
    /// Optional atlas icon rendered alongside the label.
    pub icon: Option<IconId>,
    /// Shared widget configuration.
    pub config: WidgetConfig,
}

impl ListItem {
    /// Creates a list item with default widget options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            config: WidgetConfig::default(),
        }
    }

    /// Creates a list item with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: None,
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
        }
    }

    /// Creates a list item with an icon and default widget options.
    pub fn with_icon(label: impl Into<String>, icon: IconId) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            config: WidgetConfig::default(),
        }
    }

    /// Creates a list item with an icon and explicit widget options.
    pub fn with_icon_opt(label: impl Into<String>, icon: IconId, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
        }
    }

    /// Measures the row label and optional icon.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let mut width = padding * 2;
        let mut visual_h = 0;
        if let Some(icon) = self.icon {
            let size = atlas.get_icon_size(icon);
            width += size.width + padding;
            visual_h = size.height;
        }
        if !self.label.is_empty() {
            width += text_size(style, atlas, self.config.font, &self.label).width;
        }
        let height = content_height(style, atlas, self.config.font, visual_h);
        Dimensioni::new(width.max(0), height)
    }

    /// List items submit on click and otherwise keep no local transient state.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: &[UiInputEvent]) -> ResourceState {
        submit_on_click(ctx)
    }

    /// Paints row highlight, optional icon, and label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.screen_content_rect();

        if ctx.focused() || ctx.hovered() {
            let mut color = ControlColor::Button;
            if ctx.focused() {
                color.focus();
            } else {
                color.hover();
            }
            let fill = ctx.style().colors[color as usize];
            ctx.draw_rect(bounds, fill);
        }

        let mut text_rect = bounds;
        if let Some(icon) = self.icon {
            // Icons consume the left padding + icon width before the text region starts.
            let padding = ctx.style().padding.max(0);
            let icon_size = ctx.atlas().get_icon_size(icon);
            let icon_x = bounds.x + padding;
            let icon_y = bounds.y + ((bounds.height - icon_size.height) / 2).max(0);
            let icon_rect = rect(icon_x, icon_y, icon_size.width, icon_size.height);
            let consumed = icon_size.width + padding * 2;
            text_rect.x += consumed;
            text_rect.width = (text_rect.width - consumed).max(0);
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.draw_icon(icon, icon_rect, color);
        }

        if !self.label.is_empty() {
            let font = ctx.style().resolve_font_choice(self.config.font);
            ctx.draw_control_text_with_font(font, &self.label, text_rect, ControlColor::Text, self.config.opt);
        }
    }
}

implement_widget!(ListItem, update_widget, paint_widget, preferred_size_widget);

#[derive(Clone)]
/// Persistent state for list boxes.
pub struct ListBox {
    /// Label displayed for the list box.
    pub label: String,
    /// Optional image rendered alongside the label.
    pub image: Option<TextureId>,
    /// Shared widget configuration.
    pub config: WidgetConfig,
}

impl ListBox {
    /// Creates a list box with default widget options.
    pub fn new(label: impl Into<String>, image: Option<TextureId>) -> Self {
        Self {
            label: label.into(),
            image,
            config: WidgetConfig::default(),
        }
    }

    /// Creates a list box with explicit widget options.
    pub fn with_opt(label: impl Into<String>, image: Option<TextureId>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            image,
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
        }
    }

    /// Measures list-box inline label and optional image.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let visual = self.image.map(TextureId::size);
        inline_content_size(style, atlas, self.config.font, &self.label, visual)
    }

    /// List boxes submit on click and otherwise keep no local transient state.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: &[UiInputEvent]) -> ResourceState {
        submit_on_click(ctx)
    }

    /// Paints list-box frame, label, and optional image.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let rect = ctx.screen_content_rect();
        if let Some(colorid) = widget_fill_color(ctx, ControlColor::Button, WidgetFillOption::HOVER | WidgetFillOption::CLICK) {
            ctx.draw_rect(rect, ctx.style().colors[colorid as usize]);
        }
        let visual_size = self.image.map(TextureId::size);
        let layout = layout_inline_content(rect, ctx.style(), &self.label, visual_size);
        if !self.label.is_empty() {
            let font = ctx.style().resolve_font_choice(self.config.font);
            ctx.draw_control_text_with_font(font, &self.label, layout.text, ControlColor::Text, self.config.opt);
        }
        if let (Some(image), Some(visual)) = (self.image, layout.visual) {
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.push_image(image, visual, color);
        }
    }
}

implement_widget!(ListBox, update_widget, paint_widget, preferred_size_widget);
