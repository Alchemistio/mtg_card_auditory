# MTG Asset Auditor

Application Windows native en Rust pour inventorier, prévisualiser et auditer des ressources graphiques depuis un dossier local ou un dépôt GitHub choisi par l'utilisateur.

État : socle initial ; consulter docs/ROADMAP.md.

## Objectifs
- Choisir tout dépôt GitHub public ou privé, sa branche et un sous-répertoire.
- Explorer l'arbre GitHub sans cloner systématiquement tout le dépôt.
- Télécharger sélectivement les fichiers dans un workspace choisi.
- Scanner des dossiers locaux.
- Produire inventaires JSON/CSV, SHA-256, métadonnées et planches de contact.
- Prévisualiser, classer, repérer les doublons, examiner archives et licences.
- Générer un rapport HTML hors ligne.
- Ne jamais modifier ni supprimer les sources.
- Construire un exécutable Windows via GitHub Actions.

## Développement
Prérequis : Rust stable et Visual Studio Build Tools (MSVC).

    cargo run
    cargo build --release

## GitHub
L'application utilise l'API REST GitHub. Pour un dépôt privé, un jeton à permissions minimales (Contents: read) est demandé. Il ne doit jamais être commité, exporté dans les rapports ou enregistré en clair.

Le dépôt cible est configurable : aucun dépôt n'est codé en dur.

## Documentation
- docs/REQUIREMENTS.md
- docs/ARCHITECTURE.md
- docs/ROADMAP.md
- docs/SECURITY.md
