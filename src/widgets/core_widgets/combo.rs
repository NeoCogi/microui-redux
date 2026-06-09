//! Combo-box retained state.
//!
//! The combo widget tracks selected item text and popup-open state; the context root layer owns the
//! actual popup traversal.

use super::*;

/// Persistent state used by `combo_box` to track popup and selection.
#[derive(Clone)]
pub struct Combo {
    /// Currently selected item index.
    selected: usize,
    /// Whether the combo popup should be open.
    open: bool,
    /// Shared widget configuration.
    pub config: WidgetConfig,
    /// Label text for the currently selected item.
    label: String,
    /// Whether the selected index has been clamped for the current item count.
    clamped: bool,
    /// Last button rectangle used to place the popup.
    last_anchor: Recti,
}

impl Combo {
    /// Creates a new combo state.
    pub fn new() -> Self {
        Self {
            selected: 0,
            open: false,
            config: WidgetConfig::default(),
            label: String::new(),
            clamped: false,
            last_anchor: Recti::default(),
        }
    }

    /// Creates a new combo state with explicit widget options.
    pub fn with_opt(opt: WidgetOption, scroll_behavior: ScrollBehavior) -> Self {
        Self {
            selected: 0,
            open: false,
            config: WidgetConfig::new(opt, scroll_behavior),
            label: String::new(),
            clamped: false,
            last_anchor: Recti::default(),
        }
    }

    /// Returns the popup anchor computed during the latest combo draw.
    pub fn anchor(&self) -> Recti {
        self.last_anchor
    }

    /// Returns the currently selected item index.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns `true` when the combo popup should be open this frame.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the popup and marks the combo open.
    pub fn open_popup(&mut self) {
        self.open = true;
    }

    /// Closes the popup.
    pub fn close_popup(&mut self) {
        self.open = false;
    }

    /// Measures the combo header label plus dropdown indicator.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let text_w = if self.label.is_empty() {
            0
        } else {
            text_size(style, atlas, self.config.font, self.label.as_str()).width
        };
        let indicator = atlas.get_icon_size(EXPAND_DOWN_ICON);
        let width = (padding * 3 + text_w + indicator.width).max(0);
        let height = content_height(style, atlas, self.config.font, indicator.height);
        Dimensioni::new(width, height)
    }

    /// Updates the cached label and clamps the selected index to the provided items.
    pub fn update_items<S: AsRef<str>>(&mut self, items: &[S]) {
        self.clamped = false;
        if items.is_empty() {
            if self.selected != 0 {
                self.selected = 0;
                self.clamped = true;
            }
            self.label.clear();
            return;
        }

        if self.selected >= items.len() {
            // Clamp stale selections after the backing item list changes.
            self.selected = items.len() - 1;
            self.clamped = true;
        }

        self.label.clear();
        if let Some(label) = items.get(self.selected) {
            self.label.push_str(label.as_ref());
        }
    }

    /// Applies a submitted popup item selection and closes the popup.
    pub fn select<S: AsRef<str>>(&mut self, index: usize, items: &[S]) -> Option<String> {
        if items.is_empty() {
            self.selected = 0;
            self.label.clear();
            self.close_popup();
            return None;
        }

        self.selected = index.min(items.len() - 1);
        self.label.clear();
        self.label.push_str(items[self.selected].as_ref());
        let selected_label = self.label.clone();
        self.close_popup();
        Some(selected_label)
    }

    /// Updates popup open state and reports submit/active transitions.
    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>, _input: &[UiInputEvent]) -> ResourceState {
        let mut res = ResourceState::NONE;
        if self.clamped {
            res |= ResourceState::CHANGE;
            self.clamped = false;
        }

        if ctx.clicked() {
            // Clicking the header toggles the popup; closing clears popup-local focus.
            self.open = !self.open;
            if !self.open {
                self.close_popup();
            }
        }
        if ctx.clicked() {
            res |= ResourceState::SUBMIT | ResourceState::ACTIVE;
        }
        if self.open {
            res |= ResourceState::ACTIVE;
        }
        res
    }

    /// Paints the combo header and records the popup anchor below it.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>) {
        let header = ctx.screen_rect();
        self.last_anchor = rect(header.x, header.y + header.height, header.width, 1);
        ctx.draw_widget_frame(header, ControlColor::Button, self.config.opt);

        let indicator_size = ctx.atlas().get_icon_size(EXPAND_DOWN_ICON);
        let indicator_x = header.x + header.width - indicator_size.width;
        let indicator_y = header.y + ((header.height - indicator_size.height) / 2).max(0);
        let indicator = rect(indicator_x, indicator_y, indicator_size.width, indicator_size.height);

        let mut text_rect = header;
        let reserved_width = indicator_size.width;
        text_rect.width = (text_rect.width - reserved_width).max(0);
        let font = ctx.style().resolve_font_choice(self.config.font);
        ctx.draw_control_text_with_font(font, self.label.as_str(), text_rect, ControlColor::Text, self.config.opt);

        ctx.draw_widget_frame(indicator, ControlColor::Button, self.config.opt);
        let icon_color = ctx.style().colors[ControlColor::Text as usize];
        ctx.draw_icon(EXPAND_DOWN_ICON, indicator, icon_color);
    }
}

impl Widget for Combo {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.config.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, input: Vec<UiInputEvent>) -> ResourceState {
        self.update_widget(ctx, &input)
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>) {
        self.paint_widget(ctx);
    }
}
