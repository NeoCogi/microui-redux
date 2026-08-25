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

//! Flat retained windows, window-owned popups, and cross-window traversal policy.

use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::{MouseButton, Node, RootHandle, UiInputEvent, Vec2i, rect};

use super::root_chrome::{RootChromeGeometry, RootChromePart, RootInteraction, record_root_background, record_root_overlay, root_chrome_geometry, root_handle};

/// Failure reported by a checked window or popup mutation.
///
/// Window chrome and popup definitions are owned directly by the window manager, so failures describe
/// only stale identity or explicit ownership and layer policy. No application borrow can make one of
/// these mutations partially succeed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootMutationError {
    /// The supplied [`RootId`] does not identify a retained window or dialog.
    UnknownRoot,
    /// The supplied [`PopupHandle`] no longer identifies a window-owned popup definition.
    UnknownPopup,
    /// A dialog owner is stale, hidden when shown, or is not an ordinary window.
    InvalidRootParent,
    /// The requested fixed layer is outside the supported inclusive range `0..=15`.
    InvalidLayer(u8),
    /// The requested window occupies the manager-controlled modal layer.
    ManagedLayer,
    /// A popup owner is hidden, blocked by a modal, or missing from the active parent path.
    InvalidPopupParent,
}

/// Stable identifier for one popup definition inside its owning window.
///
/// The identifier is intentionally private. Application code proves popup identity by carrying a
/// [`PopupHandle`], so popup definitions cannot be passed to generic window APIs.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct PopupId(usize);

/// Cloneable non-owning capability for one popup retained inside a window.
///
/// A popup has no independent screen root, layer, visibility flag, or destruction operation. Its
/// owner window retains the application tree, while the window manager derives visibility from the
/// single active popup path. Dropping this handle never closes or destroys the popup.
#[derive(Clone)]
pub struct PopupHandle {
    /// Stable owner used to find the window-local definition.
    owner: RootId,
    /// Stable popup identity within `owner`.
    id: PopupId,
    /// Weak endpoint for policy-driven dismissal notifications.
    submitted: crate::WidgetEventPortHandle<crate::RootSubmitted>,
}

impl PopupHandle {
    /// Creates the weak capability returned for one newly retained popup.
    fn new(owner: RootId, id: PopupId, submitted: crate::WidgetEventPortHandle<crate::RootSubmitted>) -> Self {
        // Construction stays private so callers cannot forge an owner/definition pair.
        Self { owner, id, submitted }
    }

    /// Returns whether the owning window still retains this popup definition.
    pub fn is_alive(&self) -> bool {
        // The event owner is stored in the popup record and expires when that record is dropped.
        self.submitted.is_alive()
    }

    /// Returns the weak endpoint emitted when popup policy removes this popup from the active path.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<crate::RootSubmitted> {
        // Cloning the weak endpoint does not retain the popup tree or owner window.
        self.submitted.clone()
    }
}

/// Storage and traversal state shared by a window body or popup body.
///
/// This is a value nested directly in its owner; it is not a registry or a polymorphic root. The
/// small common record keeps measurement and painting identical without duplicating window and
/// popup layout code.
struct Surface {
    /// Diagnostic name and optional window title text.
    name: String,
    /// Frame, padding, and automatic-size policy.
    options: WindowOption,
    /// Authoritative outer rectangle in screen coordinates.
    rect: Recti,
    /// Chrome and application-body geometry from the latest layout commit.
    geometry: RootChromeGeometry,
    /// Retained application-authored tree drawn inside `geometry.body`.
    tree: WidgetTree,
}

impl Surface {
    /// Creates a surface around one uniquely owned application node.
    fn new(name: String, options: WindowOption, rect: Recti, content: Node) -> Self {
        // Geometry begins empty and is replaced before a visible surface can receive input or paint.
        Self {
            name,
            options,
            rect,
            geometry: RootChromeGeometry::default(),
            tree: WidgetTree::new(content),
        }
    }

    /// Measures automatic axes and lays out the application tree in the derived body.
    fn layout(&mut self, style: &Style, atlas: &crate::AtlasHandle, viewport: Recti) {
        let auto_width = self.options.intersects(WindowOption::AUTO_WIDTH);
        let auto_height = self.options.intersects(WindowOption::AUTO_HEIGHT);
        if self.options.intersects(WindowOption::AUTO_SIZE) {
            // Convert retained outer bounds to application-body bounds before asking the content
            // tree for intrinsic size. Fixed axes retain their programmed outer extent.
            let shell = root_chrome_geometry(self.rect, Dimensioni::default(), &self.name, self.options, style, atlas);
            let horizontal_chrome = self.rect.width.saturating_sub(shell.body.width);
            let vertical_chrome = self.rect.height.saturating_sub(shell.body.height);
            let constraints = crate::Constraints::new(
                if auto_width {
                    crate::AvailableSpace::Unbounded
                } else {
                    crate::AvailableSpace::bounded(self.rect.width).shrink(horizontal_chrome)
                },
                if auto_height {
                    crate::AvailableSpace::Unbounded
                } else {
                    crate::AvailableSpace::bounded(self.rect.height).shrink(vertical_chrome)
                },
            );
            let child = self.tree.measure(style, atlas, constraints);
            let intrinsic = root_chrome_geometry(Recti::default(), child, &self.name, self.options, style, atlas).intrinsic_outer;
            if auto_width {
                self.rect.width = intrinsic.width;
            }
            if auto_height {
                self.rect.height = intrinsic.height;
            }
        }

        // Store one geometry snapshot shared by application layout, chrome hit testing, and paint.
        self.geometry = root_chrome_geometry(self.rect, Dimensioni::default(), &self.name, self.options, style, atlas);
        self.tree.layout(style, atlas.clone(), self.geometry.body, viewport);
    }

    /// Returns whether the outer surface contains one screen-space point.
    fn contains(&self, point: Vec2i) -> bool {
        // Root and popup hit testing both begin with the same positive-area rectangle predicate.
        self.rect.contains(&point)
    }
}

/// One persistent retained application tree and its traversal-local runtime.
struct WidgetTree {
    /// Sole application-authored root node.
    root: Node,
    /// Measurement, layout, input, and paint state associated with `root`.
    runtime: UiRuntime,
}

impl WidgetTree {
    /// Creates traversal state for one newly owned application node.
    fn new(root: Node) -> Self {
        // UiRuntime owns no nodes; this record keeps the parallel values together by construction.
        Self { root, runtime: UiRuntime::new() }
    }

    /// Starts a retained update cycle while preserving focus and capture.
    fn begin_update(&mut self) {
        self.runtime.begin_update();
    }

    /// Clears focus, hover, capture, and staged input without dropping retained widgets.
    fn clear_transient_targets(&mut self) {
        self.runtime.clear_transient_targets();
    }

    /// Measures the complete application tree under body-space constraints.
    fn measure(&mut self, style: &Style, atlas: &crate::AtlasHandle, constraints: crate::Constraints) -> Dimensioni {
        self.runtime.measure_tree_root(&mut self.root, style, atlas, constraints)
    }

    /// Commits the complete application tree to one screen-space body rectangle.
    fn layout(&mut self, style: &Style, atlas: crate::AtlasHandle, rect: Recti, viewport: Recti) {
        self.runtime.layout_tree_root(&mut self.root, style, atlas, rect, viewport);
    }

    /// Returns whether an application node owns pointer capture.
    fn has_capture(&self) -> bool {
        self.runtime.has_pointer_capture()
    }

    /// Prepares event-local routing for this tree.
    fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        self.runtime.begin_input_event(pointer_input_enabled, event);
    }

    /// Routes drag continuation or release directly to the captured application node.
    fn route_captured_pointer(&mut self, style: &Style, mouse_buttons: MouseButton, event: &UiInputEvent) -> Option<bool> {
        self.runtime
            .route_captured_pointer_input_event(std::slice::from_mut(&mut self.root), style, mouse_buttons, event)
    }

    /// Returns whether this runtime accepts an ordinary pointer hit for the current event.
    fn accepts_pointer_input(&self) -> bool {
        self.runtime.accepts_pointer_input()
    }

    /// Routes one ordinary hit and returns the selected retained node identity.
    fn route_pointer(&mut self, style: &Style, event: &UiInputEvent, mouse_buttons: MouseButton) -> Option<crate::ui_node::RuntimeNodeId> {
        // Capture changes only after the selected target and its ancestors classify the event.
        let (owner, result) = self.runtime.route_input_event_to_node_ref(&mut self.root, style, event)?;
        self.runtime.update_pointer_capture(owner, result, event, mouse_buttons);
        Some(owner)
    }

    /// Routes keyboard or text input to this tree's current focus owner.
    fn route_focus(&mut self, style: &Style, event: &UiInputEvent) {
        self.runtime.route_focus_input_event(std::slice::from_mut(&mut self.root), style, event);
    }

    /// Updates every participating application node after event routing.
    fn update(&mut self, style: &Style, atlas: crate::AtlasHandle, input: crate::input::InputSnapshot) {
        self.runtime.update_tree_root(&mut self.root, style, atlas, input);
    }

    /// Records the application tree into the manager display list.
    fn paint(&mut self, display_list: &mut crate::render::DisplayList, style: &Style, atlas: crate::AtlasHandle) {
        self.runtime.paint_tree_root(&mut self.root, display_list, style, atlas);
    }

    /// Resolves one retained node rectangle for relational popup anchors and tests.
    #[cfg(test)]
    fn node_rect(&self, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.runtime.debug_node_rect(std::slice::from_ref(&self.root), node)
    }

    /// Returns committed content size for retained layout tests.
    #[cfg(test)]
    fn content_size(&self) -> Dimensioni {
        self.runtime.debug_root_content_size()
    }

    /// Returns traversal counters for phase-order tests.
    #[cfg(test)]
    fn metrics(&self) -> crate::ui_node::RuntimeMetrics {
        self.runtime.debug_metrics()
    }

    /// Counts application-authored nodes without manager chrome.
    #[cfg(test)]
    fn node_count(&self) -> usize {
        self.root.debug_node_count()
    }
}

/// Direct window policy without a generic parent forest.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum WindowMode {
    /// Ordinary application window in one caller-selected fixed layer.
    Normal {
        /// Numeric stacking layer in the public fixed range.
        layer: u8,
    },
    /// Modal dialog owned directly by one ordinary application window.
    Modal {
        /// Stable ordinary owner controlling dialog lifetime and show eligibility.
        owner: RootId,
    },
}

/// Resolved stacking band used by paint and pointer priority.
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum StackBand {
    /// Caller-selectable application band.
    Fixed(u8),
    /// Manager-controlled modal band above every fixed layer.
    Modal,
}

/// Complete back-to-front ordering key for a visible surface.
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StackKey {
    /// Fixed or modal band.
    band: StackBand,
    /// Popup surfaces sort after all ordinary surfaces in the same band.
    transient: bool,
    /// Window z-order or popup depth inside the active path.
    z_index: i32,
}

/// One popup definition retained directly inside its owner window.
struct Popup {
    /// Stable definition identity.
    id: PopupId,
    /// Declared parent popup, or `None` for a top-level popup under the window.
    parent: Option<PopupId>,
    /// Shared surface state and application tree.
    surface: Surface,
    /// Strong owner of policy-driven dismissal events.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootSubmitted>>>,
}

impl Popup {
    /// Hides an active popup by clearing runtime targets and emitting one dismissal.
    fn dismiss(&mut self) {
        // Active-path removal is the only semantic close transition; inactive definitions never emit.
        self.surface.tree.clear_transient_targets();
        self.submitted_event.borrow_mut().emit(crate::RootSubmitted::PopupDismissed);
    }
}

/// One flat retained window or modal dialog.
pub(super) struct WindowEntry {
    /// Stable public identity.
    pub(super) id: RootId,
    /// Ordinary fixed-layer or directly owned modal policy.
    mode: WindowMode,
    /// Monotonic order within the resolved stacking band.
    pub(super) z_index: i32,
    /// Whether traversal currently includes this window.
    visible: bool,
    /// Manager-owned title or resize gesture.
    interaction: RootInteraction,
    /// Shared surface state and retained application tree.
    surface: Surface,
    /// Strong owner of user-driven geometry-change events.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootChanged>>>,
    /// Strong owner of window close submissions.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootSubmitted>>>,
    /// Popup definitions whose lifetime is exactly this window's lifetime.
    popups: Vec<Popup>,
}

impl WindowEntry {
    /// Returns the structural stacking band derived from this window's mode.
    fn band(&self) -> StackBand {
        match self.mode {
            WindowMode::Normal { layer } => StackBand::Fixed(layer),
            WindowMode::Modal { .. } => StackBand::Modal,
        }
    }

    /// Returns the ordering key used by every window competition.
    fn stack_key(&self) -> StackKey {
        StackKey {
            band: self.band(),
            transient: false,
            z_index: self.z_index,
        }
    }

    /// Returns whether chrome or application content owns pointer capture.
    fn has_capture(&self) -> bool {
        self.interaction != RootInteraction::None || self.surface.tree.has_capture()
    }

    /// Clears every transient input identity retained by this window.
    fn clear_transient_targets(&mut self) {
        // Manager chrome and application runtime participate in one global capture policy.
        self.interaction = RootInteraction::None;
        self.surface.tree.clear_transient_targets();
    }

    /// Shows or hides the retained window without dropping its application state.
    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.clear_transient_targets();
        }
    }

    /// Replaces chrome options and revokes a gesture disabled by the new policy.
    fn set_options(&mut self, options: WindowOption) {
        self.surface.options = options;
        let disabled = (options.intersects(WindowOption::NO_TITLE) && self.interaction == RootInteraction::Moving)
            || (options.intersects(WindowOption::NO_RESIZE | WindowOption::AUTO_SIZE) && self.interaction == RootInteraction::Resizing);
        if disabled {
            self.clear_transient_targets();
        }
    }

    /// Returns the chrome region under one screen-space point.
    fn chrome_part_at(&self, point: Vec2i) -> Option<RootChromePart> {
        // Geometry was committed from this exact screen-space rectangle during layout.
        self.surface.geometry.hit_test(point)
    }

    /// Emits the authoritative geometry after a user move or resize.
    fn emit_changed(&mut self) {
        self.changed_event.borrow_mut().emit(crate::RootChanged { rect: self.surface.rect });
    }

    /// Emits one semantic window submission at the next safe application dispatch boundary.
    fn emit_submitted(&mut self, event: crate::RootSubmitted) {
        self.submitted_event.borrow_mut().emit(event);
    }
}

/// Sole semantic representation of currently visible popups.
pub(super) struct PopupPath {
    /// Window or dialog whose popup definitions form this branch.
    owner: RootId,
    /// Parent-to-child popup identities; every prefix is visible.
    popups: Vec<PopupId>,
}

/// Lightweight identity used while traversing windows and the active popup path.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SurfaceId {
    /// Ordinary window or modal dialog surface.
    Window(RootId),
    /// Popup surface nested in its owner window.
    Popup(RootId, PopupId),
}

impl WindowManager {
    /// Registers one flat window after validating any direct modal owner.
    fn register_window(
        &mut self,
        mode: WindowMode,
        name: &str,
        rect: Recti,
        content: Node,
        options: WindowOption,
        visible: bool,
    ) -> Result<RootHandle, RootMutationError> {
        if let WindowMode::Modal { owner } = mode {
            let owner = self.window_index(owner)?;
            if !matches!(self.windows[owner].mode, WindowMode::Normal { .. }) {
                return Err(RootMutationError::InvalidRootParent);
            }
        }

        // Event owners live in the flat entry; returned handles retain neither them nor the tree.
        let changed_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let changed = crate::WidgetEventPortHandle::new(&changed_event);
        let submitted = crate::WidgetEventPortHandle::new(&submitted_event);
        let id = self.next_root_id();
        let z_index = if visible { self.next_z_index() } else { -1 };
        self.windows.push(WindowEntry {
            id,
            mode,
            z_index,
            visible,
            interaction: RootInteraction::None,
            surface: Surface::new(name.to_owned(), options, rect, content),
            changed_event,
            submitted_event,
            popups: Vec::new(),
        });
        self.invalidate_ui_commit();
        Ok(root_handle(id, changed, submitted))
    }

    /// Allocates a root identifier that will never be reused.
    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained window id counter overflowed");
        id
    }

    /// Allocates a popup identifier that will never be reused.
    fn next_popup_id(&mut self) -> PopupId {
        let id = PopupId(self.next_popup_id);
        self.next_popup_id = self.next_popup_id.checked_add(1).expect("retained popup id counter overflowed");
        id
    }

    /// Allocates the next monotonic window z-index.
    fn next_z_index(&mut self) -> i32 {
        self.last_zindex = self.last_zindex.saturating_add(1);
        self.last_zindex
    }

    /// Creates an open ordinary window around one application node.
    pub fn create_window(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        // Ordinary construction has no fallible owner edge.
        self.register_window(WindowMode::Normal { layer: DEFAULT_LAYER }, name, rect, content, WindowOption::FRAME, true)
            .expect("ordinary window registration cannot fail")
    }

    /// Creates a hidden modal dialog directly owned by an ordinary window.
    pub fn create_dialog(&mut self, owner: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        self.register_window(WindowMode::Modal { owner }, name, rect, content, WindowOption::FRAME, false)
    }

    /// Creates a hidden top-level popup definition inside one window or dialog.
    pub fn create_popup(&mut self, owner: RootId, name: &str, content: Node) -> Result<PopupHandle, RootMutationError> {
        let owner_index = self.window_index(owner)?;
        // Owner validation is the only fallible step; local definition insertion cannot fail.
        Ok(self.register_popup(owner_index, None, name, content))
    }

    /// Creates a hidden child popup beside one declared parent popup.
    pub fn create_subpopup(&mut self, parent: &PopupHandle, name: &str, content: Node) -> Result<PopupHandle, RootMutationError> {
        let (owner_index, _) = self.popup_location(parent)?;
        // The typed parent fixes both the validated owner and immutable direct ancestry.
        Ok(self.register_popup(owner_index, Some(parent.id), name, content))
    }

    /// Registers one popup definition in an already resolved owner window.
    fn register_popup(&mut self, owner_index: usize, parent: Option<PopupId>, name: &str, content: Node) -> PopupHandle {
        // Append directly to the owner; popups have no second registry or independent lifetime.
        let id = self.next_popup_id();
        let owner = self.windows[owner_index].id;
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted = crate::WidgetEventPortHandle::new(&submitted_event);
        self.windows[owner_index].popups.push(Popup {
            id,
            parent,
            surface: Surface::new(name.to_owned(), Self::default_popup_options(), Recti::default(), content),
            submitted_event,
        });
        self.invalidate_ui_commit();
        PopupHandle::new(owner, id, submitted)
    }

    /// Replaces a retained window title silently.
    pub fn set_root_name(&mut self, root: RootId, name: String) -> Result<(), RootMutationError> {
        let index = self.window_index(root)?;
        self.windows[index].surface.name = name;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a window outer rectangle silently.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) -> Result<(), RootMutationError> {
        let index = self.window_index(root)?;
        self.windows[index].surface.rect = rect;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a window outer size without changing its screen origin.
    pub fn set_root_size(&mut self, root: RootId, size: Dimensioni) -> Result<(), RootMutationError> {
        let index = self.window_index(root)?;
        self.windows[index].surface.rect.width = size.width;
        self.windows[index].surface.rect.height = size.height;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces window chrome options and reconciles capture immediately.
    pub fn set_root_options(&mut self, root: RootId, options: WindowOption) -> Result<(), RootMutationError> {
        let index = self.window_index(root)?;
        self.windows[index].set_options(options);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces popup frame, padding, and automatic-size policy through its typed handle.
    pub fn set_popup_options(&mut self, popup: &PopupHandle, options: WindowOption) -> Result<(), RootMutationError> {
        let (owner, index) = self.popup_location(popup)?;
        // Popups never acquire manager chrome; enforce that invariant regardless of caller flags.
        self.windows[owner].popups[index].surface.options = options | WindowOption::NO_TITLE | WindowOption::NO_RESIZE;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Assigns an ordinary window to one fixed application stacking layer.
    pub fn set_root_layer(&mut self, root: RootId, layer: u8) -> Result<(), RootMutationError> {
        if layer > MAX_LAYER {
            return Err(RootMutationError::InvalidLayer(layer));
        }
        let index = self.window_index(root)?;
        let WindowMode::Normal { layer: current } = &mut self.windows[index].mode else {
            return Err(RootMutationError::ManagedLayer);
        };
        *current = layer;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Returns the fixed or modal layer policy of one window.
    pub fn root_layer_binding(&self, root: RootId) -> Result<LayerBinding, RootMutationError> {
        let index = self.window_index(root)?;
        Ok(match self.windows[index].mode {
            WindowMode::Normal { layer } => LayerBinding::Fixed(layer),
            WindowMode::Modal { .. } => LayerBinding::Modal,
        })
    }

    /// Shows or hides a window while retaining its application and popup definitions.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        let index = self.window_index(root)?;
        if visible {
            if let WindowMode::Modal { owner } = self.windows[index].mode {
                let owner = self.window_index(owner)?;
                if !self.windows[owner].visible {
                    return Err(RootMutationError::InvalidRootParent);
                }
                if self.active_modal_root() != Some(root) {
                    self.dismiss_active_popups();
                }
            }
            self.windows[index].set_visible(true);
            self.raise_window(index);
        } else {
            // A normal window directly owns its dialogs; there is no recursive generic root tree.
            let affected = self.owned_window_ids(root);
            if self.active_popup.as_ref().is_some_and(|path| affected.contains(&path.owner)) {
                self.dismiss_active_popups();
            }
            for affected in affected {
                let affected = self.window_index(affected).expect("collected window must remain registered");
                self.windows[affected].set_visible(false);
            }
            if self.active_root.is_some_and(|active| active == root) {
                self.active_root = None;
            }
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Shows one popup at the current pointer position.
    pub fn show_popup(&mut self, popup: &PopupHandle) -> Result<(), RootMutationError> {
        let mouse = self.input.snapshot().mouse_pos;
        self.show_popup_at(popup, rect(mouse.x, mouse.y, 1, 1))
    }

    /// Shows one popup at an exact screen-space anchor and updates the sole active path.
    pub fn show_popup_at(&mut self, popup: &PopupHandle, anchor: Recti) -> Result<(), RootMutationError> {
        let (owner_index, popup_index) = self.popup_location(popup)?;
        if !self.popup_owner_is_eligible(popup.owner) {
            return Err(RootMutationError::InvalidPopupParent);
        }
        let parent = self.windows[owner_index].popups[popup_index].parent;

        // Determine the retained prefix before emitting any displaced-popup dismissal events.
        let (keep, retained) = match parent {
            None => {
                let retained = self
                    .active_popup
                    .as_ref()
                    .is_some_and(|path| path.owner == popup.owner && path.popups.first() == Some(&popup.id));
                (usize::from(retained), retained)
            }
            Some(parent) => {
                let Some(path) = self.active_popup.as_ref().filter(|path| path.owner == popup.owner) else {
                    return Err(RootMutationError::InvalidPopupParent);
                };
                let Some(parent_index) = path.popups.iter().position(|id| *id == parent) else {
                    return Err(RootMutationError::InvalidPopupParent);
                };
                let retained = path.popups.get(parent_index + 1) == Some(&popup.id);
                (parent_index + 1 + usize::from(retained), retained)
            }
        };

        self.truncate_active_popup_path(keep);
        if !retained {
            let path = self.active_popup.get_or_insert_with(|| PopupPath { owner: popup.owner, popups: Vec::new() });
            // `keep == 0` may replace a path owned by another window; normalize its owner here.
            path.owner = popup.owner;
            path.popups.push(popup.id);
        }
        let (owner_index, popup_index) = self.popup_location(popup)?;
        self.windows[owner_index].popups[popup_index].surface.rect = anchor;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Hides one active popup and all of its active descendants.
    pub fn hide_popup(&mut self, popup: &PopupHandle) -> Result<(), RootMutationError> {
        self.popup_location(popup)?;
        let position = self
            .active_popup
            .as_ref()
            .filter(|path| path.owner == popup.owner)
            .and_then(|path| path.popups.iter().position(|id| *id == popup.id));
        if let Some(position) = position {
            self.truncate_active_popup_path(position);
        }
        Ok(())
    }

    /// Raises one flat window inside its structural stacking band.
    pub fn bring_root_to_front(&mut self, root: RootId) -> Result<(), RootMutationError> {
        let index = self.window_index(root)?;
        if matches!(self.windows[index].mode, WindowMode::Modal { .. }) && self.windows[index].visible && self.active_modal_root() != Some(root) {
            self.dismiss_active_popups();
        }
        self.raise_window(index);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Destroys one window and, for an ordinary window, its directly owned dialogs.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        if self.window_index(root).is_err() {
            return false;
        }
        let removed = self.owned_window_ids(root);
        if self.active_popup.as_ref().is_some_and(|path| removed.contains(&path.owner)) {
            self.dismiss_active_popups();
        }
        if self.active_root.is_some_and(|active| removed.contains(&active)) {
            self.active_root = None;
        }
        self.windows.retain(|window| !removed.contains(&window.id));
        self.invalidate_ui_commit();
        true
    }

    /// Returns the index of one retained window or dialog.
    fn window_index(&self, root: RootId) -> Result<usize, RootMutationError> {
        self.windows.iter().position(|window| window.id == root).ok_or(RootMutationError::UnknownRoot)
    }

    /// Resolves one weak popup capability to its owner and definition indices.
    fn popup_location(&self, popup: &PopupHandle) -> Result<(usize, usize), RootMutationError> {
        let owner = self.window_index(popup.owner).map_err(|_| RootMutationError::UnknownPopup)?;
        let popup_index = self.windows[owner]
            .popups
            .iter()
            .position(|candidate| candidate.id == popup.id)
            .ok_or(RootMutationError::UnknownPopup)?;
        Ok((owner, popup_index))
    }

    /// Collects one window and any dialogs it owns directly.
    fn owned_window_ids(&self, root: RootId) -> Vec<RootId> {
        let mut ids = vec![root];
        if self
            .window_index(root)
            .ok()
            .is_some_and(|index| matches!(self.windows[index].mode, WindowMode::Normal { .. }))
        {
            ids.extend(self.windows.iter().filter_map(|window| match window.mode {
                WindowMode::Modal { owner } if owner == root => Some(window.id),
                _ => None,
            }));
        }
        ids
    }

    /// Returns whether a popup owner may participate under current visibility and modal policy.
    fn popup_owner_is_eligible(&self, owner: RootId) -> bool {
        let Ok(index) = self.window_index(owner) else { return false };
        if !self.windows[index].visible {
            return false;
        }
        match self.active_modal_root() {
            Some(modal) => owner == modal,
            None => matches!(self.windows[index].mode, WindowMode::Normal { .. }),
        }
    }

    /// Removes the active path suffix after `keep`, notifying deepest popups first.
    fn truncate_active_popup_path(&mut self, keep: usize) {
        let Some(mut path) = self.active_popup.take() else { return };
        let keep = keep.min(path.popups.len());
        let removed = path.popups.split_off(keep);
        for id in removed.into_iter().rev() {
            if let Ok(owner) = self.window_index(path.owner)
                && let Some(popup) = self.windows[owner].popups.iter_mut().find(|popup| popup.id == id)
            {
                popup.dismiss();
            }
        }
        if !path.popups.is_empty() {
            self.active_popup = Some(path);
        }
        self.invalidate_ui_commit();
    }

    /// Dismisses the complete active popup branch.
    fn dismiss_active_popups(&mut self) {
        self.truncate_active_popup_path(0);
    }

    /// Raises one window without changing its band.
    fn raise_window(&mut self, index: usize) {
        self.windows[index].z_index = self.next_z_index();
    }

    /// Sorts flat windows from back to front.
    fn sort_windows_for_stacking(&mut self) {
        // Stable sorting preserves deterministic order after z-index saturation.
        self.windows.sort_by_key(WindowEntry::stack_key);
    }

    /// Returns the frontmost visible modal dialog.
    fn active_modal_root(&self) -> Option<RootId> {
        self.windows
            .iter()
            .filter(|window| window.visible && matches!(window.mode, WindowMode::Modal { .. }))
            .max_by_key(|window| window.stack_key())
            .map(|window| window.id)
    }

    /// Returns an immutable common surface by lightweight traversal identity.
    fn surface(&self, id: SurfaceId) -> Option<&Surface> {
        match id {
            SurfaceId::Window(root) => self.windows.iter().find(|window| window.id == root).map(|window| &window.surface),
            SurfaceId::Popup(owner, popup) => self
                .windows
                .iter()
                .find(|window| window.id == owner)?
                .popups
                .iter()
                .find(|candidate| candidate.id == popup)
                .map(|popup| &popup.surface),
        }
    }

    /// Returns a mutable common surface by lightweight traversal identity.
    fn surface_mut(&mut self, id: SurfaceId) -> Option<&mut Surface> {
        match id {
            SurfaceId::Window(root) => self.windows.iter_mut().find(|window| window.id == root).map(|window| &mut window.surface),
            SurfaceId::Popup(owner, popup) => self
                .windows
                .iter_mut()
                .find(|window| window.id == owner)?
                .popups
                .iter_mut()
                .find(|candidate| candidate.id == popup)
                .map(|popup| &mut popup.surface),
        }
    }

    /// Returns the full ordering key for one currently visible traversal surface.
    fn surface_key(&self, id: SurfaceId) -> Option<StackKey> {
        match id {
            SurfaceId::Window(root) => self.windows.iter().find(|window| window.id == root).map(WindowEntry::stack_key),
            SurfaceId::Popup(owner, popup) => {
                let owner = self.windows.iter().find(|window| window.id == owner)?;
                let depth = self.active_popup.as_ref()?.popups.iter().position(|id| *id == popup)?;
                Some(StackKey {
                    band: owner.band(),
                    transient: true,
                    z_index: i32::try_from(depth).unwrap_or(i32::MAX),
                })
            }
        }
    }

    /// Returns whether a traversal surface belongs to the current input group.
    fn surface_is_eligible(&self, id: SurfaceId, modal: Option<RootId>) -> bool {
        match (id, modal) {
            (SurfaceId::Window(root), Some(modal)) => root == modal,
            (SurfaceId::Popup(owner, _), Some(modal)) => owner == modal,
            (SurfaceId::Window(root), None) => self
                .window_index(root)
                .ok()
                .is_some_and(|index| matches!(self.windows[index].mode, WindowMode::Normal { .. })),
            (SurfaceId::Popup(owner, _), None) => self
                .window_index(owner)
                .ok()
                .is_some_and(|index| matches!(self.windows[index].mode, WindowMode::Normal { .. })),
        }
    }

    /// Returns whether a surface owns manager or application pointer capture.
    fn surface_has_capture(&self, id: SurfaceId) -> bool {
        match id {
            SurfaceId::Window(root) => self.windows.iter().find(|window| window.id == root).is_some_and(WindowEntry::has_capture),
            SurfaceId::Popup(_, _) => self.surface(id).is_some_and(|surface| surface.tree.has_capture()),
        }
    }

    /// Returns active popup surface identities in parent-to-child order.
    fn active_popup_surfaces(&self) -> Vec<SurfaceId> {
        self.active_popup
            .as_ref()
            .map(|path| path.popups.iter().map(|popup| SurfaceId::Popup(path.owner, *popup)).collect())
            .unwrap_or_default()
    }

    /// Clears capture and focus from every surface except `keep`.
    fn clear_other_captures(&mut self, keep: SurfaceId) {
        for window in &mut self.windows {
            if SurfaceId::Window(window.id) != keep && window.has_capture() {
                window.clear_transient_targets();
            }
        }
        for surface in self.active_popup_surfaces() {
            if surface != keep && self.surface_has_capture(surface) {
                self.surface_mut(surface)
                    .expect("active popup surface must exist")
                    .tree
                    .clear_transient_targets();
            }
        }
    }

    /// Clears transient input state from one traversal surface.
    fn clear_surface_targets(&mut self, id: SurfaceId) {
        match id {
            SurfaceId::Window(root) => {
                if let Some(window) = self.windows.iter_mut().find(|window| window.id == root) {
                    window.clear_transient_targets();
                }
            }
            SurfaceId::Popup(_, _) => {
                if let Some(surface) = self.surface_mut(id) {
                    surface.tree.clear_transient_targets();
                }
            }
        }
    }

    /// Routes one event to window chrome and reports whether the overlay consumed it.
    fn route_chrome_event(&mut self, root: RootId, event: &UiInputEvent) -> bool {
        let Ok(index) = self.window_index(root) else { return false };
        match event {
            UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => match self.windows[index].chrome_part_at(*pos) {
                Some(RootChromePart::Close) => {
                    // Apply visibility before queuing Close so subscribers observe final policy.
                    self.set_root_visible(root, false).expect("chrome target must remain registered");
                    let index = self.window_index(root).expect("closing a window must not destroy it");
                    self.windows[index].emit_submitted(crate::RootSubmitted::Close);
                    true
                }
                Some(RootChromePart::Resize) => {
                    self.windows[index].surface.tree.clear_transient_targets();
                    self.windows[index].interaction = RootInteraction::Resizing;
                    true
                }
                Some(RootChromePart::Title) => {
                    self.windows[index].surface.tree.clear_transient_targets();
                    self.windows[index].interaction = RootInteraction::Moving;
                    true
                }
                None => false,
            },
            UiInputEvent::MouseDrag { pos, delta, .. } => {
                let initial = self.windows[index].surface.rect;
                match self.windows[index].interaction {
                    RootInteraction::Moving => {
                        self.windows[index].surface.rect.x = initial.x.saturating_add(delta.x);
                        self.windows[index].surface.rect.y = initial.y.saturating_add(delta.y);
                    }
                    RootInteraction::Resizing => {
                        let minimum = self.windows[index].surface.geometry.minimum_outer;
                        self.windows[index].surface.rect.width = initial.width.saturating_add(delta.x).max(minimum.width);
                        self.windows[index].surface.rect.height = initial.height.saturating_add(delta.y).max(minimum.height);
                    }
                    RootInteraction::None => return self.windows[index].chrome_part_at(*pos).is_some(),
                }
                if (
                    self.windows[index].surface.rect.x,
                    self.windows[index].surface.rect.y,
                    self.windows[index].surface.rect.width,
                    self.windows[index].surface.rect.height,
                ) != (initial.x, initial.y, initial.width, initial.height)
                {
                    self.windows[index].emit_changed();
                    self.invalidate_ui_commit();
                }
                true
            }
            UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT) && self.windows[index].interaction != RootInteraction::None => {
                self.windows[index].interaction = RootInteraction::None;
                true
            }
            _ => event.position().is_some_and(|point| self.windows[index].chrome_part_at(point).is_some()),
        }
    }

    /// Performs one synchronization layout, then one update/layout pair per queued event.
    pub(crate) fn update(&mut self, dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        // Polling contexts have no application dispatcher, so the safe boundary performs no work.
        self.update_with(dimensions, atlas, &mut (), |_, _| false);
    }

    /// Performs retained updates while exposing each safe subscriber-dispatch boundary.
    pub(crate) fn update_with<DispatchState>(
        &mut self,
        dimensions: Dimensioni,
        atlas: &crate::AtlasHandle,
        dispatch_state: &mut DispatchState,
        mut after_event: impl FnMut(&mut Self, &mut DispatchState) -> bool,
    ) {
        self.ui_commit = None;
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        for window in &mut self.windows {
            window.surface.tree.begin_update();
            for popup in &mut window.popups {
                popup.surface.tree.begin_update();
            }
        }
        self.layout(viewport, atlas);
        if after_event(self, dispatch_state) {
            self.layout(viewport, atlas);
        }

        while let Some(event) = self.input.pop_event() {
            let input = self.input.snapshot();
            self.update_for_event(atlas, &event, input);
            // Subscribers run after all retained borrows are released and before the next layout.
            after_event(self, dispatch_state);
            self.layout(viewport, atlas);
        }
        self.ui_commit = Some(dimensions);
    }

    /// Lays out visible windows and exactly the popup surfaces in the active path.
    fn layout(&mut self, viewport: Recti, atlas: &crate::AtlasHandle) {
        self.sort_windows_for_stacking();
        let style = self.style;
        for window in &mut self.windows {
            if window.visible {
                window.surface.layout(&style, atlas, viewport);
            } else {
                window.clear_transient_targets();
            }
        }

        let active = self.active_popup_surfaces();
        for window in &mut self.windows {
            for popup in &mut window.popups {
                if !active.contains(&SurfaceId::Popup(window.id, popup.id)) {
                    popup.surface.tree.clear_transient_targets();
                }
            }
        }
        for popup in active {
            self.surface_mut(popup)
                .expect("active popup definition must remain retained")
                .layout(&style, atlas, viewport);
        }
    }

    /// Routes and applies one normalized event across every eligible surface.
    fn update_for_event(&mut self, atlas: &crate::AtlasHandle, event: &UiInputEvent, input: crate::input::InputSnapshot) {
        let style = self.style;
        if matches!(event, UiInputEvent::MouseDown { .. }) {
            // Dismiss before target resolution so the same outside press reaches the revealed surface.
            self.dismiss_outside_popup(input.mouse_pos);
        }

        let hover = event.is_pointer().then(|| self.input_surface_at(input.mouse_pos)).flatten();
        if matches!(event, UiInputEvent::MouseDown { .. })
            && let Some(surface) = hover
        {
            self.activate_pointer_surface(surface);
            let owner = match surface {
                SurfaceId::Window(root) | SurfaceId::Popup(root, _) => root,
            };
            self.bring_root_to_front(owner).expect("pointer target owner must remain registered");
            self.clear_other_captures(surface);
        }

        let drag = self.drag_input_surface();
        let pointer = match event {
            UiInputEvent::MouseDrag { .. } => drag,
            _ => hover,
        };
        let keyboard = self.keyboard_input_surface();
        let modal = self.active_modal_root();

        for index in 0..self.windows.len() {
            let id = SurfaceId::Window(self.windows[index].id);
            if self.windows[index].visible && self.surface_is_eligible(id, modal) {
                self.windows[index].surface.tree.begin_input_event(pointer == Some(id), event);
            }
        }
        for id in self.active_popup_surfaces() {
            if self.surface_is_eligible(id, modal) {
                self.surface_mut(id)
                    .expect("active popup must remain retained")
                    .tree
                    .begin_input_event(pointer == Some(id), event);
            }
        }

        if event.is_pointer() {
            let captured = matches!(event, UiInputEvent::MouseDrag { .. } | UiInputEvent::MouseUp { .. })
                .then(|| self.captured_input_surface())
                .flatten();
            let mut handled = captured.is_some_and(|surface| match surface {
                SurfaceId::Window(root) => {
                    // Chrome receives continuation only when it owns the window's capture. A
                    // captured application widget must keep drag and release even over chrome.
                    let chrome_captured = self
                        .window_index(root)
                        .ok()
                        .is_some_and(|index| self.windows[index].interaction != RootInteraction::None);
                    if chrome_captured {
                        self.route_chrome_event(root, event)
                    } else {
                        self.surface_mut(surface)
                            .expect("captured window must remain retained")
                            .tree
                            .route_captured_pointer(&style, input.mouse_buttons, event)
                            .is_some()
                    }
                }
                SurfaceId::Popup(_, _) => self
                    .surface_mut(surface)
                    .expect("captured popup must remain retained")
                    .tree
                    .route_captured_pointer(&style, input.mouse_buttons, event)
                    .is_some(),
            });
            if !handled && let Some(surface) = pointer {
                handled = matches!(surface, SurfaceId::Window(root) if self.route_chrome_event(root, event));
                if !handled && self.surface(surface).is_some_and(|surface| surface.tree.accepts_pointer_input()) {
                    let _ = self
                        .surface_mut(surface)
                        .expect("pointer surface must remain retained")
                        .tree
                        .route_pointer(&style, event, input.mouse_buttons);
                }
            }
        } else if event.is_focus_input()
            && let Some(surface) = keyboard
        {
            self.surface_mut(surface)
                .expect("keyboard surface must remain retained")
                .tree
                .route_focus(&style, event);
        }

        self.sort_windows_for_stacking();
        let modal = self.active_modal_root();
        for index in 0..self.windows.len() {
            let id = SurfaceId::Window(self.windows[index].id);
            if !self.windows[index].visible || !self.surface_is_eligible(id, modal) {
                self.windows[index].clear_transient_targets();
                continue;
            }
            self.windows[index].surface.tree.update(&style, atlas.clone(), input);
        }
        for id in self.active_popup_surfaces() {
            if self.surface_is_eligible(id, modal) {
                self.surface_mut(id)
                    .expect("active popup must remain retained")
                    .tree
                    .update(&style, atlas.clone(), input);
            } else {
                self.clear_surface_targets(id);
            }
        }
        if self.active_root.is_some_and(|root| !self.root_is_visible(root)) {
            self.active_root = None;
        }
    }

    /// Paints committed windows and inserts the active popup path at its owner's band boundary.
    pub(crate) fn paint(&mut self, dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        self.display_list.clear();
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        self.sort_windows_for_stacking();
        let popup_band = self
            .active_popup
            .as_ref()
            .and_then(|path| self.window_index(path.owner).ok())
            .map(|owner| self.windows[owner].band());
        let popup_surfaces = self.active_popup_surfaces();

        for index in 0..self.windows.len() {
            if self.windows[index].visible {
                let window = &mut self.windows[index];
                record_root_background(&mut self.display_list, viewport, window.surface.rect, window.surface.options, &self.style);
                window.surface.tree.paint(&mut self.display_list, &self.style, atlas.clone());
                record_root_overlay(
                    &mut self.display_list,
                    viewport,
                    &window.surface.name,
                    window.surface.geometry,
                    &self.style,
                    atlas,
                );
            }
            let band = self.windows[index].band();
            let next_band = self.windows.get(index + 1).map(WindowEntry::band);
            if popup_band == Some(band) && next_band != Some(band) {
                // The path owner is retained, so exactly one matching band boundary must exist.
                self.paint_popups(&popup_surfaces, viewport, atlas);
            }
        }
    }

    /// Paints popup surfaces parent-first without manager title or resize chrome.
    fn paint_popups(&mut self, popups: &[SurfaceId], viewport: Recti, atlas: &crate::AtlasHandle) {
        let style = self.style;
        for popup in popups {
            let SurfaceId::Popup(owner, popup) = *popup else {
                unreachable!("popup paint list contains only popup identities")
            };
            let owner = self.window_index(owner).expect("active popup owner must remain retained");
            let popup = self.windows[owner]
                .popups
                .iter()
                .position(|candidate| candidate.id == popup)
                .expect("active popup definition must remain retained");
            let surface = &mut self.windows[owner].popups[popup].surface;
            record_root_background(&mut self.display_list, viewport, surface.rect, surface.options, &style);
            surface.tree.paint(&mut self.display_list, &style, atlas.clone());
        }
    }

    /// Truncates an active popup branch according to one outside press.
    fn dismiss_outside_popup(&mut self, mouse: Vec2i) {
        let Some(path) = self.active_popup.as_ref() else { return };
        // Use the same front-surface resolution as pointer routing. Raw popup rectangles may be
        // occluded by a window in a higher layer and therefore cannot alone keep a popup open.
        let keep = match self.input_surface_at(mouse) {
            Some(SurfaceId::Popup(owner, popup)) if owner == path.owner => {
                path.popups.iter().position(|candidate| *candidate == popup).map_or(0, |index| index + 1)
            }
            _ => 0,
        };
        self.truncate_active_popup_path(keep);
    }

    /// Records ordinary-window keyboard activation for one pointer surface.
    fn activate_pointer_surface(&mut self, surface: SurfaceId) {
        let owner = match surface {
            SurfaceId::Window(root) | SurfaceId::Popup(root, _) => root,
        };
        if let Ok(index) = self.window_index(owner)
            && matches!(self.windows[index].mode, WindowMode::Normal { .. })
        {
            self.active_root = Some(owner);
        }
    }

    /// Returns whether one retained window is visible.
    fn root_is_visible(&self, root: RootId) -> bool {
        self.window_index(root).ok().is_some_and(|index| self.windows[index].visible)
    }

    /// Returns the front eligible surface containing one pointer point.
    fn input_surface_at(&self, point: Vec2i) -> Option<SurfaceId> {
        let modal = self.active_modal_root();
        let mut front = None;
        for window in &self.windows {
            let id = SurfaceId::Window(window.id);
            if window.visible && self.surface_is_eligible(id, modal) && window.surface.contains(point) {
                let key = window.stack_key();
                if front.is_none_or(|(front_key, _)| key > front_key) {
                    front = Some((key, id));
                }
            }
        }
        for id in self.active_popup_surfaces() {
            if self.surface_is_eligible(id, modal)
                && self.surface(id).is_some_and(|surface| surface.contains(point))
                && let Some(key) = self.surface_key(id)
                && front.is_none_or(|(front_key, _)| key > front_key)
            {
                front = Some((key, id));
            }
        }
        front.map(|(_, id)| id)
    }

    /// Returns the front visible surface in the current modal group.
    fn front_input_surface(&self) -> Option<SurfaceId> {
        let modal = self.active_modal_root();
        let mut front = None;
        for window in &self.windows {
            let id = SurfaceId::Window(window.id);
            if window.visible && self.surface_is_eligible(id, modal) {
                let key = window.stack_key();
                if front.is_none_or(|(front_key, _)| key > front_key) {
                    front = Some((key, id));
                }
            }
        }
        for id in self.active_popup_surfaces() {
            if self.surface_is_eligible(id, modal)
                && let Some(key) = self.surface_key(id)
                && front.is_none_or(|(front_key, _)| key > front_key)
            {
                front = Some((key, id));
            }
        }
        front.map(|(_, id)| id)
    }

    /// Returns the eligible surface that currently owns pointer capture.
    fn captured_input_surface(&self) -> Option<SurfaceId> {
        let modal = self.active_modal_root();
        for window in &self.windows {
            let id = SurfaceId::Window(window.id);
            if window.visible && self.surface_is_eligible(id, modal) && window.has_capture() {
                return Some(id);
            }
        }
        self.active_popup_surfaces()
            .into_iter()
            .find(|id| self.surface_is_eligible(*id, modal) && self.surface_has_capture(*id))
    }

    /// Returns the surface receiving pointer-drag continuation.
    fn drag_input_surface(&self) -> Option<SurfaceId> {
        self.captured_input_surface().or_else(|| self.front_input_surface())
    }

    /// Returns the sole surface receiving keyboard and text input.
    fn keyboard_input_surface(&self) -> Option<SurfaceId> {
        if let Some(modal) = self.active_modal_root() {
            return Some(SurfaceId::Window(modal));
        }
        self.captured_input_surface()
            .or_else(|| self.active_root.filter(|root| self.root_is_visible(*root)).map(SurfaceId::Window))
            .or_else(|| {
                self.front_input_surface().and_then(|surface| {
                    let owner = match surface {
                        SurfaceId::Window(root) | SurfaceId::Popup(root, _) => root,
                    };
                    self.window_index(owner)
                        .ok()
                        .filter(|index| matches!(self.windows[*index].mode, WindowMode::Normal { .. }))
                        .map(|_| SurfaceId::Window(owner))
                })
            })
    }

    /// Returns the popup options installed for new definitions.
    const fn default_popup_options() -> WindowOption {
        WindowOption::FRAME
            .union(WindowOption::AUTO_SIZE)
            .union(WindowOption::NO_RESIZE)
            .union(WindowOption::NO_TITLE)
    }

    /// Returns visible surface names in exact paint order for tests.
    #[cfg(test)]
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        let mut names = self
            .windows
            .iter()
            .filter(|window| window.visible)
            .map(|window| (window.stack_key(), window.surface.name.clone()))
            .collect::<Vec<_>>();
        for popup in self.active_popup_surfaces() {
            if let (Some(key), Some(surface)) = (self.surface_key(popup), self.surface(popup)) {
                names.push((key, surface.name.clone()));
            }
        }
        names.sort_by_key(|(key, _)| *key);
        names.into_iter().map(|(_, name)| name).collect()
    }

    /// Returns one window z-index for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_zindex(&self, root: RootId) -> Option<i32> {
        self.windows.iter().find(|window| window.id == root).map(|window| window.z_index)
    }

    /// Returns one window name for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_name(&self, root: RootId) -> Option<String> {
        self.windows.iter().find(|window| window.id == root).map(|window| window.surface.name.clone())
    }

    /// Returns one window rectangle for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_rect(&self, root: RootId) -> Option<Recti> {
        self.windows.iter().find(|window| window.id == root).map(|window| window.surface.rect)
    }

    /// Returns one window visibility value for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_visible(&self, root: RootId) -> Option<bool> {
        self.windows.iter().find(|window| window.id == root).map(|window| window.visible)
    }

    /// Returns whether window chrome owns a gesture for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_active(&self, root: RootId) -> Option<bool> {
        self.windows
            .iter()
            .find(|window| window.id == root)
            .map(|window| window.interaction != RootInteraction::None)
    }

    /// Returns whether window title movement is active for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_moving(&self, root: RootId) -> Option<bool> {
        self.windows
            .iter()
            .find(|window| window.id == root)
            .map(|window| window.interaction == RootInteraction::Moving)
    }

    /// Returns whether window resizing is active for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_resizing(&self, root: RootId) -> Option<bool> {
        self.windows
            .iter()
            .find(|window| window.id == root)
            .map(|window| window.interaction == RootInteraction::Resizing)
    }

    /// Returns the active ordinary window for tests.
    #[cfg(test)]
    pub(crate) fn debug_active_root(&self) -> Option<RootId> {
        self.active_root
    }

    /// Returns the active modal dialog for tests.
    #[cfg(test)]
    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        self.active_modal_root()
    }

    /// Returns a window body rectangle for chrome geometry tests.
    #[cfg(test)]
    pub(crate) fn debug_root_body(&self, root: RootId, atlas: &crate::AtlasHandle) -> Option<Recti> {
        let window = self.windows.iter().find(|window| window.id == root)?;
        Some(
            root_chrome_geometry(
                window.surface.rect,
                Dimensioni::default(),
                &window.surface.name,
                window.surface.options,
                &self.style,
                atlas,
            )
            .body,
        )
    }

    /// Returns window runtime metrics for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_runtime_metrics(&self, root: RootId) -> Option<crate::ui_node::RuntimeMetrics> {
        self.windows.iter().find(|window| window.id == root).map(|window| window.surface.tree.metrics())
    }

    /// Returns combined window pointer capture for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_has_pointer_capture(&self, root: RootId) -> Option<bool> {
        self.windows.iter().find(|window| window.id == root).map(WindowEntry::has_capture)
    }

    /// Counts application nodes in one window for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_node_count(&self, root: RootId) -> Option<usize> {
        self.windows
            .iter()
            .find(|window| window.id == root)
            .map(|window| window.surface.tree.node_count())
    }

    /// Returns one application node rectangle inside a window.
    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.windows.iter().find(|window| window.id == root)?.surface.tree.node_rect(node)
    }

    /// Returns title, close, and resize geometry for one window.
    #[cfg(test)]
    pub(crate) fn debug_root_chrome(&self, root: RootId, atlas: &crate::AtlasHandle) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        let window = self.windows.iter().find(|window| window.id == root)?;
        let geometry = root_chrome_geometry(
            window.surface.rect,
            Dimensioni::default(),
            &window.surface.name,
            window.surface.options,
            &self.style,
            atlas,
        );
        Some((geometry.title, geometry.close, geometry.resize))
    }

    /// Returns a popup rectangle through its typed handle for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_rect(&self, popup: &PopupHandle) -> Option<Recti> {
        let (owner, popup) = self.popup_location(popup).ok()?;
        Some(self.windows[owner].popups[popup].surface.rect)
    }

    /// Returns whether a popup belongs to the sole active path for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_visible(&self, popup: &PopupHandle) -> Option<bool> {
        self.popup_location(popup).ok()?;
        Some(
            self.active_popup
                .as_ref()
                .is_some_and(|path| path.owner == popup.owner && path.popups.contains(&popup.id)),
        )
    }

    /// Returns popup content size through its typed handle for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_content_size(&self, popup: &PopupHandle) -> Option<Dimensioni> {
        let (owner, popup) = self.popup_location(popup).ok()?;
        Some(self.windows[owner].popups[popup].surface.tree.content_size())
    }

    /// Returns one retained node rectangle inside a popup for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_node_rect(&self, popup: &PopupHandle, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        let (owner, popup) = self.popup_location(popup).ok()?;
        self.windows[owner].popups[popup].surface.tree.node_rect(node)
    }

    /// Returns the active popup names in parent-to-child order for path tests.
    #[cfg(test)]
    pub(crate) fn debug_active_popup_names(&self) -> Vec<String> {
        self.active_popup_surfaces()
            .into_iter()
            .filter_map(|popup| self.surface(popup).map(|surface| surface.name.clone()))
            .collect()
    }
}
