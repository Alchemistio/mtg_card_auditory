# Feuille de route

## Phase 0 — Fondation
- [x] Dépôt Rust initialisé.
- [x] Cahier des charges, architecture et sécurité.
- [x] Fenêtre desktop initiale.
- [x] Première exploration de l'arbre GitHub.
- [x] Téléchargement sélectif initial des blobs.
- [x] CI Windows complète sur commit `c1c71fd` : formatage, Clippy, tests, build Release et upload d'artefact.
- [ ] CI des retries HTTP, limites de blobs, annulation locale, filtres distants et progression en octets en attente ; ne pas considérer ce jalon validé avant le run du dernier commit.

## Phase 1 — Connectivité GitHub robuste
- [x] Parser URL HTTPS canonique de dépôt GitHub, hôte exact et validation stricte du chemin/noms ; tests des URL malformées.
- [x] Détecter la branche par défaut et résoudre branche/tag/commit en SHA immuable.
- [x] Résoudre le SHA de l'arbre Git à partir du commit.
- [x] Si l'arbre récursif est tronqué, parcourir les sous-arbres avec détection de cycle sur la pile active (les arbres identiques réutilisés à plusieurs chemins restent inventoriés).
- [x] Sous-dossier filtré côté interface.
- [x] Exploration et téléchargement GitHub dans des workers d'arrière-plan.
- [x] Progression de téléchargement par fichier.
- [x] Timeout HTTP et diagnostic structuré des rate limits (429 / 403 avec quota épuisé).
- [x] Manifeste de provenance des téléchargements : dépôt, référence demandée, commit SHA, tree SHA, horodatage, chemins et blob SHA.
- [x] Écriture exclusive des blobs et manifestes (aucun écrasement), nettoyage d'un fichier partiel en cas d'erreur ; test de non-écrasement.
- [x] Nouvelles tentatives bornées (3 retries maximum, soit 4 requêtes totales) sur erreurs réseau transitoires et réponses HTTP 429/502/503/504 ; prise en compte bornée de `Retry-After` ; tests de politique de retry.
- [x] Cache GitHub local sous le workspace, indexé par owner/repository/commit/blob SHA et vérifié avec le SHA-1 Git avant réutilisation.
- [ ] Reprise d'une opération multi-fichiers après fermeture et affichage du quota restant.
- [ ] Dépôts privés et stockage du jeton dans Windows Credential Manager.
- [x] Taille maximale par blob (90 Mio), réponse JSON bornée (128 Mio), contrôle de taille et vérification du SHA-1 Git de l'objet blob.
- [x] Annulation coopérative des téléchargements GitHub entre deux blobs ; le blob en cours termine avant l'arrêt.
- [ ] Téléchargement streaming et reprise d'une opération multi-fichiers après fermeture.
- [x] Filtrage de la liste distante par chemin, extension et plage de taille.
- [x] Filtre distant par famille technique (classification indicative par extension).

## Phase 2 — Sources locales et inventaire
- [x] Sélection de plusieurs dossiers locaux, chemins préfixés par source et détection des sélections imbriquées/dupliquées.
- [x] Parcours local parallèle via Rayon, sans suivre les liens symboliques.
- [x] Scan local exécuté dans un worker d'arrière-plan.
- [x] Progression en octets traités (mise à jour par lots de 100 fichiers et à la fin).
- [x] Modèle d'inventaire versionné (v1), tri stable et SHA-256 en flux ; anciens inventaires v1 acceptés avec champs optionnels par défaut.
- [x] Détection des doublons binaires exacts par SHA-256, sans suppression.
- [x] Comparaison de deux inventaires JSON par chemin et SHA-256 (ajouté/supprimé/modifié/identique).
- [x] Export JSON versionné, CSV UTF-8 et rapport HTML autonome hors ligne.
- [x] Export JSON versionné du rapport de comparaison, hors des deux sources.
- [x] Classification indicative par extension.
- [x] Repérage des fichiers LICENSE/LICENCE/COPYING/NOTICE/README/AUTHORS/CREDITS sans interprétation juridique.
- [x] Métadonnées raster : format, dimensions, type de canaux et présence d'alpha depuis le décodeur sans décodage des pixels.
- [x] Métadonnées SVG initiales : XML bien formé, viewBox et détection indicative de références externes.
- [x] Métadonnées de polices : famille et sous-famille lorsque lisibles par ttf-parser.
- [x] Inspection ZIP sans extraction : nombre d'entrées, taille déclarée, chemins suspects et ratios anormaux ; fichier limité à 512 Mio.
- [x] Génération de planches raster PNG paginées (16 éléments/page) avec index texte ; SHA vérifié avant aperçu.
- [x] Refus des exports dans l'arborescence source.
- [x] Vue filtrable des résultats locaux dans l'interface (500 premières lignes affichées, exports complets).
- [x] Annulation coopérative du scan local depuis l'interface, vérifiée entre les étapes de parcours et d'analyse.
- [x] Progression locale en octets, agrégée à partir des tailles des fichiers et mise à jour par lots de 100 fichiers.
- [ ] Estimation du temps restant.

## Phase 3 — Analyse des formats
- [~] Raster : format, dimensions, canaux et alpha ; orientation, limites, décodage contrôlé et miniatures à faire.
- [~] SVG : parsing XML sans exécution, viewBox et détection de références externes ; aperçu sandboxé et inspection CSS complète à faire.
- [~] Fonts : famille/sous-famille ; métadonnées avancées et formats non pris en charge à traiter.
- [~] ZIP : inspection de la table centrale sans extraction ; analyse de formats 7z/RAR à faire.
- [ ] MSE : packages, dépendances et couches.
- [ ] Collecte du contenu des notices, licences et provenance avec preuves.

## Phase 4 — Revue visuelle
- [~] Première planche de contact raster paginée avec index.
- [ ] Galerie visuelle HTML avec navigation, filtres et comparaison.
- [ ] Planche contact avancée avec libellés, annotations et sélection.
- [ ] Candidats visuels similaires (les doublons binaires exacts sont déjà identifiés).
- [ ] Revue humaine persistante.

## Phase 5 — Rapports et intégration MTG

> L'intégration au MTG Card Remodeler est volontairement suspendue jusqu'à finalisation de son cahier des charges. Aucun code ni asset n'est intégré au Remodeler à ce stade.
- [x] Rapport HTML autonome initial.
- [x] Rapport JSON de comparaison initial.
- [ ] Statistiques et alertes enrichies.
- [ ] Export vers staging, jamais directement vers LIBRARY.
- [ ] Réconciliation du manifeste MTG Card Remodeler.
- [ ] Tests de cohérence et non-régression.

## Phase 6 — Livraison
- [ ] CI test, clippy, build sur le dernier commit.
- [ ] Exécutable Windows x64 autonome.
- [ ] Versionnement et notes de version.
- [ ] GitHub Releases.
- [ ] Signature de code à étudier.
