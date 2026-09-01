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

//! Retained list-item widget.
//!
//! `ListItem` represents one selectable row in a retained list.

use super::*;
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

impl crate::LeafWidget for ListItem {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        self.preferred_size_widget(style, atlas, constraints)
    }
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

/// Snapshot emitted when the user submits a list item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListItemSubmitted {
    /// Item label at submission time.
    pub label: String,
}

impl crate::WidgetEvent for ListItemSubmitted {}

/// Concrete retained list item, including its semantic state.
pub struct ListItem {
    /// Initialization-only icon.
    icon: Option<IconId>,
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Mutable label displayed for the item.
    label: String,
    /// Runtime-owned source for user submissions.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<ListItemSubmitted>>>,
}

impl ListItem {
    /// Constructs a retained node and a weak typed handle to its concrete list item.
    pub fn create(parameters: ListItemParameters) -> (TypedWidgetHandle<Self>, Node) {
        let widget = ListItemBuilder::create_widget(parameters);
        Node::typed_widget(widget)
    }

    /// Constructs a list item that emits through an enclosing list view's shared port.
    pub(crate) fn create_with_event_port(
        parameters: ListItemParameters,
        submitted_event: Rc<RefCell<crate::event::WidgetEventPort<ListItemSubmitted>>>,
    ) -> (TypedWidgetHandle<Self>, Node) {
        let widget = Self {
            icon: parameters.icon,
            font: parameters.font,
            opt: parameters.opt,
            label: parameters.label,
            submitted_event,
        };
        Node::typed_widget(widget)
    }

    /// Returns the current label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Replaces the label without recording a user submission.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Returns the native event endpoint emitted once for every user submission.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<ListItemSubmitted> {
        <Self as crate::TypedWidget<ListItemSubmitted>>::event(self)
    }

    /// Measures the row label and optional icon.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        let padding = style.padding.max(0);
        let mut width = padding.saturating_mul(2);
        let mut visual_h = 0;
        if let Some(icon) = self.icon {
            let size = atlas.get_icon_size(icon);
            width = width.saturating_add(size.width.max(0)).saturating_add(padding);
            visual_h = size.height;
        }
        if !self.label.is_empty() {
            width = width.saturating_add(text_size(style, atlas, self.font, &self.label).width.max(0));
        }
        let height = content_height(style, atlas, self.font, visual_h);
        Dimensioni::new(width.max(0), height)
    }

    /// Paints row highlight, optional icon, and label.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();

        // The default normal role is transparent; image themes may still provide ordinary row art.
        ctx.draw_appearance_center(AppearanceRole::ListItem, bounds);

        let mut text_rect = bounds;
        if let Some(icon) = self.icon {
            // Icons consume the left padding + icon width before the text region starts.
            let padding = ctx.style().padding.max(0);
            let icon_size = ctx.atlas().get_icon_size(icon);
            // Style values and retained allocations can independently reach coordinate limits.
            // Saturating the nonnegative extents keeps paint total while preserving ordinary
            // geometry exactly.
            let icon_width = icon_size.width.max(0);
            let icon_height = icon_size.height.max(0);
            let icon_x = bounds.x.saturating_add(padding);
            let icon_y = bounds.y.saturating_add(bounds.height.saturating_sub(icon_height).max(0) / 2);
            let icon_rect = rect(icon_x, icon_y, icon_width, icon_height);
            let consumed = icon_width.saturating_add(padding.saturating_mul(2));
            text_rect.x = text_rect.x.saturating_add(consumed);
            text_rect.width = text_rect.width.saturating_sub(consumed).max(0);
            let color = ctx.control_color(ControlColor::Text);
            ctx.draw_icon(icon, icon_rect, color);
        }

        if !self.label.is_empty() {
            let font = ctx.style().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, &self.label, text_rect, ControlColor::Text, self.opt);
        }
    }
}

impl TypedWidgetHandle<ListItem> {
    /// Clones the current item label while the widget is retained.
    pub fn label(&self) -> Option<String> {
        self.try_read(|widget| widget.label().to_owned())
    }

    /// Replaces the item label without emitting a submission.
    pub fn set_label(&self, label: impl Into<String>) -> Option<()> {
        self.try_update_with(label.into(), |widget, label| widget.set_label(label)).ok()
    }

    /// Returns the list item's native submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<ListItemSubmitted> {
        self.widget_event()
    }
}

impl Widget for ListItem {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        if ctx.action(input, self.keyboard_behavior()) != Some(KeyboardAction::Activate) {
            return;
        }
        // Clone before emission so observers receive an immutable selection snapshot.
        let label = self.label.clone();
        self.submitted_event.borrow_mut().emit(ListItemSubmitted { label });
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::TAB_STOP | KeyboardBehavior::ACTIVATE_ENTER | KeyboardBehavior::ACTIVATE_SPACE
    }
}

impl crate::TypedWidget<ListItemSubmitted> for ListItem {
    fn event(&self) -> crate::WidgetEventPortHandle<ListItemSubmitted> {
        crate::WidgetEventPortHandle::new(&self.submitted_event)
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
            label: parameters.label,
            submitted_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Boundary tests for retained list-item paint geometry.

    use super::*;
    use crate::render::DisplayList;
    use crate::test_support::{test_atlas, test_style};

    /// Verifies application-provided maximum padding cannot overflow icon or label placement.
    #[test]
    fn extreme_padding_keeps_icon_paint_total() {
        let atlas = test_atlas();
        let mut style = test_style(&atlas);
        style.padding = i32::MAX;
        let icon = atlas.icon_id("check").expect("the shared fixture icon must exist");
        let mut item = ListItemBuilder::create_widget(ListItemParameters::with_icon("item", icon));
        let bounds = rect(0, 0, 20, 20);
        let mut display_list = DisplayList::new();
        let mut ctx = WidgetPaintCtx::new_with_content_geometry(bounds, &mut display_list, bounds, &style, &atlas, true, true, false, false, false, true);

        item.paint(&mut ctx);

        // Hover fill remains visible; extreme icon/text geometry may be clipped completely.
        assert!(display_list.debug_operation_count() >= 1);
    }
}
