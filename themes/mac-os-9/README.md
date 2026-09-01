# Mac OS 9 example theme

This bundled theme maps the late-1990s Platinum visual language onto microui-redux's typed
appearance catalog. It uses chamfered pixel corners, black outlines, pale raised controls, recessed
off-white inputs, restrained lavender focus accents, gray racing-stripe active title artwork, a
flat receding inactive frame, and a compact beveled resize grip.
Normal, hovered, pressed, focused, combined focus/pointer, disabled, and top-level inactive control states are separate
PNGs; ordinary selection rows and menu backgrounds demonstrate the schema's flat fallbacks.

The visual research references were the
[Apple Mac OS 8 Human Interface Guidelines](https://dev.os9.ca/techpubs/mac/pdf/HIGOS8Guidelines.pdf)
and the [classic-stylesheets Mac OS 9 theme](https://github.com/nielssp/classic-stylesheets). The
guidelines document active racing stripes and the flat light-gray inactive frame; the independent
stylesheet reconstruction was used to cross-check the seven-gray Platinum palette and one-pixel
geometry. No source image, CSS, SVG, or other file was copied or transformed. Every tiny PNG in this
directory is original, deterministic pixel artwork authored for this repository and is distributed
under the repository's BSD-3-Clause terms.

`window_frame.insets` is three pixels on every side, so its right and bottom values also define the
one-axis resize regions. The title images have zero structural insets and scale across the complete
caption allocation; active horizontal bands alternate `#777777` and white over `#CCCCCC`, while the
inactive title remains flat `#DDDDDD`.
