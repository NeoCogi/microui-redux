#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
target="wasm32-unknown-unknown"
artifact_name="microui-redux-demo-full"
artifact_dir="$workspace_root/target/$target/release"
site_dir="$workspace_root/target/web-demo"
staging_dir="$workspace_root/target/web-demo-staging"
web_tools_dir="$workspace_root/target/web-tools"
wasm_bindgen="$web_tools_dir/bin/wasm-bindgen"
wasm_bindgen_version="0.2.127"
command="${1:-serve}"
port="${2:-8000}"

build() {
  rustup target add "$target"
  if [[ ! -x "$wasm_bindgen" ]] || [[ "$($wasm_bindgen --version)" != "wasm-bindgen $wasm_bindgen_version" ]]; then
    cargo install \
      --root "$web_tools_dir" \
      --version "$wasm_bindgen_version" \
      --locked \
      wasm-bindgen-cli
  fi
  cargo build \
    --manifest-path "$workspace_root/Cargo.toml" \
    -p microui-redux-demo-full \
    --bin "$artifact_name" \
    --target "$target" \
    --release \
    --no-default-features \
    --features "webgl prebuilt-atlas"

  rm -rf "$staging_dir"
  mkdir -p "$staging_dir"
  "$wasm_bindgen" \
    --target web \
    --no-typescript \
    --out-dir "$staging_dir" \
    --out-name "$artifact_name" \
    "$artifact_dir/$artifact_name.wasm"
  install -m 0644 "$workspace_root/demos/demo-full/web/index.html" "$staging_dir/index.html"
  touch "$staging_dir/.nojekyll"
  rm -rf "$site_dir"
  mv "$staging_dir" "$site_dir"

  echo "Web demo built in $site_dir"
}

case "$command" in
  build)
    build
    ;;
  serve)
    build
    echo "Serving http://127.0.0.1:$port"
    python3 -m http.server "$port" --bind 127.0.0.1 --directory "$site_dir"
    ;;
  *)
    echo "usage: $0 [build|serve] [port]" >&2
    exit 2
    ;;
esac
