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

//! Atlas-bound semantic icon capabilities used by built-in UI components.

use crate::atlas::{AtlasHandle, IconId};

/// Atlas icon IDs selected for the semantic roles used by built-in components.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ThemeIcons {
    /// Window and dialog close affordance.
    pub close: IconId,
    /// Collapsed disclosure affordance.
    pub expand: IconId,
    /// Expanded disclosure affordance.
    pub collapse: IconId,
    /// Checked checkbox mark.
    pub check: IconId,
    /// Combo-box dropdown affordance.
    pub expand_down: IconId,
    /// Open-folder file-dialog item.
    pub open_folder: IconId,
    /// Closed-folder file-dialog item.
    pub closed_folder: IconId,
    /// Regular file-dialog item.
    pub file: IconId,
}

impl ThemeIcons {
    /// Resolves the built-in semantic roles into capabilities minted by `atlas`.
    ///
    /// Conventional lowercase names are used directly. Missing roles are configuration errors:
    /// built-in widgets retain these concrete capabilities and must never manufacture positional
    /// fallbacks or defer lookup until paint.
    ///
    /// # Panics
    ///
    /// Panics with the missing role name when any required semantic icon is absent.
    pub fn from_atlas(atlas: &AtlasHandle) -> Self {
        /// Resolves one required semantic role with a precise construction diagnostic.
        fn required(atlas: &AtlasHandle, name: &'static str) -> IconId {
            // Resolve exact lowercase names only; accepting historic uppercase aliases would keep
            // two naming conventions alive and hide stale generated metadata.
            atlas
                .icon_id(name)
                .unwrap_or_else(|| panic!("atlas does not contain required theme icon `{name}`"))
        }

        // Every field is minted by this exact atlas, making the resulting bundle safe to retain in
        // WindowManager and FileDialog without carrying the AtlasHandle beside it.
        Self {
            close: required(atlas, "close"),
            expand: required(atlas, "expand"),
            collapse: required(atlas, "collapse"),
            check: required(atlas, "check"),
            expand_down: required(atlas, "expand_down"),
            open_folder: required(atlas, "open_folder"),
            closed_folder: required(atlas, "closed_folder"),
            file: required(atlas, "file"),
        }
    }

    /// Reports whether every semantic icon capability belongs to `atlas`.
    pub(crate) fn belongs_to(&self, atlas: &AtlasHandle) -> bool {
        // Keep the ownership check explicit so adding a future semantic field requires updating the
        // validation list instead of being silently omitted by type erasure or iteration metadata.
        atlas.contains_icon(self.close)
            && atlas.contains_icon(self.expand)
            && atlas.contains_icon(self.collapse)
            && atlas.contains_icon(self.check)
            && atlas.contains_icon(self.expand_down)
            && atlas.contains_icon(self.open_folder)
            && atlas.contains_icon(self.closed_folder)
            && atlas.contains_icon(self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, CharEntry, FontEntry, Recti, SourceFormat, Vec2i};

    /// Verifies semantic construction is name-based and independent of table position.
    #[test]
    fn from_atlas_resolves_semantic_icons_independent_of_slot_order() {
        let pixels = [0xFF, 0xFF, 0xFF, 0xFF];
        let icons = [
            ("white", Recti::new(0, 0, 1, 1)),
            ("file", Recti::new(0, 0, 1, 1)),
            ("check", Recti::new(0, 0, 1, 1)),
            ("close", Recti::new(0, 0, 1, 1)),
            ("expand_down", Recti::new(0, 0, 1, 1)),
            ("collapse", Recti::new(0, 0, 1, 1)),
            ("expand", Recti::new(0, 0, 1, 1)),
            ("closed_folder", Recti::new(0, 0, 1, 1)),
            ("open_folder", Recti::new(0, 0, 1, 1)),
        ];
        let glyphs = [(
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(1, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "body",
            FontEntry {
                line_size: 1,
                baseline: 1,
                font_size: 1,
                entries: &glyphs,
            },
        )];
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
        });

        let bindings = ThemeIcons::from_atlas(&atlas);

        assert_eq!(bindings.close, atlas.icon_id("close").unwrap());
        assert_eq!(bindings.expand, atlas.icon_id("expand").unwrap());
        assert_eq!(bindings.collapse, atlas.icon_id("collapse").unwrap());
        assert_eq!(bindings.check, atlas.icon_id("check").unwrap());
        assert_eq!(bindings.expand_down, atlas.icon_id("expand_down").unwrap());
        assert_eq!(bindings.open_folder, atlas.icon_id("open_folder").unwrap());
        assert_eq!(bindings.closed_folder, atlas.icon_id("closed_folder").unwrap());
        assert_eq!(bindings.file, atlas.icon_id("file").unwrap());
    }
}
