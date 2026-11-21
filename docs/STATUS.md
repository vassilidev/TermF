# TermF — état d'avancement et reprise

> Mis à jour le 2026-11-21. La conception de référence est [ARCHITECTURE.md](ARCHITECTURE.md), la feuille de route au §17. Ce document dit **où on en est**, **comment on travaille** et **quoi faire ensuite**, pour qu'une nouvelle session reprenne sans rien perdre.

## 1. Ce qui est sur `main`

| Phase | Contenu | Où |
|---|---|---|
| 0 | Workspace Cargo, Tauri 2 + SvelteKit, sessions PTY avec ring buffer, grille, découverte adaptative des dépôts (BFS depuis `~`, budget, dossiers ignorés, tri par activité), interface FR/EN | `crates/termf-core`, `crates/termf-term`, `app/` |
| 1 | API locale (socket + jeton par session), CLI `termf`, statuts Claude via hooks (plugin éphémère), notifications, Remote Control, projets éditables, diffusion, dispositions, terminal rapide (`` Ctrl+` `` dans l'app, global en option), explorateur + recherche floue + recherche + éditeur CodeMirror + diff, mode « rester éveillé » | `crates/termf-cli`, `crates/termf-files`, `app/src-tauri/src/{api,files,config,quick_terminal,keep_awake}.rs` |
| 1b | Système de design (tokens AA clair/sombre, Inter + JetBrains Mono embarquées, icônes Lucide), barre du haut, palette `⌘K`, tuiles, écrans vides — validé visuellement par le propriétaire | `app/src/app.css`, `app/src/lib/*` |
| 2 (backend) | Bibliothèque en couches (livré → global → projet → dépôt, le dépôt ne peut ni s'activer seul ni masquer un autre niveau), matérialisation pour Claude (`--plugin-dir`, `--append-system-prompt-file`), import `~/.claude`, export/import de config sûr (liens, fichiers privés, modes, atomicité) | `crates/termf-library`, `library/` (contenu livré), `app/src-tauri/src/library.rs` |
| 2 (UI) | Vue Bibliothèque (portée, onglets, « pourquoi », interrupteurs, import Claude Code), export/import avec aperçu et choix par élément, **conventions assistées par IA** (§11.1 : `claude -p` en lecture seule, revue élément par élément, mesure d'impact en bac à sable macOS, application journalisée, interrupteur + niveau par phrase) — validé visuellement par le propriétaire (PR #8) | `crates/termf-library/src/conventions.rs`, `app/src/lib/{LibraryView,Conventions,ConfigTransfer}.svelte` |
| 2b (backend) | Clés SSH : inventaire, génération en process, agent (askpass), `~/.ssh/config` (aperçu + écriture atomique), clé par dépôt, test, rotation, corbeille | `crates/termf-ssh`, `app/src-tauri/src/ssh.rs`, `termf ssh …` |
| 2b (backend) | Synchro chiffrée (§5.4) : remotes git et dossier synchronisé, Argon2id + clé de données enveloppée (mot de passe + clé de récupération), XChaCha20-Poly1305, noms masqués, trousseau de l'OS, fusion 3 voies (TOML clé par clé), `local.toml` non synchronisé, fichiers illisibles ignorés et signalés, synchro de fond dans l'app, `termf sync …` (PR #9) | `crates/termf-sync`, `app/src-tauri/src/sync.rs`, `crates/termf-cli/src/sync.rs` |
| Distribution | CI 3 OS sur chaque push/PR ; workflow `release.yml` (tag `v*` ou lancement manuel) → installeurs macOS arm64/x64, Windows, Linux, CLI embarquée, release en **brouillon** | `.github/workflows/` |

La release brouillon `v0.1.0` a été construite **avant** la passe design : la supprimer (`gh release delete v0.1.0`) puis relancer `gh workflow run release.yml` sur `main` avant de la publier (pré-version).

## 2. En cours au moment de la passation

- **Interface des clés SSH** : PR #10 (branche `phase2b/ssh-ui`). Page de captures envoyée au propriétaire, **validation attendue** ; corrections de la revue (`/code-review medium`) en cours, avec nouvelles captures pour les écrans touchés. Laissé de côté faute de commande Rust : retirer de l'agent une clé sans fichier, tester un dépôt avec son `core.sshCommand`, GitLab, clés `-sk` ; textes renvoyés par le backend en anglais.
- **Interface de la synchro chiffrée** : un agent travaille sur la branche `phase2b/sync-ui` (front seul, page de captures `scratchpad/sync-ui-review/`), PR à venir. Si aucune PR n'existe, reprendre le worktree. Ensuite : revue, corrections, CI 3 OS, page envoyée au propriétaire, merge après sa validation.
- Choix de la synchro à confirmer par le propriétaire (signalés, non bloquants) : XChaCha20-Poly1305 direct plutôt que `age` ; changer le mot de passe émet une nouvelle clé de récupération ; premier raccordement avec des fichiers locaux = fusion sans base commune ; branche `main` fixe ; auteur des commits `TermF <sync@termf.invalid>` ; mot de passe ≥ 8 caractères ; fichier absent du remote = pas d'information, pas une suppression.

## 3. Reste à faire, dans l'ordre

1. Merger l'**interface des clés SSH** et l'**interface de la synchro** (ci-dessus) après validation visuelle.
2. Synchro, plus tard : remotes S3 / WebDAV / HTTPS, coffre et clés SSH synchronisés, oublier un appareil, cloner un dépôt manquant, recharger les réglages après un pull.
3. **Phase 3 — tickets** : SQLite (runs, événements), connecteurs GitHub puis Jira, inbox, « je colle l'URL → un agent + worktree + PR ».
4. **Phase 4 — l'équipe d'agents** : mode headless stream-json + vue conversation + prendre/rendre la main, rôles (déjà livrés dans `library/roles`), chef d'orchestre (plan JSON), moteur de workflows et de règles (contrôles `scope`, `text-changes`), MCP `termf`, jauge de contexte, canari, passation.
5. **Phase 5 — revue visuelle avant/après** dans l'app.
6. **Phase 6 — coffre & accès prod**.
7. **Phase 7 — finition** : signature/notarisation, mises à jour automatiques, budgets de perf et `cargo audit` en CI.

### Contrats des commandes SSH pour l'interface

`ssh_inventory` → `{dir, keys, agent, hosts, repos, problems}` ; `ssh_generate {name, kind: "ed25519"|"ecdsa"|"rsa", comment, passphrase?}` → chemin ; `ssh_agent_add {path, passphrase?}` ; `ssh_agent_remove {path}` ; `ssh_github_upload {path, title}` ; `ssh_host_preview {host}` → `{diff, warnings}` ; `ssh_host_save {host}` ; `ssh_assign {repo, path?}` ; `ssh_test {host}` → `{ok, message}` ; `ssh_fix_permissions` ; `ssh_rotate {path, name, kind, comment, passphrase?, upload}` → `{key, uploaded, hosts, repos, warning, errors}` ; `ssh_trash {path}`. `HostEntry = {alias, hostName?, user?, port?, identityFile?, identitiesOnly?}`. Détails des types : `app/src-tauri/src/ssh.rs`.

### Contrats des commandes de synchro pour l'interface

`sync_status` → `Status` ; `sync_probe {remote}` → `bool` (vrai : le remote contient déjà une synchro, l'activer la rejoint avec son mot de passe, sans nouvelle clé de récupération) ; `sync_setup {remote, password, scopes, deviceName?}` → `{recoveryKey?, report, error?}` ; `sync_unlock {password}` ; `sync_recover {recoveryKey, password}` → nouvelle clé de récupération ; `sync_push` et `sync_pull` → `Report` ; `sync_resolve {path, resolution}` → `Report` ; `sync_devices` → `[{id, name, os, lastSync, current}]` ; `sync_change_password {current, password}` → nouvelle clé de récupération ; `sync_scopes {scopes}` → `Status` ; `sync_disable`. Événement `sync-status` (`Status`) après chaque synchro de fond.
`Status = {enabled, remote?, scopes, deviceId?, deviceName?, unlocked, remembered, lastSync?, lastError?, conflicts, missingRepos}` ; `remote = {kind: "git", url} | {kind: "folder", path}` ; `Scope = "config"|"projects"|"conventions"` (`"vault"`, `"sshKeys"` refusés pour l'instant) ; `Report = {pulled, pushed, conflicts, missingRepos, skipped}` ; `skipped = [{path, error}]` (fichier illisible, TOML invalide : ni envoyé ni reçu jusqu'à correction, aussi dans `lastError`) ; `Conflict = {path, keys, local?, remote?}` ; `resolution = {keep: "local"|"remote"} | {keep: "merged", text}` ; `MissingRepo = {project, path, remote?}` ; dates en secondes Unix. `unlocked: false` : demander le mot de passe (`sync_unlock`). Détails : `app/src-tauri/src/sync.rs`, `crates/termf-sync`.

## 4. Dettes et points non vérifiés connus

- **Jamais testé à la main dans la vraie fenêtre** : l'interface a été vérifiée dans Chrome avec un IPC simulé, l'app réelle au démarrage et via la CLI. Windows et Linux : compilés et testés en CI, jamais lancés.
- Raccourcis : `` Ctrl+` `` sur clavier AZERTY non vérifié ; sous Linux/Windows, `Ctrl+K` / `Ctrl+J` dans un terminal vont au shell (il faut `Shift`).
- Statut : après Échap, retour à « inactif » seulement via `idle_prompt` (~60 s) ; `termf ls` liste aussi le terminal rapide.
- Fichiers : `Cmd+Q` ne protège pas les onglets non enregistrés ; un outil défini seulement dans `.zshrc` n'est pas trouvé pour `gh` / l'éditeur externe (PATH de connexion sans `-i`).
- Bibliothèque : règles livrées en français et le reste en anglais ; « promouvoir / dériver » non fait ; surcharges par lancement non branchées sur `pty_spawn` ; `applies_to` ne filtre pas encore (pas de rôle en session manuelle), donc un ajustement de rôle issu d'une convention atteint toutes les sessions ; archive `tar.gz` sans signature ni `--with-secrets`.
- Conventions : mesure d'impact seulement sur macOS (`sandbox-exec`), « non mesuré » ailleurs ; hooks en `/bin/sh` (pas sous Windows) ; une archive importée qui contient à la fois le `pack.toml` d'un projet et la règle-hook l'enregistrerait (il faudrait un registre de confiance hors config) ; dialogues natifs jamais ouverts à la main ; jamais lancé avec opus sur un gros projet ni avec une réponse « questions » réelle.
- SSH : clés matérielles `-sk`, GitLab, ACL Windows, askpass sous Windows non faits ou non vérifiés ; avis RUSTSEC-2023-0071 sur `rsa` (déchiffrement RSA non utilisé).
- Windows : named pipe avec l'ACL par défaut (protégé par le jeton) ; `shell_quote` retire les `"`.

## 5. Méthode de travail (à reprendre telle quelle)

- **Une tranche = un sous-agent** dans un worktree isolé, sur une branche `phaseN/<sujet>`, qui ouvre une PR et **ne merge jamais**. Brief autonome : sections du plan à lire, périmètre, fichiers partagés à toucher a minima, tests attendus, vérifications, livrable.
- **Revue** : `/code-review high <PR>` (medium pour de l'UI pure). Les constats repartent à l'agent auteur (il a le contexte), puis nouvelle CI.
- **Merge** : seulement si les 3 CI (macOS, Ubuntu, Windows) sont `SUCCESS` ; si `main` a bougé, `gh pr update-branch <PR> --rebase` puis CI à nouveau ; merge en squash, branche supprimée, worktree retiré.
- **Interface** : toute modification visible passe par une page de captures avant/après (Chrome + IPC simulé, clair et sombre) envoyée au propriétaire, qui valide, refuse ou demande des modifications **avant** le merge.
- **Disque limité** : `cargo clean` dans chaque worktree terminé ; profil dev allégé ; pas de build release inutile.
- **Commits** terminés par les lignes `Co-Authored-By` et `Claude-Session` ; les règles de `CLAUDE.md` s'appliquent à tous (périmètre strict, demander en cas de doute, aucun texte modifié sans demande, rien de propre à une machine).
- Les secrets, le vrai `~/.ssh`, le vrai agent SSH et la vraie config ne sont **jamais** utilisés dans les tests : `HOME` et `SSH_AUTH_SOCK` pointent vers des dossiers jetables.
