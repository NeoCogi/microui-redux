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
use std::cell::RefCell;
use std::collections::HashMap;

/// Arguments forwarded to custom rendering callbacks.
pub struct CustomRenderArgs {
    /// Rectangle describing the widget's content area.
    pub content_area: Rect<i32>,
    /// Final clipped region that is visible.
    pub view: Rect<i32>, // clipped area
    /// Latest mouse interaction affecting the widget.
    pub mouse_event: MouseEvent,
    /// Scroll delta consumed for this widget, if any.
    pub scroll_delta: Option<Vec2i>,
    /// Options provided when the widget was created.
    pub widget_opt: WidgetOption,
    /// Behaviour options provided when the widget was created.
    pub behaviour_opt: WidgetBehaviourOption,
    /// Currently active modifier keys.
    pub key_mods: KeyMode,
    /// Currently active navigation keys.
    pub key_codes: KeyCode,
    /// Text input collected while the widget was focused.
    pub text_input: String,
}

/// Shared context passed to widget handlers.
pub struct WidgetCtx<'a> {
    id: Id,
    rect: Recti,
    commands: &'a mut Vec<Command>,
    clip_stack: &'a mut Vec<Recti>,
    style: &'a Style,
    atlas: &'a AtlasHandle,
    focus: &'a mut Option<Id>,
    updated_focus: &'a mut bool,
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
    ) -> Self {
        Self {
            id,
            rect,
            commands,
            clip_stack,
            style,
            atlas,
            focus,
            updated_focus,
        }
    }

    /// Returns the widget identifier.
    pub fn id(&self) -> Id { self.id }

    /// Returns the widget rectangle.
    pub fn rect(&self) -> Recti { self.rect }

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
    pub fn push_clip_rect(&mut self, rect: Recti) {
        let last = self.current_clip_rect();
        self.clip_stack.push(rect.intersect(&last).unwrap_or_default());
    }

    /// Pops the current clip rectangle.
    pub fn pop_clip_rect(&mut self) { self.clip_stack.pop(); }

    /// Executes `f` with the provided clip rect applied.
    pub fn with_clip<F: FnOnce(&mut Self)>(&mut self, rect: Recti, f: F) {
        self.push_clip_rect(rect);
        f(self);
        self.pop_clip_rect();
    }

    fn current_clip_rect(&self) -> Recti { self.clip_stack.last().copied().unwrap_or(UNCLIPPED_RECT) }
}

/// Controls how text should wrap when rendered inside a container.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextWrap {
    /// Render text on a single line without wrapping.
    None,
    /// Wrap text at word boundaries when it exceeds the cell width.
    Word,
}

#[derive(Default)]
struct TextEditState {
    cursor: usize,
}

/// Draw commands recorded during container traversal.
pub enum Command {
    /// Pushes or pops a clip rectangle.
    Clip {
        /// Rect to clip against.
        rect: Recti,
    },
    /// Draws a solid rectangle.
    Recti {
        /// Target rectangle.
        rect: Recti,
        /// Fill color.
        color: Color,
    },
    /// Draws text.
    Text {
        /// Font to use.
        font: FontId,
        /// Top-left text position.
        pos: Vec2i,
        /// Text color.
        color: Color,
        /// UTF-8 string to render.
        text: String,
    },
    /// Draws an icon from the atlas.
    Icon {
        /// Target rectangle.
        rect: Recti,
        /// Icon identifier.
        id: IconId,
        /// Tint color.
        color: Color,
    },
    /// Draws an arbitrary image (slot or texture).
    Image {
        /// Target rectangle.
        rect: Recti,
        /// Image identifier.
        image: Image,
        /// Tint color.
        color: Color,
    },
    /// Re-renders a slot before drawing it.
    SlotRedraw {
        /// Target rectangle.
        rect: Recti,
        /// Slot to update.
        id: SlotId,
        /// Tint color.
        color: Color,
        /// Callback generating pixels.
        payload: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
    /// Invokes a user callback for custom rendering.
    CustomRender(CustomRenderArgs, Box<dyn FnMut(Dimensioni, &CustomRenderArgs)>),
    /// Sentinel used when no command is enqueued.
    None,
}

impl Default for Command {
    fn default() -> Self { Command::None }
}

/// Core UI building block that records commands and hosts layouts.
pub struct Container {
    pub(crate) atlas: AtlasHandle,
    /// Style used when drawing widgets in the container.
    pub style: Style,
    /// Human-readable name for the container.
    pub name: String,
    /// Outer rectangle including frame and title.
    pub rect: Recti,
    /// Inner rectangle excluding frame/title.
    pub body: Recti,
    /// Size of the content region based on layout traversal.
    pub content_size: Vec2i,
    /// Accumulated scroll offset.
    pub scroll: Vec2i,
    /// Z-index used to order overlapping windows.
    pub zindex: i32,
    /// Recorded draw commands for this frame.
    pub command_list: Vec<Command>,
    /// Stack of clip rectangles applied while drawing.
    pub clip_stack: Vec<Recti>,
    pub(crate) layout: LayoutManager,
    /// ID of the widget currently hovered, if any.
    pub hover: Option<Id>,
    /// ID of the widget currently focused, if any.
    pub focus: Option<Id>,
    /// Tracks whether focus changed this frame.
    pub updated_focus: bool,
    /// Internal state for the window title bar.
    pub(crate) title_state: InternalState,
    /// Internal state for the window close button.
    pub(crate) close_state: InternalState,
    /// Internal state for the window resize handle.
    pub(crate) resize_state: InternalState,
    /// Internal state for the vertical scrollbar.
    pub(crate) scrollbar_y_state: InternalState,
    /// Internal state for the horizontal scrollbar.
    pub(crate) scrollbar_x_state: InternalState,
    /// Shared access to the input state.
    pub input: Rc<RefCell<Input>>,
    /// Whether this container is the current hover root.
    pub in_hover_root: bool,
    /// Buffer used when editing number widgets.
    pub number_edit_buf: String,
    /// ID of the number widget currently in edit mode, if any.
    pub number_edit: Option<Id>,
    /// Tracks whether a popup was just opened this frame to avoid instant auto-close.
    pub popup_just_opened: bool,
    text_states: HashMap<Id, TextEditState>,
    pending_scroll: Option<Vec2i>,
    /// Determines whether container scrollbars and scroll consumption are enabled.
    scroll_enabled: bool,

    panels: Vec<ContainerHandle>,
}

/// Persistent state used by `combo_box` to track popup and selection.
#[derive(Clone)]
pub struct ComboState {
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

impl ComboState {
    /// Creates a new combo state with the provided popup handle.
    pub fn new(popup: WindowHandle) -> Self {
        Self { popup, selected: 0, open: false, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a new combo state with explicit widget options.
    pub fn with_opt(popup: WindowHandle, opt: WidgetOption, bopt: WidgetBehaviourOption) -> Self {
        Self { popup, selected: 0, open: false, opt, bopt }
    }
}

impl WidgetState for ComboState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

impl Container {
    pub(crate) fn new(name: &str, atlas: AtlasHandle, style: &Style, input: Rc<RefCell<Input>>) -> Self {
        Self {
            name: name.to_string(),
            style: style.clone(),
            atlas: atlas,
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Vec2i::default(),
            scroll: Vec2i::default(),
            zindex: 0,
            command_list: Vec::default(),
            clip_stack: Vec::default(),
            hover: None,
            focus: None,
            updated_focus: false,
            layout: LayoutManager::default(),
            title_state: InternalState::new("!title"),
            close_state: InternalState::new("!close"),
            resize_state: InternalState::new("!resize"),
            scrollbar_y_state: InternalState::new("!scrollbary"),
            scrollbar_x_state: InternalState::new("!scrollbarx"),
            number_edit_buf: String::default(),
            number_edit: None,
            popup_just_opened: false,
            in_hover_root: false,
            input: input,
            text_states: HashMap::new(),
            pending_scroll: None,
            scroll_enabled: true,

            panels: Default::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.hover = None;
        self.focus = None;
        self.updated_focus = false;
        self.in_hover_root = false;
        self.text_states.clear();
        self.pending_scroll = None;
        self.scroll_enabled = true;
    }

    pub(crate) fn prepare(&mut self) {
        self.command_list.clear();
        assert!(self.clip_stack.len() == 0);
        self.panels.clear();
        self.pending_scroll = None;
        self.scroll_enabled = true;
    }

    pub(crate) fn seed_pending_scroll(&mut self, delta: Option<Vec2i>) { self.pending_scroll = delta; }

    #[inline(never)]
    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        for command in self.command_list.drain(0..) {
            match command {
                Command::Text { text, pos, color, font } => {
                    canvas.draw_chars(font, &text, pos, color);
                }
                Command::Recti { rect, color } => {
                    canvas.draw_rect(rect, color);
                }
                Command::Icon { id, rect, color } => {
                    canvas.draw_icon(id, rect, color);
                }
                Command::Clip { rect } => {
                    canvas.set_clip_rect(rect);
                }
                Command::Image { rect, image, color } => {
                    canvas.draw_image(image, rect, color);
                }
                Command::SlotRedraw { rect, id, color, payload } => {
                    canvas.draw_slot_with_function(id, rect, color, payload.clone());
                }
                Command::CustomRender(mut cra, mut f) => {
                    canvas.flush();
                    let prev_clip = canvas.current_clip_rect();
                    let merged_clip = match prev_clip.intersect(&cra.view) {
                        Some(rect) => rect,
                        None => Recti::new(cra.content_area.x, cra.content_area.y, 0, 0),
                    };
                    canvas.set_clip_rect(merged_clip);
                    cra.view = merged_clip;
                    (*f)(canvas.current_dimension(), &cra);
                    canvas.flush();
                    canvas.set_clip_rect(prev_clip);
                }
                Command::None => (),
            }
        }

        for ap in &mut self.panels {
            ap.render(canvas)
        }
    }

    /// Pushes a new clip rectangle combined with the previous clip.
    pub fn push_clip_rect(&mut self, rect: Recti) {
        let last = self.get_clip_rect();
        self.clip_stack.push(rect.intersect(&last).unwrap_or_default());
    }

    /// Restores the previous clip rectangle from the stack.
    pub fn pop_clip_rect(&mut self) { self.clip_stack.pop(); }

    /// Returns the active clip rectangle, or an unclipped rect when the stack is empty.
    pub fn get_clip_rect(&mut self) -> Recti {
        match self.clip_stack.last() {
            Some(r) => *r,
            None => UNCLIPPED_RECT,
        }
    }

    /// Determines whether `r` is fully visible, partially visible, or completely clipped.
    pub fn check_clip(&mut self, r: Recti) -> Clip {
        let cr = self.get_clip_rect();
        if r.x > cr.x + cr.width || r.x + r.width < cr.x || r.y > cr.y + cr.height || r.y + r.height < cr.y {
            return Clip::All;
        }
        if r.x >= cr.x && r.x + r.width <= cr.x + cr.width && r.y >= cr.y && r.y + r.height <= cr.y + cr.height {
            return Clip::None;
        }
        return Clip::Part;
    }

    /// Enqueues a draw command to be consumed during rendering.
    pub fn push_command(&mut self, cmd: Command) { self.command_list.push(cmd); }

    /// Adjusts the current clip rectangle.
    pub fn set_clip(&mut self, rect: Recti) { self.push_command(Command::Clip { rect }); }

    /// Manually updates which widget owns focus.
    pub fn set_focus(&mut self, id: Option<Id>) {
        self.focus = id;
        self.updated_focus = true;
    }

    /// Records a filled rectangle draw command.
    pub fn draw_rect(&mut self, mut rect: Recti, color: Color) {
        rect = rect.intersect(&self.get_clip_rect()).unwrap_or_default();
        if rect.width > 0 && rect.height > 0 {
            self.push_command(Command::Recti { rect, color });
        }
    }

    /// Records a rectangle outline.
    pub fn draw_box(&mut self, r: Recti, color: Color) {
        self.draw_rect(rect(r.x + 1, r.y, r.width - 2, 1), color);
        self.draw_rect(rect(r.x + 1, r.y + r.height - 1, r.width - 2, 1), color);
        self.draw_rect(rect(r.x, r.y, 1, r.height), color);
        self.draw_rect(rect(r.x + r.width - 1, r.y, 1, r.height), color);
    }

    /// Records a text draw command.
    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        let tsize = self.atlas.get_text_size(font, str);
        let rect: Recti = rect(pos.x, pos.y, tsize.width, tsize.height);
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }

        self.push_command(Command::Text {
            text: String::from(str),
            pos,
            color,
            font,
        });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    /// Records an icon draw command.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::Icon { id, rect, color });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    /// Records a slot draw command.
    pub fn draw_slot(&mut self, id: SlotId, rect: Recti, color: Color) { self.push_image(Image::Slot(id), rect, color); }

    /// Records a slot redraw that uses a callback to fill pixels.
    pub fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::SlotRedraw { id, rect, color, payload: f });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    #[inline(never)]
    /// Draws multi-line text within the container using automatic wrapping.
    pub fn text(&mut self, text: &str) { self.text_with_wrap(text, TextWrap::None); }

    #[inline(never)]
    /// Draws multi-line text within the container using the provided wrapping mode.
    /// The block is rendered inside an internal column with zero spacing so consecutive
    /// lines sit back-to-back while the outer widget spacing/padding remains intact.
    pub fn text_with_wrap(&mut self, text: &str, wrap: TextWrap) {
        let font = self.style.font;
        let color = self.style.colors[ControlColor::Text as usize];
        let line_height = self.atlas.get_font_height(font) as i32;
        let baseline = self.atlas.get_font_baseline(font);
        let saved_spacing = self.layout.style.spacing;
        self.layout.style.spacing = 0;
        self.column(|ui| {
            ui.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Fixed(line_height));

            for line in text.lines() {
                match wrap {
                    TextWrap::None => {
                        let r = ui.layout.next();
                        let line_top = Self::baseline_aligned_top(r, line_height, baseline);
                        ui.draw_text(font, line, vec2(r.x, line_top), color);
                    }
                    TextWrap::Word => {
                        let mut r = ui.layout.next();
                        let mut rx = r.x;
                        let mut line_top = Self::baseline_aligned_top(r, line_height, baseline);
                        let words = line.split_inclusive(' ');
                        for w in words {
                            let tw = ui.atlas.get_text_size(font, w).width;
                            if tw + rx > r.x + r.width && rx > r.x {
                                r = ui.layout.next();
                                rx = r.x;
                                line_top = Self::baseline_aligned_top(r, line_height, baseline);
                            }
                            ui.draw_text(font, w, vec2(rx, line_top), color);
                            rx += tw;
                        }
                    }
                }
            }
        });
        self.layout.style.spacing = saved_spacing;
    }

    /// Draws a frame and optional border using the specified color.
    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let color = self.style.colors[colorid as usize];
        self.draw_rect(rect, color);
        if colorid == ControlColor::ScrollBase || colorid == ControlColor::ScrollThumb || colorid == ControlColor::TitleBG {
            return;
        }
        let border_color = self.style.colors[ControlColor::Border as usize];
        if border_color.a != 0 {
            self.draw_box(expand_rect(rect, 1), border_color);
        }
    }

    /// Draws a widget background, applying hover/focus accents when needed.
    pub fn draw_widget_frame(&mut self, id: Id, rect: Recti, mut colorid: ControlColor, opt: WidgetOption) {
        if opt.has_no_frame() {
            return;
        }
        if self.focus == Some(id) {
            colorid.focus()
        } else if self.hover == Some(id) {
            colorid.hover()
        }
        self.draw_frame(rect, colorid);
    }

    fn widget_fill_color(&self, id: Id, base: ControlColor, fill: WidgetFillOption) -> Option<ControlColor> {
        if self.focus == Some(id) && fill.fill_click() {
            let mut color = base;
            color.focus();
            Some(color)
        } else if self.hover == Some(id) && fill.fill_hover() {
            let mut color = base;
            color.hover();
            Some(color)
        } else if fill.fill_normal() {
            Some(base)
        } else {
            None
        }
    }

    /// Draws a container frame, skipping rendering when the option disables it.
    pub fn draw_container_frame(&mut self, id: Id, rect: Recti, mut colorid: ControlColor, opt: ContainerOption) {
        if opt.has_no_frame() {
            return;
        }

        if self.focus == Some(id) {
            colorid.focus()
        } else if self.hover == Some(id) {
            colorid.hover()
        }
        self.draw_frame(rect, colorid);
    }

    #[inline(never)]
    /// Draws widget text with the appropriate alignment flags.
    pub fn draw_control_text(&mut self, str: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let mut pos: Vec2i = Vec2i { x: 0, y: 0 };
        let font = self.style.font;
        let tsize = self.atlas.get_text_size(font, str);
        let padding = self.style.padding;
        let color = self.style.colors[colorid as usize];
        let line_height = self.atlas.get_font_height(font) as i32;
        let baseline = self.atlas.get_font_baseline(font);

        self.push_clip_rect(rect);
        pos.y = Self::baseline_aligned_top(rect, line_height, baseline);
        if opt.is_aligned_center() {
            pos.x = rect.x + (rect.width - tsize.width) / 2;
        } else if opt.is_aligned_right() {
            pos.x = rect.x + rect.width - tsize.width - padding;
        } else {
            pos.x = rect.x + padding;
        }
        self.draw_text(font, str, pos, color);
        self.pop_clip_rect();
    }

    /// Returns `true` if the cursor is inside `rect` and the container owns the hover root.
    pub fn mouse_over(&mut self, rect: Recti, in_hover_root: bool) -> bool {
        let clip_rect = self.get_clip_rect();
        rect.contains(&self.input.borrow().mouse_pos) && clip_rect.contains(&self.input.borrow().mouse_pos) && in_hover_root
    }

    #[inline(never)]
    /// Updates hover/focus state for the widget described by `id` and optionally consumes scroll.
    pub fn update_control<W: WidgetState>(&mut self, id: Id, rect: Recti, state: &W) -> ControlState {
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let in_hover_root = self.in_hover_root;
        let mouseover = self.mouse_over(rect, in_hover_root);
        if self.focus == Some(id) {
            // is this the same ID of the focused widget? by default set it to true unless otherwise
            self.updated_focus = true;
        }
        if opt.is_not_interactive() {
            return ControlState::default();
        }
        if mouseover && self.input.borrow().mouse_down.is_none() {
            self.hover = Some(id);
        }
        if self.focus == Some(id) {
            if !self.input.borrow().mouse_pressed.is_none() && !mouseover {
                self.set_focus(None);
            }
            if self.input.borrow().mouse_down.is_none() && !opt.is_holding_focus() {
                self.set_focus(None);
            }
        }
        if self.hover == Some(id) {
            if !self.input.borrow().mouse_pressed.is_none() {
                self.set_focus(Some(id));
            } else if !mouseover {
                self.hover = None;
            }
        }

        let mut scroll = None;
        if bopt.is_grab_scroll() && self.hover == Some(id) {
            if let Some(delta) = self.pending_scroll {
                if delta.x != 0 || delta.y != 0 {
                    self.pending_scroll = None;
                    scroll = Some(delta);
                }
            }
        }

        if self.focus == Some(id) {
            let mouse_pos = self.input.borrow().mouse_pos;
            let origin = vec2(self.body.x, self.body.y);
            self.input.borrow_mut().rel_mouse_pos = mouse_pos - origin;
        }

        let input = self.input.borrow();
        let focused = self.focus == Some(id);
        let hovered = self.hover == Some(id);
        let clicked = focused && input.mouse_pressed.is_left();
        let active = focused && input.mouse_down.is_left();
        drop(input);

        ControlState {
            hovered,
            focused,
            clicked,
            active,
            scroll_delta: scroll,
        }
    }

    /// Resets transient per-frame state after widgets have been processed.
    pub fn finish(&mut self) {
        if !self.updated_focus {
            self.focus = None;
        }
        self.updated_focus = false;
    }

    #[inline(never)]
    fn node(&mut self, state: &mut NodeState, is_treenode: bool) -> NodeStateValue {
        let id: Id = state.get_id();
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let mut r = self.layout.next();
        let opt = state.opt;
        let _ = self.update_control(id, r, state);

        let expanded = state.state.is_expanded();
        let active = expanded ^ (self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id));

        if is_treenode {
            if self.hover == Some(id) {
                self.draw_frame(r, ControlColor::ButtonHover);
            }
        } else {
            self.draw_widget_frame(id, r, ControlColor::Button, opt);
        }
        let color = self.style.colors[ControlColor::Text as usize];
        self.draw_icon(if expanded { COLLAPSE_ICON } else { EXPAND_ICON }, rect(r.x, r.y, r.height, r.height), color);
        r.x += r.height - self.style.padding;
        r.width -= r.height - self.style.padding;
        self.draw_control_text(state.label.as_str(), r, ControlColor::Text, opt);
        let new_state = if active { NodeStateValue::Expanded } else { NodeStateValue::Closed };
        state.state = new_state;
        new_state
    }

    /// Builds a collapsible header row that executes `f` when expanded.
    pub fn header<F: FnOnce(&mut Self)>(&mut self, state: &mut NodeState, f: F) -> NodeStateValue {
        let new_state = self.node(state, false);
        if new_state.is_expanded() {
            f(self);
        }
        new_state
    }

    /// Builds a tree node with automatic indentation while expanded.
    pub fn treenode<F: FnOnce(&mut Self)>(&mut self, state: &mut NodeState, f: F) -> NodeStateValue {
        let res = self.node(state, true);
        if res.is_expanded() {
            let indent = self.style.indent;
            self.layout.adjust_indent(indent);
            f(self);
            self.layout.adjust_indent(-indent);
        }

        res
    }

    fn clamp(x: i32, a: i32, b: i32) -> i32 { min(b, max(a, x)) }

    /// Returns the y coordinate where a line of text should start so its baseline sits at the control midpoint.
    fn baseline_aligned_top(rect: Recti, line_height: i32, baseline: i32) -> i32 {
        if rect.height >= line_height {
            return rect.y + (rect.height - line_height) / 2;
        }

        let baseline_center = rect.y + rect.height / 2;
        let min_top = rect.y + rect.height - line_height;
        let max_top = rect.y;
        Self::clamp(baseline_center - baseline, min_top, max_top)
    }

    fn vertical_text_padding(padding: i32) -> i32 { max(1, padding / 2) }

    pub(crate) fn consume_pending_scroll(&mut self) {
        if !self.scroll_enabled {
            return;
        }
        let delta = match self.pending_scroll {
            Some(delta) if delta.x != 0 || delta.y != 0 => delta,
            _ => return,
        };

        let mut consumed = false;
        let mut scroll = self.scroll;
        let mut content_size = self.content_size;
        let padding = self.style.padding * 2;
        content_size.x += padding;
        content_size.y += padding;
        let body = self.body;

        let maxscroll_y = content_size.y - body.height;
        if delta.y != 0 && maxscroll_y > 0 && body.height > 0 {
            let new_scroll = Self::clamp(scroll.y + delta.y, 0, maxscroll_y);
            if new_scroll != scroll.y {
                scroll.y = new_scroll;
                consumed = true;
            }
        }

        let maxscroll_x = content_size.x - body.width;
        if delta.x != 0 && maxscroll_x > 0 && body.width > 0 {
            let new_scroll = Self::clamp(scroll.x + delta.x, 0, maxscroll_x);
            if new_scroll != scroll.x {
                scroll.x = new_scroll;
                consumed = true;
            }
        }

        if consumed {
            self.scroll = scroll;
            self.pending_scroll = None;
        }
    }

    #[inline(never)]
    fn scrollbars(&mut self, body: &mut Recti) {
        let sz = self.style.scrollbar_size;
        let mut cs: Vec2i = self.content_size;
        cs.x += self.style.padding * 2;
        cs.y += self.style.padding * 2;
        self.push_clip_rect(body.clone());
        if cs.y > self.body.height {
            body.width -= sz;
        }
        if cs.x > self.body.width {
            body.height -= sz;
        }
        let body = *body;
        let maxscroll = cs.y - body.height;
        if maxscroll > 0 && body.height > 0 {
            let id: Id = self.scrollbar_y_state.get_id();
            let mut base = body;
            base.x = body.x + body.width;
            base.width = self.style.scrollbar_size;
            let scroll_state = (self.scrollbar_y_state.opt, self.scrollbar_y_state.bopt);
            let _ = self.update_control(id, base, &scroll_state);
            if self.focus == Some(id) && self.input.borrow().mouse_down.is_left() {
                self.scroll.y += self.input.borrow().mouse_delta.y * cs.y / base.height;
            }

            self.draw_frame(base, ControlColor::ScrollBase);
            let mut thumb = base;
            thumb.height = if self.style.thumb_size > base.height * body.height / cs.y {
                self.style.thumb_size
            } else {
                base.height * body.height / cs.y
            };
            thumb.y += self.scroll.y * (base.height - thumb.height) / maxscroll;
            self.draw_frame(thumb, ControlColor::ScrollThumb);
            self.scroll.y = Self::clamp(self.scroll.y, 0, maxscroll);
        } else {
            self.scroll.y = 0;
        }
        let maxscroll_0 = cs.x - body.width;
        if maxscroll_0 > 0 && body.width > 0 {
            let id_0: Id = self.scrollbar_x_state.get_id();
            let mut base_0 = body;
            base_0.y = body.y + body.height;
            base_0.height = self.style.scrollbar_size;
            let scroll_state = (self.scrollbar_x_state.opt, self.scrollbar_x_state.bopt);
            let _ = self.update_control(id_0, base_0, &scroll_state);
            if self.focus == Some(id_0) && self.input.borrow().mouse_down.is_left() {
                self.scroll.x += self.input.borrow().mouse_delta.x * cs.x / base_0.width;
            }

            self.draw_frame(base_0, ControlColor::ScrollBase);
            let mut thumb_0 = base_0;
            thumb_0.width = if self.style.thumb_size > base_0.width * body.width / cs.x {
                self.style.thumb_size
            } else {
                base_0.width * body.width / cs.x
            };
            thumb_0.x += self.scroll.x * (base_0.width - thumb_0.width) / maxscroll_0;
            self.draw_frame(thumb_0, ControlColor::ScrollThumb);
            self.scroll.x = Self::clamp(self.scroll.x, 0, maxscroll_0);
        } else {
            self.scroll.x = 0;
        }
        self.pop_clip_rect();
    }

    /// Configures layout state for the container's client area, handling scrollbars when necessary.
    pub fn push_container_body(&mut self, body: Recti, opt: ContainerOption, bopt: WidgetBehaviourOption) {
        let mut body = body;
        self.scroll_enabled = !bopt.is_no_scroll();
        if self.scroll_enabled {
            self.scrollbars(&mut body);
        }
        let style = self.style;
        let padding = -style.padding;
        let scroll = self.scroll;
        self.layout.reset(expand_rect(body, padding), scroll);
        self.layout.style = self.style.clone();
        let font_height = self.atlas.get_font_height(self.style.font) as i32;
        let vertical_pad = Self::vertical_text_padding(self.style.padding);
        let icon_height = self.atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let default_height = max(font_height + vertical_pad * 2, icon_height);
        self.layout.set_default_cell_height(default_height);
        self.body = body;
    }

    fn pop_panel(&mut self, panel: &mut ContainerHandle) {
        let layout_body = panel.inner().layout.current_body();
        let layout_max = panel.inner().layout.current_max();
        let container = &mut panel.inner_mut();

        match layout_max {
            None => (),
            Some(lm) => container.content_size = Vec2i::new(lm.x - layout_body.x, lm.y - layout_body.y),
        }

        container.layout.pop_scope();
    }

    #[inline(never)]
    fn begin_panel(&mut self, panel: &mut ContainerHandle, opt: ContainerOption, bopt: WidgetBehaviourOption) {
        let rect = self.layout.next();
        let container = &mut panel.inner_mut();
        container.prepare();

        container.rect = rect;
        if !opt.has_no_frame() {
            self.draw_frame(rect, ControlColor::PanelBG);
        }

        container.in_hover_root = self.in_hover_root;
        if self.pending_scroll.is_some() && self.mouse_over(rect, self.in_hover_root) {
            container.pending_scroll = self.pending_scroll.take();
        }
        container.push_container_body(rect, opt, bopt);
        let clip_rect = container.body;
        container.push_clip_rect(clip_rect);
    }

    fn end_panel(&mut self, panel: &mut ContainerHandle) {
        panel.inner_mut().pop_clip_rect();
        self.pop_panel(panel);
        {
            let mut inner = panel.inner_mut();
            inner.consume_pending_scroll();
            let pending = inner.pending_scroll.take();
            if self.pending_scroll.is_none() {
                self.pending_scroll = pending;
            }
        }
        self.panels.push(panel.clone())
    }

    /// Embeds another container handle inside the current layout.
    pub fn panel<F: FnOnce(&mut ContainerHandle)>(&mut self, panel: &mut ContainerHandle, opt: ContainerOption, bopt: WidgetBehaviourOption, f: F) {
        self.begin_panel(panel, opt, bopt);

        // call the panel function
        f(panel);

        self.end_panel(panel);
    }

    /// Temporarily overrides the row definition and restores it after `f` executes.
    pub fn with_row<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], height: SizePolicy, f: F) {
        let snapshot = self.layout.snapshot_row_state();
        self.layout.row(widths, height);
        f(self);
        self.layout.restore_row_state(snapshot);
    }

    /// Creates a nested column scope where each call to `next_cell` yields a single column.
    pub fn column<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.layout.begin_column();
        f(self);
        self.layout.end_column();
    }

    /// Returns the next layout cell's rectangle.
    pub fn next_cell(&mut self) -> Recti { self.layout.next() }

    /// Replaces the container's style.
    pub fn set_style(&mut self, style: Style) { self.style = style; }

    /// Returns a copy of the current style.
    pub fn get_style(&self) -> Style { self.style.clone() }

    /// Displays static text using the default text color.
    pub fn label(&mut self, text: &str) {
        let layout = self.layout.next();
        self.draw_control_text(text, layout, ControlColor::Text, WidgetOption::NONE);
    }

    #[inline(never)]
    /// Draws a button using the provided persistent state.
    pub fn button(&mut self, state: &mut ButtonState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = state.get_id();
        let r: Recti = self.layout.next();
        let _ = self.update_control(id, r, state);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }
        if !state.opt.has_no_frame() {
            if let Some(colorid) = self.widget_fill_color(id, ControlColor::Button, state.fill) {
                self.draw_frame(r, colorid);
            }
        }
        match &state.content {
            ButtonContent::Text { label, icon } => {
                if !label.is_empty() {
                    self.draw_control_text(label, r, ControlColor::Text, state.opt);
                }
                if let Some(icon) = icon {
                    let color = self.style.colors[ControlColor::Text as usize];
                    self.draw_icon(*icon, r, color);
                }
            }
            ButtonContent::Image { label, image } => {
                if !label.is_empty() {
                    self.draw_control_text(label, r, ControlColor::Text, state.opt);
                }
                if let Some(image) = *image {
                    let color = self.style.colors[ControlColor::Text as usize];
                    self.push_image(image, r, color);
                }
            }
            ButtonContent::Slot { label, slot, paint } => {
                if !label.is_empty() {
                    self.draw_control_text(label, r, ControlColor::Text, state.opt);
                }
                let color = self.style.colors[ControlColor::Text as usize];
                self.draw_slot_with_function(*slot, r, color, paint.clone());
            }
        }
        res
    }

    #[inline(never)]
    /// Compatibility shim for state-based buttons that render text and optional icons.
    pub fn button_ex(&mut self, state: &mut ButtonState) -> ResourceState { self.button(state) }

    #[inline(never)]
    /// Compatibility shim for state-based buttons that render images.
    pub fn button_ex2(&mut self, state: &mut ButtonState) -> ResourceState { self.button(state) }

    /// Renders a list entry that only highlights while hovered or active.
    pub fn list_item(&mut self, state: &mut ListItemState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = state.get_id();
        let item_rect = self.layout.next();
        let _ = self.update_control(id, item_rect, state);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }

        if self.focus == Some(id) || self.hover == Some(id) {
            let mut color = ControlColor::Button;
            if self.focus == Some(id) {
                color.focus();
            } else {
                color.hover();
            }
            let fill = self.style.colors[color as usize];
            self.draw_rect(item_rect, fill);
        }

        let mut text_rect = item_rect;
        if let Some(icon) = state.icon {
            let padding = self.style.padding.max(0);
            let icon_size = self.atlas.get_icon_size(icon);
            let icon_x = item_rect.x + padding;
            let icon_y = item_rect.y + ((item_rect.height - icon_size.height) / 2).max(0);
            let icon_rect = rect(icon_x, icon_y, icon_size.width, icon_size.height);
            let consumed = icon_size.width + padding * 2;
            text_rect.x += consumed;
            text_rect.width = (text_rect.width - consumed).max(0);
            let color = self.style.colors[ControlColor::Text as usize];
            self.draw_icon(icon, icon_rect, color);
        }

        if !state.label.is_empty() {
            self.draw_control_text(&state.label, text_rect, ControlColor::Text, state.opt);
        }
        res
    }

    #[inline(never)]
    /// Shim for list boxes that only fills on hover or click.
    pub fn list_box(&mut self, state: &mut ListBoxState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = state.get_id();
        let r: Recti = self.layout.next();
        let _ = self.update_control(id, r, state);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }
        if !state.opt.has_no_frame() {
            if let Some(colorid) = self.widget_fill_color(id, ControlColor::Button, WidgetFillOption::HOVER | WidgetFillOption::CLICK) {
                self.draw_frame(r, colorid);
            }
        }
        if !state.label.is_empty() {
            self.draw_control_text(&state.label, r, ControlColor::Text, state.opt);
        }
        if let Some(image) = state.image {
            let color = self.style.colors[ControlColor::Text as usize];
            self.push_image(image, r, color);
        }
        res
    }

    #[inline(never)]
    /// Draws a combo box that opens a popup listing `items` and writes back the selected index.
    pub fn combo_box<S: AsRef<str>>(&mut self, state: &mut ComboState, items: &[S]) -> (Recti, bool, ResourceState) {
        let mut res = ResourceState::NONE;

        // Keep the selected index within bounds so we never index past the slice.
        if state.selected >= items.len() {
            if !items.is_empty() {
                state.selected = items.len() - 1;
                res |= ResourceState::CHANGE;
            } else if state.selected != 0 {
                state.selected = 0;
                res |= ResourceState::CHANGE;
            }
        }

        let id: Id = state.get_id();
        let header: Recti = self.layout.next();
        let _ = self.update_control(id, header, state);

        let header_clicked = self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id);
        let popup_open = state.popup.is_open();

        // Toggle the popup when the header is clicked.
        if header_clicked {
            res |= ResourceState::ACTIVE;
        }

        self.draw_widget_frame(id, header, ControlColor::Button, state.opt);
        let label = items.get(state.selected).map(|s| s.as_ref()).unwrap_or("");
        let indicator_size = self.atlas.get_icon_size(EXPAND_DOWN_ICON);
        let indicator_x = header.x + header.width - indicator_size.width;
        let indicator_y = header.y + ((header.height - indicator_size.height) / 2).max(0);
        let indicator = rect(indicator_x, indicator_y, indicator_size.width, indicator_size.height);

        let mut text_rect = header;
        let reserved_width = indicator_size.width;
        text_rect.width = (text_rect.width - reserved_width).max(0);
        self.draw_control_text(label, text_rect, ControlColor::Text, state.opt);
        self.draw_widget_frame(id, indicator, ControlColor::Button, state.opt);
        let icon_color = self.style.colors[ControlColor::Text as usize];
        self.draw_icon(EXPAND_DOWN_ICON, indicator, icon_color);

        if popup_open {
            res |= ResourceState::ACTIVE;
        }

        let anchor = rect(header.x, header.y + header.height, header.width, 1);
        (anchor, header_clicked, res)
    }

    fn push_image(&mut self, image: Image, rect: Recti, color: Color) {
        let clipped = self.check_clip(rect);
        match clipped {
            Clip::All => return,
            Clip::Part => {
                let clip = self.get_clip_rect();
                self.set_clip(clip)
            }
            _ => (),
        }
        self.push_command(Command::Image { image, rect, color });
        if clipped != Clip::None {
            self.set_clip(UNCLIPPED_RECT);
        }
    }

    #[inline(never)]
    /// Compatibility shim for state-based buttons that render atlas slots.
    pub fn button_ex3(&mut self, state: &mut ButtonState) -> ResourceState { self.button(state) }

    #[inline(never)]
    /// Draws a checkbox labeled with `label` and toggles `state` when clicked.
    pub fn checkbox(&mut self, state: &mut CheckboxState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = state.get_id();
        let mut r: Recti = self.layout.next();
        let box_0: Recti = rect(r.x, r.y, r.height, r.height);
        let _ = self.update_control(id, r, state);
        if self.input.borrow().mouse_pressed.is_left() && self.focus == Some(id) {
            res |= ResourceState::CHANGE;
            state.value = !state.value;
        }
        self.draw_widget_frame(id, box_0, ControlColor::Base, state.opt);
        if state.value {
            let color = self.style.colors[ControlColor::Text as usize];
            self.draw_icon(CHECK_ICON, box_0, color);
        }
        r = rect(r.x + box_0.width, r.y, r.width - box_0.width, r.height);
        self.draw_control_text(&state.label, r, ControlColor::Text, state.opt);
        return res;
    }

    #[inline(never)]
    fn input_to_mouse_event(&self, id: Id, rect: &Recti) -> MouseEvent {
        let input = self.input.borrow();
        let orig = Vec2i::new(rect.x, rect.y);

        let prev_pos = input.last_mouse_pos - orig;
        let curr_pos = input.mouse_pos - orig;
        let mouse_down = input.mouse_down;
        let mouse_pressed = input.mouse_pressed;
        drop(input);

        if self.focus == Some(id) && mouse_down.is_left() {
            return MouseEvent::Drag { prev_pos, curr_pos };
        }

        if self.hover == Some(id) && mouse_pressed.is_left() {
            return MouseEvent::Click(curr_pos);
        }

        if self.hover == Some(id) {
            return MouseEvent::Move(curr_pos);
        }
        MouseEvent::None
    }

    #[inline(never)]
    /// Allocates a widget cell and hands rendering control to user code.
    pub fn custom_render_widget<F: FnMut(Dimensioni, &CustomRenderArgs) + 'static>(
        &mut self,
        state: &mut CustomState,
        f: F,
    ) {
        let id: Id = state.get_id();
        let rect: Recti = self.layout.next();
        let control = self.update_control(id, rect, state);

        let mouse_event = self.input_to_mouse_event(id, &rect);

        let active = self.focus == Some(id) && self.in_hover_root;
        let input = self.input.borrow();
        let key_mods = if active { input.key_state() } else { KeyMode::NONE };
        let key_codes = if active { input.key_codes() } else { KeyCode::NONE };
        let text_input = if active { input.text_input().to_owned() } else { String::new() };
        drop(input);
        let cra = CustomRenderArgs {
            content_area: rect,
            view: self.get_clip_rect(),
            mouse_event,
            scroll_delta: control.scroll_delta,
            widget_opt: state.opt,
            behaviour_opt: state.bopt,
            key_mods,
            key_codes,
            text_input,
        };
        self.command_list.push(Command::CustomRender(cra, Box::new(f)));
    }

    #[inline(never)]
    /// Internal textbox helper operating on a fixed rectangle.
    fn textbox_raw_with_id(&mut self, buf: &mut String, id: Id, r: Recti, opt: WidgetOption, bopt: WidgetBehaviourOption) -> ResourceState {
        // Track submit/change flags and keep the widget focused while active.
        let mut res = ResourceState::NONE;
        let control_state = (opt | WidgetOption::HOLD_FOCUS, bopt);
        let _ = self.update_control(id, r, &control_state);

        // Cursor position is stored per textbox so we can edit at arbitrary positions.
        let mut cursor = {
            let entry = self.text_states.entry(id).or_insert_with(|| TextEditState { cursor: buf.len() });
            if self.focus != Some(id) {
                entry.cursor = buf.len();
            }
            entry.cursor
        };

        // Snapshot the current frame's input so we only borrow the RefCell once.
        let input_text = { self.input.borrow().input_text.clone() };
        let (key_pressed, key_codes, mouse_pressed, mouse_pos) = {
            let input = self.input.borrow();
            (input.key_pressed, input.key_code_pressed, input.mouse_pressed, input.mouse_pos)
        };

        if self.focus == Some(id) {
            // Insert any typed characters at the cursor position.
            if !input_text.is_empty() {
                let insert_at = cursor.min(buf.len());
                buf.insert_str(insert_at, input_text.as_str());
                cursor = insert_at + input_text.len();
                res |= ResourceState::CHANGE;
            }

            // Handle backspace, making sure we don't cut UTF-8 graphemes in half.
            if key_pressed.is_backspace() && cursor > 0 && !buf.is_empty() {
                let mut new_cursor = cursor.min(buf.len());
                new_cursor -= 1;
                while new_cursor > 0 && !buf.is_char_boundary(new_cursor) {
                    new_cursor -= 1;
                }
                buf.replace_range(new_cursor..cursor, "");
                cursor = new_cursor;
                res |= ResourceState::CHANGE;
            }

            // Left/right arrows move by grapheme.
            if key_codes.is_left() && cursor > 0 {
                let mut new_cursor = cursor - 1;
                while new_cursor > 0 && !buf.is_char_boundary(new_cursor) {
                    new_cursor -= 1;
                }
                cursor = new_cursor;
            }

            if key_codes.is_right() && cursor < buf.len() {
                let mut new_cursor = cursor + 1;
                while new_cursor < buf.len() && !buf.is_char_boundary(new_cursor) {
                    new_cursor += 1;
                }
                cursor = new_cursor;
            }

            if key_pressed.is_return() {
                self.set_focus(None);
                res |= ResourceState::SUBMIT;
            }
        }

        self.draw_widget_frame(id, r, ControlColor::Base, opt);

        let font = self.style.font;
        let line_height = self.atlas.get_font_height(font) as i32;
        let baseline = self.atlas.get_font_baseline(font);
        let descent = (line_height - baseline).max(0);

        // Center the line height around the cell midpoint. This ensures the baseline sits in the
        // middle (unless the cell is shorter than the font metrics, in which case we clamp).
        let mut texty = r.y + r.height / 2 - line_height / 2;
        if texty < r.y {
            texty = r.y;
        }
        let max_texty = (r.y + r.height - line_height).max(r.y);
        if texty > max_texty {
            texty = max_texty;
        }
        let baseline_y = texty + line_height - descent;

        // // Debug overlay: green = cell, red = baseline, blue = line-height box.
        // self.draw_box(r, color(0, 255, 0, 64));
        // self.draw_rect(rect(r.x, baseline_y, r.width, 1), color(255, 0, 0, 255));
        // println!("rect: {:?} - baseline {}", r, baseline_y);
        // self.draw_box(rect(r.x, texty, r.width, line_height), color(0, 0, 255, 64));

        let text_metrics = self.atlas.get_text_size(font, buf.as_str());
        let padding = self.style.padding;
        let ofx = r.width - padding - text_metrics.width - 1;
        let textx = r.x + if ofx < padding { ofx } else { padding };

        if self.focus == Some(id) && mouse_pressed.is_left() && self.mouse_over(r, self.in_hover_root) {
            let click_x = mouse_pos.x - textx;
            if click_x <= 0 {
                cursor = 0;
            } else {
                let mut last_width = 0;
                let mut new_cursor = buf.len();
                for (idx, ch) in buf.char_indices() {
                    let next = idx + ch.len_utf8();
                    let width = self.atlas.get_text_size(font, &buf[..next]).width;
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
                cursor = new_cursor.min(buf.len());
            }
        }

        cursor = cursor.min(buf.len());
        if let Some(entry) = self.text_states.get_mut(&id) {
            entry.cursor = cursor;
        }

        let caret_offset = if cursor == 0 {
            0
        } else {
            self.atlas.get_text_size(font, &buf[..cursor]).width
        };

        if self.focus == Some(id) {
            let color = self.style.colors[ControlColor::Text as usize];
            self.push_clip_rect(r);
            // Render text at the top of the content area. The baseline is `texty + baseline`.
            self.draw_text(font, buf.as_str(), vec2(textx, texty), color);
            let caret_top = (baseline_y - baseline + 2).max(r.y).min(r.y + r.height);
            let caret_bottom = (baseline_y + descent - 2).max(r.y).min(r.y + r.height);
            let caret_height = (caret_bottom - caret_top).max(1);
            self.draw_rect(rect(textx + caret_offset, caret_top, 1, caret_height), color);
            self.pop_clip_rect();
        } else {
            self.draw_control_text(buf.as_str(), r, ControlColor::Text, opt);
        }
        res
    }

    /// Draws a textbox in the provided rectangle using the supplied state.
    pub fn textbox_raw(&mut self, state: &mut TextboxState, r: Recti) -> ResourceState {
        let id = state.get_id();
        self.textbox_raw_with_id(&mut state.buf, id, r, state.opt, state.bopt)
    }

    #[inline(never)]
    fn number_textbox(&mut self, precision: usize, value: &mut Real, r: Recti, id: Id) -> ResourceState {
        if self.input.borrow().mouse_pressed.is_left() && self.input.borrow().key_state().is_shift() && self.hover == Some(id) {
            self.number_edit = Some(id);
            self.number_edit_buf.clear();
            self.number_edit_buf.push_str(format!("{:.*}", precision, value).as_str());
        }

        if self.number_edit == Some(id) {
            let mut temp = self.number_edit_buf.clone();
            let res: ResourceState = self.textbox_raw_with_id(&mut temp, id, r, WidgetOption::NONE, WidgetBehaviourOption::NONE);
            self.number_edit_buf = temp;
            if res.is_submitted() || self.focus != Some(id) {
                match self.number_edit_buf.parse::<f32>() {
                    Ok(v) => {
                        *value = v as Real;
                        self.number_edit = None;
                    }
                    _ => (),
                }
                self.number_edit = None;
            } else {
                return ResourceState::ACTIVE;
            }
        }
        return ResourceState::NONE;
    }

    /// Draws a textbox using the next available layout cell.
    pub fn textbox_ex(&mut self, state: &mut TextboxState) -> ResourceState {
        let r: Recti = self.layout.next();
        return self.textbox_raw(state, r);
    }

    #[inline(never)]
    /// Draws a horizontal slider bound to `state`.
    pub fn slider_ex(&mut self, state: &mut SliderState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let last = state.value;
        let mut v = last;
        let id = state.get_id();
        let base = self.layout.next();
        if !self.number_textbox(state.precision, &mut v, base, id).is_none() {
            return res;
        }
        let control = self.update_control(id, base, state);
        if let Some(delta) = control.scroll_delta {
            let wheel = if delta.y != 0 { delta.y.signum() } else { delta.x.signum() };
            if wheel != 0 {
                let step_amount = if state.step != 0. { state.step } else { (state.high - state.low) / 100.0 };
                v += wheel as Real * step_amount;
                if state.step != 0. {
                    v = (v + state.step / 2 as Real) / state.step * state.step;
                }
            }
        }
        if self.focus == Some(id) && (!self.input.borrow().mouse_down.is_none() | self.input.borrow().mouse_pressed.is_left()) {
            v = state.low + (self.input.borrow().mouse_pos.x - base.x) as Real * (state.high - state.low) / base.width as Real;
            if state.step != 0. {
                v = (v + state.step / 2 as Real) / state.step * state.step;
            }
        }
        v = if state.high < (if state.low > v { state.low } else { v }) {
            state.high
        } else if state.low > v {
            state.low
        } else {
            v
        };
        state.value = v;
        if last != v {
            res |= ResourceState::CHANGE;
        }
        self.draw_widget_frame(id, base, ControlColor::Base, state.opt);
        let w = self.style.thumb_size;
        let x = ((v - state.low) * (base.width - w) as Real / (state.high - state.low)) as i32;
        let thumb = rect(base.x + x, base.y, w, base.height);
        self.draw_widget_frame(id, thumb, ControlColor::Button, state.opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", state.precision, state.value).as_str());
        self.draw_control_text(buff.as_str(), base, ControlColor::Text, state.opt);
        return res;
    }

    #[inline(never)]
    /// Draws a numeric input that can be edited via keyboard or by dragging.
    pub fn number_ex(&mut self, state: &mut NumberState) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = state.get_id();
        let base: Recti = self.layout.next();
        let last: Real = state.value;
        if !self.number_textbox(state.precision, &mut state.value, base, id).is_none() {
            return res;
        }
        let _ = self.update_control(id, base, state);
        if self.focus == Some(id) && self.input.borrow().mouse_down.is_left() {
            state.value += self.input.borrow().mouse_delta.x as Real * state.step;
        }
        if state.value != last {
            res |= ResourceState::CHANGE;
        }
        self.draw_widget_frame(id, base, ControlColor::Base, state.opt);
        let mut buff = String::new();
        buff.push_str(format!("{:.*}", state.precision, state.value).as_str());
        self.draw_control_text(buff.as_str(), base, ControlColor::Text, state.opt);
        return res;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, FontEntry, SourceFormat};

    const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

    fn make_test_atlas() -> AtlasHandle {
        let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
        let icons: Vec<(&str, Recti)> = ICON_NAMES
            .iter()
            .map(|name| (*name, Recti::new(0, 0, 1, 1)))
            .collect();
        let entries = vec![
            (
                '_',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'a',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'b',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
        ];
        let fonts = vec![(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &entries,
            },
        )];
        let source = AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &[],
        };
        AtlasHandle::from(&source)
    }

    fn make_container() -> Container {
        let atlas = make_test_atlas();
        let input = Rc::new(RefCell::new(Input::default()));
        let mut container = Container::new("test", atlas, &Style::default(), input);
        container.in_hover_root = true;
        container.push_container_body(rect(0, 0, 100, 30), ContainerOption::NONE, WidgetBehaviourOption::NONE);
        container
    }

    #[test]
    fn textbox_left_moves_over_multibyte() {
        let mut container = make_container();
        let input = container.input.clone();
        let mut state = TextboxState::new("a\u{1F600}b");
        let id = state.get_id();
        container.set_focus(Some(id));
        container.text_states.insert(id, TextEditState { cursor: 5 });

        input.borrow_mut().keydown_code(KeyCode::LEFT);
        let rect = container.layout.next();
        container.textbox_raw(&mut state, rect);

        let cursor = container.text_states.get(&id).unwrap().cursor;
        assert_eq!(cursor, 1);
    }

    #[test]
    fn textbox_backspace_removes_multibyte() {
        let mut container = make_container();
        let input = container.input.clone();
        let mut state = TextboxState::new("a\u{1F600}b");
        let id = state.get_id();
        container.set_focus(Some(id));
        container.text_states.insert(id, TextEditState { cursor: 5 });

        input.borrow_mut().keydown(KeyMode::BACKSPACE);
        let rect = container.layout.next();
        container.textbox_raw(&mut state, rect);

        let cursor = container.text_states.get(&id).unwrap().cursor;
        assert_eq!(state.buf, "ab");
        assert_eq!(cursor, 1);
    }
}
