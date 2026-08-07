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

//! Semantic icon bindings used by built-in UI components.

use crate::atlas::{
    AtlasHandle, CHECK_ICON, CLOSE_ICON, CLOSED_FOLDER_16_ICON, COLLAPSE_ICON, EXPAND_DOWN_ICON, EXPAND_ICON, FILE_16_ICON, IconId, OPEN_FOLDER_16_ICON,
};

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

impl Default for ThemeIcons {
    fn default() -> Self {
        Self {
            close: CLOSE_ICON,
            expand: EXPAND_ICON,
            collapse: COLLAPSE_ICON,
            check: CHECK_ICON,
            expand_down: EXPAND_DOWN_ICON,
            open_folder: OPEN_FOLDER_16_ICON,
            closed_folder: CLOSED_FOLDER_16_ICON,
            file: FILE_16_ICON,
        }
    }
}

impl ThemeIcons {
    /// Rebinds every semantic role whose conventional name exists in `atlas`.
    pub fn bind_named(&mut self, atlas: &AtlasHandle) {
        self.close = atlas.icon_id("close").or_else(|| atlas.icon_id("CLOSE")).unwrap_or(self.close);
        self.expand = atlas.icon_id("expand").or_else(|| atlas.icon_id("PLUS")).unwrap_or(self.expand);
        self.collapse = atlas.icon_id("collapse").or_else(|| atlas.icon_id("MINUS")).unwrap_or(self.collapse);
        self.check = atlas.icon_id("check").or_else(|| atlas.icon_id("CHECK")).unwrap_or(self.check);
        self.expand_down = atlas
            .icon_id("expand_down")
            .or_else(|| atlas.icon_id("EXPAND_DOWN"))
            .unwrap_or(self.expand_down);
        self.open_folder = atlas
            .icon_id("open_folder")
            .or_else(|| atlas.icon_id("OPEN_FOLDER_16"))
            .unwrap_or(self.open_folder);
        self.closed_folder = atlas
            .icon_id("closed_folder")
            .or_else(|| atlas.icon_id("CLOSED_FOLDER_16"))
            .unwrap_or(self.closed_folder);
        self.file = atlas.icon_id("file").or_else(|| atlas.icon_id("FILE_16")).unwrap_or(self.file);
    }

    pub(crate) fn bind_default_named(&mut self, atlas: &AtlasHandle) {
        let defaults = Self::default();
        if self.close == defaults.close {
            self.close = atlas.icon_id("close").or_else(|| atlas.icon_id("CLOSE")).unwrap_or(self.close);
        }
        if self.expand == defaults.expand {
            self.expand = atlas.icon_id("expand").or_else(|| atlas.icon_id("PLUS")).unwrap_or(self.expand);
        }
        if self.collapse == defaults.collapse {
            self.collapse = atlas.icon_id("collapse").or_else(|| atlas.icon_id("MINUS")).unwrap_or(self.collapse);
        }
        if self.check == defaults.check {
            self.check = atlas.icon_id("check").or_else(|| atlas.icon_id("CHECK")).unwrap_or(self.check);
        }
        if self.expand_down == defaults.expand_down {
            self.expand_down = atlas
                .icon_id("expand_down")
                .or_else(|| atlas.icon_id("EXPAND_DOWN"))
                .unwrap_or(self.expand_down);
        }
        if self.open_folder == defaults.open_folder {
            self.open_folder = atlas
                .icon_id("open_folder")
                .or_else(|| atlas.icon_id("OPEN_FOLDER_16"))
                .unwrap_or(self.open_folder);
        }
        if self.closed_folder == defaults.closed_folder {
            self.closed_folder = atlas
                .icon_id("closed_folder")
                .or_else(|| atlas.icon_id("CLOSED_FOLDER_16"))
                .unwrap_or(self.closed_folder);
        }
        if self.file == defaults.file {
            self.file = atlas.icon_id("file").or_else(|| atlas.icon_id("FILE_16")).unwrap_or(self.file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, Recti, SourceFormat};

    #[test]
    fn named_binding_does_not_depend_on_legacy_slot_order() {
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
        let atlas = AtlasHandle::from(&AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &[],
            format: SourceFormat::Raw,
        });

        let mut bindings = ThemeIcons::default();
        bindings.bind_named(&atlas);

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
