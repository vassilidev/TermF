# Claude Code : ce que TermF doit gérer (inventaire de référence)

> Relevé le 2026-11-21 dans la documentation officielle (code.claude.com), pour Claude Code 2.1.330 (la doc cite des versions jusqu'à ~2.1.305). « (non vérifié) » : absent des pages lues. Avant de coder une tranche, revérifier la page citée et `claude --help` sur la version installée. Tous les chemins `~/.claude*` (y compris `~/.claude.json` et le trousseau) suivent `$CLAUDE_CONFIG_DIR` s'il est défini.

## Règles pour une app externe

- Écrire les fichiers de réglages **atomiquement** (temporaire + rename), en JSON strict ; un JSON invalide provoque une erreur de réglages au démarrage suivant.
- Ne pas éditer `~/.claude.json` pendant que Claude Code tourne : passer par `claude mcp …` / `claude plugin …`. Ne jamais toucher `installed_plugins.json`, `.credentials.json` ni les jetons OAuth des MCP.
- **Ne pas parser les transcripts JSONL ni `~/.claude/jobs/`** : format interne, change entre versions. Sources stables : JSON de la statusline, `claude -p --output-format json|stream-json`, `claude agents --json`, `claude auth status`, `transcript_path` des hooks (comme pointeur).
- Une session lancée par TermF reçoit ses surcharges par `--settings <fichier-ou-json>` (portée session, n'écrit rien chez l'utilisateur), `--plugin-dir`, `--mcp-config`, `--append-system-prompt-file`.

## Inventaire

| Domaine | Où c'est stocké | Comment le gérer de l'extérieur | Doc |
|---|---|---|---|
| **Boucles et cron de session** (`/loop`, CronCreate/List/Delete) | En mémoire de la session (max 50, récurrents expirés après 7 j, ≥ 1 min) ; `.claude/loop.md` / `~/.claude/loop.md` pour le prompt par défaut | **Impossible de l'extérieur** : n'existent que dans une session vivante ; restaurés par `--resume`. Désactivation : `CLAUDE_CODE_DISABLE_CRON=1` | /docs/en/scheduled-tasks |
| **Tâches planifiées de Claude Desktop** | `~/.claude/scheduled-tasks/<nom>/SKILL.md` (frontmatter `name`, `description`, prompt en corps) ; planning et état : emplacement non vérifié | Lecture du dossier ; le prompt est éditable ; planning seulement via l'app Desktop (≥ 1 h) | /docs/en/desktop-scheduled-tasks |
| **Routines cloud** (claude.ai) | Côté Anthropic | Web (claude.ai/code/routines) ; dans une session `/schedule list|update|run` ; déclenchement API `POST …/fire` avec jeton par routine (bêta, en-tête `experimental-cc-routine-2026-04-01`, comptes claude.ai) ; suppression et jetons API : web seulement. Limites : 100 exécutions planifiées/h, 30 « run now »+API/h par tâche | /docs/en/routines |
| **Réglages** | `~/.claude/settings.json` (utilisateur), `.claude/settings.json` (projet partagé), `.claude/settings.local.json` (projet local), géré (`/Library/Application Support/ClaudeCode/managed-settings.json`, `/etc/claude-code/`, `C:\Program Files\ClaudeCode\`, MDM `com.anthropic.claudecode`, registre `HKLM\SOFTWARE\Policies\ClaudeCode`), `~/.claude.json` (config globale) | Édition atomique, schéma `https://json.schemastore.org/claude-code-settings.json` ; priorité géré > `--settings` > local > projet > utilisateur ; listes fusionnées ; rechargement à chaud sauf `model`/`effortLevel` ; le projet ne peut pas mettre `defaultMode` à `auto`/`bypassPermissions` | /docs/en/settings, /docs/en/settings-reference |
| **Permissions et bac à sable** | Clés `permissions.{allow,ask,deny,defaultMode,additionalDirectories…}` et `sandbox.*` des réglages | Édition des réglages ; modes `default`, `acceptEdits`, `plan`, `auto`, `dontAsk`, `bypassPermissions` ; drapeaux `--permission-mode`, `--allowedTools`, `--disallowedTools` | /docs/en/permissions, /docs/en/sandboxing |
| **Hooks** | Clé `hooks` des réglages, `hooks/hooks.json` des plugins, frontmatter des skills et agents | Édition des réglages ; 33 événements ; types `command`, `http`, `mcp_tool`, `prompt`, `agent` | /docs/en/hooks |
| **Serveurs MCP** | Local et utilisateur dans `~/.claude.json`, projet dans `.mcp.json`, géré dans `managed-mcp.json` | `claude mcp add|add-json|list|get|remove|login|logout|reset-project-choices` ; `.mcp.json` éditable directement | /docs/en/mcp |
| **Plugins et marketplaces** | `~/.claude/plugins/` (`installed_plugins.json` à ne pas éditer, cache, `data/<id>/`) ; clés `enabledPlugins`, `extraKnownMarketplaces`, `pluginConfigs` | `claude plugin …` (sous-commandes d'installation : non vérifié, voir `claude plugin --help`) | /docs/en/plugins-reference |
| **Skills, agents, commandes, styles de sortie** | `~/.claude/{skills,agents,commands,output-styles}/`, `.claude/…` du projet, plugins ; clé `skillOverrides`, `outputStyle` | Pas de CLI : écrire les fichiers (rechargement à chaud, sauf styles au démarrage) | /docs/en/skills, /docs/en/sub-agents, /docs/en/output-styles |
| **Mémoire** | `CLAUDE.md` géré / `~/.claude/CLAUDE.md` / `./CLAUDE.md` ou `./.claude/CLAUDE.md` / `./CLAUDE.local.md`, imports `@chemin`, `.claude/rules/*.md` et `~/.claude/rules/*.md` ; mémoire auto `~/.claude/projects/<projet>/memory/` (`MEMORY.md` + un fichier par souvenir) | Fichiers texte, lus au chargement suivant ; clés `autoMemoryEnabled`, `autoMemoryDirectory`, `claudeMdExcludes` | /docs/en/memory |
| **Sessions** | `~/.claude/projects/<projet>/<session>.jsonl` (interne), `history.jsonl`, `file-history/`, `plans/` | `claude -c`, `-r <id|nom>`, `--fork-session`, `--session-id`, `-n` ; `claude purge [chemin] [--dry-run]` ; rétention `cleanupPeriodDays` (30 j) ; deux processus sur une même session peuvent perdre des messages | /docs/en/sessions |
| **Sessions de fond** | `~/.claude/jobs/` (interne), `~/.claude/daemon/` | `claude --bg`, `claude agents --json`, `attach`, `logs`, `stop`, `respawn`, `rm`, `daemon status|stop` | /docs/en/agent-view |
| **Usage, coûts, limites** | — | JSON de la statusline (`context_window.used_percentage`, `context_window_size`, `cost.total_cost_usd`, `rate_limits.five_hour|seven_day`) ; `claude -p --output-format json` (`total_cost_usd`) ; `--max-budget-usd` ; `/usage` n'est pas une API | /docs/en/statusline, /docs/en/costs |
| **Remote Control** | Clé `remoteControlAtStartup` | `claude --remote-control`, `claude remote-control`, `/rc` ; abonnement claude.ai requis | /docs/en/remote-control |
| **Worktrees** | `<dépôt>/.claude/worktrees/<nom>`, `.worktreeinclude`, clé `worktree.baseRef` | `claude --worktree` (TermF gère ses propres worktrees, §9) | /docs/en/worktrees |
| **Compte** | Trousseau macOS, sinon `~/.claude/.credentials.json` (0600) | `claude auth login|logout|status` (JSON), `claude setup-token` ; plusieurs comptes = un `CLAUDE_CONFIG_DIR` par compte | /docs/en/authentication |
| **Version et mises à jour** | `~/.local/share/claude/versions/` | `claude --version`, `claude update`, `claude install <version|stable|latest>`, `claude doctor` ; clés `autoUpdatesChannel`, `minimumVersion` ; `DISABLE_AUTOUPDATER`, `DISABLE_UPDATES` | /docs/en/setup |
| **Modèle et effort** | Clés `model`, `effortLevel`, `availableModels`, `fallbackModel`, `modelOverrides` | Réglages ou `--model`, `--effort` | /docs/en/model-config |
| **Raccourcis, thèmes, statusline** | `~/.claude/keybindings.json`, `~/.claude/themes/*.json`, clés `theme`, `statusLine` | Fichiers (rechargement à chaud) | /docs/en/keybindings, /docs/en/terminal-config |
| **Variables d'environnement** | Clé `env` des réglages | Réglages | /docs/en/env-vars |

## Ce qu'une app externe ne peut pas faire

1. Lister, créer ou annuler les boucles et cron d'une session depuis l'extérieur.
2. Gérer les jetons API des routines cloud ou supprimer une routine depuis la CLI (web seulement).
3. Lire un flux d'usage stable hors statusline et `-p`.
4. Répondre aux demandes de permission d'une session interactive (seulement `--permission-prompt-tool`, hooks `PermissionRequest`).
5. Créer skills, agents, styles ou règles par une commande : on écrit les fichiers.
6. Modifier la configuration côté claude.ai (réglages gérés par le serveur, connecteurs, politique d'organisation).
