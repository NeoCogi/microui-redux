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
use microui_redux::{AtlasHandle, Dimensioni};

#[cfg(feature = "builder")]
use microui_redux::builder;

pub fn default_slots() -> Vec<Dimensioni> {
    vec![Dimensioni::new(64, 64), Dimensioni::new(24, 32), Dimensioni::new(64, 24)]
}

#[cfg(feature = "builder")]
pub fn atlas_config<'a>(slots: &'a [Dimensioni]) -> builder::Config<'a> {
    builder::Config {
        texture_height: 256,
        texture_width: 256,
        white_icon: String::from("assets/WHITE.png"),
        close_icon: String::from("assets/CLOSE.png"),
        expand_icon: String::from("assets/PLUS.png"),
        collapse_icon: String::from("assets/MINUS.png"),
        check_icon: String::from("assets/CHECK.png"),
        expand_down_icon: String::from("assets/EXPAND_DOWN.png"),
        open_folder_16_icon: String::from("assets/OPEN_FOLDER_16.png"),
        closed_folder_16_icon: String::from("assets/CLOSED_FOLDER_16.png"),
        file_16_icon: String::from("assets/FILE_16.png"),
        default_font: String::from("assets/NORMAL.ttf"),
        default_font_size: 12,
        slots,
    }
}

#[cfg(feature = "builder")]
pub fn load_atlas(slots: &[Dimensioni]) -> AtlasHandle {
    builder::Builder::from_config(&atlas_config(slots)).expect("valid atlas config").to_atlas()
}

#[cfg(not(feature = "builder"))]
mod prebuilt {
    use microui_redux::AtlasHandle;
    include!(concat!(env!("OUT_DIR"), "/prebuilt_atlas.rs"));

    pub fn load() -> AtlasHandle {
        AtlasHandle::from(&PREBUILT_ATLAS)
    }
}

#[cfg(not(feature = "builder"))]
pub fn load_atlas(_slots: &[Dimensioni]) -> AtlasHandle {
    prebuilt::load()
}
