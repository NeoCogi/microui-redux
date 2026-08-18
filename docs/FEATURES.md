# Cargo features

- `builder` *(default)* – enables the runtime atlas builder and PNG decoding helpers used by the examples.
- `png_source` – accepts PNG-compressed serialized atlases and `ImageSource::Png { .. }`; pixels are decoded to RGBA when loaded.
- `save-to-rust` – enables `AtlasHandle::to_rust_files` to emit the current atlas as Rust code for embedding.
- `prebuilt-atlas` – opt-in example atlas embedding; without it, examples build their atlas at runtime.
- `external-atlas` – example-only loader for a repository-root `atlas.png` paired with the checked-in `examples/common/external_atlas_metadata.rs` metadata.
- `example-backend` – shared internal gate used by examples; pair it with at least one concrete backend.
- `example-glow` / `example-vulkan` / `example-wgpu` – concrete example backends. Features are additive; examples select Glow, then Vulkan, then WGPU when several are enabled. Enable only the desired backend for normal interactive runs.

Disabling default features leaves only the raw RGBA upload path (`ImageSource::Raw { .. }`):
`cargo build --no-default-features`

The demos build their atlas at runtime unless you opt into `prebuilt-atlas`, so `--no-default-features` example builds should include `builder`:
`cargo run --example demo-full --no-default-features --features "example-vulkan builder"`

Equivalent command using the shared gate explicitly:
`cargo run --example demo-full --no-default-features --features "example-backend example-vulkan builder"`

To embed the generated atlas instead, add `prebuilt-atlas` explicitly:
`cargo run --example demo-full --no-default-features --features "example-vulkan prebuilt-atlas"`

`external-atlas` is a repository-development path, not a self-contained package feature. It expects
an existing `atlas.png` whose pixels match the checked-in metadata exactly; `atlas.png` is ignored
by Git and excluded from the crate package. The repository does not currently provide a command
that regenerates this pair. Prefer `builder` or `prebuilt-atlas` unless you maintain both files
together. If both atlas-loading features are enabled, `prebuilt-atlas` takes precedence over
`external-atlas`.

To export an atlas as Rust, enable `save-to-rust` (and `png_source` when serializing PNG-backed atlas data) and call `AtlasHandle::to_rust_files`. The helper binary requires `builder`, `save-to-rust`, and `png_source`:
`cargo run --bin atlas_export --features "builder save-to-rust png_source" -- --output path/to/atlas.rs`
