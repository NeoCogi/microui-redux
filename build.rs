use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=assets/NORMAL.ttf");
    println!("cargo:rerun-if-changed=assets/WHITE.png");
    println!("cargo:rerun-if-changed=assets/CLOSE.png");
    println!("cargo:rerun-if-changed=assets/PLUS.png");
    println!("cargo:rerun-if-changed=assets/MINUS.png");
    println!("cargo:rerun-if-changed=assets/CHECK.png");

    if env::var("MICROUI_BUILD_TOOL").is_ok() {
        return Ok(());
    }

    if env::var("CARGO_FEATURE_BUILDER").is_ok() {
        return Ok(());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let generated = out_dir.join("prebuilt_atlas.rs");

    run_atlas_export(&generated)?;
    println!("cargo:rerun-if-changed=examples/common/atlas_assets.rs");
    Ok(())
}

fn run_atlas_export(output: &Path) -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::new("cargo");
    cmd.env("MICROUI_BUILD_TOOL", "1").args([
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
