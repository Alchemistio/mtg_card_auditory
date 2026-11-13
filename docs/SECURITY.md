# Sécurité et intégrité des sources

## Principes
- Les dépôts GitHub sont interrogés en lecture seule.
- Les dossiers locaux sont parcourus en lecture seule.
- Aucun fichier source n'est supprimé, déplacé, renommé, normalisé ou écrasé.
- Les fichiers téléchargés sont écrits uniquement dans le workspace choisi.
- Les rapports sont exportés hors du dossier local analysé.
- Les fichiers HTML, scripts, macros et contenus récupérés ne sont jamais exécutés.
- Les SVG sont analysés comme XML sans rendu.
- Les archives ZIP sont inspectées par leur table centrale ; aucune extraction n'est effectuée.

## Chemins et liens
- Les chemins GitHub sont composés uniquement de composants relatifs normaux.
- Les chemins contenant racine, préfixe, parent ou composant spécial sont rejetés.
- Les destinations existantes ne sont pas écrasées.
- Les liens symboliques présents dans les chemins de destination sont refusés.
- Le dossier de destination est canonicalisé et doit rester dans le workspace.
- Le scan local ne suit pas les liens symboliques et limite la profondeur de parcours.
- Les exports JSON/CSV sont refusés si le chemin final se trouve dans l'arborescence source.

## Archives
- ZIP : lecture des métadonnées de l'archive sans extraire de contenu.
- Les chemins absolus, parent, points, antislashs et préfixes de volume sont signalés.
- Les ratios de compression inhabituels et tailles décompressées déclarées importantes sont signalés.
- Le nombre d'entrées (200 000) et la taille décompressée déclarée (8 Gio) sont plafonnés.
- Les formats 7z/RAR ne sont pas encore inspectés.

## Secrets GitHub
- Le champ du jeton est masqué.
- Le jeton n'est conservé qu'en mémoire dans le prototype.
- Ne jamais écrire le jeton dans les journaux, exports ou fichiers de configuration.
- Utiliser un jeton à permissions minimales, Contents: read.
- Le stockage Windows Credential Manager reste à implémenter.

## Limites restantes
- Le téléchargement des blobs utilise actuellement l'API Base64 et n'est pas encore optimisé pour les gros fichiers.
- La défense contre les changements concurrents du système de fichiers (TOCTOU) devra être renforcée lors de la mise en place du moteur asynchrone.
- Les quotas API, temporisations, limites de taille et reprise doivent être renforcés.
- Les analyseurs de formats actifs devront rester sans extraction/exécution automatique.
