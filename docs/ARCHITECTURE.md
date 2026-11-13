# Architecture

## État de l'implémentation
Le prototype est organisé en modules Rust :
- `main.rs` : interface egui, workers d'arrière-plan, progression du scan et des téléchargements, choix des sources et actions d'export/comparaison.
- `github.rs` : API REST, parsing URL, timeout HTTP, erreurs de quota, résolution de branche/tag/commit vers le SHA du commit puis SHA de l'arbre, exploration récursive et repli sur sous-arbres si l'arbre est tronqué, blobs.
- `models.rs` : entrées d'arbre GitHub, objet de listing avec SHA immuables et manifeste de provenance des téléchargements.
- `inventory.rs` : parcours local parallèle avec progression, SHA-256, classification, métadonnées raster/SVG/fonts/ZIP, repérage de notices, doublons exacts, comparaison d'inventaires, planches raster PNG paginées et exports JSON/CSV/HTML.
- `Cargo.toml` : dépendances UI, HTTP, analyseurs et export.

Les workers utilisent des canaux de messages pour ne pas bloquer la boucle d'affichage pendant le scan local ou les opérations GitHub. Des drapeaux atomiques partagés permettent de demander l'arrêt du scan local ou du téléchargement GitHub ; un fichier en cours d'analyse ou un blob en cours de téléchargement peut se terminer avant l'arrêt. L'annulation coopérative du scan local est implémentée ; elle est vérifiée entre les étapes de parcours et d'analyse. L'annulation des téléchargements GitHub est vérifiée entre deux blobs ; le blob en cours termine avant l'arrêt.

## Flux GitHub actuel
1. Saisir une URL HTTPS canonique `github.com/owner/repository` ou propriétaire/dépôt ; le parseur vérifie l'hôte exact et refuse les identifiants, ports, query strings, fragments et sous-chemins.
2. Résoudre la branche par défaut si la référence est vide.
3. Résoudre la référence en commit, puis utiliser le SHA de l'arbre associé.
4. Demander l'arbre récursif ; si GitHub le tronque, descendre par sous-arbres avec garde-fous de profondeur et de cycles actifs.
5. Filtrer par sous-dossier, puis filtrer localement la liste distante par chemin, extension, plage de taille et famille technique (classification indicative par extension).
6. Sélectionner les blobs distants ; rechercher d'abord une copie dans `.mtg-auditor-cache/github/owner/repository/commit/blob-sha` sous le workspace, puis télécharger si nécessaire.
7. Vérifier les chemins de destination, refuser les liens symboliques et ouvrir les fichiers avec création exclusive afin de ne pas écraser une destination existante.
8. Écrire un manifeste JSON liant chaque fichier téléchargé au dépôt, à la référence, au commit SHA, au tree SHA et au blob SHA.
9. Les blobs sont récupérés via l'API Git Blobs flux brut ; la taille déclarée doit être connue, est limitée à 90 Mio et la réponse JSON à 128 Mio ; la taille décodée est comparée à celle déclarée. Le SHA-1 Git est recalculé sur l'objet `blob {taille}\\0{octets}` avant toute écriture ou mise en cache.
10. Le cache local est isolé sous `.mtg-auditor-cache/github/owner/repository/commit/blob-sha` dans le workspace. Chaque entrée est revalidée contre le SHA-1 Git attendu ; une entrée invalide est supprimée puis retéléchargée. Le cache ne modifie aucune source locale ni distante.
11. Un journal JSONL par commit est conservé sous `.mtg-auditor-cache/download-session-{commit}.jsonl`. Chaque succès est synchronisé sur disque ; à la relance, seuls les fichiers dont le chemin, le blob SHA, la taille et le contenu correspondent sont sautés. Une dernière ligne incomplète après arrêt brutal est ignorée.
12. Le journal ne contient aucun jeton. Son parent est canonicalisé dans le workspace et les liens symboliques ou fichiers non réguliers sont refusés. Une destination absente ou dont l'empreinte ne correspond pas n'est jamais écrasée ; elle produit une erreur explicite.
13. Le téléchargement streaming et la concurrence bornée restent à faire.
15. Les erreurs réseau de connexion/délai dépassé et les réponses 429/502/503/504 déclenchent jusqu'à trois nouvelles tentatives (quatre requêtes au total) avec délai exponentiel borné ; `Retry-After` numérique est respecté avec un plafond de 10 secondes. Les 403 de quota primaire épuisé sont diagnostiqués avec quota restant et reset, sans attente longue automatique.
14. Les fichiers téléchargés sont écrits avec `write_all`/`sync_all`; la création exclusive évite l'écrasement du fichier final, sans résoudre complètement les courses TOCTOU sur les répertoires parents.

## Flux local actuel
1. Choisir un ou plusieurs dossiers sources distincts (les sélections identiques ou imbriquées sont refusées).
2. Parcourir chaque dossier récursivement sans suivre les liens symboliques, avec profondeur maximale ; les chemins multi-sources sont préfixés par `source-0001`, `source-0002`, etc.
3. Calculer SHA-256 en blocs et relever taille/date/extension ; progression en octets traités, notifiée par lots de 100 fichiers.
4. Classifier indicativement par extension et repérer les fichiers de type LICENSE/NOTICE/README sans en déduire de statut légal.
5. Extraire dimensions, format, type de canaux et alpha raster sans décodage complet ; analyser le XML SVG sans rendu ; relever famille/sous-famille des polices prises en charge ; inspecter la table centrale ZIP sans extraction.
6. Détecter les doublons exacts par SHA-256 sans supprimer ni modifier les fichiers.
7. Comparer avec un inventaire JSON antérieur par chemin et SHA-256.
8. Exporter JSON versionné, CSV UTF-8 ou HTML autonome après validation que la destination est hors de la source.
9. Générer des planches raster PNG de 16 éléments/page et un index texte ; le SHA est revérifié avant décodage.
10. Les rapports de comparaison JSON sont écrits hors des deux sources comparées. Pour un inventaire multi-sources, les exports et planches vérifient chaque racine source ; les planches résolvent chaque préfixe vers le dossier correspondant.

## Séparation des états
Source, inventaire, analyse calculée, revue humaine et exports sont séparés. Une revue visuelle ou juridique ne peut jamais être validée par heuristique. Les classifications automatiques restent des suggestions.

## Workspace
- `.mtg-auditor-cache/github/owner/repo/commit/blob-sha` : cache local de blobs vérifiés par SHA-1 Git.
- `runs/run-id/inventory.json`
- `runs/run-id/assets.csv`
- `runs/run-id/report.html`
- `runs/run-id/contact-sheets/`
- `runs/run-id/gallery/index.html` (à implémenter)
- `runs/run-id/logs/` (à implémenter)

La reprise après fermeture et la persistance des revues restent à développer ; les sources distantes et locales ne sont jamais altérées.

## Limites techniques
- Les requêtes HTTP utilisent reqwest bloquant dans les workers ; les tentatives transitoires sont bornées. La reprise après fermeture reste à ajouter.
- Le jeton privé est seulement conservé en mémoire ; Credential Manager reste à implémenter.
- La protection contre les courses TOCTOU sur le workspace doit être renforcée.
- Les analyseurs ZIP ne décompressent rien ; 7z/RAR ne sont pas pris en charge.
- Les miniatures avancées, aperçu SVG sandboxé, MSE, licences/provenance et revue humaine sont à faire.

## Déploiement
GitHub Actions construit un binaire Windows x64. Le commit `c1c71fd` a passé Clippy, tests, build Release et upload d'artefact. Les commits récents ont passé la CI Windows complète (formatage, Clippy, tests, build Release et artefact), y compris les changements de multi-sources dès validation du run associé. La signature de code pourra être ajoutée avec un certificat externe.
