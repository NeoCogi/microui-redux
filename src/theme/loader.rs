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

use super::{AppearanceRole, FlatPalette, FontRole, RoleTable, Skin, SkinBundle, VisualCatalog, VisualState};
use crate::{
    atlas::{
        AtlasHandle,
        builder::{Builder, BuilderError},
    },
    Color, IconId, ImageError, ImageSource, NinePatch, NinePatchImage, SliceInsets,
};

/// Theme-file schema version understood by this crate release.
pub const THEME_SCHEMA_VERSION: u32 = 1;

/// Maximum number of parent documents resolved for one loaded theme.
///
/// The bound keeps recursive filesystem input from consuming an unbounded call stack. Ordinary
/// theme families should require only one or two layers; thirty-two remains generous while making
/// pathological acyclic chains a typed validation error.
const MAX_THEME_INHERITANCE_DEPTH: usize = 32;

/// A loaded theme with its display name, rebuilt resource atlas, and complete resolved skin.
///
/// Fonts, semantic icons, and PNG state artwork share one immutable atlas. Install this complete
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
    /// A document extends itself directly or through a parent chain.
    InheritanceCycle {
        /// Canonical file path encountered for the second time.
        path: PathBuf,
    },
    /// A parent chain exceeds the loader's explicit recursion bound.
    InheritanceDepth {
        /// Definition path that would exceed the supported depth.
        path: PathBuf,
        /// Maximum number of parent documents accepted by the loader.
        maximum: usize,
    },
    /// The document's display name is empty or whitespace-only.
    EmptyName,
    /// An appearance map key does not name a built-in semantic role.
    UnknownAppearance {
        /// Unrecognized exact JSON object key.
        name: String,
    },
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
    /// A PNG referenced by the JSON definition could not be read.
    ImageRead {
        /// Fully resolved path relative to the definition file.
        path: PathBuf,
        /// Concrete filesystem failure.
        source: io::Error,
    },
    /// A referenced PNG could not be decoded under the shared image policy.
    ImageDecode {
        /// Fully resolved image path.
        path: PathBuf,
        /// Concrete image-layer diagnostic.
        source: ImageError,
    },
    /// The theme's fonts or state artwork could not be packed or structurally finalized.
    AtlasBuild {
        /// Concrete atlas-builder failure retaining its asset or validation classification.
        source: BuilderError,
    },
    /// A resolved font asset path cannot be represented by the string-based atlas builder API.
    FontPathNotUtf8 {
        /// Fully resolved path containing non-UTF-8 platform bytes.
        path: PathBuf,
    },
}

impl fmt::Display for ThemeLoadError {
    /// Formats each failure with the relevant document, appearance, or PNG path.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep classifications visible in text while retaining structured sources for callers.
        match self {
            Self::DefinitionRead { path, source } => write!(formatter, "failed to read theme definition {}: {source}", path.display()),
            Self::DefinitionJson { path, source } => write!(formatter, "invalid theme definition {}: {source}", path.display()),
            Self::UnsupportedSchema { expected, actual } => {
                write!(formatter, "unsupported theme schema {actual}; expected {expected}")
            }
            Self::InheritanceCycle { path } => write!(formatter, "theme inheritance cycle reaches {} more than once", path.display()),
            Self::InheritanceDepth { path, maximum } => {
                write!(formatter, "theme inheritance at {} exceeds the maximum depth of {maximum}", path.display())
            }
            Self::EmptyName => formatter.write_str("theme name must not be empty"),
            Self::UnknownAppearance { name } => write!(formatter, "unknown theme appearance `{name}`"),
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
            Self::ImageRead { path, source } => write!(formatter, "failed to read theme PNG {}: {source}", path.display()),
            Self::ImageDecode { path, source } => write!(formatter, "failed to decode theme PNG {}: {source}", path.display()),
            Self::AtlasBuild { source } => write!(formatter, "failed to build theme resource atlas: {source}"),
            Self::FontPathNotUtf8 { path } => write!(formatter, "theme font path is not valid UTF-8: {}", path.display()),
        }
    }
}

impl Error for ThemeLoadError {
    /// Exposes concrete I/O, JSON, image, or texture causes without erasing theme classifications.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Schema and inset errors are fully represented by their own fields and have no lower cause.
        match self {
            Self::DefinitionRead { source, .. } | Self::ImageRead { source, .. } => Some(source),
            Self::DefinitionJson { source, .. } => Some(source),
            Self::ImageDecode { source, .. } => Some(source),
            Self::AtlasBuild { source } => Some(source),
            Self::UnsupportedSchema { .. }
            | Self::InheritanceCycle { .. }
            | Self::InheritanceDepth { .. }
            | Self::EmptyName
            | Self::UnknownAppearance { .. }
            | Self::InvalidInsets { .. }
            | Self::FontPathNotUtf8 { .. } => None,
        }
    }
}

/// One fully constructed theme atlas and the capabilities assigned to unique PNG paths.
///
/// Keeping this intermediate concrete prevents installation from rediscovering numeric icon slots
/// or decoding files a second time. It is private because only a matching [`ThemeDefinition`] may
/// translate these construction capabilities into semantic appearances.
pub(crate) struct ThemeAtlas {
    /// Immutable pixels and metadata ready for renderer installation.
    atlas: AtlasHandle,
    /// Fully resolved image paths mapped to their atlas-owned icon capabilities.
    images: BTreeMap<PathBuf, IconId>,
}

impl ThemeAtlas {
    /// Borrows the immutable atlas used to construct the matching base [`Skin`].
    pub(crate) fn atlas(&self) -> &AtlasHandle {
        // Installation consumes this intermediate later; exposing only a shared handle here keeps
        // the path-to-capability table inseparable from the atlas that minted its IDs.
        &self.atlas
    }
}

/// Strict syntax tree for exactly one authored JSON file before inheritance resolution.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeDocument {
    /// Version selecting the exact schema contract.
    schema_version: u32,
    /// Human-readable selector label contributed by this most-derived document.
    name: String,
    /// Optional parent JSON path resolved relative to this document.
    extends: Option<PathBuf>,
    /// Optional complete semantic font recipe replacing the inherited recipe.
    fonts: Option<FontCatalogDocument>,
    /// Sparse scalar and flat-color overrides merged onto the parent layer.
    #[serde(default)]
    skin: SkinDocument,
    /// String-keyed syntax converted to concrete [`AppearanceRole`] values during resolution.
    #[serde(default)]
    appearances: BTreeMap<String, AppearanceDocument>,
}

/// Fully inherited and path-resolved compiler input used to build one runtime skin bundle.
///
/// This value contains no unvalidated appearance strings and no paths whose meaning depends on a
/// later current directory. Parent layers are gone: scalar fields and every role/state override
/// have already been merged into one concrete typed definition.
pub(crate) struct ThemeDefinition {
    /// Human-readable selector label from the most-derived source document.
    name: String,
    /// Optional complete semantic font recipe with fully resolved file paths.
    fonts: Option<FontCatalogDocument>,
    /// Fully merged sparse scalar and flat-color overrides.
    skin: SkinDocument,
    /// Closed role table containing optional authored appearance overrides.
    appearances: RoleTable<Option<AppearanceDocument>>,
}

impl ThemeDefinition {
    /// Reads, inherits, path-resolves, and validates one JSON theme definition.
    pub(crate) fn read(path: &Path) -> Result<Self, ThemeLoadError> {
        // One explicit ancestry stack handles both direct and indirect cycles while preserving a
        // small recursive implementation whose maximum depth is checked before another read.
        let mut ancestry = Vec::new();
        Self::read_layer(path, &mut ancestry)
    }

    /// Resolves one source layer and all of its parents into a single typed definition.
    fn read_layer(path: &Path, ancestry: &mut Vec<PathBuf>) -> Result<Self, ThemeLoadError> {
        if ancestry.len() >= MAX_THEME_INHERITANCE_DEPTH {
            // Reject the next layer before recursion so the configured maximum is exact and the
            // stack cannot grow in response to untrusted acyclic input.
            return Err(ThemeLoadError::InheritanceDepth {
                path: path.to_path_buf(),
                maximum: MAX_THEME_INHERITANCE_DEPTH,
            });
        }

        // Read through the caller's spelling for precise I/O diagnostics, then canonicalize the
        // existing file solely for cycle identity. This treats `a/../theme.json` and symlinked
        // paths as the same source without imposing a stringly path-normalization algorithm.
        let bytes = fs::read(path).map_err(|source| ThemeLoadError::DefinitionRead { path: path.to_path_buf(), source })?;
        let canonical_path = fs::canonicalize(path).map_err(|source| ThemeLoadError::DefinitionRead { path: path.to_path_buf(), source })?;
        if ancestry.contains(&canonical_path) {
            return Err(ThemeLoadError::InheritanceCycle { path: canonical_path });
        }
        let directory = canonical_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        ancestry.push(canonical_path);

        let document: ThemeDocument =
            serde_json::from_slice(bytes.as_slice()).map_err(|source| ThemeLoadError::DefinitionJson { path: path.to_path_buf(), source })?;
        if document.schema_version != THEME_SCHEMA_VERSION {
            return Err(ThemeLoadError::UnsupportedSchema {
                expected: THEME_SCHEMA_VERSION,
                actual: document.schema_version,
            });
        }
        if document.name.trim().is_empty() {
            return Err(ThemeLoadError::EmptyName);
        }

        let parent = match document.extends.as_ref() {
            Some(relative_parent) => {
                // Absolute paths remain absolute under Path::join; relative paths intentionally
                // belong to the document that declares the inheritance edge.
                let parent_path = directory.join(relative_parent);
                Some(Self::read_layer(parent_path.as_path(), ancestry)?)
            }
            None => None,
        };
        ancestry.pop();
        Self::resolve(document, directory.as_path(), parent)
    }

    /// Merges one decoded source document over its optional fully resolved parent.
    fn resolve(mut document: ThemeDocument, directory: &Path, parent: Option<Self>) -> Result<Self, ThemeLoadError> {
        // Resolve asset ownership before merging. Parent paths were resolved in their own layer,
        // so moving their typed documents into the child cannot reinterpret relative filenames.
        if let Some(fonts) = &mut document.fonts {
            fonts.resolve_paths(directory);
        }
        for appearance in document.appearances.values_mut() {
            appearance.resolve_paths(directory);
        }

        let mut definition = parent.unwrap_or_else(|| Self {
            name: String::new(),
            fonts: None,
            skin: SkinDocument::default(),
            appearances: RoleTable::filled(None),
        });
        definition.name = document.name;
        if let Some(fonts) = document.fonts {
            // Font catalogs are complete by schema, so a child replaces the entire inherited
            // semantic recipe instead of mixing file and texture dimensions across layers.
            definition.fonts = Some(fonts);
        }
        definition.skin.merge(document.skin);
        for (name, appearance) in document.appearances {
            let role = AppearanceRole::from_json_name(name.as_str()).ok_or_else(|| ThemeLoadError::UnknownAppearance { name })?;
            let mut merged = definition.appearances.get(role).clone().unwrap_or_default();
            merged.merge(appearance);
            definition.appearances.set(role, Some(merged));
        }
        Ok(definition)
    }

    /// Strictly parses one standalone source string for schema-focused unit tests.
    #[cfg(test)]
    fn parse_for_test(json: &str) -> Result<Self, ThemeLoadError> {
        // Tests without filesystem inheritance still cross the same version, name, typed-role, and
        // path-resolution boundary. A synthetic relative path keeps diagnostics deterministic.
        let path = Path::new("theme.json");
        let document: ThemeDocument = serde_json::from_str(json).map_err(|source| ThemeLoadError::DefinitionJson { path: path.to_path_buf(), source })?;
        if document.schema_version != THEME_SCHEMA_VERSION {
            return Err(ThemeLoadError::UnsupportedSchema {
                expected: THEME_SCHEMA_VERSION,
                actual: document.schema_version,
            });
        }
        if document.name.trim().is_empty() {
            return Err(ThemeLoadError::EmptyName);
        }
        assert!(document.extends.is_none(), "standalone parser cannot resolve an inheritance path");
        Self::resolve(document, Path::new("."), None)
    }

    /// Builds the atlas selected by this definition while preserving required base resources.
    ///
    /// Every unique state PNG is packed once beside fonts and icons. A completely flat document
    /// without a font recipe can reuse the base allocation exactly; adding artwork without new
    /// fonts repacks existing glyph bitmaps so the original font files are not required.
    pub(crate) fn build_atlas(&self, base: &AtlasHandle) -> Result<ThemeAtlas, ThemeLoadError> {
        let image_paths = self.image_paths();
        if self.fonts.is_none() && image_paths.is_empty() {
            // No atlas-visible resource changes are requested, so preserving pointer identity also
            // lets selecting a palette-only theme avoid an unnecessary backend atlas upload.
            return Ok(ThemeAtlas {
                atlas: base.clone(),
                images: BTreeMap::new(),
            });
        }

        let mut decoded_images = Vec::with_capacity(image_paths.len());
        for path in image_paths {
            // Validate the complete file set before atlas packing. A missing or malformed later
            // asset therefore retains its precise path classification even when a compact base
            // atlas would also be too small for an artwork-only rebuild.
            let bytes = fs::read(path.as_path()).map_err(|source| ThemeLoadError::ImageRead { path: path.clone(), source })?;
            let (image_width, image_height, pixels) = crate::load_image_bytes(ImageSource::Png { bytes: bytes.as_slice() })
                .map_err(|source| ThemeLoadError::ImageDecode { path: path.clone(), source })?;
            decoded_images.push((path, image_width, image_height, pixels));
        }

        let (width, height) = self
            .fonts
            .as_ref()
            .map(|fonts| (fonts.texture_width, fonts.texture_height))
            .unwrap_or_else(|| (base.width(), base.height()));
        let mut builder = if self.fonts.is_some() {
            // Explicit recipes replace the five semantic roles while unrelated application fonts
            // and every icon survive from the stable source catalog. This keeps named FontRef
            // values valid without retaining stale typography from an earlier theme.
            let semantic_font_names = FontRole::ALL.map(FontRole::atlas_name);
            Builder::from_atlas_with_size_excluding_fonts(base, width, height, &semantic_font_names)
        } else {
            // Artwork-only themes retain exact base typography by copying its baked glyphs.
            Builder::from_atlas_with_size(base, width, height)
        }
        .map_err(|source| ThemeLoadError::AtlasBuild { source })?;

        if let Some(fonts) = &self.fonts {
            for (role, font) in fonts.entries() {
                let path = font.path.clone();
                let path_text = path.to_str().ok_or_else(|| ThemeLoadError::FontPathNotUtf8 { path: path.clone() })?;
                builder
                    .add_font_named(role.atlas_name(), path_text, font.size)
                    .map_err(|source| ThemeLoadError::AtlasBuild { source })?;
            }
        }

        let mut images = BTreeMap::new();
        for (index, (path, image_width, image_height, pixels)) in decoded_images.into_iter().enumerate() {
            // The opaque internal name is deterministic, while semantic lookup retains the fully
            // resolved PathBuf and each appearance retains its own slice and tint metadata.
            let icon = builder
                .add_icon_pixels_named(format!("@microui-theme-image/{index}").as_str(), image_width, image_height, pixels.as_slice())
                .map_err(|source| ThemeLoadError::AtlasBuild { source })?;
            images.insert(path, icon);
        }

        let atlas = builder.build().map_err(|source| ThemeLoadError::AtlasBuild { source })?;
        Ok(ThemeAtlas { atlas, images })
    }

    /// Resolves flat fallbacks and binds explicitly supplied PNG states to baked atlas regions.
    pub(crate) fn install(self, theme_atlas: ThemeAtlas, mut skin: Skin) -> Result<LoadedTheme, ThemeLoadError> {
        let ThemeAtlas { atlas, images } = theme_atlas;
        // Installation may only bind appearance icons onto a Skin created for this exact atlas;
        // enforcing that here prevents a LoadedTheme from ever containing mixed resources.
        assert!(skin.belongs_to(&atlas), "theme base skin contains image capabilities from another atlas");
        // Apply palette and metric overrides before constructing fallbacks, so every omitted PNG
        // state reflects the JSON theme's own flat colors rather than the built-in default palette.
        let mut palette = FlatPalette::default();
        self.skin.apply(&mut skin, &mut palette);
        let frame_insets = self.skin.frame_insets.map(InsetsDocument::into_insets).unwrap_or_else(|| skin.frame_insets());
        validate_non_negative("generic_frame", "skin.frame_insets", frame_insets)?;
        validate_non_negative("window_content", "skin.metrics.window_content_insets", skin.metrics.window_content_insets)?;
        validate_non_negative("window_frame", "skin.metrics.window_border", skin.metrics.window_border)?;
        skin.visuals = VisualCatalog::from_flat_palette(frame_insets, &palette);
        skin.effects.focus_outline = palette.focus;
        skin.effects.window_activation = palette.window_focus;
        skin.chrome.set_backdrop_color(palette.title_background);
        for role in AppearanceRole::ALL {
            let Some(document) = self.appearances.get(role) else {
                continue;
            };
            let name = role.json_name();
            let mut visuals = skin.visuals.get(role);
            let destination_insets = document
                .insets
                .map(InsetsDocument::into_insets)
                .unwrap_or_else(|| visuals.get(VisualState::Normal).patch.insets);
            validate_non_negative(name, "insets", destination_insets)?;

            // Role-level insets also apply to states that intentionally omit a PNG and retain their
            // flat fallback, keeping layout stable across hover/focus transitions.
            for state in VisualState::ALL {
                let mut visual = *visuals.get(state);
                visual.patch = visual.patch.with_insets(destination_insets);
                visuals.set(state, visual);
            }
            for (state, state_document) in document.states() {
                let Some(state_document) = state_document else {
                    continue;
                };
                if let Some(foreground) = state_document.foreground {
                    // A foreground can change without image artwork, but remains stored in the
                    // same complete visual as the state's background patch.
                    let mut visual = *visuals.get(state);
                    visual.foreground = foreground.into_color();
                    visuals.set(state, visual);
                }
                let Some(path) = state_document.png.as_ref() else {
                    continue;
                };
                let source_insets = state_document.source_insets.map(InsetsDocument::into_insets).unwrap_or(destination_insets);
                let icon = *images.get(path).expect("every declared theme PNG must have one baked atlas capability");
                let image_size = atlas.get_icon_size(icon);
                validate_source_insets(name, source_insets, image_size.width, image_size.height)?;
                let tint = state_document
                    .tint
                    .map(ColorDocument::into_color)
                    .unwrap_or(Color { r: 255, g: 255, b: 255, a: 255 });
                let image = NinePatchImage::new(icon, source_insets, tint);
                let mut visual = *visuals.get(state);
                visual.patch = NinePatch::image(destination_insets, image);
                visuals.set(state, visual);
            }
            skin.visuals.set(role, visuals);
        }

        // Pair atlas and resolved skin at the only public construction boundary before naming it.
        Ok(LoadedTheme::new(self.name, SkinBundle::new(atlas, skin)))
    }

    /// Returns every fully resolved PNG path in deterministic deduplicated order.
    fn image_paths(&self) -> BTreeSet<PathBuf> {
        // Role and state documents retain independent slicing/tint metadata, while one path set is
        // sufficient for immutable pixel packing. BTreeSet also stabilizes generated atlas names.
        self.appearances
            .iter()
            .filter_map(Option::as_ref)
            .flat_map(AppearanceDocument::states)
            .filter_map(|(_, state)| state.and_then(|state| state.png.as_ref()))
            .cloned()
            .collect()
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
        // Traverse through the same typed role table used by atlas insertion so no semantic font
        // can retain ambiguous path ownership after inheritance resolution.
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

impl FontDocument {
    /// Makes this font path independent of any later child document directory.
    fn resolve_path(&mut self, directory: &Path) {
        // Path::join preserves an already absolute path and anchors an ordinary relative path to
        // the source layer that owns this concrete font recipe.
        self.path = directory.join(self.path.as_path());
    }
}

/// Optional skin metrics and colors applied before appearance fallback construction.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SkinDocument {
    /// Default layout cell width override.
    default_cell_width: Option<i32>,
    /// General widget inner padding override.
    padding: Option<i32>,
    /// Independent application-body inset applied after root-owned title and menu chrome.
    window_content_insets: Option<InsetsDocument>,
    /// Layout spacing override.
    spacing: Option<i32>,
    /// Nested-content indentation override.
    indent: Option<i32>,
    /// Window title height override.
    title_height: Option<i32>,
    /// Optional platform-oriented title and caption-button arrangement.
    window_chrome_layout: Option<WindowChromeLayoutDocument>,
    /// Structural window-edge thickness independent from frame-art corner spans.
    window_border: Option<InsetsDocument>,
    /// Scrollbar cross-axis thickness override.
    scrollbar_size: Option<i32>,
    /// Minimum slider and scrollbar thumb size override.
    thumb_size: Option<i32>,
    /// Structural fallback frame insets.
    frame_insets: Option<InsetsDocument>,
    /// Optional semantic flat palette overrides.
    colors: ColorPaletteDocument,
}

impl SkinDocument {
    /// Merges one more-derived sparse skin layer over this inherited layer.
    fn merge(&mut self, overlay: Self) {
        // Every optional scalar has one explicit ownership point. Replacing only present values
        // preserves parent fields without reflection, string maps, or a second runtime skin type.
        replace_if_some(&mut self.default_cell_width, overlay.default_cell_width);
        replace_if_some(&mut self.padding, overlay.padding);
        replace_if_some(&mut self.window_content_insets, overlay.window_content_insets);
        replace_if_some(&mut self.spacing, overlay.spacing);
        replace_if_some(&mut self.indent, overlay.indent);
        replace_if_some(&mut self.title_height, overlay.title_height);
        replace_if_some(&mut self.window_chrome_layout, overlay.window_chrome_layout);
        replace_if_some(&mut self.window_border, overlay.window_border);
        replace_if_some(&mut self.scrollbar_size, overlay.scrollbar_size);
        replace_if_some(&mut self.thumb_size, overlay.thumb_size);
        replace_if_some(&mut self.frame_insets, overlay.frame_insets);
        self.colors.merge(overlay.colors);
    }

    /// Applies present fields and returns foreground inputs for visual fallback construction.
    fn apply(&self, skin: &mut Skin, palette: &mut FlatPalette) {
        // Keep assignment explicit so the strict JSON schema and public Skin fields cannot drift
        // through reflection or stringly typed mutation.
        assign_if_some(&mut skin.metrics.default_cell_width, self.default_cell_width);
        assign_if_some(&mut skin.metrics.padding, self.padding);
        if let Some(window_content_insets) = self.window_content_insets {
            // Root layout consumes the concrete four-edge value directly, so asymmetric theme
            // insets require no erased metric map or widget-specific special casing.
            skin.metrics.window_content_insets = window_content_insets.into_insets();
        }
        assign_if_some(&mut skin.metrics.spacing, self.spacing);
        assign_if_some(&mut skin.metrics.indent, self.indent);
        assign_if_some(&mut skin.metrics.title_height, self.title_height);
        if let Some(window_chrome_layout) = self.window_chrome_layout {
            // The schema enum converts once into the public runtime enum, keeping deserialization
            // details out of Skin when JSON support is not compiled.
            skin.chrome = window_chrome_layout.into_skin(crate::Color { r: 0, g: 0, b: 0, a: 0 });
        }
        if let Some(window_border) = self.window_border {
            // Keep the schema-to-runtime conversion explicit because negative components are
            // rejected by installation before they can affect layout or hit testing.
            skin.metrics.window_border = window_border.into_insets();
        }
        assign_if_some(&mut skin.metrics.scrollbar_size, self.scrollbar_size);
        assign_if_some(&mut skin.metrics.thumb_size, self.thumb_size);
        // The authored palette remains compiler input and is consumed into visuals by install.
        self.colors.apply(palette);
    }
}

/// Strict JSON spelling for the two supported manager-owned window chrome arrangements.
#[derive(Copy, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WindowChromeLayoutDocument {
    /// Conventional left title with all caption controls at the trailing edge.
    TrailingButtons,
    /// Platinum-style centered title with a leading close box and compact trailing controls.
    ClassicMac,
}

impl WindowChromeLayoutDocument {
    /// Converts one schema value into its complete data-driven runtime recipe.
    const fn into_skin(self, title_backdrop: crate::Color) -> crate::WindowChromeSkin {
        // Exhaustive matching keeps a future schema spelling from silently inheriting an unrelated
        // runtime policy.
        match self {
            Self::TrailingButtons => crate::WindowChromeSkin::trailing_buttons(),
            Self::ClassicMac => crate::WindowChromeSkin::classic_mac(title_backdrop),
        }
    }
}

/// Optional flat colors named by semantic use rather than numeric palette slots.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ColorPaletteDocument {
    /// Ordinary control text.
    text: Option<ColorDocument>,
    /// Generic frame border.
    border: Option<ColorDocument>,
    /// Window body background.
    window_background: Option<ColorDocument>,
    /// Passive window title background.
    title_background: Option<ColorDocument>,
    /// Window title text.
    title_text: Option<ColorDocument>,
    /// Text and semantic icon color used by disabled widgets and windows.
    disabled_text: Option<ColorDocument>,
    /// Shared flat background and control fill used by disabled widgets and windows.
    disabled_background: Option<ColorDocument>,
    /// Window title and caption-symbol color used by disabled chrome.
    disabled_title_text: Option<ColorDocument>,
    /// Panel and viewport background.
    panel_background: Option<ColorDocument>,
    /// Ordinary button fill.
    button: Option<ColorDocument>,
    /// Hovered button fill.
    button_hover: Option<ColorDocument>,
    /// Ordinary input fill.
    input: Option<ColorDocument>,
    /// Hovered input fill.
    input_hover: Option<ColorDocument>,
    /// Scrollbar track fill.
    scrollbar_track: Option<ColorDocument>,
    /// Scrollbar thumb fill.
    scrollbar_thumb: Option<ColorDocument>,
    /// Focused control accent.
    focus: Option<ColorDocument>,
    /// Active window accent.
    window_focus: Option<ColorDocument>,
    /// Menu text and marker color.
    menu_foreground: Option<ColorDocument>,
    /// Menu bar and popup background.
    menu_background: Option<ColorDocument>,
}

impl ColorPaletteDocument {
    /// Merges one more-derived sparse palette layer over inherited semantic colors.
    fn merge(&mut self, overlay: Self) {
        // Keep this list parallel to `apply`: the source compiler owns inheritance while the
        // runtime Skin receives only the final resolved palette values.
        replace_if_some(&mut self.text, overlay.text);
        replace_if_some(&mut self.border, overlay.border);
        replace_if_some(&mut self.window_background, overlay.window_background);
        replace_if_some(&mut self.title_background, overlay.title_background);
        replace_if_some(&mut self.title_text, overlay.title_text);
        replace_if_some(&mut self.disabled_text, overlay.disabled_text);
        replace_if_some(&mut self.disabled_background, overlay.disabled_background);
        replace_if_some(&mut self.disabled_title_text, overlay.disabled_title_text);
        replace_if_some(&mut self.panel_background, overlay.panel_background);
        replace_if_some(&mut self.button, overlay.button);
        replace_if_some(&mut self.button_hover, overlay.button_hover);
        replace_if_some(&mut self.input, overlay.input);
        replace_if_some(&mut self.input_hover, overlay.input_hover);
        replace_if_some(&mut self.scrollbar_track, overlay.scrollbar_track);
        replace_if_some(&mut self.scrollbar_thumb, overlay.scrollbar_thumb);
        replace_if_some(&mut self.focus, overlay.focus);
        replace_if_some(&mut self.window_focus, overlay.window_focus);
        replace_if_some(&mut self.menu_foreground, overlay.menu_foreground);
        replace_if_some(&mut self.menu_background, overlay.menu_background);
    }

    /// Applies supplied palette fields and returns foregrounds needed by fallback construction.
    fn apply(&self, palette: &mut FlatPalette) {
        // Assign every authored semantic color directly to a named compiler input. The resulting
        // palette is consumed immediately and never retained beside the resolved visual catalog.
        assign_color(&mut palette.text, self.text);
        assign_color(&mut palette.border, self.border);
        assign_color(&mut palette.window_background, self.window_background);
        assign_color(&mut palette.title_background, self.title_background);
        assign_color(&mut palette.title_foreground, self.title_text);
        assign_color(&mut palette.disabled_foreground, self.disabled_text);
        assign_color(&mut palette.disabled_background, self.disabled_background);
        assign_color(&mut palette.disabled_title_foreground, self.disabled_title_text);
        assign_color(&mut palette.panel_background, self.panel_background);
        assign_color(&mut palette.button, self.button);
        assign_color(&mut palette.button_hovered, self.button_hover);
        assign_color(&mut palette.input, self.input);
        assign_color(&mut palette.input_hovered, self.input_hover);
        assign_color(&mut palette.scrollbar_track, self.scrollbar_track);
        assign_color(&mut palette.scrollbar_thumb, self.scrollbar_thumb);
        assign_color(&mut palette.focus, self.focus);
        assign_color(&mut palette.window_focus, self.window_focus);
        assign_color(&mut palette.menu_foreground, self.menu_foreground);
        assign_color(&mut palette.menu_background, self.menu_background);
    }
}

/// One role's destination insets and optional per-state PNG definitions.
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AppearanceDocument {
    /// Destination-space outer row and column sizes shared by every state.
    insets: Option<InsetsDocument>,
    /// Normal state image.
    normal: Option<StateDocument>,
    /// Pointer-hover image.
    hovered: Option<StateDocument>,
    /// Pointer-pressed image.
    pressed: Option<StateDocument>,
    /// Keyboard-focused image.
    focused: Option<StateDocument>,
    /// Simultaneously hovered and focused image.
    hovered_focused: Option<StateDocument>,
    /// Simultaneously pressed and focused image.
    pressed_focused: Option<StateDocument>,
    /// Disabled image.
    disabled: Option<StateDocument>,
}

impl AppearanceDocument {
    /// Merges one more-derived role layer over an inherited appearance recipe.
    fn merge(&mut self, overlay: Self) {
        // Role insets inherit independently from state recipes. State objects merge field by field
        // so a child can retint or recolor inherited artwork without restating its owned path.
        replace_if_some(&mut self.insets, overlay.insets);
        merge_state(&mut self.normal, overlay.normal);
        merge_state(&mut self.hovered, overlay.hovered);
        merge_state(&mut self.pressed, overlay.pressed);
        merge_state(&mut self.focused, overlay.focused);
        merge_state(&mut self.hovered_focused, overlay.hovered_focused);
        merge_state(&mut self.pressed_focused, overlay.pressed_focused);
        merge_state(&mut self.disabled, overlay.disabled);
    }

    /// Resolves every PNG path against the directory of this exact source layer.
    fn resolve_paths(&mut self, directory: &Path) {
        // Named schema fields adapt once into typed state order, then each present state owns the
        // same concrete path-resolution behavior.
        for (_, state) in self.states_mut() {
            if let Some(state) = state {
                state.resolve_path(directory);
            }
        }
    }

    /// Returns every optional state document paired with its exact enum value.
    fn states(&self) -> [(VisualState, Option<&StateDocument>); VisualState::COUNT] {
        // Named fields make JSON readable; this fixed adapter preserves the catalog's typed order.
        [
            (VisualState::Normal, self.normal.as_ref()),
            (VisualState::Hovered, self.hovered.as_ref()),
            (VisualState::Pressed, self.pressed.as_ref()),
            (VisualState::Focused, self.focused.as_ref()),
            (VisualState::HoveredFocused, self.hovered_focused.as_ref()),
            (VisualState::PressedFocused, self.pressed_focused.as_ref()),
            (VisualState::Disabled, self.disabled.as_ref()),
        ]
    }

    /// Returns mutable optional state documents in exact enum order for source normalization.
    fn states_mut(&mut self) -> [(VisualState, Option<&mut StateDocument>); VisualState::COUNT] {
        // Separate named borrows are disjoint struct fields, so the fixed array safely provides one
        // mutable reference per state without an index map or runtime downcast.
        [
            (VisualState::Normal, self.normal.as_mut()),
            (VisualState::Hovered, self.hovered.as_mut()),
            (VisualState::Pressed, self.pressed.as_mut()),
            (VisualState::Focused, self.focused.as_mut()),
            (VisualState::HoveredFocused, self.hovered_focused.as_mut()),
            (VisualState::PressedFocused, self.pressed_focused.as_mut()),
            (VisualState::Disabled, self.disabled.as_mut()),
        ]
    }
}

/// Optional image supplied for one exact role state.
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StateDocument {
    /// Optional text and semantic-glyph color for this exact role and state.
    foreground: Option<ColorDocument>,
    /// PNG path relative to the containing JSON file; absence keeps the flat fallback.
    png: Option<PathBuf>,
    /// Source-space PNG slices; defaults to the role's destination insets.
    source_insets: Option<InsetsDocument>,
    /// Optional RGBA modulation; defaults to opaque white.
    tint: Option<ColorDocument>,
}

impl StateDocument {
    /// Merges one more-derived state layer over inherited visual source fields.
    fn merge(&mut self, overlay: Self) {
        // Each field is independently optional in the schema, so child documents can adjust
        // foreground, slicing, or tint while retaining a parent-owned PNG path.
        replace_if_some(&mut self.foreground, overlay.foreground);
        replace_if_some(&mut self.png, overlay.png);
        replace_if_some(&mut self.source_insets, overlay.source_insets);
        replace_if_some(&mut self.tint, overlay.tint);
    }

    /// Makes this state's optional PNG path independent of later inheritance layers.
    fn resolve_path(&mut self, directory: &Path) {
        // Only an authored image needs path ownership; flat states remain allocation-free.
        if let Some(path) = &mut self.png {
            *path = directory.join(path.as_path());
        }
    }
}

/// Four explicit slice components used in JSON documents.
#[derive(Copy, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct InsetsDocument {
    /// Left fixed column width.
    left: i32,
    /// Top fixed row height.
    top: i32,
    /// Right fixed column width.
    right: i32,
    /// Bottom fixed row height.
    bottom: i32,
}

impl InsetsDocument {
    /// Converts the schema value into the renderer's concrete inset type.
    const fn into_insets(self) -> SliceInsets {
        // Preserve exact values so the validation helper can report malformed negative input.
        SliceInsets::new(self.left, self.top, self.right, self.bottom)
    }
}

/// One exact RGBA array in JSON.
#[derive(Copy, Clone, Deserialize)]
struct ColorDocument(
    /// Red, green, blue, and alpha channels in byte order.
    [u8; 4],
);

impl ColorDocument {
    /// Converts the JSON byte tuple into the crate's public color value.
    const fn into_color(self) -> Color {
        // Array length and channel ranges were enforced by serde before this conversion.
        Color {
            r: self.0[0],
            g: self.0[1],
            b: self.0[2],
            a: self.0[3],
        }
    }
}

/// Replaces one optional source field only when the more-derived layer supplies a value.
fn replace_if_some<T>(destination: &mut Option<T>, overlay: Option<T>) {
    // Moving the concrete value keeps inheritance generic only over one homogeneous field type;
    // it never permits heterogeneous or runtime-erased theme data.
    if let Some(value) = overlay {
        *destination = Some(value);
    }
}

/// Merges one optional state document without duplicating seven field-level branches.
fn merge_state(destination: &mut Option<StateDocument>, overlay: Option<StateDocument>) {
    // A present child state refines an inherited state when available and otherwise becomes the
    // complete first layer for that typed role/state slot.
    if let Some(overlay) = overlay {
        match destination {
            Some(destination) => destination.merge(overlay),
            None => *destination = Some(overlay),
        }
    }
}

/// Assigns one optional copy value without hiding field-to-field schema mapping.
fn assign_if_some<T: Copy>(destination: &mut T, value: Option<T>) {
    // Omitted JSON fields deliberately preserve the atlas-derived base Skin value.
    if let Some(value) = value {
        *destination = value;
    }
}

/// Assigns one optional JSON color to a concrete public color field.
fn assign_color(destination: &mut Color, value: Option<ColorDocument>) {
    // Keep color conversion centralized so every palette field uses identical channel ordering.
    if let Some(value) = value {
        *destination = value.into_color();
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
    use crate::test_support::test_atlas;

    /// Installs one repository-bundled theme through the same filesystem and PNG path as Context.
    fn install_bundled_theme(relative_path: &str) -> (LoadedTheme, usize) {
        // Resolve from the Cargo manifest so tests remain independent of the process working dir.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
        let definition = ThemeDefinition::read(path.as_path()).expect("bundled theme JSON must remain valid");
        let theme_atlas = definition.build_atlas(&test_atlas()).expect("bundled theme resource atlas must build");
        let image_count = theme_atlas.images.len();
        let style = Skin::from_atlas(theme_atlas.atlas());
        let loaded = definition
            .install(theme_atlas, style)
            .expect("bundled theme PNGs must install from baked regions");
        (loaded, image_count)
    }

    /// Verifies missing state PNGs remain state-specific flat fallbacks after palette replacement.
    #[test]
    fn omitted_png_states_use_theme_flat_colors() {
        let document = ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Flat only",
                "skin": {
                    "colors": {
                        "button": [1, 2, 3, 255],
                        "button_hover": [4, 5, 6, 255],
                        "disabled_background": [7, 8, 9, 255]
                    }
                },
                "appearances": {
                    "button": { "insets": { "left": 2, "top": 2, "right": 2, "bottom": 2 } }
                }
            }"#,
        )
        .expect("test definition must match the strict schema");
        let atlas = test_atlas();
        let theme_atlas = document.build_atlas(&atlas).expect("flat theme must retain the base atlas");
        let style = Skin::from_atlas(theme_atlas.atlas());
        let loaded = document.install(theme_atlas, style).expect("flat-only theme must install");

        let normal = loaded.bundle().skin().appearance(AppearanceRole::Button, VisualState::Normal);
        let hovered = loaded.bundle().skin().appearance(AppearanceRole::Button, VisualState::Hovered);
        let disabled = loaded.bundle().skin().appearance(AppearanceRole::Button, VisualState::Disabled);
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
    fn state_foreground_override_does_not_require_a_png() {
        let document = ThemeDefinition::parse_for_test(
            r#"{
                "schema_version": 1,
                "name": "Foreground states",
                "skin": { "colors": { "menu_foreground": [1, 2, 3, 255] } },
                "appearances": {
                    "menu_item": {
                        "hovered": { "foreground": [250, 251, 252, 255] },
                        "disabled": { "foreground": [90, 91, 92, 255] }
                    }
                }
            }"#,
        )
        .expect("foreground-only states must match the strict schema");
        let atlas = test_atlas();
        let theme_atlas = document.build_atlas(&atlas).expect("foreground-only theme must retain the base atlas");
        let style = Skin::from_atlas(theme_atlas.atlas());
        let loaded = document.install(theme_atlas, style).expect("foreground-only theme must install");

        let normal = loaded.bundle().skin().foreground(AppearanceRole::MenuItem, VisualState::Normal);
        let hovered = loaded.bundle().skin().foreground(AppearanceRole::MenuItem, VisualState::Hovered);
        let disabled = loaded.bundle().skin().foreground(AppearanceRole::MenuItem, VisualState::Disabled);
        assert_eq!((normal.r, normal.g, normal.b, normal.a), (1, 2, 3, 255));
        assert_eq!((hovered.r, hovered.g, hovered.b, hovered.a), (250, 251, 252, 255));
        assert_eq!((disabled.r, disabled.g, disabled.b, disabled.a), (90, 91, 92, 255));
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

    /// Verifies inheritance merges typed fields while retaining each layer's asset directory.
    #[test]
    fn inherited_documents_resolve_paths_before_deep_typed_merge() {
        let root = tempfile::tempdir().expect("temporary inheritance directory must be available");
        let parent_directory = root.path().join("parent");
        let child_directory = root.path().join("child");
        fs::create_dir_all(parent_directory.as_path()).expect("parent directory must be creatable");
        fs::create_dir_all(child_directory.as_path()).expect("child directory must be creatable");
        fs::write(
            parent_directory.join("theme.json"),
            r#"{
                "schema_version": 1,
                "name": "Parent",
                "skin": {
                    "padding": 3,
                    "colors": { "button": [1, 2, 3, 255] }
                },
                "appearances": {
                    "button": {
                        "normal": { "png": "parent.png", "tint": [4, 5, 6, 255] }
                    }
                }
            }"#,
        )
        .expect("parent JSON must be writable");
        fs::write(
            child_directory.join("theme.json"),
            r#"{
                "schema_version": 1,
                "name": "Child",
                "extends": "../parent/theme.json",
                "skin": {
                    "spacing": 7,
                    "colors": { "button_hover": [8, 9, 10, 255] }
                },
                "appearances": {
                    "button": {
                        "normal": { "foreground": [11, 12, 13, 255] },
                        "hovered": { "png": "child.png" }
                    }
                }
            }"#,
        )
        .expect("child JSON must be writable");

        let definition = ThemeDefinition::read(child_directory.join("theme.json").as_path()).expect("typed inheritance must resolve");
        let button = definition
            .appearances
            .get(AppearanceRole::Button)
            .as_ref()
            .expect("button appearance must be inherited");
        let normal = button.normal.as_ref().expect("normal state must be inherited and refined");
        let hovered = button.hovered.as_ref().expect("hovered state must come from the child");

        assert_eq!(definition.name, "Child");
        assert_eq!(definition.skin.padding, Some(3));
        assert_eq!(definition.skin.spacing, Some(7));
        let parent_button = definition.skin.colors.button.expect("parent button color must survive").into_color();
        let child_hover = definition.skin.colors.button_hover.expect("child hover color must apply").into_color();
        assert_eq!((parent_button.r, parent_button.g, parent_button.b, parent_button.a), (1, 2, 3, 255));
        assert_eq!((child_hover.r, child_hover.g, child_hover.b, child_hover.a), (8, 9, 10, 255));
        assert_eq!(normal.png.as_deref(), Some(parent_directory.join("parent.png").as_path()));
        let child_foreground = normal.foreground.expect("child foreground must refine the inherited state").into_color();
        let parent_tint = normal.tint.expect("parent tint must survive the child refinement").into_color();
        assert_eq!(
            (child_foreground.r, child_foreground.g, child_foreground.b, child_foreground.a),
            (11, 12, 13, 255)
        );
        assert_eq!((parent_tint.r, parent_tint.g, parent_tint.b, parent_tint.a), (4, 5, 6, 255));
        assert_eq!(hovered.png.as_deref(), Some(child_directory.join("child.png").as_path()));
    }

    /// Verifies a cyclic parent graph fails with its concrete canonical source path.
    #[test]
    fn inheritance_cycles_are_rejected_before_source_merge() {
        let root = tempfile::tempdir().expect("temporary cycle directory must be available");
        let first = root.path().join("first.json");
        let second = root.path().join("second.json");
        fs::write(first.as_path(), r#"{ "schema_version": 1, "name": "First", "extends": "second.json" }"#).expect("first cycle document must be writable");
        fs::write(second.as_path(), r#"{ "schema_version": 1, "name": "Second", "extends": "first.json" }"#).expect("second cycle document must be writable");

        let error = match ThemeDefinition::read(first.as_path()) {
            Ok(_) => panic!("cyclic inheritance must fail before a definition is returned"),
            Err(error) => error,
        };

        assert!(matches!(error, ThemeLoadError::InheritanceCycle { path } if path == fs::canonicalize(first).unwrap()));
    }

    /// Verifies appearance strings become typed roles while source documents are resolved.
    #[test]
    fn unknown_inherited_appearance_is_rejected_during_read() {
        let root = tempfile::tempdir().expect("temporary appearance directory must be available");
        let parent = root.path().join("parent.json");
        let child = root.path().join("child.json");
        fs::write(
            parent.as_path(),
            r#"{
                "schema_version": 1,
                "name": "Parent",
                "appearances": { "buton": {} }
            }"#,
        )
        .expect("invalid parent document must be writable");
        fs::write(child.as_path(), r#"{ "schema_version": 1, "name": "Child", "extends": "parent.json" }"#).expect("child document must be writable");

        let error = match ThemeDefinition::read(child.as_path()) {
            Ok(_) => panic!("unknown parent appearance must fail during typed source resolution"),
            Err(error) => error,
        };

        assert!(matches!(error, ThemeLoadError::UnknownAppearance { name } if name == "buton"));
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

    /// Verifies the bundled Windows theme and every original PNG install through the public schema.
    #[test]
    fn bundled_windows_95_theme_reuses_shared_png_regions() {
        let (loaded, images) = install_bundled_theme("themes/windows-95/theme.json");
        assert_eq!(loaded.name(), "Windows 95");
        assert_eq!(images, 11, "each shared PNG path must be baked exactly once");
        let atlas = loaded.bundle().atlas();
        assert_eq!((atlas.width(), atlas.height()), (512, 256));
        assert_eq!(
            atlas.clone_font_table().len(),
            5,
            "the theme atlas must contain exactly its five semantic font roles"
        );
        assert_eq!(atlas.get_font_size(atlas.font_id("heading").unwrap()), 18);
        let insets = loaded.bundle().skin().appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (4, 4, 4, 4));
        assert!(matches!(
            loaded.bundle().skin().appearance(AppearanceRole::Button, VisualState::Disabled).content,
            crate::NinePatchContent::Image { .. }
        ));
        assert!(matches!(
            loaded
                .bundle()
                .skin()
                .appearance(AppearanceRole::DialogFrameActive, VisualState::Normal)
                .content,
            crate::NinePatchContent::Image { .. }
        ));
    }

    /// Verifies the earlier Windows theme remains a distinct definition with period title artwork.
    #[test]
    fn bundled_windows_311_theme_uses_white_and_blue_title_images() {
        let (loaded, images) = install_bundled_theme("themes/windows-3.11/theme.json");
        assert_eq!(loaded.name(), "Windows 3.11 for Workgroups");
        assert_eq!(images, 21, "each shared PNG path must be baked exactly once");
        let insets = loaded.bundle().skin().appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (23, 23, 23, 23));
        let border = loaded.bundle().skin().metrics.window_border;
        assert_eq!((border.left, border.top, border.right, border.bottom), (4, 4, 4, 4));
        let content = loaded.bundle().skin().metrics.window_content_insets;
        assert_eq!(
            (content.left, content.top, content.right, content.bottom),
            (0, 0, 0, 0),
            "period Windows applications decide their own content margins inside root chrome"
        );
        // Ordinary windows retain their long L-corner bitmap. Only modal dialogs use the uniform
        // four-pixel blue focus frame visible around period Windows 3.11 dialog boxes.
        assert!(matches!(
            loaded
                .bundle()
                .skin()
                .appearance(AppearanceRole::WindowFrameActive, VisualState::Normal)
                .content,
            crate::NinePatchContent::Image { .. }
        ));
        let dialog_frame = loaded.bundle().skin().appearance(AppearanceRole::DialogFrameActive, VisualState::Normal);
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
                    && matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (195, 199, 203, 255))
        ));
        let menu_popup = loaded.bundle().skin().appearance(AppearanceRole::MenuPopup, VisualState::Normal);
        assert_eq!(
            (menu_popup.insets.left, menu_popup.insets.top, menu_popup.insets.right, menu_popup.insets.bottom),
            (2, 2, 2, 2)
        );
        assert!(matches!(
            menu_popup.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.top, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (0, 0, 0, 255))
                    && matches!(cells.center, crate::NinePatchCell::Color { color } if (color.r, color.g, color.b, color.a) == (255, 255, 255, 255))
        ));
        assert!(matches!(
            loaded.bundle().skin().appearance(AppearanceRole::WindowTitle, VisualState::Pressed).content,
            crate::NinePatchContent::Image { .. }
        ));
        assert!(matches!(
            loaded
                .bundle()
                .skin()
                .appearance(AppearanceRole::WindowTitleActive, VisualState::Pressed)
                .content,
            crate::NinePatchContent::Image { .. }
        ));
        assert!(matches!(
            loaded.bundle().skin().appearance(AppearanceRole::WindowTitle, VisualState::Disabled).content,
            crate::NinePatchContent::Image { .. }
        ));
        for state in [
            VisualState::Normal,
            VisualState::Hovered,
            VisualState::Pressed,
            VisualState::Focused,
            VisualState::HoveredFocused,
            VisualState::PressedFocused,
        ] {
            // Passive Windows 3.11 titles are white and require black text, while selected blue
            // titles retain white text for every enabled interaction combination.
            let passive = loaded.bundle().skin().foreground(AppearanceRole::WindowTitle, state);
            let active = loaded.bundle().skin().foreground(AppearanceRole::WindowTitleActive, state);
            assert_eq!((passive.r, passive.g, passive.b, passive.a), (0, 0, 0, 255));
            assert_eq!((active.r, active.g, active.b, active.a), (255, 255, 255, 255));
        }
        let disabled_title = loaded.bundle().skin().foreground(AppearanceRole::WindowTitle, VisualState::Disabled);
        assert_eq!((disabled_title.r, disabled_title.g, disabled_title.b, disabled_title.a), (125, 125, 125, 255));
        let minimize_glyph = loaded.bundle().skin().appearance(AppearanceRole::WindowMinimizeGlyph, VisualState::Normal);
        let crate::NinePatchContent::Image { image: minimize_image } = minimize_glyph.content else {
            panic!("Windows 3.11 minimize glyph must use baked artwork");
        };
        let minimize_size = loaded.bundle().atlas().get_icon_size(minimize_image.icon);
        assert_eq!((minimize_size.width, minimize_size.height), (9, 9));
        // Disabling must preserve the raised caption face authored by the theme. Letting these
        // roles fall back to their flat palette appearance would combine a one-pixel black frame
        // with the role's three-pixel visual insets and produce an incorrect heavy black square.
        for role in [
            AppearanceRole::WindowCloseButton,
            AppearanceRole::WindowMinimizeButton,
            AppearanceRole::WindowMaximizeButton,
            AppearanceRole::WindowRestoreButton,
        ] {
            let normal = loaded.bundle().skin().appearance(role, VisualState::Normal);
            let disabled = loaded.bundle().skin().appearance(role, VisualState::Disabled);
            assert!(matches!(
                (normal.content, disabled.content),
                (crate::NinePatchContent::Image { image: normal }, crate::NinePatchContent::Image { image: disabled })
                    if normal.icon == disabled.icon
            ));
        }
        let selected_text = loaded.bundle().skin().foreground(AppearanceRole::MenuItem, VisualState::Hovered);
        assert_eq!((selected_text.r, selected_text.g, selected_text.b, selected_text.a), (255, 255, 255, 255));
        // Checked menu rows retain their marker without becoming permanently highlighted. The
        // normal patch must therefore remain transparent over the white popup panel, while an
        // actual hover still selects the authored blue bitmap and contrasting white foreground.
        let checked_normal = loaded.bundle().skin().appearance(AppearanceRole::MenuItemSelected, VisualState::Normal);
        assert!(matches!(
            checked_normal.content,
            crate::NinePatchContent::Flat { cells }
                if matches!(cells.center, crate::NinePatchCell::Empty)
        ));
        assert!(matches!(
            loaded
                .bundle()
                .skin()
                .appearance(AppearanceRole::MenuItemSelected, VisualState::Hovered)
                .content,
            crate::NinePatchContent::Image { .. }
        ));
        let checked_normal_text = loaded.bundle().skin().foreground(AppearanceRole::MenuItemSelected, VisualState::Normal);
        let checked_hovered_text = loaded.bundle().skin().foreground(AppearanceRole::MenuItemSelected, VisualState::Hovered);
        assert_eq!(
            (checked_normal_text.r, checked_normal_text.g, checked_normal_text.b, checked_normal_text.a),
            (0, 0, 0, 255)
        );
        assert_eq!(
            (checked_hovered_text.r, checked_hovered_text.g, checked_hovered_text.b, checked_hovered_text.a),
            (255, 255, 255, 255)
        );
        let focus = loaded.bundle().skin().effects.focus_outline;
        assert_eq!(
            (focus.r, focus.g, focus.b, focus.a),
            (0, 0, 0, 255),
            "period control focus must preserve black combo and slider frames"
        );
        let slider_normal = loaded.bundle().skin().appearance(AppearanceRole::SliderTrack, VisualState::Normal);
        let slider_focused = loaded.bundle().skin().appearance(AppearanceRole::SliderTrack, VisualState::Focused);
        assert!(matches!(
            (slider_normal.content, slider_focused.content),
            (crate::NinePatchContent::Image { image: normal }, crate::NinePatchContent::Image { image: focused })
                if normal.icon == focused.icon
        ));
        for state in [
            VisualState::Hovered,
            VisualState::Pressed,
            VisualState::Focused,
            VisualState::HoveredFocused,
            VisualState::PressedFocused,
        ] {
            // Combo popup choices are ordinary retained ListItems. Their complete interactive
            // state ladder must therefore carry both blue selection art and contrasting text.
            assert!(matches!(
                loaded.bundle().skin().appearance(AppearanceRole::ListItem, state).content,
                crate::NinePatchContent::Image { .. }
            ));
            let foreground = loaded.bundle().skin().foreground(AppearanceRole::ListItem, state);
            assert_eq!((foreground.r, foreground.g, foreground.b, foreground.a), (255, 255, 255, 255));
        }
    }

    /// Verifies the bundled Mac theme installs its controls, title strips, frame, and grip artwork.
    #[test]
    fn bundled_mac_os_9_theme_reuses_shared_png_regions() {
        let (loaded, images) = install_bundled_theme("themes/mac-os-9/theme.json");
        assert_eq!(loaded.name(), "Mac OS 9");
        assert_eq!(images, 24, "each shared PNG path must be baked exactly once");
        let chrome = loaded.bundle().skin().chrome;
        assert_eq!(chrome.title_alignment, crate::WindowTitleAlignment::Centered);
        assert_eq!(chrome.captions.close_side, crate::CaptionButtonSide::Leading);
        assert!(!chrome.captions.show_when_inactive);
        assert_eq!(loaded.bundle().skin().metrics.title_height, 18);
        let content = loaded.bundle().skin().metrics.window_content_insets;
        assert_eq!(
            (content.left, content.top, content.right, content.bottom),
            (0, 0, 0, 0),
            "Platinum windows expose their complete application body below root chrome"
        );
        let insets = loaded.bundle().skin().appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (3, 3, 3, 3));
        let active_title = loaded.bundle().skin().appearance(AppearanceRole::WindowTitleActive, VisualState::Pressed);
        let crate::NinePatchContent::Image { image: active_title_image } = active_title.content else {
            panic!("Mac OS 9 active title must use baked artwork");
        };
        let active_title_size = loaded.bundle().atlas().get_icon_size(active_title_image.icon);
        assert_eq!((active_title_size.width, active_title_size.height), (8, 18));
        assert!(matches!(
            loaded.bundle().skin().appearance(AppearanceRole::Button, VisualState::Disabled).content,
            crate::NinePatchContent::Image { .. }
        ));
        assert!(matches!(
            loaded
                .bundle()
                .skin()
                .appearance(AppearanceRole::DialogFrameActive, VisualState::Normal)
                .content,
            crate::NinePatchContent::Image { .. }
        ));

        // Hovering or pressing a passive frame must not borrow the darker active-frame bitmap.
        // This guards the Platinum distinction before the manager supplies the active role.
        let normal_frame = loaded.bundle().skin().appearance(AppearanceRole::WindowFrame, VisualState::Normal);
        let pressed_frame = loaded.bundle().skin().appearance(AppearanceRole::WindowFrame, VisualState::Pressed);
        assert!(matches!(
            (normal_frame.content, pressed_frame.content),
            (crate::NinePatchContent::Image { image: normal }, crate::NinePatchContent::Image { image: pressed })
                if normal.icon == pressed.icon
        ));

        // The Mac layout consumes complete caption-face images and does not overlay generic
        // Windows glyphs. Pressed artwork remains a distinct upload for visible inset feedback.
        let close = loaded.bundle().skin().appearance(AppearanceRole::WindowCloseButton, VisualState::Normal);
        let close_pressed = loaded.bundle().skin().appearance(AppearanceRole::WindowCloseButton, VisualState::Pressed);
        let (crate::NinePatchContent::Image { image: close_image }, crate::NinePatchContent::Image { image: pressed_image }) =
            (close.content, close_pressed.content)
        else {
            panic!("Mac OS 9 close states must use baked artwork");
        };
        let close_size = loaded.bundle().atlas().get_icon_size(close_image.icon);
        assert_eq!((close_size.width, close_size.height), (12, 12));
        assert_ne!(close_image.icon, pressed_image.icon);

        // Platinum popup selection uses black image-backed rows with white foreground text, while
        // the popup itself retains its authored thick black and beveled frame.
        let menu_popup = loaded.bundle().skin().appearance(AppearanceRole::MenuPopup, VisualState::Normal);
        let crate::NinePatchContent::Image { image: menu_popup_image } = menu_popup.content else {
            panic!("Mac OS 9 popup must use baked artwork");
        };
        let menu_popup_size = loaded.bundle().atlas().get_icon_size(menu_popup_image.icon);
        assert_eq!((menu_popup_size.width, menu_popup_size.height), (7, 7));
        let selected_text = loaded.bundle().skin().foreground(AppearanceRole::MenuItem, VisualState::Hovered);
        assert_eq!((selected_text.r, selected_text.g, selected_text.b, selected_text.a), (255, 255, 255, 255));
    }
}
