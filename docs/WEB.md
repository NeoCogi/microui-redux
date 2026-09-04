# WebGL canvas demo

The browser demo compiles `microui-redux-demo-full` directly for Rust's
`wasm32-unknown-unknown` target. `web-sys` supplies DOM input, canvas sizing, and
`requestAnimationFrame`; `microui-redux-renderer-webgl-canvas` renders through the canvas's WebGL 2
context. SDL remains a native-only dependency.

## Prerequisites

Install the current stable Rust toolchain. The build script installs the Rust WebAssembly target
and a matching `wasm-bindgen` CLI under `target/web-tools` when needed. No non-Rust SDK is required.

## Build and run locally

From the workspace root, build the static site and start its local server:

```bash
./scripts/web-demo.sh serve
```

Open `http://127.0.0.1:8000`. To choose another port:

```bash
./scripts/web-demo.sh serve 8080
```

For a build without the server, run:

```bash
./scripts/web-demo.sh build
```

The output directory is `target/web-demo`. Serve that directory over HTTP rather than opening
`index.html` directly, because browsers load the generated WebAssembly file with `fetch`.

The package contains:

- `index.html`, which owns the full-page canvas and reports startup failures;
- the small JavaScript/WebAssembly bindings generated for the WebGL and DOM calls;
- the WebAssembly module, with the prebuilt UI atlas, Suzanne mesh, and `FACEPALM.png` embedded.

The browser host requests WebGL 2 from the canvas, scales its backing buffer for the device pixel
ratio, translates pointer/wheel/keyboard input into microui events, and schedules rendering with
`requestAnimationFrame`. The full demo reserves a 2-MiB WebAssembly stack for its retained graph.
The browser build has no virtual filesystem: the file-dialog entry is disabled, while all bundled
themes and demonstration assets are embedded and remain available.

## GitHub Pages

The `WebGL demo` workflow builds the site on every push to `master` and supports manual dispatch.
It requests GitHub Pages enablement and configures Actions as the publishing source. If repository
or organization policy prevents automatic enablement, select **GitHub Actions** in
**Settings > Pages** once. A successful deployment publishes the demo at:

<https://neocogi.github.io/microui-redux/>

The workflow deploys only generated files from `target/web-demo`; generated bindings and binaries do
not need to be committed.
