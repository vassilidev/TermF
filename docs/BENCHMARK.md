# Banc : l'équipe d'agents de TermF contre une session Claude Code seule

> Mesuré le 2026-11-22 avec Claude Code 2.1.330, Sonnet sur les deux bras. Rejouer : [`bench/README.md`](../bench/README.md). Résultats bruts : `bench/results/2026-11-22*/`. Trois séries : la première passe (6 tickets, l'équipe telle qu'elle était), la seconde passe (l'équipe rendue proportionnée, remesurée), des tickets complexes et réalistes sur un projet plus gros ; une quatrième, sur un vrai projet privé, est résumée plus bas (« Sur un vrai projet »).

> **Coût = équivalent API calculé par Claude Code** (`total_cost_usd`). Les essais tournent sur un abonnement Claude (Claude Code connecté à claude.ai) : rien n'est facturé en plus, c'est le quota d'usage qui est consommé. Les rapports de coût entre les bras gardent tout leur sens ; les plafonds par essai sont des plafonds d'usage (équivalent API). *Costs are the API-equivalent cost Claude Code computes; on a subscription nothing more is billed, the usage quota is consumed.*

## Summary (English)

Realistic tickets on two small projects (a task tracker, and a shared task-list app of a few thousand lines), each done by **TermF's agent team** (CTO triage, tasks, checks, code review, security review, delivery stopped before the pull request) and by **one plain Claude Code session** with no skill, plugin or subagent. Same model (Sonnet), same starting commit, same ticket text, same usage cap (API-equivalent); hidden tests the agents never see; a blind judge (Sonnet) that only sees the ticket and the diff.

- **First pass (24 trials): quality was a tie, the team cost 7.3 times more and took 4.3 times longer.** Both arms passed 100 % of the hidden tests; the judge gave 8.4 to the team and 8.3 to the plain session; on small tickets the core change was often the same line for line.
- **Second pass: the team was made proportionate** (the CTO sizes each ticket: trivial, `direct` with one developer and one short review, a lean workflow for medium work, the full team only for large, vague or sensitive work; gates only for large runs; unscripted typecheck and lint run with the tests and send failures back to the task's own session; scope discipline; lighter role prompts). On small tickets: **0.72 → 0.34 USD (−53 %), 101 → 69 s, 6.2 → 3 sessions, no question asked, no type error left**, same quality. It is still **3.4 times the cost and 2.7 times the time of a plain session**: the floor is ~20 k tokens of prompt and tools each session writes to the cache, which roles do not share.
- **Complex, realistic tickets (sharing with roles and expiring invitations, "the app is slow on big lists", a lost update under concurrency): here the team wins on quality.** Both pass the hidden tests, but the judge, told to weigh security, robustness and edge cases, finds **90 % of the implicit criteria met by the team against 72 %** (100 % against 38 % on the slow lists ticket), an overall **7.7 against 7.4**, more tests (60 against 45) and no type error; **for 4.8 times the cost and 3.7 times the time** (2.14 against 0.45 USD, 8 against 2 minutes). The CTO sent the lost-update ticket to `direct`: there the team was no better than one session.
- **A real project (private, a Laravel app in production, 4 tickets of its backlog)**: the CTO sized each ticket (one `trivial`, two `direct`, the URL-access security flaws as a full L run). The proportionate team did the small ones as well or better (judge 7 vs 6, 8 vs 7) for 3 to 4 times the cost; on the large security ticket it hit the trial's usage cap during its security review and did not do better than one session (judge 4 vs 5). On average there: judge 6.3 vs 6.0 for 4.4 times the cost.
- **Limits**: small samples (1 to 2 trials per ticket and arm on the later series), one model, an LLM judge that only sees the diff, hidden tests that never separate the arms.

## En bref

**Sur un petit ticket bien écrit, une session seule suffit et l'équipe coûte encore 3,4 fois plus (contre 7 fois avant cette PR), à qualité égale. Sur un ticket complexe, flou ou sensible d'un petit projet, l'équipe fait mieux : les critères implicites qu'un bon ingénieur doit deviner (sécurité, robustesse, cas limites) sont remplis à 90 % contre 72 %, avec plus de tests et aucune erreur de type, pour 4,8 fois le coût et 3,7 fois le temps.** Sur un vrai projet (Laravel, 4 tickets du propriétaire), elle ne fait pas mieux en moyenne (juge 6,3 contre 6,0, 4,4 fois le coût) : mieux sur les petits tickets, moins bien sur le gros ticket de sécurité, arrêté au plafond d'usage avant la fin de sa revue de sécurité. La seconde passe a rendu l'équipe proportionnée : un petit ticket va de la demande à la livraison en trois sessions et sans aucune question ; le reste de l'écart tient au coût fixe de chaque session.

## Sur un vrai projet (série privée)

Quatre vrais tickets du tableau de bord d'un projet du propriétaire : une application Laravel 10 / Livewire 2 en production (un back-office avec rôles et permissions, des listes DataTables, des PDF, plusieurs centaines de fichiers, ses propres tests). Le projet n'est **jamais modifié** : chaque essai le clone dans un dossier jetable (`git clone --no-hardlinks`, sans remote, au commit de la branche principale du 2026-11-20), avec ses dépendances copiées et un `.env` de test (SQLite jetable, aucun service externe). Les tickets, les 25 tests cachés PHPUnit écrits pour le banc et les résultats restent hors de ce dépôt (`--suite`) : seuls des chiffres sont publiés ici. Les tests du projet ne sont pas verts au départ sur SQLite (42 sur 90 passent : des tests périmés et des tests qui attendent une copie de la base de production) ; l'étape de contrôles de l'équipe y est donc « l'application démarre et ses routes se chargent ».

| Ticket | bras | essais | temps (s) | coût ($) | sessions | tests cachés | critères (juge) | juge /10 | sécurité /10 | tests du projet qui passent |
|---|---|---|---|---|---|---|---|---|---|---|
| Sécurité : accès par URL aux pages et aux données | à l'ancienne | 1 | 366 | 1,32 | 1 | 14/14 | 45 % | 5,0 | 6,0 | 51 / 99 |
| | équipe (complète, L, arrêtée au plafond d'usage) | 1 | 727 | 5,14 | 12 | 12/14 | 27 % | 4,0 | 6,0 | 137 / 191 |
| Listes : 25 lignes par défaut | à l'ancienne | 1 | 25 | 0,07 | 1 | 3/3 | 33 % | 6,0 | 10 | 42 / 90 |
| | équipe (`trivial`) | 1 | 115 | 0,23 | 2 | 3/3 | 33 % | 7,0 | 10 | 42 / 90 |
| Clés primaires manquantes (migration) | à l'ancienne | 1 | 225 | 0,39 | 1 | 3/4 | 67 % | 6,0 | 8,0 | 42 / 90 |
| | équipe (`direct`, arrêtée au plafond d'usage) | 1 | 725 | 2,50 | 5 | 4/4 | 67 % | 6,0 | 8,0 | 52 / 100 |
| Phrase d'accueil du tableau de bord | à l'ancienne | 1 | 31 | 0,10 | 1 | 3/4 | 80 % | 7,0 | 9,0 | 44 / 92 |
| | équipe (`direct`) | 1 | 99 | 0,42 | 3 | 4/4 | 80 % | 8,0 | 9,0 | 42 / 90 |
| **moyenne** | à l'ancienne | 4 | 162 | 0,47 | 1 | 88 % | 56 % | 6,0 | 8,3 | |
| | équipe | 4 | 417 | 2,07 | 5,5 | 96 % | 48 % | 6,3 | 8,3 | |

Le CTO a dimensionné chaque ticket comme le veut la règle de proportionnalité : « 25 lignes par défaut » en `trivial` (2 sessions, aucune gate, aucune revue séparée), la phrase d'accueil et les clés primaires en `direct`, la faille d'accès par URL en équipe complète (taille L, sensible : gates du routage et du plan, plusieurs tâches, revue, sécurité).

- **Petits tickets** (25 lignes, phrase d'accueil) : l'équipe fait aussi bien ou mieux (juge 7 contre 6 et 8 contre 7, 4/4 contre 3/4 aux tests cachés de la phrase d'accueil) pour 3 à 4 fois le coût et 3 à 4 fois le temps.
- **Clés primaires (migration)** : même note du juge (6) des deux côtés ; l'équipe passe les 4 tests cachés (3 sur 4 seule) et ajoute 10 tests, mais en `direct` avec deux tours de revue et la revue des migrations elle bute sur le plafond d'usage (2,50 $) : 6 fois le coût, 3 fois le temps.
- **Sécurité (accès par URL)** : la session seule ferme la plupart des failles (14/14 aux tests cachés), mais le juge relève une faille grave restante (une suppression de fichiers temporaires sans contrôle) et des tests qui appellent rarement les vraies routes : juge 5, sécurité 6. L'équipe complète découpe le travail en 8 tâches et écrit près de 100 tests, mais atteint le plafond d'usage de l'essai (5 $, que le CTO avait lui-même conseillé) pendant sa revue de sécurité : 12/14, juge 4, trois constats graves. **Sur ce ticket, l'équipe n'a pas fait mieux, pour 3,9 fois le coût** ; deux autres essais de l'équipe, coupés par un redémarrage de la machine, en étaient au même point (3,60 et 4,18 $ après 19 minutes).

Les tests du projet qui passaient passent toujours dans tous les essais (42 au départ ; certains essais en ajoutent) et `php artisan` démarre partout. **En moyenne sur ce vrai projet, l'équipe ne fait pas mieux qu'une session seule** (juge 6,3 contre 6,0, critères 48 % contre 56 %, tests cachés 96 % contre 88 %) pour 4,4 fois le coût et 2,6 fois le temps : les petits tickets sont mieux faits par l'équipe proportionnée, le gros ticket de sécurité dépasse ce qu'un essai plafonné lui laisse.


## Seconde passe : une équipe proportionnée

Après la première passe, l'équipe a été changée pour être **intelligente et autonome sans alourdir le moindre ticket** (règle du propriétaire), puis remesurée sur les mêmes tickets, avec `tsc` installé sur la machine du banc pour les deux bras (`bench/results/2026-11-22-v2`, `-v3`).

Ce qui a changé dans TermF :

1. **Proportionnalité** (rôle CTO) : trivial → `trivial` (une tâche, les contrôles du projet font la revue) ; petit → `direct` (une tâche, une revue courte) ; moyen → le workflow du projet avec seulement les étapes utiles ; seuls les gros, flous ou sensibles ont l'équipe complète. Le CTO donne lui-même la tâche des modes `direct` et `trivial` : ni cadrage, ni plan du lead.
2. **Autonomie** : la gate du routage et la gate du plan n'attendent par défaut que pour un ticket L (`[team] plan_gate = "large"`, `approve_route` non réglé) ; un petit ticket va de la demande à la livraison sans rien demander.
3. **Contrôles automatiques** : le typecheck et le lint que le dépôt configure (`tsc`, `eslint`, `mypy`, `pyright`, `ruff`, `cargo clippy`) tournent avec ses tests, même sans script ; un échec revient à la tâche (deux fois au plus) au lieu d'attendre l'utilisateur.
4. **Reprise de la même session** : une tâche renvoyée par la revue ou par les contrôles continue dans sa propre conversation au lieu d'une session neuve.
5. **Périmètre** : le brief dit de ne changer que ce que le ticket demande (le reste en propositions) et la revue renvoie tout changement non demandé.
6. **Moins de surcoût par session** : un rôle ne lit que les cartes de ses propres skills de la stack (prompt du CTO 15 → 8 Ko, du PO 13 → 6 Ko, du back 16 → 12 Ko) ; la revue de sécurité tourne à côté de la revue de code.
7. **Dossiers des tâches préparés** comme celui du run (dépendances, `.env`), pour que l'agent puisse lancer les contrôles d'un vrai projet.

| | essais | temps (s) | coût ($) | sessions | tests cachés | critères (juge) | juge /10 | constats « hors ticket » | typecheck ok | tests du projet |
|---|---|---|---|---|---|---|---|---|---|---|
| simples · à l'ancienne | 3 | 26 | 0,10 | 1 | 100 % | 100 % | 9,0 | 0,3 | 100 % | 12 |
| simples · équipe, avant | 6 | 101 | 0,72 | 6,2 | 100 % | 100 % | 8,7 | 1,0 | 67 % | 15 |
| simples · équipe, après | 7 | 69 | 0,34 | 3,0 | 100 % | 100 % | 9,0 | 0,3 | 100 % | 14 |
| complexes · à l'ancienne | 3 | 72 | 0,22 | 1 | 100 % | 96 % | 8,0 | 1,0 | 100 % | 18 |
| complexes · équipe, avant | 6 | 320 | 1,60 | 9,0 | 100 % | 100 % | 8,2 | 1,7 | 33 % | 34 |
| complexes · équipe, après | 3 | 278 | 1,45 | 7,7 | 100 % | 100 % | 8,0 | 2,0 | 100 % | 30 |

**Sur un petit ticket**, l'équipe passe de 6,2 à 3 sessions (tri, une tâche, une revue), **de 0,72 à 0,34 $ (−53 %) et de 101 à 69 s (−32 %)**, sans aucune question, à qualité égale ou meilleure (juge 9,0, aucune erreur de type, presque plus rien hors du ticket). **Elle reste 3,4 fois plus chère et 2,7 fois plus lente qu'une session seule** : l'objectif de 1,5 à 2 fois n'est pas atteint. Le plancher est connu : chaque session paie ~20 000 jetons de prompt système, d'outils et de rôle écrits dans le cache, parce que le préfixe diffère d'un rôle à l'autre (outils, skills, prompt) et que le cache ne se partage donc pas entre le tri, la tâche et la revue ; une session seule, elle, retrouve le sien d'un essai à l'autre. **Sur un ticket complexe**, −9 % de coût et −13 % de temps, une session de moins en moyenne, et plus aucune erreur de type laissée (33 % → 100 % de typecheck propre) ; la qualité mesurée ne bouge pas.

Ce que le CTO a fait des tickets simples : S1 (bug) et S2 (champ + migration) en `direct` à chaque fois ; S3 (messages d'erreur, API et page) en `direct` une fois la règle élargie à « un seul développeur, même sur l'API et sa page » (avec la première règle, « une seule zone », il gardait l'équipe : 7 à 8 sessions, 0,96 $). Aucun ticket n'a été jugé trivial : tous demandent un test.

## Tickets complexes et réalistes

Trois tickets sur `bench/fixture-lists/`, une appli de listes de tâches partagées de 2 100 lignes (comptes et jetons, listes, tâches, étiquettes, commentaires, journal d'activité, tâches de fond, une page en JS, 35 tests, un peu de dette) avec des problèmes plantés : pas de partage, une liste lente (N+1, aucun index, pas de pagination), une écriture perdue sous concurrence. Les tickets donnent le contrat de l'API mais laissent implicites la sécurité, la robustesse et les cas limites ; les tests cachés (41) les vérifient (un lecteur ne peut pas écrire, un lien expiré est refusé, aucune fuite entre comptes ; 10 000 tâches en moins de 150 ms par page, chaque tâche une fois dans l'ordre, égalités comprises ; 20 modifications concurrentes toutes appliquées, versions et 409), et le juge pèse sécurité, robustesse et cas limites.

| Ticket | bras | essais | temps (s) | coût ($) | sessions | tests cachés | critères (juge) | juge /10 | sécurité /10 | typecheck ok | tests du projet |
|---|---|---|---|---|---|---|---|---|---|---|---|
| R1 partage avec rôles et invitations | à l'ancienne | 2 | 203 | 0,67 | 1 | 15/15 | 86 % | 8,0 | 8,0 | 100 % | 53 |
| | équipe | 1 | 953 | 3,32 | 7 | 15/15 | 91 % | 8,0 | 9,0 | 100 % | 89 |
| R2 « l'appli est lente sur les grosses listes » | à l'ancienne | 2 | 103 | 0,35 | 1 | 14/14 | 38 % | 7,0 | 8,0 | 50 % | 40 |
| | équipe | 1 | 348 | 2,51 | 10 | 14/14 | 100 % | 8,0 | 8,0 | 100 % | 49 |
| R3 « des modifications disparaissent » | à l'ancienne | 1 | 49 | 0,22 | 1 | 12/12 | 67 % | 7,0 | 8,0 | 100 % | 39 |
| | équipe (`direct`) | 1 | 173 | 0,61 | 3 | 12/12 | 83 % | 7,0 | 8,0 | 100 % | 42 |
| **moyenne** | à l'ancienne | 5 | 132 | 0,45 | 1 | 100 % | 72 % | 7,4 | 8,0 | 80 % | 45 |
| | équipe | 3 | 492 | 2,14 | 6,7 | 100 % | 90 % | 7,7 | 8,3 | 100 % | 60 |

Ce que le juge dit, concrètement :

- **R2, la session seule** fait une pagination par curseur propre mais laisse des trous qu'un bon ingénieur aurait vus : aucun index pour les filtres `done` et `assignee`, une condition `OR` sur la date d'échéance qui empêche SQLite d'utiliser l'index, des colonnes internes qui fuient dans les réponses, aucun test d'écriture entre deux pages (« partiel » sur 4 critères sur 4 au premier essai). **L'équipe** charge les vues par lots, couvre tris, filtres et égalités, ajoute les index par une migration, et le juge ne trouve rien de grave : 4 critères sur 4.
- **R1** : les deux bras sont sérieux (jetons hachés, 404 pour un inconnu, 403 pour un membre sans droit) ; la session seule laisse des défauts sur la page (lien d'invitation servi sur un chemin imbriqué, liste des membres pas rafraîchie), l'équipe ne teste pas sa page (« partiel »). L'équipe y a passé 16 minutes, dont 1,32 $ pour la seule tâche du back.
- **R3** : le CTO l'a jugé petit et l'a envoyé en `direct` (3 sessions, 0,61 $) ; les deux bras font la même correction (lecture, vérification et écriture dans une seule transaction synchrone, version et 409) et la même erreur : le journal d'activité sort de la transaction (un changement sans trace si l'écriture du journal échoue). Juge 7 des deux côtés. Une perte de données sous concurrence aurait mérité l'équipe complète : la règle du CTO ne citait pas l'intégrité des données ; elle cite désormais les données qui peuvent se perdre et la concurrence parmi les sujets sensibles (non remesuré).

## Protocole

- **Projets** : `bench/fixture/` pour les six premiers tickets, `bench/fixture-lists/` (2 100 lignes, problèmes plantés) pour les tickets réalistes ; une suite privée (`--suite`) pour un vrai projet. `bench/fixture/` est un petit gestionnaire de tâches réaliste (API JSON en TypeScript exécuté directement par Node 22.18+, SQLite par `node:sqlite`, migrations par `PRAGMA user_version`, une page dans `public/`, 7 tests `node:test`, un README). Aucune dépendance : rien à installer, rien qui varie d'un essai à l'autre. Un bug est laissé dedans exprès (`?done=false` liste les tâches faites), comme dans un vrai dépôt.
- **Tickets** (`bench/tickets/`, rédigés comme de vrais tickets, critères d'acceptation numérotés) : trois simples (S1 corriger un bug, S2 un champ « échéance » avec sa migration, S3 messages d'erreur et validation du titre) et trois complexes (C1 étiquettes : API, tables, filtre, page, tests ; C2 remplacer `done` par un statut à trois états avec migration des données et compatibilité ; C3 comptes, connexion, jetons, tâches par utilisateur : sécurité).
- **Deux bras, même modèle (Sonnet), même départ** : une copie fraîche du projet, le même commit, le même texte de ticket, le même plafond d'usage de 3 $ (équivalent API) par essai.
  - **À l'ancienne** : une seule session `claude -p "Implement this ticket in this repository." + ticket`, effort `high`, sans skill (`--disable-slash-commands`), sans sous-agent (`Agent` interdit), sans plugin, sans MCP, sans les réglages ni le CLAUDE.md du testeur (`--setting-sources project,local --strict-mcp-config`), permissions ouvertes dans le dossier ; elle travaille jusqu'à dire qu'elle a fini.
  - **Équipe TermF** : le vrai parcours, lancé par le test ignoré `crates/termf-cli/tests/bench.rs` comme le harnais de la QA : tri du CTO → gate du routage → cadrage → plan → gate du plan → tâches → `npm test` → règles → revue → sécurité → mise à jour avec la base → gate de livraison, **arrêté avant la PR** (remote local, rien sur GitHub). La bibliothèque livrée, chaque rôle sur Sonnet (l'effort reste celui des rôles et du CTO), les mêmes drapeaux Claude Code, la config et les données TermF jetables. Le banc joue un utilisateur pressé : il approuve chaque gate, répond à chaque questionnaire par la proposition du CTO, accorde chaque permission, relève le budget conseillé par le CTO quand il est atteint (jusqu'au plafond), approuve un contrôle qui échoue, abandonne une étape renvoyée deux fois.
  - 2 essais par ticket et par bras, l'ordre des bras alterné d'un essai à l'autre, 4 essais en parallèle.
- **Mesures** : temps mur, coût (rapporté par Claude Code), sessions, tours, jetons. Qualité : **tests cachés** (`bench/hidden/`, 55 tests, copiés dans le projet seulement une fois l'agent fini : ils parlent à l'API, lisent la base, et vérifient quelques traits de la page), tests du projet (`npm test`) et tests de la première version toujours verts, typecheck (`tsc` strict), fichiers touchés hors de `src/`, `test/`, `public/` et `README.md`, README mis à jour quand le ticket le demande. **Juge à l'aveugle** : Sonnet, sans outil, ne voit que le ticket et le diff (jamais le bras) ; il dit pour chaque critère « rempli / partiel / non rempli », donne 7 notes sur 10 (justesse, sécurité, lisibilité, tests, gestion d'erreurs, cas limites, périmètre), liste ses constats de revue (gravité, catégorie) et donne une note globale.

## Première passe : résultats (avant les améliorations)

### Moyennes

| | bras | essais | temps (s) | coût ($) | sessions | tests cachés % | tests du projet verts % | critères remplis % (juge) | juge /10 | constats | graves | hors périmètre | lignes | typecheck ok % | doc % | tests du projet |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| simples | à l'ancienne | 6 | 26 | 0,10 | 1,0 | 100 | 100 | 97 | 8,7 | 4,5 | 0 | 0 | 96 | 100 | 100 | 12 |
| simples | équipe | 6 | 101 | 0,72 | 6,2 | 100 | 100 | 100 | 8,7 | 4,7 | 0 | 0 | 112 | 67 | 100 | 15 |
| complexes | à l'ancienne | 6 | 72 | 0,23 | 1,0 | 100 | 100 | 96 | 8,0 | 6,2 | 0 | 0 | 391 | 50 | 100 | 16 |
| complexes | équipe | 6 | 320 | 1,60 | 9,0 | 100 | 100 | 100 | 8,2 | 6,5 | 0 | 0 | 618 | 33 | 100 | 34 |
| tous | à l'ancienne | 12 | 49 | 0,16 | 1,0 | 100 | 100 | 96 | 8,3 | 5,3 | 0 | 0 | 244 | 75 | 100 | 14 |
| tous | équipe | 12 | 210 | 1,16 | 7,6 | 100 | 100 | 100 | 8,4 | 5,6 | 0 | 0 | 365 | 50 | 100 | 24 |

« Hors périmètre » compte les fichiers touchés hors de `src/`, `test/`, `public/`, `README.md` : aucun bras n'en a touché ; ce que le juge appelle « périmètre » (des changements de comportement non demandés dans ces fichiers) est dans les constats. « Tests du projet » : le nombre de tests de `npm test` à la fin (7 au départ).

### Par ticket

| Ticket | bras | temps (s) | coût ($) | sessions | tests cachés | critères % | juge /10 | constats (moyens) | typecheck ok | tests du projet |
|---|---|---|---|---|---|---|---|---|---|---|
| S1 bug `?done=false` | à l'ancienne | 14 | 0,06 | 1 | 6/6 | 100 | 9,0 | 3,0 (0) | 2/2 | 10 |
| | équipe | 81 | 0,56 | 5,5 | 6/6 | 100 | 9,0 | 3,0 (0) | 2/2 | 12 |
| S2 échéance + migration | à l'ancienne | 29 | 0,11 | 1 | 8/8 | 100 | 9,0 | 4,5 (0) | 2/2 | 12 |
| | équipe | 106 | 0,77 | 6,0 | 8/8 | 100 | 8,0 | 6,0 (1) | 2/2 | 20 |
| S3 messages d'erreur | à l'ancienne | 34 | 0,12 | 1 | 9/9 | 93 | 8,0 | 6,0 (0) | 2/2 | 13 |
| | équipe | 116 | 0,82 | 7,0 | 9/9 | 100 | 9,0 | 5,0 (0) | 0/2 | 13 |
| C1 étiquettes | à l'ancienne | 57 | 0,19 | 1 | 11/11 | 100 | 8,0 | 6,5 (1) | 1/2 | 13 |
| | équipe | 246 | 1,49 | 9,0 | 11/11 | 100 | 8,0 | 6,5 (0,5) | 2/2 | 45 |
| C2 statut + migration | à l'ancienne | 42 | 0,14 | 1 | 10/10 | 88 | 8,0 | 5,5 (0,5) | 1/2 | 14 |
| | équipe | 307 | 1,46 | 9,5 | 10/10 | 100 | 8,5 | 6,0 (1) | 0/2 | 25 |
| C3 comptes (sécurité) | à l'ancienne | 116 | 0,36 | 1 | 11/11 | 100 | 8,0 | 6,5 (2) | 1/2 | 22 |
| | équipe | 405 | 1,86 | 8,5 | 11/11 | 100 | 8,0 | 7,0 (1) | 0/2 | 32 |

Moyennes de 2 essais ; tests cachés identiques sur les deux essais. Constats : moyenne par essai, entre parenthèses ceux de gravité moyenne ; aucun constat grave sur les 24 essais.

## Première passe : ce que l'équipe fait mieux

- **Plus de tests** : 34 tests à la fin d'un ticket complexe contre 16 (45 contre 13 sur les étiquettes), dont un fichier de test de la migration à part sur C2. Le juge note moins de constats « tests manquants » chez elle (14 contre 18).
- **Tous les critères remplis** selon le juge (100 % contre 96 %) : la session seule en laisse un « partiel » de temps en temps (S3 : l'affichage de l'erreur sur la page n'est pas testé ; C2 : un corps JSON `null` non géré).
- **Le regard autour du ticket** : sur C1, la tâche QA a vu que le bug `?done=false` déjà présent cassait le critère « combine avec `done` » ; la revue l'a renvoyé au développeur, qui l'a corrigé. La session seule l'a corrigé aussi, sans le dire.
- **Un journal** : chaque décision du CTO (« Ruling: skip frame — the ticket lists 7 testable criteria — cost if wrong: … »), le plan, les rapports, les deux tours de revue : on sait pourquoi le code est comme il est. La session seule ne laisse que son diff et son message final.

## Première passe : ce qu'elle faisait moins bien

- **Le coût et le temps**, de loin : 7,3 fois le coût et 4,3 fois le temps en moyenne ; sur S1, **le même correctif ligne pour ligne** (`src/app.ts` et `src/tasks.ts` identiques dans les deux bras) coûte 0,56 $ et 81 s contre 0,06 $ et 14 s. Le plancher est structurel : une session par étape (tri, plan, tâche, revue, sécurité), et chaque session écrit ~20 000 jetons de prompt système, de rôle et de cartes de la bibliothèque dans le cache (100 000 à 122 000 jetons de cache créés pour un S1, contre 8 500 pour la session seule). Les étapes s'enchaînent en série : le temps s'additionne.
- **Des étapes qui répètent le ticket** : sur un petit ticket aux critères déjà testables, le cadrage (PO) et le plan (lead) reformulent ce que le ticket dit déjà ; le PO de S1 a refait l'analyse de la cause que le lead a refaite à son tour. Le CTO gardait le cadrage sur 3 tickets simples sur 6, comme sa consigne le lui demandait (« Skip the product owner only for an obvious urgent fix »).
- **Un budget conseillé trop court** : le CTO conseille ce que coûte le code, pas les revues ; deux runs (S1 au premier essai, S3) se sont arrêtés à « budget » avant la revue, en attendant l'utilisateur.
- **Plus de changements hors du ticket aux yeux du juge** : 16 constats « périmètre » contre 7 (durcissement non demandé sur C3 : limite de corps, en-têtes ; `?done=` rendu strict partout). C'est aussi le revers du regard autour du ticket.
- **Plus d'erreurs de type laissées** : `tsc` échoue sur 6 essais de l'équipe sur 12 contre 3 sur 12 (surtout `Parameter 't' implicitly has an 'any' type` dans les tests, une fois `Cannot use namespace 'DatabaseSync' as a type`). Le projet exécute son TypeScript sans le vérifier et n'a pas de script `typecheck` : personne ne le lance, mais les rôles de l'équipe écrivent plus de code de test.
- **Des points de friction du parcours** : un contrôle `npm test` rouge après les tâches (C1, le test du QA sur le bug déjà présent) passe si l'utilisateur pressé approuve ; une tâche QA s'arrête (« needs an answer ») sur un bug qui n'est pas le sien ; une décision du CTO peut dire « skip frame » pendant que son routage le garde (vu une fois : le cadrage a tourné quand même).
- **Pas de sur-ingénierie marquée** dans un bras comme dans l'autre : le juge dit « some » 3 fois chez l'équipe, 2 fois chez la session seule.

## Première passe : exemples tirés des diffs

- **S1, le même correctif** : les deux bras écrivent exactement ces lignes dans `src/app.ts` (`const raw = url.searchParams.get("done"); if (raw !== null && raw !== "true" && raw !== "false") return send(res, 400, …)`) et changent `filter: { done?: string }` en `boolean` dans `src/tasks.ts`. La différence est dans les tests : l'équipe ajoute une aide `makeOpenAndDone()` et trois tests séparés, la session seule trois tests en 25 lignes. Juge : 9 / 10 pour les deux.
- **C2, la même migration** : `ALTER TABLE tasks ADD COLUMN status … CHECK (status IN ('todo', 'doing', 'done')); UPDATE tasks SET status = CASE …; ALTER TABLE tasks DROP COLUMN done` dans les deux bras, au mot près. L'équipe ajoute `test/migration.test.ts` (une base de la première version migrée au démarrage) ; la session seule teste la migration dans le fichier existant.
- **C3, la sécurité** : les deux bras font scrypt salé, jetons de 32 octets hachés en base, requêtes filtrées par `user_id`, même réponse 401 pour un mauvais nom et un mauvais mot de passe, avec un hachage factice pour égaliser le temps de réponse. Le juge reproche aux deux l'absence de borne sur la longueur du mot de passe face au coût de scrypt, et des durcissements non demandés.

## Première passe : premiers correctifs et leur effet

1. **Raccourci « petit ticket » du CTO** (`library/roles/cto.md`) : sauter le cadrage quand la demande liste déjà des critères d'acceptation testables (avant : seulement pour un correctif urgent évident).
2. **Budget conseillé qui couvre toutes les étapes** (`library/roles/cto.md`) : chaque tâche, chaque tour de revue, la revue de sécurité ; un conseil trop court arrête le run avant ses revues.
3. **Décisions et routage cohérents** (`crates/termf-team/src/route.rs`) : une décision du CTO « Ruling: skip <étape> » alors que `skip` garde cette étape optionnelle est renvoyée au CTO (« a ruling skips step frame, but skip does not list it ») au lieu de faire tourner l'étape que le journal dit sautée. Test : `checks_the_workflow_its_steps_and_the_squads`.

Relancé sur les tickets simples, bras équipe (`bench/results/2026-11-22-after*`) :

| | essais | temps (s) | coût ($) | sessions | cadrage lancé | arrêts « budget » | tests cachés | juge /10 |
|---|---|---|---|---|---|---|---|---|
| avant | 6 (+1 du premier essai) | 101 | 0,72 | 6,2 | 3 / 6 | 1 / 6 (2 / 7) | 100 % | 8,7 |
| correctifs 1 et 2 | 6 | 107 | 0,69 | 5,7 | 2 / 6 (S3, dont une fois contre sa propre décision) | 0 / 6 | 100 % | 8,8 |
| S3 avec les trois correctifs | 2 | 109 | 0,64 | 5,0 | 0 / 2 | 0 / 2 | 100 % | 9,0 |

L'effet est réel mais petit : une session de moins sur les tickets où le cadrage tournait, plus d'arrêt pour budget. Le temps ne baisse pas (le bruit d'un essai à l'autre est de ±20 s). Les deux derniers essais de S3 ont tourné avec un plafond de 1,2 $ au lieu de 3 $ (le reste du budget du banc) : le CTO a conseillé 0,70 et 0,80 $, sans arrêt.

## Ce qui reste (dans `docs/STATUS.md` §4)

Fait dans cette PR : la tâche donnée par le CTO pour un petit ticket (plus de plan du lead), le typecheck et le lint non scriptés, la revue de sécurité à côté de la revue de code, des prompts de rôle allégés, la reprise de la même session. Reste :

- **Le coût fixe par session** (~20 000 jetons de prompt, d'outils et de rôle écrits dans le cache à chaque session, sans partage entre rôles) : le plancher d'un petit ticket en mode `direct` (3 sessions, ~0,34 $ contre ~0,10 $ seul). À essayer : un préfixe commun à tous les rôles (mêmes outils, le rôle et les cartes dans le premier message plutôt que dans le prompt système) pour que le tri, la tâche et la revue partagent le cache.
- **Le modèle par étape** : le CTO ne choisit pas encore un modèle moins cher (Haiku) pour une tâche mécanique ; le banc impose Sonnet partout.
- **Le tri lui-même** coûte 0,10 à 0,15 $ : il lit le code pour écrire une tâche très détaillée ; un tri plus court pour un petit ticket est à mesurer.
- **Les gros tickets restent lents** : 16 minutes et 3,32 $ pour le partage avec rôles (R1), dont 1,32 $ pour la seule tâche du back.

## Limites du banc

- **Deux petits projets propres** (340 et 2 100 lignes) sans dépendance ; un vrai dépôt n'est mesuré que par la série privée résumée plus haut.
- **Des tickets précis** dans la première passe (le terrain où une session seule excelle) ; les tickets réalistes laissent l'implicite au développeur, mais donnent le contrat de l'API pour que les tests cachés puissent l'appeler.
- **Peu d'essais dans les dernières séries** (1 ou 2 par ticket et par bras, budget oblige) : les moyennes des tickets réalistes reposent sur 5 essais seuls et 3 de l'équipe.
- **Un seul modèle** (Sonnet ; Haiku pas essayé faute de budget), Claude Code 2.1.330.
- **2 essais par ticket et par bras** : le bruit est grand (S3 équipe : 103 à 138 s selon l'essai).
- **Le juge est un LLM** (Sonnet) qui ne voit que le diff : il a pris pour un bug l'import d'une fonction déjà exportée hors du diff, et compte comme « hors périmètre » des corrections justifiées.
- **Les tests cachés ne départagent pas** : 100 % partout. Ils ont été corrigés trois fois pendant le banc pour être justes (vérifications de la page trop liées à une écriture, `authorization` en minuscules, comparaison de lignes SQLite), puis tous les essais remesurés depuis leur diff (`--remeasure`).
- **Le temps de l'utilisateur n'est pas compté** : le banc approuve chaque gate à l'instant ; un vrai utilisateur ajoute ses attentes à chaque gate de l'équipe. Depuis la seconde passe, un ticket S ou M n'en a plus aucune avant la livraison.
- **La machine du banc a TypeScript installé** (`tsc` de `app/node_modules` sur le PATH des deux bras) depuis la seconde passe ; la première passe ne l'avait pas, d'où des erreurs de type laissées des deux côtés.
- Les essais tournaient 4 par 4 en parallèle : le temps mur peut en souffrir un peu, des deux côtés.
- Mesuré sur `main` avant la PR #100 (QA de l'équipe) : son correctif des questionnaires aurait évité 3 nouvelles tentatives d'étape sur 2 runs de C2 (« give one implication per option »), quelques centimes et secondes de l'équipe sur ce ticket.
