# Feuille de route

## Phase 0 — Fondation
- [x] Dépôt Rust initialisé.
- [x] Cahier des charges, architecture et sécurité.
- [x] Fenêtre desktop initiale.
- [x] Première exploration de l'arbre GitHub.
- [x] Téléchargement sélectif initial des blobs.
- [ ] Compilation Windows CI vérifiée.

## Phase 1 — Connectivité GitHub robuste
- [ ] Parser URL de dépôt.
- [ ] Détecter branche par défaut et résoudre SHA commit.
- [ ] Branches, tags, commits et sous-dossiers.
- [ ] Dépôts privés et stockage du jeton dans Windows Credential Manager.
- [ ] Arbres tronqués, sous-arbres, rate limits et reprise.
- [ ] Téléchargement streaming, taille max, cache et reprise.
- [ ] Filtres par extension, taille, chemin et famille.

## Phase 2 — Sources locales et inventaire
- [ ] Sélection multi-dossiers.
- [ ] Parcours parallèle et exclusions.
- [ ] Modèle versionné, IDs stables et SHA-256.
- [ ] Exports JSON/CSV.
- [ ] Comparaison entre analyses.

## Phase 3 — Analyse des formats
- [ ] Raster : dimensions, alpha, orientation et miniatures.
- [ ] SVG : XML, viewBox, sécurité et aperçu.
- [ ] Fonts : métadonnées.
- [ ] ZIP : inventaire sûr et chemins suspects.
- [ ] MSE : packages, dépendances et couches.
- [ ] Collecte des notices et provenance.

## Phase 4 — Revue visuelle
- [ ] Planches de contact paginées.
- [ ] Galerie HTML hors ligne.
- [ ] Filtres, comparaison et étiquettes.
- [ ] Doublons binaires et candidats visuels.
- [ ] Revue humaine persistante.

## Phase 5 — Rapports et intégration MTG
- [ ] Rapport HTML.
- [ ] Statistiques et alertes.
- [ ] Export vers staging, jamais directement vers LIBRARY.
- [ ] Réconciliation du manifeste MTG Card Remodeler.
- [ ] Tests de cohérence et non-régression.

## Phase 6 — Livraison
- [ ] CI test, clippy, build.
- [ ] Exécutable Windows x64 autonome.
- [ ] Versionnement et notes de version.
- [ ] GitHub Releases.
- [ ] Signature de code à étudier.
