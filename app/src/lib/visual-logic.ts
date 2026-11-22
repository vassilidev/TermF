// No imports but types, so `pnpm test` runs it in plain Node.

export type Decision = "pending" | "approved" | "rejected" | "changes";
export type Rect = { x: number; y: number; w: number; h: number };
/** In pixels of the "after" image. */
export type Pin = { x: number; y: number; comment: string };
export type TextChange = { change: "added" | "removed"; text: string; justifiedBy: string | null; flagged: boolean };
export type Screen = {
  id: string;
  source: "route" | "agent" | "storybook";
  path: string;
  state: string | null;
  viewport: "desktop" | "tablet" | "mobile";
  width: number;
  height: number;
  theme: "light" | "dark";
  before: string | null;
  after: string | null;
  heatmap: string | null;
  rects: Rect[];
  ratio: number;
  changed: boolean;
  texts: TextChange[];
  error: string | null;
  decision: Decision;
  comment: string;
  pins: Pin[];
  decided: number | null;
};
export type Progress = { phase: "prepare" | "before" | "after" | "diff" | "done"; done: number; total: number; message: string };
export type Version = {
  n: number;
  state: "capturing" | "ready" | "failed" | "done";
  step: string | null;
  only: string[];
  before: string | null;
  after: string | null;
  progress: Progress;
  screens: Screen[];
  notes: string[];
  error: string | null;
  outcome: Decision | null;
  created: number;
  finished: number | null;
};
export type Review = {
  id: number;
  run: number | null;
  project: string;
  repo: string;
  repoPath: string;
  title: string;
  ticket: string | null;
  base: string;
  head: string | null;
  versions: Version[];
  created: number;
  updated: number;
};
export type FileDiff = { path: string; diff: string };
export type Swipe = "approved" | "rejected" | "changes" | "skip";
/** `single`: one image, the after one or, held, the before one (the quick mode). */
export type Mode = "side" | "slider" | "overlay" | "heatmap" | "single";

export const pending = (v: Version): Screen[] => v.screens.filter((s) => s.decision === "pending");

/** Several screens wait: the quick mode first. */
export const swipeFirst = (v: Version | undefined): boolean => v?.state === "ready" && pending(v).length > 1;

/** The cards still to answer: in the version's order, the ones passed over last, in the order they were passed. */
export function deck(v: Version, skipped: string[]): Screen[] {
  const left = pending(v);
  const at = (s: Screen) => skipped.indexOf(s.id);
  return [...left.filter((s) => at(s) < 0), ...left.filter((s) => at(s) >= 0).sort((a, b) => at(a) - at(b))];
}

/** What a version whose every screen is decided becomes, as the engine settles it. */
export function outcome(screens: Pick<Screen, "decision">[]): Decision {
  if (screens.some((s) => s.decision === "rejected")) return "rejected";
  if (screens.some((s) => s.decision === "changes")) return "changes";
  return "approved";
}

/** Deciding this screen so leaves none pending: the outcome that then goes to the team, else null. */
export function settles(v: Version, id: string, decision: Decision): Decision | null {
  const after = v.screens.map((s) => (s.id === id ? { decision } : s));
  return after.some((s) => s.decision === "pending") ? null : outcome(after);
}

export function counts(v: Version): Record<Decision, number> {
  const c = { pending: 0, approved: 0, rejected: 0, changes: 0 };
  for (const s of v.screens) c[s.decision]++;
  return c;
}

/** The reviews of a project, the ones waiting for a decision first, then the newest. */
export function waiting(reviews: Review[], project: string | null): Review[] {
  return reviews.filter((r) => (project === null || r.project === project) && r.versions.at(-1)?.state === "ready");
}

/** Drag or trackpad distance and speed past which a card goes: right approves, left rejects, up asks, down passes. */
export function swipeKind(dx: number, dy: number, vx: number, vy: number, w: number, h: number): Swipe | null {
  const lx = Math.min(170, w * 0.25);
  const ly = Math.min(130, h * 0.2);
  if (Math.abs(dx) >= Math.abs(dy)) {
    if (dx > lx || (vx > 0.5 && dx > 40)) return "approved";
    if (dx < -lx || (vx < -0.5 && dx < -40)) return "rejected";
  } else {
    if (dy < -ly || (vy < -0.5 && dy < -40)) return "changes";
    if (dy > ly || (vy > 0.5 && dy > 40)) return "skip";
  }
  return null;
}

/** The view of an image: its scale, and where its top left corner sits in the frame. */
export type View = { scale: number; x: number; y: number };

export const MIN_SCALE = 0.05;
export const MAX_SCALE = 8;

/** The whole width in the frame (pages are tall), or the whole image when `contain`. */
export function fit(frameW: number, frameH: number, imageW: number, imageH: number, contain = false): View {
  if (!frameW || !imageW || !imageH) return { scale: 1, x: 0, y: 0 };
  const scale = Math.min(contain ? Math.min(frameW / imageW, frameH / imageH) : frameW / imageW, 1);
  return { scale, x: (frameW - imageW * scale) / 2, y: contain ? Math.max(0, (frameH - imageH * scale) / 2) : 0 };
}

/** Zooms by `factor` around a point of the frame, which stays under the pointer. */
export function zoomAt(view: View, px: number, py: number, factor: number): View {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, view.scale * factor));
  const f = scale / view.scale;
  return { scale, x: px - (px - view.x) * f, y: py - (py - view.y) * f };
}

/** Keeps the image over the frame: no empty band on a side it overflows, centered on a side it does not fill. */
export function clamp(view: View, frameW: number, frameH: number, imageW: number, imageH: number, contain = false): View {
  const axis = (at: number, frame: number, size: number, center: boolean) =>
    size > frame ? Math.min(0, Math.max(frame - size, at)) : center ? (frame - size) / 2 : 0;
  return {
    scale: view.scale,
    x: axis(view.x, frameW, imageW * view.scale, true),
    y: axis(view.y, frameH, imageH * view.scale, contain),
  };
}

/** A point of the frame, in pixels of the image, or null off the image. */
export function toImage(view: View, px: number, py: number, imageW: number, imageH: number): { x: number; y: number } | null {
  const x = Math.round((px - view.x) / view.scale);
  const y = Math.round((py - view.y) / view.scale);
  return x < 0 || y < 0 || x >= imageW || y >= imageH ? null : { x, y };
}

/** Lines added and removed by a unified diff, its headers left out. */
export function diffStats(diff: string): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const line of diff.split("\n")) {
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) added++;
    else if (line.startsWith("-")) removed++;
  }
  return { added, removed };
}

/** The share of changed pixels: a change too small to round still reads above zero. */
export function percent(ratio: number, lang: string): string {
  const shown = ratio > 0 && ratio < 0.001 ? 0.001 : ratio;
  return new Intl.NumberFormat(lang, { style: "percent", maximumFractionDigits: shown < 0.1 ? 1 : 0 }).format(shown);
}
