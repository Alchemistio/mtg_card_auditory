# Feuille de route

## Phase 0 — Fondation
- [x] Dépôt Rust initialisé.
- [x] Cahier des charges, architecture et sécurité.
- [x] Fenêtre desktop initiale.
- [x] Première exploration de l'arbre GitHub.
- [x] Téléchargement sélectif initial des blobs.
- [ ] Compilation Windows CI vérifiée sur le dernier commit (un commit antérieur avec raster/SVG/polices a passé tout le pipeline).

## Phase 1 — Connectivité GitHub robuste
- [x] Parser URL de dépôt GitHub.
- [x] Détecter la branche par défaut et résoudre branche/tag/commit en SHA immuable.
- [x] Résoudre le SHA de l'arbre Git à partir du commit.
- [x] Si l'arbre récursif est tronqué, parcourir les sous-arbres avec détection de cycle et limite de profondeur.
- [x] Sous-dossier filtré côté interface.
- [ ] Dépôts privés et stockage du jeton dans Windows Credential Manager.
- [ ] Gestion structurée des rate limits, temporisation et reprise.
- [ ] Téléchargement streaming, taille max, cache et reprise.
- [ ] Filtres par extension, taille, chemin et famille.

## Phase 2 — Sources locales et inventaire
- [ ] Sélection multi-dossiers.
- [x] Parcours local parallèle via Rayon, sans suivre les liens symboliques.
- [x] Modèle d'inventaire versionné (v1), tri stable et SHA-256 en flux.
- [x] Classification indicative par extension.
- [x] Métadonnées raster initiales : format détecté et dimensions depuis l'en-tête.
- [x] Métadonnées SVG initiales : XML bien formé, viewBox et détection indicative de références externes.
- [x] Métadonnées de polices : famille et sous-famille lorsque lisibles par ttf-parser.
- [x] Inspection ZIP sans extraction : nombre d'entrées, taille déclarée, chemins suspects et ratios anormaux.
- [x] Export JSON versionné et CSV UTF-8.
- [x] Refus des exports dans l'arborescence source.
- [ ] Comparaison entre analyses.
- [ ] Progression réelle, annulation et reprise pour les scans longs.

## Phase 3 — Analyse des formats
- [~] Raster : détection du format et dimensions initiales ; alpha, orientation, limites, décodage contrôlé et miniatures à faire.
- [~] SVG : parsing XML sans exécution, viewBox et détection de références externes ; aperçu sandboxé et inspection CSS complète à faire.
- [~] Fonts : famille/sous-famille ; métadonnées avancées et formats non pris en charge à traiter.
- [~] ZIP : inspection de la table centrale sans extraction ; analyse de formats 7z/RAR à faire.
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
