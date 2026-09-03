# Cargo features

- `builder` *(enabled by default through `theme-json`)* – enables the runtime atlas builder and PNG decoding helpers used by the examples.
- `png_source` – accepts static PNG-compressed serialized atlases and `ImageSource::Png { .. }`; pixels are decoded to RGBA and pass the same strict atlas validation as raw sources before a handle is returned. Animated PNGs are rejected.
- `theme-json` *(default)* – enables strict versioned JSON theme loading and relative per-state
  image patches. It includes `builder` and `png_source`; repeated references to one resolved image
  path share one Context-owned texture upload.
- `save-to-rust` *(default)* – enables `AtlasHandle::to_rust_files` to emit the current atlas as Rust code for embedding.
- `prebuilt-atlas` – opt-in build-time generation and embedding of the example atlas; without it,
  examples build their atlas at runtime. The build script invokes `atlas_export`, so source assets
  and a working Cargo toolchain are still required while compiling.
- `external-atlas` – example-only loader for a repository-root `atlas.png` paired with the checked-in `examples/common/external_atlas_metadata.rs` metadata.
- `example-backend` – shared internal gate used by examples; pair it with at least one concrete backend.
- `example-glow` / `example-vulkan` / `example-wgpu` – concrete example backends. Features are additive; examples select Glow, then Vulkan, then WGPU when several are enabled. Enable only the desired backend for normal interactive runs.

Disabling default features leaves only the raw RGBA upload path (`ImageSource::Raw { .. }`):
`cargo build --no-default-features`

Serialized atlases are loaded with `AtlasHandle::try_from(&source)`. Construction returns a
concrete `AtlasError` for malformed pixels or metadata; there is no lossy or infallible atlas-load
path. Every font must include an underscore fallback, and all atlas rectangles must fit inside the
declared texture. Public image decoding, serialized atlas construction, builder atlas allocation,
and builder icon decoding share a 64-MiB limit for each decoded RGBA or normalized color buffer
(`MAX_DECODED_RGBA_BYTES`, exactly 4,096 × 4,096 four-byte pixels).

`load_image_bytes` returns the concrete `ImageError`. Its variants distinguish invalid signed raw
dimensions, checked storage failures, exact RGBA-length mismatches, dimension mismatches, decoder
failures, and unsupported animation. `ImageStorageError` retains the failed dimensions and
allocation limit. Higher layers preserve that structure: `AtlasError::Image` and
`BuilderError::Image` expose `ImageError` as their standard error source instead of translating it
through `std::io::Error` or matching diagnostic text.

The demos build their atlas at runtime unless you opt into `prebuilt-atlas`. `demo-full` requires
`theme-json`, which enables `builder`, so a `--no-default-features` run needs one concrete backend
and `theme-json`:
`cargo run --example demo-full --no-default-features --features "example-vulkan theme-json"`

To generate and embed the atlas at build time instead, add `prebuilt-atlas` explicitly. Because
`demo-full` also requires `theme-json`, this changes its runtime asset-loading path but does not
remove `builder` from that example's compiled feature set:
`cargo run --example demo-full --no-default-features --features "example-vulkan prebuilt-atlas theme-json"`

`external-atlas` is a repository-development path and must not be enabled from a crates.io package.
It expects
an existing `atlas.png` whose pixels match the checked-in metadata exactly; `atlas.png` is ignored
by Git and excluded from the crate package. The repository does not currently provide a command
that regenerates this pair. Prefer `builder` or `prebuilt-atlas` unless you maintain both files
together. If both atlas-loading features are enabled, `prebuilt-atlas` takes precedence over
`external-atlas`.

To export an atlas as Rust, enable `save-to-rust` (and `png_source` when serializing PNG-backed atlas data) and call `AtlasHandle::to_rust_files`. The helper binary requires `builder`, `save-to-rust`, and `png_source`:
`cargo run --bin atlas_export --features "builder save-to-rust png_source" -- --output path/to/atlas.rs`
