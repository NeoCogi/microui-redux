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

//! Concrete retained widgets used by application menus.
//!
//! [`MenuItem`] is a public leaf widget with its own typed submission source. `MenuBar` and
//! `MenuSeparator` are private presentation details composed by [`crate::WindowMenu`].

use super::*;
use std::{cell::RefCell, rc::Rc};

/// Horizontal inset on both sides of one top-level menu heading.
fn heading_horizontal_padding(style: &Style) -> i32 {
    // A heading should remain comfortably clickable even in compact themes. Reusing Style padding
    // also makes the bar scale with the rest of the retained controls without a menu-only metric.
    style.padding.max(1)
}

/// Vertical extent shared by menu headings and ordinary panel rows.
fn menu_row_height(style: &Style, atlas: &AtlasHandle, font: FontChoice) -> i32 {
    // Checked entries may display the theme check icon, so row measurement must account for both
    // text and the visual column before either bar or panel geometry is committed.
    let check_height = atlas.get_icon_size(style.icons.check).height;
    content_height(style, atlas, font, check_height)
}

/// Event emitted by the window-local bar when one top-level heading changes open state.
#[derive(Copy, Clone, Debug)]
pub(crate) struct MenuBarSubmitted {
    /// Index of the selected top-level menu in the component's authoritative specification.
    pub(crate) index: usize,
    /// Whether this submission requests that the indexed menu remain open.
    pub(crate) open: bool,
    /// One-pixel screen-space anchor immediately below the submitted heading.
    pub(crate) anchor: Recti,
}

impl crate::WidgetEvent for MenuBarSubmitted {}

/// Retained top-level menu bar mounted as the first child of a window-content column.
pub(crate) struct MenuBar {
    /// Immutable top-level labels copied from the component's semantic specification.
    labels: Vec<String>,
    /// Heading currently rendered as open, or `None` while no panel is visible.
    open_menu: Option<usize>,
    /// Heading most recently reached by a routed pointer move inside this bar.
    hovered_menu: Option<usize>,
    /// Body font used consistently for measurement and paint.
    font: FontChoice,
    /// Interaction options; preserving focus keeps a menu click from stealing editor focus.
    opt: WidgetOption,
    /// Runtime-owned source consumed exclusively by the coordinating `WindowMenu`.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<MenuBarSubmitted>>>,
}

impl MenuBar {
    /// Creates the retained bar node, weak typed handle, and internal submission endpoint.
    pub(crate) fn create(labels: Vec<String>) -> (TypedWidgetHandle<Self>, Node, WidgetEventPortHandle<MenuBarSubmitted>) {
        // Construct the port before erasing the widget so both the component and retained producer
        // refer to the same queue without giving either a second ownership path to the node.
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted = WidgetEventPortHandle::new(&submitted_event);
        let widget = Self {
            labels,
            open_menu: None,
            hovered_menu: None,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::PRESERVE_FOCUS,
            submitted_event,
        };
        let (handle, node) = Node::typed_widget(widget);
        (handle, node, submitted)
    }

    /// Reconciles the heading highlight with the component-owned popup state.
    pub(crate) fn set_open_menu(&mut self, open_menu: Option<usize>) {
        // Invalid indices are normalized to closed state so replacing a specification can never
        // leave the bar painting a heading that no longer exists.
        self.open_menu = open_menu.filter(|index| *index < self.labels.len());
    }

    /// Returns the currently open heading for component behavior tests.
    #[cfg(test)]
    pub(crate) const fn open_menu(&self) -> Option<usize> {
        self.open_menu
    }

    /// Computes each heading allocation from authoritative font metrics and padding.
    fn heading_rects(&self, bounds: Recti, style: &Style, atlas: &AtlasHandle) -> Vec<Recti> {
        // The number of headings is normally tiny. Rebuilding this short vector during measure,
        // input, and paint avoids retaining a geometry cache that could become stale after a style
        // change or a new parent allocation.
        let padding = heading_horizontal_padding(style);
        let height = bounds.height.max(0);
        let font = style.resolve_font_choice(self.font);
        let mut x = bounds.x;
        self.labels
            .iter()
            .map(|label| {
                let text_width = atlas.get_text_size(font, label).width.max(0);
                let width = text_width.saturating_add(padding.saturating_mul(2));
                let heading = Recti::new(x, bounds.y, width, height);
                x = x.saturating_add(width);
                heading
            })
            .collect()
    }

    /// Resolves the heading under one bar-local pointer position.
    fn heading_at(&self, bounds: Recti, style: &Style, atlas: &AtlasHandle, position: Vec2i) -> Option<usize> {
        self.heading_rects(bounds, style, atlas).iter().position(|heading| heading.contains(&position))
    }

    /// Emits one fully owned popup request after committing the bar's semantic open state.
    fn submit_heading(&mut self, ctx: &WidgetUpdateCtx<'_>, index: usize, heading: Recti) {
        // Clicking an already-open heading toggles the complete menu closed; clicking a different
        // heading switches panels without waiting for the old popup's outside-dismissal event.
        let open = self.open_menu != Some(index);
        self.open_menu = open.then_some(index);

        // Widget input and layout are local, whereas popup roots are placed in screen coordinates.
        // Capture the authoritative transform in the event so the later safe dispatch boundary does
        // not need to borrow the bar or rely on previous-frame geometry.
        let screen = ctx.screen_content_rect();
        let anchor = Recti::new(
            screen.x.saturating_add(heading.x),
            screen.y.saturating_add(heading.y).saturating_add(heading.height),
            heading.width,
            1,
        );
        self.submitted_event.borrow_mut().emit(MenuBarSubmitted { index, open, anchor });
    }
}

impl LeafWidget for MenuBar {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // The parent stretches the bar to the exact window body width. Preferred width still reports
        // the complete heading row so auto-sized windows cannot truncate their menu labels.
        let padding = heading_horizontal_padding(style);
        let font = style.resolve_font_choice(self.font);
        let width = self.labels.iter().fold(0i32, |width, label| {
            width
                .saturating_add(atlas.get_text_size(font, label).width.max(0))
                .saturating_add(padding.saturating_mul(2))
        });
        Dimensioni::new(width, menu_row_height(style, atlas, self.font))
    }
}

impl Widget for MenuBar {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Hover is a routed snapshot rather than a permanent semantic value. Clear it as soon as
        // another node owns the pointer so paint cannot leave a stale heading highlighted.
        if !ctx.hovered() {
            self.hovered_menu = None;
        }

        let bounds = ctx.local_rect();
        match input {
            Some(UiInputEvent::MouseMove { pos, .. }) | Some(UiInputEvent::MouseDrag { pos, .. }) => {
                self.hovered_menu = self.heading_at(bounds, ctx.style(), ctx.atlas(), *pos);
            }
            Some(UiInputEvent::MouseDown { pos, button }) if button.intersects(MouseButton::LEFT) => {
                let headings = self.heading_rects(bounds, ctx.style(), ctx.atlas());
                let Some(index) = headings.iter().position(|heading| heading.contains(pos)) else {
                    return;
                };
                self.hovered_menu = Some(index);
                self.submit_heading(ctx, index, headings[index]);
            }
            _ => {}
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        // PanelBG distinguishes the persistent menu strip from ordinary window content while
        // retaining the context's established semantic palette and style-override behavior.
        ctx.draw_rect(bounds, ctx.style().colors[ControlColor::PanelBG as usize]);

        let font = ctx.style().resolve_font_choice(self.font);
        for (index, heading) in self.heading_rects(bounds, ctx.style(), ctx.atlas()).into_iter().enumerate() {
            if self.open_menu == Some(index) {
                ctx.draw_rect(heading, ctx.style().colors[ControlColor::ButtonFocus as usize]);
            } else if self.hovered_menu == Some(index) {
                ctx.draw_rect(heading, ctx.style().colors[ControlColor::ButtonHover as usize]);
            }
            ctx.draw_control_text_with_font(font, &self.labels[index], heading, ControlColor::Text, WidgetOption::NONE);
        }
    }
}

/// Visual state displayed in the fixed marker column of one menu item.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MenuItemMark {
    /// The item has no persistent marker.
    None,
    /// A checkable item whose boolean controls check-glyph visibility.
    Checked(bool),
    /// A radio item whose boolean controls selection-marker visibility.
    Radio(bool),
}

/// One-shot construction input for a concrete menu item.
pub struct MenuItemParameters {
    /// Initial user-visible label.
    pub label: String,
    /// Whether the item initially accepts pointer submission.
    pub enabled: bool,
    /// Initial check or radio presentation.
    pub mark: MenuItemMark,
    /// Presentation-only accelerator text aligned at the right edge.
    pub shortcut_hint: Option<String>,
    /// Font used by both label and accelerator text.
    pub font: FontChoice,
}

impl WidgetParameters for MenuItemParameters {}

impl MenuItemParameters {
    /// Creates an enabled, unmarked item without a shortcut hint.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            mark: MenuItemMark::None,
            shortcut_hint: None,
            font: FontChoice::Role(FontRole::Body),
        }
    }

    /// Replaces the initial enabled state.
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Makes the item initially disabled.
    pub const fn disabled(self) -> Self {
        self.enabled(false)
    }

    /// Replaces the initial marker state.
    pub const fn mark(mut self, mark: MenuItemMark) -> Self {
        self.mark = mark;
        self
    }

    /// Configures an initially checked or unchecked item.
    pub const fn checked(self, checked: bool) -> Self {
        self.mark(MenuItemMark::Checked(checked))
    }

    /// Configures an initially selected or unselected radio item.
    pub const fn radio(self, selected: bool) -> Self {
        self.mark(MenuItemMark::Radio(selected))
    }

    /// Adds presentation-only accelerator text such as Ctrl+O.
    pub fn shortcut_hint(mut self, shortcut_hint: impl Into<String>) -> Self {
        self.shortcut_hint = Some(shortcut_hint.into());
        self
    }

    /// Replaces the font used by the item.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Event emitted by one specific menu item after an enabled pointer submission.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct MenuItemSubmitted;

impl crate::WidgetEvent for MenuItemSubmitted {}

/// Concrete retained menu item with its own semantic state and event source.
pub struct MenuItem {
    /// Mutable user-visible label.
    label: String,
    /// Mutable interaction state.
    enabled: bool,
    /// Mutable check or radio presentation.
    mark: MenuItemMark,
    /// Mutable presentation-only accelerator text.
    shortcut_hint: Option<String>,
    /// Initialization-only font choice.
    font: FontChoice,
    /// Preserve existing keyboard focus while operating menus.
    opt: WidgetOption,
    /// Runtime-owned source registered independently for this item.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<MenuItemSubmitted>>>,
}

impl MenuItem {
    /// Constructs one retained item node and its weak typed handle.
    pub fn create(parameters: MenuItemParameters) -> (TypedWidgetHandle<Self>, Node) {
        // The node is the sole strong owner; registration and live updates use only the weak handle.
        Node::typed_widget(Self {
            label: parameters.label,
            enabled: parameters.enabled,
            mark: parameters.mark,
            shortcut_hint: parameters.shortcut_hint,
            font: parameters.font,
            opt: WidgetOption::PRESERVE_FOCUS,
            submitted_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        })
    }

    /// Returns the current user-visible label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Replaces the label without emitting a submission.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Returns whether the item currently accepts submission.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables pointer submission immediately.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Returns the current check or radio presentation.
    pub const fn mark(&self) -> MenuItemMark {
        self.mark
    }

    /// Replaces the check or radio presentation without emitting a submission.
    pub const fn set_mark(&mut self, mark: MenuItemMark) {
        self.mark = mark;
    }

    /// Returns the current presentation-only accelerator text.
    pub fn shortcut_hint(&self) -> Option<&str> {
        self.shortcut_hint.as_deref()
    }

    /// Replaces presentation-only accelerator text.
    pub fn set_shortcut_hint(&mut self, shortcut_hint: Option<String>) {
        self.shortcut_hint = shortcut_hint;
    }

    /// Returns the item's native submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<MenuItemSubmitted> {
        <Self as crate::TypedWidget<MenuItemSubmitted>>::event(self)
    }

    /// Derives a subdued text color without adding a menu-only palette slot.
    fn text_color(&self, style: &Style) -> Color {
        let mut color = style.colors[ControlColor::Text as usize];
        if !self.enabled {
            // Alpha preserves the theme hue and works consistently on every renderer backend.
            color.a = ((u16::from(color.a) * 45) / 100).max(1) as u8;
        }
        color
    }

    /// Splits the allocation into a marker column and shared label/hint region.
    fn row_regions(bounds: Recti, style: &Style, atlas: &AtlasHandle) -> (Recti, Recti) {
        let padding = style.padding.max(1);
        let marker_width = atlas.get_icon_size(style.icons.check).width.max(0);
        let marker = Recti::new(bounds.x.saturating_add(padding), bounds.y, marker_width, bounds.height);
        let text_x = marker.x.saturating_add(marker.width);
        let text = Recti::new(
            text_x,
            bounds.y,
            bounds.x.saturating_add(bounds.width).saturating_sub(text_x).max(0),
            bounds.height,
        );
        (marker, text)
    }

    /// Draws the current marker centered in its fixed column.
    fn paint_marker(&self, ctx: &mut WidgetPaintCtx<'_>, marker: Recti, color: Color) {
        match self.mark {
            MenuItemMark::None | MenuItemMark::Checked(false) | MenuItemMark::Radio(false) => {}
            MenuItemMark::Checked(true) => {
                let size = ctx.atlas().get_icon_size(ctx.style().icons.check);
                let icon = Recti::new(
                    marker.x.saturating_add((marker.width - size.width) / 2),
                    marker.y.saturating_add((marker.height - size.height) / 2),
                    size.width,
                    size.height,
                );
                ctx.draw_icon(ctx.style().icons.check, icon, color);
            }
            MenuItemMark::Radio(true) => {
                // The required atlas has no radio glyph, so a compact square is the stable fallback.
                let extent = (marker.width.min(marker.height) / 3).max(3);
                let indicator = Recti::new(
                    marker.x.saturating_add((marker.width - extent) / 2),
                    marker.y.saturating_add((marker.height - extent) / 2),
                    extent,
                    extent,
                );
                ctx.draw_rect(indicator, color);
            }
        }
    }
}

impl LeafWidget for MenuItem {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // Reserve one marker column for every item so mixed marked and unmarked rows align.
        let padding = style.padding.max(1);
        let font = style.resolve_font_choice(self.font);
        let marker_width = atlas.get_icon_size(style.icons.check).width.max(0);
        let label_width = atlas.get_text_size(font, &self.label).width.max(0);
        let shortcut_width = self
            .shortcut_hint
            .as_deref()
            .map(|hint| atlas.get_text_size(font, hint).width.max(0))
            .unwrap_or(0);
        let shortcut_extent = if shortcut_width == 0 { 0 } else { padding.saturating_add(shortcut_width) };
        let width = padding
            .saturating_add(marker_width)
            .saturating_add(padding)
            .saturating_add(label_width)
            .saturating_add(shortcut_extent)
            .saturating_add(padding);
        Dimensioni::new(width, menu_row_height(style, atlas, self.font))
    }
}

impl Widget for MenuItem {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // Disabled items retain ordinary routing for hover cleanup but never publish application work.
        if self.enabled && ctx.clicked() {
            self.submitted_event.borrow_mut().emit(MenuItemSubmitted);
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        if self.enabled && (ctx.hovered() || ctx.focused()) {
            ctx.draw_rect(bounds, ctx.style().colors[ControlColor::ButtonHover as usize]);
        }

        let color = self.text_color(ctx.style());
        let (marker, text) = Self::row_regions(bounds, ctx.style(), ctx.atlas());
        self.paint_marker(ctx, marker, color);
        let font = ctx.style().resolve_font_choice(self.font);
        ctx.draw_control_text_color_with_font(font, &self.label, text, color, WidgetOption::NONE);
        if let Some(hint) = &self.shortcut_hint {
            ctx.draw_control_text_color_with_font(font, hint, text, color, WidgetOption::ALIGN_RIGHT);
        }
    }
}

impl crate::TypedWidget<MenuItemSubmitted> for MenuItem {
    fn event(&self) -> WidgetEventPortHandle<MenuItemSubmitted> {
        WidgetEventPortHandle::new(&self.submitted_event)
    }
}

impl TypedWidgetHandle<MenuItem> {
    /// Returns this specific item's native submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<MenuItemSubmitted> {
        self.widget_event()
    }

    /// Returns the current enabled state while the retained item is alive.
    pub fn is_enabled(&self) -> Option<bool> {
        self.try_read(MenuItem::is_enabled)
    }

    /// Enables or disables this specific retained item.
    pub fn set_enabled(&self, enabled: bool) -> Option<()> {
        self.try_update(|item| item.set_enabled(enabled))
    }

    /// Returns the current marker while the retained item is alive.
    pub fn mark(&self) -> Option<MenuItemMark> {
        self.try_read(MenuItem::mark)
    }

    /// Replaces this specific item's marker.
    pub fn set_mark(&self, mark: MenuItemMark) -> Option<()> {
        self.try_update(|item| item.set_mark(mark))
    }
}

/// Non-interactive rule inserted between two non-empty menu groups.
pub(crate) struct MenuSeparator {
    /// Separator rows never participate in input routing.
    opt: WidgetOption,
}

impl MenuSeparator {
    /// Creates one private retained separator node.
    pub(crate) fn create() -> Node {
        // No typed handle is needed because a separator has no mutable semantic state.
        Node::typed_widget(Self { opt: WidgetOption::NO_INTERACT }).1
    }
}

impl LeafWidget for MenuSeparator {
    fn measure(&self, style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(0, style.spacing.max(3))
    }
}

impl Widget for MenuSeparator {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Center one rule in the allocation and leave horizontal breathing room.
        let bounds = ctx.local_rect();
        let padding = ctx.style().padding.max(1);
        let rule = Recti::new(
            bounds.x.saturating_add(padding),
            bounds.y.saturating_add(bounds.height / 2),
            bounds.width.saturating_sub(padding.saturating_mul(2)).max(0),
            1,
        );
        ctx.draw_rect(rule, ctx.style().colors[ControlColor::Border as usize]);
    }
}
