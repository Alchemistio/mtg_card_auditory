use anyhow::{Context, Result};
use image::ImageReader;
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::{BufReader, Read}, path::Path, time::UNIX_EPOCH};
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
    pub raster_format: Option<String>,
    pub raster_width: Option<u32>,
    pub raster_height: Option<u32>,
    pub svg_viewbox: Option<String>,
    pub svg_has_external_refs: Option<bool>,
    pub svg_xml_valid: Option<bool>,
    pub font_family: Option<String>,
    pub font_subfamily: Option<String>,
    pub archive_entry_count: Option<usize>,
    pub archive_uncompressed_bytes: Option<u64>,
    pub archive_suspicious_paths: Option<usize>,
    pub analysis_warning: Option<String>,
    pub review_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryDocument {
    pub schema_version: u32,
    pub source_root: String,
    pub generated_unix_seconds: u64,
    pub files_scanned: usize,
    pub total_bytes: u64,
    pub assets: Vec<AssetRecord>,
}

fn is_hidden_or_metadata(entry: &DirEntry) -> bool {
    entry.file_name().to_string_lossy().starts_with('.')
}

fn classify_extension(extension: &str) -> &'static str {
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

fn raster_metadata(path: &Path, family: &str) -> (Option<String>, Option<u32>, Option<u32>, Option<String>) {
    if family != "raster" {
        return (None, None, None, None);
    }
    let reader = match ImageReader::open(path).and_then(|reader| reader.with_guessed_format()) {
        Ok(reader) => reader,
        Err(error) => return (None, None, None, Some(format!("Détection du format image impossible : {}", error))),
    };
    let format = reader.format().map(|value| format!("{value:?}"));
    match reader.into_dimensions() {
        Ok((width, height)) => (format, Some(width), Some(height), None),
        Err(error) => (format, None, None, Some(format!("En-tête image illisible : {}", error))),
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
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => return (None, None, None, Some(format!("Ouverture ZIP impossible : {}", error))),
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => return (None, None, None, Some(format!("Structure ZIP invalide : {}", error))),
    };
    if archive.len() > 200_000 {
        return (Some(archive.len()), None, None, Some("Archive ignorée : plus de 200 000 entrées.".to_string()));
    }
    let mut uncompressed_bytes = 0_u64;
    let mut suspicious_paths = 0_usize;
    let mut suspicious_ratio = false;
    for index in 0..archive.len() {
        let entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => return (Some(archive.len()), None, None, Some(format!("Entrée ZIP illisible : {}", error))),
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
            return (Some(archive.len()), Some(uncompressed_bytes), Some(suspicious_paths),
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
    (Some(archive.len()), Some(uncompressed_bytes), Some(suspicious_paths), warning)
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

pub fn scan_directory(root: &Path) -> Result<InventoryDocument> {
    let canonical_root = root.canonicalize()
        .with_context(|| format!("Dossier source inaccessible : {}", root.display()))?;
    if !canonical_root.is_dir() {
        anyhow::bail!("La source sélectionnée n'est pas un dossier.");
    }

    let mut assets = Vec::new();
    let mut total_bytes = 0_u64;
    for item in WalkDir::new(&canonical_root)
        .follow_links(false)
        .max_depth(128)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_hidden_or_metadata(entry))
    {
        let entry = item.context("Erreur pendant le parcours du dossier")?;
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        let metadata = entry.metadata()?;
        let relative = path.strip_prefix(&canonical_root)
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
        let (raster_format, raster_width, raster_height, raster_warning) =
            raster_metadata(path, detected_family);
        let (svg_viewbox, svg_has_external_refs, svg_xml_valid, svg_warning) =
            svg_metadata(path, detected_family);
        let (font_family, font_subfamily, font_warning) = font_metadata(path, detected_family);
        let (archive_entry_count, archive_uncompressed_bytes, archive_suspicious_paths, archive_warning) =
            zip_metadata(path, &extension);
        let analysis_warning = raster_warning.or(svg_warning).or(font_warning).or(archive_warning);
        total_bytes = total_bytes.saturating_add(size_bytes);
        assets.push(AssetRecord {
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
            svg_viewbox,
            svg_has_external_refs,
            svg_xml_valid,
            font_family,
            font_subfamily,
            archive_entry_count,
            archive_uncompressed_bytes,
            archive_suspicious_paths,
            analysis_warning,
            review_status: "needs-review".to_string(),
        });
    }
    assets.sort_by(|a, b| a.relative_path.to_lowercase().cmp(&b.relative_path.to_lowercase()));
    let generated_unix_seconds = std::time::SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(InventoryDocument {
        schema_version: INVENTORY_SCHEMA_VERSION,
        source_root: canonical_root.display().to_string(),
        generated_unix_seconds,
        files_scanned: assets.len(),
        total_bytes,
        assets,
    })
}

fn ensure_export_outside_source(document: &InventoryDocument, destination: &Path) -> Result<()> {
    let source_root = Path::new(&document.source_root).canonicalize()?;
    let parent = destination.parent().context("Le chemin d'export n'a pas de dossier parent")?;
    let canonical_parent = parent.canonicalize()
        .with_context(|| format!("Dossier de destination inaccessible : {}", parent.display()))?;
    if canonical_parent.starts_with(&source_root) {
        anyhow::bail!("Export refusé : choisis un dossier de sortie situé hors de la source analysée.");
    }
    if destination.exists() {
        let canonical_destination = destination.canonicalize()?;
        if canonical_destination.starts_with(&source_root) {
            anyhow::bail!("Export refusé : la destination résout vers un fichier de la source analysée.");
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classifies_known_and_unknown_extensions() {
        assert_eq!(classify_extension("png"), "raster");
        assert_eq!(classify_extension("SVG"), "svg");
        assert_eq!(classify_extension("mse-set"), "mse");
        assert_eq!(classify_extension("exe"), "unknown");
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
            assets: Vec::new(),
        };
        assert!(ensure_export_outside_source(&document, &root.join("inventory.json")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
