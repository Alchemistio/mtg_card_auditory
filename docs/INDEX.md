# Index documentaire

Cet index est le point d'entrée obligatoire pour consulter les sources du projet. À chaque reprise, lire cet index et la feuille de route, puis ouvrir les documents pertinents avant de poser une question ou de modifier le code.

| Domaine | Document de référence | Quand le consulter |
|---|---|---|
| Exigences produit | [REQUIREMENTS.md](REQUIREMENTS.md) | Périmètre, fonctionnalités attendues, critères d'acceptation et décisions produit |
| Architecture | [ARCHITECTURE.md](ARCHITECTURE.md) | Organisation du code, flux, modèles, composants et choix techniques |
| Sécurité | [SECURITY.md](SECURITY.md) | Fichiers locaux, chemins, liens symboliques, permissions, secrets, archives, TOCTOU et exports |
| Packages MSE | [MSE_ANALYSIS.md](MSE_ANALYSIS.md) | Formats MSE, en-têtes, dépendances, ressources, budgets, références et cycles |
| Développement | [WORKFLOW.md](WORKFLOW.md) | Cycle de reprise, règles de travail autonome, CI, commits et condition d'arrêt |
| Planification | [ROADMAP.md](ROADMAP.md) | État des tâches, dépendances, priorités, preuves et prochaines opérations |
| Présentation et commandes | [../README.md](../README.md) | Fonctionnalités visibles, limites connues, prérequis et commandes de développement |

## Règles de consultation

- La feuille de route et le workflow sont lus à chaque reprise.
- Pour une tâche donnée, consulter au minimum le document de son domaine et vérifier le code/test correspondant sur le HEAD courant.
- En cas de contradiction, ne pas choisir arbitrairement : relever l'écart, vérifier les exigences et le comportement du code, puis corriger la documentation concernée.
- Les statuts de la feuille de route ne remplacent pas les preuves d'exécution ; conserver les commits, tests et runs CI pertinents.
- Aucun document ne doit être créé en doublon : mettre à jour le document existant et cet index si nécessaire.
