//! Window manager root registry, visibility policy, chrome handling, and traversal.

use super::*;
use crate::render::Painter;
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
    opt: WindowOption,
    visible: bool,
    kind: WindowKind,
    just_opened: bool,
    active_chrome: Option<WindowChromePart>,
    pub(super) z_index: i32,
    pub(super) roots: Vec<UiNode>,
    pub(super) z_order: Vec<UiNodeId>,
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
    fn new(rect: Recti, style: &Style, atlas: &crate::AtlasHandle, opt: WindowOption) -> Self {
        let client = crate::frame::frame_geometry(rect, opt.intersects(WindowOption::FRAME), style).content_or_empty();
        let title = (!opt.intersects(WindowOption::NO_TITLE))
            .then(|| Recti::new(client.x, client.y, client.width, root_titlebar_height(style, atlas).min(client.height.max(0))));
        let close = title.and_then(|title| {
            (!opt.intersects(WindowOption::NO_CLOSE)).then(|| Recti::new(title.x + title.width - title.height, title.y, title.height, title.height))
        });
        let resize = (!opt.intersects(WindowOption::AUTO_SIZE) && !opt.intersects(WindowOption::NO_RESIZE)).then(|| {
            let size = style.scrollbar_size.max(0);
            Recti::new(
                rect.x.saturating_add(rect.width).saturating_sub(size),
                rect.y.saturating_add(rect.height).saturating_sub(size),
                size.min(rect.width.max(0)),
                size.min(rect.height.max(0)),
            )
        });
        let mut body = client;
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

impl<B: RendererBackend> Context<B> {
    fn register_root(&mut self, kind: WindowKind, name: &str, rect: Recti, tree: UiNodeSet, opt: WindowOption, visible: bool) -> RootId {
        let id = self.next_root_id();
        let roots = tree.into_roots();
        let z_order = roots.iter().map(UiNode::id).collect();
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
            roots,
            z_order,
            runtime: UiRuntime::new(),
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
        self.register_root(WindowKind::Window, name, rect, tree, WindowOption::FRAME, true)
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
            entry.runtime.set_focus_node(&entry.roots, node_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn scroll_area_scroll(&self, root: RootId, node_id: crate::NodeId) -> Option<crate::Vec2i> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| crate::ui_node::scroll_area_state(&entry.roots, node_id))
            .map(|state| state.scroll)
    }

    #[cfg(test)]
    pub(crate) fn scroll_area_body(&self, root: RootId, node_id: crate::NodeId) -> Option<Recti> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        let state = crate::ui_node::scroll_area_state(&entry.roots, node_id)?;
        entry.runtime.debug_node_local_rect(&entry.roots, node_id, state.body)
    }

    #[cfg(test)]
    pub(crate) fn scroll_area_content_size(&self, root: RootId, node_id: crate::NodeId) -> Option<Dimensioni> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| crate::ui_node::scroll_area_state(&entry.roots, node_id))
            .map(|state| state.content_size)
    }

    pub(crate) fn root_spacing(&self) -> i32 {
        self.style.spacing.max(0)
    }

    /// Registers a hidden dialog root.
    pub fn create_dialog(&mut self, name: &str, rect: Recti, tree: UiNodeSet) -> RootId {
        self.register_root(WindowKind::Dialog, name, rect, tree, WindowOption::FRAME, false)
    }

    /// Registers a hidden popup root.
    pub fn create_popup(&mut self, name: &str, tree: UiNodeSet) -> RootId {
        self.register_root(WindowKind::Popup, name, Recti::default(), tree, Self::default_popup_options(), false)
    }

    /// Replaces the retained UI node set for a registered root.
    pub fn set_root_nodes(&mut self, root: RootId, tree: UiNodeSet) {
        if let Some(entry) = self.roots.iter_mut().find(|entry| entry.id == root) {
            let previous_roots = std::mem::replace(&mut entry.roots, tree.into_roots());
            entry.z_order = entry.roots.iter().map(UiNode::id).collect();
            let mut next_runtime = UiRuntime::new();
            next_runtime.transfer_runtime_state_from(&mut entry.roots, &previous_roots, &entry.runtime);
            entry.runtime = next_runtime;
            #[cfg(test)]
            {
                self.root_projection_replacements += 1;
            }
        }
    }

    /// Replaces the chrome options for a registered root.
    pub fn set_root_options(&mut self, root: RootId, opt: WindowOption) {
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

    pub(super) fn render_window_manager(&mut self, dimensions: Dimensioni) {
        // Context owns the frame list lifecycle. Recording below appends every visible root in
        // painter order, and Renderer consumes the completed list exactly once at the end.
        for entry in &mut self.roots {
            if entry.visible && entry.opt.intersects(WindowOption::AUTO_SIZE) {
                let size = entry
                    .runtime
                    .measure_auto_size(&entry.roots, self.style.as_ref(), &self.renderer.atlas(), entry.opt, entry.rect.width);
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
        // Per-root focus slots may retain inactive state, but keyboard/text input has one root
        // authority: the front visible root selected by the window manager.
        let keyboard_root = roots
            .iter()
            .filter(|entry| entry.visible)
            .max_by_key(|entry| entry.z_index)
            .map(|entry| entry.id);
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
                let pointer_input_enabled = hover_root == Some(entry.id) && !chrome_capturing_pointer;
                let chrome = WindowChrome::new(entry.rect, self.style.as_ref(), &self.renderer.atlas(), entry.opt);
                self.record_window_frame(entry, dimensions);
                let input = self.input.borrow();
                entry.runtime.begin_frame(pointer_input_enabled);
                entry
                    .runtime
                    .layout_frame_roots(&mut entry.roots, self.style.as_ref(), self.renderer.atlas(), chrome.body);
                Self::route_entry_input(entry, self.style.as_ref(), &input, keyboard_root == Some(entry.id));
                let atlas = self.renderer.atlas();
                entry.runtime.update_paint_frame(
                    &mut entry.roots,
                    entry.id,
                    &entry.name,
                    &mut self.display_list,
                    atlas,
                    self.style.as_ref(),
                    &input,
                    &mut self.frame_results,
                    chrome.body,
                );
                drop(input);
                self.record_window_chrome(entry, chrome, dimensions);
            }
        }
        self.roots = roots;
    }

    fn route_entry_input(entry: &mut WindowEntry, style: &Style, input: &Input, route_focus_input: bool) {
        if entry.runtime.accepts_pointer_input() || entry.runtime.capture.is_some() {
            for event in pointer_events_from_input(input) {
                if entry
                    .runtime
                    .route_captured_pointer_input_event(&mut entry.roots, style, input, &event)
                    .is_some()
                {
                    continue;
                }
                if !entry.runtime.accepts_pointer_input() {
                    continue;
                }

                let mut routed = None;
                let root_transform = entry.runtime.root_transform();
                for root in entry.z_order.iter().copied().rev() {
                    let Some(root) = entry.roots.iter_mut().find(|node| node.id() == root) else {
                        continue;
                    };
                    routed = entry.runtime.route_input_event_to_node_ref(root, root_transform, style, &event);
                    if routed.is_some() {
                        break;
                    }
                }

                if let Some((owner, result)) = routed {
                    entry.runtime.update_pointer_capture(owner, result, &event, input);
                }
            }
        }

        if route_focus_input {
            entry.runtime.route_focus_input_events(&mut entry.roots, style, input);
        }
    }

    /// Records the root background and border before retained contents.
    fn record_window_frame(&mut self, entry: &WindowEntry, dimensions: Dimensioni) {
        let viewport = Recti::new(0, 0, dimensions.width.max(0), dimensions.height.max(0));
        let mut painter = Painter::screen_space(&mut self.display_list, viewport);
        let fill = self.style.colors[ControlColor::WindowBG as usize];
        if entry.opt.intersects(WindowOption::FRAME) {
            crate::frame::paint_internal_frame(&mut painter, entry.rect, Some(fill), self.style.frame_border());
        } else {
            painter.fill_rect(entry.rect, fill);
        }
    }

    /// Records title, close, and resize chrome after retained contents.
    fn record_window_chrome(&mut self, entry: &WindowEntry, chrome: WindowChrome, dimensions: Dimensioni) {
        let viewport = Recti::new(0, 0, dimensions.width.max(0), dimensions.height.max(0));
        let atlas = self.renderer.atlas();
        let mut painter = Painter::screen_space(&mut self.display_list, viewport);

        if let Some(title) = chrome.title {
            record_root_fill(&mut painter, self.style.as_ref(), title, ControlColor::TitleBG);
            let mut text = title;
            if let Some(close) = chrome.close {
                text.width = (close.x.max(title.x) - title.x).max(0);
            }
            record_root_title_text(&mut painter, self.style.as_ref(), &atlas, text, &entry.name);

            if let Some(close) = chrome.close {
                let color = self.style.colors[ControlColor::TitleText as usize];
                painter.icon(crate::CLOSE_ICON, close, color);
            }
        }

        if let Some(resize) = chrome.resize
            && resize.width > 0
            && resize.height > 0
        {
            let client = crate::frame::frame_geometry(entry.rect, entry.opt.intersects(WindowOption::FRAME), self.style.as_ref()).content_or_empty();
            if let Some(visual) = resize.intersect(&client) {
                let fill = self.style.colors[ControlColor::WindowBG as usize];
                crate::frame::paint_internal_frame(&mut painter, visual, Some(fill), self.style.frame_border());
            }
        }
    }

    fn update_window_manager_chrome(
        &mut self,
        hover_root: Option<RootId>,
        mouse_pos: crate::Vec2i,
        mouse_pressed: MouseButton,
        mouse_down: MouseButton,
        mouse_delta: crate::Vec2i,
    ) {
        let atlas = self.renderer.atlas();
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

    const fn default_popup_options() -> WindowOption {
        WindowOption::FRAME
            .union(WindowOption::AUTO_SIZE)
            .union(WindowOption::NO_RESIZE)
            .union(WindowOption::NO_TITLE)
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
            .map(|entry| WindowChrome::new(entry.rect, self.style.as_ref(), &self.renderer.atlas(), entry.opt).body)
    }

    #[cfg(test)]
    pub(crate) fn debug_root_content_size(&self, root: RootId) -> Option<Dimensioni> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .map(|entry| entry.runtime.debug_root_content_size())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_runtime_metrics(&self, root: RootId) -> Option<crate::ui_node::RuntimeMetrics> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| entry.runtime.debug_metrics())
    }

    #[cfg(test)]
    pub(crate) fn debug_root_structure(&self, root: RootId) -> Option<(usize, usize)> {
        let entry = self.roots.iter().find(|entry| entry.id == root)?;
        Some((
            entry.roots.iter().map(UiNode::debug_node_count).sum(),
            entry.roots.iter().map(UiNode::debug_erased_adapter_count).sum(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::NodeId) -> Option<Recti> {
        self.roots
            .iter()
            .find(|entry| entry.id == root)
            .and_then(|entry| entry.runtime.debug_node_rect(&entry.roots, node))
    }

    #[cfg(test)]
    pub(crate) fn debug_root_chrome(&self, root: RootId) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        self.roots.iter().find(|entry| entry.id == root).map(|entry| {
            let chrome = WindowChrome::new(entry.rect, self.style.as_ref(), &self.renderer.atlas(), entry.opt);
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

fn root_min_size(style: &Style, atlas: &crate::AtlasHandle, opt: WindowOption, title: &str) -> Dimensioni {
    let auto_size = opt.intersects(WindowOption::AUTO_SIZE);
    let mut width: i32 = if auto_size { 1 } else { 96 };
    let mut height: i32 = if auto_size { 1 } else { 64 };
    if !opt.intersects(WindowOption::NO_TITLE) {
        let title_height = root_titlebar_height(style, atlas);
        let title_width = atlas.get_text_size(style.title_font, title).width;
        let close_width = if opt.intersects(WindowOption::NO_CLOSE) { 0 } else { title_height };
        let padding = style.padding.max(0);
        width = width.max(title_width.saturating_add(close_width).saturating_add(padding.saturating_mul(2)));
        let title_min_height = if auto_size {
            title_height
        } else {
            title_height.saturating_add(padding.saturating_mul(2))
        };
        height = height.max(title_min_height);
    }
    let border = if opt.intersects(WindowOption::FRAME) {
        style.frame_border().width.checked_mul(2).expect("root frame extent overflowed i32")
    } else {
        0
    };
    Dimensioni::new(
        width.checked_add(border).expect("root minimum width overflowed i32"),
        height.checked_add(border).expect("root minimum height overflowed i32"),
    )
}

/// Records a flat window-chrome fill through the retained screen-space painter.
fn record_root_fill(painter: &mut Painter<'_>, style: &Style, rect: Recti, color: ControlColor) {
    painter.fill_rect(rect, style.colors[color as usize]);
}

/// Records title text with an operation-local clip.
fn record_root_title_text(painter: &mut Painter<'_>, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, title: &str) {
    if rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let color = style.colors[ControlColor::TitleText as usize];
    let pos = crate::text_layout::control_text_position_with_font(style, atlas, style.title_font, title, rect, crate::WidgetOption::NONE);
    painter.with_clip(rect, |painter| painter.text(style.title_font, title, pos, color));
}
