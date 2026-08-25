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
use crate::{MouseButton, Node, RootHandle, RootChrome, TypedWidgetHandle, UiInputEvent, Vec2i, rect};

use super::root_chrome::{create_root_chrome, record_root_overlay, root_handle, RootChromeParameters};
#[cfg(test)]
use super::root_chrome::root_chrome_geometry;

/// Failure reported when the root registry cannot complete a checked state mutation.
///
/// These errors describe registry identity, cross-root policy, and checked access to the retained
/// root widget. They deliberately live beside the private `WindowEntry` registry records and the
/// mutation implementations rather than in `root_chrome`: [`RootChrome`] owns local presentation
/// state, while the registry alone knows whether an identifier is registered and which private
/// `WindowKind` policy applies to it.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootMutationError {
    /// The supplied [`RootId`] does not identify a root currently owned by the context.
    ///
    /// Destroying a root invalidates its identifier for all later checked mutations. Root
    /// identifiers are never reused, so this result cannot accidentally address a newer root.
    UnknownRoot,
    /// The retained root widget is already borrowed by an active typed-access closure.
    ///
    /// Checked mutations return this error instead of panicking or partially applying a cross-root
    /// transaction. The caller may retry after the conflicting access closure has returned.
    Borrowed,
    /// The requested ownership edge is invalid for the child root kind.
    ///
    /// Ordinary child windows and modal dialogs require a window or dialog parent. Popups may be
    /// owned by any root kind so submenu popups can be children of other popup roots.
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
    /// This includes a hidden parent, a popup parent outside the active popup branch, or a parent
    /// outside the active modal subtree.
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

    /// Returns the weak typed handle for the popup's concrete root-chrome widget.
    pub fn widget(&self) -> &TypedWidgetHandle<RootChrome> {
        // Reuse RootHandle's non-owning chrome access; this does not extend the popup lifetime.
        self.root.widget()
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
    /// An ordinary independently interactive application window.
    Window,
    /// A modal dialog that excludes input to every root below it.
    Modal,
    /// A transient root governed by exclusive visibility and outside-press dismissal policy.
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
    root: Node,
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

    fn route_pointer(&mut self, style: &Style, event: &UiInputEvent, root_chrome_hit: bool, mouse_buttons: MouseButton) {
        if let Some((owner, result)) = self.runtime.route_root_input_event_to_node_ref(&mut self.root, style, event, root_chrome_hit) {
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
    /// Direct logical children in creation order.
    ///
    /// Children remain independent screen-space roots. This collection controls ownership,
    /// lifecycle, inherited stacking policy, and overlay ancestry rather than layout or clipping.
    children: Vec<RootId>,
    /// Caller-selected layer used only by an independent top-level window.
    fixed_layer: u8,
    /// Window-manager visibility used by ownership and modal policy without borrowing root chrome.
    ///
    /// Manager mutations update this value with `RootChrome`. Root-local close handling can only
    /// change visibility from true to false and is reconciled immediately after its routed event.
    /// Keeping this small lifecycle fact beside the tree makes cross-root operations independent of
    /// an application access closure temporarily borrowing one root widget.
    visible: bool,
    /// Cached band derived from `fixed_layer`, `parent`, and modal ancestry.
    ///
    /// The cache keeps sorting allocation-free. Tree mutations refresh it synchronously, so it is
    /// never a second source of ownership truth.
    effective_band: StackBand,
    /// Monotonic ordering value inside the effective band and transient tier.
    pub(super) z_index: i32,
    /// Last explicit modal activation order, independent of ordinary fronting operations.
    ///
    /// The ownership tree answers which roots belong to a modal group, but it cannot encode which
    /// visible sibling dialog was shown most recently. Retaining that one scalar per dialog
    /// preserves modal restoration without rebuilding a separate modal stack.
    modal_activation: i32,
    /// Weak typed access to the root chrome owned by `tree`.
    pub(super) root_widget: TypedWidgetHandle<RootChrome>,
    /// Sole strong owner of the complete retained widget subtree for this screen-space root.
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

    /// Clears runtime interaction identities and the application-visible root chrome mode.
    fn clear_transient_targets(&mut self) {
        // Generic descendant widgets reconcile private modes from their next inactive update.
        // RootChrome is different because callers can observe `is_active` immediately after a
        // window-manager operation, so its mode changes in the same ownership transaction.
        self.tree.clear_transient_targets();
        self.root_widget
            .try_update(RootChrome::clear_interaction_silent)
            .expect("registered root widget unavailable while clearing transient targets");
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
        if let Some(parent_index) = parent_index
            && kind != WindowKind::Popup
            && self.roots[parent_index].kind == WindowKind::Popup
        {
            return Err(RootMutationError::InvalidRootParent);
        }
        if visible
            && let Some(parent_index) = parent_index
            && !self.roots[parent_index].visible
        {
            return Err(RootMutationError::InvalidRootParent);
        }
        // Allocate lifecycle identity before construction; IDs are never derived from node identity.
        let id = self.next_root_id();
        // Root chrome returns one concrete Container and the weak typed widget handle Context registers.
        let (root_widget, changed, submitted, root) = create_root_chrome(RootChromeParameters {
            name: name.to_owned(),
            options,
            rect,
            visible,
            content,
        });
        // Finish the private branch with the same Node::container boundary used by application code.
        let root = Node::container(root);
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
        // Context is the sole tree owner; the entry's typed widget handle cannot retain the root.
        self.roots.push(WindowEntry {
            id,
            kind,
            parent,
            children: Vec::new(),
            fixed_layer: DEFAULT_LAYER,
            visible,
            effective_band,
            z_index,
            modal_activation: -1,
            root_widget: root_widget.clone(),
            tree: WidgetTree::new(root),
        });
        // Record the reverse edge only after the child entry exists, keeping both directions
        // synchronized at every observable mutation boundary.
        if let Some(parent_index) = parent_index {
            self.roots[parent_index].children.push(id);
        }
        // New topology requires a layout commit before rendering or pointer routing.
        self.invalidate_ui_commit();
        Ok(root_handle(id, root_widget, changed, submitted))
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
    /// The child remains independently positioned in screen space. Its parent controls recursive
    /// lifetime and supplies the inherited stacking band, but never clips or lays out the child.
    pub fn create_child_window(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // Child windows begin visible, so registration also verifies that the supplied owner is
        // currently visible and can uphold the all-visible-ancestors invariant.
        self.register_root(Some(parent), WindowKind::Window, name, rect, content, WindowOption::FRAME, true)
    }

    /// Creates a hidden retained dialog owned by one stable parent root.
    ///
    /// Show it with [`crate::Context::set_root_visible`]. A visible dialog enters the dedicated
    /// modal layer and its owned subtree becomes the active modal input group until the dialog is
    /// hidden or destroyed.
    pub fn create_dialog(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // Dialogs begin hidden and therefore may be prepared beneath a currently hidden owner.
        self.register_root(Some(parent), WindowKind::Modal, name, rect, content, WindowOption::FRAME, false)
    }

    /// Creates a hidden auto-sized popup owned by one stable parent root.
    ///
    /// Show it through [`crate::Context::show_popup`] or [`crate::Context::show_popup_at`]. Its
    /// parent supplies the inherited effective band and recursive lifetime. Within that band, the
    /// popup's transient tier is above ordinary roots; it never crosses a higher fixed layer. A
    /// popup owned by the active modal subtree participates in that exclusive input group. An
    /// outside press or competing popup request hides it and records a submission.
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
        self.update_root_widget(root, |state| state.set_name_silent(name))
    }

    /// Replaces a root rectangle silently while retaining any compatible captured chrome mode.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) -> Result<(), RootMutationError> {
        self.update_root_widget(root, |state| state.set_rect_silent(rect))
    }

    /// Replaces a root size silently without changing its origin.
    pub fn set_root_size(&mut self, root: RootId, size: Dimensioni) -> Result<(), RootMutationError> {
        self.update_root_widget(root, |state| state.set_size_silent(size))
    }

    /// Replaces root chrome options silently.
    pub fn set_root_options(&mut self, root: RootId, options: WindowOption) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        let was_active = self.roots[index]
            .root_widget
            .try_read(RootChrome::is_active)
            .ok_or(RootMutationError::Borrowed)?;
        self.update_root_widget(root, |state| state.set_options_silent(options))?;
        let active = self.roots[index]
            .root_widget
            .try_read(RootChrome::is_active)
            .unwrap_or_else(|| self.root_access_failure(index));
        if was_active && !active {
            self.roots[index].clear_transient_targets();
        }
        Ok(())
    }

    /// Assigns one independent top-level window to a fixed application stacking layer.
    ///
    /// Layers are ordered from zero at the bottom through fifteen at the top. Owned windows, popups,
    /// and dialogs reject direct assignment because their effective bands come from ownership or
    /// modal policy. Every non-modal descendant follows the new band in the same transaction.
    pub fn set_root_layer(&mut self, root: RootId, layer: u8) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        if layer > MAX_LAYER {
            return Err(RootMutationError::InvalidLayer(layer));
        }
        if self.roots[index].kind != WindowKind::Window || self.roots[index].parent.is_some() {
            return Err(RootMutationError::ManagedLayer);
        }

        // Store the only caller-controlled layer value, then refresh cached descendant bands by
        // following the authoritative ownership tree parent-first.
        self.roots[index].fixed_layer = layer;
        self.roots[index].effective_band = StackBand::Fixed(layer);
        self.refresh_subtree_bands(root)?;
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
            (WindowKind::Window, None) => LayerBinding::Fixed(self.roots[index].fixed_layer),
            (WindowKind::Popup, None) => unreachable!("popup roots always have a stable parent"),
        })
    }

    /// Shows or hides a retained root, preserving its tree and concrete widget state.
    ///
    /// Showing a dialog makes it the front modal root. Hiding any root hides its complete owned
    /// subtree; popup descendants record dismissal, while the explicitly hidden target does not.
    /// Showing a popup is rejected because anchored popup policy must reconcile the active branch;
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

        // Preflight the target before dismissing another overlay so a conflicting application borrow
        // cannot leave the previous state closed without making the requested root visible.
        self.roots[target].root_widget.try_update(|_| {}).ok_or(RootMutationError::Borrowed)?;
        let active_modal = self.active_modal_root();
        if kind == WindowKind::Modal {
            self.dismiss_visible_popup()?;
        }
        self.roots[target]
            .root_widget
            .try_update(|state| state.set_visible_silent(true))
            .ok_or(RootMutationError::Borrowed)?;
        self.roots[target].visible = true;

        match kind {
            WindowKind::Modal => self.activate_modal(root),
            WindowKind::Window => {
                self.raise_root_subtree(root);
                // Raising an ordinary root never steals active modal ownership. Re-raise the modal
                // subtree captured before this transaction so modal sibling ordering remains stable.
                if let Some(modal) = active_modal {
                    self.raise_root_subtree(modal);
                }
            }
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

        // Retain the target itself when it is already on the active branch, otherwise retain its
        // popup parent. A non-popup parent starts a new globally exclusive popup branch.
        let keep = if self.active_popup_branch_contains(target_id) {
            Some(target_id)
        } else if self.roots[self.root_index(parent)?].kind == WindowKind::Popup {
            if !self.active_popup_branch_contains(parent) {
                return Err(RootMutationError::InvalidPopupParent);
            }
            Some(parent)
        } else {
            None
        };
        let displaced = self.popup_branch_after(keep)?;

        // Preflight every checked mutable root access before changing visibility so one conflicting
        // application borrow cannot expose a partially replaced popup chain.
        for index in displaced.iter().copied().chain(std::iter::once(target)) {
            self.roots[index].root_widget.try_update(|_| {}).ok_or(RootMutationError::Borrowed)?;
        }
        for index in displaced {
            self.roots[index]
                .root_widget
                .try_update(RootChrome::dismiss_popup)
                .ok_or(RootMutationError::Borrowed)?;
            self.roots[index].clear_transient_targets();
            self.roots[index].visible = false;
        }
        self.roots[target]
            .root_widget
            .try_update(|state| {
                state.set_rect_silent(anchor);
                state.set_visible_silent(true);
            })
            .ok_or(RootMutationError::Borrowed)?;
        self.roots[target].visible = true;

        // Commit the one active popup leaf only after all retained widget mutations succeed. Parent
        // links recover every retained ancestor without duplicating the chain in manager state.
        self.active_popup = Some(target_id);
        self.raise_root_subtree(target_id);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Dismisses the sole visible popup branch through checked retained access.
    fn dismiss_visible_popup(&mut self) -> Result<(), RootMutationError> {
        self.dismiss_popup_branch_after(None)
    }

    /// Dismisses the active popup descendants after `keep`, deepest first.
    ///
    /// `keep` must be a popup on the active branch. Passing `None` dismisses the complete branch.
    /// The operation preflights every mutable widget borrow so it cannot expose a partially closed
    /// sequence when application code currently holds one root state.
    fn dismiss_popup_branch_after(&mut self, keep: Option<RootId>) -> Result<(), RootMutationError> {
        let popups = self.popup_branch_after(keep)?;
        for index in &popups {
            self.roots[*index].root_widget.try_update(|_| {}).ok_or(RootMutationError::Borrowed)?;
        }
        for index in popups {
            self.roots[index]
                .root_widget
                .try_update(RootChrome::dismiss_popup)
                .ok_or(RootMutationError::Borrowed)?;
            self.roots[index].clear_transient_targets();
            self.roots[index].visible = false;
        }
        self.active_popup = keep;
        Ok(())
    }

    /// Raises a registered root inside its effective layer and reports whether it exists.
    ///
    /// The operation cannot cross a numeric application-layer boundary, and the active modal
    /// dialog remains above every application root.
    pub fn bring_root_to_front(&mut self, root: RootId) -> bool {
        if !self.roots.iter().any(|entry| entry.id == root) {
            return false;
        }
        // Capture modal ownership before changing z-order. Raising an ordinary root or an inactive
        // dialog must not steal the exclusive input group from the current front modal.
        let active_modal = self.active_modal_root();
        self.raise_root_subtree(root);
        if let Some(modal) = active_modal
            && modal != root
        {
            self.raise_root_subtree(modal);
        }
        self.invalidate_ui_commit();
        true
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
        let parent = self.roots[self.root_index(root).expect("destroy root was just resolved")].parent;

        // If the active popup leaf is being destroyed, retain only a popup parent outside the
        // removed subtree. The parent relationship is resolved before entries are dropped.
        if self.active_popup.is_some_and(|active| self.is_descendant_or_self(active, root)) {
            self.active_popup = parent.filter(|parent| {
                self.root_index(*parent)
                    .is_ok_and(|index| self.roots[index].kind == WindowKind::Popup && self.root_is_visible(*parent))
            });
        }
        if self.active_root.is_some_and(|active| self.is_descendant_or_self(active, root)) {
            self.active_root = None;
        }

        // Remove the forward edge from the surviving parent, then drop every subtree entry. Root
        // identifiers are never reused, so no surviving handle can accidentally address a new root.
        if let Some(parent) = parent {
            let parent_index = self.root_index(parent).expect("owned root parent must remain registered");
            self.roots[parent_index].children.retain(|child| *child != root);
        }
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
        let mut result = Vec::new();
        let mut pending = vec![root];
        while let Some(current) = pending.pop() {
            let index = self.root_index(current)?;
            result.push(current);
            // Push in reverse so the caller observes the original child creation order.
            pending.extend(self.roots[index].children.iter().rev().copied());
        }
        Ok(result)
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

    /// Refreshes cached effective bands for one complete subtree from authoritative parent edges.
    fn refresh_subtree_bands(&mut self, root: RootId) -> Result<(), RootMutationError> {
        let subtree = self.subtree_ids(root)?;
        for child in subtree.into_iter().skip(1) {
            let child_index = self.root_index(child)?;
            let band = if self.roots[child_index].kind == WindowKind::Modal {
                StackBand::Modal
            } else {
                let parent = self.roots[child_index].parent.expect("owned descendant must have a parent");
                let parent_index = self.root_index(parent)?;
                self.roots[parent_index].effective_band
            };
            self.roots[child_index].effective_band = band;
        }
        Ok(())
    }

    /// Returns checked visibility for a registered root during a fallible cross-root mutation.
    fn root_is_visible_checked(&self, root: RootId) -> Result<bool, RootMutationError> {
        let index = self.root_index(root)?;
        self.roots[index]
            .root_widget
            .try_read(RootChrome::is_visible)
            .ok_or(RootMutationError::Borrowed)
    }

    /// Returns the popup parent of `root`, stopping when ownership reaches a non-popup root.
    fn popup_parent(&self, root: RootId) -> Option<RootId> {
        let index = self.root_index(root).ok()?;
        let parent = self.roots[index].parent?;
        let parent_index = self.root_index(parent).ok()?;
        (self.roots[parent_index].kind == WindowKind::Popup).then_some(parent)
    }

    /// Returns whether one popup appears on the branch ending at `active_popup`.
    fn active_popup_branch_contains(&self, popup: RootId) -> bool {
        let mut cursor = self.active_popup;
        while let Some(current) = cursor {
            if current == popup {
                return true;
            }
            cursor = self.popup_parent(current);
        }
        false
    }

    /// Resolves active popup registry indices after an optional retained ancestor.
    ///
    /// Indices are returned deepest-first, which is both the semantic dismissal order and the order
    /// needed to close descendants before their parent. A requested retained root outside the active
    /// branch is rejected rather than silently corrupting the active leaf invariant.
    fn popup_branch_after(&self, keep: Option<RootId>) -> Result<Vec<usize>, RootMutationError> {
        let mut result = Vec::new();
        let mut cursor = self.active_popup;
        while let Some(current) = cursor {
            if Some(current) == keep {
                return Ok(result);
            }
            result.push(self.root_index(current)?);
            cursor = self.popup_parent(current);
        }
        if keep.is_some() {
            return Err(RootMutationError::InvalidPopupParent);
        }
        Ok(result)
    }

    /// Hides one root and every owned descendant as a single checked transaction.
    fn hide_root_subtree(&mut self, root: RootId) -> Result<(), RootMutationError> {
        self.hide_root_subtree_impl(root, true)
    }

    /// Implements recursive hiding with explicit control over target-popup submission.
    ///
    /// Publicly hiding a popup is silent, while a popup made invalid by a parent-local close must
    /// submit dismissal so its coordinating control can reconcile semantic open state.
    fn hide_root_subtree_impl(&mut self, root: RootId, silent_target_popup: bool) -> Result<(), RootMutationError> {
        let subtree = self.subtree_ids(root)?;
        let target_kind = self.roots[self.root_index(root)?].kind;
        let indices = subtree.iter().rev().copied().map(|root| self.root_index(root)).collect::<Result<Vec<_>, _>>()?;

        // Preflight every root before mutating any visibility. This preserves the existing atomic
        // checked-access contract even when a parent owns many independent retained trees.
        for index in &indices {
            self.roots[*index].root_widget.try_update(|_| {}).ok_or(RootMutationError::Borrowed)?;
        }
        for index in indices {
            let id = self.roots[index].id;
            let visible = self.roots[index]
                .root_widget
                .try_read(RootChrome::is_visible)
                .ok_or(RootMutationError::Borrowed)?;
            if visible && self.roots[index].kind == WindowKind::Popup && (id != root || target_kind != WindowKind::Popup || !silent_target_popup) {
                // Descendant popup closure is semantic dismissal. Explicitly hiding the target popup
                // remains silent, matching the public generic-visibility contract.
                self.roots[index]
                    .root_widget
                    .try_update(RootChrome::dismiss_popup)
                    .ok_or(RootMutationError::Borrowed)?;
            } else {
                self.roots[index]
                    .root_widget
                    .try_update(|state| state.set_visible_silent(false))
                    .ok_or(RootMutationError::Borrowed)?;
            }
            self.roots[index].clear_transient_targets();
            self.roots[index].visible = false;
        }

        // Active popup state falls back to a visible popup parent outside the hidden subtree. Other
        // hidden subtrees cannot intersect the globally unique active popup branch.
        if self.active_popup.is_some_and(|active| self.is_descendant_or_self(active, root)) {
            self.active_popup = self.popup_parent(root).filter(|parent| self.root_is_visible(*parent));
        }
        if self.active_root.is_some_and(|active| self.is_descendant_or_self(active, root)) {
            self.active_root = None;
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    fn update_root_widget(&mut self, root: RootId, update: impl FnOnce(&mut RootChrome)) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        if self.roots[index].root_widget.try_update(update).is_some() {
            self.invalidate_ui_commit();
            Ok(())
        } else if self.roots[index].root_widget.is_alive() {
            Err(RootMutationError::Borrowed)
        } else {
            panic!("registered root lost its persistent RootChrome owner")
        }
    }

    fn root_access_failure(&self, index: usize) -> ! {
        if self.roots[index].root_widget.is_alive() {
            panic!("registered root widget is unexpectedly borrowed during traversal")
        }
        panic!("registered root lost its persistent RootChrome owner")
    }

    /// Assigns a fresh z-index without applying modal policy.
    fn raise_root_index(&mut self, index: usize) {
        // next_z_index = previous_z_index + 1.
        self.last_zindex = self.last_zindex.saturating_add(1);
        self.roots[index].z_index = self.last_zindex;
    }

    /// Raises one logical root together with its owned subtree, parent before descendants.
    fn raise_root_subtree(&mut self, root: RootId) {
        let subtree = self.subtree_ids(root).expect("raised root subtree must remain registered");
        for root in subtree {
            let index = self.root_index(root).expect("raised descendant must remain registered");
            self.raise_root_index(index);
        }
    }

    /// Orders the registry from back to front using the one authoritative stacking key.
    fn sort_roots_for_stacking(&mut self) {
        // Stable sorting preserves registry order when the saturating z-index counter eventually
        // produces ties. Every other ordering query uses WindowEntry::stack_key directly, so paint,
        // layout traversal, and hit testing cannot disagree about layer or popup precedence.
        self.roots.sort_by_key(WindowEntry::stack_key);
    }

    /// Makes one visible dialog the front modal input group and clears ineligible interaction state.
    ///
    /// Owned descendants may join the modal group; every root outside that subtree remains blocked
    /// until no modal root is visible.
    fn activate_modal(&mut self, root: RootId) {
        let index = self.root_index(root).expect("modal root must remain registered");
        assert!(self.roots[index].kind == WindowKind::Modal, "modal root must have modal kind");

        for index in 0..self.roots.len() {
            if !self.is_descendant_or_self(self.roots[index].id, root) {
                self.roots[index].clear_transient_targets();
            }
        }
        self.raise_root_subtree(root);
        // Record explicit showing after raising the complete modal group. Later fronting operations
        // may change paint z-order, but they must not rewrite which sibling dialog is restored.
        let index = self.root_index(root).expect("activated modal root must remain registered");
        self.roots[index].modal_activation = self.last_zindex;
    }

    /// Returns the most recently activated visible modal root.
    fn active_modal_root(&self) -> Option<RootId> {
        self.roots
            .iter()
            .filter(|entry| entry.kind == WindowKind::Modal && entry.visible)
            .max_by_key(|entry| entry.modal_activation)
            .map(|entry| entry.id)
    }

    /// Reconciles state-local root closure with logical ownership after each routed event.
    ///
    /// Root chrome can close itself without calling `set_root_visible`. Any still-visible child of a
    /// newly hidden parent is therefore found here and its complete subtree is closed. Popup targets
    /// submit dismissal because the parent closure, not an explicit popup hide, invalidated them.
    fn reconcile_closed_subtrees(&mut self) {
        // Root chrome can only close itself, never show itself. Synchronize that one local
        // transition before consulting parent visibility or modal ordering.
        for index in 0..self.roots.len() {
            if self.roots[index].visible {
                self.roots[index].visible = self.roots[index]
                    .root_widget
                    .try_read(RootChrome::is_visible)
                    .unwrap_or_else(|| self.root_access_failure(index));
            }
        }
        loop {
            let orphan = self.roots.iter().find_map(|entry| {
                let parent = entry.parent?;
                (entry.visible && !self.root_is_visible(parent)).then_some(entry.id)
            });
            let Some(orphan) = orphan else { break };
            self.hide_root_subtree_impl(orphan, false)
                .expect("orphaned root subtree is unexpectedly borrowed after input routing");
        }

        // A popup root can also close itself. Its owned descendants were reconciled above, so the
        // surviving active leaf is simply its nearest still-visible popup parent.
        if let Some(active) = self.active_popup
            && !self.root_is_visible(active)
        {
            self.active_popup = self.popup_parent(active).filter(|parent| self.root_is_visible(*parent));
        }
        if self.active_root.is_some_and(|active| !self.root_is_visible(active)) {
            self.active_root = None;
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
            self.reconcile_closed_subtrees();
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
            let (visible, options, rect) = self.roots[index]
                .root_widget
                .try_read(|state| (state.is_visible(), state.options(), state.rect()))
                .unwrap_or_else(|| self.root_access_failure(index));
            let auto_width = options.intersects(WindowOption::AUTO_WIDTH);
            let auto_height = options.intersects(WindowOption::AUTO_HEIGHT);
            if visible && options.intersects(WindowOption::AUTO_SIZE) {
                // An automatic root axis asks for intrinsic size. A retained axis supplies its
                // programmed outer bound so root chrome can offer the exact remaining body extent
                // without exposing frame or padding arithmetic to the application.
                let constraints = crate::Constraints::new(
                    if auto_width {
                        crate::AvailableSpace::Unbounded
                    } else {
                        crate::AvailableSpace::bounded(rect.width)
                    },
                    if auto_height {
                        crate::AvailableSpace::Unbounded
                    } else {
                        crate::AvailableSpace::bounded(rect.height)
                    },
                );
                let tree = &mut self.roots[index].tree;
                let size = tree.measure(&self.style, atlas, constraints);
                let size = Dimensioni::new(
                    if auto_width { size.width } else { rect.width },
                    if auto_height { size.height } else { rect.height },
                );
                self.roots[index]
                    .root_widget
                    .try_update(|state| state.set_size_silent(size))
                    .unwrap_or_else(|| self.root_access_failure(index));
            }
        }

        for entry in &mut self.roots {
            let (visible, rect) = entry
                .root_widget
                .try_read(|state| (state.is_visible(), state.rect()))
                .expect("registered root state unavailable during frame");
            if !visible {
                entry.clear_transient_targets();
                continue;
            }

            entry.tree.layout(&self.style, atlas.clone(), rect, viewport);
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
            let _ = self.bring_root_to_front(root);
            // A new press may target any root. Transfer global pointer ownership before routing so
            // no previous root can retain a widget-level capture alongside the new press target.
            for entry in &mut self.roots {
                if entry.id != root && entry.tree.has_capture() {
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
            let visible = self.roots[index]
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable before input update");
            if visible && modal_root.is_none_or(|modal| self.is_descendant_or_self(root, modal)) {
                self.roots[index].tree.begin_input_event(pointer_root == Some(root), event);
            }
        }

        if event.is_pointer() {
            // Widget capture owns drag continuation and release cleanup only. Wheel, hover, and new
            // presses still perform ordinary hit routing, constrained by pointer_root above.
            let capture_index = matches!(event, crate::UiInputEvent::MouseDrag { .. } | crate::UiInputEvent::MouseUp { .. })
                .then(|| {
                    self.roots
                        .iter()
                        .position(|entry| modal_root.is_none_or(|modal| self.is_descendant_or_self(entry.id, modal)) && entry.tree.has_capture())
                })
                .flatten();
            let mut capture_handled = false;
            if let Some(index) = capture_index {
                let entry = &mut self.roots[index];
                capture_handled = entry.tree.route_captured_pointer(&self.style, input.mouse_buttons, event).is_some();
            }

            if !capture_handled
                && let Some(root) = pointer_root
                && let Some(index) = self.roots.iter().position(|entry| entry.id == root)
            {
                let entry = &mut self.roots[index];
                if entry.tree.accepts_pointer_input() {
                    // Root chrome is a window-manager overlay, not a customizable container hit
                    // surface. Resolve it here before generic allocation-based tree targeting.
                    let root_chrome_hit = entry
                        .root_widget
                        .try_read(|state| event.position().is_some_and(|pos| state.pointer_hits_chrome(pos)))
                        .expect("registered root state unavailable during pointer targeting");
                    // Capture is updated only after the selected target and its ancestors have
                    // finished classifying the event.
                    entry.tree.route_pointer(&self.style, event, root_chrome_hit, input.mouse_buttons);
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
            let visible = self.roots[index]
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable during input update");
            if !visible || modal_root.is_some_and(|modal| !self.is_descendant_or_self(root, modal)) {
                self.roots[index].clear_transient_targets();
                continue;
            }
            self.roots[index].tree.update(&self.style, atlas.clone(), input);

            let visible = self.roots[index]
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable after root update");
            if !visible {
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
            let visible = entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable during paint");
            if !visible {
                continue;
            }
            entry.tree.paint(&mut self.display_list, &self.style, atlas.clone());
            let root_style = entry
                .root_widget
                .try_style_override()
                .expect("registered root style unavailable during overlay paint")
                .unwrap_or(self.style);
            entry
                .root_widget
                .try_read(|state| record_root_overlay(&mut self.display_list, viewport, state, &root_style, atlas))
                .expect("registered root state unavailable during overlay paint");
        }
    }

    fn dismiss_outside_popup(&mut self, mouse: Vec2i) {
        // Descendants are always raised after their ancestors. Walk from the active leaf upward and
        // retain the first popup containing the press; a press outside the branch retains nothing.
        let mut keep = None;
        let mut cursor = self.active_popup;
        while let Some(popup) = cursor {
            let index = self.root_index(popup).expect("active popup must remain registered");
            let contains = self.roots[index]
                .root_widget
                .try_read(|state| state.is_visible() && state.rect().contains(&mouse))
                .unwrap_or_else(|| self.root_access_failure(index));
            if contains {
                keep = Some(popup);
                break;
            }
            cursor = self.popup_parent(popup);
        }
        self.dismiss_popup_branch_after(keep)
            .expect("popup root unavailable during outside-press dismissal");
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
        self.roots[index].visible
    }

    fn front_root_at(&self, point: Vec2i) -> Option<RootId> {
        self.roots
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.roots[*index]
                    .root_widget
                    .try_read(|state| state.is_visible() && state.rect().contains(&point))
                    .unwrap_or_else(|| self.root_access_failure(*index))
            })
            .max_by_key(|(_, entry)| entry.stack_key())
            .map(|(_, entry)| entry.id)
    }

    fn front_visible_root(&self) -> Option<RootId> {
        self.roots
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.roots[*index]
                    .root_widget
                    .try_read(RootChrome::is_visible)
                    .unwrap_or_else(|| self.root_access_failure(*index))
            })
            .max_by_key(|(_, entry)| entry.stack_key())
            .map(|(_, entry)| entry.id)
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
            .enumerate()
            .filter(|(index, entry)| {
                let eligible = self.is_descendant_or_self(entry.id, modal);
                eligible
                    && self.roots[*index]
                        .root_widget
                        .try_read(|state| state.is_visible() && state.rect().contains(&point))
                        .unwrap_or_else(|| self.root_access_failure(*index))
            })
            .max_by_key(|(_, entry)| entry.stack_key())
            .map(|(_, entry)| entry.id)
    }

    /// Returns the root that exclusively accepts continuation of an in-progress pointer drag.
    fn drag_input_root(&self) -> Option<RootId> {
        let modal = self.active_modal_root();
        self.roots
            .iter()
            .find(|entry| modal.is_none_or(|modal| self.is_descendant_or_self(entry.id, modal)) && entry.tree.has_capture())
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
            .find(|entry| entry.tree.has_capture())
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
            .enumerate()
            .filter(|(_, entry)| self.is_descendant_or_self(entry.id, modal))
            .filter(|(index, entry)| {
                entry
                    .root_widget
                    .try_read(RootChrome::is_visible)
                    .unwrap_or_else(|| self.root_access_failure(*index))
            })
            .max_by_key(|(_, entry)| entry.stack_key())
            .map(|(_, entry)| entry.id)
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
            .enumerate()
            .filter_map(|(index, entry)| {
                entry
                    .root_widget
                    .try_read(|state| state.is_visible().then(|| (entry.stack_key(), state.name().to_owned())))
                    .unwrap_or_else(|| self.root_access_failure(index))
            })
            .collect::<Vec<_>>();
        names.sort_by_key(|(key, _)| *key);
        names.into_iter().map(|(_, name)| name).collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_zindex(&self, root: RootId) -> Option<i32> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.z_index)
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
        let style = entry.root_widget.try_style_override()?.unwrap_or(self.style);
        entry
            .root_widget
            .try_read(|state| root_chrome_geometry(state.rect(), Dimensioni::default(), state.name(), state.options(), &style, atlas).body)
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
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.tree.has_capture())
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
        let style = entry.root_widget.try_style_override()?.unwrap_or(self.style);
        entry.root_widget.try_read(|state| {
            let geometry = root_chrome_geometry(state.rect(), Dimensioni::default(), state.name(), state.options(), &style, atlas);
            (geometry.title, geometry.close, geometry.resize)
        })
    }
}
