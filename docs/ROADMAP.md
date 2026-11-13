# Feuille de route

## Phase 0 — Fondation
- [x] Dépôt Rust initialisé.
- [x] Cahier des charges, architecture et sécurité.
- [x] Fenêtre desktop initiale.
- [x] Première exploration de l'arbre GitHub.
- [x] Téléchargement sélectif initial des blobs.
- [x] CI Windows complète sur commit `6310f92` : Clippy, tests, build Release et upload d'artefact.
- [ ] Revalidation du dernier commit après ajout des métadonnées alpha et planches de contact.

## Phase 1 — Connectivité GitHub robuste
- [x] Parser URL de dépôt GitHub.
- [x] Détecter la branche par défaut et résoudre branche/tag/commit en SHA immuable.
- [x] Résoudre le SHA de l'arbre Git à partir du commit.
- [x] Si l'arbre récursif est tronqué, parcourir les sous-arbres avec détection de cycle et limite de profondeur.
- [x] Sous-dossier filtré côté interface.
- [x] Exploration et téléchargement GitHub dans des workers d'arrière-plan.
- [ ] Dépôts privés et stockage du jeton dans Windows Credential Manager.
- [ ] Gestion structurée des rate limits, temporisation et reprise.
- [ ] Téléchargement streaming, taille max, cache et reprise.
- [ ] Filtres par extension, taille, chemin et famille.

## Phase 2 — Sources locales et inventaire
- [ ] Sélection multi-dossiers.
- [x] Parcours local parallèle via Rayon, sans suivre les liens symboliques.
- [x] Scan local exécuté dans un worker d'arrière-plan.
- [x] Modèle d'inventaire versionné (v1), tri stable et SHA-256 en flux.
- [x] Détection des doublons binaires exacts par SHA-256, sans suppression.
- [x] Classification indicative par extension.
- [x] Métadonnées raster : format, dimensions, type de canaux et présence d'alpha depuis le décodeur sans décodage des pixels.
- [x] Métadonnées SVG initiales : XML bien formé, viewBox et détection indicative de références externes.
- [x] Métadonnées de polices : famille et sous-famille lorsque lisibles par ttf-parser.
- [x] Inspection ZIP sans extraction : nombre d'entrées, taille déclarée, chemins suspects et ratios anormaux.
- [x] Export JSON versionné, CSV UTF-8 et rapport HTML autonome hors ligne.
- [x] Génération de planches raster PNG paginées (16 éléments/page) avec index texte ; SHA vérifié avant aperçu.
- [x] Refus des exports dans l'arborescence source.
- [x] Vue filtrable des résultats locaux dans l'interface (500 premières lignes affichées, exports complets).
- [ ] Comparaison entre analyses.
- [ ] Progression détaillée, annulation et reprise pour les scans longs.

## Phase 3 — Analyse des formats
- [~] Raster : format, dimensions, canaux et alpha ; orientation, limites, décodage contrôlé et miniatures à faire.
- [~] SVG : parsing XML sans exécution, viewBox et détection de références externes ; aperçu sandboxé et inspection CSS complète à faire.
- [~] Fonts : famille/sous-famille ; métadonnées avancées et formats non pris en charge à traiter.
- [~] ZIP : inspection de la table centrale sans extraction ; analyse de formats 7z/RAR à faire.
- [ ] MSE : packages, dépendances et couches.
- [ ] Collecte des notices et provenance.

## Phase 4 — Revue visuelle
- [~] Première planche de contact raster paginée avec index.
- [ ] Galerie visuelle HTML avec navigation, filtres et comparaison.
- [ ] Planche contact avancée avec libellés, annotations et sélection.
- [ ] Candidats visuels similaires (les doublons binaires exacts sont déjà identifiés).
- [ ] Revue humaine persistante.

## Phase 5 — Rapports et intégration MTG
- [x] Rapport HTML autonome initial.
- [ ] Statistiques et alertes enrichies.
- [ ] Export vers staging, jamais directement vers LIBRARY.
- [ ] Réconciliation du manifeste MTG Card Remodeler.
- [ ] Tests de cohérence et non-régression.

## Phase 6 — Livraison
- [ ] CI test, clippy, build sur le dernier commit.
- [ ] Exécutable Windows x64 autonome.
- [ ] Versionnement et notes de version.
- [ ] GitHub Releases.
- [ ] Signature de code à étudier.
