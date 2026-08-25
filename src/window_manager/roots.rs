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

//! Root registry, cross-root policy, and persistent tree traversal.

use super::*;
use std::{cell::RefCell, rc::Rc};

use crate::{MouseButton, Node, RootHandle, UiInputEvent, Vec2i, rect};

use super::root_chrome::{RootChromePart, RootInteraction, RootState, record_root_background, record_root_overlay, root_chrome_geometry, root_handle};

/// Failure reported when the root registry cannot complete a checked state mutation.
///
/// These errors describe registry identity and cross-root policy. Manager-owned chrome state cannot
/// be externally borrowed, so a mutation either succeeds completely or fails on one of the stable
/// registry invariants represented here.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootMutationError {
    /// The supplied [`RootId`] does not identify a root currently owned by the context.
    ///
    /// Destroying a root invalidates its identifier for all later checked mutations. Root
    /// identifiers are never reused, so this result cannot accidentally address a newer root.
    UnknownRoot,
    /// The requested parent cannot own the new root kind, or a visible child has a hidden parent.
    ///
    /// Child windows and dialogs require an ordinary window parent, while popup roots may own only
    /// popup descendants. These rules keep persistent-root activation out of modal and transient
    /// policy. A stale parent is reported as [`Self::UnknownRoot`].
    InvalidRootParent,
    /// The requested fixed layer is outside the supported inclusive range `0..=15`.
    InvalidLayer(u8),
    /// The root's layer is controlled by popup inheritance or modal policy.
    ///
    /// Only independent windows own a directly configurable fixed layer. Owned windows and popups
    /// inherit through their stable parent; dialogs always use the dedicated modal layer.
    ManagedLayer,
    /// A popup was requested through generic visibility instead of anchored popup policy.
    PopupShowRequired,
    /// The popup's stable parent cannot currently open the requested child.
    ///
    /// This includes a hidden parent or a parent outside the active modal subtree.
    InvalidPopupParent,
}

/// Cloneable non-owning capability for one retained popup root.
///
/// Only [`crate::Context::create_popup`] and [`crate::EventContext::create_popup`] construct this
/// type. That construction boundary proves that the wrapped root was registered with popup policy,
/// allowing anchored placement to accept a [`PopupHandle`] instead of accepting an arbitrary
/// [`RootId`] and reporting a runtime root-kind error. Destroying the root still invalidates this
/// weak capability, and later checked mutations report [`RootMutationError::UnknownRoot`].
#[derive(Clone)]
pub struct PopupHandle {
    /// Generic root capability retained for shared lifecycle, chrome-state, and event access.
    root: RootHandle,
}

impl PopupHandle {
    /// Wraps a newly registered popup root in the capability required by popup-only operations.
    fn new(root: RootHandle) -> Self {
        // Keep construction private to the root registry so safe application code cannot turn an
        // ordinary window or modal-dialog handle into a popup capability.
        Self { root }
    }

    /// Returns the lifecycle identifier accepted by generic root operations.
    pub fn id(&self) -> RootId {
        // Forward the stable identity without exposing a constructor for this typed capability.
        self.root.id()
    }

    /// Returns whether the Context still owns this popup definition.
    pub fn is_alive(&self) -> bool {
        // PopupHandle mirrors RootHandle liveness without exposing manager-owned chrome state.
        self.root.is_alive()
    }

    /// Returns the native event endpoint emitted after a user-driven move or resize.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<crate::RootChanged> {
        // Preserve the same cloneable event capability exposed by an ordinary root handle.
        self.root.changed()
    }

    /// Returns the native event endpoint emitted for close and popup-dismissal submissions.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<crate::RootSubmitted> {
        // Popup dismissal remains observable without exposing or duplicating the owned event port.
        self.root.submitted()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum WindowKind {
    /// An ordinary top-level or owned application window.
    Window,
    /// A modal dialog that excludes input to every root below it.
    Modal,
    /// A transient root governed by exclusive visibility and popup-dismissal policy.
    Popup,
}

/// Resolved stacking band used by ordering and modal eligibility.
///
/// `Fixed` preserves the application's numeric layer while `Modal` remains structurally above all
/// values in the fixed range. Keeping the modal band out of the public numeric range makes it
/// impossible for an ordinary root to collide with modal policy.
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum StackBand {
    Fixed(u8),
    Modal,
}

/// Total ordering key shared by layout traversal, paint, and pointer hit testing.
///
/// Popups use a transient tier inside their inherited band. Consequently they cover ordinary
/// roots in that same layer without escaping above a higher numeric layer.
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StackKey {
    band: StackBand,
    transient: bool,
    z_index: i32,
}

/// One persistent retained tree and its traversal-local runtime state.
pub(super) struct WidgetTree {
    /// Sole application-authored node at this screen-space root.
    root: Node,
    /// Traversal state for measurement, input, update, and painting of `root`.
    runtime: UiRuntime,
}

impl WidgetTree {
    fn new(root: Node) -> Self {
        Self { root, runtime: UiRuntime::new() }
    }

    fn begin_update(&mut self) {
        self.runtime.begin_update();
    }

    fn clear_transient_targets(&mut self) {
        self.runtime.clear_transient_targets();
    }

    fn measure(&mut self, style: &Style, atlas: &crate::AtlasHandle, constraints: crate::Constraints) -> Dimensioni {
        self.runtime.measure_tree_root(&mut self.root, style, atlas, constraints)
    }

    fn layout(&mut self, style: &Style, atlas: crate::AtlasHandle, rect: Recti, viewport: Recti) {
        self.runtime.layout_tree_root(&mut self.root, style, atlas, rect, viewport);
    }

    fn has_capture(&self) -> bool {
        self.runtime.has_pointer_capture()
    }

    fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        self.runtime.begin_input_event(pointer_input_enabled, event);
    }

    fn route_captured_pointer(&mut self, style: &Style, mouse_buttons: MouseButton, event: &UiInputEvent) -> Option<bool> {
        self.runtime
            .route_captured_pointer_input_event(std::slice::from_mut(&mut self.root), style, mouse_buttons, event)
    }

    fn accepts_pointer_input(&self) -> bool {
        self.runtime.accepts_pointer_input()
    }

    /// Routes one ordinary pointer event into the application tree.
    fn route_pointer(&mut self, style: &Style, event: &UiInputEvent, mouse_buttons: MouseButton) {
        // Manager-owned chrome has already consumed any overlapping hit before this method runs.
        if let Some((owner, result)) = self.runtime.route_input_event_to_node_ref(&mut self.root, style, event) {
            self.runtime.update_pointer_capture(owner, result, event, mouse_buttons);
        }
    }

    fn route_focus(&mut self, style: &Style, event: &UiInputEvent) {
        self.runtime.route_focus_input_event(std::slice::from_mut(&mut self.root), style, event);
    }

    fn update(&mut self, style: &Style, atlas: crate::AtlasHandle, input: crate::input::InputSnapshot) {
        self.runtime.update_tree_root(&mut self.root, style, atlas, input);
    }

    fn paint(&mut self, display_list: &mut crate::render::DisplayList, style: &Style, atlas: crate::AtlasHandle) {
        self.runtime.paint_tree_root(&mut self.root, display_list, style, atlas);
    }

    #[cfg(test)]
    fn content_size(&self) -> Dimensioni {
        self.runtime.debug_root_content_size()
    }

    #[cfg(test)]
    fn metrics(&self) -> crate::ui_node::RuntimeMetrics {
        self.runtime.debug_metrics()
    }

    #[cfg(test)]
    fn node_count(&self) -> usize {
        self.root.debug_node_count()
    }

    #[cfg(test)]
    fn node_rect(&self, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.runtime.debug_node_rect(std::slice::from_ref(&self.root), node)
    }
}

/// Lifecycle and cross-root metadata for one retained tree.
pub(super) struct WindowEntry {
    /// Stable identity used by public weak handles and internal ownership edges.
    pub(super) id: RootId,
    /// Root behavior governing chrome, modality, and transient dismissal.
    pub(super) kind: WindowKind,
    /// Direct logical owner; `None` is reserved for independent top-level windows.
    parent: Option<RootId>,
    /// Cached band derived from the top-level layer, `parent`, and modal ancestry.
    ///
    /// The cache keeps sorting allocation-free. Tree mutations refresh it synchronously, so it is
    /// never a second source of ownership truth.
    effective_band: StackBand,
    /// Monotonic ordering value inside the effective band and transient tier.
    pub(super) z_index: i32,
    /// Authoritative manager-owned chrome and surface state.
    pub(super) state: RootState,
    /// Strong owner of move and resize notifications.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootChanged>>>,
    /// Strong owner of close and popup-dismissal notifications.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootSubmitted>>>,
    /// Sole strong owner of the complete application widget tree for this screen-space root.
    pub(super) tree: WidgetTree,
}

impl WindowEntry {
    /// Returns the complete stacking key used everywhere roots compete for visual priority.
    fn stack_key(&self) -> StackKey {
        StackKey {
            band: self.effective_band,
            transient: self.kind == WindowKind::Popup,
            z_index: self.z_index,
        }
    }

    /// Returns whether chrome or application content owns an in-progress pointer gesture.
    fn has_capture(&self) -> bool {
        // Both capture sources participate in the same cross-root exclusivity policy.
        self.state.is_active() || self.tree.has_capture()
    }

    /// Emits a user-driven geometry change through the root's weak public event endpoint.
    fn emit_changed(&mut self) {
        // Report the authoritative rectangle after applying the complete pointer delta.
        self.changed_event.borrow_mut().emit(crate::RootChanged { rect: self.state.rect });
    }

    /// Emits one semantic root submission without exposing the owned event queue.
    fn emit_submitted(&mut self, event: crate::RootSubmitted) {
        // Subscribers run only at WindowManager's dispatch boundary after this borrow is released.
        self.submitted_event.borrow_mut().emit(event);
    }

    /// Hides a popup and reports policy dismissal exactly once.
    fn dismiss_popup(&mut self) {
        // A hidden popup is already outside the active branch and must not emit duplicate closure.
        if self.state.visible {
            self.state.set_visible(false);
            self.emit_submitted(crate::RootSubmitted::PopupDismissed);
        }
        self.clear_transient_targets();
    }

    /// Clears runtime interaction identities and manager-owned chrome capture.
    fn clear_transient_targets(&mut self) {
        // Keep both capture sources synchronized when cross-root policy revokes interaction.
        self.tree.clear_transient_targets();
        self.state.clear_interaction();
    }
}

impl WindowManager {
    /// Wraps one application node in private chrome and registers it in the owned-root forest.
    ///
    /// `parent` is logical ownership only: every registered root retains independent screen-space
    /// geometry and its own widget runtime. Registration is fallible for owned roots because stale
    /// identifiers and invalid ownership edges must not create an orphaned tree node.
    fn register_root(
        &mut self,
        parent: Option<RootId>,
        kind: WindowKind,
        name: &str,
        rect: Recti,
        content: Node,
        options: WindowOption,
        visible: bool,
    ) -> Result<RootHandle, RootMutationError> {
        // Resolve and validate the parent before allocating identity or constructing retained state,
        // so a failed creation has no observable side effects.
        let parent_index = parent.map(|parent| self.root_index(parent)).transpose()?;
        if let Some(parent_index) = parent_index {
            let parent_kind = self.roots[parent_index].kind;
            if kind != WindowKind::Popup && parent_kind != WindowKind::Window {
                return Err(RootMutationError::InvalidRootParent);
            }
        }
        if visible && let Some(parent_index) = parent_index {
            if !self.roots[parent_index].state.visible {
                return Err(RootMutationError::InvalidRootParent);
            }
        }
        // Allocate lifecycle identity before construction; IDs are never derived from node identity.
        let id = self.next_root_id();
        // The registry strongly owns semantic event queues and exposes only weak handles to callers.
        let changed_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let changed = crate::WidgetEventPortHandle::new(&changed_event);
        let submitted = crate::WidgetEventPortHandle::new(&submitted_event);
        // Hidden roots remain registered but sit outside visible z-order until explicitly shown.
        let z_index = if visible {
            // next_z_index = previous_z_index + 1.
            self.last_zindex = self.last_zindex.saturating_add(1);
            self.last_zindex
        } else {
            -1
        };
        // Modal roots establish their dedicated band. Every other owned root inherits the already
        // resolved parent band, while an independent window starts at the default fixed layer.
        let effective_band = match kind {
            WindowKind::Modal => StackBand::Modal,
            WindowKind::Window | WindowKind::Popup => parent_index
                .map(|parent_index| self.roots[parent_index].effective_band)
                .unwrap_or(StackBand::Fixed(DEFAULT_LAYER)),
        };
        // Context owns both plain chrome state and the application tree; neither has a second owner.
        self.roots.push(WindowEntry {
            id,
            kind,
            parent,
            effective_band,
            z_index,
            state: RootState::new(name.to_owned(), options, rect, visible),
            changed_event,
            submitted_event,
            tree: WidgetTree::new(content),
        });
        // New topology requires a layout commit before rendering or pointer routing.
        self.invalidate_ui_commit();
        Ok(root_handle(id, changed, submitted))
    }

    fn next_root_id(&mut self) -> RootId {
        // Return the current value, then reserve the next without wrapping to a reused identifier.
        let id = RootId::from_raw(self.next_root_id);
        // next_root_id = current_root_id + 1.
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained root id counter overflowed");
        id
    }

    /// Creates an open retained window around one uniquely owned application node.
    ///
    /// The returned handle is weak; `Context` owns the root until explicit destruction.
    pub fn create_window(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        // A top-level window has no fallible ownership edge, so registration can only succeed.
        self.register_root(None, WindowKind::Window, name, rect, content, WindowOption::FRAME, true)
            .expect("top-level window registration cannot have an invalid parent")
    }

    /// Creates an open retained child window with stable logical ownership.
    ///
    /// The ordinary-window parent controls recursive lifetime and supplies the inherited stacking
    /// band, but never clips, offsets, or lays out the independently positioned child.
    pub fn create_child_window(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // Child windows begin visible, so registration also verifies that the supplied owner is
        // currently visible and can uphold the all-visible-ancestors invariant.
        self.register_root(Some(parent), WindowKind::Window, name, rect, content, WindowOption::FRAME, true)
    }

    /// Creates a hidden retained dialog owned by one stable ordinary window.
    ///
    /// Show it with [`crate::Context::set_root_visible`]. Every visible dialog occupies the dedicated
    /// modal layer; the frontmost dialog and its owned subtree form the active modal input group.
    /// Ordinary routing resumes only after no dialog remains visible.
    pub fn create_dialog(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // Dialogs begin hidden and therefore may be prepared beneath a currently hidden owner.
        self.register_root(Some(parent), WindowKind::Modal, name, rect, content, WindowOption::FRAME, false)
    }

    /// Creates a hidden auto-sized popup owned by one stable parent root.
    ///
    /// Show it through [`crate::Context::show_popup`] or [`crate::Context::show_popup_at`]. Its
    /// parent supplies the inherited effective band and recursive lifetime. Within that band, the
    /// popup's transient tier is above ordinary roots; it never crosses a higher fixed layer. A
    /// popup owned by the active modal subtree participates in that exclusive input group. Any
    /// policy-driven hide—including replacement, an outside press, or recursive ancestor hiding—
    /// records a dismissal submission.
    pub fn create_popup(&mut self, parent: RootId, name: &str, content: Node) -> Result<PopupHandle, RootMutationError> {
        // Wrap the generic registry handle at the only construction point that installs popup
        // policy. Callers can therefore prove popup identity by type instead of a runtime check.
        self.register_root(
            Some(parent),
            WindowKind::Popup,
            name,
            Recti::default(),
            content,
            Self::default_popup_options(),
            false,
        )
        .map(PopupHandle::new)
    }

    /// Replaces a retained root title without emitting a root-change event.
    pub fn set_root_name(&mut self, root: RootId, name: String) -> Result<(), RootMutationError> {
        // Chrome state is manager-owned, so this checked mutation cannot fail after ID resolution.
        let index = self.root_index(root)?;
        self.roots[index].state.name = name;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a root rectangle silently while retaining any compatible captured chrome mode.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) -> Result<(), RootMutationError> {
        // Programmatic geometry replaces all four authoritative outer-rectangle components.
        let index = self.root_index(root)?;
        self.roots[index].state.rect = rect;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a root size silently without changing its origin.
    pub fn set_root_size(&mut self, root: RootId, size: Dimensioni) -> Result<(), RootMutationError> {
        // Preserve screen position while replacing only the allocated outer extents.
        let index = self.root_index(root)?;
        self.roots[index].state.rect.width = size.width;
        self.roots[index].state.rect.height = size.height;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces root chrome options silently.
    pub fn set_root_options(&mut self, root: RootId, options: WindowOption) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        let was_active = self.roots[index].state.is_active();
        self.roots[index].state.set_options(options);
        if was_active && !self.roots[index].state.is_active() {
            // Disabling the active chrome affordance also revokes application runtime targets.
            self.roots[index].clear_transient_targets();
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Assigns one independent top-level window to a fixed application stacking layer.
    ///
    /// Layers are ordered from zero at the bottom through fifteen at the top. Owned windows, popups,
    /// and dialogs reject direct assignment because their effective bands come from ownership or
    /// modal policy. Every non-modal descendant before an intervening modal boundary follows the
    /// new band in the same transaction.
    pub fn set_root_layer(&mut self, root: RootId, layer: u8) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        if layer > MAX_LAYER {
            return Err(RootMutationError::InvalidLayer(layer));
        }
        if self.roots[index].kind != WindowKind::Window || self.roots[index].parent.is_some() {
            return Err(RootMutationError::ManagedLayer);
        }

        // The effective band is also the fixed-layer storage for an independent window. Propagate
        // it parent-first; modal descendants establish a new band that their children inherit.
        self.roots[index].effective_band = StackBand::Fixed(layer);
        for child in self.subtree_ids(root)?.into_iter().skip(1) {
            let child_index = self.root_index(child)?;
            self.roots[child_index].effective_band = if self.roots[child_index].kind == WindowKind::Modal {
                StackBand::Modal
            } else {
                let parent = self.roots[child_index].parent.expect("owned descendant must have a parent");
                self.roots[self.root_index(parent)?].effective_band
            };
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Returns the registered root's layer policy derived from stable ownership.
    pub fn root_layer_binding(&self, root: RootId) -> Result<LayerBinding, RootMutationError> {
        let index = self.root_index(root)?;
        // Modal policy overrides ownership for the dialog root itself. All other owned roots expose
        // their direct parent, while independent windows expose their caller-selected fixed layer.
        Ok(match (self.roots[index].kind, self.roots[index].parent) {
            (WindowKind::Modal, _) => LayerBinding::Modal,
            (_, Some(parent)) => LayerBinding::Inherited(parent),
            (WindowKind::Window, None) => match self.roots[index].effective_band {
                StackBand::Fixed(layer) => LayerBinding::Fixed(layer),
                StackBand::Modal => unreachable!("independent windows always use a fixed band"),
            },
            (WindowKind::Popup, None) => unreachable!("popup roots always have a stable parent"),
        })
    }

    /// Shows or hides a retained root, preserving its tree and concrete widget state.
    ///
    /// Showing a dialog makes it the front modal root. Hiding any root hides its complete owned
    /// subtree, and every visible popup in that subtree records dismissal.
    /// Showing a popup is rejected because anchored popup policy must reconcile the visible branch;
    /// use [`Self::show_popup`] or [`Self::show_popup_at`].
    ///
    /// This is distinct from [`crate::Context::destroy_root`], which drops the complete retained owner.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        let target = self.root_index(root)?;
        let kind = self.roots[target].kind;
        if visible && kind == WindowKind::Popup {
            return Err(RootMutationError::PopupShowRequired);
        }

        if !visible {
            return self.hide_root_subtree(root);
        }

        // An owned root cannot become visible while its direct owner is hidden. Recursive hiding
        // maintains the stronger all-ancestors-visible invariant after this one local check.
        if let Some(parent) = self.roots[target].parent
            && !self.root_is_visible_checked(parent)?
        {
            return Err(RootMutationError::InvalidRootParent);
        }

        if kind == WindowKind::Modal {
            self.dismiss_popups_except(None);
        }
        self.roots[target].state.set_visible(true);

        match kind {
            WindowKind::Modal | WindowKind::Window => self.raise_root_subtree(root),
            WindowKind::Popup => unreachable!("generic visibility rejects popup roots"),
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Shows one typed popup at the current pointer position under its stable parent.
    ///
    /// This is the pointer-relative counterpart to [`Self::show_popup_at`]. Ownership and layer
    /// inheritance were established when the popup root was created.
    pub fn show_popup(&mut self, popup: &PopupHandle) -> Result<(), RootMutationError> {
        let mouse = self.input.snapshot().mouse_pos;
        self.show_popup_at(popup, rect(mouse.x, mouse.y, 1, 1))
    }

    /// Shows one typed popup at an exact screen-space anchor in a single root transaction.
    ///
    /// The supplied rectangle is installed before the next layout observes the popup. Showing a
    /// top-level popup replaces the current branch; a popup whose stable parent is another popup
    /// retains that ancestor and replaces only its active descendants.
    /// Accepting [`PopupHandle`] makes windows and dialogs ineligible at compile time; a handle
    /// whose root was destroyed is still reported as [`RootMutationError::UnknownRoot`].
    pub fn show_popup_at(&mut self, popup: &PopupHandle, anchor: Recti) -> Result<(), RootMutationError> {
        // Resolve the weak capability on every mutation because destroying a popup does not destroy
        // application-held handles. Root IDs are never reused, so a successful lookup identifies
        // the same popup originally wrapped by PopupHandle::new.
        let target = self.root_index(popup.id())?;
        self.show_popup_index_at(target, anchor)
    }

    /// Applies anchored popup policy to an already-resolved popup registry index.
    fn show_popup_index_at(&mut self, target: usize, anchor: Recti) -> Result<(), RootMutationError> {
        // The typed handle establishes target popup identity, while registration guarantees one
        // stable parent and an already-derived effective stacking band.
        debug_assert_eq!(self.roots[target].kind, WindowKind::Popup);
        let target_id = self.roots[target].id;
        let parent = self.roots[target].parent.expect("popup roots always have a stable parent");
        if !self.root_is_visible_checked(parent)? {
            return Err(RootMutationError::InvalidPopupParent);
        }

        // A popup under the active modal must remain inside that modal's owned subtree. This prevents
        // a blocked ordinary window from opening a transient above or beneath the active modal group.
        if let Some(modal) = self.active_modal_root()
            && !self.is_descendant_or_self(parent, modal)
        {
            return Err(RootMutationError::InvalidPopupParent);
        }

        // The visible roots and parent links already describe the popup branch. Retain the target
        // when reopening it, or its popup parent when opening a submenu; all other visible popups
        // are displaced by the request.
        let target_visible = self.root_is_visible_checked(target_id)?;
        let keep = if target_visible {
            Some(target_id)
        } else if self.roots[self.root_index(parent)?].kind == WindowKind::Popup {
            Some(parent)
        } else {
            None
        };

        self.dismiss_popups_except(keep);
        self.roots[target].state.rect = anchor;
        self.roots[target].state.set_visible(true);

        self.raise_root_subtree(target_id);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Dismisses every visible popup except `keep` and its popup ancestors.
    ///
    /// The visible popup branch is derived directly from root chrome visibility and stable parent
    /// links. Dismissing deepest-first keeps submenu submissions in intuitive closing order.
    fn dismiss_popups_except(&mut self, keep: Option<RootId>) {
        let mut popups = Vec::new();
        for (index, entry) in self.roots.iter().enumerate() {
            if entry.kind != WindowKind::Popup {
                continue;
            }
            let retained = keep.is_some_and(|leaf| self.is_descendant_or_self(leaf, entry.id));
            if entry.state.visible && !retained {
                popups.push(index);
            }
        }
        popups.sort_by_key(|index| std::cmp::Reverse(self.roots[*index].z_index));

        // Closing deepest-first preserves intuitive submenu notification order.
        for index in popups {
            self.roots[index].dismiss_popup();
        }
    }

    /// Raises a registered root inside its effective layer.
    ///
    /// The operation cannot cross a stacking-band boundary. Bringing a dialog group forward closes
    /// a popup branch belonging to the previously active dialog before z-order changes.
    pub fn bring_root_to_front(&mut self, root: RootId) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;

        // Switching dialog groups invalidates transients belonging to the previous group. All
        // checked widget access completes before z-order changes, preserving mutation atomicity.
        if self.roots[index].kind == WindowKind::Modal && self.root_is_visible_checked(root)? {
            let active_modal = self.active_modal_root();
            if active_modal != Some(root) {
                self.dismiss_popups_except(None);
            }
        }
        self.raise_root_subtree(root);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Permanently unregisters a root and its complete owned-root subtree.
    ///
    /// Every descendant widget tree and weak root handle expires in the same operation. Destroying
    /// the front modal reveals the highest remaining visible modal automatically through z-order.
    ///
    /// There is intentionally no root-content replacement operation. Destroy and recreate a root
    /// to install a different root owner, or mutate descendants through their typed widget handles.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        let Ok(subtree) = self.subtree_ids(root) else { return false };
        if self.active_root.is_some_and(|active| self.is_descendant_or_self(active, root)) {
            self.active_root = None;
        }

        // Parent links point from descendants into the removed set, so no surviving edge needs
        // repair. Root identifiers are never reused and cannot later resolve to a different root.
        self.roots.retain(|entry| !subtree.contains(&entry.id));
        self.invalidate_ui_commit();
        true
    }

    /// Returns the registry index for one stable root identifier.
    fn root_index(&self, root: RootId) -> Result<usize, RootMutationError> {
        self.roots.iter().position(|entry| entry.id == root).ok_or(RootMutationError::UnknownRoot)
    }

    /// Collects one owned subtree in deterministic parent-before-descendant order.
    fn subtree_ids(&self, root: RootId) -> Result<Vec<RootId>, RootMutationError> {
        self.root_index(root)?;
        // Root IDs increase at registration, and immutable parents necessarily receive lower IDs
        // than descendants. Sorting by identity restores parent-first order after stacking sorts
        // have rearranged the registry, without storing or maintaining reverse child edges.
        let mut subtree = self
            .roots
            .iter()
            .filter(|entry| self.is_descendant_or_self(entry.id, root))
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        subtree.sort_by_key(|root| root.0);
        Ok(subtree)
    }

    /// Returns whether `root` is `ancestor` itself or belongs to its owned subtree.
    fn is_descendant_or_self(&self, root: RootId, ancestor: RootId) -> bool {
        let mut cursor = Some(root);
        while let Some(current) = cursor {
            if current == ancestor {
                return true;
            }
            cursor = self.root_index(current).ok().and_then(|index| self.roots[index].parent);
        }
        false
    }

    /// Returns checked visibility for a registered root during a fallible cross-root mutation.
    fn root_is_visible_checked(&self, root: RootId) -> Result<bool, RootMutationError> {
        let index = self.root_index(root)?;
        Ok(self.roots[index].state.visible)
    }

    /// Hides one root and every owned descendant as a single checked transaction.
    fn hide_root_subtree(&mut self, root: RootId) -> Result<(), RootMutationError> {
        let subtree = self.subtree_ids(root)?;
        let indices = subtree.iter().rev().copied().map(|root| self.root_index(root)).collect::<Result<Vec<_>, _>>()?;

        // Descendants close before owners so popup submissions observe a coherent remaining tree.
        for index in indices {
            if self.roots[index].state.visible && self.roots[index].kind == WindowKind::Popup {
                // Every popup closure has the same semantic meaning, whether requested directly or
                // caused by an ancestor closing, so coordinating menu controls always reconcile.
                self.roots[index].dismiss_popup();
            } else {
                self.roots[index].state.set_visible(false);
                self.roots[index].clear_transient_targets();
            }
        }

        if self.active_root.is_some_and(|active| self.is_descendant_or_self(active, root)) {
            self.active_root = None;
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Assigns a fresh z-index without applying modal policy.
    fn raise_root_index(&mut self, index: usize) {
        // next_z_index = previous_z_index + 1.
        self.last_zindex = self.last_zindex.saturating_add(1);
        self.roots[index].z_index = self.last_zindex;
    }

    /// Raises one logical root and its same-band descendants without changing their visual order.
    fn raise_root_subtree(&mut self, root: RootId) {
        let mut subtree = self.subtree_ids(root).expect("raised root subtree must remain registered");
        let band = self.roots[self.root_index(root).expect("raised root must remain registered")].effective_band;
        // Preserve the subtree's existing visual order instead of resetting siblings to creation
        // order. Hidden roots may move too, but showing one always gives it a fresh front position.
        subtree.sort_by_key(|root| self.roots[self.root_index(*root).expect("raised descendant must remain registered")].stack_key());
        for root in subtree {
            let index = self.root_index(root).expect("raised descendant must remain registered");
            // A fixed-layer owner does not reorder modal descendants. Roots in the selected band
            // retain their relative order while moving as one ordinary or modal visual group.
            if self.roots[index].effective_band == band {
                self.raise_root_index(index);
            }
        }
    }

    /// Orders the registry from back to front using the one authoritative stacking key.
    fn sort_roots_for_stacking(&mut self) {
        // Stable sorting preserves registry order when the saturating z-index counter eventually
        // produces ties. Every other ordering query uses WindowEntry::stack_key directly, so paint,
        // layout traversal, and hit testing cannot disagree about layer or popup precedence.
        self.roots.sort_by_key(WindowEntry::stack_key);
    }

    /// Returns the frontmost visible modal root.
    ///
    /// Modality needs no separate activation history: the same z-order that determines painting
    /// determines exclusive input, and hiding the front dialog reveals the one immediately below.
    fn active_modal_root(&self) -> Option<RootId> {
        let mut front = None;
        for entry in &self.roots {
            if entry.kind != WindowKind::Modal {
                continue;
            }
            if entry.state.visible && front.is_none_or(|(key, _)| entry.stack_key() > key) {
                front = Some((entry.stack_key(), entry.id));
            }
        }
        front.map(|(_, root)| root)
    }

    /// Routes one pointer event to manager-owned chrome and reports whether it was consumed.
    fn route_chrome_event(&mut self, root: RootId, event: &UiInputEvent) -> bool {
        let Ok(index) = self.root_index(root) else { return false };
        match event {
            UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => {
                match self.roots[index].state.chrome_part_at(*pos) {
                    Some(RootChromePart::Close) => {
                        // Hide the complete owned subtree before queueing Close so subscribers
                        // observe final visibility and popup state at their safe dispatch boundary.
                        self.hide_root_subtree(root).expect("chrome target must remain registered while closing");
                        let index = self.root_index(root).expect("closing a root must not destroy it");
                        self.roots[index].emit_submitted(crate::RootSubmitted::Close);
                        true
                    }
                    Some(RootChromePart::Resize) => {
                        // Chrome takes pointer ownership away from any application target in this root.
                        self.roots[index].tree.clear_transient_targets();
                        self.roots[index].state.interaction = RootInteraction::Resizing;
                        true
                    }
                    Some(RootChromePart::Title) => {
                        // Title movement and resizing share the same manager-level capture lifetime.
                        self.roots[index].tree.clear_transient_targets();
                        self.roots[index].state.interaction = RootInteraction::Moving;
                        true
                    }
                    None => false,
                }
            }
            UiInputEvent::MouseDrag { pos, delta, .. } => {
                let initial = self.roots[index].state.rect;
                match self.roots[index].state.interaction {
                    RootInteraction::Moving => {
                        self.roots[index].state.rect.x = initial.x.saturating_add(delta.x);
                        self.roots[index].state.rect.y = initial.y.saturating_add(delta.y);
                    }
                    RootInteraction::Resizing => {
                        let minimum = self.roots[index].state.geometry.minimum_outer;
                        self.roots[index].state.rect.width = initial.width.saturating_add(delta.x).max(minimum.width);
                        self.roots[index].state.rect.height = initial.height.saturating_add(delta.y).max(minimum.height);
                    }
                    RootInteraction::None => {
                        // An uncaptured drag still cannot pass through chrome painted above content.
                        return self.roots[index].state.chrome_part_at(*pos).is_some();
                    }
                }
                // Every accepted drag changes at least the gesture state; emit geometry only when
                // saturation or a minimum bound did not collapse the delta to the existing rect.
                if (
                    self.roots[index].state.rect.x,
                    self.roots[index].state.rect.y,
                    self.roots[index].state.rect.width,
                    self.roots[index].state.rect.height,
                ) != (initial.x, initial.y, initial.width, initial.height)
                {
                    self.roots[index].emit_changed();
                    self.invalidate_ui_commit();
                }
                true
            }
            UiInputEvent::MouseUp { button, .. } if button.intersects(MouseButton::LEFT) && self.roots[index].state.is_active() => {
                // The release ends chrome capture even when it occurs outside the root rectangle.
                self.roots[index].state.clear_interaction();
                true
            }
            _ => {
                // Hover, scroll, non-left presses, and uncaptured releases are observational, but
                // chrome still occludes the application tree anywhere it is painted above it.
                event.position().is_some_and(|pos| self.roots[index].state.chrome_part_at(pos).is_some())
            }
        }
    }

    /// Performs one synchronization layout, then one full update/layout pair per queued event.
    pub(crate) fn update(&mut self, dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        // Polling contexts have no application dispatcher, so the safe boundary performs no work.
        self.update_with(dimensions, atlas, &mut (), |_, _| false);
    }

    /// Performs the retained update while exposing each safe subscriber-dispatch boundary.
    pub(crate) fn update_with<DispatchState>(
        &mut self,
        dimensions: Dimensioni,
        atlas: &crate::AtlasHandle,
        dispatch_state: &mut DispatchState,
        mut after_event: impl FnMut(&mut Self, &mut DispatchState) -> bool,
    ) {
        self.ui_commit = None;
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);

        for entry in &mut self.roots {
            entry.tree.begin_update();
        }
        self.layout(viewport, atlas);
        // Subscriber invocations may already be waiting without a raw input event. If they mutate
        // retained state, commit that state before routing the first queued event.
        if after_event(self, dispatch_state) {
            self.layout(viewport, atlas);
        }

        loop {
            let event = self.input.pop_event();
            let Some(event) = event else { break };
            let input = self.input.snapshot();
            self.update_for_event(atlas, &event, input);
            // Application subscribers run only after the complete cross-root update has released
            // retained borrows. Their state/topology changes are therefore safe and become visible
            // to the layout immediately below, before routing the next raw input event.
            after_event(self, dispatch_state);
            self.layout(viewport, atlas);
        }
        self.ui_commit = Some(dimensions);
    }

    /// Synchronizes auto-size and layout for every visible root.
    fn layout(&mut self, viewport: Recti, atlas: &crate::AtlasHandle) {
        self.sort_roots_for_stacking();

        for index in 0..self.roots.len() {
            let visible = self.roots[index].state.visible;
            let options = self.roots[index].state.options;
            let rect = self.roots[index].state.rect;
            let auto_width = options.intersects(WindowOption::AUTO_WIDTH);
            let auto_height = options.intersects(WindowOption::AUTO_HEIGHT);
            if visible && options.intersects(WindowOption::AUTO_SIZE) {
                // Convert retained outer bounds to application-body bounds before measuring. The
                // measured body is then converted back to an outer intrinsic size exactly once.
                let shell = root_chrome_geometry(rect, Dimensioni::default(), &self.roots[index].state.name, options, &self.style, atlas);
                let horizontal_chrome = rect.width.saturating_sub(shell.body.width);
                let vertical_chrome = rect.height.saturating_sub(shell.body.height);
                let child_constraints = crate::Constraints::new(
                    if auto_width {
                        crate::AvailableSpace::Unbounded
                    } else {
                        crate::AvailableSpace::bounded(rect.width).shrink(horizontal_chrome)
                    },
                    if auto_height {
                        crate::AvailableSpace::Unbounded
                    } else {
                        crate::AvailableSpace::bounded(rect.height).shrink(vertical_chrome)
                    },
                );
                let child = self.roots[index].tree.measure(&self.style, atlas, child_constraints);
                let intrinsic = root_chrome_geometry(Recti::default(), child, &self.roots[index].state.name, options, &self.style, atlas).intrinsic_outer;
                self.roots[index].state.rect.width = if auto_width { intrinsic.width } else { rect.width };
                self.roots[index].state.rect.height = if auto_height { intrinsic.height } else { rect.height };
            }
        }

        for entry in &mut self.roots {
            if !entry.state.visible {
                entry.clear_transient_targets();
                continue;
            }

            // Commit chrome and body geometry together so layout, hit testing, and paint share it.
            entry.state.geometry = root_chrome_geometry(
                entry.state.rect,
                Dimensioni::default(),
                &entry.state.name,
                entry.state.options,
                &self.style,
                atlas,
            );
            entry.tree.layout(&self.style, atlas.clone(), entry.state.geometry.body, viewport);
        }
    }

    /// Routes and applies one normalized event, visiting every eligible tree exactly once.
    fn update_for_event(&mut self, atlas: &crate::AtlasHandle, event: &crate::UiInputEvent, input: crate::input::InputSnapshot) {
        if matches!(event, crate::UiInputEvent::MouseDown { .. }) {
            // Outside dismissal applies equally to ordinary and modal-originated popups. The input
            // root is resolved only afterward so this same press can reach the newly revealed
            // initiating window or modal without a one-event delay.
            self.dismiss_outside_popup(input.mouse_pos);
        }

        let hover_root = event.is_pointer().then(|| self.input_root_at(input.mouse_pos)).flatten();
        if matches!(event, crate::UiInputEvent::MouseDown { .. })
            && let Some(root) = hover_root
        {
            self.activate_pointer_root(root);
            self.bring_root_to_front(root)
                .expect("routed pointer root unavailable while fronting its visual group");
            // A new press may target any root. Transfer global pointer ownership before routing so
            // no previous root can retain a widget-level capture alongside the new press target.
            for entry in &mut self.roots {
                if entry.id != root && entry.has_capture() {
                    entry.clear_transient_targets();
                }
            }
        }

        // A drag remains confined to the root that owns pointer capture (or the front eligible
        // root when no widget captured the initiating press). Wheel input has no press lifecycle,
        // so it follows the topmost eligible root under the pointer just like hover. Keyboard and
        // text continue to use the independently activated root.
        let drag_root = self.drag_input_root();
        let pointer_root = match event {
            crate::UiInputEvent::MouseDrag { .. } => drag_root,
            _ => hover_root,
        };
        let keyboard_root = self.keyboard_input_root();
        let modal_root = self.active_modal_root();
        for index in 0..self.roots.len() {
            let root = self.roots[index].id;
            let visible = self.roots[index].state.visible;
            if visible && modal_root.is_none_or(|modal| self.is_descendant_or_self(root, modal)) {
                self.roots[index].tree.begin_input_event(pointer_root == Some(root), event);
            }
        }

        if event.is_pointer() {
            // Chrome capture and widget capture both own continuation, but only one can survive a
            // new press because activation above clears every competing root's transient targets.
            let chrome_root = matches!(event, crate::UiInputEvent::MouseDrag { .. } | crate::UiInputEvent::MouseUp { .. })
                .then(|| {
                    self.roots
                        .iter()
                        .find(|entry| modal_root.is_none_or(|modal| self.is_descendant_or_self(entry.id, modal)) && entry.state.is_active())
                        .map(|entry| entry.id)
                })
                .flatten();
            let mut capture_handled = chrome_root.is_some_and(|root| self.route_chrome_event(root, event));
            let capture_index = matches!(event, crate::UiInputEvent::MouseDrag { .. } | crate::UiInputEvent::MouseUp { .. })
                .then(|| {
                    self.roots
                        .iter()
                        .position(|entry| modal_root.is_none_or(|modal| self.is_descendant_or_self(entry.id, modal)) && entry.tree.has_capture())
                })
                .flatten();
            if !capture_handled && let Some(index) = capture_index {
                let entry = &mut self.roots[index];
                capture_handled = entry.tree.route_captured_pointer(&self.style, input.mouse_buttons, event).is_some();
            }

            if !capture_handled && let Some(root) = pointer_root {
                // Chrome is painted above content, so it gets the first chance at an ordinary hit.
                let chrome_handled = self.route_chrome_event(root, event);
                if !chrome_handled
                    && let Some(index) = self.roots.iter().position(|entry| entry.id == root)
                    && self.roots[index].tree.accepts_pointer_input()
                {
                    self.roots[index].tree.route_pointer(&self.style, event, input.mouse_buttons);
                }
            }
        } else if event.is_focus_input()
            && let Some(root) = keyboard_root
            && let Some(index) = self.roots.iter().position(|entry| entry.id == root)
        {
            let entry = &mut self.roots[index];
            entry.tree.route_focus(&self.style, event);
        }

        self.sort_roots_for_stacking();
        let modal_root = self.active_modal_root();
        for index in 0..self.roots.len() {
            let root = self.roots[index].id;
            let visible = self.roots[index].state.visible;
            if !visible || modal_root.is_some_and(|modal| !self.is_descendant_or_self(root, modal)) {
                self.roots[index].clear_transient_targets();
                continue;
            }
            self.roots[index].tree.update(&self.style, atlas.clone(), input);

            if !self.roots[index].state.visible {
                self.roots[index].clear_transient_targets();
            }
        }
        if self.active_root.is_some_and(|root| !self.root_is_visible(root)) {
            self.active_root = None;
        }
    }

    /// Paints and records the already committed trees without updating or laying them out.
    pub(crate) fn paint(&mut self, dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        self.display_list.clear();
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        self.sort_roots_for_stacking();
        for entry in &mut self.roots {
            if !entry.state.visible {
                continue;
            }
            // Chrome surrounds the application display list in the same order users perceive it.
            record_root_background(&mut self.display_list, viewport, &entry.state, &self.style);
            entry.tree.paint(&mut self.display_list, &self.style, atlas.clone());
            record_root_overlay(&mut self.display_list, viewport, &entry.state, &self.style, atlas);
        }
    }

    fn dismiss_outside_popup(&mut self, mouse: Vec2i) {
        // Retain the frontmost visible popup containing the press and its ancestors. A press in an
        // ancestor but outside its submenu closes only descendants; an outside press closes all.
        let keep = self
            .roots
            .iter()
            .filter(|entry| entry.kind == WindowKind::Popup && entry.state.visible && entry.state.rect.contains(&mouse))
            .max_by_key(|entry| entry.stack_key())
            .map(|entry| entry.id);
        self.dismiss_popups_except(keep);
    }

    /// Maps a pointer target to the ordinary root whose keyboard activation it represents.
    fn activation_source(&self, root: RootId) -> Option<RootId> {
        // Popups preserve the nearest owning ordinary window's keyboard activation. Encountering a
        // modal boundary ends the search because modal focus is derived independently.
        let mut cursor = Some(root);
        while let Some(current) = cursor {
            let index = self.root_index(current).ok()?;
            match self.roots[index].kind {
                WindowKind::Window => return Some(current),
                WindowKind::Modal => return None,
                WindowKind::Popup => cursor = self.roots[index].parent,
            }
        }
        None
    }

    /// Records pointer activation without changing any root's fixed stacking layer.
    fn activate_pointer_root(&mut self, root: RootId) {
        if let Some(source) = self.activation_source(root) {
            self.active_root = Some(source);
        }
    }

    /// Returns whether a registered root is currently visible.
    fn root_is_visible(&self, root: RootId) -> bool {
        let Ok(index) = self.root_index(root) else { return false };
        self.roots[index].state.visible
    }

    /// Returns the front visible root whose outer rectangle contains `point`.
    fn front_root_at(&self, point: Vec2i) -> Option<RootId> {
        self.roots
            .iter()
            .filter(|entry| entry.state.visible && entry.state.rect.contains(&point))
            .max_by_key(|entry| entry.stack_key())
            .map(|entry| entry.id)
    }

    /// Returns the visually frontmost visible root across all stacking bands.
    fn front_visible_root(&self) -> Option<RootId> {
        self.roots
            .iter()
            .filter(|entry| entry.state.visible)
            .max_by_key(|entry| entry.stack_key())
            .map(|entry| entry.id)
    }

    /// Returns the sole pointer-eligible root at `point` under the active modal policy.
    fn input_root_at(&self, point: Vec2i) -> Option<RootId> {
        let Some(modal) = self.active_modal_root() else {
            return self.front_root_at(point);
        };
        // The active modal's owned subtree forms one exclusive input group. Roots in fixed layers,
        // lower dialogs, and unrelated descendants remain painted but cannot receive this event.
        self.roots
            .iter()
            .filter(|entry| self.is_descendant_or_self(entry.id, modal) && entry.state.visible && entry.state.rect.contains(&point))
            .max_by_key(|entry| entry.stack_key())
            .map(|entry| entry.id)
    }

    /// Returns the root that exclusively accepts continuation of an in-progress pointer drag.
    fn drag_input_root(&self) -> Option<RootId> {
        let modal = self.active_modal_root();
        self.roots
            .iter()
            .find(|entry| modal.is_none_or(|modal| self.is_descendant_or_self(entry.id, modal)) && entry.has_capture())
            .map(|entry| entry.id)
            .or_else(|| self.front_input_root())
    }

    /// Returns the sole keyboard-eligible root without deriving activation from visual stacking.
    fn keyboard_input_root(&self) -> Option<RootId> {
        if let Some(modal) = self.active_modal_root() {
            return Some(modal);
        }
        self.roots
            .iter()
            .find(|entry| entry.has_capture())
            .map(|entry| entry.id)
            .or_else(|| self.active_root.filter(|root| self.root_is_visible(*root)))
            .or_else(|| self.front_input_root().and_then(|root| self.activation_source(root)))
    }

    /// Returns the front visible root under modal eligibility for pointer confinement fallback.
    fn front_input_root(&self) -> Option<RootId> {
        let Some(modal) = self.active_modal_root() else {
            return self.front_visible_root();
        };
        self.roots
            .iter()
            .filter(|entry| self.is_descendant_or_self(entry.id, modal))
            .filter(|entry| entry.state.visible)
            .max_by_key(|entry| entry.stack_key())
            .map(|entry| entry.id)
    }

    const fn default_popup_options() -> WindowOption {
        WindowOption::FRAME
            .union(WindowOption::AUTO_SIZE)
            .union(WindowOption::NO_RESIZE)
            .union(WindowOption::NO_TITLE)
    }

    #[cfg(test)]
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        let mut names = self
            .roots
            .iter()
            .filter_map(|entry| entry.state.visible.then(|| (entry.stack_key(), entry.state.name.clone())))
            .collect::<Vec<_>>();
        names.sort_by_key(|(key, _)| *key);
        names.into_iter().map(|(_, name)| name).collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_zindex(&self, root: RootId) -> Option<i32> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.z_index)
    }

    /// Returns a retained root's displayed name for behavioral tests.
    #[cfg(test)]
    pub(crate) fn debug_root_name(&self, root: RootId) -> Option<String> {
        // Clone the private string so tests cannot acquire a borrow into manager-owned state.
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.state.name.clone())
    }

    /// Returns a retained root's authoritative screen-space rectangle for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_rect(&self, root: RootId) -> Option<Recti> {
        // Recti is copied, preserving the manager as the sole mutable geometry owner.
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.state.rect)
    }

    /// Returns whether a retained root currently participates in traversal.
    #[cfg(test)]
    pub(crate) fn debug_root_visible(&self, root: RootId) -> Option<bool> {
        // Unknown IDs remain distinguishable from registered hidden roots through Option.
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.state.visible)
    }

    /// Returns whether either manager-owned chrome gesture is active.
    #[cfg(test)]
    pub(crate) fn debug_root_active(&self, root: RootId) -> Option<bool> {
        // This exposes behavioral capture state without restoring a public mutable chrome handle.
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.state.is_active())
    }

    /// Returns whether title movement currently owns manager pointer capture.
    #[cfg(test)]
    pub(crate) fn debug_root_moving(&self, root: RootId) -> Option<bool> {
        // Exact interaction inspection keeps drag tests independent of application runtime capture.
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.state.interaction == RootInteraction::Moving)
    }

    /// Returns whether resize chrome currently owns manager pointer capture.
    #[cfg(test)]
    pub(crate) fn debug_root_resizing(&self, root: RootId) -> Option<bool> {
        // Exact interaction inspection distinguishes resize behavior from title movement.
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.state.interaction == RootInteraction::Resizing)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_layer_binding(&self, root: RootId) -> Option<LayerBinding> {
        self.root_layer_binding(root).ok()
    }

    #[cfg(test)]
    pub(crate) fn debug_active_root(&self) -> Option<RootId> {
        self.active_root
    }

    #[cfg(test)]
    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        self.active_modal_root()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_body(&self, root: RootId, atlas: &crate::AtlasHandle) -> Option<Recti> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        Some(
            root_chrome_geometry(
                entry.state.rect,
                Dimensioni::default(),
                &entry.state.name,
                entry.state.options,
                &self.style,
                atlas,
            )
            .body,
        )
    }

    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self, root: RootId) -> Option<Dimensioni> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.tree.content_size())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_runtime_metrics(&self, root: RootId) -> Option<crate::ui_node::RuntimeMetrics> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.tree.metrics())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_has_pointer_capture(&self, root: RootId) -> Option<bool> {
        self.roots.iter().find(|entry| entry.id == root).map(WindowEntry::has_capture)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_node_count(&self, root: RootId) -> Option<usize> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        Some(entry.tree.node_count())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        entry.tree.node_rect(node)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_chrome(&self, root: RootId, atlas: &crate::AtlasHandle) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        let geometry = root_chrome_geometry(
            entry.state.rect,
            Dimensioni::default(),
            &entry.state.name,
            entry.state.options,
            &self.style,
            atlas,
        );
        Some((geometry.title, geometry.close, geometry.resize))
    }
}
