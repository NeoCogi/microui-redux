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
use crate::*;
use std::rc::Rc;

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
