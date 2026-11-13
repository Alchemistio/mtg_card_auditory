# Architecture

## État de l'implémentation
Le prototype est organisé en modules Rust :
- `main.rs` : interface egui, workers d'arrière-plan, progression du scan et des téléchargements, choix des sources et actions d'export/comparaison.
- `github.rs` : API REST, parsing URL, timeout HTTP, erreurs de quota, résolution de branche/tag/commit vers le SHA du commit puis SHA de l'arbre, exploration récursive et repli sur sous-arbres si l'arbre est tronqué, blobs.
- `models.rs` : entrées d'arbre GitHub.
- `inventory.rs` : parcours local parallèle avec progression, SHA-256, classification, métadonnées raster/SVG/fonts/ZIP, repérage de notices, doublons exacts, comparaison d'inventaires, planches raster PNG paginées et exports JSON/CSV/HTML.
- `Cargo.toml` : dépendances UI, HTTP, analyseurs et export.

Les workers utilisent des canaux de messages pour ne pas bloquer la boucle d'affichage pendant le scan local ou les opérations GitHub. L'annulation et la progression en octets ne sont pas encore implémentées.

## Flux GitHub actuel
1. Saisir une URL github.com ou propriétaire/dépôt.
2. Résoudre la branche par défaut si la référence est vide.
3. Résoudre la référence en commit, puis utiliser le SHA de l'arbre associé.
4. Demander l'arbre récursif ; si GitHub le tronque, descendre par sous-arbres avec garde-fous de profondeur et de cycles.
5. Filtrer par sous-dossier.
6. Sélectionner les blobs distants et télécharger vers le workspace choisi.
7. Vérifier les chemins de destination et refuser les liens symboliques / écrasements.
8. Les blobs sont actuellement récupérés via l'API Git Blobs Base64 ; streaming/cache/reprise restent à faire.
9. Les erreurs 429 et 403 avec quota épuisé sont identifiées ; le backoff automatique reste à faire.

## Flux local actuel
1. Choisir un dossier source.
2. Parcourir récursivement sans suivre les liens symboliques, avec profondeur maximale.
3. Calculer SHA-256 en blocs et relever taille/date/extension ; progression en nombre de fichiers traités.
4. Classifier indicativement par extension et repérer les fichiers de type LICENSE/NOTICE/README sans en déduire de statut légal.
5. Extraire dimensions, format, type de canaux et alpha raster sans décodage complet ; analyser le XML SVG sans rendu ; relever famille/sous-famille des polices prises en charge ; inspecter la table centrale ZIP sans extraction.
6. Détecter les doublons exacts par SHA-256 sans supprimer ni modifier les fichiers.
7. Comparer avec un inventaire JSON antérieur par chemin et SHA-256.
8. Exporter JSON versionné, CSV UTF-8 ou HTML autonome après validation que la destination est hors de la source.
9. Générer des planches raster PNG de 16 éléments/page et un index texte ; le SHA est revérifié avant décodage.
10. Les rapports de comparaison JSON sont écrits hors des deux sources comparées.

## Séparation des états
Source, inventaire, analyse calculée, revue humaine et exports sont séparés. Une revue visuelle ou juridique ne peut jamais être validée par heuristique. Les classifications automatiques restent des suggestions.

## Workspace
- `cache/github/owner/repo/commit/blob-sha` (à implémenter)
- `runs/run-id/inventory.json`
- `runs/run-id/assets.csv`
- `runs/run-id/report.html`
- `runs/run-id/contact-sheets/`
- `runs/run-id/gallery/index.html` (à implémenter)
- `runs/run-id/logs/` (à implémenter)

Le cache et la persistance des revues restent à développer ; les sources distantes et locales ne sont jamais altérées.

## Limites techniques
- Les requêtes HTTP utilisent actuellement reqwest bloquant dans les workers ; backoff, reprise et cache doivent être ajoutés.
- Le jeton privé est seulement conservé en mémoire ; Credential Manager reste à implémenter.
- La protection contre les courses TOCTOU sur le workspace doit être renforcée.
- Les analyseurs ZIP ne décompressent rien ; 7z/RAR ne sont pas pris en charge.
- Les miniatures avancées, aperçu SVG sandboxé, MSE, licences/provenance et revue humaine sont à faire.

## Déploiement
GitHub Actions construit un binaire Windows x64. Le commit `df73f1a` a passé Clippy, tests, build Release et upload d'artefact ; la progression détaillée et le repérage de notices doivent être revalidés avant livraison. La signature de code pourra être ajoutée avec un certificat externe.
