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

//! Stable semantic and exact icon references for retained UI state.

use crate::atlas::{AtlasHandle, IconId};

use super::Skin;

/// Semantic icon roles required by built-in components.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconRole {
    /// Window and dialog close affordance.
    Close,
    /// Collapsed disclosure affordance.
    Expand,
    /// Expanded disclosure affordance.
    Collapse,
    /// Checked checkbox and menu-item mark.
    Check,
    /// Combo-box dropdown affordance.
    ExpandDown,
    /// Open-folder file-dialog item.
    OpenFolder,
    /// Closed-folder file-dialog item.
    ClosedFolder,
    /// Regular file-dialog item.
    File,
}

impl IconRole {
    /// Every semantic icon role in declaration order.
    pub const ALL: [Self; 8] = [
        Self::Close,
        Self::Expand,
        Self::Collapse,
        Self::Check,
        Self::ExpandDown,
        Self::OpenFolder,
        Self::ClosedFolder,
        Self::File,
    ];

    /// Number of semantic icon roles stored by a complete skin.
    pub const COUNT: usize = Self::ALL.len();

    /// Returns this role's position in the skin's resolved icon table.
    pub(crate) const fn index(self) -> usize {
        // Exhaustive positions prevent declaration-order casts from silently changing persisted
        // meaning if the enum is later reorganized for readability.
        match self {
            Self::Close => 0,
            Self::Expand => 1,
            Self::Collapse => 2,
            Self::Check => 3,
            Self::ExpandDown => 4,
            Self::OpenFolder => 5,
            Self::ClosedFolder => 6,
            Self::File => 7,
        }
    }

    /// Returns the conventional atlas name for this semantic icon.
    pub const fn atlas_name(self) -> &'static str {
        // Exhaustive matching makes a newly added role choose its exact resource spelling before
        // the crate compiles, avoiding parallel string lists in loaders and widgets.
        match self {
            Self::Close => "close",
            Self::Expand => "expand",
            Self::Collapse => "collapse",
            Self::Check => "check",
            Self::ExpandDown => "expand_down",
            Self::OpenFolder => "open_folder",
            Self::ClosedFolder => "closed_folder",
            Self::File => "file",
        }
    }
}

/// Stable reference to an icon used by retained UI state.
///
/// Semantic roles are resolved once into the active skin, while an exact reference carries the
/// atlas-baked [`IconId`] obtained from [`crate::ResourceCatalog`]. Both variants are small typed
/// values and require no retained strings or name lookup during measurement and paint.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconRef {
    /// Selects one required semantic icon from the active skin.
    Role(IconRole),
    /// Selects one exact application-owned resource by its stable baked identity.
    Named(IconId),
}

impl From<IconRole> for IconRef {
    /// Converts a semantic role without binding it to the current atlas.
    fn from(role: IconRole) -> Self {
        // Preserve semantic intent so a skin can select its already-resolved concrete icon.
        Self::Role(role)
    }
}

impl IconRef {
    /// Creates a semantic icon reference.
    pub fn role(role: IconRole) -> Self {
        // This constructor mirrors `named` and keeps public widget construction explicit.
        Self::Role(role)
    }

    /// Creates a stable reference to one exact named resource identity.
    pub const fn named(icon: IconId) -> Self {
        // ResourceCatalog performs the string lookup once and supplies the validated typed ID.
        Self::Named(icon)
    }

    /// Resolves this stable reference into an icon identity contained by `atlas`.
    ///
    /// # Panics
    ///
    /// Panics when the semantic or exact icon identity is absent from `atlas`.
    pub fn resolve(&self, skin: &Skin, atlas: &AtlasHandle) -> IconId {
        // Role resolution is a direct skin-array lookup. Exact resources keep their baked ID and
        // verify that the selected derived atlas copied that logical resource.
        match self {
            Self::Role(role) => skin.resolve_icon_role(atlas, *role),
            Self::Named(icon) => {
                assert!(atlas.contains_icon(*icon), "icon ID does not belong to the active skin atlas: {icon:?}");
                *icon
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, CharEntry, FontEntry, Recti, SourceFormat, Vec2i};

    /// Verifies semantic names are compiled once into IDs independent of table position.
    #[test]
    fn icon_roles_resolve_independent_of_slot_order() {
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
        // Even test metadata crosses the same strict construction boundary as application atlases;
        // this prevents a fixture from accidentally relying on malformed names, metrics, or
        // rectangles that production loading rejects.
        let atlas = AtlasHandle::try_from(&AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
        })
        .expect("semantic-icon fixture atlas must satisfy the complete atlas contract");

        let skin = Skin::from_atlas(&atlas);
        for role in IconRole::ALL {
            assert_eq!(skin.resolve_icon_role(&atlas, role), atlas.icon_id(role.atlas_name()).unwrap());
        }

        let close = atlas.icon_id("close").unwrap();
        let named = IconRef::named(close);
        assert_eq!(named.resolve(&skin, &atlas), close);
    }
}
