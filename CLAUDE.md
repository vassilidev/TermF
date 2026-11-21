# TermF

Poste de pilotage d'agents CLI (Claude Code…) sur plusieurs dépôts. La conception de référence est `docs/ARCHITECTURE.md` ; la feuille de route est au §17.

## Structure

- `crates/termf-core` : config (`termf.toml`, `projects/*.toml`), découverte des dépôts
- `crates/termf-term` : sessions PTY (portable-pty), ring buffer, attach / replay
- `app/src-tauri` : hôte Tauri 2, commandes IPC
- `app/src` : UI SvelteKit en SPA, Svelte 5, xterm.js

## Commandes

- Lancer l'app : `cd app && pnpm tauri dev`
- Front : `cd app && pnpm check && pnpm build`
- Rust : `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- Le clippy et les tests de l'app ont besoin de `app/build` : lancer `pnpm build` avant.

## Conventions

- Code, identifiants et commentaires en anglais. Les commentaires sont rares et expliquent seulement le « pourquoi ».
- Textes de l'interface dans `app/src/lib/i18n.ts` (fr + en), jamais en dur dans les composants.
- Rien de propre à une machine dans le code (chemins, dossiers, outils perso) : TermF doit marcher pour tout le monde.
- Pas de nouvelle dépendance sans raison claire : d'abord la std, puis ce qui est déjà installé.
- Ne faire que ce que la tâche demande. En cas de doute, demander avant de coder.
- Aucune modification de texte (libellés, messages, docs) non demandée.
- Toute modification d'UI doit pouvoir être montrée en avant / après.
