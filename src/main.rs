mod github;
mod models;

use eframe::egui;
use std::{collections::HashSet, path::{Path, PathBuf}};

#[derive(Default)]
struct AuditorApp {
    owner: String,
    repository: String,
    branch: String,
    subdirectory: String,
    token: String,
    workspace: String,
    status: String,
    entries: Vec<models::RemoteEntry>,
    selected: HashSet<String>,
    busy: bool,
}

impl AuditorApp {
    fn connect(&mut self) {
        self.busy = true;
        self.status = "Exploration de l'arbre GitHub…".into();
        match github::list_repository_tree(&self.owner, &self.repository, &self.branch, &self.token) {
            Ok(entries) => {
                let prefix = self.subdirectory.trim_matches('/');
                self.entries = entries.into_iter()
                    .filter(|e| prefix.is_empty() || e.path == prefix || e.path.starts_with(&format!("{}/", prefix)))
                    .collect();
                self.selected.clear();
                self.status = format!("{} entrées trouvées. Sources distantes inchangées.", self.entries.len());
            }
            Err(error) => self.status = format!("Erreur GitHub : {:#}", error),
        }
        self.busy = false;
    }

    fn download_selected(&mut self) {
        let root = PathBuf::from(self.workspace.trim());
        if self.workspace.trim().is_empty() {
            self.status = "Choisis d'abord un dossier de travail.".into();
            return;
        }
        if let Err(error) = std::fs::create_dir_all(&root) {
            self.status = format!("Impossible de créer le workspace : {}", error);
            return;
        }
        let chosen: Vec<_> = self.entries.iter()
            .filter(|e| self.selected.contains(&e.path) && e.kind == "blob")
            .cloned().collect();
        if chosen.is_empty() {
            self.status = "Aucun fichier sélectionné.".into();
            return;
        }
        self.busy = true;
        let mut downloaded = 0usize;
        for entry in chosen {
            let result = github::download_blob(&self.owner, &self.repository, &entry.sha, &self.token)
                .and_then(|bytes| {
                    let destination = safe_destination(&root, &entry.path)?;
                    if let Some(parent) = destination.parent() { std::fs::create_dir_all(parent)?; }
                    std::fs::write(destination, bytes)?;
                    Ok(())
                });
            match result {
                Ok(()) => downloaded += 1,
                Err(error) => self.status = format!("Erreur pour {} : {:#}", entry.path, error),
            }
        }
        self.busy = false;
        self.status = format!("{} fichier(s) téléchargé(s) dans {}. Sources GitHub inchangées.", downloaded, root.display());
    }
}

fn safe_destination(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let mut destination = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            std::path::Component::Normal(part) => destination.push(part),
            _ => anyhow::bail!("Chemin distant non sûr : {}", relative),
        }
    }
    Ok(destination)
}

impl eframe::App for AuditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MTG Asset Auditor");
            ui.label("Audit local et exploration de dépôts GitHub configurables");
            ui.separator();
            ui.heading("Source GitHub");
            ui.horizontal(|ui| {
                ui.label("Propriétaire");
                ui.text_edit_singleline(&mut self.owner);
                ui.label("Dépôt");
                ui.text_edit_singleline(&mut self.repository);
            });
            ui.horizontal(|ui| {
                ui.label("Branche (vide = HEAD)");
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
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
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
            ui.small("Socle initial : exploration GitHub et téléchargement sélectif. Les analyseurs et rapports arrivent par étapes.");
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
