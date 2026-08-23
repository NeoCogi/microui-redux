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

//! Retained presentation widgets used by the application-owned window-menu component.
//!
//! A menu spans two independent root trees: its bar lives inside the owning window while its panel
//! lives in an auto-sized popup root. These widgets deliberately publish only internal coordination
//! events. [`crate::WindowMenu`] owns the public semantic menu specification, reconciles both
//! widgets at context event boundaries, and exposes one typed application command source.

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

/// Command selection emitted by the popup panel to its coordinating component.
#[derive(Clone, Debug)]
pub(crate) struct MenuPanelSubmitted<C: 'static> {
    /// Application-defined command cloned from the selected semantic entry.
    pub(crate) command: C,
}

impl<C: 'static> crate::WidgetEvent for MenuPanelSubmitted<C> {}

/// Auto-sized popup content that renders and interacts with one flat menu at a time.
pub(crate) struct MenuPanel<C: Clone + 'static> {
    /// Current semantic entry snapshot supplied by the coordinating window menu.
    entries: Vec<crate::MenuEntry<C>>,
    /// Selectable row currently under the pointer, expressed as an entry index.
    selected: Option<usize>,
    /// Minimum panel width inherited from the heading that opened this menu.
    minimum_width: i32,
    /// Body font used by labels and shortcut hints.
    font: FontChoice,
    /// Panel options; hold focus prepares this leaf for later keyboard navigation.
    opt: WidgetOption,
    /// Shared command source retained across replacement entry snapshots.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<MenuPanelSubmitted<C>>>>,
}

impl<C: Clone + 'static> MenuPanel<C> {
    /// Creates one initially empty panel and its internal command endpoint.
    pub(crate) fn create() -> (TypedWidgetHandle<Self>, Node, WidgetEventPortHandle<MenuPanelSubmitted<C>>) {
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted = WidgetEventPortHandle::new(&submitted_event);
        let widget = Self {
            entries: Vec::new(),
            selected: None,
            minimum_width: 0,
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::HOLD_FOCUS,
            submitted_event,
        };
        let (handle, node) = Node::typed_widget(widget);
        (handle, node, submitted)
    }

    /// Replaces the visible menu snapshot and resets pointer selection for a fresh activation.
    pub(crate) fn set_menu(&mut self, entries: Vec<crate::MenuEntry<C>>, minimum_width: i32) {
        // Entry ownership remains with this widget until another heading opens. The application
        // component retains its authoritative specification separately so closing a popup never
        // destroys menu definitions or mutable enabled/check state.
        self.entries = entries;
        self.minimum_width = minimum_width.max(0);
        self.selected = None;
    }

    /// Returns the current entry snapshot for component behavior tests.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[crate::MenuEntry<C>] {
        &self.entries
    }

    /// Computes the vertical extent assigned to a separator group boundary.
    fn separator_height(style: &Style) -> i32 {
        // A separator reserves breathing room around its one-pixel rule. Saturating callers remain
        // well-defined even for unusual styles with zero or negative spacing.
        style.spacing.max(3)
    }

    /// Computes row rectangles in entry order for hit testing and painting.
    fn entry_rects(&self, bounds: Recti, style: &Style, atlas: &AtlasHandle) -> Vec<Recti> {
        let row_height = menu_row_height(style, atlas, self.font);
        let separator_height = Self::separator_height(style);
        let mut y = bounds.y;
        self.entries
            .iter()
            .map(|entry| {
                let height = if entry.is_separator() { separator_height } else { row_height };
                let row = Recti::new(bounds.x, y, bounds.width.max(0), height);
                y = y.saturating_add(height);
                row
            })
            .collect()
    }

    /// Returns the enabled item index at one panel-local pointer position.
    fn selectable_entry_at(&self, bounds: Recti, style: &Style, atlas: &AtlasHandle, position: Vec2i) -> Option<usize> {
        self.entry_rects(bounds, style, atlas)
            .iter()
            .position(|row| row.contains(&position))
            .filter(|index| self.entries[*index].as_item().is_some_and(crate::MenuItemSpec::is_enabled))
    }

    /// Computes the desired panel size from aligned marker, label, and shortcut columns.
    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle) -> Dimensioni {
        let padding = style.padding.max(1);
        let font = style.resolve_font_choice(self.font);
        let marker_width = atlas.get_icon_size(style.icons.check).width.max(0);
        let mut label_width = 0;
        let mut shortcut_width = 0;
        let mut height = 0i32;

        for entry in &self.entries {
            let Some(item) = entry.as_item() else {
                height = height.saturating_add(Self::separator_height(style));
                continue;
            };
            label_width = label_width.max(atlas.get_text_size(font, item.label()).width.max(0));
            shortcut_width = shortcut_width.max(
                item.shortcut_hint()
                    .map(|shortcut| atlas.get_text_size(font, shortcut).width.max(0))
                    .unwrap_or(0),
            );
            height = height.saturating_add(menu_row_height(style, atlas, self.font));
        }

        // Layout reserves marker + label + optional shortcut columns with explicit padding between
        // them. The heading width remains a lower bound so very short menus still align visually
        // with the top-level surface that opened them.
        let shortcut_extent = if shortcut_width > 0 { padding.saturating_add(shortcut_width) } else { 0 };
        let content_width = padding
            .saturating_add(marker_width)
            .saturating_add(padding)
            .saturating_add(label_width)
            .saturating_add(shortcut_extent)
            .saturating_add(padding);
        Dimensioni::new(content_width.max(self.minimum_width), height)
    }

    /// Splits one row into marker, label, and shortcut rectangles.
    fn row_columns(row: Recti, style: &Style, atlas: &AtlasHandle) -> (Recti, Recti, Recti) {
        let padding = style.padding.max(1);
        let marker_width = atlas.get_icon_size(style.icons.check).width.max(0);
        let marker = Recti::new(row.x.saturating_add(padding), row.y, marker_width, row.height);
        let shortcut_width = (row.width / 3).max(0);
        let shortcut = Recti::new(
            row.x.saturating_add(row.width).saturating_sub(padding).saturating_sub(shortcut_width),
            row.y,
            shortcut_width,
            row.height,
        );
        let label_x = marker.x.saturating_add(marker.width).saturating_add(padding);
        let label_right = shortcut.x.saturating_sub(padding);
        let label = Recti::new(label_x, row.y, label_right.saturating_sub(label_x).max(0), row.height);
        (marker, label, shortcut)
    }

    /// Derives a subdued text color without adding a menu-specific palette slot.
    fn disabled_text_color(style: &Style) -> Color {
        let mut color = style.colors[ControlColor::Text as usize];
        // Preserve hue and use alpha as the single compositing-independent disabled-state signal.
        // At least one alpha unit keeps non-transparent themes from erasing disabled labels entirely.
        color.a = ((u16::from(color.a) * 45) / 100).max(1) as u8;
        color
    }

    /// Draws a check or radio marker centered inside the row's marker column.
    fn paint_marker(ctx: &mut WidgetPaintCtx<'_>, marker: Recti, mark: crate::MenuItemMark, enabled: bool) {
        let color = if enabled {
            ctx.style().colors[ControlColor::Text as usize]
        } else {
            Self::disabled_text_color(ctx.style())
        };
        match mark {
            crate::MenuItemMark::None | crate::MenuItemMark::Checked(false) | crate::MenuItemMark::Radio(false) => {}
            crate::MenuItemMark::Checked(true) => {
                let size = ctx.atlas().get_icon_size(ctx.style().icons.check);
                let icon = Recti::new(
                    marker.x.saturating_add((marker.width - size.width) / 2),
                    marker.y.saturating_add((marker.height - size.height) / 2),
                    size.width,
                    size.height,
                );
                ctx.draw_icon(ctx.style().icons.check, icon, color);
            }
            crate::MenuItemMark::Radio(true) => {
                // The default atlas has no radio glyph. A compact filled square preserves a distinct
                // radio presentation without extending the mandatory semantic icon set.
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

impl<C: Clone + 'static> LeafWidget for MenuPanel<C> {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        self.preferred_size(style, atlas)
    }
}

impl<C: Clone + 'static> Widget for MenuPanel<C> {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        if !ctx.hovered() && !ctx.active() {
            // Retain selection during a captured drag outside the panel, but clear ordinary stale
            // hover as soon as another root or widget becomes the current pointer target.
            self.selected = None;
        }

        let bounds = ctx.local_rect();
        match input {
            Some(UiInputEvent::MouseMove { pos, .. }) | Some(UiInputEvent::MouseDrag { pos, .. }) | Some(UiInputEvent::MouseDown { pos, .. }) => {
                self.selected = self.selectable_entry_at(bounds, ctx.style(), ctx.atlas(), *pos);
            }
            Some(UiInputEvent::MouseUp { pos, button }) if button.intersects(MouseButton::LEFT) => {
                self.selected = self.selectable_entry_at(bounds, ctx.style(), ctx.atlas(), *pos);
                let Some(index) = self.selected else { return };
                let Some(item) = self.entries[index].as_item() else { return };
                self.submitted_event.borrow_mut().emit(MenuPanelSubmitted { command: item.command().clone() });
            }
            _ => {}
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        ctx.draw_rect(bounds, ctx.style().colors[ControlColor::WindowBG as usize]);
        let font = ctx.style().resolve_font_choice(self.font);

        for (index, row) in self.entry_rects(bounds, ctx.style(), ctx.atlas()).into_iter().enumerate() {
            let Some(item) = self.entries[index].as_item() else {
                // Center a one-pixel rule in the separator allocation while retaining horizontal
                // padding so adjacent popup frames remain visually distinct.
                let padding = ctx.style().padding.max(1);
                let rule = Recti::new(
                    row.x.saturating_add(padding),
                    row.y.saturating_add(row.height / 2),
                    row.width.saturating_sub(padding.saturating_mul(2)).max(0),
                    1,
                );
                ctx.draw_rect(rule, ctx.style().colors[ControlColor::Border as usize]);
                continue;
            };

            if self.selected == Some(index) {
                ctx.draw_rect(row, ctx.style().colors[ControlColor::ButtonHover as usize]);
            }

            let (marker, label, shortcut) = Self::row_columns(row, ctx.style(), ctx.atlas());
            Self::paint_marker(ctx, marker, item.marker(), item.is_enabled());
            let color = if item.is_enabled() {
                ctx.style().colors[ControlColor::Text as usize]
            } else {
                Self::disabled_text_color(ctx.style())
            };
            ctx.draw_control_text_color_with_font(font, item.label(), label, color, WidgetOption::NONE);
            if let Some(hint) = item.shortcut_hint() {
                ctx.draw_control_text_color_with_font(font, hint, shortcut, color, WidgetOption::ALIGN_RIGHT);
            }
        }
    }
}
