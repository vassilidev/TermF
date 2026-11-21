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
  views: "Vue",
  viewTerminals: "Terminaux",
  viewFiles: "Fichiers",
  filesRepo: "Dépôt",
  filesRefresh: "Actualiser",
  filesTree: "Arborescence",
  filesSearchTab: "Recherche",
  filesSearch: "Rechercher dans le dépôt…",
  filesRegex: "Expression régulière",
  filesFind: "Aller au fichier…",
  filesNoResults: "Aucun résultat.",
  filesTruncated: "Seuls les {count} premiers résultats sont affichés.",
  filesEmpty: "Ouvre un fichier dans l'arborescence, ou {key}.",
  statusModified: "Modifié",
  statusAdded: "Ajouté",
  statusDeleted: "Supprimé",
  statusUntracked: "Non suivi",
  fileDiff: "Diff avec HEAD",
  fileWorkingCopy: "Copie de travail",
  fileSave: "Enregistrer",
  fileOpenExternal: "Ouvrir dans l'éditeur externe",
  fileBinary: "Fichier binaire ou non UTF-8 : il ne s'ouvre pas dans l'éditeur.",
  fileTooLarge: "Fichier trop volumineux pour l'éditeur ({size} Mo).",
  fileUnsavedLabel: "Non enregistré",
  fileUnsaved: "« {name} » a des modifications non enregistrées. Fermer quand même ?",
  fileChanged: "« {name} » a été modifié sur le disque depuis son ouverture. L'écraser ?",
  fileStale:
    "Modifié sur le disque pendant que tu l'éditais. Tes modifications sont gardées ; l'enregistrement demandera avant d'écraser.",
  fileMixedEol:
    "Fins de ligne mixtes (LF, CRLF ou CR) : le fichier est en lecture seule pour ne pas les réécrire.",
  fileHead: "HEAD (dernier commit)",
  filesUnsavedClose: "Des fichiers ont des modifications non enregistrées. Fermer quand même ?",
  cancel: "Annuler",
  confirm: "Confirmer",
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
  views: "View",
  viewTerminals: "Terminals",
  viewFiles: "Files",
  filesRepo: "Repository",
  filesRefresh: "Refresh",
  filesTree: "Explorer",
  filesSearchTab: "Search",
  filesSearch: "Search in repository…",
  filesRegex: "Regular expression",
  filesFind: "Go to file…",
  filesNoResults: "No results.",
  filesTruncated: "Showing the first {count} results only.",
  filesEmpty: "Open a file from the tree, or press {key}.",
  statusModified: "Modified",
  statusAdded: "Added",
  statusDeleted: "Deleted",
  statusUntracked: "Untracked",
  fileDiff: "Diff against HEAD",
  fileWorkingCopy: "Working copy",
  fileSave: "Save",
  fileOpenExternal: "Open in external editor",
  fileBinary: "Binary or non-UTF-8 file: it can't be opened in the editor.",
  fileTooLarge: "File too large for the editor ({size} MB).",
  fileUnsavedLabel: "Unsaved",
  fileUnsaved: "“{name}” has unsaved changes. Close anyway?",
  fileChanged: "“{name}” changed on disk since it was opened. Overwrite it?",
  fileStale:
    "Changed on disk while you were editing. Your changes are kept; saving will ask before overwriting.",
  fileMixedEol: "Mixed line endings (LF, CRLF or CR): the file is read-only so they aren't rewritten.",
  fileHead: "HEAD (last commit)",
  filesUnsavedClose: "Some files have unsaved changes. Close anyway?",
  cancel: "Cancel",
  confirm: "Confirm",
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
