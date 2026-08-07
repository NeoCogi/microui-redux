//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

//! Custom widget state for user-provided retained drawing.
//!
//! Custom widgets reserve a normal layout cell and provide interaction payloads to callback-based
//! rendering commands.

use super::*;
use std::{cell::RefCell, rc::Rc};

/// One-shot construction input for a [`Custom`] runtime.
pub struct CustomParameters {
    /// Label used for debugging or inspection.
    pub name: String,
    /// Font used for default measurement.
    pub font: FontChoice,
    /// Base widget options.
    pub opt: WidgetOption,
}

impl WidgetParameters for CustomParameters {}

impl CustomParameters {
    /// Creates custom-render parameters with default options.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            font: FontChoice::Role(FontRole::Body),
            opt: WidgetOption::NONE,
        }
    }

    /// Creates custom-render parameters with explicit options.
    pub fn with_opt(name: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            name: name.into(),
            font: FontChoice::Role(FontRole::Body),
            opt,
        }
    }

    /// Replaces the font used for default measurement.
    pub const fn font(mut self, font: FontChoice) -> Self {
        self.font = font;
        self
    }
}

/// Concrete stateless custom-render runtime.
pub struct Custom {
    /// Initialization-only debug label.
    name: String,
    /// Initialization-only measurement font.
    font: FontChoice,
    /// Base widget options.
    opt: WidgetOption,
    /// Unit state retained with the same ownership shape as every runtime.
    state: Rc<RefCell<()>>,
}

impl Custom {
    /// Constructs the unique runtime without exposing a meaningless unit-state handle.
    pub fn create(parameters: CustomParameters) -> Self {
        CustomBuilder::create_widget(parameters)
    }

    /// Measures the custom widget's debug label as its default preferred size.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let text_w = if self.name.is_empty() {
            0
        } else {
            text_size(style, atlas, self.font, self.name.as_str()).width
        };
        let width = padding * 2 + text_w;
        let height = content_height(style, atlas, self.font, 0);
        Dimensioni::new(width.max(0), height)
    }
}

impl Widget for Custom {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl WidgetStateOwner for Custom {
    type State = ();

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

/// Builder associating custom-render parameters with the concrete runtime.
pub struct CustomBuilder;

impl WidgetBuilder for CustomBuilder {
    type Parameters = CustomParameters;
    type W = Custom;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Custom {
            name: parameters.name,
            font: parameters.font,
            opt: parameters.opt,
            state: Rc::new(RefCell::new(())),
        }
    }
}
