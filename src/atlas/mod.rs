//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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
//

//! Texture atlas handles, baked icon/font metadata, and atlas construction helpers.

use std::collections::HashMap;
use std::cell::{Cell, Ref, RefCell};
use std::error::Error;
use std::fmt::{Debug, Formatter};

use super::*;

#[derive(Debug, Clone)]
/// Metrics and atlas coordinates for a glyph.
pub struct CharEntry {
    /// Pixel offset relative to the draw origin.
    pub offset: Vec2i,
    /// Horizontal advance after drawing this glyph.
    pub advance: Vec2i,
    /// Rectangle inside the atlas texture.
    pub rect: Recti, // coordinates in the atlas
}

#[derive(Clone)]
/// Internal font record stored in the atlas.
struct Font {
    /// Distance between text baselines in pixels.
    line_size: usize,
    /// Distance from the top of a line to its baseline.
    baseline: i32,
    /// Requested font size in pixels.
    font_size: usize,
    /// Glyph entries for the printable ASCII range baked into the atlas.
    entries: HashMap<char, CharEntry>,
}

impl Debug for Font {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        let mut entries = String::new();
        for e in &self.entries {
            entries.write_fmt(format_args!("{:?}, ", e))?;
        }
        f.write_fmt(format_args!(
            "Font {{ line_size: {}, baseline: {}, font_size: {}, entries: [{}] }}",
            self.line_size, self.baseline, self.font_size, entries
        ))
    }
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Handle referencing a font stored in the atlas.
pub struct FontId(usize);

#[derive(Default, Copy, Clone)]
/// Handle referencing a bitmap icon stored in the atlas.
pub struct IconId(usize);

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
/// Handle referencing an arbitrary image slot stored in the atlas.
pub struct SlotId(usize);

impl Into<u32> for IconId {
    fn into(self) -> u32 {
        self.0 as _
    }
}

impl Into<u32> for SlotId {
    fn into(self) -> u32 {
        self.0 as _
    }
}

#[derive(Debug, Clone)]
/// Internal bitmap icon record stored in the atlas.
struct Icon {
    /// Rectangle occupied by the icon in atlas pixel coordinates.
    rect: Recti,
}

/// Mutable atlas storage shared through [`AtlasHandle`].
struct Atlas {
    /// Width of the atlas texture in pixels.
    width: usize,
    /// Height of the atlas texture in pixels.
    height: usize,
    /// RGBA pixel data in row-major order.
    pixels: Vec<Color4b>,
    /// Named fonts available to text layout and rendering.
    fonts: Vec<(String, Font)>,
    /// Named icons available to widgets.
    icons: Vec<(String, Icon)>,
    /// User-reserved atlas rectangles for external drawing needs.
    slots: Vec<Recti>,
    /// Monotonic version reserved before mutable pixel/slot updates.
    last_update_id: u64,
}

/// Interior state shared by every clone of an [`AtlasHandle`].
struct AtlasShared {
    /// Number of overlapping logical/backend frames reading this atlas.
    active_frame_readers: Cell<usize>,
    /// Atlas metadata and pixels.
    data: RefCell<Atlas>,
}

impl AtlasShared {
    fn borrow(&self) -> Ref<'_, Atlas> {
        self.data.borrow()
    }
}

#[derive(Clone)]
/// Shared handle exposing read/write access to the atlas.
pub struct AtlasHandle(Rc<AtlasShared>);

/// Failure to freeze an atlas for another overlapping frame.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AtlasFrameError {
    /// The active-reader counter cannot represent another guard.
    TooManyReaders,
    /// Atlas data was already mutably borrowed by a reentrant caller.
    BorrowConflict,
}

impl std::fmt::Display for AtlasFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyReaders => f.write_str("atlas frame-reader counter exhausted"),
            Self::BorrowConflict => f.write_str("atlas is already mutably borrowed"),
        }
    }
}

impl Error for AtlasFrameError {}

/// Failure to mutate atlas pixels or slot state.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AtlasMutationError {
    /// Atlas mutation is forbidden while any frame guard is alive.
    FrameActive,
    /// The supplied slot identifier is not present in this atlas.
    UnknownSlot(SlotId),
    /// The atlas version counter cannot advance without wrapping.
    VersionExhausted,
    /// Atlas data is already borrowed by a reentrant operation.
    BorrowConflict,
}

impl std::fmt::Display for AtlasMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameActive => f.write_str("atlas mutation is forbidden while a frame is active"),
            Self::UnknownSlot(slot) => write!(f, "unknown atlas slot {}", slot.0),
            Self::VersionExhausted => f.write_str("atlas version counter exhausted"),
            Self::BorrowConflict => f.write_str("atlas is already borrowed"),
        }
    }
}

impl Error for AtlasMutationError {}

/// RAII guard that prevents atlas mutation throughout a logical/backend frame.
#[must_use = "dropping the guard unfreezes atlas mutation"]
pub(crate) struct AtlasFrameGuard {
    atlas: AtlasHandle,
    version_at_begin: u64,
}

impl AtlasHandle {
    /// Freezes mutation until the returned guard is dropped.
    pub(crate) fn freeze_for_frame(&self) -> Result<AtlasFrameGuard, AtlasFrameError> {
        let version_at_begin = self.0.data.try_borrow().map_err(|_| AtlasFrameError::BorrowConflict)?.last_update_id;
        let readers = self.0.active_frame_readers.get().checked_add(1).ok_or(AtlasFrameError::TooManyReaders)?;
        self.0.active_frame_readers.set(readers);
        Ok(AtlasFrameGuard { atlas: self.clone(), version_at_begin })
    }
}

impl Drop for AtlasFrameGuard {
    fn drop(&mut self) {
        let readers = self.atlas.0.active_frame_readers.get();
        if readers == 0 {
            eprintln!("[microui-redux][atlas] unbalanced atlas frame guard");
            return;
        }
        self.atlas.0.active_frame_readers.set(readers - 1);

        if self.atlas.0.data.try_borrow().is_ok_and(|atlas| atlas.last_update_id != self.version_at_begin) {
            eprintln!("[microui-redux][atlas] atlas changed while frozen");
        }
    }
}

/// Identifier of the solid white icon baked into the default atlas.
pub const WHITE_ICON: IconId = IconId(0);
/// Identifier of the close icon baked into the default atlas.
pub const CLOSE_ICON: IconId = IconId(1);
/// Identifier of the expand icon baked into the default atlas.
pub const EXPAND_ICON: IconId = IconId(2);
/// Identifier of the collapse icon baked into the default atlas.
pub const COLLAPSE_ICON: IconId = IconId(3);
/// Identifier of the checkbox icon baked into the default atlas.
pub const CHECK_ICON: IconId = IconId(4);
/// Identifier of the combo-box expand icon baked into the default atlas.
pub const EXPAND_DOWN_ICON: IconId = IconId(5);
/// Identifier of the open-folder icon baked into the default atlas.
pub const OPEN_FOLDER_16_ICON: IconId = IconId(6);
/// Identifier of the closed-folder icon baked into the default atlas.
pub const CLOSED_FOLDER_16_ICON: IconId = IconId(7);
/// Identifier of the file icon baked into the default atlas.
pub const FILE_16_ICON: IconId = IconId(8);

mod image;
pub use image::load_image_bytes;
#[cfg(any(feature = "builder", feature = "png_source"))]
pub(crate) use image::checked_rgba_byte_len;
pub(crate) use image::validate_rgba_buffer;

#[cfg(feature = "builder")]
/// Helpers for constructing atlas textures at build time.
pub mod builder;

mod source;
pub use source::{AtlasSource, FontEntry, SourceFormat};

#[cfg(feature = "save-to-rust")]
mod codegen;
mod runtime;

#[cfg(test)]
mod tests;
