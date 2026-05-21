//! Button widget state and rendering.
//!
//! Buttons support text, atlas icons, external images, and dynamic atlas slots through one shared
//! layout path.

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
        image: Option<Image>,
    },
    /// A text label and a slot refreshed via a paint callback.
    Slot {
        /// Text displayed on the button.
        label: String,
        /// Slot rendered on the button.
        slot: SlotId,
        /// Callback used to fill the slot pixels.
        paint: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
}

#[derive(Clone)]
/// Persistent state for button widgets.
pub struct Button {
    /// Content rendered inside the button.
    pub content: ButtonContent,
    /// Font selection used for the button label.
    pub font: FontChoice,
    /// Widget options applied to the button.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the button.
    pub scroll_behavior: ScrollBehavior,
    /// Fill behavior for the button background.
    pub fill: WidgetFillOption,
}

impl Button {
    /// Creates a text button with default options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            font: FontChoice::default(),
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates a text button with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::NONE,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates an image button with explicit widget options and fill behavior.
    pub fn with_image(label: impl Into<String>, image: Option<Image>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Image { label: label.into(), image },
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::NONE,
            fill,
        }
    }

    /// Creates a slot button that repaints via the provided callback.
    pub fn with_slot(label: impl Into<String>, slot: SlotId, paint: Rc<dyn Fn(usize, usize) -> Color4b>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Slot { label: label.into(), slot, paint },
            font: FontChoice::default(),
            opt,
            scroll_behavior: ScrollBehavior::NONE,
            fill,
        }
    }

    /// Measures the label and optional visual content.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        match &self.content {
            ButtonContent::Text { label, icon } => {
                let visual = icon.map(|icon| atlas.get_icon_size(icon));
                inline_content_size(style, atlas, self.font, label, visual)
            }
            ButtonContent::Image { label, image } => {
                let visual = image.map(|image| image.size(atlas));
                inline_content_size(style, atlas, self.font, label, visual)
            }
            ButtonContent::Slot { label, slot, .. } => {
                let visual = Some(atlas.get_slot_size(*slot));
                inline_content_size(style, atlas, self.font, label, visual)
            }
        }
    }

    /// Buttons submit on click and do not keep extra transient state.
    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        submit_on_click(control)
    }

    /// Paints the button frame, text, and optional visual payload.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let rect = ctx.rect();
        if !self.opt.has_no_frame() {
            if let Some(colorid) = widget_fill_color(control, ControlColor::Button, self.fill) {
                ctx.draw_frame(rect, colorid);
            }
        }
        let font = ctx.style().resolve_font_choice(self.font);
        match &self.content {
            ButtonContent::Text { label, icon } => {
                // Text/icon buttons use atlas icon metrics when laying out the inline visual.
                let visual_size = icon.map(|icon| ctx.atlas().get_icon_size(icon));
                let layout = layout_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.opt);
                }
                if let (Some(icon), Some(visual)) = (icon, layout.visual) {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.draw_icon(*icon, visual, color);
                }
            }
            ButtonContent::Image { label, image } => {
                // External textures and atlas slots both report dimensions through `Image::size`.
                let visual_size = image.map(|image| image.size(ctx.atlas()));
                let layout = layout_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.opt);
                }
                if let (Some(image), Some(visual)) = (*image, layout.visual) {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.push_image(image, visual, color);
                }
            }
            ButtonContent::Slot { label, slot, paint } => {
                // Dynamic slots repaint the atlas slot immediately before drawing it.
                let visual_size = Some(ctx.atlas().get_slot_size(*slot));
                let layout = layout_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.opt);
                }
                if let Some(visual) = layout.visual {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.draw_slot_with_function(*slot, visual, color, paint.clone());
                }
            }
        }
    }
}

implement_widget!(Button, update_widget, paint_widget, preferred_size_widget);
