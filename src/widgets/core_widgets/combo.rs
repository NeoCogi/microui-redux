use super::*;

/// Persistent state used by `combo_box` to track popup and selection.
#[derive(Clone)]
pub struct Combo {
    /// Popup window backing the dropdown list.
    pub popup: WindowHandle,
    /// Currently selected item index.
    pub selected: usize,
    /// Whether the combo popup should be open.
    pub open: bool,
    /// Widget options applied to the combo header.
    pub opt: WidgetOption,
    /// Scroll behavior applied to the combo header.
    pub scroll_behavior: ScrollBehavior,
    /// Font selection used for the combo label.
    pub font: FontChoice,
    label: String,
    clamped: bool,
    last_anchor: Recti,
}

impl Combo {
    /// Creates a new combo state with the provided popup handle.
    pub fn new(popup: WindowHandle) -> Self {
        Self {
            popup,
            selected: 0,
            open: false,
            opt: WidgetOption::NONE,
            scroll_behavior: ScrollBehavior::NONE,
            font: FontChoice::default(),
            label: String::new(),
            clamped: false,
            last_anchor: Recti::default(),
        }
    }

    /// Creates a new combo state with explicit widget options.
    pub fn with_opt(popup: WindowHandle, opt: WidgetOption, scroll_behavior: ScrollBehavior) -> Self {
        Self {
            popup,
            selected: 0,
            open: false,
            opt,
            scroll_behavior,
            font: FontChoice::default(),
            label: String::new(),
            clamped: false,
            last_anchor: Recti::default(),
        }
    }

    /// Returns the popup anchor computed during the latest combo draw.
    pub fn anchor(&self) -> Recti {
        self.last_anchor
    }

    /// Returns `true` when the combo popup should be open this frame.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Closes the popup and clears any popup-local focus state.
    pub fn close_popup(&mut self) {
        self.popup.clear_focus();
        self.popup.close();
        self.open = false;
    }

    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let text_w = if self.label.is_empty() {
            0
        } else {
            text_size(style, atlas, self.font, self.label.as_str()).width
        };
        let indicator = atlas.get_icon_size(EXPAND_DOWN_ICON);
        let width = (padding * 3 + text_w + indicator.width).max(0);
        let height = content_height(style, atlas, self.font, indicator.height);
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

    fn update_widget(&mut self, _ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        if self.clamped {
            res |= ResourceState::CHANGE;
            self.clamped = false;
        }

        if control.clicked {
            self.open = !self.open;
            if !self.open {
                self.close_popup();
            }
        } else if !self.popup.is_open() {
            self.open = false;
        }
        if control.clicked {
            res |= ResourceState::SUBMIT | ResourceState::ACTIVE;
        }
        if self.open || self.popup.is_open() {
            res |= ResourceState::ACTIVE;
        }
        res
    }

    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let header = ctx.rect();
        self.last_anchor = rect(header.x, header.y + header.height, header.width, 1);
        ctx.draw_widget_frame(control, header, ControlColor::Button, self.opt);

        let indicator_size = ctx.atlas().get_icon_size(EXPAND_DOWN_ICON);
        let indicator_x = header.x + header.width - indicator_size.width;
        let indicator_y = header.y + ((header.height - indicator_size.height) / 2).max(0);
        let indicator = rect(indicator_x, indicator_y, indicator_size.width, indicator_size.height);

        let mut text_rect = header;
        let reserved_width = indicator_size.width;
        text_rect.width = (text_rect.width - reserved_width).max(0);
        let font = ctx.style().resolve_font_choice(self.font);
        ctx.draw_control_text_with_font(font, self.label.as_str(), text_rect, ControlColor::Text, self.opt);

        ctx.draw_widget_frame(control, indicator, ControlColor::Button, self.opt);
        let icon_color = ctx.style().colors[ControlColor::Text as usize];
        ctx.draw_icon(EXPAND_DOWN_ICON, indicator, icon_color);
    }
}

impl Widget for Combo {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        self.update_widget(ctx, control)
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        self.paint_widget(ctx, control);
    }
}
