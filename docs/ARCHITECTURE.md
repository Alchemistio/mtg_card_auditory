# Architecture

## Modules cibles
- UI : source, paramètres, galerie, progression.
- GitHub : API, dépôt/branche/commit/arbre, arbres partiels et blobs.
- Local : parcours récursif en lecture seule.
- Inventory : modèle unifié et IDs stables.
- Images/SVG/Fonts/Archives/MSE : analyseurs spécialisés.
- Hashes : SHA-256 et groupes de doublons.
- Licenses : preuves, provenance et statuts manuels.
- Export : JSON, CSV, HTML et planches.
- Workspace : cache, téléchargement et résultats.

## Flux GitHub
1. Saisir URL ou propriétaire/dépôt.
2. Vérifier le dépôt et résoudre la branche/tag/commit en SHA immuable.
3. Obtenir l'arbre Git récursif ; si tronqué, descendre par sous-arbres.
4. Filtrer par sous-dossier, extensions, tailles et familles.
5. Afficher les chemins, blob SHA et tailles.
6. Télécharger uniquement la sélection dans le workspace.
7. Calculer le SHA-256 des octets et lancer les analyseurs.
8. Exporter un rapport relié au commit exact.

Les limites API GitHub doivent être respectées. Les gros blobs nécessitent un téléchargement streaming plutôt que l'endpoint Base64.

## Séparation des états
Source, inventaire, analyse calculée, revue humaine et exports sont séparés. Une revue visuelle ou juridique ne peut jamais être validée par heuristique.

## Workspace
- cache/github/owner/repo/commit/blob-sha
- runs/run-id/inventory.json
- runs/run-id/assets.csv
- runs/run-id/contact-sheets/
- runs/run-id/gallery/index.html
- runs/run-id/report.html
- runs/run-id/logs/

Le cache peut être nettoyé par l'utilisateur ; les sources distantes ne sont jamais altérées.

## Déploiement
GitHub Actions construit un binaire Windows x64. La signature de code pourra être ajoutée avec un certificat externe.
