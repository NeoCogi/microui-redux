# Windows 95 example theme

This bundled theme translates the compact rectangular controls, gray raised and recessed bevels,
navy active-window accent, muted inactive title, and square resize treatment associated with
Windows 95 into microui-redux's typed appearance roles. Its JSON deliberately supplies PNGs for
normal, hovered, pressed, focused, combined focus/pointer, and disabled control states while
leaving selection rows and title colors on the schema's flat fallbacks.

The visual research reference was the
[Chicago95 GTK/Xfce theme](https://github.com/grassmunk/Chicago95). That repository is a reference
only: no source image, CSS, or other file was copied or transformed. Every tiny PNG in this
directory is original, deterministic pixel artwork authored for this repository and is distributed
under the repository's BSD-3-Clause terms.

`window_frame.insets` is four pixels on every side. The right and bottom values consequently define
four-pixel one-axis resize hit regions, while `resize-grip.png` remains the larger bottom-right
two-axis target. The raised and recessed assets use independent two- or three-pixel source slices;
destination insets stay role-specific so focus artwork does not reflow a control.
