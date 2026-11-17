# Analyse des packages Magic Set Editor (MSE)

## Sources de format

La conception s'appuie sur la documentation et le code source de Magic Set Editor 2 :

- [Documentation du format des packages](https://github.com/twanvl/MagicSetEditor2/blob/master/doc/file/package.txt)
- [Documentation du format des fichiers hiérarchiques](https://github.com/twanvl/MagicSetEditor2/blob/master/doc/file/format.txt)
- [Documentation des sets](https://github.com/twanvl/MagicSetEditor2/blob/master/doc/type/set.txt)
- [Documentation des jeux](https://github.com/twanvl/MagicSetEditor2/blob/master/doc/type/game.txt)
- [Documentation des feuilles de style](https://github.com/twanvl/MagicSetEditor2/blob/master/doc/type/stylesheet.txt)
- [Documentation des dépendances](https://github.com/twanvl/MagicSetEditor2/blob/master/doc/type/dependency.txt)
- [Documentation des numéros de version](https://github.com/twanvl/MagicSetEditor2/blob/master/doc/type/version.txt)
- [Implémentation du conteneur Package](https://github.com/twanvl/MagicSetEditor2/blob/master/src/util/io/package.cpp)
- [Déclaration C++ de Package et Packaged](https://github.com/twanvl/MagicSetEditor2/blob/master/src/util/io/package.hpp)

Ces sources décrivent le format MSE 2 ; les variantes futures ou extensions tierces devront être traitées comme inconnues tant qu'elles ne sont pas documentées.

## Structure constatée

Un package MSE est stocké soit comme un dossier, soit comme une archive ZIP. Son extension indique son type (par exemple `.mse-set`, `.mse-game`, `.mse-style`). Il contient un fichier de données principal sans extension, dont le nom correspond au type du package : `set`, `game`, `style`, etc. Des ressources annexes, notamment des images, peuvent être présentes.

Le fichier de données est du texte UTF-8 hiérarchique. Les clés et valeurs sont séparées par `: `; chaque niveau imbriqué est représenté par exactement une tabulation supplémentaire. Les packages peuvent déclarer des dépendances via `depends on`. Un set référence notamment un jeu et une feuille de style, et peut contenir des cartes, mots-clés et paramètres de set.

Les fichiers hiérarchiques peuvent également déclarer `include file:`. Cette directive est une inclusion textuelle dans MSE ; l'auditeur ne doit jamais la résoudre automatiquement ni ouvrir un chemin arbitraire issu du contenu.

## État de l'implémentation

La première tranche est en place pour les conteneurs ZIP et dossiers : les extensions `.mse-set`, `.mse-game` et `.mse-style` sont reconnues, le type attendu est affiché et la présence du fichier principal à la racine (`set`, `game` ou `style`) est vérifiée dans la table centrale ZIP ou par l'analyse capability-based du fichier principal du dossier. Le préfixe du fichier principal est lu en flux borné à 1 Mio, sans extraction ; les lignes complètes permettent d'extraire `mse version`, `name`, `short name`, `full name`, `version`, `game` et `stylesheet`. Les directives `include file:` sont détectées mais jamais suivies. Les dépendances `depends on` sont extraites dans les syntaxes compacte et en bloc (`package`/`version`), et sont résolues uniquement contre les packages présents dans les sources locales analysées ; les correspondances absentes ou multiples sont signalées. Les versions minimales sont comparées lorsque les deux valeurs suivent les formes documentées (`a.b.c` ou `YYYY-MM-DD`) ; les formats inconnus sont signalés comme non vérifiables. Les cycles du graphe de dépendances local sont détectés par parcours itératif, sans récursion sur les données analysées. Aucune dépendance n'est téléchargée ni installée. La reconnaissance des packages sous forme de dossiers, la résolution locale des dépendances, la validation des versions minimales reconnues et la résolution locale des références de jeu (par nom déclaré) et de feuille de style (par jeu et nom court) sont en place ; les références absentes ou ambiguës sont signalées. Les cycles du graphe local sont détectés sans récursion. Les ressources des packages dossiers sont indexées par chemin relatif et reliées au SHA-256 déjà calculé pour chaque fichier, sans nouvelle lecture ni copie des contenus ; les sous-packages sont isolés de leur parent. Les ressources ZIP sûres sont maintenant hachées en flux ; la taille de chaque lecture est plafonnée avant l'appel au décompresseur selon le minimum des budgets restants par entrée, package et scan, afin qu'un bloc de 64 Kio ne dépasse pas les limites ; les plafonds sont de 64 Mio par entrée, 256 Mio par package, 512 Mio par scan et 10 000 entrées ; le passage séquentiel après l'analyse parallèle rend l'allocation du budget reproductible, y compris entre les sources multiples ; les chemins suspects, ratios anormaux et ressources hors budget sont listés sans empreinte et sans extraction. Les plafonds de hachage sont testés avec des budgets réduits afin de vérifier les statuts sans créer de gros fichiers de test ; une archive ZIP MSE corrompue est également signalée sans extraction ni tentative d'indexation des ressources ; un test dédié corrompt les octets d'une ressource stockée sans modifier le CRC déclaré et vérifie le statut `read-error` sans SHA-256 ; un second altère le flux Deflate d'une ressource compressée en conservant la table centrale et vérifie le même refus sans empreinte. Une fixture distincte tronque la fin de la table centrale ZIP et vérifie que le scan produit un avertissement contrôlé sans indexer de ressources. Ce test a été exécuté avec succès sur le HEAD `006aef27e2cc9063aa06721ebe8f2e52b54a1ffd` : CI Unix run `43742700037` (68 tests) et Windows run `43742700103` (66 tests), avec formatage et Clippy ; Windows a également compilé le binaire Release et téléversé l'artefact. L'annulation du scan est contrôlée entre les entrées ZIP et pendant la lecture en flux des ressources ; elle interrompt le scan complet plutôt que de retourner un inventaire partiel. Le rapport HTML autonome expose les métadonnées MSE, dont les noms court/complet, versions MSE/package et nom du jeu, les dépendances et références résolues, les ressources et leur statut d'empreinte, avec échappement des valeurs non fiables. Le test de rapport vérifie explicitement l'affichage du nom du jeu, en plus du nom complet échappé et de la version du package. Le rapport ajoute désormais un résumé des empreintes calculées/non calculées, ventile les statuts de refus (`read-error`, `size-limit`, `suspicious-path`, etc.) et signale explicitement une indexation partielle lorsque la collecte des ressources est tronquée. Le test HTML d'intégration vérifie le compteur affiché. Validé sur HEAD `f8ee2fec023c8d529023155e779934449c9812cb` : CI Unix run `43886491085` (78 tests réussis, 0 ignoré) et Windows run `43886491171` (76 tests réussis, 0 ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14234637141` téléversé. Restent à faire : la lecture au-delà du préfixe d'en-tête borné et les tests de corruption avancés.

## Objectif fonctionnel

L'auditeur doit pouvoir :

1. reconnaître les packages MSE ZIP et les packages MSE sous forme de dossiers ;
2. identifier le type du package depuis son extension et vérifier la présence du fichier de données principal attendu ;
3. lire un en-tête borné et extraire les métadonnées descriptives disponibles (version MSE, nom court/long, version du package) ;
4. relever les dépendances déclarées, leurs versions minimales et les références d'un set vers son jeu et sa feuille de style ;
5. inventorier les ressources internes et relier leurs chemins aux empreintes déjà calculées par l'inventaire ;
6. signaler les références manquantes, chemins internes suspects, déclarations inconnues et dépendances non résolues localement, sans les corriger.

## Limites de sécurité obligatoires

- Aucun package ZIP n'est extrait sur disque.
- Les entrées ZIP sont inspectées depuis la table centrale ; les noms de chemins sont validés et les tailles déclarées plafonnées.
- Le fichier principal est lu depuis un handle capability-based pour les packages dossiers, ou depuis l'entrée ZIP attendue pour les packages archivés.
- La taille maximale du fichier principal, la taille maximale d'une ligne, le nombre de lignes et le nombre d'entrées doivent être bornés avant toute allocation importante.
- Le décodage UTF-8 est strict ; les espaces en début de ligne et les indentations mélangeant espaces/tabulations sont rejetés. Les blocs inconnus correctement tabulés restent opaques et ne sont pas interprétés comme des instructions.
- Les scripts MSE, scripts de rendu, templates d'export, macros et contenus HTML/CSS restent des données opaques : aucune exécution, rendu ou chargement externe.
- Les directives `include file:` sont détectées et rapportées, mais ne sont jamais suivies.
- Les dépendances sont comparées uniquement aux packages explicitement présents dans l'inventaire ; aucune résolution réseau ni installation automatique.
- Les ressources ne sont jamais modifiées, renommées, déplacées ou extraites.

## Découpage d'implémentation

### Étape A — reconnaissance des conteneurs

- Détecter les fichiers dont l'extension est un type de package MSE connu.
- Déterminer si le package est un ZIP valide ; relever le fichier principal attendu, les chemins, tailles et entrées suspectes.
- Détecter les dossiers dont le nom se termine par une extension de package MSE, sans confondre automatiquement chaque dossier ordinaire avec un package.
- Définir les limites d'entrées et de profondeur pour les packages dossiers.

### Étape B — lecture des en-têtes

- Lire uniquement le fichier principal attendu, avec des limites strictes en octets et en lignes.
- Parser les clés de premier niveau et les blocs de dépendances sans évaluer les valeurs scriptables.
- Distinguer métadonnées absentes, mal formées, inconnues et valides.

### Étape C — graphe de dépendances

- Relier les packages par nom et version déclarés.
- Résoudre localement les références jeu/stylesheet d'un set si les packages correspondants sont présents dans l'inventaire.
- Signaler les dépendances absentes, versions insuffisantes, références ambiguës et cycles, sans installation ni modification.

### Étape D — intégration à l'inventaire

- Ajouter un modèle de métadonnées MSE versionné et rétrocompatible.
- Conserver les SHA-256 et chemins des ressources ; ne pas dupliquer ni extraire les blobs.
- Ajouter des fixtures synthétiques et des tests de non-extraction, traversal, ZIP bomb, UTF-8 invalide, en-tête surdimensionné, tabulations invalides, includes et dépendances cycliques.

## Couverture de corruption ZIP vérifiée

Les fixtures de non-régression couvrent désormais plusieurs défaillances distinctes : archive MSE globalement corrompue, CRC de ressource incohérent, flux Deflate altéré, fin de table centrale tronquée, offset de table centrale invalide (`u32::MAX`) et troncatures physiques des données locales d'entrées Stored et Deflate. Les fixtures retirent quatre octets de la charge locale tout en ajustant l'offset de la table centrale ; le ZIP reste lisible au niveau de la table centrale et le hachage de la ressource échoue en `read-error`, sans SHA-256 ni extraction. Validé sur HEAD `c62df0a5e1841110bf46507aa54e3b410c1c6135` : Unix run `44089439239` (87/87 tests, aucun ignoré) et Windows run `44089439268` (84/84 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14330693012` (5 360 216 octets) téléversé. Pour les deux défauts de table centrale, le comportement attendu est un avertissement contrôlé et l'absence d'indexation des ressources ; aucune extraction n'est effectuée. Le test d'offset invalide a été validé sur le HEAD `2c19e229a9ba4d9cfed77862b49986d685789ff7`, puis revalidé avec le HEAD documentaire `069c34e6fd9501eb3689820e3b41cb0e2cb11394` : CI Unix run `43828091330` (70 tests réussis, aucun ignoré) et CI Windows run `43828091369` (68 tests réussis, aucun ignoré, build Release x64 et artefact téléversé).

Les doublons de chemins de ressources internes sont désormais détectés de manière conservative : le nombre brut d'entrées de la table centrale est comparé au nombre d'entrées exposées par le lecteur ZIP. Si des entrées sont masquées par déduplication, toutes les ressources visibles sont marquées `ambiguous-path` et aucun SHA-256 n'est conservé ; un suivi par nom couvre aussi les lecteurs qui exposeraient les doublons séparément. Restent les autres incohérences de tailles et flags, les noms encodés non UTF-8, les chemins ambigus non dupliqués et les doublons du fichier principal. La troncature des données locales est maintenant couverte et validée sur Unix et Windows ; les autres tailles/flags, encodages non UTF-8 et chemins ambigus restent à couvrir.

## Hors périmètre

Le rendu des cartes, l'exécution des scripts MSE, l'installation de packages, la réparation automatique, la modification des sets et la résolution de dépendances sur Internet ne font pas partie de l'audit initial.

Une fixture complémentaire altère uniquement la taille décompressée déclarée dans l'entrée centrale ZIP (valeur supérieure d'un octet au contenu réel). Le scan marque explicitement la ressource `size-mismatch` et omet son SHA-256. Validation sur le HEAD `818e4f4e43b66f118d012c296488badbc85f1625` : CI Unix run `43831180611` (formatage, Clippy, 71 tests réussis, 0 ignoré) et Windows run `43831180639` (formatage, Clippy, 69 tests réussis, 0 ignoré, build Release x64 et upload de l'artefact `14207669916`). Les autres tailles incohérentes et les données locales tronquées restent à tester.

Validation des fixtures de taille et d'encodage sur le HEAD `1385b136395618c6d1a785bf418fada68872b416` : la taille décompressée déclarée inférieure d'un octet au contenu réel est signalée `size-mismatch` sans SHA-256 ; le nom UTF-8 `cards/café-🃏.bin` est conservé et haché. CI Unix run `43833656545` : 73 tests réussis ; CI Windows run `43833656301` : 71 tests réussis, build Release x64 et artefact `14209592138`. Formatage et Clippy réussis sur les deux plateformes, aucun test ignoré. Les encodages ZIP non UTF-8 et les autres anomalies de structure restent à traiter.


## Validation des chemins ZIP dupliqués

Validé sur HEAD `9eb6aef0ed9d739e7f64e25dbd341926f44a9c8a` : une fixture ZIP valide contient deux entrées ayant le même chemin interne. Le lecteur ZIP ne les expose que comme deux entrées uniques (dont le fichier principal), alors que le nombre brut de la table centrale EOCD est trois. Le scanner détecte cette discordance et marque les ressources visibles `ambiguous-path`, sans conserver de SHA-256 ni extraire de fichier. CI Unix run `44094150470` (90/90 tests, aucun ignoré) et Windows run `44094150510` (87/87 tests, aucun ignoré), formatage et Clippy réussis ; build Release x64 et artefact `14334060248` (5 363 890 octets).


## Fichier principal MSE et entrées ZIP masquées

Le compte brut EOCD/ZIP64 est maintenant vérifié avant l'interprétation du fichier principal. Si le lecteur ZIP expose moins d'entrées que la table centrale n'en déclare, le statut du fichier principal devient indéterminé, son en-tête n'est pas interprété et un avertissement explicite est produit. Le test des chemins de ressources dupliqués vérifie aussi que le fichier principal n'est pas lu dans cette situation. Les autres incohérences de tailles, flags et offsets restent à couvrir.


## Distinction corruption de ressource et ambiguïté de table centrale

Une corruption CRC limitée aux octets d'une ressource n'implique pas que la table centrale ZIP soit ambiguë. Si le nombre brut d'entrées EOCD/ZIP64 correspond au nombre exposé par le lecteur, le fichier principal MSE reste interprétable ; la ressource corrompue est ensuite marquée `read-error` lors de sa lecture et n'obtient aucun SHA-256. Le statut du fichier principal devient indéterminé uniquement lorsque le compte brut et le compte exposé divergent (cas d'entrées centrales masquées).
