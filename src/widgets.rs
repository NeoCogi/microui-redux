//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
use super::*;
use crate::draw_context::DrawCtx;
use std::rc::Rc;

/// Shared context passed to widget handlers.
pub struct WidgetCtx<'a> {
    id: Id,
    rect: Recti,
    draw: DrawCtx<'a>,
    focus: &'a mut Option<Id>,
    updated_focus: &'a mut bool,
    in_hover_root: bool,
    input: Option<InputSnapshot>,
}

impl<'a> WidgetCtx<'a> {
    /// Creates a widget context for the given widget ID and rectangle.
    pub(crate) fn new(
        id: Id,
        rect: Recti,
        commands: &'a mut Vec<Command>,
        clip_stack: &'a mut Vec<Recti>,
        style: &'a Style,
        atlas: &'a AtlasHandle,
        focus: &'a mut Option<Id>,
        updated_focus: &'a mut bool,
        in_hover_root: bool,
        input: Option<InputSnapshot>,
    ) -> Self {
        Self {
            id,
            rect,
            draw: DrawCtx::new(commands, clip_stack, style, atlas),
            focus,
            updated_focus,
            in_hover_root,
            input,
        }
    }

    /// Returns the widget identifier.
    pub fn id(&self) -> Id { self.id }

    /// Returns the widget rectangle.
    pub fn rect(&self) -> Recti { self.rect }

    /// Returns the input snapshot for this widget, if provided.
    pub fn input(&self) -> Option<&InputSnapshot> { self.input.as_ref() }

    /// Sets focus to this widget for the current frame.
    pub fn set_focus(&mut self) {
        *self.focus = Some(self.id);
        *self.updated_focus = true;
    }

    /// Clears focus from the current widget.
    pub fn clear_focus(&mut self) {
        *self.focus = None;
        *self.updated_focus = true;
    }

    /// Pushes a new clip rectangle onto the stack.
    pub fn push_clip_rect(&mut self, rect: Recti) { self.draw.push_clip_rect(rect); }

    /// Pops the current clip rectangle.
    pub fn pop_clip_rect(&mut self) { self.draw.pop_clip_rect(); }

    /// Executes `f` with the provided clip rect applied.
    pub fn with_clip<F: FnOnce(&mut Self)>(&mut self, rect: Recti, f: F) {
        self.push_clip_rect(rect);
        f(self);
        self.pop_clip_rect();
    }

    fn current_clip_rect(&self) -> Recti { self.draw.current_clip_rect() }

    pub(crate) fn style(&self) -> &Style { self.draw.style() }

    pub(crate) fn atlas(&self) -> &AtlasHandle { self.draw.atlas() }

    /// Pushes a raw draw command into the command buffer.
    pub fn push_command(&mut self, cmd: Command) { self.draw.push_command(cmd); }

    /// Sets the current clip rectangle for subsequent draw commands.
    pub fn set_clip(&mut self, rect: Recti) { self.draw.set_clip(rect); }

    /// Returns the clipping relation between `r` and the current clip rect.
    pub fn check_clip(&self, r: Recti) -> Clip { self.draw.check_clip(r) }

    pub(crate) fn draw_rect(&mut self, rect: Recti, color: Color) { self.draw.draw_rect(rect, color); }

    /// Draws a 1-pixel box outline using the supplied color.
    pub fn draw_box(&mut self, r: Recti, color: Color) { self.draw.draw_box(r, color); }

    pub(crate) fn draw_text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        self.draw.draw_text(font, text, pos, color);
    }

    pub(crate) fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) { self.draw.draw_icon(id, rect, color); }

    pub(crate) fn push_image(&mut self, image: Image, rect: Recti, color: Color) { self.draw.push_image(image, rect, color); }

    pub(crate) fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        self.draw.draw_slot_with_function(id, rect, color, f);
    }

    pub(crate) fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) { self.draw.draw_frame(rect, colorid); }

    pub(crate) fn draw_widget_frame(&mut self, control: &ControlState, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.draw.draw_widget_frame(control.focused, control.hovered, rect, colorid, opt);
    }

    pub(crate) fn draw_control_text(&mut self, text: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        self.draw.draw_control_text(text, rect, colorid, opt);
    }

    pub(crate) fn mouse_over(&self, rect: Recti) -> bool {
        let input = match self.input.as_ref() {
            Some(input) => input,
            None => return false,
        };
        if !self.in_hover_root {
            return false;
        }
        let clip_rect = self.current_clip_rect();
        rect.contains(&input.mouse_pos) && clip_rect.contains(&input.mouse_pos)
    }
}

#[derive(Clone, Copy)]
/// Expansion state used by tree nodes, headers, and similar widgets.
pub enum NodeStateValue {
    /// Child content is visible.
    Expanded,
    /// Child content is hidden.
    Closed,
}

impl NodeStateValue {
    /// Returns `true` when the node is expanded.
    pub fn is_expanded(&self) -> bool {
        match self {
            Self::Expanded => true,
            _ => false,
        }
    }

    /// Returns `true` when the node is closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Closed => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
/// Persistent state for headers and tree nodes.
pub struct Node {
    /// Label displayed for the node.
    pub label: String,
    /// Current expansion state.
    pub state: NodeStateValue,
    /// Widget options applied to the node.
    pub opt: WidgetOption,
    /// Behaviour options applied to the node.
    pub bopt: WidgetBehaviourOption,
}

impl Node {
    /// Creates a node state with the default widget options.
    pub fn new(label: impl Into<String>, state: NodeStateValue) -> Self {
        Self { label: label.into(), state, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a node state with explicit widget options.
    pub fn with_opt(label: impl Into<String>, state: NodeStateValue, opt: WidgetOption) -> Self {
        Self { label: label.into(), state, opt, bopt: WidgetBehaviourOption::NONE }
    }

    /// Returns `true` when the node is expanded.
    pub fn is_expanded(&self) -> bool { self.state.is_expanded() }

    /// Returns `true` when the node is closed.
    pub fn is_closed(&self) -> bool { self.state.is_closed() }
}

impl Widget for Node {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        if control.clicked {
            self.state = if self.state.is_expanded() { NodeStateValue::Closed } else { NodeStateValue::Expanded };
            ResourceState::CHANGE
        } else {
            ResourceState::NONE
        }
    }
}

fn widget_fill_color(control: &ControlState, base: ControlColor, fill: WidgetFillOption) -> Option<ControlColor> {
    if control.focused && fill.fill_click() {
        let mut color = base;
        color.focus();
        Some(color)
    } else if control.hovered && fill.fill_hover() {
        let mut color = base;
        color.hover();
        Some(color)
    } else if fill.fill_normal() {
        Some(base)
    } else {
        None
    }
}

#[derive(Clone)]
/// Describes the content rendered inside a button widget.
pub enum ButtonContent {
    /// A text label and optional icon from the atlas.
    Text {
        /// Text displayed on the button.
        label: String,
        /// Optional icon rendered on the button.
        icon: Option<IconId>,
    },
    /// A text label and optional image.
    Image {
        /// Text displayed on the button.
        label: String,
        /// Optional image rendered on the button.
        image: Option<Image>,
    },
    /// A text label and a slot refreshed via a paint callback.
    Slot {
        /// Text displayed on the button.
        label: String,
        /// Slot rendered on the button.
        slot: SlotId,
        /// Callback used to fill the slot pixels.
        paint: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
}

#[derive(Clone)]
/// Persistent state for button widgets.
pub struct Button {
    /// Content rendered inside the button.
    pub content: ButtonContent,
    /// Widget options applied to the button.
    pub opt: WidgetOption,
    /// Behaviour options applied to the button.
    pub bopt: WidgetBehaviourOption,
    /// Fill behavior for the button background.
    pub fill: WidgetFillOption,
}

impl Button {
    /// Creates a text button with default options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates a text button with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            opt,
            bopt: WidgetBehaviourOption::NONE,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates an image button with explicit widget options and fill behavior.
    pub fn with_image(label: impl Into<String>, image: Option<Image>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Image { label: label.into(), image },
            opt,
            bopt: WidgetBehaviourOption::NONE,
            fill,
        }
    }

    /// Creates a slot button that repaints via the provided callback.
    pub fn with_slot(
        label: impl Into<String>,
        slot: SlotId,
        paint: Rc<dyn Fn(usize, usize) -> Color4b>,
        opt: WidgetOption,
        fill: WidgetFillOption,
    ) -> Self {
        Self {
            content: ButtonContent::Slot { label: label.into(), slot, paint },
            opt,
            bopt: WidgetBehaviourOption::NONE,
            fill,
        }
    }
}

impl Widget for Button {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        if control.clicked {
            res |= ResourceState::SUBMIT;
        }
        let rect = ctx.rect();
        if !self.opt.has_no_frame() {
            if let Some(colorid) = widget_fill_color(control, ControlColor::Button, self.fill) {
                ctx.draw_frame(rect, colorid);
            }
        }
        match &self.content {
            ButtonContent::Text { label, icon } => {
                if !label.is_empty() {
                    ctx.draw_control_text(label, rect, ControlColor::Text, self.opt);
                }
                if let Some(icon) = icon {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.draw_icon(*icon, rect, color);
                }
            }
            ButtonContent::Image { label, image } => {
                if !label.is_empty() {
                    ctx.draw_control_text(label, rect, ControlColor::Text, self.opt);
                }
                if let Some(image) = *image {
                    let color = ctx.style().colors[ControlColor::Text as usize];
                    ctx.push_image(image, rect, color);
                }
            }
            ButtonContent::Slot { label, slot, paint } => {
                if !label.is_empty() {
                    ctx.draw_control_text(label, rect, ControlColor::Text, self.opt);
                }
                let color = ctx.style().colors[ControlColor::Text as usize];
                ctx.draw_slot_with_function(*slot, rect, color, paint.clone());
            }
        }
        res
    }
}

#[derive(Clone)]
/// Persistent state for list items.
pub struct ListItem {
    /// Label displayed for the list item.
    pub label: String,
    /// Optional atlas icon rendered alongside the label.
    pub icon: Option<IconId>,
    /// Widget options applied to the list item.
    pub opt: WidgetOption,
    /// Behaviour options applied to the list item.
    pub bopt: WidgetBehaviourOption,
}

impl ListItem {
    /// Creates a list item with default widget options.
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), icon: None, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list item with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self { label: label.into(), icon: None, opt, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list item with an icon and default widget options.
    pub fn with_icon(label: impl Into<String>, icon: IconId) -> Self {
        Self { label: label.into(), icon: Some(icon), opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list item with an icon and explicit widget options.
    pub fn with_icon_opt(label: impl Into<String>, icon: IconId, opt: WidgetOption) -> Self {
        Self { label: label.into(), icon: Some(icon), opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl Widget for ListItem {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let bounds = ctx.rect();
        if control.clicked {
            res |= ResourceState::SUBMIT;
        }

        if control.focused || control.hovered {
            let mut color = ControlColor::Button;
            if control.focused {
                color.focus();
            } else {
                color.hover();
            }
            let fill = ctx.style().colors[color as usize];
            ctx.draw_rect(bounds, fill);
        }

        let mut text_rect = bounds;
        if let Some(icon) = self.icon {
            let padding = ctx.style().padding.max(0);
            let icon_size = ctx.atlas().get_icon_size(icon);
            let icon_x = bounds.x + padding;
            let icon_y = bounds.y + ((bounds.height - icon_size.height) / 2).max(0);
            let icon_rect = rect(icon_x, icon_y, icon_size.width, icon_size.height);
            let consumed = icon_size.width + padding * 2;
            text_rect.x += consumed;
            text_rect.width = (text_rect.width - consumed).max(0);
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.draw_icon(icon, icon_rect, color);
        }

        if !self.label.is_empty() {
            ctx.draw_control_text(&self.label, text_rect, ControlColor::Text, self.opt);
        }
        res
    }
}

#[derive(Clone)]
/// Persistent state for list boxes.
pub struct ListBox {
    /// Label displayed for the list box.
    pub label: String,
    /// Optional image rendered alongside the label.
    pub image: Option<Image>,
    /// Widget options applied to the list box.
    pub opt: WidgetOption,
    /// Behaviour options applied to the list box.
    pub bopt: WidgetBehaviourOption,
}

impl ListBox {
    /// Creates a list box with default widget options.
    pub fn new(label: impl Into<String>, image: Option<Image>) -> Self {
        Self { label: label.into(), image, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list box with explicit widget options.
    pub fn with_opt(label: impl Into<String>, image: Option<Image>, opt: WidgetOption) -> Self {
        Self { label: label.into(), image, opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl Widget for ListBox {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let rect = ctx.rect();
        if control.clicked {
            res |= ResourceState::SUBMIT;
        }
        if !self.opt.has_no_frame() {
            if let Some(colorid) = widget_fill_color(control, ControlColor::Button, WidgetFillOption::HOVER | WidgetFillOption::CLICK) {
                ctx.draw_frame(rect, colorid);
            }
        }
        if !self.label.is_empty() {
            ctx.draw_control_text(&self.label, rect, ControlColor::Text, self.opt);
        }
        if let Some(image) = self.image {
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.push_image(image, rect, color);
        }
        res
    }
}

#[derive(Clone)]
/// Persistent state for checkbox widgets.
pub struct Checkbox {
    /// Label displayed for the checkbox.
    pub label: String,
    /// Current value of the checkbox.
    pub value: bool,
    /// Widget options applied to the checkbox.
    pub opt: WidgetOption,
    /// Behaviour options applied to the checkbox.
    pub bopt: WidgetBehaviourOption,
}

impl Checkbox {
    /// Creates a checkbox with default widget options.
    pub fn new(label: impl Into<String>, value: bool) -> Self {
        Self { label: label.into(), value, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a checkbox with explicit widget options.
    pub fn with_opt(label: impl Into<String>, value: bool, opt: WidgetOption) -> Self {
        Self { label: label.into(), value, opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl Widget for Checkbox {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let bounds = ctx.rect();
        let box_rect = rect(bounds.x, bounds.y, bounds.height, bounds.height);
        if control.clicked {
            res |= ResourceState::CHANGE;
            self.value = !self.value;
        }
        ctx.draw_widget_frame(control, box_rect, ControlColor::Base, self.opt);
        if self.value {
            let color = ctx.style().colors[ControlColor::Text as usize];
            ctx.draw_icon(CHECK_ICON, box_rect, color);
        }
        let text_rect = rect(bounds.x + box_rect.width, bounds.y, bounds.width - box_rect.width, bounds.height);
        if !self.label.is_empty() {
            ctx.draw_control_text(&self.label, text_rect, ControlColor::Text, self.opt);
        }
        res
    }
}

#[derive(Clone)]
/// Persistent state for textbox widgets.
pub struct Textbox {
    /// Buffer edited by the textbox.
    pub buf: String,
    /// Current cursor position within the buffer (byte index).
    pub cursor: usize,
    /// Widget options applied to the textbox.
    pub opt: WidgetOption,
    /// Behaviour options applied to the textbox.
    pub bopt: WidgetBehaviourOption,
}

impl Textbox {
    /// Creates a textbox with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self { buf, cursor, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a textbox with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self { buf, cursor, opt, bopt: WidgetBehaviourOption::NONE }
    }
}

fn textbox_handle(
    ctx: &mut WidgetCtx<'_>,
    control: &ControlState,
    buf: &mut String,
    cursor: &mut usize,
    opt: WidgetOption,
) -> ResourceState {
    let mut res = ResourceState::NONE;
    let r = ctx.rect();
    if !control.focused {
        *cursor = buf.len();
    }
    let mut cursor_pos = (*cursor).min(buf.len());

    let input = ctx.input().cloned().unwrap_or_default();

    if control.focused {
        if !input.text_input.is_empty() {
            let insert_at = cursor_pos.min(buf.len());
            buf.insert_str(insert_at, input.text_input.as_str());
            cursor_pos = insert_at + input.text_input.len();
            res |= ResourceState::CHANGE;
        }

        if input.key_pressed.is_backspace() && cursor_pos > 0 && !buf.is_empty() {
            let mut new_cursor = cursor_pos.min(buf.len());
            new_cursor -= 1;
            while new_cursor > 0 && !buf.is_char_boundary(new_cursor) {
                new_cursor -= 1;
            }
            buf.replace_range(new_cursor..cursor_pos, "");
            cursor_pos = new_cursor;
            res |= ResourceState::CHANGE;
        }

        if input.key_code_pressed.is_left() && cursor_pos > 0 {
            let mut new_cursor = cursor_pos - 1;
            while new_cursor > 0 && !buf.is_char_boundary(new_cursor) {
                new_cursor -= 1;
            }
            cursor_pos = new_cursor;
        }

        if input.key_code_pressed.is_right() && cursor_pos < buf.len() {
            let mut new_cursor = cursor_pos + 1;
            while new_cursor < buf.len() && !buf.is_char_boundary(new_cursor) {
                new_cursor += 1;
            }
            cursor_pos = new_cursor;
        }

        if input.key_pressed.is_return() {
            ctx.clear_focus();
            res |= ResourceState::SUBMIT;
        }
    }

    ctx.draw_widget_frame(control, r, ControlColor::Base, opt);

    let font = ctx.style().font;
    let line_height = ctx.atlas().get_font_height(font) as i32;
    let baseline = ctx.atlas().get_font_baseline(font);
    let descent = (line_height - baseline).max(0);

    let mut texty = r.y + r.height / 2 - line_height / 2;
    if texty < r.y {
        texty = r.y;
    }
    let max_texty = (r.y + r.height - line_height).max(r.y);
    if texty > max_texty {
        texty = max_texty;
    }
    let baseline_y = texty + line_height - descent;

    let text_metrics = ctx.atlas().get_text_size(font, buf.as_str());
    let padding = ctx.style().padding;
    let ofx = r.width - padding - text_metrics.width - 1;
    let textx = r.x + if ofx < padding { ofx } else { padding };

    if control.focused && input.mouse_pressed.is_left() && ctx.mouse_over(r) {
        let click_x = input.mouse_pos.x - textx;
        if click_x <= 0 {
            cursor_pos = 0;
        } else {
            let mut last_width = 0;
            let mut new_cursor = buf.len();
            for (idx, ch) in buf.char_indices() {
                let next = idx + ch.len_utf8();
                let width = ctx.atlas().get_text_size(font, &buf[..next]).width;
                if click_x < width {
                    if click_x < (last_width + width) / 2 {
                        new_cursor = idx;
                    } else {
                        new_cursor = next;
                    }
                    break;
                }
                last_width = width;
            }
            cursor_pos = new_cursor.min(buf.len());
        }
    }

    cursor_pos = cursor_pos.min(buf.len());
    *cursor = cursor_pos;

    let caret_offset = if cursor_pos == 0 {
        0
    } else {
        ctx.atlas().get_text_size(font, &buf[..cursor_pos]).width
    };

    if control.focused {
        let color = ctx.style().colors[ControlColor::Text as usize];
        ctx.push_clip_rect(r);
        ctx.draw_text(font, buf.as_str(), vec2(textx, texty), color);
        let caret_top = (baseline_y - baseline + 2).max(r.y).min(r.y + r.height);
        let caret_bottom = (baseline_y + descent - 2).max(r.y).min(r.y + r.height);
        let caret_height = (caret_bottom - caret_top).max(1);
        ctx.draw_rect(rect(textx + caret_offset, caret_top, 1, caret_height), color);
        ctx.pop_clip_rect();
    } else {
        ctx.draw_control_text(buf.as_str(), r, ControlColor::Text, opt);
    }
    res
}

impl Widget for Textbox {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        textbox_handle(ctx, control, &mut self.buf, &mut self.cursor, self.opt)
    }
}

#[derive(Clone)]
/// Persistent state for slider widgets.
pub struct Slider {
    /// Current slider value.
    pub value: Real,
    /// Lower bound of the slider range.
    pub low: Real,
    /// Upper bound of the slider range.
    pub high: Real,
    /// Step size used for snapping (0 for continuous).
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Widget options applied to the slider.
    pub opt: WidgetOption,
    /// Behaviour options applied to the slider.
    pub bopt: WidgetBehaviourOption,
    /// Text editing state for shift-click numeric entry.
    pub edit: NumberEditState,
}

impl Slider {
    /// Creates a slider with default widget options.
    pub fn new(value: Real, low: Real, high: Real) -> Self {
        Self {
            value,
            low,
            high,
            step: 0.0,
            precision: 0,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
            edit: NumberEditState::default(),
        }
    }

    /// Creates a slider with explicit widget options.
    pub fn with_opt(value: Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            low,
            high,
            step,
            precision,
            opt,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
            edit: NumberEditState::default(),
        }
    }
}

fn number_textbox_handle(
    ctx: &mut WidgetCtx<'_>,
    control: &ControlState,
    edit: &mut NumberEditState,
    precision: usize,
    value: &mut Real,
) -> ResourceState {
    let input = ctx.input().cloned().unwrap_or_default();

    if input.mouse_pressed.is_left() && input.key_mods.is_shift() && control.hovered {
        edit.editing = true;
        edit.buf.clear();
        edit.buf.push_str(format!("{:.*}", precision, value).as_str());
        edit.cursor = edit.buf.len();
    }

    if edit.editing {
        let res = textbox_handle(ctx, control, &mut edit.buf, &mut edit.cursor, WidgetOption::NONE);
        if res.is_submitted() || !control.focused {
            if let Ok(v) = edit.buf.parse::<f32>() {
                *value = v as Real;
            }
            edit.editing = false;
            edit.cursor = 0;
        } else {
            return ResourceState::ACTIVE;
        }
    }
    ResourceState::NONE
}

impl Widget for Slider {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let base = ctx.rect();
        let last = self.value;
        let mut v = last;
        if !number_textbox_handle(ctx, control, &mut self.edit, self.precision, &mut v).is_none() {
            return res;
        }
        if let Some(delta) = control.scroll_delta {
            let wheel = if delta.y != 0 { delta.y.signum() } else { delta.x.signum() };
            if wheel != 0 {
                let step_amount = if self.step != 0. { self.step } else { (self.high - self.low) / 100.0 };
                v += wheel as Real * step_amount;
                if self.step != 0. {
                    v = (v + self.step / 2 as Real) / self.step * self.step;
                }
            }
        }
        let default_input = InputSnapshot::default();
        let input = ctx.input().unwrap_or(&default_input);
        if control.focused && (!input.mouse_down.is_none() || input.mouse_pressed.is_left()) {
            v = self.low + (input.mouse_pos.x - base.x) as Real * (self.high - self.low) / base.width as Real;
            if self.step != 0. {
                v = (v + self.step / 2 as Real) / self.step * self.step;
            }
        }
        v = if self.high < (if self.low > v { self.low } else { v }) {
            self.high
        } else if self.low > v {
            self.low
        } else {
            v
        };
        self.value = v;
        if last != v {
            res |= ResourceState::CHANGE;
        }
        ctx.draw_widget_frame(control, base, ControlColor::Base, self.opt);
        let w = ctx.style().thumb_size;
        let x = ((v - self.low) * (base.width - w) as Real / (self.high - self.low)) as i32;
        let thumb = rect(base.x + x, base.y, w, base.height);
        ctx.draw_widget_frame(control, thumb, ControlColor::Button, self.opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", self.precision, self.value).as_str());
        ctx.draw_control_text(buff.as_str(), base, ControlColor::Text, self.opt);
        res
    }
}

#[derive(Clone)]
/// Persistent state for number input widgets.
pub struct Number {
    /// Current number value.
    pub value: Real,
    /// Step applied when dragging.
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Widget options applied to the number input.
    pub opt: WidgetOption,
    /// Behaviour options applied to the number input.
    pub bopt: WidgetBehaviourOption,
    /// Text editing state for shift-click numeric entry.
    pub edit: NumberEditState,
}

#[derive(Clone, Default)]
/// Editing buffer for number-style widgets.
pub struct NumberEditState {
    /// Whether the widget is currently in edit mode.
    pub editing: bool,
    /// Text buffer for numeric input.
    pub buf: String,
    /// Cursor position within the buffer (byte index).
    pub cursor: usize,
}

impl Number {
    /// Creates a number input with default widget options.
    pub fn new(value: Real, step: Real, precision: usize) -> Self {
        Self {
            value,
            step,
            precision,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
            edit: NumberEditState::default(),
        }
    }

    /// Creates a number input with explicit widget options.
    pub fn with_opt(value: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            step,
            precision,
            opt,
            bopt: WidgetBehaviourOption::NONE,
            edit: NumberEditState::default(),
        }
    }
}

impl Widget for Number {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let base = ctx.rect();
        let last = self.value;
        if !number_textbox_handle(ctx, control, &mut self.edit, self.precision, &mut self.value).is_none() {
            return res;
        }
        let default_input = InputSnapshot::default();
        let input = ctx.input().unwrap_or(&default_input);
        if control.focused && input.mouse_down.is_left() {
            self.value += input.mouse_delta.x as Real * self.step;
        }
        if self.value != last {
            res |= ResourceState::CHANGE;
        }
        ctx.draw_widget_frame(control, base, ControlColor::Base, self.opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", self.precision, self.value).as_str());
        ctx.draw_control_text(buff.as_str(), base, ControlColor::Text, self.opt);
        res
    }
}

#[derive(Clone)]
/// Persistent state for custom render widgets.
pub struct Custom {
    /// Label used for debugging or inspection.
    pub name: String,
    /// Widget options applied to the custom widget.
    pub opt: WidgetOption,
    /// Behaviour options applied to the custom widget.
    pub bopt: WidgetBehaviourOption,
}

impl Custom {
    /// Creates a custom widget state with default options.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
        }
    }

    /// Creates a custom widget state with explicit options.
    pub fn with_opt(name: impl Into<String>, opt: WidgetOption, bopt: WidgetBehaviourOption) -> Self {
        Self { name: name.into(), opt, bopt }
    }
}

impl Widget for Custom {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for internal window/container controls.
pub struct Internal {
    /// Stable tag describing the internal control.
    pub tag: &'static str,
    /// Widget options applied to the internal control.
    pub opt: WidgetOption,
    /// Behaviour options applied to the internal control.
    pub bopt: WidgetBehaviourOption,
}

impl Internal {
    /// Creates an internal control state with a stable tag.
    pub fn new(tag: &'static str) -> Self {
        Self {
            tag,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
        }
    }
}

impl Widget for Internal {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

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
    /// Behaviour options applied to the combo header.
    pub bopt: WidgetBehaviourOption,
}

impl Combo {
    /// Creates a new combo state with the provided popup handle.
    pub fn new(popup: WindowHandle) -> Self {
        Self { popup, selected: 0, open: false, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a new combo state with explicit widget options.
    pub fn with_opt(popup: WindowHandle, opt: WidgetOption, bopt: WidgetBehaviourOption) -> Self {
        Self { popup, selected: 0, open: false, opt, bopt }
    }
}

impl Widget for Combo {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}
