// No imports, so `pnpm test` runs it in plain Node.

const parts = (path: string) => path.split(/[\\/]+/).filter(Boolean);

function separator(path: string): string {
  return path.includes("\\") && !path.includes("/") ? "\\" : "/";
}

/** `~/x` as a full path under `home`; "" when home is not known yet. */
export function expandHome(path: string, home: string): string {
  if (path !== "~" && !/^~[\\/]/.test(path)) return path;
  if (!home) return "";
  return home.replace(/[\\/]+$/, "") + separator(home) + path.slice(2).replace(/[\\/]/g, separator(home));
}

/** The repository's own folder name inside `folder`. */
export function inside(folder: string, path: string): string {
  const name = parts(path).pop() ?? "";
  return folder.replace(/[\\/]+$/, "") + separator(folder) + name;
}

/**
 * Where to clone a missing repository: beside the repositories of its project that are on this machine, else
 * where the other machine has it. "" when that needs the home folder and it is not known yet.
 */
export function cloneTarget(path: string, present: string[], home: string): string {
  const sibling = present.map((p) => expandHome(p, home)).find(Boolean);
  if (!sibling) return expandHome(path, home);
  const parent = sibling.replace(/[\\/]+[^\\/]+[\\/]*$/, "");
  return inside(parent, path);
}
