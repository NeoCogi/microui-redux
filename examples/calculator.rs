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
//! Calculator retained-mode example.
//!
//! This example builds a small calculator UI with the retained UI node set API and the Glow/SDL
//! backend from `examples/common`.
#[path = "./common/mod.rs"]
mod common;

use application::Application;
use common::{atlas_assets, *};
use microui_redux::prelude::*;

const DISPLAY_MAX_LEN: usize = 24;
const DISPLAY_HEIGHT_FRACTION: f32 = 0.20;
const KEYPAD_ROW_HEIGHT_WEIGHT: f32 = 1.0;

#[derive(Copy, Clone)]
enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Copy, Clone)]
enum Action {
    Digit(char),
    Dot,
    ToggleSign,
    Op(Operator),
    Equals,
    ClearAll,
    ClearEntry,
    Backspace,
}

struct CalcButton {
    action: Action,
    submitted: WidgetEventHandle<ButtonSubmitted>,
    widget: Option<Button>,
}

impl CalcButton {
    fn new(label: &str, action: Action) -> Self {
        let (_, runtime) = Button::create(ButtonParameters::with_opt(label, WidgetOption::FRAME | WidgetOption::ALIGN_CENTER));
        let submitted = runtime.submitted();
        Self { action, submitted, widget: Some(runtime) }
    }
}

#[derive(Clone, Copy)]
enum Message {
    Apply(Action),
}

struct Calculator {
    display: String,
    accumulator: Option<f64>,
    pending: Option<Operator>,
    clear_on_input: bool,
    error: bool,
}

impl Calculator {
    fn new() -> Self {
        Self {
            display: "0".to_string(),
            accumulator: None,
            pending: None,
            clear_on_input: false,
            error: false,
        }
    }

    fn display_text(&self) -> &str {
        &self.display
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::ClearAll => {
                self.clear_all();
            }
            Action::ClearEntry => {
                self.clear_entry();
            }
            Action::Backspace => {
                self.recover_from_error();
                self.backspace();
            }
            Action::ToggleSign => {
                self.recover_from_error();
                self.toggle_sign();
            }
            Action::Dot => {
                self.recover_from_error();
                self.insert_dot();
            }
            Action::Digit(digit) => {
                self.recover_from_error();
                self.insert_digit(digit);
            }
            Action::Op(op) => {
                if !self.error {
                    self.set_operator(op);
                }
            }
            Action::Equals => {
                if !self.error {
                    self.evaluate();
                }
            }
        }
    }

    fn clear_all(&mut self) {
        self.display = "0".to_string();
        self.accumulator = None;
        self.pending = None;
        self.clear_on_input = false;
        self.error = false;
    }

    fn clear_entry(&mut self) {
        self.display = "0".to_string();
        self.clear_on_input = false;
        self.error = false;
    }

    fn recover_from_error(&mut self) {
        if self.error {
            self.clear_all();
        }
    }

    fn insert_digit(&mut self, digit: char) {
        if !digit.is_ascii_digit() {
            return;
        }
        if self.clear_on_input {
            self.display = "0".to_string();
            self.clear_on_input = false;
        }

        if self.display == "0" {
            self.display.clear();
        } else if self.display == "-0" {
            self.display = "-".to_string();
        }

        if self.display.len() < DISPLAY_MAX_LEN {
            self.display.push(digit);
        }

        if self.display.is_empty() {
            self.display.push('0');
        }
    }

    fn insert_dot(&mut self) {
        if self.clear_on_input {
            self.display = "0".to_string();
            self.clear_on_input = false;
        }

        if !self.display.contains('.') {
            if self.display.is_empty() {
                self.display.push('0');
            }
            self.display.push('.');
        }
    }

    fn toggle_sign(&mut self) {
        if self.display == "0" || self.display == "0." {
            return;
        }

        if self.display.starts_with('-') {
            self.display.remove(0);
        } else if self.display.len() < DISPLAY_MAX_LEN {
            self.display.insert(0, '-');
        }
    }

    fn backspace(&mut self) {
        if self.clear_on_input {
            self.display = "0".to_string();
            self.clear_on_input = false;
            return;
        }

        self.display.pop();

        if self.display.is_empty() || self.display == "-" {
            self.display = "0".to_string();
        }
    }

    fn set_operator(&mut self, op: Operator) {
        let rhs = self.current_value();

        if let Some(pending) = self.pending {
            if !self.clear_on_input {
                let lhs = self.accumulator.unwrap_or(rhs);
                match Self::compute(lhs, rhs, pending) {
                    Some(result) => {
                        self.accumulator = Some(result);
                        self.display = Self::format_value(result);
                    }
                    None => {
                        self.set_error();
                        return;
                    }
                }
            }
        } else {
            self.accumulator = Some(rhs);
        }

        self.pending = Some(op);
        self.clear_on_input = true;
    }

    fn evaluate(&mut self) {
        let Some(pending) = self.pending else {
            return;
        };

        let rhs = self.current_value();
        let lhs = self.accumulator.unwrap_or(rhs);

        match Self::compute(lhs, rhs, pending) {
            Some(result) => {
                self.display = Self::format_value(result);
                self.accumulator = Some(result);
                self.pending = None;
                self.clear_on_input = true;
            }
            None => {
                self.set_error();
            }
        }
    }

    fn current_value(&self) -> f64 {
        self.display.parse::<f64>().unwrap_or(0.0)
    }

    fn compute(lhs: f64, rhs: f64, op: Operator) -> Option<f64> {
        let result = match op {
            Operator::Add => lhs + rhs,
            Operator::Subtract => lhs - rhs,
            Operator::Multiply => lhs * rhs,
            Operator::Divide => {
                if rhs == 0.0 {
                    return None;
                }
                lhs / rhs
            }
        };

        if result.is_finite() { Some(result) } else { None }
    }

    fn format_value(value: f64) -> String {
        let mut out = format!("{value:.10}");
        while out.contains('.') && out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
        if out == "-0" || out.is_empty() { "0".to_string() } else { out }
    }

    fn set_error(&mut self) {
        self.display = "Error".to_string();
        self.accumulator = None;
        self.pending = None;
        self.clear_on_input = true;
        self.error = true;
    }
}

struct State {
    root: RootHandle,
    display: WidgetStateHandle<TextboxState>,
    calculator: Calculator,
    buttons: [CalcButton; 20],
}

fn main() {
    let atlas = atlas_assets::load_atlas();
    let mut fw = Application::new(atlas.clone(), move |_gl, ctx| {
        let (display_state, display_runtime) = Textbox::create(TextboxParameters::with_opt(
            "0",
            WidgetOption::FRAME | WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT,
        ));
        let mut buttons = [
            CalcButton::new("AC", Action::ClearAll),
            CalcButton::new("CE", Action::ClearEntry),
            CalcButton::new("BS", Action::Backspace),
            CalcButton::new("/", Action::Op(Operator::Divide)),
            CalcButton::new("7", Action::Digit('7')),
            CalcButton::new("8", Action::Digit('8')),
            CalcButton::new("9", Action::Digit('9')),
            CalcButton::new("*", Action::Op(Operator::Multiply)),
            CalcButton::new("4", Action::Digit('4')),
            CalcButton::new("5", Action::Digit('5')),
            CalcButton::new("6", Action::Digit('6')),
            CalcButton::new("-", Action::Op(Operator::Subtract)),
            CalcButton::new("1", Action::Digit('1')),
            CalcButton::new("2", Action::Digit('2')),
            CalcButton::new("3", Action::Digit('3')),
            CalcButton::new("+", Action::Op(Operator::Add)),
            CalcButton::new("+/-", Action::ToggleSign),
            CalcButton::new("0", Action::Digit('0')),
            CalcButton::new(".", Action::Dot),
            CalcButton::new("=", Action::Equals),
        ];
        let (_, display_row) = Row::create(RowParameters::new(
            [SizePolicy::Remainder(0)],
            SizePolicy::Remainder(0),
            [Node::widget(display_runtime)],
        ));
        let display_row = display_row.with_policy(Policy::new(SizePolicy::Auto, SizePolicy::Fraction(DISPLAY_HEIGHT_FRACTION)));
        let columns = [SizePolicy::Weight(1.0); 4];
        let rows = [SizePolicy::Weight(KEYPAD_ROW_HEIGHT_WEIGHT); 5];
        let button_nodes = buttons
            .iter_mut()
            .map(|button| Node::widget(button.widget.take().expect("calculator tree is built once")))
            .collect::<Vec<_>>();
        let (_, grid) = Grid::create(GridParameters::new(columns, rows, button_nodes));
        let (_, keypad_column) = Column::create(ColumnParameters::new([grid]));
        let (_, keypad_row) = Row::create(RowParameters::new([SizePolicy::Remainder(0)], SizePolicy::Remainder(0), [keypad_column]));
        let keypad_row = keypad_row.with_policy(Policy::new(SizePolicy::Auto, SizePolicy::Remainder(0)));
        let (_, tree) = Column::create(ColumnParameters::new([display_row, keypad_row]));
        let root = ctx.create_window("Calculator", rect(0, 0, 320, 420), tree);
        ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_RESIZE | WindowOption::NO_TITLE)
            .expect("calculator root should remain registered");
        State {
            root,
            display: display_state,
            calculator: Calculator::new(),
            buttons,
        }
    })
    .unwrap();

    fw.event_loop_session(
        |state, session, subscribers| {
            for button in &state.buttons {
                let action = button.action;
                session
                    .connect(button.submitted.clone(), move |_| Message::Apply(action))
                    .expect("calculator button should be alive and unconnected");
            }
            subscribers.subscribe(|state: &mut State, message: &Message, _emit| match message {
                Message::Apply(action) => state.calculator.apply(*action),
            });
        },
        |ctx, state, dim| {
            ctx.set_root_rect(state.root.id(), rect(0, 0, dim.width, dim.height))
                .expect("calculator root should remain registered");
            let _ = state
                .display
                .try_update_with(state.calculator.display_text().to_owned(), |display, text| display.set_text(text));
        },
    );
}
