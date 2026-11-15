# Sécurité et intégrité des sources

## Principes
- Les dépôts GitHub sont interrogés en lecture seule.
- Les dossiers locaux sont parcourus en lecture seule.
- Aucun fichier source n'est supprimé, déplacé, renommé, normalisé ou écrasé.
- Les fichiers téléchargés sont écrits uniquement dans le workspace choisi.
- Un manifeste de provenance relie les blobs téléchargés au dépôt, à la référence, au commit SHA et au tree SHA.
- Les rapports sont exportés hors de tous les dossiers locaux analysés, y compris en mode multi-sources.
- Le rapport HTML est autonome, échappe les données inventoriées et n'embarque aucun JavaScript.
- Les planches de contact ouvrent chaque source via un handle `cap-std`, lisent au plus 100 Mio une seule fois, puis calculent le SHA-256, les dimensions et les pixels à partir du même tampon d'octets ; les liens symboliques détectés sont refusés.
- Les rapports de comparaison JSON sont exportés hors des deux sources comparées.
- Le repérage de noms LICENSE/NOTICE/README est une simple aide à l'inventaire ; il ne certifie aucune licence ni droit de redistribution.
- Les fichiers HTML, scripts, macros et contenus récupérés ne sont jamais exécutés.
- Les SVG sont analysés comme XML sans rendu ; la lecture est plafonnée à 32 Mio au niveau du flux, même si le fichier grossit après la vérification de ses métadonnées. Le parseur limite aussi le document à 100 000 événements XML, 256 niveaux d'imbrication et 256 attributs par élément ; les dépassements sont signalés comme SVG invalide sans rendu ni accès réseau. Les références `href` non fragmentaires/non `data:` et les cibles CSS `url()` externes ou relatives sont signalées de façon conservative ; elles ne sont jamais ouvertes.
- Les archives ZIP sont inspectées par leur table centrale ; aucune extraction n'est effectuée. Les packages MSE ZIP sont reconnus uniquement par extension et par présence du fichier principal attendu dans cette table ; le préfixe du fichier principal est lu avec une limite stricte de 1 Mio ; les lignes sont plafonnées à 16 Kio et 4 096 lignes. Les directives `include file:` sont uniquement signalées et ne sont jamais suivies.

## Chemins et liens
- Les URL de dépôt sont analysées avec un parseur URL dédié : HTTPS uniquement, hôte exactement `github.com`, absence d'identifiants/port/query/fragment, et chemin limité à `owner/repository`.
- Les noms owner/repository sont limités aux caractères autorisés par le parseur ; les sous-chemins arbitraires sont rejetés.
- Les chemins contenant racine, préfixe, parent ou composant spécial sont rejetés.
- Les destinations existantes ne sont pas écrasées pour les téléchargements et planches. Les exports locaux utilisent `create_new` depuis un handle du dossier parent ; un test Unix vérifie qu'une destination déjà liée symboliquement n'écrase pas sa cible, et un test multiplateforme vérifie que deux créations concurrentes produisent exactement un gagnant (CI Unix et Windows validées sur `e77dfe8`).
- Les fichiers téléchargés et les manifestes sont ouverts avec création exclusive (`create_new`) pour refuser un fichier déjà présent au moment de l'ouverture.
- Les blobs GitHub sont refusés au-delà de 90 Mio ; le téléchargement utilise le média type brut documenté `application/vnd.github.raw+json` en flux, vérifie la taille déclarée et calcule le SHA-1 Git en mémoire bornée (tampon 64 Kio).
- Le SHA-1 Git de chaque blob est recalculé avec l'en-tête objet Git `blob {taille}\0` avant écriture et avant réutilisation du cache.
- Le cache est conservé dans le workspace sous `.mtg-auditor-cache` ; les noms de segments sont validés et chaque fichier de cache est vérifié contre le SHA attendu. Une entrée invalide est écartée.
- Les écritures sont synchronisées (`sync_all`) ; si l'écriture ou la synchronisation échoue, le fichier partiel créé par l'opération est supprimé.
- Les écritures de fichiers téléchargés utilisent `write_all` puis `sync_all`; un fichier partiel est supprimé si l'écriture échoue.
- Le sous-système GitHub (cache, fichiers téléchargés, journaux JSONL et manifestes) utilise `WorkspaceDir`, wrapper autour de `cap_std::fs::Dir` ouvert sur le workspace. Les opérations sont relatives à ce handle plutôt qu'à des chemins absolus ambiants.
- Les chemins relatifs du workspace rejettent les composants absolus, parents, préfixes et noms contenant `:` (flux de données alternatifs Windows).
- Les fichiers téléchargés et temporaires sont créés exclusivement via le handle de répertoire ; la publication du cache utilise un lien dur sans écrasement après vérification SHA-1 Git.
- Les exports JSON/CSV/HTML, planches PNG et index texte utilisent un handle `cap-std` ouvert sur le dossier parent canonique, avec création exclusive, synchronisation et suppression du fichier partiel en cas d'erreur.
- Les lectures des sources lors de l'analyse, la réanalyse des images pour planches de contact et la lecture d'un inventaire JSON de comparaison utilisent des handles `cap-std` et des chemins relatifs au répertoire ouvert. Les planches lisent chaque image une seule fois dans un tampon plafonné à 100 Mio ; hash, dimensions et pixels sont calculés sur les mêmes octets. Les contenus SVG et polices sont plafonnés en lecture à 32 Mio et 64 Mio respectivement, sans dépendre uniquement de métadonnées susceptibles de devenir obsolètes.
- Ce confinement ne constitue pas une garantie TOCTOU complète : la sélection initiale des dossiers repose encore sur des chemins ambiants. `analyze_file` utilise désormais `symlink_metadata` avant et après les analyseurs, refuse les sources qui ne sont plus des fichiers réguliers (notamment un remplacement par lien symbolique après l'énumération), et compare taille, horodatage de modification et type de fichier ; si les métadonnées diffèrent, le scan échoue explicitement et ne publie pas l'inventaire partiel. Un test Unix simule le remplacement d'un fichier régulier par un lien symbolique entre l'énumération et l'analyse. Cette détection ne peut pas repérer une modification conservant simultanément taille et horodatage observable, et les analyseurs ouvrent encore plusieurs handles. Les planches, elles, utilisent un tampon unique dont le SHA-256 doit correspondre à l'inventaire avant décodage. Après l'indexation ultérieure des ressources ZIP MSE, chaque archive est de nouveau hachée en flux et ses métadonnées sont comparées avant/après ce contrôle ; si le SHA-256 diffère du hash initial ou si les métadonnées changent pendant la vérification, le scan échoue au lieu de publier un inventaire incohérent. Un test remplace l'archive après le scan initial et vérifie le rejet. Cette vérification supplémentaire ne détecte pas une mutation transitoire suivie d'une restauration exacte avant le contrôle final.
- Le scan local énumère récursivement via `cap-std`, ne suit pas les liens symboliques et limite la profondeur à 128 niveaux. Un test Unix couvre les liens de fichiers et de répertoires ; un test Windows couvre les liens de fichiers et affiche explicitement un message `SKIP` si les permissions du runner empêchent leur création.
- Un test Unix crée un lien symbolique de répertoire vers un dossier externe, puis vérifie que `WorkspaceDir::create_new` refuse de créer un fichier via ce lien et que la cible externe reste intacte. Le test Windows correspondant affiche explicitement `SKIP` si le runner refuse la création du lien. Validation historique plus récente : sur le commit `a9ed50a`, les CI Unix et Windows ont chacune exécuté 56 tests avec succès (runs `43687351991` et `43687351946`), avec formatage et Clippy ; Windows a également produit le build Release et l'artefact exécutable. Ces résultats ne valident pas les commits documentaires postérieurs ni le HEAD courant. Les tests couvrent notamment la lecture bornée des en-têtes MSE ZIP/dossiers, le non-suivi des directives include, les dépendances et références locales, les versions minimales et la détection itérative des cycles.
- Les exports JSON/CSV/HTML/PNG et index texte sont refusés si le chemin final se trouve dans l'arborescence de l'une des sources analysées.

## Archives
- ZIP : lecture des métadonnées de l'archive sans extraire de contenu.
- Les chemins absolus, parent, points, antislashs et préfixes de volume sont signalés.
- Les ratios de compression inhabituels et tailles décompressées déclarées importantes sont signalés.
- Le fichier ZIP est plafonné à 512 Mio ; le nombre d'entrées (200 000) et la taille décompressée déclarée (8 Gio) sont plafonnés.
- Les formats 7z/RAR ne sont pas encore inspectés.

## Secrets GitHub
- Le champ du jeton est masqué.
- Le jeton peut être enregistré explicitement dans Windows Credential Manager via la bibliothèque `keyring` ; il n'est jamais écrit dans les fichiers du workspace.
- Le chargement et l'enregistrement sont déclenchés explicitement par l'utilisateur ; aucun enregistrement automatique.
- Le tampon temporaire de saisie egui, le champ applicatif et les copies explicitement transmises aux workers utilisent `zeroize::Zeroizing<String>` : leur allocation est écrasée à la libération normale de la valeur.
- Le chargement depuis Windows Credential Manager crée initialement une `String` retournée par la bibliothèque ; elle est immédiatement enveloppée dans `Zeroizing`. Les détails internes de la bibliothèque, du système HTTP/TLS et les copies temporaires éventuelles ne sont pas sous contrôle complet de l'application.
- L'effacement mémoire réduit la durée de vie des secrets mais ne protège pas contre un processus compromis, un dump mémoire pris avant libération, le swap, ni toutes les copies internes de dépendances.
- Ne jamais écrire le jeton dans les journaux, exports ou fichiers de configuration.
- Utiliser un jeton à permissions minimales, Contents: read.

## Limites restantes
- Le téléchargement des blobs utilise la représentation brute GitHub en flux vers un fichier temporaire dans le cache ; la taille et le SHA-1 Git sont vérifiés avant publication. La destination finale est ensuite copiée depuis le cache avec création exclusive.
- Le journal de reprise JSONL est local au workspace sous `.mtg-auditor-cache/download-sessions/owner/repository/`, avec le SHA complet du commit dans son nom ; cela évite les collisions entre dépôts partageant un même identifiant de commit. Il ne contient que chemin, blob SHA, taille et horodatage ; aucun jeton GitHub n'y est écrit.
- Les manifestes sont séparés par dépôt sous `.mtg-auditor-cache/download-manifests/owner/repository/` et utilisent également le SHA complet du commit. Son dossier parent est canonicalisé et doit rester dans le workspace ; les liens symboliques et fichiers non réguliers sont refusés.
- Un fichier déjà journalisé n'est sauté qu'après vérification de son SHA-1 Git et de sa taille ; cette vérification se fait en flux avec un tampon de 64 Kio, sans lecture intégrale en mémoire. Une destination modifiée, manquante ou invalide n'est jamais écrasée automatiquement.
- Une ligne finale JSONL incomplète est ignorée après arrêt brutal ; les lignes antérieures invalides bloquent la reprise plutôt que de faire confiance à un journal corrompu.
- La lecture du journal JSONL de reprise est plafonnée à 64 Mio en flux ; un journal plus volumineux est refusé explicitement au lieu d'être chargé sans limite en mémoire.
- Les erreurs transitoires et réponses 429/502/503/504 utilisent des retries bornés ; le quota primaire épuisé est signalé sans attente longue automatique.
- Première phase capability-based validée par CI : les opérations GitHub sur le workspace (cache, téléchargements, journaux, manifestes) passent par `WorkspaceDir` et `cap-std`, avec chemins relatifs contrôlés et opérations de fichiers via le handle de répertoire.
- La migration n'est pas globale : la sélection initiale des dossiers et l'énumération récursive capability-based a remplacé `WalkDir`. Les lectures de contenu des fichiers analysés, la réanalyse des images des planches et l'ouverture de l'inventaire JSON de comparaison sont désormais capability-based.
- La défense TOCTOU reste donc incomplète. Le test Windows de lien symbolique de fichier affiche un message `SKIP` si le runner interdit la création du lien ; la sortie des tests est conservée dans les logs CI. Il reste à étudier les points de réanalyse Windows et à tester les scénarios de remplacement concurrent de répertoires/liens.
- Les quotas API, temporisations, limites de taille et reprise doivent être renforcés.
- Les ressources ZIP MSE sont lues en flux sans extraction, avec 64 Mio maximum par entrée, 256 Mio par package, 512 Mio par scan et 10 000 entrées indexées par package ; les erreurs de décompression/CRC sont signalées sans produire d'empreinte (tests sur ressources stockées et Deflate corrompues, validés sur le HEAD `006aef27e2cc9063aa06721ebe8f2e52b54a1ffd` par CI Unix `43742700037` et Windows `43742700103`) ; les chemins suspects, ratios extrêmes et ressources hors budget ne sont pas hachés. L'annulation du scan est vérifiée entre les entrées ZIP et pendant le hachage en flux, puis propagée comme annulation du scan complet. Les analyseurs de formats actifs devront rester sans extraction/exécution automatique.
