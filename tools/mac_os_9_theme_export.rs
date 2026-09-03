//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

#![cfg(feature = "png_source")]
//! Deterministic source generator for the bundled Mac OS 9 Platinum theme pixels.
//!
//! The output is original repository artwork assembled from primitive lines and rectangles. The
//! small generator makes every authored atlas source reproducible and reviewable without importing
//! or transforming third-party theme assets.

use std::{
    env,
    error::Error,
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
};

/// One non-premultiplied RGBA pixel used by the deterministic bitmap builder.
type Pixel = [u8; 4];

/// Fully transparent pixel used only at chamfered outer corners.
const TRANSPARENT: Pixel = [0, 0, 0, 0];
/// Deep outline color used by Platinum frames and enabled controls.
const BLACK: Pixel = [0, 0, 0, 255];
/// Bright bevel and title-stripe highlight.
const WHITE: Pixel = [255, 255, 255, 255];
/// Bright inner control highlight.
const PALE: Pixel = [241, 242, 241, 255];
/// Standard Platinum window and control fill.
const FACE: Pixel = [213, 214, 213, 255];
/// Pressed control and menu-bar fill.
const PRESSED_FACE: Pixel = [184, 185, 184, 255];
/// Secondary bevel shadow.
const SHADOW: Pixel = [169, 169, 169, 255];
/// Deep bevel shadow and active-title racing stripe.
const DARK_SHADOW: Pixel = [138, 138, 138, 255];
/// Disabled outline used instead of the enabled black edge.
const DISABLED_EDGE: Pixel = [153, 153, 153, 255];
/// Pale face used by windows that do not own top-level activation.
const INACTIVE_FACE: Pixel = [227, 228, 228, 255];
/// Restrained inactive-window perimeter and title separator.
const INACTIVE_EDGE: Pixel = [104, 104, 104, 255];

/// Mutable row-major bitmap used only while exporting one PNG source image.
struct Bitmap {
    /// Pixel width written to the PNG header.
    width: u32,
    /// Pixel height written to the PNG header.
    height: u32,
    /// Contiguous non-premultiplied RGBA bytes in top-to-bottom row order.
    pixels: Vec<u8>,
}

impl Bitmap {
    /// Allocates an opaque or transparent bitmap initialized to one exact pixel value.
    fn new(width: u32, height: u32, fill: Pixel) -> Self {
        // Multiplication is performed in usize after the tiny authored dimensions are known, and
        // every destination pixel receives all four channels explicitly.
        let pixel_count = usize::try_from(width).unwrap().saturating_mul(usize::try_from(height).unwrap());
        let mut pixels = Vec::with_capacity(pixel_count.saturating_mul(4));
        for _ in 0..pixel_count {
            pixels.extend_from_slice(&fill);
        }
        Self { width, height, pixels }
    }

    /// Replaces one in-bounds pixel and ignores coordinates outside this authored bitmap.
    fn set(&mut self, x: i32, y: i32, pixel: Pixel) {
        // Bounds checks keep the drawing helpers total while they describe compact diagonal and
        // chamfer geometry near an edge.
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let index = (usize::try_from(y).unwrap() * usize::try_from(self.width).unwrap() + usize::try_from(x).unwrap()) * 4;
        self.pixels[index..index + 4].copy_from_slice(&pixel);
    }

    /// Draws an inclusive horizontal pixel run.
    fn horizontal(&mut self, x0: i32, x1: i32, y: i32, pixel: Pixel) {
        // Inclusive endpoints make the authored coordinates correspond directly to visible source
        // pixels and let `set` handle the occasional clipped endpoint.
        for x in x0..=x1 {
            self.set(x, y, pixel);
        }
    }

    /// Draws an inclusive vertical pixel run.
    fn vertical(&mut self, x: i32, y0: i32, y1: i32, pixel: Pixel) {
        // This mirrors `horizontal`, keeping all primitive artwork integer-aligned.
        for y in y0..=y1 {
            self.set(x, y, pixel);
        }
    }

    /// Draws one complete rectangular one-pixel outline at `inset`.
    fn outline(&mut self, inset: i32, pixel: Pixel) {
        // A single helper keeps each nested Platinum frame symmetric and exposes its complete
        // layer order directly at the call site instead of repeating four unrelated line calls.
        let right = self.width as i32 - inset - 1;
        let bottom = self.height as i32 - inset - 1;
        self.horizontal(inset, right, inset, pixel);
        self.horizontal(inset, right, bottom, pixel);
        self.vertical(inset, inset, bottom, pixel);
        self.vertical(right, inset, bottom, pixel);
    }

    /// Encodes this bitmap as an eight-bit RGBA PNG at `path`.
    fn write_png(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        // A buffered writer keeps the generator deterministic while avoiding small filesystem
        // writes. The PNG crate receives the exact row-major byte array without color conversion.
        let file = File::create(path)?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.pixels)?;
        Ok(())
    }
}

/// Interaction treatment applied while drawing one raised Platinum control source.
#[derive(Copy, Clone)]
enum ControlTreatment {
    /// Ordinary enabled raised control.
    Raised,
    /// Pointer-over control, intentionally restrained like the period interface.
    Hovered,
    /// Sunken control with reversed highlights.
    Pressed,
    /// Default or focused control with a doubled dark perimeter.
    Focused,
    /// Focused control while actively pressed.
    PressedFocused,
    /// Unavailable control with softened edge contrast.
    Disabled,
}

/// Caption mark embedded into one complete compact title-control face.
#[derive(Copy, Clone)]
enum CaptionMark {
    /// Empty leading close box used by the classic Mac title bar.
    Close,
    /// Horizontal windowshade mark represented by two short rules.
    WindowShade,
    /// Single inset zoom rectangle.
    Zoom,
    /// Overlapping rectangles shown while maximized.
    Restore,
}

/// Parses `--output-dir` and regenerates every bundled Mac OS 9 source image.
fn main() -> Result<(), Box<dyn Error>> {
    // Keeping argument parsing and export separate makes the filesystem boundary small and lets
    // the artwork functions remain deterministic pure constructors.
    let output_dir = parse_output_dir()?;
    export_theme(&output_dir)
}

/// Extracts the required output directory from the generator's deliberately small CLI.
fn parse_output_dir() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--output-dir" {
            // Report the precise missing option rather than accepting an implicit working
            // directory that could overwrite unrelated PNG files.
            return args.next().map(PathBuf::from).ok_or_else(|| "--output-dir requires a path".into());
        }
    }
    Err("missing --output-dir <path>".into())
}

/// Writes the complete, named set of original Platinum source images.
fn export_theme(output_dir: &Path) -> Result<(), Box<dyn Error>> {
    // The destination is an existing version-controlled theme directory; refusing to create it
    // catches misspellings instead of scattering generated files elsewhere.
    if !output_dir.is_dir() {
        return Err(format!("theme output directory does not exist: {}", output_dir.display()).into());
    }

    let images = [
        ("raised.png", raised_control(ControlTreatment::Raised)),
        ("raised-hovered.png", raised_control(ControlTreatment::Hovered)),
        ("pressed.png", raised_control(ControlTreatment::Pressed)),
        ("focused.png", raised_control(ControlTreatment::Focused)),
        ("pressed-focused.png", raised_control(ControlTreatment::PressedFocused)),
        ("disabled.png", raised_control(ControlTreatment::Disabled)),
        ("recessed.png", recessed_control(false)),
        ("recessed-focused.png", recessed_control(true)),
        ("resize-grip.png", resize_grip()),
        ("title-active.png", title_strip(true)),
        ("title-normal.png", title_strip(false)),
        ("window-frame-active.png", window_frame(true)),
        ("window-frame.png", window_frame(false)),
        ("caption-close.png", caption_button(CaptionMark::Close, false)),
        ("caption-close-pressed.png", caption_button(CaptionMark::Close, true)),
        ("caption-minimize.png", caption_button(CaptionMark::WindowShade, false)),
        ("caption-minimize-pressed.png", caption_button(CaptionMark::WindowShade, true)),
        ("caption-maximize.png", caption_button(CaptionMark::Zoom, false)),
        ("caption-maximize-pressed.png", caption_button(CaptionMark::Zoom, true)),
        ("caption-restore.png", caption_button(CaptionMark::Restore, false)),
        ("caption-restore-pressed.png", caption_button(CaptionMark::Restore, true)),
        ("menu-bar.png", menu_bar()),
        ("menu-popup.png", menu_popup()),
        ("menu-selection.png", Bitmap::new(1, 1, BLACK)),
        ("icon-close.png", close_icon()),
        ("icon-expand.png", expand_icon()),
        ("icon-collapse.png", collapse_icon()),
        ("icon-check.png", check_icon()),
        ("icon-expand-down.png", collapse_icon()),
        ("icon-open-folder.png", open_folder_icon()),
        ("icon-closed-folder.png", closed_folder_icon()),
        ("icon-file.png", file_icon()),
    ];
    for (name, bitmap) in images {
        // Each stable filename is also the JSON texture-cache key, so regeneration preserves the
        // theme definition while replacing every source pixel atomically per file.
        bitmap.write_png(&output_dir.join(name))?;
    }
    Ok(())
}

/// Builds a thirteen-pixel chamfered raised, pressed, focused, or disabled control source.
fn raised_control(treatment: ControlTreatment) -> Bitmap {
    let pressed = matches!(treatment, ControlTreatment::Pressed | ControlTreatment::PressedFocused);
    let focused = matches!(treatment, ControlTreatment::Focused | ControlTreatment::PressedFocused);
    let disabled = matches!(treatment, ControlTreatment::Disabled);
    let fill = if pressed { PRESSED_FACE } else { FACE };
    let outline = if disabled { DISABLED_EDGE } else { BLACK };
    let mut bitmap = Bitmap::new(13, 13, fill);

    // Transparent corner pixels and diagonal joins produce the clipped rectangular outline seen on
    // Platinum push buttons without requiring curved or antialiased source pixels.
    for (x, y) in [
        (0, 0),
        (1, 0),
        (0, 1),
        (11, 0),
        (12, 0),
        (12, 1),
        (0, 11),
        (0, 12),
        (1, 12),
        (12, 11),
        (11, 12),
        (12, 12),
    ] {
        bitmap.set(x, y, TRANSPARENT);
    }
    bitmap.horizontal(2, 10, 0, outline);
    bitmap.horizontal(2, 10, 12, outline);
    bitmap.vertical(0, 2, 10, outline);
    bitmap.vertical(12, 2, 10, outline);
    for (x, y) in [(1, 1), (11, 1), (1, 11), (11, 11)] {
        bitmap.set(x, y, outline);
    }

    let upper = if pressed { DARK_SHADOW } else { WHITE };
    let lower = if pressed { WHITE } else { DARK_SHADOW };
    bitmap.horizontal(2, 10, 1, upper);
    bitmap.vertical(1, 2, 10, upper);
    bitmap.horizontal(2, 10, 11, lower);
    bitmap.vertical(11, 2, 10, lower);
    bitmap.horizontal(2, 9, 2, if disabled { PALE } else { upper });
    bitmap.vertical(2, 2, 9, if disabled { PALE } else { upper });
    bitmap.horizontal(3, 9, 10, SHADOW);
    bitmap.vertical(10, 3, 9, SHADOW);

    if focused {
        // A dark inner keyline gives the default button a period-appropriate stronger outline while
        // retaining the same five-pixel stretch-safe border span.
        bitmap.horizontal(3, 9, 3, BLACK);
        bitmap.horizontal(3, 9, 9, BLACK);
        bitmap.vertical(3, 3, 9, BLACK);
        bitmap.vertical(9, 3, 9, BLACK);
    }
    bitmap
}

/// Builds an eleven-pixel inset text, checkbox, or track source.
fn recessed_control(focused: bool) -> Bitmap {
    let mut bitmap = Bitmap::new(11, 11, WHITE);
    // The upper and leading shadow sinks the white field while the lower and trailing highlight
    // preserves the raised outer-window lighting direction.
    bitmap.horizontal(0, 10, 0, BLACK);
    bitmap.vertical(0, 0, 10, BLACK);
    bitmap.horizontal(1, 10, 10, WHITE);
    bitmap.vertical(10, 1, 10, WHITE);
    bitmap.horizontal(1, 9, 1, DARK_SHADOW);
    bitmap.vertical(1, 1, 9, DARK_SHADOW);
    bitmap.horizontal(2, 9, 9, PALE);
    bitmap.vertical(9, 2, 9, PALE);
    if focused {
        // Platinum uses a black inner focus keyline rather than recoloring the entire input field.
        bitmap.horizontal(2, 8, 2, BLACK);
        bitmap.horizontal(2, 8, 8, BLACK);
        bitmap.vertical(2, 2, 8, BLACK);
        bitmap.vertical(8, 2, 8, BLACK);
    }
    bitmap
}

/// Builds the active racing-stripe title source or the restrained inactive title source.
fn title_strip(active: bool) -> Bitmap {
    let mut bitmap = Bitmap::new(8, 20, if active { FACE } else { INACTIVE_FACE });
    if active {
        // Alternating one-pixel highlights and shadows form the six racing-stripe pairs around the
        // centered title backdrop. Neutral rows above and below keep the bar from looking striped
        // edge-to-edge.
        for y in (2..14).step_by(2) {
            bitmap.horizontal(0, 7, y, WHITE);
            bitmap.horizontal(0, 7, y + 1, DARK_SHADOW);
        }
    }
    // The title owns the separator below itself because the outer frame surrounds the complete
    // root and cannot otherwise divide title chrome from a menu bar or application body.
    bitmap.horizontal(0, 7, 18, if active { SHADOW } else { INACTIVE_FACE });
    bitmap.horizontal(0, 7, 19, if active { BLACK } else { INACTIVE_EDGE });
    bitmap
}

/// Builds the base or active six-layer Platinum window perimeter.
fn window_frame(active: bool) -> Bitmap {
    let mut bitmap = Bitmap::new(13, 13, if active { FACE } else { INACTIVE_FACE });
    // Both activation states retain the same six-pixel geometry. Active chrome uses the directional
    // black/white/face/shadow stack; inactive chrome becomes a pale slab inside a restrained edge.
    let layers = if active {
        [BLACK, WHITE, FACE, FACE, SHADOW, BLACK]
    } else {
        [INACTIVE_EDGE, INACTIVE_FACE, INACTIVE_FACE, INACTIVE_FACE, INACTIVE_FACE, INACTIVE_EDGE]
    };
    for (inset, pixel) in layers.into_iter().enumerate() {
        bitmap.outline(inset as i32, pixel);
    }
    bitmap
}

/// Builds one complete thirteen-by-fourteen title-control face with its embedded period mark.
fn caption_button(mark: CaptionMark, pressed: bool) -> Bitmap {
    let mut bitmap = Bitmap::new(13, 14, if pressed { PRESSED_FACE } else { FACE });
    // Caption boxes use a dense square keyline rather than the chamfered button shape. The spare
    // bottom row blends into the title while the upper thirteen rows carry the complete face.
    bitmap.horizontal(0, 11, 0, SHADOW);
    bitmap.vertical(0, 0, 11, SHADOW);
    bitmap.horizontal(0, 11, 12, WHITE);
    bitmap.vertical(12, 0, 12, WHITE);
    bitmap.outline(1, BLACK);
    let upper = if pressed { DARK_SHADOW } else { WHITE };
    let lower = if pressed { WHITE } else { DARK_SHADOW };
    bitmap.horizontal(2, 10, 2, upper);
    bitmap.vertical(2, 2, 10, upper);
    bitmap.horizontal(2, 10, 10, lower);
    bitmap.vertical(10, 2, 10, lower);

    // Five hard diagonal bands suggest Platinum's metallic title controls without introducing
    // gradients or antialiasing into the deterministic pixel source.
    for y in 3..10 {
        for x in 3..10 {
            let shade = match (x + y) / 3 {
                2 => SHADOW,
                3 => PRESSED_FACE,
                4 => FACE,
                5 => PALE,
                _ => WHITE,
            };
            bitmap.set(x, y, if pressed { PRESSED_FACE } else { shade });
        }
    }

    let offset = i32::from(pressed);
    match mark {
        CaptionMark::Close => {
            // Classic Mac OS close boxes are intentionally blank until interaction or document
            // state supplies meaning; the beveled face itself is the complete mark.
        }
        CaptionMark::WindowShade => {
            bitmap.horizontal(3 + offset, 8 + offset, 5 + offset, BLACK);
        }
        CaptionMark::Zoom => {
            bitmap.horizontal(6 + offset, 6 + offset, 3 + offset, BLACK);
            bitmap.vertical(6 + offset, 3 + offset, 8 + offset, BLACK);
            bitmap.horizontal(3 + offset, 8 + offset, 8 + offset, BLACK);
        }
        CaptionMark::Restore => {
            bitmap.horizontal(7 + offset, 7 + offset, 3 + offset, BLACK);
            bitmap.vertical(7 + offset, 3 + offset, 7 + offset, BLACK);
            bitmap.horizontal(3 + offset, 8 + offset, 7 + offset, BLACK);
            bitmap.horizontal(3 + offset, 8 + offset, 9 + offset, BLACK);
        }
    }
    bitmap
}

/// Builds the stretchable menu-bar background with its lower divider rule.
fn menu_bar() -> Bitmap {
    let mut bitmap = Bitmap::new(3, 3, FACE);
    // A single black lower rule separates the global-style menu strip from the window content while
    // the remaining source center stretches without introducing a raised button frame.
    bitmap.horizontal(0, 2, 2, BLACK);
    bitmap
}

/// Builds a thick black popup perimeter with a compact Platinum inner bevel.
fn menu_popup() -> Bitmap {
    let mut bitmap = Bitmap::new(7, 7, FACE);
    // Two black outer pixels satisfy the visually heavy popup edge while the white/dark inner bevel
    // retains the same top-left lighting direction as windows and controls.
    bitmap.horizontal(0, 6, 0, BLACK);
    bitmap.horizontal(0, 6, 1, BLACK);
    bitmap.horizontal(0, 6, 5, BLACK);
    bitmap.horizontal(0, 6, 6, BLACK);
    bitmap.vertical(0, 0, 6, BLACK);
    bitmap.vertical(1, 0, 6, BLACK);
    bitmap.vertical(5, 0, 6, BLACK);
    bitmap.vertical(6, 0, 6, BLACK);
    bitmap.horizontal(2, 4, 2, WHITE);
    bitmap.vertical(2, 2, 4, WHITE);
    bitmap.horizontal(2, 4, 4, DARK_SHADOW);
    bitmap.vertical(4, 2, 4, DARK_SHADOW);
    bitmap.set(3, 3, FACE);
    bitmap
}

/// Builds a compact mask for the manager's separately painted close glyph.
fn close_icon() -> Bitmap {
    let mut bitmap = Bitmap::new(9, 9, TRANSPARENT);
    // Semantic icons are opaque-white masks because widget paint supplies the state-dependent
    // content color. Two-pixel diagonals retain a crisp X when the icon is centered or clipped.
    for coordinate in 2..=6 {
        bitmap.set(coordinate, coordinate, WHITE);
        bitmap.set(coordinate + 1, coordinate, WHITE);
        bitmap.set(8 - coordinate, coordinate, WHITE);
        bitmap.set(7 - coordinate, coordinate, WHITE);
    }
    bitmap
}

/// Builds the right-facing disclosure and submenu triangle used by collapsed branches.
fn expand_icon() -> Bitmap {
    let mut bitmap = Bitmap::new(9, 9, TRANSPARENT);
    // A stepped one-bit triangle matches Platinum's compact disclosure language and remains
    // legible in either black normal text or white selected-menu text.
    for (y, right) in [(1, 2), (2, 3), (3, 5), (4, 7), (5, 5), (6, 3), (7, 2)] {
        bitmap.horizontal(2, right, y, WHITE);
    }
    bitmap
}

/// Builds the downward disclosure and combo indicator used by expanded branches.
fn collapse_icon() -> Bitmap {
    let mut bitmap = Bitmap::new(9, 9, TRANSPARENT);
    // The broad top and centered point are the vertical counterpart of `expand_icon`; sharing this
    // source with combo boxes keeps identical semantic arrows identical in the final atlas.
    for (y, left, right) in [(2, 1, 7), (3, 2, 6), (4, 3, 5), (5, 4, 4)] {
        bitmap.horizontal(left, right, y, WHITE);
    }
    bitmap
}

/// Builds the angled one-bit check used by checkboxes and checked menu rows.
fn check_icon() -> Bitmap {
    let mut bitmap = Bitmap::new(11, 11, TRANSPARENT);
    // The short rising arm and longer falling arm are deliberately doubled rather than
    // antialiased, preserving the dense small-scale mark of the original interface.
    for (x, y) in [(1, 5), (2, 6), (3, 7), (4, 6), (5, 5), (6, 4), (7, 3), (8, 2), (9, 1)] {
        bitmap.set(x, y, WHITE);
        bitmap.set(x, y + 1, WHITE);
    }
    bitmap
}

/// Builds a closed-folder outline that remains recognizable under the widget content tint.
fn closed_folder_icon() -> Bitmap {
    let mut bitmap = Bitmap::new(16, 16, TRANSPARENT);
    // Semantic icons currently paint as tinted masks, so the folder uses a strong period outline
    // instead of embedding colors that the widget could not preserve across selection states.
    bitmap.horizontal(1, 6, 3, WHITE);
    bitmap.horizontal(1, 14, 5, WHITE);
    bitmap.horizontal(1, 14, 13, WHITE);
    bitmap.vertical(1, 3, 13, WHITE);
    bitmap.vertical(6, 3, 5, WHITE);
    bitmap.vertical(14, 5, 13, WHITE);
    bitmap
}

/// Builds an open-folder outline with the front leaf lowered toward the viewer.
fn open_folder_icon() -> Bitmap {
    let mut bitmap = closed_folder_icon();
    // Overlay the characteristic open leaf: a sloped upper edge, a lower baseline, and short side
    // joins. The retained back-tab makes open and closed states related rather than unrelated art.
    bitmap.horizontal(3, 14, 7, WHITE);
    bitmap.horizontal(2, 13, 14, WHITE);
    bitmap.set(2, 8, WHITE);
    bitmap.set(2, 9, WHITE);
    bitmap.set(1, 10, WHITE);
    bitmap.set(1, 11, WHITE);
    bitmap.set(1, 12, WHITE);
    bitmap.set(1, 13, WHITE);
    bitmap.set(14, 8, WHITE);
    bitmap.set(14, 9, WHITE);
    bitmap.set(13, 10, WHITE);
    bitmap.set(13, 11, WHITE);
    bitmap.set(13, 12, WHITE);
    bitmap.set(13, 13, WHITE);
    bitmap
}

/// Builds the outlined document page used by ordinary file-dialog rows.
fn file_icon() -> Bitmap {
    let mut bitmap = Bitmap::new(16, 16, TRANSPARENT);
    // The clipped upper-right corner and its two-pixel fold distinguish a document from a folder
    // without relying on color, gradients, or any pixels imported from the reference project.
    bitmap.horizontal(3, 9, 1, WHITE);
    bitmap.vertical(3, 1, 14, WHITE);
    bitmap.horizontal(3, 12, 14, WHITE);
    bitmap.vertical(12, 4, 14, WHITE);
    bitmap.set(10, 2, WHITE);
    bitmap.set(11, 3, WHITE);
    bitmap.horizontal(9, 12, 4, WHITE);
    bitmap.vertical(9, 1, 4, WHITE);
    bitmap.horizontal(5, 10, 7, WHITE);
    bitmap.horizontal(5, 10, 10, WHITE);
    bitmap
}

/// Builds the opaque sixteen-pixel lower-right diagonal resize grip.
fn resize_grip() -> Bitmap {
    let mut bitmap = Bitmap::new(16, 16, FACE);
    // Each descending black rule has a white parallel highlight one pixel inward, matching the
    // grooved diagonal handle visible in resizable Platinum windows.
    for inset in [2, 6, 10] {
        for step in 0..=inset {
            bitmap.set(15 - inset + step, 15 - step, BLACK);
            bitmap.set(14 - inset + step, 15 - step, WHITE);
        }
    }
    bitmap
}
