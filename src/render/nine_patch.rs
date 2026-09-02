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

//! Concrete three-by-three patch descriptions and destination geometry.
//!
//! A patch is deliberately a small value rather than an erased paint callback. Skin code can
//! therefore describe a background with nine typed cells, the display list can retain that exact
//! description, and the renderer can expand it without consulting a widget or theme registry.

use crate::{Color, IconId, Recti};

/// Insets separating the fixed outer rows and columns from a stretchable center cell.
///
/// Values are expressed in destination pixels. Negative values are accepted at construction
/// boundaries and normalized to zero when geometry is resolved, keeping malformed skin input from
/// producing inverted rectangles.
#[derive(Copy, Clone, Debug, Default)]
pub struct SliceInsets {
    /// Width of the left column.
    pub left: i32,
    /// Height of the top row.
    pub top: i32,
    /// Width of the right column.
    pub right: i32,
    /// Height of the bottom row.
    pub bottom: i32,
}

impl SliceInsets {
    /// Insets that leave the complete destination to the center cell.
    pub const ZERO: Self = Self::uniform(0);

    /// Creates four equal insets from one convenient scalar value.
    pub const fn uniform(value: i32) -> Self {
        // Store the caller's exact value. Geometry normalization is intentionally deferred so
        // public skin mutation remains transparent and every consumer applies the same policy.
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    /// Creates independently configurable left, top, right, and bottom insets.
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        // A plain constructor keeps theme deserialization and programmatic skin construction on
        // the same concrete representation without introducing builder-only mirror types.
        Self { left, top, right, bottom }
    }

    /// Returns an equivalent value whose four components are non-negative.
    pub(crate) fn normalized(self) -> Self {
        // Clamp once before either layout or painting arithmetic. This prevents the two paths from
        // disagreeing when an application installs a skin containing a negative inset.
        Self {
            left: self.left.max(0),
            top: self.top.max(0),
            right: self.right.max(0),
            bottom: self.bottom.max(0),
        }
    }

    /// Returns the combined horizontal inset using saturating arithmetic.
    pub(crate) fn horizontal_extent(self) -> i32 {
        // Normalization makes both operands non-negative; saturation still protects the public i32
        // style domain from overflowing while constraints are derived.
        let insets = self.normalized();
        insets.left.saturating_add(insets.right)
    }

    /// Returns the combined vertical inset using saturating arithmetic.
    pub(crate) fn vertical_extent(self) -> i32 {
        // Match horizontal extent policy so measurement cannot produce axis-dependent overflow
        // behavior for otherwise equivalent theme values.
        let insets = self.normalized();
        insets.top.saturating_add(insets.bottom)
    }

    /// Returns the widest component for scalar effects such as a focus outline.
    pub(crate) fn maximum_component(self) -> i32 {
        // The maximum is derived after normalization because a negative style component contributes
        // no visible or structural thickness anywhere else in the patch pipeline.
        let insets = self.normalized();
        insets.left.max(insets.top).max(insets.right).max(insets.bottom)
    }

    /// Raises every normalized component to at least `minimum`.
    pub(crate) fn at_least(self, minimum: i32) -> Self {
        // Active outlines use this to remain visible when an ordinary frame has zero thickness.
        // Normalize both inputs so the helper cannot reintroduce negative patch geometry.
        let insets = self.normalized();
        let minimum = minimum.max(0);
        Self {
            left: insets.left.max(minimum),
            top: insets.top.max(minimum),
            right: insets.right.max(minimum),
            bottom: insets.bottom.max(minimum),
        }
    }
}

/// Concrete flat content drawn in one cell of a [`NinePatch`].
///
/// The enum is intentionally closed and typed. A complete image-backed patch uses the separate
/// [`NinePatchImage`] representation, so neither rendering mode needs a dynamic payload or a
/// `std::any::Any` downcast.
#[derive(Copy, Clone)]
pub enum NinePatchCell {
    /// Leaves the cell transparent and records no renderer work.
    Empty,
    /// Fills the complete cell with one premultiplier-independent RGBA color.
    Color {
        /// Color submitted through the renderer's opaque-white atlas tile.
        color: Color,
    },
}

impl NinePatchCell {
    /// Creates a color cell while treating a fully transparent color as an empty cell.
    pub const fn color(color: Color) -> Self {
        // Canonicalizing transparent colors here makes visibility checks and renderer execution
        // agree without repeatedly inspecting alpha in every patch consumer.
        if color.a == 0 { Self::Empty } else { Self::Color { color } }
    }

    /// Reports whether this cell can contribute any visible pixels.
    pub(crate) const fn is_visible(self) -> bool {
        // Empty is the only non-rendering variant. Future image cells can extend this exhaustive
        // match with their own explicit alpha or resource policy.
        matches!(self, Self::Color { .. })
    }
}

/// Nine named cells arranged in top-to-bottom, left-to-right order.
///
/// Named fields keep style and theme construction readable and prevent a positional array from
/// silently swapping an edge or corner. Renderer traversal still obtains one fixed array without
/// allocation through a private row-conversion helper.
#[derive(Copy, Clone)]
pub struct NinePatchCells {
    /// Fixed top-left corner.
    pub top_left: NinePatchCell,
    /// Horizontally stretchable top edge.
    pub top: NinePatchCell,
    /// Fixed top-right corner.
    pub top_right: NinePatchCell,
    /// Vertically stretchable left edge.
    pub left: NinePatchCell,
    /// Horizontally and vertically stretchable center.
    pub center: NinePatchCell,
    /// Vertically stretchable right edge.
    pub right: NinePatchCell,
    /// Fixed bottom-left corner.
    pub bottom_left: NinePatchCell,
    /// Horizontally stretchable bottom edge.
    pub bottom: NinePatchCell,
    /// Fixed bottom-right corner.
    pub bottom_right: NinePatchCell,
}

/// One atlas bitmap divided into a three-by-three image grid.
///
/// Source insets are measured inside the referenced atlas icon and may differ from destination
/// [`SliceInsets`]. The opaque [`IconId`] keeps theme artwork tied to its exact immutable atlas,
/// while ordinary application images continue to use the separate external-texture draw path.
#[derive(Copy, Clone)]
pub struct NinePatchImage {
    /// Atlas capability whose rectangle contains all nine source cells.
    pub icon: IconId,
    /// Fixed source rows and columns measured inside the icon rectangle.
    pub source_insets: SliceInsets,
    /// RGBA modulation applied uniformly to all nine sampled cells.
    pub tint: Color,
}

impl NinePatchImage {
    /// Creates one explicitly sliced atlas-image description.
    pub const fn new(icon: IconId, source_insets: SliceInsets, tint: Color) -> Self {
        // Retain the capability rather than raw UV coordinates. Atlas ownership validation and
        // rectangle lookup can then remain centralized at Skin and renderer boundaries.
        Self { icon, source_insets, tint }
    }
}

/// Closed content vocabulary for a complete three-by-three patch.
#[derive(Copy, Clone)]
pub enum NinePatchContent {
    /// Nine independently visible flat-color cells.
    Flat {
        /// Named cells traversed in geometric order by the renderer.
        cells: NinePatchCells,
    },
    /// One texture source divided by its own source-space slice insets.
    Image {
        /// Complete typed image description.
        image: NinePatchImage,
    },
}

impl NinePatchCells {
    /// Creates a grid whose complete visible area uses one cell value.
    pub const fn all(cell: NinePatchCell) -> Self {
        // Copy the typed value into every named position. This is useful for theme defaults whose
        // edge and center appearance are intentionally identical.
        Self {
            top_left: cell,
            top: cell,
            top_right: cell,
            left: cell,
            center: cell,
            right: cell,
            bottom_left: cell,
            bottom: cell,
            bottom_right: cell,
        }
    }

    /// Creates a transparent grid with only its center cell populated.
    pub const fn center(cell: NinePatchCell) -> Self {
        // A center-only patch is the three-by-three representation of the former filled quad.
        // Keeping it in this vocabulary ensures flat and image themes share one display operation.
        Self {
            top_left: NinePatchCell::Empty,
            top: NinePatchCell::Empty,
            top_right: NinePatchCell::Empty,
            left: NinePatchCell::Empty,
            center: cell,
            right: NinePatchCell::Empty,
            bottom_left: NinePatchCell::Empty,
            bottom: NinePatchCell::Empty,
            bottom_right: NinePatchCell::Empty,
        }
    }

    /// Creates equal border cells around an independently configured center.
    pub const fn framed(border: NinePatchCell, center: NinePatchCell) -> Self {
        // Corners and edges deliberately share one value for the current flat skin. The named
        // fields remain independently mutable for later classic raised/sunken theme definitions.
        Self {
            top_left: border,
            top: border,
            top_right: border,
            left: border,
            center,
            right: border,
            bottom_left: border,
            bottom: border,
            bottom_right: border,
        }
    }

    /// Returns rows in geometric order without allocating or exposing positional storage publicly.
    pub(crate) const fn rows(self) -> [[NinePatchCell; 3]; 3] {
        // Renderer and diagnostic expansion consume the same ordering, making named construction
        // authoritative while avoiding nine repeated matches at each call site.
        [
            [self.top_left, self.top, self.top_right],
            [self.left, self.center, self.right],
            [self.bottom_left, self.bottom, self.bottom_right],
        ]
    }

    /// Reports whether at least one cell can produce visible output.
    pub(crate) const fn has_visible_cell(self) -> bool {
        // Spell out the fixed cells so the result stays allocation-free and a future field addition
        // cannot accidentally be omitted from an iterator assembled elsewhere.
        self.top_left.is_visible()
            || self.top.is_visible()
            || self.top_right.is_visible()
            || self.left.is_visible()
            || self.center.is_visible()
            || self.right.is_visible()
            || self.bottom_left.is_visible()
            || self.bottom.is_visible()
            || self.bottom_right.is_visible()
    }

    /// Reports whether one of the eight cells surrounding the center can produce pixels.
    pub(crate) const fn has_visible_border(self) -> bool {
        // Border-only window overlays must not be recorded when a flat patch contains only a center
        // fill. Keeping this fixed expression adjacent to `has_visible_cell` makes the omission of
        // the center deliberate and reviewable.
        self.top_left.is_visible()
            || self.top.is_visible()
            || self.top_right.is_visible()
            || self.left.is_visible()
            || self.right.is_visible()
            || self.bottom_left.is_visible()
            || self.bottom.is_visible()
            || self.bottom_right.is_visible()
    }
}

/// Backend-neutral visual patch divided into three rows and three columns.
///
/// The outer columns and rows retain the requested inset thickness while the center consumes the
/// remaining destination. If a destination is too small, opposing insets are reduced
/// proportionally so generated cells never overlap or escape the supplied rectangle.
#[derive(Copy, Clone)]
pub struct NinePatch {
    /// Destination-space sizes of the outer columns and rows.
    pub insets: SliceInsets,
    /// Concrete flat or image content associated with the complete grid.
    pub content: NinePatchContent,
    /// Whether expansion includes the stretchable center cell.
    pub center_visible: bool,
}

impl NinePatch {
    /// Creates a flat patch from explicit destination insets and named cells.
    pub const fn new(insets: SliceInsets, cells: NinePatchCells) -> Self {
        // Preserve exact author input. All geometry consumers normalize through [`Self::geometry`]
        // so skin construction does not need a second validation representation.
        Self {
            insets,
            content: NinePatchContent::Flat { cells },
            center_visible: true,
        }
    }

    /// Creates an image patch with independent destination and source slice geometry.
    pub const fn image(insets: SliceInsets, image: NinePatchImage) -> Self {
        // Store the complete source description in one explicit enum variant. Image patches never
        // masquerade as nine unrelated external images and therefore remain easy to validate.
        Self {
            insets,
            content: NinePatchContent::Image { image },
            center_visible: true,
        }
    }

    /// Creates the three-by-three equivalent of one solid filled rectangle.
    pub const fn solid(color: Color) -> Self {
        // Zero outer insets collapse the eight surrounding cells and assign the full destination
        // to the center color cell.
        Self::new(SliceInsets::ZERO, NinePatchCells::center(NinePatchCell::color(color)))
    }

    /// Creates a uniformly colored frame around an optional center fill.
    pub const fn framed(insets: SliceInsets, border: Color, fill: Option<Color>) -> Self {
        // Absence and transparent color both become an empty center. This preserves structural
        // insets without requiring a visible fill operation.
        let center = match fill {
            Some(color) => NinePatchCell::color(color),
            None => NinePatchCell::Empty,
        };
        Self::new(insets, NinePatchCells::framed(NinePatchCell::color(border), center))
    }

    /// Replaces only the center cell while retaining every edge and corner.
    pub const fn with_center(mut self, fill: Option<Color>) -> Self {
        // Only flat content has an independently replaceable center. A theme image already owns
        // its complete center artwork, so deriving a role-specific flat fill leaves it unchanged.
        if let NinePatchContent::Flat { cells } = &mut self.content {
            cells.center = match fill {
                Some(color) => NinePatchCell::color(color),
                None => NinePatchCell::Empty,
            };
        }
        self
    }

    /// Replaces destination insets while retaining the complete flat or image payload.
    pub const fn with_insets(mut self, insets: SliceInsets) -> Self {
        // Theme documents use this when a role supplies structural insets but omits one or more PNG
        // states. Those states keep their flat content while matching the role's requested layout.
        self.insets = insets;
        self
    }

    /// Omits the center cell while retaining all eight border cells.
    pub const fn without_center(mut self) -> Self {
        // Window frames are initially recorded below their client content, then repeat only their
        // edges above descendants. A typed visibility bit applies identically to flat and image
        // patches and avoids manufacturing a second border-only image representation.
        self.center_visible = false;
        self
    }

    /// Reports whether recording this patch can produce visible renderer work.
    pub(crate) fn is_visible(self) -> bool {
        // Destination area is checked by Painter because it owns coordinates. Flat patches inspect
        // their cells, while an image needs positive source geometry and non-zero modulation alpha.
        match self.content {
            NinePatchContent::Flat { cells } => {
                if self.center_visible {
                    cells.has_visible_cell()
                } else {
                    cells.has_visible_border()
                }
            }
            NinePatchContent::Image { image } => {
                // A border-only image also requires at least one positive destination inset. With
                // four zero insets all eight border destinations collapse to empty rectangles.
                let has_destination = self.center_visible || self.insets.maximum_component() > 0;
                has_destination && image.tint.a != 0
            }
        }
    }

    /// Returns the image payload when this patch references atlas-backed artwork.
    pub(crate) const fn image_content(self) -> Option<NinePatchImage> {
        // The exhaustive match gives renderer preflight a typed ownership query without inspecting
        // private enum layout or maintaining a parallel list of theme atlas regions.
        match self.content {
            NinePatchContent::Flat { .. } => None,
            NinePatchContent::Image { image } => Some(image),
        }
    }

    /// Resolves the nine destination rectangles for one authoritative outer allocation.
    pub(crate) fn geometry(self, outer: Recti) -> [[Recti; 3]; 3] {
        // Resolve each axis independently. The returned partitions exactly consume the positive
        // destination extent and remain zero-sized for a non-positive axis.
        geometry_with_insets(outer, self.insets)
    }
}

/// Resolves one rectangle into nine exact cells using the supplied slice insets.
pub(crate) fn geometry_with_insets(outer: Recti, insets: SliceInsets) -> [[Recti; 3]; 3] {
    // Source and destination grids share this partition policy. Source documents are validated
    // to fit, while tiny destinations may proportionally collapse their fixed outer cells.
    let insets = insets.normalized();
    let widths = partition_axis(outer.width, insets.left, insets.right);
    let heights = partition_axis(outer.height, insets.top, insets.bottom);
    let xs = partition_origins(outer.x, widths);
    let ys = partition_origins(outer.y, heights);

    // Construct the fixed grid explicitly so callers cannot confuse row and column ordering.
    [
        [
            Recti::new(xs[0], ys[0], widths[0], heights[0]),
            Recti::new(xs[1], ys[0], widths[1], heights[0]),
            Recti::new(xs[2], ys[0], widths[2], heights[0]),
        ],
        [
            Recti::new(xs[0], ys[1], widths[0], heights[1]),
            Recti::new(xs[1], ys[1], widths[1], heights[1]),
            Recti::new(xs[2], ys[1], widths[2], heights[1]),
        ],
        [
            Recti::new(xs[0], ys[2], widths[0], heights[2]),
            Recti::new(xs[1], ys[2], widths[1], heights[2]),
            Recti::new(xs[2], ys[2], widths[2], heights[2]),
        ],
    ]
}

/// Divides one non-negative destination extent into leading, center, and trailing lengths.
fn partition_axis(extent: i32, leading: i32, trailing: i32) -> [i32; 3] {
    // Non-positive destinations have no drawable cells. Keeping their partitions at zero avoids
    // manufacturing positive geometry from an invalid outer rectangle.
    let extent = extent.max(0);
    if extent == 0 {
        return [0, 0, 0];
    }

    let leading = leading.max(0);
    let trailing = trailing.max(0);
    let requested = leading.saturating_add(trailing);
    if requested <= extent {
        // The requested fixed cells fit exactly, leaving all remaining pixels to the center.
        return [leading, extent - requested, trailing];
    }
    if requested == 0 {
        // This branch is defensive because a positive extent cannot be smaller than a zero request.
        return [0, extent, 0];
    }

    // Scale both fixed sides into the available extent using wide arithmetic. Rounding the leading
    // side to nearest and assigning the remainder to the trailing side guarantees exact coverage
    // without overlap even when only one pixel survives.
    let scaled_leading = ((i64::from(extent) * i64::from(leading) + i64::from(requested) / 2) / i64::from(requested)) as i32;
    let scaled_leading = scaled_leading.clamp(0, extent);
    [scaled_leading, 0, extent - scaled_leading]
}

/// Derives the three axis origins from an initial coordinate and exact partition lengths.
fn partition_origins(origin: i32, lengths: [i32; 3]) -> [i32; 3] {
    // Saturating addition matches the rest of the retained geometry layer. Extreme application
    // coordinates remain deterministic and clipping later discards any unreachable pixels.
    let center = origin.saturating_add(lengths[0]);
    let trailing = center.saturating_add(lengths[1]);
    [origin, center, trailing]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;

    /// Verifies ordinary insets preserve fixed outer cells and assign the remainder to the center.
    #[test]
    fn patch_geometry_partitions_a_regular_destination() {
        let patch = NinePatch::framed(SliceInsets::new(2, 3, 4, 5), color(1, 2, 3, 255), None);
        let cells = patch.geometry(Recti::new(10, 20, 20, 18));

        assert_eq!(rect_tuple(cells[0][0]), (10, 20, 2, 3));
        assert_eq!(rect_tuple(cells[0][1]), (12, 20, 14, 3));
        assert_eq!(rect_tuple(cells[1][1]), (12, 23, 14, 10));
        assert_eq!(rect_tuple(cells[2][2]), (26, 33, 4, 5));
    }

    /// Verifies oversized opposing insets shrink without overlap and still cover the destination.
    #[test]
    fn patch_geometry_scales_insets_for_a_tiny_destination() {
        let patch = NinePatch::framed(SliceInsets::new(4, 3, 6, 7), color(1, 2, 3, 255), None);
        let cells = patch.geometry(Recti::new(5, 6, 3, 2));

        assert_eq!(row_width(cells[0]), 3);
        assert_eq!(row_width(cells[1]), 3);
        assert_eq!(row_width(cells[2]), 3);
        assert_eq!(column_height(&cells, 0), 2);
        assert_eq!(column_height(&cells, 1), 2);
        assert_eq!(column_height(&cells, 2), 2);
        assert_eq!(cells[1][1].width, 0);
        assert_eq!(cells[1][1].height, 0);
    }

    /// Verifies transparent color construction canonicalizes to a non-rendering cell.
    #[test]
    fn transparent_cells_are_not_visible() {
        let transparent = NinePatch::solid(color(1, 2, 3, 0));
        let opaque = NinePatch::solid(color(1, 2, 3, 1));

        assert!(!transparent.is_visible());
        assert!(opaque.is_visible());
    }

    /// Verifies border-only overlays omit a visible center without losing their edge cells.
    #[test]
    fn border_only_patch_visibility_ignores_the_center_cell() {
        let center_only = NinePatch::solid(color(1, 2, 3, 255)).without_center();
        let framed = NinePatch::framed(SliceInsets::uniform(1), color(4, 5, 6, 255), Some(color(7, 8, 9, 255))).without_center();

        assert!(!center_only.is_visible());
        assert!(framed.is_visible());
        assert!(!framed.center_visible);
    }

    /// Verifies replacing the flat center never destroys a complete image-backed patch.
    #[test]
    fn image_patch_ignores_flat_center_replacement() {
        let atlas = crate::test_support::test_atlas();
        let icon = atlas.icon_id("close").expect("test atlas must contain the close icon");
        let image = NinePatchImage::new(icon, SliceInsets::uniform(2), color(255, 255, 255, 255));
        let patch = NinePatch::image(SliceInsets::uniform(3), image).with_center(Some(color(1, 2, 3, 255)));

        let Some(resolved) = patch.image_content() else {
            panic!("image content was replaced by a flat center");
        };
        assert_eq!(resolved.icon, icon);
        assert_eq!(resolved.source_insets.left, 2);
    }

    /// Converts an external rectangle into an equality-friendly tuple for focused assertions.
    fn rect_tuple(rect: Recti) -> (i32, i32, i32, i32) {
        // rs-math3d deliberately does not require structural equality for every generic rectangle.
        (rect.x, rect.y, rect.width, rect.height)
    }

    /// Sums the widths of one resolved patch row.
    fn row_width(row: [Recti; 3]) -> i32 {
        // NinePatch geometry guarantees non-negative lengths, so saturation is only defensive here.
        row[0].width.saturating_add(row[1].width).saturating_add(row[2].width)
    }

    /// Sums the heights of one resolved patch column.
    fn column_height(cells: &[[Recti; 3]; 3], column: usize) -> i32 {
        // The test supplies a fixed valid column index corresponding to the three patch columns.
        cells[0][column]
            .height
            .saturating_add(cells[1][column].height)
            .saturating_add(cells[2][column].height)
    }
}
