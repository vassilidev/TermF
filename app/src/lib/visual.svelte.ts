import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "$lib/i18n";
import { app } from "$lib/state.svelte";
import type { Decision, Pin, Progress, Review, Screen, Version } from "$lib/visual-logic";

type Key = Parameters<typeof t>[0];

export const decisionLabel: Record<Decision, Key> = {
  pending: "review5bPending",
  approved: "review5bApproved",
  rejected: "review5bRejected",
  changes: "review5bChanges",
};

const viewports: Record<Screen["viewport"], Key> = { desktop: "review5bDesktop", tablet: "review5bTablet", mobile: "review5bMobile" };

/** The route and its state, then where it was captured: "Ordinateur · clair". */
export const screenTitle = (s: Screen) => (s.state ? `${s.path} · ${s.state}` : s.path);
export const screenMeta = (s: Screen) => `${t(viewports[s.viewport])} · ${t(s.theme === "dark" ? "review5bDark" : "review5bLight")}`;

/** Where a decided version went: on to the team, or back to its agent; a review of two commits has no team. */
export function outcomeText(review: Review, v: Version): string {
  if (!v.outcome) return "";
  if (review.run === null) return t("review5bOutcomeNoTeam", { n: v.n });
  const key = ({ approved: "review5bOutcomeApproved", changes: "review5bOutcomeChanges", rejected: "review5bOutcomeRejected" } as const)[
    v.outcome as "approved" | "changes" | "rejected"
  ];
  return t(key, { n: v.n, next: v.n + 1 });
}

/** `selected`: the review its page shows; `progress`: by review, the capture under way. */
export const visual = $state({
  reviews: [] as Review[],
  selected: null as number | null,
  progress: {} as Record<number, Progress & { version: number }>,
});

/** Keeps the copy updated last: a command's answer may come after a newer `visual-updated`. */
export function upsertReview(review: Review) {
  const at = visual.reviews.findIndex((r) => r.id === review.id);
  if (at < 0) visual.reviews.unshift(review);
  else if (visual.reviews[at].updated < review.updated) visual.reviews[at] = review;
}

export function watchVisual(): () => void {
  invoke<Review[]>("visual_reviews").then(
    (list) => list.reverse().forEach(upsertReview),
    (e) => (app.error = String(e)),
  );
  const stops = [
    listen<Review>("visual-updated", ({ payload }) => upsertReview(payload)),
    listen<Progress & { review: number; version: number }>("visual-progress", ({ payload }) => {
      visual.progress[payload.review] = payload;
    }),
  ];
  return () => stops.forEach((s) => void s.then((f) => f()));
}

export const reviewOf = (run: number) => visual.reviews.find((r) => r.run === run);

export async function decide(id: number, screen: string, decision: Decision, comment?: string, pins?: Pin[]) {
  upsertReview(await invoke<Review>("visual_decide", { id, screen, decision, comment, pins }));
}

const images = new Map<string, Promise<string>>();

/** A capture as a data URL (the app's CSP has no blob:), read once per review, version, screen and kind. */
export function image(id: number, version: number, screen: string, kind: "before" | "after" | "heatmap"): Promise<string> {
  const key = `${id}/${version}/${screen}/${kind}`;
  let url = images.get(key);
  if (!url) {
    url = invoke<ArrayBuffer>("visual_image", { id, version, screen, kind }).then(
      (bytes) =>
        new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(reader.result as string);
          reader.onerror = () => reject(reader.error);
          reader.readAsDataURL(new Blob([bytes], { type: "image/png" }));
        }),
    );
    url.catch(() => images.delete(key));
    images.set(key, url);
    // Captures weigh megabytes: the oldest read go first.
    if (images.size > 48) images.delete(images.keys().next().value!);
  }
  return url;
}
