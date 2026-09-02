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
pub fn atlas_config() -> builder::Config {
    // Config owns its recipes and platform paths, so this factory returns one self-contained value
    // without static borrowed arrays or UTF-8-only path storage.
    builder::Config {
        texture_height: 256,
        texture_width: 512,
        white_icon: "assets/WHITE.png".into(),
        icons: vec![
            builder::IconAsset {
                name: "close".into(),
                path: "assets/CLOSE.png".into(),
            },
            builder::IconAsset {
                name: "expand".into(),
                path: "assets/PLUS.png".into(),
            },
            builder::IconAsset {
                name: "collapse".into(),
                path: "assets/MINUS.png".into(),
            },
            builder::IconAsset {
                name: "check".into(),
                path: "assets/CHECK.png".into(),
            },
            builder::IconAsset {
                name: "expand_down".into(),
                path: "assets/EXPAND_DOWN.png".into(),
            },
            builder::IconAsset {
                name: "open_folder".into(),
                path: "assets/OPEN_FOLDER_16.png".into(),
            },
            builder::IconAsset {
                name: "closed_folder".into(),
                path: "assets/CLOSED_FOLDER_16.png".into(),
            },
            builder::IconAsset {
                name: "file".into(),
                path: "assets/FILE_16.png".into(),
            },
        ],
        fonts: vec![
            builder::FontAsset {
                name: "body".into(),
                path: "assets/NORMAL.ttf".into(),
                size: 12,
            },
            builder::FontAsset {
                name: "small".into(),
                path: "assets/NORMAL.ttf".into(),
                size: 10,
            },
            builder::FontAsset {
                name: "title".into(),
                path: "assets/BOLD.ttf".into(),
                size: 12,
            },
            builder::FontAsset {
                name: "heading".into(),
                path: "assets/NORMAL.ttf".into(),
                size: 18,
            },
            builder::FontAsset {
                name: "mono".into(),
                path: "assets/CONSOLE.ttf".into(),
                size: 14,
            },
            builder::FontAsset {
                name: "calculator-display".into(),
                path: "assets/CONSOLE.ttf".into(),
                size: 28,
            },
        ],
    }
}

#[cfg(all(not(feature = "prebuilt-atlas"), not(feature = "external-atlas"), feature = "builder"))]
pub fn load_atlas() -> AtlasHandle {
    // Asset decoding and completed-atlas validation are distinct fallible boundaries. Finalizing
    // through build also preserves the atlas identity already stamped into builder-issued IDs.
    builder::Builder::from_config(&atlas_config())
        .expect("default atlas assets must load and pack")
        .build()
        .expect("default built atlas must satisfy the complete atlas contract")
}

#[cfg(feature = "prebuilt-atlas")]
mod prebuilt {
    use microui_redux::prelude::AtlasHandle;
    include!(concat!(env!("OUT_DIR"), "/prebuilt_atlas.rs"));

    pub fn load() -> AtlasHandle {
        // Generated metadata is validated again at its runtime ownership boundary; generation is
        // not treated as permission to bypass structural checks.
        AtlasHandle::try_from(&PREBUILT_ATLAS).expect("generated prebuilt atlas must satisfy the complete atlas contract")
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
    // Decoding and metadata validation form one fallible load operation, so the diagnostic must
    // not imply that malformed rectangles or semantic rendering resources decoded successfully.
    AtlasHandle::try_from(&source).unwrap_or_else(|err| panic!("Failed to load and validate {}: {err}", atlas_path.display()))
}

#[cfg(all(not(feature = "prebuilt-atlas"), not(feature = "external-atlas"), not(feature = "builder")))]
compile_error!(
    "examples/common/atlas_assets.rs requires `builder` for runtime atlas generation, `external-atlas` for atlas.png loading, or `prebuilt-atlas` for embedded atlas data"
);
