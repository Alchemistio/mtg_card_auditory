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

La première tranche est en place pour les conteneurs ZIP et dossiers : les extensions `.mse-set`, `.mse-game` et `.mse-style` sont reconnues, le type attendu est affiché et la présence du fichier principal à la racine (`set`, `game` ou `style`) est vérifiée dans la table centrale ZIP ou par l'analyse capability-based du fichier principal du dossier. Le préfixe du fichier principal est lu en flux borné à 1 Mio, sans extraction ; les lignes complètes permettent d'extraire `mse version`, `name`, `short name`, `full name`, `version`, `game` et `stylesheet`. Les directives `include file:` sont détectées mais jamais suivies. Les dépendances `depends on` sont extraites dans les syntaxes compacte et en bloc (`package`/`version`), et sont résolues uniquement contre les packages présents dans les sources locales analysées ; les correspondances absentes ou multiples sont signalées. Les versions minimales sont comparées lorsque les deux valeurs suivent les formes documentées (`a.b.c` ou `YYYY-MM-DD`) ; les formats inconnus sont signalés comme non vérifiables. Les cycles du graphe de dépendances local sont détectés par parcours itératif, sans récursion sur les données analysées. Aucune dépendance n'est téléchargée ni installée. La reconnaissance des packages sous forme de dossiers et la première résolution locale des dépendances sont en place. La résolution locale des références de jeu (par nom déclaré) et de feuille de style (par nom court et jeu) est également en place ; les références absentes ou ambiguës sont signalées sans téléchargement. La lecture complète des en-têtes, la validation des versions minimales et l'inventaire des ressources restent à faire.

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
- Le décodage UTF-8 et l'indentation sont validés ; les constructions inconnues sont conservées comme avertissements, pas interprétées comme des instructions.
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

## Hors périmètre

Le rendu des cartes, l'exécution des scripts MSE, l'installation de packages, la réparation automatique, la modification des sets et la résolution de dépendances sur Internet ne font pas partie de l'audit initial.
