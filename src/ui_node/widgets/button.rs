//! Button widget state and rendering.
//!
//! Buttons support text, semantic atlas icons, and external textures through one shared layout
//! path.

use super::*;
use crate::ui_node::runtime_update_state;
use crate::widgets::{record_pending_event, take_pending_event};
use std::{cell::RefCell, rc::Rc};

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

/// One-shot construction input for a [`Button`].
pub struct ButtonParameters {
    /// Content rendered inside the button.
    pub content: ButtonContent,
    /// Font used for text content.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
    /// Fill behavior for the button background.
    pub fill: WidgetFillOption,
}

impl WidgetParameters for ButtonParameters {}

impl ButtonParameters {
    /// Creates text-button parameters with default options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::FRAME,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates text-button parameters with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            font: FontChoice::Role(FontRole::Body),
            opt,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates button parameters with a semantic atlas icon.
    pub fn with_icon(label: impl Into<String>, icon: IconId, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: Some(icon) },
            font: FontChoice::Role(FontRole::Body),
            opt,
            fill,
        }
    }

    /// Creates image-button parameters with explicit options and fill behavior.
    pub fn with_image(label: impl Into<String>, image: Option<TextureId>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Image { label: label.into(), image },
            font: FontChoice::Role(FontRole::Body),
            opt,
            fill,
        }
    }

    /// Creates scaled-image button parameters.
    pub fn with_scaled_image(label: impl Into<String>, image: Option<TextureId>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::ScaledImage { label: label.into(), image },
            font: FontChoice::Role(FontRole::Body),
            opt,
            fill,
        }
    }

    /// Replaces the font used for text content.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent button state.
pub struct ButtonState {
    /// User submissions waiting to be consumed.
    pending_submissions: u32,
}

impl WidgetState for ButtonState {}

impl ButtonState {
    /// Consumes one pending user submission.
    pub fn take_submitted(&mut self) -> bool {
        take_pending_event(&mut self.pending_submissions)
    }
}

/// Concrete button runtime and sole strong owner of its application state.
pub struct Button {
    /// Initialization-only content.
    content: ButtonContent,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Initialization-only fill behavior.
    fill: WidgetFillOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ButtonState>>,
}

impl Button {
    /// Constructs a typed state handle and unique button runtime.
    pub fn create(parameters: ButtonParameters) -> (WidgetStateHandle<ButtonState>, Self) {
        let widget = ButtonBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures the label and optional visual content.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        match &self.content {
            ButtonContent::Text { label, icon } => {
                let visual = icon.map(|icon| atlas.get_icon_size(icon));
                inline_content_size(style, atlas, self.font, label, visual)
            }
            ButtonContent::Image { label, image } => {
                let visual = image.map(TextureId::size);
                inline_content_size(style, atlas, self.font, label, visual)
            }
            ButtonContent::ScaledImage { label, image } => {
                let visual = image.map(TextureId::size);
                if visual.is_some() && _avail.width > 0 {
                    scaled_visual_content_size(_avail, visual)
                } else {
                    inline_content_size(style, atlas, self.font, label, visual)
                }
            }
        }
    }

    /// Paints the button frame, text, and optional visual payload.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let rect = ctx.local_rect();
        if let Some(colorid) = widget_fill_color(ctx, ControlColor::Button, self.fill) {
            ctx.draw_rect(rect, ctx.style().colors[colorid as usize]);
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
                let visual_size = image.map(TextureId::size);
                let layout = layout_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.opt);
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
                    ctx.draw_control_text_with_font(font, label, layout.text, ControlColor::Text, self.opt);
                }
                if let (Some(image), Some(visual)) = (*image, layout.visual) {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.push_image(image, visual, color);
                }
            }
        }
    }
}

impl Widget for Button {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        if !ctx.clicked() {
            return;
        }
        runtime_update_state(&self.state, "Button::update", |state| {
            record_pending_event(&mut state.pending_submissions);
        });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for Button {
    type State = ButtonState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating button parameters with the concrete runtime.
pub struct ButtonBuilder;

impl WidgetBuilder for ButtonBuilder {
    type Parameters = ButtonParameters;
    type W = Button;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Button {
            content: parameters.content,
            font: parameters.font,
            opt: parameters.opt,
            fill: parameters.fill,
            state: Rc::new(RefCell::new(ButtonState { pending_submissions: 0 })),
        }
    }
}
