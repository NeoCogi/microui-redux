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
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis};
use crate::text_layout::build_text_lines;
use crate::widget_tree::{RuntimeTreeNode, RuntimeTreeNodeKind, TreeCustomRender, WidgetHandle, WidgetStateHandleDyn};
use std::cell::RefCell;
use std::hash::{Hash, Hasher};

macro_rules! widget_layout {
    ($(#[$meta:meta])* $name:ident, $state:ty, $builder:expr) => {
        $(#[$meta])*
        pub fn $name(&mut self, results: &mut FrameResults, state: &mut $state) -> ResourceState {
            let rect = self.next_widget_rect(state);
            ($builder)(self, results, state, rect)
        }
    };
    ($(#[$meta:meta])* $name:ident, $state:ty) => {
        $(#[$meta])*
        pub fn $name(&mut self, results: &mut FrameResults, state: &mut $state) -> ResourceState {
            self.handle_widget(results, state, None)
        }
    };
}

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
    fn default() -> Self {
        Command::None
    }
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
    pub(crate) hover: Option<WidgetId>,
    /// ID of the widget currently focused, if any.
    pub(crate) focus: Option<WidgetId>,
    /// Child container that currently owns pointer routing inside this container.
    hover_root_child: Option<ContainerId>,
    /// Rectangle occupied by the child container that currently owns pointer routing.
    hover_root_child_rect: Option<Recti>,
    /// Child container selected to own pointer routing on the next frame.
    next_hover_root_child: Option<ContainerId>,
    /// Rectangle for the child container selected to own pointer routing on the next frame.
    next_hover_root_child_rect: Option<Recti>,
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
    /// Previous/current frame cache for tree node geometry and interaction state.
    tree_cache: WidgetTreeCache,

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
            hover_root_child: None,
            hover_root_child_rect: None,
            next_hover_root_child: None,
            next_hover_root_child_rect: None,
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
            tree_cache: WidgetTreeCache::default(),

            panels: Default::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.hover = None;
        self.focus = None;
        self.hover_root_child = None;
        self.hover_root_child_rect = None;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.updated_focus = false;
        self.in_hover_root = false;
        self.input_snapshot = None;
        self.pending_scroll = None;
        self.scroll_enabled = true;
        self.tree_cache.clear();
    }

    pub(crate) fn prepare(&mut self) {
        self.command_list.clear();
        assert!(self.clip_stack.len() == 0);
        self.panels.clear();
        self.input_snapshot = None;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.pending_scroll = None;
        self.scroll_enabled = true;
        self.tree_cache.begin_frame();
    }

    pub(crate) fn seed_pending_scroll(&mut self, delta: Option<Vec2i>) {
        self.pending_scroll = delta;
    }

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
    pub fn push_clip_rect(&mut self, rect: Recti) {
        let mut draw = self.draw_ctx();
        draw.push_clip_rect(rect);
    }

    /// Restores the previous clip rectangle from the stack.
    pub fn pop_clip_rect(&mut self) {
        let mut draw = self.draw_ctx();
        draw.pop_clip_rect();
    }

    /// Returns the active clip rectangle, or an unclipped rect when the stack is empty.
    pub fn get_clip_rect(&mut self) -> Recti {
        self.draw_ctx().current_clip_rect()
    }

    /// Determines whether `r` is fully visible, partially visible, or completely clipped.
    pub fn check_clip(&mut self, r: Recti) -> Clip {
        self.draw_ctx().check_clip(r)
    }

    /// Adjusts the current clip rectangle.
    pub fn set_clip(&mut self, rect: Recti) {
        let mut draw = self.draw_ctx();
        draw.set_clip(rect);
    }

    /// Manually updates which widget owns focus.
    pub fn set_focus(&mut self, widget_id: Option<WidgetId>) {
        self.focus = widget_id;
        self.updated_focus = true;
    }

    /// Records a filled rectangle draw command.
    pub fn draw_rect(&mut self, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_rect(rect, color);
    }

    /// Records a rectangle outline.
    pub fn draw_box(&mut self, r: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_box(r, color);
    }

    /// Records a text draw command.
    pub fn draw_text(&mut self, font: FontId, str: &str, pos: Vec2i, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_text(font, str, pos, color);
    }

    /// Records an icon draw command.
    pub fn draw_icon(&mut self, id: IconId, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.draw_icon(id, rect, color);
    }

    /// Records a slot draw command.
    pub fn draw_slot(&mut self, id: SlotId, rect: Recti, color: Color) {
        let mut draw = self.draw_ctx();
        draw.push_image(Image::Slot(id), rect, color);
    }

    /// Records a slot redraw that uses a callback to fill pixels.
    pub fn draw_slot_with_function(&mut self, id: SlotId, rect: Recti, color: Color, f: Rc<dyn Fn(usize, usize) -> Color4b>) {
        let mut draw = self.draw_ctx();
        draw.draw_slot_with_function(id, rect, color, f);
    }

    #[inline(never)]
    /// Draws multi-line text within the container without wrapping.
    pub fn text(&mut self, text: &str) {
        self.text_with_wrap(text, TextWrap::None);
    }

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
    pub fn draw_frame(&mut self, rect: Recti, colorid: ControlColor) {
        let mut draw = self.draw_ctx();
        draw.draw_frame(rect, colorid);
    }

    /// Draws a widget background, applying hover/focus accents when needed.
    pub fn draw_widget_frame(&mut self, widget_id: WidgetId, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let focused = self.focus == Some(widget_id);
        let hovered = self.hover == Some(widget_id);
        let mut draw = self.draw_ctx();
        draw.draw_widget_frame(focused, hovered, rect, colorid, opt);
    }

    /// Draws a container frame, skipping rendering when the option disables it.
    pub fn draw_container_frame(&mut self, widget_id: WidgetId, rect: Recti, mut colorid: ControlColor, opt: ContainerOption) {
        if opt.has_no_frame() {
            return;
        }

        if self.focus == Some(widget_id) {
            colorid.focus()
        } else if self.hover == Some(widget_id) {
            colorid.hover()
        }
        let mut draw = self.draw_ctx();
        draw.draw_frame(rect, colorid);
    }

    #[inline(never)]
    /// Draws widget text with the appropriate alignment flags.
    pub fn draw_control_text(&mut self, str: &str, rect: Recti, colorid: ControlColor, opt: WidgetOption) {
        let mut draw = self.draw_ctx();
        draw.draw_control_text(str, rect, colorid, opt);
    }

    fn hit_test_rect(&mut self, rect: Recti, in_hover_root: bool) -> bool {
        let clip_rect = self.get_clip_rect();
        rect.contains(&self.input.borrow().mouse_pos) && clip_rect.contains(&self.input.borrow().mouse_pos) && in_hover_root
    }

    fn pointer_blocked_by_child(&self) -> bool {
        match self.hover_root_child_rect {
            Some(rect) => rect.contains(&self.input.borrow().mouse_pos),
            None => false,
        }
    }

    /// Returns `true` if the cursor is inside `rect` and the container can currently own hover there.
    pub fn mouse_over(&mut self, rect: Recti, in_hover_root: bool) -> bool {
        self.hit_test_rect(rect, in_hover_root && !self.pointer_blocked_by_child())
    }

    fn update_control_with_opts(&mut self, widget_id: WidgetId, rect: Recti, opt: WidgetOption, bopt: WidgetBehaviourOption) -> ControlState {
        let in_hover_root = self.in_hover_root;
        let mouseover = self.mouse_over(rect, in_hover_root);
        if self.focus == Some(widget_id) {
            // is this the same ID of the focused widget? by default set it to true unless otherwise
            self.updated_focus = true;
        }
        if opt.is_not_interactive() {
            return ControlState::default();
        }
        if mouseover && self.input.borrow().mouse_down.is_none() {
            self.hover = Some(widget_id);
        }
        if self.focus == Some(widget_id) {
            let should_clear_focus = {
                let input = self.input.borrow();
                let pressed_outside = !input.mouse_pressed.is_none() && !mouseover;
                let released_without_hold_focus = input.mouse_down.is_none() && !opt.is_holding_focus();
                pressed_outside || released_without_hold_focus
            };
            if should_clear_focus {
                self.set_focus(None);
            }
        }
        if self.hover == Some(widget_id) {
            if !mouseover {
                self.hover = None;
            } else if !self.input.borrow().mouse_pressed.is_none() {
                self.set_focus(Some(widget_id));
            }
        }

        let mut scroll = None;
        if bopt.is_grab_scroll() && self.hover == Some(widget_id) {
            if let Some(delta) = self.pending_scroll {
                if delta.x != 0 || delta.y != 0 {
                    self.pending_scroll = None;
                    scroll = Some(delta);
                }
            }
        }

        if self.focus == Some(widget_id) {
            let mouse_pos = self.input.borrow().mouse_pos;
            let origin = vec2(self.body.x, self.body.y);
            self.input.borrow_mut().rel_mouse_pos = mouse_pos - origin;
        }

        let focused = self.focus == Some(widget_id);
        let hovered = self.hover == Some(widget_id);
        let (clicked, active) = {
            let input = self.input.borrow();
            (focused && input.mouse_pressed.is_left(), focused && input.mouse_down.is_left())
        };

        ControlState {
            hovered,
            focused,
            clicked,
            active,
            scroll_delta: scroll,
        }
    }

    #[inline(never)]
    /// Updates hover/focus state for the widget described by `widget_id` and optionally consumes scroll.
    pub fn update_control<W: Widget + ?Sized>(&mut self, widget_id: WidgetId, rect: Recti, state: &W) -> ControlState {
        self.update_control_with_opts(widget_id, rect, *state.widget_opt(), *state.behaviour_opt())
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

    pub(crate) fn widget_ctx(&mut self, widget_id: WidgetId, rect: Recti, input: Option<Rc<InputSnapshot>>) -> WidgetCtx<'_> {
        WidgetCtx::new(
            widget_id,
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

    fn run_widget<W: Widget + ?Sized>(
        &mut self,
        results: &mut FrameResults,
        state: &mut W,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> (ControlState, ResourceState) {
        let widget_id = widget_id_of(state);
        let control = self.update_control_with_opts(widget_id, rect, opt, bopt);
        let mut ctx = self.widget_ctx(widget_id, rect, input);
        let res = state.handle(&mut ctx, &control);
        results.record(widget_id, res);
        (control, res)
    }

    fn next_widget_rect<W: Widget + ?Sized>(&mut self, state: &W) -> Recti {
        // Widget helpers measure before placing so Auto rows can follow each widget's intrinsic size.
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = state.preferred_size(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_preferred(preferred)
    }

    fn handle_widget<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, state: &mut W, input: Option<Rc<InputSnapshot>>) -> ResourceState {
        let rect = self.next_widget_rect(state);
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let (_, res) = self.run_widget(results, state, rect, input, opt, bopt);
        res
    }

    fn handle_widget_in_rect<W: Widget + ?Sized>(
        &mut self,
        results: &mut FrameResults,
        state: &mut W,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> ResourceState {
        let (_, res) = self.run_widget(results, state, rect, input, opt, bopt);
        res
    }

    fn handle_widget_raw<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, state: &mut W) -> ResourceState {
        let rect = self.next_widget_rect(state);
        let opt = state.effective_widget_opt();
        let bopt = state.effective_behaviour_opt();
        let input = if state.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let (_, res) = self.run_widget(results, state, rect, input, opt, bopt);
        res
    }

    fn next_widget_rect_dyn(&mut self, widget: &dyn WidgetStateHandleDyn) -> Recti {
        let body = self.layout.current_body();
        let avail = Dimensioni::new(body.width.max(0), body.height.max(0));
        let preferred = widget.preferred_size(self.style.as_ref(), &self.atlas, avail);
        self.layout.next_with_preferred(preferred)
    }

    fn run_widget_dyn(
        &mut self,
        results: &mut FrameResults,
        widget: &dyn WidgetStateHandleDyn,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> (ControlState, ResourceState) {
        let widget_id = widget.widget_id();
        let control = self.update_control_with_opts(widget_id, rect, opt, bopt);
        let mut ctx = self.widget_ctx(widget_id, rect, input);
        let res = widget.handle(&mut ctx, &control);
        results.record(widget_id, res);
        (control, res)
    }

    fn next_widget_rect_handle<W: Widget>(&mut self, handle: &WidgetHandle<W>) -> Recti {
        let state = handle.borrow();
        self.next_widget_rect(&*state)
    }

    fn run_widget_handle<W: Widget>(
        &mut self,
        results: &mut FrameResults,
        handle: &WidgetHandle<W>,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
    ) -> (ControlState, ResourceState) {
        let widget_id = {
            let state = handle.borrow();
            widget_id_of(&*state)
        };
        let control = self.update_control_with_opts(widget_id, rect, opt, bopt);
        let mut ctx = self.widget_ctx(widget_id, rect, input);
        let res = {
            let mut state = handle.borrow_mut();
            state.handle(&mut ctx, &control)
        };
        results.record(widget_id, res);
        (control, res)
    }

    /// Resets transient per-frame state after widgets have been processed.
    pub fn finish(&mut self) {
        for panel in &mut self.panels {
            panel.finish();
        }
        if !self.updated_focus {
            self.focus = None;
        }
        self.updated_focus = false;
        self.hover_root_child = self.next_hover_root_child;
        self.hover_root_child_rect = self.next_hover_root_child_rect;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.tree_cache.finish_frame();
    }

    /// Returns the outer container rectangle.
    pub fn rect(&self) -> Recti {
        self.rect
    }

    /// Sets the outer container rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.rect = rect;
    }

    /// Returns the inner container body rectangle.
    pub fn body(&self) -> Recti {
        self.body
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.scroll
    }

    /// Sets the current scroll offset.
    pub fn set_scroll(&mut self, scroll: Vec2i) {
        self.scroll = scroll;
    }

    /// Returns the content size derived from layout traversal.
    pub fn content_size(&self) -> Vec2i {
        self.content_size
    }

    /// Returns the previous frame cache entry for `node_id`, if any.
    pub fn previous_node_state(&self, node_id: NodeId) -> Option<NodeCacheEntry> {
        self.tree_cache.prev(node_id).copied()
    }

    /// Returns the current frame cache entry for `node_id`, if any.
    pub fn current_node_state(&self, node_id: NodeId) -> Option<NodeCacheEntry> {
        self.tree_cache.current(node_id).copied()
    }

    fn run_node_scope(&mut self, results: &mut FrameResults, state: &mut Node) -> NodeStateValue {
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let r = self.next_widget_rect(state);
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let _ = self.handle_widget_in_rect(results, state, r, None, opt, bopt);
        state.state
    }

    fn node_scope<F: FnOnce(&mut Self)>(&mut self, results: &mut FrameResults, state: &mut Node, indent: bool, f: F) -> NodeStateValue {
        let node_state = self.run_node_scope(results, state);
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
        node_state
    }

    /// Builds a collapsible header row that executes `f` when expanded.
    pub fn header<F: FnOnce(&mut Self)>(&mut self, results: &mut FrameResults, state: &mut Node, f: F) -> NodeStateValue {
        self.node_scope(results, state, false, f)
    }

    /// Builds a tree node with automatic indentation while expanded.
    pub fn treenode<F: FnOnce(&mut Self)>(&mut self, results: &mut FrameResults, state: &mut Node, f: F) -> NodeStateValue {
        self.node_scope(results, state, true, f)
    }

    fn clamp(x: i32, a: i32, b: i32) -> i32 {
        min(b, max(a, x))
    }

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

    fn vertical_text_padding(padding: i32) -> i32 {
        max(1, padding / 2)
    }

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
            let scrollbar_y_id = widget_id_of(&self.scrollbar_y_state);
            let base = scrollbar_base(ScrollAxis::Vertical, body, scrollbar_size);
            let control = self.update_control_with_opts(scrollbar_y_id, base, self.scrollbar_y_state.opt, self.scrollbar_y_state.bopt);
            {
                let mut ctx = WidgetCtx::new(
                    scrollbar_y_id,
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
            let scrollbar_x_id = widget_id_of(&self.scrollbar_x_state);
            let base_0 = scrollbar_base(ScrollAxis::Horizontal, body, scrollbar_size);
            let control = self.update_control_with_opts(scrollbar_x_id, base_0, self.scrollbar_x_state.opt, self.scrollbar_x_state.bopt);
            {
                let mut ctx = WidgetCtx::new(
                    scrollbar_x_id,
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
        let panel_id = container_id_of(panel);
        if self.hit_test_rect(rect, self.in_hover_root) {
            self.next_hover_root_child = Some(panel_id);
            self.next_hover_root_child_rect = Some(rect);
        }
        let container = &mut panel.inner_mut();
        container.prepare();
        container.style = self.style.clone();

        container.rect = rect;
        if !opt.has_no_frame() {
            self.draw_frame(rect, ControlColor::PanelBG);
        }

        container.in_hover_root = self.in_hover_root && self.hover_root_child == Some(panel_id);
        if self.pending_scroll.is_some() && container.in_hover_root {
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
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.row(widths, height);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Sets the active row flow without restoring the previous flow.
    ///
    /// This matches legacy header/tree behavior where flow changes persist for
    /// subsequent layout calls in the current scope.
    pub fn set_row_flow(&mut self, widths: &[SizePolicy], height: SizePolicy) {
        self.layout.row(widths, height);
    }

    /// Temporarily overrides the layout with explicit column and row tracks and restores it after `f`.
    ///
    /// Widgets are emitted row-major within the provided track matrix.
    pub fn with_grid<F: FnOnce(&mut Self)>(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        self.layout.grid(widths, heights);
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Temporarily uses a vertical stack flow and restores the previous flow after `f` executes.
    ///
    /// Each `next_cell`/widget call in the scope gets a dedicated row using `height`.
    /// Width defaults to `Remainder(0)` so cells fill available horizontal space.
    pub fn stack<F: FnOnce(&mut Self)>(&mut self, height: SizePolicy, f: F) {
        self.stack_with_width_direction(SizePolicy::Remainder(0), height, StackDirection::TopToBottom, f);
    }

    /// Same as [`Container::stack`], but controls whether items are emitted top-down or bottom-up.
    pub fn stack_direction<F: FnOnce(&mut Self)>(&mut self, height: SizePolicy, direction: StackDirection, f: F) {
        self.stack_with_width_direction(SizePolicy::Remainder(0), height, direction, f);
    }

    /// Same as [`Container::stack`], but allows overriding the stack cell width policy.
    pub fn stack_with_width<F: FnOnce(&mut Self)>(&mut self, width: SizePolicy, height: SizePolicy, f: F) {
        self.stack_with_width_direction(width, height, StackDirection::TopToBottom, f);
    }

    /// Same as [`Container::stack_with_width`], but controls whether items are emitted top-down or bottom-up.
    pub fn stack_with_width_direction<F: FnOnce(&mut Self)>(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: F) {
        let snapshot = self.layout.snapshot_flow_state();
        if direction == StackDirection::TopToBottom {
            self.layout.stack(width, height);
        } else {
            self.layout.stack_with_direction(width, height, direction);
        }
        f(self);
        self.layout.restore_flow_state(snapshot);
    }

    /// Creates a nested column scope where each call to `next_cell` yields a single column.
    pub fn column<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.layout.begin_column();
        f(self);
        self.layout.end_column();
    }

    /// Temporarily applies horizontal indentation for nested scopes.
    pub fn with_indent<F: FnOnce(&mut Self)>(&mut self, delta: i32, f: F) {
        self.layout.adjust_indent(delta);
        f(self);
        self.layout.adjust_indent(-delta);
    }

    /// Returns the next raw layout cell rectangle.
    ///
    /// Unlike widget helper methods (`button`, `textbox`, etc.), this does not consult
    /// a widget's `preferred_size`; it uses only the current row/column policies.
    pub fn next_cell(&mut self) -> Recti {
        self.layout.next()
    }

    /// Runs a widget in an explicit rectangle.
    pub fn widget_in_rect<W: Widget + ?Sized>(&mut self, results: &mut FrameResults, widget: &mut W, rect: Recti) -> ResourceState {
        let opt = widget.effective_widget_opt();
        let bopt = widget.effective_behaviour_opt();
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        self.handle_widget_in_rect(results, widget, rect, input, opt, bopt)
    }

    fn run_tree_nodes(&mut self, results: &mut FrameResults, nodes: &[RuntimeTreeNode<'_>]) {
        for node in nodes {
            self.run_tree_node(results, node);
        }
    }

    fn pre_handle_tree_nodes(&mut self, nodes: &[RuntimeTreeNode<'_>]) {
        for node in nodes {
            self.pre_handle_tree_node(node);
        }
    }

    fn pre_handle_tree_node(&mut self, node: &RuntimeTreeNode<'_>) {
        match node.kind() {
            // Parent tree nodes use previous-frame geometry so they can update
            // structural state before the current frame's layout is computed.
            RuntimeTreeNodeKind::Header { state } | RuntimeTreeNodeKind::Tree { state } => {
                if self.cached_tree_click(node.id()) {
                    let mut state = state.borrow_mut();
                    state.state = if state.state.is_expanded() {
                        NodeStateValue::Closed
                    } else {
                        NodeStateValue::Expanded
                    };
                }
                if state.borrow().state.is_expanded() {
                    self.pre_handle_tree_nodes(node.children());
                }
            }
            RuntimeTreeNodeKind::Container { handle, .. } => {
                let mut handle = handle.clone();
                handle.with_mut(|container| {
                    container.pre_handle_tree_nodes(node.children());
                });
            }
            RuntimeTreeNodeKind::Row { .. } | RuntimeTreeNodeKind::Grid { .. } | RuntimeTreeNodeKind::Column | RuntimeTreeNodeKind::Stack { .. } => {
                self.pre_handle_tree_nodes(node.children())
            }
            RuntimeTreeNodeKind::Widget { .. } | RuntimeTreeNodeKind::CustomRender { .. } | RuntimeTreeNodeKind::Run { .. } => {}
        }
    }

    fn cached_tree_click(&mut self, node_id: NodeId) -> bool {
        let Some(cached) = self.tree_cache.prev(node_id).copied() else {
            return false;
        };

        self.mouse_over(cached.rect, self.in_hover_root) && self.input.borrow().mouse_pressed.is_left()
    }

    fn record_tree_node(&mut self, node_id: NodeId, state: NodeCacheEntry) {
        self.tree_cache.record(node_id, state);
    }

    fn record_tree_group_from_children(&mut self, node_id: NodeId, children: &[RuntimeTreeNode<'_>]) {
        let mut bounds: Option<Recti> = None;
        for child in children {
            if let Some(child_state) = self.tree_cache.current(child.id()) {
                bounds = Some(match bounds {
                    Some(existing_rect) => {
                        let min_x = existing_rect.x.min(child_state.rect.x);
                        let min_y = existing_rect.y.min(child_state.rect.y);
                        let max_x = (existing_rect.x + existing_rect.width).max(child_state.rect.x + child_state.rect.width);
                        let max_y = (existing_rect.y + existing_rect.height).max(child_state.rect.y + child_state.rect.height);
                        rect(min_x, min_y, max_x - min_x, max_y - min_y)
                    }
                    None => child_state.rect,
                });
            }
        }

        if let Some(rect) = bounds {
            self.record_tree_node(
                node_id,
                NodeCacheEntry {
                    rect,
                    body: rect,
                    content_size: vec2(rect.width, rect.height),
                    control: ControlState::default(),
                    result: ResourceState::NONE,
                },
            );
        }
    }

    fn handle_tree_widget(&mut self, results: &mut FrameResults, node_id: NodeId, widget: &dyn WidgetStateHandleDyn) {
        let rect = self.next_widget_rect_dyn(widget);
        let opt = widget.effective_widget_opt();
        let bopt = widget.effective_behaviour_opt();
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let (control, result) = self.run_widget_dyn(results, widget, rect, input, opt, bopt);
        self.record_tree_node(
            node_id,
            NodeCacheEntry {
                rect,
                body: rect,
                content_size: Vec2i::default(),
                control,
                result,
            },
        );
    }

    fn handle_tree_custom_render(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Custom>, render: &TreeCustomRender) {
        let rect = self.next_widget_rect_handle(state);
        let (opt, bopt, needs_input) = {
            let state = state.borrow();
            (state.effective_widget_opt(), state.effective_behaviour_opt(), state.needs_input_snapshot())
        };
        let input = if needs_input { Some(self.snapshot_input()) } else { None };
        let (control, result) = self.run_widget_handle(results, state, rect, input, opt, bopt);

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
        let render = render.clone();
        self.command_list.push(Command::CustomRender(
            cra,
            Box::new(move |dim, args| {
                (*render.borrow_mut())(dim, args);
            }),
        ));

        self.record_tree_node(
            node_id,
            NodeCacheEntry {
                rect,
                body: rect,
                content_size: Vec2i::default(),
                control,
                result,
            },
        );
    }

    fn run_tree_node_scope(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Node>) -> NodeStateValue {
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let rect = self.next_widget_rect_handle(state);
        let (opt, bopt, stable_state) = {
            let state = state.borrow();
            (*state.widget_opt(), *state.behaviour_opt(), state.state)
        };
        let (control, result) = self.run_widget_handle(results, state, rect, None, opt, bopt);

        // Header/tree nodes already consumed structural clicks from the previous
        // frame's cached rect. Reset the state after drawing so the old Widget
        // implementation does not toggle it a second time.
        if control.clicked {
            state.borrow_mut().state = stable_state;
        }

        self.record_tree_node(
            node_id,
            NodeCacheEntry {
                rect,
                body: rect,
                content_size: Vec2i::default(),
                control,
                result,
            },
        );
        stable_state
    }

    fn run_tree_node(&mut self, results: &mut FrameResults, node: &RuntimeTreeNode<'_>) {
        match node.kind() {
            RuntimeTreeNodeKind::Widget { widget } => {
                self.handle_tree_widget(results, node.id(), *widget);
            }
            RuntimeTreeNodeKind::CustomRender { state, render } => {
                self.handle_tree_custom_render(results, node.id(), state, render);
            }
            RuntimeTreeNodeKind::Run { run } => {
                (*run.borrow_mut())(self, results);
            }
            RuntimeTreeNodeKind::Container { handle, opt, behaviour } => {
                let mut handle = handle.clone();
                self.begin_panel(&mut handle, *opt, *behaviour);
                handle.with_mut(|container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.end_panel(&mut handle);
                let (rect, body, content_size) = handle.with(|container| (container.rect(), container.body(), container.content_size()));
                self.record_tree_node(
                    node.id(),
                    NodeCacheEntry {
                        rect,
                        body,
                        content_size,
                        control: ControlState::default(),
                        result: ResourceState::NONE,
                    },
                );
            }
            RuntimeTreeNodeKind::Header { state } => {
                if self.run_tree_node_scope(results, node.id(), state).is_expanded() {
                    self.run_tree_nodes(results, node.children());
                }
            }
            RuntimeTreeNodeKind::Tree { state } => {
                if self.run_tree_node_scope(results, node.id(), state).is_expanded() {
                    let indent_size = self.style.as_ref().indent;
                    self.layout.adjust_indent(indent_size);
                    self.run_tree_nodes(results, node.children());
                    self.layout.adjust_indent(-indent_size);
                }
            }
            RuntimeTreeNodeKind::Row { widths, height } => {
                self.with_row(widths, *height, |container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
            RuntimeTreeNodeKind::Grid { widths, heights } => {
                self.with_grid(widths, heights, |container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
            RuntimeTreeNodeKind::Column => {
                self.column(|container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
            RuntimeTreeNodeKind::Stack { width, height, direction } => {
                self.stack_with_width_direction(*width, *height, *direction, |container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
        }
    }

    /// Evaluates a prebuilt widget tree using the current container layout.
    pub fn widget_tree(&mut self, results: &mut FrameResults, tree: &WidgetTree) {
        let runtime_roots = tree.runtime_roots();
        self.pre_handle_tree_nodes(&runtime_roots);
        self.run_tree_nodes(results, &runtime_roots);
    }

    /// Builds a widget tree and evaluates it immediately.
    #[track_caller]
    pub fn build_tree(&mut self, results: &mut FrameResults, f: impl FnOnce(&mut WidgetTreeBuilder)) {
        let location = std::panic::Location::caller();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        location.file().hash(&mut hasher);
        location.line().hash(&mut hasher);
        location.column().hash(&mut hasher);
        let tree = WidgetTreeBuilder::build_with_seed(hasher.finish(), f);
        self.widget_tree(results, &tree);
    }

    /// Same as [`Container::build_tree`], but lets callers provide an explicit
    /// root key when the same call site builds multiple independent trees.
    ///
    /// The key is mixed with the caller location instead of replacing it so a
    /// reused logical key in unrelated call sites still lands in a distinct
    /// root namespace.
    #[track_caller]
    pub fn build_tree_with_key<K: Hash>(&mut self, key: K, results: &mut FrameResults, f: impl FnOnce(&mut WidgetTreeBuilder)) {
        let location = std::panic::Location::caller();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        location.file().hash(&mut hasher);
        location.line().hash(&mut hasher);
        location.column().hash(&mut hasher);
        key.hash(&mut hasher);
        let tree = WidgetTreeBuilder::build_with_seed(hasher.finish(), f);
        self.widget_tree(results, &tree);
    }

    /// Evaluates each widget state using the current flow.
    pub fn widgets(&mut self, results: &mut FrameResults, runs: &mut [WidgetRef<'_>]) {
        for widget in runs.iter_mut() {
            let _ = self.handle_widget_raw(results, &mut **widget);
        }
    }

    /// Emits a row flow and evaluates each widget run in order.
    pub fn row_widgets(&mut self, results: &mut FrameResults, widths: &[SizePolicy], height: SizePolicy, runs: &mut [WidgetRef<'_>]) {
        self.with_row(widths, height, |container| {
            container.widgets(results, runs);
        });
    }

    /// Emits a grid flow and evaluates each widget run in row-major order.
    pub fn grid_widgets(&mut self, results: &mut FrameResults, widths: &[SizePolicy], heights: &[SizePolicy], runs: &mut [WidgetRef<'_>]) {
        self.with_grid(widths, heights, |container| {
            container.widgets(results, runs);
        });
    }

    /// Emits a nested column scope and evaluates each widget run in order.
    pub fn column_widgets(&mut self, results: &mut FrameResults, runs: &mut [WidgetRef<'_>]) {
        self.column(|container| {
            container.widgets(results, runs);
        });
    }

    /// Emits a stack flow and evaluates each widget run in order.
    pub fn stack_widgets(&mut self, results: &mut FrameResults, width: SizePolicy, height: SizePolicy, direction: StackDirection, runs: &mut [WidgetRef<'_>]) {
        self.stack_with_width_direction(width, height, direction, |container| {
            container.widgets(results, runs);
        });
    }

    /// Replaces the container's style.
    pub fn set_style(&mut self, style: Style) {
        self.style = Rc::new(style);
    }

    /// Returns a copy of the current style.
    pub fn get_style(&self) -> Style {
        (*self.style).clone()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, FontEntry, SourceFormat};

    const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

    fn make_test_atlas() -> AtlasHandle {
        let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
        let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
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

    fn begin_test_frame(container: &mut Container, body: Recti) {
        container.prepare();
        container.rect = body;
        container.content_size = Vec2i::default();
        container.push_container_body(body, ContainerOption::NONE, WidgetBehaviourOption::NONE);
    }

    fn make_panel_handle(container: &Container, name: &str) -> ContainerHandle {
        ContainerHandle::new(Container::new(name, container.atlas.clone(), container.style.clone(), container.input.clone()))
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
        let textbox_id = widget_id_of(&state);
        container.set_focus(Some(textbox_id));
        state.cursor = 5;

        input.borrow_mut().keydown_code(KeyCode::LEFT);
        let rect = container.layout.next();
        let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.bopt);
        let control = container.update_control(textbox_id, rect, &control_state);
        let input = container.snapshot_input();
        let mut ctx = container.widget_ctx(textbox_id, rect, Some(input));
        state.handle(&mut ctx, &control);

        let cursor = state.cursor;
        assert_eq!(cursor, 1);
    }

    #[test]
    fn textbox_backspace_removes_multibyte() {
        let mut container = make_container();
        let input = container.input.clone();
        let mut state = Textbox::new("a\u{1F600}b");
        let textbox_id = widget_id_of(&state);
        container.set_focus(Some(textbox_id));
        state.cursor = 5;

        input.borrow_mut().keydown(KeyMode::BACKSPACE);
        let rect = container.layout.next();
        let control_state = (state.opt | WidgetOption::HOLD_FOCUS, state.bopt);
        let control = container.update_control(textbox_id, rect, &control_state);
        let input = container.snapshot_input();
        let mut ctx = container.widget_ctx(textbox_id, rect, Some(input));
        state.handle(&mut ctx, &control);

        let cursor = state.cursor;
        assert_eq!(state.buf, "ab");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn widget_textbox_backspace_removes_multibyte() {
        let mut container = make_container();
        let input = container.input.clone();
        let mut state = Textbox::new("a\u{1F600}b");
        container.set_focus(Some(widget_id_of(&state)));
        state.cursor = 5;
        let mut results = FrameResults::default();
        let state_id = widget_id_of(&state);

        input.borrow_mut().keydown(KeyMode::BACKSPACE);
        let mut runs = [widget_ref(&mut state)];
        container.widgets(&mut results, &mut runs);

        assert!(results.state(state_id).is_changed());
        assert_eq!(state.buf, "ab");
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn clicking_away_does_not_refocus_stale_hover_widget() {
        let mut container = make_container();
        let input = container.input.clone();
        let button = Button::new("A");
        let button_id = widget_id_of(&button);
        let button_rect = rect(0, 0, 50, 20);

        // Frame N: hover the button once so hover state is established.
        input.borrow_mut().mousemove(10, 10);
        let control = container.update_control(button_id, button_rect, &button);
        assert!(control.hovered);
        assert!(!control.focused);

        // Frame N+1: click elsewhere. The stale hover entry must not grab focus.
        {
            let mut i = input.borrow_mut();
            i.mousemove(80, 10);
            i.mousedown(80, 10, MouseButton::LEFT);
            i.mouseup(80, 10, MouseButton::LEFT);
        }
        let control = container.update_control(button_id, button_rect, &button);
        assert!(!control.hovered);
        assert!(!control.focused);
    }

    #[test]
    fn row_widgets_record_states() {
        let mut container = make_container();
        let mut button_a = Button::new("A");
        let mut button_b = Button::new("B");
        let mut results = FrameResults::default();
        let button_a_id = widget_id_of(&button_a);
        let button_b_id = widget_id_of(&button_b);
        let mut runs = [widget_ref(&mut button_a), widget_ref(&mut button_b)];

        container.row_widgets(&mut results, &[SizePolicy::Auto, SizePolicy::Auto], SizePolicy::Auto, &mut runs);

        assert!(results.state(button_a_id).is_none());
        assert!(results.state(button_b_id).is_none());
    }

    #[test]
    fn widget_tree_records_leaf_states() {
        let mut container = make_container();
        let button_a = widget_handle(Button::new("A"));
        let button_b = widget_handle(Button::new("B"));
        let mut results = FrameResults::default();

        container.build_tree(&mut results, |tree| {
            tree.row(&[SizePolicy::Auto, SizePolicy::Auto], SizePolicy::Auto, |tree| {
                tree.widget(button_a.clone());
                tree.widget(button_b.clone());
            });
        });

        assert!(results.state_of_handle(&button_a).is_none());
        assert!(results.state_of_handle(&button_b).is_none());
    }

    #[test]
    fn widget_tree_dispatches_panel_children() {
        let mut parent = make_container();
        let panel = make_panel_handle(&parent, "panel");
        let button = widget_handle(Button::new("inside"));
        let mut results = FrameResults::default();

        parent.build_tree(&mut results, |tree| {
            tree.row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |tree| {
                tree.container(panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                    tree.widget(button.clone());
                });
            });
        });

        assert!(results.state_of_handle(&button).is_none());
    }

    #[test]
    fn tree_nodes_expand_children_in_same_frame_from_cached_rects() {
        let mut container = make_container();
        let input = container.input.clone();
        let header = widget_handle(Node::header("Header", NodeStateValue::Closed));
        let child = widget_handle(Button::new("Child"));
        let seed = 0xfeed_face_u64;
        let mut header_node_id = NodeId::new(0);
        let mut child_node_id = NodeId::new(0);

        begin_test_frame(&mut container, rect(0, 0, 100, 40));
        let mut results = FrameResults::default();
        let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
            header_node_id = tree.header(header.clone(), |tree| {
                child_node_id = tree.widget(child.clone());
            });
        });
        container.widget_tree(&mut results, &tree);
        assert!(container.current_node_state(header_node_id).is_some());
        assert!(container.current_node_state(child_node_id).is_none());
        container.finish();

        let header_rect = container.previous_node_state(header_node_id).expect("header cache missing").rect;
        {
            let mut i = input.borrow_mut();
            i.mousemove(header_rect.x + 1, header_rect.y + 1);
            i.mousedown(header_rect.x + 1, header_rect.y + 1, MouseButton::LEFT);
        }

        begin_test_frame(&mut container, rect(0, 0, 100, 40));
        let mut results = FrameResults::default();
        let tree = WidgetTreeBuilder::build_with_seed(seed, |tree| {
            tree.header(header.clone(), |tree| {
                child_node_id = tree.widget(child.clone());
            });
        });
        container.widget_tree(&mut results, &tree);

        assert!(header.borrow().is_expanded());
        assert!(container.current_node_state(child_node_id).is_some());
        container.finish();
    }

    #[test]
    fn panel_hover_root_switches_between_siblings_on_next_frame() {
        let mut parent = make_container();
        let input = parent.input.clone();
        let mut left = make_panel_handle(&parent, "left");
        let mut right = make_panel_handle(&parent, "right");

        input.borrow_mut().mousemove(75, 10);
        begin_test_frame(&mut parent, rect(0, 0, 100, 20));
        parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
            container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
            container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
        });
        assert!(!left.inner().in_hover_root);
        assert!(!right.inner().in_hover_root);
        parent.finish();

        let mut left_active = false;
        let mut right_active = false;
        begin_test_frame(&mut parent, rect(0, 0, 100, 20));
        parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
            container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
                left_active = panel.inner().in_hover_root;
            });
            container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
                right_active = panel.inner().in_hover_root;
            });
        });
        assert!(!left_active);
        assert!(right_active);
        parent.finish();

        input.borrow_mut().mousemove(25, 10);
        left_active = false;
        right_active = false;
        begin_test_frame(&mut parent, rect(0, 0, 100, 20));
        parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
            container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
                left_active = panel.inner().in_hover_root;
            });
            container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
                right_active = panel.inner().in_hover_root;
            });
        });
        assert!(!left_active);
        assert!(right_active);
        parent.finish();

        left_active = false;
        right_active = false;
        begin_test_frame(&mut parent, rect(0, 0, 100, 20));
        parent.with_row(&[SizePolicy::Fixed(50), SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
            container.panel(&mut left, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
                left_active = panel.inner().in_hover_root;
            });
            container.panel(&mut right, ContainerOption::NONE, WidgetBehaviourOption::NONE, |panel| {
                right_active = panel.inner().in_hover_root;
            });
        });
        assert!(left_active);
        assert!(!right_active);
    }

    #[test]
    fn parent_widgets_are_only_blocked_while_mouse_is_inside_active_child_rect() {
        let mut parent = make_container();
        let input = parent.input.clone();
        let mut panel = make_panel_handle(&parent, "panel");

        input.borrow_mut().mousemove(10, 10);
        begin_test_frame(&mut parent, rect(0, 0, 100, 20));
        parent.with_row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
            container.panel(&mut panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
        });
        parent.finish();

        let blocked_button = Button::new("blocked");
        input.borrow_mut().mousemove(10, 10);
        begin_test_frame(&mut parent, rect(0, 0, 100, 20));
        let blocked = parent.update_control(widget_id_of(&blocked_button), rect(0, 0, 40, 20), &blocked_button);
        assert!(!blocked.hovered);
        parent.with_row(&[SizePolicy::Fixed(50)], SizePolicy::Fixed(20), |container| {
            container.panel(&mut panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |_| {});
        });
        parent.finish();

        let free_button = Button::new("free");
        input.borrow_mut().mousemove(75, 10);
        begin_test_frame(&mut parent, rect(0, 0, 100, 20));
        let free = parent.update_control(widget_id_of(&free_button), rect(60, 0, 30, 20), &free_button);
        assert!(free.hovered);
    }
}
