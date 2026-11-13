# Sécurité et intégrité des sources

## Principes
- Les dépôts GitHub sont interrogés en lecture seule.
- Les dossiers locaux sont parcourus en lecture seule.
- Aucun fichier source n'est supprimé, déplacé, renommé, normalisé ou écrasé.
- Les fichiers téléchargés sont écrits uniquement dans le workspace choisi.
- Un manifeste de provenance relie les blobs téléchargés au dépôt, à la référence, au commit SHA et au tree SHA.
- Les rapports sont exportés hors du dossier local analysé.
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
- Les blobs GitHub sont refusés au-delà de 90 Mio ; le corps JSON est limité à 128 Mio et la taille décodée doit correspondre à la taille déclarée par Git.
- Les écritures sont synchronisées (`sync_all`) ; si l'écriture ou la synchronisation échoue, le fichier partiel créé par l'opération est supprimé.
- Les écritures de fichiers téléchargés utilisent `write_all` puis `sync_all`; un fichier partiel est supprimé si l'écriture échoue.
- Les liens symboliques présents dans les chemins de destination sont refusés.
- Le dossier de destination est canonicalisé et doit rester dans le workspace.
- Le scan local ne suit pas les liens symboliques et limite la profondeur de parcours.
- Les exports JSON/CSV/HTML/PNG et index texte sont refusés si le chemin final se trouve dans l'arborescence source.

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
- Le téléchargement des blobs utilise actuellement l'API Base64 et n'est pas encore optimisé pour les gros fichiers.
- Les erreurs de quota sont détectées mais aucun backoff automatique n'est encore appliqué.
- La défense TOCTOU reste incomplète : la création exclusive protège le fichier final contre l'écrasement, mais ne neutralise pas toutes les courses sur les répertoires parents ni le remplacement de liens entre vérification et ouverture. Une approche basée sur des handles de répertoire et des opérations relatives est à étudier.
- Les quotas API, temporisations, limites de taille et reprise doivent être renforcés.
- Les analyseurs de formats actifs devront rester sans extraction/exécution automatique.
