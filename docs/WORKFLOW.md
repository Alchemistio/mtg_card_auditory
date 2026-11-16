# Workflow de développement autonome

## Objectif

Faire progresser MTG Asset Auditor de manière continue, sûre et vérifiable. Une CI en attente n'est pas un motif pour interrompre le travail lorsqu'il reste des tâches indépendantes réalisables.

## Règles impératives

1. Travailler exclusivement dans le dépôt `Alchemistio/mtg_card_auditory`, branche `main`.
2. Ne jamais modifier le MTG Card Remodeler public ou privé, ni y intégrer de code, d'assets ou de documentation. Cette interdiction reste active jusqu'à finalisation explicite de son cahier des charges et autorisation de l'utilisateur.
3. Ne pas attendre passivement une CI : après chaque push, poursuivre toutes les tâches qui ne dépendent pas de son résultat.
4. Ne jamais annoncer une validation CI, test, compilation, sécurité ou packaging sans preuve issue d'une exécution réelle.
5. Conserver les commits petits, cohérents et réversibles ; inspecter le HEAD avant chaque modification et utiliser une mise à jour de référence protégée contre les courses.
6. Mettre à jour la documentation et la feuille de route avec chaque changement fonctionnel ou de sécurité.
7. Ne pas poser de question sauf blocage réellement crucial ou décision produit irréversible qui ne peut être déduite des exigences existantes.
8. Ne pas créer de rappels ou d'automatisations.

## Consultation obligatoire de la feuille de route et de l'index

À chaque reprise, avant toute question ou modification :

1. Lire intégralement `docs/ROADMAP.md` et `docs/INDEX.md` sur le HEAD courant de `main`.
2. Utiliser l'index pour repérer les documents de référence correspondant à chaque catégorie de tâche (exigences, architecture, sécurité, MSE, tests/CI, livraison).
3. Lire les passages pertinents des documents référencés, puis vérifier dans le code et les tests si la tâche est réellement accomplie.
4. Comparer chaque tâche ouverte à l'état réel du dépôt. Corriger les tâches obsolètes, scinder les tâches trop larges en unités vérifiables, ajouter les dépendances et critères d'acceptation manquants.
5. Prioriser les tâches autonomes et poursuivre jusqu'à ce qu'il ne reste que des blocages externes ou des décisions indispensables.
6. Mettre à jour la feuille de route après chaque livraison : statut `[ ]` non commencée, `[~]` en cours/partielle, `[x]` terminée avec preuve. Une tâche n'est `[x]` que si son critère d'acceptation a été vérifié sur le code et, lorsque pertinent, par tests/CI.
7. Mettre à jour l'index lorsqu'un document est créé, renommé, fusionné ou change de périmètre.

Si `docs/INDEX.md` ou `docs/ROADMAP.md` manque, le recréer immédiatement à partir des documents réellement présents avant de poursuivre.

## Cycle de travail obligatoire

### A. Reprise de session

- Lire le HEAD de `main`, les derniers commits et les workflows Actions actifs.
- Contrôler les résultats des runs en cours ou récemment terminés : état global, jobs, étapes, logs en cas d'échec et artefacts.
- Reprendre les tâches ouvertes de `docs/ROADMAP.md`, `docs/SECURITY.md`, `docs/ARCHITECTURE.md` et les audits de code.
- Vérifier les documents existants avant d'en créer un doublon.

### B. Analyse avant modification

- Identifier les tâches indépendantes de la CI en cours.
- Examiner le code, les tests, les dépendances et la documentation réellement présents sur le HEAD.
- Rechercher les risques de sécurité, régressions, incohérences documentaires et fonctionnalités inachevées.
- Prioriser les corrections qui améliorent sécurité, intégrité des sources, reproductibilité et tests.

### C. Implémentation continue, sans attente passive

Après un push et pendant que les workflows s'exécutent, continuer notamment :
- audits statiques du code et revue des chemins ambiants/capability-based ;
- préparation ou ajout de tests unitaires et d'intégration indépendants ;
- corrections documentaires et mise à jour de la feuille de route ;
- analyse des dépendances, verrouillage et reproductibilité des builds ;
- conception et implémentation des fonctionnalités qui ne dépendent pas d'un résultat CI ;
- vérification des interfaces et des erreurs limites ;
- revue des risques TOCTOU, liens symboliques, permissions, fichiers concurrents et annulation.

Ne pas lancer une modification qui rendrait ambiguë ou annulerait la CI en cours si elle dépend du même workflow ou risque d'annuler le run. Dans ce cas, continuer les audits et préparer le changement, puis pousser lorsque le run courant est terminé ou lorsque le changement est indépendant et que l'annulation est sans conséquence.

### D. Contrôle après chaque changement

- Examiner le diff complet et vérifier qu'aucun fichier hors périmètre n'a été touché.
- Exécuter localement les contrôles disponibles lorsque l'environnement le permet.
- Ajouter/adapter les tests pertinents.
- Mettre à jour les documents concernés.
- Créer un commit descriptif et pousser sur le dépôt d'audit uniquement.
- Relever les identifiants des workflows déclenchés.

### E. Suivi CI sans bloquer les autres travaux

- Consulter les runs, jobs, étapes et logs réels.
- Si une CI est en attente/en cours, ne pas déclarer le changement validé et poursuivre les tâches indépendantes.
- Si elle échoue, diagnostiquer les logs précis, corriger la cause, ajouter un test de non-régression si pertinent, puis relancer.
- Si elle réussit, vérifier toutes les étapes pertinentes et les éventuels messages `SKIP`. Une CI réussie avec un test ignoré ne prouve pas que ce test a été exécuté.
- Vérifier le statut de chaque workflow pertinent (Windows et Unix) pour le HEAD exact, pas celui d'un commit antérieur.
- Les workflows doivent contrôler `cargo fmt --all -- --check` sans exécuter préalablement `cargo fmt --all`, afin que le contrôle détecte réellement les fichiers mal formatés au lieu de les corriger dans le runner.

### F. Fin de cycle / condition d'arrêt

Ne s'arrêter que lorsque :
- toutes les tâches actuellement réalisables sans nouvelle décision utilisateur sont terminées ;
- toutes les modifications réalisées sont documentées et poussées ;
- les contrôles locaux disponibles ont été exécutés ;
- les CI pertinentes du dernier HEAD ont été vérifiées, ou leur état restant est explicitement indiqué si l'environnement ne permet pas d'attendre ;
- les prochaines tâches restantes sont réellement bloquées par une CI, une dépendance externe, un accès manquant ou une décision cruciale.

Si une CI est toujours active mais qu'aucune autre tâche indépendante n'est réalisable, indiquer précisément ce qui reste à vérifier et pourquoi l'attente est nécessaire. Ne pas présenter cette attente comme un achèvement du projet.

## Rapport de reprise à l'utilisateur

À chaque pause inévitable, fournir brièvement :
- les commits réalisés ;
- les vérifications confirmées et celles non terminées ;
- l'état des workflows Windows et Unix du HEAD ;
- les tâches restantes et leur blocage réel ;
- les estimations d'avancement, clairement qualifiées d'indicatives.

## Protection du périmètre

Le projet d'audit reste autonome. Aucun changement au Remodeler principal n'est permis dans ce workflow. Les pourcentages d'avancement sont des estimations et ne remplacent jamais les preuves de tests ou de livraison.


## Balayage obligatoire de tous les domaines avant clôture d'un cycle

La feuille de route est désormais pilotée par domaine, en plus de la file P0–P3. Cette règle s'ajoute au cycle existant et ne le remplace pas.

Avant de déclarer un cycle de code terminé ou de s'arrêter parce que la CI est en cours :

1. Relire le « Tableau de pilotage par domaine » de `docs/ROADMAP.md`.
2. Passer en revue **chaque domaine**, y compris ceux qui ne sont pas le sujet du commit courant : fondation/workflow, GitHub/sources distantes, inventaire/exports, sécurité/concurrence, formats/MSE, métadonnées raster/SVG/polices/notices, rapports, galerie/revue humaine, performance/expérience, packaging/publication.
3. Pour chacun, vérifier le code et les critères pertinents, puis consigner l'une des décisions suivantes : `AVANCER` (tâche indépendante réalisable maintenant), `PRÉPARER` (analyse/test possible mais push ou validation bloqué par une CI active ou une dépendance), `BLOQUÉ` (raison externe ou décision utilisateur indispensable), ou `RIEN À FAIRE` (critères déjà satisfaits pour ce domaine).
4. Si une tâche peut être faite sans introduire un changement incohérent, la faire avancer. Garder les commits ciblés : le balayage transversal n'autorise pas à mélanger des fonctionnalités sans lien dans un même commit.
5. Ne pas pousser une nouvelle modification qui risque d'annuler un workflow encore actif. Dans ce cas, faire les audits, lectures, conception et préparation de tests possibles, puis reprendre la livraison après la fin du run.
6. Actualiser les pourcentages de maturité uniquement avec justification concrète ; ne jamais les augmenter simplement parce qu'un commit a été poussé. Conserver la distinction entre travail codé, testé localement et validé par CI.
7. Ajouter au rapport de reprise une courte synthèse du balayage : domaines avancés, domaines préparés, domaines bloqués et raisons.

Le balayage est une obligation d'examen et de priorisation, pas une obligation de modifier artificiellement chaque domaine à chaque cycle.
