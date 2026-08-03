//! Combo-box retained state.
//!
//! The combo widget tracks selected item text and popup-open state; the context root layer owns the
//! actual popup traversal.

use super::*;
use crate::ui_node::{runtime_read_state, runtime_update_state};
use crate::widgets::{record_pending_event, take_pending_event};
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`Combo`].
pub struct ComboParameters {
    /// Font used for the current label.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for ComboParameters {}

impl ComboParameters {
    /// Creates combo parameters with default widget options.
    pub const fn new() -> Self {
        Self {
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates combo parameters with explicit widget options.
    pub const fn with_opt(opt: WidgetOption) -> Self {
        Self {
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used for the current label.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

impl Default for ComboParameters {
    fn default() -> Self {
        Self::new()
    }
}

/// Application-facing persistent combo state.
pub struct ComboState {
    /// Currently selected item index.
    selected: usize,
    /// Whether the combo popup should be open.
    open: bool,
    /// Label text for the currently selected item.
    label: String,
    /// Last button rectangle used to place the popup.
    last_anchor: Recti,
    /// User-visible selection changes waiting to be consumed.
    pending_changes: u32,
    /// User header submissions waiting to be consumed.
    pending_submissions: u32,
}

impl WidgetState for ComboState {}

impl ComboState {
    /// Returns the popup anchor computed during the latest combo draw.
    pub fn anchor(&self) -> Recti {
        self.last_anchor
    }

    /// Returns the currently selected item index.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns the current selected-item label.
    pub fn label(&self) -> &str {
        &self.label
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

    /// Updates the cached label and clamps the selected index to the provided items.
    pub fn update_items<S: AsRef<str>>(&mut self, items: &[S]) {
        if items.is_empty() {
            if self.selected != 0 {
                self.selected = 0;
                record_pending_event(&mut self.pending_changes);
            }
            self.label.clear();
            return;
        }

        if self.selected >= items.len() {
            // Clamp stale selections after the backing item list changes.
            self.selected = items.len() - 1;
            record_pending_event(&mut self.pending_changes);
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

    /// Consumes one pending user-visible selection change.
    pub fn take_changed(&mut self) -> bool {
        take_pending_event(&mut self.pending_changes)
    }

    /// Consumes one pending header submission.
    pub fn take_submitted(&mut self) -> bool {
        take_pending_event(&mut self.pending_submissions)
    }
}

/// Concrete combo runtime and sole strong owner of its application state.
pub struct Combo {
    /// Initialization-only font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Persistent state allocation.
    state: Rc<RefCell<ComboState>>,
}

impl Combo {
    /// Constructs a typed state handle and unique combo runtime.
    pub fn create(parameters: ComboParameters) -> (WidgetStateHandle<ComboState>, Self) {
        let widget = ComboBuilder::create_widget(parameters);
        let state = widget.state_handle();
        (state, widget)
    }

    /// Measures the combo header label plus dropdown indicator.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Combo::measure", |state| {
            let padding = style.padding.max(0);
            let text_w = if state.label.is_empty() {
                0
            } else {
                text_size(style, atlas, self.font, state.label.as_str()).width
            };
            let indicator = atlas.get_icon_size(EXPAND_DOWN_ICON);
            let width = (padding * 3 + text_w + indicator.width).max(0);
            let height = content_height(style, atlas, self.font, indicator.height);
            Dimensioni::new(width, height)
        })
    }

    /// Updates popup open state and records header submissions.
    fn update_widget(&mut self, ctx: &mut WidgetUpdateCtx<'_>) {
        runtime_update_state(&self.state, "Combo::update", |state| {
            if ctx.clicked() {
                state.open = !state.open;
                record_pending_event(&mut state.pending_submissions);
            }
        })
    }

    /// Paints the combo header and records the popup anchor below it.
    fn paint_widget(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let header = ctx.local_rect();
        let screen_header = ctx.screen_content_rect();
        ctx.draw_widget_fill(header, ControlColor::Button);

        let indicator_size = ctx.atlas().get_icon_size(EXPAND_DOWN_ICON);
        let indicator_x = header.x + header.width - indicator_size.width;
        let indicator_y = header.y + ((header.height - indicator_size.height) / 2).max(0);
        let indicator = rect(indicator_x, indicator_y, indicator_size.width, indicator_size.height);

        let mut text_rect = header;
        let reserved_width = indicator_size.width;
        text_rect.width = (text_rect.width - reserved_width).max(0);
        runtime_update_state(&self.state, "Combo::paint", |state| {
            state.last_anchor = rect(screen_header.x, screen_header.y + screen_header.height, screen_header.width, 1);
            let font = ctx.style().resolve_font_choice(self.font);
            ctx.draw_control_text_with_font(font, state.label.as_str(), text_rect, ControlColor::Text, self.opt);
        });

        let indicator_content = ctx.draw_widget_internal_frame(indicator, ControlColor::Button);
        let icon_color = ctx.style().colors[ControlColor::Text as usize];
        if let Some(indicator_content) = indicator_content {
            ctx.draw_icon(EXPAND_DOWN_ICON, indicator_content, icon_color);
        }
    }
}

impl Widget for Combo {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
        self.update_widget(ctx)
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        self.paint_widget(ctx);
    }
}

impl WidgetStateOwner for Combo {
    type State = ComboState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating combo parameters with the concrete runtime.
pub struct ComboBuilder;

impl WidgetBuilder for ComboBuilder {
    type Parameters = ComboParameters;
    type W = Combo;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Combo {
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(ComboState {
                selected: 0,
                open: false,
                label: String::new(),
                last_anchor: Recti::default(),
                pending_changes: 0,
                pending_submissions: 0,
            })),
        }
    }
}
