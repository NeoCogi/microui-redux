use super::*;

#[derive(Clone)]
/// Persistent state for list items.
pub struct ListItem {
    /// Label displayed for the list item.
    pub label: String,
    /// Optional atlas icon rendered alongside the label.
    pub icon: Option<IconId>,
    /// Font selection used for the list item label.
    pub font: FontChoice,
    /// Widget options applied to the list item.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the list item.
    pub scroll_behavior: ScrollBehavior,
}

impl ListItem {
    /// Creates a list item with default widget options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            font: FontChoice::default(),
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

    /// Creates a list item with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: None,
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

    /// Creates a list item with an icon and default widget options.
    pub fn with_icon(label: impl Into<String>, icon: IconId) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            font: FontChoice::default(),
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

    /// Creates a list item with an icon and explicit widget options.
    pub fn with_icon_opt(label: impl Into<String>, icon: IconId, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

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
            width += text_size(style, atlas, self.font, &self.label).width;
        }
        let height = content_height(style, atlas, self.font, visual_h);
        Dimensioni::new(width.max(0), height)
    }

    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        submit_on_click(control)
    }

    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let bounds = ctx.rect();

        if control.focused || control.hovered {
            let mut color = ControlColor::Button;
            if control.focused {
                color.focus();
            } else {
                color.hover();
            }
            let fill = ctx.style().colors[color as usize];
            ctx.draw_rect(bounds, fill);
        }

        let mut text_rect = bounds;
        if let Some(icon) = self.icon {
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
            let font = ctx.style().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, &self.label, text_rect, ControlColor::Text, self.opt);
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
    pub image: Option<Image>,
    /// Font selection used for the list box label.
    pub font: FontChoice,
    /// Widget options applied to the list box.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the list box.
    pub scroll_behavior: ScrollBehavior,
}

impl ListBox {
    /// Creates a list box with default widget options.
    pub fn new(label: impl Into<String>, image: Option<Image>) -> Self {
        Self {
            label: label.into(),
            image,
            font: FontChoice::default(),
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

    /// Creates a list box with explicit widget options.
    pub fn with_opt(label: impl Into<String>, image: Option<Image>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            image,
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::NONE,
        }
    }

    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let visual = self.image.map(|image| image.size(atlas));
        inline_content_size(style, atlas, self.font, &self.label, visual)
    }

    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        submit_on_click(control)
    }

    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let rect = ctx.rect();
        if !self.opt.has_no_frame() {
            if let Some(colorid) = widget_fill_color(control, ControlColor::Button, WidgetFillOption::HOVER | WidgetFillOption::CLICK) {
                ctx.draw_frame(rect, colorid);
            }
        }
        let visual_size = self.image.map(|image| image.size(ctx.atlas()));
        let layout = layout_inline_content(rect, ctx.style(), &self.label, visual_size);
        if !self.label.is_empty() {
            let font = ctx.style().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, &self.label, layout.text, ControlColor::Text, self.opt);
        }
        if let (Some(image), Some(visual)) = (self.image, layout.visual) {
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.push_image(image, visual, color);
        }
    }
}

implement_widget!(ListBox, update_widget, paint_widget, preferred_size_widget);
