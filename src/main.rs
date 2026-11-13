mod github;
mod inventory;
mod models;

use anyhow::Context;
use eframe::egui;
use std::{collections::HashSet, path::{Path, PathBuf}, sync::mpsc::{self, Receiver, TryRecvError}, time::Duration};

#[derive(Default)]
struct AuditorApp {
    repository_url: String,
    owner: String,
    repository: String,
    branch: String,
    subdirectory: String,
    token: String,
    workspace: String,
    local_source: String,
    local_filter: String,
    contact_page: String,
    local_inventory: Option<inventory::InventoryDocument>,
    comparison: Option<inventory::InventoryComparison>,
    scan_receiver: Option<Receiver<std::result::Result<inventory::InventoryDocument, String>>>,
    github_receiver: Option<Receiver<std::result::Result<Vec<models::RemoteEntry>, String>>>,
    download_receiver: Option<Receiver<String>>,
    status: String,
    entries: Vec<models::RemoteEntry>,
    selected: HashSet<String>,
    busy: bool,
}

impl AuditorApp {
    fn connect(&mut self) {
        let owner = self.owner.clone();
        let repository = self.repository.clone();
        let branch = self.branch.clone();
        let token = self.token.clone();
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
        let source = PathBuf::from(self.local_source.trim());
        let (sender, receiver) = mpsc::channel();
        self.scan_receiver = Some(receiver);
        self.busy = true;
        self.status = "Inventaire local parallèle et calcul SHA-256 en arrière-plan…".into();
        std::thread::spawn(move || {
            let result = inventory::scan_directory(&source).map_err(|error| format!("{:#}", error));
            let _ = sender.send(result);
        });
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
        let token = self.token.clone();
        let (sender, receiver) = mpsc::channel();
        self.download_receiver = Some(receiver);
        self.busy = true;
        self.status = "Téléchargement GitHub en arrière-plan…".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                std::fs::create_dir_all(&root)?;
                let canonical_root = root.canonicalize()?;
                let mut downloaded = 0usize;
                let mut errors = Vec::new();
                for entry in chosen {
                    let result = github::download_blob(&owner, &repository, &entry.sha, &token)
                        .and_then(|bytes| {
                            let destination = safe_destination(&canonical_root, &entry.path)?;
                            if destination.exists() {
                                anyhow::bail!("Le fichier existe déjà : {}", destination.display());
                            }
                            if let Some(parent) = destination.parent() { std::fs::create_dir_all(parent)?; }
                            let canonical_parent = destination.parent().context("Parent de destination absent")?.canonicalize()?;
                            if !canonical_parent.starts_with(&canonical_root) {
                                anyhow::bail!("Le chemin de destination sort du workspace : {}", entry.path);
                            }
                            std::fs::write(destination, bytes)?;
                            Ok(())
                        });
                    match result {
                        Ok(()) => downloaded += 1,
                        Err(error) => errors.push(format!("{} : {:#}", entry.path, error)),
                    }
                }
                if errors.is_empty() {
                    Ok(format!("{} fichier(s) téléchargé(s) dans {}. Sources GitHub inchangées.", downloaded, canonical_root.display()))
                } else {
                    Ok(format!("{} fichier(s) téléchargé(s), {} erreur(s). Première erreur : {}", downloaded, errors.len(), errors[0]))
                }
            })().unwrap_or_else(|error| format!("Échec du téléchargement : {:#}", error));
            let _ = sender.send(result);
        });
    }
}

fn safe_destination(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let canonical_root = root.canonicalize()?;
    let mut destination = canonical_root.clone();
    for component in Path::new(relative).components() {
        match component {
            std::path::Component::Normal(part) => {
                destination.push(part);
                if let Ok(metadata) = std::fs::symlink_metadata(&destination) {
                    if metadata.file_type().is_symlink() {
                        anyhow::bail!("Lien symbolique refusé dans le workspace : {}", destination.display());
                    }
                }
            }
            _ => anyhow::bail!("Chemin distant non sûr : {}", relative),
        }
    }
    if let Some(parent) = destination.parent() {
        if parent.exists() && !parent.canonicalize()?.starts_with(&canonical_root) {
            anyhow::bail!("Le chemin de destination sort du workspace : {}", relative);
        }
    }
    Ok(destination)
}

impl eframe::App for AuditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let completed_scan = self.scan_receiver.as_ref().and_then(|receiver| match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => Some(Err("Le worker d'inventaire s'est arrêté sans résultat.".to_string())),
            Err(TryRecvError::Empty) => None,
        });
        if let Some(result) = completed_scan {
            self.scan_receiver = None;
            self.busy = false;
            match result {
                Ok(document) => {
                    self.status = format!(
                        "Inventaire terminé : {} fichiers, {} octets, {} doublon(s) exact(s). Sources en lecture seule.",
                        document.files_scanned, document.total_bytes, document.duplicate_files
                    );
                    self.local_inventory = Some(document);
                }
                Err(error) => self.status = format!("Erreur d'inventaire : {}", error),
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
                Ok(entries) => {
                    let prefix = self.subdirectory.trim_matches('/').to_string();
                    self.entries = entries.into_iter()
                        .filter(|entry| prefix.is_empty() || entry.path == prefix || entry.path.starts_with(&format!("{}/", prefix)))
                        .collect();
                    self.selected.clear();
                    self.status = format!("{} entrées trouvées. Sources distantes inchangées.", self.entries.len());
                }
                Err(error) => self.status = format!("Erreur GitHub : {}", error),
            }
        }
        let completed_download = self.download_receiver.as_ref().and_then(|receiver| match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => Some("Le worker de téléchargement s'est arrêté sans résultat.".to_string()),
            Err(TryRecvError::Empty) => None,
        });
        if let Some(result) = completed_download {
            self.download_receiver = None;
            self.busy = false;
            self.status = result;
        }
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MTG Asset Auditor");
            ui.label("Audit local et exploration de dépôts GitHub configurables");
            ui.separator();

            ui.heading("Source locale");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.local_source);
                if ui.button("Choisir un dossier…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.local_source = path.display().to_string();
                    }
                }
                if ui.add_enabled(!self.busy, egui::Button::new("Analyser")).clicked() {
                    self.scan_local();
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
                if let Some(comparison) = self.comparison.as_ref() {
                    ui.label(format!(
                        "Comparaison : +{} / −{} / modifiés {} / inchangés {}",
                        comparison.added_paths.len(), comparison.removed_paths.len(),
                        comparison.changed_assets.len(), comparison.unchanged_files
                    ));
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
                ui.add(egui::TextEdit::singleline(&mut self.token).password(true).desired_width(300.0));
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
            });
            ui.separator();
            ui.heading("Fichiers distants");
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for entry in &self.entries {
                    if entry.kind != "blob" { continue; }
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
