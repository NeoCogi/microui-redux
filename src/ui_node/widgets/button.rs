//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Button widget state and rendering.
//!
//! Buttons support text, arbitrary atlas icons, and external textures through one shared inline
//! placement path.

use super::*;
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

impl crate::LeafWidget for Button {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
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

    /// Creates button parameters with an atlas icon.
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

/// Semantic payload emitted when the user submits a button.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ButtonSubmitted;

impl crate::WidgetEvent for ButtonSubmitted {}

/// Concrete retained button, including its semantic and interaction state.
pub struct Button {
    /// Initialization-only content.
    content: ButtonContent,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Initialization-only fill behavior.
    fill: WidgetFillOption,
    /// Runtime-owned source for user submissions.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<ButtonSubmitted>>>,
}

impl Button {
    /// Constructs a retained node and a weak typed handle to its concrete button.
    pub fn create(parameters: ButtonParameters) -> (crate::TypedWidgetHandle<Self>, crate::Node) {
        let widget = ButtonBuilder::create_widget(parameters);
        crate::Node::typed_widget(widget)
    }

    /// Returns the native event endpoint emitted once for every user submission.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<ButtonSubmitted> {
        <Self as crate::TypedWidget<ButtonSubmitted>>::event(self)
    }

    /// Measures the label and optional visual content.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
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
                if visual.is_some() && constraints.width.bound().is_some() {
                    scaled_visual_content_size(constraints, visual)
                } else {
                    inline_content_size(style, atlas, self.font, label, visual)
                }
            }
        }
    }

    /// Paints the button frame, text, and optional visual payload.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let rect = ctx.local_rect();
        if !self.opt.intersects(WidgetOption::FRAME) && widget_fill_visible(ctx, self.fill) {
            // Framed buttons are painted once by retained traversal across their complete outer
            // allocation. An unframed button asks for only the role's stretchable center payload.
            ctx.draw_appearance_center(AppearanceRole::Button, rect);
        }
        let font = ctx.style().resolve_font_choice(self.font);
        match &self.content {
            ButtonContent::Text { label, icon } => {
                // Text/icon buttons use atlas icon metrics when placing the inline visual.
                let visual_size = icon.map(|icon| ctx.atlas().get_icon_size(icon));
                let placement = place_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, placement.text, AppearanceRole::Button, self.opt);
                }
                if let (Some(icon), Some(visual)) = (icon, placement.visual) {
                    let color = ctx.foreground(AppearanceRole::Button);
                    ctx.draw_icon(*icon, visual, color);
                }
            }
            ButtonContent::Image { label, image } => {
                let visual_size = image.map(TextureId::size);
                let placement = place_inline_content(rect, ctx.style(), label, visual_size);
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, placement.text, AppearanceRole::Button, self.opt);
                }
                if let (Some(image), Some(visual)) = (*image, placement.visual) {
                    let color = ctx.foreground(AppearanceRole::Button);
                    ctx.push_image(image, visual, color);
                }
            }
            ButtonContent::ScaledImage { label, image } => {
                let visual_size = image.map(TextureId::size);
                let placement = if visual_size.is_some() {
                    place_scaled_visual_content(rect, visual_size)
                } else {
                    place_inline_content(rect, ctx.style(), label, visual_size)
                };
                if !label.is_empty() {
                    ctx.draw_control_text_with_font(font, label, placement.text, AppearanceRole::Button, self.opt);
                }
                if let (Some(image), Some(visual)) = (*image, placement.visual) {
                    let color = ctx.foreground(AppearanceRole::Button);
                    ctx.push_image(image, visual, color);
                }
            }
        }
    }
}

impl crate::TypedWidgetHandle<Button> {
    /// Returns the button's native submission endpoint.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<ButtonSubmitted> {
        self.widget_event()
    }
}

impl Widget for Button {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn frame_appearance_role(&self) -> AppearanceRole {
        // Buttons use one stateful role for their complete runtime-owned outer frame.
        AppearanceRole::Button
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        if ctx.action(input, self.keyboard_behavior()) != Some(KeyboardAction::Activate) {
            return;
        }
        // Pointer click, Enter, and Space converge on the button's single typed submission port.
        self.submitted_event.borrow_mut().emit(ButtonSubmitted);
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        // Buttons are ordinary sequential focus targets. Shared activation capabilities are added
        // independently so focus traversal does not depend on button-specific routing knowledge.
        KeyboardBehavior::TAB_STOP | KeyboardBehavior::ACTIVATE_ENTER | KeyboardBehavior::ACTIVATE_SPACE
    }
}

impl crate::TypedWidget<ButtonSubmitted> for Button {
    fn event(&self) -> crate::WidgetEventPortHandle<ButtonSubmitted> {
        crate::WidgetEventPortHandle::new(&self.submitted_event)
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
            submitted_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}
