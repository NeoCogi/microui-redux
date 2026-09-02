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

//! Stable semantic and named icon references for retained UI state.

use std::sync::Arc;

use crate::atlas::{AtlasHandle, IconId};

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

    /// Resolves this role into a capability minted by `atlas`.
    ///
    /// # Panics
    ///
    /// Panics with the missing role name when the required semantic icon is absent.
    pub fn resolve(self, atlas: &AtlasHandle) -> IconId {
        let name = self.atlas_name();
        // Resolve exact lowercase names only; accepting historic aliases would preserve two public
        // naming conventions and conceal malformed generated metadata.
        atlas
            .icon_id(name)
            .unwrap_or_else(|| panic!("atlas does not contain required skin icon `{name}`"))
    }
}

/// Stable reference to an icon used by retained UI state.
///
/// The reference carries a semantic role or resource name rather than an allocation-bound
/// [`IconId`]. It can consequently survive a complete skin/atlas replacement and resolve against
/// the newly installed atlas during measurement or paint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconRef {
    /// Resolves through one required semantic icon role.
    Role(IconRole),
    /// Resolves one exact application-owned atlas icon name.
    Named(Arc<str>),
}

impl From<IconRole> for IconRef {
    /// Converts a semantic role without binding it to the current atlas.
    fn from(role: IconRole) -> Self {
        // Preserve the role so switching skins also switches the concrete icon capability.
        Self::Role(role)
    }
}

impl IconRef {
    /// Creates a semantic icon reference.
    pub fn role(role: IconRole) -> Self {
        // This constructor mirrors `named` and keeps public widget construction explicit.
        Self::Role(role)
    }

    /// Creates a stable reference to one exact atlas icon name.
    pub fn named(name: impl Into<Arc<str>>) -> Self {
        let name = name.into();
        // Empty names are never valid atlas keys and should fail where retained state is built.
        assert!(!name.is_empty(), "icon reference name must not be empty");
        Self::Named(name)
    }

    /// Resolves this stable reference into a capability owned by `atlas`.
    ///
    /// # Panics
    ///
    /// Panics when the semantic or named icon is absent from `atlas`.
    pub fn resolve(&self, atlas: &AtlasHandle) -> IconId {
        // Resolution is deliberately late but concrete: retained data stores this typed enum, and
        // renderer-facing code receives an ordinary IconId for only the active atlas.
        match self {
            Self::Role(role) => role.resolve(atlas),
            Self::Named(name) => atlas.icon_id(name).unwrap_or_else(|| panic!("atlas does not contain referenced icon `{name}`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, CharEntry, FontEntry, Recti, SourceFormat, Vec2i};

    /// Verifies semantic resolution is name-based and independent of table position.
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

        for role in IconRole::ALL {
            assert_eq!(role.resolve(&atlas), atlas.icon_id(role.atlas_name()).unwrap());
        }

        let named = IconRef::named("close");
        assert_eq!(named.resolve(&atlas), atlas.icon_id("close").unwrap());
    }
}
