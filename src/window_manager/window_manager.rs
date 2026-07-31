//! Window-manager root registry, cross-root policy, and persistent tree traversal.

use super::*;
use crate::ui_node::pointer_events_from_input;
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
    pub(super) just_opened: bool,
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
            just_opened: false,
            z_index,
            root_state: root_state.clone(),
            tree: WidgetTree { root, runtime: UiRuntime::new() },
        });
        root_handle(id, root_state)
    }

    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained root id counter overflowed");
        id
    }

    /// Creates an open retained window around one uniquely owned application node.
    pub fn create_window(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        self.register_root(WindowKind::Window, name, rect, content, WindowOption::FRAME, true)
    }

    /// Creates a hidden retained dialog around one uniquely owned application node.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        self.register_root(WindowKind::Dialog, name, rect, content, WindowOption::FRAME, false)
    }

    /// Creates a hidden auto-sized popup around one uniquely owned application node.
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
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        let target = self.root_index(root)?;
        let mouse = self.input.borrow().mouse_pos;
        let kind = self.roots[target].kind;
        let was_visible = self.roots[target]
            .root_state
            .try_read(RootState::is_visible)
            .ok_or(RootMutationError::Borrowed)?;

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
            self.roots[target].just_opened = !was_visible;
        } else {
            self.update_root_state(root, |state| state.set_visible_silent(visible))?;
        }

        if visible {
            self.last_zindex = self.last_zindex.saturating_add(1);
            self.roots[target].z_index = self.last_zindex;
        } else {
            self.roots[target].tree.clear_transient_targets();
        }
        Ok(())
    }

    /// Raises a registered root and reports whether it exists.
    pub fn bring_root_to_front(&mut self, root: RootId) -> bool {
        let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) else {
            return false;
        };
        self.last_zindex = self.last_zindex.saturating_add(1);
        entry.z_index = self.last_zindex;
        true
    }

    /// Permanently unregisters a root and releases its complete retained tree.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        let Some(index) = self.roots.iter().position(|entry| entry.id == root) else {
            return false;
        };
        self.roots.remove(index);
        true
    }

    fn root_index(&self, root: RootId) -> Result<usize, RootMutationError> {
        self.roots.iter().position(|entry| entry.id == root).ok_or(RootMutationError::UnknownRoot)
    }

    fn update_root_state(&mut self, root: RootId, update: impl FnOnce(&mut RootState)) -> Result<(), RootMutationError> {
        let index = self.root_index(root)?;
        if self.roots[index].root_state.try_update(update).is_some() {
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

    // Used by the preserved file-dialog implementation while that module is temporarily uncompiled.
    #[allow(dead_code)]
    pub(crate) fn root_spacing(&self) -> i32 {
        self.style.spacing.max(0)
    }

    pub(super) fn render_window_manager(&mut self, dimensions: Dimensioni) {
        let atlas = self.renderer.atlas();
        let viewport = Recti::new(0, 0, dimensions.width.max(0), dimensions.height.max(0));

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
                    .measure_tree_root(&self.roots[index].tree.root, self.style.as_ref(), &atlas, available);
                self.roots[index]
                    .root_state
                    .try_update(|state| state.set_size_silent(size))
                    .unwrap_or_else(|| self.root_access_failure(index));
            }
        }

        let (mouse_pos, mouse_pressed) = {
            let input = self.input.borrow();
            (input.mouse_pos, input.mouse_pressed)
        };
        if !mouse_pressed.is_empty() {
            self.dismiss_outside_popup(mouse_pos);
        }
        let hover_root = self.front_root_at(mouse_pos);
        if !mouse_pressed.is_empty()
            && let Some(root) = hover_root
        {
            let _ = self.bring_root_to_front(root);
        }

        let keyboard_root = self
            .roots
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.roots[*index]
                    .root_state
                    .try_read(RootState::is_visible)
                    .unwrap_or_else(|| self.root_access_failure(*index))
            })
            .max_by_key(|(_, entry)| entry.z_index)
            .map(|(_, entry)| entry.id);

        let mut roots = std::mem::take(&mut self.roots);
        roots.sort_by_key(|entry| entry.z_index);
        for entry in &mut roots {
            let (visible, rect) = entry
                .root_state
                .try_read(|state| (state.is_visible(), state.rect()))
                .expect("registered root state unavailable during frame");
            if !visible {
                entry.tree.clear_transient_targets();
                continue;
            }

            let pointer_input = hover_root == Some(entry.id);
            entry.tree.runtime.begin_frame(pointer_input);
            entry
                .tree
                .runtime
                .layout_tree_root(&mut entry.tree.root, self.style.as_ref(), atlas.clone(), rect, viewport);
            {
                let input = self.input.borrow();
                Self::route_entry_input(entry, self.style.as_ref(), &input, keyboard_root == Some(entry.id));
                entry
                    .tree
                    .runtime
                    .update_tree_root(&mut entry.tree.root, self.style.as_ref(), atlas.clone(), &input);
            }

            let (visible, next_rect) = entry
                .root_state
                .try_read(|state| (state.is_visible(), state.rect()))
                .expect("registered root state unavailable after root update");
            if !visible {
                entry.tree.clear_transient_targets();
                continue;
            }
            entry
                .tree
                .runtime
                .layout_tree_root(&mut entry.tree.root, self.style.as_ref(), atlas.clone(), next_rect, viewport);
            entry
                .tree
                .runtime
                .paint_tree_root(&mut entry.tree.root, &mut self.display_list, self.style.as_ref(), atlas.clone());
            entry
                .root_state
                .try_read(|state| record_root_overlay(&mut self.display_list, viewport, state, self.style.as_ref(), &atlas))
                .expect("registered root state unavailable during overlay paint");
            entry.just_opened = false;
        }
        self.roots = roots;
    }

    fn dismiss_outside_popup(&mut self, mouse: Vec2i) {
        let popup = self.roots.iter().enumerate().find_map(|(index, entry)| {
            if entry.kind != WindowKind::Popup || entry.just_opened {
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

    fn route_entry_input(entry: &mut WindowEntry, style: &Style, input: &Input, route_focus_input: bool) {
        let roots = std::slice::from_mut(&mut entry.tree.root);
        if entry.tree.runtime.accepts_pointer_input() || entry.tree.runtime.capture.is_some() {
            for event in pointer_events_from_input(input) {
                if entry.tree.runtime.route_captured_pointer_input_event(roots, style, input, &event).is_some() {
                    continue;
                }
                if !entry.tree.runtime.accepts_pointer_input() {
                    continue;
                }
                let transform = entry.tree.runtime.root_transform();
                if let Some((owner, result)) = entry.tree.runtime.route_input_event_to_node_ref(&mut roots[0], transform, style, &event) {
                    entry.tree.runtime.update_pointer_capture(owner, result, &event, input);
                }
            }
        }
        if route_focus_input {
            entry.tree.runtime.route_focus_input_events(roots, style, input);
        }
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
