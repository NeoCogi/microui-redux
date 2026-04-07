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
use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=assets/NORMAL.ttf");
    println!("cargo:rerun-if-changed=assets/BOLD.ttf");
    println!("cargo:rerun-if-changed=assets/CONSOLE.ttf");
    println!("cargo:rerun-if-changed=assets/WHITE.png");
    println!("cargo:rerun-if-changed=assets/CLOSE.png");
    println!("cargo:rerun-if-changed=assets/PLUS.png");
    println!("cargo:rerun-if-changed=assets/MINUS.png");
    println!("cargo:rerun-if-changed=assets/CHECK.png");
    println!("cargo:rerun-if-changed=assets/EXPAND_DOWN.png");
    println!("cargo:rerun-if-changed=assets/OPEN_FOLDER_16.png");
    println!("cargo:rerun-if-changed=assets/CLOSED_FOLDER_16.png");
    println!("cargo:rerun-if-changed=assets/FILE_16.png");

    if env::var("MICROUI_BUILD_TOOL").is_ok() {
        return Ok(());
    }

    if env::var("CARGO_FEATURE_BUILDER").is_ok() {
        return Ok(());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let generated = out_dir.join("prebuilt_atlas.rs");
    let atlas_target_dir = out_dir.join("atlas_export_target");
    fs::create_dir_all(&atlas_target_dir)?;

    run_atlas_export(&generated, &atlas_target_dir)?;
    println!("cargo:rerun-if-changed=examples/common/atlas_assets.rs");
    Ok(())
}

fn run_atlas_export(output: &Path, target_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::new("cargo");
    // Use a dedicated target dir so release builds don't deadlock on Cargo's global target lock.
    cmd.env("MICROUI_BUILD_TOOL", "1").env("CARGO_TARGET_DIR", target_dir).args([
        "run",
        "--bin",
        "atlas_export",
        "--features",
        "builder,save-to-rust",
        "--release",
        "--",
        "--output",
        output.to_str().expect("valid path"),
    ]);

    let status = cmd.status()?;
    if !status.success() {
        return Err("atlas_export failed".into());
    }
    Ok(())
}
