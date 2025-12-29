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
use crate::draw_ops::{DrawCtxAccess, DrawOps};
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis};
use crate::text_layout::build_text_lines;
use std::cell::RefCell;

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

/// Controls how text should wrap when rendered inside a container.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextWrap {
    /// Render text on a single line without wrapping.
    None,
    /// Wrap text at word boundaries when it exceeds the cell width.
    Word,
}

/// Draw commands recorded during container traversal.
pub(crate) enum Command {
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
    pub(crate) style: Rc<Style>,
    /// Human-readable name for the container.
    pub(crate) name: String,
    /// Outer rectangle including frame and title.
    pub(crate) rect: Recti,
    /// Inner rectangle excluding frame/title.
    pub(crate) body: Recti,
    /// Size of the content region based on layout traversal.
    pub(crate) content_size: Vec2i,
    /// Accumulated scroll offset.
    pub(crate) scroll: Vec2i,
    /// Z-index used to order overlapping windows.
    pub(crate) zindex: i32,
    /// Recorded draw commands for this frame.
    pub(crate) command_list: Vec<Command>,
    /// Stack of clip rectangles applied while drawing.
    pub(crate) clip_stack: Vec<Recti>,
    pub(crate) layout: LayoutManager,
    /// ID of the widget currently hovered, if any.
    pub(crate) hover: Option<Id>,
    /// ID of the widget currently focused, if any.
    pub(crate) focus: Option<Id>,
    /// Tracks whether focus changed this frame.
    pub(crate) updated_focus: bool,
    /// Internal state for the vertical scrollbar.
    pub(crate) scrollbar_y_state: Internal,
    /// Internal state for the horizontal scrollbar.
    pub(crate) scrollbar_x_state: Internal,
    /// Shared access to the input state.
    pub(crate) input: Rc<RefCell<Input>>,
    /// Cached per-frame input snapshot for widgets that need it.
    input_snapshot: Option<Rc<InputSnapshot>>,
    /// Whether this container is the current hover root.
    pub(crate) in_hover_root: bool,
    /// Tracks whether a popup was just opened this frame to avoid instant auto-close.
    pub(crate) popup_just_opened: bool,
    pending_scroll: Option<Vec2i>,
    /// Determines whether container scrollbars and scroll consumption are enabled.
    scroll_enabled: bool,

    panels: Vec<ContainerHandle>,
}

impl Container {
    pub(crate) fn new(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>) -> Self {
        Self {
            name: name.to_string(),
            style,
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
            scrollbar_y_state: Internal::new("!scrollbary"),
            scrollbar_x_state: Internal::new("!scrollbarx"),
            popup_just_opened: false,
            in_hover_root: false,
            input: input,
            input_snapshot: None,
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
        self.input_snapshot = None;
        self.pending_scroll = None;
        self.scroll_enabled = true;
    }

    pub(crate) fn prepare(&mut self) {
        self.command_list.clear();
        assert!(self.clip_stack.len() == 0);
        self.panels.clear();
        self.input_snapshot = None;
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

    fn draw_ctx(&mut self) -> DrawCtx<'_> {
        DrawCtx::new(&mut self.command_list, &mut self.clip_stack, self.style.as_ref(), &self.atlas)
    }

    /// Pushes a new clip rectangle combined with the previous clip.
    pub fn push_clip_rect(&mut self, rect: Recti) { DrawOps::push_clip_rect(self, rect); }

    /// Restores the previous clip rectangle from the stack.
    pub fn pop_clip_rect(&mut self) { DrawOps::pop_clip_rect(self); }

    /// Returns the active clip rectangle, or an unclipped rect when the stack is empty.
    pub fn get_clip_rect(&mut self) -> Recti { DrawOps::current_clip_rect(self) }

    /// Determines whether `r` is fully visible, partially visible, or completely clipped.
    pub fn check_clip(&mut self, r: Recti) -> Clip { DrawOps::check_clip(self, r) }

    /// Adjusts the current clip rectangle.
    pub fn set_clip(&mut self, rect: Recti) { DrawOps::set_clip(self, rect); }

    /// Manually updates which widget owns focus.
    pub fn set_focus(&mut self, id: Option<Id>) {
        self.focus = id;
        self.updated_focus = true;
    }

    /// Records a filled rectangle draw command.
    pub fn draw_rect(&mut self, rect: Recti, color: Color) { DrawOps::draw_rect(self, rect, color); }

    /// Records a rectangle outline.
    pub fn draw_box(&mut self, r: Recti, color: Color) { DrawOps::draw_box(self, r, color); }

    /// Records a text draw command.
    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        DrawOps::draw_text(self, font, str, pos, color);
    }

    /// Records an icon draw command.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) { DrawOps::draw_icon(self, id, rect, color); }

    /// Records a slot draw command.
    pub fn draw_slot(&mut self, id: SlotId, rect: Recti, color: Color) {
        DrawOps::push_image(self, Image::Slot(id), rect, color);
    }

    /// Records a slot redraw that uses a callback to fill pixels.
    pub fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        DrawOps::draw_slot_with_function(self, id, rect, color, f);
    }

    #[inline(never)]
    /// Draws multi-line text within the container without wrapping.
    pub fn text(&mut self, text: &str) { self.text_with_wrap(text, TextWrap::None); }

    #[inline(never)]
    /// Draws multi-line text within the container using the provided wrapping mode.
    /// The block is rendered inside an internal column with zero spacing so consecutive
    /// lines sit back-to-back while the outer widget spacing/padding remains intact.
    pub fn text_with_wrap(&mut self, text: &str, wrap: TextWrap) {
        if text.is_empty() {
            return;
        }
        let style = self.style.as_ref();
        let font = style.font;
        let color = style.colors[ControlColor::Text as usize];
        let line_height = self.atlas.get_font_height(font) as i32;
        let baseline = self.atlas.get_font_baseline(font);
        let saved_spacing = self.layout.style.spacing;
        self.layout.style.spacing = 0;
        self.column(|ui| {
            ui.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Fixed(line_height));
            let first_rect = ui.layout.next();
            let max_width = first_rect.width;
            let mut lines = build_text_lines(text, wrap, max_width, font, &ui.atlas);
            if text.ends_with('\n') {
                if let Some(last) = lines.last() {
                    if last.start == text.len() && last.end == text.len() {
                        lines.pop();
                    }
                }
            }
            for (idx, line) in lines.iter().enumerate() {
                let r = if idx == 0 { first_rect } else { ui.layout.next() };
                let line_top = Self::baseline_aligned_top(r, line_height, baseline);
                let slice = &text[line.start..line.end];
                if !slice.is_empty() {
                    ui.draw_text(font, slice, vec2(r.x, line_top), color);
                }
            }
        });
        self.layout.style.spacing = saved_spacing;
    }

    /// Draws a frame and optional border using the specified color.
    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) { DrawOps::draw_frame(self, rect, colorid); }

    /// Draws a widget background, applying hover/focus accents when needed.
    pub fn draw_widget_frame(&mut self, id: Id, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let focused = self.focus == Some(id);
        let hovered = self.hover == Some(id);
        DrawOps::draw_widget_frame(self, focused, hovered, rect, colorid, opt);
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
        DrawOps::draw_frame(self, rect, colorid);
    }

    #[inline(never)]
    /// Draws widget text with the appropriate alignment flags.
    pub fn draw_control_text(&mut self, str: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        DrawOps::draw_control_text(self, str, rect, colorid, opt);
    }

    /// Returns `true` if the cursor is inside `rect` and the container owns the hover root.
    pub fn mouse_over(&mut self, rect: Recti, in_hover_root: bool) -> bool {
        let clip_rect = self.get_clip_rect();
        rect.contains(&self.input.borrow().mouse_pos) && clip_rect.contains(&self.input.borrow().mouse_pos) && in_hover_root
    }

    fn update_control_with_opts(&mut self, id: Id, rect: Recti, opt: WidgetOption, bopt: WidgetBehaviourOption) -> ControlState {
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

    #[inline(never)]
    /// Updates hover/focus state for the widget described by `id` and optionally consumes scroll.
    pub fn update_control<W: Widget>(&mut self, id: Id, rect: Recti, state: &W) -> ControlState {
        self.update_control_with_opts(id, rect, *state.widget_opt(), *state.behaviour_opt())
    }

    fn snapshot_input(&mut self) -> Rc<InputSnapshot> {
        if let Some(snapshot) = &self.input_snapshot {
            return snapshot.clone();
        }

        let input = self.input.borrow();
        let snapshot = Rc::new(InputSnapshot {
            mouse_pos: input.mouse_pos,
            mouse_delta: input.mouse_delta,
            mouse_down: input.mouse_down,
            mouse_pressed: input.mouse_pressed,
            key_mods: input.key_down,
            key_pressed: input.key_pressed,
            key_codes: input.key_code_down,
            key_code_pressed: input.key_code_pressed,
            text_input: input.input_text.clone(),
        });
        self.input_snapshot = Some(snapshot.clone());
        snapshot
    }

    pub(crate) fn widget_ctx(&mut self, id: Id, rect: Recti, input: Option<Rc<InputSnapshot>>) -> WidgetCtx<'_> {
        WidgetCtx::new(
            id,
            rect,
            &mut self.command_list,
            &mut self.clip_stack,
            self.style.as_ref(),
            &self.atlas,
            &mut self.focus,
            &mut self.updated_focus,
            self.in_hover_root,
            input,
        )
    }

    fn run_widget<W: Widget>(
        &mut self,
        state: &mut W,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> (ControlState, ResourceState) {
        let id = state.get_id();
        let control = self.update_control_with_opts(id, rect, opt, bopt);
        let mut ctx = self.widget_ctx(id, rect, input);
        let res = state.handle(&mut ctx, &control);
        (control, res)
    }

    fn handle_widget<W: Widget>(&mut self, state: &mut W, input: Option<Rc<InputSnapshot>>) -> ResourceState {
        let rect = self.layout.next();
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let (_, res) = self.run_widget(state, rect, input, opt, bopt);
        res
    }

    fn handle_widget_in_rect<W: Widget>(
        &mut self,
        state: &mut W,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> ResourceState {
        let (_, res) = self.run_widget(state, rect, input, opt, bopt);
        res
    }

    /// Resets transient per-frame state after widgets have been processed.
    pub fn finish(&mut self) {
        if !self.updated_focus {
            self.focus = None;
        }
        self.updated_focus = false;
    }

    /// Returns the outer container rectangle.
    pub fn rect(&self) -> Recti { self.rect }

    /// Sets the outer container rectangle.
    pub fn set_rect(&mut self, rect: Recti) { self.rect = rect; }

    /// Returns the inner container body rectangle.
    pub fn body(&self) -> Recti { self.body }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i { self.scroll }

    /// Sets the current scroll offset.
    pub fn set_scroll(&mut self, scroll: Vec2i) { self.scroll = scroll; }

    /// Returns the content size derived from layout traversal.
    pub fn content_size(&self) -> Vec2i { self.content_size }

    fn node_scope<F: FnOnce(&mut Self)>(&mut self, state: &mut Node, indent: bool, f: F) -> NodeStateValue {
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let r = self.layout.next();
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let _ = self.handle_widget_in_rect(state, r, None, opt, bopt);
        if state.state.is_expanded() {
            if indent {
                let indent_size = self.style.as_ref().indent;
                self.layout.adjust_indent(indent_size);
                f(self);
                self.layout.adjust_indent(-indent_size);
            } else {
                f(self);
            }
        }
        state.state
    }

    /// Builds a collapsible header row that executes `f` when expanded.
    pub fn header<F: FnOnce(&mut Self)>(&mut self, state: &mut Node, f: F) -> NodeStateValue {
        self.node_scope(state, false, f)
    }

    /// Builds a tree node with automatic indentation while expanded.
    pub fn treenode<F: FnOnce(&mut Self)>(&mut self, state: &mut Node, f: F) -> NodeStateValue {
        self.node_scope(state, true, f)
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
        let padding = self.style.as_ref().padding * 2;
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
        let (scrollbar_size, padding, thumb_size) = {
            let style = self.style.as_ref();
            (style.scrollbar_size, style.padding, style.thumb_size)
        };
        let sz = scrollbar_size;
        let mut cs: Vec2i = self.content_size;
        cs.x += padding * 2;
        cs.y += padding * 2;
        let base_body = *body;
        self.push_clip_rect(body.clone());
        if cs.y > base_body.height {
            body.width -= sz;
        }
        if cs.x > base_body.width {
            body.height -= sz;
        }
        let body = *body;
        let maxscroll = scrollbar_max_scroll(cs.y, body.height);
        if maxscroll > 0 && body.height > 0 {
            let id: Id = self.scrollbar_y_state.get_id();
            let base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
            let control = self.update_control_with_opts(id, base, self.scrollbar_y_state.opt, self.scrollbar_y_state.bopt);
            {
                let mut ctx = WidgetCtx::new(
                    id,
                    base,
                    &mut self.command_list,
                    &mut self.clip_stack,
                    self.style.as_ref(),
                    &self.atlas,
                    &mut self.focus,
                    &mut self.updated_focus,
                    self.in_hover_root,
                    None,
                );
                let _ = self.scrollbar_y_state.handle(&mut ctx, &control);
            }
            if control.active {
                let delta = scrollbar_drag_delta(ScrollAxis::Vertical, self.input.borrow().mouse_delta, cs.y, base);
                self.scroll.y += delta;
            }
            self.scroll.y = Self::clamp(self.scroll.y, 0, maxscroll);
            self.draw_frame(base, ControlColor::ScrollBase);
            let thumb = scrollbar_thumb(ScrollAxis::Vertical, base, body.height, cs.y, self.scroll.y, thumb_size);
            self.draw_frame(thumb, ControlColor::ScrollThumb);
        } else {
            self.scroll.y = 0;
        }
        let maxscroll_0 = scrollbar_max_scroll(cs.x, body.width);
        if maxscroll_0 > 0 && body.width > 0 {
            let id_0: Id = self.scrollbar_x_state.get_id();
            let base_0 = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
            let control = self.update_control_with_opts(id_0, base_0, self.scrollbar_x_state.opt, self.scrollbar_x_state.bopt);
            {
                let mut ctx = WidgetCtx::new(
                    id_0,
                    base_0,
                    &mut self.command_list,
                    &mut self.clip_stack,
                    self.style.as_ref(),
                    &self.atlas,
                    &mut self.focus,
                    &mut self.updated_focus,
                    self.in_hover_root,
                    None,
                );
                let _ = self.scrollbar_x_state.handle(&mut ctx, &control);
            }
            if control.active {
                let delta = scrollbar_drag_delta(ScrollAxis::Horizontal, self.input.borrow().mouse_delta, cs.x, base_0);
                self.scroll.x += delta;
            }
            self.scroll.x = Self::clamp(self.scroll.x, 0, maxscroll_0);
            self.draw_frame(base_0, ControlColor::ScrollBase);
            let thumb_0 = scrollbar_thumb(ScrollAxis::Horizontal, base_0, body.width, cs.x, self.scroll.x, thumb_size);
            self.draw_frame(thumb_0, ControlColor::ScrollThumb);
        } else {
            self.scroll.x = 0;
        }
        self.pop_clip_rect();
    }

    /// Configures layout state for the container's client area, handling scrollbars when necessary.
    pub fn push_container_body(&mut self, body: Recti, _opt: ContainerOption, bopt: WidgetBehaviourOption) {
        let mut body = body;
        self.scroll_enabled = !bopt.is_no_scroll();
        if self.scroll_enabled {
            self.scrollbars(&mut body);
        }
        let (layout_padding, style_padding, font, style_clone) = {
            let style = self.style.as_ref();
            (-style.padding, style.padding, style.font, style.clone())
        };
        let scroll = self.scroll;
        self.layout.reset(expand_rect(body, layout_padding), scroll);
        self.layout.style = style_clone;
        let font_height = self.atlas.get_font_height(font) as i32;
        let vertical_pad = Self::vertical_text_padding(style_padding);
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
        container.style = self.style.clone();

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
    pub fn set_style(&mut self, style: Style) { self.style = Rc::new(style); }

    /// Returns a copy of the current style.
    pub fn get_style(&self) -> Style { (*self.style).clone() }

    /// Displays static text using the default text color.
    pub fn label(&mut self, text: &str) {
        let layout = self.layout.next();
        self.draw_control_text(text, layout, ControlColor::Text, WidgetOption::NONE);
    }

    #[inline(never)]
    /// Draws a button using the provided persistent state.
    pub fn button(&mut self, state: &mut Button) -> ResourceState {
        self.handle_widget(state, None)
    }

    /// Renders a list entry that only highlights while hovered or active.
    pub fn list_item(&mut self, state: &mut ListItem) -> ResourceState {
        self.handle_widget(state, None)
    }

    #[inline(never)]
    /// Shim for list boxes that only fills on hover or click.
    pub fn list_box(&mut self, state: &mut ListBox) -> ResourceState {
        self.handle_widget(state, None)
    }

    #[inline(never)]
    /// Draws the combo box header, clamps the selected index, and returns the popup anchor.
    /// The caller is responsible for opening the popup and updating `state.selected` from its list.
    pub fn combo_box<S: AsRef<str>>(&mut self, state: &mut Combo, items: &[S]) -> (Recti, bool, ResourceState) {
        state.update_items(items);
        let header = self.layout.next();
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let res = self.handle_widget_in_rect(state, header, None, opt, bopt);
        let header_clicked = res.is_submitted();
        let anchor = rect(header.x, header.y + header.height, header.width, 1);
        (anchor, header_clicked, res)
    }


    #[inline(never)]
    /// Draws a checkbox labeled with `label` and toggles `state` when clicked.
    pub fn checkbox(&mut self, state: &mut Checkbox) -> ResourceState {
        self.handle_widget(state, None)
    }

    #[inline(never)]
    fn input_to_mouse_event(&self, control: &ControlState, input: &InputSnapshot, rect: Recti) -> MouseEvent {
        let orig = Vec2i::new(rect.x, rect.y);

        let prev_pos = input.mouse_pos - input.mouse_delta - orig;
        let curr_pos = input.mouse_pos - orig;
        let mouse_down = input.mouse_down;
        let mouse_pressed = input.mouse_pressed;

        if control.focused && mouse_down.is_left() {
            return MouseEvent::Drag { prev_pos, curr_pos };
        }

        if control.hovered && mouse_pressed.is_left() {
            return MouseEvent::Click(curr_pos);
        }

        if control.hovered {
            return MouseEvent::Move(curr_pos);
        }
        MouseEvent::None
    }

    #[inline(never)]
    /// Allocates a widget cell and hands rendering control to user code.
    pub fn custom_render_widget<F: FnMut(Dimensioni, &CustomRenderArgs) + 'static>(
        &mut self,
        state: &mut Custom,
        f: F,
    ) {
        let rect = self.layout.next();
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let (control, _) = self.run_widget(state, rect, None, opt, bopt);

        let input = self.snapshot_input();
        let input_ref = input.as_ref();
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
            widget_opt: state.opt,
            behaviour_opt: state.bopt,
            key_mods,
            key_codes,
            text_input,
        };
        self.command_list.push(Command::CustomRender(cra, Box::new(f)));
    }

    /// Draws a textbox in the provided rectangle using the supplied state.
    pub fn textbox_raw(&mut self, state: &mut Textbox, r: Recti) -> ResourceState {
        let input = self.snapshot_input();
        let opt = state.opt | WidgetOption::HOLD_FOCUS;
        self.handle_widget_in_rect(state, r, Some(input), opt, state.bopt)
    }

    /// Draws a textbox using the next available layout cell.
    pub fn textbox_ex(&mut self, state: &mut Textbox) -> ResourceState {
        let r: Recti = self.layout.next();
        self.textbox_raw(state, r)
    }

    /// Draws a multi-line text area in the provided rectangle using the supplied state.
    pub fn textarea_raw(&mut self, state: &mut TextArea, r: Recti) -> ResourceState {
        let input = self.snapshot_input();
        let opt = state.opt | WidgetOption::HOLD_FOCUS;
        self.handle_widget_in_rect(state, r, Some(input), opt, state.bopt)
    }

    /// Draws a multi-line text area using the next available layout cell.
    pub fn textarea_ex(&mut self, state: &mut TextArea) -> ResourceState {
        let r: Recti = self.layout.next();
        self.textarea_raw(state, r)
    }

    #[inline(never)]
    /// Draws a horizontal slider bound to `state`.
    pub fn slider_ex(&mut self, state: &mut Slider) -> ResourceState {
        let rect = self.layout.next();
        let mut opt = state.opt;
        if state.edit.editing {
            opt |= WidgetOption::HOLD_FOCUS;
        }
        let input = self.snapshot_input();
        self.handle_widget_in_rect(state, rect, Some(input), opt, state.bopt)
    }

    #[inline(never)]
    /// Draws a numeric input that can be edited via keyboard or by dragging.
    pub fn number_ex(&mut self, state: &mut Number) -> ResourceState {
        let rect = self.layout.next();
        let mut opt = state.opt;
        if state.edit.editing {
            opt |= WidgetOption::HOLD_FOCUS;
        }
        let input = self.snapshot_input();
        self.handle_widget_in_rect(state, rect, Some(input), opt, state.bopt)
    }
}

impl DrawCtxAccess for Container {
    fn with_draw_ctx<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut DrawCtx<'_>) -> R,
    {
        let mut draw = self.draw_ctx();
        f(&mut draw)
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
        let mut container = Container::new("test", atlas, Rc::new(Style::default()), input);
        container.in_hover_root = true;
        container.push_container_body(rect(0, 0, 100, 30), ContainerOption::NONE, WidgetBehaviourOption::NONE);
        container
    }

    #[test]
    fn scrollbars_use_current_body() {
        let mut container = make_container();
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        container.style = Rc::new(style);

        container.body = rect(0, 0, 1, 1);
        container.content_size = Vec2i::new(0, 0);

        let mut body = rect(0, 0, 100, 100);
        container.scrollbars(&mut body);

        assert_eq!(body.width, 100);
        assert_eq!(body.height, 100);
    }

    #[test]
    fn scrollbars_shrink_body_when_needed() {
        let mut container = make_container();
        let mut style = Style::default();
        style.padding = 0;
        style.scrollbar_size = 10;
        container.style = Rc::new(style);

        container.content_size = Vec2i::new(200, 200);

        let mut body = rect(0, 0, 100, 100);
        container.scrollbars(&mut body);

        assert_eq!(body.width, 90);
        assert_eq!(body.height, 90);
    }

    #[test]
    fn textbox_left_moves_over_multibyte() {
        let mut container = make_container();
        let input = container.input.clone();
        let mut state = Textbox::new("a\u{1F600}b");
        let id = state.get_id();
        container.set_focus(Some(id));
        state.cursor = 5;

        input.borrow_mut().keydown_code(KeyCode::LEFT);
        let rect = container.layout.next();
        let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.bopt);
        let control = container.update_control(id, rect, &control_state);
        let input = container.snapshot_input();
        let mut ctx = container.widget_ctx(id, rect, Some(input));
        state.handle(&mut ctx, &control);

        let cursor = state.cursor;
        assert_eq!(cursor, 1);
    }

    #[test]
    fn textbox_backspace_removes_multibyte() {
        let mut container = make_container();
        let input = container.input.clone();
        let mut state = Textbox::new("a\u{1F600}b");
        let id = state.get_id();
        container.set_focus(Some(id));
        state.cursor = 5;

        input.borrow_mut().keydown(KeyMode::BACKSPACE);
        let rect = container.layout.next();
        let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.bopt);
        let control = container.update_control(id, rect, &control_state);
        let input = container.snapshot_input();
        let mut ctx = container.widget_ctx(id, rect, Some(input));
        state.handle(&mut ctx, &control);

        let cursor = state.cursor;
        assert_eq!(state.buf, "ab");
        assert_eq!(cursor, 1);
    }
}
