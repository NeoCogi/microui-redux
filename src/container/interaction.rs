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
//! Hit-testing, focus management, and per-frame input snapshot helpers.

use super::*;

impl Container {
    /// Manually updates which widget owns focus.
    pub fn set_focus(&mut self, widget_id: Option<WidgetId>) {
        self.interaction.focus = widget_id.map(InteractionId::widget);
        self.interaction.updated_focus = true;
    }

    pub(crate) fn hit_test_rect(&mut self, rect: Recti, in_hover_root: bool) -> bool {
        let clip_rect = self.get_clip_rect();
        rect.contains(&self.input.borrow().mouse_pos) && clip_rect.contains(&self.input.borrow().mouse_pos) && in_hover_root
    }

    fn pointer_blocked_by_child(&self) -> bool {
        match self.interaction.hover_root_child_rect {
            Some(rect) => rect.contains(&self.input.borrow().mouse_pos),
            None => false,
        }
    }

    /// Returns `true` if the cursor is inside `rect` and the container can currently own hover there.
    pub fn mouse_over(&mut self, rect: Recti, in_hover_root: bool) -> bool {
        self.hit_test_rect(rect, in_hover_root && !self.pointer_blocked_by_child())
    }

    pub(crate) fn update_control_for(
        &mut self,
        interaction_id: InteractionId,
        rect: Recti,
        opt: WidgetOption,
        bopt: WidgetBehaviourOption,
        focus_policy: FocusPolicy,
    ) -> ControlState {
        let in_hover_root = self.interaction.in_hover_root;
        let mouseover = self.mouse_over(rect, in_hover_root);
        if self.interaction.focus == Some(interaction_id) {
            self.interaction.updated_focus = true;
        }
        if opt.is_not_interactive() {
            return ControlState::default();
        }
        if mouseover && self.input.borrow().mouse_down.is_none() {
            self.interaction.hover = Some(interaction_id);
        }
        if self.interaction.focus == Some(interaction_id) {
            let should_clear_focus = {
                let input = self.input.borrow();
                let pressed_outside = !input.mouse_pressed.is_none() && !mouseover;
                let released_without_hold_focus = input.mouse_down.is_none() && focus_policy.releases_on_mouse_up();
                pressed_outside || released_without_hold_focus
            };
            if should_clear_focus {
                self.set_focus(None);
            }
        }
        if self.interaction.hover == Some(interaction_id) {
            if !mouseover {
                self.interaction.hover = None;
            } else if !self.input.borrow().mouse_pressed.is_none() {
                self.interaction.focus = Some(interaction_id);
                self.interaction.updated_focus = true;
            }
        }

        let mut scroll = None;
        if bopt.is_grab_scroll() && self.interaction.hover == Some(interaction_id) {
            if let Some(delta) = self.interaction.pending_scroll {
                if delta.x != 0 || delta.y != 0 {
                    self.interaction.pending_scroll = None;
                    scroll = Some(delta);
                }
            }
        }

        if self.interaction.focus == Some(interaction_id) {
            let mouse_pos = self.input.borrow().mouse_pos;
            let origin = vec2(self.body.x, self.body.y);
            self.input.borrow_mut().rel_mouse_pos = mouse_pos - origin;
        }

        let focused = self.interaction.focus == Some(interaction_id);
        let hovered = self.interaction.hover == Some(interaction_id);
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

    pub(crate) fn update_control_with_opts(&mut self, widget_id: WidgetId, rect: Recti, opt: WidgetOption, bopt: WidgetBehaviourOption) -> ControlState {
        let focus_policy = FocusPolicy::from_widget_options(opt);
        self.update_control_for(InteractionId::widget(widget_id), rect, opt, bopt, focus_policy)
    }

    #[inline(never)]
    /// Updates hover/focus state for the widget described by `widget_id` and optionally consumes scroll.
    pub fn update_control<W: Widget + ?Sized>(&mut self, widget_id: WidgetId, rect: Recti, state: &W) -> ControlState {
        self.update_control_for(
            InteractionId::widget(widget_id),
            rect,
            state.effective_widget_opt(),
            state.effective_behaviour_opt(),
            state.focus_policy(),
        )
    }

    pub(crate) fn snapshot_input(&mut self) -> Rc<InputSnapshot> {
        if let Some(snapshot) = &self.interaction.input_snapshot {
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
        self.interaction.input_snapshot = Some(snapshot.clone());
        snapshot
    }

    pub(crate) fn widget_ctx(&mut self, widget_id: WidgetId, rect: Recti, input: Option<Rc<InputSnapshot>>) -> WidgetCtx<'_> {
        self.widget_ctx_for(widget_id, InteractionId::widget(widget_id), rect, input)
    }

    pub(crate) fn widget_ctx_for(
        &mut self,
        widget_id: WidgetId,
        interaction_id: InteractionId,
        rect: Recti,
        input: Option<Rc<InputSnapshot>>,
    ) -> WidgetCtx<'_> {
        WidgetCtx::new_with_interaction(
            widget_id,
            interaction_id,
            rect,
            &mut self.draw.commands,
            &mut self.draw.triangle_vertices,
            &mut self.draw.clip_stack,
            self.style.as_ref(),
            &self.atlas,
            &mut self.interaction.focus,
            &mut self.interaction.updated_focus,
            self.interaction.in_hover_root,
            input,
        )
    }

    pub(crate) fn input_to_mouse_event(&self, control: &ControlState, input: &InputSnapshot, rect: Recti) -> MouseEvent {
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
}
