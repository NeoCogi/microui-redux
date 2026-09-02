//
// Copyright 2026-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the conditions in LICENSE are met.
//

//! Complete patch and semantic-content colors selected from the typed appearance catalog.

use crate::{Color, NinePatch};

/// Complete paint description selected for one semantic role and meaningful state.
///
/// The patch and its adjacent text or glyph color intentionally travel together.
/// Keeping them in one concrete value prevents independently mutated catalogs from describing two
/// different states for the same control.
#[derive(Copy, Clone)]
pub struct Visual {
    /// Background, border, or image-backed nine-patch painted for the visual.
    pub patch: NinePatch,
    /// Color used for text and semantic glyphs drawn over the patch.
    ///
    /// A transparent value naturally suppresses separately drawn semantic content when the patch
    /// already contains the control's complete label or symbol.
    pub content_color: Color,
}

impl Visual {
    /// Creates one complete visual from its patch and semantic-content color.
    pub const fn new(patch: NinePatch, content_color: Color) -> Self {
        // Requiring both halves at construction keeps a visual complete at every API boundary.
        Self { patch, content_color }
    }
}
