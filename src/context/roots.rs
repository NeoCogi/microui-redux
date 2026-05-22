//! Retained root registry, visibility policy, and root traversal.

use super::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Runtime behavior class for a registered root.
pub(super) enum RootKind {
    Window,
    Dialog,
    Popup,
}

/// Registered retained root plus the tree/options needed to render it every frame.
pub(super) struct RootEntry {
    /// Stable application-facing identifier for this root.
    pub(super) id: RootId,
    /// Window/dialog/popup runtime handle containing container state.
    pub(super) handle: WindowHandle,
    /// Retained tree rendered inside the root body.
    tree: WidgetTree,
    /// Container chrome/sizing options.
    opt: ContainerOption,
    /// Scroll behavior applied to the root body.
    scroll_behavior: ScrollBehavior,
    /// Whether this root should be considered during frame traversal.
    visible: bool,
    /// Behavior class for opening, closing, and hover routing.
    kind: RootKind,
}

impl<R: Renderer> Context<R> {
    /// Creates an open top-level window handle with a new root id.
    fn new_window(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let root_id = self.next_root_id();
        let mut window = WindowHandle::window(root_id, name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect);
        self.bring_to_front(&mut window);
        window
    }

    /// Creates a hidden dialog handle with a new root id.
    fn new_dialog(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let root_id = self.next_root_id();
        WindowHandle::dialog(root_id, name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect)
    }

    /// Creates a hidden popup handle with a new root id.
    fn new_popup(&mut self, name: &str) -> WindowHandle {
        let root_id = self.next_root_id();
        WindowHandle::popup(root_id, name, self.canvas.get_atlas(), self.style.clone(), self.input.clone())
    }

    /// Creates a retained panel handle for use with [`crate::WidgetTreeBuilder::container`].
    ///
    /// The handle owns panel-local focus, hover, scroll, layout cache, and draw commands across
    /// frames; application code supplies its children through the retained tree.
    pub fn new_panel(&mut self, name: &str) -> ContainerHandle {
        ContainerHandle::new(Container::new(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone()))
    }

    /// Allocates the next stable root id.
    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained root id counter overflowed");
        id
    }

    /// Stores a retained root entry and returns the handle's stable root id.
    fn register_root(
        &mut self,
        kind: RootKind,
        handle: WindowHandle,
        tree: WidgetTree,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        visible: bool,
    ) -> RootId {
        let id = handle.root_id();
        // The handle owns mutable runtime state; the tree remains replaceable by app code.
        self.retained_roots.push(RootEntry {
            id,
            handle,
            tree,
            opt,
            scroll_behavior,
            visible,
            kind,
        });
        id
    }

    /// Finds a mutable registered root by stable id.
    fn root_entry_mut(&mut self, root: RootId) -> Option<&mut RootEntry> {
        self.retained_roots.iter_mut().find(|entry| entry.id == root)
    }

    /// Registers an open retained window and returns its stable root identifier.
    ///
    /// The window is rendered by subsequent calls to [`Context::update_ui`] without the
    /// application re-submitting its tree.
    pub fn create_window(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        let window = self.new_window(name, rect);
        self.register_root(RootKind::Window, window, tree, ContainerOption::NONE, ScrollBehavior::NONE, true)
    }

    /// Registers a retained dialog root.
    ///
    /// Dialogs start hidden; call [`Context::set_root_visible`] with `true` to open the dialog and
    /// bring it to the front.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        let dialog = self.new_dialog(name, rect);
        self.register_root(RootKind::Dialog, dialog, tree, ContainerOption::NONE, ScrollBehavior::NONE, false)
    }

    /// Registers a retained popup root.
    ///
    /// Popups start hidden; calling [`Context::set_root_visible`] with `true` opens the popup at the
    /// current mouse position.
    pub fn create_popup(&mut self, name: &str, tree: WidgetTree) -> RootId {
        let popup = self.new_popup(name);
        self.register_root(RootKind::Popup, popup, tree, Self::default_popup_options(), ScrollBehavior::NONE, false)
    }

    /// Replaces the retained widget tree for a registered root.
    ///
    /// Invalid root identifiers are ignored.
    pub fn set_root_tree(&mut self, root: RootId, tree: WidgetTree) {
        if let Some(entry) = self.root_entry_mut(root) {
            entry.tree = tree;
        }
    }

    /// Replaces the container options and scroll behavior for a registered root.
    ///
    /// Popups are created with the default retained popup options. This method can override those
    /// defaults for retained roots that need custom chrome or sizing.
    pub fn set_root_options(&mut self, root: RootId, opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        if let Some(entry) = self.root_entry_mut(root) {
            entry.opt = opt;
            entry.scroll_behavior = scroll_behavior;
        }
    }

    /// Shows or hides a registered retained root.
    ///
    /// Windows reopen in their existing z-order. Dialogs and newly opened popups are brought to the
    /// front.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) {
        let Some(index) = self.retained_roots.iter().position(|entry| entry.id == root) else {
            return;
        };
        self.set_root_visible_at(index, visible);
    }

    /// Returns the window handle owned by a registered retained root.
    pub fn root_handle(&self, root: RootId) -> Option<WindowHandle> {
        self.retained_roots.iter().find(|entry| entry.id == root).map(|entry| entry.handle.clone())
    }

    /// Bumps the window's Z order so it renders above others.
    pub fn bring_to_front(&mut self, window: &mut WindowHandle) {
        self.last_zindex += 1;
        window.set_zindex(self.last_zindex);
    }

    /// Applies visibility changes by index so callers can avoid a second root lookup.
    fn set_root_visible_at(&mut self, index: usize, visible: bool) {
        let mouse_pos = self.input.borrow().mouse_pos;
        let mut bring_to_front = None;
        let mut hover_root = None;

        {
            let entry = &mut self.retained_roots[index];
            if !visible {
                entry.visible = false;
                entry.handle.close();
                return;
            }

            let was_open = entry.handle.is_open();
            entry.visible = true;
            match entry.kind {
                RootKind::Window => {
                    // Windows preserve their z-order when reopened.
                    entry.handle.open();
                }
                RootKind::Dialog => {
                    entry.handle.open();
                    if !was_open {
                        // Newly opened dialogs should float above normal windows.
                        bring_to_front = Some(entry.handle.clone());
                    }
                }
                RootKind::Popup => {
                    if !was_open {
                        // Popups anchor at the current pointer and become the hover root for the
                        // opening frame so their first click does not immediately close them.
                        entry.handle.set_rect(rect(mouse_pos.x, mouse_pos.y, 1, 1));
                        entry.handle.open();
                        entry.handle.set_root_hover_active(true);
                        entry.handle.mark_popup_just_opened();
                        bring_to_front = Some(entry.handle.clone());
                        hover_root = Some(entry.handle.clone());
                    }
                }
            }
        }

        if let Some(mut window) = bring_to_front {
            self.bring_to_front(&mut window);
        }
        if let Some(window) = hover_root {
            self.next_hover_root = Some(window.clone());
            self.hover_root = Some(window);
        }
    }

    /// Brings a window forward only when it is not already top-most.
    fn bring_to_front_if_behind(&mut self, window: &mut WindowHandle) {
        if window.zindex() < self.last_zindex {
            self.bring_to_front(window);
        }
    }

    #[inline(never)]
    /// Starts command recording and hover/scroll routing for a root container.
    fn begin_root_container(&mut self, window: &mut WindowHandle) {
        window.prepare_for_frame(self.frame);
        self.root_list.push(window.clone());

        // Highest z-index root under the pointer becomes next frame's hover root.
        if window.root_contains_point(self.input.borrow().mouse_pos)
            && (self.next_hover_root.is_none() || window.zindex() > self.next_hover_root.as_ref().unwrap().zindex())
        {
            self.next_hover_root = Some(window.clone());
        }
        let scroll_delta = self.input.borrow().scroll_delta;
        let pending_scroll = if window.root_in_hover_root() && (scroll_delta.x != 0 || scroll_delta.y != 0) {
            Some(scroll_delta)
        } else {
            None
        };
        window.begin_root_command_scope(pending_scroll);
    }

    #[inline(never)]
    /// Ends command recording for a root container.
    fn end_root_container(&mut self, window: &mut WindowHandle) {
        window.finish_root_command_scope();
    }

    #[inline(never)]
    #[must_use]
    /// Opens a root for retained traversal and returns whether its body should be rendered.
    fn begin_window(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior) -> bool {
        if !window.is_open() {
            return false;
        }

        if !self.update_popup_root_state(window) {
            return false;
        }

        self.begin_root_container(window);
        window.begin_window(&mut self.frame_results, opt, scroll_behavior);

        true
    }

    /// Completes retained traversal for a root and applies resize results.
    fn end_window(&mut self, window: &mut WindowHandle, opt: ContainerOption) {
        window.end_window();
        self.end_root_container(window);
        window.finish_resize(&mut self.frame_results, opt);
    }

    /// Handles popup auto-close behavior before a popup root is traversed.
    fn update_popup_root_state(&mut self, window: &mut WindowHandle) -> bool {
        if !window.root_is_popup() {
            return true;
        }

        if window.root_popup_just_opened() {
            window.clear_root_popup_just_opened();
            return true;
        }

        let click_outside_popup = {
            let input = self.input.borrow();
            // A popup closes only on a press outside both its hover root and rectangle.
            !input.mouse_pressed.is_none() && !window.root_in_hover_root() && !window.root_contains_point(input.mouse_pos)
        };
        if click_outside_popup {
            window.close();
            return false;
        }

        true
    }

    /// Measures, begins, traverses, and ends one window-like retained tree.
    fn render_window_tree(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        if window.is_open() {
            window.set_root_style(self.style.clone());
            if opt.is_auto_sizing() {
                // Auto-size is measured against committed previous-frame results before the live traversal.
                window.measure_auto_size(&self.frame_results, opt, scroll_behavior, tree);
            }
        }

        if self.begin_window(window, opt, scroll_behavior) {
            {
                let mut inner = window.inner_mut();
                // Widget traversal records layout, interaction, and draw commands into the root container.
                inner.main.widget_tree(&mut self.frame_results, tree);
            }
            self.end_window(window, opt);

            if !window.is_open() {
                window.reset_after_close();
            }
        }
    }

    /// Renders an open dialog and forces it to remain the active hover/root focus layer.
    fn render_dialog_tree(&mut self, window: &mut WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        if window.is_open() {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            window.set_root_hover_active(true);
            self.bring_to_front_if_behind(window);

            self.render_window_tree(window, opt, scroll_behavior, tree);
        }
    }

    /// Renders one registered retained root if it is visible and still open.
    fn render_retained_root(&mut self, entry: &mut RootEntry) {
        if !entry.visible {
            return;
        }
        if !entry.handle.is_open() {
            entry.visible = false;
            return;
        }

        match entry.kind {
            RootKind::Window => self.render_window_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Dialog => self.render_dialog_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Popup => self.render_window_tree(&mut entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
        }

        if !entry.handle.is_open() {
            entry.visible = false;
        }
    }

    /// Traverses all retained roots without borrowing `self.retained_roots` during rendering.
    pub(super) fn render_registered_roots(&mut self) {
        // Rendering needs `&mut self` for z-order, hover, and frame results, so take the root list
        // out temporarily to avoid aliasing the vector while entries are rendered.
        let mut roots = std::mem::take(&mut self.retained_roots);
        for entry in &mut roots {
            self.render_retained_root(entry);
        }
        self.retained_roots = roots;
    }

    /// Returns the chrome options used by retained popups unless the app overrides them.
    const fn default_popup_options() -> ContainerOption {
        ContainerOption::AUTO_SIZE.union(ContainerOption::NO_RESIZE).union(ContainerOption::NO_TITLE)
    }
}
