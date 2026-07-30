//! List item and list-box widgets.
//!
//! `ListItem` represents one selectable row and `ListBox` stores shared selection state for a
//! retained list.

use super::*;
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::widgets::{record_pending_event, take_pending_event};
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`ListItem`].
pub struct ListItemParameters {
    /// Initial label displayed for the list item.
    pub label: String,
    /// Optional atlas icon rendered alongside the label.
    pub icon: Option<IconId>,
    /// Font used for the label.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for ListItemParameters {}

impl ListItemParameters {
    /// Creates list-item parameters with default widget options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates list-item parameters with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: None,
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Creates list-item parameters with an icon and default widget options.
    pub fn with_icon(label: impl Into<String>, icon: IconId) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates list-item parameters with an icon and explicit widget options.
    pub fn with_icon_opt(label: impl Into<String>, icon: IconId, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            icon: Some(icon),
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used for the label.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent list-item state.
pub struct ListItemState {
    /// Mutable label displayed for the item.
    label: String,
    /// User submissions waiting to be consumed.
    pending_submissions: u32,
}

impl WidgetState for ListItemState {}

impl ListItemState {
    /// Returns the current label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Replaces the label without recording a user submission.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Consumes one pending user submission.
    pub fn take_submitted(&mut self) -> bool {
        take_pending_event(&mut self.pending_submissions)
    }
}

/// Concrete list-item runtime and sole strong owner of its application state.
pub struct ListItem {
    /// Initialization-only icon.
    icon: Option<IconId>,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ListItemState>>,
}

impl ListItem {
    /// Constructs a typed state handle and unique list-item runtime.
    pub fn create(parameters: ListItemParameters) -> (WidgetStateHandle<ListItemState>, Self) {
        let widget = ListItemBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
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
        runtime_read_state(&self.state, "ListItem::measure", |state| {
            if !state.label.is_empty() {
                width += text_size(style, atlas, self.font, &state.label).width;
            }
            let height = content_height(style, atlas, self.font, visual_h);
            Dimensioni::new(width.max(0), height)
        })
    }

    /// Paints row highlight, optional icon, and label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();

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

        runtime_read_state(&self.state, "ListItem::paint", |state| {
            if !state.label.is_empty() {
                let font = ctx.style().resolve_font_choice(self.font);
                ctx.draw_control_text_with_font(font, &state.label, text_rect, ControlColor::Text, self.opt);
            }
        });
    }
}

impl Widget for ListItem {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        if !ctx.clicked() {
            return ResourceState::NONE;
        }
        runtime_update_state(&self.state, "ListItem::update", |state| {
            record_pending_event(&mut state.pending_submissions);
        });
        ResourceState::SUBMIT
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for ListItem {
    type State = ListItemState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating list-item parameters with the concrete runtime.
pub struct ListItemBuilder;

impl WidgetBuilder for ListItemBuilder {
    type Parameters = ListItemParameters;
    type W = ListItem;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        ListItem {
            icon: parameters.icon,
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(ListItemState {
                label: parameters.label,
                pending_submissions: 0,
            })),
        }
    }
}

/// One-shot construction input for a [`ListBox`].
pub struct ListBoxParameters {
    /// Label displayed for the list box.
    pub label: String,
    /// Optional image rendered alongside the label.
    pub image: Option<TextureId>,
    /// Font used for the label.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for ListBoxParameters {}

impl ListBoxParameters {
    /// Creates list-box parameters with default widget options.
    pub fn new(label: impl Into<String>, image: Option<TextureId>) -> Self {
        Self {
            label: label.into(),
            image,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates list-box parameters with explicit widget options.
    pub fn with_opt(label: impl Into<String>, image: Option<TextureId>, opt: WidgetOption) -> Self {
        Self {
            label: label.into(),
            image,
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used for the label.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Application-facing persistent list-box state.
pub struct ListBoxState {
    /// User submissions waiting to be consumed.
    pending_submissions: u32,
}

impl WidgetState for ListBoxState {}

impl ListBoxState {
    /// Consumes one pending user submission.
    pub fn take_submitted(&mut self) -> bool {
        take_pending_event(&mut self.pending_submissions)
    }
}

/// Concrete list-box runtime and sole strong owner of its application state.
pub struct ListBox {
    /// Initialization-only label.
    label: String,
    /// Initialization-only image.
    image: Option<TextureId>,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ListBoxState>>,
}

impl ListBox {
    /// Constructs a typed state handle and unique list-box runtime.
    pub fn create(parameters: ListBoxParameters) -> (WidgetStateHandle<ListBoxState>, Self) {
        let widget = ListBoxBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures list-box inline label and optional image.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let visual = self.image.map(TextureId::size);
        inline_content_size(style, atlas, self.font, &self.label, visual)
    }

    /// Paints list-box frame, label, and optional image.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let rect = ctx.local_rect();
        if let Some(colorid) = widget_fill_color(ctx, ControlColor::Button, WidgetFillOption::HOVER | WidgetFillOption::CLICK) {
            ctx.draw_rect(rect, ctx.style().colors[colorid as usize]);
        }
        let visual_size = self.image.map(TextureId::size);
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

impl Widget for ListBox {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        if !ctx.clicked() {
            return ResourceState::NONE;
        }
        runtime_update_state(&self.state, "ListBox::update", |state| {
            record_pending_event(&mut state.pending_submissions);
        });
        ResourceState::SUBMIT
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for ListBox {
    type State = ListBoxState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating list-box parameters with the concrete runtime.
pub struct ListBoxBuilder;

impl WidgetBuilder for ListBoxBuilder {
    type Parameters = ListBoxParameters;
    type W = ListBox;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        ListBox {
            label: parameters.label,
            image: parameters.image,
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(ListBoxState { pending_submissions: 0 })),
        }
    }
}
