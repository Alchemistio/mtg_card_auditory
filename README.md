# MTG Asset Auditor

Application desktop Windows native en Rust pour inventorier et auditer des ressources depuis un dossier local ou un dépôt GitHub choisi par l'utilisateur.

**État : prototype fonctionnel en développement.** Le scan local parallèle SHA-256, les exports JSON/CSV, les premières métadonnées raster, SVG et polices, l'inspection ZIP sans extraction, l'exploration GitHub configurable et le repli sur sous-arbres en cas d'arbre récursif tronqué sont intégrés au code. La validation Windows complète du dernier commit reste en cours dans GitHub Actions. Un commit antérieur avec raster/SVG/polices a passé Clippy, les tests et le build Release Windows.

## Fonctionnalités présentes dans le socle
- Interface desktop française (egui).
- Source GitHub configurable par URL ou propriétaire/dépôt.
- Résolution de la branche par défaut et des références branche/tag/commit vers un SHA de commit et un SHA d'arbre.
- Exploration récursive de l'arbre GitHub ; repli sur parcours de sous-arbres si l'arbre récursif est tronqué, avec garde-fous de profondeur et de cycles.
- Jeton GitHub saisi dans un champ masqué, conservé en mémoire seulement.
- Téléchargement sélectif des blobs dans un workspace, sans écrasement volontaire des fichiers existants.
- Scan local parallèle via Rayon, sans suivre les liens symboliques.
- SHA-256 calculé en flux, inventaire versionné, tri stable et classification indicative par extension.
- Raster : format détecté et dimensions lues depuis l'en-tête, sans décodage des pixels.
- SVG : contrôle XML, lecture du viewBox et détection indicative de références externes ; aucun SVG n'est rendu ou exécuté.
- Polices : extraction initiale de la famille et sous-famille lorsque le format est pris en charge.
- ZIP : lecture de la table centrale, comptage des entrées, taille décompressée déclarée, chemins suspects et ratios anormaux ; aucune extraction.
- Exports JSON et CSV UTF-8 ; les exports dans le dossier source scanné sont refusés.
- Garde-fous contre les chemins distants dangereux et les liens symboliques dans les destinations de téléchargement.
- Tests unitaires du parsing d'URL, du classement, de l'inventaire, du SVG et de la détection de chemins ZIP suspects.

## Limites connues
- Gestion structurée des rate limits, streaming des blobs volumineux, cache et reprise.
- Credential Manager, progression asynchrone, annulation et scan multi-sources.
- Alpha/orientation, miniatures, aperçu SVG sandboxé, inspection CSS approfondie, métadonnées avancées de polices, 7z/RAR et analyse MSE.
- Galerie, planches, licences, provenance, revue humaine et rapport HTML.
- La défense contre les modifications concurrentes du système de fichiers (TOCTOU) n'est pas complète.
- L'interface est un prototype ; l'ergonomie et les tests de charge restent à faire.
- Un commit antérieur avec raster/SVG/polices a passé Clippy, les tests et le build Release Windows ; le dernier commit incluant ZIP et scan parallèle doit encore être validé.

## Développement
Prérequis : Rust stable et Visual Studio Build Tools (MSVC).

    cargo fmt --all
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all
    cargo run
    cargo build --release

## Sécurité et GitHub
L'application analyse les sources en lecture seule. Les téléchargements sont écrits dans le workspace choisi ; les exports locaux sont refusés à l'intérieur du dossier analysé. Les scripts, macros et contenus HTML ne sont jamais exécutés. Les SVG sont analysés comme XML sans rendu. Les ZIP ne sont jamais extraits.

Pour un dépôt privé, utiliser un jeton à permissions minimales (Contents: read). Ne jamais le commiter ni l'inclure dans les rapports. Il n'est actuellement conservé qu'en mémoire pendant l'exécution.

Le dépôt cible est configurable : aucun dépôt MTG n'est codé en dur.

## Documentation
- [Cahier des charges](docs/REQUIREMENTS.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Feuille de route](docs/ROADMAP.md)
- [Sécurité](docs/SECURITY.md)
