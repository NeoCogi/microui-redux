//! Button widget state and rendering.
//!
//! Buttons support text, semantic atlas icons, and external textures through one shared layout
//! path.

use super::*;

#[derive(Clone)]
/// Describes the content rendered inside a button widget.
pub enum ButtonContent {
    /// A text label and optional icon from the atlas.
    Text {
        /// Text displayed on the button.
        label: String,
        /// Optional icon rendered on the button.
        icon: Option<IconId>,
    },
    /// A text label and optional image.
    Image {
        /// Text displayed on the button.
        label: String,
        /// Optional image rendered on the button.
        image: Option<TextureId>,
    },
    /// An optional image scaled to the allocated button width while preserving aspect ratio.
    ScaledImage {
        /// Text displayed on the button.
        label: String,
        /// Optional image rendered on the button.
        image: Option<TextureId>,
    },
}

#[derive(Clone)]
/// Persistent state for button widgets.
pub struct Button {
    /// Content rendered inside the button.
    pub content: ButtonContent,
    /// Shared widget configuration.
    pub config: WidgetConfig,
    /// Fill behavior for the button background.
    pub fill: WidgetFillOption,
}

impl Button {
    /// Creates a text button with default options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            config: WidgetConfig::new(WidgetOption::FRAME, ScrollBehavior::NONE),
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates a text button with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates a button with a semantic icon baked into the atlas.
    pub fn with_icon(label: impl Into<String>, icon: IconId, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: Some(icon) },
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
            fill,
        }
    }

    /// Creates an image button with explicit widget options and fill behavior.
    pub fn with_image(label: impl Into<String>, image: Option<TextureId>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Image { label: label.into(), image },
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
            fill,
        }
    }

    /// Creates an image button that scales its image to the allocated width and preserves aspect ratio.
    pub fn with_scaled_image(label: impl Into<String>, image: Option<TextureId>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::ScaledImage { label: label.into(), image },
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
            fill,
        }
    }

    /// Measures the label and optional visual content.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        match &self.content {
            ButtonContent::Text { label, icon } => {
                let visual = icon.map(|icon| atlas.get_icon_size(icon));
                inline_content_size(style, atlas, self.config.font, label, visual)
            }
            ButtonContent::Image { label, image } => {
                let visual = image.map(TextureId::size);
                inline_content_size(style, atlas, self.config.font, label, visual)
            }
            ButtonContent::ScaledImage { label, image } => {
                let visual = image.map(TextureId::size);
                if visual.is_some() && _avail.width > 0 {
                    scaled_visual_content_size(_avail, visual)
                } else {
                    inline_content_size(style, atlas, self.config.font, label, visual)
                }
            }
        }
    }

    /// Buttons submit on click and do not keep extra transient state.
    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>, _input: &[UiInputEvent]) -> ResourceState {
        submit_on_click(ctx)
    }

    /// Paints the button frame, text, and optional visual payload.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>) {
        let rect = ctx.screen_content_rect();
        if let Some(colorid) = widget_fill_color(ctx, ControlColor::Button, self.fill) {
            ctx.draw_rect(rect, ctx.style().colors[colorid as usize]);
        }
        let font = ctx.style().resolve_font_choice(self.config.font);
        match &self.content {
            ButtonContent::Text { label, icon } => {
                // Text/icon buttons use atlas icon metrics when laying out the inline visual.
                let visual_size = icon.map(|icon| ctx.atlas().get_icon_size(icon));
                let layout = layout_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.config.opt);
                }
                if let (Some(icon), Some(visual)) = (icon, layout.visual) {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.draw_icon(*icon, visual, color);
                }
            }
            ButtonContent::Image { label, image } => {
                let visual_size = image.map(TextureId::size);
                let layout = layout_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.config.opt);
                }
                if let (Some(image), Some(visual)) = (*image, layout.visual) {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.push_image(image, visual, color);
                }
            }
            ButtonContent::ScaledImage { label, image } => {
                let visual_size = image.map(TextureId::size);
                let layout = if visual_size.is_some() {
                    layout_scaled_visual_content(rect, visual_size)
                } else {
                    layout_inline_content(rect, ctx.style(), label, visual_size)
                };
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.config.opt);
                }
                if let (Some(image), Some(visual)) = (*image, layout.visual) {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.push_image(image, visual, color);
                }
            }
        }
    }
}

implement_widget!(Button, update_widget, paint_widget, preferred_size_widget);
