# Banc : l'équipe d'agents de TermF contre une session Claude Code seule

> Mesuré le 2026-11-22 avec Claude Code 2.1.330, Sonnet sur les deux bras. Rejouer : [`bench/README.md`](../bench/README.md). Résultats bruts : `bench/results/2026-11-22*/`.

## Summary (English)

Six realistic tickets (three simple, three complex: a feature across API, data, page and tests; a refactor with a data migration; accounts and access rights) on a small TypeScript / SQLite project, each done twice by **TermF's agent team** (CTO triage, plan, tasks, checks, code review, security review, delivery stopped before the pull request) and twice by **one plain Claude Code session** with no skill, plugin or subagent. Same model (Sonnet), same starting commit, same ticket text, same budget cap. 24 trials, about 24 USD in all.

- **Quality is a tie on what can be measured.** Both arms pass **100 % of the hidden tests** on every ticket (55 tests the agents never saw), keep the project's tests green, update the docs when asked. The blind judge (Sonnet, sees only the ticket and the diff) gives **8.4 / 10 to the team and 8.3 to the plain session**, finds no serious issue in either, and rates the acceptance criteria met at **100 % for the team against 96 %**. On the simple tickets the core change is often **the same line for line**.
- **The team is 4.3 times slower and 7.3 times more expensive**: 101 s and 0.72 USD against 26 s and 0.10 USD on a simple ticket (6.2 sessions against 1), 320 s and 1.60 USD against 72 s and 0.23 USD on a complex one (9 sessions).
- **What the team does better**: about 1.7 times more tests (34 against 16 on complex tickets), every criterion met, and its QA and review catch what is around the ticket (the pre-existing `?done=false` bug on the tags ticket). **What it does worse**: cost and time; more changes outside the ticket in the judge's eyes (16 scope findings against 7); more type errors left behind (it fails `tsc` in 50 % of the trials against 25 %), since nobody runs a typecheck the project does not script; on a small ticket, the framing and planning sessions repeat what the ticket already says.
- **Fixed in this pull request**: the CTO now skips the framing step when the ticket already lists testable criteria, advises a budget that covers the reviews (no more run stopped before its review), and a ruling that skips a step the routing keeps is sent back. Effect on the simple tickets: 6.2 → 5.7 sessions (5.0 with the last fix), 0.72 → 0.69 USD (0.64), no budget stop (1 in 6 before, 2 in 7 with the first quick run). The team stays about 6 times the cost of a plain session on a small ticket: the floor is one session per step (triage, plan, task, review, security), each writing ~20 k tokens of system prompt and library to the cache.
- **Limits**: one small project, one model, 2 trials per ticket and arm, precise tickets (where a plain session shines), an LLM judge that only sees the diff.

## En bref

**Sur ce banc, l'équipe ne fait pas mieux que Claude Code seul en qualité mesurable, et elle coûte 7 fois plus pour 4 fois plus de temps.** Les deux bras réussissent tous les tests cachés ; le juge à l'aveugle les met à égalité (8,4 contre 8,3) ; l'équipe écrit deux fois plus de tests et remplit tous les critères, mais touche plus souvent au-delà du ticket et laisse plus d'erreurs de type. Sur un petit ticket bien écrit, l'équipe est surtout chère : cinq ou six sessions là où une suffit. Trois correctifs simples sont dans cette PR (cadrage sauté quand le ticket a ses critères, budget conseillé qui couvre les revues, décisions du CTO cohérentes avec son routage) ; ils retirent une session et les arrêts pour budget, pas l'écart de fond.

## Protocole

- **Projet** : `bench/fixture/`, un petit gestionnaire de tâches réaliste (API JSON en TypeScript exécuté directement par Node 22.18+, SQLite par `node:sqlite`, migrations par `PRAGMA user_version`, une page dans `public/`, 7 tests `node:test`, un README). Aucune dépendance : rien à installer, rien qui varie d'un essai à l'autre. Un bug est laissé dedans exprès (`?done=false` liste les tâches faites), comme dans un vrai dépôt.
- **Tickets** (`bench/tickets/`, rédigés comme de vrais tickets, critères d'acceptation numérotés) : trois simples (S1 corriger un bug, S2 un champ « échéance » avec sa migration, S3 messages d'erreur et validation du titre) et trois complexes (C1 étiquettes : API, tables, filtre, page, tests ; C2 remplacer `done` par un statut à trois états avec migration des données et compatibilité ; C3 comptes, connexion, jetons, tâches par utilisateur : sécurité).
- **Deux bras, même modèle (Sonnet), même départ** : une copie fraîche du projet, le même commit, le même texte de ticket, le même plafond de 3 $ par essai.
  - **À l'ancienne** : une seule session `claude -p "Implement this ticket in this repository." + ticket`, effort `high`, sans skill (`--disable-slash-commands`), sans sous-agent (`Agent` interdit), sans plugin, sans MCP, sans les réglages ni le CLAUDE.md du testeur (`--setting-sources project,local --strict-mcp-config`), permissions ouvertes dans le dossier ; elle travaille jusqu'à dire qu'elle a fini.
  - **Équipe TermF** : le vrai parcours, lancé par le test ignoré `crates/termf-cli/tests/bench.rs` comme le harnais de la QA : tri du CTO → gate du routage → cadrage → plan → gate du plan → tâches → `npm test` → règles → revue → sécurité → mise à jour avec la base → gate de livraison, **arrêté avant la PR** (remote local, rien sur GitHub). La bibliothèque livrée, chaque rôle sur Sonnet (l'effort reste celui des rôles et du CTO), les mêmes drapeaux Claude Code, la config et les données TermF jetables. Le banc joue un utilisateur pressé : il approuve chaque gate, répond à chaque questionnaire par la proposition du CTO, accorde chaque permission, relève le budget conseillé par le CTO quand il est atteint (jusqu'au plafond), approuve un contrôle qui échoue, abandonne une étape renvoyée deux fois.
  - 2 essais par ticket et par bras, l'ordre des bras alterné d'un essai à l'autre, 4 essais en parallèle.
- **Mesures** : temps mur, coût (rapporté par Claude Code), sessions, tours, jetons. Qualité : **tests cachés** (`bench/hidden/`, 55 tests, copiés dans le projet seulement une fois l'agent fini : ils parlent à l'API, lisent la base, et vérifient quelques traits de la page), tests du projet (`npm test`) et tests de la première version toujours verts, typecheck (`tsc` strict), fichiers touchés hors de `src/`, `test/`, `public/` et `README.md`, README mis à jour quand le ticket le demande. **Juge à l'aveugle** : Sonnet, sans outil, ne voit que le ticket et le diff (jamais le bras) ; il dit pour chaque critère « rempli / partiel / non rempli », donne 7 notes sur 10 (justesse, sécurité, lisibilité, tests, gestion d'erreurs, cas limites, périmètre), liste ses constats de revue (gravité, catégorie) et donne une note globale.

## Résultats

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

## Ce que l'équipe fait mieux

- **Plus de tests** : 34 tests à la fin d'un ticket complexe contre 16 (45 contre 13 sur les étiquettes), dont un fichier de test de la migration à part sur C2. Le juge note moins de constats « tests manquants » chez elle (14 contre 18).
- **Tous les critères remplis** selon le juge (100 % contre 96 %) : la session seule en laisse un « partiel » de temps en temps (S3 : l'affichage de l'erreur sur la page n'est pas testé ; C2 : un corps JSON `null` non géré).
- **Le regard autour du ticket** : sur C1, la tâche QA a vu que le bug `?done=false` déjà présent cassait le critère « combine avec `done` » ; la revue l'a renvoyé au développeur, qui l'a corrigé. La session seule l'a corrigé aussi, sans le dire.
- **Un journal** : chaque décision du CTO (« Ruling: skip frame — the ticket lists 7 testable criteria — cost if wrong: … »), le plan, les rapports, les deux tours de revue : on sait pourquoi le code est comme il est. La session seule ne laisse que son diff et son message final.

## Ce qu'elle fait moins bien

- **Le coût et le temps**, de loin : 7,3 fois le coût et 4,3 fois le temps en moyenne ; sur S1, **le même correctif ligne pour ligne** (`src/app.ts` et `src/tasks.ts` identiques dans les deux bras) coûte 0,56 $ et 81 s contre 0,06 $ et 14 s. Le plancher est structurel : une session par étape (tri, plan, tâche, revue, sécurité), et chaque session écrit ~20 000 jetons de prompt système, de rôle et de cartes de la bibliothèque dans le cache (100 000 à 122 000 jetons de cache créés pour un S1, contre 8 500 pour la session seule). Les étapes s'enchaînent en série : le temps s'additionne.
- **Des étapes qui répètent le ticket** : sur un petit ticket aux critères déjà testables, le cadrage (PO) et le plan (lead) reformulent ce que le ticket dit déjà ; le PO de S1 a refait l'analyse de la cause que le lead a refaite à son tour. Le CTO gardait le cadrage sur 3 tickets simples sur 6, comme sa consigne le lui demandait (« Skip the product owner only for an obvious urgent fix »).
- **Un budget conseillé trop court** : le CTO conseille ce que coûte le code, pas les revues ; deux runs (S1 au premier essai, S3) se sont arrêtés à « budget » avant la revue, en attendant l'utilisateur.
- **Plus de changements hors du ticket aux yeux du juge** : 16 constats « périmètre » contre 7 (durcissement non demandé sur C3 : limite de corps, en-têtes ; `?done=` rendu strict partout). C'est aussi le revers du regard autour du ticket.
- **Plus d'erreurs de type laissées** : `tsc` échoue sur 6 essais de l'équipe sur 12 contre 3 sur 12 (surtout `Parameter 't' implicitly has an 'any' type` dans les tests, une fois `Cannot use namespace 'DatabaseSync' as a type`). Le projet exécute son TypeScript sans le vérifier et n'a pas de script `typecheck` : personne ne le lance, mais les rôles de l'équipe écrivent plus de code de test.
- **Des points de friction du parcours** : un contrôle `npm test` rouge après les tâches (C1, le test du QA sur le bug déjà présent) passe si l'utilisateur pressé approuve ; une tâche QA s'arrête (« needs an answer ») sur un bug qui n'est pas le sien ; une décision du CTO peut dire « skip frame » pendant que son routage le garde (vu une fois : le cadrage a tourné quand même).
- **Pas de sur-ingénierie marquée** dans un bras comme dans l'autre : le juge dit « some » 3 fois chez l'équipe, 2 fois chez la session seule.

## Exemples tirés des diffs

- **S1, le même correctif** : les deux bras écrivent exactement ces lignes dans `src/app.ts` (`const raw = url.searchParams.get("done"); if (raw !== null && raw !== "true" && raw !== "false") return send(res, 400, …)`) et changent `filter: { done?: string }` en `boolean` dans `src/tasks.ts`. La différence est dans les tests : l'équipe ajoute une aide `makeOpenAndDone()` et trois tests séparés, la session seule trois tests en 25 lignes. Juge : 9 / 10 pour les deux.
- **C2, la même migration** : `ALTER TABLE tasks ADD COLUMN status … CHECK (status IN ('todo', 'doing', 'done')); UPDATE tasks SET status = CASE …; ALTER TABLE tasks DROP COLUMN done` dans les deux bras, au mot près. L'équipe ajoute `test/migration.test.ts` (une base de la première version migrée au démarrage) ; la session seule teste la migration dans le fichier existant.
- **C3, la sécurité** : les deux bras font scrypt salé, jetons de 32 octets hachés en base, requêtes filtrées par `user_id`, même réponse 401 pour un mauvais nom et un mauvais mot de passe, avec un hachage factice pour égaliser le temps de réponse. Le juge reproche aux deux l'absence de borne sur la longueur du mot de passe face au coût de scrypt, et des durcissements non demandés.

## Améliorations faites dans cette PR et leur effet

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

- Le coût fixe par session (~20 k jetons écrits dans le cache par rôle) : le premier levier, à mesurer (cartes de la bibliothèque par rôle, ordre du prompt pour partager le cache).
- Un ticket S à une seule tâche passe encore par le plan du lead : le CTO pourrait donner la tâche lui-même.
- Le typecheck : faire lancer `tsc` (ou le typecheck de la stack) aux rôles quand le projet est en TypeScript, même sans script.
- Les étapes en série : la revue de code et la revue de sécurité pourraient tourner en parallèle.

## Limites du banc

- **Un seul projet, petit et propre** (340 lignes), sans dépendance : un vrai dépôt (gros, mal documenté, plusieurs dépôts) est là où l'équipe est censée apporter le plus ; ce banc ne le mesure pas.
- **Des tickets précis** : chaque ticket donne ses critères et ses messages d'erreur au mot près. C'est le terrain où une session seule excelle ; un ticket flou ou une épopée donnerait sans doute un autre résultat (le CTO pose alors ses questions).
- **Un seul modèle** (Sonnet ; Haiku pas essayé faute de budget), Claude Code 2.1.330.
- **2 essais par ticket et par bras** : le bruit est grand (S3 équipe : 103 à 138 s selon l'essai).
- **Le juge est un LLM** (Sonnet) qui ne voit que le diff : il a pris pour un bug l'import d'une fonction déjà exportée hors du diff, et compte comme « hors périmètre » des corrections justifiées.
- **Les tests cachés ne départagent pas** : 100 % partout. Ils ont été corrigés trois fois pendant le banc pour être justes (vérifications de la page trop liées à une écriture, `authorization` en minuscules, comparaison de lignes SQLite), puis tous les essais remesurés depuis leur diff (`--remeasure`).
- **Le temps de l'utilisateur n'est pas compté** : le banc approuve chaque gate à l'instant ; un vrai utilisateur ajoute ses attentes à chaque gate de l'équipe.
- Les essais tournaient 4 par 4 en parallèle : le temps mur peut en souffrir un peu, des deux côtés.
