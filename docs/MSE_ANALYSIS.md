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


## Validation des enregistrements EOCD et de la table centrale ZIP/ZIP64 — 18 novembre 2026

Avant de lire ou d'interpréter le fichier principal MSE d'une archive, l'auditeur recherche un EOCD dont le commentaire se termine exactement à EOF. Il valide ensuite les bornes et la structure de la table centrale, y compris les en-têtes de chaque entrée, leurs champs de longueur variable et les informations de disque. La variante ZIP64 vérifie le locator, l'enregistrement ZIP64, la cohérence des comptes et les bornes de la table centrale. Les archives multi-disques sont refusées.

Les signatures EOCD présentes dans les commentaires sont traitées comme des candidates et non comme des preuves d'EOCD. Une candidate ZIP64 malformée (par exemple un offset hors fichier) est écartée afin de poursuivre la recherche d'une candidate antérieure. Deux tests couvrent respectivement un faux EOCD et un faux locator/EOCD ZIP64 dans le commentaire. La seconde fixture vérifie que le compte brut de l'archive valide est retrouvé malgré la candidate ZIP64 impossible.

Validation finale sur HEAD `658d06bfcd406210d44d4546cae3ade5a5f81d23` : CI Unix run `44326068359` (formatage, Clippy et tests réussis) et CI Windows run `44326068305` (formatage, Clippy, tests, build Release x64 et upload de l'exécutable réussis). Aucun résultat de cette validation ne justifie d'élargir le périmètre d'interprétation : les ressources restent non extraites et les scripts MSE non exécutés.

Restent à tester : combinaisons de flags, tailles et offsets incohérents, variantes ZIP64/SFX, noms non UTF-8, chemins ambigus non dupliqués et doublons du fichier principal.


## Régressions structurelles EOCD et en-têtes centraux — 18 novembre 2026

Quatre cas adversariaux supplémentaires sont validés au niveau du compteur brut ZIP : signature d'en-tête central altérée, entrée centrale déclarant un disque non nul, taille centrale annoncée trop courte d'un octet et nombre d'entrées EOCD supérieur au nombre d'en-têtes réellement présents. Le résultat attendu est le rejet structurel (`None`) ; le scanner MSE ne doit donc pas considérer ce compte comme une preuve de cohérence.

Validation sur HEAD `8e3b2ed5ff278e1e77b8b3c74ffac8deac420d76` : CI Unix run `44331127526` et Windows run `44331127504` réussies. Les workflows ont passé formatage, Clippy et tests ; Windows a aussi passé le build Release x64 et l'upload de l'exécutable. Les flags ZIP, les discordances entre en-têtes locaux et centraux, les encodages non UTF-8, les chemins ambigus non dupliqués et les cas SFX/ZIP64 demeurent à couvrir.


## Rejet des flags de chiffrement ZIP — 18 novembre 2026

La validation de chaque en-tête central refuse désormais trois indicateurs du general purpose bit flag : chiffrement traditionnel (`0x0001`), chiffrement fort (`0x0040`) et en-têtes masqués (`0x2000`). Le compteur brut de table centrale retourne une structure invalide si l'un de ces bits est activé ; le statut MSE ne doit alors pas être interprété à partir de cette archive. Aucun déchiffrement ni extraction n'est tenté.

Le test `rejects_encrypted_zip_central_directory_entries` vérifie séparément les trois indicateurs sur une archive de fixture. Validation sur HEAD `3e62239ce521d38acd0c842c8562366e7a0bbf89` : CI Unix run `44335421823` et Windows run `44335420030` réussies ; formatage, Clippy et tests ont passé sur les deux plateformes, puis build Release x64 et upload de l'exécutable ont passé sur Windows. Les contrôles des en-têtes locaux, des offsets ZIP64/SFX et des noms non UTF-8 restent ouverts.


## Cohérence des en-têtes locaux et centraux ZIP — 18 novembre 2026

Avant l'interprétation des ressources MSE, le compteur brut rapproche chaque entrée de sa référence locale : signature d'en-tête, flags généraux, méthode de compression et nom brut doivent correspondre. L'offset local classique est ajusté pour prendre en compte un préfixe SFX ; le début des données, après l'en-tête fixe, le nom et le champ extra local, doit précéder la table centrale.

Le contrôle ne rejette pas une archive uniquement parce que le CRC ou les tailles déclarées dans les deux en-têtes divergent, ou parce que les données compressées sont physiquement tronquées. Cela permet au lecteur de conserver l'entrée et de fournir un diagnostic au niveau de la ressource (`read-error` ou `size-mismatch`) plutôt que de rendre tout le package opaque. Aucune extraction n'est effectuée.

Limite : l'offset local ZIP64 par entrée (`0xffffffff`) est refusé, car son extra ZIP64 n'est pas encore résolu ici. Les autres variantes ZIP64/SFX adversariales, les noms non UTF-8 et les chemins ambigus restent à couvrir.

Validation sur HEAD `84171296dbbf935cf116ac9c62a7404ea9196661` : Unix run `44341054195` (104/104 tests, formatage et Clippy) ; Windows run `44341054213` (formatage, Clippy, tests, build Release x64 et upload de l'exécutable). Aucun test ignoré.


## Archive SFX préfixée — 18 novembre 2026

Une régression construit une archive ZIP à deux entrées, puis lui ajoute un stub préfixé de type SFX sans modifier les offsets classiques contenus dans la table centrale. Le compteur brut doit reconnaître deux entrées et valider leurs en-têtes locaux en appliquant l'ajustement d'offset ; le test couvre aussi le maintien du curseur dans la table centrale entre les deux entrées.

Validation sur HEAD `817ab7811315bfec6ec496cc2474a2bb5c3232a1` : Unix run `44342345771` (105/105 tests, formatage et Clippy) et Windows run `44342345766` (tests, formatage, Clippy, build Release x64 et upload de l'exécutable). Les cas SFX plus complexes et les offsets ZIP64 par entrée restent à couvrir.


## Validation ZIP64 par entrée — 18 novembre 2026

Le compteur structurel de la table centrale reconnaît maintenant les sentinelles ZIP64 par entrée pour les tailles compressée/décompressée et l'offset local. Pour toute valeur sentinelle, l'extra field `0x0001` doit être unique et contenir exactement les valeurs 64 bits attendues, dans l'ordre défini par les champs sentinelles. Les champs tronqués, dupliqués ou de longueur incorrecte rendent la structure non fiable.

L'offset local est lu dans le champ ZIP64 central après les valeurs de taille éventuelles ; l'en-tête local correspondant reste soumis aux contrôles existants de signature, flags, méthode, nom brut et position avant la table centrale. Une fixture valide transforme une entrée ZIP simple en entrée dont les tailles locales et centrales utilisent les sentinelles 32 bits et des valeurs ZIP64 dans les extras. La fixture a nécessité une longueur déclarée de 20 octets pour chaque extra (4 octets d'identifiant/longueur et 16 octets de données).

Tests et CI validés sur HEAD `28bfa8615a304c94a04dcf0214bde60b443a258d` : Unix run `44390560588` et Windows run `44390562401` ont tous deux passé formatage, Clippy et tests ; Windows a également passé le build Release x64 et l'upload de l'exécutable.

Périmètre restant : fixtures pour valeurs 64 bits dépassant réellement 32 bits, combinaisons de sentinelles, cohérence des valeurs étendues locales/centrales, offsets SFX combinés à ZIP64, noms non UTF-8 et chemins ambigus. Cette tranche ne constitue pas une validation exhaustive du format ZIP64 et ne change pas le principe de non-extraction des ressources.

## Encodage des noms ZIP et extra field Unicode Path

La bibliothèque ZIP verrouillée par le projet est `2.6.1`. L'analyse des noms doit distinguer les octets stockés dans le champ filename, le décodage textuel (UTF-8 lorsque le bit EFS est actif, CP437 dans le cas historique) et l'éventuel nom UTF-8 fourni par l'extra field Info-ZIP Unicode Path `0x7075`.

Un nom dont les octets ne sont pas UTF-8 n'est pas automatiquement invalide : il peut être un nom CP437 valide si le bit EFS est désactivé. À l'inverse, un bit EFS positionné avec une séquence UTF-8 invalide doit être signalé. Le champ Unicode Path doit être validé (version, CRC32 des octets du nom d'origine, contenu UTF-8), et ne doit pas être accepté lorsqu'il est incohérent ou périmé. La couverture doit comparer les noms bruts et les noms effectifs afin de détecter les doublons et les divergences de nommage.
