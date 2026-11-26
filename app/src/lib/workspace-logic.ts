// No imports, so `pnpm test` runs it in plain Node.

/** `full`: names and icons; `rail`: icons only; `hidden`: gone. `fold`: what ⌘B folds it to. */
export type NavMode = "full" | "rail" | "hidden";
export type Nav = { width: number; mode: NavMode; fold: "rail" | "hidden" };

export const NAV = { width: 232, min: 184, max: 420, rail: 52, snap: 120 };
const fresh: Nav = { width: NAV.width, mode: "full", fold: "rail" };

const clampWidth = (w: number) => Math.round(Math.min(NAV.max, Math.max(NAV.min, w)));

/** What was kept, whatever was written there. */
export function readNav(text: string | null): Nav {
  try {
    const v = JSON.parse(text ?? "") as Partial<Nav>;
    return {
      width: typeof v.width === "number" && Number.isFinite(v.width) ? clampWidth(v.width) : NAV.width,
      mode: v.mode === "rail" || v.mode === "hidden" ? v.mode : "full",
      fold: v.fold === "hidden" ? "hidden" : "rail",
    };
  } catch {
    return { ...fresh };
  }
}

/** ⌘B: folded to its fold, or back in full. */
export const toggleNav = (n: Nav): Nav => ({ ...n, mode: n.mode === "full" ? n.fold : "full" });

/** Its edge dragged to `x`: under the snap it folds to icons, as VS Code's side bar does. */
export function dragNav(n: Nav, x: number): Nav {
  if (x < NAV.snap) return { ...n, mode: "rail" };
  return { ...n, mode: "full", width: clampWidth(x) };
}

/** The width of its column, the window being narrow or not (icons only, below 1100 px). */
export function navColumn(n: Nav, narrow: boolean): number {
  if (n.mode === "hidden") return 0;
  return n.mode === "rail" || narrow ? NAV.rail : n.width;
}

/** The interface's zoom, as browsers step it. */
export const UI_ZOOM = [0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];
export const FONT = { base: 13, min: 8, max: 32 };

/** The next level of `levels` from `now` (0: back to 1). */
export function zoomStep(now: number, dir: -1 | 0 | 1, levels = UI_ZOOM): number {
  if (dir === 0) return 1;
  const next = dir > 0 ? levels.find((l) => l > now + 1e-6) : [...levels].reverse().find((l) => l < now - 1e-6);
  return next ?? now;
}

/** A terminal's font size one point bigger or smaller, within bounds (0: back to the base). */
export const fontStep = (size: number, dir: -1 | 0 | 1) =>
  dir === 0 ? FONT.base : Math.min(FONT.max, Math.max(FONT.min, size + dir));

/**
 * Paths dropped from the Finder or the file manager, as a shell types them: each escaped (POSIX) or quoted (Windows),
 * separated and followed by a space, as Terminal and iTerm2 do.
 */
export function shellPaths(paths: string[], windows: boolean): string {
  const one = (p: string) => {
    if (windows) return /[\s&()^%!;,'"`$]/.test(p) ? `"${p}"` : p;
    // A newline cannot be escaped by a backslash: quoted whole instead.
    if (/[\n\r]/.test(p)) return `'${p.replace(/'/g, "'\\''")}'`;
    return p.replace(/[^\p{L}\p{N}_./+\-:@%,=]/gu, (c) => `\\${c}`);
  };
  return paths.length ? paths.map(one).join(" ") + " " : "";
}

/**
 * Where a file dropped from the Finder lands, in the page's pixels. Tauri gives the window's physical pixels; WebKit's
 * `devicePixelRatio` already holds the interface's zoom (measured: 1 → 1.25 at a 125 % zoom), so it is the only divisor.
 */
export const dropPoint = (p: { x: number; y: number }, ratio: number) => ({ x: p.x / ratio, y: p.y / ratio });

/**
 * Whether an item of the native menu (`menu` event) runs now: as the keys of the page, never under the palette or the
 * help, and Close only for a split of the Terminals view that has the keyboard.
 */
export function menuRuns(id: string, now: { palette: boolean; help: boolean; view: string }): boolean {
  if (now.palette || now.help) return false;
  return id !== "close_split" || now.view === "terminals";
}

/** The newest first, at most `max`. */
export const pushRecent = <T>(list: T[], item: T, max = 10): T[] => [item, ...list].slice(0, max);
