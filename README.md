# MTG Asset Auditor

Application desktop Windows native en Rust pour inventorier et auditer des ressources depuis un dossier local ou un dépôt GitHub choisi par l'utilisateur.

**État : prototype fonctionnel en développement.** Le scan local parallèle SHA-256, la progression de scan en octets et de téléchargement, annulation coopérative du scan local et des téléchargements GitHub (entre deux blobs ; le blob en cours termine), les exports JSON/CSV/HTML, les doublons binaires exacts, la comparaison d'inventaires JSON, les métadonnées raster/SVG/polices/ZIP, les planches de contact PNG, l'exploration GitHub configurable et les workers d'arrière-plan sont intégrés. Les commits jusqu'au support multi-dossiers sont validés individuellement par la CI Windows complète (formatage, Clippy, tests, build Release et artefact) lorsque leur run est terminé ; consulter l'onglet Actions pour le statut de la tête de branche.

## Fonctionnalités présentes dans le socle
- Interface desktop française (egui).
- Source GitHub configurable par URL HTTPS canonique ou propriétaire/dépôt ; validation stricte de l'hôte, du chemin et des caractères du nom.
- Résolution de la branche par défaut et des références branche/tag/commit vers un SHA de commit et un SHA d'arbre.
- Exploration récursive de l'arbre GitHub ; repli sur parcours de sous-arbres si l'arbre récursif est tronqué, avec garde-fous de profondeur et de cycles. Les sous-arbres identiques réutilisés à plusieurs chemins sont conservés.
- Exploration GitHub, téléchargement et scan local exécutés en arrière-plan.
- Progression du scan local en octets traités (notification par lots de 100 fichiers et à la fin).
- Progression des téléchargements GitHub par fichier terminé.
- Manifeste JSON de provenance pour les téléchargements : propriétaire, dépôt, référence demandée, commit SHA, tree SHA, date, chemin et blob SHA.
- Timeout HTTP de 60 secondes, jusqu'à trois nouvelles tentatives (quatre requêtes au total) pour les erreurs réseau transitoires et réponses 429/502/503/504, prise en compte bornée de Retry-After (secondes ou date HTTP) ; les quotas primaires épuisés sont signalés sans attente bloquante longue.
- Téléchargement streaming des blobs GitHub via le média type brut documenté `application/vnd.github.raw+json`, plafonné à 90 Mio par fichier ; taille et SHA-1 Git vérifiés pendant le flux avec un tampon de 64 Kio, avant publication dans le cache puis copie vers le workspace.
- Cache local sous `.mtg-auditor-cache/github/owner/repository/commit/blob-sha` dans le workspace ; les entrées sont vérifiées par SHA-1 Git et les entrées invalides sont écartées avant un nouveau téléchargement.
- Jeton GitHub saisi dans un champ masqué ; chargement, enregistrement explicite et suppression via Windows Credential Manager (aucun enregistrement automatique). Le champ applicatif et les copies envoyées aux workers sont enveloppés dans `zeroize::Zeroizing<String>` pour écraser leurs allocations à la libération ; les copies internes de bibliothèques ne sont pas garanties.
- Filtrage de la liste distante par chemin, extension, plage de taille et famille technique, puis téléchargement sélectif des blobs dans un workspace, avec création exclusive des fichiers, refusant l'écrasement d'un fichier existant au moment de l'ouverture.
- Scan local parallèle via Rayon, sans suivre les liens symboliques ; sélection multi-dossiers avec préfixes de source, détection des racines imbriquées et doublons exacts entre sources.
- SHA-256 calculé en flux, inventaire versionné, tri stable et classification indicative par extension.
- Anciens inventaires JSON v1 lisibles malgré l'ajout de nouveaux champs optionnels.
- Doublons binaires exacts repérés par SHA-256 et référencés au premier chemin ; aucun fichier n'est supprimé.
- Comparaison avec un ancien inventaire JSON : chemins ajoutés, supprimés, modifiés et inchangés ; export JSON de comparaison.
- Repérage des fichiers LICENSE/LICENCE/COPYING/NOTICE/README/AUTHORS/CREDITS pour revue ultérieure ; aucun type de licence n'est déduit.
- Raster : format détecté, dimensions, type de canaux et présence d'alpha depuis le décodeur, sans décodage des pixels.
- SVG : contrôle XML, lecture du viewBox et détection indicative de références externes ; aucun SVG n'est rendu ou exécuté.
- Polices : extraction initiale de la famille et sous-famille lorsque le format est pris en charge.
- ZIP : lecture de la table centrale, comptage des entrées, taille décompressée déclarée, chemins suspects et ratios anormaux ; aucun décompactage ; archives de plus de 512 Mio ignorées.
- Exports JSON et CSV UTF-8, rapport HTML autonome sans JavaScript.
- Planches raster PNG de 16 images par page, avec fichier texte d'index ; le SHA-256 source est revérifié avant génération.
- Vue locale filtrable ; jusqu'à 500 lignes affichées à la fois, exports complets.
- Garde-fous contre les chemins distants dangereux et les liens symboliques dans les destinations de téléchargement ; les exports restent hors de la source.

## Limites connues
- Cache GitHub local par commit/blob vérifié avec le SHA-1 Git.
- Journal de progression persistant par dépôt et commit complet dans `.mtg-auditor-cache/download-sessions/owner/repository/` ; les fichiers terminés sont repris uniquement si leur taille et leur SHA-1 Git correspondent encore. Les manifestes sont également séparés par dépôt dans `.mtg-auditor-cache/download-manifests/`. Les fichiers absents ou modifiés ne sont jamais écrasés. La vérification des fichiers repris est effectuée en flux avec un tampon de 64 Kio.
- La reprise concerne les fichiers terminés d'une sélection relancée. La reprise d'un blob interrompu au niveau des octets n'est pas implémentée ; la documentation GitHub de l'API Git Blobs ne garantit pas la prise en charge de `Range` ou d'ETag, donc aucun fragment partiel n'est réutilisé.
- Estimation du temps restant.
- Le contenu et les obligations des fichiers de licence doivent être analysés et reliés à une provenance vérifiable ; aucune validation juridique automatique.
- Orientation, miniatures avancées, aperçu SVG sandboxé, inspection CSS approfondie, métadonnées avancées de polices, 7z/RAR et analyse MSE.
- Galerie visuelle, planches annotées, licences/provenance, revue humaine et export HTML de comparaison.
- La défense contre les modifications concurrentes du système de fichiers (TOCTOU) n'est pas complète.
- L'interface est un prototype ; l'ergonomie et les tests de charge restent à faire.
- La CI Windows complète est exécutée à chaque commit. Le statut du commit courant doit être vérifié dans l'onglet Actions ; ne pas extrapoler la validation d'un commit précédent.

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

**Périmètre actuel :** cet outil reste autonome. Aucune intégration ni modification du MTG Card Remodeler ne sera entreprise tant que son propre cahier des charges n'est pas finalisé.

## Documentation
- [Cahier des charges](docs/REQUIREMENTS.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Feuille de route](docs/ROADMAP.md)
- [Sécurité](docs/SECURITY.md)
