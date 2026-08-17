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
//! Default atlas asset configuration shared by examples and the build-time atlas exporter.

use microui_redux::prelude::AtlasHandle;
#[cfg(all(feature = "external-atlas", not(feature = "prebuilt-atlas")))]
use std::fs;

#[cfg(all(not(feature = "prebuilt-atlas"), not(feature = "external-atlas"), feature = "builder"))]
use microui_redux::atlas::builder;

#[cfg(all(not(feature = "prebuilt-atlas"), not(feature = "external-atlas"), feature = "builder"))]
pub fn atlas_config() -> builder::Config<'static> {
    const ICONS: &[builder::IconAsset<'static>] = &[
        builder::IconAsset { name: "close", path: "assets/CLOSE.png" },
        builder::IconAsset { name: "expand", path: "assets/PLUS.png" },
        builder::IconAsset {
            name: "collapse",
            path: "assets/MINUS.png",
        },
        builder::IconAsset { name: "check", path: "assets/CHECK.png" },
        builder::IconAsset {
            name: "expand_down",
            path: "assets/EXPAND_DOWN.png",
        },
        builder::IconAsset {
            name: "open_folder",
            path: "assets/OPEN_FOLDER_16.png",
        },
        builder::IconAsset {
            name: "closed_folder",
            path: "assets/CLOSED_FOLDER_16.png",
        },
        builder::IconAsset { name: "file", path: "assets/FILE_16.png" },
    ];
    const FONTS: &[builder::FontAsset<'static>] = &[
        builder::FontAsset {
            name: "body",
            path: "assets/NORMAL.ttf",
            size: 12,
        },
        builder::FontAsset {
            name: "small",
            path: "assets/NORMAL.ttf",
            size: 10,
        },
        builder::FontAsset {
            name: "title",
            path: "assets/BOLD.ttf",
            size: 12,
        },
        builder::FontAsset {
            name: "heading",
            path: "assets/NORMAL.ttf",
            size: 18,
        },
        builder::FontAsset {
            name: "mono",
            path: "assets/CONSOLE.ttf",
            size: 14,
        },
        builder::FontAsset {
            name: "calculator-display",
            path: "assets/CONSOLE.ttf",
            size: 28,
        },
    ];

    builder::Config {
        texture_height: 256,
        texture_width: 512,
        white_icon: String::from("assets/WHITE.png"),
        icons: ICONS,
        default_font: String::from("assets/NORMAL.ttf"),
        default_font_size: 12,
        fonts: FONTS,
    }
}

#[cfg(all(not(feature = "prebuilt-atlas"), not(feature = "external-atlas"), feature = "builder"))]
pub fn load_atlas() -> AtlasHandle {
    builder::Builder::from_config(&atlas_config()).expect("valid atlas config").to_atlas()
}

#[cfg(feature = "prebuilt-atlas")]
mod prebuilt {
    use microui_redux::prelude::AtlasHandle;
    include!(concat!(env!("OUT_DIR"), "/prebuilt_atlas.rs"));

    pub fn load() -> AtlasHandle {
        AtlasHandle::from(&PREBUILT_ATLAS)
    }
}

#[cfg(feature = "prebuilt-atlas")]
pub fn load_atlas() -> AtlasHandle {
    prebuilt::load()
}

#[cfg(all(feature = "external-atlas", not(feature = "prebuilt-atlas")))]
mod external {
    include!("external_atlas_metadata.rs");
}

#[cfg(all(feature = "external-atlas", not(feature = "prebuilt-atlas")))]
pub fn load_atlas() -> AtlasHandle {
    let atlas_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("atlas.png");
    let pixels = fs::read(&atlas_path).unwrap_or_else(|err| panic!("Failed to read {}: {err}", atlas_path.display()));
    let source = external::external_atlas_source(&pixels);
    AtlasHandle::try_from(&source).unwrap_or_else(|err| panic!("Failed to decode {}: {err}", atlas_path.display()))
}

#[cfg(all(not(feature = "prebuilt-atlas"), not(feature = "external-atlas"), not(feature = "builder")))]
compile_error!(
    "examples/common/atlas_assets.rs requires `builder` for runtime atlas generation, `external-atlas` for atlas.png loading, or `prebuilt-atlas` for embedded atlas data"
);
