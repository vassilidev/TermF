# TermF

Un poste de pilotage léger et local pour agents IA de code (Claude Code d'abord), sur tous tes dépôts. En Rust et Tauri, sans Electron.

[English version](README.md)

> **En tout début de développement.** Les phases 0 et 1 de la [feuille de route](docs/ARCHITECTURE.md#17-feuille-de-route) sont terminées. TermF trouve tous les dépôts git de ta machine et ouvre des agents ou des shells dans autant de dépôts que tu veux, côte à côte dans une grille de terminaux. Il montre quel agent t'attend et te prévient. Il propose aussi un explorateur et un éditeur de fichiers, des projets, la diffusion vers plusieurs sessions, des dispositions sauvegardées, un terminal rapide, une CLI `termf` et un mode qui empêche la veille pendant que les agents travaillent.

## Où va le projet

- Ouvrir des dizaines de sessions d'agents sur plusieurs dépôts en deux clics, et voir d'un coup d'œil laquelle t'attend.
- Relier les projets aux tickets (Jira, GitHub Projects) et laisser une équipe d'agents par rôle (produit, front, back, DevOps, sécurité) mener un ticket jusqu'à la pull request, chacun avec le bon modèle et le bon effort.
- Des workflows et des règles qu'on active par projet, par ticket ou par tâche : périmètre strict, aucune modification de texte non demandée, revue visuelle avant / après.
- Un accès prod sécurisé (logs, SQL, `.env`) où l'IA n'a jamais les identifiants.
- Synchro chiffrée des réglages, gestion des clés SSH, un petit éditeur intégré.

La conception complète est dans [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Installer une version compilée

Les installeurs macOS (Apple Silicon et Intel), Windows et Linux sont joints à chaque [release](https://github.com/vassilidev/TermF/releases). Les builds ne sont pas encore signés, donc le premier lancement demande une étape de plus :

- **macOS** : après avoir déplacé TermF dans Applications, lance une fois `xattr -dr com.apple.quarantine /Applications/TermF.app` (ou clic droit sur l'app, Ouvrir, puis Ouvrir à nouveau).
- **Windows** : sur l'avertissement SmartScreen, clique sur « Informations complémentaires », puis « Exécuter quand même ».
- **Linux** : `chmod +x` sur l'AppImage, ou installe le `.deb` / `.rpm`.

## Lancer depuis les sources

Prérequis : Rust (stable), Node 22+, pnpm et les [prérequis Tauri](https://tauri.app/start/prerequisites/) de ton OS.

```sh
cd app
pnpm install
pnpm tauri dev
```

Au premier lancement, TermF crée `termf.toml` dans ton dossier de config (`~/Library/Application Support/termf` sous macOS, `~/.config/termf` sous Linux, `%APPDATA%\termf` sous Windows). Par défaut, il scanne ton dossier personnel à la recherche de dépôts et propose un agent, `claude`. Tu peux ajouter les tiens :

```toml
roots = ["~", "D:/dev"]
scan_depth = 5

[agents]
claude = "claude"
codex = "codex"
```

Pour regrouper plusieurs dépôts dans un projet, ajoute `projects/<id>.toml` à côté :

```toml
name = "Acme"

[[repos]]
path = "~/Projects/acme-api"

[[repos]]
path = "~/Projects/acme-front"
```

## Forker

TermF est fait pour être forké et adapté à ta façon de travailler. L'essentiel de la personnalisation passe par des fichiers (réglages, rôles, règles, workflows) plutôt que par le code : un fork reste petit et continue de récupérer les évolutions du dépôt d'origine. Les contributions sont aussi les bienvenues.

## Licence

[MIT](LICENSE)
