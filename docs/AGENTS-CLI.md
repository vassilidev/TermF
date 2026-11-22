# Codex CLI et Gemini CLI : ce que TermF en utilise (inventaire de référence)

> Relevé le 2026-11-22 dans les sources officielles, pour **Codex CLI 0.171.0** (publiée le 2026-11-20) et **Gemini CLI 0.70.0** (publiée le 2026-11-19). Aucun des deux n'était installé sur la machine du relevé : rien n'a été essayé en vrai, les faux binaires des tests rejouent les exemples de la doc et des tests officiels. « (non vérifié) » : déduit du code ou absent des pages lues. Avant de toucher un adaptateur, relire la page citée et `--help` de la version installée.

## Sources

| Outil | Source | Lu le |
|---|---|---|
| Codex | developers.openai.com/codex/noninteractive.md (exec, `--json`, prompt sur stdin, `resume`) | 2026-11-22 |
| Codex | developers.openai.com/codex/cli/reference, config-reference.md, config-advanced.md (`-c`, `notify`, `developer_instructions`, `mcp_servers`, sandbox) | 2026-11-22 |
| Codex | developers.openai.com/codex/hooks.md, guides/agents-md.md, mcp.md | 2026-11-22 |
| Codex | github.com/openai/codex : `codex-rs/exec/src/{exec_events,cli,lib,event_processor_with_jsonl_output}.rs`, `codex-rs/exec/src/cli_tests.rs`, `codex-rs/cli/Cargo.toml`, releases | 2026-11-22 |
| Gemini | github.com/google-gemini/gemini-cli : `docs/cli/{headless,cli-reference,session-management,gemini-md,system-prompt,sandbox}.md`, `docs/reference/{configuration,policy-engine}.md`, `docs/hooks/{index,reference}.md`, `docs/tools/mcp-server.md` | 2026-11-22 |
| Gemini | même dépôt : `packages/core/src/output/{types,stream-json-formatter.test}.ts`, `packages/cli/src/{nonInteractiveCli.ts,gemini.tsx,config/config.ts}`, releases | 2026-11-22 |

## Ce que TermF en fait

| Besoin | Codex CLI | Gemini CLI |
|---|---|---|
| Détection | `codex --version` → `codex-cli 0.171.0` ; npm `@openai/codex`, `brew install --cask codex`, script d'installation ; minimum 0.171.0 | `gemini --version` → `0.70.0` (non vérifié : déduit du code) ; npm `@google/gemini-cli`, `brew install gemini-cli` ; minimum 0.70.0 |
| Un tour sans terminal | `codex exec --json --skip-git-repo-check [--sandbox …] [-m …] [-c …] [resume <id>] -`, le message sur stdin (lu jusqu'à la fin : un processus par tour) | `gemini --output-format stream-json --prompt '' [--model …] [--approval-mode …] --policy <fichier> --session-id|--resume <id>`, le message sur stdin (`--prompt` s'y ajoute) |
| Événements | `thread.started{thread_id}`, `turn.started`, `item.started|completed{item}` (`agent_message`, `reasoning`, `command_execution`, `file_change`, `mcp_tool_call`, `web_search`, `todo_list`, `error`), `turn.completed{usage}`, `turn.failed{error}`, `error` | `init{session_id, model}`, `message{role, content, delta}`, `tool_use{tool_name, tool_id, parameters}`, `tool_result{tool_id, status, output, error}`, `error{severity, message}`, `result{status, error, stats}` |
| Reprise | `codex exec resume <thread_id>` (l'id vient de `thread.started`, TermF ne peut pas le choisir) ; terminal : `codex resume <id>` | `--session-id <uuid>` choisi par TermF (option absente de la doc, présente dans `config.ts`), puis `--resume <uuid>` ; terminal : `gemini --resume <id>` |
| Interrompre | SIGINT au groupe : le tour finit `Interrupted`, sans `turn.completed` ni `turn.failed`, code non nul | SIGINT : « Cancelling... », code 130 |
| Coût, contexte | Jetons seulement (`usage`, cumul du fil d'après le code, non vérifié à l'usage) ; ni dollars ni fenêtre | Jetons seulement (`stats`) ; ni dollars ni fenêtre |
| Modèle, effort | `-m`, `-c model_reasoning_effort=low|medium|high|xhigh|max|ultra` | `--model` ; pas d'effort (`thinkingBudget` seulement dans les réglages) |
| Permissions | `exec` ne demande jamais (`approval_policy = never`) ; `--sandbox read-only` (défaut) `| workspace-write | danger-full-access` | `--approval-mode default|auto_edit|yolo|plan` ; en mode sans terminal, une demande vaut refus |
| Statut d'une tuile | `-c notify=[<termf>, hook, agent-turn-complete, <ton notify…>]` : fin de tour seulement, le JSON en dernier argument | Hooks `SessionStart`, `BeforeAgent`, `BeforeTool`, `AfterTool`, `Notification` (`ToolPermission`), `AfterAgent`, `SessionEnd` dans un fichier de réglages système de TermF (`GEMINI_CLI_SYSTEM_SETTINGS_PATH`) |
| Instructions de la bibliothèque | `-c developer_instructions="…"` (texte ajouté, rien n'est écrit) | Tuile : `additionalContext` du hook `SessionStart` ; sans terminal : en tête du premier message (`<termf-instructions>`), ce que fait le hook dans ce mode d'après la doc |
| `termf mcp` | `-c mcp_servers.termf.command|args|env.*`, le jeton par `env_vars` (jamais sur la ligne de commande) | `mcpServers.termf` du fichier de réglages système de TermF, le jeton par `${TERMF_MCP_TOKEN}` (les réglages développent les variables) |
| Secrets (`SECRET_DENY`) | Pas de règle de lecture : le bac à sable limite l'écriture et le réseau, pas la lecture | `--policy` : règles `deny` (priorité 999) sur `printenv`, `env`, `export`, `set`, `declare -p|-x`, les commandes qui nomment `.env`, le jeton ou `dev.termf.cli`, et `read_file` / `read_many_files` des `.env*`, `*.pem`, `id_*`, `/proc/*/environ` |

## Ce qu'aucun des deux ne fait (refusé par TermF, jamais simulé)

Demande de permission répondue depuis TermF (Codex : seulement `codex app-server`, Gemini : seulement `--acp`, deux protocoles que TermF ne parle pas), réponse tenue par un schéma JSON (Codex a `--output-schema`, inutilisé : les étapes d'équipe exigent aussi le garde-fou), budget en dollars, `--max-turns`, outils autorisés ou refusés un par un, effort pour Gemini, garde-fou `termf guard` (Codex exige d'approuver ses hooks par `/hooks` ou `--dangerously-bypass-hook-trust` ; Gemini a `BeforeTool`, non branché), conversation avec les outils de TermF (CTO, expert).

## Pièges relevés

- `codex -p` veut dire `--profile`, pas un prompt ; `--full-auto` et `-a untrusted|on-failure` n'existent plus.
- Les sessions de `codex exec` n'apparaissent dans `codex resume` (liste, `--last`) qu'avec `--include-non-interactive` ; un id explicite marche.
- `--sandbox` se place avant `resume`, les autres options d'`exec` partout.
- `GEMINI_SYSTEM_MD` remplace tout le prompt système : jamais utilisé. `GEMINI_CLI_SYSTEM_SETTINGS_PATH` remplace le fichier système de la machine : TermF en recopie le contenu avant d'y ajouter ses hooks.
- Un hook Gemini qui sort en 0 doit n'écrire que du JSON sur stdout ; `AfterAgent` en code 2 relance le tour.
