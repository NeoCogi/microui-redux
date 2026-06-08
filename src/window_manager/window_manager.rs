//! Window manager root registry, visibility policy, chrome handling, and traversal.

use super::*;
use crate::ControlColor;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum WindowKind {
    Window,
    Dialog,
    Popup,
}

/// Window-manager entry that owns one retained `UiRuntime` root.
pub(super) struct WindowEntry {
    pub(super) id: RootId,
    name: String,
    rect: Recti,
    opt: ContainerOption,
    visible: bool,
    kind: WindowKind,
    just_opened: bool,
    active_chrome: Option<WindowChromePart>,
    pub(super) z_index: i32,
    pub(super) runtime: UiRuntime,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum WindowChromePart {
    Title,
    Close,
    Resize,
}

#[derive(Copy, Clone, Debug)]
struct WindowChrome {
    body: Recti,
    title: Option<Recti>,
    close: Option<Recti>,
    resize: Option<Recti>,
}

impl WindowChrome {
    fn new(rect: Recti, style: &Style, atlas: &crate::AtlasHandle, opt: ContainerOption) -> Self {
        let title = (!opt.intersects(ContainerOption::NO_TITLE)).then(|| Recti::new(rect.x, rect.y, rect.width, root_titlebar_height(style, atlas)));
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

    fn hit_test(&self, point: crate::Vec2i) -> Option<WindowChromePart> {
        if self.close.is_some_and(|rect| rect.contains(&point)) {
            Some(WindowChromePart::Close)
        } else if self.resize.is_some_and(|rect| rect.contains(&point)) {
            Some(WindowChromePart::Resize)
        } else if self.title.is_some_and(|rect| rect.contains(&point)) {
            Some(WindowChromePart::Title)
        } else {
            None
        }
    }
}

impl<R: Renderer> Context<R> {
    fn register_root(
        &mut self,
        kind: WindowKind,
        name: &str,
        rect: Recti,
        tree: UiNodeSet,
        opt: ContainerOption,
        visible: bool,
    ) -> RootId {
        let id = self.next_root_id();
        let z_index = if visible {
            self.last_zindex += 1;
            self.last_zindex
        } else {
            -1
        };
        self.roots.push(WindowEntry {
            id,
            name: name.to_string(),
            rect,
            opt,
            visible,
            kind,
            just_opened: false,
            active_chrome: None,
            z_index,
            runtime: UiRuntime::from_ui_nodes(tree),
        });
        id
    }

    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained root id counter overflowed");
        id
    }

    /// Registers an open retained window and returns its stable root identifier.
    pub fn create_window(&mut self, name: &str, rect: Recti, tree: UiNodeSet) -> RootId {
        self.register_root(WindowKind::Window, name, rect, tree, ContainerOption::NONE, true)
    }

    /// Returns whether a registered root is currently visible.
    pub fn root_visible(&self, root: RootId) -> Option<bool> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.visible)
    }

    /// Returns the current rectangle for a registered root.
    pub fn root_rect(&self, root: RootId) -> Option<Recti> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.rect)
    }

    /// Replaces the current rectangle for a registered root.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) {
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            entry.rect = rect;
        }
    }

    /// Updates a registered root size without changing its origin.
    pub fn set_root_size(&mut self, root: RootId, size: &Dimensioni) {
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            entry.rect.width = size.width;
            entry.rect.height = size.height;
        }
    }

    /// Sets focus to a node inside a registered root.
    pub fn set_root_focus_node(&mut self, root: RootId, node_id: crate::NodeId) {
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            entry.runtime.set_focus_node(node_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn scroll_area_scroll(&self, root: RootId, node_id: crate::NodeId) -> Option<crate::Vec2i> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| entry.runtime.scroll_area_state(node_id))
            .map(|state| state.scroll)
    }

    #[cfg(test)]
    pub(crate) fn scroll_area_body(&self, root: RootId, node_id: crate::NodeId) -> Option<Recti> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| entry.runtime.scroll_area_state(node_id))
            .map(|state| state.body)
    }

    #[cfg(test)]
    pub(crate) fn scroll_area_content_size(&self, root: RootId, node_id: crate::NodeId) -> Option<Dimensioni> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| entry.runtime.scroll_area_state(node_id))
            .map(|state| state.content_size)
    }

    pub(crate) fn root_control_metrics(&self) -> (i32, i32) {
        let padding = self.style.padding.max(0);
        let font_height = self.canvas.get_atlas().get_font_height(self.style.font) as i32;
        let vertical_pad = std::cmp::max(1, padding / 2);
        let icon_height = self.canvas.get_atlas().get_icon_size(crate::EXPAND_DOWN_ICON).height;
        (std::cmp::max(font_height + vertical_pad * 2, icon_height), self.style.spacing.max(0))
    }

    /// Registers a hidden dialog root.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, tree: UiNodeSet) -> RootId {
        self.register_root(WindowKind::Dialog, name, rect, tree, ContainerOption::NONE, false)
    }

    /// Registers a hidden popup root.
    pub fn create_popup(&mut self, name: &str, tree: UiNodeSet) -> RootId {
        self.register_root(
            WindowKind::Popup,
            name,
            Recti::default(),
            tree,
            Self::default_popup_options(),
            false,
        )
    }

    /// Replaces the retained UI node set for a registered root.
    pub fn set_root_nodes(&mut self, root: RootId, tree: UiNodeSet) {
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            entry.runtime.replace_ui_nodes(tree);
        }
    }

    /// Replaces the chrome options for a registered root.
    pub fn set_root_options(&mut self, root: RootId, opt: ContainerOption) {
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            entry.opt = opt;
        }
    }

    /// Shows or hides a registered root.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) {
        let mouse_pos = self.input.borrow().mouse_pos;
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            let was_visible = entry.visible;
            entry.visible = visible;
            if visible {
                if entry.kind == WindowKind::Popup && !was_visible {
                    entry.rect = rect(mouse_pos.x, mouse_pos.y, 1, 1);
                }
                self.last_zindex += 1;
                entry.z_index = self.last_zindex;
                if entry.kind == WindowKind::Popup && !was_visible {
                    entry.just_opened = true;
                }
            } else {
                entry.active_chrome = None;
            }
        }
    }

    /// Raises a registered root above other roots.
    pub fn bring_root_to_front(&mut self, root: crate::RootId) {
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            self.last_zindex += 1;
            entry.z_index = self.last_zindex;
        }
    }

    pub(super) fn render_window_manager(&mut self) {
        for entry in &mut self.roots {
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
            .roots
            .iter()
            .filter(|entry| entry.visible && entry.rect.contains(&mouse_pos))
            .max_by_key(|entry| entry.z_index)
            .map(|entry| entry.id);
        if !mouse_pressed.is_empty() {
            if let Some(root) = hover_root {
                self.bring_root_to_front(root);
            }
        }

        self.update_window_manager_chrome(hover_root, mouse_pos, mouse_pressed, mouse_down, mouse_delta);

        let mut roots = std::mem::take(&mut self.roots);
        roots.sort_by(|a, b| a.z_index.cmp(&b.z_index));
        for entry in &mut roots {
            if entry.visible {
                if entry.kind == WindowKind::Popup {
                    if entry.just_opened {
                        entry.just_opened = false;
                    } else if !mouse_pressed.is_empty() && hover_root != Some(entry.id) && !entry.rect.contains(&mouse_pos) {
                        entry.visible = false;
                        continue;
                    }
                }
                let chrome_capturing_pointer = matches!(entry.active_chrome, Some(WindowChromePart::Title | WindowChromePart::Resize));
                let hover_root_active = hover_root == Some(entry.id) && !chrome_capturing_pointer;
                let chrome = WindowChrome::new(entry.rect, self.style.as_ref(), &self.canvas.get_atlas(), entry.opt);
                self.paint_root_frame(entry);
                let input = self.input.borrow();
                entry.runtime.render_frame(
                    entry.id,
                    &entry.name,
                    &mut self.canvas,
                    self.style.as_ref(),
                    &input,
                    &mut self.frame_results,
                    chrome.body,
                    hover_root_active,
                );
                drop(input);
                self.paint_root_chrome(entry, chrome);
            }
        }
        self.roots = roots;
    }

    fn paint_root_frame(&mut self, entry: &WindowEntry) {
        if !entry.opt.intersects(ContainerOption::NO_FRAME) {
            self.draw_root_frame(entry.rect, ControlColor::WindowBG);
        }
    }

    fn paint_root_chrome(&mut self, entry: &WindowEntry, chrome: WindowChrome) {
        if let Some(title) = chrome.title {
            self.draw_root_frame(title, ControlColor::TitleBG);
            let mut text = title;
            if let Some(close) = chrome.close {
                text.width = (close.x.max(title.x) - title.x).max(0);
            }
            self.draw_root_title_text(text, &entry.name);

            if let Some(close) = chrome.close {
                let color = self.style.colors[ControlColor::TitleText as usize];
                self.canvas.draw_icon(crate::CLOSE_ICON, close, color);
            }
        }

        if let Some(resize) = chrome.resize {
            if resize.width > 0 && resize.height > 0 {
                self.draw_root_frame(resize, ControlColor::WindowBG);
            }
        }
    }

    fn draw_root_frame(&mut self, rect: Recti, color: ControlColor) {
        let fill = self.style.colors[color as usize];
        self.canvas.draw_rect(rect, fill);
        if let Some(border) = self.style.frame_border_color(color) {
            draw_canvas_box(&mut self.canvas, crate::expand_rect(rect, 1), border);
        }
    }

    fn draw_root_title_text(&mut self, rect: Recti, title: &str) {
        if rect.width <= 0 || rect.height <= 0 {
            return;
        }
        let atlas = self.canvas.get_atlas();
        let color = self.style.colors[ControlColor::TitleText as usize];
        let pos =
            crate::text_layout::control_text_position_with_font(self.style.as_ref(), &atlas, self.style.title_font, title, rect, crate::WidgetOption::NONE);
        let old_clip = self.canvas.current_clip_rect();
        let text_clip = old_clip.intersect(&rect).unwrap_or_default();
        self.canvas.set_clip_rect(text_clip);
        self.canvas.draw_chars(self.style.title_font, title, pos, color);
        self.canvas.set_clip_rect(old_clip);
    }

    fn update_window_manager_chrome(
        &mut self,
        hover_root: Option<RootId>,
        mouse_pos: crate::Vec2i,
        mouse_pressed: MouseButton,
        mouse_down: MouseButton,
        mouse_delta: crate::Vec2i,
    ) {
        let atlas = self.canvas.get_atlas();
        for entry in &mut self.roots {
            if !entry.visible {
                entry.active_chrome = None;
                continue;
            }
            let min_size = root_min_size(self.style.as_ref(), &atlas, entry.opt, &entry.name);
            entry.rect.width = entry.rect.width.max(min_size.width);
            entry.rect.height = entry.rect.height.max(min_size.height);

            if mouse_down.is_empty() {
                entry.active_chrome = None;
                continue;
            }

            if mouse_pressed.intersects(MouseButton::LEFT) && hover_root == Some(entry.id) {
                let chrome = WindowChrome::new(entry.rect, self.style.as_ref(), &atlas, entry.opt);
                match chrome.hit_test(mouse_pos) {
                    Some(WindowChromePart::Close) => {
                        entry.visible = false;
                        entry.active_chrome = None;
                        continue;
                    }
                    Some(WindowChromePart::Resize) => {
                        entry.active_chrome = Some(WindowChromePart::Resize);
                        continue;
                    }
                    Some(WindowChromePart::Title) => {
                        entry.active_chrome = Some(WindowChromePart::Title);
                    }
                    None => {}
                }
            }

            match entry.active_chrome {
                Some(WindowChromePart::Title) => {
                    entry.rect.x = entry.rect.x.saturating_add(mouse_delta.x);
                    entry.rect.y = entry.rect.y.saturating_add(mouse_delta.y);
                }
                Some(WindowChromePart::Resize) => {
                    entry.rect.width = entry.rect.width.saturating_add(mouse_delta.x).max(min_size.width);
                    entry.rect.height = entry.rect.height.saturating_add(mouse_delta.y).max(min_size.height);
                }
                Some(WindowChromePart::Close) | None => {}
            }
        }
    }

    const fn default_popup_options() -> ContainerOption {
        ContainerOption::AUTO_SIZE.union(ContainerOption::NO_RESIZE).union(ContainerOption::NO_TITLE)
    }

    #[cfg(test)]
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        let mut names: Vec<(i32, String)> = self
            .roots
            .iter()
            .filter(|entry| entry.visible)
            .map(|entry| (entry.z_index, entry.name.clone()))
            .collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        names.into_iter().map(|(_, name)| name).collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_rects(&self, root: RootId) -> Option<&[Recti]> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.runtime.debug_rects())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_texts(&self, root: RootId) -> Vec<String> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.runtime.debug_texts().to_vec())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn debug_root_zindex(&self, root: RootId) -> Option<i32> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.z_index)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_body(&self, root: RootId) -> Option<Recti> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| WindowChrome::new(entry.rect, self.style.as_ref(), &self.canvas.get_atlas(), entry.opt).body)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self, root: RootId) -> Option<Dimensioni> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.runtime.debug_root_content_size())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::NodeId) -> Option<Recti> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| entry.runtime.debug_node_rect(node))
    }

    #[cfg(test)]
    pub(crate) fn debug_root_chrome(&self, root: RootId) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| {
            let chrome = WindowChrome::new(entry.rect, self.style.as_ref(), &self.canvas.get_atlas(), entry.opt);
            (chrome.title, chrome.close, chrome.resize)
        })
    }
}

fn root_titlebar_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let font_height = atlas.get_font_height(style.title_font) as i32;
    let padding = style.padding.max(0);
    let min_title_h = font_height + (padding / 2).max(1) * 2;
    style.title_height.max(min_title_h)
}

fn root_min_size(style: &Style, atlas: &crate::AtlasHandle, opt: ContainerOption, title: &str) -> Dimensioni {
    let auto_size = opt.intersects(ContainerOption::AUTO_SIZE);
    let mut width: i32 = if auto_size { 1 } else { 96 };
    let mut height: i32 = if auto_size { 1 } else { 64 };
    if !opt.intersects(ContainerOption::NO_TITLE) {
        let title_height = root_titlebar_height(style, atlas);
        let title_width = atlas.get_text_size(style.title_font, title).width;
        let close_width = if opt.intersects(ContainerOption::NO_CLOSE) { 0 } else { title_height };
        let padding = style.padding.max(0);
        width = width.max(title_width.saturating_add(close_width).saturating_add(padding.saturating_mul(2)));
        let title_min_height = if auto_size {
            title_height
        } else {
            title_height.saturating_add(padding.saturating_mul(2))
        };
        height = height.max(title_min_height);
    }
    Dimensioni::new(width, height)
}

fn draw_canvas_box<R: Renderer>(canvas: &mut Canvas<R>, r: Recti, color: Color) {
    canvas.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
    canvas.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
    canvas.draw_rect(rect(r.x, r.y, 1, r.height), color);
    canvas.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
}
