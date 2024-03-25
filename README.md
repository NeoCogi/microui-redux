# Rxi's Microui Port to Rust
[![Crate](https://img.shields.io/crates/v/microui-redux.svg)](https://crates.io/crates/microui-redux)

This a port of Rxi's MicroUI to Rust language. 
We tried to keep the usage pattern as close to the original as possible, but also as idiomatic to Rust as possible. In contrast with ![microui-rs](https://github.com/neocogi/microui-rs), this version uses the standard library to give us more flexibity and switch to closures for all container related operations (Window, Panel, Columns, ...).

We used C2Rust to create the initial code and iterated > 60 times to get it to where it is now. Few bugs are lingering (Lost to translation!), be advised!

## Demo
Clone and build the demo (SDL2 & glow) / Tested on linux:
```
$ cargo run --example demo-sdl2
```

![random](https://github.com/eloraiby/microui-redux/raw/master/res/microui.png)

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
- [ ] Pass-Through rendering command (for 3D viewports)
- [ ] Documentation
- [ ] Examples

