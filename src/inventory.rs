use anyhow::{Context, Result};
use image::{ImageDecoder, ImageReader};
use quick_xml::{events::Event, Reader};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs::File, io::{BufReader, Read}, path::{Path, PathBuf}, sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}, time::UNIX_EPOCH};
use walkdir::{DirEntry, WalkDir};

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

fn is_hidden_or_metadata(entry: &DirEntry) -> bool {
    entry.file_name().to_string_lossy().starts_with('.')
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

fn raster_metadata(path: &Path, family: &str) -> RasterMetadata {
    if family != "raster" {
        return RasterMetadata::default();
    }
    let reader = match ImageReader::open(path).and_then(|reader| reader.with_guessed_format()) {
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

fn svg_metadata(path: &Path, family: &str) -> (Option<String>, Option<bool>, Option<bool>, Option<String>) {
    if family != "svg" {
        return (None, None, None, None);
    }
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() > 32 * 1024 * 1024 {
            return (None, None, None, Some("SVG ignoré : taille supérieure à 32 Mio.".to_string()));
        }
    }
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => return (None, None, Some(false), Some(format!("Lecture SVG impossible : {}", error))),
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

fn font_metadata(path: &Path, family: &str) -> (Option<String>, Option<String>, Option<String>) {
    if family != "font" {
        return (None, None, None);
    }
    let size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) => return (None, None, Some(format!("Métadonnées de police indisponibles : {}", error))),
    };
    if size > 64 * 1024 * 1024 {
        return (None, None, Some("Police ignorée : taille supérieure à 64 Mio.".to_string()));
    }
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(error) => return (None, None, Some(format!("Lecture de police impossible : {}", error))),
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

fn zip_metadata(path: &Path, extension: &str) -> (Option<usize>, Option<u64>, Option<usize>, Option<String>) {
    if extension != "zip" {
        if extension == "7z" || extension == "rar" {
            return (None, None, None, Some("Inspection de cette archive non prise en charge pour le moment.".to_string()));
        }
        return (None, None, None, None);
    }
    let archive_size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) => return (None, None, None, Some(format!("Métadonnées ZIP indisponibles : {}", error))),
    };
    if archive_size > 512 * 1024 * 1024 {
        return (None, None, None, Some("Archive ignorée : fichier ZIP supérieur à 512 Mio.".to_string()));
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => return (None, None, None, Some(format!("Ouverture ZIP impossible : {}", error))),
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => return (None, None, None, Some(format!("Structure ZIP invalide : {}", error))),
    };
    let entry_count = archive.len();
    if entry_count > 200_000 {
        return (Some(entry_count), None, None, Some("Archive ignorée : plus de 200 000 entrées.".to_string()));
    }
    let mut uncompressed_bytes = 0_u64;
    let mut suspicious_paths = 0_usize;
    let mut suspicious_ratio = false;
    for index in 0..archive.len() {
        let entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => return (Some(entry_count), None, None, Some(format!("Entrée ZIP illisible : {}", error))),
        };
        let name = entry.name();
        if entry.enclosed_name().is_none()
            || name.contains('\\')
            || name.contains(':')
            || name.split('/').any(|part| part == "." || part == "..")
        {
            suspicious_paths += 1;
        }
        let size = entry.size();
        let compressed = entry.compressed_size();
        if size > compressed.saturating_mul(1000) && size > 1024 * 1024 {
            suspicious_ratio = true;
        }
        uncompressed_bytes = uncompressed_bytes.saturating_add(size);
        if uncompressed_bytes > 8 * 1024 * 1024 * 1024 {
            return (Some(entry_count), Some(uncompressed_bytes), Some(suspicious_paths),
                Some("Alerte ZIP : taille décompressée déclarée supérieure à 8 Gio.".to_string()));
        }
    }
    let warning = if suspicious_paths > 0 {
        Some("Archive ZIP contenant des chemins suspects ; aucune extraction n'a été effectuée.".to_string())
    } else if suspicious_ratio {
        Some("Archive ZIP avec un ratio de compression inhabituel ; aucune extraction n'a été effectuée.".to_string())
    } else {
        None
    };
    (Some(entry_count), Some(uncompressed_bytes), Some(suspicious_paths), warning)
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("Ouverture impossible : {}", path.display()))?;
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

fn analyze_file(path: &Path, canonical_root: &Path) -> Result<AssetRecord> {
    let metadata = std::fs::metadata(path)?;
    let relative = path.strip_prefix(canonical_root)
        .context("Chemin hors du dossier source")?;
    let relative_path = relative.to_string_lossy().replace('\\', "/");
    let file_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let extension = path.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let size_bytes = metadata.len();
    let sha256 = sha256_file(path)?;
    let modified_unix_seconds = metadata.modified().ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let detected_family = classify_extension(&extension);
    let raster = raster_metadata(path, detected_family);
    let raster_format = raster.format;
    let raster_width = raster.width;
    let raster_height = raster.height;
    let raster_color_type = raster.color_type;
    let raster_has_alpha = raster.has_alpha;
    let (svg_viewbox, svg_has_external_refs, svg_xml_valid, svg_warning) =
        svg_metadata(path, detected_family);
    let (font_family, font_subfamily, font_warning) = font_metadata(path, detected_family);
    let (archive_entry_count, archive_uncompressed_bytes, archive_suspicious_paths, archive_warning) =
        zip_metadata(path, &extension);
    let analysis_warning = raster.warning.or(svg_warning).or(font_warning).or(archive_warning);
    let notice_kind = classify_notice(&file_name);
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

    let mut paths = Vec::<PathBuf>::new();
    for item in WalkDir::new(&canonical_root)
        .follow_links(false)
        .max_depth(128)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_hidden_or_metadata(entry))
    {
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("Scan annulé par l'utilisateur.");
        }
        let entry = item.context("Erreur pendant le parcours du dossier")?;
        if entry.file_type().is_file() {
            paths.push(entry.path().to_path_buf());
        }
    }

    let total_paths = paths.len();
    let total_bytes = paths.iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .fold(0_u64, |total, metadata| total.saturating_add(metadata.len()));
    let completed = AtomicUsize::new(0);
    let completed_bytes = AtomicU64::new(0);
    let mut assets: Vec<AssetRecord> = paths.par_iter()
        .map(|path| {
            if cancelled.load(Ordering::Relaxed) {
                anyhow::bail!("Scan annulé par l'utilisateur.");
            }
            let asset = analyze_file(path, &canonical_root)?;
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
        let mut size = 0_u64;
        for item in WalkDir::new(root).follow_links(false).max_depth(128)
            .into_iter().filter_entry(|entry| entry.depth() == 0 || !is_hidden_or_metadata(entry)) {
            if cancelled.load(Ordering::Relaxed) { anyhow::bail!("Scan annulé par l'utilisateur."); }
            let entry = item?;
            if entry.file_type().is_file() {
                size = size.saturating_add(entry.metadata()?.len());
            }
        }
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

pub fn export_comparison_json(comparison: &InventoryComparison, destination: &Path) -> Result<()> {
    let parent = destination.parent().context("Le chemin d'export n'a pas de dossier parent")?;
    let canonical_parent = parent.canonicalize()
        .with_context(|| format!("Dossier de destination inaccessible : {}", parent.display()))?;
    for source in [&comparison.baseline_source_root, &comparison.current_source_root] {
        let canonical_source = Path::new(source).canonicalize()?;
        if canonical_parent.starts_with(&canonical_source) {
            anyhow::bail!("Export refusé : le rapport de comparaison doit être écrit hors des deux sources.");
        }
    }
    if destination.exists() {
        let canonical_destination = destination.canonicalize()?;
        if canonical_sources.iter().any(|source| canonical_destination.starts_with(source)) {
            anyhow::bail!("Export refusé : la destination résout vers une des sources comparées.");
        }
    }
    let file = File::create(destination)
        .with_context(|| format!("Création impossible : {}", destination.display()))?;
    serde_json::to_writer_pretty(file, comparison).context("Écriture du rapport de comparaison impossible")
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
    let file = File::create(destination)
        .with_context(|| format!("Création impossible : {}", destination.display()))?;
    serde_json::to_writer_pretty(file, document).context("Écriture JSON impossible")
}

pub fn export_csv(document: &InventoryDocument, destination: &Path) -> Result<()> {
    ensure_export_outside_source(document, destination)?;
    let mut writer = csv::Writer::from_path(destination)
        .with_context(|| format!("Création CSV impossible : {}", destination.display()))?;
    for asset in &document.assets {
        writer.serialize(asset)?;
    }
    writer.flush()?;
    Ok(())
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
        let source = if source_roots.len() > 1 {
            let mut parts = asset.relative_path.splitn(2, '/');
            let prefix = parts.next().unwrap_or_default();
            let relative = parts.next().unwrap_or_default();
            let index = prefix.strip_prefix("source-").and_then(|value| value.parse::<usize>().ok()).and_then(|value| value.checked_sub(1));
            index.and_then(|value| source_roots.get(value)).map(|root| Path::new(root).join(relative))
                .context("Source de la planche introuvable dans l'inventaire multi-dossiers")?
        } else {
            Path::new(&source_roots[0]).join(&asset.relative_path)
        };
        if std::fs::symlink_metadata(&source).map(|metadata| metadata.file_type().is_symlink()).unwrap_or(true) {
            index_text.push_str(&format!("{:02}\tLIEN SYMBOLIQUE OU SOURCE ABSENTE\t{}\n", slot + 1, asset.relative_path));
            continue;
        }
        let current_hash = match sha256_file(&source) {
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
        let file_size = std::fs::metadata(&source).map(|metadata| metadata.len()).unwrap_or(u64::MAX);
        if file_size > 100 * 1024 * 1024 {
            index_text.push_str(&format!("{:02}\tIGNORÉ (fichier > 100 Mio)\t{}\n", slot + 1, asset.relative_path));
            continue;
        }
        let dimensions = ImageReader::open(&source)
            .map_err(anyhow::Error::from)
            .and_then(|reader| reader.with_guessed_format().map_err(anyhow::Error::from))
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
        let decoded = ImageReader::open(&source)
            .and_then(|reader| reader.with_guessed_format())
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
    canvas.save(destination).context("Écriture de la planche PNG impossible")?;
    std::fs::write(&sidecar, index_text).context("Écriture de l'index de planche impossible")?;
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
    std::fs::write(destination, html)
        .with_context(|| format!("Écriture du rapport HTML impossible : {}", destination.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        let (viewbox, external, valid, warning) = svg_metadata(&svg, "svg");
        assert_eq!(viewbox.as_deref(), Some("0 0 10 20"));
        assert_eq!(external, Some(true));
        assert_eq!(valid, Some(true));
        assert!(warning.is_none());
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
        let (count, _, suspicious, warning) = zip_metadata(&archive_path, "zip");
        assert_eq!(count, Some(1));
        assert_eq!(suspicious, Some(1));
        assert!(warning.is_some());
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
