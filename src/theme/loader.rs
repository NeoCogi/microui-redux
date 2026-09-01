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
    collections::BTreeMap,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::{AppearanceCatalog, AppearanceRole, Style, VisualState};
use crate::{Color, ControlColor, ImageError, ImageSource, NinePatch, NinePatchImage, SliceInsets, TextureError, TextureId};

/// Theme-file schema version understood by this crate release.
pub const THEME_SCHEMA_VERSION: u32 = 1;

/// A context-installed theme with its display name and complete resolved style.
///
/// PNG textures are owned by the Context that loaded the theme and remain alive until that Context
/// is dropped. The value therefore stays cheap to clone and contains no backend or dynamic owner.
#[derive(Clone)]
pub struct LoadedTheme {
    /// Human-readable name supplied by the JSON definition.
    name: String,
    /// Atlas- and Context-bound style containing resolved image texture handles.
    style: Style,
}

impl LoadedTheme {
    /// Creates a loaded theme after schema resolution and texture upload have succeeded.
    pub(crate) fn new(name: String, style: Style) -> Self {
        // Both values are complete at this boundary; partially installed themes never become public.
        Self { name, style }
    }

    /// Returns the human-readable theme name from the JSON file.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Borrows the complete resolved style without transferring texture handles.
    pub fn style(&self) -> &Style {
        &self.style
    }

    /// Consumes this wrapper and returns its complete resolved style.
    pub fn into_style(self) -> Style {
        self.style
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
    /// The Context backend rejected a decoded image upload.
    ImageUpload {
        /// Fully resolved image path.
        path: PathBuf,
        /// Concrete texture-layer diagnostic.
        source: TextureError,
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
            Self::ImageUpload { path, source } => write!(formatter, "failed to upload theme PNG {}: {source}", path.display()),
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
            Self::ImageUpload { source, .. } => Some(source),
            Self::UnsupportedSchema { .. } | Self::EmptyName | Self::UnknownAppearance { .. } | Self::InvalidInsets { .. } => None,
        }
    }
}

/// Strict deserialized root document retained until Context-owned installation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThemeDefinition {
    /// Version selecting the exact schema contract.
    schema_version: u32,
    /// Human-readable selector label.
    name: String,
    /// Optional scalar and flat-color overrides.
    #[serde(default)]
    style: StyleDocument,
    /// Role-keyed optional PNG state descriptions.
    #[serde(default)]
    appearances: BTreeMap<String, AppearanceDocument>,
    /// Directory used to resolve every relative PNG path.
    #[serde(skip)]
    directory: PathBuf,
}

impl ThemeDefinition {
    /// Reads and strictly deserializes one JSON theme definition.
    pub(crate) fn read(path: &Path) -> Result<Self, ThemeLoadError> {
        // Retain the exact caller path in diagnostics and resolve assets against its parent.
        let bytes = fs::read(path).map_err(|source| ThemeLoadError::DefinitionRead { path: path.to_path_buf(), source })?;
        let mut definition: Self =
            serde_json::from_slice(bytes.as_slice()).map_err(|source| ThemeLoadError::DefinitionJson { path: path.to_path_buf(), source })?;
        if definition.schema_version != THEME_SCHEMA_VERSION {
            return Err(ThemeLoadError::UnsupportedSchema {
                expected: THEME_SCHEMA_VERSION,
                actual: definition.schema_version,
            });
        }
        if definition.name.trim().is_empty() {
            return Err(ThemeLoadError::EmptyName);
        }
        definition.directory = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        Ok(definition)
    }

    /// Resolves flat fallbacks and uploads every explicitly supplied PNG through `upload`.
    pub(crate) fn install(
        self,
        mut style: Style,
        mut upload: impl FnMut(&Path, i32, i32, &[u8]) -> Result<TextureId, TextureError>,
    ) -> Result<LoadedTheme, ThemeLoadError> {
        // Apply palette and metric overrides before constructing fallbacks, so every omitted PNG
        // state reflects the JSON theme's own flat colors rather than the built-in default palette.
        self.style.apply(&mut style);
        let frame_insets = self.style.frame_insets.map(InsetsDocument::into_insets).unwrap_or_else(|| style.frame_insets());
        validate_non_negative("generic_frame", "style.frame_insets", frame_insets)?;
        style.appearances =
            AppearanceCatalog::from_flat_palette(frame_insets, style.colors, style.focus_color, style.window_focus_color, style.menu_background);
        // A theme commonly reuses one small bevel PNG across several semantic roles and states.
        // Cache the Context-owned upload by its fully resolved path while retaining source slices,
        // destination slices, and tint on each independently constructed NinePatchImage.
        let mut uploaded_images = BTreeMap::<PathBuf, (i32, i32, TextureId)>::new();

        for (name, document) in self.appearances {
            let role = AppearanceRole::from_json_name(name.as_str()).ok_or_else(|| ThemeLoadError::UnknownAppearance { name: name.clone() })?;
            let mut appearance = style.appearances.get(role);
            let destination_insets = document
                .insets
                .map(InsetsDocument::into_insets)
                .unwrap_or_else(|| appearance.get(VisualState::Normal).insets);
            validate_non_negative(name.as_str(), "insets", destination_insets)?;

            // Role-level insets also apply to states that intentionally omit a PNG and retain their
            // flat fallback, keeping layout stable across hover/focus transitions.
            for state in VisualState::ALL {
                appearance.set(state, appearance.get(state).with_insets(destination_insets));
            }
            for (state, state_document) in document.states() {
                let Some(state_document) = state_document else {
                    continue;
                };
                let Some(relative_path) = state_document.png.as_ref() else {
                    continue;
                };
                let path = self.directory.join(relative_path);
                let source_insets = state_document.source_insets.map(InsetsDocument::into_insets).unwrap_or(destination_insets);
                let (width, height, texture) = if let Some(&(width, height, texture)) = uploaded_images.get(path.as_path()) {
                    // Revalidate state-specific source slices even though the shared pixel payload
                    // has already been decoded and uploaded for another role.
                    validate_source_insets(name.as_str(), source_insets, width, height)?;
                    (width, height, texture)
                } else {
                    let bytes = fs::read(path.as_path()).map_err(|source| ThemeLoadError::ImageRead { path: path.clone(), source })?;
                    let (width, height, colors) = crate::load_image_bytes(ImageSource::Png { bytes: bytes.as_slice() })
                        .map_err(|source| ThemeLoadError::ImageDecode { path: path.clone(), source })?;
                    // Shared image validation guarantees both dimensions fit i32 and the RGBA byte
                    // count is bounded before this exact normalization copy.
                    let width = width as i32;
                    let height = height as i32;
                    validate_source_insets(name.as_str(), source_insets, width, height)?;
                    let pixels: Vec<u8> = colors.into_iter().flat_map(|color| [color.x, color.y, color.z, color.w]).collect();
                    let texture = upload(path.as_path(), width, height, pixels.as_slice())
                        .map_err(|source| ThemeLoadError::ImageUpload { path: path.clone(), source })?;
                    uploaded_images.insert(path.clone(), (width, height, texture));
                    (width, height, texture)
                };
                let tint = state_document
                    .tint
                    .map(ColorDocument::into_color)
                    .unwrap_or(Color { r: 255, g: 255, b: 255, a: 255 });
                let image = NinePatchImage::new(texture, crate::Recti::new(0, 0, width, height), source_insets, tint);
                appearance.set(state, NinePatch::image(destination_insets, image));
            }
            style.appearances.set(role, appearance);
        }

        Ok(LoadedTheme::new(self.name, style))
    }
}

/// Optional style metrics and colors applied before appearance fallback construction.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StyleDocument {
    /// Default layout cell width override.
    default_cell_width: Option<i32>,
    /// General widget inner padding override.
    padding: Option<i32>,
    /// Layout spacing override.
    spacing: Option<i32>,
    /// Nested-content indentation override.
    indent: Option<i32>,
    /// Window title height override.
    title_height: Option<i32>,
    /// Scrollbar cross-axis thickness override.
    scrollbar_size: Option<i32>,
    /// Minimum slider and scrollbar thumb size override.
    thumb_size: Option<i32>,
    /// Structural fallback frame insets.
    frame_insets: Option<InsetsDocument>,
    /// Optional semantic flat palette overrides.
    colors: ColorPaletteDocument,
}

impl StyleDocument {
    /// Applies every present scalar and color field to one atlas-bound Style.
    fn apply(&self, style: &mut Style) {
        // Keep assignment explicit so the strict JSON schema and public Style fields cannot drift
        // through reflection or stringly typed mutation.
        assign_if_some(&mut style.default_cell_width, self.default_cell_width);
        assign_if_some(&mut style.padding, self.padding);
        assign_if_some(&mut style.spacing, self.spacing);
        assign_if_some(&mut style.indent, self.indent);
        assign_if_some(&mut style.title_height, self.title_height);
        assign_if_some(&mut style.scrollbar_size, self.scrollbar_size);
        assign_if_some(&mut style.thumb_size, self.thumb_size);
        self.colors.apply(style);
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
    /// Inactive window title background.
    title_background: Option<ColorDocument>,
    /// Window title text.
    title_text: Option<ColorDocument>,
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
    /// Applies every supplied semantic color to its concrete Style destination.
    fn apply(&self, style: &mut Style) {
        // Existing ControlColor storage remains public, so map every named schema field explicitly.
        assign_color(&mut style.colors[ControlColor::Text as usize], self.text);
        assign_color(&mut style.colors[ControlColor::Border as usize], self.border);
        assign_color(&mut style.colors[ControlColor::WindowBG as usize], self.window_background);
        assign_color(&mut style.colors[ControlColor::TitleBG as usize], self.title_background);
        assign_color(&mut style.colors[ControlColor::TitleText as usize], self.title_text);
        assign_color(&mut style.colors[ControlColor::PanelBG as usize], self.panel_background);
        assign_color(&mut style.colors[ControlColor::Button as usize], self.button);
        assign_color(&mut style.colors[ControlColor::ButtonHover as usize], self.button_hover);
        assign_color(&mut style.colors[ControlColor::Base as usize], self.input);
        assign_color(&mut style.colors[ControlColor::BaseHover as usize], self.input_hover);
        assign_color(&mut style.colors[ControlColor::ScrollBase as usize], self.scrollbar_track);
        assign_color(&mut style.colors[ControlColor::ScrollThumb as usize], self.scrollbar_thumb);
        assign_color(&mut style.focus_color, self.focus);
        assign_color(&mut style.window_focus_color, self.window_focus);
        assign_color(&mut style.menu_foreground, self.menu_foreground);
        assign_color(&mut style.menu_background, self.menu_background);
    }
}

/// One role's destination insets and optional per-state PNG definitions.
#[derive(Default, Deserialize)]
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
}

/// Optional image supplied for one exact role state.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StateDocument {
    /// PNG path relative to the containing JSON file; absence keeps the flat fallback.
    png: Option<PathBuf>,
    /// Source-space PNG slices; defaults to the role's destination insets.
    source_insets: Option<InsetsDocument>,
    /// Optional RGBA modulation; defaults to opaque white.
    tint: Option<ColorDocument>,
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

/// Assigns one optional copy value without hiding field-to-field schema mapping.
fn assign_if_some<T: Copy>(destination: &mut T, value: Option<T>) {
    // Omitted JSON fields deliberately preserve the atlas-derived base Style value.
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
    fn install_bundled_theme(relative_path: &str) -> (LoadedTheme, u32) {
        // Resolve from the Cargo manifest so tests remain independent of the process working dir.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
        let definition = ThemeDefinition::read(path.as_path()).expect("bundled theme JSON must remain valid");
        let mut uploads = 0_u32;
        let loaded = definition
            .install(Style::from_atlas(&test_atlas()), |_, width, height, pixels| {
                // The loader must present one complete validated RGBA payload for each unique path.
                uploads += 1;
                assert_eq!(pixels.len(), width as usize * height as usize * 4);
                Ok(TextureId::new_test(uploads, width, height))
            })
            .expect("bundled theme PNGs must decode and install");
        (loaded, uploads)
    }

    /// Verifies missing state PNGs remain state-specific flat fallbacks after palette replacement.
    #[test]
    fn omitted_png_states_use_theme_flat_colors() {
        let document: ThemeDefinition = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "name": "Flat only",
                "style": {
                    "colors": {
                        "button": [1, 2, 3, 255],
                        "button_hover": [4, 5, 6, 255]
                    }
                },
                "appearances": {
                    "button": { "insets": { "left": 2, "top": 2, "right": 2, "bottom": 2 } }
                }
            }"#,
        )
        .expect("test definition must match the strict schema");
        let style = Style::from_atlas(&test_atlas());
        let loaded = document
            .install(style, |_, _, _, _| panic!("flat theme must not upload a texture"))
            .expect("flat-only theme must install");

        let normal = loaded.style().appearance(AppearanceRole::Button, VisualState::Normal);
        let hovered = loaded.style().appearance(AppearanceRole::Button, VisualState::Hovered);
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
    }

    /// Verifies the strict schema rejects misspelled fields instead of silently ignoring them.
    #[test]
    fn unknown_json_fields_are_rejected() {
        let error = match serde_json::from_str::<ThemeDefinition>(r#"{ "schema_version": 1, "name": "Broken", "appearences": {} }"#) {
            Ok(_) => panic!("misspelled root field must be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("unknown field `appearences`"));
    }

    /// Verifies the bundled Windows theme and every original PNG install through the public schema.
    #[test]
    fn bundled_windows_95_theme_reuses_shared_png_uploads() {
        let (loaded, uploads) = install_bundled_theme("themes/windows-95/theme.json");
        assert_eq!(loaded.name(), "Windows 95");
        assert_eq!(uploads, 11, "each shared PNG path must be uploaded exactly once");
        let insets = loaded.style().appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (4, 4, 4, 4));
    }

    /// Verifies the bundled Mac theme installs its controls, title strips, frame, and grip artwork.
    #[test]
    fn bundled_mac_os_9_theme_reuses_shared_png_uploads() {
        let (loaded, uploads) = install_bundled_theme("themes/mac-os-9/theme.json");
        assert_eq!(loaded.name(), "Mac OS 9");
        assert_eq!(uploads, 13, "each shared PNG path must be uploaded exactly once");
        let insets = loaded.style().appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets;
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (3, 3, 3, 3));
        assert!(matches!(
            loaded.style().appearance(AppearanceRole::WindowTitleActive, VisualState::Pressed).content,
            crate::NinePatchContent::Image { .. }
        ));
    }
}
