//! Retained root registry, visibility policy, and root traversal.

use super::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Runtime behavior class for a registered root.
pub(super) enum RootKind {
    /// Normal top-level window.
    Window,
    /// Modal dialog root.
    Dialog,
    /// Popup root that can auto-close when focus moves elsewhere.
    Popup,
}

/// Registered retained root plus the tree/options needed to render it every frame.
pub(super) struct RootEntry {
    /// Stable application-facing identifier for this root.
    pub(super) id: RootId,
    /// Window/dialog/popup runtime handle containing root traversal state.
    pub(super) handle: WindowHandle,
    /// Retained tree rendered inside the root body.
    tree: WidgetTree,
    /// Root chrome/sizing options.
    opt: ContainerOption,
    /// Scroll behavior applied to the root body.
    scroll_behavior: ScrollBehavior,
    /// Whether this root should be considered during frame traversal.
    visible: bool,
    /// Behavior class for opening, closing, and hover routing.
    kind: RootKind,
}

/// Experimental `UiNode` root rendered by the new runtime path.
pub(super) struct NodeRootEntry {
    /// Stable application-facing identifier for this root.
    pub(super) id: RootId,
    /// Root display name.
    name: String,
    /// Root rectangle.
    rect: Recti,
    /// Root chrome/sizing options.
    opt: ContainerOption,
    /// Scroll behavior applied to the root body.
    scroll_behavior: ScrollBehavior,
    /// Whether this root is visible.
    visible: bool,
    /// Behavior class for root routing and popup lifecycle.
    kind: RootKind,
    /// Popup guard used to avoid immediate close on the opening frame.
    just_opened: bool,
    /// Root z-order used for rendering and pointer ownership.
    pub(super) z_index: i32,
    /// Node runtime compiled from the retained tree.
    pub(super) runtime: UiRuntime,
}

impl<R: Renderer> Context<R> {
    /// Creates an open top-level window handle with a new root id.
    fn new_window(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let root_id = self.next_root_id();
        let window = WindowHandle::window(root_id, name, self.canvas.get_atlas(), self.style.clone(), self.input.clone(), initial_rect);
        self.bring_to_front(&window);
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

    /// Creates a retained scroll-area handle for use with [`crate::WidgetTreeBuilder::scroll_area`].
    ///
    /// The handle owns scroll-area-local focus, hover, scroll, layout cache, and draw commands across
    /// frames; application code supplies its children through the retained tree.
    pub fn new_scroll_area(&mut self, name: &str) -> ScrollAreaHandle {
        ScrollAreaHandle::new(ScrollArea::new(name, self.canvas.get_atlas(), self.style.clone(), self.input.clone()))
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

    /// Registers an experimental `UiNode`-runtime window.
    ///
    /// This is the first live path for the common-node runtime. It intentionally supports a
    /// smaller surface than [`Self::create_window`] while the enum-based layout/update/paint
    /// passes are brought up.
    pub fn create_node_window(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        let id = self.next_root_id();
        self.last_zindex += 1;
        let z_index = self.last_zindex;
        self.node_roots.push(NodeRootEntry {
            id,
            name: name.to_string(),
            rect,
            opt: ContainerOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            visible: true,
            kind: RootKind::Window,
            just_opened: false,
            z_index,
            runtime: UiRuntime::from_widget_tree(tree),
        });
        id
    }

    /// Registers a hidden experimental `UiNode` popup root.
    pub fn create_node_popup(&mut self, name: &str, tree: WidgetTree) -> RootId {
        let id = self.next_root_id();
        self.node_roots.push(NodeRootEntry {
            id,
            name: name.to_string(),
            rect: Recti::default(),
            opt: Self::default_popup_options(),
            scroll_behavior: ScrollBehavior::NO_SCROLL,
            visible: false,
            kind: RootKind::Popup,
            just_opened: false,
            z_index: -1,
            runtime: UiRuntime::from_widget_tree(tree),
        });
        id
    }

    /// Replaces the rectangle used by an experimental `UiNode` root.
    pub fn set_node_root_rect(&mut self, root: RootId, rect: Recti) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.rect = rect;
        }
    }

    /// Returns the rectangle used by an experimental `UiNode` root.
    pub fn node_root_rect(&self, root: RootId) -> Option<Recti> {
        self.node_roots.iter().find(|entry| entry.id == root).map(|entry| entry.rect)
    }

    /// Returns whether an experimental `UiNode` root is visible.
    pub fn node_root_visible(&self, root: RootId) -> Option<bool> {
        self.node_roots.iter().find(|entry| entry.id == root).map(|entry| entry.visible)
    }

    /// Updates the size of an experimental `UiNode` root without changing its origin.
    pub fn set_node_root_size(&mut self, root: RootId, size: &Dimensioni) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.rect.width = size.width;
            entry.rect.height = size.height;
        }
    }

    /// Sets focus to a node inside an experimental `UiNode` root.
    pub fn set_node_root_focus_node(&mut self, root: RootId, node_id: crate::NodeId) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.runtime.set_focus_node(node_id);
        }
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
            return;
        }
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.runtime.replace_widget_tree(tree);
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
            return;
        }
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
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
            if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
                let was_visible = entry.visible;
                entry.visible = visible;
                if visible {
                    self.last_zindex += 1;
                    entry.z_index = self.last_zindex;
                    if entry.kind == RootKind::Popup && !was_visible {
                        entry.just_opened = true;
                    }
                }
            }
            return;
        };
        self.set_root_visible_at(index, visible);
    }

    /// Returns the window handle owned by a registered retained root.
    pub fn root_handle(&self, root: RootId) -> Option<WindowHandle> {
        self.retained_roots.iter().find(|entry| entry.id == root).map(|entry| entry.handle.clone())
    }

    /// Bumps the window's Z order so it renders above others.
    pub fn bring_to_front(&mut self, window: &WindowHandle) {
        self.last_zindex += 1;
        window.set_zindex(self.last_zindex);
    }

    /// Bumps an experimental node root above other roots.
    fn bring_node_root_to_front(&mut self, root: crate::RootId) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            self.last_zindex += 1;
            entry.z_index = self.last_zindex;
        }
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

        if let Some(window) = bring_to_front {
            self.bring_to_front(&window);
        }
        if let Some(window) = hover_root {
            self.next_hover_root = Some(window.clone());
            self.hover_root = Some(window);
        }
    }

    /// Brings a window forward only when it is not already top-most.
    fn bring_to_front_if_behind(&mut self, window: &WindowHandle) {
        if window.zindex() < self.last_zindex {
            self.bring_to_front(window);
        }
    }

    #[inline(never)]
    /// Starts command recording and hover/scroll routing for a root container.
    fn begin_root_container(&mut self, window: &WindowHandle) {
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
    fn end_root_container(&mut self, window: &WindowHandle) {
        window.finish_root_command_scope();
    }

    /// Handles popup auto-close behavior before a popup root is traversed.
    fn update_popup_root_state(&mut self, window: &WindowHandle) -> bool {
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
            !input.mouse_pressed.is_empty() && !window.root_in_hover_root() && !window.root_contains_point(input.mouse_pos)
        };
        if click_outside_popup {
            window.close();
            return false;
        }

        true
    }

    /// Measures, begins, traverses, and ends one window-like retained tree.
    fn render_window_tree(&mut self, window: &WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
        if window.is_open() {
            window.set_root_style(self.style.clone());
            if opt.intersects(ContainerOption::AUTO_SIZE) {
                // Auto-size is measured against committed previous-frame results before the live traversal.
                window.measure_auto_size(&self.frame_results, opt, scroll_behavior, tree);
            }
        }

        if window.is_open() && self.update_popup_root_state(window) {
            self.begin_root_container(window);
            window.render_tree(&mut self.frame_results, opt, scroll_behavior, tree);
            self.end_root_container(window);

            if !window.is_open() {
                window.reset_after_close();
            }
        }
    }

    /// Renders an open dialog and forces it to remain the active hover/root focus layer.
    fn render_dialog_tree(&mut self, window: &WindowHandle, opt: ContainerOption, scroll_behavior: ScrollBehavior, tree: &WidgetTree) {
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
            RootKind::Window => self.render_window_tree(&entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Dialog => self.render_dialog_tree(&entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
            RootKind::Popup => self.render_window_tree(&entry.handle, entry.opt, entry.scroll_behavior, &entry.tree),
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

    /// Traverses and renders experimental `UiNode` roots.
    pub(super) fn render_node_roots(&mut self) {
        for entry in &mut self.node_roots {
            if entry.visible && entry.opt.intersects(ContainerOption::AUTO_SIZE) {
                let size = entry
                    .runtime
                    .measure_auto_size(self.style.as_ref(), &self.canvas.get_atlas(), entry.opt, entry.rect.width);
                entry.rect.width = size.width;
                entry.rect.height = size.height;
            }
        }

        let (mouse_pos, mouse_pressed) = {
            let input = self.input.borrow();
            (input.mouse_pos, input.mouse_pressed)
        };
        let hover_root = self
            .node_roots
            .iter()
            .filter(|entry| entry.visible && entry.rect.contains(&mouse_pos))
            .max_by_key(|entry| entry.z_index)
            .map(|entry| entry.id);
        if !mouse_pressed.is_empty() {
            if let Some(root) = hover_root {
                self.bring_node_root_to_front(root);
            }
        }

        let mut roots = std::mem::take(&mut self.node_roots);
        roots.sort_by(|a, b| a.z_index.cmp(&b.z_index));
        for entry in &mut roots {
            if entry.visible {
                if entry.kind == RootKind::Popup {
                    if entry.just_opened {
                        entry.just_opened = false;
                    } else if !mouse_pressed.is_empty() && hover_root != Some(entry.id) && !entry.rect.contains(&mouse_pos) {
                        entry.visible = false;
                        continue;
                    }
                }
                let input = self.input.borrow();
                let hover_root_active = hover_root == Some(entry.id);
                entry.runtime.render_frame(
                    entry.id,
                    &mut self.canvas,
                    self.style.as_ref(),
                    &input,
                    &mut self.frame_results,
                    entry.rect,
                    &entry.name,
                    entry.opt,
                    entry.scroll_behavior,
                    hover_root_active,
                );
            }
        }
        self.node_roots = roots;
    }

    /// Returns the chrome options used by retained popups unless the app overrides them.
    const fn default_popup_options() -> ContainerOption {
        ContainerOption::AUTO_SIZE.union(ContainerOption::NO_RESIZE).union(ContainerOption::NO_TITLE)
    }
}
