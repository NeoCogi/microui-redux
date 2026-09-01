# Mac OS 9 example theme

This bundled theme maps the late-1990s Platinum visual language onto microui-redux's typed
appearance catalog. It uses chamfered pixel corners, black outlines, pale raised controls, recessed
white inputs, blue focus accents, striped active title artwork, and a compact diagonal resize grip.
Normal, hovered, pressed, focused, combined focus/pointer, and disabled control states are separate
PNGs; ordinary selection rows and menu backgrounds demonstrate the schema's flat fallbacks.

The visual research references were the
[B00merang Mac OS 9 GTK theme](https://github.com/B00merang-Project/Mac-OS-9) and its
[classic macOS theme gallery](https://b00merang-project.github.io/macos). Those repositories are
references only: no source image, CSS, or other file was copied or transformed. Every tiny PNG in
this directory is original, deterministic pixel artwork authored for this repository and is
distributed under the repository's BSD-3-Clause terms.

`window_frame.insets` is three pixels on every side, so its right and bottom values also define the
one-axis resize regions. The title images have zero structural insets and scale across the complete
caption allocation; the active asset retains horizontal Platinum bands while hover and drag states
select that same image rather than falling back to a solid color.
