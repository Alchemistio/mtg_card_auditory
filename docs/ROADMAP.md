# Feuille de route

## Phase 0 — Fondation
- [x] Dépôt Rust initialisé.
- [x] Cahier des charges, architecture et sécurité.
- [x] Fenêtre desktop initiale.
- [x] Première exploration de l'arbre GitHub.
- [x] Téléchargement sélectif initial des blobs.
- [ ] Compilation Windows CI vérifiée (correctifs Clippy et nouveaux tests en validation sur les runs récents).

## Phase 1 — Connectivité GitHub robuste
- [x] Parser URL de dépôt GitHub.
- [x] Détecter la branche par défaut et résoudre branche/tag/commit en SHA immuable.
- [x] Sous-dossier filtré côté interface.
- [ ] Dépôts privés et stockage du jeton dans Windows Credential Manager.
- [ ] Arbres tronqués, sous-arbres, rate limits et reprise.
- [ ] Téléchargement streaming, taille max, cache et reprise.
- [ ] Filtres par extension, taille, chemin et famille.

## Phase 2 — Sources locales et inventaire
- [ ] Sélection multi-dossiers.
- [ ] Parcours parallèle et exclusions configurables.
- [x] Modèle d'inventaire versionné (v1), tri stable et SHA-256 en flux.
- [x] Classification indicative par extension.
- [x] Métadonnées raster initiales : format détecté et dimensions depuis l'en-tête, sans décodage complet des images.
- [x] Export JSON versionné et CSV UTF-8.
- [x] Refus des exports dans l'arborescence source.
- [ ] Comparaison entre analyses.
- [ ] Progression réelle, annulation et reprise pour les scans longs.

## Phase 3 — Analyse des formats
- [~] Raster : détection du format et dimensions initiales ; alpha, orientation, limites et miniatures à faire.
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
