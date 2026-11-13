# MTG Asset Auditor

Application desktop Windows native en Rust pour inventorier et auditer des ressources depuis un dossier local ou un dépôt GitHub choisi par l'utilisateur.

**État : prototype fonctionnel en développement.** Le scan local SHA-256 et les exports JSON/CSV sont intégrés au code ; la validation Windows complète reste en cours dans GitHub Actions. Consulter [la feuille de route](docs/ROADMAP.md).

## Fonctionnalités déjà présentes dans le socle
- Interface desktop française (egui).
- Source GitHub configurable par URL ou propriétaire/dépôt.
- Résolution de la branche par défaut et des références branche/tag/commit vers un SHA de commit.
- Exploration récursive de l'arbre GitHub et sélection de fichiers.
- Jeton de lecture GitHub saisi dans un champ masqué (mémoire du processus uniquement).
- Téléchargement sélectif des blobs dans un workspace, sans écrasement volontaire des fichiers existants.
- Scan récursif d'un dossier local, sans suivre les liens symboliques.
- SHA-256 calculé en flux, inventaire versionné, tri stable et classification indicative par extension.
- Exports JSON et CSV UTF-8 ; les exports dans le dossier source scanné sont refusés.
- Tests unitaires du parsing d'URL, du classement et de l'inventaire.

## Limites connues
- Les arbres GitHub tronqués sont détectés mais leur exploration par sous-arbres n'est pas encore implémentée.
- Le téléchargement GitHub actuel utilise les blobs Base64, pas encore le streaming des gros fichiers.
- Pas encore de cache/reprise, Credential Manager, progression asynchrone ni annulation.
- Les analyseurs visuels, SVG, polices, archives, MSE, licences, galerie et rapport HTML restent à développer.
- L'interface est un prototype ; l'ergonomie et les tests de charge restent à faire.
- Aucun binaire Windows n'est déclaré validé avant succès de la CI.

## Développement
Prérequis : Rust stable et Visual Studio Build Tools (MSVC).

    cargo fmt --all
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all
    cargo run
    cargo build --release

## Sécurité et GitHub
L'application analyse les sources en lecture seule. Les téléchargements sont écrits dans le workspace choisi ; les exports locaux sont refusés à l'intérieur du dossier analysé. Les scripts, macros et contenus HTML ne sont jamais exécutés.

Pour un dépôt privé, utiliser un jeton à permissions minimales (Contents: read). Ne jamais le commiter ni l'inclure dans les rapports. Il n'est actuellement conservé qu'en mémoire pendant l'exécution.

Le dépôt cible est configurable : aucun dépôt MTG n'est codé en dur.

## Documentation
- [Cahier des charges](docs/REQUIREMENTS.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Feuille de route](docs/ROADMAP.md)
- [Sécurité](docs/SECURITY.md)
