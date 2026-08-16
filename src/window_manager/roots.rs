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
use crate::{MouseButton, Node, RootHandle, RootMutationError, RootChrome, TypedWidgetHandle, UiInputEvent, Vec2i, rect};

use super::root_chrome::{create_root_chrome, record_root_overlay, root_handle, RootChromeParameters};
#[cfg(test)]
use super::root_chrome::root_chrome_geometry;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum WindowKind {
    Window,
    Modal,
    Popup,
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
        self.runtime.capture.is_some()
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
        let transform = self.runtime.root_transform();
        if let Some((owner, result)) = self
            .runtime
            .route_root_input_event_to_node_ref(&mut self.root, transform, style, event, root_chrome_hit)
        {
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
    pub(super) z_index: i32,
    pub(super) root_widget: TypedWidgetHandle<RootChrome>,
    pub(super) tree: WidgetTree,
}

impl WindowEntry {
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
        // Context is the sole tree owner; the entry's typed widget handle cannot retain the root.
        self.roots.push(WindowEntry {
            id,
            kind,
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
    /// Show it with [`crate::Context::set_root_visible`]. A visible dialog becomes the active modal root:
    /// it stays frontmost and is the only root eligible for input until hidden or destroyed.
    /// Hiding preserves all descendant state.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        self.register_root(WindowKind::Modal, name, rect, content, WindowOption::FRAME, false)
    }

    /// Creates a hidden auto-sized popup around one uniquely owned application node.
    ///
    /// Showing places it at the current pointer position. An outside press hides it and records a
    /// submission before ordinary routing may continue beneath the popup boundary. While a dialog
    /// is active, a shown popup remains visible but is kept below the dialog and receives no input.
    pub fn create_popup(&mut self, name: &str, content: Node) -> RootHandle {
        self.register_root(WindowKind::Popup, name, Recti::default(), content, Self::default_popup_options(), false)
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

    /// Shows or hides a retained root, preserving its tree and concrete widget state.
    ///
    /// Showing a dialog pushes it onto the modal stack. Hiding the active dialog restores the
    /// previous visible dialog, if any; otherwise ordinary cross-root routing resumes.
    ///
    /// This is distinct from [`crate::Context::destroy_root`], which drops the complete retained owner.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        let target = self.root_index(root)?;
        let mouse = self.input.snapshot().mouse_pos;
        let kind = self.roots[target].kind;
        if visible && kind == WindowKind::Popup {
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
                        old_state.set_visible_silent(false);
                        new_state.set_rect_silent(rect(mouse.x, mouse.y, 1, 1));
                        new_state.set_visible_silent(true);
                    })
                    .is_some()
                });
                match changed {
                    Some(true) => self.roots[other].clear_transient_targets(),
                    Some(false) | None => return Err(RootMutationError::Borrowed),
                }
            } else {
                self.update_root_widget(root, |state| {
                    if !state.is_visible() {
                        state.set_rect_silent(rect(mouse.x, mouse.y, 1, 1));
                    }
                    state.set_visible_silent(true);
                })?;
            }
        } else {
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
            if kind == WindowKind::Modal {
                self.remove_modal(root);
            }
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Raises a registered root and reports whether it exists.
    ///
    /// The active modal dialog remains above every other root raised through this operation.
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
        let Some(index) = self.roots.iter().position(|entry| entry.id == root) else {
            return false;
        };
        let kind = self.roots[index].kind;
        self.roots.remove(index);
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

    /// Makes one visible dialog the sole input root and clears every other tree's targets.
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
        self.update_with(dimensions, atlas, &mut (), |_| false);
    }

    /// Performs the retained update while exposing each safe subscriber-dispatch boundary.
    pub(crate) fn update_with<DispatchState>(
        &mut self,
        dimensions: Dimensioni,
        atlas: &crate::AtlasHandle,
        dispatch_state: &mut DispatchState,
        mut after_event: impl FnMut(&mut DispatchState) -> bool,
    ) {
        self.ui_commit = None;
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);

        // This pre-layout pass removes abandoned sessions even when no input was queued.
        self.process_file_dialogs();
        for entry in &mut self.roots {
            entry.tree.begin_update();
        }
        self.layout(viewport, atlas);
        // Subscriber invocations may already be waiting without a raw input event. If they mutate
        // retained state, commit that state before routing the first queued event.
        if after_event(dispatch_state) {
            self.layout(viewport, atlas);
        }

        loop {
            let event = self.input.pop_event();
            let Some(event) = event else { break };
            let input = self.input.snapshot();
            self.update_for_event(atlas, &event, input);
            // Dialog controls are ordinary retained widgets. Consume their committed actions only
            // after the complete cross-root update and before the matching layout commit.
            self.process_file_dialogs();
            self.reconcile_closed_modal();
            // Application subscribers run only after the complete cross-root update has released
            // retained borrows. Their state/topology changes are therefore safe and become visible
            // to the layout immediately below, before routing the next raw input event.
            after_event(dispatch_state);
            self.layout(viewport, atlas);
        }
        self.ui_commit = Some(dimensions);
    }

    /// Synchronizes auto-size and layout for every visible root.
    fn layout(&mut self, viewport: Recti, atlas: &crate::AtlasHandle) {
        self.roots.sort_by_key(|entry| entry.z_index);

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
        if self.modal_stack.is_empty() && matches!(event, crate::UiInputEvent::MouseDown { .. }) {
            self.dismiss_outside_popup(input.mouse_pos);
        }

        let hover_root = event.is_pointer().then(|| self.input_root_at(input.mouse_pos)).flatten();
        if matches!(event, crate::UiInputEvent::MouseDown { .. })
            && let Some(root) = hover_root
        {
            let _ = self.bring_root_to_front(root);
            // A new press may target any root. Transfer global pointer ownership before routing so
            // no previous root can retain a widget-level capture alongside the new press target.
            for entry in &mut self.roots {
                if entry.id != root && entry.tree.has_capture() {
                    entry.clear_transient_targets();
                }
            }
        }

        // Drag, wheel, keyboard, and text input remain confined to the current input root. Hover
        // and new-press targeting continue to follow pointer geometry across ordinary roots.
        let captured_root = self.captured_input_root();
        let pointer_root = match event {
            crate::UiInputEvent::MouseDrag { .. } | crate::UiInputEvent::Scroll { .. } => captured_root,
            _ => hover_root,
        };
        let keyboard_root = captured_root;
        let modal_root = self.modal_stack.last().copied();
        for entry in &mut self.roots {
            let visible = entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable before input update");
            if visible && modal_root.is_none_or(|modal| modal == entry.id) {
                entry.tree.begin_input_event(pointer_root == Some(entry.id), event);
            }
        }

        if event.is_pointer() {
            // Widget capture owns drag continuation and release cleanup only. Wheel, hover, and new
            // presses still perform ordinary hit routing, constrained by pointer_root above.
            let capture_index = matches!(event, crate::UiInputEvent::MouseDrag { .. } | crate::UiInputEvent::MouseUp { .. })
                .then(|| {
                    self.roots
                        .iter()
                        .position(|entry| self.modal_stack.last().is_none_or(|modal| *modal == entry.id) && entry.tree.has_capture())
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

        self.roots.sort_by_key(|entry| entry.z_index);
        let modal_root = self.modal_stack.last().copied();
        for entry in &mut self.roots {
            let visible = entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable during input update");
            if !visible || modal_root.is_some_and(|modal| modal != entry.id) {
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
    }

    /// Paints and records the already committed trees without updating or laying them out.
    pub(crate) fn paint(&mut self, dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        self.display_list.clear();
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        self.roots.sort_by_key(|entry| entry.z_index);
        for entry in &mut self.roots {
            let visible = entry
                .root_widget
                .try_read(RootChrome::is_visible)
                .expect("registered root state unavailable during paint");
            if !visible {
                continue;
            }
            entry.tree.paint(&mut self.display_list, &self.style, atlas.clone());
            entry
                .root_widget
                .try_read(|state| record_root_overlay(&mut self.display_list, viewport, state, &self.style, atlas))
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
            .max_by_key(|(_, entry)| entry.z_index)
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
            .max_by_key(|(_, entry)| entry.z_index)
            .map(|(_, entry)| entry.id)
    }

    /// Returns the sole pointer-eligible root at `point` under the active modal policy.
    fn input_root_at(&self, point: Vec2i) -> Option<RootId> {
        let Some(modal) = self.modal_stack.last().copied() else {
            return self.front_root_at(point);
        };
        let index = self.root_index(modal).expect("modal root must remain registered");
        self.roots[index]
            .root_widget
            .try_read(|state| (state.is_visible() && state.rect().contains(&point)).then_some(modal))
            .unwrap_or_else(|| self.root_access_failure(index))
    }

    /// Returns the root that exclusively accepts drag, wheel, keyboard, and text input.
    ///
    /// An active modal is always authoritative. Otherwise a widget-level pointer capture keeps its
    /// owning root authoritative even if z-order changes programmatically; without either, the
    /// ordinary front visible root remains the current input root.
    fn captured_input_root(&self) -> Option<RootId> {
        self.modal_stack
            .last()
            .copied()
            .or_else(|| self.roots.iter().find(|entry| entry.tree.has_capture()).map(|entry| entry.id))
            .or_else(|| self.front_input_root())
    }

    /// Returns the sole keyboard-eligible modal root or the ordinary front visible root.
    fn front_input_root(&self) -> Option<RootId> {
        self.modal_stack.last().copied().or_else(|| self.front_visible_root())
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
                    .try_read(|state| state.is_visible().then(|| (entry.z_index, state.name().to_owned())))
                    .unwrap_or_else(|| self.root_access_failure(index))
            })
            .collect::<Vec<_>>();
        names.sort_by_key(|(z, _)| *z);
        names.into_iter().map(|(_, name)| name).collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_zindex(&self, root: RootId) -> Option<i32> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.z_index)
    }

    #[cfg(test)]
    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        self.modal_stack.last().copied()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_body(&self, root: RootId, atlas: &crate::AtlasHandle) -> Option<Recti> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        entry
            .root_widget
            .try_read(|state| root_chrome_geometry(state.rect(), Dimensioni::default(), state.name(), state.options(), &self.style, atlas).body)
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
        entry.root_widget.try_read(|state| {
            let geometry = root_chrome_geometry(state.rect(), Dimensioni::default(), state.name(), state.options(), &self.style, atlas);
            (geometry.title, geometry.close, geometry.resize)
        })
    }
}
