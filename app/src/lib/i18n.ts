const fr = {
  projects: "Projets",
  repos: "Dépôts",
  filter: "Filtrer…",
  noRepos: "Aucun dépôt trouvé.",
  sortBy: "Trier les dépôts",
  sortRecent: "Récents",
  sortName: "Nom",
  open: "Ouvrir {agent} ({count})",
  openShell: "Ouvrir un shell ({count})",
  reload: "Recharger la config",
  running: "En cours",
  exited: "Terminé",
  exitedLabel: "terminé",
  focus: "Agrandir",
  unfocus: "Réduire",
  close: "Fermer",
  empty: "Coche des dépôts à gauche, puis « Ouvrir ».",
  truncated:
    "Scan partiel : trop de dossiers à parcourir. Ajoute tes dossiers de code dans « roots » (termf.toml).",
};

type Key = keyof typeof fr;

const en: Record<Key, string> = {
  projects: "Projects",
  repos: "Repositories",
  filter: "Filter…",
  noRepos: "No repository found.",
  sortBy: "Sort repositories",
  sortRecent: "Recent",
  sortName: "Name",
  open: "Open {agent} ({count})",
  openShell: "Open a shell ({count})",
  reload: "Reload config",
  running: "Running",
  exited: "Exited",
  exitedLabel: "exited",
  focus: "Maximize",
  unfocus: "Restore",
  close: "Close",
  empty: "Tick repositories on the left, then “Open”.",
  truncated:
    "Partial scan: too many folders. Add your code folders to “roots” (termf.toml).",
};

const messages = { fr, en };

export const lang: keyof typeof messages = navigator.language.toLowerCase().startsWith("fr")
  ? "fr"
  : "en";

export function t(key: Key, vars: Record<string, string | number> = {}): string {
  return messages[lang][key].replace(/\{(\w+)\}/g, (_, name) => String(vars[name] ?? ""));
}

const relative = new Intl.RelativeTimeFormat(lang, { numeric: "auto" });
const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31_536_000],
  ["month", 2_592_000],
  ["week", 604_800],
  ["day", 86_400],
  ["hour", 3_600],
  ["minute", 60],
];

/** "il y a 3 jours" / "3 days ago" for a Unix timestamp in seconds. */
export function ago(epochSeconds: number): string {
  const diff = epochSeconds - Date.now() / 1000;
  for (const [unit, size] of units) {
    if (Math.abs(diff) >= size) return relative.format(Math.round(diff / size), unit);
  }
  return relative.format(0, "second");
}
