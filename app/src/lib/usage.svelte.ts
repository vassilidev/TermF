import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { lang, t } from "$lib/i18n";
import type { Usage } from "$lib/claude-native.svelte";
import { approx, clock, subscribed, type Billing, type Window } from "$lib/usage-logic";

/** How the account pays (`claude_billing`, read once) and what Claude Code said of its limits (`claude_usage`). */
export const usage = $state<{ billing: Billing | null; data: Usage | null }>({ billing: null, data: null });

export async function loadBilling(refresh = false) {
  try {
    usage.billing = await invoke<Billing>("claude_billing", { refresh });
  } catch {
    usage.billing = { mode: "unknown", plan: null };
  }
}

export function watchUsage(): () => void {
  void loadBilling();
  // An event heard while it loads is newer.
  invoke<Usage>("claude_usage").then(
    (u) => {
      if (!usage.data) usage.data = u;
    },
    () => {},
  );
  const stop = listen<Usage>("claude-usage", ({ payload }) => {
    usage.data = payload;
  });
  return () => void stop.then((f) => f());
}

export const isSubscribed = () => subscribed(usage.billing);
/** A formatted amount, "≈ …" for a subscriber. */
export const approxUsd = (formatted: string) => approx(formatted, usage.billing);
/** "≈ 0,12 $ en équivalent API" for a subscriber, the amount alone with an API key. */
export const equivalent = (formatted: string) => (isSubscribed() ? t("usage2Equivalent", { cost: formatted }) : formatted);
/** The help a subscriber's amount carries; none otherwise. */
export const costHelp = () => (isSubscribed() ? t("usage2Help") : undefined);
/** A budget's label: an API-equivalent usage cap for a subscriber. */
export const budgetLabel = (label: string) => (isSubscribed() ? t("usage2Cap") : label);

/** A window's label; one TermF does not know (`seven_day_opus`…) reads as the usage limit. */
export const windowName = (name: Window["name"] | string | null) =>
  name === "fiveHour" || name === "five_hour"
    ? t("claudeDgFiveHour")
    : name === "sevenDay" || name === "seven_day"
      ? t("claudeDgSevenDay")
      : t("usage2AnyWindow");

export const resetClock = (epochSeconds: number) => clock(epochSeconds, lang);

export function billingText(billing: Billing | null): string {
  switch (billing?.mode) {
    case "subscription":
      return billing.plan ? t("usage2ModeSubscription", { plan: billing.plan.charAt(0).toUpperCase() + billing.plan.slice(1) }) : t("usage2ModeSubscriptionAny");
    case "api":
      return t("usage2ModeApi");
    case "cloud":
      return t("usage2ModeCloud");
    case "none":
      return t("usage2ModeNone");
    default:
      return t("usage2ModeUnknown");
  }
}
