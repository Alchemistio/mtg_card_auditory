# Cahier des charges — MTG Asset Auditor

## Mission
Outil Windows autonome en Rust pour auditer les assets d'un dossier local ou de n'importe quel dépôt GitHub choisi par l'utilisateur. Le logiciel analyse ; il ne supprime et ne redistribue rien automatiquement.

## Sources configurables
- Dépôt GitHub public ou privé, URL ou propriétaire/nom.
- Branche, tag ou commit ; résolution du SHA immuable.
- Sous-répertoire, filtres de chemin, extension, taille et type.
- Détection de la branche par défaut.
- Jeton à permissions minimales pour dépôts privés, champ masqué, enregistrement facultatif et explicite dans Windows Credential Manager ; aucun secret dans les rapports, journaux ou fichiers du workspace. Les buffers contrôlés par l'application sont effacés au mieux à la libération via `zeroize`.
- Exploration de l'arbre Git sans cloner les binaires en masse.
- Téléchargement sélectif, cache par commit/blob SHA vérifié avec le SHA-1 Git, reprise et limites de concurrence ; taille maximale de blob et validation de cohérence entre taille déclarée et octets reçus.
- Gestion des arbres tronqués, rate limits, erreurs 403/404/429 et réseau ; retries bornés pour les erreurs transitoires, respect prudent de `Retry-After` et diagnostic du reset de quota.
- Aucun dépôt cible codé en dur.

## Sources locales
- Sélection d'un ou plusieurs dossiers, analyse récursive, exclusions ; les chemins d'inventaire sont préfixés par source pour éviter les collisions.
- Liens symboliques et boucles traités explicitement.
- Sources locales en lecture seule.

## Analyse technique
- Inventaire stable : ID, chemin, nom, extension, taille, source et statut.
- SHA-256 des octets, distinct du Git blob SHA-1.
- Raster : format réel, dimensions, orientation, canaux et transparence.
- SVG : XML, dimensions, viewBox, références externes et aperçu sécurisé.
- Fonts : famille, style, poids et métadonnées ; aucune licence déduite.
- ZIP : liste du contenu, tailles, chemins suspects, protection zip-bomb ; extraction désactivée par défaut.
- HTML/JSON/XML/MSE/texte : type, encodage, notices et dépendances.
- Formats inconnus : empreinte et type non interprété.

## Analyse visuelle
- Miniatures et planches de contact paginées.
- Galerie HTML hors ligne.
- Filtres par famille, source, extension, dimensions et statut.
- Comparaison côte à côte, variantes et annotation humaine.
- Doublons binaires et candidats visuels, sans suppression.
- Signalement des noms génériques et ressources à identifier.

## Taxonomie MTG
Frames (Classic, Modern, M15, Old-Border, Showcase, Borderless, Full-Art, Special), Components (Borders, Crowns, Masks, Textures, Watermarks, Stamps), Symbols (Mana, Types, Loyalty, Counters, Special), Tokens, Fonts, Layouts, templates et packages MSE.
Les catégories automatiques restent des suggestions, jamais une validation.

## Provenance et licences
- Source, dépôt, branche, commit, chemin, auteur lorsqu'il est établi.
- Collecte LICENSE/LICENCE/COPYING/NOTICE/README.
- Statuts licence : unknown, pending-review, verified, not-for-redistribution.
- Statuts asset : source-only, needs-visual-review, needs-license-review, ready-for-conversion, normalized, integrated, rejected.
- Périmètre de licence explicite ; dépendances MSE et références relatives préservées.
- Historique des décisions, preuves et attribution.
- Aucune licence inventée ni validation juridique automatique.

## Exports
- JSON versionné, CSV UTF-8, rapport HTML autonome et planches.
- Statistiques par collection, format, taille, erreurs et avertissements.
- Résultats dans le dossier choisi, jamais dans le dépôt source.
- Rapport reproductible : version, date, source, commit, options et limites.

## Performance et ergonomie
- Traitement parallèle, limites mémoire et réseau.
- Progression en octets, journal persistant des téléchargements GitHub et annulation coopérative du scan local et des téléchargements entre deux blobs. À la relance, les fichiers terminés ne sont sautés qu'après vérification de leur taille et de leur SHA-1 Git ; les destinations invalides ne sont pas écrasées.
- Cible de référence : 37 371 fichiers / environ 1,75 Go.
- Interface française en premier.
- Exécutable Windows x64 autonome, sans Python/Node/Docker/runtime externe.

## Sécurité
- Lecture seule GitHub et sources locales.
- Aucun secret dans logs, exports ou commits.
- Validation des chemins et limites de taille/profondeur ; confinement capability-based des opérations workspace (téléchargements, cache, journaux, manifestes, exports JSON/CSV/HTML/PNG et index), création exclusive et tests de non-écrasement. Un test Windows conditionnel couvre la tentative de traversée d'un lien symbolique vers une cible externe ; l'exécution de cette branche doit être rendue observable. Les garanties TOCTOU doivent être documentées et testées selon les opérations et plateformes.
- Ne jamais exécuter scripts, macros, HTML ou fichiers téléchargés.
- Pas de suppression, déplacement, renommage ou normalisation automatique.
- Promotion vers une bibliothèque approuvée uniquement après revue visuelle, provenance, licence et test d'intégration.

## Référence MTG
Dépôt de référence : Alchemistio/mtg-card-remodeler-public, branche main, répertoire Assets/.
Inventaire de référence : Full-Magic-Pack-main 34 441 ; Big Pack 1 996 ; MtgCard Builder 610 ; MANA 314 ; Fonts 10. Total 37 371, environ 1,75 Go. Ces chiffres sont informatifs, jamais codés en dur.

## Acceptation
L'utilisateur doit pouvoir choisir un dépôt différent, y compris privé ; explorer sans clone complet ; sélectionner et télécharger des fichiers ; relier les rapports au commit exact ; et garantir l'absence de modification des sources.
