#![cfg(all(feature = "builder", feature = "save-to-rust"))]

use microui_redux::SourceFormat;
use std::{env, error::Error, path::PathBuf};

#[path = "../examples/common/atlas_assets.rs"]
mod atlas_assets;

fn main() -> Result<(), Box<dyn Error>> {
    let output = parse_output_arg()?;
    export_atlas(&output)?;
    Ok(())
}

fn parse_output_arg() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--output" {
            if let Some(path) = args.next() {
                return Ok(PathBuf::from(path));
            } else {
                return Err("--output requires a path".into());
            }
        }
    }
    Err("missing --output <path>".into())
}

fn export_atlas(path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let slots = atlas_assets::default_slots();
    let atlas = atlas_assets::load_atlas(&slots);
    atlas.to_rust_files("PREBUILT_ATLAS", SourceFormat::Raw, path.to_str().unwrap())?;
    Ok(())
}
