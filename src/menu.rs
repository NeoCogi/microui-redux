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
//! A menu bar is consumed into one manager-owned surface for the bar and one surface for each popup.
//! Each [`MenuSurface`] is a concrete one-level container whose row values are its direct children;
//! it measures, hit-tests, anchors, and paints those children from the same data. There is no
//! per-row widget subtree, interaction node, or three-cell presentation tree. Keyboard focus is the
//! manager's active surface identity plus that container's selected direct-child slot.

use crate::{MenuRole};

use std::{cell::RefCell, fmt, rc::Rc};

use crate::ui_node::widgets::content_height;
use crate::math::RectExt;
use crate::{
    AppearanceRole, AtlasHandle, Color, Dimensioni, FontRef, FontRole, MouseButton, Recti, Skin, UiInputEvent, Vec2i, VisualState, WidgetEventPortHandle,
    WidgetOption, WidgetPaintCtx,
};

#[cfg(test)]
mod tests;

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
    /// Whether the item initially accepts pointer or keyboard submission.
    pub enabled: bool,
    /// Initial check or radio presentation.
    pub mark: MenuItemMark,
    /// Presentation-only accelerator text aligned at the right edge.
    pub shortcut_hint: Option<String>,
    /// Font used by both the label and accelerator text.
    pub font: FontRef,
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
            font: FontRef::Role(FontRole::Body),
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
    pub fn font(mut self, font: FontRef) -> Self {
        // One font choice drives measurement and paint for both strings.
        self.font = font;
        self
    }
}

/// Event emitted by one specific menu item after an enabled pointer or keyboard submission.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct MenuItemSubmitted;

impl crate::WidgetEvent for MenuItemSubmitted {}

/// Failure reported when a [`Ui`](crate::Ui) cannot resolve one menu-item capability.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MenuItemAccessError {
    /// The item is unmounted, belongs to another UI manager, or has been destroyed.
    UnknownItem,
}

impl fmt::Display for MenuItemAccessError {
    /// Describes why the requested menu-item capability could not be resolved.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep the message independent of private item identities: those values are intentionally
        // process-local implementation details and would not help an application recover.
        match self {
            Self::UnknownItem => f.write_str("the menu item is unmounted, destroyed, or belongs to another Context"),
        }
    }
}

/// Marks menu-item lookup failures as leaf errors with no lower-level cause.
///
/// Resolution is a direct identity lookup, so every failure is completely represented by this
/// enum rather than wrapping a backend, allocation, or transport error.
impl std::error::Error for MenuItemAccessError {}

/// Verifies the complete public formatting and error-chain contract for menu-item lookup errors.
#[cfg(test)]
mod menu_item_access_error_tests {
    use super::MenuItemAccessError;

    /// Confirms that the error remains usable through the standard error trait.
    fn assert_standard_error<T: std::error::Error>() {}

    /// Covers the stable diagnostic text for every concrete lookup failure.
    #[test]
    fn display_describes_every_variant() {
        // Keep the expected text next to the sole variant so adding another failure requires an
        // explicit message instead of inheriting an opaque debug representation.
        let cases = [(
            MenuItemAccessError::UnknownItem,
            "the menu item is unmounted, destroyed, or belongs to another Context",
        )];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    /// Covers the standard error implementation and this leaf error's empty source chain.
    #[test]
    fn error_contract_has_no_hidden_source() {
        // Menu-item lookup performs no fallible lower-level operation, so exposing a fabricated
        // source would make callers infer a causal error that does not exist.
        assert_standard_error::<MenuItemAccessError>();
        let error = MenuItemAccessError::UnknownItem;
        assert!(std::error::Error::source(&error).is_none());
    }
}

/// Process-unique identity for one concrete actionable menu item.
///
/// The value is allocated with the declaration and moves unchanged into its eventual menu surface.
/// It is separate from submission delivery, so releasing or reusing an event-port allocation cannot
/// redirect later presentation access through [`crate::Ui::menu_item`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct MenuItemId(
    /// Shared non-reused value hidden behind the menu-item-specific type boundary.
    crate::identity::ProcessUniqueId,
);

impl MenuItemId {
    /// Allocates one item key that cannot collide across Contexts or later declarations.
    fn allocate() -> Self {
        // Allocate before constructing either the record or handle so both receive the exact same
        // immutable identity at their sole pairing boundary.
        Self(crate::identity::ProcessUniqueId::allocate())
    }
}

/// Concrete semantic record owned by a declaration and then by exactly one menu surface.
pub(crate) struct MenuItemRecord {
    /// Process-unique identity preserved across the declaration-to-surface ownership transfer.
    id: MenuItemId,
    /// Mutable presentation and interaction values stored without a second mirror type.
    pub(crate) parameters: MenuItemParameters,
    /// Strong source of this item's independently subscribable typed event.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<MenuItemSubmitted>>>,
}

/// Uniquely owned declaration value for one actionable menu row.
///
/// Move this value into [`Menu::item`]. Keep the separately returned [`MenuItemHandle`] when the
/// application needs to subscribe or change live presentation state after mounting into a surface.
pub struct MenuItem {
    /// Sole semantic owner until this declaration moves into a manager-owned menu surface.
    pub(crate) record: MenuItemRecord,
}

impl MenuItem {
    /// Creates one uniquely owned item declaration and its non-owning application capability.
    pub fn create(parameters: MenuItemParameters) -> (MenuItemHandle, Self) {
        // Stable identity is a plain value; only the event queue is shared because subscriptions
        // are weak capabilities. The semantic record itself remains uniquely owned throughout.
        let id = MenuItemId::allocate();
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let handle = MenuItemHandle::new(id, WidgetEventPortHandle::new(&submitted_event));
        let item = Self {
            record: MenuItemRecord { id, parameters, submitted_event },
        };
        (handle, item)
    }
}

/// Cloneable non-owning capability for one concrete menu item.
///
/// The private stable ID selects presentation state through [`crate::Ui::menu_item`], while the
/// weak typed endpoint subscribes to [`MenuItemSubmitted`]. Both describe the same uniquely owned
/// record, but identity never depends on the endpoint's allocation address.
pub struct MenuItemHandle {
    /// Process-unique concrete identity used only by the owning menu surface manager.
    id: MenuItemId,
    /// Weak endpoint for enabled user submissions from this same item.
    submitted: WidgetEventPortHandle<MenuItemSubmitted>,
}

impl MenuItemHandle {
    /// Creates an application capability from an independently allocated identity and endpoint.
    fn new(id: MenuItemId, submitted: WidgetEventPortHandle<MenuItemSubmitted>) -> Self {
        // MenuItem construction establishes this pair before moving the record into any declaration
        // hierarchy, so public code can never manufacture a mismatched handle.
        Self { id, submitted }
    }

    /// Returns the private concrete key used for mounted presentation lookup.
    pub(crate) const fn id(&self) -> MenuItemId {
        // Copying identity neither inspects nor extends the weak submission endpoint lifetime.
        self.id
    }

    /// Returns this item's weak typed submission endpoint.
    pub fn submitted(&self) -> WidgetEventPortHandle<MenuItemSubmitted> {
        // Clone only the weak endpoint so subscribing cannot retain the declaration or mounted menu
        // record and does not participate in identity lookup.
        self.submitted.clone()
    }
}

impl Clone for MenuItemHandle {
    /// Clones the application capability without allocating identity or retaining the item.
    fn clone(&self) -> Self {
        // Every clone preserves the ID/endpoint pairing established by MenuItem::create.
        Self {
            id: self.id,
            submitted: self.submitted.clone(),
        }
    }
}

impl fmt::Debug for MenuItemHandle {
    /// Reports endpoint liveness without exposing the private process-local identifier.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep the numeric identity private because it is neither forgeable nor persistent API.
        f.debug_struct("MenuItemHandle").field("submitted", &self.submitted).finish_non_exhaustive()
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
        // Collect once at the ownership boundary; registration consumes this vector without clones.
        Self { menus: menus.into_iter().collect() }
    }

    /// Transfers the uniquely owned top-level declarations to window registration.
    pub(crate) fn into_menus(self) -> Vec<Menu> {
        // The manager consumes this vector directly into forest nodes without an intermediate
        // compiled hierarchy or numeric menu namespace.
        self.menus
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
        // Entries remain declaration data until a window consumes the complete bar.
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
        // Preserve exact declaration order; registration performs no implicit grouping.
        self.entries.push(MenuEntry::Separator);
        self
    }

    /// Appends one recursively composed submenu.
    pub fn submenu(mut self, submenu: Menu) -> Self {
        // The child remains uniquely owned and is flattened only during window registration.
        self.entries.push(MenuEntry::Submenu(submenu));
        self
    }

    /// Separates the parent-facing label from the rows consumed by one popup surface.
    pub(crate) fn into_parts(self) -> (String, Vec<MenuEntry>) {
        // Moving both values lets the manager install labels and rows without cloning declaration
        // strings or retaining a second recursive topology.
        (self.label, self.entries)
    }
}

/// One ordered logical row in a declarative menu.
pub(crate) enum MenuEntry {
    /// Application-authored actionable item.
    Item(MenuItem),
    /// Explicit visual rule.
    Separator,
    /// Recursively owned child menu opened from this row.
    Submenu(Menu),
}

/// Compact runtime representation of one bar heading or popup row.
pub(crate) enum MenuSlot {
    /// Concrete live item record owned directly by this row.
    Item(MenuItemRecord),
    /// Non-interactive visual rule.
    Separator,
    /// Label whose child popup is resolved from the surface forest's direct edges.
    Branch {
        /// Immutable user-visible branch label.
        label: String,
    },
}

/// Semantic action produced by pointer or keyboard interaction with one menu slot.
pub(crate) enum MenuAction {
    /// Open, toggle, or replace the child attached to this local branch slot.
    OpenSlot(usize),
    /// Replace the open child with the hovered branch while menu hot-tracking is active.
    HoverOpenSlot(usize),
    /// Close the open child branch after hovering a direct non-branch sibling.
    HoverCloseChild,
    /// Close the complete active path after an enabled item queued its typed event.
    SubmitAndClose,
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

/// Manager-owned concrete presentation for one complete menu bar or popup.
pub(crate) struct MenuSurface {
    /// Bar headings or popup rows owned directly by this surface.
    rows: Vec<MenuSlot>,
    /// Whether rows flow vertically and paint popup-only marker and arrow decoration.
    popup: bool,
    /// Slot currently under the pointer, or `None` when no logical row is hovered.
    ///
    /// Absence is represented structurally so no valid future slot index can collide with an
    /// out-of-band sentinel value.
    hovered_slot: Option<usize>,
    /// Direct child selected while this menu container is the active keyboard surface.
    ///
    /// Pointer hover stays independent so closing the scope can restore ordinary hover rendering
    /// without reconstructing either state from the other.
    keyboard_slot: Option<usize>,
    /// Slot whose child popup belongs to the active path, or `None` when this surface has no open
    /// child.
    ///
    /// This is paint-only state: popup ownership and the authoritative active path remain in the
    /// window manager.
    open_slot: Option<usize>,
    /// Whether this surface consumed the current left-button gesture.
    ///
    /// The manager uses this concrete bit only to keep drag/release away from application trees.
    /// Pointer actions remain press-only, while manager-owned keyboard selection is independent.
    captured: bool,
    /// Screen-space content rectangle assigned during the latest layout commit.
    rect: Recti,
    /// Screen-space viewport clip shared by hit testing and paint.
    clip: Recti,
    /// Latest local geometry shared by update, anchoring, and paint.
    geometry: MenuSurfaceLayout,
}

impl MenuSurface {
    /// Creates one compact concrete presentation surface.
    pub(crate) fn new(rows: Vec<MenuSlot>, popup: bool) -> Self {
        // Geometry and transient pointer state begin empty and become authoritative during layout.
        Self {
            rows,
            popup,
            hovered_slot: None,
            keyboard_slot: None,
            open_slot: None,
            captured: false,
            rect: Recti::default(),
            clip: Recti::default(),
            geometry: MenuSurfaceLayout::default(),
        }
    }

    /// Returns this surface's intrinsic menu extent and refreshes local slot geometry.
    pub(crate) fn measure(&mut self, style: &Skin, atlas: &AtlasHandle) -> Dimensioni {
        // Menus deliberately keep intrinsic row geometry. Root layout stretches only the bar's
        // assigned width, while popup outer auto-size consumes this exact preferred size. Moving
        // the old slot vector into the helper retains its allocation across committed layouts.
        let slots = std::mem::take(&mut self.geometry.slots);
        self.geometry = if self.popup {
            layout_popup(&self.rows, style, atlas, slots)
        } else {
            layout_bar(&self.rows, style, atlas, slots)
        };
        self.geometry.size
    }

    /// Commits one screen-space allocation without changing intrinsic slot rectangles.
    pub(crate) fn layout(&mut self, rect: Recti, viewport: Recti) {
        // Slots remain local so the same cached values feed painting and relational anchors. The bar
        // may be wider than its headings; popup surfaces retain their measured intrinsic allocation.
        self.rect = rect;
        self.clip = viewport;
    }

    /// Returns whether this concrete surface owns a menu pointer gesture.
    pub(crate) fn has_capture(&self) -> bool {
        self.captured
    }

    /// Returns whether one screen point lies in this surface's allocated and clipped region.
    pub(crate) fn contains(&self, point: Vec2i) -> bool {
        self.rect.contains_point(point) && self.clip.contains_point(point)
    }

    /// Clears only pointer hover while retaining any active press capture.
    pub(crate) fn clear_pointer_hover(&mut self) {
        // The manager resets menu hover before routing every pointer event because a persistent bar
        // does not receive events aimed at the application body that shares its root surface.
        self.hovered_slot = None;
    }

    /// Clears hover and capture without changing the forest-derived open highlight.
    pub(crate) fn clear_pointer_targets(&mut self) {
        // Open state is synchronized separately from active forest edges and must survive ordinary
        // hover loss until the manager commits a different popup path.
        self.clear_pointer_hover();
        self.captured = false;
    }

    /// Clears this surface's keyboard selection without changing pointer or open-path state.
    pub(crate) fn clear_keyboard_target(&mut self) {
        // WindowManager calls this across every menu surface when its single menu scope ends.
        self.keyboard_slot = None;
    }

    /// Selects the first enabled item or branch in declaration order.
    pub(crate) fn focus_first(&mut self) -> bool {
        // Separators and disabled items remain visible but are never keyboard landing points.
        self.keyboard_slot = self.rows.iter().position(Self::slot_accepts_keyboard);
        self.keyboard_slot.is_some()
    }

    /// Selects the last enabled item or branch in declaration order.
    pub(crate) fn focus_last(&mut self) -> bool {
        // Reverse position preserves the original slot index without allocating a filtered list.
        self.keyboard_slot = self.rows.iter().rposition(Self::slot_accepts_keyboard);
        self.keyboard_slot.is_some()
    }

    /// Moves selection by one eligible slot with wrapping.
    pub(crate) fn move_keyboard_focus(&mut self, forward: bool) -> bool {
        let count = self.rows.len();
        if count == 0 {
            self.keyboard_slot = None;
            return false;
        }

        // Inspect at most every row once. Starting just beyond the current slot gives stable wrap
        // behavior, while a missing selection starts at the direction-appropriate edge.
        let start = match (self.keyboard_slot, forward) {
            (Some(slot), true) => (slot + 1) % count,
            (Some(0), false) => count - 1,
            (Some(slot), false) => slot - 1,
            (None, true) => 0,
            (None, false) => count - 1,
        };
        for offset in 0..count {
            let slot = if forward {
                (start + offset) % count
            } else {
                (start + count - offset) % count
            };
            if Self::slot_accepts_keyboard(&self.rows[slot]) {
                self.keyboard_slot = Some(slot);
                return true;
            }
        }
        self.keyboard_slot = None;
        false
    }

    /// Selects one exact enabled item or branch, usually after returning from its child popup.
    pub(crate) fn focus_slot(&mut self, slot: usize) -> bool {
        // Validate the row role at the concrete owner boundary so manager topology cannot install
        // selection on a separator or disabled item.
        let selectable = self.rows.get(slot).is_some_and(Self::slot_accepts_keyboard);
        self.keyboard_slot = selectable.then_some(slot);
        selectable
    }

    /// Returns the currently selected keyboard slot.
    pub(crate) const fn keyboard_slot(&self) -> Option<usize> {
        self.keyboard_slot
    }

    /// Returns the selected slot only when it owns a submenu branch.
    pub(crate) fn keyboard_branch_slot(&self) -> Option<usize> {
        let slot = self.keyboard_slot?;
        matches!(self.rows.get(slot), Some(MenuSlot::Branch { .. })).then_some(slot)
    }

    /// Activates the selected branch or enabled item and queues the item's typed event.
    pub(crate) fn activate_keyboard_slot(&mut self) -> Option<MenuAction> {
        let slot = self.keyboard_slot?;
        match self.rows.get_mut(slot)? {
            MenuSlot::Branch { .. } => Some(MenuAction::OpenSlot(slot)),
            MenuSlot::Item(item) if item.parameters.enabled => {
                // Queue only the semantic event; WindowManager closes the complete path after this
                // mutable surface borrow ends and before application dispatch begins.
                item.submitted_event.borrow_mut().emit(MenuItemSubmitted);
                Some(MenuAction::SubmitAndClose)
            }
            MenuSlot::Item(_) | MenuSlot::Separator => None,
        }
    }

    /// Returns whether one slot may participate in menu keyboard traversal.
    fn slot_accepts_keyboard(slot: &MenuSlot) -> bool {
        match slot {
            MenuSlot::Branch { .. } => true,
            MenuSlot::Item(item) => item.parameters.enabled,
            MenuSlot::Separator => false,
        }
    }

    /// Routes one pointer event and returns a concrete manager action plus consumption state.
    pub(crate) fn route_pointer(&mut self, input: &UiInputEvent, hot_tracking: bool) -> MenuRoute {
        let Some(position) = input.position() else {
            return MenuRoute::unhandled();
        };
        let inside = self.rect.contains_point(position) && self.clip.contains_point(position);
        if !inside && !self.captured {
            self.hovered_slot = None;
            return MenuRoute::unhandled();
        }

        // Slot geometry is local while normalized input is screen-space. Translate once and use the
        // same rectangles later painted by `paint`, including full-row separator occlusion.
        let local = Vec2i::new(position.x.saturating_sub(self.rect.x), position.y.saturating_sub(self.rect.y));
        let slot = inside
            .then(|| self.geometry.slots.iter().position(|bounds| bounds.contains_point(local)))
            .flatten();
        self.hovered_slot = slot;
        if self.keyboard_slot.is_some()
            && let Some(slot) = slot
            && self.rows.get(slot).is_some_and(Self::slot_accepts_keyboard)
        {
            // Pointer movement inside an active keyboard scope updates the same visible selection
            // without making pointer hover the persistent source of keyboard state.
            self.keyboard_slot = Some(slot);
        }

        // Hover never opens an inactive bar. Once a menu path exists, however, the complete bar
        // and popup ancestry enter one hot-tracking scope: sibling headings replace the top-level
        // popup, submenu branches extend or replace the path, and ordinary sibling rows close a
        // now-unrelated child branch. Moving outside menu entries intentionally emits no action so
        // application widgets and the travel corridor between a parent row and child remain inert.
        if hot_tracking && matches!(input, UiInputEvent::MouseMove { .. } | UiInputEvent::MouseDrag { .. }) {
            let action = slot.and_then(|slot| match self.rows.get(slot)? {
                MenuSlot::Branch { .. } if self.open_slot != Some(slot) => Some(MenuAction::HoverOpenSlot(slot)),
                MenuSlot::Item(_) | MenuSlot::Separator if self.open_slot.is_some() => Some(MenuAction::HoverCloseChild),
                MenuSlot::Branch { .. } | MenuSlot::Item(_) | MenuSlot::Separator => None,
            });
            if action.is_some() {
                return MenuRoute::handled(action);
            }
        }

        // Only the left press commits policy. Capture keeps the remainder of that physical gesture
        // out of the application tree even when popup closure removes this surface from traversal.
        if matches!(input, UiInputEvent::MouseDown { button, .. } if button.intersects(MouseButton::LEFT)) {
            self.captured = true;
            let action = slot.and_then(|slot| match self.rows.get_mut(slot)? {
                MenuSlot::Branch { .. } => Some(MenuAction::OpenSlot(slot)),
                MenuSlot::Item(item) if item.parameters.enabled => {
                    // Emission only queues a typed value. Application code cannot run until the
                    // manager has consumed `SubmitAndClose` and released every surface borrow.
                    item.submitted_event.borrow_mut().emit(MenuItemSubmitted);
                    Some(MenuAction::SubmitAndClose)
                }
                MenuSlot::Item(_) | MenuSlot::Separator => None,
            });
            return MenuRoute::handled(action);
        }
        if matches!(input, UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT)) {
            self.captured = false;
        }
        MenuRoute::handled(None)
    }

    /// Paints the complete bar or popup from the geometry shared with measurement and hit testing.
    pub(crate) fn paint(&mut self, display_list: &mut crate::render::DisplayList, style: &Skin, atlas: &AtlasHandle, window_active: bool, enabled: bool) {
        // Reuse the concrete built-in paint services without retaining a `Widget`, node identity, or
        // runtime. This keeps control text and atlas clipping exactly aligned with other controls.
        let mut ctx = WidgetPaintCtx::new_with_content_geometry(
            self.rect,
            display_list,
            self.clip,
            style,
            atlas,
            enabled,
            self.hovered_slot.is_some(),
            false,
            false,
            self.captured,
            window_active,
        );
        // One uninterrupted semantic panel and one slot loop replace container, row, and cell
        // paint passes while allowing every row state to select its own PNG.
        let style = ctx.skin().clone();
        let panel_role = if self.popup {
            AppearanceRole::Menu(MenuRole::Popup)
        } else {
            AppearanceRole::Menu(MenuRole::Bar)
        };
        let panel_state = if enabled { VisualState::Normal } else { VisualState::Disabled };
        let _ = ctx.draw_appearance_state(panel_role, panel_state, ctx.local_rect());
        for (slot, entry) in self.rows.iter().enumerate() {
            let row = self.geometry.slots[slot];
            match entry {
                MenuSlot::Item(item) => {
                    let hovered = self.hovered_slot == Some(slot);
                    let focused = self.keyboard_slot == Some(slot);
                    let state = VisualState::from_interaction(enabled && item.parameters.enabled, hovered, focused, self.captured && hovered);
                    // Check and radio state already has a dedicated marker glyph. Keeping the row
                    // on MenuItem prevents persistent marker data from overriding interaction art.
                    let role = AppearanceRole::Menu(MenuRole::Item);
                    let _ = ctx.draw_appearance_state(role, state, row);
                    let marker = Recti::new(row.x, row.y, self.geometry.marker_width.max(0), row.height);
                    let text = text_region(row, self.geometry.marker_width);
                    // Text, marks, and shortcut hints resolve the same semantic role and exact
                    // interaction state as the row background.
                    let color = ctx.foreground_state(role, state);
                    paint_item_marker(&mut ctx, marker, item.parameters.mark, color);
                    let font = style.resolve_font(ctx.atlas(), &item.parameters.font);
                    // Both strings share one clip so control padding is applied exactly once.
                    ctx.draw_control_text_color_with_font(font, &item.parameters.label, text, color, WidgetOption::NONE);
                    if let Some(hint) = &item.parameters.shortcut_hint {
                        ctx.draw_control_text_color_with_font(font, hint, text, color, WidgetOption::ALIGN_RIGHT);
                    }
                }
                MenuSlot::Separator => paint_separator(&mut ctx, row),
                MenuSlot::Branch { label } => {
                    let hovered = self.hovered_slot == Some(slot);
                    let focused = self.keyboard_slot == Some(slot);
                    let state = VisualState::from_interaction(enabled, hovered, focused, self.captured && hovered);
                    let role = if self.popup {
                        AppearanceRole::Menu(MenuRole::Item)
                    } else if self.open_slot == Some(slot) {
                        AppearanceRole::Menu(MenuRole::TitleOpen)
                    } else {
                        AppearanceRole::Menu(MenuRole::Title)
                    };
                    let _ = ctx.draw_appearance_state(role, state, row);
                    // Bar headings use their full slot; popup branches reserve the marker gutter.
                    let text = if self.popup { text_region(row, self.geometry.marker_width) } else { row };
                    let font = style.resolve_font(ctx.atlas(), &FontRef::Role(FontRole::Body));
                    let color = ctx.foreground_state(role, state);
                    ctx.draw_control_text_color_with_font(font, label, text, color, WidgetOption::NONE);
                    if self.popup {
                        paint_submenu_arrow(&mut ctx, text, color);
                    }
                }
            }
        }
    }

    /// Resolves one local slot into screen coordinates for popup anchoring and diagnostics.
    pub(crate) fn slot_rect(&self, slot: usize) -> Option<Recti> {
        // Translation occurs at the concrete owner boundary; no runtime node lookup is involved.
        self.geometry.slots.get(slot).copied().map(|bounds| {
            Recti::new(
                self.rect.x.saturating_add(bounds.x),
                self.rect.y.saturating_add(bounds.y),
                bounds.width,
                bounds.height,
            )
        })
    }

    /// Copies every committed slot rectangle into screen space for focused manager tests.
    #[cfg(test)]
    pub(crate) fn slot_rects(&self) -> Vec<Recti> {
        self.geometry.slots.iter().enumerate().filter_map(|(slot, _)| self.slot_rect(slot)).collect()
    }

    /// Returns the label of one branch slot for test-only diagnostic synthesis.
    #[cfg(test)]
    pub(crate) fn branch_label(&self, slot: usize) -> Option<&str> {
        match self.rows.get(slot)? {
            MenuSlot::Branch { label } => Some(label),
            MenuSlot::Item(_) | MenuSlot::Separator => None,
        }
    }

    /// Returns the committed full surface allocation for root-layout tests.
    #[cfg(test)]
    pub(crate) fn surface_rect(&self) -> Recti {
        self.rect
    }

    /// Replaces the paint-only child highlight derived from active forest edges.
    pub(crate) fn set_open_slot(&mut self, slot: Option<usize>) {
        self.open_slot = slot;
    }

    /// Returns a manager-owned item record through its stable typed capability.
    pub(crate) fn item(&self, handle: &MenuItemHandle) -> Option<&MenuItemRecord> {
        // Stable value comparison remains valid after every old event Weak has disappeared and its
        // former allocation address becomes eligible for reuse.
        self.rows.iter().find_map(|row| match row {
            MenuSlot::Item(item) if handle.id() == item.id => Some(item),
            MenuSlot::Item(_) | MenuSlot::Separator | MenuSlot::Branch { .. } => None,
        })
    }

    /// Returns mutable manager-owned item state through its stable typed capability.
    pub(crate) fn item_mut(&mut self, handle: &MenuItemHandle) -> Option<&mut MenuItemRecord> {
        // The mutable path uses the same immutable ID relation as read access, so event connection
        // state and endpoint allocation never influence which record is lent.
        self.rows.iter_mut().find_map(|row| match row {
            MenuSlot::Item(item) if handle.id() == item.id => Some(item),
            MenuSlot::Item(_) | MenuSlot::Separator | MenuSlot::Branch { .. } => None,
        })
    }
}

/// Concrete pointer-routing result produced without a retained widget node.
pub(crate) struct MenuRoute {
    /// Whether the menu surface occluded or captured this pointer event.
    pub(crate) handled: bool,
    /// Optional semantic action for the manager to apply after releasing the surface borrow.
    pub(crate) action: Option<MenuAction>,
}

impl MenuRoute {
    /// Constructs a result for an event outside a non-captured menu surface.
    fn unhandled() -> Self {
        Self { handled: false, action: None }
    }

    /// Constructs a consumed result with an optional pointer-produced semantic action.
    fn handled(action: Option<MenuAction>) -> Self {
        Self { handled: true, action }
    }
}

/// Computes horizontal heading slots for one persistent bar surface.
fn layout_bar(headings: &[MenuSlot], style: &Skin, atlas: &AtlasHandle, mut slots: Vec<Recti>) -> MenuSurfaceLayout {
    // An explicitly empty bar is inert and consumes no application-content height. Keeping this
    // edge case in the shared geometry function also prevents paint and hit testing from disagreeing.
    slots.clear();
    if headings.is_empty() {
        return MenuSurfaceLayout {
            size: Dimensioni::default(),
            slots,
            marker_width: 0,
        };
    }

    // Headings use one body font and retain intrinsic widths even when the bar stretches.
    let padding = style.metrics.padding.max(1);
    let font_choice = FontRef::Role(FontRole::Body);
    let font = style.resolve_font(atlas, &font_choice);
    let check = crate::IconRole::Check.resolve(atlas);
    let preferred_height = content_height(style, atlas, &font_choice, atlas.get_icon_size(check).height);
    let mut x = 0_i32;
    slots.reserve(headings.len());
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
fn layout_popup(rows: &[MenuSlot], style: &Skin, atlas: &AtlasHandle, mut slots: Vec<Recti>) -> MenuSurfaceLayout {
    // MenuPopup is the sole popup shell, so its structural insets surround every row without a
    // second WindowFrame. Measure raw glyph maxima because control text supplies its own padding.
    slots.clear();
    slots.reserve(rows.len());
    let panel_insets = style
        .visual(AppearanceRole::Menu(MenuRole::Popup), VisualState::Normal)
        .patch
        .insets
        .normalized();
    let padding = style.metrics.padding.max(1);
    let mut label_width = 0_i32;
    let mut trailing_width = 0_i32;
    let mut marker = false;
    let mut y = panel_insets.top;
    for entry in rows {
        let height = match entry {
            MenuSlot::Item(item) => {
                let font = style.resolve_font(atlas, &item.parameters.font);
                label_width = label_width.max(atlas.get_text_size(font, &item.parameters.label).width.max(0));
                trailing_width = trailing_width.max(
                    item.parameters
                        .shortcut_hint
                        .as_deref()
                        .map(|hint| atlas.get_text_size(font, hint).width.max(0))
                        .unwrap_or(0),
                );
                // False check/radio values retain their role so toggling never moves adjacent text.
                marker |= !matches!(item.parameters.mark, MenuItemMark::None);
                content_height(
                    style,
                    atlas,
                    &item.parameters.font,
                    atlas.get_icon_size(crate::IconRole::Check.resolve(atlas)).height,
                )
            }
            MenuSlot::Separator => style.metrics.spacing.max(3),
            MenuSlot::Branch { label, .. } => {
                let font_choice = FontRef::Role(FontRole::Body);
                let font = style.resolve_font(atlas, &font_choice);
                label_width = label_width.max(atlas.get_text_size(font, label).width.max(0));
                let arrow = atlas.get_icon_size(crate::IconRole::Expand.resolve(atlas));
                trailing_width = trailing_width.max(arrow.width.max(0));
                // Include arrow height as well as check height to avoid vertical glyph clipping.
                let visual_height = arrow.height.max(atlas.get_icon_size(crate::IconRole::Check.resolve(atlas)).height);
                content_height(style, atlas, &font_choice, visual_height)
            }
        };
        slots.push(Recti::new(panel_insets.left, y, 0, height));
        y = y.saturating_add(height);
    }

    let marker_width = if marker {
        // Radio marks use a drawn fallback, so reserve a usable column even if the check icon is empty.
        atlas
            .get_icon_size(crate::IconRole::Check.resolve(atlas))
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
    let row_width = marker_width.saturating_add(text_width);
    let preferred_width = row_width.saturating_add(panel_insets.horizontal_extent());
    // Complete widths in place after the shared intrinsic maximum is known; no second vector is needed.
    for slot in &mut slots {
        slot.width = row_width;
    }
    MenuSurfaceLayout {
        size: Dimensioni::new(preferred_width, y.saturating_add(panel_insets.bottom)),
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
            let icon_id = crate::IconRole::Check.resolve(ctx.atlas());
            let size = ctx.atlas().get_icon_size(icon_id);
            ctx.draw_icon(icon_id, trailing_rect(bounds, size, 0), color);
        }
        MenuItemMark::Radio(true) => {
            // The atlas has no radio glyph. Center its fallback square in the check-icon column,
            // excluding the leading gutter padding represented by the wider `bounds` rectangle.
            let check_width = ctx
                .atlas()
                .get_icon_size(crate::IconRole::Check.resolve(ctx.atlas()))
                .width
                .max(MIN_MARKER_COLUMN_WIDTH);
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
fn paint_submenu_arrow(ctx: &mut WidgetPaintCtx<'_>, bounds: Recti, color: Color) {
    // Align the glyph with the right padding used by right-aligned shortcut text.
    let style = ctx.skin().clone();
    let icon_id = crate::IconRole::Expand.resolve(ctx.atlas());
    let size = ctx.atlas().get_icon_size(icon_id);
    let icon = trailing_rect(bounds, size, style.metrics.padding.max(1));
    ctx.draw_icon(icon_id, icon, color);
}

/// Paints one centered subdued separator rule.
fn paint_separator(ctx: &mut WidgetPaintCtx<'_>, row: Recti) {
    // Keep horizontal breathing room and derive low contrast from the menu foreground.
    let padding = ctx.skin().metrics.padding.max(1);
    let rule = Recti::new(
        row.x.saturating_add(padding),
        row.y.saturating_add(row.height / 2),
        row.width.saturating_sub(padding.saturating_mul(2)).max(0),
        1,
    );
    let mut color = ctx.foreground_state(AppearanceRole::Menu(MenuRole::Popup), VisualState::Normal);
    color.a = ((u16::from(color.a) * 45) / 100).max(1) as u8;
    ctx.draw_rect(rule, color);
}
