//! Container-level convenience APIs for built-in widgets and custom rendering.

use super::*;

impl Container {
    /// Replaces the container's style.
    pub fn set_style(&mut self, style: Style) {
        self.style = Rc::new(style);
    }

    /// Returns a copy of the current style.
    pub fn get_style(&self) -> Style {
        *self.style
    }

    /// Displays static text using the default text color.
    ///
    /// This helper uses intrinsic text metrics to request a preferred cell size.
    pub fn label(&mut self, text: &str) {
        let (font, padding) = {
            let style = self.style.as_ref();
            (style.font, style.padding.max(0))
        };
        let text_width = if text.is_empty() {
            0
        } else {
            self.atlas.get_text_size(font, text).width.max(0)
        };
        let line_height = self.atlas.get_font_height(font) as i32;
        let vertical_pad = Self::vertical_text_padding(padding);
        let preferred = Dimensioni::new(
            text_width.saturating_add(padding.saturating_mul(2)),
            line_height.saturating_add(vertical_pad.saturating_mul(2)),
        );
        let layout = self.layout.next_with_preferred(preferred);
        self.draw_control_text(text, layout, ControlColor::Text, WidgetOption::NONE);
    }

    widget_layout!(
        #[inline(never)]
        /// Draws a button using the provided persistent state.
        button,
        Button
    );

    widget_layout!(
        /// Renders a list entry that only highlights while hovered or active.
        list_item,
        ListItem
    );

    widget_layout!(
        #[inline(never)]
        /// Shim for list boxes that only fills on hover or click.
        list_box,
        ListBox
    );

    #[inline(never)]
    /// Draws the combo box header, clamps the selected index, and returns the popup anchor.
    /// The caller is responsible for opening the popup and updating `state.selected` from its list.
    pub fn combo_box<S: AsRef<str>>(&mut self, results: &mut FrameResults, state: &mut Combo, items: &[S]) -> (Recti, bool, ResourceState) {
        state.update_items(items);
        let header = self.next_widget_rect(state);
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let res = self.handle_widget_in_rect(results, state, header, None, opt, bopt);
        let header_clicked = res.is_submitted();
        let anchor = rect(header.x, header.y + header.height, header.width, 1);
        (anchor, header_clicked, res)
    }

    widget_layout!(
        #[inline(never)]
        /// Draws a checkbox labeled with `label` and toggles `state` when clicked.
        checkbox,
        Checkbox
    );

    #[inline(never)]
    /// Allocates a widget cell from `Custom` state preferred size and hands rendering control to user code.
    pub fn custom_render_widget<F: FnMut(Dimensioni, &CustomRenderArgs) + 'static>(&mut self, results: &mut FrameResults, state: &mut Custom, f: F) {
        self.widget_custom_render(results, state, f);
    }

    #[inline(never)]
    /// Runs a widget and records a custom render callback with the resulting interaction context.
    pub fn widget_custom_render<W: Widget + ?Sized, F: FnMut(Dimensioni, &CustomRenderArgs) + 'static>(
        &mut self,
        results: &mut FrameResults,
        widget: &mut W,
        f: F,
    ) {
        let rect = self.next_widget_rect(widget);
        let opt = widget.effective_widget_opt();
        let bopt = widget.effective_behaviour_opt();
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let (control, _) = self.run_widget(results, widget, rect, input, opt, bopt);

        let snapshot = self.snapshot_input();
        let input_ref = snapshot.as_ref();
        let mouse_event = self.input_to_mouse_event(&control, input_ref, rect);

        let active = control.focused && self.in_hover_root;
        let key_mods = if active { input_ref.key_mods } else { KeyMode::NONE };
        let key_codes = if active { input_ref.key_codes } else { KeyCode::NONE };
        let text_input = if active { input_ref.text_input.clone() } else { String::new() };
        let cra = CustomRenderArgs {
            content_area: rect,
            view: self.get_clip_rect(),
            mouse_event,
            scroll_delta: control.scroll_delta,
            widget_opt: opt,
            behaviour_opt: bopt,
            key_mods,
            key_codes,
            text_input,
        };
        self.command_list.push(Command::CustomRender(cra, Box::new(f)));
    }

    widget_layout!(
        /// Draws a textbox using the next available layout cell.
        textbox,
        Textbox,
        |this: &mut Self, results: &mut FrameResults, state: &mut Textbox, rect: Recti| {
            let input = Some(this.snapshot_input());
            let opt = state.opt | WidgetOption::HOLD_FOCUS;
            this.handle_widget_in_rect(results, state, rect, input, opt, state.bopt)
        }
    );

    widget_layout!(
        /// Draws a multi-line text area using the next available layout cell.
        textarea,
        TextArea,
        |this: &mut Self, results: &mut FrameResults, state: &mut TextArea, rect: Recti| {
            let input = Some(this.snapshot_input());
            let opt = state.opt | WidgetOption::HOLD_FOCUS;
            this.handle_widget_in_rect(results, state, rect, input, opt, state.bopt)
        }
    );

    widget_layout!(
        #[inline(never)]
        /// Draws a horizontal slider bound to `state`.
        slider,
        Slider,
        |this: &mut Self, results: &mut FrameResults, state: &mut Slider, rect: Recti| {
            let mut opt = state.opt;
            if state.edit.editing {
                opt |= WidgetOption::HOLD_FOCUS;
            }
            let input = Some(this.snapshot_input());
            this.handle_widget_in_rect(results, state, rect, input, opt, state.bopt)
        }
    );

    widget_layout!(
        #[inline(never)]
        /// Draws a numeric input that can be edited via keyboard or by dragging.
        number,
        Number,
        |this: &mut Self, results: &mut FrameResults, state: &mut Number, rect: Recti| {
            let mut opt = state.opt;
            if state.edit.editing {
                opt |= WidgetOption::HOLD_FOCUS;
            }
            let input = Some(this.snapshot_input());
            this.handle_widget_in_rect(results, state, rect, input, opt, state.bopt)
        }
    );
}
