# Feuille de route

## Tableau de pilotage par domaine

Les pourcentages ci-dessous sont des estimations de maturité fonctionnelle, pas des mesures de lignes de code ni des garanties de qualité. Ils doivent être réévalués à chaque reprise à partir du code, des critères d'acceptation et des preuves CI du HEAD courant. Les tâches détaillées et leurs critères restent ceux des sections P0 à P3 et des phases ci-dessous.

| Domaine | Avancement estimé | État réel et prochain travail |
|---|---:|---|
| Fondation, architecture et workflow | 90 % | Base Rust, architecture, index documentaire et workflow autonome en place. Reste à garder les documents synchronisés, consolider les critères de livraison et fermer les derniers risques transversaux. |
| Connectivité GitHub et intégrité des sources distantes | 90 % | Résolution SHA, exploration, cache vérifié, provenance, reprise et téléchargements bornés en place. Reste la reprise au niveau des octets, conditionnée à un support Range/ETag fiable, et les validations finales de bout en bout. |
| Inventaire local et exports | 85 % | Parcours capability-based, SHA-256, comparaison et exports JSON/CSV/HTML en place ; progression en octets, annulation et ETA indicative sont implémentées. Restent les cas concurrents/répertoires et quelques contrôles d'intégrité. |
| Sécurité des fichiers et concurrence (TOCTOU) | 71 % | Protections capability-based, contrôles de métadonnées avant/après, ouvertures sans suivi composant par composant pour fichiers et sous-répertoires récursifs, et tampon unique des planches. `analyze_file` ouvre désormais un seul handle via `open_file_nofollow` et le réutilise pour SHA-256, raster, SVG, polices, ZIP et en-têtes MSE, avec `seek(0)` explicite avant chaque passe ; les métadonnées du handle et du chemin sont contrôlées avant/après. Test JPEG hash puis orientation sur le même handle ; re-hachage SHA-256 final du handle après les analyseurs. Test Unix : mutation de même longueur avec restauration du mtime ; test Windows : mutation de même longueur et détection SHA-256 (SetFileTime refusé sur handle ouvert). Validé sur HEAD `9a0f03e789acbe9067e5c6d7d39ec946877920ef` : Unix run `44086193250` (84/84 tests, aucun ignoré), Windows run `44086193385` (81/81 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14328539658` (5 359 706 octets, SHA-256 `98bdbe9353459cce1cf73113dfcfc4cace8fb4036a952cf430f5aa3854a97eff`). Validé sur HEAD `7737b10dd54201dd60d0229fab36726a56910065` : Unix run `44056083010` (83/83 tests) et Windows run `44056083096` (80/80 tests), aucun ignoré ; formatage et Clippy réussis sur les deux ; tests Unix/Windows de liens symboliques réussis sans `SKIP` ; build Release x64 et artefact Windows `14315552903` (5 358 237 octets, SHA-256 `7e216e68a8bc64f1a9f639299e1597daf02a6d1017a2eea64c6bc79b6aa7ae14`). Restent les remplacements de répertoires après énumération et les courses de destinations. L'indexation ultérieure des ressources ZIP MSE conserve une ouverture et un re-hachage distincts. |
| Analyse des formats, ZIP et MSE | 70 % | Reconnaissance MSE, dépendances/références, budgets et corruptions ZIP testées. Premier contrôle de noms Windows ZIP ajouté : chemins ambigus (périphériques réservés, caractères interdits, séparateurs/segments dangereux, points/espaces terminaux) écartés du hachage et collisions de casse/normalisation détectées ; validé sur HEAD `c935ea9a8c3f6ff0781403c9fc16a5adb6190ff9` par Unix run `44513214541` et Windows run `44513214470` (formatage, Clippy et tests verts ; build Release et upload Windows verts). Le parseur de la table centrale valide désormais les noms bruts avant de faire confiance au lecteur ZIP : EFS (bit 11) exige un UTF-8 strict, les noms sans EFS restent interprétables en CP437, et l'extra field Unicode Path `0x7075` est contrôlé (structure TLV, unicité, version 1, CRC32 du nom brut, UTF-8 strict). L'en-tête local est relu avec son extra field ; si les deux côtés fournissent un chemin Unicode, ils doivent correspondre. Les noms bruts locaux et centraux doivent rester identiques. Tests unitaires dédiés ajoutés pour UTF-8 invalide, CP437, CRC, version, UTF-8 Unicode et champs tronqués/dupliqués ; un champ Unicode contradictoire avec EFS est également rejeté. Premier passage CI Unix `44514703601` et Windows `44514703608` : échec limité au formatage `cargo fmt --check`, avant compilation/tests ; correctif de formatage et durcissement EFS préparés dans le commit suivant. Le compte brut EOCD/ZIP64 est vérifié avant l'interprétation MSE ; les candidates EOCD sont filtrées par fin de commentaire et cohérence de table centrale. Les en-têtes locaux/centraux sont désormais rapprochés (signature, flags, méthode, nom et borne du début des données), avec ajustement des offsets classiques SFX ; CRC/tailles/troncatures restent diagnostiqués au niveau des ressources. Validé sur HEAD `84171296dbbf935cf116ac9c62a7404ea9196661` : Unix `44341054195` (104/104) et Windows `44341054213` (tests, build Release et upload réussis). Restent ZIP64 par entrée, fixtures SFX adversariales, noms non UTF-8, chemins ambigus et doublons du fichier principal. |
| Métadonnées raster, SVG, polices et notices | 63 % | Métadonnées raster de base et orientation EXIF validées sur Unix/Windows ; extrait de notice UTF-8 plafonné à 4 Kio capturé dans le même flux que le SHA-256, échappé dans le rapport et avec troncature signalée, validé sur Unix/Windows. SVG borné sans exécution, viewBox et dimensions racine validées et affichées dans le rapport HTML ; polices identifiées. Restent dimensions extrêmes/limites de décodage raster, couverture CSS SVG et aperçu sandboxé, formats de polices complémentaires et provenance documentaire plus complète, sans interprétation juridique. |
| Rapports, comparaison et synthèses MSE | 50 % | Le rapport HTML expose les noms complets, versions de package et noms de jeu MSE ; les tests vérifient leur affichage et l'échappement des valeurs non fiables. Résumé des empreintes calculées/non calculées, ventilation par motif de refus et avertissement d'indexation partielle validés sur Unix/Windows. Restent filtres et staging séparé de LIBRARY. |
| Galerie et revue humaine | 16 % | Planche contact raster paginée avec application de l'orientation EXIF lors de l'aperçu, validée sur Unix/Windows ; restent galerie hors ligne, navigation/filtres, ouverture sûre, annotations, décisions persistantes et suggestions de similarité sans suppression automatique. |
| Performance, expérience d'analyse et estimation | 23 % | Progression en octets, annulation et ETA indicative calculée depuis la vitesse moyenne observée en place et validées sur Unix/Windows ; restent mesures sur corpus synthétiques, calibration de l'estimation et contrôle de la réactivité UI. |
| Packaging et publication | 35 % | CI Windows construit et publie un artefact x64. Reste vérifier l'autonomie sur Windows propre, versionnement, notes de version, GitHub Release reproductible et étude de signature. |

**Avancement global indicatif : 46 %.** Il reste volontairement prudent : plusieurs domaines sont amorcés mais n'ont pas encore passé leurs critères d'acceptation de bout en bout.

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
   - Réalisé dans cette tranche : ouverture sans suivi composant par composant avec `DirExt::open_dir_nofollow` pour les parents et `OpenOptionsFollowExt::follow(FollowSymlinks::No)` pour le fichier final ; tous les analyseurs, y compris raster, SVG, polices, ZIP/MSE et la réanalyse des planches utilisent ce chemin, avec vérification du type régulier après ouverture. Tests Unix sur liens parent/final et tests Windows sur liens parent/final ajoutés. Validé sur HEAD `71acb65cda232ed9b043d7d24c558a7fda5c881c` : Unix 83/83 et Windows 80/80, aucun test ignoré, formatage et Clippy réussis ; tous les tests de liens symboliques ont été exécutés sans `SKIP`, build Release x64 et artefact Windows `14315271017`.
   - Réalisé et validé sur HEAD `7737b10dd54201dd60d0229fab36726a56910065` : `analyze_file` ouvre une seule fois le fichier via `open_file_nofollow`, puis réutilise ce handle pour le SHA-256 et les analyseurs raster/SVG/police/ZIP/MSE ; chaque lecteur se repositionne explicitement au début. Les métadonnées du handle ouvert sont comparées à celles du chemin avant et après les lectures. Un test JPEG calcule le SHA-256 puis lit l'orientation sur le même handle, vérifiant le repositionnement. CI Unix `44056083010` (83/83) et Windows `44056083096` (80/80), formatage/Clippy et build Release Windows réussis.
   - Réalisé et validé sur HEAD `9a0f03e789acbe9067e5c6d7d39ec946877920ef` : re-hachage SHA-256 du même handle après les analyseurs ; test Unix de mutation à longueur constante avec restauration du mtime, test Windows de mutation à longueur constante et rejet SHA-256. CI Unix `44086193250` (84/84 tests, aucun ignoré) et Windows `44086193385` (81/81 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14328539658`. Cette vérification détecte les mutations persistantes entre le premier hash et la fin des analyseurs, mais pas une mutation transitoire restaurée avant le re-hachage final.
   - Réalisé dans cette tranche : la récursion ouvre chaque sous-répertoire depuis le handle capability-based du parent via `open_dir_nofollow`, puis énumère depuis le handle enfant épinglé ; une substitution entre `file_type()` et l'ouverture échoue au lieu de suivre un lien. Test Unix déterministe et test Windows conditionnel ajoutés ; validation CI à venir.
   - Validé sur HEAD `2084c365776bf4545120da438ecee70262a2109e` : ouverture capability-based sans suivi de chaque sous-répertoire après énumération ; test Unix de remplacement par lien symbolique et test Windows exécuté sans `SKIP`. CI Unix run `44090432550` : formatage, Clippy et 88/88 tests réussis (0 ignoré). CI Windows run `44090432520` : formatage, Clippy et 85/85 tests réussis (0 ignoré), build Release x64 et artefact `14331761845` (5 358 116 octets).
   - Reste à faire : sécuriser l'identité des répertoires (racine et sous-répertoires) face au remplacement par un autre répertoire régulier et ajouter des tests de course sur le parent de destination. Une première tranche compare désormais device/inode via cap_fs_ext::MetadataExt pour les fichiers et ajoute un test de remplacement de même taille avec mtime restauré ; validation CI à venir. Les chemins absolus de sources et de parents d'export sont désormais parcourus composant par composant sans suivre les liens symboliques ; tests de non-régression en CI. Validé sur HEAD `1a22a9562b2d909cf3cc58edfc6d6c519cc3a94f` : tests Unix de non-suivi récursif et d'ouverture absolue ; CI Unix run `44091331420` (89/89 tests, aucun ignoré) et Windows run `44091331435` (86/86 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14331308670` (5 361 954 octets). Le test Windows d'ouverture absolue a été exécuté sans `SKIP`. L'indexation secondaire des ressources ZIP MSE réouvre volontairement l'archive et revalide son SHA-256.

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
   - Une fixture dédiée tronque physiquement la fin des données locales d'une entrée Stored tout en conservant la table centrale ZIP lisible ; le test attend `read-error`, aucun SHA-256 et aucune extraction. Validé sur HEAD `81e01d25beb26ea2e999b02999fbb4d5d5c25f25` : CI Unix run `43869501069` (75/75 tests, aucun ignoré) et Windows run `43869501208` (73/73 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et upload de l'artefact Windows réussis. Une fixture dédiée tronque les données locales d'une entrée Deflate en conservant la table centrale lisible ; le test attend `read-error`, aucun SHA-256 et aucune extraction. Validé sur HEAD `c62df0a5e1841110bf46507aa54e3b410c1c6135` : Unix run `44089439239` (87/87 tests, aucun ignoré) et Windows run `44089439268` (84/84 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14330693012` (5 360 216 octets, SHA-256 `b399103fc3f04826531c35e2fbb7383faecbe72823d3581594ff3888f0968b3d`). Le nombre d'entrées de la table centrale ZIP est comparé au nombre d'entrées exposées par le lecteur ; les chemins dupliqués masqués par le lecteur rendent toutes les ressources `ambiguous-path`, sans SHA-256. Le contrôle conserve aussi la détection des doublons lorsque le lecteur expose les occurrences séparément ; Validé sur HEAD `9eb6aef0ed9d739e7f64e25dbd341926f44a9c8a` : fixture ZIP valide avec deux noms de ressources dupliqués ; le lecteur masque une occurrence, mais le compteur EOCD révèle 3 entrées brutes contre 2 exposées. La ressource visible est `ambiguous-path`, sans SHA-256. CI Unix run `44094150470` (90/90 tests, aucun ignoré) et Windows run `44094150510` (87/87 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14334060248` (5 363 890 octets). Restent les autres incohérences de tailles/flags/offsets, les encodages ZIP non UTF-8, les autres chemins ambigus et anomalies.
   - Acceptation : erreurs contrôlées, aucune panique, aucune extraction, budgets respectés avant chaque lecture, annulation propagée et ressources non hachées marquées explicitement.

6. **Compléter les métadonnées raster et SVG sans activer de contenu**.
   - Raster : extraction de l'orientation via `ImageDecoder::orientation()` et affichage HTML ajoutés ; fixture JPEG EXIF validée sur HEAD `7737b10dd54201dd60d0229fab36726a56910065` (Unix `44056083010`, 83/83 ; Windows `44056083096`, 80/80 ; aucun ignoré, formatage/Clippy et build Release Windows réussis). Restent dimensions extrêmes, limites de décodage et miniatures plafonnées.
   - SVG : lecture plafonnée à 32 Mio, 100 000 événements XML, profondeur maximale 256 et 256 attributs par élément ; tests de dépassement de profondeur, d'événements et d'attributs. Les références relatives et les `url()` CSS avec espaces/guillemets sont détectées de façon conservative. Validation du HEAD `90702ae` : CI Unix `43741242233` (67 tests) et Windows `43741242200` (65 tests) vertes, avec formatage, Clippy, build Release et upload de l'exécutable. Dimensions SVG racine (width/height) ajoutées au modèle et au rapport ; valeurs positives finies, unités reconnues, valeurs extrêmes rejetées. Validé sur HEAD `b659a80ee417c38b29c71a09c0c9a5a3d7f9bb9d` : Unix run `43867802138` (74/74 tests) et Windows run `43867802018` (72/72 tests), formatage et Clippy réussis ; build Release x64 et upload d'artefact Windows réussis. L'assertion de non-régression vérifie également l'affichage HTML de `24px` et `12px`. Validé sur HEAD `3db39ce30622eedfe538756d9b14041a59292e3b` : Unix run `43868416295` (74/74 tests) et Windows run `43868416163` (72/72 tests), formatage et Clippy réussis ; build Release x64 et upload d'artefact Windows réussis. Le parseur vérifie désormais que l'élément `svg` est réellement l'élément racine, ne prend ni ses dimensions ni son `viewBox` depuis un SVG imbriqué et refuse les documents à plusieurs éléments racine. Validé sur HEAD `3fd2a04b852e9cc8c3bdd401d56b5a394b5277a4` : Unix run `44088269460` (86/86 tests, aucun ignoré) et Windows run `44088272773` (83/83 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14329589875` (5 360 216 octets, SHA-256 `b2691ce3b594c0c31aeed440d8bf87f9ba494ed9ac455f7dfbe9fd54a816c750`). Validé sur HEAD `a88f2dd4a5260c9b96401acf47220eff0b0fdc84` : Unix run `44087512178` (85/85 tests, aucun ignoré) et Windows run `44087511554` (82/82 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14330147060` (5 359 735 octets, SHA-256 `cc2f46263f02b72d9502fc503eb7929c8d12e7992007939571245f2e208f4681`). Restent : couverture complète de la grammaire CSS et aperçu sandboxé.
   - Acceptation : fixtures malveillantes synthétiques, temps/mémoire bornés et aucun script/ressource externe exécuté.

7. **Préciser les formats de polices et les notices** — partiellement réalisé.
   - Réalisé : classification des fichiers de notice et extrait UTF-8 plafonné à 4 Kio capturé dans le même flux que le SHA-256 ; octets non UTF-8 non affichés, caractères de contrôle neutralisés, échappement HTML et marqueur de troncature. Validé sur HEAD `3249fbc6d17012c03cee25bda62aefd0fe30eece` : CI Unix run `43891292770` (80 tests réussis, 0 ignoré) et Windows run `43891292737` (78 tests réussis, 0 ignoré), formatage et Clippy réussis ; build Release x64 et artefact Windows `14237900273` téléversé (5 335 434 octets, SHA-256 archive `f870bb3a70c7d2c294c723be56baa3acbbae8e37c9b5b98e8163fc87c0d9f132`).
   - Acceptation restante : couverture des formats de polices, provenance documentaire enrichie et cas de fichiers invalides/volumineux ; aucune licence ni statut juridique ne doit être inféré.

### P2 — Revue et rapports

8. **Galerie hors ligne et revue humaine persistante** — amorcée.
   - Réalisé : planche contact raster paginée, lecture unique dans un tampon borné, vérification SHA-256 et application de l'orientation EXIF lorsque disponible. Test JPEG orientation 6 validé sur HEAD `06b815c5ab423ba92016356ff8c366e4f1cfb12d` : CI Unix run `43889816446` (80/80 tests, aucun ignoré) et Windows run `43889816473` (78/78 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact Windows ID `14235129257` téléversé (5 338 528 octets, SHA-256 archive `536d621915fbd40546df31817a27b3f5c7b4640e61f206247f12cdd751590280`).
   - Acceptation restante : galerie hors ligne, navigation paginée interactive, filtres, ouverture locale sûre, annotations et décisions persistées séparément des sources ; aucune ressource active exécutée.

9. **Rapports de comparaison et synthèses MSE** — partiellement réalisé.
   - Réalisé : rapport HTML autonome avec échappement, métadonnées MSE, dépendances/références, compteurs d'empreintes calculées/non calculées, ventilation des motifs de refus et avertissement d'indexation partielle. Validé sur HEAD `f8ee2fec023c8d529023155e779934449c9812cb` : CI Unix run `43886491085` (78/78 tests, aucun ignoré) et Windows run `43886491171` (76/76 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact Windows ID `14234637141` téléversé (5 321 748 octets, SHA-256 archive `75283597009f9e39ba87b801933dba946fb8f930ce020db074c66b30785114c6`).
   - Acceptation restante : filtres de rapport, synthèse des packages/dépendances/références plus exploitable et export JSON versionné.

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
- [~] Énumération récursive et lectures de contenu capability-based ; SVG et polices plafonnés en flux (32/64 Mio). `analyze_file` vérifie désormais taille, horodatage et type de fichier avant/après les analyseurs, et invalide le scan si ces métadonnées changent. Après l'indexation des ressources ZIP MSE, les archives sont re-hachées et le scan est invalidé si le contenu diffère du SHA initial. Limites restantes : mutations conservant les mêmes métadonnées entre les passes de lecture, remplacement transitoire suivi d'une restauration avant le contrôle final, sélection initiale des dossiers, remplacement de répertoires et courses de destinations. `analyze_file` réutilise maintenant un même handle ouvert sans suivi pour le hash et les analyseurs. Les planches utilisent désormais un tampon unique dont le SHA-256, les dimensions et les pixels sont cohérents.
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
- [x] Extrait de notice UTF-8 plafonné à 4 Kio, capturé pendant le calcul du SHA-256 et affiché avec échappement HTML ; contenus invalides non affichés, tests de contenu, troncature, binaire et correspondance hash/extrait validés sur Unix/Windows (HEAD `3249fbc6d17012c03cee25bda62aefd0fe30eece`, runs `43891292770`/`43891292737`).
- [x] Métadonnées raster : format, dimensions, type de canaux et présence d'alpha depuis le décodeur sans décodage des pixels.
- [x] Métadonnées SVG initiales : XML bien formé, viewBox et détection indicative de références externes.
- [x] Métadonnées de polices : famille et sous-famille lorsque lisibles par ttf-parser.
- [x] Inspection ZIP sans extraction : nombre d'entrées, taille déclarée, chemins suspects et ratios anormaux ; fichier limité à 512 Mio.
- [x] Génération de planches raster PNG paginées (16 éléments/page) avec index texte ; SHA vérifié avant aperçu.
- [x] Refus des exports dans l'arborescence source.
- [x] Vue filtrable des résultats locaux dans l'interface (500 premières lignes affichées, exports complets).
- [x] Annulation coopérative du scan local depuis l'interface, vérifiée à chaque entrée de répertoire et entrée énumérée, puis pendant l'analyse.
- [x] Progression locale en octets, agrégée à partir des tailles des fichiers et mise à jour par lots de 100 fichiers.
- [~] Estimation du temps restant : ETA indicative à partir de la vitesse moyenne observée, masquée avant 2 secondes d'échantillonnage ou lorsque le calcul n'est pas fiable ; tests unitaires validés sur HEAD `5d680835f02f951a7816f300befd03579fdf04ee` : Unix run `43870711246` (77 tests réussis, aucun ignoré) et Windows run `43870711092` (75 tests réussis, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact Windows validés. Restent les benchmarks sur corpus synthétiques et la vérification de la réactivité UI.

## Phase 3 — Analyse des formats
- [~] Raster : format, dimensions, canaux et alpha ; extraction d'orientation et affichage HTML ajoutés avec fixture JPEG EXIF, validés sur Unix/Windows (HEAD `7737b10`, runs `44056083010`/`44056083096`) ; limites, décodage contrôlé et miniatures à faire.
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
- [~] Planche de contact raster paginée avec index ; orientation EXIF appliquée au rendu quand elle est fournie par le décodeur. Test JPEG et validation CI Unix/Windows réussis sur HEAD `06b815c5ab423ba92016356ff8c366e4f1cfb12d` (80/78 tests, aucun ignoré).
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


### Balayage transversal du cycle — 17 novembre 2026

- **Sécurité/concurrence :** l'identité physique des fichiers et répertoires est comparée via device/volume + inode/file ID. L'ouverture récursive revalide le nom contre le handle, le scan simple vérifie la racine en fin de parcours et les exports vérifient leur parent avant/après écriture. La CI Unix et Windows du HEAD `3e42cf7` est verte. Restent les tests Windows de remplacement par répertoire ordinaire et l'analyse de la sélection initiale de racine.
- **ZIP/MSE :** le lecteur ZIP peut masquer une entrée centrale dupliquée avant l'analyse du fichier principal. Le compte brut EOCD/ZIP64 est maintenant comparé au nombre exposé avant la lecture de l'en-tête ; en cas d'écart, l'en-tête n'est pas interprété et le statut du fichier principal reste indéterminé. Validation CI à relancer après correction d'une assertion de test CRC indépendante.
- **Métadonnées raster/SVG/polices :** aucune modification indépendante sûre identifiée dans ce cycle sans élargir les budgets/contrats de décodage ; conserver comme prochaine revue dédiée.
- **Rapports, galerie, performance et packaging :** les chantiers restent indépendants mais exigent respectivement une conception de filtres, une persistance de revue, un corpus de benchmark et un protocole de livraison ; pas de modification artificielle dans ce commit.
- **CI de référence avant ce changement :** HEAD `2142cadf5a68aea3475f8774bcd0734e4d5ca6f5`, Unix run `44094582257` et Windows run `44094582102`, tous deux réussis.


### Balayage transversal du cycle — 18 novembre 2026

- **Sécurité/concurrence :** la CI du correctif MSE/CRC est verte sur Unix et Windows. Nouveau chantier : cap-fs-ext 4.0.3 expose MetadataExt::{dev, ino} sur Unix et Windows (volume serial + file index sur Windows). La comparaison d'identité physique des fichiers est implémentée avec un test de remplacement de même taille et mtime restauré ; validation CI en attente. Le contrôle d'identité des répertoires et les courses sur parents de destination restent à traiter séparément.
- **ZIP/MSE :** correction CRC validée sur les deux plateformes ; ambiguïté de table centrale reste distincte d'une corruption de données de ressource. Les flags de chiffrement historique (0x0001), chiffrement fort (0x0040) et en-têtes masqués (0x2000) sont désormais refusés dans chaque entrée centrale ; test dédié validé sur Unix et Windows (HEAD `3e62239`).
- **Métadonnées, rapports, galerie, performance, packaging et connectivité :** aucun changement autonome sûr n'est intégré dans ce cycle ; les critères et prochains travaux restent ceux des domaines ci-dessus.


### Sécurité des répertoires — 18 novembre 2026

- [x] Comparer l'identité physique des répertoires avant ouverture, sur le handle ouvert et après ouverture (volume/device + file ID/inode), dans l'ouverture absolue et l'ouverture des sous-répertoires.
- [x] Revalider le lien nom/handle après l'énumération récursive d'un sous-répertoire pour détecter son remplacement pendant le scan.
- [x] Ajouter des tests Unix de remplacement d'un répertoire ordinaire à nom constant et de refus d'un composant symbolique lors de l'ouverture absolue.
- [ ] Valider formatage, Clippy, tests et build Release sur Windows et Unix ; compléter les tests de remplacement de racine source et de parent d'export. La validation CI du commit précédent ne couvre pas ce nouveau changement.


### Sécurité des répertoires — suivi après CI du 18 novembre 2026

- [x] La CI Unix run `44260601762` et Windows run `44260601711` valide le contrôle d'identité des répertoires avant/après ouverture et après énumération sur HEAD `f50247aa633b9cc78ea78954f05847192a08d17f`.
- [x] Vérifier à la fin du scan que le chemin de la racine source désigne toujours le même répertoire que le handle épinglé.
- [x] Vérifier le parent d'export contre son handle avant écriture et après synchronisation ; supprimer le fichier partiel depuis le handle épinglé en cas de divergence.
- [x] Ajouter des tests Unix déterministes de remplacement persistant de la racine source et du parent d'export.
- [x] Validation complète sur HEAD `3e42cf75d586525a45e6449739459f1a823ac5e4` : CI Unix run `44263552864` (formatage, Clippy et tests réussis) et CI Windows run `44263553046` (formatage, Clippy, tests, build Release x64 et upload de l'exécutable réussis).
- [x] Le contrôle de cohérence de la racine source et du parent d'export est couvert par deux tests Unix déterministes ; l'export supprime le fichier partiel via le handle épinglé en cas de divergence.
- [ ] Étendre si possible les tests de remplacement de répertoire aux runners Windows sans dépendre de la création de liens symboliques ; étudier la sélection initiale des racines, qui reste fondée sur des chemins ambiants.
- [ ] Reprendre le balayage transversal : ZIP/MSE (flags/offsets/encodages), métadonnées raster, rapports/filtres, galerie/revue persistante, benchmarks et packaging autonome.


### Validation sécurité/concurrence — 18 novembre 2026 (HEAD `3e42cf7`)

- CI Unix `44263552864` : Format check, Clippy et tests réussis.
- CI Windows `44263553046` : Format check, Clippy, tests, build Release x64 et upload de l'exécutable réussis.
- Les contrôles d'identité des répertoires et la vérification de cohérence du chemin avec le handle sont validés sur les deux plateformes pour les étapes exécutées par ces workflows.
- Limite : les tests de remplacement par répertoire ordinaire de la racine et du parent d'export sont déterministes sur Unix ; cette validation ne prouve pas qu'un scénario équivalent a été exécuté sur Windows. Les substitutions transitoires restaurées entre contrôles restent hors garantie.


### Validation de la structure EOCD et ZIP64 — 18 novembre 2026

- [x] Valider les candidates EOCD en exigeant que leur commentaire se termine exactement à EOF ; une signature EOCD contenue dans un commentaire ne doit pas supplanter l'enregistrement réel.
- [x] Valider les bornes de la table centrale classique et ZIP64 : comptes, tailles, offsets, signatures, longueurs des en-têtes, numéros de disque et extension numérique centrale éventuelle.
- [x] Refuser l'interprétation MSE lorsque le compte brut de la table centrale est absent ou différent du nombre d'entrées exposées par le lecteur ZIP.
- [x] Ajouter une régression pour une fausse signature EOCD dans le commentaire (HEAD `c19d35e1759a9c92a09bc388edc74c0a0f9df0eb`).
- [x] Ajouter une régression pour une fausse candidate ZIP64 dans le commentaire, avec offset impossible ; ignorer cette candidate et poursuivre l'examen d'un EOCD antérieur valide (HEAD `658d06bfcd406210d44d4546cae3ade5a5f81d23`).
- [x] CI Unix `44326068359` : formatage, Clippy et suite de tests réussis.
- [x] CI Windows `44326068305` : formatage, Clippy, tests, build Release x64 et upload de l'exécutable réussis.
- [x] Ajouter des tests directs de rejet d'une signature d'en-tête central corrompue et d'une entrée déclarant un disque non nul ; validés sur HEAD `f747e6966fa54a43d2ed90b0df3d14d2e4b8d33d`.
- [x] Ajouter des tests directs de rejet d'une taille centrale inférieure d'un octet et d'un compte EOCD supérieur au nombre d'en-têtes centraux ; validés sur HEAD `8e3b2ed5ff278e1e77b8b3c74ffac8deac420d76`.
- [x] CI Unix run `44331127526` : formatage, Clippy et tests réussis.
- [x] CI Windows run `44331127504` : formatage, Clippy, tests, build Release x64 et upload de l'exécutable réussis.
- [ ] Poursuivre les fixtures adversariales sur les flags ZIP, offsets ZIP64/SFX et incohérences de tailles locales/centrales ; tester les encodages de noms non UTF-8, les chemins ambigus et les doublons du fichier principal.
- [ ] Faire le balayage transversal des domaines raster/SVG, rapports et filtres, galerie/revue persistante, performance et packaging autonome avant de choisir la prochaine tranche de code.


### Validation des flags de chiffrement ZIP — 18 novembre 2026

- [x] Lire les general purpose bit flags de chaque en-tête central et refuser les entrées marquées par le chiffrement traditionnel `0x0001`, le chiffrement fort `0x0040` ou les en-têtes masqués `0x2000`.
- [x] Ajouter une régression qui active séparément chacun des trois flags et exige le rejet du compteur brut de table centrale.
- [x] Validation sur HEAD `3e62239ce521d38acd0c842c8562366e7a0bbf89` : CI Unix run `44335421823` (formatage, Clippy et tests réussis) et Windows run `44335420030` (formatage, Clippy, tests, build Release x64 et upload de l'exécutable réussis).
- [ ] Poursuivre avec la cohérence en-têtes locaux/centraux, offsets ZIP64/SFX et encodages/chemins non UTF-8 ; garder le rejet fail-closed et aucune extraction.


### Validation de la cohérence des en-têtes ZIP locaux/centraux — 18 novembre 2026

- [x] Vérifier, pour chaque entrée centrale, la présence d'un en-tête local avant la table centrale et comparer signature, flags, méthode de compression et nom brut.
- [x] Vérifier que le début des données locales, après l'en-tête fixe, le nom et le champ extra, reste avant la table centrale.
- [x] Appliquer l'ajustement d'offset des archives SFX classiques ; refuser explicitement l'offset local ZIP64 par entrée (`0xffffffff`) tant que son champ extra n'est pas résolu.
- [x] Préserver les diagnostics de ressources : ne pas transformer un CRC/une taille incohérente ou des données tronquées en rejet structurel global ; le lecteur doit pouvoir signaler `read-error` ou `size-mismatch`.
- [x] Tests adversariaux de noms locaux/centraux divergents et méthodes divergentes ; tests existants de CRC invalide, tailles incohérentes et données tronquées conservés.
- [x] Validation complète sur HEAD `84171296dbbf935cf116ac9c62a7404ea9196661` : Unix run `44341054195` (formatage, Clippy, 104/104 tests) et Windows run `44341054213` (formatage, Clippy, tests, build Release x64 et upload de l'exécutable).
- [ ] Compléter ZIP64 par entrée (résolution des offsets et tailles depuis les champs extra), variantes SFX plus complexes, noms non UTF-8, chemins ambigus et doublons du fichier principal MSE.
- [ ] Balayage transversal suivant : limites raster et décodage, couverture CSS SVG/polices, filtres et staging des rapports, galerie/revue persistante, benchmarks et autonomie du packaging.


### Régression SFX ZIP préfixée — 18 novembre 2026

- [x] Ajouter une fixture ZIP avec deux entrées et un stub SFX préfixé, en conservant les offsets ZIP classiques relatifs au début de l'archive.
- [x] Vérifier que le compteur brut applique l'ajustement SFX et lit correctement les deux entrées sans perdre sa position dans la table centrale.
- [x] Validation sur HEAD `817ab7811315bfec6ec496cc2474a2bb5c3232a1` : Unix run `44342345771` (formatage, Clippy, 105/105 tests) et Windows run `44342345766` (formatage, Clippy, tests, build Release x64 et upload de l'exécutable).
- [ ] Couvrir les variantes SFX plus complexes, ZIP64 par entrée, noms non UTF-8, chemins ambigus et doublons du fichier principal MSE.

### Balayage transversal après validation SFX — 18 novembre 2026

- **Sécurité/concurrence :** les contrôles de racine, répertoires, fichiers et parents d'export sont documentés ; les scénarios Windows de remplacement par répertoire ordinaire et les substitutions transitoires restaurées restent des limites.
- **ZIP/MSE :** EOCD, table centrale, flags de chiffrement, rapprochement local/central et SFX classique préfixé sont couverts. Prochaine tranche ciblée : offsets/tailles ZIP64 par entrée, puis noms non UTF-8 et ambiguïtés de chemins.
- **Raster :** l'aperçu est limité à 100 Mio par fichier et 30 Mpx avant décodage ; le contrôle suivant doit vérifier que les limites du décodeur sont effectivement imposées lors du rendu, et ajouter une fixture de dimensions extrêmes.
- **SVG/polices :** SVG XML est borné et ne charge aucune référence ; couverture CSS complète, aperçu sandboxé et formats de polices supplémentaires restent à concevoir.
- **Rapports/staging :** filtres et synthèses restent à définir ; staging séparé de LIBRARY non implémenté.
- **Galerie/revue :** galerie HTML, annotations et décisions persistantes restent à concevoir.
- **Performance :** benchmarks sur corpus synthétique et réactivité UI restent à mesurer.
- **Packaging :** le build Release x64 et l'artefact CI existent ; autonomie sur Windows propre, versionnement, GitHub Release reproductible et signature restent à valider.
- **Décision :** ne pas modifier ces domaines sans critères/test précis ; poursuivre d'abord les constructions ZIP64 par entrée, qui prolongent directement le contrôle de structure actuellement testé.


### Validation ZIP64 par entrée — 18 novembre 2026 (HEAD `28bfa861`)

- [x] Résoudre l'offset d'en-tête local ZIP64 depuis le champ extra central `0x0001`, en sautant les valeurs de taille décompressée et compressée lorsqu'elles sont signalées par les sentinelles 32 bits.
- [x] Valider la présence d'un champ extra ZIP64 unique et sa longueur exacte lorsque les tailles centrales ou l'offset local utilisent une sentinelle ; rejeter les champs tronqués, dupliqués ou de longueur incohérente.
- [x] Ajouter une fixture ZIP64 cohérente avec tailles locales et centrales étendues, sentinelles 32 bits et longueurs de champs extra exactes (20 octets au total pour l'en-tête extra et les deux valeurs 64 bits).
- [x] Validation sur HEAD `28bfa8615a304c94a04dcf0214bde60b443a258d` : CI Unix run `44390560588` (formatage, Clippy et tests réussis) et CI Windows run `44390562401` (formatage, Clippy, tests, build Release x64 et upload de l'exécutable réussis).
- [ ] Étendre les fixtures ZIP64 aux combinaisons de sentinelles de tailles et d'offset, aux valeurs 64 bits réellement élevées, aux offsets SFX + ZIP64 et aux incohérences entre valeurs étendues locales/centrales. Le parseur valide actuellement la structure/longueur des champs extra et utilise l'offset local étendu ; il ne faut pas assimiler cela à une validation exhaustive de toutes les valeurs ZIP64.
- [ ] Poursuivre les noms non UTF-8, les chemins ambigus et les doublons du fichier principal MSE ; aucun chemin ne doit être extrait ni normalisé de façon à masquer une collision.

### Balayage transversal — 18 novembre 2026 (après ZIP64 par entrée)

- **Sécurité/concurrence :** contrôles de racines et parents d'export présents ; les scénarios de remplacement persistent sur Windows et les substitutions transitoires restaurées restent des limites.
- **ZIP/MSE :** EOCD, bornes centrales, flags de chiffrement, rapprochement local/central, SFX classique et un sous-ensemble ZIP64 par entrée sont couverts. Priorités restantes : matrice ZIP64 complète, noms encodés non UTF-8, chemins ambigus et collisions de nom du fichier principal.
- **Raster :** inventaire lit les dimensions via le décodeur sans décoder les pixels ; planche contact limite les fichiers à 100 Mio et 30 Mpx avant décodage. Le décodeur de la planche contact n'impose pas encore explicitement ses propres limites `image::Limits`. Ajouter largeur/hauteur/allocation bornées au décodeur et tests dimensions extrêmes.
- **SVG/polices :** analyse XML bornée et références externes refusées ; CSS complet, sandbox de rendu et formats de polices supplémentaires à concevoir.
- **Rapports/staging :** filtres, synthèses et espace de staging séparé de LIBRARY à spécifier.
- **Galerie/revue :** galerie HTML, annotations et décisions persistantes à concevoir.
- **Performance :** benchmarks de corpus synthétique et mesure de réactivité UI à établir.
- **Packaging :** CI Windows construit et publie un exécutable Release x64 ; autonomie sur machine propre, versionnement, release reproductible et signature restent ouverts.
- **Avancement global :** conserver l'estimation à 46 % ; la validation ZIP64 ferme une tranche ciblée mais ne change pas encore les critères d'achèvement des grands domaines.


### Limites du décodage raster en planche contact — 18 novembre 2026

- [x] Conserver les contrôles préalables de taille fichier (100 Mio) et de dimensions/aire (30 Mpx) avant décodage.
- [x] Configurer explicitement `image::Limits` sur le lecteur utilisé pour le rendu de planche contact : largeur et hauteur maximales à 30 000 000 pixels chacune et budget d'allocation demandé de 256 Mio.
- [x] Ajouter un test de régression BMP à en-tête valide déclarant une largeur de 30 000 001 pixels ; le décodeur doit rejeter l'image avant allocation des pixels.
- [x] Validation sur HEAD `b2b8988f3bf3ee50833f35bdc9832d0999c71d5b` : CI Unix run `44391633791` et CI Windows run `44391633781` ont passé formatage, Clippy et tests ; Windows a également passé le build Release et l'upload de l'exécutable.
- [ ] Ajouter des tests de dimensions extrêmes pour plusieurs codecs réellement activés, notamment hauteur excessive, dimensions nulles, débordements d'aire et formats dont le décodeur ne respecte pas le budget d'allocation.
- [ ] Ne pas considérer `max_alloc` comme une garantie stricte : la documentation de la crate indique que certains décodeurs peuvent ignorer cette limite. Les contrôles d'entrée du projet et les limites de dimensions restent la première barrière.

### Prochaine tranche — noms ZIP et chemins d'entrée

- [x] Vérifier sur la source exacte de `zip 2.6.1` l'existence de `ZipFile::name_raw()` et `ZipFile::extra_data()` ; limite établie : `name_raw()` expose les octets après traitement éventuel de `0x7075`, donc ne suffit pas à conserver systématiquement le champ filename central original.
- [ ] Définir une politique explicite pour les noms UTF-8 invalides, les noms marqués UTF-8 mais invalides, les séparateurs `/` et `\\`, les segments `.` / `..`, les chemins absolus, les préfixes de lecteur et les noms en collision après normalisation.
- [ ] Détecter les doublons exacts et les collisions après normalisation sans extraire les entrées ; conserver les octets originaux pour les diagnostics.
- [ ] Ajouter des fixtures adversariales et vérifier que les noms ambigus ne peuvent ni masquer des ressources ni provoquer de divergence entre plateformes.

### Précisions sur le décodage des noms ZIP — 18 novembre 2026

- La crate verrouillée est `zip 2.6.1`. L'API de nom brut doit être testée sur cette version précise, pas déduite de la documentation d'une version ultérieure.
- Le champ brut `name_raw` est le candidat pour conserver les octets du nom d'entrée ; le nom textuel peut suivre les règles d'encodage ZIP (bit EFS/UTF-8, sinon CP437) et les informations de l'extra field Info-ZIP Unicode Path `0x7075`.
- Ne pas traiter tous les octets non UTF-8 comme invalides : CP437 est une représentation historique valide lorsque le bit EFS n'est pas positionné. En revanche, un nom déclaré UTF-8 par EFS et non UTF-8 doit être signalé.
- Vérifier les extras `0x7075` (version, CRC32 du nom brut, UTF-8 valide), leur présence locale et centrale, leur cohérence et les divergences entre nom brut et nom effectif. Le CRC invalide signifie que le champ Unicode Path ne doit pas remplacer le nom principal selon la spécification.
- Les tests de collision doivent couvrir à la fois les octets bruts et les noms effectifs après décodage ; ajouter les cas Unicode Path qui renomme une entrée, les noms CP437 et les noms EFS invalides avant d'utiliser `name_raw` comme unique source de vérité.


## Validation des noms ZIP ambigus sur Windows — 19 novembre 2026

Le parcours des ressources ZIP et le résumé des métadonnées détectent désormais les noms incompatibles ou ambigus sur Windows : chemins absolus, antislashs, deux-points, caractères interdits/de contrôle, segments vides intermédiaires ou `.`/`..`, noms terminés par espace/point et noms de périphériques réservés (dont variantes COM/LPT superscript). Une clé de collision insensible à la casse et normalisant les points/espaces terminaux repère aussi les entrées distinctes qui désignent le même nom côté Windows. Les entrées suspectes ne sont pas hachées ; elles sont comptabilisées comme chemins suspects. Les noms Unicode valides tels que `cartes/é.png` restent acceptés par ce contrôle.

Validation sur HEAD `c935ea9a8c3f6ff0781403c9fc16a5adb6190ff9` : CI Unix run `44513214541` (Format check, Clippy et tests réussis) et CI Windows run `44513214470` (Format check, Clippy, tests, build Release x64 et upload de l'exécutable réussis).

**Limite de portée :** cette tranche travaille sur le nom déjà décodé/exposé par la crate ZIP `2.6.1`. Elle ne valide pas encore les octets bruts du champ filename, la cohérence du bit EFS, le fallback CP437 ni l'extra field Info-ZIP Unicode Path `0x7075` (version, CRC32 et UTF-8). Ne pas présenter ce contrôle comme une validation complète de l'encodage des noms ZIP.


### Résultats de l'inspection de l'API zip 2.6.1 — 19 novembre 2026

La source taguée `v2.6.1` de `zip-rs/zip2` confirme les méthodes publiques `ZipFile::name_raw()` et `ZipFile::extra_data()`. Le lecteur central interprète le bit EFS (bit 11) avec `String::from_utf8_lossy`, et utilise CP437 lorsque EFS est absent. Il traite ensuite l'extra field `0x7075` : version lue puis ignorée par l'implémentation, CRC32 vérifié contre le champ brut courant, contenu décodé en UTF-8 et utilisé pour remplacer `file_name_raw` et `file_name`. Ainsi, `name_raw()` peut retourner le contenu Unicode Path, pas nécessairement les octets filename originaux.

Conséquence de sécurité : ne pas se reposer sur `name_raw()` pour auditer strictement EFS ou conserver le filename central original. La prochaine implémentation doit analyser les champs filename et extra depuis les octets de la table centrale et de l'en-tête local, vérifier strictement la version 1, le CRC32 et l'UTF-8 de `0x7075`, puis comparer les représentations. La bibliothèque masque actuellement les erreurs UTF-8 EFS via remplacement lossy ; ce comportement doit être contourné ou détecté par notre parseur brut.
