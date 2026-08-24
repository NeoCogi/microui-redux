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
    /// The requested fixed layer is outside the supported inclusive range `0..=15`.
    InvalidLayer(u8),
    /// The root's layer is controlled by popup inheritance or modal policy.
    ///
    /// Only ordinary windows own a directly configurable fixed layer. Show a popup from its
    /// initiating root to establish inheritance; dialogs always use the dedicated modal layer.
    ManagedLayer,
    /// A popup was requested through generic visibility without identifying its initiating root.
    PopupInitiatorRequired,
    /// The supplied popup initiator cannot establish a valid live inheritance relationship.
    ///
    /// This includes using the target popup as its own initiator, using an unbound popup as the
    /// initiator, or attempting to open a non-modal popup while another modal root is active.
    InvalidPopupInitiator,
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
    pub(super) id: RootId,
    pub(super) kind: WindowKind,
    /// Publicly observable source of this root's stacking band.
    pub(super) layer_binding: LayerBinding,
    /// Cached effective band, synchronized whenever a fixed source layer or popup initiator changes.
    effective_band: StackBand,
    pub(super) z_index: i32,
    pub(super) root_widget: TypedWidgetHandle<RootChrome>,
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
    /// Wraps one application node in private root chrome and registers its independent runtime.
    fn register_root(&mut self, kind: WindowKind, name: &str, rect: Recti, content: Node, options: WindowOption, visible: bool) -> RootHandle {
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
        // A window starts in the compatibility-preserving top application layer, a dialog enters
        // the structurally higher modal band, and a hidden popup remains unbound until the show
        // operation identifies the root that initiated it.
        let (layer_binding, effective_band) = match kind {
            WindowKind::Window => (LayerBinding::Fixed(DEFAULT_LAYER), StackBand::Fixed(DEFAULT_LAYER)),
            WindowKind::Modal => (LayerBinding::Modal, StackBand::Modal),
            WindowKind::Popup => (LayerBinding::Unbound, StackBand::Fixed(DEFAULT_LAYER)),
        };
        // Context is the sole tree owner; the entry's typed widget handle cannot retain the root.
        self.roots.push(WindowEntry {
            id,
            kind,
            layer_binding,
            effective_band,
            z_index,
            root_widget: root_widget.clone(),
            tree: WidgetTree::new(root),
        });
        // New topology requires a layout commit before rendering or pointer routing.
        self.invalidate_ui_commit();
        root_handle(id, root_widget, changed, submitted)
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
        self.register_root(WindowKind::Window, name, rect, content, WindowOption::FRAME, true)
    }

    /// Creates a hidden retained dialog around one uniquely owned application node.
    ///
    /// Show it with [`crate::Context::set_root_visible`]. A visible dialog enters the dedicated
    /// modal layer and becomes the active modal input group. Only that dialog and a popup it
    /// initiates remain input-eligible until the dialog is hidden or destroyed. Hiding preserves
    /// all descendant state.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        self.register_root(WindowKind::Modal, name, rect, content, WindowOption::FRAME, false)
    }

    /// Creates a hidden auto-sized popup around one uniquely owned application node.
    ///
    /// Show it through [`crate::Context::show_popup`] or [`crate::Context::show_popup_at`], which
    /// require the initiating root and bind the popup to that root's effective layer. Within that
    /// layer, the popup's transient tier is above ordinary roots; it never crosses a higher fixed
    /// layer. A popup initiated by the active dialog occupies the transient tier above the modal
    /// itself. An outside press or another popup request hides it and records a submission.
    pub fn create_popup(&mut self, name: &str, content: Node) -> PopupHandle {
        // Wrap the generic registry handle at the only construction point that installs popup
        // policy. Callers can therefore prove popup identity by type instead of a runtime check.
        PopupHandle::new(self.register_root(WindowKind::Popup, name, Recti::default(), content, Self::default_popup_options(), false))
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

    /// Assigns one ordinary window to a fixed application stacking layer.
    ///
    /// Layers are ordered from zero at the bottom through fifteen at the top. Popup and modal roots
    /// reject this operation because their bindings are governed by their initiator and the modal
    /// stack respectively. A visible popup initiated by this window follows the new layer in the
    /// same transaction.
    pub fn set_root_layer(&mut self, root: RootId, layer: u8) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        if layer > MAX_LAYER {
            return Err(RootMutationError::InvalidLayer(layer));
        }
        if self.roots[index].kind != WindowKind::Window {
            return Err(RootMutationError::ManagedLayer);
        }

        // The fixed source and all popups currently inheriting from it move atomically from the
        // perspective of the next layout, input, or paint traversal. Popup initiators are
        // normalized to a non-popup root when shown, so a single scan covers submenu chains too.
        self.roots[index].layer_binding = LayerBinding::Fixed(layer);
        self.roots[index].effective_band = StackBand::Fixed(layer);
        for entry in &mut self.roots {
            if entry.layer_binding == LayerBinding::Inherited(root) {
                entry.effective_band = StackBand::Fixed(layer);
            }
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Returns the registered root's current layer-binding policy.
    pub fn root_layer_binding(&self, root: RootId) -> Result<LayerBinding, RootMutationError> {
        let index = self.root_index(root)?;
        Ok(self.roots[index].layer_binding)
    }

    /// Shows or hides a retained root, preserving its tree and concrete widget state.
    ///
    /// Showing a dialog pushes it onto the modal stack. Hiding the active dialog restores the
    /// previous visible dialog, if any; otherwise ordinary cross-root routing resumes.
    /// Showing a popup is rejected because this generic operation cannot identify the source layer;
    /// use [`Self::show_popup`] or [`Self::show_popup_at`]. Hiding a popup remains supported.
    ///
    /// This is distinct from [`crate::Context::destroy_root`], which drops the complete retained owner.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        let target = self.root_index(root)?;
        let kind = self.roots[target].kind;
        if visible && kind == WindowKind::Popup {
            // Visibility alone cannot establish the layer-inheritance invariant. Require callers to
            // use show_popup or show_popup_at, both of which name the root that initiated the
            // transient before it becomes observable.
            return Err(RootMutationError::PopupInitiatorRequired);
        } else {
            // Entering a new modal domain closes the one globally visible popup before the dialog
            // becomes observable. This prevents a transient belonging to the previous ordinary or
            // modal domain from occupying the popup tier above the newly active dialog.
            if visible && kind == WindowKind::Modal {
                self.dismiss_visible_popup()?;
            }
            // A source root and its visible transient close as one checked transaction. Preflight
            // the popup borrow before changing source visibility so a conflicting application
            // access cannot leave half of that relationship observable.
            if !visible {
                self.dismiss_popup_initiated_by(root)?;
            }
            self.update_root_widget(root, |state| state.set_visible_silent(visible))?;
        }

        if visible {
            if kind == WindowKind::Modal {
                self.push_modal(root);
            } else {
                self.raise_root_index(target);
                self.raise_active_modal();
            }
        } else {
            self.roots[target].clear_transient_targets();
            if self.active_root == Some(root) {
                self.active_root = None;
            }
            if kind == WindowKind::Modal {
                self.remove_modal(root);
            }
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Shows one typed popup at the current pointer position and inherits its initiator's layer.
    ///
    /// This is the pointer-relative counterpart to [`Self::show_popup_at`]. Naming the initiator is
    /// mandatory because popup visibility and layer inheritance are committed atomically.
    pub fn show_popup(&mut self, popup: &PopupHandle, initiator: RootId) -> Result<(), RootMutationError> {
        let mouse = self.input.snapshot().mouse_pos;
        self.show_popup_at(popup, initiator, rect(mouse.x, mouse.y, 1, 1))
    }

    /// Shows one typed popup at an exact screen-space anchor in a single root transaction.
    ///
    /// The supplied rectangle is installed before the next layout observes the popup. Showing it
    /// dismisses any other visible popup and publishes that root's ordinary dismissal event.
    /// Accepting [`PopupHandle`] makes windows and dialogs ineligible at compile time; a handle
    /// whose root was destroyed is still reported as [`RootMutationError::UnknownRoot`].
    pub fn show_popup_at(&mut self, popup: &PopupHandle, initiator: RootId, anchor: Recti) -> Result<(), RootMutationError> {
        // Resolve the weak capability on every mutation because destroying a popup does not destroy
        // application-held handles. Root IDs are never reused, so a successful lookup identifies
        // the same popup originally wrapped by PopupHandle::new.
        let target = self.root_index(popup.id())?;
        let source = self.popup_layer_source(initiator, popup.id())?;
        self.show_popup_index_at(target, source, anchor)
    }

    /// Applies anchored popup policy to an already-resolved popup registry index.
    fn show_popup_index_at(&mut self, target: usize, source: RootId, anchor: Recti) -> Result<(), RootMutationError> {
        // The public typed handle establishes target popup identity, while popup_layer_source has
        // normalized the initiator to a visible non-popup root with an authoritative stack band.
        debug_assert_eq!(self.roots[target].kind, WindowKind::Popup);
        let source_index = self.root_index(source)?;
        let inherited_band = self.roots[source_index].effective_band;

        // Find the displaced popup before borrowing either concrete RootChrome mutably. Retained
        // handles use checked RefCell access, so a conflicting application borrow returns a typed
        // error without partially changing visibility, geometry, or semantic dismissal state.
        let mut other = None;
        for (index, entry) in self.roots.iter().enumerate() {
            if index == target || entry.kind != WindowKind::Popup {
                continue;
            }
            if entry.root_widget.try_read(RootChrome::is_visible).ok_or(RootMutationError::Borrowed)? {
                other = Some(index);
                break;
            }
        }

        if let Some(other) = other {
            let old = self.roots[other].root_widget.clone();
            let new = self.roots[target].root_widget.clone();
            let changed = old.try_update(|old_state| {
                new.try_update(|new_state| {
                    // Replacement is indistinguishable from an outside press to the displaced
                    // component: close it semantically before making the new popup observable.
                    old_state.dismiss_popup();
                    new_state.set_rect_silent(anchor);
                    new_state.set_visible_silent(true);
                })
                .is_some()
            });
            match changed {
                Some(true) => self.roots[other].clear_transient_targets(),
                Some(false) | None => return Err(RootMutationError::Borrowed),
            }
        } else {
            let root = self.roots[target].id;
            self.update_root_widget(root, |state| {
                state.set_rect_silent(anchor);
                state.set_visible_silent(true);
            })?;
        }

        // Commit inheritance only after all checked widget borrows succeeded, so a failed popup
        // replacement cannot leave a hidden root attached to a new source. The transient tier in
        // StackKey places this popup above roots in the inherited band but below the next layer.
        self.roots[target].layer_binding = LayerBinding::Inherited(source);
        self.roots[target].effective_band = inherited_band;
        self.raise_root_index(target);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Resolves a popup initiator to the non-popup root that owns its effective layer.
    fn popup_layer_source(&self, initiator: RootId, target: RootId) -> Result<RootId, RootMutationError> {
        if initiator == target {
            return Err(RootMutationError::InvalidPopupInitiator);
        }
        let initiator_index = self.root_index(initiator)?;
        let initiator_visible = self.roots[initiator_index]
            .root_widget
            .try_read(RootChrome::is_visible)
            .ok_or(RootMutationError::Borrowed)?;
        if !initiator_visible {
            return Err(RootMutationError::InvalidPopupInitiator);
        }

        // A popup can initiate a submenu. Because every shown popup stores an already-normalized
        // non-popup source, one lookup collapses an arbitrarily deep transient chain without
        // retaining a parent/child window hierarchy or permitting inheritance cycles.
        let source = match self.roots[initiator_index].kind {
            WindowKind::Window | WindowKind::Modal => initiator,
            WindowKind::Popup => match self.roots[initiator_index].layer_binding {
                LayerBinding::Inherited(source) => source,
                LayerBinding::Fixed(_) | LayerBinding::Unbound | LayerBinding::Modal => {
                    return Err(RootMutationError::InvalidPopupInitiator);
                }
            },
        };
        let source_index = self.root_index(source)?;
        let source_visible = self.roots[source_index]
            .root_widget
            .try_read(RootChrome::is_visible)
            .ok_or(RootMutationError::Borrowed)?;
        if !source_visible {
            return Err(RootMutationError::InvalidPopupInitiator);
        }

        // A modal transaction may only create transients belonging to the active modal. This keeps
        // a blocked ordinary root from placing an interactive popup into or underneath the modal
        // input domain through a programmatic call.
        if let Some(modal) = self.modal_stack.last().copied()
            && source != modal
        {
            return Err(RootMutationError::InvalidPopupInitiator);
        }
        Ok(source)
    }

    /// Dismisses the one globally visible popup if it inherits from `source`.
    fn dismiss_popup_initiated_by(&mut self, source: RootId) -> Result<(), RootMutationError> {
        // Several retained menu popups may remember the same source after earlier openings. Search
        // for the visible member rather than stopping at the first hidden inherited binding.
        let popup = self.roots.iter().enumerate().find_map(|(index, entry)| {
            if entry.kind != WindowKind::Popup || entry.layer_binding != LayerBinding::Inherited(source) {
                return None;
            }
            entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .map(|visible| visible.then_some(index))
                .ok_or(RootMutationError::Borrowed)
                .transpose()
        });
        let Some(index) = popup.transpose()? else { return Ok(()) };
        self.roots[index]
            .root_widget
            .try_update(RootChrome::dismiss_popup)
            .ok_or(RootMutationError::Borrowed)?;
        self.roots[index].clear_transient_targets();
        Ok(())
    }

    /// Dismisses the globally visible popup, if any, through checked retained access.
    fn dismiss_visible_popup(&mut self) -> Result<(), RootMutationError> {
        let popup = self.roots.iter().enumerate().find_map(|(index, entry)| {
            if entry.kind != WindowKind::Popup {
                return None;
            }
            entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .map(|visible| visible.then_some(index))
                .ok_or(RootMutationError::Borrowed)
                .transpose()
        });
        let Some(index) = popup.transpose()? else { return Ok(()) };
        self.roots[index]
            .root_widget
            .try_update(RootChrome::dismiss_popup)
            .ok_or(RootMutationError::Borrowed)?;
        self.roots[index].clear_transient_targets();
        Ok(())
    }

    /// Raises a registered root inside its effective layer and reports whether it exists.
    ///
    /// The operation cannot cross a numeric application-layer boundary, and the active modal
    /// dialog remains above every application root.
    pub fn bring_root_to_front(&mut self, root: RootId) -> bool {
        let Some(index) = self.roots.iter().position(|entry| entry.id == root) else {
            return false;
        };
        self.raise_root_index(index);
        if self.modal_stack.last().copied() != Some(root) {
            self.raise_active_modal();
        }
        self.invalidate_ui_commit();
        true
    }

    /// Permanently unregisters a root and releases its complete retained tree.
    ///
    /// Destroying the active dialog restores the previous visible dialog, if any;
    /// otherwise ordinary cross-root routing resumes.
    ///
    /// There is intentionally no root-content replacement operation. Destroy and recreate a root
    /// to install a different root owner, or mutate descendants through their typed widget handles.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        if !self.roots.iter().any(|entry| entry.id == root) {
            return false;
        }
        // A popup cannot remain meaningfully visible after its initiating root disappears. Root
        // destruction is already an unconditional lifetime operation, so a conflicting transient
        // borrow is an invariant violation rather than a recoverable partial mutation.
        self.dismiss_popup_initiated_by(root)
            .expect("popup initiated by a destroyed root is unexpectedly borrowed");
        let index = self.root_index(root).expect("destroy target must remain registered after popup dismissal");
        let kind = self.roots[index].kind;
        self.roots.remove(index);
        // Hidden popups may retain their last source binding so layer inspection remains useful
        // while a source is merely hidden. Once that source is destroyed, clear every such stale
        // relationship; the next show operation must establish a new live initiator.
        for entry in &mut self.roots {
            if entry.layer_binding == LayerBinding::Inherited(root) {
                entry.layer_binding = LayerBinding::Unbound;
                entry.effective_band = StackBand::Fixed(DEFAULT_LAYER);
            }
        }
        if self.active_root == Some(root) {
            self.active_root = None;
        }
        if kind == WindowKind::Modal {
            self.remove_modal(root);
        }
        self.invalidate_ui_commit();
        true
    }

    fn root_index(&self, root: RootId) -> Result<usize, RootMutationError> {
        self.roots.iter().position(|entry| entry.id == root).ok_or(RootMutationError::UnknownRoot)
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

    /// Orders the registry from back to front using the one authoritative stacking key.
    fn sort_roots_for_stacking(&mut self) {
        // Stable sorting preserves registry order when the saturating z-index counter eventually
        // produces ties. Every other ordering query uses WindowEntry::stack_key directly, so paint,
        // layout traversal, and hit testing cannot disagree about layer or popup precedence.
        self.roots.sort_by_key(WindowEntry::stack_key);
    }

    /// Makes one visible dialog the active modal input group and clears every other tree's targets.
    ///
    /// A popup initiated by this dialog may temporarily join the group; ordinary application-layer
    /// roots remain blocked until every dialog has left the modal stack.
    fn push_modal(&mut self, root: RootId) {
        let index = self.root_index(root).expect("modal root must remain registered");
        assert!(self.roots[index].kind == WindowKind::Modal, "modal root must have modal kind");

        let changed = self.modal_stack.last().copied() != Some(root);
        self.modal_stack.retain(|candidate| *candidate != root);
        self.modal_stack.push(root);
        if changed {
            for entry in &mut self.roots {
                if entry.id != root {
                    entry.clear_transient_targets();
                }
            }
        }
        let index = self.root_index(root).expect("modal root must remain registered");
        self.raise_root_index(index);
    }

    /// Removes a dialog from modal policy and restores the most recently activated survivor.
    fn remove_modal(&mut self, root: RootId) {
        let was_active = self.modal_stack.last().copied() == Some(root);
        self.modal_stack.retain(|candidate| *candidate != root);
        if was_active && !self.modal_stack.is_empty() {
            self.raise_active_modal();
        }
    }

    /// Restores the active modal root above a root that was just shown or fronted.
    fn raise_active_modal(&mut self) {
        let Some(root) = self.modal_stack.last().copied() else { return };
        let index = self.root_index(root).expect("modal root must remain registered");
        self.raise_root_index(index);
    }

    /// Reconciles modal ownership after a state-local title close.
    fn reconcile_closed_modal(&mut self) {
        let Some(root) = self.modal_stack.last().copied() else { return };
        let index = self.root_index(root).expect("modal root must remain registered");
        let visible = self.roots[index]
            .root_widget
            .try_read(RootChrome::is_visible)
            .unwrap_or_else(|| self.root_access_failure(index));
        if !visible {
            self.remove_modal(root);
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
            self.reconcile_closed_modal();
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
        let modal_root = self.modal_stack.last().copied();
        for entry in &mut self.roots {
            let visible = entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable before input update");
            if visible && modal_root.is_none_or(|modal| entry.id == modal || entry.layer_binding == LayerBinding::Inherited(modal)) {
                entry.tree.begin_input_event(pointer_root == Some(entry.id), event);
            }
        }

        if event.is_pointer() {
            // Widget capture owns drag continuation and release cleanup only. Wheel, hover, and new
            // presses still perform ordinary hit routing, constrained by pointer_root above.
            let capture_index = matches!(event, crate::UiInputEvent::MouseDrag { .. } | crate::UiInputEvent::MouseUp { .. })
                .then(|| {
                    self.roots.iter().position(|entry| {
                        self.modal_stack
                            .last()
                            .is_none_or(|modal| *modal == entry.id || entry.layer_binding == LayerBinding::Inherited(*modal))
                            && entry.tree.has_capture()
                    })
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
        let modal_root = self.modal_stack.last().copied();
        for entry in &mut self.roots {
            let visible = entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable during input update");
            if !visible || modal_root.is_some_and(|modal| entry.id != modal && entry.layer_binding != LayerBinding::Inherited(modal)) {
                entry.clear_transient_targets();
                continue;
            }
            entry.tree.update(&self.style, atlas.clone(), input);

            let visible = entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable after root update");
            if !visible {
                entry.clear_transient_targets();
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
        let popup = self.roots.iter().enumerate().find_map(|(index, entry)| {
            if entry.kind != WindowKind::Popup {
                return None;
            }
            entry
                .root_widget
                .try_read(|state| state.is_visible() && !state.rect().contains(&mouse))
                .unwrap_or_else(|| self.root_access_failure(index))
                .then_some(index)
        });
        if let Some(index) = popup {
            self.roots[index]
                .root_widget
                .try_update(RootChrome::dismiss_popup)
                .unwrap_or_else(|| self.root_access_failure(index));
            self.roots[index].clear_transient_targets();
        }
    }

    /// Maps a pointer target to the ordinary root whose keyboard activation it represents.
    fn activation_source(&self, root: RootId) -> Option<RootId> {
        let index = self.root_index(root).ok()?;
        match self.roots[index].kind {
            WindowKind::Window => Some(root),
            // A popup preserves its initiating root's keyboard activation. Modal roots need no
            // ordinary activation because the modal stack is already authoritative while visible.
            WindowKind::Popup => match self.roots[index].layer_binding {
                LayerBinding::Inherited(source) => {
                    let source_index = self.root_index(source).ok()?;
                    (self.roots[source_index].kind == WindowKind::Window).then_some(source)
                }
                LayerBinding::Fixed(_) | LayerBinding::Unbound | LayerBinding::Modal => None,
            },
            WindowKind::Modal => None,
        }
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
        self.roots[index]
            .root_widget
            .try_read(RootChrome::is_visible)
            .unwrap_or_else(|| self.root_access_failure(index))
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
        let Some(modal) = self.modal_stack.last().copied() else {
            return self.front_root_at(point);
        };
        // The active modal and popups initiated by it form one exclusive input group. Roots in all
        // fixed layers, lower dialogs, and their transients remain painted but cannot receive the
        // event even when the pointer lies outside the active modal rectangle.
        self.roots
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                let eligible = entry.id == modal || entry.layer_binding == LayerBinding::Inherited(modal);
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
        let modal = self.modal_stack.last().copied();
        self.roots
            .iter()
            .find(|entry| modal.is_none_or(|modal| entry.id == modal || entry.layer_binding == LayerBinding::Inherited(modal)) && entry.tree.has_capture())
            .map(|entry| entry.id)
            .or_else(|| self.front_input_root())
    }

    /// Returns the sole keyboard-eligible root without deriving activation from visual stacking.
    fn keyboard_input_root(&self) -> Option<RootId> {
        if let Some(modal) = self.modal_stack.last().copied() {
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
        let Some(modal) = self.modal_stack.last().copied() else {
            return self.front_visible_root();
        };
        self.roots
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.id == modal || entry.layer_binding == LayerBinding::Inherited(modal))
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
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.layer_binding)
    }

    #[cfg(test)]
    pub(crate) fn debug_active_root(&self) -> Option<RootId> {
        self.active_root
    }

    #[cfg(test)]
    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        self.modal_stack.last().copied()
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
