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

//! Compact declarative window menus.
//!
//! A menu bar compiles to one retained leaf for the bar and one retained leaf for each popup. Each
//! leaf measures, hit-tests, anchors, and paints all of its logical slots from the same leaf-local
//! data. Menu rows are therefore values rather than retained widget subtrees: there is no per-row
//! container, interaction node, or three-cell presentation tree.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use crate::ui_node::RuntimeNodeId;
use crate::ui_node::widgets::content_height;
use crate::{
    AtlasHandle, Color, Constraints, ControlColor, Dimensioni, FontChoice, FontRole, LeafWidget, Linear, LinearItem, LinearParameters, MouseButton, Node,
    Recti, Style, TypedWidgetHandle, UiInputEvent, Widget, WidgetEventPortHandle, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx,
};

#[cfg(test)]
mod tests;

/// Compact index of one parent-first compiled popup.
type MenuId = usize;

/// Smallest marker column that can contain the atlas-independent radio fallback.
const MIN_MARKER_COLUMN_WIDTH: i32 = 3;

/// Visual state displayed in the optional marker gutter of one menu item.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MenuItemMark {
    /// The item has no persistent marker role.
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
    /// Font used by both the label and accelerator text.
    pub font: FontChoice,
}

impl MenuItemParameters {
    /// Creates an enabled, unmarked item without a shortcut hint.
    pub fn new(label: impl Into<String>) -> Self {
        // Keep defaults explicit so adding a field cannot silently change the public constructor.
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
        // Builder methods mutate only the not-yet-shared declaration value.
        self.enabled = enabled;
        self
    }

    /// Makes the item initially disabled.
    pub const fn disabled(self) -> Self {
        // Delegate to the general builder so initialization has one implementation.
        self.enabled(false)
    }

    /// Replaces the initial marker state.
    pub const fn mark(mut self, mark: MenuItemMark) -> Self {
        // The role is retained when its boolean is false so alignment remains stable.
        self.mark = mark;
        self
    }

    /// Configures an initially checked or unchecked item.
    pub const fn checked(self, checked: bool) -> Self {
        // Preserve the checkable role independently from its selected value.
        self.mark(MenuItemMark::Checked(checked))
    }

    /// Configures an initially selected or unselected radio item.
    pub const fn radio(self, selected: bool) -> Self {
        // Preserve the radio role independently from its selected value.
        self.mark(MenuItemMark::Radio(selected))
    }

    /// Adds presentation-only accelerator text such as `Ctrl+O`.
    ///
    /// This does not register a keyboard shortcut or submit the item from keyboard input.
    pub fn shortcut_hint(mut self, shortcut_hint: impl Into<String>) -> Self {
        // Store display text only; application shortcut policy remains outside menu presentation.
        self.shortcut_hint = Some(shortcut_hint.into());
        self
    }

    /// Replaces the font used by this item.
    pub const fn font(mut self, font: FontChoice) -> Self {
        // One font choice drives measurement and paint for both strings.
        self.font = font;
        self
    }
}

/// Event emitted by one specific menu item after an enabled pointer submission.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct MenuItemSubmitted;

impl crate::WidgetEvent for MenuItemSubmitted {}

/// Mutable semantic state shared by one menu definition and its weak application handle.
struct MenuItemState {
    /// Mutable user-visible label.
    label: String,
    /// Mutable interaction state.
    enabled: bool,
    /// Mutable check or radio presentation.
    mark: MenuItemMark,
    /// Mutable presentation-only accelerator text.
    shortcut_hint: Option<String>,
    /// Font used consistently for measurement and paint.
    font: FontChoice,
    /// Strong source of this item's independently subscribable typed event.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<MenuItemSubmitted>>>,
    /// Weak invalidation link installed when the item enters its unique popup surface.
    owner: Option<TypedWidgetHandle<MenuSurface>>,
}

/// Uniquely owned declaration value for one actionable menu row.
///
/// Move this value into [`Menu::item`]. Keep the separately returned [`MenuItemHandle`] when the
/// application needs to subscribe or change live presentation state after compilation into a leaf.
pub struct MenuItem {
    /// Sole strong state owner until this declaration moves into a compiled popup leaf.
    state: Rc<RefCell<MenuItemState>>,
}

impl MenuItem {
    /// Creates one uniquely owned item declaration and its weak application capability.
    pub fn create(parameters: MenuItemParameters) -> (MenuItemHandle, Self) {
        // Allocate semantic state once; no retained row or visual-cell allocations are created.
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let state = Rc::new(RefCell::new(MenuItemState {
            label: parameters.label,
            enabled: parameters.enabled,
            mark: parameters.mark,
            shortcut_hint: parameters.shortcut_hint,
            font: parameters.font,
            submitted_event: submitted_event.clone(),
            owner: None,
        }));
        let handle = MenuItemHandle {
            state: Rc::downgrade(&state),
            submitted: WidgetEventPortHandle::new(&submitted_event),
        };
        (handle, Self { state })
    }
}

/// Cloneable weak access to one item retained by a compiled menu.
#[derive(Clone)]
pub struct MenuItemHandle {
    /// Weak semantic access that cannot keep an unmounted or destroyed menu alive.
    state: Weak<RefCell<MenuItemState>>,
    /// Weak native event capability returned without borrowing semantic state.
    submitted: WidgetEventPortHandle<MenuItemSubmitted>,
}

impl MenuItemHandle {
    /// Returns whether a declaration or compiled menu still owns this item.
    pub fn is_alive(&self) -> bool {
        // Observe liveness without temporarily retaining the item allocation.
        self.state.strong_count() != 0
    }

    /// Returns this specific item's native submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<MenuItemSubmitted> {
        // The returned clone remains weak and preserves item ownership semantics.
        self.submitted.clone()
    }

    /// Returns a snapshot of the current user-visible label while the item is alive.
    pub fn label(&self) -> Option<String> {
        // Clone the presentation value so no RefCell borrow escapes this method.
        self.read(|state| state.label.clone())
    }

    /// Replaces the user-visible label without emitting a submission.
    pub fn set_label(&self, label: impl Into<String>) -> Option<()> {
        // A label mutation always requests fresh popup measurement.
        self.update(label.into(), |state, label| state.label = label)
    }

    /// Returns the current enabled state while the item is alive.
    pub fn is_enabled(&self) -> Option<bool> {
        // Copy the boolean while holding only a brief immutable borrow.
        self.read(|state| state.enabled)
    }

    /// Enables or disables this specific item immediately.
    pub fn set_enabled(&self, enabled: bool) -> Option<()> {
        // The compact surface consults this value directly during action resolution and paint.
        self.update(enabled, |state, enabled| state.enabled = enabled)
    }

    /// Returns the current check or radio presentation while the item is alive.
    pub fn mark(&self) -> Option<MenuItemMark> {
        // Copy the marker so no shared-state borrow crosses the API boundary.
        self.read(|state| state.mark)
    }

    /// Replaces this item's check or radio presentation.
    pub fn set_mark(&self, mark: MenuItemMark) -> Option<()> {
        // Changing marker role may add or remove the popup's shared marker gutter.
        self.update(mark, |state, mark| state.mark = mark)
    }

    /// Returns the optional shortcut snapshot while this item remains alive.
    pub fn shortcut_hint(&self) -> Option<Option<String>> {
        // The outer option reports handle access; the inner option preserves an absent live hint.
        self.read(|state| state.shortcut_hint.clone())
    }

    /// Replaces presentation-only shortcut text without registering a keyboard shortcut.
    pub fn set_shortcut_hint(&self, shortcut_hint: Option<String>) -> Option<()> {
        // Accelerator width participates in shared text-region measurement.
        self.update(shortcut_hint, |state, shortcut_hint| state.shortcut_hint = shortcut_hint)
    }

    /// Reads one copied or cloned property without exposing the shared-state borrow.
    fn read<T>(&self, read: impl FnOnce(&MenuItemState) -> T) -> Option<T> {
        // Upgrade and borrow only for the callback so dead or currently mutating items return `None`.
        let state = self.state.upgrade()?;
        let state = state.try_borrow().ok()?;
        Some(read(&state))
    }

    /// Applies one typed state mutation and invalidates the unique owning popup surface.
    fn update<T>(&self, value: T, update: impl FnOnce(&mut MenuItemState, T)) -> Option<()> {
        // End the state borrow before invalidating; one conservative measurement path keeps all
        // property mutations correct without a second classification or invalidation protocol.
        let state = self.state.upgrade()?;
        let owner = {
            let mut state = state.try_borrow_mut().ok()?;
            update(&mut state, value);
            state.owner.clone()
        };
        if let Some(owner) = owner {
            // Dirty the leaf and its ancestor path so automatic popup sizing remains authoritative.
            let invalidated = owner.try_update(|_| ());
            debug_assert!(invalidated.is_some(), "a retained item must not outlive its menu surface");
        }
        Some(())
    }
}

/// Complete declarative menu bar installed intrinsically into one window.
pub struct MenuBar {
    /// Top-level menus retained in left-to-right heading order.
    menus: Vec<Menu>,
}

impl MenuBar {
    /// Creates a bar from uniquely owned top-level menu descriptions.
    pub fn new(menus: impl IntoIterator<Item = Menu>) -> Self {
        // Collect once at the ownership boundary; compilation consumes this vector without clones.
        Self { menus: menus.into_iter().collect() }
    }

    /// Compiles declarations into one bar leaf, one leaf per popup, and one controller.
    pub(crate) fn compile(self, window_name: &str, content: Node) -> (Node, Vec<CompiledMenuPopup>, MenuController) {
        // Flatten hierarchy once. Each resulting popup later moves its rows into its own leaf, so
        // the controller shares only one tiny pending-action cell rather than a second topology.
        let mut flat = Vec::new();
        let mut headings = Vec::with_capacity(self.menus.len());
        for (slot, menu) in self.menus.into_iter().enumerate() {
            let (target, label) = flatten_menu(menu, None, slot, &mut flat);
            headings.push(MenuSlot::Branch { label, target });
        }
        let pending = Rc::new(RefCell::new(None));

        // The bar remains an ordinary child of a two-item vertical shell above application content.
        let (bar_handle, bar_node) = Node::typed_widget(MenuSurface::new(headings, false, pending.clone()));
        let bar_surface = MenuSurfaceRef { node: bar_node.id(), handle: bar_handle };
        let (_, shell) = Linear::create(LinearParameters::vertical([LinearItem::content(bar_node), LinearItem::flex(content, 1.0)]));

        // Menus are parent-first, so every child anchor can clone its parent surface immediately.
        let mut popup_surfaces: Vec<MenuSurfaceRef> = Vec::with_capacity(flat.len());
        let mut popups = Vec::with_capacity(flat.len());
        for menu in flat {
            let FlatMenu { label, parent, trigger_slot, rows } = menu;
            let (surface_handle, surface_node) = Node::typed_widget(MenuSurface::new(rows, true, pending.clone()));
            let surface = MenuSurfaceRef {
                node: surface_node.id(),
                handle: surface_handle,
            };

            // Parent-first flattening guarantees that a child can clone its source surface here.
            let source = match parent {
                Some(parent) => &popup_surfaces[parent],
                None => &bar_surface,
            };
            let anchor = MenuAnchor {
                node: source.node,
                surface: source.handle.clone(),
                slot: trigger_slot,
            };

            // Attach each item through the just-created surface without allocating backlink arrays.
            let attached = surface.handle.try_read(|menu| menu.attach_items(&surface.handle));
            debug_assert!(attached.is_some(), "a new menu surface must remain alive during compilation");
            popup_surfaces.push(surface);
            popups.push(CompiledMenuPopup {
                parent,
                name: format!("{window_name} {label} Menu"),
                anchor,
                content: surface_node,
            });
        }

        let controller = MenuController {
            pending,
            #[cfg(test)]
            popups: popup_surfaces,
        };
        // A tuple avoids retaining a one-use transport wrapper at the manager ownership seam.
        (shell, popups, controller)
    }
}

/// One top-level menu or recursively nested submenu.
pub struct Menu {
    /// User-visible heading or submenu-row label.
    label: String,
    /// Ordered logical entries transferred into compact popup leaves.
    entries: Vec<MenuEntry>,
}

impl Menu {
    /// Creates an empty menu with the supplied user-visible label.
    pub fn new(label: impl Into<String>) -> Self {
        // Entries remain declaration data until a window compiles the complete bar.
        Self { label: label.into(), entries: Vec::new() }
    }

    /// Appends one uniquely owned actionable item.
    pub fn item(mut self, item: MenuItem) -> Self {
        // Moving the item prevents accidental reuse and removes public handle/node pairs.
        self.entries.push(MenuEntry::Item(item));
        self
    }

    /// Appends one explicit non-interactive separator row.
    pub fn separator(mut self) -> Self {
        // Preserve exact declaration order; compilation performs no implicit grouping.
        self.entries.push(MenuEntry::Separator);
        self
    }

    /// Appends one recursively composed submenu.
    pub fn submenu(mut self, submenu: Menu) -> Self {
        // The child remains uniquely owned and is flattened only during window registration.
        self.entries.push(MenuEntry::Submenu(submenu));
        self
    }
}

/// One ordered logical row in a declarative menu.
enum MenuEntry {
    /// Application-authored actionable item.
    Item(MenuItem),
    /// Explicit visual rule.
    Separator,
    /// Recursively owned child menu opened from this row.
    Submenu(Menu),
}

/// Temporary parent-first popup record consumed during surface construction.
struct FlatMenu {
    /// Direct label used to name the popup surface.
    ///
    /// Keeping only the direct label avoids retaining and formatting recursive diagnostic paths;
    /// [`MenuId`] remains the authoritative, unambiguous runtime identity.
    label: String,
    /// Direct parent menu, or `None` for a bar heading.
    parent: Option<MenuId>,
    /// Heading index or parent-row index that triggers this menu.
    trigger_slot: usize,
    /// Direct compact rows moved into this popup's sole leaf.
    rows: Vec<MenuSlot>,
}

/// Compact runtime representation of one bar heading or popup row.
enum MenuSlot {
    /// Shared live item state owned strongly by this row.
    Item(Rc<RefCell<MenuItemState>>),
    /// Non-interactive visual rule.
    Separator,
    /// Label and indexed popup opened by either a bar heading or submenu row.
    Branch {
        /// Immutable user-visible branch label.
        label: String,
        /// Compiled popup index staged when this slot is pressed.
        target: MenuId,
    },
}

/// Flattens one declaration and returns its stable index plus its presentation label.
fn flatten_menu(menu: Menu, parent: Option<MenuId>, trigger_slot: usize, flat: &mut Vec<FlatMenu>) -> (MenuId, String) {
    // Reserve the parent index before recursion, then return each consumed label to its caller so
    // exactly one leaf owns it: the bar for top-level menus or the parent row for child menus.
    let Menu { label, entries } = menu;
    let id = flat.len();
    flat.push(FlatMenu {
        label: label.clone(),
        parent,
        trigger_slot,
        rows: Vec::new(),
    });

    let mut rows = Vec::with_capacity(entries.len());
    for (slot, entry) in entries.into_iter().enumerate() {
        rows.push(match entry {
            MenuEntry::Item(item) => MenuSlot::Item(item.state),
            MenuEntry::Separator => MenuSlot::Separator,
            MenuEntry::Submenu(child) => {
                let (target, label) = flatten_menu(child, Some(id), slot, flat);
                MenuSlot::Branch { label, target }
            }
        });
    }
    flat[id].rows = rows;
    (id, label)
}

/// Semantic action transferred from a menu surface after its widget update finishes.
pub(crate) enum MenuAction {
    /// Open or extend the active path to one compiled popup index.
    Open(MenuId),
    /// Close the path and emit through one enabled item's short-lived native endpoint.
    Invoke(Rc<RefCell<crate::event::WidgetEventPort<MenuItemSubmitted>>>),
}

/// Shared output of menu measurement, hit testing, anchoring, and paint geometry.
#[derive(Default)]
struct MenuSurfaceLayout {
    /// Preferred content extent.
    size: Dimensioni,
    /// Heading or row rectangles in declaration order.
    slots: Vec<Recti>,
    /// Width before the shared control-text region in popup rows.
    marker_width: i32,
}

/// The only retained widget type used to present an entire menu bar or popup.
struct MenuSurface {
    /// Bar headings or popup rows owned directly by this one retained leaf.
    rows: Vec<MenuSlot>,
    /// Whether rows flow vertically and paint popup-only marker and arrow decoration.
    popup: bool,
    /// Tiny action cell shared with sibling menu leaves and the owning window manager.
    pending: Rc<RefCell<Option<MenuAction>>>,
    /// Slot currently under the pointer, or `None` when no logical row is hovered.
    ///
    /// Absence is represented structurally so no valid future slot index can collide with an
    /// out-of-band sentinel value.
    hovered_slot: Option<usize>,
    /// Slot whose child popup belongs to the active path, or `None` when this surface has no open
    /// child.
    ///
    /// This is paint-only state: popup ownership and the authoritative active path remain in the
    /// window manager.
    open_slot: Option<usize>,
    /// Latest retained geometry shared by update, anchoring, and paint.
    geometry: RefCell<MenuSurfaceLayout>,
}

impl MenuSurface {
    /// Creates one compact retained presentation leaf.
    fn new(rows: Vec<MenuSlot>, popup: bool, pending: Rc<RefCell<Option<MenuAction>>>) -> Self {
        // All presentation data stays leaf-local; only one action slot crosses the manager seam.
        Self {
            rows,
            popup,
            pending,
            hovered_slot: None,
            open_slot: None,
            geometry: RefCell::new(MenuSurfaceLayout::default()),
        }
    }

    /// Installs this leaf as the weak invalidation owner of each direct item row.
    fn attach_items(&self, owner: &TypedWidgetHandle<Self>) {
        // Unique declarations guarantee no prior owner; bar slots contain no item variants.
        for row in &self.rows {
            if let MenuSlot::Item(item) = row {
                let previous = item.borrow_mut().owner.replace(owner.clone());
                debug_assert!(previous.is_none(), "a menu item can belong to only one popup");
            }
        }
    }

    /// Resolves one local slot into a manager-consumed semantic action.
    fn action_for(&self, slot: usize) -> Option<MenuAction> {
        // Disabled items and separators intentionally produce no action and leave the path open.
        match self.rows.get(slot)? {
            MenuSlot::Branch { target, .. } => Some(MenuAction::Open(*target)),
            MenuSlot::Item(item) => {
                let item = item.borrow();
                // No application dispatch can intervene between this check and manager emission.
                item.enabled.then(|| MenuAction::Invoke(item.submitted_event.clone()))
            }
            MenuSlot::Separator => None,
        }
    }

    /// Measures all logical slots and caches the exact geometry used by later surface phases.
    ///
    /// This ordinary method is the concrete menu implementation. The retained-widget trait below
    /// is intentionally only an adapter while menu surfaces still travel through `Node`.
    fn measure_surface(&self, style: &Style, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // Menus keep intrinsic slot geometry: Linear assigns the bar its desired height, while the
        // manager auto-sizes and viewport-clips popups. Cache those same rectangles for anchors.
        let layout = if self.popup {
            layout_popup(&self.rows, style, atlas)
        } else {
            layout_bar(&self.rows, style, atlas)
        };
        let size = layout.size;
        *self.geometry.borrow_mut() = layout;
        size
    }

    /// Applies one routed input event to hover presentation and the manager action bridge.
    ///
    /// Keeping this behavior on the concrete surface makes pointer policy reusable when the window
    /// manager owns menu bodies directly; the widget adapter contributes no independent state.
    fn update_surface(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Losing root-local hover clears row presentation even when this widget receives no event.
        if !ctx.hovered() {
            self.hovered_slot = None;
        }
        let Some(input) = input else { return };
        let Some(position) = input.position() else { return };
        // Reuse clip-aware runtime hit testing against the exact rectangles retained by measure.
        let slot = self.geometry.borrow().slots.iter().position(|slot| ctx.mouse_over(*slot, position));
        self.hovered_slot = slot;

        // Only the left press commits menu policy; drag and release remain capture tail.
        if matches!(input, UiInputEvent::MouseDown { button, .. } if button.intersects(MouseButton::LEFT)) {
            // Disabled and separator slots overwrite with `None`, so no stale action can survive.
            *self.pending.borrow_mut() = slot.and_then(|slot| self.action_for(slot));
        }
    }

    /// Paints the complete bar or popup from the geometry shared with measurement and hit testing.
    ///
    /// Drawing remains a single background fill plus one logical-slot loop. This method deliberately
    /// accepts the existing concrete paint context so extraction changes ownership seams, not pixels.
    fn paint_surface(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // One uninterrupted fill and one slot loop replace container, row, and cell paint passes.
        let style = *ctx.style();
        let layout = self.geometry.borrow();
        ctx.draw_rect(ctx.local_rect(), style.menu_background);
        for (slot, entry) in self.rows.iter().enumerate() {
            let row = layout.slots[slot];
            match entry {
                MenuSlot::Item(item) => {
                    let item = item.borrow();
                    if item.enabled && self.hovered_slot == Some(slot) {
                        ctx.draw_rect(row, style.colors[ControlColor::ButtonHover as usize]);
                    }
                    let marker = Recti::new(row.x, row.y, layout.marker_width.max(0), row.height);
                    let text = text_region(row, layout.marker_width);
                    // Preserve the theme hue while subduing disabled rows through opacity alone.
                    let mut color = style.menu_foreground;
                    if !item.enabled {
                        color.a = ((u16::from(color.a) * 45) / 100).max(1) as u8;
                    }
                    paint_item_marker(ctx, marker, item.mark, color);
                    let font = style.resolve_font_choice(item.font);
                    // Both strings share one clip so control padding is applied exactly once.
                    ctx.draw_control_text_color_with_font(font, &item.label, text, color, WidgetOption::NONE);
                    if let Some(hint) = &item.shortcut_hint {
                        ctx.draw_control_text_color_with_font(font, hint, text, color, WidgetOption::ALIGN_RIGHT);
                    }
                }
                MenuSlot::Separator => paint_separator(ctx, row),
                MenuSlot::Branch { label, .. } => {
                    if self.open_slot == Some(slot) {
                        ctx.draw_rect(row, style.colors[ControlColor::ButtonFocus as usize]);
                    } else if self.hovered_slot == Some(slot) {
                        ctx.draw_rect(row, style.colors[ControlColor::ButtonHover as usize]);
                    }
                    // Bar headings use their full slot; popup branches reserve the marker gutter.
                    let text = if self.popup { text_region(row, layout.marker_width) } else { row };
                    let font = style.resolve_font_choice(FontChoice::Role(FontRole::Body));
                    ctx.draw_control_text_color_with_font(font, label, text, style.menu_foreground, WidgetOption::NONE);
                    if self.popup {
                        paint_submenu_arrow(ctx, text);
                    }
                }
            }
        }
    }
}

impl LeafWidget for MenuSurface {
    /// Adapts retained-tree measurement to the concrete menu-surface implementation.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni {
        // Keep the compatibility seam behavior-free so direct manager ownership can remove it later.
        self.measure_surface(style, atlas, constraints)
    }
}

impl Widget for MenuSurface {
    /// Preserves application keyboard focus during menu pointer interaction.
    fn widget_opt(&self) -> &WidgetOption {
        // The whole leaf is interactive; disabled logical rows are filtered during action resolution.
        &WidgetOption::PRESERVE_FOCUS
    }

    /// Adapts retained-tree input routing to the concrete menu-surface implementation.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // All semantic and presentation behavior belongs to the ordinary surface method above.
        self.update_surface(ctx, input);
    }

    /// Adapts retained-tree painting to the concrete menu-surface implementation.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Keeping this delegate trivial prevents the temporary widget seam from diverging visually.
        self.paint_surface(ctx);
    }
}

/// Computes horizontal heading slots for one persistent bar surface.
fn layout_bar(headings: &[MenuSlot], style: &Style, atlas: &AtlasHandle) -> MenuSurfaceLayout {
    // An explicitly empty bar is inert and consumes no application-content height. Keeping this
    // edge case in the shared geometry function also prevents paint and hit testing from disagreeing.
    if headings.is_empty() {
        return MenuSurfaceLayout::default();
    }

    // Headings use one body font and retain intrinsic widths even when the bar stretches.
    let padding = style.padding.max(1);
    let font_choice = FontChoice::Role(FontRole::Body);
    let font = style.resolve_font_choice(font_choice);
    let preferred_height = content_height(style, atlas, font_choice, atlas.get_icon_size(style.icons.check).height);
    let mut x = 0_i32;
    let mut slots = Vec::with_capacity(headings.len());
    for heading in headings {
        let MenuSlot::Branch { label, .. } = heading else {
            // Construction keeps this impossible while accepting one shared compact slot type.
            debug_assert!(false, "a menu bar can contain only branch slots");
            continue;
        };
        let width = atlas.get_text_size(font, label).width.max(0).saturating_add(padding.saturating_mul(2));
        slots.push(Recti::new(x, 0, width, preferred_height));
        x = x.saturating_add(width);
    }
    MenuSurfaceLayout {
        size: Dimensioni::new(x, preferred_height),
        slots,
        marker_width: 0,
    }
}

/// Computes vertical popup rows and one shared marker/text split.
fn layout_popup(rows: &[MenuSlot], style: &Style, atlas: &AtlasHandle) -> MenuSurfaceLayout {
    // Measure raw glyph maxima because common control text supplies its own inner padding.
    let padding = style.padding.max(1);
    let mut label_width = 0_i32;
    let mut trailing_width = 0_i32;
    let mut marker = false;
    let mut y = 0_i32;
    let mut slots = Vec::with_capacity(rows.len());
    for entry in rows {
        let height = match entry {
            MenuSlot::Item(item) => {
                let item = item.borrow();
                let font = style.resolve_font_choice(item.font);
                label_width = label_width.max(atlas.get_text_size(font, &item.label).width.max(0));
                trailing_width = trailing_width.max(
                    item.shortcut_hint
                        .as_deref()
                        .map(|hint| atlas.get_text_size(font, hint).width.max(0))
                        .unwrap_or(0),
                );
                // False check/radio values retain their role so toggling never moves adjacent text.
                marker |= !matches!(item.mark, MenuItemMark::None);
                content_height(style, atlas, item.font, atlas.get_icon_size(style.icons.check).height)
            }
            MenuSlot::Separator => style.spacing.max(3),
            MenuSlot::Branch { label, .. } => {
                let font_choice = FontChoice::Role(FontRole::Body);
                let font = style.resolve_font_choice(font_choice);
                label_width = label_width.max(atlas.get_text_size(font, label).width.max(0));
                let arrow = atlas.get_icon_size(style.icons.expand);
                trailing_width = trailing_width.max(arrow.width.max(0));
                // Include arrow height as well as check height to avoid vertical glyph clipping.
                let visual_height = arrow.height.max(atlas.get_icon_size(style.icons.check).height);
                content_height(style, atlas, font_choice, visual_height)
            }
        };
        slots.push(Recti::new(0, y, 0, height));
        y = y.saturating_add(height);
    }

    let marker_width = if marker {
        // Radio marks use a drawn fallback, so reserve a usable column even if the check icon is empty.
        atlas
            .get_icon_size(style.icons.check)
            .width
            .max(MIN_MARKER_COLUMN_WIDTH)
            .saturating_add(padding)
    } else {
        0
    };
    // Labels and right-aligned hints/arrows share a text clip. Three paddings reserve left inset,
    // inter-content gap, and right inset whenever trailing content exists.
    let text_width = if trailing_width > 0 {
        label_width.saturating_add(trailing_width).saturating_add(padding.saturating_mul(3))
    } else {
        label_width.saturating_add(padding.saturating_mul(2))
    };
    let preferred_width = marker_width.saturating_add(text_width);
    // Complete widths in place after the shared intrinsic maximum is known; no second vector is needed.
    for slot in &mut slots {
        slot.width = preferred_width;
    }
    MenuSurfaceLayout {
        size: Dimensioni::new(preferred_width, y),
        slots,
        marker_width,
    }
}

/// Returns the shared label/shortcut/arrow region inside one popup row.
fn text_region(row: Recti, marker_width: i32) -> Recti {
    // Both text alignments receive this rectangle so padding is applied exactly once.
    Recti::new(
        row.x.saturating_add(marker_width),
        row.y,
        row.width.saturating_sub(marker_width).max(0),
        row.height,
    )
}

/// Places one visual vertically centered at a region's inset right edge.
fn trailing_rect(bounds: Recti, size: Dimensioni, inset: i32) -> Recti {
    // Saturating horizontal arithmetic keeps malformed theme metrics from wrapping coordinates.
    Recti::new(
        bounds.x.saturating_add(bounds.width).saturating_sub(inset).saturating_sub(size.width),
        bounds.y.saturating_add((bounds.height - size.height) / 2),
        size.width,
        size.height,
    )
}

/// Paints one live check or radio indicator.
fn paint_item_marker(ctx: &mut WidgetPaintCtx<'_>, bounds: Recti, mark: MenuItemMark, color: Color) {
    // Hidden boolean values retain marker layout width but record no visual primitive.
    match mark {
        MenuItemMark::None | MenuItemMark::Checked(false) | MenuItemMark::Radio(false) => {}
        MenuItemMark::Checked(true) => {
            let icon_id = ctx.style().icons.check;
            let size = ctx.atlas().get_icon_size(icon_id);
            ctx.draw_icon(icon_id, trailing_rect(bounds, size, 0), color);
        }
        MenuItemMark::Radio(true) => {
            // The atlas has no radio glyph. Center its fallback square in the check-icon column,
            // excluding the leading gutter padding represented by the wider `bounds` rectangle.
            let check_width = ctx.atlas().get_icon_size(ctx.style().icons.check).width.max(MIN_MARKER_COLUMN_WIDTH);
            let available = check_width.min(bounds.height.max(0));
            let extent = (available / 3).max(1).min(available);
            let column_x = bounds.x.saturating_add(bounds.width).saturating_sub(check_width);
            let indicator = Recti::new(
                column_x.saturating_add((check_width - extent) / 2),
                bounds.y.saturating_add((bounds.height - extent) / 2),
                extent,
                extent,
            );
            ctx.draw_rect(indicator, color);
        }
    }
}

/// Paints one right-facing submenu arrow in the shared text region.
fn paint_submenu_arrow(ctx: &mut WidgetPaintCtx<'_>, bounds: Recti) {
    // Align the glyph with the right padding used by right-aligned shortcut text.
    let style = *ctx.style();
    let size = ctx.atlas().get_icon_size(style.icons.expand);
    let icon = trailing_rect(bounds, size, style.padding.max(1));
    ctx.draw_icon(style.icons.expand, icon, style.menu_foreground);
}

/// Paints one centered subdued separator rule.
fn paint_separator(ctx: &mut WidgetPaintCtx<'_>, row: Recti) {
    // Keep horizontal breathing room and derive low contrast from the menu foreground.
    let padding = ctx.style().padding.max(1);
    let rule = Recti::new(
        row.x.saturating_add(padding),
        row.y.saturating_add(row.height / 2),
        row.width.saturating_sub(padding.saturating_mul(2)).max(0),
        1,
    );
    let mut color = ctx.style().menu_foreground;
    color.a = ((u16::from(color.a) * 45) / 100).max(1) as u8;
    ctx.draw_rect(rule, color);
}

/// Weak identity and geometry access for one retained menu surface.
#[derive(Clone)]
struct MenuSurfaceRef {
    /// Stable root node identity used to resolve this leaf's screen origin.
    node: RuntimeNodeId,
    /// Weak presentation access for open highlights and slot geometry.
    handle: TypedWidgetHandle<MenuSurface>,
}

/// Compact retained relationship that opens and positions one popup.
pub(crate) struct MenuAnchor {
    /// Root node of the bar or parent popup containing the trigger slot.
    pub(crate) node: RuntimeNodeId,
    /// Weak access to that root surface's cached logical rectangles.
    surface: TypedWidgetHandle<MenuSurface>,
    /// Heading or row index inside the source surface.
    slot: usize,
}

impl MenuAnchor {
    /// Resolves the trigger slot in screen coordinates from its source root rectangle.
    pub(crate) fn trigger_rect(&self, source: Recti) -> Option<Recti> {
        // Slot geometry is source-local; translate only its origin and preserve dimensions.
        self.surface
            .try_read(|surface| surface.geometry.borrow().slots.get(self.slot).copied())
            .flatten()
            .map(|slot| Recti::new(source.x.saturating_add(slot.x), source.y.saturating_add(slot.y), slot.width, slot.height))
    }

    /// Reconciles this slot's paint-only open state with popup visibility.
    pub(crate) fn set_open(&self, open: bool) {
        // Avoid measurement invalidation because open state changes color, never geometry.
        let updated = self.surface.try_update_without_measurement(|surface| {
            // Clearing an inactive anchor must not clear a different open slot on this surface.
            if open {
                surface.open_slot = Some(self.slot);
            } else if surface.open_slot == Some(self.slot) {
                surface.open_slot = None;
            }
        });
        debug_assert!(updated.is_some(), "a retained menu anchor must outlive its popup definition");
    }
}

/// One compact menu popup ready to transfer into its owning window.
pub(crate) struct CompiledMenuPopup {
    /// Index of the direct parent popup, or `None` for a top-level menu.
    pub(crate) parent: Option<MenuId>,
    /// Diagnostic name derived from the window and this popup's direct label.
    pub(crate) name: String,
    /// Source surface and slot used for placement and derived highlight.
    pub(crate) anchor: MenuAnchor,
    /// Sole retained leaf presenting every direct row.
    pub(crate) content: Node,
}

/// Window-owned endpoint for actions produced by any surface in one menu bar.
pub(crate) struct MenuController {
    /// Pending-action cell also retained by each compact surface.
    pending: Rc<RefCell<Option<MenuAction>>>,
    /// Popup surface identities used only by unit-test geometry inspection.
    #[cfg(test)]
    popups: Vec<MenuSurfaceRef>,
}

impl MenuController {
    /// Removes and returns the action produced by the latest routed menu press.
    pub(crate) fn take_action(&self) -> Option<MenuAction> {
        // Taking rather than copying ensures a physical press is applied at most once.
        self.pending.borrow_mut().take()
    }

    /// Resolves every cached row of one popup into screen coordinates for tests.
    #[cfg(test)]
    pub(crate) fn popup_slot_rects(&self, menu: MenuId, source: Recti) -> Option<Vec<Recti>> {
        // Copy rectangles out without exposing private surface handles or node identities.
        let surface = self.popups.get(menu)?;
        surface.handle.try_read(|surface| {
            surface
                .geometry
                .borrow()
                .slots
                .iter()
                .map(|slot| Recti::new(source.x.saturating_add(slot.x), source.y.saturating_add(slot.y), slot.width, slot.height))
                .collect()
        })
    }

    /// Returns the root node identity of one popup surface for tests.
    #[cfg(test)]
    pub(crate) fn popup_node(&self, menu: MenuId) -> Option<RuntimeNodeId> {
        // Popup vector order is identical to compact MenuId indexing.
        self.popups.get(menu).map(|surface| surface.node)
    }
}
