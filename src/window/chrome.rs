//! Retained chrome controls for windows, dialogs, and popups.

use super::*;
use crate::{id::IdNamespace, widget::FrameResults, widget_tree::NodeLayout};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct WindowChromeIds {
    pub(crate) title: Id,
    pub(crate) close: Id,
    pub(crate) resize: Id,
}

impl WindowChromeIds {
    /// Derives chrome ids directly from a raw root seed.
    pub(crate) fn from_root_seed(seed: usize) -> Self {
        Self {
            title: chrome_node_id(seed, ChromePart::Title),
            close: chrome_node_id(seed, ChromePart::Close),
            resize: chrome_node_id(seed, ChromePart::Resize),
        }
    }

    /// Derives chrome ids from a retained root id.
    pub(super) fn from_root(root_id: RootId) -> Self {
        Self::from_root_seed(root_id.raw())
    }
}

#[derive(Copy, Clone)]
/// Chrome id slot used by deterministic id hashing.
enum ChromePart {
    Title,
    Close,
    Resize,
}

/// Derives a stable id for one root chrome control.
fn chrome_node_id(seed: usize, part: ChromePart) -> Id {
    let part = match part {
        ChromePart::Title => 1,
        ChromePart::Close => 2,
        ChromePart::Resize => 3,
    };
    IdNamespace::WINDOW_CHROME.id([seed as u64, part])
}

/// Converts chrome interaction into the resource result reported for the chrome node.
fn chrome_result(control: &ControlState, submit_on_click: bool) -> ResourceState {
    let mut result = ResourceState::NONE;
    if submit_on_click && control.clicked {
        result |= ResourceState::SUBMIT;
    }
    if control.active {
        result |= ResourceState::ACTIVE;
    }
    result
}

#[derive(Copy, Clone)]
/// Type of chrome node being dispatched.
enum WindowChromePart {
    Title,
    Close,
    Resize,
}

#[derive(Copy, Clone)]
/// One concrete chrome control and its allocated rectangle.
struct WindowChromeNode {
    id: Id,
    part: WindowChromePart,
    rect: Recti,
}

impl WindowChromeNode {
    /// Creates a chrome node from id, role, and rect.
    fn new(id: Id, part: WindowChromePart, rect: Recti) -> Self {
        Self { id, part, rect }
    }
}

/// Small retained tree facade for title, close, and resize chrome controls.
pub(super) struct WindowChromeTree {
    pub(super) ids: WindowChromeIds,
    title_state: Internal,
    close_state: Internal,
    resize_state: Internal,
}

impl WindowChromeTree {
    /// Creates chrome dispatch state from stable ids.
    pub(super) fn new(ids: WindowChromeIds) -> Self {
        Self {
            ids,
            title_state: Internal::new("!title"),
            close_state: Internal::new("!close"),
            resize_state: Internal::new("!resize"),
        }
    }

    /// Resolves the titlebar drag node when the window has a title.
    fn title_node(&self, container: &TraversalHost, opt: ContainerOption) -> Option<WindowChromeNode> {
        if opt.intersects(ContainerOption::NO_TITLE) {
            return None;
        }

        let mut rect = container.rect();
        rect.height = Window::titlebar_height(container);
        Some(WindowChromeNode::new(self.ids.title, WindowChromePart::Title, rect))
    }

    /// Resolves the close button node inside the titlebar.
    fn close_node(&self, title_rect: Recti, opt: ContainerOption) -> Option<WindowChromeNode> {
        if opt.intersects(ContainerOption::NO_CLOSE) {
            return None;
        }

        let rect = rect(
            title_rect.x + title_rect.width - title_rect.height,
            title_rect.y,
            title_rect.height,
            title_rect.height,
        );
        Some(WindowChromeNode::new(self.ids.close, WindowChromePart::Close, rect))
    }

    /// Resolves the resize handle node when resizing is enabled.
    fn resize_node(&self, container: &TraversalHost, opt: ContainerOption) -> Option<WindowChromeNode> {
        if opt.intersects(ContainerOption::AUTO_SIZE) || opt.intersects(ContainerOption::NO_RESIZE) {
            return None;
        }

        let size = container.style().scrollbar_size;
        let container_rect = container.rect();
        let rect = rect(
            container_rect.x + container_rect.width - size,
            container_rect.y + container_rect.height - size,
            size,
            size,
        );
        Some(WindowChromeNode::new(self.ids.resize, WindowChromePart::Resize, rect))
    }

    fn resize_min_size() -> Dimensioni {
        Dimensioni::new(96, 64)
    }

    /// Applies in-progress chrome drags before layout and painting use the current root rectangle.
    pub(super) fn apply_active_drag_deltas(&self, container: &mut TraversalHost, opt: ContainerOption) {
        let delta = container.input().borrow().mouse_delta;
        if delta.x == 0 && delta.y == 0 {
            return;
        }

        if !opt.intersects(ContainerOption::NO_TITLE) && container.node_pointer_active(self.ids.title) {
            container.translate_rect(delta);
        }

        if !opt.intersects(ContainerOption::AUTO_SIZE) && !opt.intersects(ContainerOption::NO_RESIZE) && container.node_pointer_active(self.ids.resize) {
            container.resize_rect_by(delta, Self::resize_min_size());
        }
    }

    /// Updates one chrome node, paints it, and records its retained interaction result.
    fn dispatch_node(
        container: &mut TraversalHost,
        results: &mut FrameResults,
        node: WindowChromeNode,
        state: &mut Internal,
        dispatch_site: &'static str,
    ) -> ControlState {
        container.record_tree_layout(
            node.id,
            NodeLayout::new(node.rect, node.rect, Dimensioni::new(node.rect.width, node.rect.height)),
        );
        let (control, widget_result) = container.update_internal_node_unblocked(node.id, state, node.rect);
        container.paint_internal_node(node.id, state, node.rect, &control);
        let submit_on_click = matches!(node.part, WindowChromePart::Close);
        let result = widget_result | chrome_result(&control, submit_on_click);
        container.record_tree_control(node.id, control);
        // Chrome controls publish through the same result channel as retained tree widgets.
        results.record_node_with_context(container.retained_id_for_node(node.id), node.id, result, dispatch_site);
        control
    }

    /// Updates and paints titlebar chrome, applying drag/close side effects.
    pub(super) fn render_title_bar(&mut self, container: &mut TraversalHost, results: &mut FrameResults, win_state: &mut WindowState, opt: ContainerOption) {
        let Some(title_node) = self.title_node(container, opt) else {
            return;
        };

        let title_text_color = container.style().colors[ControlColor::TitleText as usize];
        container.draw_frame(title_node.rect, ControlColor::TitleBG);

        Self::dispatch_node(container, results, title_node, &mut self.title_state, "window chrome title");
        let name = container.name().to_string();
        let title_font = container.style().title_font;
        container.draw_control_text_with_font(title_font, &name, title_node.rect, ControlColor::TitleText, WidgetOption::NONE);

        let Some(close_node) = self.close_node(title_node.rect, opt) else {
            return;
        };

        container.draw_icon(CLOSE_ICON, close_node.rect, title_text_color);
        let close_control = Self::dispatch_node(container, results, close_node, &mut self.close_state, "window chrome close");
        if close_control.clicked {
            // Close is expressed as window state so the context can reset after traversal ends.
            *win_state = WindowState::Closed;
        }
    }

    /// Updates and paints the resize handle.
    pub(super) fn render_resize_handle(&mut self, container: &mut TraversalHost, results: &mut FrameResults, opt: ContainerOption) {
        let Some(resize_node) = self.resize_node(container, opt) else {
            return;
        };

        Self::dispatch_node(container, results, resize_node, &mut self.resize_state, "window chrome resize");
        container.draw_frame(resize_node.rect, ControlColor::WindowBG);
    }
}
