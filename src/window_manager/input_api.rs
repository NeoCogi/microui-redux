//! Public input forwarding APIs for [`Context`].

use super::*;

impl<B: RendererBackend> Context<B> {
    /// Queues one mouse-pointer position transition without coalescing.
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.input.mousemove(x, y);
        self.invalidate_ui_commit();
    }

    /// Queues one mouse-button press at the supplied pointer position.
    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input.mousedown(x, y, btn);
        self.invalidate_ui_commit();
    }

    /// Queues one mouse-button release at the supplied pointer position.
    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.input.mouseup(x, y, btn);
        self.invalidate_ui_commit();
    }

    /// Queues one scroll-wheel or trackpad delta without accumulating adjacent calls.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.input.scroll(x, y);
        self.invalidate_ui_commit();
    }

    /// Queues one modifier/control-key press.
    pub fn keydown(&mut self, key: KeyMode) {
        self.input.keydown(key);
        self.invalidate_ui_commit();
    }

    /// Queues one modifier/control-key release.
    pub fn keyup(&mut self, key: KeyMode) {
        self.input.keyup(key);
        self.invalidate_ui_commit();
    }

    /// Queues one navigation-key press.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.input.keydown_code(code);
        self.invalidate_ui_commit();
    }

    /// Queues one navigation-key release.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.input.keyup_code(code);
        self.invalidate_ui_commit();
    }

    /// Queues one UTF-8 text transition, including an empty string.
    pub fn text(&mut self, text: &str) {
        self.input.text(text);
        self.invalidate_ui_commit();
    }
}
