# Sécurité et intégrité des sources

## Principes
- Les dépôts GitHub sont interrogés en lecture seule.
- Les dossiers locaux sont parcourus en lecture seule.
- Aucun fichier source n'est supprimé, déplacé, renommé, normalisé ou écrasé.
- Les fichiers téléchargés sont écrits uniquement dans le workspace choisi.
- Un manifeste de provenance relie les blobs téléchargés au dépôt, à la référence, au commit SHA et au tree SHA.
- Les rapports sont exportés hors de tous les dossiers locaux analysés, y compris en mode multi-sources.
- Le rapport HTML est autonome, échappe les données inventoriées et n'embarque aucun JavaScript.
- Les planches de contact revérifient le SHA-256 des sources avant de les décoder et refusent les liens symboliques.
- Les rapports de comparaison JSON sont exportés hors des deux sources comparées.
- Le repérage de noms LICENSE/NOTICE/README est une simple aide à l'inventaire ; il ne certifie aucune licence ni droit de redistribution.
- Les fichiers HTML, scripts, macros et contenus récupérés ne sont jamais exécutés.
- Les SVG sont analysés comme XML sans rendu.
- Les archives ZIP sont inspectées par leur table centrale ; aucune extraction n'est effectuée.

## Chemins et liens
- Les URL de dépôt sont analysées avec un parseur URL dédié : HTTPS uniquement, hôte exactement `github.com`, absence d'identifiants/port/query/fragment, et chemin limité à `owner/repository`.
- Les noms owner/repository sont limités aux caractères autorisés par le parseur ; les sous-chemins arbitraires sont rejetés.
- Les chemins contenant racine, préfixe, parent ou composant spécial sont rejetés.
- Les destinations existantes ne sont pas écrasées pour les téléchargements et planches.
- Les fichiers téléchargés et les manifestes sont ouverts avec création exclusive (`create_new`) pour refuser un fichier déjà présent au moment de l'ouverture.
- Les blobs GitHub sont refusés au-delà de 90 Mio ; le téléchargement utilise le média type brut documenté `application/vnd.github.raw+json` en flux, vérifie la taille déclarée et calcule le SHA-1 Git en mémoire bornée (tampon 64 Kio).
- Le SHA-1 Git de chaque blob est recalculé avec l'en-tête objet Git `blob {taille}\\0` avant écriture et avant réutilisation du cache.
- Le cache est conservé dans le workspace sous `.mtg-auditor-cache` ; les noms de segments sont validés et chaque fichier de cache est vérifié contre le SHA attendu. Une entrée invalide est écartée.
- Les écritures sont synchronisées (`sync_all`) ; si l'écriture ou la synchronisation échoue, le fichier partiel créé par l'opération est supprimé.
- Les écritures de fichiers téléchargés utilisent `write_all` puis `sync_all`; un fichier partiel est supprimé si l'écriture échoue.
- Les liens symboliques présents dans les chemins de destination sont refusés.
- Le dossier de destination est canonicalisé et doit rester dans le workspace.
- Le scan local ne suit pas les liens symboliques et limite la profondeur de parcours.
- Les exports JSON/CSV/HTML/PNG et index texte sont refusés si le chemin final se trouve dans l'arborescence de l'une des sources analysées.

## Archives
- ZIP : lecture des métadonnées de l'archive sans extraire de contenu.
- Les chemins absolus, parent, points, antislashs et préfixes de volume sont signalés.
- Les ratios de compression inhabituels et tailles décompressées déclarées importantes sont signalés.
- Le fichier ZIP est plafonné à 512 Mio ; le nombre d'entrées (200 000) et la taille décompressée déclarée (8 Gio) sont plafonnés.
- Les formats 7z/RAR ne sont pas encore inspectés.

## Secrets GitHub
- Le champ du jeton est masqué.
- Le jeton n'est conservé qu'en mémoire dans le prototype.
- Ne jamais écrire le jeton dans les journaux, exports ou fichiers de configuration.
- Utiliser un jeton à permissions minimales, Contents: read.
- Le stockage Windows Credential Manager reste à implémenter.

## Limites restantes
- Le téléchargement des blobs utilise la représentation brute GitHub en flux vers un fichier temporaire dans le cache ; la taille et le SHA-1 Git sont vérifiés avant publication. La destination finale est ensuite copiée depuis le cache avec création exclusive.
- Le journal de reprise JSONL est local au workspace et ne contient que chemin, blob SHA, taille et horodatage ; aucun jeton GitHub n'y est écrit. Son dossier parent est canonicalisé et doit rester dans le workspace ; les liens symboliques et fichiers non réguliers sont refusés.
- Un fichier déjà journalisé n'est sauté qu'après vérification de son SHA-1 Git et de sa taille. Une destination modifiée, manquante ou invalide n'est jamais écrasée automatiquement.
- Une ligne finale JSONL incomplète est ignorée après arrêt brutal ; les lignes antérieures invalides bloquent la reprise plutôt que de faire confiance à un journal corrompu.
- Les erreurs transitoires et réponses 429/502/503/504 utilisent des retries bornés ; le quota primaire épuisé est signalé sans attente longue automatique.
- La défense TOCTOU reste incomplète : la création exclusive protège le fichier final contre l'écrasement, mais ne neutralise pas toutes les courses sur les répertoires parents ni le remplacement de liens entre vérification et ouverture. Une approche basée sur des handles de répertoire et des opérations relatives est à étudier.
- Les quotas API, temporisations, limites de taille et reprise doivent être renforcés.
- Les analyseurs de formats actifs devront rester sans extraction/exécution automatique.
