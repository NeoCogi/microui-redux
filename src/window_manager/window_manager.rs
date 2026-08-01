//! Window-manager root registry, cross-root policy, and persistent tree traversal.

use super::*;
use crate::{Node, RootHandle, RootMutationError, RootState, Vec2i, WidgetStateHandle};

use super::root_chrome::{record_root_overlay, root_handle, RootChromeContainer, RootChromeParameters};
#[cfg(test)]
use super::root_chrome::root_chrome_geometry;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum WindowKind {
    Window,
    Dialog,
    Popup,
}

/// One persistent retained tree and its traversal-local runtime state.
pub(super) struct WidgetTree {
    pub(super) root: Node,
    pub(super) runtime: UiRuntime,
}

impl WidgetTree {
    fn clear_transient_targets(&mut self) {
        self.runtime.clear_transient_targets(std::slice::from_mut(&mut self.root));
    }
}

/// Lifecycle and cross-root metadata for one retained tree.
pub(super) struct WindowEntry {
    pub(super) id: RootId,
    pub(super) kind: WindowKind,
    pub(super) z_index: i32,
    pub(super) root_state: WidgetStateHandle<RootState>,
    pub(super) tree: WidgetTree,
}

impl<B: RendererBackend> Context<B> {
    fn register_root(&mut self, kind: WindowKind, name: &str, rect: Recti, content: Node, options: WindowOption, visible: bool) -> RootHandle {
        let id = self.next_root_id();
        let (root_state, root) = RootChromeContainer::create(RootChromeParameters {
            name: name.to_owned(),
            options,
            rect,
            visible,
            content,
        });
        let z_index = if visible {
            self.last_zindex = self.last_zindex.saturating_add(1);
            self.last_zindex
        } else {
            -1
        };
        self.roots.push(WindowEntry {
            id,
            kind,
            z_index,
            root_state: root_state.clone(),
            tree: WidgetTree { root, runtime: UiRuntime::new() },
        });
        self.invalidate_ui_commit();
        root_handle(id, root_state)
    }

    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
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
    /// Show it with [`Context::set_root_visible`]; hiding preserves all descendant state.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        self.register_root(WindowKind::Dialog, name, rect, content, WindowOption::FRAME, false)
    }

    /// Creates a hidden auto-sized popup around one uniquely owned application node.
    ///
    /// Showing places it at the current pointer position. An outside press hides it and records a
    /// submission before ordinary routing may continue beneath the popup boundary.
    pub fn create_popup(&mut self, name: &str, content: Node) -> RootHandle {
        self.register_root(WindowKind::Popup, name, Recti::default(), content, Self::default_popup_options(), false)
    }

    /// Replaces a root rectangle silently while retaining any compatible captured chrome mode.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) -> Result<(), RootMutationError> {
        self.update_root_state(root, |state| state.set_rect_silent(rect))
    }

    /// Replaces a root size silently without changing its origin.
    pub fn set_root_size(&mut self, root: RootId, size: Dimensioni) -> Result<(), RootMutationError> {
        self.update_root_state(root, |state| state.set_size_silent(size))
    }

    /// Replaces root chrome options silently.
    pub fn set_root_options(&mut self, root: RootId, options: WindowOption) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        let was_active = self.roots[index].root_state.try_read(RootState::is_active).ok_or(RootMutationError::Borrowed)?;
        self.update_root_state(root, |state| state.set_options_silent(options))?;
        let active = self.roots[index]
            .root_state
            .try_read(RootState::is_active)
            .unwrap_or_else(|| self.root_access_failure(index));
        if was_active && !active {
            self.roots[index].tree.clear_transient_targets();
        }
        Ok(())
    }

    /// Shows or hides a retained root, preserving its tree and typed state.
    ///
    /// This is distinct from [`Context::destroy_root`], which drops the complete retained owner.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        let target = self.root_index(root)?;
        let mouse = self.input.borrow().mouse_pos;
        let kind = self.roots[target].kind;
        if visible && kind == WindowKind::Popup {
            let mut other = None;
            for (index, entry) in self.roots.iter().enumerate() {
                if index == target || entry.kind != WindowKind::Popup {
                    continue;
                }
                if entry.root_state.try_read(RootState::is_visible).ok_or(RootMutationError::Borrowed)? {
                    other = Some(index);
                    break;
                }
            }
            if let Some(other) = other {
                let old = self.roots[other].root_state.clone();
                let new = self.roots[target].root_state.clone();
                let changed = old.try_update(|old_state| {
                    new.try_update(|new_state| {
                        old_state.set_visible_silent(false);
                        new_state.set_rect_silent(rect(mouse.x, mouse.y, 1, 1));
                        new_state.set_visible_silent(true);
                    })
                    .is_some()
                });
                match changed {
                    Some(true) => self.roots[other].tree.clear_transient_targets(),
                    Some(false) | None => return Err(RootMutationError::Borrowed),
                }
            } else {
                self.update_root_state(root, |state| {
                    if !state.is_visible() {
                        state.set_rect_silent(rect(mouse.x, mouse.y, 1, 1));
                    }
                    state.set_visible_silent(true);
                })?;
            }
        } else {
            self.update_root_state(root, |state| state.set_visible_silent(visible))?;
        }

        if visible {
            self.last_zindex = self.last_zindex.saturating_add(1);
            self.roots[target].z_index = self.last_zindex;
        } else {
            self.roots[target].tree.clear_transient_targets();
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Raises a registered root and reports whether it exists.
    pub fn bring_root_to_front(&mut self, root: RootId) -> bool {
        let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) else {
            return false;
        };
        self.last_zindex = self.last_zindex.saturating_add(1);
        entry.z_index = self.last_zindex;
        self.invalidate_ui_commit();
        true
    }

    /// Permanently unregisters a root and releases its complete retained tree.
    ///
    /// There is intentionally no root-content replacement operation. Destroy and recreate a root
    /// to install a different root owner, or mutate descendants through their container state.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        let Some(index) = self.roots.iter().position(|entry| entry.id == root) else {
            return false;
        };
        self.roots.remove(index);
        self.invalidate_ui_commit();
        true
    }

    fn root_index(&self, root: RootId) -> Result<usize, RootMutationError> {
        self.roots.iter().position(|entry| entry.id == root).ok_or(RootMutationError::UnknownRoot)
    }

    fn update_root_state(&mut self, root: RootId, update: impl FnOnce(&mut RootState)) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        if self.roots[index].root_state.try_update(update).is_some() {
            self.invalidate_ui_commit();
            Ok(())
        } else if self.roots[index].root_state.is_alive() {
            Err(RootMutationError::Borrowed)
        } else {
            panic!("registered root lost its persistent RootState owner")
        }
    }

    fn root_access_failure(&self, index: usize) -> ! {
        if self.roots[index].root_state.is_alive() {
            panic!("registered root state is unexpectedly borrowed during traversal")
        }
        panic!("registered root lost its persistent RootState owner")
    }

    /// Performs one synchronization layout, then one full update/layout pair per queued event.
    pub(super) fn update_window_manager(&mut self, dimensions: Dimensioni) {
        let atlas = self.renderer.atlas();
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);

        // This pre-layout pass removes abandoned sessions even when no input was queued.
        self.process_file_dialogs();
        for entry in &mut self.roots {
            entry.tree.runtime.begin_update();
        }
        self.layout_window_manager(viewport, &atlas);

        loop {
            let event = { self.input.borrow_mut().pop_event() };
            let Some(event) = event else { break };
            let input = self.input.borrow().snapshot();
            self.update_window_manager_for_event(&atlas, &event, input);
            // Dialog controls are ordinary retained widgets. Consume their committed actions only
            // after the complete cross-root update and before the matching layout commit.
            self.process_file_dialogs();
            self.layout_window_manager(viewport, &atlas);
        }
    }

    /// Synchronizes auto-size and layout for every visible root.
    fn layout_window_manager(&mut self, viewport: Recti, atlas: &crate::AtlasHandle) {
        self.roots.sort_by_key(|entry| entry.z_index);

        for index in 0..self.roots.len() {
            let (visible, options, rect) = self.roots[index]
                .root_state
                .try_read(|state| (state.is_visible(), state.options(), state.rect()))
                .unwrap_or_else(|| self.root_access_failure(index));
            if visible && options.intersects(WindowOption::AUTO_SIZE) {
                let available = Dimensioni::new(rect.width.max(1), 10_000);
                let size = self.roots[index]
                    .tree
                    .runtime
                    .measure_tree_root(&self.roots[index].tree.root, self.style.as_ref(), atlas, available);
                self.roots[index]
                    .root_state
                    .try_update(|state| state.set_size_silent(size))
                    .unwrap_or_else(|| self.root_access_failure(index));
            }
        }

        for entry in &mut self.roots {
            let (visible, rect) = entry
                .root_state
                .try_read(|state| (state.is_visible(), state.rect()))
                .expect("registered root state unavailable during frame");
            if !visible {
                entry.tree.clear_transient_targets();
                continue;
            }

            entry
                .tree
                .runtime
                .layout_tree_root(&mut entry.tree.root, self.style.as_ref(), atlas.clone(), rect, viewport);
        }
    }

    /// Routes and applies one normalized event, visiting every eligible tree exactly once.
    fn update_window_manager_for_event(&mut self, atlas: &crate::AtlasHandle, event: &crate::UiInputEvent, input: crate::input::InputSnapshot) {
        if matches!(event, crate::UiInputEvent::MouseDown { .. }) {
            self.dismiss_outside_popup(input.mouse_pos);
        }

        let hover_root = event.is_pointer().then(|| self.front_root_at(input.mouse_pos)).flatten();
        if matches!(event, crate::UiInputEvent::MouseDown { .. })
            && let Some(root) = hover_root
        {
            let _ = self.bring_root_to_front(root);
        }

        let keyboard_root = self.front_visible_root();
        for entry in &mut self.roots {
            let visible = entry
                .root_state
                .try_read(RootState::is_visible)
                .expect("registered root state unavailable before input update");
            if visible {
                entry.tree.runtime.begin_input_event(hover_root == Some(entry.id), event);
            }
        }

        if event.is_pointer() {
            let capture_index = self.roots.iter().position(|entry| entry.tree.runtime.capture.is_some());
            let mut capture_handled = false;
            if let Some(index) = capture_index {
                let entry = &mut self.roots[index];
                let roots = std::slice::from_mut(&mut entry.tree.root);
                capture_handled = entry
                    .tree
                    .runtime
                    .route_captured_pointer_input_event(roots, self.style.as_ref(), input.mouse_buttons, event)
                    .is_some();
            }

            if !capture_handled
                && let Some(root) = hover_root
                && let Some(index) = self.roots.iter().position(|entry| entry.id == root)
            {
                let entry = &mut self.roots[index];
                if entry.tree.runtime.accepts_pointer_input() {
                    let transform = entry.tree.runtime.root_transform();
                    if let Some((owner, result)) = entry
                        .tree
                        .runtime
                        .route_input_event_to_node_ref(&mut entry.tree.root, transform, self.style.as_ref(), event)
                    {
                        entry.tree.runtime.update_pointer_capture(owner, result, event, input.mouse_buttons);
                    }
                }
            }
        } else if event.is_focus_input()
            && let Some(root) = keyboard_root
            && let Some(index) = self.roots.iter().position(|entry| entry.id == root)
        {
            let entry = &mut self.roots[index];
            let roots = std::slice::from_mut(&mut entry.tree.root);
            entry.tree.runtime.route_focus_input_event(roots, self.style.as_ref(), event);
        }

        self.roots.sort_by_key(|entry| entry.z_index);
        for entry in &mut self.roots {
            let visible = entry
                .root_state
                .try_read(RootState::is_visible)
                .expect("registered root state unavailable during input update");
            if !visible {
                entry.tree.clear_transient_targets();
                continue;
            }
            entry
                .tree
                .runtime
                .update_tree_root(&mut entry.tree.root, self.style.as_ref(), atlas.clone(), input);

            let visible = entry
                .root_state
                .try_read(RootState::is_visible)
                .expect("registered root state unavailable after root update");
            if !visible {
                entry.tree.clear_transient_targets();
            }
        }
    }

    /// Paints and records the already committed trees without updating or laying them out.
    pub(super) fn paint_window_manager(&mut self, dimensions: Dimensioni) {
        self.display_list.clear();
        let atlas = self.renderer.atlas();
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        self.roots.sort_by_key(|entry| entry.z_index);
        for entry in &mut self.roots {
            let visible = entry
                .root_state
                .try_read(RootState::is_visible)
                .expect("registered root state unavailable during paint");
            if !visible {
                continue;
            }
            entry
                .tree
                .runtime
                .paint_tree_root(&mut entry.tree.root, &mut self.display_list, self.style.as_ref(), atlas.clone());
            entry
                .root_state
                .try_read(|state| record_root_overlay(&mut self.display_list, viewport, state, self.style.as_ref(), &atlas))
                .expect("registered root state unavailable during overlay paint");
        }
    }

    fn dismiss_outside_popup(&mut self, mouse: Vec2i) {
        let popup = self.roots.iter().enumerate().find_map(|(index, entry)| {
            if entry.kind != WindowKind::Popup {
                return None;
            }
            entry
                .root_state
                .try_read(|state| state.is_visible() && !state.rect().contains(&mouse))
                .unwrap_or_else(|| self.root_access_failure(index))
                .then_some(index)
        });
        if let Some(index) = popup {
            self.roots[index]
                .root_state
                .try_update(RootState::dismiss_popup)
                .unwrap_or_else(|| self.root_access_failure(index));
            self.roots[index].tree.clear_transient_targets();
        }
    }

    fn front_root_at(&self, point: Vec2i) -> Option<RootId> {
        self.roots
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.roots[*index]
                    .root_state
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
                    .root_state
                    .try_read(RootState::is_visible)
                    .unwrap_or_else(|| self.root_access_failure(*index))
            })
            .max_by_key(|(_, entry)| entry.z_index)
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
                    .root_state
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
    pub(crate) fn debug_root_body(&self, root: RootId) -> Option<Recti> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        entry.root_state.try_read(|state| {
            root_chrome_geometry(
                state.rect(),
                Dimensioni::default(),
                state.name(),
                state.options(),
                self.style.as_ref(),
                &self.renderer.atlas(),
            )
            .body
        })
    }

    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self, root: RootId) -> Option<Dimensioni> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.tree.runtime.debug_root_content_size())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_runtime_metrics(&self, root: RootId) -> Option<crate::ui_node::RuntimeMetrics> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.tree.runtime.debug_metrics())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_has_pointer_capture(&self, root: RootId) -> Option<bool> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.tree.runtime.capture.is_some())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_structure(&self, root: RootId) -> Option<(usize, usize)> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        Some((entry.tree.root.debug_node_count(), entry.tree.root.debug_erased_adapter_count()))
    }

    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        entry.tree.runtime.debug_node_rect(std::slice::from_ref(&entry.tree.root), node)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_chrome(&self, root: RootId) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        entry.root_state.try_read(|state| {
            let geometry = root_chrome_geometry(
                state.rect(),
                Dimensioni::default(),
                state.name(),
                state.options(),
                self.style.as_ref(),
                &self.renderer.atlas(),
            );
            (geometry.title, geometry.close, geometry.resize)
        })
    }
}
