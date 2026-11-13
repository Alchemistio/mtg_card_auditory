mod github;
mod inventory;
mod models;

use anyhow::Context;
use eframe::egui;
use cap_std::fs::{Dir as CapabilityDir, OpenOptions as CapabilityOpenOptions};
use zeroize::Zeroizing;

enum ScanMessage {
    Progress(u64, u64),
    Finished(std::result::Result<inventory::InventoryDocument, String>),
}

enum DownloadMessage {
    Progress(usize, usize, String),
    Finished(String),
}
use std::{collections::HashSet, io::Write, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, TryRecvError}, Arc}, time::Duration};

#[derive(Default)]
struct AuditorApp {
    repository_url: String,
    owner: String,
    repository: String,
    branch: String,
    github_commit_sha: String,
    github_tree_sha: String,
    subdirectory: String,
    remote_filter: String,
    remote_extension_filter: String,
    remote_min_size: String,
    remote_max_size: String,
    remote_family_filter: String,
    token: Zeroizing<String>,
    workspace: String,
    local_source: String,
    local_filter: String,
    contact_page: String,
    local_inventory: Option<inventory::InventoryDocument>,
    comparison: Option<inventory::InventoryComparison>,
    scan_receiver: Option<Receiver<ScanMessage>>,
    scan_cancel: Option<Arc<AtomicBool>>, 
    github_receiver: Option<Receiver<std::result::Result<models::RepositoryTree, String>>>,
    download_receiver: Option<Receiver<DownloadMessage>>,
    download_cancel: Option<Arc<AtomicBool>>, 
    status: String,
    entries: Vec<models::RemoteEntry>,
    selected: HashSet<String>,
    busy: bool,
}

impl AuditorApp {
    fn load_github_token(&mut self) {
        let result = keyring::Entry::new("mtg-asset-auditor", "github-token")
            .context("Initialisation du Windows Credential Manager impossible")
            .and_then(|entry| entry.get_password().context("Lecture du jeton enregistré impossible"));
        match result {
            Ok(token) => {
                self.token = Zeroizing::new(token);
                self.status = "Jeton GitHub chargé depuis le Gestionnaire d’identifiants Windows.".into();
            }
            Err(error) => self.status = format!("Aucun jeton disponible ou lecture impossible : {:#}", error),
        }
    }

    fn save_github_token(&mut self) {
        if self.token.trim().is_empty() {
            self.status = "Saisis un jeton avant de l’enregistrer.".into();
            return;
        }
        let result = keyring::Entry::new("mtg-asset-auditor", "github-token")
            .context("Initialisation du Windows Credential Manager impossible")
            .and_then(|entry| entry.set_password(&self.token).context("Enregistrement du jeton impossible"));
        self.status = match result {
            Ok(()) => "Jeton enregistré dans le Gestionnaire d’identifiants Windows.".into(),
            Err(error) => format!("Enregistrement sécurisé impossible : {:#}", error),
        };
    }

    fn delete_github_token(&mut self) {
        let result = keyring::Entry::new("mtg-asset-auditor", "github-token")
            .context("Initialisation du Windows Credential Manager impossible")
            .and_then(|entry| entry.delete_credential().context("Suppression du jeton enregistré impossible"));
        match result {
            Ok(()) => {
                self.token = Zeroizing::default();
                self.status = "Jeton supprimé du Gestionnaire d’identifiants Windows et du champ courant.".into();
            }
            Err(error) => self.status = format!("Suppression impossible (jeton absent ou magasin indisponible) : {:#}", error),
        }
    }

    fn connect(&mut self) {
        let owner = self.owner.clone();
        let repository = self.repository.clone();
        let branch = self.branch.clone();
        let token = Zeroizing::new(self.token.to_string());
        let (sender, receiver) = mpsc::channel();
        self.github_receiver = Some(receiver);
        self.busy = true;
        self.status = "Exploration GitHub en arrière-plan…".into();
        std::thread::spawn(move || {
            let result = github::list_repository_tree(&owner, &repository, &branch, &token)
                .map_err(|error| format!("{:#}", error));
            let _ = sender.send(result);
        });
    }

    fn parse_url(&mut self) {
        match github::parse_repository_url(&self.repository_url) {
            Ok((owner, repository)) => {
                self.owner = owner;
                self.repository = repository;
                self.status = "URL reconnue. Vérifie la branche et lance l'exploration.".into();
            }
            Err(error) => self.status = format!("URL invalide : {:#}", error),
        }
    }

    fn scan_local(&mut self) {
        if self.local_source.trim().is_empty() {
            self.status = "Choisis un dossier source à analyser.".into();
            return;
        }
        let sources: Vec<PathBuf> = self.local_source.lines().map(str::trim).filter(|line| !line.is_empty()).map(PathBuf::from).collect();
        if sources.is_empty() {
            self.status = "Ajoute au moins un dossier source.".into();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        self.scan_receiver = Some(receiver);
        self.scan_cancel = Some(cancelled);
        self.busy = true;
        self.status = "Inventaire local parallèle et calcul SHA-256 en arrière-plan…".into();
        std::thread::spawn(move || {
            let progress_sender = sender.clone();
            let progress = |done, total| { let _ = progress_sender.send(ScanMessage::Progress(done, total)); };
            let result = if sources.len() == 1 {
                inventory::scan_directory_with_progress_and_cancel(&sources[0], progress, &worker_cancelled)
            } else {
                inventory::scan_multiple_directories_with_progress_and_cancel(&sources, progress, &worker_cancelled)
            }.map_err(|error| format!("{:#}", error));
            let _ = sender.send(ScanMessage::Finished(result));
        });
    }

    fn cancel_scan(&mut self) {
        if let Some(cancelled) = self.scan_cancel.as_ref() {
            cancelled.store(true, Ordering::Relaxed);
            self.status = "Annulation du scan demandée ; arrêt après les opérations en cours…".into();
        }
    }

    fn export_local_json(&mut self) {
        let Some(document) = self.local_inventory.as_ref() else {
            self.status = "Aucun inventaire à exporter.".into();
            return;
        };
        if let Some(path) = rfd::FileDialog::new().set_file_name("asset-inventory.json").save_file() {
            match inventory::export_json(document, &path) {
                Ok(()) => self.status = format!("Inventaire JSON exporté : {}", path.display()),
                Err(error) => self.status = format!("Échec export JSON : {:#}", error),
            }
        }
    }

    fn export_local_csv(&mut self) {
        let Some(document) = self.local_inventory.as_ref() else {
            self.status = "Aucun inventaire à exporter.".into();
            return;
        };
        if let Some(path) = rfd::FileDialog::new().set_file_name("asset-inventory.csv").save_file() {
            match inventory::export_csv(document, &path) {
                Ok(()) => self.status = format!("Inventaire CSV exporté : {}", path.display()),
                Err(error) => self.status = format!("Échec export CSV : {:#}", error),
            }
        }
    }

    fn export_local_html(&mut self) {
        let Some(document) = self.local_inventory.as_ref() else {
            self.status = "Aucun inventaire à exporter.".into();
            return;
        };
        if let Some(path) = rfd::FileDialog::new().set_file_name("asset-inventory.html").save_file() {
            match inventory::export_html(document, &path) {
                Ok(()) => self.status = format!("Rapport HTML exporté : {}", path.display()),
                Err(error) => self.status = format!("Échec export HTML : {:#}", error),
            }
        }
    }

    fn compare_local_inventory(&mut self) {
        if self.local_inventory.is_none() {
            self.status = "Lance d'abord un inventaire local.".into();
            return;
        }
        if let Some(path) = rfd::FileDialog::new().add_filter("Inventaire JSON", &["json"]).pick_file() {
            let result = (|| -> anyhow::Result<inventory::InventoryComparison> {
                let file = std::fs::File::open(&path)?;
                let baseline: inventory::InventoryDocument = serde_json::from_reader(file)?;
                let current = self.local_inventory.as_ref().context("Inventaire courant absent")?;
                Ok(inventory::compare_inventories(current, &baseline))
            })();
            match result {
                Ok(comparison) => {
                    self.status = format!(
                        "Comparaison terminée : {} ajouté(s), {} supprimé(s), {} modifié(s), {} inchangé(s).",
                        comparison.added_paths.len(), comparison.removed_paths.len(),
                        comparison.changed_assets.len(), comparison.unchanged_files
                    );
                    self.comparison = Some(comparison);
                }
                Err(error) => self.status = format!("Comparaison impossible : {:#}", error),
            }
        }
    }

    fn export_comparison(&mut self) {
        let Some(comparison) = self.comparison.as_ref() else {
            self.status = "Aucune comparaison à exporter.".into();
            return;
        };
        if let Some(path) = rfd::FileDialog::new().set_file_name("inventory-comparison.json").save_file() {
            match inventory::export_comparison_json(comparison, &path) {
                Ok(()) => self.status = format!("Rapport de comparaison exporté : {}", path.display()),
                Err(error) => self.status = format!("Échec export comparaison : {:#}", error),
            }
        }
    }

    fn export_contact_sheet(&mut self) {
        let Some(document) = self.local_inventory.as_ref() else {
            self.status = "Lance d'abord un inventaire local.".into();
            return;
        };
        let page_number = self.contact_page.trim().parse::<usize>().unwrap_or(1).max(1);
        if let Some(path) = rfd::FileDialog::new().set_file_name(format!("contact-sheet-{:04}.png", page_number)).save_file() {
            match inventory::export_contact_sheet(document, page_number - 1, &path) {
                Ok(rendered) => self.status = format!("Planche {} créée avec {} aperçu(x) : {} (index .txt associé).", page_number, rendered, path.display()),
                Err(error) => self.status = format!("Échec de la planche : {:#}", error),
            }
        }
    }

    fn cancel_download(&mut self) {
        if let Some(cancelled) = self.download_cancel.as_ref() {
            cancelled.store(true, Ordering::Relaxed);
            self.status = "Annulation demandée ; le blob en cours peut terminer avant l'arrêt…".into();
        }
    }

    fn download_selected(&mut self) {
        let root = PathBuf::from(self.workspace.trim());
        if self.workspace.trim().is_empty() {
            self.status = "Choisis d'abord un dossier de travail.".into();
            return;
        }
        let chosen: Vec<_> = self.entries.iter()
            .filter(|entry| self.selected.contains(&entry.path) && entry.kind == "blob")
            .cloned().collect();
        if chosen.is_empty() {
            self.status = "Aucun fichier sélectionné.".into();
            return;
        }
        let owner = self.owner.clone();
        let repository = self.repository.clone();
        let token = Zeroizing::new(self.token.to_string());
        let commit_sha = self.github_commit_sha.clone();
        let tree_sha = self.github_tree_sha.clone();
        let requested_reference = self.branch.clone();
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        self.download_receiver = Some(receiver);
        self.download_cancel = Some(cancelled);
        self.busy = true;
        self.status = "Téléchargement GitHub en arrière-plan…".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                std::fs::create_dir_all(&root)?;
                let canonical_root = root.canonicalize()?;
                let workspace_fs = WorkspaceDir::open(&canonical_root)?;
                let mut downloaded = 0usize;
                let mut errors = Vec::new();
                let mut manifest_files = Vec::new();
                let total = chosen.len();
                if !safe_github_namespace_segment(&owner)
                    || !safe_github_namespace_segment(&repository)
                    || !safe_github_namespace_segment(&commit_sha) {
                    anyhow::bail!("Identifiant GitHub invalide pour le journal de reprise.");
                }
                let journal_relative = PathBuf::from(".mtg-auditor-cache")
                    .join("download-sessions").join(&owner).join(&repository)
                    .join(format!("download-session-{}.jsonl", commit_sha));
                workspace_fs.create_dir_all(journal_relative.parent().context("Parent du journal absent")?)?;
                if let Ok(metadata) = workspace_fs.symlink_metadata(&journal_relative) {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        anyhow::bail!("Le journal de reprise doit être un fichier régulier, sans lien symbolique.");
                    }
                }
                let mut journal = workspace_fs.open_append(&journal_relative)?;
                let mut completed_records = HashSet::new();
                if workspace_fs.symlink_metadata(&journal_relative)?.len() > 0 {
                    let journal_text = workspace_fs.read_to_string(&journal_relative)
                        .context("Journal de reprise illisible ; aucune reprise automatique")?;
                    let journal_lines: Vec<&str> = journal_text.split_terminator('\n').collect();
                    for (line_index, line) in journal_lines.iter().enumerate() {
                        if line.trim().is_empty() { continue; }
                        let is_unterminated_tail = line_index + 1 == journal_lines.len() && !journal_text.ends_with('\n');
                        let record: models::DownloadProgressRecord = match serde_json::from_str(line) {
                            Ok(record) => record,
                            Err(_) if is_unterminated_tail => break,
                            Err(error) => return Err(error).context("Enregistrement invalide dans le journal de reprise"),
                        };
                        completed_records.insert((record.path, record.blob_sha, record.size_bytes));
                    }
                }
                let manifest_name = format!("download-manifest-{}.json", commit_sha);
                let manifest_relative = PathBuf::from(".mtg-auditor-cache")
                    .join("download-manifests").join(&owner).join(&repository).join(&manifest_name);
                workspace_fs.create_dir_all(manifest_relative.parent().context("Parent du manifeste absent")?)?;
                let manifest_conflict = chosen.iter().any(|entry| Path::new(&entry.path) == manifest_relative);
                let progress_sender = sender.clone();
                let mut processed = 0usize;
                let mut was_cancelled = false;
                for (index, entry) in chosen.into_iter().enumerate() {
                    if worker_cancelled.load(Ordering::Relaxed) {
                        was_cancelled = true;
                        break;
                    }
                    processed = index + 1;
                    let expected_size = entry.size.unwrap_or(0);
                    let journal_key = (entry.path.clone(), entry.sha.clone(), expected_size);
                    if completed_records.contains(&journal_key) {
                        let relative_path = safe_workspace_relative(&entry.path)?;
                        if let Ok(metadata) = workspace_fs.symlink_metadata(&relative_path) {
                            let valid = if metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == expected_size {
                                workspace_fs.open_read(&relative_path).map_or(false, |mut file| github::verify_git_blob_reader(&entry.sha, &mut file, expected_size).is_ok())
                            } else { false };
                            if valid {
                                downloaded += 1;
                                manifest_files.push(models::DownloadedFile {
                                    path: entry.path.clone(),
                                    blob_sha: entry.sha.clone(),
                                    size_bytes: expected_size,
                                });
                                let _ = progress_sender.send(DownloadMessage::Progress(index + 1, total, entry.path));
                                continue;
                            }
                        }
                        errors.push(format!("{} : journal présent mais fichier absent ou empreinte invalide ; aucun écrasement effectué", entry.path));
                        let _ = progress_sender.send(DownloadMessage::Progress(index + 1, total, entry.path));
                        continue;
                    }
                    let result = cached_github_blob(&workspace_fs, &owner, &repository, &commit_sha, &entry.sha, entry.size, &token)
                        .and_then(|cache_file| {
                            let destination = safe_workspace_relative(&entry.path)?;
                            let mut source = workspace_fs.open_read(&cache_file)?;
                            let mut target = workspace_fs.create_new(&destination)
                                .with_context(|| format!("Écriture capability sans écrasement impossible : {}", entry.path))?;
                            if let Err(error) = std::io::copy(&mut source, &mut target).and_then(|_| target.sync_all()) {
                                drop(target);
                                let _ = workspace_fs.remove_file(&destination);
                                return Err(error.into());
                            }
                            drop(target);
                            let verify_result = workspace_fs.open_read(&destination)
                                .map_err(anyhow::Error::from)
                                .and_then(|mut file| github::verify_git_blob_reader(&entry.sha, &mut file, entry.size.unwrap_or(0)));
                            if let Err(error) = verify_result {
                                let _ = workspace_fs.remove_file(&destination);
                                return Err(error).context("Vérification finale du fichier copié impossible");
                            }
                            let record = models::DownloadProgressRecord {
                                path: entry.path.clone(),
                                blob_sha: entry.sha.clone(),
                                size_bytes: entry.size.unwrap_or(0),
                                completed_unix_seconds: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                            };
                            serde_json::to_writer(&mut journal, &record)?;
                            journal.write_all(b"\n")?;
                            journal.sync_all()?;
                            completed_records.insert((record.path, record.blob_sha, record.size_bytes));
                            Ok(())
                        });
                    match result {
                        Ok(()) => {
                            downloaded += 1;
                            manifest_files.push(models::DownloadedFile {
                                path: entry.path.clone(),
                                blob_sha: entry.sha.clone(),
                                size_bytes: entry.size.unwrap_or(0),
                            });
                        }
                        Err(error) => errors.push(format!("{} : {:#}", entry.path, error)),
                    }
                    let _ = progress_sender.send(DownloadMessage::Progress(index + 1, total, entry.path));
                }
                let manifest_status = if manifest_conflict {
                    "Manifeste ignoré : un chemin distant sélectionné porte le même nom.".to_string()
                } else if !manifest_files.is_empty() && !commit_sha.is_empty() {
                    if workspace_fs.symlink_metadata(&manifest_relative).is_ok() {
                        "Manifeste existant non écrasé.".to_string()
                    } else {
                        let manifest = models::DownloadManifest {
                            schema_version: 1,
                            provider: "github".to_string(),
                            owner: owner.clone(),
                            repository: repository.clone(),
                            requested_reference: if requested_reference.trim().is_empty() { "default-branch".to_string() } else { requested_reference },
                            commit_sha: commit_sha.clone(),
                            tree_sha: tree_sha.clone(),
                            downloaded_unix_seconds: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                            downloaded_files: manifest_files,
                        };
                        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
                        workspace_fs.write_new(&manifest_relative, &manifest_bytes)
                            .context("Impossible d'écrire le manifeste sans écraser un fichier existant")?;
                        format!(" Manifeste de provenance : {}", manifest_relative.display())
                    }
                } else {
                    String::new()
                };
                let cancellation_status = if was_cancelled {
                    format!(" Téléchargement annulé après {}/{} fichier(s) traité(s).", processed, total)
                } else {
                    String::new()
                };
                if errors.is_empty() {
                    Ok(format!("{} fichier(s) téléchargé(s) dans {}.{}{} Sources GitHub inchangées.", downloaded, canonical_root.display(), manifest_status, cancellation_status))
                } else {
                    Ok(format!("{} fichier(s) téléchargé(s), {} erreur(s). Première erreur : {}.{}{}", downloaded, errors.len(), errors[0], manifest_status, cancellation_status))
                }
            })().unwrap_or_else(|error| format!("Échec du téléchargement : {:#}", error));
            let _ = sender.send(DownloadMessage::Finished(result));
        });
    }
}

struct WorkspaceDir {
    inner: CapabilityDir,
}

impl WorkspaceDir {
    fn open(path: &Path) -> anyhow::Result<Self> {
        Ok(Self { inner: CapabilityDir::open_ambient_dir(path, cap_std::ambient_authority())? })
    }

    fn parent_and_name<'a>(&self, relative: &'a Path, create: bool) -> anyhow::Result<(CapabilityDir, &'a std::ffi::OsStr)> {
        validate_relative_path(relative)?;
        let name = relative.file_name().context("Nom de fichier relatif absent")?;
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let directory = if parent.as_os_str().is_empty() {
            self.inner.try_clone()?
        } else {
            if create { self.inner.create_dir_all(parent)?; }
            self.inner.open_dir(parent)?
        };
        Ok((directory, name))
    }

    fn create_dir_all(&self, relative: &Path) -> anyhow::Result<()> {
        validate_relative_path(relative)?;
        self.inner.create_dir_all(relative)?;
        Ok(())
    }

    fn create_new(&self, relative: &Path) -> anyhow::Result<cap_std::fs::File> {
        let (parent, name) = self.parent_and_name(relative, true)?;
        let mut options = CapabilityOpenOptions::new();
        options.write(true).create_new(true);
        Ok(parent.open_with(name, &options)?)
    }

    fn open_append(&self, relative: &Path) -> anyhow::Result<cap_std::fs::File> {
        let (parent, name) = self.parent_and_name(relative, true)?;
        let mut options = CapabilityOpenOptions::new();
        options.append(true).create(true);
        Ok(parent.open_with(name, &options)?)
    }

    fn open_read(&self, relative: &Path) -> anyhow::Result<cap_std::fs::File> {
        let (parent, name) = self.parent_and_name(relative, false)?;
        Ok(parent.open(name)?)
    }

    fn read_to_string(&self, relative: &Path) -> anyhow::Result<String> {
        Ok(self.inner.read_to_string(relative)?)
    }

    fn symlink_metadata(&self, relative: &Path) -> anyhow::Result<cap_std::fs::Metadata> {
        Ok(self.inner.symlink_metadata(relative)?)
    }

    fn remove_file(&self, relative: &Path) -> anyhow::Result<()> {
        Ok(self.inner.remove_file(relative)?)
    }

    fn write_new(&self, relative: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        let mut file = self.create_new(relative)?;
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = self.remove_file(relative);
            return Err(error.into());
        }
        Ok(())
    }
}

fn validate_relative_path(relative: &Path) -> anyhow::Result<()> {
    if relative.as_os_str().is_empty() {
        anyhow::bail!("Chemin relatif workspace invalide : {}", relative.display());
    }
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            anyhow::bail!("Composant de chemin interdit dans le workspace : {}", relative.display());
        };
        if name.to_string_lossy().contains(':') {
            anyhow::bail!("Flux de données alternatif Windows refusé : {}", relative.display());
        }
    }
    Ok(())
}

fn safe_workspace_relative(relative: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(relative);
    validate_relative_path(&path)?;
    Ok(path)
}

fn cached_github_blob(
    workspace: &WorkspaceDir,
    owner: &str,
    repository: &str,
    commit_sha: &str,
    blob_sha: &str,
    expected_size: Option<u64>,
    token: &str,
) -> anyhow::Result<PathBuf> {
    fn safe_segment(value: &str) -> bool {
        !value.is_empty()
            && value != "."
            && value != ".."
            && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    }
    if !safe_segment(owner) || !safe_segment(repository) || !safe_segment(commit_sha) || !safe_segment(blob_sha) {
        anyhow::bail!("Identifiant de cache GitHub invalide.");
    }
    let size = expected_size.context("Taille du blob absente.")?;
    let cache_directory = PathBuf::from(".mtg-auditor-cache").join("github")
        .join(owner).join(repository).join(commit_sha);
    workspace.create_dir_all(&cache_directory)?;
    let cache_file = cache_directory.join(format!("{}.blob", blob_sha));
    if let Ok(metadata) = workspace.symlink_metadata(&cache_file) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            anyhow::bail!("Entrée de cache non régulière ou lien symbolique refusé.");
        }
        let valid = workspace.open_read(&cache_file)
            .map_or(false, |mut file| github::verify_git_blob_reader(blob_sha, &mut file, size).is_ok());
        if valid { return Ok(cache_file); }
        workspace.remove_file(&cache_file).context("Impossible d'écarter une entrée de cache invalide")?;
    }

    let temporary = cache_directory.join(format!(".{}.{}.tmp", blob_sha, std::process::id()));
    if let Ok(metadata) = workspace.symlink_metadata(&temporary) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            anyhow::bail!("Fichier temporaire de cache non régulier ou lien symbolique refusé.");
        }
        workspace.remove_file(&temporary).context("Impossible de nettoyer un fichier temporaire abandonné")?;
    }

    github::download_blob_to_file(owner, repository, blob_sha, token, expected_size, &workspace.inner, &temporary)
        .context("Téléchargement streaming du blob impossible")?;
    let verified = workspace.open_read(&temporary)
        .map_err(anyhow::Error::from)
        .and_then(|mut file| github::verify_git_blob_reader(blob_sha, &mut file, size));
    if let Err(error) = verified {
        let _ = workspace.remove_file(&temporary);
        return Err(error).context("Vérification du fichier temporaire de cache impossible");
    }

    match workspace.inner.hard_link(&temporary, &workspace.inner, &cache_file) {
        Ok(()) => {
            workspace.remove_file(&temporary)?;
            Ok(cache_file)
        }
        Err(_error) if workspace.symlink_metadata(&cache_file).is_ok() => {
            let _ = workspace.remove_file(&temporary);
            let mut existing = workspace.open_read(&cache_file)?;
            github::verify_git_blob_reader(blob_sha, &mut existing, size)
                .context("Une entrée de cache concurrente existe mais son empreinte est invalide")?;
            Ok(cache_file)
        }
        Err(error) => {
            let _ = workspace.remove_file(&temporary);
            Err(error).context("Publication atomique du cache sans écrasement impossible")
        }
    }
}

fn write_new_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("Le fichier existe déjà ou ne peut pas être créé : {}", path.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

fn safe_github_namespace_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

impl eframe::App for AuditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut scan_messages = Vec::new();
        if let Some(receiver) = self.scan_receiver.as_ref() {
            loop {
                match receiver.try_recv() {
                    Ok(message) => {
                        let finished = matches!(&message, ScanMessage::Finished(_));
                        scan_messages.push(message);
                        if finished { break; }
                    }
                    Err(TryRecvError::Disconnected) => {
                        scan_messages.push(ScanMessage::Finished(Err("Le worker d'inventaire s'est arrêté sans résultat.".to_string())));
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        for message in scan_messages {
            match message {
                ScanMessage::Progress(done, total) => {
                    self.status = format!("Analyse en cours : {} / {} octets…", done, total);
                }
                ScanMessage::Finished(result) => {
                    self.scan_receiver = None;
                    self.scan_cancel = None;
                    self.busy = false;
                    match result {
                        Ok(document) => {
                            self.status = format!(
                                "Inventaire terminé : {} fichiers, {} octets, {} doublon(s) exact(s), {} source(s). Sources en lecture seule.",
                                document.files_scanned, document.total_bytes, document.duplicate_files, document.source_roots.len().max(1)
                            );
                            self.local_inventory = Some(document);
                        }
                        Err(error) => self.status = format!("Erreur d'inventaire : {}", error),
                    }
                }
            }
        }
        let completed_github = self.github_receiver.as_ref().and_then(|receiver| match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => Some(Err("Le worker GitHub s'est arrêté sans résultat.".to_string())),
            Err(TryRecvError::Empty) => None,
        });
        if let Some(result) = completed_github {
            self.github_receiver = None;
            self.busy = false;
            match result {
                Ok(listing) => {
                    let prefix = self.subdirectory.trim_matches('/').to_string();
                    self.github_commit_sha = listing.commit_sha;
                    self.github_tree_sha = listing.tree_sha;
                    self.entries = listing.entries.into_iter()
                        .filter(|entry| prefix.is_empty() || entry.path == prefix || entry.path.starts_with(&format!("{}/", prefix)))
                        .collect();
                    self.selected.clear();
                    self.status = format!("{} entrées trouvées — commit {}. Sources distantes inchangées.", self.entries.len(), &self.github_commit_sha[..7.min(self.github_commit_sha.len())]);
                }
                Err(error) => self.status = format!("Erreur GitHub : {}", error),
            }
        }
        let mut download_messages = Vec::new();
        if let Some(receiver) = self.download_receiver.as_ref() {
            loop {
                match receiver.try_recv() {
                    Ok(message) => {
                        let finished = matches!(&message, DownloadMessage::Finished(_));
                        download_messages.push(message);
                        if finished { break; }
                    }
                    Err(TryRecvError::Disconnected) => {
                        download_messages.push(DownloadMessage::Finished("Le worker de téléchargement s'est arrêté sans résultat.".to_string()));
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        for message in download_messages {
            match message {
                DownloadMessage::Progress(done, total, path) => {
                    self.status = format!("Téléchargement : {} / {} — {}", done, total, path);
                }
                DownloadMessage::Finished(result) => {
                    self.download_receiver = None;
                    self.download_cancel = None;
                    self.busy = false;
                    self.status = result;
                }
            }
        }
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MTG Asset Auditor");
            ui.label("Audit local et exploration de dépôts GitHub configurables");
            ui.separator();

            ui.heading("Source locale");
            ui.label("Un ou plusieurs dossiers (un chemin par ligne) :");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::multiline(&mut self.local_source).desired_rows(2).desired_width(520.0));
                if ui.button("Ajouter un dossier…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        let value = path.display().to_string();
                        if !self.local_source.lines().any(|line| line.trim() == value) {
                            if !self.local_source.trim().is_empty() { self.local_source.push('\n'); }
                            self.local_source.push_str(&value);
                        }
                    }
                }
                if ui.button("Effacer la sélection").clicked() { self.local_source.clear(); }
                if ui.add_enabled(!self.busy, egui::Button::new("Analyser")).clicked() {
                    self.scan_local();
                }
                if ui.add_enabled(self.scan_cancel.is_some(), egui::Button::new("Annuler le scan")).clicked() {
                    self.cancel_scan();
                }
            });
            if let Some((files_scanned, total_bytes, duplicate_files, schema_version)) = self.local_inventory.as_ref().map(|document| {
                (document.files_scanned, document.total_bytes, document.duplicate_files, document.schema_version)
            }) {
                ui.label(format!("{} fichiers • {} octets • {} doublons exacts • schéma v{}", files_scanned, total_bytes, duplicate_files, schema_version));
                ui.horizontal(|ui| {
                    if ui.button("Exporter JSON…").clicked() { self.export_local_json(); }
                    if ui.button("Exporter CSV…").clicked() { self.export_local_csv(); }
                    if ui.button("Rapport HTML…").clicked() { self.export_local_html(); }
                    ui.label("Page planche");
                    ui.add(egui::TextEdit::singleline(&mut self.contact_page).desired_width(36.0));
                    if ui.button("Planche PNG…").clicked() { self.export_contact_sheet(); }
                    if ui.button("Comparer JSON…").clicked() { self.compare_local_inventory(); }
                });
                if let Some((added, removed, changed, unchanged)) = self.comparison.as_ref().map(|comparison| {
                    (comparison.added_paths.len(), comparison.removed_paths.len(),
                     comparison.changed_assets.len(), comparison.unchanged_files)
                }) {
                    ui.label(format!(
                        "Comparaison : +{} / −{} / modifiés {} / inchangés {}",
                        added, removed, changed, unchanged
                    ));
                    if ui.button("Exporter comparaison JSON…").clicked() {
                        self.export_comparison();
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Filtrer les résultats");
                    ui.text_edit_singleline(&mut self.local_filter);
                });
                let filter = self.local_filter.to_lowercase();
                if let Some(document) = self.local_inventory.as_ref() {
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        let mut shown = 0usize;
                        for asset in &document.assets {
                            if !filter.is_empty() && !asset.relative_path.to_lowercase().contains(&filter) {
                                continue;
                            }
                            if shown >= 500 {
                                ui.label("Affichage limité aux 500 premiers résultats correspondants ; l'export contient l'inventaire complet.");
                                break;
                            }
                            shown += 1;
                            ui.horizontal_wrapped(|ui| {
                                ui.label(&asset.relative_path);
                                ui.label(egui::RichText::new(format!("{} • {} octets", asset.detected_family, asset.size_bytes)).weak());
                                if let (Some(width), Some(height)) = (asset.raster_width, asset.raster_height) {
                                    ui.label(format!("{}×{}", width, height));
                                }
                                if let Some(color_type) = &asset.raster_color_type {
                                    ui.label(color_type);
                                }
                                if let Some(has_alpha) = asset.raster_has_alpha {
                                    ui.label(if has_alpha { "Alpha" } else { "Opaque" });
                                }
                                if let Some(viewbox) = &asset.svg_viewbox {
                                    ui.label(format!("viewBox {}", viewbox));
                                }
                                if let Some(family) = &asset.font_family {
                                    ui.label(format!("Police : {}", family));
                                }
                                if let Some(count) = asset.archive_entry_count {
                                    ui.label(format!("ZIP : {} entrées", count));
                                }
                                if let Some(notice) = &asset.notice_kind {
                                    ui.colored_label(egui::Color32::LIGHT_GREEN, format!("Notice : {}", notice));
                                }
                                if let Some(first_path) = &asset.duplicate_of {
                                    ui.colored_label(egui::Color32::LIGHT_BLUE, format!("Doublon de {}", first_path));
                                }
                                if let Some(warning) = &asset.analysis_warning {
                                    ui.colored_label(egui::Color32::YELLOW, warning);
                                }
                            });
                        }
                        if shown == 0 {
                            ui.label("Aucun résultat correspondant.");
                        }
                    });
                }
            }

            ui.separator();
            ui.heading("Source GitHub");
            ui.horizontal(|ui| {
                ui.label("URL du dépôt");
                ui.text_edit_singleline(&mut self.repository_url);
                if ui.button("Lire l'URL").clicked() { self.parse_url(); }
            });
            ui.horizontal(|ui| {
                ui.label("Propriétaire");
                ui.text_edit_singleline(&mut self.owner);
                ui.label("Dépôt");
                ui.text_edit_singleline(&mut self.repository);
            });
            ui.horizontal(|ui| {
                ui.label("Branche / tag / commit (vide = défaut)");
                ui.text_edit_singleline(&mut self.branch);
            });
            ui.horizontal(|ui| {
                ui.label("Sous-dossier");
                ui.text_edit_singleline(&mut self.subdirectory);
            });
            ui.horizontal(|ui| {
                ui.label("Jeton GitHub (dépôt privé)");
                let mut token_edit = Zeroizing::new(self.token.to_string());
                let response = ui.add(egui::TextEdit::singleline(&mut *token_edit).password(true).desired_width(300.0));
                if response.changed() {
                    self.token = Zeroizing::new(token_edit.to_string());
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Charger le jeton enregistré").clicked() { self.load_github_token(); }
                if ui.button("Enregistrer dans Windows").clicked() { self.save_github_token(); }
                if ui.button("Supprimer le jeton enregistré").clicked() { self.delete_github_token(); }
            });
            if ui.add_enabled(!self.busy, egui::Button::new("Explorer le dépôt")).clicked() {
                self.connect();
            }

            ui.separator();
            ui.heading("Espace de travail");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.workspace);
                if ui.button("Parcourir…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.workspace = path.display().to_string();
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.busy && !self.selected.is_empty(), egui::Button::new("Télécharger la sélection")).clicked() {
                    self.download_selected();
                }
                ui.label(format!("{} fichier(s) sélectionné(s)", self.selected.len()));
                if ui.add_enabled(self.download_cancel.is_some(), egui::Button::new("Annuler le téléchargement")).clicked() {
                    self.cancel_download();
                }
            });
            ui.separator();
            ui.heading("Fichiers distants");
            ui.horizontal(|ui| {
                ui.label("Filtrer le chemin");
                ui.text_edit_singleline(&mut self.remote_filter);
                ui.label("Extension");
                ui.text_edit_singleline(&mut self.remote_extension_filter);
                ui.label("Taille min. (octets)");
                ui.add(egui::TextEdit::singleline(&mut self.remote_min_size).desired_width(70.0));
                ui.label("max.");
                ui.add(egui::TextEdit::singleline(&mut self.remote_max_size).desired_width(70.0));
                ui.label("Famille");
                ui.add(egui::TextEdit::singleline(&mut self.remote_family_filter).desired_width(90.0));
            });
            let remote_filter = self.remote_filter.trim().to_lowercase();
            let extension_filter = self.remote_extension_filter.trim().trim_start_matches('.').to_lowercase();
            let family_filter = self.remote_family_filter.trim().to_lowercase();
            let minimum_size = self.remote_min_size.trim().parse::<u64>().ok();
            let maximum_size = self.remote_max_size.trim().parse::<u64>().ok();
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for entry in &self.entries {
                    if entry.kind != "blob" { continue; }
                    if !remote_filter.is_empty() && !entry.path.to_lowercase().contains(&remote_filter) {
                        continue;
                    }
                    if !extension_filter.is_empty() {
                        let extension = Path::new(&entry.path).extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or_default()
                            .to_lowercase();
                        if extension != extension_filter { continue; }
                    }
                    let family = Path::new(&entry.path).extension()
                        .and_then(|value| value.to_str())
                        .map(inventory::classify_extension)
                        .unwrap_or("unknown");
                    if !family_filter.is_empty() && family != family_filter { continue; }
                    let size = entry.size.unwrap_or(0);
                    if minimum_size.is_some_and(|minimum| size < minimum) { continue; }
                    if maximum_size.is_some_and(|maximum| size > maximum) { continue; }
                    let mut checked = self.selected.contains(&entry.path);
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut checked, "").changed() {
                            if checked { self.selected.insert(entry.path.clone()); }
                            else { self.selected.remove(&entry.path); }
                        }
                        ui.label(&entry.path);
                        ui.label(egui::RichText::new(format!("{} octets", entry.size.unwrap_or(0))).weak());
                    });
                }
            });
            ui.separator();
            ui.label(&self.status);
            if self.busy { ui.spinner(); }
            ui.small("Inventaire local SHA-256, métadonnées raster initiales, exports JSON/CSV et exploration GitHub par URL. Les sources ne sont jamais modifiées.");
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "MTG Asset Auditor",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(AuditorApp::default()))),
    )
}

#[cfg(test)]
mod write_tests {
    use super::{safe_github_namespace_segment, write_new_file};
    use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn github_namespace_segments_reject_path_traversal_and_separators() {
        assert!(safe_github_namespace_segment("owner-1"));
        assert!(safe_github_namespace_segment("repo_name.v2"));
        assert!(!safe_github_namespace_segment(""));
        assert!(!safe_github_namespace_segment("."));
        assert!(!safe_github_namespace_segment(".."));
        assert!(!safe_github_namespace_segment("../outside"));
        assert!(!safe_github_namespace_segment("owner/repository"));
        assert!(!safe_github_namespace_segment("owner\\\\repository"));
    }

    #[test]
    fn workspace_capability_rejects_escape_and_preserves_existing_files() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("mtg-auditor-cap-test-{}-{}", std::process::id(), nonce));
        fs::create_dir_all(&directory).unwrap();
        let workspace = super::WorkspaceDir::open(&directory).unwrap();
        let nested = PathBuf::from("nested/folder/asset.bin");
        workspace.write_new(&nested, b"original").unwrap();
        assert!(workspace.write_new(&nested, b"replacement").is_err());
        assert_eq!(workspace.inner.read(&nested).unwrap(), b"original");
        assert!(super::safe_workspace_relative("../outside").is_err());
        assert!(super::safe_workspace_relative("C:\\outside").is_err());
        assert!(super::safe_workspace_relative("folder/file.txt:alternate").is_err());
        drop(workspace);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exclusive_write_never_overwrites_existing_files() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("mtg-auditor-write-test-{}-{}", std::process::id(), nonce));
        fs::create_dir_all(&directory).unwrap();
        let destination: PathBuf = directory.join("asset.bin");

        write_new_file(&destination, b"original").unwrap();
        assert!(write_new_file(&destination, b"replacement").is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"original");

        fs::remove_dir_all(directory).unwrap();
    }
}
