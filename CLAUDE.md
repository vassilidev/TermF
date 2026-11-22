# TermF

Poste de pilotage d'agents CLI (Claude Code…) sur plusieurs dépôts. La conception de référence est `docs/ARCHITECTURE.md` ; la feuille de route est au §17. **L'état d'avancement, la méthode de travail et la suite sont dans `docs/STATUS.md` : le lire en premier pour reprendre.**

## Structure

- `crates/termf-core` : config (`termf.toml`, `projects/*.toml`), découverte des dépôts
- `crates/termf-term` : sessions PTY (portable-pty), ring buffer, attach / replay
- `crates/termf-library` : bibliothèque en couches (skills, commandes, rôles, préférences, règles), ce qu'en reçoit une session Claude Code, export / import de la config
- `app/src-tauri` : hôte Tauri 2, commandes IPC
- `app/src` : UI SvelteKit en SPA, Svelte 5, xterm.js

## Commandes

- Lancer l'app : `cd app && pnpm tauri dev`
- Front : `cd app && pnpm check && pnpm build`
- Paquet de l'app avec la CLI `termf` embarquée : `cd app && pnpm bundle` (`pnpm tauri build` seul ne l'embarque pas)
- Rust : `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- Le clippy et les tests de l'app ont besoin de `app/build` : lancer `pnpm build` avant.
- Évals des skills et des rôles, avec le vrai Claude Code (payant, Haiku, 3 $ au plus par défaut) : `cd app && pnpm evals` (`--help`).

## Conventions

- Code, identifiants et commentaires en anglais. Les commentaires sont rares et expliquent seulement le « pourquoi ».
- Textes de l'interface dans `app/src/lib/i18n.ts` (fr + en), jamais en dur dans les composants.
- Rien de propre à une machine dans le code (chemins, dossiers, outils perso) : TermF doit marcher pour tout le monde.
- Pas de nouvelle dépendance sans raison claire : d'abord la std, puis ce qui est déjà installé.
- Ne faire que ce que la tâche demande. En cas de doute, demander avant de coder.
- Aucune modification de texte (libellés, messages, docs) non demandée.
- Toute modification d'UI doit pouvoir être montrée en avant / après.
