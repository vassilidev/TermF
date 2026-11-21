# Termf — Plan d'architecture

> v0.1 — 2026-11-21 — document de conception. Rien n'est encore implémenté. « Termf » est un nom provisoire.

---

## 0. En bref

- **Termf** est un poste de pilotage local pour agents CLI (Claude Code d'abord, puis Codex, Gemini et shell) répartis sur N dépôts, avec une équipe d'agents par rôle, des tickets reliés, un accès prod sécurisé et un petit IDE.
- **Stack** : cœur en Rust + fenêtre Tauri 2 (webview du système, pas de Chromium embarqué) + UI Svelte 5 + xterm.js pour les terminaux + CodeMirror 6 pour l'éditeur. **Pas d'Electron.**
- **C'est Termf qui orchestre, pas un LLM.** Le « chef d'orchestre IA » ne fait que découper et planifier. Les agents ne se parlent pas directement : ils passent par Termf (journal du run + outils MCP). Tout est donc visible, interruptible et chiffré en coût.
- **Une équipe éphémère par ticket et une session neuve par étape**, avec un brief de passation. Le contexte reste petit par construction : c'est un `/clear` garanti. Ton astuce du prénom est intégrée et vérifiée automatiquement (le « canari »).
- **Isolation** : un worktree git par run et par dépôt, avec des ports et des noms docker alloués par run. Plusieurs tickets peuvent tourner en parallèle sans se marcher dessus.
- **Workflows et règles à la carte** : un workflow générique, adapté projet par projet, choisi par projet et par ticket. Des **règles activables par projet, par ticket ou par tâche**, chacune avec un niveau (`advise` / `warn` / `block`). Exemples livrés : périmètre strict (en cas de doute, on s'arrête et on demande), aucune modification textuelle non demandée (contrôlée sur le diff), revue visuelle avant/après. Tu peux en écrire d'autres.
- **Prod** : coffre chiffré, recettes en lecture seule, approbation humaine, masquage automatique des secrets et journal d'audit chaîné. L'IA n'a jamais les identifiants.
- **Configuration** en fichiers texte (TOML + Markdown), en couches global → projet → dépôt → run. L'export et l'import tiennent en un seul fichier, avec les secrets exclus ou chiffrés. Une synchro chiffrée par mot de passe, via le remote de ton choix (git, S3, WebDAV…), rend même l'export inutile.

---

## 1. Principes directeurs

1. **Local-first.** Aucun serveur Termf. Les seules sorties réseau sont les API de tickets, les fournisseurs LLM (via les CLI) et la prod (à la demande).
2. **Sobre.** 0 % de CPU au repos, aucun processus résiduel une fois un ticket terminé, aucun rendu pour un terminal non affiché.
3. **Visible.** Tout ce que fait un agent est observable : statut, contexte, coût, diff, questions, journal.
4. **Contrôlé.** L'humain valide aux points clés (plan, visuel, prod, PR). Par défaut, dans le doute, on s'arrête et on demande.
5. **Agnostique.** Claude Code est le premier adaptateur, pas une dépendance du cœur.
6. **Configuration as code.** Tout est en fichiers texte versionnables, comparables et portables.
7. **Sécurisé par défaut.** Moindre privilège pour les agents. Les secrets ne sont jamais en clair sur disque ni dans le contexte d'un LLM.

---

## 2. Choix techniques

### 2.1 Stack retenue

| Couche | Choix | Pourquoi |
|---|---|---|
| Cœur | **Rust + tokio** | Performance, sûreté mémoire (manipulation de secrets), meilleur écosystème terminal |
| Fenêtre | **Tauri 2** | Webview du système (WKWebView / WebView2 / WebKitGTK), binaire d'environ 10-15 Mo, RAM 3 à 5 fois plus basse qu'Electron, modèle de permissions strict (capabilities) |
| UI | **Svelte 5 + TypeScript + Vite** | Pas de DOM virtuel, bundle minuscule, réactivité fine (runes) |
| Terminal (rendu) | **xterm.js 5** + addon WebGL (repli sur canvas) | Standard de fait (VS Code) |
| PTY | **`portable-pty`** (issu de WezTerm) | ConPTY sous Windows, PTY Unix ailleurs |
| Éditeur | **CodeMirror 6** | Environ 10 fois plus léger que Monaco, modulaire |
| Fichiers | **`ignore`** + **`nucleo`** (fuzzy finder de Helix) + **`grep-searcher`** (ripgrep) | Respecte `.gitignore`, très rapide |
| Git | **`gix`** en lecture + CLI `git` en écriture | Status et diff rapides, CLI pour la robustesse des worktrees |
| Stockage | **SQLite** (`rusqlite`, mode WAL) | Runs, événements, coûts, audit |
| Config | **TOML** (`toml_edit`, qui préserve les commentaires) + **Markdown à frontmatter** | Lisible et comparable, format compatible Claude Code |
| Gabarits de prompts | **MiniJinja** | Léger, syntaxe Jinja |
| Analyse de diff | **tree-sitter** | Extraction des textes modifiés (§7.5) |
| Captures UI | **CDP via `chromiumoxide`** sur Chrome/Chromium local (Chrome for Testing téléchargé à la demande) | Sans dépendance Node |
| Crypto | **`age`**, **`keyring`**, **`secrecy`/`zeroize`**, Argon2id | Coffre, export chiffré, effacement mémoire |
| SQL | **`sqlparser`** | Vérification lecture seule |

### 2.2 Alternatives écartées

- **Electron** : refusé (RAM, Chromium embarqué, énergie).
- **Go + Wails** : viable (même modèle de webview), mais l'écosystème PTY/ConPTY y est moins mûr, il n'a pas d'équivalent aux capabilities de Tauri, et le GC rend l'effacement des secrets en mémoire moins maîtrisable.
- **UI 100 % native sur GPU** (gpui de Zed, iced, Slint) : la plus sobre en théorie, mais il faudrait réécrire un émulateur de terminal, un éditeur et toute l'UI riche, soit environ 3 fois plus de délai. **L'option reste ouverte** : le cœur Rust ne dépend pas de l'UI (§3), donc une UI native pourra remplacer la webview plus tard sans toucher au cœur.
- **Monaco** : trop lourd pour un « notepad propre ».

### 2.3 Points de vigilance par plateforme

- **Linux** : WebKitGTK est moins rapide et le WebGL de xterm y est parfois instable, donc repli automatique sur le rendu canvas. Dépendance à `libwebkit2gtk-4.1`. Si aucun Secret Service n'est disponible, on bascule sur le coffre fichier.
- **Windows** : ConPTY, named pipes, chemins longs. Claude Code tourne en natif ou dans WSL : WSL est une « cible d'exécution » à part entière, Termf pilote l'agent via `wsl.exe` dans la distribution choisie.
- **macOS** : notarisation, Keychain, Touch ID pour déverrouiller le coffre.

### 2.4 Budgets de performance (vérifiés en CI)

| Mesure | Cible |
|---|---|
| Démarrage à froid jusqu'à une fenêtre utilisable | < 400 ms |
| RAM au repos (fenêtre ouverte, 0 agent) | < 120 Mo |
| RAM par session de terminal masquée | < 3 Mo (scrollback de 10 000 lignes) |
| CPU au repos | ≈ 0 % (aucun timer actif) |
| Latence frappe → écho | < 16 ms |
| 30 sessions actives dont 4 visibles | 60 fps |

---

## 3. Architecture globale

### 3.1 Processus

```
┌──────────────── Termf.app — un seul processus, résident dans la barre système ────────────────┐
│  UI (webview Svelte)   ⇅  IPC Tauri (commandes + événements, liste blanche)                     │
│ ─────────────────────────────────────────────────────────────────────────────────────────────── │
│  termf-core (Rust, tokio)                                                                       │
│   ├─ Registre (projets, dépôts)            ├─ Moteur de workflow + ordonnanceur                │
│   ├─ Sessions (PTY / headless)             ├─ Orchestrateur (ticket → plan → DAG)              │
│   ├─ Adaptateurs d'agents                  ├─ Moteur de règles (périmètre, texte, secrets…)    │
│   ├─ Connecteurs (Jira, GitHub…)           ├─ Coffre + Ops prod + Audit                        │
│   ├─ Espace de travail (worktrees, ports)  └─ Store SQLite + bus d'événements                  │
│   └─ API locale : socket Unix / named pipe, JSON-RPC 2.0 ───────────────┐                       │
└──────────────────────────────────────────────────────────────────────────┼──────────────────────┘
          ┌──────────────────────────────┬──────────────────────────────────┼───────────────────┐
   `termf` (CLI, depuis n'importe   `termf hook <event>`             `termf mcp` (stdio)    Chromium headless
    quel terminal)                  appelé par les hooks             serveur MCP exposé     (captures avant /
                                    de Claude Code                   aux agents             après)

   Processus enfants : claude / codex / gemini / shells — en PTY (interactif) ou en headless stream-json
```

**Décision** : un seul processus hôte en v1. Fermer la fenêtre ne tue rien : l'app reste dans la barre système. Le cœur est une bibliothèque, ce qui permettra d'en extraire plus tard un démon `termfd` headless (serveur distant, SSH) sans réécriture. Si l'app plante, les sessions Claude sont reprises via `claude --resume <session-id>` (les IDs sont persistés en base).

### 3.2 Bus d'événements

Chaque changement d'état est un événement typé (`SessionOutput`, `AgentStatusChanged`, `StepCompleted`, `QuestionAsked`, `ApprovalRequested`, `ContextUsage`, `GuardFailed`…). Il est publié sur un bus (`tokio::sync::broadcast`), persisté en SQLite (sauf la sortie brute des terminaux) et poussé à l'UI et aux CLI abonnées. L'UI n'est qu'une vue du bus : `termf watch` montre exactement la même chose dans n'importe quel terminal.

### 3.3 Structure du dépôt

```
termf/
├─ Cargo.toml                 # workspace
├─ crates/
│  ├─ termf-core/             # domaine, store, bus, workflow, ordonnanceur, orchestrateur, règles
│  ├─ termf-term/             # PTY, ring buffers, détection d'état
│  ├─ termf-agents/           # adaptateurs CLI (claude-code, codex, gemini, shell)
│  ├─ termf-connectors/       # Jira, GitHub (puis Linear, GitLab)
│  ├─ termf-vault/            # coffre, crypto, ops prod, audit
│  └─ termf-cli/              # binaire `termf` (CLI + hook + mcp)
├─ app/
│  ├─ src-tauri/              # hôte Tauri : commandes, tray, raccourcis globaux
│  └─ src/                    # UI SvelteKit (SPA) + Svelte 5
├─ library/                   # contenu livré : rôles, workflows, skills, préférences, recettes
└─ docs/
```

On démarre avec ces 6 crates et on ne découpe davantage que lorsqu'un besoin réel apparaît.

---

## 4. Modèle de domaine

```
Projet « Acme »  (1..N dépôts = le « même univers »)
 ├─ Dépôts : acme-api, acme-front, acme-infra
 ├─ Trackers : Jira ACME, GitHub Project #4
 ├─ Workflow (hérite du global, surchargeable)
 ├─ Bibliothèque activée (skills, commandes, rôles, préférences)
 └─ Connexions prod (via le coffre)

Ticket ──lance──▶ Run ──contient──▶ Étapes (DAG) ──exécutées par──▶ Sessions (agent × rôle × dépôt)
                   │                                                    └─ PTY ou headless, un session-id Claude
                   ├─ Worktrees (un par dépôt touché)
                   ├─ Journal (markdown + événements)
                   ├─ Revues (plan, visuelle, diff, sécurité)
                   └─ Coûts / tokens / durée
```

| Entité | Rôle | Où elle vit |
|---|---|---|
| **Projet** | Groupe de 1..N dépôts d'un même produit | `config/projects/<id>.toml` |
| **Dépôt** | Un dépôt git local | Déclaré dans le projet ; `.termf/` optionnel dans le dépôt |
| **Rôle** | Gabarit d'agent : prompt, modèle, effort, outils, périmètre | `library/roles/*.md` |
| **Workflow** | DAG d'étapes typées + politique | `library/workflows/*.toml` |
| **Run** | Exécution d'un ticket | SQLite + `runs/<id>/` |
| **Session** | Un processus agent | SQLite (session-id, pid, état, usage) |
| **Connexion** | Accès prod (sans secret) | Config + coffre |
| **Recette** | Opération prod paramétrée et bornée | `library/recipes/*.toml` |

Un projet à un seul dépôt n'est qu'un cas particulier : il n'y a qu'un seul concept.

---

## 5. Configuration, couches, import / export

### 5.1 Arborescence

Les chemins sont résolus par la crate `directories` : `~/.config/termf` sous Linux, `~/Library/Application Support/termf` sous macOS, `%APPDATA%\termf` sous Windows. Les **données** (SQLite, runs, worktrees, logs) vivent dans un dossier séparé, non exporté.

```
config/
├─ termf.toml                  # app : thème, raccourcis, limites, prénom (canari)
├─ projects/
│  └─ acme.toml
├─ library/
│  ├─ roles/        orchestrator.md po.md front.md back.md devops.md secops.md
│  ├─ workflows/    default.toml hotfix.toml spike.toml
│  ├─ skills/       <nom>/SKILL.md          # format Claude Code
│  ├─ commands/     <nom>.md                # format Claude Code
│  ├─ rules/        scope-strict/ no-text-changes/ visual-review/ tests-required/ …
│  ├─ preferences/  conventional-commits.md front-conventions.md …
│  └─ recipes/      logs.toml sql-readonly.toml env-pull.toml
└─ vault.age                   # coffre chiffré, si le trousseau OS n'est pas utilisé

<dépôt>/.termf/                # optionnel, versionné avec le dépôt, partageable avec l'équipe
├─ project.toml
└─ library/ (mêmes sous-dossiers)
```

### 5.2 Résolution par couches

`défauts livrés → global → projet → dépôt (.termf/) → run (surcharges ponctuelles UI/CLI)`

- Valeurs simples : la couche la plus spécifique gagne.
- Activations : on bascule avec `+nom` / `-nom`.
- Éléments de bibliothèque de même nom : le plus spécifique gagne.
- `termf config explain <clé>` indique quelle couche fournit la valeur (comme `git config --show-origin`).

```toml
# config/projects/acme.toml
id = "acme"
name = "Acme"

[[repos]]
id = "api"
path = "~/Projects/acme-api"
remote = "git@github.com:acme/acme-api.git"
roles = ["back", "devops", "secops"]
setup = "pnpm install --frozen-lockfile"
checks = "pnpm lint && pnpm test"

[[repos]]
id = "front"
path = "~/Projects/acme-front"
remote = "git@github.com:acme/acme-front.git"
roles = ["front", "secops"]
setup = "pnpm install --frozen-lockfile"
checks = "pnpm lint && pnpm test && pnpm build"
dev = { cmd = "pnpm dev --port $PORT", ready = "http://localhost:$PORT" }
visual = { routes = ["/", "/login", "/dashboard"], login = "recipes/front-test-login.toml" }

[trackers.jira]
site = "acme.atlassian.net"
project = "ACME"
auth = "vault:jira-acme"

[trackers.github]
repo = "acme/acme-front"
project = 4
auth = "gh"

[workflows]
default = "default"
by_ticket_type = { Bug = "hotfix", Spike = "spike" }

[rules]
scope-strict = "block"
no-text-changes = "block"
visual-review = "block"
tests-required = "warn"

[library]
preferences = ["+front-conventions"]
skills = ["+tailwind-conventions"]

[models]
front = { model = "sonnet", effort = "high" }
```

### 5.3 Export / import

- `termf config export [--project acme] [--with-secrets] -o sauvegarde.termf` produit une archive `tar.zst` signée (config + bibliothèque). Avec `--with-secrets`, le coffre est ajouté, chiffré `age` par une phrase de passe dédiée. **Sans ce drapeau, aucun secret n'est inclus.**
- Les chemins des dépôts sont rendus portables (`~/…` + `remote`) : à l'import sur une autre machine, Termf propose de cloner les dépôts manquants.
- **Import** : aperçu en diff (ajouts, modifications, conflits), puis choix « fusionner » ou « remplacer » élément par élément.
- **Import depuis Claude Code** : `~/.claude/skills`, `~/.claude/agents`, `~/.claude/commands` et le `.claude/` d'un dépôt sont importés dans la bibliothèque.

### 5.4 Synchronisation chiffrée entre machines (sans export / import)

Même config sur toutes tes machines, synchronisée automatiquement via un remote de ton choix et chiffrée de bout en bout avec un mot de passe. Le remote ne voit que des données illisibles.

- **Remotes interchangeables** (un seul à la fois, indépendant de tout service TermF) : dépôt git (n'importe quel remote privé), stockage S3 compatible (AWS, R2, MinIO), WebDAV (Nextcloud), simple dossier synchronisé (iCloud Drive, Dropbox, Syncthing) ou URL HTTPS (PUT/GET).
- **Chiffrement côté client** :
  - le mot de passe de synchro est dérivé par Argon2id en une clé, puis chaque fichier est chiffré avec `age` (XChaCha20-Poly1305) ;
  - les noms de fichiers sont eux aussi masqués (chemins hachés) ;
  - le mot de passe n'est jamais envoyé : la clé dérivée est gardée dans le trousseau de l'OS de chaque machine, ce qui veut dire qu'on ne le saisit qu'une fois par machine.
- **Clé de récupération** : elle est affichée une fois, à l'activation, pour être imprimée ou rangée. Sans elle, un mot de passe perdu rend les données irrécupérables.
- **Périmètres activables un par un** :
  - config et bibliothèque ;
  - projets ;
  - conventions ;
  - coffre (secrets, `.env`, connexions) ;
  - clés SSH (§12.7), en option explicite.
- **Ce qui reste local** : les chemins des dépôts diffèrent d'une machine à l'autre. Un projet est synchronisé avec le `remote` de chaque dépôt, et chaque machine garde sa propre correspondance remote → chemin dans `local.toml` (jamais synchronisé). Si un dépôt manque, TermF propose de le cloner.
- **Mécanique** :
  - **push** automatique après chaque modification (regroupé sur 5 s) ;
  - **pull** au démarrage, au retour sur la fenêtre et toutes les 5 minutes tant que l'app est ouverte ;
  - fusion à 3 voies par fichier à partir de la dernière version synchronisée (au niveau de la clé pour le TOML) ;
  - un vrai conflit affiche les deux versions côte à côte pour choisir.
- **Appareils** : la liste des machines (nom, dernière synchro) est consultable. Révoquer une machine revient à changer le mot de passe : tout est rechiffré, et l'ancienne machine ne peut plus rien déchiffrer.
- **CLI** : `termf sync setup | status | push | pull`.

---

## 6. Couche agents

### 6.1 Contrat d'adaptateur

```rust
trait AgentRuntime {
    fn id(&self) -> &'static str;                          // "claude-code", "codex", "gemini", "shell"
    fn detect(&self) -> Result<RuntimeInfo>;               // binaire, version, authentification
    fn capabilities(&self) -> Caps;                        // interactive, headless, resume, model, effort, hooks, plugins, budget
    fn spawn(&self, spec: &SessionSpec) -> Result<Spawned>;
    fn parse_event(&self, line: &[u8]) -> Option<AgentEvent>;
}
```

`SessionSpec` regroupe : rôle, worktree, mode (interactif ou headless), modèle, effort, prompt système ajouté, éléments de bibliothèque activés, permissions, serveurs MCP, budget et session-id.

### 6.2 Adaptateur Claude Code (options vérifiées sur Claude Code 2.1.330)

| Besoin de Termf | Mécanisme Claude Code |
|---|---|
| Modèle par rôle | `--model` (alias `fable` / `opus` / `sonnet` / `haiku`, ou ID complet) + `--fallback-model` |
| Raisonnement par rôle | `--effort low\|medium\|high\|xhigh\|max` |
| Session identifiée d'avance | `--session-id <uuid>` généré par Termf (pour retrouver le transcript) + `-n <nom>` |
| Rôle + préférences | `--append-system-prompt-file <fichier généré>` |
| Skills, commandes, sous-agents, hooks et MCP activés | **plugin éphémère généré pour chaque session** → `--plugin-dir runs/<id>/sessions/<sid>/plugin`. Rien n'est écrit dans le dépôt. |
| Permissions, liste d'interdits (`.env`, clés) | `--settings <json>` (`permissions.allow/deny`), `--permission-mode`, `--allowedTools` / `--disallowedTools` |
| Outils Termf exposés à l'agent | `--mcp-config` → `termf mcp` (+ `--strict-mcp-config` pour les rôles sensibles) |
| Pilotage structuré | `-p --input-format stream-json --output-format stream-json --include-hook-events` |
| Plan de l'orchestrateur en JSON validé | `--json-schema` |
| Plafond de coût par étape | `--max-budget-usd` |
| Questions de permission remontées dans l'UI | `--permission-prompts host` + outil de permission du MCP Termf |
| Reprise après plantage, ou prise de main | `--resume <session-id>` dans un PTY |
| Isolation de la config utilisateur (rôles déterministes) | `--setting-sources`, voire `--bare` |
| Contrôle à distance depuis le web ou le mobile | `--remote-control [nom]` + `--remote-control-session-name-prefix` (§6.6) |

L'adaptateur lit `claude --version` au démarrage. Un **test de contrat** en CI compare la sortie de `claude --help` à un instantané, car ces options évoluent vite.

### 6.3 Deux modes, une seule session

- **Headless** (par défaut pour les runs orchestrés) : des événements structurés alimentent une vue « conversation » propre (messages, appels d'outils, diffs), avec l'usage exact en tokens par tour, le coût et le statut, sans coût de rendu de terminal.
- **Interactif** (par défaut pour les sessions manuelles « en masse ») : la vraie TUI de Claude dans un PTY.
- **Prendre la main** : à la fin d'un tour, Termf arrête le processus headless et rouvre `claude --resume <id>` dans un PTY. Tu discutes directement avec l'agent. Avec « Rendre la main », Termf reprend en headless. Le session-id et le contexte restent les mêmes.

### 6.4 Détection d'état (le badge qui compte)

États : `démarrage · travaille · attend ta réponse · attend une approbation · inactif · terminé · erreur · budget atteint`.

Sources, de la plus fiable à la moins fiable :

1. Événements stream-json (headless).
2. **Hooks Claude Code injectés par le plugin éphémère** (`UserPromptSubmit`, `PreToolUse`, `Notification`, `Stop`, `SubagentStop`, `PreCompact`, `SessionEnd`) qui appellent `termf hook`, lequel écrit sur la socket.
3. Heuristique sur la sortie du terminal pour les CLI génériques (Codex, shell).

Une notification système n'est émise que pour `attend *`, `terminé` et `erreur`, pas pour le reste.

### 6.5 Contexte : mesure, canari, remise à zéro

- **Mesure exacte** : l'usage en tokens du dernier tour (champ `usage` du stream-json en headless ; en interactif, `context_window.used_percentage` et `context_window_size` du JSON de la statusline, que TermF reçoit via `termf statusline` injecté par `--settings`, en enchaînant la statusline de l'utilisateur) alimente une jauge en % de la fenêtre du modèle, pour chaque session. Le transcript JSONL n'est pas lu (format interne).
- **Canari (ton astuce)** : la préférence `canary` ajoute au prompt système « commence chaque réponse par {{ user.first_name }} ». Termf vérifie automatiquement chaque réponse. Si le prénom manque, la session reçoit le badge « contexte dégradé » et la passation est déclenchée. C'est un signal qualitatif qui complète la jauge quantitative.
- **Politique par défaut** :
  - **Équipe éphémère par run** : les sessions sont créées au lancement du ticket et détruites à la fin. Elles ne sont jamais réutilisées d'un ticket à l'autre, ce qui équivaut à un `/clear` garanti.
  - **Session neuve par étape** : chaque étape démarre une nouvelle session avec un **brief de passation** tiré du journal, pas avec la conversation précédente.
  - **Seuils** : avertissement à 50 %. À 70 %, ou si le canari disparaît, l'agent écrit `handoff.md` (état, décisions, reste à faire), la session est fermée puis relancée sur ce brief. Les seuils se règlent par rôle.
  - **Sessions manuelles** : le bouton « Remise à zéro » envoie `/clear` dans le PTY, ou tue et relance la session.
  - **Remise à zéro automatique avec reprise (obligatoire, pour toute session Claude lancée par TermF, manuelle ou non)** : au-delà du seuil, à la fin d'un tour, TermF fait écrire à l'agent sa passation (état, décisions, reste à faire), envoie `/clear`, puis le relance sur cette passation sans intervention humaine. Activée par défaut, seuil et désactivation réglables. Le bouton « Remise à zéro » déclenche la même séquence à la demande.

### 6.6 Remote Control

Claude Code sait exposer une session locale pour la continuer depuis claude.ai ou l'app mobile (Remote Control). TermF le rend accessible en un clic.

- **Interrupteur 📱 sur chaque session**, avec un réglage par défaut par projet (`remote_control = "off" | "ask" | "on"`) et par rôle dans les workflows (par exemple : seulement le chef d'orchestre).
- **Noms lisibles** : chaque session est nommée `<projet>/<ticket>/<rôle>`, pour la retrouver tout de suite sur le téléphone.
- **Activation à chaud** : si la version de Claude Code propose une commande dans la session pour l'activer, TermF l'utilise. Sinon, il relance la session en fin de tour avec `--resume <id> --remote-control <nom>` : même session, même contexte.
- **Accès depuis le téléphone** : TermF affiche un QR code ou un lien vers la session. Mission Control a une liste « Accessibles à distance » et un bouton « Tout couper ».
- **Sécurité** : Remote Control est désactivé et verrouillé pour toute session qui a accès à la prod (règle « trifecta », §12.4). Un indicateur reste visible tant qu'une session est contrôlable à distance.

---

## 7. Orchestration

### 7.1 Qui orchestre ?

**Termf, en code déterministe**, est le chef d'orchestre : machine à états, ordonnancement, gates, budgets. L'**agent « chef d'orchestre »** (LLM) a un rôle étroit : lire le ticket et le contexte du projet, produire un **plan JSON validé par schéma**, puis re-planifier si une étape échoue ou si une question bloque. Il ne lance pas d'agents lui-même (pas d'outil `Task` imbriqué) : tout reste visible, interruptible, et le coût est suivi rôle par rôle.

Les agents **ne se parlent pas directement**. Ils communiquent par le **tableau noir du run** (journal, artefacts, rapports) et par l'outil `ask_role`, que Termf route. Par exemple, le front demande au back « quel est le format de `/users` ? ». Termf pose la question à une session ponctuelle du rôle back sur le dépôt api, puis consigne la réponse dans le journal.

### 7.2 Outils MCP exposés aux agents (`termf mcp`)

| Outil | Effet |
|---|---|
| `ask_user(question, options?)` | Met le run en « attend ta réponse » et bloque jusqu'à la réponse |
| `ask_role(role, question)` | Question routée vers un autre rôle, réponse consignée au journal |
| `read_ticket()` / `read_journal()` | Contexte du run (le ticket est marqué comme donnée non fiable) |
| `report({...})` | **Obligatoire pour terminer une étape** : statut, résumé, critères couverts, `text_changes`, `affected_views`, constats hors périmètre |
| `ops.run(recipe, params)` | Seulement si le rôle y a droit ; toujours soumis à approbation (§12) |

### 7.3 Rôles livrés

| Rôle | Mission | Modèle | Effort | Accès |
|---|---|---|---|---|
| **Chef d'orchestre** | Découpe, plan, arbitrage, re-planification | Opus 5.5 | high | Lecture seule sur tous les dépôts |
| **PO / PM** | Lecture et enrichissement du ticket, critères d'acceptation, specs, questions de clarification, mise à jour du ticket | Opus 5.5 | medium | Lecture des dépôts ; écriture sur le tracker via gate |
| **Dev Front** | UI, UX, composants, accessibilité, vues affectées | Sonnet 5.5 | high | Écriture dans le périmètre front |
| **Dev Back** | API, base de données, migrations, tests | Sonnet 5.5 | high | Écriture dans le périmètre back |
| **DevOps** | CI/CD, infra as code, dépendances, spec technique | Sonnet 5.5 | medium | Écriture des fichiers CI et infra |
| **SecOps / Pentest** | Revue sécurité du diff, recherche de failles, tests d'abus, dépendances vulnérables | Opus 5.5 | xhigh | Lecture + tests en bac à sable ; pas d'accès prod |
| **Utilitaires** | Résumés, messages de commit, passation, triage | Haiku 5.5 | low | Aucun outil d'écriture |
| **Escalade** | Re-planification après 2 échecs, ou à la demande | Fable 5.2 | high | Comme le rôle escaladé |

- La config utilise des alias (`opus`, `sonnet`…) pour suivre automatiquement les derniers modèles.
- Prix API indicatifs par MTok (entrée / sortie) : Haiku 5.5 $0,10 / $0,50 · Sonnet 5.5 $2 / $10 · Opus 5.5 $4 / $20 · Fable 5.2 $8 / $40. Avec un abonnement Claude, ce sont les quotas qui comptent.
- **Effort automatique** : chaque tâche du plan a une taille `S`, `M` ou `L`, qui donne l'effort `medium`, `high` ou `xhigh`, borné par la plage autorisée du rôle.

Format d'un rôle (Markdown + frontmatter, compatible avec le format d'agent Claude Code) :

```markdown
---
name: front
description: Développeur front-end (UI, UX, accessibilité)
runtime: claude-code
model: sonnet
effort: high
effort_range: [medium, xhigh]
permission_mode: acceptEdits
deny: ["Read(**/.env*)", "Bash(git push*)"]
skills: [ui-review, a11y-check]
preferences: [front-conventions]
writes: ["apps/web/**", "packages/ui/**"]
---
Tu es le développeur front de l'équipe. …
```

### 7.4 Déroulé d'un ticket

```
Coller une URL ou une clé (ou « Lancer » depuis l'inbox, ou `termf run ACME-123`)
  │
  ▼ 1. Résolution   le connecteur renvoie un ticket normalisé (titre, description, critères, PJ, liens, commentaires)
  ▼ 2. Cadrage (PO) critères d'acceptation explicites + liste « hors périmètre » + liste de doutes
  │                  └─ un doute ? → STOP : questions à l'humain (ou en commentaire du ticket), le run attend
  ▼ 3. Plan (chef)  JSON validé : tâches {rôle, dépôt, dépendances, critères couverts, fichiers pressentis, taille}
  │                  └─ GATE humaine « plan » (désactivable dans le workflow)
  ▼ 4. Exécution    DAG, en parallèle là où c'est indépendant (front ∥ back), worktrees isolés
  ▼ 5. Contrôles    lint / tests / build → règles actives (périmètre, texte, secrets…) → revue SecOps du diff
  ▼ 6. Visuel       si la règle visual-review est active et que l'UI a changé : avant / après → GATE « visuel »
  ▼ 7. Livraison    commits, branches, PR liées entre elles, ticket mis à jour (lien PR, transition)
  ▼ 8. Clôture      résumé, coûts, journal joint ; équipe détruite ; worktrees nettoyés après merge
```

Chaque « demander des modifications » renvoie au rôle responsable une **nouvelle session** avec le brief et le retour, pas l'ancien contexte complet.

### 7.5 Workflows : un générique, des variantes, choisis par projet et par ticket

Un workflow est un DAG d'étapes typées (`agent`, `shell`, `gate`, `rules`, `visual`, `connector`, `foreach`) décrit dans `library/workflows/*.toml`. Les étapes sont indexées par `id`, si bien qu'une variante n'écrit que ce qui change (`extends`). Une étape désactivée est transparente : les étapes qui en dépendent héritent de ses propres dépendances.

```toml
# library/workflows/default.toml
name = "default"
description = "Cadrage → plan → dev → contrôles → revue → PR"
max_parallel_agents = 4
budget_usd = 15

[steps.frame]
type = "agent"
role = "po"
prompt = "prompts/frame.md"

[steps.plan]
type = "agent"
role = "orchestrator"
needs = ["frame"]

[steps.approve-plan]
type = "gate"
needs = ["plan"]

[steps.implement]
type = "foreach"
over = "plan.tasks"
needs = ["approve-plan"]
step = { type = "agent", role = "{{ task.role }}", repo = "{{ task.repo }}", prompt = "prompts/implement.md" }

[steps.checks]
type = "shell"
needs = ["implement"]
run = "{{ repo.checks }}"

[steps.rules]
type = "rules"
needs = ["checks"]

[steps.security]
type = "agent"
role = "secops"
needs = ["rules"]

[steps.visual]
type = "visual"
needs = ["rules"]

[steps.deliver]
type = "connector"
needs = ["security", "visual"]
actions = ["commit", "pull_request", "ticket.link_pr", "ticket.transition:In Review"]
```

Une variante livrée, puis une adaptation propre à un dépôt :

```toml
# library/workflows/hotfix.toml
extends = "default"
description = "Correctif urgent : pas de gate plan, priorité haute"
priority = "high"

[steps.approve-plan]
enabled = false
```

```toml
# <dépôt>/.termf/workflows/default.toml
extends = "default"
max_parallel_agents = 2

[steps.checks]
run = "make ci"
```

**Quel workflow pour quel ticket** : le projet fixe un workflow par défaut et peut en associer un à chaque type de ticket (`[workflows]`, §5.2). Au lancement, « Lancer avec… » ou `termf run ACME-123 --workflow spike` l'emporte.

### 7.6 Règles de workflow, activables à la carte

Les règles livrées ci-dessous sont des **exemples**. Tu peux en écrire d'autres, et chacune s'active, se règle ou se coupe **par projet, par ticket et par tâche**.

**Anatomie d'une règle** (`library/rules/<nom>/rule.md`) :

- des **consignes**, injectées dans le prompt des rôles visés (`applies_to`) ;
- un **contrôle automatique** optionnel (`check`) : intégré (`scope`, `text-changes`, `secrets`, `ui-changed`), ou script maison qui reçoit le diff en JSON sur stdin et renvoie ses constats ;
- une **gate** optionnelle (`gate`), par exemple la revue visuelle ;
- un **niveau** par défaut.

| Niveau | Effet |
|---|---|
| `off` | Règle ignorée |
| `advise` | Consignes injectées dans le prompt, aucun contrôle |
| `warn` | Consignes + contrôle ; les écarts sont signalés dans la revue sans bloquer |
| `block` | Consignes + contrôle ; un écart fait échouer l'étape (une correction automatique, puis gate humaine) |

**Activation**, du plus général au plus précis (le plus précis l'emporte) :

`défaut de la règle → global → workflow → projet → dépôt → ticket → tâche`

- **Ticket** : interrupteurs dans « Lancer avec… », ou `termf run ACME-123 --rule visual-review=off --rule tests-required=block`.
- **Tâche** : à la gate « plan », chaque tâche a ses propres interrupteurs (par exemple, revue visuelle seulement sur T2).
- Le run affiche toujours les règles actives et leur niveau, pour qu'on sache ce qui s'applique.

```markdown
---
name: scope-strict
description: Ne faire que ce que le ticket demande ; dans le doute, s'arrêter et demander
applies_to: [all]
check: scope
level: block
---
- Fais uniquement ce que le ticket et ses critères d'acceptation demandent. Aucun refactor, renommage, nettoyage ou amélioration « au passage ».
- Si une consigne est ambiguë, incomplète ou contradictoire, ou si tu hésites entre deux interprétations : ne modifie rien, appelle `ask_user` avec ta question et les options possibles, puis arrête-toi.
- Un problème repéré hors périmètre se signale dans ton rapport (section « hors périmètre ») ; il ne se corrige pas.
```

```markdown
---
name: no-text-changes
description: Toute modification textuelle non demandée est interdite
applies_to: [front, back, devops]
check: text-changes
level: block
---
- Ne modifie aucun texte visible ou éditorial (libellés, messages d'erreur, traductions, contenus, README, commentaires existants) si le ticket ne le demande pas explicitement.
- Si ta tâche exige un nouveau texte, reprends mot pour mot celui du ticket. S'il n'est pas fourni, demande-le via `ask_user`.
- Déclare chaque texte ajouté ou modifié dans `report.text_changes`, avec le critère qui le justifie.
```

```markdown
---
name: visual-review
description: Revue avant / après de chaque écran modifié, avant la PR
applies_to: [front]
check: ui-changed
gate: visual
level: block
---
- Déclare dans `report.affected_views` chaque écran et chaque état touchés par ta modification.
```

Exemple de règle maison :

```markdown
---
name: no-console-log
description: Pas de console.log dans le code livré
applies_to: [front, back]
check: { run: "check.sh" }
level: warn
---
- N'ajoute pas de `console.log` ; utilise le logger du projet.
```

Un script de contrôle est du code : celui d'une règle venue d'un dépôt ne s'exécute qu'après confirmation (confiance par dépôt, §12.6).

#### Les contrôles des règles livrées

**1. `scope-strict` : ne rien faire de plus que ce que le ticket demande** (contrôle `scope`)

- Le PO produit des critères d'acceptation explicites et une liste « hors périmètre ». Chaque tâche du plan référence les critères qu'elle couvre.
- Si le plan contient des `questions`, **le run ne passe pas à l'exécution** tant qu'elles n'ont pas de réponse.
- Pendant l'exécution, en cas de doute, l'agent appelle `ask_user` et s'arrête. Le run passe en « attend ta réponse », une notification part, et la réponse est consignée au journal (et, en option, publiée sur le ticket).
- **Contrôle `scope`** : tout fichier modifié hors des globs `writes` du rôle est un écart. Tout fichier non relié à une tâche ou hors des fichiers pressentis est signalé et doit être justifié dans le `report`. Un diff disproportionné par rapport à la taille estimée est signalé.
- Les problèmes repérés hors périmètre sont **signalés dans le rapport, jamais corrigés**.

**2. `no-text-changes` : aucune modification textuelle non demandée** (contrôle `text-changes`)

- Termf analyse le diff avec **tree-sitter** (TS/JS/JSX/TSX, Vue, Svelte, HTML, PHP, Python, Go, Rust…, avec un repli regex pour le reste) et en extrait chaque texte modifié : littéraux de chaîne, nœuds texte JSX/HTML, attributs `alt`, `title`, `placeholder` et `aria-label`, fichiers i18n (json, yaml, po, xliff), Markdown, messages d'erreur, ainsi que les commentaires (réglable).
- Chaque texte modifié doit être **justifié** : soit il apparaît tel quel dans le ticket ou les critères, soit l'agent l'a déclaré dans `report.text_changes` avec le critère qu'il sert. Même justifié, il est mis en évidence dans la revue.
- En niveau `block`, **un texte non déclaré fait échouer l'étape** : l'agent reçoit l'ordre de l'annuler (une seule boucle automatique), sinon le run passe en gate humaine. En `warn`, le texte est seulement signalé.
- Exclusions réglables : tests, snapshots, logs internes.

**3. `visual-review` : revue avant / après** (contrôle `ui-changed` + gate `visual`) : détaillée au §8.

---

## 8. Revue visuelle avant / après

Elle s'applique quand la règle `visual-review` est active (§7.6) et que l'UI a changé.

### 8.1 Mécanique

1. **Détection `ui_changed`** : le diff touche des fichiers front (globs par dépôt : `**/*.{tsx,jsx,vue,svelte,css,scss}`, templates, assets, i18n).
2. **Écrans à capturer**, en réunissant trois sources :
   - les routes clés du projet (`visual.routes`) ;
   - les vues déclarées par l'agent front (`report.affected_views` : URL, état, viewport) ;
   - les stories Storybook touchées, si le projet en a.
3. **Deux environnements** : « avant » est un worktree de la branche de base (mis en cache par commit), « après » est le worktree du run. Chacun a son serveur de dev sur un port alloué par Termf. La connexion passe par une recette dédiée, **sur un compte de test uniquement, jamais en prod**.
4. **Capture** : Chromium headless (CDP) en desktop, tablette et mobile, en clair et sombre si pertinent. Les animations sont coupées, les polices attendues, et les zones dynamiques (dates, avatars) peuvent être masquées.
5. **Diff** : comparaison pixel avec seuil, zones modifiées encadrées, plus la liste des **textes modifiés dans le DOM** (recoupée avec le contrôle `text-changes`).

### 8.2 Page de revue

```
┌─ Revue visuelle — ACME-123 · v2 ─────────────────────────────────────────────────────────┐
│ Écrans : [/dashboard ● desktop] [/dashboard mobile] [/login ✓]       Mode : ◧ Côte à côte │
│                                                                       ⇔ Curseur  ◫ Calque │
│ ┌──────────── AVANT ────────────┐ ┌──────────── APRÈS ───────────┐    ▦ Heatmap           │
│ │                               │ │          ┌─────┐ ①           │                        │
│ │                               │ │          │ CSV │             │ Textes modifiés (1)    │
│ │                               │ │          └─────┘             │  + « Exporter en CSV »  │
│ │                               │ │                              │    ↳ AC2 ✓ (ticket)    │
│ └───────────────────────────────┘ └──────────────────────────────┘                        │
│ ① « Bouton trop collé au filtre, 8px de plus »                    Fichiers : 3 · diff ▸    │
│                                                                                           │
│        [ ✓ Valider ]        [ ✎ Demander des modifications ]        [ ✕ Refuser ]          │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

- Quatre modes : **côte à côte**, **curseur glissant**, **calque** (superposition) et **heatmap** des différences.
- Dans chaque écran, un clic sur une zone pose une **épingle avec un commentaire** (comme dans Figma).
- À côté de l'image : le diff de code des fichiers liés et la liste des textes modifiés avec leur justification.
- Les trois décisions :
  - **Valider** : le run continue.
  - **Refuser** : le run s'arrête et le worktree est conservé.
  - **Demander des modifications** : les épingles, commentaires et recadrages partent vers l'agent front, qui reçoit une nouvelle session, puis une nouvelle capture donne une revue v2. L'historique v1, v2… est conservé.
- En option, les images avant/après sont jointes à la PR en commentaire.

---

## 9. Isolation et parallélisme

- **Worktrees** : `<data>/worktrees/<projet>/<run>/<dépôt>`, sur la branche `termf/<TICKET>-<slug>`, créés depuis la branche de base du dépôt. Plusieurs tickets en parallèle ont donc plusieurs worktrees, sans collision.
- **Deux rôles sur le même dépôt dans un même run** : si leurs fichiers pressentis se recoupent, les tâches sont **sérialisées**. Sinon, chacun a son worktree et une étape d'intégration fusionne (un conflit déclenche une gate).
- Termf gère lui-même les worktrees plutôt que `claude --worktree`, pour le multi-dépôt, le nettoyage et la base de la revue visuelle.
- **Dépendances** : la commande `setup` du dépôt tourne une fois par worktree. On profite des caches partagés (pnpm, uv, cargo) et, quand c'est possible, d'un clonage copy-on-write de `node_modules` (`clonefile` APFS, reflink btrfs).
- **Worktrees intelligents selon la stack (obligatoire)** : TermF détecte la stack de chaque dépôt (Node et son gestionnaire, PHP/Composer, Python, Ruby, Go, Rust, Docker Compose, base de données) et prépare le worktree en conséquence, réglable par dépôt :
  - **jamais de copie brute** des dossiers de dépendances (`node_modules`, `vendor`, `.venv`, `target`…) : clone copy-on-write quand le système de fichiers le permet (coût disque quasi nul), sinon installation depuis le cache partagé du gestionnaire (`pnpm install --frozen-lockfile`, `composer install`, `uv sync`…), sinon rien et l'agent installe ;
  - les fichiers ignorés mais nécessaires (`.env`, configs locales) sont copiés selon une liste par dépôt (même syntaxe que `.worktreeinclude` de Claude Code) ;
  - **base de données clonée par worktree** quand le projet en a une (détectée depuis `.env` / `DATABASE_URL` / la config du framework) : Postgres `CREATE DATABASE … TEMPLATE …`, MySQL/MariaDB dump + restauration, SQLite copie copy-on-write ; le `.env` du worktree pointe vers la copie, qui est supprimée à la fermeture du run ; jamais une base distante ou de prod ;
  - Docker Compose : `COMPOSE_PROJECT_NAME` et ports propres au run (allocateur ci-dessous).
- **Ports** : un allocateur par run (plage 41000-41999), injecté via l'environnement (`PORT`, `TERMF_PORT_<SERVICE>`). Docker Compose reçoit `COMPOSE_PROJECT_NAME=termf_<run>`.
- **Ordonnanceur** : limite globale d'agents simultanés (6 par défaut), limite par runtime (quotas d'abonnement), priorités (hotfix > normal), file visible. Une limite d'usage détectée met le run **en pause**, pas en échec.
- **Budgets** : par étape (`--max-budget-usd`), par run et par jour. Un **arrêt d'urgence** global (raccourci dédié) stoppe proprement tous les agents (SIGINT, puis SIGTERM).
- **Git** : les agents ne poussent jamais. Termf commit à la fin de chaque étape (message rédigé par Haiku, au format Conventional Commits), puis pousse et ouvre la PR à l'étape de livraison, après les gates.

---

## 10. Connecteurs de tickets

```rust
trait TicketProvider {
    fn resolve(&self, input: &str) -> Option<TicketRef>;   // URL, "ACME-123", "#42", "front#42"
    async fn fetch(&self, r: &TicketRef) -> Result<Ticket>;
    async fn search(&self, query: &str) -> Result<Vec<TicketSummary>>;
    async fn comment(&self, r: &TicketRef, body: &str) -> Result<()>;
    async fn transition(&self, r: &TicketRef, to: &str) -> Result<()>;
    async fn link_pr(&self, r: &TicketRef, url: &str) -> Result<()>;
}
```

| | Jira Cloud | GitHub Issues / Projects v2 |
|---|---|---|
| API | REST v3 (conversion ADF → Markdown) | REST + GraphQL (Projects v2) |
| Authentification | Jeton API ou OAuth 3LO, stocké au coffre | Réutilise `gh auth token`, sinon device flow ou PAT fine-grained |
| Inbox | JQL configurable (`assignee = currentUser() AND status = "To Do"`) | Requête Projects v2 (colonne, assignation) |
| Écriture | Commentaire, transition, lien PR | Commentaire, champ Status, lien PR |

- **Plusieurs façons de récupérer les tickets, par projet** (un projet peut en combiner plusieurs) :
  - **connecteur natif** : GitHub Issues et GitHub Projects v2 via `gh`, Jira Cloud via l'API REST (jeton au trousseau de l'OS) — rapide, sans coût de tokens ;
  - **serveur MCP** : n'importe quel serveur déjà configuré dans Claude Code (Atlassian, GitHub, Linear, etc.), interrogé en headless (`claude -p --json-schema`, petit modèle) avec **seulement ses outils de lecture** autorisés ; la sortie est validée par le schéma des tickets puis normalisée. Ça réutilise l'authentification déjà faite dans Claude Code et couvre les trackers sans connecteur natif. Résultats mis en cache, rafraîchis à la demande ;
  - plus tard, un client MCP direct dans TermF pour les serveurs à authentification statique.
- **Cliquer pour lancer** : chaque ticket de l'inbox d'un projet a un bouton « Lancer » qui enchaîne résolution → worktree → session de l'agent (§7.4), sans autre saisie.
- **Résolveur universel** : tout ce qui est collé (URL Jira, URL d'issue, `ACME-123`, `#42`) est rattaché au bon projet grâce aux trackers déclarés.
- **Synchro** : polling avec ETag, seulement quand l'inbox est visible ou qu'un run attend une réponse sur le ticket. Intervalle de 2 à 5 minutes avec backoff. Pas de webhooks (l'app est locale).
- **Le contenu d'un ticket n'est pas fiable** (injection de prompt) : il est encadré comme donnée dans les prompts, et l'écriture sur le tracker passe par une gate.
- Plus tard : Linear, GitLab, Azure DevOps.

---

## 11. Bibliothèque : skills, commandes, préférences

- **Types** : `skills/` (format `SKILL.md` de Claude Code), `commands/`, `roles/`, `preferences/` (fragments Markdown de style de code, sans contrôle), `rules/` (règles de workflow, §7.6), `workflows/`, `recipes/`.
- **Frontmatter commun** : `name`, `description`, `applies_to` (rôles), `tags`, `runtime`.
- **Matrice d'activation** dans l'UI : une ligne par élément, des colonnes global / projet / dépôt, des interrupteurs. Au lancement d'un run, « Lancer avec… » permet des bascules ponctuelles.
- **Matérialisation par session** :
  - Termf assemble le plugin éphémère (`plugin.json`, `skills/`, `commands/`, `agents/`, `hooks/hooks.json`, `.mcp.json`) et le passe via `--plugin-dir`.
  - Les préférences et les consignes des règles actives sont filtrées par rôle, ordonnées, puis passées via `--append-system-prompt-file`.
  - **Rien n'est écrit dans le dépôt**, et le `CLAUDE.md` / `.claude/` existant du dépôt reste respecté (réglable).
- **Autres CLI** : l'adaptateur traduit le contenu (`AGENTS.md`, `GEMINI.md` déposés dans le worktree et exclus via `.git/info/exclude`).
- Actions sur un élément : « promouvoir » (projet → global) et « dériver » (global → copie projet modifiable).

**Préférences livrées** (style, sans contrôle automatique) : `canary`, `conventional-commits`, `front-conventions`.

**Règles livrées** (§7.6) : `scope-strict`, `no-text-changes`, `visual-review`, `tests-required`, `no-new-deps-without-asking`.

### 11.1 Conventions de projet, assistées par IA

Tu décris tes conventions en langage naturel. L'IA les traduit en éléments concrets (règles, contrôles, skills, commandes, rôles, hooks), tu valides élément par élément, puis tu actives ou désactives à la demande.

**Déroulé**

1. **J'écris** : dans l'onglet « Conventions » du projet, une zone de texte libre, guidée par des exemples (« composants en PascalCase, Tailwind uniquement, pas de CSS inline, tests Vitest obligatoires pour toute logique, textes via i18n… »).
2. **L'IA me fait préciser** : si c'est ambigu ou incomplet, elle pose ses questions avant de proposer quoi que ce soit (règle `scope-strict`). Par exemple : « Les tests sont-ils obligatoires aussi pour les composants purement visuels ? ».
3. **L'IA propose une implémentation** : en headless (`claude -p --json-schema`, Opus, effort `high`), elle lit le ou les dépôts du projet pour s'ancrer dans l'existant (`package.json`, configs de lint et de format, `CLAUDE.md`, quelques fichiers représentatifs). Elle produit un **pack de conventions** :

   | Ce qu'elle peut générer | Exemple |
   |---|---|
   | Règle + contrôle automatique | `no-inline-css` en `block`, avec un script de contrôle sur le diff |
   | Config d'outil existant (diff proposé dans le dépôt) | Règle ESLint `react/forbid-dom-props: style` |
   | Préférence (consignes pour les agents) | « Utilise les classes Tailwind du design system » |
   | Skill | `create-component` : comment créer un composant selon les conventions |
   | Commande | `/new-component <Nom>` |
   | Ajustement de rôle | Le rôle front reçoit la préférence et la skill |
   | Hook | Formateur lancé après chaque édition (`PostToolUse`) |

   **Le plus déterministe possible** : une convention vérifiable par un outil du projet devient une règle de cet outil ; sinon un script de contrôle ; et seulement en dernier recours une simple consigne dans le prompt. Chaque élément explique ce qu'il fait, quelle phrase du texte il implémente et comment il est vérifié.
4. **Je vois l'impact avant d'activer** : les contrôles proposés sont exécutés à blanc sur le dépôt actuel, et l'aperçu indique le nombre de violations existantes par convention (sans rien modifier).
5. **Je valide élément par élément** : valider / refuser / demander une modification (en commentaire), puis l'IA révise le pack. Les éléments qui modifient le dépôt (config ESLint…) passent par un diff explicite.
6. **J'active à la demande** : chaque convention a un interrupteur et un niveau (`advise` / `warn` / `block`), avec la même mécanique d'activation que les règles : projet, ticket, tâche (§7.6).

**Stockage** : `<projet>/conventions/<slug>/` contient `source.md` (ton texte, la source de vérité), `pack.toml` (manifeste qui relie chaque phrase aux éléments générés) et les éléments générés. Quand tu modifies le texte, la régénération montre un diff entre l'ancienne et la nouvelle version du pack. Rien n'est réécrit sans validation.

### 11.2 Agent de veille : améliorer la bibliothèque à partir du web (obligatoire)

Un agent parcourt Internet (documentation officielle d'Anthropic et de Claude Code, guides de bonnes pratiques, collections reconnues de skills et de commandes) et critique puis propose d'améliorer la qualité de la bibliothèque : prompts des rôles, préférences, règles, skills, commandes, ainsi que de nouveaux éléments utiles.

- **Lancement** : à la demande, quand on veut (vue Bibliothèque, palette `⌘K`, CLI), sur toute la bibliothèque ou sur les éléments choisis. Pas de lancement automatique.
- **Critique** : pour chaque élément examiné, ses défauts (flou, contradictions, redondances, consignes dépassées, manques) avec la bonne pratique et la source qui les fondent, même quand aucune réécriture n'est proposée.
- **Exécution** : headless (`claude -p --json-schema`), avec seulement la recherche et la lecture web et une lecture seule de la bibliothèque ; aucun outil d'écriture, aucun accès aux dépôts ni aux secrets.
- **Propositions** : élément par élément, avec le diff par rapport à la version actuelle, ce que ça améliore et les **sources citées** (URL). Même revue que les conventions (§11.1) : valider, refuser ou demander une modification ; application journalisée ; une proposition refusée n'est pas reproposée.
- **Couches respectées** : une amélioration d'un élément livré s'écrit comme surcharge dans la couche globale, jamais dans les fichiers livrés ; un élément de projet reste dans sa couche.
- **Contenu du web non fiable** (injection de prompt) : il n'est jamais exécuté ; un hook ou un script proposé est montré en entier et doit être accepté explicitement.

### 11.3 Gestion complète de Claude Code (obligatoire)

TermF gère tout ce que Claude Code permet de configurer, de planifier ou de consulter, depuis l'app et la CLI, sans avoir à ouvrir une session ni à éditer des fichiers à la main : tâches planifiées (cron locaux, boucles, routines), réglages et permissions par portée, hooks, serveurs MCP, plugins et marketplaces, skills, agents, commandes, styles de sortie, mémoire (`CLAUDE.md`, mémoire automatique), sessions et transcripts, usage et coûts, Remote Control, comptes et mises à jour. L'inventaire vérifié (emplacements, commandes, ce qu'une app externe peut ou ne peut pas faire) est dans [CLAUDE-CODE.md](CLAUDE-CODE.md).

- **Tâches planifiées** : les cron de session de Claude Code ne sont pas pilotables de l'extérieur et meurent avec la session ; les routines cloud ne se gèrent qu'en partie hors du web. TermF a donc **son propre planificateur** : une tâche = prompt + projet/dépôt + planning cron + mode (headless `claude -p` ou session dans une tuile) + modèle, effort, mode de permission, budget. Elle tourne tant que l'app est ouverte, et aussi app fermée via le planificateur de l'OS (launchd, systemd, Planificateur de tâches) qui appelle `termf schedule run <id>`. Historique et résultats des exécutions en base (§14), notification à la fin. TermF liste aussi, en lecture, les tâches de Claude Desktop (`~/.claude/scheduled-tasks/`) et les routines cloud (lien vers le web pour ce qui n'est pas pilotable).
- **Réglages, permissions, hooks, variables, modèle** : éditeur par portée (utilisateur, projet, local ; géré en lecture seule) avec validation par le schéma officiel, aperçu du diff, écriture atomique, et indication de la portée qui décide pour chaque clé.
- **MCP et plugins** : à travers `claude mcp …` et `claude plugin …`, jamais en éditant `~/.claude.json` ni `installed_plugins.json`.
- **Skills, agents, commandes, styles, règles natifs de Claude Code** et **mémoire** (`CLAUDE.md` par portée, mémoire automatique) : parcourir, éditer, activer ; passerelle avec la bibliothèque de TermF (import déjà fait, export vers les dossiers de Claude).
- **Sessions** : lister par projet, reprendre, dupliquer (`--fork-session`), renommer, purger (`claude purge`) ; sessions de fond via `claude agents --json`.
- **Usage, coûts et limites** : jauge de contexte et quotas 5 h / 7 j lus dans le JSON de la statusline des sessions lancées par TermF ; coût des runs headless.
- **Compte et version** : `claude auth status|login|logout`, version, canal, `claude update`, `claude doctor`.

---

## 12. Accès production sécurisé (« Coffre & Ops »)

### 12.1 Modèle de menace

| Menace | Contre-mesure principale |
|---|---|
| Fuite d'un secret dans le contexte d'un LLM | L'IA n'a jamais d'identifiants ; les sorties passent par un masquage automatique |
| Injection de prompt (logs, tickets, données en base) qui pousse à exfiltrer ou détruire | Règle « trifecta » (§12.4) + approbation humaine + recettes bornées |
| Secrets en clair sur disque (worktrees, logs, scrollback) | Coffre chiffré, `.env` temporaires en 0600 supprimés en fin de run, logs filtrés |
| Commande destructrice par erreur (humain ou IA) | Lecture seule par défaut, confirmation tapée pour l'écriture |
| Vol du portable | Coffre chiffré, verrouillage auto, biométrie |
| Dépôt ou plugin malveillant | Confiance explicite par dépôt (§12.6) |

### 12.2 Coffre

- **Stockage** : la clé maître est dans le trousseau de l'OS (Keychain, Credential Manager, Secret Service). Les secrets sont dans `vault.age`, chiffré avec une clé dérivée par Argon2id. Termf accepte aussi des **références** vers un gestionnaire externe (`op://`, `bw://`, `doppler://`, `aws-sm://`, `vault://`), résolues à l'usage et jamais copiées.
- **Déverrouillage** : mot de passe maître, Touch ID ou Windows Hello. Verrouillage automatique après 10 minutes d'inactivité et à la mise en veille. La portée « prod » demande une **ré-authentification à chaque usage** (comme `sudo`, valable 5 minutes).
- **En mémoire** : `SecretString` + `zeroize`, core dumps désactivés, `mlock` quand c'est possible.
- **Environnement des agents** : les agents démarrent avec une liste blanche minimale de variables (`PATH`, `HOME`, `LANG`, `TERM`, plus les variables explicitement autorisées). Les secrets n'y figurent jamais.
- **Interdits par défaut** dans les settings générés : `Read(**/.env*)`, `Read(**/*.pem)`, `Read(**/id_*)`, `Bash(*printenv*)`, `Bash(env)`.

### 12.3 Connexions et recettes

Types de connexion :

- **SSH** via le `ssh` du système (ssh-agent ou agent 1Password, ProxyJump pour les bastions) : les clés restent dans l'agent SSH.
- **Kubernetes** (contexte kubectl).
- **Postgres / MySQL** via un tunnel SSH ouvert par Termf sur un port local éphémère.
- **Logs** : CloudWatch, GCP Logging, Loki, Datadog, ou `journalctl` / fichiers via SSH.
- **PaaS** : CLI Vercel, Fly, Railway…

```toml
# library/recipes/sql-readonly.toml
name = "sql-readonly"
description = "Requête SQL en lecture seule sur la base de prod"
connection = "acme-prod-db"
mode = "read"
params = { query = { type = "sql" } }
limits = { rows = 500, timeout_s = 15 }
ai_access = "redacted"     # none | redacted | full
approval = "always"
redact = ["email", "phone", "iban", "jwt", "vault_values"]
```

**Lecture seule en défense en profondeur** :

1. Utilisateur de base dédié en lecture seule (Termf le vérifie à la création de la connexion et avertit sinon).
2. Analyse par `sqlparser` : une seule instruction, et uniquement `SELECT`, `EXPLAIN` ou `SHOW`.
3. Session ouverte en `SET TRANSACTION READ ONLY` avec `statement_timeout`.
4. `LIMIT` ajouté automatiquement.

Les recettes d'**écriture** (redémarrage, rollback) existent, mais l'IA peut seulement les **proposer**. Seul un humain les exécute, après une confirmation tapée (le nom de l'environnement).

### 12.4 L'IA et la prod

- Les agents n'ont **ni identifiants ni accès direct**. Ils disposent seulement de `ops.run(recette, params)`.
- Déroulé d'une demande :
  1. Termf valide les paramètres.
  2. Une carte d'approbation montre la commande ou la requête exacte, la cible et le mode.
  3. Tu approuves.
  4. Termf exécute.
  5. La sortie est masquée : valeurs connues du coffre, motifs type gitleaks, entropie, PII selon la politique.
  6. Le résultat, tronqué aux limites, est renvoyé à l'agent.
- Une connexion en `ai_access = "none"` est invisible pour l'IA.
- **Règle « trifecta »** : une session qui lit des données de prod ne peut pas avoir en même temps d'outils de sortie (WebFetch, réseau via Bash, écriture sur le tracker) sans approbation. Termf l'applique en générant pour elle un jeu d'outils restreint (`--disallowedTools`, `--strict-mcp-config`).
- **`.env`** : la recette « Récupérer le .env » charge le fichier en mémoire et l'affiche **masqué**. Chaque valeur se révèle individuellement, après ré-authentification. On peut ensuite :
  - copier une valeur (presse-papiers vidé après 30 s) ;
  - l'écrire dans le `.env` du worktree (0600, exclu de git, supprimé en fin de run) ;
  - comparer local et prod (clés seulement, valeurs hachées).

  Par défaut, l'IA ne voit que **les noms des clés**.
- **Mode manuel, sans IA** : les mêmes recettes sont disponibles depuis l'UI et la CLI (`termf ops logs acme-prod --since 1h --grep ERROR`). L'UI offre aussi une console SQL protégée et un terminal SSH intégré.

### 12.5 Audit

Le journal est en ajout seul (table SQLite + export JSONL). Chaque entrée est **chaînée par hash** (SHA-256 de l'entrée précédente), ce qui rend toute altération détectable. Une entrée enregistre :

- qui : humain, ou agent avec son run et sa session ;
- quoi : recette, paramètres, cible ;
- l'approbation ;
- la taille du résultat et les masquages appliqués.

Un visualiseur est intégré, avec export.

### 12.6 Sécurité de l'application elle-même

- **Tauri** : CSP stricte, aucun contenu distant dans la webview, capabilities minimales par fenêtre, liste blanche des commandes IPC. La webview ne reçoit jamais de secret, sauf sur révélation explicite.
- **Socket locale** : permissions 0600 sous Unix, ACL utilisateur sur le named pipe Windows, et **jeton par session** transmis aux enfants (`TERMF_TOKEN`) pour qu'un autre processus local ne puisse pas piloter Termf.
- **Logs** : `tracing` avec une couche de masquage. La sortie des PTY n'y est jamais écrite.
- **Distribution** : mises à jour signées (minisign via l'updater Tauri), binaires signés (notarisation Apple, Authenticode). Côté chaîne d'approvisionnement : `cargo deny`, `cargo audit`, `pnpm audit`, lockfiles.
- **Confiance par dépôt** (comme le « workspace trust » de VS Code) : Termf demande confirmation avant le premier agent dans un dépôt, et avant d'exécuter les commandes `setup`, `dev` ou `checks` d'un `.termf/project.toml` nouveau ou modifié (contrôle par hash).

### 12.7 Gestion des clés SSH

**Inventaire**, dans une vue unique :

- les clés de `~/.ssh` (paires privée/publique, `.pub` orphelins) ;
- les clés chargées dans l'agent SSH (`ssh-add -l`), y compris les agents 1Password ou Secretive ;
- les hôtes déclarés dans `~/.ssh/config`.

Pour chaque clé :

- type, taille et empreinte SHA256 ;
- commentaire ;
- protection par passphrase ou non ;
- permissions de fichier (alerte si elles sont trop ouvertes) ;
- présence dans l'agent ;
- hôtes et dépôts qui l'utilisent ;
- présence sur GitHub ou GitLab (empreintes comparées via l'API).

**Actions** :

- **Générer** : ed25519 par défaut, ed25519-sk / ecdsa-sk pour une clé matérielle, RSA 4096 si un vieux serveur l'exige. Passphrase fortement encouragée, commentaire.
- **Ajouter à l'agent ou l'en retirer** (sous macOS, la passphrase est gardée dans le Keychain via `--apple-use-keychain`).
- **Copier la clé publique**, ou **l'envoyer sur GitHub** (`gh ssh-key add` / API) ou GitLab.
- **Déclarer un hôte** dans `~/.ssh/config` (alias, HostName, User, IdentityFile, IdentitiesOnly), avec aperçu du diff avant écriture.
- **Associer une clé à un dépôt** (`core.sshCommand` propre au dépôt), puis **tester la connexion** (`ssh -T`).
- **Corriger les permissions**.
- **Faire tourner une clé** : en générer une nouvelle, l'envoyer, mettre à jour la config, puis retirer l'ancienne après confirmation.
- **Supprimer** : la clé est déplacée dans une corbeille (`~/.ssh/.termf-trash/`) après confirmation, jamais effacée directement.

**Implémentation** : le parsing et les empreintes passent par la crate `ssh-key` (RustCrypto). La génération et l'agent passent par les `ssh-keygen` / `ssh-add` du système (présents sous macOS, Linux et Windows 10+), pour une compatibilité maximale.

**Sécurité** : le contenu d'une clé privée n'est jamais affiché, jamais envoyé à la webview et jamais lisible par un agent (règles `deny`, §12.2). La synchronisation des clés (§5.4) est optionnelle et chiffrée.

---

## 13. UI / UX

### 13.1 Principes

Esprit Brave : sobre, dense mais aéré, une seule couleur d'accent, Inter + JetBrains Mono, thème clair/sombre automatique. **Clavier d'abord** : tout est accessible par la palette `⌘K` / `Ctrl+K`. Pas de modale sans nécessité. Chaque statut combine couleur + icône + texte, pour rester lisible sans dépendre de la couleur.

### 13.2 Disposition

```
┌───────┬─────────────────────────────────────────────────────────────┬──────────────────────┐
│ ◉ Mis-│  ACME-123 · Export CSV des factures     cadrage ✓ plan ✓ dev ● │ Inspecteur           │
│  sion │ ┌──────────────┬──────────────┬──────────────┐              │ Coût   1,84 $ / 15   │
│ ───── │ │ PO        ✓  │ Back   ● 34% │ Front  ⏸ ?   │              │ Contexte ▓▓▓░░░ 34 % │
│ Acme  │ │ critères OK  │ tests verts  │ T'ATTEND     │              │ Canari ✓             │
│  api  │ └──────────────┴──────────────┴──────────────┘              │ Questions (1)        │
│  front│                                                             │ Approbations (0)     │
│ Beta  │   [conversation ou terminal de l'agent sélectionné]         │ Journal ▸            │
│ ───── │                                                             │ Diff ▸               │
│ Inbox │                                                             │                      │
│ Coffre│                                                             │                      │
├───────┴─────────────────────────────────────────────────────────────┴──────────────────────┤
│ ⌃` ▸ @front ajoute 8px entre le bouton et le filtre          (suit le run ACME-123)          │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 13.3 Vues

1. **Mission Control** (accueil) : tous les runs en cartes, répartis en colonnes `En file · En cours · T'attend · En revue · Terminé`. La colonne **T'attend** est toujours en tête et son compteur s'affiche sur l'icône de la barre système et du dock. Chaque carte montre le ticket, les rôles avec leur statut, la jauge de contexte, le coût et la durée.
2. **Run** : timeline du DAG, tuiles d'agents, conversation ou terminal de l'agent sélectionné, journal, diffs, approbations.
3. **Grille de sessions** (« en masse ») : on coche N dépôts, puis « Ouvrir Claude ».
   - Grille automatique ou en 2×2 / 3×3, et mode focus (une session agrandie, les autres en vignettes).
   - **Diffusion** : on tape une fois et le message part vers les sessions cochées (`⌘⇧↵`), avec un gabarit par session (`{{ repo.name }}`).
   - Dispositions sauvegardées (exemple : « matin », 6 dépôts).
4. **Inbox tickets** : par projet, avec filtres et aperçu. Le bouton **Lancer** permet de choisir le workflow et d'ajuster modèles, effort, règles et bascules de bibliothèque.
5. **Explorateur / éditeur** :
   - arborescence, `⌘P` (fuzzy), `⌘⇧F` (recherche ripgrep) ;
   - onglets CodeMirror, statut git, diff côte à côte ;
   - « Ouvrir dans l'éditeur externe » (VS Code, Zed, JetBrains).
6. **Revue visuelle** (§8).
7. **Bibliothèque** : matrice d'activation, édition des éléments, import / export.
8. **Coffre & Ops** : connexions, recettes, console SQL, logs en direct filtrables, audit.
9. **Réglages**.

### 13.4 Terminal rapide (toujours disponible)

- Un panneau déroulant façon Quake, ouvert par `` Ctrl+` `` quand TermF a le focus, ou par le bouton `>_`.
- Le **raccourci global**, actif même quand l'app est en arrière-plan, est optionnel (`quick_terminal_shortcut` dans `termf.toml`). Un raccourci global actif par défaut volerait celui d'autres apps : `` Ctrl+` `` est le terminal de VS Code.
- Il comprend :
  - les commandes shell, exécutées dans le dossier du dépôt ou du worktree sélectionné ;
  - les commandes `termf` ;
  - `@rôle message`, envoyé à l'agent de ce rôle dans le run actif ;
  - `#ACME-123`, qui ouvre le run ;
  - `!ops …`.
- **Il suit le contexte** : il se place automatiquement dans le worktree de l'agent sélectionné. L'historique est conservé par projet.
- Tout cela fonctionne aussi depuis n'importe quel terminal via la CLI : `termf run <url>`, `termf ls`, `termf watch`, `termf say @front "…"`, `termf open acme/front`, `termf approve <id>`, `termf stop --all`.

### 13.5 Raccourcis principaux

| Action | macOS | Windows / Linux |
|---|---|---|
| Palette de commandes | `⌘K` | `Ctrl+K` |
| Terminal rapide (dans l'app ; global en option) | `` ⌃` `` | `` Ctrl+` `` |
| Lancer un ticket (coller une URL) | `⌘⇧L` | `Ctrl+Shift+L` |
| Fichier (fuzzy) | `⌘P` | `Ctrl+P` |
| Recherche dans le projet | `⌘⇧F` | `Ctrl+Shift+F` |
| Prochain agent qui t'attend | `⌘J` | `Ctrl+J` |
| Diffuser vers les sessions cochées | `⌘⇧↵` | `Ctrl+Shift+Enter` |
| Arrêt d'urgence de tous les agents | `⌘⇧.` | `Ctrl+Shift+.` |

### 13.6 Notifications

Des notifications natives uniquement pour : une question posée, une approbation demandée, un run terminé ou en échec, un budget atteint. Elles sont regroupées, avec un mode « Ne pas déranger ».

---

## 14. Persistance et observabilité

- **SQLite** : runs, étapes, sessions, événements typés (payload JSON), approbations, revues visuelles, coûts, audit chaîné. Mode WAL, rétention réglable (par exemple 90 jours d'événements).
- **Dossier par run** `runs/<id>/` :
  - `journal.md`, lisible par un humain : ticket, plan, décisions, questions et réponses, résumé par étape, coûts ;
  - `events.jsonl` ;
  - `visual/v<n>/{before,after,diff}/*.png` ;
  - les plugins éphémères des sessions, supprimés à la clôture.
- **Sortie des PTY** : ring buffer en mémoire (10 000 lignes, réglable), rejoué quand on rattache la vue. Un log compressé (zstd) est possible par session ; il est désactivé par défaut et **interdit** pour les sessions liées à la prod.
- **Coûts** : lus dans le stream-json (usage et coût), puis agrégés par jour, projet, rôle et modèle.

---

## 15. Performance et énergie

- Aucun polling, sauf pour les tickets (avec backoff, et seulement quand c'est utile).
- La sortie des PTY est regroupée en lots de 16 ms au plus et n'est envoyée à l'UI que pour les terminaux **visibles**. Les autres se contentent de remplir leur ring buffer.
- Fenêtre masquée ou réduite : le rendu de l'UI est suspendu (Page Visibility), le cœur continue.
- Les agents headless ne coûtent aucun rendu.
- Les vues sont chargées à la demande (CodeMirror, revue visuelle).
- Équipes éphémères : 0 processus résiduel après un run. Les serveurs de dev sont arrêtés après la revue visuelle.
- Routage des modèles : Haiku pour les tâches utilitaires, effort réglé par rôle et par taille de tâche.
- **Mode « rester éveillé »** : la mise en veille du système suspend les agents, alors que le verrouillage de l'écran ne les arrête pas. TermF empêche donc la veille avec la crate `keepawake` (IOKit sous macOS, `SetThreadExecutionState` sous Windows, D-Bus sous Linux). Trois modes :
  - `auto`, par défaut : actif tant qu'au moins une session travaille, plus 2 minutes de marge ;
  - `always` ;
  - `off`.

  L'option séparée « garder l'écran allumé » empêche aussi le verrouillage automatique. Elle est désactivée par défaut, pour l'énergie et parce qu'un poste déverrouillé sans surveillance est un risque. Un indicateur reste visible tant que la veille est bloquée. Un portable dont on rabat l'écran se met en veille quoi qu'il arrive, sauf branché sur secteur avec un écran externe.
- **Mesuré** : benchmarks en CI (démarrage, RAM au repos, stress à 30 sessions), plus un panneau « Énergie » dans l'app (CPU et RAM par processus enfant).

---

## 16. Distribution

- **CI GitHub Actions** en matrice :
  - macOS universel arm64 + x64 (`.dmg` notarisé) ;
  - Windows x64 + arm64 (MSI ou NSIS signés) ;
  - Linux (AppImage, `.deb`, `.rpm` ; Flatpak plus tard).
- **Updater** Tauri signé, avec deux canaux : stable et beta.
- La **CLI `termf`** est embarquée dans l'app et s'installe dans le PATH depuis les réglages.
- **Premier lancement** : détection de git, des CLI d'agents, de `gh` (optionnel) et d'un Chrome/Chromium, avec téléchargement de Chrome for Testing à la demande pour la revue visuelle.

---

## 17. Feuille de route

Les délais sont des ordres de grandeur pour un développeur seul assisté par IA. **Chaque phase est utilisable seule.**

| Phase | Contenu | Terminée quand… |
|---|---|---|
| **0. Socle** (~2 sem.) | Workspace Cargo, Tauri 2, Svelte 5, config TOML global/projet, découverte adaptative des dépôts (tri par activité), sessions PTY + ring buffer + xterm.js, grille de terminaux, interface FR/EN | J'ouvre 10 `claude` dans 10 dépôts en 2 clics, avec moins de 200 Mo de RAM |
| **1. Pilotage manuel** (~2 sem.) | Projets multi-dépôts, diffusion, dispositions, statuts via hooks (plugin éphémère + `termf hook`), notifications, terminal rapide (raccourci dans l'app, global en option), CLI `termf`, explorateur + fuzzy + CodeMirror + diff, interrupteur Remote Control (§6.6), mode « rester éveillé » (§15) | Je vois d'un coup d'œil quel agent m'attend, et je peux en reprendre un depuis mon téléphone |
| **1b. Design & UX** (~1 sem.) | Système de design (tokens, typographie embarquée, icônes), barre du haut, palette `⌘K`, raccourcis, états vides et premier lancement, accessibilité | Je trouve toute action au clavier en moins de 2 s, sans lire de doc |
| **2. Bibliothèque & config** (~2-3 sem.) | Skills, commandes, rôles, préférences ; couches ; matrice d'activation ; import `~/.claude` ; export/import ; **conventions assistées par IA** (§11.1) | Je transfère ma config sur une autre machine en un fichier ; j'écris mes conventions et j'active le pack proposé |
| **2b. Synchro & SSH** (~2 sem.) | Synchro chiffrée multi-remote (§5.4), gestionnaire de clés SSH (§12.7) | Ma config et mes clés me suivent d'une machine à l'autre sans export |
| **3. Tickets** (~2 sem.) | SQLite (runs, événements) ; GitHub puis Jira, inbox, résolveur d'URL, « lancer un ticket » avec un agent + worktree + PR | Je colle une URL et j'obtiens une PR liée au ticket |
| **4. Équipe & orchestration** (~3-4 sem.) | Headless stream-json, vue conversation, prendre/rendre la main ; rôles, chef d'orchestre (plan JSON), moteur de workflow, gates, ordonnanceur, budgets ; MCP Termf ; jauge, canari, passation ; moteur de règles (projet / ticket / tâche) et contrôles périmètre et texte | Deux tickets multi-rôles tournent en parallèle sans interférence |
| **5. Revue visuelle** (~2 sem.) | Ports, serveurs de dev, captures CDP, diff, page de revue avec épingles, boucle de retour | Je valide ou refuse chaque écran modifié avant la PR |
| **6. Coffre & Ops** (~3 sem.) | Coffre, biométrie, connexions SSH/DB/logs, recettes, masquage, approbations, audit chaîné, console SQL en lecture seule | Je récupère logs, SQL et `.env` de prod, avec ou sans IA, sans qu'aucun secret ne sorte |
| **7. Finition** (continu) | Paquets signés pour les 3 OS, updater, budgets de perf en CI, accessibilité, onboarding, adaptateurs Codex et Gemini | — |

Logique de l'ordre : de la valeur quotidienne dès les phases 0-1, puis la partie la plus risquée (l'orchestration) une fois les fondations en place, et la prod en dernier parce que c'est là que l'exigence de sécurité est la plus haute.

---

## 18. Risques et décisions ouvertes

| Risque | Mitigation |
|---|---|
| Les options de Claude Code évoluent vite | Adaptateur isolé, détection de version, test de contrat sur `claude --help` |
| Quotas d'abonnement atteints avec beaucoup d'agents | Limites par runtime, détection de « usage limit » qui met en pause, routage vers Haiku/Sonnet |
| Performance de WebKitGTK (Linux) | Repli canvas, budgets testés en CI Linux |
| Coûts qui s'emballent | Budgets par étape, run et jour ; arrêt d'urgence |
| Injection de prompt via tickets ou logs | Contenu marqué non fiable, règle trifecta, approbations |
| Agents qui débordent du périmètre | Règles en niveau `block` + revue visuelle + gate de PR |
| Captures instables (données dynamiques) | Masques, données de seed, animations coupées, seuil |
| Complexité globale | Phases livrables indépendamment |

**Décisions prises** (2026-11-21) :

1. Stack Tauri 2 + Rust + Svelte 5.
2. Un seul processus hôte en v1, sans démon séparé.
3. Runs orchestrés en headless avec vue conversation, et prise de main en PTY.
4. Gate « plan » activée par défaut au début, désactivable une fois la confiance établie.
5. Nom : **TermF**.
6. Licence MIT, interface en français et en anglais (§19).

---

## 19. Open source et personnalisation

TermF est open source (licence MIT) et pensé pour être **forké** : chacun l'adapte à sa façon de travailler.

- **Personnaliser par des fichiers plutôt que par du code** : réglages, projets, rôles, règles, workflows, conventions et recettes sont des fichiers texte. Un fork qui ne touche que ces fichiers reste petit et continue de récupérer les évolutions du dépôt d'origine sans conflit.
- **Points d'extension explicites dans le code** : `AgentRuntime` (nouvelle CLI d'agent, §6.1), `TicketProvider` (nouveau tracker, §10), types d'étapes de workflow et contrôles de règles (§7.5, §7.6). Ajouter un adaptateur ne demande pas de toucher au cœur.
- **Rien de propre à une machine dans le code** : aucun chemin, dossier ou outil personnel codé en dur. Par exemple, la découverte des dépôts part du dossier personnel de chacun et s'adapte à toute organisation (`~/Projects`, `~/code`, `~/Sites`, `~/go/src`…). Elle ignore les dossiers système, médias et dépendances, respecte les dossiers protégés de macOS, plafonne le nombre de dossiers lus et trie par dernière activité.
- **Packs communautaires** (plus tard) : partager des rôles, règles, workflows ou conventions sous forme de dépôt git importable par URL, sans forker le code. Un pack qui contient des scripts de contrôle demande une confirmation de confiance (§12.6).
- **Interface traduisible** : textes dans `app/src/lib/i18n.ts`, livrés en français et en anglais, langue du système par défaut.
- **Aucune télémétrie, aucun serveur TermF.**

---

## Annexe A — Schéma du plan produit par le chef d'orchestre

```json
{
  "summary": "Ajouter l'export CSV des factures",
  "acceptance": [
    { "id": "AC1", "text": "Endpoint GET /invoices/export renvoie un CSV" },
    { "id": "AC2", "text": "Bouton « Exporter en CSV » sur la page Factures" }
  ],
  "out_of_scope": ["Export PDF", "Filtres avancés"],
  "questions": [],
  "tasks": [
    { "id": "T1", "role": "back", "repo": "api", "size": "M", "covers": ["AC1"],
      "goal": "…", "files_hint": ["src/routes/invoices.ts"], "depends_on": [], "ui_impact": false },
    { "id": "T2", "role": "front", "repo": "front", "size": "S", "covers": ["AC2"],
      "goal": "…", "files_hint": ["src/pages/Invoices.tsx"], "depends_on": ["T1"], "ui_impact": true }
  ]
}
```

Si `questions` n'est pas vide, le run attend les réponses avant toute exécution.

## Annexe B — Rapport obligatoire d'une étape (`report`)

```json
{
  "status": "done",
  "summary": "…",
  "covers": ["AC2"],
  "text_changes": [{ "file": "src/pages/Invoices.tsx", "text": "Exporter en CSV", "justified_by": "AC2" }],
  "affected_views": [{ "url": "/invoices", "state": "liste non vide", "viewports": ["desktop", "mobile"] }],
  "out_of_scope_findings": ["Le tri par date est cassé sur /invoices (non corrigé)"]
}
```

## Annexe C — Glossaire

- **Projet** : 1..N dépôts d'un même produit.
- **Run** : exécution d'un ticket par une équipe éphémère.
- **Étape** : nœud du workflow (agent, shell, gate, règles, visuel, connecteur).
- **Gate** : point d'arrêt qui attend une décision humaine.
- **Règle** : consigne activable par projet, ticket ou tâche, avec un niveau (`advise`, `warn`, `block`) ; elle peut embarquer un contrôle automatique et une gate.
- **Passation** : brief écrit par un agent pour que la session suivante reparte d'un contexte propre.
- **Canari** : marqueur (ton prénom) dont la disparition signale un contexte dégradé.
- **Recette** : opération prod paramétrée, bornée et auditée.
