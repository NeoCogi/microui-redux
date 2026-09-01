# Windows 3.11 for Workgroups example theme

This bundled theme implements the earlier Windows 3.11 visual language as a theme distinct from
Windows 95. It uses bright `#0000AA` active captions, white inactive captions and menus, compact
black outlines, tight white/dark-gray bevels, square controls, and the original gray application
background. Pointer hover does not brighten controls because Windows 3.11 did not use a modern
hover glow; pressed and keyboard-focused artwork remain separate states.
Deactivation is a separate top-level state: subdued foreground and background fallbacks propagate
through the complete child hierarchy, while the frame, title, and button can retain PNG artwork.

The visual research reference was the
[B00merang Windows 3.11 GTK/Xfwm theme](https://github.com/B00merang-Project/Windows-3.11),
including its published desktop thumbnail and window-decoration geometry. That GPL repository is a
reference only: no source image, CSS, XPM, SVG, or other file was copied or transformed. Every tiny
PNG in this directory is original, deterministic pixel artwork authored for this repository and is
distributed under the repository's BSD-3-Clause terms.

The four-pixel window frame follows the period's black-gray-gray-black outline, while title
backgrounds are independent flat image patches. Control source slices preserve the outer black
line, inner highlight, and double lower/right shadow without tying destination layout to bitmap
dimensions.
