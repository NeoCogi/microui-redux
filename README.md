# Rxi's Microui Port to Idiomatic Rust
[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

This a port of Rxi's MicroUI to Rust language.
We tried to keep the usage pattern as close to the original as possible, but also as idiomatic to Rust as possible. In contrast with ![microui-rs](https://github.com/neocogi/microui-rs), this version uses the standard library to give us more flexibity and switch to closures for all container related operations (Window, Panel, Columns, ...).

Originally, we used C2Rust to create the initial code and iterated > 60 times to make microui-rs. This builds on top of that by much more!

## Demo
Clone and build the demo (SDL2 & glow) / Tested on linux:
```
$ cargo run --example demo-sdl2-glow-full
```

![random](https://github.com/NeoCogi/microui-redux/raw/master/res/microui.png)

## Roadmap

### Version 1.0
- [x] Use `std` (`Vec`, `parse`, ...)
- [x] Containers contain clip stack and command list
- [x] Move `begin_*`, `end_*` functions to closures
- [x] Move to AtlasRenderer Trait
- [x] Remove/Refactor `Pool`
- [x] Change layout code
- [x] Treenode as tree
- [x] Manage windows lifetime & ownership outside of context (use root windows)
- [x] Manage containers lifetime & ownership outside of contaienrs
- [x] Software based textured rectangle clipping
- [x] Add Atlasser to the code
    - [x] Runtime atlasser
        - [x] Icon
        - [x] Font (Hash Table)
    - [x] Separate Atlas Builder from the Atlas
    - [x] Builder feature
    - [x] Save Atlas to rust
    - [x] Atlas loader from const rust
- [x] Image widget
- [x] Png Atlas source
- [x] Pass-Through rendering command (for 3D viewports)
- [ ] Custom Rendering widget
    - [x] Mouse input event
    - [ ] Keyboard event
    - [ ] Text event
    - [x] Rendering
- [x] Dialog support
- [x] File dialog
- [x] API/Examples loop/iterations
    - [x] Simple example
    - [x] Full api use example (3d/dialog/..)
- [ ] Documentation

