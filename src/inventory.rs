use anyhow::{Context, Result};
use cap_fs_ext::{
    DirExt, FollowSymlinks, OpenOptions as CapabilityOpenOptions, OpenOptionsFollowExt,
};
use cap_std::{ambient_authority, fs::Dir as CapabilityDir};
use image::{ImageDecoder, ImageReader};
use quick_xml::{events::Event, Reader};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    time::UNIX_EPOCH,
};

pub const INVENTORY_SCHEMA_VERSION: u32 = 1;
const MAX_MSE_ZIP_HASH_BYTES_PER_SCAN: u64 = 512 * 1024 * 1024;
const MAX_NOTICE_EXCERPT_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MseResourceRecord {
    /// Chemin relatif au package MSE, dossier ou archive ZIP.
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub hash_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: String,
    pub relative_path: String,
    pub file_name: String,
    pub extension: String,
    pub size_bytes: u64,
    pub modified_unix_seconds: Option<u64>,
    pub sha256: String,
    pub detected_family: String,
    #[serde(default)]
    pub raster_format: Option<String>,
    #[serde(default)]
    pub raster_width: Option<u32>,
    #[serde(default)]
    pub raster_height: Option<u32>,
    #[serde(default)]
    pub raster_color_type: Option<String>,
    #[serde(default)]
    pub raster_has_alpha: Option<bool>,
    #[serde(default)]
    pub raster_orientation: Option<String>,
    #[serde(default)]
    pub svg_viewbox: Option<String>,
    #[serde(default)]
    pub svg_width: Option<String>,
    #[serde(default)]
    pub svg_height: Option<String>,
    #[serde(default)]
    pub svg_has_external_refs: Option<bool>,
    #[serde(default)]
    pub svg_xml_valid: Option<bool>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_subfamily: Option<String>,
    #[serde(default)]
    pub archive_entry_count: Option<usize>,
    #[serde(default)]
    pub archive_uncompressed_bytes: Option<u64>,
    #[serde(default)]
    pub archive_suspicious_paths: Option<usize>,
    #[serde(default)]
    pub mse_package_type: Option<String>,
    #[serde(default)]
    pub mse_main_file_present: Option<bool>,
    #[serde(default)]
    pub mse_version: Option<String>,
    #[serde(default)]
    pub mse_short_name: Option<String>,
    #[serde(default)]
    pub mse_full_name: Option<String>,
    #[serde(default)]
    pub mse_package_version: Option<String>,
    #[serde(default)]
    pub mse_game: Option<String>,
    #[serde(default)]
    pub mse_name: Option<String>,
    #[serde(default)]
    pub mse_game_resolution_status: Option<String>,
    #[serde(default)]
    pub mse_game_resolved_path: Option<String>,
    #[serde(default)]
    pub mse_stylesheet: Option<String>,
    #[serde(default)]
    pub mse_stylesheet_resolution_status: Option<String>,
    #[serde(default)]
    pub mse_stylesheet_resolved_path: Option<String>,
    #[serde(default)]
    pub mse_has_include_directives: Option<bool>,
    #[serde(default)]
    pub mse_dependencies: Vec<MseDependency>,
    #[serde(default)]
    pub mse_resources: Vec<MseResourceRecord>,
    #[serde(default)]
    pub mse_resources_truncated: bool,
    #[serde(default)]
    pub analysis_warning: Option<String>,
    #[serde(default)]
    pub notice_kind: Option<String>,
    #[serde(default)]
    pub notice_excerpt: Option<String>,
    #[serde(default)]
    pub notice_excerpt_truncated: bool,
    #[serde(default)]
    pub duplicate_of: Option<String>,
    pub review_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryDocument {
    pub schema_version: u32,
    pub source_root: String,
    pub generated_unix_seconds: u64,
    pub files_scanned: usize,
    pub total_bytes: u64,
    #[serde(default)]
    pub duplicate_files: usize,
    #[serde(default)]
    pub source_roots: Vec<String>,
    pub assets: Vec<AssetRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangedAsset {
    pub relative_path: String,
    pub previous_sha256: String,
    pub current_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryComparison {
    pub schema_version: u32,
    pub baseline_source_root: String,
    pub current_source_root: String,
    #[serde(default)]
    pub baseline_source_roots: Vec<String>,
    #[serde(default)]
    pub current_source_roots: Vec<String>,
    pub added_paths: Vec<String>,
    pub removed_paths: Vec<String>,
    pub changed_assets: Vec<ChangedAsset>,
    pub unchanged_files: usize,
}

fn open_absolute_directory_nofollow(path: &Path) -> Result<CapabilityDir> {
    if !path.is_absolute() {
        anyhow::bail!(
            "Le chemin de répertoire doit être absolu : {}",
            path.display()
        );
    }

    let mut root = PathBuf::new();
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => root.push(prefix.as_os_str()),
            Component::RootDir => root.push(component.as_os_str()),
            Component::Normal(name) => components.push(name.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                anyhow::bail!("Chemin de répertoire non canonique : {}", path.display())
            }
        }
    }
    if root.as_os_str().is_empty() {
        anyhow::bail!("Racine du chemin absolu introuvable : {}", path.display());
    }

    let mut directory =
        CapabilityDir::open_ambient_dir(&root, ambient_authority()).with_context(|| {
            format!(
                "Ouverture de la racine du chemin impossible : {}",
                root.display()
            )
        })?;
    let mut traversed = root;
    for name in components {
        traversed.push(&name);
        directory = directory.open_dir_nofollow(&name).with_context(|| {
            format!(
                "Ouverture de répertoire sans suivi impossible : {}",
                traversed.display()
            )
        })?;
    }
    Ok(directory)
}

fn open_file_nofollow(workspace: &CapabilityDir, relative: &Path) -> Result<cap_std::fs::File> {
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(name) => Ok(name.to_os_string()),
            _ => anyhow::bail!("Chemin relatif non canonique : {}", relative.display()),
        })
        .collect::<Result<Vec<_>>>()?;
    let (file_name, parent_components) = components
        .split_last()
        .context("Chemin de fichier relatif vide")?;

    let mut parent = workspace.try_clone()?;
    for component in parent_components {
        parent = parent.open_dir_nofollow(component).with_context(|| {
            format!(
                "Ouverture de répertoire sans suivi impossible : {}",
                component.to_string_lossy()
            )
        })?;
    }

    let mut options = CapabilityOpenOptions::new();
    options.read(true);
    options.follow(FollowSymlinks::No);
    let file = parent
        .open_with(Path::new(file_name), &options)
        .with_context(|| {
            format!(
                "Ouverture de fichier sans suivi impossible : {}",
                relative.display()
            )
        })?;
    if !file.metadata()?.file_type().is_file() {
        anyhow::bail!(
            "Source non régulière après ouverture sans suivi : {}",
            relative.display()
        );
    }
    Ok(file)
}

fn open_child_directory_nofollow(
    parent: &CapabilityDir,
    name: &std::ffi::OsStr,
    relative: &Path,
) -> Result<CapabilityDir> {
    parent.open_dir_nofollow(name).with_context(|| {
        format!(
            "Ouverture de sous-répertoire sans suivi impossible : {}",
            relative.display()
        )
    })
}

fn collect_files_from_open_directory(
    directory_handle: &CapabilityDir,
    relative_directory: &Path,
    depth: usize,
    paths: &mut Vec<PathBuf>,
    cancelled: &AtomicBool,
) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("Scan annulé par l'utilisateur.");
    }
    if depth >= 128 {
        return Ok(());
    }
    for entry in directory_handle.read_dir(".").with_context(|| {
        format!(
            "Énumération capability-based impossible : {}",
            relative_directory.display()
        )
    })? {
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("Scan annulé par l'utilisateur.");
        }
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let relative = relative_directory.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            // L'entrée peut être remplacée après file_type(). Ouvrir depuis le
            // handle parent, sans suivi, puis énumérer le handle enfant épinglé.
            let child = open_child_directory_nofollow(directory_handle, &name, &relative)?;
            collect_files_from_open_directory(&child, &relative, depth + 1, paths, cancelled)?;
        } else if file_type.is_file() {
            paths.push(relative);
        }
    }
    Ok(())
}

fn collect_files_capability(
    workspace: &CapabilityDir,
    directory: &Path,
    depth: usize,
    paths: &mut Vec<PathBuf>,
    cancelled: &AtomicBool,
) -> Result<()> {
    let directory_handle = if directory == Path::new(".") {
        workspace.try_clone()?
    } else {
        let mut current = workspace.try_clone()?;
        for component in directory.components() {
            match component {
                Component::Normal(name) => {
                    current = open_child_directory_nofollow(&current, name, directory)?
                }
                _ => anyhow::bail!(
                    "Chemin de répertoire non canonique : {}",
                    directory.display()
                ),
            }
        }
        current
    };
    collect_files_from_open_directory(&directory_handle, directory, depth, paths, cancelled)
}

fn classify_notice(file_name: &str) -> Option<String> {
    let normalized = file_name.to_ascii_uppercase();
    if normalized.starts_with("LICENSE") || normalized.starts_with("LICENCE") {
        Some("license-notice".to_string())
    } else if normalized.starts_with("COPYING") {
        Some("copying-notice".to_string())
    } else if normalized.starts_with("NOTICE") {
        Some("notice-file".to_string())
    } else if normalized.starts_with("README") {
        Some("readme".to_string())
    } else if normalized.starts_with("AUTHORS") || normalized.starts_with("CONTRIBUTORS") {
        Some("contributors".to_string())
    } else if normalized.starts_with("CREDITS") {
        Some("credits".to_string())
    } else {
        None
    }
}

pub fn classify_extension(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" => "raster",
        "svg" => "svg",
        "ttf" | "otf" | "woff" | "woff2" => "font",
        "zip" | "7z" | "rar" => "archive",
        "mse-set" | "mse-game" | "mse-style" => "mse",
        "json" | "xml" | "html" | "htm" | "txt" | "md" | "css" => "text",
        _ => "unknown",
    }
}

#[derive(Default)]
struct RasterMetadata {
    format: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    color_type: Option<String>,
    has_alpha: Option<bool>,
    orientation: Option<String>,
    warning: Option<String>,
}

#[cfg(all(test, unix))]
fn raster_metadata(workspace: &CapabilityDir, relative: &Path, family: &str) -> RasterMetadata {
    if family != "raster" {
        return RasterMetadata::default();
    }
    let mut file = match open_file_nofollow(workspace, relative) {
        Ok(file) => file,
        Err(error) => {
            return RasterMetadata {
                warning: Some(format!("Détection du format image impossible : {}", error)),
                ..RasterMetadata::default()
            }
        }
    };
    raster_metadata_from_file(&mut file, family)
}

fn raster_metadata_from_file(file: &mut cap_std::fs::File, family: &str) -> RasterMetadata {
    if family != "raster" {
        return RasterMetadata::default();
    }
    if let Err(error) = file.seek(SeekFrom::Start(0)) {
        return RasterMetadata {
            warning: Some(format!(
                "Positionnement du lecteur image impossible : {}",
                error
            )),
            ..RasterMetadata::default()
        };
    }
    let reader = match ImageReader::new(BufReader::new(&mut *file)).with_guessed_format() {
        Ok(reader) => reader,
        Err(error) => {
            return RasterMetadata {
                warning: Some(format!("Détection du format image impossible : {}", error)),
                ..RasterMetadata::default()
            }
        }
    };
    let format = reader.format().map(|value| format!("{value:?}"));
    let mut decoder = match reader.into_decoder() {
        Ok(decoder) => decoder,
        Err(error) => {
            return RasterMetadata {
                format,
                warning: Some(format!("En-tête image illisible : {}", error)),
                ..RasterMetadata::default()
            }
        }
    };
    let (orientation, warning) = match decoder.orientation() {
        Ok(value) => (Some(format!("{value:?}")), None),
        Err(error) => (
            None,
            Some(format!("Orientation image illisible : {}", error)),
        ),
    };
    let (width, height) = decoder.dimensions();
    let color_type = decoder.color_type();
    RasterMetadata {
        format,
        width: Some(width),
        height: Some(height),
        color_type: Some(format!("{color_type:?}")),
        has_alpha: Some(color_type.has_alpha()),
        orientation,
        warning,
    }
}

fn read_limited<R: Read>(reader: R, maximum_bytes: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .context("Lecture du fichier limitée impossible")?;
    if bytes.len() as u64 > maximum_bytes {
        anyhow::bail!(
            "Fichier supérieur à la limite de lecture de {} octets.",
            maximum_bytes
        );
    }
    Ok(bytes)
}

fn read_prefix<R: Read>(reader: R, maximum_bytes: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(maximum_bytes)
        .read_to_end(&mut bytes)
        .context("Lecture du préfixe limitée impossible")?;
    Ok(bytes)
}

type SvgMetadata = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<bool>,
    Option<bool>,
    Option<String>,
);
fn parse_svg_dimension(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return None;
    }
    let split = value
        .find(|character: char| {
            !(character.is_ascii_digit() || matches!(character, '+' | '-' | '.'))
        })
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let amount = number.parse::<f64>().ok()?;
    if !amount.is_finite() || amount <= 0.0 || amount > 1_000_000_000.0 {
        return None;
    }
    let unit = unit.to_ascii_lowercase();
    if !matches!(
        unit.as_str(),
        "" | "px"
            | "in"
            | "cm"
            | "mm"
            | "pt"
            | "pc"
            | "em"
            | "ex"
            | "ch"
            | "rem"
            | "%"
            | "vw"
            | "vh"
            | "vmin"
            | "vmax"
    ) {
        return None;
    }
    Some(value.to_string())
}
#[cfg(test)]
fn svg_metadata(workspace: &CapabilityDir, relative: &Path, family: &str) -> SvgMetadata {
    if family != "svg" {
        return (None, None, None, None, None, None);
    }
    let mut file = match open_file_nofollow(workspace, relative) {
        Ok(file) => file,
        Err(error) => {
            return (
                None,
                None,
                None,
                None,
                Some(false),
                Some(format!("Ouverture SVG impossible : {}", error)),
            )
        }
    };
    svg_metadata_from_file(&mut file, family)
}

fn svg_metadata_from_file(file: &mut cap_std::fs::File, family: &str) -> SvgMetadata {
    if family != "svg" {
        return (None, None, None, None, None, None);
    }
    const MAX_SVG_BYTES: u64 = 32 * 1024 * 1024;
    if let Err(error) = file.seek(SeekFrom::Start(0)) {
        return (
            None,
            None,
            None,
            None,
            None,
            Some(format!(
                "Positionnement du lecteur SVG impossible : {}",
                error
            )),
        );
    }
    let bytes = match read_limited(&mut *file, MAX_SVG_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                None,
                None,
                None,
                None,
                None,
                Some(format!(
                    "Lecture SVG impossible ou supérieure à 32 Mio : {}",
                    error
                )),
            )
        }
    };
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => {
            return (
                None,
                None,
                None,
                None,
                Some(false),
                Some(format!("SVG non UTF-8 : {}", error)),
            )
        }
    };
    let mut reader = Reader::from_str(&source);
    reader.config_mut().trim_text(true);
    let mut viewbox = None;
    let mut width = None;
    let mut height = None;
    let mut external_refs = false;
    let mut found_svg_root = false;
    let mut root_element_seen = false;
    let mut event_count = 0_usize;
    let mut depth = 0_usize;
    loop {
        let event = reader.read_event();
        event_count = event_count.saturating_add(1);
        if event_count > 100_000 {
            return (
                viewbox,
                width,
                height,
                Some(external_refs),
                Some(false),
                Some("SVG refusé : nombre maximal d'événements XML dépassé.".to_string()),
            );
        }
        if matches!(&event, Ok(Event::Start(_))) {
            depth = depth.saturating_add(1);
            if depth > 256 {
                return (
                    viewbox,
                    width,
                    height,
                    Some(external_refs),
                    Some(false),
                    Some("SVG refusé : profondeur XML maximale dépassée.".to_string()),
                );
            }
        }
        let is_root_element = match &event {
            Ok(Event::Start(_)) => depth == 1,
            Ok(Event::Empty(_)) => depth == 0,
            _ => false,
        };
        if is_root_element {
            if root_element_seen {
                return (
                    viewbox,
                    width,
                    height,
                    Some(external_refs),
                    Some(false),
                    Some("SVG refusé : plusieurs éléments racine XML.".to_string()),
                );
            }
            root_element_seen = true;
        }
        match event {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if is_root_element && element.name().as_ref() == b"svg" {
                    found_svg_root = true;
                }
                let mut attribute_count = 0_usize;
                for attribute in element.attributes() {
                    attribute_count = attribute_count.saturating_add(1);
                    if attribute_count > 256 {
                        return (
                            viewbox,
                            width,
                            height,
                            Some(external_refs),
                            Some(false),
                            Some(
                                "SVG refusé : nombre maximal d'attributs par élément dépassé."
                                    .to_string(),
                            ),
                        );
                    }
                    let attribute = match attribute {
                        Ok(attribute) => attribute,
                        Err(error) => {
                            return (
                                viewbox,
                                width,
                                height,
                                Some(external_refs),
                                Some(false),
                                Some(format!("Attribut SVG invalide : {}", error)),
                            )
                        }
                    };
                    let key = attribute.key.as_ref();
                    let value = match attribute.decode_and_unescape_value(reader.decoder()) {
                        Ok(value) => value.into_owned(),
                        Err(error) => {
                            return (
                                viewbox,
                                width,
                                height,
                                Some(external_refs),
                                Some(false),
                                Some(format!("Valeur d'attribut SVG invalide : {}", error)),
                            )
                        }
                    };
                    if is_root_element
                        && element.name().as_ref() == b"svg"
                        && key == b"width"
                        && width.is_none()
                    {
                        width = parse_svg_dimension(&value);
                    }
                    if is_root_element
                        && element.name().as_ref() == b"svg"
                        && key == b"height"
                        && height.is_none()
                    {
                        height = parse_svg_dimension(&value);
                    }
                    if is_root_element && key == b"viewBox" && viewbox.is_none() {
                        viewbox = Some(value.clone());
                    }
                    if key.ends_with(b"href") && is_external_reference(&value) {
                        external_refs = true;
                    }
                    if key == b"style" && contains_external_css_reference(&value) {
                        external_refs = true;
                    }
                }
            }
            Ok(Event::End(_)) => {
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return (
                    viewbox,
                    width,
                    height,
                    Some(external_refs),
                    Some(false),
                    Some(format!("XML SVG invalide : {}", error)),
                )
            }
        }
    }
    if !found_svg_root {
        return (
            viewbox,
            width,
            height,
            Some(external_refs),
            Some(false),
            Some("Élément racine SVG absent.".to_string()),
        );
    }
    if contains_external_css_reference(&source) {
        external_refs = true;
    }
    (
        viewbox,
        width,
        height,
        Some(external_refs),
        Some(true),
        None,
    )
}

fn font_metadata_from_file(
    file: &mut cap_std::fs::File,
    family: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    if family != "font" {
        return (None, None, None);
    }
    const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
    let size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            return (
                None,
                None,
                Some(format!("Métadonnées de police indisponibles : {}", error)),
            )
        }
    };
    if size > MAX_FONT_BYTES {
        return (
            None,
            None,
            Some("Police ignorée : taille supérieure à 64 Mio.".to_string()),
        );
    }
    if let Err(error) = file.seek(SeekFrom::Start(0)) {
        return (
            None,
            None,
            Some(format!(
                "Positionnement du lecteur de police impossible : {}",
                error
            )),
        );
    }
    let data = match read_limited(&mut *file, MAX_FONT_BYTES) {
        Ok(data) => data,
        Err(error) => {
            return (
                None,
                None,
                Some(format!(
                    "Lecture de police impossible ou supérieure à 64 Mio : {}",
                    error
                )),
            )
        }
    };
    let face = match ttf_parser::Face::parse(&data, 0) {
        Ok(face) => face,
        Err(error) => {
            return (
                None,
                None,
                Some(format!(
                    "Format de police non pris en charge ou invalide : {}",
                    error
                )),
            )
        }
    };
    let mut font_family = None;
    let mut font_subfamily = None;
    for name in face.names() {
        if name.name_id == 1 && font_family.is_none() {
            font_family = name.to_string();
        } else if name.name_id == 2 && font_subfamily.is_none() {
            font_subfamily = name.to_string();
        }
    }
    (font_family, font_subfamily, None)
}

fn is_external_reference(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && !value.starts_with('#') && !value.to_ascii_lowercase().starts_with("data:")
}

fn contains_external_css_reference(value: &str) -> bool {
    let normalized = value
        .chars()
        .filter(|character| {
            !character.is_ascii_whitespace() && *character != '"' && *character != '\''
        })
        .collect::<String>()
        .to_ascii_lowercase();
    let mut remaining = normalized.as_str();
    while let Some(start) = remaining.find("url(") {
        let after_url = &remaining[start + 4..];
        let Some(end) = after_url.find(')') else {
            break;
        };
        let target = &after_url[..end];
        if !target.is_empty() && !target.starts_with('#') && !target.starts_with("data:") {
            return true;
        }
        remaining = &after_url[end + 1..];
    }
    normalized.contains("@import")
}

fn mse_package_type(extension: &str) -> Option<&'static str> {
    match extension {
        "mse-set" => Some("set"),
        "mse-game" => Some("game"),
        "mse-style" => Some("style"),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MseDependency {
    pub package: String,
    pub min_version: Option<String>,
    #[serde(default)]
    pub resolution_status: Option<String>,
    #[serde(default)]
    pub resolved_path: Option<String>,
    #[serde(default)]
    pub version_status: Option<String>,
    #[serde(default)]
    pub cycle_detected: Option<bool>,
}

fn finish_mse_dependency(
    dependencies: &mut Vec<MseDependency>,
    package: &mut Option<String>,
    version: &mut Option<String>,
) -> Result<()> {
    if let Some(package) = package.take() {
        if package.len() > 4096 {
            anyhow::bail!("Nom de package MSE supérieur à 4 Kio.");
        }
        if dependencies.len() >= 128 {
            anyhow::bail!("Plus de 128 dépendances MSE déclarées.");
        }
        dependencies.push(MseDependency {
            package,
            min_version: version.take(),
            resolution_status: None,
            resolved_path: None,
            version_status: None,
            cycle_detected: None,
        });
    } else {
        version.take();
    }
    Ok(())
}

#[derive(Default)]
struct MseHeader {
    mse_version: Option<String>,
    name: Option<String>,
    short_name: Option<String>,
    full_name: Option<String>,
    package_version: Option<String>,
    game: Option<String>,
    stylesheet: Option<String>,
    has_include_directives: Option<bool>,
    dependencies: Vec<MseDependency>,
}

fn parse_mse_header(bytes: &[u8]) -> Result<MseHeader> {
    const MAX_MSE_HEADER_LINES: usize = 4096;
    const MAX_MSE_LINE_BYTES: usize = 16 * 1024;
    let source = std::str::from_utf8(bytes).context("En-tête MSE non UTF-8")?;
    let mut header = MseHeader::default();
    let mut has_include_directives = false;
    let mut in_dependency_block = false;
    let mut dependency_package = None;
    let mut dependency_version = None;
    for (line_index, line) in source.lines().enumerate() {
        if line_index >= MAX_MSE_HEADER_LINES {
            anyhow::bail!("En-tête MSE supérieur à {} lignes.", MAX_MSE_HEADER_LINES);
        }
        if line.len() > MAX_MSE_LINE_BYTES {
            anyhow::bail!(
                "Ligne d'en-tête MSE supérieure à {} octets.",
                MAX_MSE_LINE_BYTES
            );
        }
        let unindented = line.trim_start_matches(['\t', ' ']);
        if unindented.starts_with("include file:") {
            has_include_directives = true;
        }
        if line.starts_with(' ') {
            anyhow::bail!(
                "Indentation MSE invalide : les espaces en début de ligne sont interdits."
            );
        }
        let indentation = line
            .chars()
            .take_while(|character| *character == '\t')
            .count();
        if line.starts_with('\t') {
            if line.chars().nth(indentation) == Some(' ') {
                anyhow::bail!("Indentation MSE invalide : mélange de tabulations et d'espaces.");
            }
            if in_dependency_block && indentation == 1 {
                if let Some((key, value)) = unindented.split_once(':') {
                    let value = value.trim();
                    match key.trim().to_ascii_lowercase().as_str() {
                        "package" => {
                            finish_mse_dependency(
                                &mut header.dependencies,
                                &mut dependency_package,
                                &mut dependency_version,
                            )?;
                            if !value.is_empty() {
                                dependency_package = Some(value.to_string());
                            }
                        }
                        "version" if !value.is_empty() => {
                            dependency_version = Some(value.to_string());
                        }
                        _ => {}
                    }
                }
            }
            continue;
        }
        if in_dependency_block {
            finish_mse_dependency(
                &mut header.dependencies,
                &mut dependency_package,
                &mut dependency_version,
            )?;
            in_dependency_block = false;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if key == "depends on" {
            if value.is_empty() {
                in_dependency_block = true;
            } else {
                let mut parts = value.split_whitespace();
                if let Some(package) = parts.next() {
                    if package.len() > 4096 {
                        anyhow::bail!("Nom de package MSE supérieur à 4 Kio.");
                    }
                    if header.dependencies.len() >= 128 {
                        anyhow::bail!("Plus de 128 dépendances MSE déclarées.");
                    }
                    header.dependencies.push(MseDependency {
                        package: package.to_string(),
                        min_version: parts.next().map(str::to_string),
                        resolution_status: None,
                        resolved_path: None,
                        version_status: None,
                        cycle_detected: None,
                    });
                }
            }
            continue;
        }
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "mse version" => header.mse_version = Some(value.to_string()),
            "name" => header.name = Some(value.to_string()),
            "short name" => header.short_name = Some(value.to_string()),
            "full name" => header.full_name = Some(value.to_string()),
            "version" => header.package_version = Some(value.to_string()),
            "game" => header.game = Some(value.to_string()),
            "stylesheet" | "style" => header.stylesheet = Some(value.to_string()),
            _ => {}
        }
    }
    if in_dependency_block {
        finish_mse_dependency(
            &mut header.dependencies,
            &mut dependency_package,
            &mut dependency_version,
        )?;
    }
    header.has_include_directives = Some(has_include_directives);
    Ok(header)
}

#[derive(Default)]
struct ZipMetadata {
    entry_count: Option<usize>,
    uncompressed_bytes: Option<u64>,
    suspicious_paths: Option<usize>,
    warning: Option<String>,
    mse_main_file_present: Option<bool>,
    mse_header: Option<MseHeader>,
}

impl ZipMetadata {
    fn new(
        entry_count: Option<usize>,
        uncompressed_bytes: Option<u64>,
        suspicious_paths: Option<usize>,
        warning: Option<String>,
        mse_main_file_present: Option<bool>,
    ) -> Self {
        Self {
            entry_count,
            uncompressed_bytes,
            suspicious_paths,
            warning,
            mse_main_file_present,
            mse_header: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MseZipResourceLimits {
    max_resources: usize,
    max_resource_bytes: u64,
    max_total_hash_bytes: u64,
}

fn mse_zip_resources_with_limits_and_cancel(
    workspace: &CapabilityDir,
    relative: &Path,
    main_name: &str,
    limits: MseZipResourceLimits,
    scan_budget: &mut u64,
    cancelled: &AtomicBool,
) -> (Vec<MseResourceRecord>, bool) {
    let file = match open_file_nofollow(workspace, relative) {
        Ok(file) => file,
        Err(_) => return (Vec::new(), true),
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(_) => return (Vec::new(), true),
    };
    if archive.len() > 200_000 {
        return (Vec::new(), true);
    }

    let mut resources = Vec::<MseResourceRecord>::new();
    let mut seen_paths = HashMap::<String, usize>::new();
    let mut hashed_bytes = 0_u64;
    let mut truncated = false;
    for index in 0..archive.len() {
        if cancelled.load(Ordering::Relaxed) {
            truncated = true;
            break;
        }
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        let name = entry.name().to_string();
        if name == main_name || entry.is_dir() {
            continue;
        }
        if name.len() > 4096 {
            truncated = true;
            continue;
        }
        if resources.len() >= limits.max_resources {
            truncated = true;
            break;
        }
        let size = entry.size();
        if let Some(previous_index) = seen_paths.get(&name).copied() {
            if let Some(previous) = resources.get_mut(previous_index) {
                previous.sha256 = None;
                previous.hash_status = "ambiguous-path".to_string();
            }
            resources.push(MseResourceRecord {
                relative_path: name,
                size_bytes: size,
                sha256: None,
                hash_status: "ambiguous-path".to_string(),
            });
            truncated = true;
            continue;
        }
        let suspicious_path = entry.enclosed_name().is_none()
            || name.contains('\\')
            || name.contains(':')
            || name.split('/').any(|part| part == "." || part == "..");
        let hash_status = if suspicious_path {
            "suspicious-path"
        } else if size > entry.compressed_size().saturating_mul(1000) && size > 1024 * 1024 {
            "suspicious-ratio"
        } else if size > limits.max_resource_bytes {
            "size-limit"
        } else if hashed_bytes.saturating_add(size) > limits.max_total_hash_bytes {
            "total-budget"
        } else if size > *scan_budget {
            "scan-budget"
        } else {
            let scan_budget_before = *scan_budget;
            let mut hasher = Sha256::new();
            let mut buffer = [0_u8; 64 * 1024];
            let mut bytes_read = 0_u64;
            let mut read_error = false;
            let mut budget_status = None;
            loop {
                if cancelled.load(Ordering::Relaxed) {
                    *scan_budget = (*scan_budget).saturating_sub(bytes_read);
                    return (resources, true);
                }
                // Ne jamais demander au décompresseur un bloc qui dépasse le budget
                // restant : contrôler après Read() autoriserait jusqu'à 64 Kio de
                // traitement supplémentaire à chaque limite.
                let entry_remaining = limits.max_resource_bytes.saturating_sub(bytes_read);
                let package_remaining = limits
                    .max_total_hash_bytes
                    .saturating_sub(hashed_bytes.saturating_add(bytes_read));
                let scan_remaining = scan_budget_before.saturating_sub(bytes_read);
                let remaining = entry_remaining.min(package_remaining).min(scan_remaining);
                if remaining == 0 {
                    if bytes_read == size {
                        break;
                    }
                    budget_status = Some(if entry_remaining == 0 {
                        "size-limit"
                    } else if package_remaining == 0 {
                        "total-budget"
                    } else {
                        "scan-budget"
                    });
                    break;
                }
                // Lire au plus un octet au-delà de la taille déclarée permet de
                // détecter une entrée incohérente sans franchir les budgets.
                let declared_remaining = size.saturating_sub(bytes_read).saturating_add(1);
                let read_limit =
                    remaining.min(declared_remaining).min(buffer.len() as u64) as usize;
                match entry.read(&mut buffer[..read_limit]) {
                    Ok(0) => break,
                    Ok(count) => {
                        bytes_read = bytes_read.saturating_add(count as u64);
                        if bytes_read > size {
                            budget_status = Some("size-mismatch");
                            break;
                        }
                        hasher.update(&buffer[..count]);
                    }
                    Err(_) => {
                        read_error = true;
                        break;
                    }
                }
            }
            *scan_budget = (*scan_budget).saturating_sub(bytes_read);
            if let Some(status) = budget_status {
                status
            } else if read_error {
                "read-error"
            } else if bytes_read != size {
                "size-mismatch"
            } else {
                hashed_bytes = hashed_bytes.saturating_add(bytes_read);
                let record_index = resources.len();
                resources.push(MseResourceRecord {
                    relative_path: name.clone(),
                    size_bytes: size,
                    sha256: Some(format!("{:x}", hasher.finalize())),
                    hash_status: "hashed".to_string(),
                });
                seen_paths.insert(name, record_index);
                continue;
            }
        };
        let record_index = resources.len();
        resources.push(MseResourceRecord {
            relative_path: name.clone(),
            size_bytes: size,
            sha256: None,
            hash_status: hash_status.to_string(),
        });
        seen_paths.insert(name, record_index);
    }
    (resources, truncated)
}

#[cfg(test)]
fn zip_metadata(workspace: &CapabilityDir, relative: &Path, extension: &str) -> ZipMetadata {
    let expected_main = mse_package_type(extension);
    if extension != "zip" && expected_main.is_none() {
        if extension == "7z" || extension == "rar" {
            return ZipMetadata::new(
                None,
                None,
                None,
                Some("Inspection de cette archive non prise en charge pour le moment.".to_string()),
                None,
            );
        }
        return ZipMetadata::default();
    }
    let mut file = match open_file_nofollow(workspace, relative) {
        Ok(file) => file,
        Err(error) => {
            return ZipMetadata::new(
                None,
                None,
                None,
                Some(format!("Ouverture ZIP impossible : {}", error)),
                None,
            )
        }
    };
    zip_metadata_from_file(&mut file, extension)
}

fn zip_metadata_from_file(file: &mut cap_std::fs::File, extension: &str) -> ZipMetadata {
    let expected_main = mse_package_type(extension);
    if extension != "zip" && expected_main.is_none() {
        if extension == "7z" || extension == "rar" {
            return ZipMetadata::new(
                None,
                None,
                None,
                Some("Inspection de cette archive non prise en charge pour le moment.".to_string()),
                None,
            );
        }
        return ZipMetadata::default();
    }
    let archive_size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            return ZipMetadata::new(
                None,
                None,
                None,
                Some(format!("Métadonnées ZIP indisponibles : {}", error)),
                None,
            )
        }
    };
    if archive_size > 512 * 1024 * 1024 {
        return ZipMetadata::new(
            None,
            None,
            None,
            Some("Archive ignorée : fichier ZIP supérieur à 512 Mio.".to_string()),
            None,
        );
    }
    if let Err(error) = file.seek(SeekFrom::Start(0)) {
        return ZipMetadata::new(
            None,
            None,
            None,
            Some(format!(
                "Positionnement du lecteur ZIP impossible : {}",
                error
            )),
            None,
        );
    }
    let mut archive = match zip::ZipArchive::new(&mut *file) {
        Ok(archive) => archive,
        Err(error) => {
            return ZipMetadata::new(
                None,
                None,
                None,
                Some(format!("Structure ZIP invalide : {}", error)),
                None,
            )
        }
    };
    let entry_count = archive.len();
    if entry_count > 200_000 {
        return ZipMetadata::new(
            Some(entry_count),
            None,
            None,
            Some("Archive ignorée : plus de 200 000 entrées.".to_string()),
            None,
        );
    }
    let mut uncompressed_bytes = 0_u64;
    let mut suspicious_paths = 0_usize;
    let mut suspicious_ratio = false;
    let mut mse_main_file_present = expected_main.map(|_| false);
    for index in 0..archive.len() {
        let entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => {
                return ZipMetadata::new(
                    Some(entry_count),
                    None,
                    Some(suspicious_paths),
                    Some(format!("Entrée ZIP illisible : {}", error)),
                    mse_main_file_present,
                )
            }
        };
        let name = entry.name();
        if entry.enclosed_name().is_none()
            || name.contains('\\')
            || name.contains(':')
            || name.split('/').any(|part| part == "." || part == "..")
        {
            suspicious_paths += 1;
        }
        if expected_main.is_some_and(|expected| name == expected) {
            mse_main_file_present = Some(true);
        }
        let size = entry.size();
        let compressed = entry.compressed_size();
        if size > compressed.saturating_mul(1000) && size > 1024 * 1024 {
            suspicious_ratio = true;
        }
        uncompressed_bytes = uncompressed_bytes.saturating_add(size);
        if uncompressed_bytes > 8 * 1024 * 1024 * 1024 {
            return ZipMetadata::new(
                Some(entry_count),
                Some(uncompressed_bytes),
                Some(suspicious_paths),
                Some("Alerte ZIP : taille décompressée déclarée supérieure à 8 Gio.".to_string()),
                mse_main_file_present,
            );
        }
    }
    let mut mse_header = None;
    let mut mse_header_warning = None;
    if mse_main_file_present == Some(true) {
        if let Some(main_name) = expected_main {
            const MAX_MSE_HEADER_BYTES: u64 = 1024 * 1024;
            match archive.by_name(main_name) {
                Ok(entry) => {
                    let truncated = entry.size() > MAX_MSE_HEADER_BYTES;
                    match read_prefix(entry, MAX_MSE_HEADER_BYTES) {
                        Ok(mut bytes) => {
                            if truncated {
                                if let Some(last_newline) =
                                    bytes.iter().rposition(|byte| *byte == b'\n')
                                {
                                    bytes.truncate(last_newline + 1);
                                } else {
                                    bytes.clear();
                                    mse_header_warning = Some(
                                        "Aucune ligne complète dans le préfixe MSE autorisé."
                                            .to_string(),
                                    );
                                }
                            }
                            if !bytes.is_empty() {
                                match parse_mse_header(&bytes) {
                                    Ok(header) => {
                                        if header.mse_version.is_none() {
                                            mse_header_warning = Some(
                                                "Version MSE absente de l'en-tête du package."
                                                    .to_string(),
                                            );
                                        }
                                        mse_header = Some(header);
                                    }
                                    Err(error) => {
                                        mse_header_warning = Some(format!(
                                            "Lecture de l'en-tête MSE impossible : {}",
                                            error
                                        ));
                                    }
                                }
                            }
                            if truncated {
                                let note = "En-tête MSE limité au préfixe de 1 Mio ; les données suivantes ne sont pas analysées.".to_string();
                                mse_header_warning = Some(match mse_header_warning.take() {
                                    Some(existing) => format!("{} {}", existing, note),
                                    None => note,
                                });
                            }
                        }
                        Err(error) => {
                            mse_header_warning =
                                Some(format!("Lecture de l'en-tête MSE impossible : {}", error));
                        }
                    }
                }
                Err(error) => {
                    mse_header_warning =
                        Some(format!("Fichier principal MSE illisible : {}", error));
                }
            }
        }
    }
    let warning = if suspicious_paths > 0 {
        Some(
            "Archive ZIP contenant des chemins suspects ; aucune extraction n'a été effectuée."
                .to_string(),
        )
    } else if suspicious_ratio {
        Some("Archive ZIP avec un ratio de compression inhabituel ; aucune extraction n'a été effectuée.".to_string())
    } else if mse_main_file_present == Some(false) {
        Some(format!(
            "Package MSE incomplet : fichier principal '{}' absent.",
            expected_main.unwrap_or_default()
        ))
    } else {
        None
    };
    let warning = match (warning, mse_header_warning) {
        (Some(existing), Some(mse)) => Some(format!("{} {}", existing, mse)),
        (Some(existing), None) => Some(existing),
        (None, Some(mse)) => Some(mse),
        (None, None) => None,
    };
    let mut metadata = ZipMetadata::new(
        Some(entry_count),
        Some(uncompressed_bytes),
        Some(suspicious_paths),
        warning,
        mse_main_file_present,
    );
    metadata.mse_header = mse_header;
    metadata
}

fn mse_package_identifier(asset: &AssetRecord) -> Option<String> {
    asset.mse_package_type.as_ref()?;
    if asset.mse_main_file_present != Some(true) {
        return None;
    }
    let relative = Path::new(&asset.relative_path);
    if let Some(extension) = mse_folder_package_extension(relative) {
        if mse_package_type(&extension).is_some_and(|main| asset.file_name == main) {
            return relative
                .parent()?
                .file_name()?
                .to_str()
                .map(str::to_ascii_lowercase);
        }
    }
    Some(asset.file_name.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MseVersion {
    Dotted(u64, u64, u64),
    Date(u64, u64, u64),
}

fn parse_mse_version(value: &str) -> Option<MseVersion> {
    let dotted: Vec<&str> = value.split('.').collect();
    if dotted.len() == 3
        && dotted
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Some(MseVersion::Dotted(
            dotted[0].parse().ok()?,
            dotted[1].parse().ok()?,
            dotted[2].parse().ok()?,
        ));
    }
    let date: Vec<&str> = value.split('-').collect();
    if date.len() == 3
        && date[0].len() == 4
        && date[1].len() == 2
        && date[2].len() == 2
        && date
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        let year = date[0].parse().ok()?;
        let month = date[1].parse().ok()?;
        let day = date[2].parse().ok()?;
        let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap_year => 29,
            2 => 28,
            _ => 0,
        };
        if (1..=days_in_month).contains(&day) {
            return Some(MseVersion::Date(year, month, day));
        }
    }
    None
}

fn mse_version_satisfies(actual: &str, minimum: &str) -> Option<bool> {
    let actual = parse_mse_version(actual)?;
    let minimum = parse_mse_version(minimum)?;
    match (actual, minimum) {
        (MseVersion::Dotted(a, b, c), MseVersion::Dotted(x, y, z)) => Some((a, b, c) >= (x, y, z)),
        (MseVersion::Date(a, b, c), MseVersion::Date(x, y, z)) => Some((a, b, c) >= (x, y, z)),
        _ => None,
    }
}

fn attach_mse_zip_resources(
    assets: &mut [AssetRecord],
    workspace: &CapabilityDir,
    scan_budget: &mut u64,
    cancelled: &AtomicBool,
) {
    for asset in assets.iter_mut() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if asset.mse_main_file_present != Some(true) || asset.archive_entry_count.is_none() {
            continue;
        }
        let Some(main_name) = mse_package_type(&asset.extension) else {
            continue;
        };
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            workspace,
            Path::new(&asset.relative_path),
            main_name,
            MseZipResourceLimits {
                max_resources: 10_000,
                max_resource_bytes: 64 * 1024 * 1024,
                max_total_hash_bytes: 256 * 1024 * 1024,
            },
            scan_budget,
            cancelled,
        );
        asset.mse_resources = resources;
        asset.mse_resources_truncated = truncated;
    }
}

fn verify_mse_zip_sources_unchanged(
    assets: &[AssetRecord],
    workspace: &CapabilityDir,
) -> Result<()> {
    for asset in assets {
        if asset.mse_main_file_present != Some(true)
            || asset.archive_entry_count.is_none()
            || mse_package_type(&asset.extension).is_none()
        {
            continue;
        }
        let relative = Path::new(&asset.relative_path);
        let before = workspace.symlink_metadata(relative).with_context(|| {
            format!(
                "Métadonnées ZIP MSE indisponibles : {}",
                asset.relative_path
            )
        })?;
        if !before.file_type().is_file() {
            anyhow::bail!(
                "Archive MSE absente, non régulière ou remplacée par un lien symbolique : {}",
                asset.relative_path
            );
        }
        let current_hash = sha256_file_capability(workspace, relative).with_context(|| {
            format!("Vérification ZIP MSE impossible : {}", asset.relative_path)
        })?;
        let after = workspace.symlink_metadata(relative).with_context(|| {
            format!(
                "Métadonnées ZIP MSE indisponibles après indexation : {}",
                asset.relative_path
            )
        })?;
        if !file_metadata_is_stable(&before, &after) || current_hash != asset.sha256 {
            anyhow::bail!(
                "Archive MSE modifiée pendant ou après son indexation ; résultat invalidé : {}",
                asset.relative_path
            );
        }
    }
    Ok(())
}

fn attach_mse_folder_resources(assets: &mut [AssetRecord]) {
    const MAX_RESOURCES_PER_PACKAGE: usize = 10_000;
    let mut package_indices = HashMap::<String, usize>::new();
    let mut package_main_paths = HashSet::<String>::new();
    for (index, asset) in assets.iter().enumerate() {
        if asset.mse_main_file_present != Some(true) {
            continue;
        }
        let Some(extension) = mse_folder_package_extension(Path::new(&asset.relative_path)) else {
            continue;
        };
        let Some(main_name) = mse_package_type(&extension) else {
            continue;
        };
        if asset.file_name == main_name {
            if let Some(directory) = Path::new(&asset.relative_path).parent() {
                let key = directory.to_string_lossy().replace('\\', "/");
                package_indices.insert(key, index);
                package_main_paths.insert(asset.relative_path.clone());
            }
        }
    }

    let mut resources_by_package = vec![Vec::<MseResourceRecord>::new(); assets.len()];
    let mut truncated_by_package = vec![false; assets.len()];
    for asset in assets.iter() {
        if package_main_paths.contains(&asset.relative_path) {
            continue;
        }
        let relative = Path::new(&asset.relative_path);
        let mut parent = relative.parent();
        while let Some(directory) = parent {
            let key = directory.to_string_lossy().replace('\\', "/");
            if let Some(package_index) = package_indices.get(&key).copied() {
                let package_directory = Path::new(&assets[package_index].relative_path)
                    .parent()
                    .expect("MSE folder main file has a parent directory");
                let package_relative_path = relative
                    .strip_prefix(package_directory)
                    .expect("resource belongs to its package directory")
                    .to_string_lossy()
                    .replace('\\', "/");
                let resources = &mut resources_by_package[package_index];
                if resources.len() < MAX_RESOURCES_PER_PACKAGE {
                    resources.push(MseResourceRecord {
                        relative_path: package_relative_path,
                        size_bytes: asset.size_bytes,
                        sha256: Some(asset.sha256.clone()),
                        hash_status: "hashed".to_string(),
                    });
                } else {
                    truncated_by_package[package_index] = true;
                }
                break;
            }
            parent = directory.parent();
        }
    }

    for (index, asset) in assets.iter_mut().enumerate() {
        if package_main_paths.contains(&asset.relative_path) {
            asset.mse_resources = std::mem::take(&mut resources_by_package[index]);
            asset.mse_resources_truncated = truncated_by_package[index];
        }
    }
}

fn resolve_mse_dependencies(assets: &mut [AssetRecord]) {
    let mut packages = HashMap::<String, Vec<(String, Option<String>)>>::new();
    let mut games = HashMap::<String, Vec<String>>::new();
    let mut stylesheets = HashMap::<(String, String), Vec<String>>::new();
    for asset in assets.iter() {
        if asset.mse_main_file_present == Some(true) {
            if let Some(identifier) = mse_package_identifier(asset) {
                packages.entry(identifier).or_default().push((
                    asset.relative_path.clone(),
                    asset.mse_package_version.clone(),
                ));
            }
            if asset.mse_package_type.as_deref() == Some("game") {
                if let Some(name) = asset.mse_name.as_ref() {
                    games
                        .entry(name.trim().to_lowercase())
                        .or_default()
                        .push(asset.relative_path.clone());
                }
            }
            if asset.mse_package_type.as_deref() == Some("style") {
                if let (Some(game), Some(name)) =
                    (asset.mse_game.as_ref(), asset.mse_short_name.as_ref())
                {
                    stylesheets
                        .entry((game.trim().to_lowercase(), name.trim().to_lowercase()))
                        .or_default()
                        .push(asset.relative_path.clone());
                }
            }
        }
    }
    for asset in assets.iter_mut() {
        asset.mse_game_resolution_status = None;
        asset.mse_game_resolved_path = None;
        asset.mse_stylesheet_resolution_status = None;
        asset.mse_stylesheet_resolved_path = None;
        if let Some(game) = asset.mse_game.as_ref() {
            asset.mse_game_resolution_status = Some(match games.get(&game.trim().to_lowercase()) {
                Some(paths) if paths.len() == 1 => {
                    asset.mse_game_resolved_path = paths.first().cloned();
                    "resolved".to_string()
                }
                Some(_) => "ambiguous".to_string(),
                None => "missing".to_string(),
            });
        }
        if asset.mse_package_type.as_deref() == Some("set") {
            if let (Some(game), Some(stylesheet)) =
                (asset.mse_game.as_ref(), asset.mse_stylesheet.as_ref())
            {
                asset.mse_stylesheet_resolution_status = Some(
                    match stylesheets
                        .get(&(game.trim().to_lowercase(), stylesheet.trim().to_lowercase()))
                    {
                        Some(paths) if paths.len() == 1 => {
                            asset.mse_stylesheet_resolved_path = paths.first().cloned();
                            "resolved".to_string()
                        }
                        Some(_) => "ambiguous".to_string(),
                        None => "missing".to_string(),
                    },
                );
            }
        }
        for dependency in &mut asset.mse_dependencies {
            dependency.resolved_path = None;
            dependency.version_status = None;
            dependency.resolution_status = Some(
                match packages.get(&dependency.package.trim().to_ascii_lowercase()) {
                    Some(candidates) if candidates.len() == 1 => {
                        let (path, actual_version) = &candidates[0];
                        dependency.resolved_path = Some(path.clone());
                        dependency.version_status = Some(
                            match dependency.min_version.as_deref() {
                                None => "not-required",
                                Some(minimum) => match actual_version
                                    .as_deref()
                                    .and_then(|actual| mse_version_satisfies(actual, minimum))
                                {
                                    Some(true) => "satisfied",
                                    Some(false) => "insufficient",
                                    None => "unknown",
                                },
                            }
                            .to_string(),
                        );
                        "resolved".to_string()
                    }
                    Some(_) => "ambiguous".to_string(),
                    None => "missing".to_string(),
                },
            );
        }
    }
    mark_mse_dependency_cycles(assets);
}

fn mark_mse_dependency_cycles(assets: &mut [AssetRecord]) {
    let mut adjacency = HashMap::<String, Vec<String>>::new();
    let mut reverse = HashMap::<String, Vec<String>>::new();
    let mut nodes = HashSet::<String>::new();
    for asset in assets.iter() {
        if asset.mse_main_file_present != Some(true) {
            continue;
        }
        let source = asset.relative_path.clone();
        nodes.insert(source.clone());
        for dependency in &asset.mse_dependencies {
            if dependency.resolution_status.as_deref() != Some("resolved") {
                continue;
            }
            let Some(target) = dependency.resolved_path.as_ref() else {
                continue;
            };
            adjacency
                .entry(source.clone())
                .or_default()
                .push(target.clone());
            reverse
                .entry(target.clone())
                .or_default()
                .push(source.clone());
            nodes.insert(target.clone());
        }
    }

    // Iterative Kosaraju traversal avoids recursion on untrusted package graphs.
    let mut visited = HashSet::<String>::new();
    let mut finish_order = Vec::<String>::new();
    for root in &nodes {
        if !visited.insert(root.clone()) {
            continue;
        }
        let mut stack = vec![(root.clone(), 0usize)];
        while !stack.is_empty() {
            let next = {
                let (node, next_index) = stack.last_mut().expect("non-empty DFS stack");
                let neighbors = adjacency.get(node).map(Vec::as_slice).unwrap_or(&[]);
                if *next_index < neighbors.len() {
                    let neighbor = neighbors[*next_index].clone();
                    *next_index += 1;
                    Some(neighbor)
                } else {
                    None
                }
            };
            if let Some(neighbor) = next {
                if visited.insert(neighbor.clone()) {
                    stack.push((neighbor, 0));
                }
            } else if let Some((finished, _)) = stack.pop() {
                finish_order.push(finished);
            }
        }
    }

    let mut component_by_node = HashMap::<String, usize>::new();
    let mut component_sizes = Vec::<usize>::new();
    for root in finish_order.into_iter().rev() {
        if component_by_node.contains_key(&root) {
            continue;
        }
        let component = component_sizes.len();
        let mut size = 0usize;
        let mut stack = vec![root.clone()];
        component_by_node.insert(root, component);
        while let Some(node) = stack.pop() {
            size += 1;
            if let Some(neighbors) = reverse.get(&node) {
                for neighbor in neighbors {
                    if !component_by_node.contains_key(neighbor) {
                        component_by_node.insert(neighbor.clone(), component);
                        stack.push(neighbor.clone());
                    }
                }
            }
        }
        component_sizes.push(size);
    }

    let mut cyclic_components = HashSet::<usize>::new();
    for (component, size) in component_sizes.iter().enumerate() {
        if *size > 1 {
            cyclic_components.insert(component);
        }
    }
    for (source, targets) in &adjacency {
        if !targets.iter().any(|target| target == source) {
            continue;
        }
        if let Some(component) = component_by_node.get(source) {
            cyclic_components.insert(*component);
        }
    }

    for asset in assets.iter_mut() {
        for dependency in &mut asset.mse_dependencies {
            dependency.cycle_detected = dependency.resolved_path.as_ref().and_then(|target| {
                let source_component = component_by_node.get(&asset.relative_path)?;
                let target_component = component_by_node.get(target)?;
                Some(
                    source_component == target_component
                        && cyclic_components.contains(source_component),
                )
            });
        }
    }
}
fn mse_folder_package_extension(relative: &Path) -> Option<String> {
    let directory_name = relative.parent()?.file_name()?.to_str()?;
    let extension = Path::new(directory_name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    mse_package_type(&extension).map(|_| extension)
}

fn mse_folder_main_metadata_from_file(file: &mut cap_std::fs::File) -> ZipMetadata {
    const MAX_MSE_HEADER_BYTES: u64 = 1024 * 1024;
    let size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            return ZipMetadata::new(
                None,
                None,
                None,
                Some(format!(
                    "Métadonnées du fichier principal MSE indisponibles : {}",
                    error
                )),
                Some(true),
            )
        }
    };
    if let Err(error) = file.seek(SeekFrom::Start(0)) {
        return ZipMetadata::new(
            None,
            None,
            None,
            Some(format!(
                "Positionnement du lecteur MSE impossible : {}",
                error
            )),
            Some(true),
        );
    }
    let mut warning = None;
    let mut header = None;
    match read_prefix(&mut *file, MAX_MSE_HEADER_BYTES) {
        Ok(mut bytes) => {
            let truncated = size > MAX_MSE_HEADER_BYTES;
            if truncated {
                if let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') {
                    bytes.truncate(last_newline + 1);
                } else {
                    bytes.clear();
                    warning =
                        Some("Aucune ligne complète dans le préfixe MSE autorisé.".to_string());
                }
            }
            if !bytes.is_empty() {
                match parse_mse_header(&bytes) {
                    Ok(parsed) => {
                        if parsed.mse_version.is_none() {
                            warning =
                                Some("Version MSE absente de l'en-tête du package.".to_string());
                        }
                        header = Some(parsed);
                    }
                    Err(error) => {
                        warning = Some(format!("Lecture de l'en-tête MSE impossible : {}", error))
                    }
                }
            }
            if truncated {
                let note = "En-tête MSE limité au préfixe de 1 Mio ; les données suivantes ne sont pas analysées.".to_string();
                warning = Some(match warning.take() {
                    Some(existing) => format!("{} {}", existing, note),
                    None => note,
                });
            }
        }
        Err(error) => warning = Some(format!("Lecture de l'en-tête MSE impossible : {}", error)),
    }
    let mut metadata = ZipMetadata::new(None, None, None, warning, Some(true));
    metadata.mse_header = header;
    metadata
}

fn sha256_file_capability_with_prefix(
    workspace: &CapabilityDir,
    relative: &Path,
    prefix_limit: usize,
) -> Result<(String, Vec<u8>)> {
    let mut file = open_file_nofollow(workspace, relative).with_context(|| {
        format!(
            "Ouverture capability-based impossible : {}",
            relative.display()
        )
    })?;
    sha256_reader_with_prefix(&mut file, prefix_limit)
}

fn sha256_reader_with_prefix(
    file: &mut cap_std::fs::File,
    prefix_limit: usize,
) -> Result<(String, Vec<u8>)> {
    file.seek(SeekFrom::Start(0))?;
    let prefix_limit = prefix_limit.min(MAX_NOTICE_EXCERPT_BYTES);
    let mut reader = BufReader::with_capacity(1024 * 1024, &mut *file);
    let mut hasher = Sha256::new();
    let mut prefix = Vec::with_capacity(prefix_limit);
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        let prefix_read = prefix_limit.saturating_sub(prefix.len()).min(read);
        prefix.extend_from_slice(&buffer[..prefix_read]);
    }
    Ok((format!("{:x}", hasher.finalize()), prefix))
}

fn verify_reader_sha256_unchanged(
    file: &mut cap_std::fs::File,
    expected_sha256: &str,
) -> Result<()> {
    let (actual_sha256, _) = sha256_reader_with_prefix(file, 0)?;
    if actual_sha256 != expected_sha256 {
        anyhow::bail!("Contenu du fichier modifié pendant l'analyse (SHA-256 différent).");
    }
    Ok(())
}

fn sha256_file_capability(workspace: &CapabilityDir, relative: &Path) -> Result<String> {
    sha256_file_capability_with_prefix(workspace, relative, 0).map(|(hash, _)| hash)
}

fn notice_excerpt_from_prefix(prefix: &[u8]) -> Option<String> {
    let text = match std::str::from_utf8(prefix) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&prefix[..error.valid_up_to()]).ok()?
        }
        Err(_) => return None,
    };
    let mut excerpt = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
            excerpt.push('�');
        } else {
            excerpt.push(character);
        }
    }
    Some(excerpt)
}

fn file_metadata_is_stable(before: &cap_std::fs::Metadata, after: &cap_std::fs::Metadata) -> bool {
    before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
        && before.file_type().is_file() == after.file_type().is_file()
}

fn analyze_file(
    path: &Path,
    canonical_root: &Path,
    workspace: &CapabilityDir,
) -> Result<AssetRecord> {
    let relative = path
        .strip_prefix(canonical_root)
        .context("Chemin hors du dossier source")?;
    let metadata = workspace.symlink_metadata(relative)?;
    if !metadata.file_type().is_file() {
        anyhow::bail!(
            "Source absente, non régulière ou remplacée par un lien symbolique : {}",
            relative.display()
        );
    }
    let mut source_file = open_file_nofollow(workspace, relative)?;
    let opened_metadata = source_file.metadata()?;
    if !file_metadata_is_stable(&metadata, &opened_metadata) {
        anyhow::bail!(
            "Source remplacée entre son contrôle et son ouverture : {}",
            relative.display()
        );
    }
    let relative_path = relative.to_string_lossy().replace('\\', "/");
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let size_bytes = opened_metadata.len();
    let notice_kind = classify_notice(&file_name);
    let (sha256, notice_prefix) = sha256_reader_with_prefix(
        &mut source_file,
        if notice_kind.is_some() {
            MAX_NOTICE_EXCERPT_BYTES
        } else {
            0
        },
    )?;
    let notice_excerpt = notice_kind
        .as_ref()
        .and_then(|_| notice_excerpt_from_prefix(&notice_prefix));
    let notice_excerpt_truncated = notice_kind.is_some() && size_bytes > notice_prefix.len() as u64;
    let modified_unix_seconds = opened_metadata
        .modified()
        .ok()
        .and_then(|time| time.into_std().duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let detected_family = classify_extension(&extension);
    let raster = raster_metadata_from_file(&mut source_file, detected_family);
    let raster_format = raster.format;
    let raster_width = raster.width;
    let raster_height = raster.height;
    let raster_color_type = raster.color_type;
    let raster_has_alpha = raster.has_alpha;
    let raster_orientation = raster.orientation;
    let (svg_viewbox, svg_width, svg_height, svg_has_external_refs, svg_xml_valid, svg_warning) =
        svg_metadata_from_file(&mut source_file, detected_family);
    let (font_family, font_subfamily, font_warning) =
        font_metadata_from_file(&mut source_file, detected_family);
    let mut package_metadata = zip_metadata_from_file(&mut source_file, &extension);
    let folder_package_extension = mse_folder_package_extension(relative);
    let folder_main_type = folder_package_extension
        .as_deref()
        .and_then(mse_package_type);
    let is_folder_package_main = folder_main_type.is_some_and(|main_name| file_name == main_name);
    if is_folder_package_main {
        package_metadata = mse_folder_main_metadata_from_file(&mut source_file);
    }
    let ZipMetadata {
        entry_count: archive_entry_count,
        uncompressed_bytes: archive_uncompressed_bytes,
        suspicious_paths: archive_suspicious_paths,
        warning: archive_warning,
        mse_main_file_present,
        mse_header,
    } = package_metadata;
    let analysis_warning = raster
        .warning
        .or(svg_warning)
        .or(font_warning)
        .or(archive_warning);
    let mse_package_type = mse_package_type(&extension)
        .or_else(|| {
            if is_folder_package_main {
                folder_package_extension
                    .as_deref()
                    .and_then(mse_package_type)
            } else {
                None
            }
        })
        .map(str::to_string);
    let mse_version = mse_header
        .as_ref()
        .and_then(|header| header.mse_version.clone());
    let mse_short_name = mse_header
        .as_ref()
        .and_then(|header| header.short_name.clone());
    let mse_full_name = mse_header
        .as_ref()
        .and_then(|header| header.full_name.clone());
    let mse_package_version = mse_header
        .as_ref()
        .and_then(|header| header.package_version.clone());
    let mse_game = mse_header.as_ref().and_then(|header| header.game.clone());
    let mse_name = mse_header.as_ref().and_then(|header| header.name.clone());
    let mse_stylesheet = mse_header
        .as_ref()
        .and_then(|header| header.stylesheet.clone());
    let mse_has_include_directives = mse_header
        .as_ref()
        .and_then(|header| header.has_include_directives);
    let mse_dependencies = mse_header
        .as_ref()
        .map_or_else(Vec::new, |header| header.dependencies.clone());

    // Re-hacher le même handle après les analyseurs : une mutation de contenu
    // conservant taille et horodatage ne doit pas valider un inventaire incohérent.
    verify_reader_sha256_unchanged(&mut source_file, &sha256).with_context(|| {
        format!(
            "Vérification finale du contenu impossible : {}",
            relative.display()
        )
    })?;

    // Tous les analyseurs d'un asset réutilisent le même handle ouvert sans suivi.
    // Contrôler le handle et le chemin après les lectures pour détecter les changements
    // persistants observables sans prétendre figer les écritures concurrentes.
    let final_handle_metadata = source_file.metadata()?;
    let final_metadata = workspace.symlink_metadata(relative)?;
    if !file_metadata_is_stable(&opened_metadata, &final_handle_metadata)
        || !file_metadata_is_stable(&opened_metadata, &final_metadata)
    {
        anyhow::bail!(
            "Fichier modifié ou remplacé pendant l'analyse ; résultat invalidé : {}",
            relative.display()
        );
    }

    Ok(AssetRecord {
        id: format!("sha256:{}:{}", sha256, relative_path),
        relative_path,
        file_name,
        extension,
        size_bytes,
        modified_unix_seconds,
        sha256,
        detected_family: detected_family.to_string(),
        raster_format,
        raster_width,
        raster_height,
        raster_color_type,
        raster_has_alpha,
        raster_orientation,
        svg_viewbox,
        svg_width,
        svg_height,
        svg_has_external_refs,
        svg_xml_valid,
        font_family,
        font_subfamily,
        archive_entry_count,
        archive_uncompressed_bytes,
        archive_suspicious_paths,
        mse_package_type,
        mse_main_file_present,
        mse_version,
        mse_short_name,
        mse_full_name,
        mse_package_version,
        mse_game,
        mse_name,
        mse_game_resolution_status: None,
        mse_game_resolved_path: None,
        mse_stylesheet,
        mse_stylesheet_resolution_status: None,
        mse_stylesheet_resolved_path: None,
        mse_has_include_directives,
        mse_dependencies,
        mse_resources: Vec::new(),
        mse_resources_truncated: false,
        analysis_warning,
        notice_kind,
        notice_excerpt,
        notice_excerpt_truncated,
        duplicate_of: None,
        review_status: "needs-review".to_string(),
    })
}

#[cfg(test)]
pub fn scan_directory(root: &Path) -> Result<InventoryDocument> {
    scan_directory_with_progress(root, |_, _| {})
}

#[cfg(test)]
pub fn scan_directory_with_progress<F>(root: &Path, progress: F) -> Result<InventoryDocument>
where
    F: Fn(u64, u64) + Sync,
{
    scan_directory_with_progress_and_cancel(root, progress, &AtomicBool::new(false))
}

pub fn scan_directory_with_progress_and_cancel<F>(
    root: &Path,
    progress: F,
    cancelled: &AtomicBool,
) -> Result<InventoryDocument>
where
    F: Fn(u64, u64) + Sync,
{
    let mut mse_hash_budget = MAX_MSE_ZIP_HASH_BYTES_PER_SCAN;
    scan_directory_with_progress_and_cancel_with_budget(
        root,
        progress,
        cancelled,
        &mut mse_hash_budget,
    )
}

fn scan_directory_with_progress_and_cancel_with_budget<F>(
    root: &Path,
    progress: F,
    cancelled: &AtomicBool,
    mse_hash_budget: &mut u64,
) -> Result<InventoryDocument>
where
    F: Fn(u64, u64) + Sync,
{
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("Dossier source inaccessible : {}", root.display()))?;
    if !canonical_root.is_dir() {
        anyhow::bail!("La source sélectionnée n'est pas un dossier.");
    }

    let workspace = open_absolute_directory_nofollow(&canonical_root)
        .context("Ouverture capability-based sans suivi de la source impossible")?;

    let mut relative_paths = Vec::<PathBuf>::new();
    collect_files_capability(
        &workspace,
        Path::new("."),
        0,
        &mut relative_paths,
        cancelled,
    )?;
    if cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("Scan annulé par l'utilisateur.");
    }
    let total_paths = relative_paths.len();
    let total_bytes = relative_paths
        .iter()
        .filter_map(|relative| workspace.metadata(relative).ok())
        .fold(0_u64, |total, metadata| {
            total.saturating_add(metadata.len())
        });
    let paths: Vec<PathBuf> = relative_paths
        .iter()
        .map(|relative| canonical_root.join(relative))
        .collect();
    let completed = AtomicUsize::new(0);
    let completed_bytes = AtomicU64::new(0);
    let mut assets: Vec<AssetRecord> = paths
        .par_iter()
        .map(|path| {
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("Scan annulé par l'utilisateur.");
            }
            let asset = analyze_file(path, &canonical_root, &workspace)?;
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("Scan annulé par l'utilisateur.");
            }
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let bytes_done = completed_bytes
                .fetch_add(asset.size_bytes, Ordering::Relaxed)
                .saturating_add(asset.size_bytes);
            if done.is_multiple_of(100) || done == total_paths {
                progress(bytes_done, total_bytes);
            }
            Ok(asset)
        })
        .collect::<Result<Vec<_>>>()?;
    assets.sort_by(|a, b| {
        a.relative_path
            .to_lowercase()
            .cmp(&b.relative_path.to_lowercase())
    });
    resolve_mse_dependencies(&mut assets);
    attach_mse_folder_resources(&mut assets);
    attach_mse_zip_resources(&mut assets, &workspace, mse_hash_budget, cancelled);
    verify_mse_zip_sources_unchanged(&assets, &workspace)?;
    if cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("Scan annulé par l'utilisateur.");
    }
    let mut first_path_by_hash = HashMap::<String, String>::new();
    let mut duplicate_files = 0usize;
    for asset in &mut assets {
        if let Some(first_path) = first_path_by_hash.get(&asset.sha256) {
            asset.duplicate_of = Some(first_path.clone());
            duplicate_files += 1;
        } else {
            first_path_by_hash.insert(asset.sha256.clone(), asset.relative_path.clone());
        }
    }
    let total_bytes = assets
        .iter()
        .fold(0_u64, |total, asset| total.saturating_add(asset.size_bytes));
    let generated_unix_seconds = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();
    Ok(InventoryDocument {
        schema_version: INVENTORY_SCHEMA_VERSION,
        source_root: canonical_root.display().to_string(),
        generated_unix_seconds,
        files_scanned: assets.len(),
        total_bytes,
        duplicate_files,
        source_roots: vec![canonical_root.display().to_string()],
        assets,
    })
}

/// Audite plusieurs dossiers en lecture seule. Les chemins sont préfixés par un identifiant
/// stable de source afin d'éviter toute collision entre deux dossiers portant les mêmes noms.
pub fn scan_multiple_directories_with_progress_and_cancel<F>(
    roots: &[PathBuf],
    progress: F,
    cancelled: &AtomicBool,
) -> Result<InventoryDocument>
where
    F: Fn(u64, u64) + Sync,
{
    let mut mse_hash_budget = MAX_MSE_ZIP_HASH_BYTES_PER_SCAN;
    scan_multiple_directories_with_progress_and_cancel_with_budget(
        roots,
        progress,
        cancelled,
        &mut mse_hash_budget,
    )
}

fn scan_multiple_directories_with_progress_and_cancel_with_budget<F>(
    roots: &[PathBuf],
    progress: F,
    cancelled: &AtomicBool,
    mse_hash_budget: &mut u64,
) -> Result<InventoryDocument>
where
    F: Fn(u64, u64) + Sync,
{
    if roots.is_empty() {
        anyhow::bail!("Sélectionne au moins un dossier source.");
    }
    let mut canonical_roots = Vec::with_capacity(roots.len());
    for root in roots {
        let canonical = root
            .canonicalize()
            .with_context(|| format!("Dossier source inaccessible : {}", root.display()))?;
        if !canonical.is_dir() {
            anyhow::bail!(
                "La source sélectionnée n'est pas un dossier : {}",
                canonical.display()
            );
        }
        if canonical_roots.contains(&canonical) {
            anyhow::bail!(
                "Le même dossier source a été sélectionné plusieurs fois : {}",
                canonical.display()
            );
        }
        canonical_roots.push(canonical);
    }
    for (index, root) in canonical_roots.iter().enumerate() {
        for other in canonical_roots.iter().skip(index + 1) {
            if root.starts_with(other) || other.starts_with(root) {
                anyhow::bail!("Les dossiers sources se chevauchent ; sélectionne des dossiers distincts sans relation parent/enfant.");
            }
        }
    }
    let total_bytes = canonical_roots
        .iter()
        .try_fold(0_u64, |total, root| -> Result<u64> {
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("Scan annulé par l'utilisateur.");
            }
            let workspace = open_absolute_directory_nofollow(root)
                .context("Ouverture capability-based sans suivi de la source impossible")?;
            let mut relative_paths = Vec::new();
            collect_files_capability(
                &workspace,
                Path::new("."),
                0,
                &mut relative_paths,
                cancelled,
            )?;
            let size = relative_paths
                .iter()
                .filter_map(|relative| workspace.metadata(relative).ok())
                .fold(0_u64, |sum, metadata| sum.saturating_add(metadata.len()));
            Ok(total.saturating_add(size))
        })?;
    let mut assets = Vec::new();
    let mut completed_bytes = 0_u64;
    let mut source_roots = Vec::with_capacity(canonical_roots.len());
    for (index, root) in canonical_roots.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("Scan annulé par l'utilisateur.");
        }
        source_roots.push(root.display().to_string());
        let document = scan_directory_with_progress_and_cancel_with_budget(
            root,
            |done, _| {
                progress(completed_bytes.saturating_add(done), total_bytes);
            },
            cancelled,
            &mut *mse_hash_budget,
        )?;
        completed_bytes = completed_bytes.saturating_add(document.total_bytes);
        let prefix = format!("source-{:04}", index + 1);
        for mut asset in document.assets {
            asset.relative_path = format!("{}/{}", prefix, asset.relative_path);
            asset.id = format!("sha256:{}:{}", asset.sha256, asset.relative_path);
            asset.duplicate_of = None;
            assets.push(asset);
        }
        progress(completed_bytes, total_bytes);
    }
    assets.sort_by(|a, b| {
        a.relative_path
            .to_lowercase()
            .cmp(&b.relative_path.to_lowercase())
    });
    resolve_mse_dependencies(&mut assets);
    let mut first_path_by_hash = HashMap::<String, String>::new();
    let mut duplicate_files = 0usize;
    for asset in &mut assets {
        if let Some(first_path) = first_path_by_hash.get(&asset.sha256) {
            asset.duplicate_of = Some(first_path.clone());
            duplicate_files += 1;
        } else {
            first_path_by_hash.insert(asset.sha256.clone(), asset.relative_path.clone());
        }
    }
    let total_bytes = assets
        .iter()
        .fold(0_u64, |total, asset| total.saturating_add(asset.size_bytes));
    Ok(InventoryDocument {
        schema_version: INVENTORY_SCHEMA_VERSION,
        source_root: source_roots[0].clone(),
        generated_unix_seconds: std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs(),
        files_scanned: assets.len(),
        total_bytes,
        duplicate_files,
        source_roots,
        assets,
    })
}

pub fn compare_inventories(
    current: &InventoryDocument,
    baseline: &InventoryDocument,
) -> InventoryComparison {
    let baseline_by_path: HashMap<&str, &AssetRecord> = baseline
        .assets
        .iter()
        .map(|asset| (asset.relative_path.as_str(), asset))
        .collect();
    let current_paths: HashMap<&str, &AssetRecord> = current
        .assets
        .iter()
        .map(|asset| (asset.relative_path.as_str(), asset))
        .collect();
    let mut added_paths = Vec::new();
    let mut changed_assets = Vec::new();
    let mut unchanged_files = 0usize;

    for asset in &current.assets {
        match baseline_by_path.get(asset.relative_path.as_str()) {
            None => added_paths.push(asset.relative_path.clone()),
            Some(previous) if previous.sha256 != asset.sha256 => {
                changed_assets.push(ChangedAsset {
                    relative_path: asset.relative_path.clone(),
                    previous_sha256: previous.sha256.clone(),
                    current_sha256: asset.sha256.clone(),
                })
            }
            Some(_) => unchanged_files += 1,
        }
    }

    let mut removed_paths: Vec<String> = baseline
        .assets
        .iter()
        .filter(|asset| !current_paths.contains_key(asset.relative_path.as_str()))
        .map(|asset| asset.relative_path.clone())
        .collect();
    added_paths.sort();
    removed_paths.sort();
    changed_assets.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    InventoryComparison {
        schema_version: 1,
        baseline_source_root: baseline.source_root.clone(),
        current_source_root: current.source_root.clone(),
        baseline_source_roots: if baseline.source_roots.is_empty() {
            vec![baseline.source_root.clone()]
        } else {
            baseline.source_roots.clone()
        },
        current_source_roots: if current.source_roots.is_empty() {
            vec![current.source_root.clone()]
        } else {
            current.source_roots.clone()
        },
        added_paths,
        removed_paths,
        changed_assets,
        unchanged_files,
    }
}

fn with_new_output_file<T>(
    destination: &Path,
    write: impl FnOnce(&mut cap_std::fs::File) -> Result<T>,
) -> Result<T> {
    let parent = destination
        .parent()
        .context("Le chemin d'export n'a pas de dossier parent")?;
    let name = destination
        .file_name()
        .context("Nom de fichier de destination absent")?;
    if name.to_string_lossy().contains(':') {
        anyhow::bail!("Flux de données alternatif Windows refusé dans le nom de destination.");
    }
    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("Dossier de destination inaccessible : {}", parent.display()))?;
    let directory = open_absolute_directory_nofollow(&canonical_parent)
        .context("Ouverture capability sans suivi du dossier de destination impossible")?;
    let mut options = CapabilityOpenOptions::new();
    options.write(true).create_new(true);
    let mut file = directory.open_with(name, &options).with_context(|| {
        format!(
            "Création exclusive impossible (fichier déjà présent ?) : {}",
            destination.display()
        )
    })?;
    let result = (|| {
        let value = write(&mut file)?;
        file.sync_all()
            .context("Synchronisation du fichier exporté impossible")?;
        Ok(value)
    })();
    if result.is_err() {
        drop(file);
        let _ = directory.remove_file(name);
    }
    result
}

pub fn export_comparison_json(comparison: &InventoryComparison, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("Le chemin d'export n'a pas de dossier parent")?;
    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("Dossier de destination inaccessible : {}", parent.display()))?;
    let baseline_roots = if comparison.baseline_source_roots.is_empty() {
        vec![comparison.baseline_source_root.clone()]
    } else {
        comparison.baseline_source_roots.clone()
    };
    let current_roots = if comparison.current_source_roots.is_empty() {
        vec![comparison.current_source_root.clone()]
    } else {
        comparison.current_source_roots.clone()
    };
    let canonical_sources = baseline_roots
        .iter()
        .chain(current_roots.iter())
        .map(|source| Path::new(source).canonicalize())
        .collect::<std::io::Result<Vec<_>>>()?;
    if canonical_sources
        .iter()
        .any(|source| canonical_parent.starts_with(source))
    {
        anyhow::bail!("Export refusé : le rapport de comparaison doit être écrit hors de toutes les sources comparées.");
    }
    if destination.exists() {
        let canonical_destination = destination.canonicalize()?;
        if canonical_sources
            .iter()
            .any(|source| canonical_destination.starts_with(source))
        {
            anyhow::bail!("Export refusé : la destination résout vers une des sources comparées.");
        }
    }
    with_new_output_file(destination, |file| {
        serde_json::to_writer_pretty(file, comparison)
            .context("Écriture du rapport de comparaison impossible")
    })
}

fn ensure_export_outside_source(document: &InventoryDocument, destination: &Path) -> Result<()> {
    let source_roots = if document.source_roots.is_empty() {
        vec![document.source_root.clone()]
    } else {
        document.source_roots.clone()
    };
    let canonical_sources = source_roots
        .iter()
        .map(|source| Path::new(source).canonicalize())
        .collect::<std::io::Result<Vec<_>>>()?;
    let parent = destination
        .parent()
        .context("Le chemin d'export n'a pas de dossier parent")?;
    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("Dossier de destination inaccessible : {}", parent.display()))?;
    if canonical_sources
        .iter()
        .any(|source| canonical_parent.starts_with(source))
    {
        anyhow::bail!("Export refusé : choisis un dossier de sortie situé hors de toutes les sources analysées.");
    }
    if destination.exists() {
        let canonical_destination = destination.canonicalize()?;
        if canonical_sources
            .iter()
            .any(|source| canonical_destination.starts_with(source))
        {
            anyhow::bail!("Export refusé : la destination résout vers une des sources analysées.");
        }
    }
    Ok(())
}

pub fn export_json(document: &InventoryDocument, destination: &Path) -> Result<()> {
    ensure_export_outside_source(document, destination)?;
    with_new_output_file(destination, |file| {
        serde_json::to_writer_pretty(file, document).context("Écriture JSON impossible")
    })
}

pub fn export_csv(document: &InventoryDocument, destination: &Path) -> Result<()> {
    ensure_export_outside_source(document, destination)?;
    with_new_output_file(destination, |file| {
        let mut writer = csv::Writer::from_writer(file);
        for asset in &document.assets {
            writer.serialize(asset)?;
        }
        writer.flush()?;
        Ok(())
    })
}

pub fn export_contact_sheet(
    document: &InventoryDocument,
    page_index: usize,
    destination: &Path,
) -> Result<usize> {
    ensure_export_outside_source(document, destination)?;
    if destination
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("png"))
    {
        anyhow::bail!("La planche de contact doit être exportée au format PNG.");
    }
    if destination.exists() {
        anyhow::bail!(
            "Le fichier de planche existe déjà : {}",
            destination.display()
        );
    }
    let sidecar = destination.with_extension("txt");
    ensure_export_outside_source(document, &sidecar)?;
    if sidecar.exists() {
        anyhow::bail!("Le fichier d'index existe déjà : {}", sidecar.display());
    }
    let raster_assets: Vec<&AssetRecord> = document
        .assets
        .iter()
        .filter(|asset| {
            asset.detected_family == "raster"
                && asset.raster_width.is_some()
                && asset.raster_height.is_some()
        })
        .collect();
    let page_size = 16usize;
    let start = page_index.saturating_mul(page_size);
    if start >= raster_assets.len() {
        anyhow::bail!(
            "Cette page de planche est vide ; {} fichier(s) raster indexé(s).",
            raster_assets.len()
        );
    }
    let selected = &raster_assets[start..raster_assets.len().min(start + page_size)];
    let mut canvas = image::RgbaImage::from_pixel(880, 720, image::Rgba([245, 246, 248, 255]));
    let mut index_text = String::from("Index de la planche de contact\n");
    let source_roots = if document.source_roots.is_empty() {
        vec![document.source_root.clone()]
    } else {
        document.source_roots.clone()
    };
    let mut rendered = 0usize;
    for (slot, asset) in selected.iter().enumerate() {
        let (source_root, relative) = if source_roots.len() > 1 {
            let mut parts = asset.relative_path.splitn(2, '/');
            let prefix = parts.next().unwrap_or_default();
            let relative = parts.next().unwrap_or_default();
            let index = prefix
                .strip_prefix("source-")
                .and_then(|value| value.parse::<usize>().ok())
                .and_then(|value| value.checked_sub(1));
            let root = index
                .and_then(|value| source_roots.get(value))
                .context("Source de la planche introuvable dans l'inventaire multi-dossiers")?;
            (root.as_str(), PathBuf::from(relative))
        } else {
            (
                source_roots[0].as_str(),
                PathBuf::from(&asset.relative_path),
            )
        };
        let workspace = match open_absolute_directory_nofollow(Path::new(source_root)) {
            Ok(workspace) => workspace,
            Err(_) => {
                index_text.push_str(&format!(
                    "{:02}\tSOURCE ABSENTE OU INACCESSIBLE\t{}\n",
                    slot + 1,
                    asset.relative_path
                ));
                continue;
            }
        };
        if workspace
            .symlink_metadata(&relative)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true)
        {
            index_text.push_str(&format!(
                "{:02}\tLIEN SYMBOLIQUE OU SOURCE ABSENTE\t{}\n",
                slot + 1,
                asset.relative_path
            ));
            continue;
        }
        let file_size = workspace
            .metadata(&relative)
            .map(|metadata| metadata.len())
            .unwrap_or(u64::MAX);
        if file_size > 100 * 1024 * 1024 {
            index_text.push_str(&format!(
                "{:02}\tIGNORÉ (fichier > 100 Mio)\t{}\n",
                slot + 1,
                asset.relative_path
            ));
            continue;
        }
        // Lire une seule fois dans un tampon plafonné : le hash, les dimensions et
        // les pixels portent ainsi exactement sur les mêmes octets, même si le chemin
        // source est remplacé après l'ouverture du handle.
        let source_file = match open_file_nofollow(&workspace, &relative) {
            Ok(file) => file,
            Err(_) => {
                index_text.push_str(&format!(
                    "{:02}\tSOURCE ABSENTE OU ILLISIBLE\t{}\n",
                    slot + 1,
                    asset.relative_path
                ));
                continue;
            }
        };
        let bytes = match read_limited(source_file, 100 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(_) => {
                index_text.push_str(&format!(
                    "{:02}\tSOURCE MODIFIÉE OU SUPÉRIEURE À 100 Mio\t{}\n",
                    slot + 1,
                    asset.relative_path
                ));
                continue;
            }
        };
        let current_hash = format!("{:x}", Sha256::digest(&bytes));
        if current_hash != asset.sha256 {
            index_text.push_str(&format!(
                "{:02}\tSOURCE MODIFIÉE DEPUIS LE SCAN\t{}\n",
                slot + 1,
                asset.relative_path
            ));
            continue;
        }
        let dimensions = ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(anyhow::Error::from)
            .and_then(|reader| reader.into_dimensions().map_err(anyhow::Error::from));
        let (width, height) = match dimensions {
            Ok(dimensions) => dimensions,
            Err(_) => {
                index_text.push_str(&format!(
                    "{:02}\tILLISIBLE\t{}\n",
                    slot + 1,
                    asset.relative_path
                ));
                continue;
            }
        };
        if u64::from(width).saturating_mul(u64::from(height)) > 30_000_000 {
            index_text.push_str(&format!(
                "{:02}\tIGNORÉ (plus de 30 Mpx)\t{}\n",
                slot + 1,
                asset.relative_path
            ));
            continue;
        }
        let decoded = (|| -> Result<image::DynamicImage> {
            let reader = ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
            let mut decoder = reader.into_decoder()?;
            let orientation = decoder
                .orientation()
                .unwrap_or(image::metadata::Orientation::NoTransforms);
            let mut image = image::DynamicImage::from_decoder(decoder)?;
            image.apply_orientation(orientation);
            Ok(image)
        })();
        let image = match decoded {
            Ok(image) => image.thumbnail(200, 160).to_rgba8(),
            Err(_) => {
                index_text.push_str(&format!(
                    "{:02}\tDÉCODAGE IMPOSSIBLE\t{}\n",
                    slot + 1,
                    asset.relative_path
                ));
                continue;
            }
        };
        let column = slot % 4;
        let row = slot / 4;
        let x = (column as i64) * 220 + ((200 - image.width()) / 2) as i64 + 10;
        let y = (row as i64) * 180 + ((160 - image.height()) / 2) as i64 + 10;
        image::imageops::overlay(&mut canvas, &image, x, y);
        rendered += 1;
        index_text.push_str(&format!("{:02}\t{}\n", slot + 1, asset.relative_path));
    }
    with_new_output_file(destination, |file| {
        image::DynamicImage::ImageRgba8(canvas)
            .write_to(file, image::ImageFormat::Png)
            .context("Écriture de la planche PNG impossible")?;
        Ok(())
    })?;
    with_new_output_file(&sidecar, |file| {
        file.write_all(index_text.as_bytes())
            .context("Écriture de l'index de planche impossible")?;
        Ok(())
    })?;
    Ok(rendered)
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn append_mse_reference_html(
    html: &mut String,
    label: &str,
    value: &str,
    status: Option<&str>,
    resolved_path: Option<&str>,
) {
    html.push_str("<div><b>");
    html.push_str(label);
    html.push_str(" :</b> ");
    html.push_str(&escape_html(value));
    match status {
        Some("resolved") => {
            if let Some(path) = resolved_path {
                html.push_str(" — présent : <code>");
                html.push_str(&escape_html(path));
                html.push_str("</code>");
            }
        }
        Some("missing") => {
            html.push_str(" — <span class=\"warning\">absent des sources analysées</span>")
        }
        Some("ambiguous") => html.push_str(" — <span class=\"warning\">référence ambiguë</span>"),
        _ => {}
    }
    html.push_str("</div>");
}

fn append_mse_resource_summary_html(html: &mut String, resources: &[MseResourceRecord]) {
    let hashed = resources
        .iter()
        .filter(|resource| resource.sha256.is_some())
        .count();
    let unhashed = resources.len().saturating_sub(hashed);
    html.push_str("<div><b>Empreintes :</b> ");
    html.push_str(&hashed.to_string());
    html.push_str(" calculées ; ");
    html.push_str(&unhashed.to_string());
    html.push_str(" non calculées");
    if unhashed > 0 {
        html.push_str("<ul>");
        for status in [
            "suspicious-path",
            "ambiguous-path",
            "suspicious-ratio",
            "size-limit",
            "total-budget",
            "scan-budget",
            "read-error",
            "size-mismatch",
        ] {
            let count = resources
                .iter()
                .filter(|resource| resource.sha256.is_none() && resource.hash_status == status)
                .count();
            if count > 0 {
                html.push_str("<li>");
                html.push_str(&escape_html(status));
                html.push_str(" : ");
                html.push_str(&count.to_string());
                html.push_str("</li>");
            }
        }
        html.push_str("</ul>");
    }
    html.push_str("</div>");
}

fn append_mse_metadata_html(html: &mut String, asset: &AssetRecord) {
    let Some(package_type) = asset.mse_package_type.as_deref() else {
        return;
    };
    html.push_str("<section><b>Package MSE : ");
    html.push_str(&escape_html(package_type));
    html.push_str("</b>");
    if let Some(present) = asset.mse_main_file_present {
        html.push_str(if present {
            " — fichier principal présent"
        } else {
            " — <span class=\"warning\">fichier principal absent</span>"
        });
    }
    if let Some(version) = &asset.mse_version {
        html.push_str("<div>Version MSE : ");
        html.push_str(&escape_html(version));
        html.push_str("</div>");
    }
    if let Some(name) = &asset.mse_short_name {
        html.push_str("<div>Nom : ");
        html.push_str(&escape_html(name));
        html.push_str("</div>");
    }
    if let Some(name) = &asset.mse_full_name {
        html.push_str("<div>Nom complet : ");
        html.push_str(&escape_html(name));
        html.push_str("</div>");
    }
    if let Some(version) = &asset.mse_package_version {
        html.push_str("<div>Version du package : ");
        html.push_str(&escape_html(version));
        html.push_str("</div>");
    }
    if let Some(name) = &asset.mse_name {
        html.push_str("<div>Nom du jeu : ");
        html.push_str(&escape_html(name));
        html.push_str("</div>");
    }
    if let Some(game) = &asset.mse_game {
        append_mse_reference_html(
            html,
            "Jeu",
            game,
            asset.mse_game_resolution_status.as_deref(),
            asset.mse_game_resolved_path.as_deref(),
        );
    }
    if let Some(stylesheet) = &asset.mse_stylesheet {
        append_mse_reference_html(
            html,
            "Feuille de style",
            stylesheet,
            asset.mse_stylesheet_resolution_status.as_deref(),
            asset.mse_stylesheet_resolved_path.as_deref(),
        );
    }
    if asset.mse_has_include_directives == Some(true) {
        html.push_str("<div class=\"warning\">Directive include file détectée ; non suivie.</div>");
    }
    if !asset.mse_dependencies.is_empty() {
        html.push_str("<div><b>Dépendances :</b><ul>");
        for dependency in &asset.mse_dependencies {
            html.push_str("<li><code>");
            html.push_str(&escape_html(&dependency.package));
            html.push_str("</code>");
            if let Some(minimum) = &dependency.min_version {
                html.push_str(" — version minimale ");
                html.push_str(&escape_html(minimum));
            }
            match dependency.resolution_status.as_deref() {
                Some("resolved") => {
                    if let Some(path) = &dependency.resolved_path {
                        html.push_str(" — présent : <code>");
                        html.push_str(&escape_html(path));
                        html.push_str("</code>");
                    }
                }
                Some("missing") => html.push_str(" — <span class=\"warning\">absent</span>"),
                Some("ambiguous") => html.push_str(" — <span class=\"warning\">ambigu</span>"),
                _ => {}
            }
            match dependency.version_status.as_deref() {
                Some("satisfied") => html.push_str(" — version satisfaite"),
                Some("insufficient") => {
                    html.push_str(" — <span class=\"warning\">version insuffisante</span>")
                }
                Some("unknown") => {
                    html.push_str(" — <span class=\"warning\">version non vérifiable</span>")
                }
                _ => {}
            }
            if dependency.cycle_detected == Some(true) {
                html.push_str(" — <span class=\"warning\">cycle détecté</span>");
            }
            html.push_str("</li>");
        }
        html.push_str("</ul></div>");
    }
    if !asset.mse_resources.is_empty() || asset.mse_resources_truncated {
        append_mse_resource_summary_html(html, &asset.mse_resources);
        if asset.mse_resources_truncated {
            html.push_str("<div class=\"warning\">Indexation des ressources incomplète.</div>");
        }
        html.push_str("<details><summary>Ressources internes indexées : ");
        html.push_str(&asset.mse_resources.len().to_string());
        html.push_str("</summary><ul>");
        for resource in asset.mse_resources.iter().take(100) {
            html.push_str("<li><code>");
            html.push_str(&escape_html(&resource.relative_path));
            html.push_str("</code> — ");
            html.push_str(&resource.size_bytes.to_string());
            html.push_str(" octets");
            if let Some(hash) = &resource.sha256 {
                html.push_str(" — SHA-256 <code>");
                html.push_str(&escape_html(hash));
                html.push_str("</code>");
            } else {
                html.push_str(" — <span class=\"warning\">empreinte non calculée : ");
                html.push_str(&escape_html(&resource.hash_status));
                html.push_str("</span>");
            }
            html.push_str("</li>");
        }
        if asset.mse_resources.len() > 100 {
            html.push_str("<li>Affichage limité aux 100 premières ressources ; le JSON conserve la liste indexée.</li>");
        }
        html.push_str("</ul>");
        if asset.mse_resources_truncated {
            html.push_str("<p class=\"warning\">Indexation partielle : une limite de sécurité ou une erreur a empêché l'indexation complète.</p>");
        }
        html.push_str("</details>");
    }
    html.push_str("</section>");
}

pub fn export_html(document: &InventoryDocument, destination: &Path) -> Result<()> {
    ensure_export_outside_source(document, destination)?;
    let mut html = String::from(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Rapport d'inventaire d'assets</title><style>body{font:14px system-ui,sans-serif;margin:2rem;color:#20242a;background:#fff}h1{margin-bottom:.3rem}.meta{color:#59636e}.stats{display:flex;gap:1.5rem;flex-wrap:wrap;margin:1.5rem 0}.stat{padding:.8rem 1rem;background:#f1f3f5;border-radius:.5rem}table{border-collapse:collapse;width:100%;font-size:12px}th,td{border:1px solid #d8dde3;padding:.45rem;text-align:left;vertical-align:top}th{background:#eef1f4;position:sticky;top:0}code{overflow-wrap:anywhere}.warning{color:#8a4b00}.duplicate{color:#075a91}</style></head><body>"
    );
    html.push_str("<h1>Rapport d'inventaire d'assets</h1><p class=\"meta\">Source : ");
    html.push_str(&escape_html(&document.source_root));
    html.push_str("</p><div class=\"stats\"><div class=\"stat\"><b>Fichiers</b><br>");
    html.push_str(&document.files_scanned.to_string());
    html.push_str("</div><div class=\"stat\"><b>Taille totale</b><br>");
    html.push_str(&document.total_bytes.to_string());
    html.push_str(" octets</div><div class=\"stat\"><b>Doublons exacts</b><br>");
    html.push_str(&document.duplicate_files.to_string());
    html.push_str("</div><div class=\"stat\"><b>Schéma</b><br>v");
    html.push_str(&document.schema_version.to_string());
    html.push_str("</div></div><table><thead><tr><th>Chemin</th><th>Famille</th><th>Taille</th><th>SHA-256</th><th>Métadonnées</th><th>Revue</th></tr></thead><tbody>");
    for asset in &document.assets {
        html.push_str("<tr><td><code>");
        html.push_str(&escape_html(&asset.relative_path));
        html.push_str("</code></td><td>");
        html.push_str(&escape_html(&asset.detected_family));
        if let Some(notice) = &asset.notice_kind {
            html.push_str(&format!(" ({})", escape_html(notice)));
        }
        html.push_str("</td><td>");
        html.push_str(&asset.size_bytes.to_string());
        html.push_str("</td><td><code>");
        html.push_str(&escape_html(&asset.sha256));
        html.push_str("</code></td><td>");
        if let Some(excerpt) = &asset.notice_excerpt {
            html.push_str("<details><summary>Extrait de notice (non interprété)");
            if asset.notice_excerpt_truncated {
                html.push_str(" — limité à 4 Kio");
            }
            html.push_str("</summary><pre>");
            html.push_str(&escape_html(excerpt));
            html.push_str("</pre></details>");
        } else if asset.notice_kind.is_some() {
            html.push_str("<div>Extrait non disponible (contenu non UTF-8).</div>");
        }
        if let (Some(width), Some(height)) = (asset.raster_width, asset.raster_height) {
            html.push_str(&format!("Raster : {}×{} ", width, height));
        }
        if let Some(color_type) = &asset.raster_color_type {
            html.push_str(&format!("Canaux : {} ", escape_html(color_type)));
        }
        if let Some(has_alpha) = asset.raster_has_alpha {
            html.push_str(if has_alpha { "Alpha " } else { "Sans alpha " });
        }
        if let Some(orientation) = &asset.raster_orientation {
            html.push_str(&format!("Orientation : {} ", escape_html(orientation)));
        }
        if let Some(width) = &asset.svg_width {
            html.push_str(&format!("SVG largeur : {} ", escape_html(width)));
        }
        if let Some(height) = &asset.svg_height {
            html.push_str(&format!("SVG hauteur : {} ", escape_html(height)));
        }
        if let Some(viewbox) = &asset.svg_viewbox {
            html.push_str(&format!("viewBox : {} ", escape_html(viewbox)));
        }
        if let Some(family) = &asset.font_family {
            html.push_str(&format!("Police : {} ", escape_html(family)));
        }
        if let Some(count) = asset.archive_entry_count {
            html.push_str(&format!("ZIP : {} entrées ", count));
        }
        append_mse_metadata_html(&mut html, asset);
        if let Some(first_path) = &asset.duplicate_of {
            html.push_str("<span class=\"duplicate\">Doublon de ");
            html.push_str(&escape_html(first_path));
            html.push_str("</span> ");
        }
        if let Some(warning) = &asset.analysis_warning {
            html.push_str("<p class=\"warning\">");
            html.push_str(&escape_html(warning));
            html.push_str("</p>");
        }
        html.push_str("</td><td>");
        html.push_str(&escape_html(&asset.review_status));
        html.push_str("</td></tr>");
    }
    html.push_str("</tbody></table><p class=\"meta\">Rapport autonome hors ligne. Les catégories sont indicatives et ne constituent ni une validation de licence ni une approbation de ressource.</p></body></html>");
    with_new_output_file(destination, |file| {
        file.write_all(html.as_bytes()).with_context(|| {
            format!(
                "Écriture du rapport HTML impossible : {}",
                destination.display()
            )
        })?;
        Ok(())
    })
}

#[cfg(test)]
mod raster_metadata_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn raster_metadata_rejects_final_symlink() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("mtg-auditor-raster-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("target.jpg"), b"not an image").unwrap();
        symlink(root.join("target.jpg"), root.join("alias.jpg")).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let metadata = raster_metadata(&workspace, Path::new("alias.jpg"), "raster");
        assert!(
            metadata
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("sans suivi")),
            "raster metadata must reject a final symlink: {:?}",
            metadata.warning
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn detects_same_size_content_mutation_even_when_mtime_is_restored() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-content-mutation-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.bin");
        std::fs::write(&path, b"original").unwrap();
        let original_modified = std::fs::metadata(&path).unwrap().modified().unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut file = open_file_nofollow(&workspace, Path::new("sample.bin")).unwrap();
        let (initial_sha256, _) = sha256_reader_with_prefix(&mut file, 0).unwrap();
        let opened_metadata = file.metadata().unwrap();

        std::fs::write(&path, b"modified").unwrap();
        std::fs::File::open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(original_modified))
            .unwrap();
        let current_metadata = workspace.symlink_metadata(Path::new("sample.bin")).unwrap();
        assert_eq!(opened_metadata.len(), current_metadata.len());
        assert_eq!(
            opened_metadata.modified().unwrap(),
            current_metadata.modified().unwrap()
        );
        assert!(file_metadata_is_stable(&opened_metadata, &current_metadata));

        let error = verify_reader_sha256_unchanged(&mut file, &initial_sha256)
            .expect_err("same-size content mutation must invalidate the analysis");
        assert!(error.to_string().contains("SHA-256 différent"));

        drop(file);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn detects_same_size_content_mutation_on_windows() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-content-mutation-windows-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.bin");
        std::fs::write(&path, b"original").unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut file = open_file_nofollow(&workspace, Path::new("sample.bin")).unwrap();
        let (initial_sha256, _) = sha256_reader_with_prefix(&mut file, 0).unwrap();
        let opened_metadata = file.metadata().unwrap();

        // Windows peut refuser SetFileTime tant que le handle de lecture est ouvert.
        // La longueur reste identique ; le test vérifie la détection par le contenu.
        std::fs::write(&path, b"modified").unwrap();
        let current_metadata = workspace.symlink_metadata(Path::new("sample.bin")).unwrap();
        assert_eq!(opened_metadata.len(), current_metadata.len());

        let error = verify_reader_sha256_unchanged(&mut file, &initial_sha256)
            .expect_err("same-size content mutation must invalidate the analysis");
        assert!(error.to_string().contains("SHA-256 différent"));

        drop(file);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_jpeg_exif_orientation_without_decoding_pixels() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-raster-orientation-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();

        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(b"II");
        exif.extend_from_slice(&42_u16.to_le_bytes());
        exif.extend_from_slice(&8_u32.to_le_bytes());
        exif.extend_from_slice(&1_u16.to_le_bytes());
        exif.extend_from_slice(&0x0112_u16.to_le_bytes());
        exif.extend_from_slice(&3_u16.to_le_bytes());
        exif.extend_from_slice(&1_u32.to_le_bytes());
        exif.extend_from_slice(&6_u16.to_le_bytes());
        exif.extend_from_slice(&0_u16.to_le_bytes());
        exif.extend_from_slice(&0_u32.to_le_bytes());

        let segment_length = u16::try_from(exif.len() + 2).unwrap();
        let mut with_exif = jpeg[..2].to_vec();
        with_exif.extend_from_slice(&[0xff, 0xe1]);
        with_exif.extend_from_slice(&segment_length.to_be_bytes());
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);
        std::fs::write(root.join("oriented.jpg"), &with_exif).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut file = open_file_nofollow(&workspace, Path::new("oriented.jpg")).unwrap();
        let (sha256, _) = sha256_reader_with_prefix(&mut file, 0).unwrap();
        assert_eq!(sha256, format!("{:x}", Sha256::digest(&with_exif)));
        let metadata = raster_metadata_from_file(&mut file, "raster");
        assert_eq!(metadata.orientation.as_deref(), Some("Rotate90"));
        assert_eq!(metadata.width, Some(1));
        assert_eq!(metadata.height, Some(1));
        drop(file);
        drop(workspace);

        let inventory = scan_directory(&root).unwrap();
        let asset = inventory
            .assets
            .iter()
            .find(|asset| asset.file_name == "oriented.jpg")
            .unwrap();
        assert_eq!(asset.raster_orientation.as_deref(), Some("Rotate90"));

        let report_path = root.with_extension("report.html");
        export_html(&inventory, &report_path).unwrap();
        let report = std::fs::read_to_string(&report_path).unwrap();
        assert!(report.contains("Orientation : Rotate90"));

        std::fs::remove_file(report_path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod html_report_tests {
    use super::*;

    #[test]
    fn html_report_includes_escaped_mse_dependencies_and_resource_hashes() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-html-{}", std::process::id()));
        let source = root.join("source");
        let package = source.join("sample.mse-set");
        let game = source.join("magic.mse-game");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(package.join("cards")).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(
            source.join("sized.svg"),
            r#"<svg width="24px" height="12px"/>"#,
        )
        .unwrap();
        let license_bytes = b"Example notice <script>alert(1)</script>\n";
        std::fs::write(source.join("LICENSE.txt"), license_bytes).unwrap();
        std::fs::write(
            source.join("NOTICE.txt"),
            vec![b'N'; MAX_NOTICE_EXCERPT_BYTES + 32],
        )
        .unwrap();
        std::fs::write(source.join("NOTICE-BINARY.txt"), [0xff, 0xfe]).unwrap();
        std::fs::write(package.join("set"),
            b"mse version: 0.3.4\nshort name: <script>alert(1)</script>\nfull name: <b>Complete Set</b>\nversion: 2.1.0\ngame: Magic\ndepends on: magic.mse-game 1.0.0\n").unwrap();
        std::fs::write(package.join("cards/example.png"), b"resource bytes").unwrap();
        std::fs::write(
            game.join("game"),
            b"mse version: 0.3.4\nversion: 1.0.0\nname: Magic\n",
        )
        .unwrap();

        let inventory = scan_directory(&source).unwrap();
        let destination = root.join("report.html");
        export_html(&inventory, &destination).unwrap();
        let html = std::fs::read_to_string(&destination).unwrap();
        assert!(html.contains("Package MSE : set"));
        assert!(html.contains("Nom complet : &lt;b&gt;Complete Set&lt;/b&gt;"));
        assert!(html.contains("Version du package : 2.1.0"));
        assert!(html.contains("Nom du jeu : Magic"));
        assert!(html.contains("SVG largeur : 24px"));
        assert!(html.contains("SVG hauteur : 12px"));
        assert!(html.contains("Dépendances :"));
        assert!(html.contains("Ressources internes indexées : 1"));
        assert!(html.contains("1 calculées ; 0 non calculées"));
        let license = inventory
            .assets
            .iter()
            .find(|asset| asset.file_name == "LICENSE.txt")
            .unwrap();
        assert_eq!(
            license.notice_excerpt.as_deref(),
            Some("Example notice <script>alert(1)</script>\n")
        );
        assert_eq!(
            license.sha256,
            format!("{:x}", Sha256::digest(license_bytes))
        );
        assert!(!license.notice_excerpt_truncated);
        let notice = inventory
            .assets
            .iter()
            .find(|asset| asset.file_name == "NOTICE.txt")
            .unwrap();
        assert_eq!(
            notice.notice_excerpt.as_ref().map(String::len),
            Some(MAX_NOTICE_EXCERPT_BYTES)
        );
        assert!(notice.notice_excerpt_truncated);
        let binary_notice = inventory
            .assets
            .iter()
            .find(|asset| asset.file_name == "NOTICE-BINARY.txt")
            .unwrap();
        assert_eq!(binary_notice.notice_excerpt, None);
        assert!(html.contains("Extrait de notice (non interprété)"));
        assert!(html.contains("limité à 4 Kio"));
        assert!(html.contains("Extrait non disponible (contenu non UTF-8)."));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("SHA-256"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod bounded_read_tests {
    use super::*;

    #[test]
    fn bounded_reader_accepts_exact_limit() {
        let bytes = read_limited(&b"12345"[..], 5).unwrap();
        assert_eq!(bytes, b"12345");
    }

    #[test]
    fn bounded_reader_rejects_data_over_limit() {
        let error = read_limited(&b"123456"[..], 5).unwrap_err();
        assert!(error.to_string().contains("supérieur à la limite"));
    }

    #[test]
    fn bounded_reader_accepts_empty_input_with_zero_limit() {
        assert!(read_limited(&b""[..], 0).unwrap().is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, path::PathBuf};

    #[test]
    fn capability_export_refuses_overwrite_and_removes_partial_output() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-cap-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let output = root.join("report.json");
        with_new_output_file(&output, |file| {
            file.write_all(b"first").context("test write")?;
            Ok(())
        })
        .unwrap();
        assert!(with_new_output_file(&output, |file| {
            file.write_all(b"second").context("test write")?;
            Ok(())
        })
        .is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"first");
        let partial = root.join("partial.json");
        assert!(with_new_output_file(&partial, |_file| -> Result<()> {
            Err(anyhow::anyhow!("simulated export failure"))
        })
        .is_err());
        assert!(!partial.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn capability_export_refuses_symlink_destination() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-export-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("outside.txt");
        let destination = root.join("report.json");
        std::fs::write(&target, b"preserve me").unwrap();
        symlink(&target, &destination).unwrap();

        assert!(with_new_output_file(&destination, |file| {
            file.write_all(b"overwrite")?;
            Ok(())
        })
        .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"preserve me");
        assert!(std::fs::symlink_metadata(&destination)
            .unwrap()
            .file_type()
            .is_symlink());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_export_creation_has_exactly_one_winner() {
        use std::sync::{Arc, Barrier};
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-export-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let destination = root.join("report.json");
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for content in [b"first".to_vec(), b"second".to_vec()] {
            let destination = destination.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                with_new_output_file(&destination, |file| {
                    file.write_all(&content)?;
                    Ok(())
                })
            }));
        }
        barrier.wait();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let content = std::fs::read(&destination).unwrap();
        assert!(content == b"first" || content == b"second");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn absolute_directory_open_rejects_symlinked_path_component() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!(
            "mtg-auditor-absolute-dir-nofollow-{}",
            std::process::id()
        ));
        let real = base.join("real");
        let alias = base.join("alias");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&real).unwrap();
        symlink(&real, &alias).unwrap();

        let error = open_absolute_directory_nofollow(&alias)
            .expect_err("absolute path traversal must reject a symlink component");
        assert!(error.to_string().contains("sans suivi"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn absolute_directory_open_rejects_symlinked_path_component_when_supported() {
        use std::os::windows::fs::symlink_dir;
        let base = std::env::temp_dir().join(format!(
            "mtg-auditor-absolute-dir-nofollow-{}",
            std::process::id()
        ));
        let real = base.join("real");
        let alias = base.join("alias");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&real).unwrap();
        if let Err(error) = symlink_dir(&real, &alias) {
            eprintln!("SKIP: création de lien symbolique de répertoire Windows refusée: {error}");
            std::fs::remove_dir_all(base).unwrap();
            return;
        }

        let error = open_absolute_directory_nofollow(&alias)
            .expect_err("absolute path traversal must reject a symlink component");
        assert!(error.to_string().contains("sans suivi"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn nofollow_file_open_rejects_symlinked_parent_components() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-nofollow-parent-{}",
            std::process::id()
        ));
        let real = root.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("sample.txt"), b"safe").unwrap();
        symlink(&real, root.join("alias")).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let error = open_file_nofollow(&workspace, Path::new("alias/sample.txt"))
            .expect_err("symlinked parent component must be rejected");
        assert!(error.to_string().contains("sans suivi"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn nofollow_file_open_rejects_final_symlink_component() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("mtg-auditor-nofollow-final-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("target.txt"), b"safe").unwrap();
        symlink(root.join("target.txt"), root.join("link.txt")).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let error = open_file_nofollow(&workspace, Path::new("link.txt"))
            .expect_err("final symlink component must be rejected");
        assert!(error.to_string().contains("sans suivi"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn nofollow_file_open_rejects_symlinked_parent_when_supported() {
        use std::os::windows::fs::symlink_dir;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-nofollow-parent-{}",
            std::process::id()
        ));
        let real = root.join("real");
        let alias = root.join("alias");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("sample.txt"), b"safe").unwrap();
        if let Err(error) = symlink_dir(&real, &alias) {
            eprintln!("SKIP: création de lien symbolique de répertoire Windows refusée: {error}");
            std::fs::remove_dir_all(root).unwrap();
            return;
        }

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let error = open_file_nofollow(&workspace, Path::new("alias/sample.txt"))
            .expect_err("symlinked parent component must be rejected");
        assert!(error.to_string().contains("sans suivi"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn nofollow_file_open_rejects_final_symlink_when_supported() {
        use std::os::windows::fs::symlink_file;

        let root =
            std::env::temp_dir().join(format!("mtg-auditor-nofollow-final-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        std::fs::write(&target, b"safe").unwrap();
        if let Err(error) = symlink_file(&target, &link) {
            eprintln!("SKIP: création de lien symbolique Windows refusée: {error}");
            std::fs::remove_dir_all(root).unwrap();
            return;
        }

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let error = open_file_nofollow(&workspace, Path::new("link.txt"))
            .expect_err("final symlink component must be rejected");
        assert!(error.to_string().contains("sans suivi"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn local_scan_does_not_follow_file_or_directory_symlinks() {
        use std::os::unix::fs::symlink;
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-symlink-test-{}", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), b"outside").unwrap();
        symlink(outside.join("secret.txt"), root.join("file-link.txt")).unwrap();
        symlink(&outside, root.join("directory-link")).unwrap();

        let inventory = scan_directory(&root).unwrap();
        assert_eq!(inventory.files_scanned, 0);
        assert!(inventory.assets.is_empty());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn local_scan_does_not_follow_file_symlinks_when_supported() {
        use std::os::windows::fs::symlink_file;
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-symlink-test-{}", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let target = outside.join("secret.txt");
        std::fs::write(&target, b"outside").unwrap();
        let link = root.join("file-link.txt");
        if let Err(error) = symlink_file(&target, &link) {
            eprintln!(
                "SKIP: création de lien symbolique Windows refusée par l'environnement: {error}"
            );
            std::fs::remove_dir_all(base).unwrap();
            return;
        }

        let inventory = scan_directory(&root).unwrap();
        assert_eq!(inventory.files_scanned, 0);
        assert!(inventory.assets.is_empty());
        assert_eq!(std::fs::read(&target).unwrap(), b"outside");
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn local_scan_rejects_directory_replaced_by_symlink_before_recursion() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!(
            "mtg-auditor-directory-symlink-race-{}",
            std::process::id()
        ));
        let root = base.join("root");
        let outside = base.join("outside");
        let child_path = root.join("child");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&child_path).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.txt");
        std::fs::write(&secret, b"outside secret").unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        // Simule une entrée observée comme répertoire puis remplacée avant ouverture.
        std::fs::remove_dir(&child_path).unwrap();
        symlink(&outside, &child_path).unwrap();
        let error = open_child_directory_nofollow(
            &workspace,
            std::ffi::OsStr::new("child"),
            Path::new("child"),
        )
        .expect_err("a replaced directory symlink must not be followed");
        assert!(error.to_string().contains("sans suivi"));
        assert_eq!(std::fs::read(&secret).unwrap(), b"outside secret");
        drop(workspace);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn local_scan_rejects_directory_replaced_by_symlink_when_supported() {
        use std::os::windows::fs::symlink_dir;
        let base = std::env::temp_dir().join(format!(
            "mtg-auditor-directory-symlink-race-{}",
            std::process::id()
        ));
        let root = base.join("root");
        let outside = base.join("outside");
        let child_path = root.join("child");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&child_path).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.txt");
        std::fs::write(&secret, b"outside secret").unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        std::fs::remove_dir(&child_path).unwrap();
        if let Err(error) = symlink_dir(&outside, &child_path) {
            eprintln!("SKIP: création de lien symbolique de répertoire Windows refusée: {error}");
            drop(workspace);
            std::fs::remove_dir_all(base).unwrap();
            return;
        }
        let error = open_child_directory_nofollow(
            &workspace,
            std::ffi::OsStr::new("child"),
            Path::new("child"),
        )
        .expect_err("a replaced directory symlink must not be followed");
        assert!(error.to_string().contains("sans suivi"));
        assert_eq!(std::fs::read(&secret).unwrap(), b"outside secret");
        drop(workspace);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn cancelled_scan_stops_before_analyzing_source_files() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-cancel-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("sample.txt"), b"sample").unwrap();
        let cancelled = AtomicBool::new(true);
        let result = scan_directory_with_progress_and_cancel(&root, |_, _| {}, &cancelled);
        assert!(result.unwrap_err().to_string().contains("annulé"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn classifies_known_and_unknown_extensions() {
        assert_eq!(classify_extension("png"), "raster");
        assert_eq!(classify_extension("SVG"), "svg");
        assert_eq!(classify_extension("mse-set"), "mse");
        assert_eq!(classify_extension("exe"), "unknown");
        assert_eq!(
            classify_notice("LICENSE-MIT"),
            Some("license-notice".to_string())
        );
        assert_eq!(
            classify_notice("NOTICE.txt"),
            Some("notice-file".to_string())
        );
    }

    #[test]
    fn summarizes_mse_resource_hash_statuses_in_html() {
        let resources = vec![
            MseResourceRecord {
                relative_path: "cards/hashed.bin".to_string(),
                size_bytes: 10,
                sha256: Some("abc".to_string()),
                hash_status: "hashed".to_string(),
            },
            MseResourceRecord {
                relative_path: "cards/broken.bin".to_string(),
                size_bytes: 12,
                sha256: None,
                hash_status: "read-error".to_string(),
            },
            MseResourceRecord {
                relative_path: "cards/unsafe.bin".to_string(),
                size_bytes: 14,
                sha256: None,
                hash_status: "suspicious-path".to_string(),
            },
        ];
        let mut html = String::new();
        append_mse_resource_summary_html(&mut html, &resources);
        assert!(html.contains("1 calculées ; 2 non calculées"));
        assert!(html.contains("read-error : 1"));
        assert!(html.contains("suspicious-path : 1"));
    }

    #[test]
    fn detects_svg_viewbox_and_external_references_without_rendering() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-svg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let svg = root.join("test.svg");
        std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="2.5cm" viewBox="0 0 10 20"><image href="https://example.invalid/image.png"/></svg>"#).unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (viewbox, width, height, external, valid, warning) =
            svg_metadata(&workspace, Path::new("test.svg"), "svg");
        assert_eq!(viewbox.as_deref(), Some("0 0 10 20"));
        assert_eq!(width.as_deref(), Some("100%"));
        assert_eq!(height.as_deref(), Some("2.5cm"));
        assert_eq!(external, Some(true));
        assert_eq!(valid, Some(true));
        assert!(warning.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_invalid_or_extreme_svg_dimensions() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-svg-dimensions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("invalid.svg"),
            r#"<svg width="NaN" height="1000000001px"/>"#,
        )
        .unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (_, width, height, _, valid, warning) =
            svg_metadata(&workspace, Path::new("invalid.svg"), "svg");
        assert_eq!(width, None);
        assert_eq!(height, None);
        assert_eq!(valid, Some(true));
        assert!(warning.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_nested_svg_when_document_root_is_not_svg() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-svg-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("nested.svg"),
            r#"<html><svg width="99" height="77"/></html>"#,
        )
        .unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (viewbox, width, height, _, valid, warning) =
            svg_metadata(&workspace, Path::new("nested.svg"), "svg");
        assert_eq!(viewbox, None);
        assert_eq!(width, None);
        assert_eq!(height, None);
        assert_eq!(valid, Some(false));
        assert!(warning.unwrap().contains("racine SVG absent"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_multiple_top_level_svg_document_elements() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-svg-multiple-roots-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("multiple.svg"),
            r#"<html/><svg width="99" height="77"/>"#,
        )
        .unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (_, width, height, _, valid, warning) =
            svg_metadata(&workspace, Path::new("multiple.svg"), "svg");
        assert_eq!(width, None);
        assert_eq!(height, None);
        assert_eq!(valid, Some(false));
        assert!(warning.is_some());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_quoted_css_and_relative_svg_references() {
        let root = std::env::temp_dir().join(format!("mtg-auditor-svg-css-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("references.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" style="fill: url( '#gradient' ); filter: url( 'https://example.invalid/filter.svg' )"><image href="../images/card.png"/></svg>"#,
        )
        .unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (_, _, _, external, valid, warning) =
            svg_metadata(&workspace, Path::new("references.svg"), "svg");
        assert_eq!(external, Some(true));
        assert_eq!(valid, Some(true));
        assert!(warning.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_svg_with_too_many_attributes_on_one_element() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-svg-attributes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut source = String::from("<svg");
        for index in 0..257 {
            source.push_str(&format!(" a{}=\"x\"", index));
        }
        source.push_str("/>");
        std::fs::write(root.join("attributes.svg"), source).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (_, _, _, _, valid, warning) =
            svg_metadata(&workspace, Path::new("attributes.svg"), "svg");
        assert_eq!(valid, Some(false));
        assert!(warning.unwrap().contains("attributs par élément"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_svg_with_excessive_xml_nesting() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-svg-depth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut source = String::from("<svg>");
        for _ in 0..256 {
            source.push_str("<g>");
        }
        for _ in 0..256 {
            source.push_str("</g>");
        }
        source.push_str("</svg>");
        std::fs::write(root.join("deep.svg"), source).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (_, _, _, _, valid, warning) = svg_metadata(&workspace, Path::new("deep.svg"), "svg");
        assert_eq!(valid, Some(false));
        assert!(warning.unwrap().contains("profondeur XML maximale"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_svg_with_excessive_xml_event_count() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-svg-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut source = String::from("<svg>");
        for _ in 0..100_001 {
            source.push_str("<g/>");
        }
        source.push_str("</svg>");
        std::fs::write(root.join("many.svg"), source).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (_, _, _, _, valid, warning) = svg_metadata(&workspace, Path::new("many.svg"), "svg");
        assert_eq!(valid, Some(false));
        assert!(warning.unwrap().contains("nombre maximal d'événements"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_file_metadata_change_during_analysis() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mutation-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.txt");
        std::fs::write(&path, b"before").unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let before = workspace.metadata(Path::new("sample.txt")).unwrap();
        std::fs::write(&path, b"after content with a different size").unwrap();
        let after = workspace.metadata(Path::new("sample.txt")).unwrap();
        assert!(!file_metadata_is_stable(&before, &after));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn analysis_rejects_file_replaced_by_symlink_after_enumeration() {
        use std::os::unix::fs::symlink;
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-symlink-race-{}", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let source = root.join("sample.txt");
        let external = outside.join("secret.txt");
        std::fs::write(&source, b"original").unwrap();
        std::fs::write(&external, b"outside secret").unwrap();

        // Simule une entrée régulière observée pendant l'énumération, puis remplacée
        // par un lien avant que l'analyse du chemin relatif ne commence.
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        std::fs::remove_file(&source).unwrap();
        symlink(&external, &source).unwrap();

        let error = analyze_file(&source, &root, &workspace).unwrap_err();
        assert!(error.to_string().contains("lien symbolique"));
        assert_eq!(std::fs::read(&external).unwrap(), b"outside secret");
        drop(workspace);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn scan_is_sorted_and_hashes_file_contents() {
        let root: PathBuf =
            std::env::temp_dir().join(format!("mtg-auditor-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(root.join("z.txt"), b"same bytes").unwrap();
        std::fs::write(root.join("b/a.png"), b"same bytes").unwrap();
        let result = scan_directory(&root).unwrap();
        assert_eq!(result.files_scanned, 2);
        assert!(result.assets[0].relative_path < result.assets[1].relative_path);
        assert_eq!(result.assets[0].sha256, result.assets[1].sha256);
        assert_eq!(result.assets[0].sha256.len(), 64);
        assert!(result.assets[0].analysis_warning.is_some());
        assert_eq!(result.duplicate_files, 1);
        assert_eq!(result.assets[1].duplicate_of.as_deref(), Some("b/a.png"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_suspicious_zip_paths_without_extracting() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-zip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("unsafe.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"not extracted").unwrap();
        writer.finish().unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let metadata = zip_metadata(&workspace, Path::new("unsafe.zip"), "zip");
        assert_eq!(metadata.entry_count, Some(1));
        assert_eq!(metadata.suspicious_paths, Some(1));
        assert!(metadata.warning.is_some());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enforces_per_entry_and_total_budgets_for_mse_zip_resource_hashes() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-zip-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file("large.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"123456").unwrap();
        writer
            .start_file("first.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"12345").unwrap();
        writer
            .start_file("later.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut scan_budget = 10_u64;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 5,
                max_total_hash_bytes: 5,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );
        assert!(!truncated);
        assert_eq!(resources.len(), 3);
        assert_eq!(resources[0].hash_status, "size-limit");
        assert_eq!(resources[0].sha256, None);
        assert_eq!(resources[1].hash_status, "hashed");
        assert!(resources[1].sha256.is_some());
        assert_eq!(resources[2].hash_status, "total-budget");
        assert_eq!(resources[2].sha256, None);
        assert_eq!(scan_budget, 5);

        let mut scan_budget = 5_u64;
        let (scan_limited, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 5,
                max_total_hash_bytes: 10,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );
        assert!(!truncated);
        assert_eq!(scan_limited[1].hash_status, "hashed");
        assert_eq!(scan_limited[2].hash_status, "scan-budget");
        assert_eq!(scan_limited[2].sha256, None);
        assert_eq!(scan_budget, 0);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn applies_one_deterministic_mse_zip_hash_budget_across_a_scan() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-scan-budget-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for package_name in ["a.mse-set", "b.mse-set"] {
            let file = File::create(root.join(package_name)).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("set", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"mse version: 0.3.4\n").unwrap();
            writer
                .start_file("cards/image.bin", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"1234").unwrap();
            writer.finish().unwrap();
        }

        let mut scan_budget = 4_u64;
        let inventory = scan_directory_with_progress_and_cancel_with_budget(
            &root,
            |_, _| {},
            &AtomicBool::new(false),
            &mut scan_budget,
        )
        .unwrap();
        let first = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "a.mse-set")
            .unwrap();
        let second = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "b.mse-set")
            .unwrap();
        assert_eq!(first.mse_resources[0].hash_status, "hashed");
        assert!(first.mse_resources[0].sha256.is_some());
        assert_eq!(second.mse_resources[0].hash_status, "scan-budget");
        assert!(second.mse_resources[0].sha256.is_none());
        assert_eq!(scan_budget, 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stops_mse_zip_resource_indexing_when_scan_is_cancelled() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file("cards/image.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"resource").unwrap();
        writer.finish().unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let cancelled = AtomicBool::new(true);
        let mut scan_budget = 1024;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 1024,
                max_total_hash_bytes: 1024,
            },
            &mut scan_budget,
            &cancelled,
        );
        assert!(resources.is_empty());
        assert!(truncated);
        assert_eq!(scan_budget, 1024);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn marks_duplicate_mse_zip_resource_paths_as_ambiguous_without_hashing() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-duplicate-path-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file(
                "cards/first.bin",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"first resource").unwrap();
        writer
            .start_file(
                "cards/other.bin",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"second resource").unwrap();
        writer.finish().unwrap();

        // Le writer ZIP refuse les noms dupliqués ; modifier les deux occurrences
        // (en-tête local et table centrale) produit une fixture ZIP valide mais ambiguë.
        let mut archive_bytes = std::fs::read(&archive_path).unwrap();
        let original_name = b"cards/other.bin";
        let duplicate_name = b"cards/first.bin";
        assert_eq!(original_name.len(), duplicate_name.len());
        let offsets: Vec<usize> = archive_bytes
            .windows(original_name.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == original_name).then_some(offset))
            .collect();
        assert_eq!(offsets.len(), 2);
        for offset in offsets {
            archive_bytes[offset..offset + duplicate_name.len()].copy_from_slice(duplicate_name);
        }
        std::fs::write(&archive_path, archive_bytes).unwrap();
        let archive = zip::ZipArchive::new(File::open(&archive_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 3);
        drop(archive);

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut scan_budget = 1024;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 1024,
                max_total_hash_bytes: 1024,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );
        assert!(truncated);
        assert_eq!(resources.len(), 2);
        assert!(resources.iter().all(|resource| {
            resource.relative_path == "cards/first.bin"
                && resource.hash_status == "ambiguous-path"
                && resource.sha256.is_none()
        }));
        assert!(!root.join("cards/duplicate.bin").exists());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn marks_mse_zip_resource_with_invalid_crc_as_read_error() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-invalid-crc-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let payload = b"CRC-CORRUPTION-PROBE";
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file(
                "cards/corrupt.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();

        // Corrompre uniquement les données de l'entrée : la table centrale et
        // le CRC déclaré restent intacts, afin que l'erreur survienne à la lecture.
        let mut archive_bytes = std::fs::read(&archive_path).unwrap();
        let payload_offset = archive_bytes
            .windows(payload.len())
            .position(|window| window == payload)
            .expect("payload ZIP présent");
        archive_bytes[payload_offset] ^= 0x01;
        std::fs::write(&archive_path, archive_bytes).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut scan_budget = 1024;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 1024,
                max_total_hash_bytes: 1024,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );

        assert!(!truncated);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].relative_path, "cards/corrupt.bin");
        assert_eq!(resources[0].hash_status, "read-error");
        assert!(resources[0].sha256.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn marks_mse_zip_resource_with_corrupt_deflate_stream_as_read_error() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-invalid-deflate-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let payload = b"DEFLATE-CORRUPTION-PROBE-".repeat(128);
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file(
                "cards/broken.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(&payload).unwrap();
        writer.finish().unwrap();

        // Altérer le flux compressé sans modifier la table centrale ni le CRC.
        let mut archive_bytes = std::fs::read(&archive_path).unwrap();
        let filename = b"cards/broken.bin";
        let filename_offset = archive_bytes
            .windows(filename.len())
            .position(|window| window == filename)
            .expect("nom de ressource dans l'en-tête local");
        let local_header = filename_offset - 30;
        assert_eq!(
            &archive_bytes[local_header..local_header + 4],
            b"PK\x03\x04"
        );
        let filename_len = u16::from_le_bytes([
            archive_bytes[local_header + 26],
            archive_bytes[local_header + 27],
        ]) as usize;
        let extra_len = u16::from_le_bytes([
            archive_bytes[local_header + 28],
            archive_bytes[local_header + 29],
        ]) as usize;
        assert_eq!(filename_len, filename.len());
        let compressed_size = u32::from_le_bytes([
            archive_bytes[local_header + 18],
            archive_bytes[local_header + 19],
            archive_bytes[local_header + 20],
            archive_bytes[local_header + 21],
        ]) as usize;
        assert!(compressed_size > 8);
        let data_start = filename_offset + filename_len + extra_len;
        archive_bytes[data_start + compressed_size / 2] ^= 0xff;
        std::fs::write(&archive_path, archive_bytes).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut scan_budget = 16 * 1024;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 16 * 1024,
                max_total_hash_bytes: 16 * 1024,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );

        assert!(!truncated);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].relative_path, "cards/broken.bin");
        assert_eq!(resources[0].hash_status, "read-error");
        assert!(resources[0].sha256.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn indexes_mse_zip_resources_with_bounded_streaming_sha256() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-zip-resources-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let content = b"zip resource bytes";
        std::fs::write(root.join("reference.png"), content).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(b"mse version: 0.3.4\nshort name: Zip Set\n")
            .unwrap();
        writer
            .start_file(
                "cards/example.png",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(content).unwrap();
        writer.finish().unwrap();

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        let reference = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "reference.png")
            .unwrap();
        assert_eq!(package.mse_resources.len(), 1);
        assert_eq!(package.mse_resources[0].relative_path, "cards/example.png");
        assert_eq!(package.mse_resources[0].size_bytes, content.len() as u64);
        assert_eq!(
            package.mse_resources[0].sha256.as_deref(),
            Some(reference.sha256.as_str())
        );
        assert_eq!(package.mse_resources[0].hash_status, "hashed");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn flags_suspicious_mse_zip_resource_paths_without_extracting() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-traversal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(b"mse version: 0.3.4\nshort name: Safe Set\n")
            .unwrap();
        writer
            .start_file("../escape.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"not extracted").unwrap();
        writer.finish().unwrap();

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        assert_eq!(package.mse_main_file_present, Some(true));
        assert_eq!(package.mse_short_name.as_deref(), Some("Safe Set"));
        assert_eq!(package.archive_suspicious_paths, Some(1));
        assert!(package
            .analysis_warning
            .as_deref()
            .is_some_and(|warning| warning.contains("chemins suspects")));
        assert!(!root.join("escape.png").exists());
        assert_eq!(package.mse_resources.len(), 1);
        assert_eq!(package.mse_resources[0].hash_status, "suspicious-path");
        assert!(package.mse_resources[0].sha256.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn flags_mse_zip_extreme_compression_ratio_without_decompression() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-ratio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("images/repetitive.bin", options).unwrap();
        writer.write_all(&vec![0_u8; 4 * 1024 * 1024]).unwrap();
        writer.finish().unwrap();

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        assert_eq!(package.mse_main_file_present, Some(true));
        assert!(package
            .analysis_warning
            .as_deref()
            .is_some_and(|warning| warning.contains("ratio de compression")));
        assert_eq!(package.mse_resources.len(), 1);
        assert_eq!(package.mse_resources[0].hash_status, "suspicious-ratio");
        assert!(package.mse_resources[0].sha256.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_non_utf8_mse_headers() {
        assert!(parse_mse_header(&[
            b'm', b's', b'e', b' ', b'v', b'e', b'r', b's', b'i', b'o', b'n', b':', b' ', 0xff
        ])
        .is_err());
    }

    #[test]
    fn detects_mse_set_zip_main_file_without_extracting() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-set-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(b"mse version: 0.3.4\nset info: fixture")
            .unwrap();
        writer
            .start_file(
                "cards/example.txt",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"resource").unwrap();
        writer.finish().unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let metadata = zip_metadata(&workspace, Path::new("sample.mse-set"), "mse-set");
        assert_eq!(metadata.entry_count, Some(2));
        assert_eq!(metadata.mse_main_file_present, Some(true));
        assert!(metadata.warning.is_none());
        assert!(
            !root.join("set").exists(),
            "ZIP contents must never be extracted"
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_mse_game_and_stylesheet_references_by_declared_names() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-reference-test-{}",
            std::process::id()
        ));
        let game = root.join("my-game.mse-game");
        let style = root.join("my-game-new.mse-style");
        let set = root.join("sample.mse-set");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&style).unwrap();
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(game.join("game"), b"mse version: 0.3.4\nname: My Game\n").unwrap();
        std::fs::write(
            style.join("style"),
            b"mse version: 0.3.4\nshort name: New\ngame: My Game\n",
        )
        .unwrap();
        std::fs::write(
            set.join("set"),
            b"mse version: 0.3.4\ngame: My Game\nstylesheet: New\n",
        )
        .unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(main.mse_game_resolution_status.as_deref(), Some("resolved"));
        assert_eq!(
            main.mse_game_resolved_path.as_deref(),
            Some("my-game.mse-game/game")
        );
        assert_eq!(
            main.mse_stylesheet_resolution_status.as_deref(),
            Some("resolved")
        );
        assert_eq!(
            main.mse_stylesheet_resolved_path.as_deref(),
            Some("my-game-new.mse-style/style")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_missing_mse_game_and_stylesheet_references() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-missing-reference-{}",
            std::process::id()
        ));
        let set = root.join("sample.mse-set");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(
            set.join("set"),
            b"mse version: 0.3.4\ngame: Unknown Game\nstylesheet: Missing Style\n",
        )
        .unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(main.mse_game_resolution_status.as_deref(), Some("missing"));
        assert_eq!(
            main.mse_stylesheet_resolution_status.as_deref(),
            Some("missing")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn marks_ambiguous_mse_game_and_stylesheet_references() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-ambiguous-reference-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        for name in ["game-a.mse-game", "game-b.mse-game"] {
            let package = root.join(name);
            std::fs::create_dir_all(&package).unwrap();
            std::fs::write(package.join("game"), b"mse version: 0.3.4\nname: My Game\n").unwrap();
        }
        for name in ["style-a.mse-style", "style-b.mse-style"] {
            let package = root.join(name);
            std::fs::create_dir_all(&package).unwrap();
            std::fs::write(
                package.join("style"),
                b"mse version: 0.3.4\nshort name: New\ngame: My Game\n",
            )
            .unwrap();
        }
        let set = root.join("sample.mse-set");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(
            set.join("set"),
            b"mse version: 0.3.4\ngame: My Game\nstylesheet: New\n",
        )
        .unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(
            main.mse_game_resolution_status.as_deref(),
            Some("ambiguous")
        );
        assert_eq!(
            main.mse_stylesheet_resolution_status.as_deref(),
            Some("ambiguous")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_mse_dependency_cycles_without_recursive_graph_walks() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-cycle-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (directory, contents) in [
            (
                "a.mse-game",
                "mse version: 0.3.4\nversion: 1.0.0\nname: A\ndepends on: b.mse-game\n",
            ),
            (
                "b.mse-game",
                "mse version: 0.3.4\nversion: 1.0.0\nname: B\ndepends on: a.mse-game\n",
            ),
            (
                "c.mse-game",
                "mse version: 0.3.4\nversion: 1.0.0\nname: C\ndepends on: b.mse-game\n",
            ),
        ] {
            let package = root.join(directory);
            std::fs::create_dir_all(&package).unwrap();
            std::fs::write(package.join("game"), contents.as_bytes()).unwrap();
        }
        let inventory = scan_directory(&root).unwrap();
        let a = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "a.mse-game/game")
            .unwrap();
        let b = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "b.mse-game/game")
            .unwrap();
        let c = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "c.mse-game/game")
            .unwrap();
        assert_eq!(a.mse_dependencies[0].cycle_detected, Some(true));
        assert_eq!(b.mse_dependencies[0].cycle_detected, Some(true));
        assert_eq!(c.mse_dependencies[0].cycle_detected, Some(false));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_mse_calendar_dates_and_leap_years() {
        assert!(parse_mse_version("2008-02-29").is_some());
        assert!(parse_mse_version("2000-02-29").is_some());
        assert!(parse_mse_version("1900-02-29").is_none());
        assert!(parse_mse_version("2007-02-29").is_none());
        assert!(parse_mse_version("2007-04-31").is_none());
        assert!(parse_mse_version("2007-13-01").is_none());
        assert!(parse_mse_version("2007-00-10").is_none());
    }

    #[test]
    fn validates_mse_dependency_minimum_versions_for_numeric_and_date_formats() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-version-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let packages = [
            ("modern.mse-game", "1.10.0"),
            ("older.mse-game", "1.0.0"),
            ("dated.mse-game", "2007-06-07"),
            ("unknown.mse-game", ""),
        ];
        for (directory, version) in packages {
            let package = root.join(directory);
            std::fs::create_dir_all(&package).unwrap();
            let version_line = if version.is_empty() {
                String::new()
            } else {
                format!("version: {}\n", version)
            };
            std::fs::write(
                package.join("game"),
                format!("mse version: 0.3.4\n{}name: {}\n", version_line, directory).as_bytes(),
            )
            .unwrap();
        }
        let set = root.join("sample.mse-set");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("set"),
            b"mse version: 0.3.4\ndepends on: modern.mse-game 1.2.0\ndepends on: older.mse-game 1.2.0\ndepends on: dated.mse-game 2007-06-06\ndepends on: unknown.mse-game 1.0.0\ndepends on: modern.mse-game\n").unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(
            main.mse_dependencies[0].version_status.as_deref(),
            Some("satisfied")
        );
        assert_eq!(
            main.mse_dependencies[1].version_status.as_deref(),
            Some("insufficient")
        );
        assert_eq!(
            main.mse_dependencies[2].version_status.as_deref(),
            Some("satisfied")
        );
        assert_eq!(
            main.mse_dependencies[3].version_status.as_deref(),
            Some("unknown")
        );
        assert_eq!(
            main.mse_dependencies[4].version_status.as_deref(),
            Some("not-required")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_mse_dependencies_locally_and_reports_missing_packages() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-resolve-test-{}",
            std::process::id()
        ));
        let game = root.join("magic.mse-game");
        let set = root.join("sample.mse-set");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(
            game.join("game"),
            b"mse version: 0.3.4\nversion: 2007-06-06\nshort name: Magic\n",
        )
        .unwrap();
        std::fs::write(set.join("set"),
            b"mse version: 0.3.4\ndepends on: magic.mse-game 2007-06-06\ndepends on: absent.mse-style 1.0\n").unwrap();
        let incomplete_archive = File::create(root.join("absent.mse-style")).unwrap();
        let mut incomplete_writer = zip::ZipWriter::new(incomplete_archive);
        incomplete_writer
            .start_file(
                "resources/only.txt",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        incomplete_writer.write_all(b"not a main file").unwrap();
        incomplete_writer.finish().unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(
            main.mse_dependencies[0].resolution_status.as_deref(),
            Some("resolved")
        );
        assert_eq!(
            main.mse_dependencies[0].resolved_path.as_deref(),
            Some("magic.mse-game/game")
        );
        assert_eq!(
            main.mse_dependencies[0].version_status.as_deref(),
            Some("satisfied")
        );
        assert_eq!(
            main.mse_dependencies[1].resolution_status.as_deref(),
            Some("missing")
        );
        assert!(main.mse_dependencies[1].resolved_path.is_none());
        let incomplete = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "absent.mse-style")
            .unwrap();
        assert_eq!(incomplete.mse_main_file_present, Some(false));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn marks_mse_dependency_ambiguous_when_multiple_local_packages_match() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-ambiguous-test-{}",
            std::process::id()
        ));
        let game_folder = root.join("magic.mse-game");
        let duplicate = root.join("duplicate");
        let set = root.join("sample.mse-set");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&game_folder).unwrap();
        std::fs::create_dir_all(&duplicate).unwrap();
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(game_folder.join("game"), b"mse version: 0.3.4\n").unwrap();
        let archive = File::create(duplicate.join("magic.mse-game")).unwrap();
        let mut writer = zip::ZipWriter::new(archive);
        writer
            .start_file("game", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer.finish().unwrap();
        std::fs::write(
            set.join("set"),
            b"mse version: 0.3.4\ndepends on: magic.mse-game\n",
        )
        .unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(
            main.mse_dependencies[0].resolution_status.as_deref(),
            Some("ambiguous")
        );
        assert!(main.mse_dependencies[0].resolved_path.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shares_one_deterministic_mse_zip_hash_budget_across_multiple_sources() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-multi-budget-{}",
            std::process::id()
        ));
        let first = root.join("first");
        let second = root.join("second");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        for (directory, package_name) in [(&first, "a.mse-set"), (&second, "b.mse-set")] {
            let file = File::create(directory.join(package_name)).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("set", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"mse version: 0.3.4\n").unwrap();
            writer
                .start_file("cards/image.bin", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"1234").unwrap();
            writer.finish().unwrap();
        }

        let roots = vec![first, second];
        let mut scan_budget = 4_u64;
        let inventory = scan_multiple_directories_with_progress_and_cancel_with_budget(
            &roots,
            |_, _| {},
            &AtomicBool::new(false),
            &mut scan_budget,
        )
        .unwrap();
        let first_package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "source-0001/a.mse-set")
            .unwrap();
        let second_package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "source-0002/b.mse-set")
            .unwrap();
        assert_eq!(first_package.mse_resources[0].hash_status, "hashed");
        assert_eq!(second_package.mse_resources[0].hash_status, "scan-budget");
        assert_eq!(scan_budget, 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_mse_dependencies_across_multiple_selected_sources() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-multi-resolve-{}",
            std::process::id()
        ));
        let first = root.join("first");
        let second = root.join("second");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(first.join("magic.mse-game")).unwrap();
        std::fs::create_dir_all(second.join("sample.mse-set")).unwrap();
        std::fs::write(first.join("magic.mse-game/game"), b"mse version: 0.3.4\n").unwrap();
        std::fs::write(
            second.join("sample.mse-set/set"),
            b"mse version: 0.3.4\ndepends on: magic.mse-game\n",
        )
        .unwrap();
        let inventory = scan_multiple_directories_with_progress_and_cancel(
            &[first, second],
            |_, _| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "source-0002/sample.mse-set/set")
            .unwrap();
        assert_eq!(
            main.mse_dependencies[0].resolution_status.as_deref(),
            Some("resolved")
        );
        assert_eq!(
            main.mse_dependencies[0].resolved_path.as_deref(),
            Some("source-0001/magic.mse-game/game")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn indexes_folder_mse_resources_with_existing_sha256_without_nested_package_leakage() {
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-resources-{}", std::process::id()));
        let package = root.join("sample.mse-set");
        let nested = package.join("nested.mse-game");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(package.join("cards")).unwrap();
        std::fs::create_dir_all(nested.join("cards")).unwrap();
        std::fs::write(
            package.join("set"),
            b"mse version: 0.3.4\nshort name: Resource Set\n",
        )
        .unwrap();
        std::fs::write(package.join("cards/example.png"), b"outer resource").unwrap();
        std::fs::write(
            nested.join("game"),
            b"mse version: 0.3.4\nname: Nested Game\n",
        )
        .unwrap();
        std::fs::write(nested.join("cards/nested.png"), b"nested resource").unwrap();

        let inventory = scan_directory(&root).unwrap();
        let set = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(set.mse_resources.len(), 1);
        assert_eq!(set.mse_resources[0].relative_path, "cards/example.png");
        assert_eq!(set.mse_resources[0].size_bytes, 14);
        let outer_asset = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/cards/example.png")
            .unwrap();
        assert_eq!(
            set.mse_resources[0].sha256.as_deref(),
            Some(outer_asset.sha256.as_str())
        );
        assert_eq!(set.mse_resources[0].hash_status, "hashed");

        let game = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/nested.mse-game/game")
            .unwrap();
        assert_eq!(game.mse_resources.len(), 1);
        assert_eq!(game.mse_resources[0].relative_path, "cards/nested.png");
        assert!(!set
            .mse_resources
            .iter()
            .any(|resource| resource.relative_path.starts_with("nested.mse-game/")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_mse_folder_package_main_file_without_following_includes() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-folder-test-{}",
            std::process::id()
        ));
        let package = root.join("sample.mse-set");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(package.join("cards")).unwrap();
        std::fs::write(
            package.join("set"),
            b"mse version: 0.3.4\nshort name: Folder Set\ninclude file: ../outside.txt\n",
        )
        .unwrap();
        std::fs::write(package.join("cards/example.txt"), b"resource").unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set/set")
            .unwrap();
        assert_eq!(main.mse_package_type.as_deref(), Some("set"));
        assert_eq!(main.mse_main_file_present, Some(true));
        assert_eq!(main.mse_version.as_deref(), Some("0.3.4"));
        assert_eq!(main.mse_short_name.as_deref(), Some("Folder Set"));
        assert_eq!(main.mse_has_include_directives, Some(true));
        assert!(!inventory
            .assets
            .iter()
            .any(|asset| asset.mse_package_type.is_some()
                && asset.relative_path == "sample.mse-set/cards/example.txt"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_mse_header_indentation_with_spaces_or_mixed_whitespace() {
        assert!(parse_mse_header(b"mse version: 0.3.4\n short name: Invalid\n").is_err());
        assert!(parse_mse_header(b"mse version: 0.3.4\n\t short name: Invalid\n").is_err());
        assert!(
            parse_mse_header(b"mse version: 0.3.4\ndepends on:\n package: invalid.mse-game\n")
                .is_err()
        );
    }

    #[test]
    fn parses_compact_and_block_mse_dependencies() {
        let source = b"mse version: 0.3.4\ndepends on: magic.mse-game 2007-06-06\ndepends on:\n\tpackage: shared.mse-style\n\tversion: 1.2.3\n";
        let header = parse_mse_header(source).unwrap();
        assert_eq!(header.dependencies.len(), 2);
        assert_eq!(header.dependencies[0].package, "magic.mse-game");
        assert_eq!(
            header.dependencies[0].min_version.as_deref(),
            Some("2007-06-06")
        );
        assert_eq!(header.dependencies[1].package, "shared.mse-style");
        assert_eq!(header.dependencies[1].min_version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn reads_bounded_mse_header_fields_without_following_includes() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-header-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(
            b"mse version: 0.3.4\nshort name: My Set\ngame: magic\nstylesheet: new\ninclude file: nested.txt\ncard:\n\tname: ignored\n"
        ).unwrap();
        writer.finish().unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let metadata = zip_metadata(&workspace, Path::new("sample.mse-set"), "mse-set");
        let header = metadata.mse_header.unwrap();
        assert_eq!(header.mse_version.as_deref(), Some("0.3.4"));
        assert_eq!(header.short_name.as_deref(), Some("My Set"));
        assert_eq!(header.game.as_deref(), Some("magic"));
        assert_eq!(header.stylesheet.as_deref(), Some("new"));
        assert_eq!(header.has_include_directives, Some(true));
        assert!(
            !root.join("set").exists(),
            "ZIP contents must never be extracted"
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_bounded_prefix_of_large_mse_main_file() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-large-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-game");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("game", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(b"mse version: 0.3.4\nshort name: Large Game\n")
            .unwrap();
        writer.write_all(&vec![b'x'; 1024 * 1024 + 1]).unwrap();
        writer.finish().unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let metadata = zip_metadata(&workspace, Path::new("sample.mse-game"), "mse-game");
        let header = metadata.mse_header.unwrap();
        assert_eq!(header.mse_version.as_deref(), Some("0.3.4"));
        assert_eq!(header.short_name.as_deref(), Some("Large Game"));
        assert!(metadata.warning.unwrap().contains("préfixe de 1 Mio"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_corrupt_mse_zip_without_panicking_or_indexing_resources() {
        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-corrupt-zip-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("sample.mse-set"), b"this is not a ZIP archive").unwrap();

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        assert_eq!(package.mse_package_type.as_deref(), Some("set"));
        assert!(package.analysis_warning.is_some());
        assert!(package.archive_entry_count.is_none());
        assert!(package.mse_resources.is_empty());
        assert!(!package.mse_resources_truncated);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_truncated_mse_zip_central_directory_without_indexing_resources() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-truncated-central-directory-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file("cards/image.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"resource payload").unwrap();
        writer.finish().unwrap();

        let mut bytes = std::fs::read(&archive_path).unwrap();
        assert!(bytes.len() > 8);
        bytes.truncate(bytes.len() - 8);
        std::fs::write(&archive_path, bytes).unwrap();

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        assert_eq!(package.mse_package_type.as_deref(), Some("set"));
        assert!(package.analysis_warning.is_some());
        assert!(package.mse_main_file_present.is_none());
        assert!(package.mse_resources.is_empty());
        assert!(!package.mse_resources_truncated);
        assert!(!root.join("cards/image.bin").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_truncated_mse_zip_deflate_payload_without_hashing_resource() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-truncated-deflate-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let payload = b"DEFLATE-TRUNCATION-PROBE-".repeat(256);
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file(
                "cards/truncated-deflate.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(&payload).unwrap();
        writer.finish().unwrap();

        let mut bytes = std::fs::read(&archive_path).unwrap();
        let resource_name = b"cards/truncated-deflate.bin";
        let name_offset = bytes
            .windows(resource_name.len())
            .position(|window| window == resource_name)
            .expect("local resource filename");
        let local_header = name_offset - 30;
        let name_length =
            u16::from_le_bytes([bytes[local_header + 26], bytes[local_header + 27]]) as usize;
        let extra_length =
            u16::from_le_bytes([bytes[local_header + 28], bytes[local_header + 29]]) as usize;
        assert_eq!(name_length, resource_name.len());
        let data_start = local_header + 30 + name_length + extra_length;
        let compressed_size = u32::from_le_bytes(
            bytes[local_header + 18..local_header + 22]
                .try_into()
                .unwrap(),
        ) as usize;
        assert!(compressed_size > 8);
        let removed_bytes = 4;
        let truncate_at = data_start + compressed_size - removed_bytes;
        bytes.drain(truncate_at..truncate_at + removed_bytes);
        let eocd = bytes
            .windows(4)
            .rposition(|window| window == b"PK\x05\x06")
            .expect("ZIP end-of-central-directory record");
        let central_offset = u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap());
        bytes[eocd + 16..eocd + 20]
            .copy_from_slice(&(central_offset - removed_bytes as u32).to_le_bytes());
        std::fs::write(&archive_path, bytes).unwrap();

        let archive = zip::ZipArchive::new(File::open(&archive_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        drop(archive);

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        assert_eq!(package.mse_resources.len(), 1);
        assert_eq!(
            package.mse_resources[0].relative_path,
            "cards/truncated-deflate.bin"
        );
        assert_eq!(package.mse_resources[0].hash_status, "read-error");
        assert!(package.mse_resources[0].sha256.is_none());
        assert!(!root.join("cards/truncated-deflate.bin").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_truncated_mse_zip_local_payload_without_hashing_resource() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-truncated-local-data-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "set",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        let resource_name = b"cards/truncated.bin";
        let payload = b"local-payload-that-will-be-truncated";
        writer
            .start_file(
                "cards/truncated.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();

        let mut bytes = std::fs::read(&archive_path).unwrap();
        let name_offset = bytes
            .windows(resource_name.len())
            .position(|window| window == resource_name)
            .expect("local resource filename");
        let local_header = name_offset - 30;
        let name_length =
            u16::from_le_bytes([bytes[local_header + 26], bytes[local_header + 27]]) as usize;
        let extra_length =
            u16::from_le_bytes([bytes[local_header + 28], bytes[local_header + 29]]) as usize;
        assert_eq!(name_length, resource_name.len());
        let data_start = local_header + 30 + name_length + extra_length;
        let declared_size = u32::from_le_bytes(
            bytes[local_header + 18..local_header + 22]
                .try_into()
                .unwrap(),
        ) as usize;
        assert_eq!(declared_size, payload.len());
        let removed_bytes = 4;
        let truncate_at = data_start + declared_size - removed_bytes;
        bytes.drain(truncate_at..truncate_at + removed_bytes);
        let eocd = bytes
            .windows(4)
            .rposition(|window| window == b"PK\x05\x06")
            .expect("ZIP end-of-central-directory record");
        let central_offset = u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap());
        bytes[eocd + 16..eocd + 20]
            .copy_from_slice(&(central_offset - removed_bytes as u32).to_le_bytes());
        std::fs::write(&archive_path, bytes).unwrap();

        let archive = zip::ZipArchive::new(File::open(&archive_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        drop(archive);

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        assert_eq!(package.mse_resources.len(), 1);
        assert_eq!(
            package.mse_resources[0].relative_path,
            "cards/truncated.bin"
        );
        assert_eq!(package.mse_resources[0].hash_status, "read-error");
        assert!(package.mse_resources[0].sha256.is_none());
        assert!(!root.join("cards/truncated.bin").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reports_mse_zip_with_invalid_central_directory_offset_without_indexing_resources() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-invalid-central-directory-offset-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file("cards/image.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"resource payload").unwrap();
        writer.finish().unwrap();

        let mut bytes = std::fs::read(&archive_path).unwrap();
        let eocd = bytes
            .windows(4)
            .rposition(|window| window == b"PK\x05\x06")
            .expect("ZIP end-of-central-directory record");
        bytes[eocd + 16..eocd + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        std::fs::write(&archive_path, bytes).unwrap();

        let inventory = scan_directory(&root).unwrap();
        let package = inventory
            .assets
            .iter()
            .find(|asset| asset.relative_path == "sample.mse-set")
            .unwrap();
        assert_eq!(package.mse_package_type.as_deref(), Some("set"));
        assert!(package.analysis_warning.is_some());
        assert!(package.archive_entry_count.is_none());
        assert!(package.mse_main_file_present.is_none());
        assert!(package.mse_resources.is_empty());
        assert!(!package.mse_resources_truncated);
        assert!(!root.join("cards/image.bin").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn marks_mse_zip_resource_with_inconsistent_declared_size_as_size_mismatch() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-size-mismatch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let payload = b"declared-size-mismatch";
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file(
                "cards/mismatch.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();

        // Modifier uniquement la taille décompressée déclarée dans la table
        // centrale : les octets stockés et l'en-tête local restent inchangés.
        let mut bytes = std::fs::read(&archive_path).unwrap();
        let filename = b"cards/mismatch.bin";
        let filename_offset = bytes
            .windows(filename.len())
            .rposition(|window| window == filename)
            .expect("nom de ressource dans la table centrale");
        let central_header = filename_offset - 46;
        assert_eq!(&bytes[central_header..central_header + 4], b"PK\x01\x02");
        let declared_size = u32::from_le_bytes([
            bytes[central_header + 24],
            bytes[central_header + 25],
            bytes[central_header + 26],
            bytes[central_header + 27],
        ]);
        assert_eq!(declared_size as usize, payload.len());
        bytes[central_header + 24..central_header + 28]
            .copy_from_slice(&((payload.len() + 1) as u32).to_le_bytes());
        std::fs::write(&archive_path, bytes).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut scan_budget = 1024;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 1024,
                max_total_hash_bytes: 1024,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );

        assert!(!truncated);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].relative_path, "cards/mismatch.bin");
        assert_eq!(resources[0].hash_status, "size-mismatch");
        assert!(resources[0].sha256.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn marks_mse_zip_resource_with_actual_size_larger_than_declared_as_size_mismatch() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-size-understated-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let payload = b"actual-resource-payload";
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file(
                "cards/understated.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();

        // La table centrale annonce un octet de moins que les données réellement présentes.
        let mut bytes = std::fs::read(&archive_path).unwrap();
        let filename = b"cards/understated.bin";
        let filename_offset = bytes
            .windows(filename.len())
            .rposition(|window| window == filename)
            .expect("nom de ressource dans la table centrale");
        let central_header = filename_offset - 46;
        assert_eq!(&bytes[central_header..central_header + 4], b"PK\x01\x02");
        bytes[central_header + 24..central_header + 28]
            .copy_from_slice(&((payload.len() - 1) as u32).to_le_bytes());
        std::fs::write(&archive_path, bytes).unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut scan_budget = 1024;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 1024,
                max_total_hash_bytes: 1024,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );

        assert!(!truncated);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].relative_path, "cards/understated.bin");
        assert_eq!(resources[0].hash_status, "size-mismatch");
        assert!(resources[0].sha256.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_and_hashes_mse_zip_resource_with_unusual_utf8_name() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "mtg-auditor-mse-unicode-name-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let resource_name = "cards/café-🃏.bin";
        let payload = b"unicode resource payload";
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file(
                resource_name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();

        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let mut scan_budget = 1024;
        let (resources, truncated) = mse_zip_resources_with_limits_and_cancel(
            &workspace,
            Path::new("sample.mse-set"),
            "set",
            MseZipResourceLimits {
                max_resources: 10,
                max_resource_bytes: 1024,
                max_total_hash_bytes: 1024,
            },
            &mut scan_budget,
            &AtomicBool::new(false),
        );

        assert!(!truncated);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].relative_path, resource_name);
        assert_eq!(resources[0].hash_status, "hashed");
        assert!(resources[0].sha256.is_some());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_mse_zip_package_missing_its_main_file() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-game-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-game");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "resources/icon.png",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"fixture").unwrap();
        writer.finish().unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let metadata = zip_metadata(&workspace, Path::new("sample.mse-game"), "mse-game");
        assert_eq!(metadata.mse_main_file_present, Some(false));
        assert!(metadata.warning.unwrap().contains("game"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_early_v1_inventory_without_new_optional_fields() {
        let legacy = r#"{
            "schema_version": 1,
            "source_root": "C:/assets",
            "generated_unix_seconds": 1,
            "files_scanned": 1,
            "total_bytes": 5,
            "assets": [{
                "id": "old-id",
                "relative_path": "sample.txt",
                "file_name": "sample.txt",
                "extension": "txt",
                "size_bytes": 5,
                "modified_unix_seconds": null,
                "sha256": "abc",
                "detected_family": "text",
                "review_status": "needs-review",
                "mse_dependencies": [{"package": "legacy.mse-game", "min_version": "1.0"}]
            }]
        }"#;
        let inventory: InventoryDocument = serde_json::from_str(legacy).unwrap();
        assert_eq!(inventory.duplicate_files, 0);
        assert_eq!(inventory.assets[0].raster_width, None);
        assert_eq!(inventory.assets[0].raster_orientation, None);
        assert_eq!(inventory.assets[0].notice_excerpt, None);
        assert!(!inventory.assets[0].notice_excerpt_truncated);
        assert_eq!(inventory.assets[0].duplicate_of, None);
        assert_eq!(
            inventory.assets[0].mse_dependencies[0].resolution_status,
            None
        );
        assert_eq!(inventory.assets[0].mse_dependencies[0].resolved_path, None);
        assert_eq!(inventory.assets[0].mse_dependencies[0].version_status, None);
        assert_eq!(inventory.assets[0].mse_dependencies[0].cycle_detected, None);
        assert_eq!(inventory.assets[0].mse_name, None);
    }

    #[test]
    fn compares_added_removed_changed_and_unchanged_files() {
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-compare-test-{}", std::process::id()));
        let baseline_root = base.join("baseline");
        let current_root = base.join("current");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&baseline_root).unwrap();
        std::fs::create_dir_all(&current_root).unwrap();
        std::fs::write(baseline_root.join("same.txt"), b"same").unwrap();
        std::fs::write(baseline_root.join("changed.txt"), b"before").unwrap();
        std::fs::write(baseline_root.join("removed.txt"), b"gone").unwrap();
        std::fs::write(current_root.join("same.txt"), b"same").unwrap();
        std::fs::write(current_root.join("changed.txt"), b"after").unwrap();
        std::fs::write(current_root.join("added.txt"), b"new").unwrap();
        let baseline = scan_directory(&baseline_root).unwrap();
        let current = scan_directory(&current_root).unwrap();
        let comparison = compare_inventories(&current, &baseline);
        assert_eq!(comparison.added_paths, vec!["added.txt"]);
        assert_eq!(comparison.removed_paths, vec!["removed.txt"]);
        assert_eq!(comparison.changed_assets.len(), 1);
        assert_eq!(comparison.unchanged_files, 1);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn contact_sheet_exports_png_and_safe_index() {
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-sheet-test-{}", std::process::id()));
        let source = base.join("source");
        let output = base.join("output");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&output).unwrap();
        let source_image = source.join("card.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 40, 60, 128]))
            .save(&source_image)
            .unwrap();
        let document = scan_directory(&source).unwrap();
        let sheet_path = output.join("sheet.png");
        let rendered = export_contact_sheet(&document, 0, &sheet_path).unwrap();
        assert_eq!(rendered, 1);
        assert!(sheet_path.exists());
        assert!(output.join("sheet.txt").exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn contact_sheet_applies_jpeg_exif_orientation() {
        let base = std::env::temp_dir().join(format!(
            "mtg-auditor-sheet-orientation-{}",
            std::process::id()
        ));
        let source = base.join("source");
        let output = base.join("output");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&output).unwrap();

        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode(
                &[255, 0, 0, 0, 0, 255],
                2,
                1,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(b"II");
        exif.extend_from_slice(&42_u16.to_le_bytes());
        exif.extend_from_slice(&8_u32.to_le_bytes());
        exif.extend_from_slice(&1_u16.to_le_bytes());
        exif.extend_from_slice(&0x0112_u16.to_le_bytes());
        exif.extend_from_slice(&3_u16.to_le_bytes());
        exif.extend_from_slice(&1_u32.to_le_bytes());
        exif.extend_from_slice(&6_u16.to_le_bytes());
        exif.extend_from_slice(&0_u16.to_le_bytes());
        exif.extend_from_slice(&0_u32.to_le_bytes());
        let segment_length = u16::try_from(exif.len() + 2).unwrap();
        let mut with_exif = jpeg[..2].to_vec();
        with_exif.extend_from_slice(&[0xff, 0xe1]);
        with_exif.extend_from_slice(&segment_length.to_be_bytes());
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);
        std::fs::write(source.join("oriented.jpg"), with_exif).unwrap();

        let document = scan_directory(&source).unwrap();
        let sheet_path = output.join("sheet.png");
        assert_eq!(export_contact_sheet(&document, 0, &sheet_path).unwrap(), 1);

        let sheet = image::open(&sheet_path).unwrap().to_rgba8();
        let background = image::Rgba([245, 246, 248, 255]);
        let mut bounds = (u32::MAX, u32::MAX, 0_u32, 0_u32);
        for y in 10..170 {
            for x in 10..210 {
                if *sheet.get_pixel(x, y) != background {
                    bounds.0 = bounds.0.min(x);
                    bounds.1 = bounds.1.min(y);
                    bounds.2 = bounds.2.max(x);
                    bounds.3 = bounds.3.max(y);
                }
            }
        }
        assert!(bounds.0 != u32::MAX, "contact sheet image was not rendered");
        let rendered_width = bounds.2 - bounds.0;
        let rendered_height = bounds.3 - bounds.1;
        assert!(
            rendered_height > rendered_width,
            "EXIF Rotate90 should render a portrait thumbnail, got {}x{}",
            rendered_width,
            rendered_height
        );

        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn mse_zip_indexing_rejects_archive_changed_after_scan() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("mtg-auditor-mse-mutation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file("cards/image.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"original").unwrap();
        writer.finish().unwrap();

        let inventory = scan_directory(&root).unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("set", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"mse version: 0.3.4\n").unwrap();
        writer
            .start_file("cards/image.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"replaced").unwrap();
        writer.finish().unwrap();

        let error = verify_mse_zip_sources_unchanged(&inventory.assets, &workspace).unwrap_err();
        assert!(error.to_string().contains("modifiée"));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn contact_sheet_rejects_source_changed_after_scan() {
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-sheet-mutation-{}", std::process::id()));
        let source = base.join("source");
        let output = base.join("output");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&output).unwrap();
        let source_image = source.join("card.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 40, 60, 128]))
            .save(&source_image)
            .unwrap();
        let document = scan_directory(&source).unwrap();
        image::RgbaImage::from_pixel(9, 9, image::Rgba([60, 40, 20, 255]))
            .save(&source_image)
            .unwrap();
        let sheet_path = output.join("sheet.png");
        let rendered = export_contact_sheet(&document, 0, &sheet_path).unwrap();
        assert_eq!(rendered, 0);
        let index = std::fs::read_to_string(output.join("sheet.txt")).unwrap();
        assert!(index.contains("SOURCE MODIFIÉE DEPUIS LE SCAN"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn scans_multiple_roots_without_path_collisions_and_detects_cross_root_duplicates() {
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-multi-test-{}", std::process::id()));
        let first = base.join("first");
        let second = base.join("second");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("same.txt"), b"shared").unwrap();
        std::fs::write(second.join("same.txt"), b"shared").unwrap();
        let roots = vec![first.clone(), second.clone()];
        let document = scan_multiple_directories_with_progress_and_cancel(
            &roots,
            |_, _| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(document.source_roots.len(), 2);
        assert_eq!(document.files_scanned, 2);
        assert_ne!(
            document.assets[0].relative_path,
            document.assets[1].relative_path
        );
        assert_eq!(document.duplicate_files, 1);
        assert!(document
            .assets
            .iter()
            .all(|asset| asset.relative_path.starts_with("source-000")));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rejects_nested_multi_source_roots() {
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-multi-nested-{}", std::process::id()));
        let child = base.join("child");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&child).unwrap();
        let result = scan_multiple_directories_with_progress_and_cancel(
            &[base.clone(), child],
            |_, _| {},
            &AtomicBool::new(false),
        );
        assert!(result.unwrap_err().to_string().contains("chevauchent"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn refuses_comparison_export_inside_any_multi_source_root() {
        let base =
            std::env::temp_dir().join(format!("mtg-auditor-multi-export-{}", std::process::id()));
        let baseline = base.join("baseline");
        let current_a = base.join("current-a");
        let current_b = base.join("current-b");
        std::fs::create_dir_all(&baseline).unwrap();
        std::fs::create_dir_all(&current_a).unwrap();
        std::fs::create_dir_all(&current_b).unwrap();
        let comparison = InventoryComparison {
            schema_version: 1,
            baseline_source_root: baseline.canonicalize().unwrap().display().to_string(),
            current_source_root: current_a.canonicalize().unwrap().display().to_string(),
            baseline_source_roots: vec![baseline.canonicalize().unwrap().display().to_string()],
            current_source_roots: vec![
                current_a.canonicalize().unwrap().display().to_string(),
                current_b.canonicalize().unwrap().display().to_string(),
            ],
            added_paths: Vec::new(),
            removed_paths: Vec::new(),
            changed_assets: Vec::new(),
            unchanged_files: 0,
        };
        assert!(export_comparison_json(&comparison, &current_b.join("comparison.json")).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn html_escaping_blocks_markup_in_user_paths() {
        assert_eq!(
            escape_html("<script>&\"'"),
            "&lt;script&gt;&amp;&quot;&#39;"
        );
    }

    #[test]
    fn refuses_export_inside_source_tree() {
        let root: PathBuf =
            std::env::temp_dir().join(format!("mtg-auditor-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let document = InventoryDocument {
            schema_version: INVENTORY_SCHEMA_VERSION,
            source_root: root.canonicalize().unwrap().display().to_string(),
            generated_unix_seconds: 0,
            files_scanned: 0,
            total_bytes: 0,
            duplicate_files: 0,
            source_roots: vec![root.canonicalize().unwrap().display().to_string()],
            assets: Vec::new(),
        };
        assert!(ensure_export_outside_source(&document, &root.join("inventory.json")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
