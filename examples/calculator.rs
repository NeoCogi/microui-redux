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
#[path = "./common/mod.rs"]
mod common;

use application::Application;
use common::{atlas_assets, *};
use microui_redux::*;

const DISPLAY_MAX_LEN: usize = 24;
const DISPLAY_HEIGHT_WEIGHT: f32 = 20.0;
const KEYPAD_ROW_HEIGHT_WEIGHT: f32 = 20.0;

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

#[derive(Clone)]
struct CalcButton {
    action: Action,
    widget: WidgetHandle<Button>,
}

impl CalcButton {
    fn new(label: &str, action: Action) -> Self {
        Self {
            action,
            widget: widget_handle(Button::with_opt(label, WidgetOption::ALIGN_CENTER)),
        }
    }
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
    window: WindowHandle,
    display: WidgetHandle<Textbox>,
    calculator: Calculator,
    buttons: [CalcButton; 20],
    tree: WidgetTree,
}

fn main() {
    let slots = atlas_assets::default_slots();
    let atlas = atlas_assets::load_atlas(&slots);
    let mut fw = Application::new(atlas.clone(), move |_gl, ctx| {
        let display = widget_handle(Textbox::with_opt("0", WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT));
        let buttons = [
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
        let button_widgets: Vec<_> = buttons.iter().map(|button| button.widget.clone()).collect();
        let tree = WidgetTreeBuilder::build({
            let display = display.clone();
            move |tree| {
                tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Weight(DISPLAY_HEIGHT_WEIGHT), |tree| {
                    tree.widget(display.clone());
                });
                tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |tree| {
                    tree.column(|tree| {
                        let columns = [
                            SizePolicy::Weight(1.0),
                            SizePolicy::Weight(1.0),
                            SizePolicy::Weight(1.0),
                            SizePolicy::Weight(1.0),
                        ];
                        let rows = [SizePolicy::Weight(KEYPAD_ROW_HEIGHT_WEIGHT); 5];
                        tree.grid(&columns, &rows, |tree| {
                            for button in &button_widgets {
                                tree.widget(button.clone());
                            }
                        });
                    });
                });
            }
        });
        State {
            window: ctx.new_window("Calculator", rect(0, 0, 320, 420)),
            display,
            calculator: Calculator::new(),
            buttons,
            tree,
        }
    })
    .unwrap();

    fw.event_loop(|ctx, state| {
        ctx.frame(|ctx| {
            let dim = ctx.canvas().current_dimension();
            state.window.set_size(&dim);
            {
                let mut display = state.display.borrow_mut();
                display.buf = state.calculator.display_text().to_string();
                display.cursor = display.buf.len();
            }

            ctx.window(
                &mut state.window.clone(),
                ContainerOption::NO_RESIZE | ContainerOption::NO_TITLE,
                WidgetBehaviourOption::NONE,
                &state.tree,
            );

            let results = ctx.committed_results();
            for button in &state.buttons {
                if results.state_of_handle(&button.widget).is_submitted() {
                    state.calculator.apply(button.action);
                }
            }
        });
    });
}
