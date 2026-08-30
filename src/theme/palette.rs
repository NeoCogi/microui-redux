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

//! Semantic color roles used by built-in controls.

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
/// Identifiers for each of the built-in style colors.
pub enum ControlColor {
    /// Number of color entries in [`crate::Style::colors`].
    Max = 12,
    /// Thumb of scrollbars.
    ScrollThumb = 11,
    /// Base frame of scrollbars.
    ScrollBase = 10,
    /// Base color while the pointer hovers the widget.
    BaseHover = 9,
    /// Default base color.
    Base = 8,
    /// Button color while the pointer hovers the widget.
    ButtonHover = 7,
    /// Default button color.
    Button = 6,
    /// Panel background color.
    PanelBG = 5,
    /// Window title text color.
    TitleText = 4,
    /// Window title background color.
    TitleBG = 3,
    /// Window background color.
    WindowBG = 2,
    /// Outline/border color.
    Border = 1,
    /// Default text color.
    Text = 0,
}

impl ControlColor {
    /// Promotes the enum to the hover variant when relevant.
    pub fn hover(&mut self) {
        *self = match self {
            Self::Base => Self::BaseHover,
            Self::Button => Self::ButtonHover,
            _ => *self,
        }
    }
}
