// No imports but types, so `pnpm test` runs it in plain Node.
import type { LimitWindow, Limits } from "$lib/claude-native.svelte";

/** `claude_billing`: how the account pays. Claude Code's dollars are list prices: real money only with an API key. */
export type Billing = { mode: "subscription" | "api" | "cloud" | "none" | "unknown"; plan: string | null };

/** From it, a window is nearly used up: TermF warns (the same threshold as `usage::NEARLY_USED`). */
export const NEARLY_USED = 80;

export const subscribed = (billing: Billing | null) => billing?.mode === "subscription";

/** "≈ 0,12 $" for a subscriber, whose dollars only measure what the limits lose. */
export const approx = (formatted: string, billing: Billing | null) => (subscribed(billing) ? `≈\u00a0${formatted}` : formatted);

export type Window = { name: "fiveHour" | "sevenDay"; window: LimitWindow };

/** The fuller of the 5-hour and 7-day windows; a spend limit is a gateway's dollars, not the subscription's quota. */
export function fullest(limits: Limits | null | undefined): Window | null {
  const windows: Window[] = [];
  if (limits?.fiveHour) windows.push({ name: "fiveHour", window: limits.fiveHour });
  if (limits?.sevenDay) windows.push({ name: "sevenDay", window: limits.sevenDay });
  return windows.reduce<Window | null>((top, w) => (top && top.window.usedPercentage >= w.window.usedPercentage ? top : w), null);
}

/** The window to warn about, when one is at the threshold or more. */
export function pressure(limits: Limits | null | undefined, threshold = NEARLY_USED): Window | null {
  const top = fullest(limits);
  return top && top.window.usedPercentage >= threshold ? top : null;
}

/** When a window resets: the time today, the day and time later. */
export function clock(epochSeconds: number, locale: string, now = Date.now()): string {
  const at = new Date(epochSeconds * 1000);
  const sameDay = at.toDateString() === new Date(now).toDateString();
  const options: Intl.DateTimeFormatOptions = sameDay
    ? { hour: "2-digit", minute: "2-digit" }
    : { weekday: "short", hour: "2-digit", minute: "2-digit" };
  return new Intl.DateTimeFormat(locale, options).format(at);
}
