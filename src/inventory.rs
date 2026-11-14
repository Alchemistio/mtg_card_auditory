use anyhow::{Context, Result};
use cap_std::{ambient_authority, fs::{Dir as CapabilityDir, OpenOptions as CapabilityOpenOptions}};
use image::{ImageDecoder, ImageReader};
use quick_xml::{events::Event, Reader};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io::{BufReader, Read, Write}, path::{Path, PathBuf}, sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}, time::UNIX_EPOCH};


pub const INVENTORY_SCHEMA_VERSION: u32 = 1;

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
    pub svg_viewbox: Option<String>,
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
    pub mse_stylesheet: Option<String>,
    #[serde(default)]
    pub mse_has_include_directives: Option<bool>,
    #[serde(default)]
    pub mse_dependencies: Vec<MseDependency>,
    #[serde(default)]
    pub analysis_warning: Option<String>,
    #[serde(default)]
    pub notice_kind: Option<String>,
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

fn collect_files_capability(
    workspace: &CapabilityDir,
    directory: &Path,
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
    for entry in workspace.read_dir(directory)
        .with_context(|| format!("Énumération capability-based impossible : {}", directory.display()))? {
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("Scan annulé par l'utilisateur.");
        }
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let relative = directory.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files_capability(workspace, &relative, depth + 1, paths, cancelled)?;
        } else if file_type.is_file() {
            paths.push(relative);
        }
    }
    Ok(())
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
    warning: Option<String>,
}

fn raster_metadata(workspace: &CapabilityDir, relative: &Path, family: &str) -> RasterMetadata {
    if family != "raster" {
        return RasterMetadata::default();
    }
    let reader = match workspace.open(relative).map(BufReader::new).map(ImageReader::new).and_then(|reader| reader.with_guessed_format()) {
        Ok(reader) => reader,
        Err(error) => return RasterMetadata {
            warning: Some(format!("Détection du format image impossible : {}", error)),
            ..RasterMetadata::default()
        },
    };
    let format = reader.format().map(|value| format!("{value:?}"));
    let decoder = match reader.into_decoder() {
        Ok(decoder) => decoder,
        Err(error) => return RasterMetadata {
            format,
            warning: Some(format!("En-tête image illisible : {}", error)),
            ..RasterMetadata::default()
        },
    };
    let (width, height) = decoder.dimensions();
    let color_type = decoder.color_type();
    RasterMetadata {
        format,
        width: Some(width),
        height: Some(height),
        color_type: Some(format!("{color_type:?}")),
        has_alpha: Some(color_type.has_alpha()),
        warning: None,
    }
}

fn read_limited<R: Read>(reader: R, maximum_bytes: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(maximum_bytes.saturating_add(1)).read_to_end(&mut bytes)
        .context("Lecture du fichier limitée impossible")?;
    if bytes.len() as u64 > maximum_bytes {
        anyhow::bail!("Fichier supérieur à la limite de lecture de {} octets.", maximum_bytes);
    }
    Ok(bytes)
}

fn read_prefix<R: Read>(reader: R, maximum_bytes: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(maximum_bytes).read_to_end(&mut bytes)
        .context("Lecture du préfixe limitée impossible")?;
    Ok(bytes)
}

fn svg_metadata(workspace: &CapabilityDir, relative: &Path, family: &str) -> (Option<String>, Option<bool>, Option<bool>, Option<String>) {
    if family != "svg" {
        return (None, None, None, None);
    }
    const MAX_SVG_BYTES: u64 = 32 * 1024 * 1024;
    let file = match workspace.open(relative) {
        Ok(file) => file,
        Err(error) => return (None, None, Some(false), Some(format!("Ouverture SVG impossible : {}", error))),
    };
    let bytes = match read_limited(file, MAX_SVG_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => return (None, None, None, Some(format!("Lecture SVG impossible ou supérieure à 32 Mio : {}", error))),
    };
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => return (None, None, Some(false), Some(format!("SVG non UTF-8 : {}", error))),
    };
    let mut reader = Reader::from_str(&source);
    reader.config_mut().trim_text(true);
    let mut viewbox = None;
    let mut external_refs = false;
    let mut found_svg_root = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if element.name().as_ref() == b"svg" {
                    found_svg_root = true;
                }
                for attribute in element.attributes() {
                    let attribute = match attribute {
                        Ok(attribute) => attribute,
                        Err(error) => return (viewbox, Some(external_refs), Some(false), Some(format!("Attribut SVG invalide : {}", error))),
                    };
                    let key = attribute.key.as_ref();
                    let value = match attribute.decode_and_unescape_value(reader.decoder()) {
                        Ok(value) => value.into_owned(),
                        Err(error) => return (viewbox, Some(external_refs), Some(false), Some(format!("Valeur d'attribut SVG invalide : {}", error))),
                    };
                    if key == b"viewBox" && viewbox.is_none() {
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
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return (viewbox, Some(external_refs), Some(false), Some(format!("XML SVG invalide : {}", error))),
        }
    }
    if !found_svg_root {
        return (viewbox, Some(external_refs), Some(false), Some("Élément racine SVG absent.".to_string()));
    }
    if contains_external_css_reference(&source) {
        external_refs = true;
    }
    (viewbox, Some(external_refs), Some(true), None)
}

fn font_metadata(workspace: &CapabilityDir, relative: &Path, family: &str) -> (Option<String>, Option<String>, Option<String>) {
    if family != "font" {
        return (None, None, None);
    }
    let size = match workspace.metadata(relative) {
        Ok(metadata) => metadata.len(),
        Err(error) => return (None, None, Some(format!("Métadonnées de police indisponibles : {}", error))),
    };
    if size > 64 * 1024 * 1024 {
        return (None, None, Some("Police ignorée : taille supérieure à 64 Mio.".to_string()));
    }
    const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
    let file = match workspace.open(relative) {
        Ok(file) => file,
        Err(error) => return (None, None, Some(format!("Ouverture de police impossible : {}", error))),
    };
    let data = match read_limited(file, MAX_FONT_BYTES) {
        Ok(data) => data,
        Err(error) => return (None, None, Some(format!("Lecture de police impossible ou supérieure à 64 Mio : {}", error))),
    };
    let face = match ttf_parser::Face::parse(&data, 0) {
        Ok(face) => face,
        Err(error) => return (None, None, Some(format!("Format de police non pris en charge ou invalide : {}", error))),
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
    let value = value.trim().to_ascii_lowercase();
    value.starts_with("http://") || value.starts_with("https://") || value.starts_with("//")
}

fn contains_external_css_reference(value: &str) -> bool {
    let value = value.to_ascii_lowercase().replace(' ', "");
    value.contains("url(http://") || value.contains("url(https://") || value.contains("url(//")
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
        });
    } else {
        version.take();
    }
    Ok(())
}

#[derive(Default)]
struct MseHeader {
    mse_version: Option<String>,
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
            anyhow::bail!("Ligne d'en-tête MSE supérieure à {} octets.", MAX_MSE_LINE_BYTES);
        }
        let unindented = line.trim_start_matches(['\t', ' ']);
        if unindented.starts_with("include file:") {
            has_include_directives = true;
        }
        let indentation = line.chars().take_while(|character| *character == '\t').count();
        if line.starts_with('\t') || line.starts_with(' ') {
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

fn zip_metadata(workspace: &CapabilityDir, relative: &Path, extension: &str) -> ZipMetadata {
    let expected_main = mse_package_type(extension);
    if extension != "zip" && expected_main.is_none() {
        if extension == "7z" || extension == "rar" {
            return ZipMetadata::new(
                None, None, None,
                Some("Inspection de cette archive non prise en charge pour le moment.".to_string()),
                None,
            );
        }
        return ZipMetadata::default();
    }
    let archive_size = match workspace.metadata(relative) {
        Ok(metadata) => metadata.len(),
        Err(error) => return ZipMetadata::new(
            None, None, None, Some(format!("Métadonnées ZIP indisponibles : {}", error)), None
        ),
    };
    if archive_size > 512 * 1024 * 1024 {
        return ZipMetadata::new(
            None, None, None, Some("Archive ignorée : fichier ZIP supérieur à 512 Mio.".to_string()), None
        );
    }
    let file = match workspace.open(relative) {
        Ok(file) => file,
        Err(error) => return ZipMetadata::new(
            None, None, None, Some(format!("Ouverture ZIP impossible : {}", error)), None
        ),
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => return ZipMetadata::new(
            None, None, None, Some(format!("Structure ZIP invalide : {}", error)), None
        ),
    };
    let entry_count = archive.len();
    if entry_count > 200_000 {
        return ZipMetadata::new(
            Some(entry_count), None, None,
            Some("Archive ignorée : plus de 200 000 entrées.".to_string()), None
        );
    }
    let mut uncompressed_bytes = 0_u64;
    let mut suspicious_paths = 0_usize;
    let mut suspicious_ratio = false;
    let mut mse_main_file_present = expected_main.map(|_| false);
    for index in 0..archive.len() {
        let entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => return ZipMetadata::new(
                Some(entry_count), None, Some(suspicious_paths),
                Some(format!("Entrée ZIP illisible : {}", error)), mse_main_file_present
            ),
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
                Some(entry_count), Some(uncompressed_bytes), Some(suspicious_paths),
                Some("Alerte ZIP : taille décompressée déclarée supérieure à 8 Gio.".to_string()),
                mse_main_file_present
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
                                if let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') {
                                    bytes.truncate(last_newline + 1);
                                } else {
                                    bytes.clear();
                                    mse_header_warning = Some(
                                        "Aucune ligne complète dans le préfixe MSE autorisé.".to_string()
                                    );
                                }
                            }
                            if !bytes.is_empty() {
                                match parse_mse_header(&bytes) {
                                    Ok(header) => {
                                        if header.mse_version.is_none() {
                                            mse_header_warning = Some("Version MSE absente de l'en-tête du package.".to_string());
                                        }
                                        mse_header = Some(header);
                                    }
                                    Err(error) => {
                                        mse_header_warning = Some(format!("Lecture de l'en-tête MSE impossible : {}", error));
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
                            mse_header_warning = Some(format!("Lecture de l'en-tête MSE impossible : {}", error));
                        }
                    }
                }
                Err(error) => {
                    mse_header_warning = Some(format!("Fichier principal MSE illisible : {}", error));
                }
            }
        }
    }
    let warning = if suspicious_paths > 0 {
        Some("Archive ZIP contenant des chemins suspects ; aucune extraction n'a été effectuée.".to_string())
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
        Some(entry_count), Some(uncompressed_bytes), Some(suspicious_paths), warning, mse_main_file_present
    );
    metadata.mse_header = mse_header;
    metadata
}


fn mse_folder_package_extension(relative: &Path) -> Option<String> {
    let directory_name = relative.parent()?.file_name()?.to_str()?;
    let extension = Path::new(directory_name).extension()?.to_str()?.to_ascii_lowercase();
    mse_package_type(&extension).map(|_| extension)
}

fn mse_folder_main_metadata(workspace: &CapabilityDir, relative: &Path) -> ZipMetadata {
    const MAX_MSE_HEADER_BYTES: u64 = 1024 * 1024;
    let size = match workspace.metadata(relative) {
        Ok(metadata) => metadata.len(),
        Err(error) => return ZipMetadata::new(None, None, None,
            Some(format!("Métadonnées du fichier principal MSE indisponibles : {}", error)), Some(true)),
    };
    let file = match workspace.open(relative) {
        Ok(file) => file,
        Err(error) => return ZipMetadata::new(None, None, None,
            Some(format!("Ouverture du fichier principal MSE impossible : {}", error)), Some(true)),
    };
    let mut warning = None;
    let mut header = None;
    match read_prefix(file, MAX_MSE_HEADER_BYTES) {
        Ok(mut bytes) => {
            let truncated = size > MAX_MSE_HEADER_BYTES;
            if truncated {
                if let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') {
                    bytes.truncate(last_newline + 1);
                } else {
                    bytes.clear();
                    warning = Some("Aucune ligne complète dans le préfixe MSE autorisé.".to_string());
                }
            }
            if !bytes.is_empty() {
                match parse_mse_header(&bytes) {
                    Ok(parsed) => {
                        if parsed.mse_version.is_none() {
                            warning = Some("Version MSE absente de l'en-tête du package.".to_string());
                        }
                        header = Some(parsed);
                    }
                    Err(error) => warning = Some(format!("Lecture de l'en-tête MSE impossible : {}", error)),
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

fn sha256_file_capability(workspace: &CapabilityDir, relative: &Path) -> Result<String> {
    let file = workspace.open(relative)
        .with_context(|| format!("Ouverture capability-based impossible : {}", relative.display()))?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 { break; }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn analyze_file(path: &Path, canonical_root: &Path, workspace: &CapabilityDir) -> Result<AssetRecord> {
    let relative = path.strip_prefix(canonical_root)
        .context("Chemin hors du dossier source")?;
    let metadata = workspace.metadata(relative)?;
    let relative_path = relative.to_string_lossy().replace('\\', "/");
    let file_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let extension = path.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let size_bytes = metadata.len();
    let sha256 = sha256_file_capability(workspace, relative)?;
    let modified_unix_seconds = metadata.modified().ok()
        .and_then(|time| time.into_std().duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let detected_family = classify_extension(&extension);
    let raster = raster_metadata(workspace, relative, detected_family);
    let raster_format = raster.format;
    let raster_width = raster.width;
    let raster_height = raster.height;
    let raster_color_type = raster.color_type;
    let raster_has_alpha = raster.has_alpha;
    let (svg_viewbox, svg_has_external_refs, svg_xml_valid, svg_warning) =
        svg_metadata(workspace, relative, detected_family);
    let (font_family, font_subfamily, font_warning) = font_metadata(workspace, relative, detected_family);
    let mut package_metadata = zip_metadata(workspace, relative, &extension);
    let folder_package_extension = mse_folder_package_extension(relative);
    let folder_main_type = folder_package_extension.as_deref().and_then(mse_package_type);
    let is_folder_package_main = folder_main_type.is_some_and(|main_name| file_name == main_name);
    if is_folder_package_main {
        package_metadata = mse_folder_main_metadata(workspace, relative);
    }
    let ZipMetadata {
        entry_count: archive_entry_count,
        uncompressed_bytes: archive_uncompressed_bytes,
        suspicious_paths: archive_suspicious_paths,
        warning: archive_warning,
        mse_main_file_present,
        mse_header,
    } = package_metadata;
    let analysis_warning = raster.warning.or(svg_warning).or(font_warning).or(archive_warning);
    let notice_kind = classify_notice(&file_name);
    let mse_package_type = mse_package_type(&extension)
        .or_else(|| if is_folder_package_main { folder_package_extension.as_deref().and_then(mse_package_type) } else { None })
        .map(str::to_string);
    let mse_version = mse_header.as_ref().and_then(|header| header.mse_version.clone());
    let mse_short_name = mse_header.as_ref().and_then(|header| header.short_name.clone());
    let mse_full_name = mse_header.as_ref().and_then(|header| header.full_name.clone());
    let mse_package_version = mse_header.as_ref().and_then(|header| header.package_version.clone());
    let mse_game = mse_header.as_ref().and_then(|header| header.game.clone());
    let mse_stylesheet = mse_header.as_ref().and_then(|header| header.stylesheet.clone());
    let mse_has_include_directives = mse_header.as_ref().and_then(|header| header.has_include_directives);
    let mse_dependencies = mse_header.as_ref().map_or_else(Vec::new, |header| header.dependencies.clone());
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
        svg_viewbox,
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
        mse_stylesheet,
        mse_has_include_directives,
        mse_dependencies,
        analysis_warning,
        notice_kind,
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
    let canonical_root = root.canonicalize()
        .with_context(|| format!("Dossier source inaccessible : {}", root.display()))?;
    if !canonical_root.is_dir() {
        anyhow::bail!("La source sélectionnée n'est pas un dossier.");
    }

    let workspace = CapabilityDir::open_ambient_dir(&canonical_root, ambient_authority())
        .context("Ouverture capability-based de la source impossible")?;

    let mut relative_paths = Vec::<PathBuf>::new();
    collect_files_capability(&workspace, Path::new("."), 0, &mut relative_paths, cancelled)?;
    if cancelled.load(Ordering::Relaxed) {
        anyhow::bail!("Scan annulé par l'utilisateur.");
    }
    let total_paths = relative_paths.len();
    let total_bytes = relative_paths.iter()
        .filter_map(|relative| workspace.metadata(relative).ok())
        .fold(0_u64, |total, metadata| total.saturating_add(metadata.len()));
    let paths: Vec<PathBuf> = relative_paths.iter().map(|relative| canonical_root.join(relative)).collect();
    let completed = AtomicUsize::new(0);
    let completed_bytes = AtomicU64::new(0);
    let mut assets: Vec<AssetRecord> = paths.par_iter()
        .map(|path| {
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("Scan annulé par l'utilisateur.");
            }
            let asset = analyze_file(path, &canonical_root, &workspace)?;
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("Scan annulé par l'utilisateur.");
            }
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let bytes_done = completed_bytes.fetch_add(asset.size_bytes, Ordering::Relaxed)
                .saturating_add(asset.size_bytes);
            if done.is_multiple_of(100) || done == total_paths {
                progress(bytes_done, total_bytes);
            }
            Ok(asset)
        })
        .collect::<Result<Vec<_>>>()?;
    assets.sort_by(|a, b| a.relative_path.to_lowercase().cmp(&b.relative_path.to_lowercase()));
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
    let total_bytes = assets.iter().fold(0_u64, |total, asset| total.saturating_add(asset.size_bytes));
    let generated_unix_seconds = std::time::SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
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
    if roots.is_empty() {
        anyhow::bail!("Sélectionne au moins un dossier source.");
    }
    let mut canonical_roots = Vec::with_capacity(roots.len());
    for root in roots {
        let canonical = root.canonicalize()
            .with_context(|| format!("Dossier source inaccessible : {}", root.display()))?;
        if !canonical.is_dir() {
            anyhow::bail!("La source sélectionnée n'est pas un dossier : {}", canonical.display());
        }
        if canonical_roots.contains(&canonical) {
            anyhow::bail!("Le même dossier source a été sélectionné plusieurs fois : {}", canonical.display());
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
    let total_bytes = canonical_roots.iter().try_fold(0_u64, |total, root| -> Result<u64> {
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("Scan annulé par l'utilisateur.");
        }
        let workspace = CapabilityDir::open_ambient_dir(root, ambient_authority())
            .context("Ouverture capability-based de la source impossible")?;
        let mut relative_paths = Vec::new();
        collect_files_capability(&workspace, Path::new("."), 0, &mut relative_paths, cancelled)?;
        let size = relative_paths.iter()
            .filter_map(|relative| workspace.metadata(relative).ok())
            .fold(0_u64, |sum, metadata| sum.saturating_add(metadata.len()));
        Ok(total.saturating_add(size))
    })?;
    let mut assets = Vec::new();
    let mut completed_bytes = 0_u64;
    let mut source_roots = Vec::with_capacity(canonical_roots.len());
    for (index, root) in canonical_roots.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) { anyhow::bail!("Scan annulé par l'utilisateur."); }
        source_roots.push(root.display().to_string());
        let document = scan_directory_with_progress_and_cancel(root, |done, _| {
            progress(completed_bytes.saturating_add(done), total_bytes);
        }, cancelled)?;
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
    assets.sort_by(|a, b| a.relative_path.to_lowercase().cmp(&b.relative_path.to_lowercase()));
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
    let total_bytes = assets.iter().fold(0_u64, |total, asset| total.saturating_add(asset.size_bytes));
    Ok(InventoryDocument {
        schema_version: INVENTORY_SCHEMA_VERSION,
        source_root: source_roots[0].clone(),
        generated_unix_seconds: std::time::SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        files_scanned: assets.len(),
        total_bytes,
        duplicate_files,
        source_roots,
        assets,
    })
}

pub fn compare_inventories(current: &InventoryDocument, baseline: &InventoryDocument) -> InventoryComparison {
    let baseline_by_path: HashMap<&str, &AssetRecord> = baseline.assets.iter()
        .map(|asset| (asset.relative_path.as_str(), asset))
        .collect();
    let current_paths: HashMap<&str, &AssetRecord> = current.assets.iter()
        .map(|asset| (asset.relative_path.as_str(), asset))
        .collect();
    let mut added_paths = Vec::new();
    let mut changed_assets = Vec::new();
    let mut unchanged_files = 0usize;

    for asset in &current.assets {
        match baseline_by_path.get(asset.relative_path.as_str()) {
            None => added_paths.push(asset.relative_path.clone()),
            Some(previous) if previous.sha256 != asset.sha256 => changed_assets.push(ChangedAsset {
                relative_path: asset.relative_path.clone(),
                previous_sha256: previous.sha256.clone(),
                current_sha256: asset.sha256.clone(),
            }),
            Some(_) => unchanged_files += 1,
        }
    }

    let mut removed_paths: Vec<String> = baseline.assets.iter()
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
        baseline_source_roots: if baseline.source_roots.is_empty() { vec![baseline.source_root.clone()] } else { baseline.source_roots.clone() },
        current_source_roots: if current.source_roots.is_empty() { vec![current.source_root.clone()] } else { current.source_roots.clone() },
        added_paths,
        removed_paths,
        changed_assets,
        unchanged_files,
    }
}

fn with_new_output_file<T>(destination: &Path, write: impl FnOnce(&mut cap_std::fs::File) -> Result<T>) -> Result<T> {
    let parent = destination.parent().context("Le chemin d'export n'a pas de dossier parent")?;
    let name = destination.file_name().context("Nom de fichier de destination absent")?;
    if name.to_string_lossy().contains(':') {
        anyhow::bail!("Flux de données alternatif Windows refusé dans le nom de destination.");
    }
    let canonical_parent = parent.canonicalize()
        .with_context(|| format!("Dossier de destination inaccessible : {}", parent.display()))?;
    let directory = CapabilityDir::open_ambient_dir(&canonical_parent, cap_std::ambient_authority())
        .context("Ouverture capability du dossier de destination impossible")?;
    let mut options = CapabilityOpenOptions::new();
    options.write(true).create_new(true);
    let mut file = directory.open_with(name, &options)
        .with_context(|| format!("Création exclusive impossible (fichier déjà présent ?) : {}", destination.display()))?;
    let result = (|| {
        let value = write(&mut file)?;
        file.sync_all().context("Synchronisation du fichier exporté impossible")?;
        Ok(value)
    })();
    if result.is_err() {
        drop(file);
        let _ = directory.remove_file(name);
    }
    result
}

pub fn export_comparison_json(comparison: &InventoryComparison, destination: &Path) -> Result<()> {
    let parent = destination.parent().context("Le chemin d'export n'a pas de dossier parent")?;
    let canonical_parent = parent.canonicalize()
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
    let canonical_sources = baseline_roots.iter().chain(current_roots.iter())
        .map(|source| Path::new(source).canonicalize())
        .collect::<std::io::Result<Vec<_>>>()?;
    if canonical_sources.iter().any(|source| canonical_parent.starts_with(source)) {
        anyhow::bail!("Export refusé : le rapport de comparaison doit être écrit hors de toutes les sources comparées.");
    }
    if destination.exists() {
        let canonical_destination = destination.canonicalize()?;
        if canonical_sources.iter().any(|source| canonical_destination.starts_with(source)) {
            anyhow::bail!("Export refusé : la destination résout vers une des sources comparées.");
        }
    }
    with_new_output_file(destination, |file| {
        serde_json::to_writer_pretty(file, comparison).context("Écriture du rapport de comparaison impossible")
    })
}

fn ensure_export_outside_source(document: &InventoryDocument, destination: &Path) -> Result<()> {
    let source_roots = if document.source_roots.is_empty() { vec![document.source_root.clone()] } else { document.source_roots.clone() };
    let canonical_sources = source_roots.iter().map(|source| Path::new(source).canonicalize()).collect::<std::io::Result<Vec<_>>>()?;
    let parent = destination.parent().context("Le chemin d'export n'a pas de dossier parent")?;
    let canonical_parent = parent.canonicalize()
        .with_context(|| format!("Dossier de destination inaccessible : {}", parent.display()))?;
    if canonical_sources.iter().any(|source| canonical_parent.starts_with(source)) {
        anyhow::bail!("Export refusé : choisis un dossier de sortie situé hors de toutes les sources analysées.");
    }
    if destination.exists() {
        let canonical_destination = destination.canonicalize()?;
        if canonical_sources.iter().any(|source| canonical_destination.starts_with(source)) {
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

pub fn export_contact_sheet(document: &InventoryDocument, page_index: usize, destination: &Path) -> Result<usize> {
    ensure_export_outside_source(document, destination)?;
    if destination.extension().and_then(|extension| extension.to_str()).is_none_or(|extension| !extension.eq_ignore_ascii_case("png")) {
        anyhow::bail!("La planche de contact doit être exportée au format PNG.");
    }
    if destination.exists() {
        anyhow::bail!("Le fichier de planche existe déjà : {}", destination.display());
    }
    let sidecar = destination.with_extension("txt");
    ensure_export_outside_source(document, &sidecar)?;
    if sidecar.exists() {
        anyhow::bail!("Le fichier d'index existe déjà : {}", sidecar.display());
    }
    let raster_assets: Vec<&AssetRecord> = document.assets.iter()
        .filter(|asset| asset.detected_family == "raster" && asset.raster_width.is_some() && asset.raster_height.is_some())
        .collect();
    let page_size = 16usize;
    let start = page_index.saturating_mul(page_size);
    if start >= raster_assets.len() {
        anyhow::bail!("Cette page de planche est vide ; {} fichier(s) raster indexé(s).", raster_assets.len());
    }
    let selected = &raster_assets[start..raster_assets.len().min(start + page_size)];
    let mut canvas = image::RgbaImage::from_pixel(880, 720, image::Rgba([245, 246, 248, 255]));
    let mut index_text = String::from("Index de la planche de contact\n");
    let source_roots = if document.source_roots.is_empty() { vec![document.source_root.clone()] } else { document.source_roots.clone() };
    let mut rendered = 0usize;
    for (slot, asset) in selected.iter().enumerate() {
        let (source_root, relative) = if source_roots.len() > 1 {
            let mut parts = asset.relative_path.splitn(2, '/');
            let prefix = parts.next().unwrap_or_default();
            let relative = parts.next().unwrap_or_default();
            let index = prefix.strip_prefix("source-").and_then(|value| value.parse::<usize>().ok()).and_then(|value| value.checked_sub(1));
            let root = index.and_then(|value| source_roots.get(value))
                .context("Source de la planche introuvable dans l'inventaire multi-dossiers")?;
            (root.as_str(), PathBuf::from(relative))
        } else {
            (source_roots[0].as_str(), PathBuf::from(&asset.relative_path))
        };
        let workspace = match CapabilityDir::open_ambient_dir(Path::new(source_root), ambient_authority()) {
            Ok(workspace) => workspace,
            Err(_) => {
                index_text.push_str(&format!("{:02}\tSOURCE ABSENTE OU INACCESSIBLE\t{}\n", slot + 1, asset.relative_path));
                continue;
            }
        };
        if workspace.symlink_metadata(&relative).map(|metadata| metadata.file_type().is_symlink()).unwrap_or(true) {
            index_text.push_str(&format!("{:02}\tLIEN SYMBOLIQUE OU SOURCE ABSENTE\t{}\n", slot + 1, asset.relative_path));
            continue;
        }
        let current_hash = match sha256_file_capability(&workspace, &relative) {
            Ok(hash) => hash,
            Err(_) => {
                index_text.push_str(&format!("{:02}\tSOURCE ABSENTE OU ILLISIBLE\t{}\n", slot + 1, asset.relative_path));
                continue;
            }
        };
        if current_hash != asset.sha256 {
            index_text.push_str(&format!("{:02}\tSOURCE MODIFIÉE DEPUIS LE SCAN\t{}\n", slot + 1, asset.relative_path));
            continue;
        }
        let file_size = workspace.metadata(&relative).map(|metadata| metadata.len()).unwrap_or(u64::MAX);
        if file_size > 100 * 1024 * 1024 {
            index_text.push_str(&format!("{:02}\tIGNORÉ (fichier > 100 Mio)\t{}\n", slot + 1, asset.relative_path));
            continue;
        }
        let dimensions_file = match workspace.open(&relative) {
            Ok(file) => file,
            Err(_) => {
                index_text.push_str(&format!("{:02}\tILLISIBLE\t{}\n", slot + 1, asset.relative_path));
                continue;
            }
        };
        let dimensions = ImageReader::new(BufReader::new(dimensions_file))
            .with_guessed_format()
            .map_err(anyhow::Error::from)
            .and_then(|reader| reader.into_dimensions().map_err(anyhow::Error::from));
        let (width, height) = match dimensions {
            Ok(dimensions) => dimensions,
            Err(_) => {
                index_text.push_str(&format!("{:02}\tILLISIBLE\t{}\n", slot + 1, asset.relative_path));
                continue;
            }
        };
        if u64::from(width).saturating_mul(u64::from(height)) > 30_000_000 {
            index_text.push_str(&format!("{:02}\tIGNORÉ (plus de 30 Mpx)\t{}\n", slot + 1, asset.relative_path));
            continue;
        }
        let decoded_file = match workspace.open(&relative) {
            Ok(file) => file,
            Err(_) => {
                index_text.push_str(&format!("{:02}\tDÉCODAGE IMPOSSIBLE\t{}\n", slot + 1, asset.relative_path));
                continue;
            }
        };
        let decoded = ImageReader::new(BufReader::new(decoded_file))
            .with_guessed_format()
            .map_err(anyhow::Error::from)
            .and_then(|reader| reader.decode().map_err(anyhow::Error::from));
        let image = match decoded {
            Ok(image) => image.thumbnail(200, 160).to_rgba8(),
            Err(_) => {
                index_text.push_str(&format!("{:02}\tDÉCODAGE IMPOSSIBLE\t{}\n", slot + 1, asset.relative_path));
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
        image::DynamicImage::ImageRgba8(canvas).write_to(file, image::ImageFormat::Png)
            .context("Écriture de la planche PNG impossible")?;
        Ok(())
    })?;
    with_new_output_file(&sidecar, |file| {
        file.write_all(index_text.as_bytes()).context("Écriture de l'index de planche impossible")?;
        Ok(())
    })?;
    Ok(rendered)
}

fn escape_html(value: &str) -> String {
    value.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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
        if let (Some(width), Some(height)) = (asset.raster_width, asset.raster_height) {
            html.push_str(&format!("Raster : {}×{} ", width, height));
        }
        if let Some(color_type) = &asset.raster_color_type {
            html.push_str(&format!("Canaux : {} ", escape_html(color_type)));
        }
        if let Some(has_alpha) = asset.raster_has_alpha {
            html.push_str(if has_alpha { "Alpha " } else { "Sans alpha " });
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
        file.write_all(html.as_bytes())
            .with_context(|| format!("Écriture du rapport HTML impossible : {}", destination.display()))?;
        Ok(())
    })
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
        let root = std::env::temp_dir().join(format!("mtg-auditor-cap-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let output = root.join("report.json");
        with_new_output_file(&output, |file| {
            file.write_all(b"first").context("test write")?;
            Ok(())
        }).unwrap();
        assert!(with_new_output_file(&output, |file| {
            file.write_all(b"second").context("test write")?;
            Ok(())
        }).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"first");
        let partial = root.join("partial.json");
        assert!(with_new_output_file(&partial, |_file| -> Result<()> {
            Err(anyhow::anyhow!("simulated export failure"))
        }).is_err());
        assert!(!partial.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn local_scan_does_not_follow_file_or_directory_symlinks() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("mtg-auditor-symlink-test-{}", std::process::id()));
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
        let base = std::env::temp_dir().join(format!("mtg-auditor-symlink-test-{}", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let target = outside.join("secret.txt");
        std::fs::write(&target, b"outside").unwrap();
        let link = root.join("file-link.txt");
        if let Err(error) = symlink_file(&target, &link) {
            eprintln!("SKIP: création de lien symbolique Windows refusée par l'environnement: {error}");
            std::fs::remove_dir_all(base).unwrap();
            return;
        }

        let inventory = scan_directory(&root).unwrap();
        assert_eq!(inventory.files_scanned, 0);
        assert!(inventory.assets.is_empty());
        assert_eq!(std::fs::read(&target).unwrap(), b"outside");
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn cancelled_scan_stops_before_analyzing_source_files() {
        let root = std::env::temp_dir().join(format!("mtg-auditor-cancel-test-{}", std::process::id()));
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
        assert_eq!(classify_notice("LICENSE-MIT"), Some("license-notice".to_string()));
        assert_eq!(classify_notice("NOTICE.txt"), Some("notice-file".to_string()));
    }

    #[test]
    fn detects_svg_viewbox_and_external_references_without_rendering() {
        let root = std::env::temp_dir().join(format!("mtg-auditor-svg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let svg = root.join("test.svg");
        std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 20"><image href="https://example.invalid/image.png"/></svg>"#).unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let (viewbox, external, valid, warning) = svg_metadata(&workspace, Path::new("test.svg"), "svg");
        assert_eq!(viewbox.as_deref(), Some("0 0 10 20"));
        assert_eq!(external, Some(true));
        assert_eq!(valid, Some(true));
        assert!(warning.is_none());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_is_sorted_and_hashes_file_contents() {
        let root: PathBuf = std::env::temp_dir().join(format!("mtg-auditor-test-{}", std::process::id()));
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
        let root = std::env::temp_dir().join(format!("mtg-auditor-zip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("unsafe.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer.start_file("../escape.txt", zip::write::SimpleFileOptions::default()).unwrap();
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
    fn detects_mse_set_zip_main_file_without_extracting() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("mtg-auditor-mse-set-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer.start_file("set", zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(b"mse version: 0.3.4\nset info: fixture").unwrap();
        writer.start_file("cards/example.txt", zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(b"resource").unwrap();
        writer.finish().unwrap();
        let workspace = CapabilityDir::open_ambient_dir(&root, ambient_authority()).unwrap();
        let metadata = zip_metadata(&workspace, Path::new("sample.mse-set"), "mse-set");
        assert_eq!(metadata.entry_count, Some(2));
        assert_eq!(metadata.mse_main_file_present, Some(true));
        assert!(metadata.warning.is_none());
        assert!(!root.join("set").exists(), "ZIP contents must never be extracted");
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_mse_folder_package_main_file_without_following_includes() {
        let root = std::env::temp_dir().join(format!("mtg-auditor-mse-folder-test-{}", std::process::id()));
        let package = root.join("sample.mse-set");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(package.join("cards")).unwrap();
        std::fs::write(package.join("set"),
            b"mse version: 0.3.4\nshort name: Folder Set\ninclude file: ../outside.txt\n").unwrap();
        std::fs::write(package.join("cards/example.txt"), b"resource").unwrap();
        let inventory = scan_directory(&root).unwrap();
        let main = inventory.assets.iter().find(|asset| asset.relative_path == "sample.mse-set/set").unwrap();
        assert_eq!(main.mse_package_type.as_deref(), Some("set"));
        assert_eq!(main.mse_main_file_present, Some(true));
        assert_eq!(main.mse_version.as_deref(), Some("0.3.4"));
        assert_eq!(main.mse_short_name.as_deref(), Some("Folder Set"));
        assert_eq!(main.mse_has_include_directives, Some(true));
        assert!(!inventory.assets.iter().any(|asset| asset.mse_package_type.is_some()
            && asset.relative_path == "sample.mse-set/cards/example.txt"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_compact_and_block_mse_dependencies() {
        let source = b"mse version: 0.3.4\ndepends on: magic.mse-game 2007-06-06\ndepends on:\n\tpackage: shared.mse-style\n\tversion: 1.2.3\n";
        let header = parse_mse_header(source).unwrap();
        assert_eq!(header.dependencies.len(), 2);
        assert_eq!(header.dependencies[0].package, "magic.mse-game");
        assert_eq!(header.dependencies[0].min_version.as_deref(), Some("2007-06-06"));
        assert_eq!(header.dependencies[1].package, "shared.mse-style");
        assert_eq!(header.dependencies[1].min_version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn reads_bounded_mse_header_fields_without_following_includes() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("mtg-auditor-mse-header-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-set");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer.start_file("set", zip::write::SimpleFileOptions::default()).unwrap();
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
        assert!(!root.join("set").exists(), "ZIP contents must never be extracted");
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_bounded_prefix_of_large_mse_main_file() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("mtg-auditor-mse-large-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-game");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer.start_file("game", zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(b"mse version: 0.3.4\nshort name: Large Game\n").unwrap();
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
    fn reports_mse_zip_package_missing_its_main_file() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("mtg-auditor-mse-game-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.mse-game");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer.start_file("resources/icon.png", zip::write::SimpleFileOptions::default()).unwrap();
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
                "review_status": "needs-review"
            }]
        }"#;
        let inventory: InventoryDocument = serde_json::from_str(legacy).unwrap();
        assert_eq!(inventory.duplicate_files, 0);
        assert_eq!(inventory.assets[0].raster_width, None);
        assert_eq!(inventory.assets[0].duplicate_of, None);
    }

    #[test]
    fn compares_added_removed_changed_and_unchanged_files() {
        let base = std::env::temp_dir().join(format!("mtg-auditor-compare-test-{}", std::process::id()));
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
        let base = std::env::temp_dir().join(format!("mtg-auditor-sheet-test-{}", std::process::id()));
        let source = base.join("source");
        let output = base.join("output");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&output).unwrap();
        let source_image = source.join("card.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 40, 60, 128])).save(&source_image).unwrap();
        let document = scan_directory(&source).unwrap();
        let sheet_path = output.join("sheet.png");
        let rendered = export_contact_sheet(&document, 0, &sheet_path).unwrap();
        assert_eq!(rendered, 1);
        assert!(sheet_path.exists());
        assert!(output.join("sheet.txt").exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn scans_multiple_roots_without_path_collisions_and_detects_cross_root_duplicates() {
        let base = std::env::temp_dir().join(format!("mtg-auditor-multi-test-{}", std::process::id()));
        let first = base.join("first");
        let second = base.join("second");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("same.txt"), b"shared").unwrap();
        std::fs::write(second.join("same.txt"), b"shared").unwrap();
        let roots = vec![first.clone(), second.clone()];
        let document = scan_multiple_directories_with_progress_and_cancel(
            &roots, |_, _| {}, &AtomicBool::new(false)
        ).unwrap();
        assert_eq!(document.source_roots.len(), 2);
        assert_eq!(document.files_scanned, 2);
        assert_ne!(document.assets[0].relative_path, document.assets[1].relative_path);
        assert_eq!(document.duplicate_files, 1);
        assert!(document.assets.iter().all(|asset| asset.relative_path.starts_with("source-000")));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rejects_nested_multi_source_roots() {
        let base = std::env::temp_dir().join(format!("mtg-auditor-multi-nested-{}", std::process::id()));
        let child = base.join("child");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&child).unwrap();
        let result = scan_multiple_directories_with_progress_and_cancel(
            &[base.clone(), child], |_, _| {}, &AtomicBool::new(false)
        );
        assert!(result.unwrap_err().to_string().contains("chevauchent"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn refuses_comparison_export_inside_any_multi_source_root() {
        let base = std::env::temp_dir().join(format!("mtg-auditor-multi-export-{}", std::process::id()));
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
        assert_eq!(escape_html("<script>&\"'"), "&lt;script&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn refuses_export_inside_source_tree() {
        let root: PathBuf = std::env::temp_dir().join(format!("mtg-auditor-export-test-{}", std::process::id()));
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
