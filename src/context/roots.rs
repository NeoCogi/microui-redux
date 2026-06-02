//! Node-root registry, visibility policy, and root traversal.

use super::*;
use crate::ControlColor;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootKind {
    Window,
    Dialog,
    Popup,
}

/// `UiRuntime` root rendered by the retained node path.
pub(super) struct NodeRootEntry {
    pub(super) id: RootId,
    name: String,
    rect: Recti,
    opt: ContainerOption,
    scroll_behavior: ScrollBehavior,
    visible: bool,
    kind: RootKind,
    just_opened: bool,
    active_chrome: Option<NodeRootChromePart>,
    pub(super) z_index: i32,
    pub(super) runtime: UiRuntime,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum NodeRootChromePart {
    Title,
    Close,
    Resize,
}

#[derive(Copy, Clone, Debug)]
struct NodeRootChrome {
    body: Recti,
    title: Option<Recti>,
    close: Option<Recti>,
    resize: Option<Recti>,
}

impl NodeRootChrome {
    fn new(rect: Recti, style: &Style, atlas: &crate::AtlasHandle, opt: ContainerOption) -> Self {
        let title = (!opt.intersects(ContainerOption::NO_TITLE)).then(|| Recti::new(rect.x, rect.y, rect.width, node_root_titlebar_height(style, atlas)));
        let close = title.and_then(|title| {
            (!opt.intersects(ContainerOption::NO_CLOSE)).then(|| Recti::new(title.x + title.width - title.height, title.y, title.height, title.height))
        });
        let resize = (!opt.intersects(ContainerOption::AUTO_SIZE) && !opt.intersects(ContainerOption::NO_RESIZE)).then(|| {
            let size = style.scrollbar_size.max(0);
            Recti::new(rect.x + rect.width - size, rect.y + rect.height - size, size, size)
        });
        let mut body = rect;
        if let Some(title) = title {
            body.y += title.height;
            body.height = body.height.saturating_sub(title.height);
        }
        Self { body, title, close, resize }
    }

    fn hit_test(&self, point: crate::Vec2i) -> Option<NodeRootChromePart> {
        if self.close.is_some_and(|rect| rect.contains(&point)) {
            Some(NodeRootChromePart::Close)
        } else if self.resize.is_some_and(|rect| rect.contains(&point)) {
            Some(NodeRootChromePart::Resize)
        } else if self.title.is_some_and(|rect| rect.contains(&point)) {
            Some(NodeRootChromePart::Title)
        } else {
            None
        }
    }
}

impl<R: Renderer> Context<R> {
    fn register_node_root(
        &mut self,
        kind: RootKind,
        name: &str,
        rect: Recti,
        tree: WidgetTree,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        visible: bool,
    ) -> RootId {
        let id = self.next_root_id();
        let z_index = if visible {
            self.last_zindex += 1;
            self.last_zindex
        } else {
            -1
        };
        self.node_roots.push(NodeRootEntry {
            id,
            name: name.to_string(),
            rect,
            opt,
            scroll_behavior,
            visible,
            kind,
            just_opened: false,
            active_chrome: None,
            z_index,
            runtime: UiRuntime::from_widget_tree(tree),
        });
        id
    }

    /// Creates a retained scroll-area handle for use with [`crate::WidgetTreeBuilder::scroll_area`].
    pub fn new_scroll_area(&mut self, name: &str) -> ScrollAreaHandle {
        ScrollAreaHandle::new(ScrollArea::new(name))
    }

    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained root id counter overflowed");
        id
    }

    /// Registers an open retained window and returns its stable root identifier.
    pub fn create_window(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        self.register_node_root(RootKind::Window, name, rect, tree, ContainerOption::NONE, ScrollBehavior::NONE, true)
    }

    /// Compatibility alias for the old transitional node-root constructor.
    pub fn create_node_window(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        self.create_window(name, rect, tree)
    }

    /// Compatibility alias for the old transitional node-popup constructor.
    pub fn create_node_popup(&mut self, name: &str, tree: WidgetTree) -> RootId {
        self.create_popup(name, tree)
    }

    /// Compatibility alias for [`Self::set_root_rect`].
    pub fn set_node_root_rect(&mut self, root: RootId, rect: Recti) {
        self.set_root_rect(root, rect);
    }

    /// Compatibility alias for [`Self::root_rect`].
    pub fn node_root_rect(&self, root: RootId) -> Option<Recti> {
        self.root_rect(root)
    }

    /// Compatibility alias for [`Self::root_visible`].
    pub fn node_root_visible(&self, root: RootId) -> Option<bool> {
        self.root_visible(root)
    }

    /// Returns whether a registered root is currently visible.
    pub fn root_visible(&self, root: RootId) -> Option<bool> {
        self.node_roots.iter().find(|entry| entry.id == root).map(|entry| entry.visible)
    }

    /// Returns the current rectangle for a registered root.
    pub fn root_rect(&self, root: RootId) -> Option<Recti> {
        self.node_roots.iter().find(|entry| entry.id == root).map(|entry| entry.rect)
    }

    /// Replaces the current rectangle for a registered root.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.rect = rect;
        }
    }

    /// Updates a registered root size without changing its origin.
    pub fn set_node_root_size(&mut self, root: RootId, size: &Dimensioni) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.rect.width = size.width;
            entry.rect.height = size.height;
        }
    }

    /// Compatibility alias for [`Self::set_root_focus_node`].
    pub fn set_node_root_focus_node(&mut self, root: RootId, node_id: crate::NodeId) {
        self.set_root_focus_node(root, node_id);
    }

    /// Sets focus to a node inside a registered root.
    pub fn set_root_focus_node(&mut self, root: RootId, node_id: crate::NodeId) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.runtime.set_focus_node(node_id);
        }
    }

    pub(crate) fn root_control_metrics(&self) -> (i32, i32) {
        let padding = self.style.padding.max(0);
        let font_height = self.canvas.get_atlas().get_font_height(self.style.font) as i32;
        let vertical_pad = std::cmp::max(1, padding / 2);
        let icon_height = self.canvas.get_atlas().get_icon_size(crate::EXPAND_DOWN_ICON).height;
        (std::cmp::max(font_height + vertical_pad * 2, icon_height), self.style.spacing.max(0))
    }

    /// Registers a hidden dialog root.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, tree: WidgetTree) -> RootId {
        self.register_node_root(RootKind::Dialog, name, rect, tree, ContainerOption::NONE, ScrollBehavior::NONE, false)
    }

    /// Registers a hidden popup root.
    pub fn create_popup(&mut self, name: &str, tree: WidgetTree) -> RootId {
        self.register_node_root(
            RootKind::Popup,
            name,
            Recti::default(),
            tree,
            Self::default_popup_options(),
            ScrollBehavior::NO_SCROLL,
            false,
        )
    }

    /// Replaces the retained widget tree for a registered root.
    pub fn set_root_tree(&mut self, root: RootId, tree: WidgetTree) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.runtime.replace_widget_tree(tree);
        }
    }

    /// Replaces the chrome options and scroll behavior for a registered root.
    pub fn set_root_options(&mut self, root: RootId, opt: ContainerOption, scroll_behavior: ScrollBehavior) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            entry.opt = opt;
            entry.scroll_behavior = scroll_behavior;
        }
    }

    /// Shows or hides a registered root.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) {
        let mouse_pos = self.input.borrow().mouse_pos;
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            let was_visible = entry.visible;
            entry.visible = visible;
            if visible {
                if entry.kind == RootKind::Popup && !was_visible {
                    entry.rect = rect(mouse_pos.x, mouse_pos.y, 1, 1);
                }
                self.last_zindex += 1;
                entry.z_index = self.last_zindex;
                if entry.kind == RootKind::Popup && !was_visible {
                    entry.just_opened = true;
                }
            } else {
                entry.active_chrome = None;
            }
        }
    }

    /// Raises a registered root above other roots.
    pub fn bring_root_to_front(&mut self, root: crate::RootId) {
        if let Some(entry) = self.node_roots.iter_mut().find(|entry| entry.id == root) {
            self.last_zindex += 1;
            entry.z_index = self.last_zindex;
        }
    }

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

        let (mouse_pos, mouse_pressed, mouse_down, mouse_delta) = {
            let input = self.input.borrow();
            (input.mouse_pos, input.mouse_pressed, input.mouse_down, input.mouse_delta)
        };
        let hover_root = self
            .node_roots
            .iter()
            .filter(|entry| entry.visible && entry.rect.contains(&mouse_pos))
            .max_by_key(|entry| entry.z_index)
            .map(|entry| entry.id);
        if !mouse_pressed.is_empty() {
            if let Some(root) = hover_root {
                self.bring_root_to_front(root);
            }
        }

        self.update_node_root_window_manager_chrome(hover_root, mouse_pos, mouse_pressed, mouse_down, mouse_delta);

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
                let hover_root_active = hover_root == Some(entry.id);
                let chrome = NodeRootChrome::new(entry.rect, self.style.as_ref(), &self.canvas.get_atlas(), entry.opt);
                self.paint_node_root_frame(entry);
                let input = self.input.borrow();
                entry.runtime.render_frame(
                    entry.id,
                    &entry.name,
                    &mut self.canvas,
                    self.style.as_ref(),
                    &input,
                    &mut self.frame_results,
                    chrome.body,
                    entry.scroll_behavior,
                    hover_root_active,
                );
                drop(input);
                self.paint_node_root_chrome(entry, chrome);
            }
        }
        self.node_roots = roots;
    }

    fn paint_node_root_frame(&mut self, entry: &NodeRootEntry) {
        if !entry.opt.intersects(ContainerOption::NO_FRAME) {
            self.draw_node_root_frame(entry.rect, ControlColor::WindowBG);
        }
    }

    fn paint_node_root_chrome(&mut self, entry: &NodeRootEntry, chrome: NodeRootChrome) {
        if let Some(title) = chrome.title {
            self.draw_node_root_frame(title, ControlColor::TitleBG);
            self.draw_node_root_title_text(title, &entry.name);

            if let Some(close) = chrome.close {
                let color = self.style.colors[ControlColor::TitleText as usize];
                self.canvas.draw_icon(crate::CLOSE_ICON, close, color);
            }
        }

        if let Some(resize) = chrome.resize {
            if resize.width > 0 && resize.height > 0 {
                self.draw_node_root_frame(resize, ControlColor::WindowBG);
            }
        }
    }

    fn draw_node_root_frame(&mut self, rect: Recti, color: ControlColor) {
        let fill = self.style.colors[color as usize];
        self.canvas.draw_rect(rect, fill);
        if let Some(border) = self.style.frame_border_color(color) {
            draw_canvas_box(&mut self.canvas, crate::expand_rect(rect, 1), border);
        }
    }

    fn draw_node_root_title_text(&mut self, rect: Recti, title: &str) {
        let atlas = self.canvas.get_atlas();
        let color = self.style.colors[ControlColor::TitleText as usize];
        let pos =
            crate::text_layout::control_text_position_with_font(self.style.as_ref(), &atlas, self.style.title_font, title, rect, crate::WidgetOption::NONE);
        self.canvas.draw_chars(self.style.title_font, title, pos, color);
    }

    fn update_node_root_window_manager_chrome(
        &mut self,
        hover_root: Option<RootId>,
        mouse_pos: crate::Vec2i,
        mouse_pressed: MouseButton,
        mouse_down: MouseButton,
        mouse_delta: crate::Vec2i,
    ) {
        if mouse_down.is_empty() {
            for entry in &mut self.node_roots {
                entry.active_chrome = None;
            }
            return;
        }

        let atlas = self.canvas.get_atlas();
        for entry in &mut self.node_roots {
            if !entry.visible {
                entry.active_chrome = None;
                continue;
            }

            if mouse_pressed.intersects(MouseButton::LEFT) && hover_root == Some(entry.id) {
                let chrome = NodeRootChrome::new(entry.rect, self.style.as_ref(), &atlas, entry.opt);
                match chrome.hit_test(mouse_pos) {
                    Some(NodeRootChromePart::Close) => {
                        entry.visible = false;
                        entry.active_chrome = None;
                        continue;
                    }
                    Some(NodeRootChromePart::Resize) => {
                        entry.active_chrome = Some(NodeRootChromePart::Resize);
                        continue;
                    }
                    Some(NodeRootChromePart::Title) => {
                        entry.active_chrome = Some(NodeRootChromePart::Title);
                    }
                    None => {}
                }
            }

            match entry.active_chrome {
                Some(NodeRootChromePart::Title) => {
                    entry.rect.x = entry.rect.x.saturating_add(mouse_delta.x);
                    entry.rect.y = entry.rect.y.saturating_add(mouse_delta.y);
                }
                Some(NodeRootChromePart::Resize) => {
                    entry.rect.width = entry.rect.width.saturating_add(mouse_delta.x).max(96);
                    entry.rect.height = entry.rect.height.saturating_add(mouse_delta.y).max(64);
                }
                Some(NodeRootChromePart::Close) | None => {}
            }
        }
    }

    const fn default_popup_options() -> ContainerOption {
        ContainerOption::AUTO_SIZE.union(ContainerOption::NO_RESIZE).union(ContainerOption::NO_TITLE)
    }

    #[cfg(test)]
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        let mut names: Vec<(i32, String)> = self
            .node_roots
            .iter()
            .filter(|entry| entry.visible)
            .map(|entry| (entry.z_index, entry.name.clone()))
            .collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        names.into_iter().map(|(_, name)| name).collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_rects(&self, root: RootId) -> Option<&[Recti]> {
        self.node_roots.iter().find(|entry| entry.id == root).map(|entry| entry.runtime.debug_rects())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_texts(&self, root: RootId) -> Vec<String> {
        self.node_roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.runtime.debug_texts().to_vec())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_zindex(&self, root: RootId) -> Option<i32> {
        self.node_roots.iter().find(|entry| entry.id == root).map(|entry| entry.z_index)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_body(&self, root: RootId) -> Option<Recti> {
        self.node_roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| NodeRootChrome::new(entry.rect, self.style.as_ref(), &self.canvas.get_atlas(), entry.opt).body)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self, root: RootId) -> Option<Dimensioni> {
        self.node_roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.runtime.debug_root_content_size())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::NodeId) -> Option<Recti> {
        self.node_roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| entry.runtime.debug_node_rect(node))
    }

    #[cfg(test)]
    pub(crate) fn debug_root_chrome(&self, root: RootId) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        self.node_roots.iter().find(|entry| entry.id == root).map(|entry| {
            let chrome = NodeRootChrome::new(entry.rect, self.style.as_ref(), &self.canvas.get_atlas(), entry.opt);
            (chrome.title, chrome.close, chrome.resize)
        })
    }
}

fn node_root_titlebar_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.title_font) as i32;
    let padding = style.padding.max(0);
    let min_title_h = font_height + (padding / 2).max(1) * 2;
    style.title_height.max(min_title_h)
}

fn draw_canvas_box<R: Renderer>(canvas: &mut Canvas<R>, r: Recti, color: Color) {
    canvas.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
    canvas.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
    canvas.draw_rect(rect(r.x, r.y, 1, r.height), color);
    canvas.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
}
