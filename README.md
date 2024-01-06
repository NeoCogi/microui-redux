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
- [ ] Mechanism to garbage collect/free unused containers (use root lists and mark/sweep?)
- [ ] Software based textured rectangle clipping
- [ ] Add Atlasser to the code
- [ ] Image widget
- [ ] Documentation
- [ ] Examples

