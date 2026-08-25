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

//! Concrete retained presentation widgets used by declarative window menus.
//!
//! [`MenuItem`] is the sole public widget in this module and publishes its own typed command event.
//! The remaining types are private framework surfaces: the window manager matches their retained
//! node IDs directly and derives their open highlight from its one active popup path.

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

/// Non-interactive horizontal surface that owns every top-level menu heading.
pub(crate) struct MenuBarSurface;

impl MenuBarSurface {
    /// Transfers ordered heading nodes into one background-painting bar container.
    pub(crate) fn create(headings: impl IntoIterator<Item = Node>) -> Node {
        // The generic Container remains the sole strong owner; this zero-sized widget stores no
        // duplicate label, geometry, hover, or open-menu state.
        Node::container(Container::new(Self, headings).1)
    }
}

impl ContainerWidget for MenuBarSurface {
    /// Measures one compact horizontal line from its concrete heading children.
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        // Each heading owns its text measurement. Summing those preferred widths removes the old
        // bar-local label vector and manual heading-rectangle reconstruction.
        let child_constraints = Constraints::new(AvailableSpace::Unbounded, constraints.height);
        let mut preferred = Dimensioni::default();
        for index in 0..ctx.child_count() {
            let child = ctx.measure_child(index, child_constraints).unwrap_or_default();
            preferred.width = preferred.width.saturating_add(child.width.max(0));
            preferred.height = preferred.height.max(child.height.max(0));
        }
        preferred
    }

    /// Places headings left-to-right at their intrinsic widths across the assigned bar height.
    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // The parent stretches the surface to the window body width. Only the occupied heading extent
        // is published as logical content; the surface itself still paints the unoccupied remainder.
        let constraints = Constraints::new(AvailableSpace::Unbounded, AvailableSpace::bounded(rect.height));
        let mut x = rect.x;
        let mut content_height = rect.height.max(0);
        for index in 0..children.len() {
            let preferred = ctx.measure_child(children, index, constraints).unwrap_or_default();
            let child = Recti::new(x, rect.y, preferred.width.max(0), rect.height.max(0));
            let _ = ctx.layout_child(children, index, child);
            x = x.saturating_add(child.width);
            content_height = content_height.max(preferred.height.max(0));
        }
        ctx.set_content_size(Dimensioni::new(x.saturating_sub(rect.x), content_height));
    }
}

impl Widget for MenuBarSurface {
    /// Makes the bar background transparent to routing while leaving heading children interactive.
    fn widget_opt(&self) -> &WidgetOption {
        // NO_INTERACT affects only this container's own surface; the router still visits its children.
        &WidgetOption::NO_INTERACT
    }

    /// Performs no semantic update because the manager acts on routed heading node IDs directly.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // Hover and click state live on the individual heading nodes, not on their background parent.
    }

    /// Paints the complete persistent bar before its heading children paint labels and highlights.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Filling the assigned width keeps the bar visually part of the window even after its labels end.
        ctx.draw_rect(ctx.local_rect(), ctx.style().menu_background);
    }
}

/// One top-level heading whose identity directly activates its compiled popup.
pub(crate) struct MenuHeading {
    /// Immutable user-visible label transferred from the declarative menu.
    label: String,
    /// Body font used consistently for preferred measurement and paint.
    font: FontChoice,
    /// Paint-only highlight derived from the manager-owned active popup path.
    open: bool,
}

impl MenuHeading {
    /// Creates one independently routed heading and a weak handle for derived highlight updates.
    pub(crate) fn create(label: String) -> (TypedWidgetHandle<Self>, Node) {
        // The returned node supplies stable geometry and identity; the weak handle owns no lifecycle.
        Node::typed_widget(Self {
            label,
            font: FontChoice::Role(FontRole::Body),
            open: false,
        })
    }

    /// Replaces only the path-derived open presentation bit.
    pub(crate) const fn set_open(&mut self, open: bool) {
        // This state never decides popup visibility and therefore requires no event or reconciliation port.
        self.open = open;
    }

    /// Returns the derived highlight for compiler tests.
    #[cfg(test)]
    pub(crate) const fn is_open(&self) -> bool {
        // Expose no production query because the window manager already owns the authoritative path.
        self.open
    }
}

impl LeafWidget for MenuHeading {
    /// Measures exactly one padded label cell.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // Per-heading measurement lets ordinary retained layout own every clickable rectangle.
        let padding = heading_horizontal_padding(style);
        let font = style.resolve_font_choice(self.font);
        let width = atlas.get_text_size(font, &self.label).width.max(0).saturating_add(padding.saturating_mul(2));
        Dimensioni::new(width, menu_row_height(style, atlas, self.font))
    }
}

impl Widget for MenuHeading {
    /// Preserves the application's existing keyboard focus while operating the menu bar.
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::PRESERVE_FOCUS
    }

    /// Performs no local action because the manager consumes this node's routed identity.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // Generic routing still commits hover, click, and capture snapshots used by paint and policy.
    }

    /// Paints manager-derived open state, runtime hover state, and the immutable heading label.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        if self.open {
            ctx.draw_rect(bounds, ctx.style().colors[ControlColor::ButtonFocus as usize]);
        } else if ctx.hovered() || ctx.focused() {
            ctx.draw_rect(bounds, ctx.style().colors[ControlColor::ButtonHover as usize]);
        }
        let font = ctx.style().resolve_font_choice(self.font);
        ctx.draw_control_text_color_with_font(font, &self.label, bounds, ctx.style().menu_foreground, WidgetOption::NONE);
    }
}

/// Private relational trigger for one already-compiled child menu popup.
pub(crate) struct MenuSubmenu {
    /// Immutable user-visible label transferred from the recursive menu declaration.
    label: String,
    /// Body font used consistently for preferred measurement and paint.
    font: FontChoice,
    /// Paint-only highlight derived from the manager-owned active popup path.
    open: bool,
}

impl MenuSubmenu {
    /// Creates one independently routed row and a weak handle for derived highlight updates.
    pub(crate) fn create(label: String) -> (TypedWidgetHandle<Self>, Node) {
        // Runtime node identity supplies both interaction matching and right-edge placement, so the
        // widget needs no event port, screen-space anchor calculation, or popup reference.
        Node::typed_widget(Self {
            label,
            font: FontChoice::Role(FontRole::Body),
            open: false,
        })
    }

    /// Replaces only the path-derived open presentation bit.
    pub(crate) const fn set_open(&mut self, open: bool) {
        // The active popup path remains authoritative; this value only selects a paint color.
        self.open = open;
    }

    /// Returns the derived highlight for compiler tests.
    #[cfg(test)]
    pub(crate) const fn is_open(&self) -> bool {
        // Expose no production query because the window manager already owns the authoritative path.
        self.open
    }
}

impl LeafWidget for MenuSubmenu {
    /// Measures one marker-aligned label row plus its trailing expansion glyph.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // Share the marker column with ordinary MenuItem rows so mixed popup contents align exactly.
        let padding = style.padding.max(1);
        let marker_width = atlas.get_icon_size(style.icons.check).width.max(0);
        let arrow_width = atlas.get_icon_size(style.icons.expand).width.max(0);
        let font = style.resolve_font_choice(self.font);
        let label_width = atlas.get_text_size(font, &self.label).width.max(0);
        let width = padding
            .saturating_add(marker_width)
            .saturating_add(padding)
            .saturating_add(label_width)
            .saturating_add(padding)
            .saturating_add(arrow_width)
            .saturating_add(padding);
        Dimensioni::new(width, menu_row_height(style, atlas, self.font))
    }
}

impl Widget for MenuSubmenu {
    /// Preserves the application's existing keyboard focus while operating a cascading menu.
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::PRESERVE_FOCUS
    }

    /// Performs no local action because the manager consumes this node's routed identity.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // Generic routing still commits hover, click, and capture snapshots used by paint and policy.
    }

    /// Paints the derived open state, runtime hover state, label, and expansion glyph.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        if self.open {
            ctx.draw_rect(bounds, ctx.style().colors[ControlColor::ButtonFocus as usize]);
        } else if ctx.hovered() || ctx.focused() {
            ctx.draw_rect(bounds, ctx.style().colors[ControlColor::ButtonHover as usize]);
        }

        // Retain the established marker, label, and right-arrow columns used by ordinary menu rows.
        let style = *ctx.style();
        let padding = style.padding.max(1);
        let marker_width = ctx.atlas().get_icon_size(style.icons.check).width.max(0);
        let arrow_size = ctx.atlas().get_icon_size(style.icons.expand);
        let text_x = bounds.x.saturating_add(padding).saturating_add(marker_width);
        let arrow = Recti::new(
            bounds.x.saturating_add(bounds.width).saturating_sub(padding).saturating_sub(arrow_size.width),
            bounds.y.saturating_add((bounds.height - arrow_size.height) / 2),
            arrow_size.width,
            arrow_size.height,
        );
        let text = Recti::new(text_x, bounds.y, arrow.x.saturating_sub(text_x).max(0), bounds.height);
        let font = style.resolve_font_choice(self.font);
        ctx.draw_control_text_color_with_font(font, &self.label, text, style.menu_foreground, WidgetOption::NONE);
        ctx.draw_icon(style.icons.expand, arrow, style.menu_foreground);
    }
}

/// Zero-gap vertical menu surface shared by top-level popup menus and every submenu.
pub(crate) struct MenuList;

impl MenuList {
    /// Transfers ordered rows into one background-painting retained container.
    pub(crate) fn create(rows: impl IntoIterator<Item = Node>) -> Node {
        // The concrete container becomes the sole strong owner of every item, separator, and trigger.
        Node::container(Container::new(Self, rows).1)
    }
}

impl ContainerWidget for MenuList {
    /// Measures a zero-gap vertical stack at the widest row's preferred width.
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        // Retain the caller's horizontal constraint but leave height unbounded so every row contributes.
        let child_constraints = Constraints::new(constraints.width, AvailableSpace::Unbounded);
        let mut preferred = Dimensioni::default();
        for index in 0..ctx.child_count() {
            let child = ctx.measure_child(index, child_constraints).unwrap_or_default();
            preferred.width = preferred.width.max(child.width);
            preferred.height = preferred.height.saturating_add(child.height);
        }
        preferred
    }

    /// Places every row at the shared popup width in declaration order.
    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Re-measure at the exact shared width so responsive rows publish authoritative heights.
        let constraints = Constraints::new(AvailableSpace::bounded(rect.width), AvailableSpace::Unbounded);
        let mut y = rect.y;
        let mut content_width = 0;
        for index in 0..children.len() {
            let preferred = ctx.measure_child(children, index, constraints).unwrap_or_default();
            let child = Recti::new(rect.x, y, rect.width.max(0), preferred.height.max(0));
            let _ = ctx.layout_child(children, index, child);
            y = y.saturating_add(child.height);
            content_width = content_width.max(preferred.width);
        }
        ctx.set_content_size(Dimensioni::new(content_width.max(rect.width), y.saturating_sub(rect.y)));
    }
}

impl Widget for MenuList {
    /// Makes only the list background transparent while its actionable child rows remain routable.
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    /// Performs no update because row widgets and manager policy own all menu interaction.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // The list owns layout and background presentation only.
    }

    /// Paints one uninterrupted popup background before its rows paint.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Children paint after this surface, so their hover and open highlights remain visible.
        ctx.draw_rect(ctx.local_rect(), ctx.style().menu_background);
    }
}

/// Visual state displayed in the fixed marker column of one menu item.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MenuItemMark {
    /// The item has no persistent marker.
    None,
    /// A checkable item whose application-managed boolean controls check-glyph visibility.
    Checked(bool),
    /// A radio item whose application-managed boolean controls selection-marker visibility.
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
    ///
    /// This does not register a shortcut or cause keyboard input to submit the item.
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
    /// Preserves keyboard focus and adds `NO_INTERACT` while the item is disabled.
    opt: WidgetOption,
    /// Runtime-owned source registered independently for this item.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<MenuItemSubmitted>>>,
}

impl MenuItem {
    /// Constructs one retained item node and its weak typed handle.
    pub fn create(parameters: MenuItemParameters) -> (TypedWidgetHandle<Self>, Node) {
        // Disabled state must participate in target selection, not merely suppress a late event. Store
        // NO_INTERACT in the same options queried by the router before the node can become observable.
        let mut opt = WidgetOption::PRESERVE_FOCUS;
        if !parameters.enabled {
            opt.insert(WidgetOption::NO_INTERACT);
        }
        // The node is the sole strong owner; registration and live updates use only the weak handle.
        Node::typed_widget(Self {
            label: parameters.label,
            enabled: parameters.enabled,
            mark: parameters.mark,
            shortcut_hint: parameters.shortcut_hint,
            font: parameters.font,
            opt,
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
    pub fn set_enabled(&mut self, enabled: bool) {
        // Update semantic and router-visible state together so disabled rows cannot become the
        // manager's post-update menu invocation target or retain transient hover/click snapshots.
        self.enabled = enabled;
        if enabled {
            self.opt.remove(WidgetOption::NO_INTERACT);
        } else {
            self.opt.insert(WidgetOption::NO_INTERACT);
        }
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
        let mut color = style.menu_foreground;
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
    /// Measures the marker, label, optional shortcut hint, and their fixed padding columns.
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
    /// Returns the focus-preserving options, including dynamic disabled routing policy.
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    /// Emits this item's native typed event exactly once for an enabled left-button click transition.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // NO_INTERACT prevents disabled routing; retain the semantic guard in case another widget
        // disables this item after target selection but before its turn in the update traversal.
        if self.enabled && ctx.clicked() {
            self.submitted_event.borrow_mut().emit(MenuItemSubmitted);
        }
    }

    /// Paints live marker, label, shortcut, enabled color, and ordinary runtime hover state.
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

/// Explicit non-interactive rule inserted by one declarative [`crate::Menu`].
pub(crate) struct MenuSeparator;

impl MenuSeparator {
    /// Creates one private retained separator node.
    pub(crate) fn create() -> Node {
        // No typed handle or stored option is needed because a separator has no mutable state.
        Node::typed_widget(Self).1
    }
}

impl LeafWidget for MenuSeparator {
    /// Reserves a compact vertical gap for one centered rule.
    fn measure(&self, style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // Keep the rule legible even when a theme requests zero spacing.
        Dimensioni::new(0, style.spacing.max(3))
    }
}

impl Widget for MenuSeparator {
    /// Prevents the visual rule from becoming an invocation target.
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    /// Performs no update because the separator is purely presentational.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        // NO_INTERACT also guarantees the runtime never routes a concrete input event here.
    }

    /// Paints a subdued one-pixel rule centered in the assigned row.
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
        let mut color = ctx.style().menu_foreground;
        color.a = ((u16::from(color.a) * 45) / 100).max(1) as u8;
        ctx.draw_rect(rule, color);
    }
}
