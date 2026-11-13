use anyhow::{Context, Result};
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
        total_bytes = total_bytes.saturating_add(size_bytes);
        assets.push(AssetRecord {
            id: format!("sha256:{}:{}", sha256, relative_path),
            relative_path,
            file_name,
            extension: extension.clone(),
            size_bytes,
            modified_unix_seconds,
            sha256,
            detected_family: classify_extension(&extension).to_string(),
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

pub fn export_json(document: &InventoryDocument, destination: &Path) -> Result<()> {
    let file = File::create(destination)
        .with_context(|| format!("Création impossible : {}", destination.display()))?;
    serde_json::to_writer_pretty(file, document).context("Écriture JSON impossible")
}

pub fn export_csv(document: &InventoryDocument, destination: &Path) -> Result<()> {
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
        std::fs::remove_dir_all(root).unwrap();
    }
}
