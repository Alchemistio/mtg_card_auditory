# Feuille de route

## Tableau de pilotage par domaine

Les pourcentages ci-dessous sont des estimations de maturité fonctionnelle, pas des mesures de lignes de code ni des garanties de qualité. Ils doivent être réévalués à chaque reprise à partir du code, des critères d'acceptation et des preuves CI du HEAD courant. Les tâches détaillées et leurs critères restent ceux des sections P0 à P3 et des phases ci-dessous.

| Domaine | Avancement estimé | État réel et prochain travail |
|---|---:|---|
| Fondation, architecture et workflow | 90 % | Base Rust, architecture, index documentaire et workflow autonome en place. Reste à garder les documents synchronisés, consolider les critères de livraison et fermer les derniers risques transversaux. |
| Connectivité GitHub et intégrité des sources distantes | 90 % | Résolution SHA, exploration, cache vérifié, provenance, reprise et téléchargements bornés en place. Reste la reprise au niveau des octets, conditionnée à un support Range/ETag fiable, et les validations finales de bout en bout. |
| Inventaire local et exports | 85 % | Parcours capability-based, SHA-256, comparaison et exports JSON/CSV/HTML en place. Reste l'estimation de durée, les cas concurrents/répertoires et quelques contrôles d'intégrité. |
| Sécurité des fichiers et concurrence (TOCTOU) | 60 % | Protections capability-based, non-suivi des liens symboliques sur les chemins testés, contrôles de métadonnées avant/après et tampon unique des planches. Reste le contrôle exact ouverture/metadata, mutations préservant taille/mtime, handle unique ou snapshot, remplacement de racines/sous-répertoires et races de destinations, avec preuves Windows/Unix explicites. |
| Analyse des formats, ZIP et MSE | 65 % | Reconnaissance MSE, dépendances/références, budgets et plusieurs corruptions ZIP testées. Les deux sens de taille incohérente et un nom UTF-8 atypique sont validés ; une fixture de données locales tronquées est ajoutée et attend sa validation CI. Restent tailles/flags/offsets supplémentaires, encodages non UTF-8, chemins ambigus, limites et autres cas adversariaux. |
| Métadonnées raster, SVG, polices et notices | 55 % | Métadonnées de base en place ; SVG borné sans exécution, viewBox et dimensions racine intégrées ; polices identifiées. Reste orientation/limites/décodage raster, couverture CSS SVG et aperçu sandboxé, formats de polices complémentaires et collecte de notices avec provenance sans interprétation juridique. |
| Rapports, comparaison et synthèses MSE | 48 % | Le rapport HTML expose les noms complets, versions de package et noms de jeu MSE ; les tests vérifient leur affichage et l'échappement des valeurs non fiables. Reste synthèse des ressources ignorées/raisons de troncature, filtres et staging séparé de LIBRARY. |
| Galerie et revue humaine | 15 % | Planche contact raster paginée initiale. Reste galerie hors ligne, navigation/filtres, ouverture sûre, annotations, décisions persistantes et suggestions de similarité sans suppression automatique. |
| Performance, expérience d'analyse et estimation | 20 % | Progression en octets et annulation disponibles. Reste mesures sur corpus synthétiques, estimation de durée recalculée et contrôle de la réactivité UI. |
| Packaging et publication | 35 % | CI Windows construit et publie un artefact x64. Reste vérifier l'autonomie sur Windows propre, versionnement, notes de version, GitHub Release reproductible et étude de signature. |

**Avancement global indicatif : 45 %.** Il reste volontairement prudent : plusieurs domaines sont amorcés mais n'ont pas encore passé leurs critères d'acceptation de bout en bout.

### Règle de balayage transversal
Avant de terminer chaque cycle de développement, examiner systématiquement chacun des domaines ci-dessus et répondre dans le suivi de travail : (1) existe-t-il une tâche concrète et autonome qui peut avancer maintenant ? (2) dépend-elle du code ou d'une CI en cours ? (3) quel test/critère permettra de la valider ? Si une tâche indépendante peut être avancée sans dégrader la cohérence du commit ni annuler une CI, la réaliser ; sinon, préparer l'analyse ou le test et l'inscrire comme prochaine action avec son blocage précis. Ne pas forcer des modifications artificielles dans tous les domaines à chaque cycle.

## File d'exécution priorisée et critères d'acceptation

Cette file complète les phases ci-dessus : elle transforme les chantiers partiels en tâches vérifiables. L'ordre peut évoluer si un audit révèle un risque plus prioritaire.

### P0 — Reproductibilité et intégrité des sources

1. **Versionner le lockfile** — terminé.
   - Réalisé : `Cargo.lock` v4 (115 720 octets) récupéré depuis l'artefact Windows historique ; son manifeste correspond au `Cargo.toml` courant (blob SHA `2d1a0d7ae3204b099c2c1c27e6de139b37dd4abc`). Windows et Unix utilisent désormais ce lockfile ; `cargo generate-lockfile` et l'upload de lockfile temporaire ont été retirés ; Clippy, tests et build Windows utilisent `--locked`.
   - Preuve : CI Unix `43738862441` et Windows `43738862446` vertes sur le même commit de PR `dbdd55f70e66369989bb5ac3c0bbc2a24eea5d7b` ; l'arbre de ce commit est identique à celui du commit de fusion `37521c3fdd3451592afdd5af59b80897184846d7`. Validation supplémentaire du HEAD `e77dfe8` : Unix run `43739937199` (62 tests réussis) et Windows run `43739937239` (formatage, Clippy, tests, build Release et upload de l'exécutable réussis). `cargo fmt` ne propose pas d'option `--locked` dans sa commande dédiée.

2. **Détecter les modifications concurrentes pendant l'analyse locale** — partiellement implémentée.
   - Réalisé : comparer taille, horodatage de modification et type de fichier avant/après les lectures des analyseurs ; le scan échoue explicitement si ces métadonnées changent ; test déterministe de changement de taille. Après l'indexation secondaire des ressources ZIP MSE, recalculer le SHA-256 des archives et refuser l'inventaire si le contenu diffère du hash initial ou si les métadonnées changent pendant ce contrôle ; test de non-régression sur archive remplacée après scan.
   - Réalisé dans cette tranche : `analyze_file` contrôle `symlink_metadata` avant/après les analyseurs et refuse un chemin devenu lien symbolique ou fichier non régulier ; test Unix simulant le remplacement après énumération. Les archives ZIP MSE appliquent le même refus au contrôle final.
   - Reste à faire : course exacte entre le contrôle `symlink_metadata` et l'ouverture, mutations conservant taille/horodatage, handle unique/instantané, sélection initiale et remplacement de répertoires.

3. **Fermer la fenêtre de remplacement des planches de contact** — terminée.
   - Réalisé : chaque image est lue une seule fois depuis un handle capability-based dans un tampon limité à 100 Mio ; SHA-256, dimensions et pixels sont calculés à partir des mêmes octets. Test de non-régression : une source modifiée depuis le scan est refusée et notée dans l'index. Aucun fichier source n'est écrit.

4. **Éprouver la sélection initiale des sources et les exports sous concurrence** — partiellement implémentée.
   - Réalisé : test Unix de destination d'export symlinkée (la cible reste intacte), test concurrent à deux écrivains (exactement un gagnant, exécuté sur CI Unix et Windows), test d'échec d'écriture avec suppression du fichier partiel, et test de remplacement d'une source par lien symbolique après énumération (test Unix).
   - Reste à faire : remplacement concurrent de répertoire racine/sous-répertoire, course entre validation du parent de destination et ouverture du handle, et scénarios Windows de destination symlinkée/création concurrente. Les tests Windows conditionnels doivent rendre chaque `SKIP` visible et ne pas être comptés comme preuve d'exécution.

### P1 — Robustesse des formats et tests

5. **Compléter les fixtures ZIP/MSE malformées**.
   - Réalisé : archive MSE globalement corrompue et ressource dont les octets ne correspondent plus au CRC déclaré ; le test dédié confirme `read-error` sans SHA-256, sans panique ni extraction. Validé sur CI Unix `43740381338` (63 tests) et Windows `43740381471` (61 tests).
   - Réalisé dans cette tranche : test d'une ressource Deflate dont le flux compressé est altéré sans modifier la table centrale ni le CRC ; le statut attendu est `read-error`, sans SHA-256 ni extraction. Validé sur le HEAD `006aef27e2cc9063aa06721ebe8f2e52b54a1ffd` : CI Unix run `43742700037` (68 tests réussis) et Windows run `43742700103` (66 tests réussis), avec formatage et Clippy ; Windows a aussi compilé le binaire Release et téléversé l'artefact.
   - Validé sur le HEAD `de0115b2edf51d4e85f0c0cbb7e4093e8de587d6` : fixture ZIP MSE dont la fin de table centrale est tronquée ; le scan signale un avertissement contrôlé et n'indexe aucune ressource. CI Unix run `43747947924` : formatage, Clippy et 69 tests réussis ; CI Windows run `43747947944` : formatage, Clippy, 67 tests réussis, build Release x64 et upload de l'artefact `mtg-asset-auditor-windows-x64` réussis.
   - Validé sur le HEAD `2c19e229a9ba4d9cfed77862b49986d685789ff7` : fixture MSE ZIP avec offset de table centrale invalide (`u32::MAX`) ; avertissement contrôlé, aucune ressource indexée ni extraite. CI Unix run `43805292965` : formatage, Clippy et 70 tests réussis ; CI Windows run `43805292966` : formatage, Clippy, 68 tests réussis, build Release x64 et upload de l'exécutable réussis. Aucun test ignoré dans les résultats des suites.
   - Validé sur le HEAD `069c34e6fd9501eb3689820e3b41cb0e2cb11394` (commit documentaire) : CI Unix run `43828091330` a exécuté formatage, Clippy et 70 tests (70 réussis, 0 échoué, 0 ignoré) ; CI Windows run `43828091369` a exécuté formatage, Clippy et 68 tests (68 réussis, 0 échoué, 0 ignoré), puis build Release x64 et upload de `mtg-asset-auditor-windows-x64` (artefact ID `14207382470`, 5 312 059 octets).
   - Validé sur le HEAD `818e4f4e43b66f118d012c296488badbc85f1625` : fixture ZIP MSE dont la taille décompressée déclarée dans la table centrale dépasse d'un octet la taille réelle ; la ressource est marquée `size-mismatch` sans SHA-256. CI Unix run `43831180611` : formatage, Clippy et 71 tests réussis (0 échec, 0 ignoré). CI Windows run `43831180639` : formatage, Clippy et 69 tests réussis (0 échec, 0 ignoré), build Release x64 et upload de l'artefact `mtg-asset-auditor-windows-x64` réussis (artefact `14207669916`).
   - Validé sur le HEAD `1385b136395618c6d1a785bf418fada68872b416` : taille décompressée centrale inférieure d'un octet au contenu réel (`size-mismatch`, sans SHA-256) et conservation/hachage d'un nom UTF-8 atypique (`cards/café-🃏.bin`). CI Unix run `43833656545` : formatage, Clippy et 73 tests réussis (0 échec, 0 ignoré). CI Windows run `43833656301` : formatage, Clippy, 71 tests réussis (0 échec, 0 ignoré), build Release x64 et upload de l'artefact `14209592138` réussis.
   - Une fixture dédiée tronque physiquement la fin des données locales d'une entrée Stored tout en conservant la table centrale ZIP lisible ; le test attend `read-error`, aucun SHA-256 et aucune extraction. Validation CI en attente. Restent les autres incohérences de tailles/flags/offsets, les encodages ZIP non UTF-8, les chemins ambigus et autres anomalies.
   - Acceptation : erreurs contrôlées, aucune panique, aucune extraction, budgets respectés avant chaque lecture, annulation propagée et ressources non hachées marquées explicitement.

6. **Compléter les métadonnées raster et SVG sans activer de contenu**.
   - Raster : orientation, dimensions extrêmes, limites de décodage et miniatures plafonnées.
   - SVG : lecture plafonnée à 32 Mio, 100 000 événements XML, profondeur maximale 256 et 256 attributs par élément ; tests de dépassement de profondeur, d'événements et d'attributs. Les références relatives et les `url()` CSS avec espaces/guillemets sont détectées de façon conservative. Validation du HEAD `90702ae` : CI Unix `43741242233` (67 tests) et Windows `43741242200` (65 tests) vertes, avec formatage, Clippy, build Release et upload de l'exécutable. Dimensions SVG racine (width/height) ajoutées au modèle et au rapport ; valeurs positives finies, unités reconnues, valeurs extrêmes rejetées. Validé sur HEAD `b659a80ee417c38b29c71a09c0c9a5a3d7f9bb9d` : Unix run `43867802138` (74/74 tests) et Windows run `43867802018` (72/72 tests), formatage et Clippy réussis ; build Release x64 et upload d'artefact Windows réussis. L'assertion de non-régression vérifie également l'affichage HTML de `24px` et `12px`. Validé sur HEAD `3db39ce30622eedfe538756d9b14041a59292e3b` : Unix run `43868416295` (74/74 tests) et Windows run `43868416163` (72/72 tests), formatage et Clippy réussis ; build Release x64 et upload d'artefact Windows réussis. Restent : couverture complète de la grammaire CSS et aperçu sandboxé.
   - Acceptation : fixtures malveillantes synthétiques, temps/mémoire bornés et aucun script/ressource externe exécuté.

7. **Préciser les formats de polices et les notices**.
   - Acceptation : formats reconnus listés, fichiers invalides/volumineux bornés ; notices collectées avec chemin, hash et extrait borné, sans inférer de licence ni statut juridique.

### P2 — Revue et rapports

8. **Galerie hors ligne et revue humaine persistante**.
   - Acceptation : navigation paginée, filtres, ouverture locale sûre, annotations et décisions persistées séparément des sources ; aucune ressource active exécutée.

9. **Rapports de comparaison et synthèses MSE**.
   - Acceptation : synthèse des packages, dépendances, références, ressources ignorées et raisons de troncature ; HTML autonome avec échappement systématique et export JSON versionné.

10. **Candidats visuels similaires**.
    - Acceptation : métrique documentée, seuil ajustable, faux positifs présentés comme suggestions et aucune déduplication/suppression automatique.

### P3 — Livraison

11. **Mesures de performance et estimation de durée**.
    - Acceptation : estimation basée sur progression observée, recalculée et présentée comme indicative ; tests sur corpus synthétique de tailles variées et absence de blocage UI.

12. **Paquet Windows autonome et publication**.
    - Acceptation : exécutable x64 lancé sur Windows propre sans runtime non prévu ; version sémantique, notes de version, artefacts signés si certificat disponible et GitHub Release reproductible.

## Phase 0 — Fondation
- [x] Index documentaire central créé dans `docs/INDEX.md`, avec correspondance entre domaines et documents de référence.
- [x] README actualisé pour référencer l'index et ne plus présenter les fonctions MSE déjà intégrées comme non implémentées.
- [x] Documentation d'architecture corrigée pour rétablir le retour à la ligne entre les références de modules.
- [x] Architecture et cahier des charges réalignés sur l'énumération capability-based déjà implémentée et sur les fonctions MSE réellement disponibles.
- [x] Audit documentaire de reprise : preuves CI historiques actualisées dans `SECURITY.md` (56 tests sur `a9ed50a`, sans les présenter comme validation du HEAD courant) et description de l'architecture corrigée pour refléter la résolution locale des dépendances/références MSE.
- [x] Workflow de reprise modifié pour consulter obligatoirement l'index et la feuille de route à chaque session, vérifier les tâches sur le code et ne clôturer qu'avec des preuves.
- [x] Dépôt Rust initialisé.
- [x] Cahier des charges, architecture et sécurité.
- [x] Fenêtre desktop initiale.
- [x] Première exploration de l'arbre GitHub.
- [x] Téléchargement sélectif initial des blobs.
- [x] CI Windows complète sur commit `c1c71fd` : formatage, Clippy, tests, build Release et upload d'artefact.
- [x] CI Windows : formatage, Clippy, 43 tests, build Release et upload d'artefact ; validation du HEAD `93b7b65` (run `43635452185`).
- [x] CI Unix : formatage, Clippy et 43 tests de sécurité (dont liens symboliques, lectures bornées, reconnaissance MSE ZIP/dossiers, en-têtes, dépendances, références, versions minimales et cycles) ; validation du HEAD `93b7b65` (run `43635452155`).
- [x] Workflow de développement autonome documenté dans `docs/WORKFLOW.md` ; les tâches indépendantes continuent pendant l'exécution des CI.
- [x] Correction des workflows Windows et Unix : suppression de l'étape `cargo fmt --all` qui modifiait les sources avant le contrôle ; le formatage est désormais vérifié sans mutation. Validation des nouveaux workflows encore en attente sur un HEAD récent.

## Phase 1 — Connectivité GitHub robuste
- [x] Effacement au mieux des buffers contrôlés par l'application via `zeroize::Zeroizing<String>` : champ, tampon temporaire egui et copies explicites transmises aux workers. CI Windows validée ; les copies internes des bibliothèques et du système restent hors garantie.
- [x] `Cargo.lock` v4 versionné depuis l'artefact historique `cargo-lockfile` (run `43625687052`, 115 720 octets) ; cohérence vérifiée avec le manifeste courant par `cargo metadata --locked --no-deps`. Les workflows Windows et Unix utilisent désormais le lockfile commun ; Clippy/tests/build Windows sont verrouillés avec `--locked`. CI Unix et Windows vertes sur le même arbre source (commit PR `dbdd55f`, arbre identique au commit de fusion `37521c3`).
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
- [x] Nouvelles tentatives bornées (3 retries maximum, soit 4 requêtes totales) sur erreurs réseau transitoires et réponses HTTP 429/502/503/504 ; prise en compte bornée de `Retry-After` numérique ou date HTTP ; tests de politique de retry.
- [x] Cache GitHub local sous le workspace, indexé par owner/repository/commit/blob SHA et vérifié avec le SHA-1 Git avant réutilisation.
- [x] Journal JSONL persistant des téléchargements par dépôt et commit complet ; reprise des fichiers terminés avec vérification SHA-1 Git et taille, sans écrasement des destinations invalides.
- [x] Lecture du journal JSONL bornée à 64 Mio en flux, avec test de non-régression à la limite et au-delà.
- [x] Isolation des journaux de reprise et manifestes par owner/repository pour éviter les collisions entre dépôts.
- [x] Diagnostic du quota restant et de la fenêtre de reset depuis les en-têtes de réponse GitHub (valeurs `x-ratelimit-remaining` et `x-ratelimit-reset`).
- [x] Stockage explicite du jeton dans Windows Credential Manager : charger, enregistrer et supprimer ; aucun enregistrement automatique. CI Windows complète validée.
- [x] Taille maximale par blob (90 Mio), contrôle de taille et vérification du SHA-1 Git de l'objet blob.
- [x] Annulation coopérative des téléchargements GitHub entre deux blobs ; le blob en cours termine avant l'arrêt.
- [x] Téléchargement streaming des blobs GitHub vers le cache temporaire, contrôle taille et SHA-1 Git en flux, puis copie vers la destination sans charger le blob entier en mémoire.
- [ ] Reprise d'un blob interrompu au niveau des octets : non engagée tant que l'API source ne documente pas `Range`/ETag de façon exploitable ; aucun fragment partiel n'est réutilisé.
- [x] Première migration capability-based des opérations GitHub (cache, destinations, journaux et manifestes) vers `WorkspaceDir`/`cap-std`, avec chemins relatifs validés ; CI Windows complète validée sur `24d85e6`.
- [x] Migrer les exports JSON/CSV/HTML, planches PNG et index texte vers des handles `cap-std`, création exclusive, synchronisation et nettoyage des fichiers partiels ; CI Windows complète validée sur `77a115e`.
- [x] Ajouter des tests de non-suivi des liens symboliques de fichiers et de répertoires sous Unix, et de fichiers sous Windows ; le test Windows affiche `SKIP` si les permissions du runner interdisent la création du lien.
- [x] Ajouter un test de confinement `WorkspaceDir` contre un lien symbolique de répertoire vers une cible externe sous Unix et Windows ; le test Windows affiche explicitement `SKIP` si le runner interdit la création du lien.
- [x] Documenter le workflow de développement autonome dans `docs/WORKFLOW.md` : poursuivre les tâches indépendantes pendant les CI, contrôler les logs et les `SKIP`, et ne s'arrêter qu'en l'absence de travail réalisable.
- [~] Énumération récursive et lectures de contenu capability-based ; SVG et polices plafonnés en flux (32/64 Mio). `analyze_file` vérifie désormais taille, horodatage et type de fichier avant/après les analyseurs, et invalide le scan si ces métadonnées changent. Après l'indexation des ressources ZIP MSE, les archives sont re-hachées et le scan est invalidé si le contenu diffère du SHA initial. Limites restantes : mutations conservant les mêmes métadonnées pendant l'analyse, handles multiples, remplacement transitoire suivi d'une restauration avant le contrôle final, sélection initiale des dossiers et courses de répertoires. Les planches utilisent désormais un tampon unique dont le SHA-256, les dimensions et les pixels sont cohérents.
- [x] Filtrage de la liste distante par chemin, extension et plage de taille.
- [x] Filtre distant par famille technique (classification indicative par extension).

## Phase 2 — Sources locales et inventaire
- [x] Sélection de plusieurs dossiers locaux, chemins préfixés par source et détection des sélections imbriquées/dupliquées.
- [x] Parcours local récursif capability-based et parallèle via Rayon, sans suivre les liens symboliques, profondeur maximale 128 ; tests dédiés Unix/Windows.
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
- [x] Annulation coopérative du scan local depuis l'interface, vérifiée à chaque entrée de répertoire et entrée énumérée, puis pendant l'analyse.
- [x] Progression locale en octets, agrégée à partir des tailles des fichiers et mise à jour par lots de 100 fichiers.
- [ ] Estimation du temps restant.

## Phase 3 — Analyse des formats
- [~] Raster : format, dimensions, canaux et alpha ; orientation, limites, décodage contrôlé et miniatures à faire.
- [~] SVG : parsing XML sans exécution, viewBox, dimensions racine width/height et références relatives/`url()` CSS détectées de façon conservative ; lecture 32 Mio, 100 000 événements, profondeur 256 et 256 attributs par élément. Dimensions racine et affichage HTML validés sur Unix/Windows (HEAD `3db39ce`, runs `43868416295` et `43868416163`) ; 74 tests Unix et 72 Windows, aucun échec ni test ignoré ; build Release x64 et artefact Windows validés. Couverture CSS complète et aperçu sandboxé à faire.
- [~] Fonts : famille/sous-famille ; métadonnées avancées et formats non pris en charge à traiter.
- [~] ZIP : inspection de la table centrale sans extraction ; analyse de formats 7z/RAR à faire.
- [x] MSE : reconnaissance des packages ZIP, vérification du fichier principal, lecture bornée à 1 Mio des champs scalaires et extraction des dépendances compactes/en bloc, sans extraction.
- [x] MSE : reconnaissance des packages sous forme de dossiers par leur fichier principal (`set`, `game`, `style`), lecture bornée et détection des directives include sans les suivre.
- [x] MSE : résolution des dépendances par identifiant de package parmi les sources locales sélectionnées ; statuts résolu/absent/ambigu, sans téléchargement ni installation.
- [x] MSE : références jeu/feuille de style résolues localement par nom déclaré, avec états résolu/absent/ambigu ; versions minimales `a.b.c` et `YYYY-MM-DD` comparées, formats inconnus non vérifiables.
- [~] MSE : cycles du graphe local détectés ; ressources des packages dossiers indexées par SHA-256 existants et ressources ZIP hachées en flux sous budgets bornés, avec taille de lecture limitée avant chaque appel au décompresseur ; vues de graphe et enrichissement des rapports restent à faire.
- [~] MSE : tests de non-extraction, fichier principal absent, archive corrompue, traversal, ratio extrême, budgets par entrée/package/scan et annulation du hachage ZIP ; UTF-8 invalide et espaces/mélanges d'indentation couverts ; corruption avancée et constructions inconnues à compléter.
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
- [~] Rapport HTML : métadonnées MSE, dépendances, références et ressources indexées avec échappement HTML ; enrichissement des filtres et synthèses à faire.
- [ ] Export vers staging, jamais directement vers LIBRARY.
- [ ] Réconciliation du manifeste MTG Card Remodeler.
- [ ] Tests de cohérence et non-régression.

## Phase 6 — Livraison
- [x] CI Windows (formatage, Clippy, 56 tests, build Release, upload exe) et CI Unix (formatage, Clippy, 56 tests) validées sur le HEAD `a9ed50a` : runs `43687351946` et `43687351991`.
- [ ] Exécutable Windows x64 autonome.
- [ ] Versionnement et notes de version.
- [ ] GitHub Releases.
- [ ] Signature de code à étudier.
