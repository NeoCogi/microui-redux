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

//! Versioned JSON theme decoding and typed appearance installation.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::{
    ChromeRole, ChromeState, ControlRole, ControlState, FlatPalette, FontRole, IconRole, MenuRole, MenuState, PointerState, Skin, SkinBundle, SkinMetrics,
    SurfaceRole, SurfaceState, WindowChromeSkin,
};
use crate::{
    atlas::{
        AtlasHandle,
        builder::{Builder, BuilderError},
    },
    Color, IconId, NinePatch, NinePatchCell, NinePatchCells, NinePatchImage, SliceInsets,
};

/// Theme-file schema version understood by this crate release.
pub const THEME_SCHEMA_VERSION: u32 = 1;

/// A loaded theme with its display name, rebuilt resource atlas, and complete resolved skin.
///
/// Fonts, semantic icons, and image-backed state patches share one immutable atlas. Install this complete
/// bundle through [`crate::Context::set_theme`] so renderer pixels and every atlas-scoped
/// capability change together in one transaction.
#[derive(Clone)]
pub struct LoadedTheme {
    /// Human-readable name supplied by the JSON definition.
    name: String,
    /// Validated resolved skin and the exact immutable atlas containing all of its resources.
    bundle: SkinBundle,
}

impl LoadedTheme {
    /// Creates a named installable theme from one already validated skin bundle.
    ///
    /// # Panics
    ///
    /// Panics if `name` is empty or contains only whitespace.
    pub fn new(name: impl Into<String>, bundle: SkinBundle) -> Self {
        let name = name.into();
        assert!(!name.trim().is_empty(), "theme name must not be empty");
        // SkinBundle already established semantic-resource completeness and image ownership. Keep
        // LoadedTheme responsible only for the human-facing selection name.
        Self { name, bundle }
    }

    /// Returns the human-readable theme name from the JSON file.
    pub fn name(&self) -> &str {
        // Lend the owned selector label without exposing the bundle's construction internals.
        self.name.as_str()
    }

    /// Borrows the validated skin/atlas pair installed by this theme.
    pub fn bundle(&self) -> &SkinBundle {
        // Expose the atomic unit so callers cannot accidentally select a skin and atlas from
        // different loaded themes.
        &self.bundle
    }
}

/// Concrete failure produced while reading, decoding, validating, or installing a theme.
#[derive(Debug)]
pub enum ThemeLoadError {
    /// The JSON definition file could not be read.
    DefinitionRead {
        /// Exact path requested by the caller.
        path: PathBuf,
        /// Concrete filesystem failure.
        source: io::Error,
    },
    /// The definition is not valid JSON for the strict schema.
    DefinitionJson {
        /// Exact JSON path whose bytes were parsed.
        path: PathBuf,
        /// Structured serde JSON diagnostic.
        source: serde_json::Error,
    },
    /// The document targets a different schema version.
    UnsupportedSchema {
        /// Only schema version accepted by this crate release.
        expected: u32,
        /// Version declared by the document.
        actual: u32,
    },
    /// The document's display name is empty or whitespace-only.
    EmptyName,
    /// Destination or source slice insets are negative or do not fit their source image.
    InvalidInsets {
        /// Semantic appearance key containing the invalid value.
        appearance: String,
        /// Schema field that supplied the invalid value.
        field: &'static str,
        /// Complete rejected inset value.
        insets: SliceInsets,
        /// Optional image dimensions used for source-bound validation.
        image_size: Option<(i32, i32)>,
    },
    /// The theme's fonts or image artwork could not be read, decoded, packed, or finalized.
    AtlasBuild {
        /// Concrete atlas-builder failure retaining its asset or validation classification.
        source: BuilderError,
    },
}

impl fmt::Display for ThemeLoadError {
    /// Formats each failure with the relevant document, appearance, or nested asset diagnostic.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep classifications visible in text while retaining structured sources for callers.
        match self {
            Self::DefinitionRead { path, source } => write!(formatter, "failed to read theme definition {}: {source}", path.display()),
            Self::DefinitionJson { path, source } => write!(formatter, "invalid theme definition {}: {source}", path.display()),
            Self::UnsupportedSchema { expected, actual } => {
                write!(formatter, "unsupported theme schema {actual}; expected {expected}")
            }
            Self::EmptyName => formatter.write_str("theme name must not be empty"),
            Self::InvalidInsets { appearance, field, insets, image_size } => {
                write!(
                    formatter,
                    "invalid {field} for appearance `{appearance}`: left={}, top={}, right={}, bottom={}",
                    insets.left, insets.top, insets.right, insets.bottom
                )?;
                if let Some((width, height)) = image_size {
                    write!(formatter, " for image {width}x{height}")?;
                }
                Ok(())
            }
            Self::AtlasBuild { source } => write!(formatter, "failed to build theme resource atlas: {source}"),
        }
    }
}

impl Error for ThemeLoadError {
    /// Exposes concrete I/O, JSON, image, or texture causes without erasing theme classifications.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Schema and inset errors are fully represented by their own fields and have no lower cause.
        match self {
            Self::DefinitionRead { source, .. } => Some(source),
            Self::DefinitionJson { source, .. } => Some(source),
            Self::AtlasBuild { source } => Some(source),
            Self::UnsupportedSchema { .. } | Self::EmptyName | Self::InvalidInsets { .. } => None,
        }
    }
}

/// One fully constructed theme atlas and the capabilities assigned to unique image paths.
///
/// Keeping this intermediate concrete prevents installation from rediscovering numeric icon slots
/// or decoding files a second time. It is private because only a matching [`ThemeDefinition`] may
/// translate these construction capabilities into semantic appearances.
struct ThemeAtlas {
    /// Immutable pixels and metadata ready for renderer installation.
    atlas: AtlasHandle,
    /// Fully resolved image paths mapped to their atlas-owned icon capabilities.
    images: BTreeMap<PathBuf, IconId>,
}

/// Strict syntax tree and compiler input for exactly one self-contained JSON theme file.
///
/// The document deliberately has no parent link or overlay semantics. Optional fields mean
/// "retain the built-in fallback" rather than "search another document," so every authored value
/// has one source directory and one unambiguous effect.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeDefinition {
    /// Version selecting the exact schema contract.
    schema_version: u32,
    /// Human-readable selector label for the loaded theme.
    name: String,
    /// Optional complete semantic font recipe replacing the base atlas's semantic fonts.
    fonts: Option<FontCatalogDocument>,
    /// Optional complete semantic icon recipe replacing the base atlas's semantic icons.
    icons: Option<IconCatalogDocument>,
    /// Sparse scalar and flat-color overrides applied to the built-in skin.
    #[serde(default)]
    skin: ThemeSkinDocument,
    /// Sparse appearance overrides applied after flat fallback construction.
    #[serde(default)]
    appearances: AppearanceCatalogDocument,
}

/// Strict categorized appearance syntax for one source document.
///
/// Each JSON object key is deserialized directly into its family's concrete role enum. The loader
/// therefore never receives an unvalidated role string and cannot acquire knowledge of the widget
/// or manager implementation that eventually consumes the visual.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AppearanceCatalogDocument {
    /// Structural surface appearances keyed by their category-local names.
    surface: BTreeMap<SurfaceRole, SurfaceAppearanceDocument>,
    /// Interactive control appearances keyed by their category-local names.
    control: BTreeMap<ControlRole, ControlAppearanceDocument>,
    /// Compact menu appearances keyed by their category-local names.
    menu: BTreeMap<MenuRole, MenuAppearanceDocument>,
    /// Manager-owned window chrome appearances keyed by their category-local names.
    chrome: BTreeMap<ChromeRole, ChromeAppearanceDocument>,
}

impl AppearanceCatalogDocument {
    /// Resolves every authored image path relative to its declaring source document.
    fn resolve_paths(&mut self, directory: &Path) {
        // Each family keeps its concrete document shape; explicit loops avoid a typeless common
        // appearance object whose state fields would admit invalid combinations.
        for appearance in self.surface.values_mut() {
            appearance.resolve_paths(directory);
        }
        for appearance in self.control.values_mut() {
            appearance.resolve_paths(directory);
        }
        for appearance in self.menu.values_mut() {
            appearance.resolve_paths(directory);
        }
        for appearance in self.chrome.values_mut() {
            appearance.resolve_paths(directory);
        }
    }
}

impl ThemeDefinition {
    /// Reads, validates, and anchors every asset in one JSON theme definition.
    fn read(path: &Path) -> Result<Self, ThemeLoadError> {
        // The file path is retained verbatim in diagnostics. Its lexical parent is sufficient to
        // anchor assets because one document is now their only possible owner.
        let bytes = fs::read(path).map_err(|source| ThemeLoadError::DefinitionRead { path: path.to_path_buf(), source })?;
        let mut definition: Self =
            serde_json::from_slice(bytes.as_slice()).map_err(|source| ThemeLoadError::DefinitionJson { path: path.to_path_buf(), source })?;
        definition.validate_header()?;

        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        definition.resolve_paths(directory);
        Ok(definition)
    }

    /// Validates the root fields that require semantic checks beyond JSON deserialization.
    fn validate_header(&self) -> Result<(), ThemeLoadError> {
        // Keep schema and name validation next to decoding while giving filesystem and unit-test
        // entry points one identical contract.
        if self.schema_version != THEME_SCHEMA_VERSION {
            return Err(ThemeLoadError::UnsupportedSchema {
                expected: THEME_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        if self.name.trim().is_empty() {
            return Err(ThemeLoadError::EmptyName);
        }
        Ok(())
    }

    /// Resolves every relative asset path against this document's directory exactly once.
    fn resolve_paths(&mut self, directory: &Path) {
        // Fonts, semantic icons, and appearance images share one source owner. No later compilation
        // stage needs to retain the definition filename or reinterpret a path.
        if let Some(fonts) = &mut self.fonts {
            fonts.resolve_paths(directory);
        }
        if let Some(icons) = &mut self.icons {
            icons.resolve_paths(directory);
        }
        self.appearances.resolve_paths(directory);
    }

    /// Strictly parses one standalone source string for schema-focused unit tests.
    #[cfg(test)]
    fn parse_for_test(json: &str) -> Result<Self, ThemeLoadError> {
        // A synthetic relative path keeps schema diagnostics and asset anchoring deterministic.
        let path = Path::new("theme.json");
        let mut definition: Self = serde_json::from_str(json).map_err(|source| ThemeLoadError::DefinitionJson { path: path.to_path_buf(), source })?;
        definition.validate_header()?;
        definition.resolve_paths(Path::new("."));
        Ok(definition)
    }

    /// Builds the atlas selected by this definition while preserving required base resources.
    ///
    /// Every unique image-patch source is packed once beside fonts and icons. A completely flat document
    /// without a font recipe can reuse the base allocation exactly; adding artwork without new
    /// fonts repacks existing glyph bitmaps so the original font files are not required.
    fn build_atlas(&self, base: &AtlasHandle) -> Result<ThemeAtlas, ThemeLoadError> {
        let image_paths = self.image_paths();
        if self.fonts.is_none() && self.icons.is_none() && image_paths.is_empty() {
            // No atlas-visible resource changes are requested, so preserving pointer identity also
            // lets selecting a palette-only theme avoid an unnecessary backend atlas upload.
            return Ok(ThemeAtlas {
                atlas: base.clone(),
                images: BTreeMap::new(),
            });
        }

        let (width, height) = self
            .fonts
            .as_ref()
            .map(|fonts| (fonts.texture_width, fonts.texture_height))
            .unwrap_or_else(|| (base.width(), base.height()));
        // Replacement is expressed entirely in the atlas's two resource namespaces. The builder
        // preserves every excluded recipe's stable capability while it copies all application-owned
        // resources that the theme does not replace.
        let semantic_font_names = FontRole::ALL.map(FontRole::atlas_name);
        let semantic_icon_names = IconRole::ALL.map(IconRole::atlas_name);
        let excluded_fonts = self.fonts.as_ref().map(|_| semantic_font_names.as_slice()).unwrap_or(&[]);
        let excluded_icons = self.icons.as_ref().map(|_| semantic_icon_names.as_slice()).unwrap_or(&[]);
        let mut builder = Builder::from_atlas_with_size_excluding(base, width, height, excluded_fonts, excluded_icons)
            .map_err(|source| ThemeLoadError::AtlasBuild { source })?;

        if let Some(fonts) = &self.fonts {
            for (role, font) in fonts.entries() {
                builder
                    .add_font_named(role.atlas_name(), font.path.as_path(), font.size)
                    .map_err(|source| ThemeLoadError::AtlasBuild { source })?;
            }
        }

        if let Some(icons) = &self.icons {
            for (role, icon) in icons.entries() {
                builder
                    .add_icon_named(role.atlas_name(), icon.path.as_path())
                    .map_err(|source| ThemeLoadError::AtlasBuild { source })?;
            }
        }

        let mut images = BTreeMap::new();
        for (index, path) in image_paths.into_iter().enumerate() {
            // Builder owns bounded file I/O, PNG decoding, transactional packing, and typed image
            // diagnostics. The loader supplies only a deterministic private resource name.
            let icon = builder
                .add_icon_named(format!("@microui-theme-image/{index}").as_str(), path.as_path())
                .map_err(|source| ThemeLoadError::AtlasBuild { source })?;
            images.insert(path, icon);
        }

        let atlas = builder.build().map_err(|source| ThemeLoadError::AtlasBuild { source })?;
        Ok(ThemeAtlas { atlas, images })
    }

    /// Resolves flat fallbacks and binds explicitly supplied image patches to baked atlas regions.
    fn install(self, theme_atlas: ThemeAtlas) -> Result<LoadedTheme, ThemeLoadError> {
        let ThemeAtlas { atlas, images } = theme_atlas;
        // Constructing the fallback here makes a mismatched atlas/skin pair unrepresentable inside
        // the loader rather than asserting that a multi-stage caller supplied the matching value.
        let mut skin = Skin::from_atlas(&atlas);
        // Apply palette and metric overrides before constructing fallbacks, so every omitted patch
        // state reflects the JSON theme's own flat colors rather than the built-in default palette.
        skin.metrics = self.skin.metrics;
        skin.window_chrome = self.skin.window_chrome;
        let frame_insets = self.skin.fallback_frame_insets;
        validate_non_negative("generic_frame", "skin.fallback_frame_insets", frame_insets)?;
        validate_non_negative("window_content", "skin.metrics.window_content_insets", skin.metrics.window_content_insets)?;
        validate_non_negative("window_frame", "skin.metrics.window_border", skin.metrics.window_border)?;
        skin.replace_flat_visuals(frame_insets, &self.skin.palette);
        for (role, document) in &self.appearances.surface {
            // Each family resolves through its own typed Skin accessors. The shared compiler below
            // transforms only concrete visual data and never receives behavioral callbacks.
            let name = format!("surface.{role:?}");
            let destination_insets = destination_insets(&name, document.insets, skin.surface(*role, SurfaceState::Normal).patch.insets)?;
            for (state, authored) in document.states() {
                let visual = compile_visual(&atlas, &images, &name, destination_insets, authored, skin.surface(*role, state))?;
                skin.set_surface(*role, state, visual);
            }
        }
        for (role, document) in &self.appearances.control {
            let name = format!("control.{role:?}");
            let base_state = ControlState::Enabled(PointerState::Normal);
            let destination_insets = destination_insets(&name, document.insets, skin.control(*role, base_state).patch.insets)?;
            for (state, authored) in document.states() {
                let visual = compile_visual(&atlas, &images, &name, destination_insets, authored, skin.control(*role, state))?;
                skin.set_control(*role, state, visual);
            }
        }
        for (role, document) in &self.appearances.menu {
            let name = format!("menu.{role:?}");
            let destination_insets = destination_insets(&name, document.insets, skin.menu(*role, MenuState::Normal).patch.insets)?;
            for (state, authored) in document.states() {
                let visual = compile_visual(&atlas, &images, &name, destination_insets, authored, skin.menu(*role, state))?;
                skin.set_menu(*role, state, visual);
            }
        }
        for (role, document) in &self.appearances.chrome {
            let name = format!("chrome.{role:?}");
            let destination_insets = destination_insets(&name, document.insets, skin.chrome(*role, ChromeState::Base).patch.insets)?;
            for (state, authored) in document.states() {
                let visual = compile_visual(&atlas, &images, &name, destination_insets, authored, skin.chrome(*role, state))?;
                skin.set_chrome(*role, state, visual);
            }
        }

        // Pair atlas and resolved skin at the only public construction boundary before naming it.
        Ok(LoadedTheme::new(self.name, SkinBundle::new(atlas, skin)))
    }

    /// Returns every fully resolved image path in deterministic deduplicated order.
    fn image_paths(&self) -> BTreeSet<PathBuf> {
        // Image patch variants retain independent slicing/tint metadata, while one path set is
        // sufficient for immutable pixel packing. BTreeSet also stabilizes generated atlas names.
        let mut paths = BTreeSet::new();
        for appearance in self.appearances.surface.values() {
            for (_, visual) in appearance.states() {
                collect_image_path(visual, &mut paths);
            }
        }
        for appearance in self.appearances.control.values() {
            for (_, visual) in appearance.states() {
                collect_image_path(visual, &mut paths);
            }
        }
        for appearance in self.appearances.menu.values() {
            for (_, visual) in appearance.states() {
                collect_image_path(visual, &mut paths);
            }
        }
        for appearance in self.appearances.chrome.values() {
            for (_, visual) in appearance.states() {
                collect_image_path(visual, &mut paths);
            }
        }
        paths
    }
}

/// Reads and compiles one self-contained JSON theme against the stable application resource atlas.
///
/// This is the loader module's only entry point. Definition parsing, path anchoring, atlas
/// construction, and visual installation remain private stages that cannot be called out of order
/// or supplied with mismatched intermediate values.
pub(crate) fn load(path: &Path, base: &AtlasHandle) -> Result<LoadedTheme, ThemeLoadError> {
    // The complete operation is CPU-local and immutable. Context uploads the returned bundle only
    // when the application explicitly selects it through `set_theme`.
    let definition = ThemeDefinition::read(path)?;
    let atlas = definition.build_atlas(base)?;
    definition.install(atlas)
}

/// Selects and validates the destination geometry shared by every state of one role.
fn destination_insets(name: &str, authored: Option<SliceInsets>, fallback: SliceInsets) -> Result<SliceInsets, ThemeLoadError> {
    // A role-wide value prevents interaction transitions from changing layout. Omitting it retains
    // the base state's concrete fallback geometry rather than copying another authored state.
    let insets = authored.unwrap_or(fallback);
    validate_non_negative(name, "insets", insets)?;
    Ok(insets)
}

/// Compiles one optional authored override over its exact family/state fallback visual.
fn compile_visual(
    atlas: &AtlasHandle,
    images: &BTreeMap<PathBuf, IconId>,
    name: &str,
    destination_insets: SliceInsets,
    authored: Option<&VisualOverrideDocument>,
    mut visual: super::Visual,
) -> Result<super::Visual, ThemeLoadError> {
    // Even an omitted state receives the role-wide destination geometry while retaining its own
    // fallback cells. Authored fields then replace only the explicitly selected visual channels.
    visual.patch = visual.patch.with_insets(destination_insets);
    if let Some(authored) = authored {
        if let Some(content_color) = authored.content_color {
            // The patch and semantic content color remain one complete runtime value at the mutation boundary.
            visual.content_color = content_color;
        }
        if let Some(patch) = &authored.patch {
            visual.patch = patch.compile(atlas, images, name, destination_insets)?;
        }
    }
    Ok(visual)
}

/// Adds the image path from one present image-backed visual to a deduplicated atlas work set.
fn collect_image_path(visual: Option<&VisualOverrideDocument>, paths: &mut BTreeSet<PathBuf>) {
    // Solid patches and content-color-only states have no resource dependency. Matching the closed
    // patch enum makes that distinction explicit without probing a group of loosely related fields.
    if let Some(VisualOverrideDocument {
        patch: Some(PatchDocument::Image { path, .. }),
        ..
    }) = visual
    {
        paths.insert(path.clone());
    }
}

/// Complete semantic font recipe for one rebuilt theme atlas.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FontCatalogDocument {
    /// Width in pixels of the rebuilt atlas texture.
    texture_width: usize,
    /// Height in pixels of the rebuilt atlas texture.
    texture_height: usize,
    /// Default font used by controls and ordinary text.
    body: FontDocument,
    /// Compact font used by supporting text.
    small: FontDocument,
    /// Font used by window titles and chrome.
    title: FontDocument,
    /// Larger font used by headings and typography demonstrations.
    heading: FontDocument,
    /// Fixed-width font used by console-oriented text.
    mono: FontDocument,
}

impl FontCatalogDocument {
    /// Returns all semantic recipes in stable atlas insertion order.
    fn entries(&self) -> [(FontRole, &FontDocument); 5] {
        // The explicit table keeps JSON keys, public FontRole values, and atlas names aligned
        // without reflection or string-driven role dispatch.
        [
            (FontRole::Body, &self.body),
            (FontRole::Small, &self.small),
            (FontRole::Title, &self.title),
            (FontRole::Heading, &self.heading),
            (FontRole::Mono, &self.mono),
        ]
    }

    /// Resolves every relative font filename against its declaring document directory.
    fn resolve_paths(&mut self, directory: &Path) {
        // Traverse through the same typed role list used by atlas insertion so no semantic font
        // retains a path whose base directory depends on a later compilation stage.
        self.body.resolve_path(directory);
        self.small.resolve_path(directory);
        self.title.resolve_path(directory);
        self.heading.resolve_path(directory);
        self.mono.resolve_path(directory);
    }
}

/// One font file and raster size declared by a theme.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FontDocument {
    /// Font file path resolved relative to the theme JSON directory.
    path: PathBuf,
    /// Pixel size rasterized into printable-ASCII atlas glyphs.
    size: usize,
}

/// Complete semantic icon recipe for one rebuilt theme atlas.
///
/// The object is deliberately all-or-nothing, just like [`FontCatalogDocument`]. A theme that
/// takes ownership of semantic iconography cannot accidentally leave one widget using pixels from
/// an unrelated base atlas, and every accepted key maps directly to one closed [`IconRole`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IconCatalogDocument {
    /// Window and dialog close glyph.
    close: IconDocument,
    /// Collapsed disclosure glyph.
    expand: IconDocument,
    /// Expanded disclosure glyph.
    collapse: IconDocument,
    /// Checked checkbox and checked menu-item glyph.
    check: IconDocument,
    /// Combo-box dropdown glyph.
    expand_down: IconDocument,
    /// Open folder glyph used by file dialogs.
    open_folder: IconDocument,
    /// Closed folder glyph used by file dialogs.
    closed_folder: IconDocument,
    /// Regular file glyph used by file dialogs.
    file: IconDocument,
}

impl IconCatalogDocument {
    /// Returns every semantic recipe in stable atlas insertion order.
    fn entries(&self) -> [(IconRole, &IconDocument); IconRole::COUNT] {
        // This explicit table is the single bridge from schema fields to runtime roles. Widgets
        // remain unaware of filenames, while the loader remains unaware of widget implementations.
        [
            (IconRole::Close, &self.close),
            (IconRole::Expand, &self.expand),
            (IconRole::Collapse, &self.collapse),
            (IconRole::Check, &self.check),
            (IconRole::ExpandDown, &self.expand_down),
            (IconRole::OpenFolder, &self.open_folder),
            (IconRole::ClosedFolder, &self.closed_folder),
            (IconRole::File, &self.file),
        ]
    }

    /// Resolves every relative icon filename against its declaring document directory.
    fn resolve_paths(&mut self, directory: &Path) {
        // Traverse through the same typed table used for insertion so path ownership and resource
        // identity cannot diverge between parsing and atlas construction.
        self.close.resolve_path(directory);
        self.expand.resolve_path(directory);
        self.collapse.resolve_path(directory);
        self.check.resolve_path(directory);
        self.expand_down.resolve_path(directory);
        self.open_folder.resolve_path(directory);
        self.closed_folder.resolve_path(directory);
        self.file.resolve_path(directory);
    }
}

/// One PNG file declared for a semantic icon role.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IconDocument {
    /// Icon path resolved relative to the theme JSON directory.
    path: PathBuf,
}

impl IconDocument {
    /// Makes this icon path independent of later compilation stages.
    fn resolve_path(&mut self, directory: &Path) {
        // Path::join also preserves an absolute source supplied by an embedding application.
        self.path = directory.join(self.path.as_path());
    }
}

impl FontDocument {
    /// Makes this font path independent of any later child document directory.
    fn resolve_path(&mut self, directory: &Path) {
        // Path::join preserves an already absolute path and anchors an ordinary relative path to
        // the source layer that owns this concrete font recipe.
        self.path = directory.join(self.path.as_path());
    }
}

/// Concrete runtime skin inputs applied before sparse appearance overrides.
///
/// The wrapper exists only to group independent runtime values in JSON. It does not mirror their
/// fields, translate names, or provide another customization API.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ThemeSkinDocument {
    /// Layout values consumed directly by the runtime skin.
    metrics: SkinMetrics,
    /// Shared destination geometry used while compiling flat fallback patches.
    fallback_frame_insets: SliceInsets,
    /// Concrete semantic colors compiled into the complete fallback appearance catalog.
    palette: FlatPalette,
    /// Exact manager-owned title and caption-button policy.
    window_chrome: WindowChromeSkin,
}

impl Default for ThemeSkinDocument {
    /// Returns the same concrete skin inputs used by `Skin::from_atlas`.
    fn default() -> Self {
        // Container-level Serde defaults copy from this complete value. Missing JSON fields cannot
        // therefore drift from programmatic defaults or require optional mirror fields.
        Self {
            metrics: SkinMetrics::default(),
            fallback_frame_insets: SliceInsets::uniform(1),
            palette: FlatPalette::default(),
            window_chrome: WindowChromeSkin::default(),
        }
    }
}

/// One structural surface role's destination insets and availability states.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SurfaceAppearanceDocument {
    /// Destination-space outer row and column sizes shared by both states.
    insets: Option<SliceInsets>,
    /// Enabled structural appearance.
    normal: Option<VisualOverrideDocument>,
    /// Disabled structural appearance.
    disabled: Option<VisualOverrideDocument>,
}

impl SurfaceAppearanceDocument {
    /// Resolves every surface PNG path against the declaring document directory.
    fn resolve_paths(&mut self, directory: &Path) {
        // Only the two valid structural states are traversed.
        resolve_visual_paths([self.normal.as_mut(), self.disabled.as_mut()], directory);
    }

    /// Returns every surface state paired with its optional authored value.
    fn states(&self) -> [(SurfaceState, Option<&VisualOverrideDocument>); SurfaceState::COUNT] {
        // Declaration order matches the runtime surface catalog.
        [(SurfaceState::Normal, self.normal.as_ref()), (SurfaceState::Disabled, self.disabled.as_ref())]
    }
}

/// Pointer-state fields shared by the enabled and focused branches of a control.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PointerAppearanceDocument {
    /// Pointer-neutral branch appearance.
    normal: Option<VisualOverrideDocument>,
    /// Pointer-hover branch appearance.
    hovered: Option<VisualOverrideDocument>,
    /// Visible pointer-press branch appearance.
    pressed: Option<VisualOverrideDocument>,
}

impl PointerAppearanceDocument {
    /// Resolves every pointer-state image path against the declaring document directory.
    fn resolve_paths(&mut self, directory: &Path) {
        // The branch contains exactly the pointer states representable inside ControlState.
        resolve_visual_paths([self.normal.as_mut(), self.hovered.as_mut(), self.pressed.as_mut()], directory);
    }
}

/// One interactive control role's destination insets and nested control states.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ControlAppearanceDocument {
    /// Destination-space outer row and column sizes shared by every control state.
    insets: Option<SliceInsets>,
    /// Disabled appearance, with no contradictory pointer or focus substate.
    disabled: Option<VisualOverrideDocument>,
    /// Pointer appearances while the control is enabled without keyboard focus.
    enabled: PointerAppearanceDocument,
    /// Pointer appearances while the control owns visible keyboard focus.
    focused: PointerAppearanceDocument,
}

impl ControlAppearanceDocument {
    /// Resolves every control PNG path against the declaring document directory.
    fn resolve_paths(&mut self, directory: &Path) {
        // Disabled is terminal; enabled and focused branches resolve their concrete pointer states.
        resolve_visual_paths([self.disabled.as_mut()], directory);
        self.enabled.resolve_paths(directory);
        self.focused.resolve_paths(directory);
    }

    /// Returns every concrete nested control state and its optional authored value.
    fn states(&self) -> [(ControlState, Option<&VisualOverrideDocument>); ControlState::COUNT] {
        // This order is the exact flattened storage order internal to the control family only.
        [
            (ControlState::Disabled, self.disabled.as_ref()),
            (ControlState::Enabled(PointerState::Normal), self.enabled.normal.as_ref()),
            (ControlState::Enabled(PointerState::Hovered), self.enabled.hovered.as_ref()),
            (ControlState::Enabled(PointerState::Pressed), self.enabled.pressed.as_ref()),
            (ControlState::Focused(PointerState::Normal), self.focused.normal.as_ref()),
            (ControlState::Focused(PointerState::Hovered), self.focused.hovered.as_ref()),
            (ControlState::Focused(PointerState::Pressed), self.focused.pressed.as_ref()),
        ]
    }
}

/// One menu role's destination insets and menu-specific selection states.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MenuAppearanceDocument {
    /// Destination-space outer row and column sizes shared by every menu state.
    insets: Option<SliceInsets>,
    /// Enabled menu appearance without transient selection.
    normal: Option<VisualOverrideDocument>,
    /// Pointer-hover selection appearance.
    hovered: Option<VisualOverrideDocument>,
    /// Visible pointer-press selection appearance.
    pressed: Option<VisualOverrideDocument>,
    /// Keyboard-navigation selection appearance.
    focused: Option<VisualOverrideDocument>,
    /// Appearance of an entry that owns an open popup.
    open: Option<VisualOverrideDocument>,
    /// Disabled menu appearance.
    disabled: Option<VisualOverrideDocument>,
}

impl MenuAppearanceDocument {
    /// Resolves every menu PNG path against the declaring document directory.
    fn resolve_paths(&mut self, directory: &Path) {
        // The fixed field list is the complete menu state vocabulary.
        resolve_visual_paths(
            [
                self.normal.as_mut(),
                self.hovered.as_mut(),
                self.pressed.as_mut(),
                self.focused.as_mut(),
                self.open.as_mut(),
                self.disabled.as_mut(),
            ],
            directory,
        );
    }

    /// Returns every menu state paired with its optional authored value.
    fn states(&self) -> [(MenuState, Option<&VisualOverrideDocument>); MenuState::COUNT] {
        // Declaration order matches the runtime menu catalog.
        [
            (MenuState::Normal, self.normal.as_ref()),
            (MenuState::Hovered, self.hovered.as_ref()),
            (MenuState::Pressed, self.pressed.as_ref()),
            (MenuState::Focused, self.focused.as_ref()),
            (MenuState::Open, self.open.as_ref()),
            (MenuState::Disabled, self.disabled.as_ref()),
        ]
    }
}

/// One window chrome role's destination insets and activation states.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ChromeAppearanceDocument {
    /// Destination-space outer row and column sizes shared by every chrome state.
    insets: Option<SliceInsets>,
    /// Enabled chrome appearance without window activation.
    base: Option<VisualOverrideDocument>,
    /// Enabled chrome appearance with window activation.
    active: Option<VisualOverrideDocument>,
    /// Disabled chrome appearance.
    disabled: Option<VisualOverrideDocument>,
}

impl ChromeAppearanceDocument {
    /// Resolves every chrome PNG path against the declaring document directory.
    fn resolve_paths(&mut self, directory: &Path) {
        // Chrome has no pointer or keyboard-focus states to normalize.
        resolve_visual_paths([self.base.as_mut(), self.active.as_mut(), self.disabled.as_mut()], directory);
    }

    /// Returns every chrome state paired with its optional authored value.
    fn states(&self) -> [(ChromeState, Option<&VisualOverrideDocument>); ChromeState::COUNT] {
        // Declaration order matches the runtime chrome catalog.
        [
            (ChromeState::Base, self.base.as_ref()),
            (ChromeState::Active, self.active.as_ref()),
            (ChromeState::Disabled, self.disabled.as_ref()),
        ]
    }
}

/// Sparse overrides for one exact family role and state.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct VisualOverrideDocument {
    /// Optional text and semantic-glyph color for this exact role and state.
    content_color: Option<Color>,
    /// Optional concrete patch source replacing the exact state's flat fallback.
    patch: Option<PatchDocument>,
}

impl VisualOverrideDocument {
    /// Anchors this state's optional image path to the one containing theme document.
    fn resolve_path(&mut self, directory: &Path) {
        // Content colors and resource-free flat patches own no filesystem state. The closed patch
        // enum routes path handling only to the image variant that can legally contain a path.
        if let Some(patch) = &mut self.patch {
            patch.resolve_path(directory);
        }
    }
}

/// Concrete source for an authored visual patch.
///
/// Tagging the alternatives makes image-only fields structurally unavailable to flat patches.
/// A state that omits `patch` retains its exact family/state fallback instead of manufacturing an
/// empty image recipe.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum PatchDocument {
    /// A flat color painted across all nine destination cells.
    Solid {
        /// RGBA color covering the complete patch while role insets retain layout geometry.
        color: Color,
    },
    /// A resource-free three-by-three patch with one border and one center color.
    Framed {
        /// RGBA color painted into every fixed edge and corner cell.
        border: Color,
        /// RGBA color painted into the stretchable center cell.
        center: Color,
    },
    /// One PNG divided into a typed three-by-three image patch.
    Image {
        /// PNG path resolved relative to the containing JSON document.
        path: PathBuf,
        /// Optional source-space slices; omission reuses the role's destination insets.
        source_insets: Option<SliceInsets>,
        /// Optional RGBA image modulation; omission preserves the source pixels.
        tint: Option<Color>,
    },
}

impl PatchDocument {
    /// Anchors an image patch path while leaving resource-free flat patches unchanged.
    fn resolve_path(&mut self, directory: &Path) {
        // Path ownership belongs to the concrete image variant, so no independent fields can fall
        // out of sync while the document advances from decoding to atlas construction.
        if let Self::Image { path, .. } = self {
            *path = directory.join(path.as_path());
        }
    }

    /// Compiles this closed source choice into one runtime patch owned by `atlas`.
    fn compile(
        &self,
        atlas: &AtlasHandle,
        images: &BTreeMap<PathBuf, IconId>,
        appearance: &str,
        destination_insets: SliceInsets,
    ) -> Result<NinePatch, ThemeLoadError> {
        // Exhaustive matching keeps solid construction independent of atlas resources and confines
        // source slicing, tinting, and capability lookup to the image alternative.
        match self {
            Self::Solid { color } => Ok(NinePatch::new(destination_insets, NinePatchCells::all(NinePatchCell::color(*color)))),
            Self::Framed { border, center } => Ok(NinePatch::framed(destination_insets, *border, Some(*center))),
            Self::Image { path, source_insets, tint } => {
                let source_insets = source_insets.unwrap_or(destination_insets);
                let icon = *images.get(path).expect("every declared theme image must have one baked atlas capability");
                let image_size = atlas.get_icon_size(icon);
                validate_source_insets(appearance, source_insets, image_size.width, image_size.height)?;
                let tint = tint.unwrap_or(Color { r: 255, g: 255, b: 255, a: 255 });
                Ok(NinePatch::image(destination_insets, NinePatchImage::new(icon, source_insets, tint)))
            }
        }
    }
}

/// Resolves a fixed family-specific set of optional state paths.
fn resolve_visual_paths<const N: usize>(states: [Option<&mut VisualOverrideDocument>; N], directory: &Path) {
    // Missing states remain absent; every present recipe is anchored to the containing document.
    for state in states.into_iter().flatten() {
        state.resolve_path(directory);
    }
}

/// Rejects negative destination or source inset components.
fn validate_non_negative(appearance: &str, field: &'static str, insets: SliceInsets) -> Result<(), ThemeLoadError> {
    // Destination insets may exceed a tiny runtime allocation because NinePatch collapses them, but
    // negative authored values are always a schema error instead of being silently normalized.
    if insets.left < 0 || insets.top < 0 || insets.right < 0 || insets.bottom < 0 {
        return Err(ThemeLoadError::InvalidInsets {
            appearance: appearance.to_owned(),
            field,
            insets,
            image_size: None,
        });
    }
    Ok(())
}

/// Rejects source insets that are negative or overlap inside their PNG rectangle.
fn validate_source_insets(appearance: &str, insets: SliceInsets, width: i32, height: i32) -> Result<(), ThemeLoadError> {
    // Unlike destinations, a PNG source has fixed pixels and cannot proportionally invent a valid
    // center when opposing source slices exceed its dimensions.
    let invalid = insets.left < 0
        || insets.top < 0
        || insets.right < 0
        || insets.bottom < 0
        || insets.left.saturating_add(insets.right) > width
        || insets.top.saturating_add(insets.bottom) > height;
    if invalid {
        return Err(ThemeLoadError::InvalidInsets {
            appearance: appearance.to_owned(),
            field: "source_insets",
            insets,
            image_size: Some((width, height)),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChromeRole, ControlRole, MenuRole};
    use crate::test_support::test_atlas;

    /// Installs one repository-bundled theme through the same filesystem and PNG path as Context.
    fn install_bundled_theme(relative_path: &str) -> (LoadedTheme, usize) {
        // Resolve from the Cargo manifest so tests remain independent of the process working dir.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative_path);
        let loaded = load(path.as_path(), &test_atlas()).expect("bundled theme must load through the complete entry point");
        let image_count = loaded
            .bundle()
            .atlas()
            .clone_icon_table()
            .iter()
            .filter(|(name, _)| name.starts_with("@microui-theme-image/"))
            .count();
        (loaded, image_count)
    }

    /// Verifies missing state patches remain state-specific flat fallbacks after palette replacement.
    #[test]
    fn omitted_patch_states_use_theme_flat_colors() {
        let document = ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Flat only",
                "skin": {
                    "palette": {
                        "button": [1, 2, 3, 255],
                        "button_hovered": [4, 5, 6, 255],
                        "disabled_background": [7, 8, 9, 255]
                    }
                },
                "appearances": {
                    "control": {
                        "button": { "insets": { "left": 2, "top": 2, "right": 2, "bottom": 2 } }
                    }
                }
            }"#,
        )
        .expect("test definition must match the strict schema");
        let atlas = test_atlas();
        let theme_atlas = document.build_atlas(&atlas).expect("flat theme must retain the base atlas");
        let loaded = document.install(theme_atlas).expect("flat-only theme must install");

        let normal = loaded
            .bundle()
            .skin()
            .control(ControlRole::Button, ControlState::Enabled(PointerState::Normal))
            .patch;
        let hovered = loaded
            .bundle()
            .skin()
            .control(ControlRole::Button, ControlState::Enabled(PointerState::Hovered))
            .patch;
        let disabled = loaded.bundle().skin().control(ControlRole::Button, ControlState::Disabled).patch;
        assert_eq!(normal.insets.left, 2);
        assert!(matches!(
            normal.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if color.r == 1)
        ));
        assert!(matches!(
            hovered.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if color.r == 4)
        ));
        assert!(matches!(
            disabled.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b) == (7, 8, 9))
        ));
    }

    /// Verifies one state can replace text and glyph color without supplying background artwork.
    #[test]
    fn state_content_color_override_does_not_require_a_patch() {
        let document = ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Content-color states",
                "skin": { "palette": { "menu_foreground": [1, 2, 3, 255] } },
                "appearances": {
                    "menu": {
                        "item": {
                            "hovered": { "content_color": [250, 251, 252, 255] },
                            "disabled": { "content_color": [90, 91, 92, 255] }
                        }
                    }
                }
            }"#,
        )
        .expect("content-color-only states must match the strict schema");
        let atlas = test_atlas();
        let theme_atlas = document.build_atlas(&atlas).expect("content-color-only theme must retain the base atlas");
        let loaded = document.install(theme_atlas).expect("content-color-only theme must install");

        let normal = loaded.bundle().skin().menu(MenuRole::Item, MenuState::Normal).content_color;
        let hovered = loaded.bundle().skin().menu(MenuRole::Item, MenuState::Hovered).content_color;
        let disabled = loaded.bundle().skin().menu(MenuRole::Item, MenuState::Disabled).content_color;
        assert_eq!((normal.r, normal.g, normal.b, normal.a), (1, 2, 3, 255));
        assert_eq!((hovered.r, hovered.g, hovered.b, hovered.a), (250, 251, 252, 255));
        assert_eq!((disabled.r, disabled.g, disabled.b, disabled.a), (90, 91, 92, 255));
    }

    /// Verifies a solid patch is resource-free and covers every cell while retaining role geometry.
    #[test]
    fn solid_patch_compiles_without_rebuilding_the_atlas() {
        let document = ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Solid panel",
                "appearances": {
                    "surface": {
                        "panel": {
                            "insets": { "left": 2, "top": 3, "right": 4, "bottom": 5 },
                            "normal": { "patch": { "type": "solid", "color": [11, 22, 33, 255] } }
                        }
                    }
                }
            }"#,
        )
        .expect("solid patch must match the strict tagged schema");
        let base = test_atlas();
        let theme_atlas = document.build_atlas(&base).expect("solid patch must not require atlas work");
        assert!(theme_atlas.atlas.ptr_eq(&base));
        assert!(theme_atlas.images.is_empty());
        let loaded = document.install(theme_atlas).expect("solid patch must install");
        let patch = loaded.bundle().skin().surface(SurfaceRole::Panel, SurfaceState::Normal).patch;

        assert_eq!((patch.insets.left, patch.insets.top, patch.insets.right, patch.insets.bottom), (2, 3, 4, 5));
        assert!(matches!(
            patch.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.top_left, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (11, 22, 33, 255))
                    && matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (11, 22, 33, 255))
        ));
    }

    /// Verifies image-only data cannot exist beside or inside the wrong patch alternative.
    #[test]
    fn patch_variants_reject_meaningless_field_combinations() {
        let invalid_states = [
            (r#"{ "source_insets": { "left": 1, "top": 1, "right": 1, "bottom": 1 } }"#, "source_insets"),
            (r#"{ "patch": { "type": "solid", "color": [1, 2, 3, 255], "path": "unused.png" } }"#, "path"),
            (r#"{ "patch": { "type": "image", "tint": [255, 255, 255, 255] } }"#, "path"),
        ];

        for (state, rejected_field) in invalid_states {
            let json = format!(
                r#"{{
                    "schema_version": 1,
                    "name": "Invalid patch",
                    "appearances": {{ "surface": {{ "panel": {{ "normal": {state} }} }} }}
                }}"#
            );
            let error = match ThemeDefinition::parse_for_test(&json) {
                Ok(_) => panic!("meaningless `{rejected_field}` combination must be rejected"),
                Err(error) => error,
            };
            assert!(
                error.to_string().contains(rejected_field),
                "diagnostic must identify `{rejected_field}`: {error}"
            );
        }
    }

    /// Verifies the strict schema rejects misspelled fields instead of silently ignoring them.
    #[test]
    fn unknown_json_fields_are_rejected() {
        let error = match ThemeDefinition::parse_for_test(r#"{ "schema_version": 1, "name": "Broken", "appearences": {} }"#) {
            Ok(_) => panic!("misspelled root field must be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("unknown field `appearences`"));
    }

    /// Verifies the loader accepts exactly schema v1 without maintaining compatibility branches.
    #[test]
    fn a_different_theme_schema_is_not_accepted_as_a_compatibility_format() {
        let error = match ThemeDefinition::parse_for_test(r#"{ "schema_version": 2, "name": "Future" }"#) {
            Ok(_) => panic!("schema version two must not enter the version-one compiler"),
            Err(error) => error,
        };

        // An exact version check keeps parsing deterministic without aliases or migration code.
        assert!(matches!(
            error,
            ThemeLoadError::UnsupportedSchema {
                expected: THEME_SCHEMA_VERSION,
                actual: 2,
            }
        ));
    }

    /// Verifies the removed composition feature cannot silently return through a permissive root.
    #[test]
    fn extends_is_rejected_as_an_unknown_schema_field() {
        let error = match ThemeDefinition::parse_for_test(r#"{ "schema_version": 1, "name": "Child", "extends": "parent.json" }"#) {
            Ok(_) => panic!("a parent link must not be accepted by the single-file schema"),
            Err(error) => error,
        };

        // Explicit rejection distinguishes removal from accidentally ignoring the obsolete field.
        assert!(error.to_string().contains("unknown field `extends`"));
    }

    /// Verifies misspelled appearance keys are rejected directly by the concrete role enum.
    #[test]
    fn unknown_appearance_role_key_is_rejected_during_decode() {
        let error = match ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Broken role",
                "appearances": { "control": { "buton": {} } }
            }"#,
        ) {
            Ok(_) => panic!("an unknown control role must not enter theme compilation"),
            Err(error) => error,
        };

        assert!(matches!(error, ThemeLoadError::DefinitionJson { source, .. } if source.to_string().contains("buton")));
    }

    /// Verifies a declared font catalog is complete rather than silently borrowing a missing role
    /// from whichever atlas happened to be active while the theme was loaded.
    #[test]
    fn font_catalog_requires_every_semantic_role() {
        let error = match ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Incomplete fonts",
                "fonts": {
                    "texture_width": 512,
                    "texture_height": 256,
                    "body": { "path": "body.ttf", "size": 12 },
                    "small": { "path": "body.ttf", "size": 10 },
                    "title": { "path": "title.ttf", "size": 12 },
                    "heading": { "path": "body.ttf", "size": 18 }
                }
            }"#,
        ) {
            Ok(_) => panic!("missing mono role must violate the strict font schema"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("missing field `mono`"));
    }

    /// Verifies a theme-owned semantic icon set is complete rather than mixing unrelated base
    /// artwork into whichever roles the document forgot to declare.
    #[test]
    fn icon_catalog_requires_every_semantic_role() {
        let error = match ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Incomplete icons",
                "icons": {
                    "close": { "path": "close.png" },
                    "expand": { "path": "expand.png" },
                    "collapse": { "path": "collapse.png" },
                    "check": { "path": "check.png" },
                    "expand_down": { "path": "expand-down.png" },
                    "open_folder": { "path": "open-folder.png" },
                    "closed_folder": { "path": "closed-folder.png" }
                }
            }"#,
        ) {
            Ok(_) => panic!("missing file icon role must violate the strict icon schema"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("missing field `file`"));
    }

    /// Verifies the bundled Windows theme and every original PNG install through the public schema.
    #[test]
    fn bundled_windows_95_theme_reuses_shared_png_regions() {
        let (loaded, images) = install_bundled_theme("themes/windows-95/theme.json");
        assert_eq!(loaded.name(), "Windows 95");
        assert_eq!(images, 11, "each shared PNG path must be baked exactly once");
        assert_eq!(loaded.bundle().skin().window_chrome.minimized_content_height, 2);
        assert_eq!(loaded.bundle().skin().window_chrome.captions.vertical_spacing, 2);
        let atlas = loaded.bundle().atlas();
        assert_eq!((atlas.width(), atlas.height()), (512, 256));
        assert_eq!(
            atlas.clone_font_table().len(),
            5,
            "the theme atlas must contain exactly its five semantic font roles"
        );
        assert_eq!(atlas.get_font_size(atlas.font_id("heading").unwrap()), 18);
        let insets = loaded.bundle().skin().chrome(ChromeRole::WindowFrame, ChromeState::Base).patch.insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (4, 4, 4, 4));
        assert!(matches!(
            loaded.bundle().skin().control(ControlRole::Button, ControlState::Disabled).patch.content,
            crate::NinePatchContent::Image { .. }
        ));
        assert!(matches!(
            loaded.bundle().skin().chrome(ChromeRole::DialogFrame, ChromeState::Active).patch.content,
            crate::NinePatchContent::Image { .. }
        ));
        for state in [ControlState::Focused(PointerState::Normal), ControlState::Focused(PointerState::Hovered)] {
            let disclosure = loaded.bundle().skin().control(ControlRole::Item, state).patch;
            assert!(matches!(
                disclosure.content,
                crate::NinePatchContent::Flat { cells }
                    if matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (0, 0, 128, 255))
            ));
            let content_color = loaded.bundle().skin().control(ControlRole::Item, state).content_color;
            assert_eq!((content_color.r, content_color.g, content_color.b, content_color.a), (255, 255, 255, 255));
        }
    }

    /// Verifies the earlier Windows theme keeps its period title colors inside black frames.
    #[test]
    fn bundled_windows_311_theme_draws_black_framed_white_and_blue_titles() {
        let (loaded, images) = install_bundled_theme("themes/windows-3.11/theme.json");
        assert_eq!(loaded.name(), "Windows 3.11 for Workgroups");
        assert_eq!(images, 12, "only referenced role-state artwork is baked into the theme atlas");
        assert_eq!(loaded.bundle().skin().window_chrome.minimized_content_height, 2);
        assert_eq!(loaded.bundle().skin().window_chrome.captions.vertical_spacing, 0);
        let insets = loaded.bundle().skin().chrome(ChromeRole::WindowFrame, ChromeState::Base).patch.insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (23, 23, 23, 23));
        let border = loaded.bundle().skin().metrics.window_border;
        assert_eq!((border.left, border.top, border.right, border.bottom), (4, 4, 4, 4));
        let content = loaded.bundle().skin().metrics.window_content_insets;
        assert_eq!(
            (content.left, content.top, content.right, content.bottom),
            (2, 2, 2, 2),
            "the period theme leaves a narrow application margin inside root chrome"
        );
        let window_surface = loaded.bundle().skin().surface(SurfaceRole::Window, SurfaceState::Normal).patch;
        assert!(matches!(
            window_surface.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (255, 255, 255, 255))
        ));
        // Ordinary windows retain their long L-corner bitmap. Only modal dialogs use the uniform
        // four-pixel blue focus frame visible around period Windows 3.11 dialog boxes.
        assert!(matches!(
            loaded.bundle().skin().chrome(ChromeRole::WindowFrame, ChromeState::Active).patch.content,
            crate::NinePatchContent::Image { .. }
        ));
        let dialog_frame = loaded.bundle().skin().chrome(ChromeRole::DialogFrame, ChromeState::Active).patch;
        assert_eq!(
            (
                dialog_frame.insets.left,
                dialog_frame.insets.top,
                dialog_frame.insets.right,
                dialog_frame.insets.bottom,
            ),
            (4, 4, 4, 4)
        );
        assert!(matches!(
            dialog_frame.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.top, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (0, 0, 170, 255))
                    && matches!(cells.center, crate::NinePatchCell::Empty)
        ));
        let menu_popup = loaded.bundle().skin().menu(MenuRole::Popup, MenuState::Normal).patch;
        assert_eq!(
            (menu_popup.insets.left, menu_popup.insets.top, menu_popup.insets.right, menu_popup.insets.bottom),
            (1, 1, 1, 1)
        );
        assert!(matches!(
            menu_popup.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.top, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (0, 0, 0, 255))
                    && matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (255, 255, 255, 255))
        ));
        // The outer Windows frame already supplies the title's top, left, and right black edges.
        // The title patch contributes only the missing one-pixel bottom edge, avoiding a doubled
        // line where the two patches meet while leaving themes without framed titles unchanged.
        for (state, expected_center) in [
            (ChromeState::Base, (255, 255, 255, 255)),
            (ChromeState::Active, (0, 0, 170, 255)),
            (ChromeState::Disabled, (255, 255, 255, 255)),
        ] {
            let patch = loaded.bundle().skin().chrome(ChromeRole::Title, state).patch;
            assert_eq!((patch.insets.left, patch.insets.top, patch.insets.right, patch.insets.bottom), (0, 0, 0, 1));
            assert!(matches!(
                patch.content,
                crate::NinePatchContent::Flat { cells }
                    if matches!(cells.top, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (0, 0, 0, 255))
                        && matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == expected_center)
            ));
        }
        // Base Windows 3.11 titles are white and require black text, while active blue titles use
        // white text. Pointer interaction is not a representable chrome state.
        let base = loaded.bundle().skin().chrome(ChromeRole::Title, ChromeState::Base).content_color;
        let active = loaded.bundle().skin().chrome(ChromeRole::Title, ChromeState::Active).content_color;
        assert_eq!((base.r, base.g, base.b, base.a), (0, 0, 0, 255));
        assert_eq!((active.r, active.g, active.b, active.a), (255, 255, 255, 255));
        let disabled_title = loaded.bundle().skin().chrome(ChromeRole::Title, ChromeState::Disabled).content_color;
        assert_eq!((disabled_title.r, disabled_title.g, disabled_title.b, disabled_title.a), (125, 125, 125, 255));
        let minimize = loaded
            .bundle()
            .skin()
            .control(ControlRole::MinimizeButton, ControlState::Enabled(PointerState::Normal));
        assert_eq!(
            (
                minimize.content_color.r,
                minimize.content_color.g,
                minimize.content_color.b,
                minimize.content_color.a
            ),
            (0, 0, 0, 255),
            "the caption control's content color drives its manager-owned semantic symbol"
        );
        // Disabling must preserve the raised caption face authored by the theme. Letting these
        // roles fall back to their flat palette appearance would combine a one-pixel black frame
        // with the role's three-pixel visual insets and produce an incorrect heavy black square.
        for role in [
            ControlRole::CloseButton,
            ControlRole::MinimizeButton,
            ControlRole::MaximizeButton,
            ControlRole::RestoreButton,
        ] {
            let normal = loaded.bundle().skin().control(role, ControlState::Enabled(PointerState::Normal)).patch;
            let disabled = loaded.bundle().skin().control(role, ControlState::Disabled).patch;
            assert!(matches!(
                (normal.content, disabled.content),
                (crate::NinePatchContent::Image { image: normal }, crate::NinePatchContent::Image { image: disabled })
                    if normal.icon == disabled.icon
            ));
        }
        let selected_text = loaded.bundle().skin().menu(MenuRole::Item, MenuState::Hovered).content_color;
        assert_eq!((selected_text.r, selected_text.g, selected_text.b, selected_text.a), (255, 255, 255, 255));
        let slider_normal = loaded
            .bundle()
            .skin()
            .control(ControlRole::SliderTrack, ControlState::Enabled(PointerState::Normal))
            .patch;
        let slider_focused = loaded
            .bundle()
            .skin()
            .control(ControlRole::SliderTrack, ControlState::Focused(PointerState::Normal))
            .patch;
        assert!(matches!(
            (slider_normal.content, slider_focused.content),
            (crate::NinePatchContent::Image { image: normal }, crate::NinePatchContent::Image { image: focused })
                if normal.icon == focused.icon
        ));
        for state in [
            ControlState::Enabled(PointerState::Hovered),
            ControlState::Enabled(PointerState::Pressed),
            ControlState::Focused(PointerState::Normal),
            ControlState::Focused(PointerState::Hovered),
            ControlState::Focused(PointerState::Pressed),
        ] {
            // Combo popup choices and tree rows share the semantic Item visual. Its complete
            // interaction ladder carries both blue selection art and contrasting text.
            assert!(matches!(
                loaded.bundle().skin().control(ControlRole::Item, state).patch.content,
                crate::NinePatchContent::Image { .. }
            ));
            let content_color = loaded.bundle().skin().control(ControlRole::Item, state).content_color;
            assert_eq!((content_color.r, content_color.g, content_color.b, content_color.a), (255, 255, 255, 255));
        }
    }

    /// Verifies the bundled Mac theme installs its semantic resources and asymmetric Platinum
    /// chrome geometry together with the generated state artwork.
    #[test]
    fn bundled_mac_os_9_theme_reuses_shared_png_regions() {
        let (loaded, images) = install_bundled_theme("themes/mac-os-9/theme.json");
        assert_eq!(loaded.name(), "Mac OS 9");
        assert_eq!(images, 29, "each shared PNG path must be baked exactly once");
        let chrome = loaded.bundle().skin().window_chrome;
        assert_eq!(chrome.title_alignment, crate::WindowTitleAlignment::Centered);
        assert_eq!(chrome.captions.close_side, crate::CaptionButtonSide::Leading);
        assert_eq!(chrome.captions.vertical_spacing, 3);
        assert_eq!(chrome.captions.inner_spacing, 4);
        assert_eq!(chrome.minimized_content_height, 2);
        assert!(!chrome.captions.show_without_activation);
        assert_eq!(loaded.bundle().skin().metrics.title_height, 20);
        for (role, expected_size) in [
            (FontRole::Body, 12),
            (FontRole::Small, 10),
            (FontRole::Title, 12),
            (FontRole::Heading, 18),
            (FontRole::Mono, 13),
        ] {
            let font = loaded.bundle().skin().resolve_font_role(loaded.bundle().atlas(), role);
            assert_eq!(loaded.bundle().atlas().get_font_size(font), expected_size);
        }
        for (role, expected_size) in [
            (IconRole::Close, (9, 9)),
            (IconRole::Expand, (9, 9)),
            (IconRole::Collapse, (9, 9)),
            (IconRole::Check, (11, 11)),
            (IconRole::ExpandDown, (9, 9)),
            (IconRole::OpenFolder, (16, 16)),
            (IconRole::ClosedFolder, (16, 16)),
            (IconRole::File, (16, 16)),
        ] {
            let icon = loaded.bundle().skin().resolve_icon_role(loaded.bundle().atlas(), role);
            let size = loaded.bundle().atlas().get_icon_size(icon);
            assert_eq!((size.width, size.height), expected_size);
        }
        let content = loaded.bundle().skin().metrics.window_content_insets;
        assert_eq!(
            (content.left, content.top, content.right, content.bottom),
            (0, 0, 0, 0),
            "Platinum windows expose their complete application body below root chrome"
        );
        let insets = loaded.bundle().skin().chrome(ChromeRole::WindowFrame, ChromeState::Base).patch.insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (6, 22, 6, 6));
        let border = loaded.bundle().skin().metrics.window_border;
        assert_eq!(
            (border.left, border.top, border.right, border.bottom),
            (6, 2, 6, 6),
            "the title begins below the two-pixel top edge while the remaining edges retain the six-layer frame"
        );
        let active_title = loaded.bundle().skin().chrome(ChromeRole::Title, ChromeState::Active).patch;
        let crate::NinePatchContent::Image { image: active_title_image } = active_title.content else {
            panic!("Mac OS 9 active title must use baked artwork");
        };
        let active_title_size = loaded.bundle().atlas().get_icon_size(active_title_image.icon);
        assert_eq!((active_title_size.width, active_title_size.height), (8, 20));
        assert_eq!(
            (
                active_title.insets.left,
                active_title.insets.top,
                active_title.insets.right,
                active_title.insets.bottom,
            ),
            (0, 0, 0, 2),
            "the centered label must not overwrite the title's lower shadow and black separator"
        );
        assert_eq!(
            (
                active_title_image.source_insets.left,
                active_title_image.source_insets.top,
                active_title_image.source_insets.right,
                active_title_image.source_insets.bottom,
            ),
            (0, 0, 0, 2)
        );
        let active_frame = loaded.bundle().skin().chrome(ChromeRole::WindowFrame, ChromeState::Active).patch;
        let crate::NinePatchContent::Image { image: active_frame_image } = active_frame.content else {
            panic!("Mac OS 9 active frame must use baked title-and-body artwork");
        };
        let active_frame_size = loaded.bundle().atlas().get_icon_size(active_frame_image.icon);
        assert_eq!((active_frame_size.width, active_frame_size.height), (13, 29));
        // Push buttons and checkboxes deliberately own different source geometry. Verifying their
        // baked sizes prevents later theme edits from collapsing both controls back onto the same
        // generic frame merely because both expose the same typed interaction states.
        let button = loaded.bundle().skin().control(ControlRole::Button, ControlState::Enabled(PointerState::Normal));
        let crate::NinePatchContent::Image { image: button_image } = button.patch.content else {
            panic!("Mac OS 9 buttons must use baked artwork");
        };
        let button_size = loaded.bundle().atlas().get_icon_size(button_image.icon);
        assert_eq!((button_size.width, button_size.height), (17, 17));
        let checkbox = loaded
            .bundle()
            .skin()
            .control(ControlRole::Checkbox, ControlState::Enabled(PointerState::Normal));
        let pressed_checkbox = loaded
            .bundle()
            .skin()
            .control(ControlRole::Checkbox, ControlState::Enabled(PointerState::Pressed));
        let focused_checkbox = loaded
            .bundle()
            .skin()
            .control(ControlRole::Checkbox, ControlState::Focused(PointerState::Normal));
        let disabled_checkbox = loaded.bundle().skin().control(ControlRole::Checkbox, ControlState::Disabled);
        let [checkbox_image, pressed_checkbox_image, focused_checkbox_image, disabled_checkbox_image] =
            [checkbox, pressed_checkbox, focused_checkbox, disabled_checkbox].map(|visual| {
                let crate::NinePatchContent::Image { image } = visual.patch.content else {
                    panic!("Mac OS 9 checkbox states must use baked artwork");
                };
                image
            });
        let checkbox_size = loaded.bundle().atlas().get_icon_size(checkbox_image.icon);
        assert_eq!((checkbox_size.width, checkbox_size.height), (13, 13));
        assert_ne!(checkbox_image.icon, pressed_checkbox_image.icon);
        assert_ne!(checkbox_image.icon, focused_checkbox_image.icon);
        assert_ne!(checkbox_image.icon, disabled_checkbox_image.icon);
        assert!(matches!(
            loaded.bundle().skin().chrome(ChromeRole::DialogFrame, ChromeState::Active).patch.content,
            crate::NinePatchContent::Image { .. }
        ));

        // Base and active chrome retain their independently authored frame bitmaps without a
        // pointer-state path that could select one from the other.
        let normal_frame = loaded.bundle().skin().chrome(ChromeRole::WindowFrame, ChromeState::Base).patch;
        let active_frame = loaded.bundle().skin().chrome(ChromeRole::WindowFrame, ChromeState::Active).patch;
        assert!(matches!(
            (normal_frame.content, active_frame.content),
            (crate::NinePatchContent::Image { image: normal }, crate::NinePatchContent::Image { image: active })
                if normal.icon != active.icon
        ));

        // Complete Mac caption faces suppress the manager-owned semantic symbol through their
        // ordinary transparent content color. Pressed artwork remains a distinct image for feedback.
        let close_visual = loaded
            .bundle()
            .skin()
            .control(ControlRole::CloseButton, ControlState::Enabled(PointerState::Normal));
        let close_pressed_visual = loaded
            .bundle()
            .skin()
            .control(ControlRole::CloseButton, ControlState::Enabled(PointerState::Pressed));
        assert_eq!(close_visual.content_color.a, 0);
        assert_eq!(close_pressed_visual.content_color.a, 0);
        let (crate::NinePatchContent::Image { image: close_image }, crate::NinePatchContent::Image { image: pressed_image }) =
            (close_visual.patch.content, close_pressed_visual.patch.content)
        else {
            panic!("Mac OS 9 close states must use baked artwork");
        };
        let close_size = loaded.bundle().atlas().get_icon_size(close_image.icon);
        assert_eq!((close_size.width, close_size.height), (13, 14));
        assert_ne!(close_image.icon, pressed_image.icon);

        // Platinum popup selection uses black image-backed rows with white content text, while
        // the popup itself retains its authored one-pixel black and beveled frame.
        let menu_popup = loaded.bundle().skin().menu(MenuRole::Popup, MenuState::Normal).patch;
        let crate::NinePatchContent::Image { image: menu_popup_image } = menu_popup.content else {
            panic!("Mac OS 9 popup must use baked artwork");
        };
        let menu_popup_size = loaded.bundle().atlas().get_icon_size(menu_popup_image.icon);
        assert_eq!((menu_popup_size.width, menu_popup_size.height), (7, 7));
        let selected_text = loaded.bundle().skin().menu(MenuRole::Item, MenuState::Hovered).content_color;
        assert_eq!((selected_text.r, selected_text.g, selected_text.b, selected_text.a), (255, 255, 255, 255));
        for state in [
            ControlState::Focused(PointerState::Normal),
            ControlState::Focused(PointerState::Hovered),
            ControlState::Focused(PointerState::Pressed),
        ] {
            assert!(matches!(
                loaded.bundle().skin().control(ControlRole::Item, state).patch.content,
                crate::NinePatchContent::Image { .. }
            ));
            let content_color = loaded.bundle().skin().control(ControlRole::Item, state).content_color;
            assert_eq!((content_color.r, content_color.g, content_color.b, content_color.a), (255, 255, 255, 255));
        }
    }
}
