use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

fn main() {
    let is_browser = env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") && env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("unknown");
    if !is_browser {
        return;
    }
    println!("cargo:rustc-link-arg=-zstack-size=2097152");
    generate_embedded_themes();
}

fn generate_embedded_themes() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo provides CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.join("../..");
    let themes = workspace.join("themes");
    let mut assets = ["NORMAL.ttf", "BOLD.ttf", "CONSOLE.ttf"]
        .map(|name| workspace.join("assets").join(name))
        .into_iter()
        .collect::<Vec<_>>();
    collect_pngs(&themes, &mut assets);
    assets.sort();

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR")).join("embedded_theme_assets.rs");
    let mut generated = File::create(output).expect("create embedded theme source");
    for (constant, relative) in [
        ("EMBEDDED_WINDOWS_311_THEME", "themes/windows-3.11/theme.json"),
        ("EMBEDDED_WINDOWS_95_THEME", "themes/windows-95/theme.json"),
        ("EMBEDDED_MAC_OS_9_THEME", "themes/mac-os-9/theme.json"),
    ] {
        let source = workspace.join(relative);
        println!("cargo:rerun-if-changed={}", source.display());
        writeln!(generated, "const {constant}: &[u8] = include_bytes!({:?});", source.to_string_lossy()).expect("write embedded theme definition");
    }

    writeln!(
        generated,
        "fn embedded_theme_asset(path: &std::path::Path) -> std::io::Result<Vec<u8>> {{\n    let path = normalized_embedded_path(path);\n    match path.as_str() {{"
    )
    .expect("write embedded asset resolver");
    for asset in assets {
        let relative = asset
            .strip_prefix(&workspace)
            .expect("embedded theme asset must belong to the workspace")
            .to_string_lossy()
            .replace('\\', "/");
        println!("cargo:rerun-if-changed={}", asset.display());
        writeln!(
            generated,
            "        {:?} => Ok(include_bytes!({:?}).to_vec()),",
            relative,
            asset.to_string_lossy()
        )
        .expect("write embedded asset match arm");
    }
    writeln!(
        generated,
        "        _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!(\"embedded theme asset `{{path}}` was not found\"))),\n    }}\n}}\n\nfn normalized_embedded_path(path: &std::path::Path) -> String {{\n    let mut normalized = Vec::new();\n    for component in path.components() {{\n        match component {{\n            std::path::Component::Normal(component) => normalized.push(component.to_string_lossy().into_owned()),\n            std::path::Component::ParentDir => {{ normalized.pop(); }},\n            std::path::Component::CurDir => {{}},\n            std::path::Component::RootDir | std::path::Component::Prefix(_) => normalized.clear(),\n        }}\n    }}\n    normalized.join(\"/\")\n}}"
    )
    .expect("finish embedded asset resolver");
}

fn collect_pngs(directory: &Path, assets: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read theme directory {}: {error}", directory.display()))
        .map(|entry| entry.expect("read theme directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_pngs(&path, assets);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("png") {
            assets.push(path);
        }
    }
}
