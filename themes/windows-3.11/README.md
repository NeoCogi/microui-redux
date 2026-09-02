# Windows 3.11 for Workgroups example theme

This bundled theme implements the earlier Windows 3.11 visual language as a theme distinct from
Windows 95. It uses bright `#0000AA` active captions, white base captions and menus, compact
black outlines, tight white/dark-gray bevels, square controls, and the original gray application
background. Keyboard focus keeps the black period control frame instead of borrowing the blue
selection color reserved for active titles and selected rows. Pointer hover remains deliberately
subtle rather than using a modern control glow. Combo popup rows use the period blue selection with
white text for pointer hover, keyboard focus, and their combined states, while the raised header
retains independent pressed and keyboard-focused artwork.
Activation selects the blue or white title and the corresponding frame role without recoloring the
client hierarchy. Disabled foreground, background, button, and glyph artwork remains a separate
explicit state rather than an inference from which window currently owns activation.

The visual research reference was the
[B00merang Windows 3.11 GTK/Xfwm theme](https://github.com/B00merang-Project/Windows-3.11),
including its published desktop thumbnail and window-decoration geometry. That GPL repository is a
reference only: no source image, CSS, XPM, SVG, or other file was copied or transformed. Every tiny
PNG in this directory is original, deterministic pixel artwork authored for this repository and is
distributed under the repository's BSD-3-Clause terms.

The four-pixel window edge follows the period's black-gray-gray-black outline. Its four fixed
23-pixel corner cells form the long mirrored L pieces visible in the reference while remaining
independent from the four-pixel client inset and resize hit thickness. The bottom-right L is the
visible two-axis affordance; the larger semantic grip image is intentionally transparent, so no
filled rectangle appears over the client. Those L-shaped roles belong only to ordinary windows.
Modal dialogs instead use a uniform four-pixel outer frame: black in the base role and the period
`#0000AA` focus blue while active.

Menu bars and popup interiors are white, popup shells use a solid two-pixel black frame instead of
window bevels, and highlighted rows pair the `#0000AA` selection with white state-specific foregrounds.
Caption controls use original deterministic down-triangle, up-triangle, paired restore-triangle,
and close glyphs with separately shifted pressed PNGs. No reference asset was copied or transformed.
